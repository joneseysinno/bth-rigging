# Step 0: Module refactor, Rust 2024, and scaffold

> **Status:** plan · **Owner:** AJ · **Applies to:** `bth-rigging` @ `bth-graph` branch
> **Roadmap context:** Step 0 of 8. Steps 1–7 build the rigging graph (bodies, nodes, member paths, components, bearings, parameters) from the lugs up to the boom head and down to the mats. See the *Duplo10 Lift Readback* rev C for the theory.

---

## 1. Goal

Reorganize the crate so Steps 1–7 have a place to land, **without changing a single calculation result, saved file, or screen**.

Step 0 delivers four things:

1. **Rust edition 2024.**
2. **`module.rs` convention everywhere.** No `mod.rs` files. A folder `foo/` always has a sibling `foo.rs` that declares its children.
3. **A lib + bin split.** Engineering core in `lib.rs` with no UI dependency; Dioxus app in `main.rs`.
4. **A scaffold for Steps 1–7.** Doc-only stub modules for the rig graph, solver, crane and checks, plus `docs/roadmap.md`.

### Non-goals

- No new features, UI changes or calculation changes.
- No graph model types or solver code. Stubs contain module docs only.
- No splitting of `pick_editor.rs` internals (deferred; see §10).
- No Cargo workspace yet. The layering below makes that a mechanical change later.

### Definition of done

- [ ] `edition = "2024"` in `Cargo.toml`, with `rust-version` set
- [ ] No `mod.rs` anywhere under `src/`
- [ ] `cargo test` passes, and the 41 existing tests plus the new golden tests are green
- [ ] `cargo test --lib --no-default-features` passes **without GTK/WebKit installed** (headless core)
- [ ] Golden outputs (calc package DTO + saved-layer JSON) are byte-identical to the pre-refactor baseline
- [ ] `dx serve --desktop` runs; Projects → Pick editor → Mat editor → Print PDF all behave as before
- [ ] An existing InfiniteDb data folder opens and shows all projects, picks, spreaders and mats
- [ ] Compiler warnings are no worse than the recorded baseline (6 dead-code warnings)
- [ ] Every module file starts with a `//!` doc comment
- [ ] `docs/roadmap.md` exists and links each scaffold module to its step

---

## 2. Current state

Single binary crate, flat `src/`, one `mod.rs`.

| File | Lines | Role | Depends on UI? |
|---|---:|---|---|
| `main.rs` | 65 | `App`, `Route`, launch | yes |
| `app_state.rs` | 50 | `AppCtx` + `format_lbs/num/updated` | ctx yes, formatters no |
| `models.rs` | 296 | Project, Pick, SlingLayer, Hitch, SavedSpreader, SavedMat, MatAnalysis | no |
| `catalog.rs` | 192 | WSTDA roundsling ratings + lb/ft | no |
| `hardware.rs` | 196 | Shackle ratings/weights, `split_slings`, `sling_stroke_color` | color is UI-ish |
| `calc.rs` | 677 | Layer tension, rigging weight, hardware checks | no |
| `geometry.rs` | 491 | Pick-point geometry, calculated angles | no |
| `mat_calc.rs` | 266 | Duerr mat bearing | no |
| `db.rs` | 688 | InfiniteDb store | no |
| `print.rs` | 662 | Calc-package DTO + Typst PDF | no (uses `format_lbs`) |
| `diagram.rs` | 564 | Rigging + mat SVG components | yes |
| `pages/mod.rs` | 11 | page exports | yes |
| `pages/home.rs` | 147 | Projects list | yes |
| `pages/project.rs` | 237 | Project page, PDF export (`rfd`) | yes |
| `pages/pick_editor.rs` | 1037 | Pick editor + lift report | yes |
| `pages/mat_editor.rs` | 552 | Mat editor + report | yes |

Findings that shape the plan:

- **The core is already UI-free.** The only core → UI edge is `print.rs` importing `format_lbs` from `app_state.rs`, which is a pure function. The lib/bin split is clean.
- **The `desktop` feature doesn't gate anything today.** The `dioxus` dependency hard-codes `features = ["desktop", "router"]`, so the feature flag is cosmetic, and every `cargo test` needs GTK/WebKit to build.
- **All Step 0 work happens on the `bth-graph` branch.** `main` stays untouched until Step 0 is verified.
- **All source files use CRLF line endings.**

