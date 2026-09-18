# Step 1: Rig graph, components, and parameters

> **Status:** implemented · **Owner:** AJ · **Applies to:** `bth-rigging` @ `bth-graph` branch
> **Prerequisite:** Step 0 complete (edition 2024, `module.rs` layout, lib + bin split, stubs in `src/rig/`).
> **Roadmap context:** Step 1 of 8. Fills the `rig/` scaffold with the data model. Step 2 turns parameters into 3D geometry and views; Step 3 solves the hanging rig. See *Duplo10 Lift Readback* rev C, §3.1–3.4 for the theory this implements.

---

## 1. Goal

Give the app a way to **describe any rig** — Duplo10, a basket under the unit, a plain 2-leg bridle — as data, and to store and reload it. Nothing on screen changes yet.

Step 1 delivers:

1. **The graph types**: bodies, nodes, members with paths and bearings, segments made of ordered components.
2. **The parameter layer**: named values with tolerance and source; node coordinates and member lengths are expressions of them.
3. **Validation**: a rig that can't be rigged is rejected with a specific message, not a panic.
4. **Weight roll-up**: total rigging weight and hook load straight from the graph.
5. **Persistence**: new InfiniteDb spaces, with existing picks and mats untouched.
6. **The layer template**: every existing pick converts to a graph whose weights match the layer engine exactly.
7. **The Duplo10 fixture**: the real rig from the lift plan, built in a test with provisional dimensions.

### Non-goals

- No hanging solve, no tensions, no rank test. That's Step 3; the graph carries no forces yet.
- No 3D coordinates in the UI, no new views. Step 2.
- No crane side. Steps 4–6.
- No editor screen. The Pick editor keeps using the layer engine, unchanged.
- No expression *parser*. Step 1 builds expressions through a builder API; typing `s12 + 2*G` into a field comes later.

### Definition of done

- [x] `rig::Rig` builds, validates, serializes and round-trips through the store
- [x] `rig::template::from_layers()` converts every pick in a corpus, and `rig_weight == layers::calculate_pick().rigging_weight_lbs` and `hook_load` to 1e-9
- [x] The Duplo10 fixture builds and validates: 5 bodies, 7 pick points × 2 faces, 4 basket straps, 6 strap-and-chain legs, 8 bar-end bearings
- [x] Every validation rule in §4 has a test that triggers it
- [x] Parameter tolerance sweep returns min/nominal/max for a chosen output (weight, for now)
- [x] Opening an existing data folder still shows all projects, picks, spreaders and mats; the new spaces start empty
- [x] `cargo test --lib --no-default-features` stays green and stays fast (< 5 s for the rig tests)
- [x] No change to any screen, PDF, or saved layer JSON

---

## 2. Types

Sketches, not final code. Field names are the contract the rest of the steps will use.

### 2.1 Identity and the container

```rust
// rig/id.rs
pub struct BodyId(Uuid);   pub struct NodeId(Uuid);
pub struct MemberId(Uuid); pub struct ParamId(Uuid);

// rig.rs
pub struct Rig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub schema_version: u16,          // 1
    pub params:  IndexMap<ParamId, Param>,
    pub bodies:  IndexMap<BodyId, Body>,
    pub nodes:   IndexMap<NodeId, Node>,
    pub members: IndexMap<MemberId, Member>,
    pub root: NodeId,                 // hook today, boom head in Step 4
}
```

Insertion-ordered maps keep reports, diagrams and diffs stable. UUID keys survive reordering and merges; a lookup index by name is built on load.

### 2.2 Parameters

```rust
// rig/param.rs
pub struct Param {
    pub id: ParamId,
    pub name: String,            // "s12", "span_A", "cg_x" — unique, snake_case
    pub quantity: Quantity,      // Length | Angle | Weight | Ratio | Count
    pub nominal: f64,            // ft, deg, lb — base units per Quantity
    pub minus: f64,              // tolerance, ≥ 0
    pub plus:  f64,
    pub source: ParamSource,     // Drawing | Catalog | Measured | Assumed | Derived
    pub note: String,
}

pub enum Expr {
    Const(f64),
    Param(ParamId),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

pub struct Coord3 { pub x: Expr, pub y: Expr, pub z: Expr }   // body-local, ft
```

