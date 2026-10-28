# BTH Rigging

Desktop Dioxus app for pick planning with WSTDA-RS-1 round-sling tension, hardware reaction checks, outrigger mat bearing pressure, SVG schematics, and InfiniteDb persistence.

## Run

```bash
dx serve --desktop
```

## Flow

The UI is built on the [JoInn Template](https://github.com/joneseysinno/JoInn-Template) `app-shell`: a nav rail with panes on the far left, and open documents with a per-type toolbar, side and bottom panels, and a status readout. `Ctrl+K` opens the command palette; `Alt+[` / `Alt+]` switch documents.

1. **Projects** pane — create / delete job folders; expand one to add picks, mat analyses and rigs, open them, or print the project calc package  
2. **Pick** document — **Input** panel (layers: slings, angle, shackles, spreader) on the left; PDF-ready **Lift Report** with diagram in the viewport  
3. **Mat analysis** document — **Input** panel (select/create a mat, outrigger load, pad size, allowable GBP); **Mat Bearing Report** shows effective area and pressure vs allowable  
4. **Rig** document — rotatable 3D rig tree in the viewport, **Live results** on the right, **Checks** on the left and the **Parameters** table in the bottom panel; Save / Undo / Redo in the toolbar  

Unsaved edits stay with a document after it is closed, until the app exits.

`Cargo.toml` patches `app-shell` to the local `D:/Rust/Dioxus/JoInn Template` checkout so the shell and app can change together; remove the `[patch]` section to build from the pinned git tag.

## Tests

```bash
cargo test
```

## Notes

Calculation aid only. Manufacturer tags, ASME B30.9 / B30.26, geotechnical allowable bearing, and a qualified person govern lifts and crane setup. Sling angle is from the horizontal (90° = vertical).

Sling angles are calculated from fixed sling lengths and pick-point geometry. Enter each layer's **sling length** and **pick spacing** (center to center between adjacent pick points), plus an optional **width** for two-row rectangular patterns (e.g. a 4-leg bridle). When a spreader is on the layer, its **span as rigged** (prefilled from the saved bar, editable per pick) sets that layer's pick points, and the next layer hangs from the bar's end lugs. Each sling's horizontal reach `h` is measured in plan from its attachment above to its pick point, so rectangular patterns use the true 3D reach; the angle from horizontal is `acos(h / L)` and the smallest angle on the layer governs tension. The report flags slings too short for their reach (**GEOM**) and legs whose vertical drops differ by more than 1 in (**LEGS**, unequal load share), and gives an approximate hook-to-pick-point rigging height. With no spacing entered, the manual angle is used.

Rigging self-weight is added layer by layer from the payload up to the hook: slings (count × length × lb/ft, representative Lift-All Tuflex endless roundsling values), apex and leg shackles (Crosby G-209 weights), spreader bars, and any other tare. Each layer's slings and hardware are checked against the payload plus all rigging below them. Layers saved before sling length existed load with length 0 and are flagged **NO LEN** until a length is entered. Mat load spread angle is from the vertical (45° = 1:1).
