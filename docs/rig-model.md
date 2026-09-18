# Rig graph model (Step 1)

This is the data model the rest of the solver, views and reports will use. It
describes **any** rig — Duplo10, a basket under the unit, a plain 2-leg bridle —
as a graph. Nothing on screen consumes it yet.

Theory source: *Duplo10 Lift Readback* rev C, §3.1–3.4. Step 2 places bodies in
3D; Step 3 hangs the rig and checks ratings.

## Concepts

| Term | Meaning |
|---|---|
| **Parameter** | Named scalar (`s12`, `span_A`, `load_weight`) with unit, tolerance and source. Node coordinates and member lengths are expressions of parameters. |
| **Body** | Rigid thing with a local frame, weight and CG: hook, spreader bar, lifting beam, load, or catch-all frame (tare). |
| **Node** | A point: lug, bow, edge, free knot, or the root (hook). Body-local coordinates are expressions. |
| **Member** | One physical assembly. Its **path** is the ordered list of nodes it touches. |
| **Segment** | The hardware between two consecutive path stops, made of ordered **components**. |
| **Component** | One piece of hardware (roundsling, strap, chain, shackle, …) with pin-to-pin length and self-weight. |
| **Bearing** | A `Bow` or `Edge` node a member may pass through. `μ = 0` means the strap slides freely (equal tension both sides). |

Insertion-ordered maps (`IndexMap`) keep reports and diffs stable. UUID keys
survive reordering and merges. A name lookup is a scan of the table (built on
load in the store).

## Invariants (validation)

`Rig::validate()` returns every problem at once. Errors block a solve;
warnings surface in a report.

1. Every node / body / parameter reference resolves.
2. `segments.len() == path.len() - 1` and `path.len() >= 2`.
3. Interior path stops are `Bow` or `Edge`.
4. End stops are not bearings unless the member is a self-choker (path starts and ends on the same bow).
5. A `Free` node has no body; other kinds sit on a body.
6. Parameter expressions are acyclic and resolve.
7. Quantities match (no length + weight).
8. Every segment has at least one component with a positive length.
9. Adjustable take-up sits in `[min, max]`.
10. Exactly one `Root` node, and it is `Rig::root`.
11. The graph is connected from the root through members and bodies.
12. Every body has at least one node touched by a member.
13. Both ends on the same body is a **warning** (`SelfMember`), allowed for a choker.
14. Weights and lengths are finite and ≥ 0.

Stability, determinacy and slack are **not** checked here. A geometrically
valid rig can still be a mechanism — that is Step 3's rank test.

## Expressions

There is no parser in Step 1. Expressions are an AST built through `RigBuilder`
and ordinary `+ - * /` on `ParamId` / `Expr`:

```rust
let s12 = b.param("s12", Quantity::Length, 8.0).assumed();
let s23 = b.param("s23", Quantity::Length, 8.0).assumed();
let x_p3 = -(s12 + s23); // Expr
```

`Expr::eval` walks the tree, tracks quantity, and reports `UnknownParam`,
`ParamCycle`, `QuantityMismatch` or `BadValue`. `param::sweep` evaluates a
closure at nominal and at each parameter's min and max, one at a time
(not full factorial — Step 7 upgrades that).

`ParamSource::Assumed` is what makes a report say "provisional". Weight
roll-up sets `RigWeights.assumed` when any input is assumed.

Base units are ft, lb, deg, matching the rest of the app. `Quantity` keeps a
length from being added to a weight. When `kip` is ready, `f64` becomes its
exact type and `Quantity` becomes its unit.

## Five member shapes

| Rigging | Path | Notes |
|---|---|---|
| Hook leg to a bar lug | `[hook, bar_lug]` | 1 segment: shackle · sling · shackle |
| Duplo10 basket strap | `[P1, bar_bow, P3]` | 2 segments, one strap; bow has the D/d inputs |
| Basket under the unit | `[lugA, edge1, edge2, lugB]` | 3 segments, bearings on the load body |
| Strap + adjustable chain | `[bar_lug, P2]` | 1 segment; `Adjust` on the chain |
| Choker | `[lug, choke, lug]` | `choke` is a `Bow`; rating takes the WSTDA choke WLL |

Derived without a solve: `member.nominal_length()` (Σ lengths + chain
settings), `member.weight()`, `member.min_rating()`.

## Weight roll-up

```
load_lbs              Load bodies
gear_lbs              SpreaderBar / LiftingBeam ("EGL")
rigging_lbs           member components + Frame (tare) bodies
total_below_root_lbs  load + gear + rigging  (what the hook sees)
```

The hook body itself is not included. This is the first real output of the
graph and the regression oracle against `layers::calculate_pick`.

## Layers → graph

`rig::template::from_layers(pick, layers, spreaders)` is both a migration path
and a permanent equivalence test. For every pick in the corpus:

```
graph.weights().rigging_lbs + gear_lbs  ==  calculate_pick(..).rigging_weight_lbs
graph.weights().total_below_root_lbs    ==  calculate_pick(..).hook_load_lbs
```

to 1e-9. The layer engine stays as that oracle (decision D7).

## Persistence

New InfiniteDb spaces only (8–12). Spaces 1–7 are frozen, so an existing data
folder keeps working and an older build ignores the new rows.

| Space | Id | Rows |
|---|---:|---|
| `SPACE_RIGS` | 8 | Header: id, project_id, name, schema_version, root |
| `SPACE_RIG_PARAMS` | 9 | One row per parameter, keyed `(rig hi, rig lo, index)` |
| `SPACE_RIG_BODIES` | 10 | One row per body |
| `SPACE_RIG_NODES` | 11 | One row per node |
| `SPACE_RIG_MEMBERS` | 12 | Member header + segments + components as one JSON payload |
| `SPACE_TOPOLOGY` | 4 | One hyperedge per member: kind `RigMemberPath`, roles `end` / `bearing` |

The member path lives twice: payload is the source of truth; the hyperedge
makes the graph queryable. Loading `schema_version` newer than the build
returns `DbError::SchemaTooNew`. Deleting a project cascades to its rigs.

## Duplo10 fixture

`rig::fixtures::duplo10()` is the real topology with **provisional** dimensions
tagged `Assumed` until drawings arrive. When the numbers change, only the
parameter table changes.

Asserted contents: 5 bodies (enclosure + 4 bars), 1 root, 14 load lugs
(7 pick points × 2 faces), 8 bar-end bows, 4 basket straps, 6 strap-and-chain
legs, 4 hook legs, 4 V legs, 2 hook-to-centre-bar legs.