- `Expr::eval(&self, &ParamTable) -> Result<f64, RigError>` and `Expr::params(&self) -> BTreeSet<ParamId>` are the whole API in Step 1.
- **Sweep** (`param::sweep`): evaluate a closure at nominal, and at each parameter's min and max, one at a time (one-at-a-time, not full factorial). Step 7 upgrades this to worst-case combinations.
- `ParamSource::Assumed` is what makes a report say "provisional". Any output computed from an `Assumed` parameter is tagged.

**Units.** Base units are ft, lb, deg, matching the rest of the app. A `Quantity` tag on each parameter keeps a length from being added to a weight. When `kip` is ready, `f64` becomes its exact type and `Quantity` becomes its unit; nothing else in the model changes.

### 2.3 Bodies and nodes

```rust
// rig/body.rs
pub struct Body {
    pub id: BodyId,
    pub label: String,             // "Bar L", "Al MDC enclosure"
    pub kind: BodyKind,
    pub weight: Expr,              // lb, 0 for a massless frame
    pub cg: Coord3,                // body-local
}

pub enum BodyKind {
    Hook,                                        // the fixed root's body
    SpreaderBar { span: Expr, rating: BarRating },// strut between end lugs
    LiftingBeam { span: Expr, rating: BarRating },// top lug(s) not at the ends
    Load { length: Expr, width: Expr, height: Expr },
    Frame,                                       // catch-all rigid body
}

// rig/node.rs
pub struct Node {
    pub id: NodeId,
    pub body: Option<BodyId>,      // None = free node (master link, knot point)
    pub local: Coord3,             // in the body frame; ignored when body is None
    pub label: String,             // "P3-front", "Bar L end A"
    pub kind: NodeKind,
}

pub enum NodeKind {
    Lug   { plate_normal: Axis, rating: Option<LugRating> },
    Bow   { bow_dia: Expr, mu: Expr },   // shackle bow or pin a strap reeves through
    Edge  { radius: Expr, mu: Expr, softener: bool },  // basket under the unit
    Free,                                 // master link, collector ring
    Root,                                 // hook bowl (Step 4: boom-head sheave)
}
```

`Bow` and `Edge` are the **bearing-capable** kinds: a member may pass through them. `mu = 0` means the strap slides freely and tension is equal on both sides. Step 3 uses `mu` for the capstan band; Step 1 only stores and validates it.

### 2.4 Members, segments, components

A member is one physical assembly: its **path** is the ordered list of nodes it touches, and between consecutive stops sits a **segment** made of ordered **components**.

```rust
// rig/member.rs
pub struct Member {
    pub id: MemberId,
    pub label: String,               // "Basket strap P1–P3 (front)"
    pub path: Vec<NodeId>,           // len ≥ 2; interior stops must be bearing-capable
    pub segments: Vec<Segment>,      // len == path.len() - 1
}

pub struct Segment {
    pub components: Vec<Component>,  // top → bottom, the user's order
}

// rig/component.rs
pub struct Component {
    pub kind: ComponentKind,
    pub length: Expr,                // ft, pin-to-pin contribution
    pub weight: Expr,                // lb
    pub adjust: Option<Adjust>,      // Some for chains, turnbuckles
    pub stiffness_lb: Option<Expr>,  // EA; None = treat as inextensible (Step 3)
    pub catalog: Option<CatalogRef>, // roundsling size, shackle size, chain grade
}

pub enum ComponentKind {
    RoundSling { size: u8, hitch: Hitch },
    Strap      { width_in: Expr },
    Chain      { grade: u8, size_in: Expr },
    WireRope   { dia_in: Expr },
    Shackle    { size: String },
    MasterLink,
    Turnbuckle,
}

pub struct Adjust { pub min: Expr, pub max: Expr, pub setting: Expr }  // setting ∈ [min, max]
```

This one shape covers everything in the readback:

| Rigging | Path | Notes |
|---|---|---|
| Hook leg to a bar lug | `[hook, bar_lug]` | 1 segment: shackle · sling · shackle |
| Duplo10 basket strap | `[P1, bar_bow, P3]` | 2 segments, one strap; `Bow` has the D/d check |
| Basket **under the unit** | `[lugA, edge1, edge2, lugB]` | 3 segments, bearings on the *load* body |
| Strap + adjustable chain | `[bar_lug, P2]` | 1 segment: shackle · strap · chain(adjust) · shackle, order is the user's |
| Choker | `[lug, choke, lug]` | `choke` is a `Bow` on the member itself; rating takes the WSTDA choke reduction |

