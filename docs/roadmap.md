# Roadmap: Steps 0–7

Step 0 reorganized the crate. Steps 1–7 build the rigging graph from the lugs up to the boom head and down to the mats. Theory source: *Duplo10 Lift Readback* rev C (export to `docs/theory/rigging-graph.md` at the start of Step 1 — decision D5).

## Step → modules

| Step | Status | Goal | Modules | Theory | Definition of done |
|---:|---|---|---|---|---|
| **0** | ✅ done | Module layout, edition 2024, lib/bin, scaffolds | `domain`, `catalog`, `layers`, `mats`, `store`, `report`, `format`, `app`, `ui`, stubs below | this plan | Tests + goldens green; no `mod.rs`; headless lib tests |
| **1** | ✅ done | Graph model: bodies, nodes, members, components, parameters | `rig::{body,node,member,component,param,bearing,template,build,fixtures}`, `store::rig`, `catalog::connection_hardware` | Readback Part 3 — graph elements; bearing width §4.7. Model reference: `docs/rig-model.md` | Template conversion: graph weights match `layers` for sample picks (tensions in Step 3) |
| **2** | ✅ done | Eval parameters → 3D; side/end/plan views | `rig::{eval,views}` | Readback — coordinates as expressions; projections | `EvalRig` places the graph; `Scene` goldens for Duplo10 and 2-over-4; geometry matches `layers` to 1e-9 |
| **3** | ✅ done | Solver + shared checks | `rig::solve::{rank,hang,elastic,bounds,inverse}`, `checks::{sling,chain,shackle,lug,bar}` | §3.5 determinacy (`s = m − r`, `k = dof − r`); tension-only | Rank/hang/elastic on template graphs; checks stamp OK/OVER |
| **2.5** | ▶ next | Expression parser + parameter-table editor | `rig::param` (`Expr` parse/print), `ui` parameter table | Step 1 D2; Step 2 D2-8 (split out of Step 2) | Typed text (`s12 + 2*G`) parses to the same `Expr` the builder API makes; print → parse round-trips; errors point at the column; an edit re-evaluates `EvalRig` |
| **4** | — | Crane config, chart, reeving | `crane::{config,chart,reeving}` | Readback crane capacity / reeving | Conservative chart lookup + line pull for a sample crane |
| **5** | — | Pose and clearance | `crane::{pose,clearance}` | Loaded radius → β; boom envelope | Head/hook height; clearance sweep vs graph |
| **6** | — | Crane statics → mats | `crane::statics` (+ existing `mats`) | Slew sweep; outrigger reactions | Reactions feed mat bearing analysis |
| **7** | — | Wind + tolerance envelope | `crane::wind`, `rig::solve::envelope` | Side load; tolerance sweep | Worst-case utilization per item |

**Order:** 0 → 1 → 2 → 3 → **2.5** → screen work (Step 2 D2-1) → 4 → 5 → 6 → 7.
Step 3 goes before 2.5. Step 2's residual list and taut/slack classification hand straight to the tension-only
solve, and fixtures plus the builder API are enough to test it. The parameter-table editor matters once a screen
shows the graph, and D2-1 holds screens until tensions exist. So 2.5 lands just before the first screen that uses the graph.

**Verified 2026-09-26 (Steps 0–2):** `cargo test --no-default-features` → 108 lib + 10 `eval_geometry` + 3 golden, all green, headless;
`cargo fmt --check` clean; 0 compiler warnings (baseline was 6).
**Verified 2026-09-26 (Step 3):** 136 lib + 10 `eval_geometry` + 3 golden + 8 `solve_layers`, all green, headless; fmt clean;
0 compiler warnings; no new clippy findings in Step 3 code. Plan and as-built notes: `docs/step-3-solve-checks.md`.

## Scaffold map

```
rig/          Steps 1–3, 7
crane/        Steps 4–7
checks/       Step 3
store/rig     Step 1 persistence
catalog/connection_hardware  Steps 1/3
layers/       kept as template oracle (D3)
```

## Step 1 data needs (from Duplo10 parameter checklist)

- Body: kind, frame, weight, CG
- Node: kind (lug / bearing / free), coordinate expressions, tolerance/source
- Member: path, segments
- Component: strap / chain / shackle / link order within a segment
- Bearing: capstan band, D/d, edge radius, softener
- Parameter: expression, tolerance, source

## Step 4 crane input checklist

- Model, boom length, counterweight, outrigger base / geometry
- Capacity chart (conservative lookup + deductions)
- Parts of line, efficiency η, line pull, rope below head

## Layering (enforced by review)

```
bin (app, ui) → lib
report → store, layers, mats, catalog, domain, format
store → domain, catalog   (not calc modules)
layers / mats / rig / crane / checks → catalog, domain
rig → checks              (Step 3 D3-10: rig::solve::rate is the glue)
domain, format, catalog → (nothing else in crate)
```

Nothing in the lib imports `dioxus` or `rfd`.