---

## 3. Edition 2024 (dry run already done)

I ran a migration on a scratch copy of the current tree. Your files were not touched.

| Check | Result |
|---|---|
| `cargo fix --edition` | **No code changes needed** |
| Build + `cargo test` on edition 2024 | **41 / 41 pass**, same 6 dead-code warnings |
| `cargo fmt --check` with 2024 style | Formatting-only diffs in 13 files (import sorting and similar) |

### Procedure

1. `cargo fix --edition --allow-dirty` on 2021 (expected: no-op), then commit.
2. In `Cargo.toml`:
   ```toml
   [package]
   edition = "2024"
   rust-version = "1.85"   # minimum for 2024; raise to what Dioxus 0.7.9 actually needs
   ```
   Confirm the real floor with `cargo +<ver> check` or `cargo msrv`, and record it.
3. Add `rustfmt.toml`:
   ```toml
   style_edition = "2024"
   ```
4. `cargo build && cargo test`, then commit **"edition 2024"**.
5. `cargo fmt`, then commit **"rustfmt 2024 style"**. Keep this commit formatting-only so later diffs stay readable.

### 2024 behaviors to keep in mind for new code

- **Resolver 3** (MSRV-aware) becomes the default. It has no effect until `cargo update`; the lockfile is kept.
- **`if let` scrutinee and tail-expression temporaries drop earlier.** This matters for signal/store borrows inside `rsx!` handlers.
- **`impl Trait` in return position captures all in-scope lifetimes.** Use `use<..>` bounds if a returned iterator must not borrow.
- **`gen` is a reserved keyword.** It isn't used in the codebase today.
- **`std::env::set_var` / `remove_var` are `unsafe`.** Not used today.

---

## 4. Module conventions

These rules apply to every module from Step 0 on.

1. **No `mod.rs`.** `src/rig.rs` declares `pub mod node;` etc., and children live in `src/rig/node.rs`.
2. **Parent = table of contents.** A parent file holds the `//!` overview, the `mod` declarations, and `pub use` of the module's public API. Minimal logic lives there.
3. **Import from parents, not leaves**, across module boundaries: `use crate::domain::SlingLayer`, not `crate::domain::pick::SlingLayer`.
4. **`pub(crate)` by default.** Only what the bin or future crates need is `pub`.
5. **Every file starts with `//!`**: what it owns, what it must not depend on, and which roadmap step it belongs to.
6. **Tests stay with the code** (`#[cfg(test)] mod tests`). Cross-module golden tests go in `tests/`.
7. **Dependency direction** (enforced by review now, by crates later):

```
                 ┌──────────── bin (UI) ────────────┐
                 │  app ── ui::pages ── ui::components │
                 └───────────────┬──────────────────┘
                                 │ uses
 ┌───────────────────────────── lib (core, no dioxus) ──────────────────────────────┐
 │ report ─┬─ store                                                                  │
 │         ├─ layers ── catalog ── domain ── format                                  │
 │         ├─ mats                                                                   │
 │         └─ rig ── crane ── checks ── catalog ── domain                            │
 └───────────────────────────────────────────────────────────────────────────────────┘
```

- `domain`, `format` and `catalog` depend on nothing else in the crate.
- `store` may depend on `domain` and `catalog`, and never on calculation modules.
- **Nothing in the lib imports `dioxus` or `rfd`.**

---

## 5. Crate shape: lib + bin

### `Cargo.toml` changes

```toml
[lib]
name = "bth_rigging"
path = "src/lib.rs"

[[bin]]
name = "bth-rigging"
path = "src/main.rs"
required-features = ["desktop"]

[dependencies]
dioxus = { version = "=0.7.9", features = ["router"], optional = true }
rfd    = { version = "0.15", optional = true }
# infinite-db, serde, serde_json, dirs, uuid, thiserror, chrono, typst*, stay non-optional

[features]
default = ["desktop"]
desktop = ["dep:dioxus", "dioxus/desktop", "dep:rfd"]
```