Derived values available without a solve: `member.nominal_length()` (Σ component lengths + chain settings), `member.weight()`, `member.min_rating()` and the segment each rating came from.

---

## 3. Weight roll-up (the first real output)

```rust
pub struct RigWeights {
    pub load_lbs: f64,             // Load bodies
    pub gear_lbs: f64,             // spreader bar / beam bodies (the "EGL" row)
    pub rigging_lbs: f64,          // all member components
    pub total_below_root_lbs: f64, // what the hook sees
    pub by_body: Vec<(BodyId, f64)>,
    pub by_member: Vec<(MemberId, f64)>,
    pub assumed: bool,             // any input came from an Assumed parameter
}
```

This is what proves the graph is wired correctly before any solver exists, and it reproduces the lift-plan weight table line for line.

---

## 4. Validation

`Rig::validate() -> Result<(), Vec<RigError>>`, returning every problem at once, each naming the item.

| # | Rule | Error |
|---|---|---|
| V1 | Every `NodeId` / `BodyId` / `ParamId` reference resolves | `DanglingRef` |
| V2 | `segments.len() == path.len() - 1`, `path.len() >= 2` | `PathShape` |
| V3 | Interior path stops are `Bow` or `Edge` | `NotBearingCapable` |
| V4 | End stops are not `Bow`/`Edge` unless the member is a choker on itself | `BadEnd` |
| V5 | A node with a body has a coordinate; a `Free` node has no body | `NodePlacement` |
| V6 | Parameter expressions are acyclic and resolve | `ParamCycle`, `UnknownParam` |
| V7 | Quantities match (no length + weight) | `QuantityMismatch` |
| V8 | Every segment has ≥ 1 component with a positive length | `EmptySegment` |
| V9 | `adjust.min ≤ setting ≤ adjust.max` | `AdjustOutOfRange` |
| V10 | Exactly one `Root` node, and it is the `Rig::root` | `RootCount` |
| V11 | The graph is connected from the root through members and bodies | `Disconnected` |
| V12 | Every body is reachable: it has ≥ 1 node touched by a member | `UnsupportedBody` |
| V13 | No member has both ends on the same body (warn, allowed for a choker) | `SelfMember` (warning) |
| V14 | Weights and lengths are finite and ≥ 0 | `BadValue` |

Warnings and errors are separate: warnings surface in the report, errors block the solve.

> Stability, static determinacy and slack are **not** validated here. A rig can be geometrically valid and still be a mechanism; that's Step 3's rank test.

---

## 5. Layers → graph template

`rig::template::from_layers(pick, layers, spreaders) -> Rig` is both a migration path and a permanent regression oracle.

| Layer concept | Graph result |
|---|---|
| `Pick.weight_lbs` | `Body { kind: Load }`, weight, CG at the centroid, dims from parameters (unknown → `Assumed`) |
| A layer's slings | One member per sling: `[parent_endpoint, pick_point]`, component = `RoundSling { size, hitch }` |
| `sling_length_ft` | Component `length` expression, from a generated parameter `l_layer{i}` |
| Apex / leg shackle | `Shackle` components at the ends of those segments |
| Spreader on a layer | `Body { kind: SpreaderBar { span } }` + lug nodes at ±span/2, from parameter `span_layer{i}` |
| `tare_lbs` | A `Frame` body hung at the layer's node, weight = tare |
| `geometry::resolve_geometry` pick points | `Lug` nodes on the load body at the resolved plan coordinates, z = top of load |
| Hook | `Root` node on a `Hook` body |

**Equivalence test** (the DoD item): for a corpus of picks — the `print.rs` samples, the `db.rs` fixture, the Duplo10-ish 2-over-4, one legacy pick with `sling_length_ft = 0` — assert

```
graph.weights().rigging_lbs + gear_lbs == layers::calculate_pick(..).rigging_weight_lbs
graph.weights().total_below_root_lbs   == layers::calculate_pick(..).hook_load_lbs
```

to 1e-9. When Step 3 lands, the same corpus gets tension comparisons.

---

## 6. Persistence

New spaces only. Spaces 1–7 and their payloads are frozen, so an existing data folder keeps working and an older build ignores the new rows.

