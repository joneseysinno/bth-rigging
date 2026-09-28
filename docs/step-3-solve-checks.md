# Step 3: Solver and shared checks

> **Status:** implemented · verified 2026-09-26 (157 tests green headless) · **Owner:** AJ · **Applies to:** `bth-rigging` @ `bth-graph` branch
> **Prerequisite:** Step 2 complete (`EvalRig`, nominal descent, residuals, `Scene`).
> **Roadmap context:** Step 3 of 8. Step 1 described the rig; Step 2 placed and drew it; Step 3 **hangs it and puts numbers on the legs**, then stamps every rated item OK / WARN / OVER. Step 2.5 (parser + parameter table) follows, then the first screen that shows the graph.

---

## 1. Goal

Given a validated rig, answer the questions a lift plan asks:

1. **Where does it hang?** Let gravity find the pose. A CG offset tips the load; a short leg takes the load off its neighbours.
2. **What does every member carry?** Tension-only, one tension along a member's whole path (μ = 0 at bearings).
3. **Is the answer unique?** Readback §3.5: `s = m − r` self-stress states, `k = dof − r` mechanisms.
4. **How bad can it get if the fit-up is off?** The tension envelope, plus named slack cases.
5. **What chain setting hangs it level / takes up the slack?**
6. **Is every item within rating?** Shared checks for slings, chain, shackles, lugs and bars.

### Non-goals

- **No screen.** `Scene::project_solved` gives the renderer tensions and an `Overloaded` role, but nothing in `ui/` consumes it (D3-1).
- **No crane.** The hook is still fixed ground. Steps 4–6.
- **No capstan friction in the solve.** `mu` stays stored. A strap through a bow carries one tension; the wrap angle is reported for the band.
- **No target-split inverse.** Where the load goes past "just snug" depends on stiffness. It waits for real `stiffness_lb` data and Step 7 tolerances (D3-11).
- **No change to `layers`.** It stays the oracle, and the tension gate compares against it.

### Definition of done

- [x] `solve(&rig)` hangs the rig and returns tensions, stop forces, hook load, reactions, tilt per body and the determinacy of both the carrying set and the whole graph
- [x] The hook load equals `weights.total_below_root_lbs` for every rig hung from the hook alone (1e-8)
- [x] **Tension gate:** small-displacement statics at the descent pose match `layers::calculate_pick` sling tensions to **1e-9 relative**; the full hang matches to **1e-6** (inextensible members stretch ~1e-9 of their length). Every comparable corpus layer is compared, and skips are named (NO LEN, GEOM, unequal angles, basket drawn as single legs, tare hung at one pick)
- [x] Rank test: two-leg `s = 0`; symmetric four-leg `s = 1`; Duplo10 `m = 20, r = 13, s = 7`
- [x] Hang behaviours, each with a test: CG offset swings under the hook; a leg 1 in short hands the load to the diagonal pair; a fixed-pose "up-and-over" false equilibrium is not reached
- [x] Envelope: symmetric four-leg max = `W / (2 sin θ)` per leg (the diagonal-pair rule), matches an independent LP (scipy HiGHS) on Duplo10
- [x] Named cases `ChainsSlack` / `BasketsSlack` hang and balance on Duplo10
- [x] Inverse: an offset-CG load on two chain legs levels to 0.01°; the settings satisfy the hand geometry and moment balance
- [x] Checks: sling, chain, shackle (side load), lug (out of plane), bar (hanging load + axial); `P = V / tan θ` cross-check on a spreader
- [x] `cargo test --no-default-features` green: lib 136, `eval_geometry` 10, `golden` 3, `solve_layers` 8. Lib suite 2.7 s in debug
- [x] No change to any screen, PDF, saved layer JSON, rig row, or the Step 2 scene golden

---

## 2. Pipeline

```
Rig ──evaluate──▶ EvalRig (descent pose)
                    │
                    ├─ statics_at ─▶ SolvedRig (Method::Statics)   ← equivalence gate
                    │
                    └─ Model::build ─▶ hang ─▶ EvalRig::with_pose ─▶ SolvedRig (Method::Hang)
                                                                        │
                                   bounds::envelope / named_cases ◀─────┤
                                   rate::rate / with_bounds ◀───────────┤
                                   inverse::level / take_up (re-solve)  │
                                   Scene::project_solved ◀──────────────┘
```

