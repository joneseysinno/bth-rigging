# Step 2: Evaluation to 3D, and the view layer

> **Status:** planned · **Owner:** AJ · **Applies to:** `bth-rigging` @ `bth-graph` branch
> **Prerequisite:** Step 1 complete (`rig::{id,param,body,node,member,component,bearing,build,template,fixtures}`, `store::rig`, validation V1–V14, weight roll-up, Duplo10 fixture).
> **Roadmap context:** Step 2 of 8. Fills the `rig::eval` and `rig::views` stubs. Step 1 described the rig; Step 2 **places** it and **draws** it. Step 3 hangs it and puts numbers on the legs.

---

## 1. Goal

Turn a validated graph plus its parameter table into **numbers in space**: every node at an
(x, y, z), every member as a polyline with chord lengths, reaches, drops and angles, every body
with a pose and a world CG — and then project that into the side / end / plan data a diagram or a
PDF page can draw.

Step 2 delivers:

1. **`EvalRig`** — one immutable snapshot of the graph evaluated at a parameter binding.
2. **Placement without statics** — a non-iterative *nominal descent* that positions bodies from
   member lengths and spans, with per-body authored overrides.
3. **Residuals** — slack, short-leg and unequal-drop numbers that fall out of the descent, plus the
   CG-offset (swing) hint per body.
4. **Views** — `views::Scene`: renderer-agnostic 2D geometry, dimensions, angle arcs and labels for
   `Side`, `End` and `Plan`.
5. **Geometry sweep** — the Step 1 one-at-a-time tolerance sweep extended from weight to any scalar
   the evaluation produces (angle, drop, hook height).
6. **The second equivalence gate** — for every template rig, `eval` reproduces
   `layers::geometry::resolve_geometry` angles, drops and flags to 1e-9.

### Non-goals

- **No statics.** No tensions, no reactions, no rank test, no tension-only iteration. Step 3.
- **No screen changes.** `Scene` is data with golden tests; nothing in `ui/` consumes it yet
  (decision D2-1). The pick editor keeps the layer diagram.
- **No expression parser and no parameter-table editor.** Step 1's D2 pointed them at "the Step 2
  editor"; they are pulled out into their own step (D2-8) so Step 2 stays geometry.
- **No crane.** Root is still the hook. Steps 4–6.
- **No catenary or rope stretch.** Every segment is a straight chord. `stiffness_lb` stays unused
  until Step 3's elastic pass.

### Definition of done

- [ ] `EvalRig::evaluate(&rig)` returns world coordinates for every node, a pose for every body, and
      a polyline plus chord lengths for every member
- [ ] `eval` reproduces `layers::geometry::resolve_geometry` for the Step 1 corpus: per-layer
      `angle_deg`, `drops_ft`, `reaches_ft`, `unequal_drop_in` and `rigging_height_ft` match to 1e-9,
      and the GEOM / LEGS flags agree item for item
- [ ] `duplo10()` evaluates with no errors and every bar lands at a plausible elevation; asserted
      against the fixture's provisional numbers
- [ ] Every residual has a test that triggers it: `Short`, `Slack`, `UnequalDrop`,
      `BearingSplitAssumed`, `Underconstrained`
- [ ] `views::Scene::project(&eval, ViewKind::Side | End | Plan)` produces stable, bounded geometry;
      `tests/fixtures/scene_v1.json` golden for the Duplo10 fixture and one 2-over-4 template
- [ ] Tolerance sweep returns min/nominal/max for governing angle and hook height on the fixture
- [ ] `cargo test --lib --no-default-features` green, rig tests still under 5 s
- [ ] No change to any screen, PDF, saved layer JSON, or existing rig row (`schema_version` stays 1)

---

## 2. Types

Sketches. Field names are the contract Step 3 and the report layer will use.

### 2.1 Frames and placement

```rust
// rig/eval.rs
/// Rigid-body pose: world = origin + R · local.  Right-handed, z up, ft.
pub struct Frame { pub origin: [f64; 3], pub rot: Rot }

/// Intrinsic z-y-x (yaw, pitch, roll), deg. Identity is the Step 2 default.
pub struct Rot { pub yaw: f64, pub pitch: f64, pub roll: f64 }

/// Optional author override, stored on the body. `#[serde(default)]` — a v1 rig row
/// deserializes with `None` and behaves exactly as before (D2-5).
pub enum Placement {
    Derived,                       // default: the nominal descent places it
    Pinned  { at: Coord3 },        // origin fixed by expression (load on the ground, a set piece)
    Posed   { at: Coord3, rot: RotExpr },   // origin and rotation both authored
    Level   { rot: RotExpr },      // descent picks the origin, author fixes the rotation
}
```

`Placement` is the **only** new persisted field in Step 2, it is `Option<Placement>` on `Body`, and
it is `#[serde(default)]`. No schema bump, no migration, and an older build that ignores the field
still reads the row.

