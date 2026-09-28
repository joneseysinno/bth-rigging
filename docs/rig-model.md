# Rig graph model (Steps 1–3)

This is the data model the rest of the solver, views and reports will use. It
describes **any** rig — Duplo10, a basket under the unit, a plain 2-leg bridle —
as a graph. Nothing on screen consumes it yet.

Theory source: *Duplo10 Lift Readback* rev C, §3.1–3.5. Step 2 places bodies in
3D and projects side / end / plan scenes. Step 3 hangs the rig and checks ratings
(`docs/step-3-solve-checks.md`).

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

Expressions can be built with `RigBuilder` and ordinary `+ - * /` on
`ParamId` / `Expr`, or entered as text with `rig::param::syntax::parse_expr`:

```rust
let s12 = b.param("s12", Quantity::Length, 8.0).assumed();
let g = b.param("lug_gauge_G", Quantity::Length, 12.0).assumed();
let x_p3 = -(s12 + g); // Expr

let parsed = parse_expr("s12 + 2*lug_gauge_G", &rig.params)?;
```

The expression grammar is deliberately small:

```text
expr   := term (('+' | '-') term)*
term   := unary (('*' | '/') unary)*
unary  := '-' unary | atom
atom   := literal | ident | '(' expr ')'
literal := number unit? | feet-inches
```

Binary operators are left-associative; unary minus binds more tightly than
multiplication, matching Rust builder expressions. Identifiers are
case-sensitive and use `[A-Za-z_][A-Za-z0-9_]*`. Functions, exponentiation,
implicit multiplication and `%` as an operator are unsupported. The parser
keeps byte spans internally and reports 1-based character columns, including
for multi-byte symbols. It stops at the first error. Quantity mismatches point
to the operator that combines the incompatible expressions.

Unit suffixes are recognized only immediately after a number, with or without
intervening whitespace. Values are checked and converted to base units during
parsing:

| Unit suffix | Quantity | Base-unit factor |
|---|---|---:|
| `ft`, `'` | Length | 1 ft |
| `in`, `"` | Length | 1/12 ft |
| `lb`, `lbs` | Weight | 1 lb |
| `kip` | Weight | 1000 lb |
| `deg`, `°` | Angle | 1 degree |
| `%` | Ratio | 0.01 |

Feet-inches without internal whitespace are one literal (`8'-6"` = 8.5 ft);
with whitespace (`8' - 6"`) the hyphen is subtraction. Unsupported unit words
include `ton`, `kips`, `mm`, `m` and `kg`.

`Expr::to_text` prints canonical text. For finite expressions,
`parse_expr(expr.to_text(&table), &table).expr == expr`, including negative
constants, explicit negation and nested operator trees. Parentheses preserve
AST shape; numeric text round-trips exactly. Literal unit spelling is not
stored: `6 in` becomes `Const(0.5)` and prints as `0.5`. A dangling parameter
prints as a non-rebinding `?<uuid-prefix>` marker. The AST remains the source
of truth; no expression text is persisted and no `Expr` variants or schema
version were added.

`Expr::eval` walks the tree, tracks quantity, and reports `UnknownParam`,
`ParamCycle`, `QuantityMismatch` or `BadValue`. `param::sweep` evaluates a
closure at nominal and at each parameter's min and max, one at a time
(not full factorial — Step 7 upgrades that).

`ParamSource::Assumed` is what makes a report say "provisional". Weight
roll-up sets `RigWeights.assumed` when any input is assumed. Base units are
ft, lb and deg.

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

## Placement (Step 2)

`Body.placement` is `Option<Placement>` with `#[serde(default)]`. A v1 row
loads as derived; `schema_version` stays 1.

| Variant | Origin | Rotation |
|---|---|---|
| `Derived` (default) | nominal descent | identity |
| `Pinned { at }` | authored `Coord3` | identity |
| `Posed { at, rot }` | authored | authored yaw-pitch-roll (deg) |
| `Level { rot }` | descent | authored |

The descent is one pass, no iteration: **least-squares in plan, tightest-leg
in elevation.** A body becomes placeable when a member reaches it from an
already-placed node. Gear (bars, frames) is placed before the load when both
are ready, so a long centre hang cannot steal the pose before the side bars
exist. Plan offset centres the attachment set under the support set. Each
leg permits a `t_z`; the **shortest** (max `t_z`) binds. Other legs carry slack.