| File | Contents |
|---|---|
| `rig/solve.rs` | `solve`, `solve_eval`, `solve_with_slack`, `statics_at`, `SolvedRig`, `MemberForce`, `Reaction` |
| `rig/solve/model.rs` | **new** `Model` (free bodies, knots, members, lumped loads, held bodies), `State`, `Pose` |
| `rig/solve/equilibrium.rs` | **new** `A t = b` assembly, unit forces, held-body resultants |
| `rig/solve/linalg.rs` | **new** Jacobi SVD, rank, min-norm solve, row reduction, dense solve |
| `rig/solve/rank.rs` | `Determinacy { m, dof, r, s, k, consistent }` |
| `rig/solve/elastic.rs` | compliance per column, minimum complementary energy split, tension-only active set |
| `rig/solve/hang.rs` | two-phase hang (soft relax → KKT Newton), multipliers = tensions |
| `rig/solve/lp.rs` | **new** two-phase dense simplex |
| `rig/solve/bounds.rs` | tension envelope (LP), `ChainsSlack` / `BasketsSlack` cases |
| `rig/solve/inverse.rs` | `level`, `take_up`, `SettingChange` |
| `rig/solve/rate.rs` | **new** glue: walks a solved rig → `checks`; `with_bounds`; `bar_axial` |
| `checks.rs` + `checks/*` | `Check`, `Status`, sling / chain / shackle / lug / bar |
| `catalog/chain.rs` | **new** Grade 80 / 100 WLL table |
| `rig/eval.rs` | `+ EvalRig::with_pose` (re-evaluate at a solved pose) |
| `rig/views.rs` | `+ Role::Overloaded`, `Scene::project_solved` |
| `rig/template.rs` | `+ tensions_match_layers`, `TensionGate` |
| `tests/solve_layers.rs` | **new** tension gate over the corpus |

---

## 3. The model

**Bodies.** Every body except the hook, `Pinned` and `Posed` ones is free: 6 DOF. Held bodies are supports with a reported reaction (D3-7). A `Level` body is free; its rotation is where the hang starts and what `inverse::level` aims for.

**Knots.** Free nodes (master links, collectors) have 3 DOF.

**Members.**

| Kind | When | Unknowns | Force on its stops |
|---|---|---|---|
| Tension | everything with length | 1 (T ≥ 0) | `T · Σ unit vectors to path neighbours`: one at an end, two at a bearing |
| Link | two stops, hardware only (shackle, master link, turnbuckle), length ≤ 1e-6 ft | 3 (free) | `+f` on the last stop, `−f` on the first: a ball joint (D3-8) |

A member with no length that is *not* hardware-only (a legacy NO LEN sling) makes the solve refuse with `EmptySegment`. Tensions cannot be computed from a missing length (D3-13).

**Loads.** Body weight at its CG. Each **segment's self-weight is lumped at its lower stop** (D3-3). That matches the layer engine, which conservatively counts the whole sling weight in what the sling carries.

**Equations.** For each free body, 3 force rows and 3 moment rows about its origin. For each knot, 3 force rows. Moment rows are divided by `l_char`, the largest node distance from the hook, so every row is in lb and the SVD compares like with like. With the rotation variable also scaled by `l_char`, the matrix rows are exactly the generalised forces of the hang's coordinates. So `A` doubles as the constraint Jacobian: `J = −Aᵀ`.

---

## 4. Determinacy (Readback §3.5)

`r = rank(A)` at a **physical** tolerance, `MECH_RTOL = 1e-6` relative to σ_max (D3-9). The pseudo-inverse keeps the numeric 1e-10.

Why two tolerances: at a solved pose a *slack* chain can hang 0.000002° off plumb under a bar that is free to swing that way. That leaves one equation whose only coefficient is 3e-8 of the chain force. Counting it pins the chain at zero in every admissible split. On Duplo10 it made the envelope say chain P4 could never carry, which the independent LP check exposed. An imperceptible swing satisfies that equation, so it is a mechanism direction, not a constraint.