| Space | Id | Rows |
|---|---:|---|
| `SPACE_RIGS` | 8 | Rig header: id, project_id, name, schema_version, root |
| `SPACE_RIG_PARAMS` | 9 | One row per parameter, keyed (rig hi, rig lo, index) |
| `SPACE_RIG_BODIES` | 10 | One row per body |
| `SPACE_RIG_NODES` | 11 | One row per node |
| `SPACE_RIG_MEMBERS` | 12 | Member header + segments + components as one JSON payload |
| `SPACE_TOPOLOGY` | 4 (existing) | One hyperedge per member: kind `RigMemberPath`, ordered endpoints with roles `end` / `bearing` |

Notes:

- The member's node path lives **twice**: in the member payload (source of truth, easy to load) and as a hyperedge (so graph queries work in InfiniteDb). A test asserts the two agree.
- `schema_version` on the rig header. Loading a newer version than the build knows returns `DbError::SchemaTooNew` rather than partial data.
- Deleting a project deletes its rigs and their rows, matching the existing cascade.
- Keys follow `store/keys.rs`: `(uuid_hi, uuid_lo, index)`.

---

## 7. Files (fills the Step 0 stubs)

| File | Contents | Rough size |
|---|---|---:|
| `src/rig.rs` | `Rig`, `RigError`, `pub use` of the module API, `validate()`, `weights()` | 250 |
| `src/rig/id.rs` | Typed ids, `Display`, serde | 60 |
| `src/rig/param.rs` | `Param`, `Quantity`, `ParamSource`, `Expr`, `Coord3`, eval, sweep, builder helpers | 350 |
| `src/rig/body.rs` | `Body`, `BodyKind`, `BarRating`, frame helpers | 180 |
| `src/rig/node.rs` | `Node`, `NodeKind`, `Axis`, `LugRating` | 150 |
| `src/rig/member.rs` | `Member`, `Segment`, path helpers, derived length / weight / min rating | 300 |
| `src/rig/component.rs` | `Component`, `ComponentKind`, `Adjust`, `CatalogRef` → catalog lookups | 220 |
| `src/rig/bearing.rs` | Bearing geometry helpers and D/d data (rules only; no forces yet) | 120 |
| `src/rig/build.rs` | **(new file)** Fluent builder: `RigBuilder::body().node().member()…` | 260 |
| `src/rig/template.rs` | `from_layers`, equivalence helpers | 320 |
| `src/rig/fixtures.rs` | **(new file)** `duplo10()` and small rigs used across tests | 240 |
| `src/store/rig.rs` | Spaces, save / load / list / delete, hyperedge sync, schema version | 380 |
| `docs/rig-model.md` | **(new)** the model reference: concepts, invariants, examples | — |

Everything stays under `lib`, with no `dioxus` import.

---

## 8. Work order

Each item is a commit that builds and passes tests.

**1.1 Ids and parameters.** `id.rs`, `param.rs` with `Expr` eval, cycle detection, quantity checks, sweep. Tests: eval, unknown param, cycle, sweep min/nominal/max.

**1.2 Bodies and nodes.** `body.rs`, `node.rs`. Tests: body-local coordinates evaluate; `Free` node with a body is rejected.

**1.3 Members, segments, components.** `member.rs`, `component.rs`, `bearing.rs`. Derived length, weight and minimum rating per segment. Tests cover the five rigging shapes in §2.4 as paths.

**1.4 Rig container and validation.** `rig.rs` with V1–V14 and one test each.

**1.5 Builder.** `build.rs`, so a rig reads like the drawing:

```rust
let mut b = RigBuilder::new("Duplo10");
let span_a = b.param("span_A", Length, 20.0).tol(0.0, 0.0).drawing();
let bar_l  = b.bar("Bar L", span_b_l, weight_bar);
let p3f    = b.lug(unit, "P3-front", x(expr), y(-half_g), z(lug_z));
b.member("Basket strap P1–P3 (front)")
 .from(p1f).through(bar_l_bow_f).to(p3f)
 .segment(|s| s.shackle("1-1/4").strap(strap_len).shackle("1-1/4"));
```

**1.6 Weight roll-up.** `weights()` plus the `assumed` flag, with a test against the Duplo10 lift-plan numbers using the fixture's provisional inputs.

**1.7 Template + equivalence.** `template.rs` and the corpus test in §5. This is the gate: if weights don't match the layer engine, the graph is wrong.