What this buys:

- `cargo test --lib --no-default-features` compiles the entire engineering core without Dioxus, GTK or WebKit. That makes CI, cloud sessions and the Step 3 solver loop much faster.
- `dx serve --desktop` is unchanged; the default features include `desktop`.
- Later, `lib` becomes `bth-core` in a workspace by moving `src/` modules. No design changes needed.

> **Verify during 0.3:** `dx` picks the binary correctly with `required-features`. If `dx` complains, drop `required-features` and gate with `#[cfg(feature = "desktop")]` in `main.rs` instead.

---

## 6. Target tree

Legend: **(moved)** existing code relocated, logic unchanged · **(split)** one file becomes several · **(new)** new file · **(stub)** doc-only scaffold for a later step.

```
bth-rigging/
├── Cargo.toml                    edition 2024 · [lib] + [[bin]] · optional dioxus/rfd
├── rustfmt.toml                  (new) style_edition = "2024"
├── .gitattributes                (new) keep CRLF consistent
├── Dioxus.toml                   unchanged
├── assets/                       unchanged
├── docs/
│   ├── step-0-module-refactor.md this plan
│   └── roadmap.md                (new) Steps 1–7 → modules → theory sections
├── tests/
│   ├── golden.rs                 (new) calc-package + layer JSON snapshots
│   └── fixtures/
│       ├── layers_v1.json        (new) saved SlingLayer/Pick/Spreader/Mat JSON as stored today
│       └── calc_package_v1.json  (new) assembled DTO for sample picks (printed_at stripped)
└── src/
    ├── lib.rs                    (new) core root: pub mod format, domain, catalog, layers, mats,
    │                                    store, report, rig, crane, checks
    ├── main.rs                   (moved) bin root: #![allow(non_snake_case)] · mod app; mod ui; launch
    │
    ├── app.rs                    (split from main.rs) App component, Route enum, context provider
    ├── app/
    │   └── context.rs            (split from app_state.rs) AppCtx
    ├── ui.rs                     (new) pub mod pages; pub mod components;
    ├── ui/
    │   ├── pages.rs              (replaces pages/mod.rs)
    │   ├── pages/
    │   │   ├── home.rs           (moved)
    │   │   ├── project.rs        (moved)
    │   │   ├── pick_editor.rs    (moved, internals untouched)
    │   │   └── mat_editor.rs     (moved)
    │   ├── components.rs         (new)
    │   └── components/
    │       ├── rigging_diagram.rs (split from diagram.rs) + sling_stroke_color
    │       └── mat_diagram.rs     (split from diagram.rs)
    │
    ├── format.rs                 (split from app_state.rs) format_lbs / format_num / format_updated
    │
    ├── domain.rs                 (new) pub use of all domain types
    ├── domain/
    │   ├── project.rs            (split from models.rs) Project, now_millis
    │   ├── pick.rs               (split) Pick, SlingLayer, Hitch
    │   ├── spreader.rs           (split) SavedSpreader
    │   └── mat.rs                (split) SavedMat, MatAnalysis
    │
    ├── catalog.rs                (new) pub use
    ├── catalog/
    │   ├── roundsling.rs         (moved from catalog.rs) WSTDA ratings, lb/ft
    │   ├── shackle.rs            (split from hardware.rs) SHACKLES, find_shackle
    │   └── connection_hardware.rs (stub, Step 1/3) WSTDA §4.7 bearing-width table
    │
    ├── layers.rs                 (new) legacy layer engine; pub use calculate_pick, resolve_geometry
    ├── layers/
    │   ├── calc.rs               (moved from calc.rs)
    │   ├── geometry.rs           (moved from geometry.rs)
    │   └── split.rs              (split from hardware.rs) split_slings
    │
    ├── mats.rs                   (new)
    ├── mats/
    │   └── bearing.rs            (moved from mat_calc.rs)
    │
    ├── store.rs                  (split from db.rs) RiggingStore, open/open_default, DbError
    ├── store/
    │   ├── spaces.rs             (split) SpaceIds, ensure_spaces, KIND_* constants
    │   ├── seed.rs               (split) catalog + hardware seeding, orphan-pick migration
    │   ├── keys.rs               (split) uuid_coords, *_point, layer_edge_id
    │   ├── projects.rs           (split) project CRUD
    │   ├── picks.rs              (split) pick + layer CRUD, PickHasLayer hyperedges
    │   ├── hardware.rs           (split) spreader + mat catalog CRUD
    │   ├── mat_analyses.rs       (split) mat analysis CRUD
    │   └── rig.rs                (stub, Step 1) graph persistence: spaces, hyperedge roles
    │
    ├── report.rs                 (new) pub use
    ├── report/
    │   ├── calc_package.rs       (split from print.rs) DTOs + assemble_*
    │   └── pdf.rs                (split from print.rs) Typst render, filename
    │
    ├── rig.rs                    (stub) rigging graph overview
    ├── rig/
    │   ├── param.rs              (stub, Step 1) Parameter, expression, tolerance, source
    │   ├── body.rs               (stub, Step 1) Body kinds, frame, weight, CG
    │   ├── node.rs               (stub, Step 1) Node kinds: lug, bearing, free; coordinates as expressions
    │   ├── member.rs             (stub, Step 1) Member path, segments
    │   ├── component.rs          (stub, Step 1) Strap, chain, shackle, link; order within a segment
    │   ├── bearing.rs            (stub, Step 1/3) capstan band, D/d, edge radius, softener
    │   ├── template.rs           (stub, Step 1) layers → graph conversion (results must match)
    │   ├── eval.rs               (stub, Step 2) parameters → 3D coordinates, lengths, weights
    │   ├── views.rs              (stub, Step 2) side / end / plan projection data
    │   ├── solve.rs              (stub, Step 3) solver pipeline overview
    │   └── solve/
    │       ├── rank.rs           (stub, Step 3) equilibrium matrix, s = m − r, k = dof − r
    │       ├── hang.rs           (stub, Step 3) energy minimization, multipliers → tensions
    │       ├── elastic.rs        (stub, Step 3) series stiffness, tension-only Newton
    │       ├── bounds.rs         (stub, Step 3) slack-chain / slack-basket bounding cases
    │       ├── inverse.rs        (stub, Step 3) chain setting for level / target split
    │       └── envelope.rs       (stub, Step 7) tolerance sweep, worst case per item
    │
    ├── crane.rs                  (stub) crane subgraph overview, boom head as root
    ├── crane/
    │   ├── config.rs             (stub, Step 4) model, boom length, CW, outrigger base, geometry
    │   ├── chart.rs              (stub, Step 4) capacity table, conservative lookup, deductions
    │   ├── reeving.rs            (stub, Step 4) parts of line, η, line pull, rope below head
    │   ├── pose.rs               (stub, Step 5) β from loaded radius, head/hook height, two-block
    │   ├── clearance.rs          (stub, Step 5) boom envelope vs graph, rotation sweep
    │   ├── statics.rs            (stub, Step 6) slew sweep, outrigger reactions → mats
    │   └── wind.rs               (stub, Step 7) projected area, swing angle, side load
    │
    ├── checks.rs                 (stub) rating checks shared by layers, rig and crane
    └── checks/
        ├── sling.rs              (stub, Step 3) roundsling by hitch/angle, basket legs
        ├── chain.rs              (stub, Step 3) grade/size WLL, grab-hook reduction
        ├── shackle.rs            (stub, Step 3) WLL, side-load reduction
        ├── lug.rs                (stub, Step 3) in-plane / out-of-plane
        └── bar.rs                (stub, Step 3) spreader compression P = V / tan θ
```