`consistent` means `‖A t − b‖∞ ≤ 1e-8 · W`. The pose can be an equilibrium. A hanging rig is normally a mechanism (`k > 0`: pendulum sway, a bar spinning on its own axis) held by gravity. What matters is consistency, not `k = 0`.

`SolvedRig` carries both:

- `determinacy`: the members carrying load at the solution.
- `topology`: every member taut, at the solved pose. This is the one to quote. Duplo10: `m 20, dof 30, r 13, s 7, k 17`.

---

## 5. Hang

Minimise the potential `Π = Σ W z` subject to `ℓᵢ(q) ≤ Lᵢ` (path length through bearings) and `x_a = x_b` for links. The multipliers of the active constraints are the member forces.

Each constraint is **regularised by its compliance**: `ℓᵢ − Lᵢ = cᵢ λᵢ`, with `cᵢ = Σ L / EA` over the member's components (D3-4). That does two jobs at once:

- it *is* the elastic model when components carry `stiffness_lb`;
- it makes the split unique for an indeterminate rig. The rigid default `EA = 1e9 · W_total` stretches a leg carrying the whole load by 1e-9 of its length.

Two phases:

1. **Soft relax.** Damped Newton on `Π + Σ ½ k (ℓ − L)₊²` with `k = 1e4 · W / L`, a positive-definite Hessian (μ raised until the step goes downhill), and an Armijo line search on the energy. This phase exists because pure KKT Newton finds *stationary* points. From a level start with one short leg, it happily swung the load "up and over" to hang 101° off level from a single corner. That is a real equilibrium, but not the one reached from rest. Energy descent cannot climb.
2. **KKT Newton.** Solve `[H + μI, −A; −Aᵀ, −C] [Δq; Δλ] = −[∇L; g − Cλ]`. H is a central finite difference of `∇L = b(q) − A(q)λ`, so gravity plus geometric stiffness. The system is symmetric quasi-definite, so it is always solvable with μ > 0. Backtracking uses the KKT residual, with a trust region of 0.05 `l_char`. Active set: a λ < 0 member goes slack; a slack member reached along a step is caught by bisection at the point it goes taut and activated (a blocking constraint).

Converged: `‖∇L‖∞ ≤ 1e-10 W` and `|g − Cλ| ≤ 1e-11 l_char`. Duplo10 converges in 6 iterations; the whole library suite, bounds and inverse included, runs in under 3 s in a debug build.

`EvalRig::with_pose` re-evaluates members and heights at the hung pose. A bearing member's split is the actual chord split once taut, so `BearingSplitAssumed` is gone after a hang.

---

## 6. Bounds

**Envelope** (`bounds::envelope`). At the solved pose, every tension-only equilibrium (`A t = b`, `t ≥ 0`, links free) is admissible for *some* set of length errors. The range of each tension over that set is two LPs, and it is the honest worst case for inextensible rigging.

- Rows are first reduced to `r` independent ones with the physical tolerance.
- The rhs is scaled by W.
- Two-phase Bland simplex.
- An unbounded result (a self-stress that can pre-tension a member without limit) is reported as `∞`.

**Named cases** (`bounds::named_cases`) hang the rig again with a group forced slack:

- `ChainsSlack`: any chain or adjuster.
- `BasketsSlack`: any member with an interior bearing.

A case that leaves a body with nothing attached, or that does not balance, reports `solved: None`.

Bounds are **advisory** (D3-5). `rate::with_bounds` raises an OK item to WARN with "OVER if the load shifts: X lb possible vs Y lb rated". It never stamps OVER on a bound alone.

---

## 7. Inverse

`inverse::level(&rig)`:

1. Hang the rig.
2. Rotate every free body back to its target attitude about its own CG. The hang already put each CG under its supports.
3. Read the path length each adjustable member needs at that pose, and write the change into the first adjuster on the member as a constant.
4. Hang again, repeating until the tilt is ≤ 0.01° (at most 10 passes).

Test: 5 000 lb on two chain legs, CG 1 ft off centre. One pass levels it. The two legs meet at one hook height, and `V_A · 6 = V_B · 4`.

`inverse::take_up(&rig)` shortens each slack adjuster until it is just snug at the solved pose.

