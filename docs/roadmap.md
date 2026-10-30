# Roadmap: Steps 0–7

Step 0 reorganized the crate. Steps 1–7 build the rigging graph from the lugs up to the boom head and down to the mats. Theory source: *Duplo10 Lift Readback* rev C (export to `docs/theory/rigging-graph.md` at the start of Step 1 — decision D5).

## Step → modules

| Step | Status | Goal | Modules | Theory | Definition of done |
|---:|---|---|---|---|---|
| **0** | ✅ done | Module layout, edition 2024, lib/bin, scaffolds | `domain`, `catalog`, `layers`, `mats`, `store`, `report`, `format`, `app`, `ui`, stubs below | this plan | Tests + goldens green; no `mod.rs`; headless lib tests |
| **1** | ✅ done | Graph model: bodies, nodes, members, components, parameters | `rig::{body,node,member,component,param,bearing,template,build,fixtures}`, `store::rig`, `catalog::connection_hardware` | Readback Part 3 — graph elements; bearing width §4.7. Model reference: `docs/rig-model.md` | Template conversion: graph weights match `layers` for sample picks (tensions in Step 3) |
| **2** | ✅ done | Eval parameters → 3D; side/end/plan views | `rig::{eval,views}` | Readback — coordinates as expressions; projections | `EvalRig` places the graph; `Scene` goldens for Duplo10 and 2-over-4; geometry matches `layers` to 1e-9 |
| **3** | ✅ done | Solver + shared checks | `rig::solve::{rank,hang,elastic,bounds,inverse}`, `checks::{sling,chain,shackle,lug,bar}` | §3.5 determinacy (`s = m − r`, `k = dof − r`); tension-only | Rank/hang/elastic on template graphs; checks stamp OK/OVER |
| **2.5** | 🟡 awaiting AJ §8.4 | Expression parser + parameter-table editor | `rig::param::{syntax,print}`, `rig::{edit,session}`, `ui` parameter table | Step 1 D2; Step 2 D2-8 (split out of Step 2) | Typed expressions parse to builder-equivalent ASTs; canonical print round-trips; edits recompute and rate the rig |
| **D2-1** | 🟡 awaiting AJ check | Screen work: labeled 3D rigging tree in the rig editor | `rig::tree`, `rig::views::orbit`, `ui::components::rig_tree_view`, `ui::pages::rig_editor` | Step 2 D2-1; `docs/rig-model.md` "Rig tree and tags" | Every member tagged `M1…` top-down from the hook with from / via / to and hardware text; rotatable 3D view (Iso / Side / End / Plan) with tags matching a members list grouped by what each member hangs from; rated items carry tags |
| **G** | ✅ done | Rig graph builder: build and rewire the rig on a node-graph canvas | `rig::{topo,preset,layout}`, `rig::session` (`RigEdit`), `store::rig` (space 13), `ui::components::rig_canvas`, `ui::selection`, `docs::rig::inspector` | `docs/step-g-graph-builder.md`; `docs/rig-model.md` "Topology edits" | A spreader-bar rig built from a blank rig on the canvas solves the same as the equivalent `from_layers` rig; undo/redo covers every topology edit |
| **G2** | 🟡 awaiting AJ check | Load-up builder: tick open points, gather them into a master link / spreader bar / lifting beam / basket / the hook; one-click recipes | `rig::gather`, `rig::session` (`apply_batch`), `docs::rig::build` (Build panel) | `docs/step-g2-load-up-builder.md` | Every recipe on 2- and 4-point loads validates, converges and hangs level; an offset CG hangs level; each gather is one undo step and Ungather restores the rig exactly |
| **4** | — | Crane config, chart, reeving | `crane::{config,chart,reeving}` | Readback crane capacity / reeving | Conservative chart lookup + line pull for a sample crane |
| **5** | — | Pose and clearance | `crane::{pose,clearance}` | Loaded radius → β; boom envelope | Head/hook height; clearance sweep vs graph |
| **6** | — | Crane statics → mats | `crane::statics` (+ existing `mats`) | Slew sweep; outrigger reactions | Reactions feed mat bearing analysis |
| **7** | — | Wind + tolerance envelope | `crane::wind`, `rig::solve::envelope` | Side load; tolerance sweep | Worst-case utilization per item |

**Order:** 0 → 1 → 2 → 3 → **2.5** → screen work (Step 2 D2-1) → **G** (graph builder) → **G2** (load-up builder) → 4 → 5 → 6 → 7.
Step 3 goes before 2.5. Step 2's residual list and taut/slack classification hand straight to the tension-only
solve, and fixtures plus the builder API are enough to test it. The parameter-table editor matters once a screen
shows the graph, and D2-1 holds screens until tensions exist. So 2.5 lands just before the first screen that uses the graph.

**Verified 2026-09-26 (Steps 0–2):** `cargo test --no-default-features` → 108 lib + 10 `eval_geometry` + 3 golden, all green, headless;
`cargo fmt --check` clean; 0 compiler warnings (baseline was 6).
**Verified 2026-09-26 (Step 3):** 136 lib + 10 `eval_geometry` + 3 golden + 8 `solve_layers`, all green, headless; fmt clean;
0 compiler warnings; no new clippy findings in Step 3 code. Plan and as-built notes: `docs/step-3-solve-checks.md`.
**Verified 2026-09-30 (Step 2.5 close-out, 2.5.12–2.5.19):** `cargo test --no-default-features` → 173 lib passed + 1 ignored, 10 `eval_geometry`, 3 golden, 11 `param_editor_flow` (§8.4 as code), 8 `solve_layers`, all green; `cargo fmt --check` and `cargo check --features desktop` pass; 10,000 generated expression trees round-trip; no new clippy findings (baseline 7 lib / 9 lib-test warnings remain). Duplo10 edit latency, including `apply`: release 11.2 ms median / 22.6 ms max; debug (`dx serve` profile) worst of 3 runs 14.2 ms median / 29.3 ms max. Recompute is inline (the threaded path is removed). AJ's §8.4 manual checklist remains to be run. As-built notes: `docs/step-2.5-parser-param-table.md` §13.

**Verified 2026-10-15 (screen work D2-1, rig tree):** `cargo test --no-default-features` → 186 lib passed + 1 ignored, 10 `eval_geometry`, 3 golden (unchanged), 11 `param_editor_flow`, 8 `solve_layers`; `cargo fmt --check` and `cargo check --features desktop` pass; clippy baseline unchanged (7 lib / 9 lib-test), no findings in new code. Rendered under `dx serve` on Duplo10 (Iso and Plan); the principal-axis bar fix landed after that screenshot and is covered by `duplo10_top_bar_runs_along_x_in_plan`.

**Verified 2026-10-29 (Step G2, headless):** `cargo test --no-default-features` → 220 lib passed + 1 ignored (12 new in `rig::gather`), 10 `eval_geometry`, 3 golden (unchanged), 11 `param_editor_flow`, 8 `solve_layers`; no new clippy findings in G2 code; `rustfmt --check` clean on changed files. The Build panel still needs a `dx serve` check against the local JoInn shell.

## Scaffold map

```
rig/          Steps 1–3, 7
crane/        Steps 4–7
checks/       Step 3
store/rig     Step 1 persistence; Step G canvas layout (space 13)
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