### Move map (old path → new path)

| Old | New | Notes |
|---|---|---|
| `src/main.rs` | `src/main.rs` + `src/app.rs` | `Route` and `App` move to `app.rs` |
| `src/app_state.rs` | `src/app/context.rs` + `src/format.rs` | formatters go to lib |
| `src/models.rs` | `src/domain/{project,pick,spreader,mat}.rs` | **no serde changes** |
| `src/catalog.rs` | `src/catalog/roundsling.rs` | |
| `src/hardware.rs` | `src/catalog/shackle.rs` · `src/layers/split.rs` · `src/ui/components/rigging_diagram.rs` | 3-way split |
| `src/calc.rs` | `src/layers/calc.rs` | |
| `src/geometry.rs` | `src/layers/geometry.rs` | |
| `src/mat_calc.rs` | `src/mats/bearing.rs` | |
| `src/db.rs` | `src/store.rs` + `src/store/*.rs` | `impl RiggingStore` blocks spread across files |
| `src/print.rs` | `src/report/{calc_package,pdf}.rs` | **fix `include_str!` path** |
| `src/diagram.rs` | `src/ui/components/{rigging_diagram,mat_diagram}.rs` | |
| `src/pages/mod.rs` | `src/ui/pages.rs` | |
| `src/pages/*.rs` | `src/ui/pages/*.rs` | `crate::` paths → `bth_rigging::` for core |