### 2.2 The evaluation snapshot

```rust
pub struct EvalRig {
    pub rig_id: Uuid,
    pub bindings: HashMap<ParamId, f64>,   // what this snapshot was evaluated at
    pub root_at: [f64; 3],                 // hook, default (0,0,0)
    pub bodies:  IndexMap<BodyId, BodyEval>,
    pub nodes:   IndexMap<NodeId, [f64; 3]>,   // world, ft
    pub members: IndexMap<MemberId, MemberEval>,
    pub weights: RigWeights,               // Step 1's roll-up, carried along
    pub height: HeightSummary,             // hook to lowest node, hook to top of load
    pub residuals: Vec<Residual>,          // everything that didn't close cleanly
    pub assumed: bool,
}

pub struct BodyEval {
    pub frame: Frame,
    pub weight_lbs: f64,
    pub cg_world: [f64; 3],
    pub support_centroid: Option<[f64; 3]>, // plan centroid of its upward attachments
    pub cg_offset_ft: f64,                  // plan distance CG → support centroid = swing hint
    pub placement: PlacementUsed,           // Derived | Pinned | Posed | Level
}

pub struct MemberEval {
    pub points: Vec<[f64; 3]>,     // path stops in world; len == member.path.len()
    pub chords_ft: Vec<f64>,       // len == segments.len(); straight-line stop to stop
    pub nominal_ft: Vec<f64>,      // hardware length available in each segment
    pub slack_ft: Vec<f64>,        // nominal − chord, clamped at 0
    pub short_ft: Vec<f64>,        // chord − nominal, clamped at 0  (GEOM when > 0)
    pub reach_ft: Vec<f64>,        // plan projection of each chord
    pub drop_ft:  Vec<f64>,        // vertical projection of each chord
    pub angle_deg: Vec<f64>,       // from horizontal; 90 = vertical, matches the app's convention
    pub governing_angle_deg: f64,  // min over segments
    pub wrap_deg: Vec<f64>,        // at each interior bearing, for Step 3's capstan band
    pub taut: bool,
}

pub struct Residual {
    pub kind: ResidualKind,
    pub at: ItemRef,          // Node | Body | Member(+segment)
    pub value: f64,           // ft, in, or deg depending on kind
    pub message: String,
    pub severity: Severity,   // Warn | Error
}

pub enum ResidualKind {
    Short,               // a segment is shorter than the gap it must span    → GEOM
    Slack,               // a leg hangs slack because a shorter one binds     → LEGS
    UnequalDrop,         // spread across a support group, in                 → LEGS
    BearingSplitAssumed, // the strap split at a bow was assumed, not solved
    Underconstrained,    // a body's pose is not determined by its supports
    CgOffset,            // the body's CG is not under its supports (swing)
}
```

`EvalRig` is a value, not a cache with invalidation. Re-evaluate; it is microseconds.

---

## 3. Placement: the nominal descent

This is the heart of the step, and the part with the interesting choice in it. There is no solver
yet, so the question is how a bar gets a height without one.

**The rule:** in plan, least squares; in elevation, tightest constraint wins.

Traverse from the root. A body becomes placeable when at least one member reaches it from an
already-placed node. For each placeable body, with attachment nodes `n_i` (body-local `l_i`) hanging
from supports `s_i` by members of available length `L_i`:

1. **Rotation** — identity unless the body carries a `Posed` / `Level` placement. Bars stay level,
   the load stays level (assumption **N1**).
2. **Plan offset** — `t_xy = mean(s_i,xy) − mean(l_i,xy)`. Least-squares centring of the attachment
   set under the support set (**N2**). For a symmetric rig this is exactly "centred under the hook,"
   which is what `layers::geometry` does today; it degrades gracefully for asymmetric ones.
