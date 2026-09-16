# Roadmap: Steps 1–7

Step 0 (this branch) reorganized the crate. Steps 1–7 build the rigging graph from the lugs up to the boom head and down to the mats. Theory source: *Duplo10 Lift Readback* rev C (export to `docs/theory/rigging-graph.md` at the start of Step 1 — decision D5).

## Step → modules

| Step | Goal | Modules | Theory | Definition of done |
|---:|---|---|---|---|
| **0** | Module layout, edition 2024, lib/bin, scaffolds | `domain`, `catalog`, `layers`, `mats`, `store`, `report`, `format`, `app`, `ui`, stubs below | this plan | Tests + goldens green; no `mod.rs`; headless lib tests |
| **1** | Graph model: bodies, nodes, members, components, parameters | `rig::{body,node,member,component,param,bearing,template}`, `store::rig`, `catalog::connection_hardware` | Readback Part 3 — graph elements; bearing width §4.7 | Template conversion: graph tensions match `layers` for sample picks |
| **2** | Eval parameters → 3D; side/end/plan views | `rig::{eval,views}` | Readback — coordinates as expressions; projections | Eval produces lengths/weights; views drive diagram |
| **3** | Solver + shared checks | `rig::solve::{rank,hang,elastic,bounds,inverse}`, `checks::{sling,chain,shackle,lug,bar}` | §3.5 determinacy (`s = m − r`, `k = dof − r`); tension-only | Rank/hang/elastic on template graphs; checks stamp OK/OVER |
| **4** | Crane config, chart, reeving | `crane::{config,chart,reeving}` | Readback crane capacity / reeving | Conservative chart lookup + line pull for a sample crane |
| **5** | Pose and clearance | `crane::{pose,clearance}` | Loaded radius → β; boom envelope | Head/hook height; clearance sweep vs graph |
| **6** | Crane statics → mats | `crane::statics` (+ existing `mats`) | Slew sweep; outrigger reactions | Reactions feed mat bearing analysis |
| **7** | Wind + tolerance envelope | `crane::wind`, `rig::solve::envelope` | Side load; tolerance sweep | Worst-case utilization per item |

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
domain, format, catalog → (nothing else in crate)
```

Nothing in the lib imports `dioxus` or `rfd`.