Neither clamps: a setting outside `[min, max]` is returned with `feasible: false`, and `solved` is the last pose that could be hung.

---

## 8. Checks

Pure functions in `checks/`, stamped by `rate`:

| Item | Demand | Capacity | WARN when |
|---|---|---|---|
| Roundsling | leg tension (the angle is already in it) | vertical WLL (vertical & basket legs), choker WLL | leg < 30° from horizontal; bow Ø below §4.7 minimum |
| Chain | tension | Grade 80/100 catalog WLL × hook efficiency (caller supplies; 1.0 default) | — |
| Shackle | T at an end; `2T cos(wrap/2)` at a bearing (rated once per stop) | WLL × side-load factor 100 / 70 / 50 % (in-line ≤ 5°, ≤ 45°, > 45°) | side-loaded |
| Lug (rated) | resultant of every member at it | `LugRating` | > 5° out of plane |
| Bar (rated) | Σ downward member pulls on the bar | `BarRating` | no WLL entered (0) |

Side-load and out-of-plane angles use the lug's plate normal, but only a *horizontal* normal (`X`, `Y`, `−X`, `−Y`: a vertical padeye plate). `Z` is the builder's default and reads as "not given" (D3-6).

Bar axial force comes from the solve. The bar axis is the principal direction of its nodes, and the report is Σ over the nodes on one side of the mid-point of `F · axis`. `checks::bar::compression_from_angle` is the `V / tan θ` cross-check, and it matches to 1e-6 in a test.

---

## 9. Duplo10 with the provisional numbers

These are placeholder dimensions (`Assumed`), so they show what the solver *says*, not a lift plan.

| Result | Value |
|---|---|
| Hook load | 122 983 lb (= load 117 300 + gear 4 673 + rigging) |
| Chains | **all slack**: at the 8 ft setting the baskets bind first |
| Baskets | 21 450 lb each, legs 43.3° |
| Hook legs | 36 054 lb each, 57.7° |
| V legs | 29 978 lb each, plumb |
| Hook → Bar C | 611 lb each (bar C carries only itself) |
| Topology | `m 20, r 13, s 7, k 17` |
| Envelope max | any chain 58 888 lb · basket 42 899 · hook leg 72 109 · hook → bar C 64 134 |
| `BasketsSlack` | the centre chains (P4) take 58 888 lb each; bar C's hook legs 64 134 |
| OVER as rigged | 1-1/2″ hook-leg shackles 36 054 vs 34 000 · 1-1/4″ V-leg shackles 29 978 vs 24 000 · 1-1/4″ basket bow shackles 29 444 vs 24 000 |
| Take-up | P4 chains need 4.34 ft (range 6–10); **P2/P6 need −4.35 ft**: strap + chain are ~12 ft longer than the bar-lug-to-pick gap in the assumed geometry |
| Bars | no WLL entered in the fixture → WARN |

When the drawings arrive, only the parameter table changes. The chain take-up line is the first thing to check against them.

---

## 10. Work order (as built)

**3.1 Linear algebra.** Jacobi SVD, rank, min-norm solve, row reduction, dense solve. Tests: reconstruction, rank-2 3×3, wide min-norm.

**3.2 Model + equilibrium + rank.** `Model::build`, lumped loads, links, `assemble`, `determinacy`. Tests: two-leg `s = 0` exact; four-leg `s = 1`.

**3.3 Elastic split.** Compliance weights, tension-only active set, `statics_at`.

**3.4 Hang.** KKT Newton, then the soft phase and blocking constraints after the "up-and-over" test failed. Tests: CG offset, short leg, Duplo10 balance.

**3.5 Tension gate.** `tensions_match_layers`, `tests/solve_layers.rs`. Statics 1e-9, hang 1e-6.

**3.6 Bounds.** LP, envelope, named cases, physical rank tolerance (found by cross-checking against scipy).

**3.7 Inverse.** `level`, `take_up`.

**3.8 Checks + rate.** `catalog::chain`, five check modules, `rate`, `with_bounds`, `bar_axial`.

**3.9 Scene + docs.** `Role::Overloaded`, `project_solved`, this doc, `rig-model.md`, roadmap.