3. **Reach and drop per leg** — `h_i = |s_i,xy − (l_i,xy + t_xy)|`, `d_i = √(L_i² − h_i²)`.
   `L_i < h_i` → `Short`, `d_i = 0`, severity `Warn` (the graph is still drawable, and that is the
   GEOM flag).
4. **Elevation** — leg `i` permits the body at `tz_i = s_i,z − d_i − l_i,z`. The **shortest** leg
   binds, so `t_z = max_i(tz_i)` (**N3**). Every other leg gets `slack_i = t_z − tz_i ≥ 0`, and
   `max−min` across the group in inches is the `UnequalDrop` number — the same LEGS test the layer
   engine runs, now falling out of the placement instead of being computed beside it.
5. **CG offset** — plan distance from `cg_world` to the support centroid. Not corrected, only
   reported: it is how far that body will swing when Step 3 lets it.

A body with only one support leg, or with all supports collinear in plan, is `Underconstrained`:
it is placed by the rule above and flagged, because the descent cannot know its yaw.

### 3.1 Members through a bearing

A basket strap `[P1, bow, P3]` supports the load from a bar bow and has one degree of freedom: how
the total strap length splits between the two chords. Frictionless (`mu = 0`) equilibrium splits it
so both chords make equal angles with vertical. Step 2 does not solve that; it applies, in order:

1. Plan-symmetric about the bow → split evenly, exact, no flag.
2. Otherwise → split proportional to plan reach, and raise `BearingSplitAssumed` (**N4**).

`wrap_deg` at the bow is computed from the two chord directions and stored for Step 3's capstan
band. `mu` is carried, never applied.

### 3.2 Why this shape

The descent is deliberately a **max-plus rule in z with a least-squares rule in plan**: one pass,
no iteration, no convergence to babysit, and it produces exactly the quantities that Step 3's
tension-only solver needs as a starting point — which legs are taut, which are slack, and a
feasible initial geometry. Step 3 replaces steps 2 and 4 with equilibrium and keeps everything
else, including all authored placements. The residual list becomes the active-set guess.

---

## 4. Derived geometry the app already knows