---

## 7. Execution order

Each numbered item is one commit that **builds and passes tests**. Never move and edit logic in the same commit.

### 0.1 Safety net

1. Check out `bth-graph` (create it from the current code if it doesn't exist yet; `git init` first if the folder isn't a repository), add `.gitattributes` (`* text=auto eol=crlf` for `*.rs *.toml *.md *.typ *.css`), commit **"baseline"**.
2. Add golden tests while still on the old layout:
   - **`layers_v1.json` fixture:** exact JSON currently written by `save_pick` / `save_spreader` / `save_mat` / `save_mat_analysis` for representative records, including optional fields present and absent (`sling_length_ft`, `pick_spacing_ft`, `spreader_span_ft`, legacy `spreader_wll_lbs`). The test deserializes the fixture, re-serializes it, and compares. This proves stored data is readable before and after the refactor.
   - **`calc_package_v1.json` fixture:** `assemble_calc_package` output for 3 sample picks with `printed_at` blanked:
     1. single layer, manual angle
     2. 2-layer with spreader span and calculated angles
     3. impossible geometry (GEOM)

     Plus one mat analysis. Regenerate only with `UPDATE_GOLDEN=1`.
3. Record the warning baseline: `cargo build 2>&1 | grep -c ^warning` → note in the commit message.

### 0.2 Edition 2024

Follow §3: fix → bump → commit, then fmt → commit.

### 0.3 Lib + bin split (flat)

1. Create `src/lib.rs` declaring the **existing flat** core modules: `models, catalog, hardware, calc, geometry, mat_calc, db, print`, plus a new `format` pulled out of `app_state.rs`.
2. `main.rs` keeps `mod app_state; mod diagram; mod pages;` and imports core via `bth_rigging::…`.
3. Apply the `Cargo.toml` feature changes (§5).
4. Verify `cargo test` **and** `cargo test --lib --no-default-features`.

### 0.4 Core into folders

One commit per group, in dependency order:

1. `format`
2. `domain` (from `models`)
3. `catalog` (roundsling + shackle)
4. `layers` (calc, geometry, split)
5. `mats`
6. `store` (split `db.rs` by concern; `impl RiggingStore` may span files)
7. `report` (split `print.rs`; `include_str!("../../assets/calc-package.typ")`)

After each: `cargo test --lib --no-default-features`, and golden tests stay green.

### 0.5 UI into folders

1. `app.rs` + `app/context.rs`; update the 4 `use crate::Route` imports to `crate::app::Route`.
2. `ui/pages.rs` + `ui/pages/*`, removing `pages/mod.rs`.
3. `ui/components/*` from `diagram.rs`; move `sling_stroke_color` here.
4. Run the app and click through the smoke test (§9).

### 0.6 Scaffold

1. Add the stub files marked **(stub)** in §6. Each contains only:
   - `//!` title and one-paragraph responsibility
   - `//! Step:` roadmap step number
   - `//! Theory:` pointer to the roadmap / readback section (e.g. "§3.5 determinacy")
   - `//! Inputs / Outputs:` in words
   - `//! Must not depend on:` layering rule