**1.8 Persistence.** `store/rig.rs`, round-trip test (build → save → reopen store → load → compare), hyperedge/payload agreement test, cascade delete test, and a test that an old data folder still loads.

**1.9 Fixture + docs.** `fixtures::duplo10()`, `docs/rig-model.md`, roadmap check-off.

---

## 9. Duplo10 fixture

Built in code so Steps 2–7 have something real to run against. Dimensions are **provisional** and tagged `ParamSource::Assumed` until you supply drawings; the numbers below are placeholders that give sensible geometry.

| Parameter | Placeholder | Source | Drives |
|---|---:|---|---|
| `load_length` | 48 ft | Assumed | Enclosure body |
| `load_width` | 12 ft | Assumed | Enclosure body |
| `load_height` | 11 ft | Assumed | Enclosure body |
| `load_weight` | 117,300 lb | Drawing | Lift plan table |
| `cg_x`, `cg_y`, `cg_z` | 0, 0, 5.5 ft | Assumed ± 2 ft | Where the hook lands |
| `s12`, `s23`, `s34` | 8, 8, 8 ft | Assumed | P1…P7 along the length |
| `lug_gauge_G` | 12 ft | Assumed | Front / back lug rows |
| `lug_z` | 11 ft | Assumed | Lug height on the face |
| `span_A` | 34 ft | Assumed | Top bar |
| `span_B` | 12 ft | Assumed | Bars L, C, R |
| `bar_weight_top`, `bar_weight_cross` | from 4,673 lb EGL split | Drawing | Gear weight |
| `basket_len` | 22 ft | Assumed | P1 → bow → P3 |
| `chain_leg_len`, `chain_adjust` | 9 ft, 6–10 ft | Assumed | P2, P4, P6 legs |
| `mu_bow` | 0.0 | Assumed | Sliding basket (Step 3 sweeps 0 → 0.15) |

Fixture contents, asserted by test: 5 bodies (enclosure + 4 bars), 1 root, 14 lugs, 8 bar-end bows, 4 basket straps, 6 strap-and-chain legs, 4 hook legs, 4 V legs (top bar → bars L/R), 2 hook-to-center-bar legs.

> When the real dimensions arrive, only the parameter table changes. The topology stays.

---

## 10. Decisions

| # | Question | Recommendation |
|---|---|---|
| D1 | Full `Expr` AST, or plain numbers with a parameter id? | **AST.** Pick spacing as `s12 + s23` shows up immediately, and Step 7's sweep needs the dependency graph. It's ~150 lines. |
| D2 | Expression text input this step? | **No.** Builder API only. A parser (and the dimension table UI) lands with the Step 2 editor. |
| D3 | UUID keys or slab indices? | **UUIDs**, matching the rest of the app and surviving merges. Build a name index on load. |
| D4 | `f64` or the `kip` exact types? | **`f64` now**, behind a `Quantity` tag, so the swap is contained. |
| D5 | Where do ratings live? | Component holds a `CatalogRef`; the numbers stay in `catalog/`. The check functions arrive in Step 3 (`checks/`). |
| D6 | Store members as payload, hyperedge, or both? | **Both**, with a consistency test. Payload for fast loads, hyperedge so the graph is queryable. |
| D7 | Keep the layer engine after this? | **Yes**, permanently, as the equivalence oracle (Step 0 D3). |
| D8 | Model the hook block now? | **No.** Root stays the hook. Step 4 inserts the block, reeving and boom head above it. |

---

## 11. Risks

| Risk | Mitigation |
|---|---|
| The model grows to fit every hitch and stalls | The five shapes in §2.4 are the acceptance set. Anything else waits for a real pick that needs it. |
| Template drift: graph and layer engine slowly disagree | Equivalence test runs over the corpus on every commit; it's a DoD gate. |
| Parameter explosion in the fixture | Group by prefix (`load_`, `bar_`, `lug_`); the `ParamSource` tag makes provisional values visible in reports. |
| InfiniteDb schema churn in later steps | `schema_version` from day one; new spaces only; old spaces frozen. |
| Bearings modeled too loosely for Step 3 | `mu`, bow diameter and wrap-angle inputs are all captured now, even though nothing consumes them yet. |

---

## 12. Hand-off to Step 2

Step 2 gets: a validated graph, parameters that evaluate, and member paths with lengths. It adds body placement in 3D (where each bar hangs), projection to side / end / plan views, and the editor that shows the parameter table. No solver until Step 3.