`layers::geometry` computes reach, drop, `acos(h/L)`, governing angle, unequal-drop and rigging
height for a stack of layers. Step 2 computes the same things for an arbitrary graph. Both stay
(Step 1's D7 keeps the layer engine as the oracle), and the equivalence test is the gate:

```
for pick in corpus:
    let rig  = rig::template::from_layers(pick, layers, spreaders);
    let ev   = EvalRig::evaluate(&rig)?;
    let geom = layers::geometry::resolve_geometry(layers, spreaders);
    // per layer, per sling
    ev.angle_of(layer, leg)      == geom[layer].angle_deg        ± 1e-9
    ev.drop_of(layer, leg)       == geom[layer].drops_ft[leg]    ± 1e-9
    ev.reach_of(layer, leg)      == geom[layer].reaches_ft[leg]  ± 1e-9
    ev.unequal_drop_in(layer)    == geom[layer].unequal_drop_in  ± 1e-9
    ev.height.hook_to_pick_ft    == geometry::rigging_height_ft(&geoms) ± 1e-9
    ev.flags(layer)              == { GEOM, LEGS, NO LEN }       exactly
```

A legacy layer with `sling_length_ft = 0` must come through as the same "NO LEN" state, not as a
`Short` residual — map it to a missing input, not a geometry failure.

---

## 5. Views

`Scene` is **renderer-agnostic on purpose**: the pick editor draws SVG through Dioxus, and the calc
package renders through Typst. Both must consume the same geometry or the PDF and the screen will
drift (D2-3).

```rust
// rig/views.rs
pub enum ViewKind { Side, End, Plan }   // x–z, y–z, x–y

pub struct Scene {
    pub kind: ViewKind,
    pub bounds: Rect,          // world units, ft
    pub items: Vec<Item>,      // sorted far → near by `depth`
}

pub enum Item {
    Polyline { pts: Vec<[f64; 2]>, role: Role, depth: f64, label: Option<String> },
    Marker   { at: [f64; 2], role: Role, depth: f64, label: Option<String> },
    Dim      { from: [f64; 2], to: [f64; 2], text: String, offset_ft: f64 },
    Arc      { at: [f64; 2], from_deg: f64, to_deg: f64, radius_ft: f64, text: String },
    Text     { at: [f64; 2], text: String, role: Role },
}

pub enum Role { Load, Bar, Hook, Member, SlackMember, ShortMember, Lug, Bow, Edge, Cg, Dimension, Warning }

impl Scene {
    pub fn project(eval: &EvalRig, kind: ViewKind) -> Scene;
    /// Shared world → canvas mapping so SVG and Typst agree exactly.
    pub fn fit(&self, w: f64, h: f64, margin: f64) -> Transform;
}
```

Rules:

- Coordinates stay in **feet**. No pixels in the lib. `fit()` is the only place a scale exists.
- **Role, not color.** The lib says "this is a slack member"; the renderer decides it is dashed grey.
  Sling colour by WSTDA size stays where it is, in the UI.
- **Depth sorting** so a near leg draws over a far one, using the dropped axis as depth.
- Bodies draw as their footprint: `Load` as its length × width × height box edges, `SpreaderBar` /
  `LiftingBeam` as a line between end lugs plus a depth tick, `Frame` as a marker.
- Automatic dimensions: spreader spans, pick spacing along the load, hook height, governing angle
  arc on the governing leg. Anything with a `ParamSource::Assumed` input gets its text suffixed
  `(assumed)` so a provisional drawing is obvious on the page.
- `Plan` view drops z and draws the load footprint plus every lug and bow — this is the view that
  makes an asymmetric CG visible, so the CG marker and the support centroid both appear, with the
  offset dimensioned between them.

Golden test: `Scene` serializes to JSON, `tests/fixtures/scene_v1.json` holds Duplo10 side/end/plan
and one 2-over-4 template, and `tests/golden.rs` gains the comparison next to the existing calc
package goldens.

---

## 6. Sweep, extended

Step 1's `param::sweep` evaluates a closure at nominal and at each parameter's min and max, one at a
time. Step 2 supplies the closures that matter for a drawing:

```rust
sweep(&rig.params, |t| EvalRig::evaluate_with(&rig, t)?.governing_angle_deg())
sweep(&rig.params, |t| EvalRig::evaluate_with(&rig, t)?.height.hook_to_pick_ft)
sweep(&rig.params, |t| EvalRig::evaluate_with(&rig, t)?.max_short_ft())
```

The third one is worth its own report line: it answers *"with the tolerances you gave me, can this
strap still reach?"* — a question the layer engine cannot ask. Still one-at-a-time; Step 7 makes it
worst-case combinations.

---

## 7. Files

| File | Contents | Rough size |
|---|---|---:|
| `src/rig/eval.rs` | `Frame`, `Rot`, `EvalRig`, `BodyEval`, `MemberEval`, descent, residuals | 520 |
| `src/rig/eval/descent.rs` | **(new)** placement order, plan least-squares, max-plus elevation, bearing split | 260 |
| `src/rig/views.rs` | `ViewKind`, `Scene`, `Item`, `Role`, `project`, `fit` | 380 |
| `src/rig/views/annotate.rs` | **(new)** spans, spacing, angle arcs, CG offset, assumed tags | 200 |
| `src/rig/body.rs` | `+ placement: Option<Placement>` with `#[serde(default)]` | +60 |
| `src/rig/build.rs` | `+ b.pin(body, at)`, `b.pose(body, at, rot)`, `b.level(body, rot)` | +50 |
| `src/rig/member.rs` | `+ available_length()` (Σ components + adjust setting) if not already exact | +20 |
| `src/rig/template.rs` | template rigs carry the layer stack's placements where known | +60 |
| `tests/golden.rs` | scene goldens | +90 |
| `tests/fixtures/scene_v1.json` | **(new)** | — |
| `docs/rig-model.md` | placement, residuals and views sections appended | — |

`rig::eval` and `rig::views` import nothing but `rig`, `domain` and `format`. No `dioxus`, no
`store`, no `layers` — the equivalence test lives on the `layers` side of the fence or in `tests/`.

---

## 8. Work order

Each item is a commit that builds and passes tests.

**2.1 Frames and evaluation scaffolding.** `Frame`, `Rot`, `EvalRig` shell, node world coordinates
for bodies whose pose is given. Tests: a pinned body's lugs land where the drawing says.

**2.2 Placement: plan.** Traversal order from the root, least-squares plan offset, `Underconstrained`
detection. Tests: symmetric 4-leg bridle centres under the hook; a 3-leg asymmetric case centres on
the centroid.

**2.3 Placement: elevation.** Reach/drop per leg, max-plus `t_z`, `Slack`, `Short` and
`UnequalDrop` residuals. Tests: two legs of unequal length → the shorter binds, the longer carries
the slack, and `unequal_drop_in` matches by hand.

**2.4 Bearings.** Two-segment members through a bow, symmetric split, proportional fallback with
`BearingSplitAssumed`, `wrap_deg`. Tests: the Duplo10 basket strap splits evenly; an offset bow
raises the flag.

**2.5 Derived geometry and heights.** `angle_deg` from horizontal, governing angle, `HeightSummary`.
Tests against hand calcs.

**2.6 Equivalence with `layers::geometry`.** The corpus test in §4. **This is the gate.** If angles
and drops don't match the layer engine, the placement rule is wrong, and nothing after this is
trustworthy.

**2.7 Views.** `Scene::project` for the three kinds, roles, depth sorting, `fit()`. Tests: bounds
are finite and contain every point; a scene has no NaN; item counts by role.

**2.8 Annotations.** Spans, spacing, angle arc, CG offset, assumed tags. Golden JSON.

**2.9 Sweep, fixture assertions, docs.** The three sweeps in §6, Duplo10 elevation assertions,
`docs/rig-model.md` update, roadmap check-off.

---

## 9. Decisions

| # | Question | Decision |
|---|---|---|
| **D2-1** | Does a screen consume the graph this step? | **No.** Lib plus golden scenes. The layer engine is still the oracle and the graph has no tensions; a diagram that looks right for the wrong reason is worse than no diagram. The parallel-panel option waits for Step 3. |
| **D2-2** | How do bodies get placed without a solver? | **Nominal descent with authored override.** Least-squares in plan, tightest-leg in elevation, `Placement` on any body that needs pinning. Step 3 replaces the rule, keeps the overrides. |
| **D2-3** | One scene type for SVG and PDF, or two renderers? | **One.** `Scene` in feet, `fit()` for the mapping. Two renderers means two drawings that slowly disagree. |
| **D2-4** | Rotation representation? | **Yaw-pitch-roll in degrees**, identity by default. Quaternions buy nothing at these angles and read badly in a parameter table. |
| **D2-5** | Persist placement — new field or new space? | **Field on `Body`, `#[serde(default)]`.** `schema_version` stays 1; a v1 row loads as `Derived`. |
| **D2-6** | Cache `EvalRig` on the rig? | **No.** It is a value, recomputed on demand, keyed by its bindings. Caching invalidation is not a problem worth owning yet. |
| **D2-7** | `ViewKind::Auto` (pick the plane by the largest spread)? | **Not now.** The signature takes a `ViewKind`, so adding `Auto` later is additive. |
| **D2-8** | Expression parser and parameter-table editor? | **Separate step (2.5).** Text input is a UI surface with its own error handling; folding it in doubles this step. |
| **D2-9** | Where does the `layers` equivalence test live? | **`tests/`**, not the lib, so `rig` keeps its clean import list. |

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| The descent quietly disagrees with the layer engine on a case nobody tests | §4 runs the whole corpus, per leg, not per layer summary, and the flags are compared exactly, not just the numbers |
| The descent becomes a solver by accident (iteration creeps in to "fix" a case) | One pass is a hard rule this step. A case that needs iteration gets a `Residual` and waits for Step 3 |
| Scene grows renderer concerns (pixels, colors, fonts) | `Role` enum only; a review rule — no `f64` in the lib named `px`, no color strings |
| Bearing split assumption quietly wrong on the real Duplo10 | `BearingSplitAssumed` is a visible residual and the report prints it; the symmetric case, which is the real rig, is exact |
| Placement field makes old rig rows unreadable | `#[serde(default)]` plus a test that loads a Step 1-written row |
| Golden scenes churn on every cosmetic tweak | Goldens hold geometry and roles, not text formatting; label text is a separate, smaller golden |

---

## 11. Hand-off to Step 3

Step 3 receives: world coordinates for every node, chord directions and lengths for every segment,
wrap angles at every bearing, a taut/slack classification per leg, and body CG offsets that say
which way each body wants to swing. That is precisely the input a tension-only solve needs —
the descent's residual list is the initial active set, and the rank test (`s = m − r`,
`k = dof − r`) runs on the same incidence structure the descent already walked.

`Scene` does not change in Step 3; members gain a tension value and roles gain `Overloaded`.