A two-segment member through a bow splits evenly when the chords are
plan-symmetric about the bow; otherwise the split is proportional to plan
reach and `BearingSplitAssumed` is raised. `mu` is stored, not applied.

## Evaluation snapshot

`EvalRig::evaluate(&rig)` (or `evaluate_with` at a trial parameter table) is a
value, not a cache. It carries world coordinates for every node, a pose and
world CG for every body, a polyline and chord/reach/drop/angle for every
member, the Step 1 weight roll-up, hook-to-pick / hook-to-load-top /
hook-to-lowest heights, and a residual list.

Residuals (Warn unless noted):

| Kind | Meaning | Layer flag |
|---|---|---|
| `Short` | segment shorter than the gap it must span | GEOM |
| `Slack` | a longer leg hangs because a shorter one binds | — |
| `UnequalDrop` | max−min theoretical drop across a support group, in | LEGS (> 1 in) |
| `BearingSplitAssumed` | strap split at a bow was assumed, not solved | — |
| `Underconstrained` | pose (yaw) is not determined by the supports | — |
| `CgOffset` | plan distance CG → support centroid (swing hint) | — |

A legacy `sling_length_ft = 0` is a missing input (`NO LEN`), not `Short`.
`param::sweep` can wrap `evaluate_with` for governing angle, hook height, or
`max_short_ft`.

Template graphs match `layers::geometry::resolve_geometry` to 1e-9 on reach,
drop, governing angle, unequal-drop and hook-to-pick height (test lives in
`tests/eval_geometry.rs`).

## Views

`views::Scene` is renderer-agnostic 2D geometry in **feet**. `ViewKind` is
`Side` (x–z), `End` (y–z) or `Plan` (x–y). Items carry a `Role` (Load, Bar,
Hook, Member, SlackMember, ShortMember, …) not a color. `fit(w, h, margin)`
is the only scale. Automatic dimensions cover spreader spans, pick spacing,
hook height, the governing-angle arc, and (in plan) the CG-to-support offset.
Assumed inputs suffix dim text with `(assumed)`.

Golden: `tests/fixtures/scene_v1.json` (Duplo10 and one 2-over-4 template,
three views each). Nothing in `ui/` consumes `Scene` yet.

## Solve (Step 3)

`rig::solve::solve(&rig)` hangs the rig under gravity and returns a
`SolvedRig`: the hung `EvalRig`, a `MemberForce` per member (tension, taut,
force on every path stop, chord angles, wrap), hook load, held-body reactions,
tilt per body, and two `Determinacy` records (`determinacy` for the carrying
set, `topology` for every member taut).

| Piece | Rule |
|---|---|
| Free | every body except hook / `Pinned` / `Posed` (6 DOF); free knots (3 DOF) |
| Tension member | one T along the whole path, μ = 0; force `T·Σ unit vectors` per stop |
| Link | two stops, hardware only, length ≤ 1e-6 ft → ball joint, 3 force components |
| Self-weight | each segment's weight lumped at its lower stop |
| Rank | `s = m − r`, `k = dof − r`; physical tolerance 1e-6 (`MECH_RTOL`) |
| Split | multipliers regularised by compliance `Σ L/EA`; rigid default `EA = 1e9·W` |
| Legacy NO LEN | solve refuses (`EmptySegment`) |

`statics_at(&rig, &eval)` is the small-displacement split at a fixed pose
(the equivalence gate). `bounds::envelope` gives each member's tension range
over every tension-only equilibrium at the pose (LP); `bounds::named_cases`
re-hangs with chains or baskets slack. `inverse::level` / `take_up` compute
adjuster settings. `rate::rate` / `with_bounds` stamp `checks` (sling, chain,
shackle, lug, bar) OK / WARN / OVER. `Scene::project_solved` draws the hung
pose with tension labels and `Role::Overloaded`.

Tension gate (`tests/solve_layers.rs`): statics at the descent pose match
`layers::calculate_pick` per sling to 1e-9 relative, the hang to 1e-6; hook
load always. Skips are named: NO LEN, GEOM, unequal angles, basket drawn as
single legs, tare hung at one pick.
