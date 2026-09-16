# BTH Rigging

Desktop Dioxus app for pick planning with WSTDA-RS-1 round-sling tension, hardware reaction checks, outrigger mat bearing pressure, SVG schematics, and InfiniteDb persistence.

## Run

```bash
dx serve --desktop
```

## Flow

1. **Projects** home — create / open / delete job folders  
2. **Project** — list picks and mat analyses for that job  
3. **Pick editor** — input layers (slings, angle, shackles, spreader) on the left; PDF-ready **Lift Report** with diagram on the right  
4. **Mat bearing editor** — select/create a mat, enter outrigger load, pad size, allowable GBP, and spread angle; **Mat Bearing Report** shows effective area and pressure vs allowable  

## Tests

```bash
cargo test
```

## Notes

Calculation aid only. Manufacturer tags, ASME B30.9 / B30.26, geotechnical allowable bearing, and a qualified person govern lifts and crane setup. Sling angle is from the horizontal (90° = vertical).

Sling angles are calculated from fixed sling lengths and pick-point geometry. Enter each layer's **sling length** and **pick spacing** (center to center between adjacent pick points), plus an optional **width** for two-row rectangular patterns (e.g. a 4-leg bridle). When a spreader is on the layer, its **span as rigged** (prefilled from the saved bar, editable per pick) sets that layer's pick points, and the next layer hangs from the bar's end lugs. Each sling's horizontal reach `h` is measured in plan from its attachment above to its pick point, so rectangular patterns use the true 3D reach; the angle from horizontal is `acos(h / L)` and the smallest angle on the layer governs tension. The report flags slings too short for their reach (**GEOM**) and legs whose vertical drops differ by more than 1 in (**LEGS**, unequal load share), and gives an approximate hook-to-pick-point rigging height. With no spacing entered, the manual angle is used.

Rigging self-weight is added layer by layer from the payload up to the hook: slings (count × length × lb/ft, representative Lift-All Tuflex endless roundsling values), apex and leg shackles (Crosby G-209 weights), spreader bars, and any other tare. Each layer's slings and hardware are checked against the payload plus all rigging below them. Layers saved before sling length existed load with length 0 and are flagged **NO LEN** until a length is entered. Mat load spread angle is from the vertical (45° = 1:1).