---

## 11. Decisions

| # | Question | Decision |
|---|---|---|
| **D3-1** | Does a screen consume the solve? | **No.** `Scene::project_solved` is ready (tension labels, `Overloaded`, `SlackMember`); the panel comes after Step 2.5. |
| **D3-2** | Hang, or statics at the descent pose? | **Hang by default.** `statics_at` exists for the equivalence gate, where the symmetric descent pose already is the equilibrium. |
| **D3-3** | Where does member self-weight act? | **Lumped at each segment's lower stop.** Matches the layer engine; conservative for the member. |
| **D3-4** | Indeterminate split? | **Compliance-regularised multipliers**: the elastic answer with real `stiffness_lb`, a rigid limit (`EA = 1e9 W`) otherwise. |
| **D3-5** | Do bounds stamp OVER? | **No, WARN.** "OVER if the load shifts" with the numbers. The envelope is the worst case over *all* fit-ups, which is too harsh to fail a plan on alone. |
| **D3-6** | Lug plate orientation when not authored? | `Z` (builder default) = **not given**: no side-load / out-of-plane reduction. Author `lug_axis` for real padeyes. |
| **D3-7** | What holds a body? | **Hook, `Pinned`, `Posed`** are supports (reactions reported). `Level` is free; its rotation is a start pose and the inverse target. |
| **D3-8** | Zero-length template joints? | **Links**: ball joints with a 3-component force. Two-stop, hardware-only, length ≤ 1e-6 ft. |
| **D3-9** | Rank tolerance? | **1e-6 physical** for rank and the LP; 1e-10 numeric for the pseudo-inverse (§4). |
| **D3-10** | Layering: may `rig` call `checks`? | **Yes.** `checks` depends only on `catalog` + `domain`; `rig::solve::rate` is the glue. |
| **D3-11** | Target-split inverse? | **Deferred** until members carry `stiffness_lb` and Step 7 has tolerances. With rigid rigging, the split past snug is set by 1e-9 ft. |
| **D3-12** | Spreader check basis? | **Total hanging load vs bar WLL.** ⚠ The layer engine checks `load / ends` against the WLL. If the saved-spreader WLL is the bar's total rating, that check is unconservative by the number of ends. **Open question for AJ**; `layers` is unchanged because it is the oracle. |
| **D3-13** | Legacy NO LEN in a solve? | **Refuse** with `EmptySegment` naming the member. No silent rigid substitute. |
| **D3-14** | Choker angle reduction (< 120°)? | **Not applied**: there is no choke-angle input yet. Choker WLL only. |
| **D3-15** | Chain grade notation? | `8` / `10` (as stamped, "G8") = Grade 80 / 100. |

---

## 12. Risks

| Risk | Mitigation |
|---|---|
| Newton lands on a far equilibrium | Soft energy-descent phase first; blocking constraints; test with a short leg that used to flip 101° |
| Fixed-pose LP pins a slack leg at 0 through a 1e-8 coefficient | Physical rank tolerance; envelope cross-checked against scipy HiGHS on Duplo10 |
| Hang stretch perturbs the tension gate | Gate statics at 1e-9 and hang at 1e-6 separately; the rigid factor is 1e9 |
| Envelope reads as a failure | WARN, never OVER, with both numbers |
| Template quirks masquerade as solver error | Named gate skips: basket-as-single-legs, tare at one pick |
| Plate normals left at the default hide side load | D3-6 says so on the page; author `lug_axis` on real padeyes (Duplo10 load lugs already are) |

---

## 13. Hand-off

**To Step 2.5 (parser + parameter table).** The solve is fast enough to re-run on every edit. `SolvedRig` + `rate` + `project_solved` are the data a parameter-table screen shows next to the drawing.

**To Step 4 (crane).** The root becomes the boom head. `Model` already treats the hook body as a held support, and `hook_load_lbs` + `reactions` are what the reeving and chart lookup need. Line pull and parts of line enter as a member between the head and the hook block.

**To Step 7 (tolerances).** `param::sweep` can wrap `solve`. The envelope becomes a *bounded* envelope once length tolerances limit how far a leg can be off, and the target-split inverse opens up with `stiffness_lb`.