2. Parents (`rig.rs`, `rig/solve.rs`, `crane.rs`, `checks.rs`, `store/rig.rs`, `catalog/connection_hardware.rs`) declare the stubs with `pub mod`.
3. Stubs define **no items**, so there are no dead-code warnings and no API to maintain yet.
4. Write `docs/roadmap.md`: a table of Step → goal → modules → theory section → definition of done. Paste the Duplo10 parameter and crane input checklists as the Step 1/4 data needs.

### 0.7 Verify and close

Run §9 in full, then tag `step-0` on `bth-graph`.

---

## 8. Gotchas

| Risk | Where | Mitigation |
|---|---|---|
| `include_str!` is relative to the **source file** | `print.rs` → `report/pdf.rs` | Change to `"../../assets/calc-package.typ"`; the compile error catches it |
| `asset!("/assets/…")` is relative to the **crate root** | `app.rs` | No change needed; verify the CSS loads |
| `crate::Route` used by pages | 4 files | Becomes `crate::app::Route`, still in the bin crate |
| Core using a UI helper | `print.rs` → `format_lbs` | `format` moves to lib first (0.3) |
| **Saved data format** | serde field names, enum variant names (`Hitch::Vertical`), `KIND_*` strings, `SpaceId` numbers, `layer_edge_id` formula, DB path `data_dir/bth-rigging/db` | Frozen in Step 0. The `layers_v1.json` golden test + opening a real DB copy prove it |
| Seeded catalog rows | `seed_catalog_if_empty` / `seed_hardware_if_empty` | Logic moves verbatim; don't reorder the seed insert loops |
| `#![allow(non_snake_case)]` | crate attribute for Dioxus components | Keep it in `main.rs`; add it to `lib.rs` only if needed (the lib has no components) |
| `impl RiggingStore` split across files | `store/*.rs` | Allowed in Rust; private fields need `pub(super)` or accessor methods |
| Test-only helpers used across modules | `sample_layer`, `layer()` | Put them in `domain/pick.rs` under `#[cfg(test)] pub(crate) mod fixtures` |
| CRLF churn | all files | `.gitattributes` in 0.1; review diffs with `--ignore-cr-at-eol` |
| `dx` + `required-features` | bin target | Fallback in §5 |

---

## 9. Verification checklist

**Automated**

- [ ] `cargo test`: 41 existing tests + golden tests pass
- [ ] `cargo test --lib --no-default-features` passes on a machine or container **without** WebKit/GTK
- [ ] `cargo clippy --all-targets` shows no new lints versus baseline
- [ ] `cargo fmt --check` is clean
- [ ] `grep -r "mod.rs" src` returns nothing; `find src -name mod.rs` returns nothing
- [ ] `grep -rE "dioxus|rfd" src/{lib,format,domain,catalog,layers,mats,store,report,rig,crane,checks}*` returns nothing

**Manual smoke test** (`dx serve --desktop`, using a **copy** of the real data folder)

- [ ] Home lists existing projects
- [ ] Open a project → picks and mat analyses listed
- [ ] Open a saved pick: layers, spreader span, calculated angles, rigging weight and hook load match a screenshot taken before 0.1
- [ ] Edit spacing → angle updates; save → reopen → persists
- [ ] Mat editor opens a saved analysis; numbers match
- [ ] Print calc package → PDF saves and opens; page matches the pre-refactor PDF

---

## 10. Decisions for AJ

| # | Decision | Recommendation |
|---|---|---|
| D1 | Lib + bin split now, or keep a single bin? | **Split now.** Headless tests and the future `bth-core` crate come almost free. |
| D2 | Split `pick_editor.rs` (1,037 lines) into components in Step 0? | **Defer to Step 2.** The rig editor will reshape it; moving it now doubles the churn. |
| D3 | Keep the layer engine (`layers/`) after the graph lands? | **Keep it as a template** (Step 1 `rig/template.rs`) and require graph results to match it. That gives a permanent regression oracle. |
| D4 | Move to a Cargo workspace (`bth-core`, `bth-desktop`) in Step 0? | **Not yet.** Do it when a second front end or a CLI appears. |
| D5 | Where the theory lives | Export the readback theory (Parts 3–4) to `docs/theory/rigging-graph.md` at the start of Step 1, so it's versioned with the code. |
