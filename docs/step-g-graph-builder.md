# Step G: Rig graph builder (node-graph canvas)

> **Status:** implemented · verified 2026-10-28 · **Applies to:** `bth-rigging`
> **Prerequisite:** Steps 1–3 and 2.5 (graph model, eval, solve, parameter edits, session), screen work D2-1 (3D rig tree).
> **Roadmap context:** before this step a rig could only come from a pick's layer list (`from_layers`) or the Duplo10 demo, and only its parameter values could change. Step G lets the user build and rewire the graph itself.

---

## 1. Goal

Build a rig the way a rigger draws it: put the hook, spreader bars, lifting
beams, master links and the load on a diagram, then hang hardware runs
(shackle, sling, shackle) between their attachment points. Every change
re-solves and re-rates the rig live, and undo/redo covers all of it.

### Non-goals

- Retiring the pick layer input. "Rig from pick" still creates a starting rig; whether the layer input goes away is decided later.
- Crane configuration (Steps 4–7).
- Placing ports by dragging in 3D. Port coordinates are numbers or expressions in the inspector.

### Definition of done

- [x] A rig can be built from a blank rig on the canvas (hook, two legs, spreader bar, legs to the load, shackles at every connection) and solves the same as the equivalent `from_layers` rig (`topo::tests::bar_template_rebuilt_by_edits_solves_the_same`, `spreader_rig_from_presets_solves`).
- [x] Undo and redo cover every topology edit (`session::tests::topology_edits_recompute_and_undo_exactly`).
- [x] `cargo test --no-default-features`, `cargo fmt --check` and `cargo check --features desktop` pass.

## 2. How the canvas maps to the engine

| On the canvas | In the engine |
|---|---|
| Block: hook, spreader bar, lifting beam, load, frame | `Body` |
| Port on a block: lug, bow, edge, the hook's bowl | `Node` on that body |
| Master link (junction) block | `Node` of kind `Free` |
| Wire between ports | `Member`; its `path` is the ports it touches |
| Chips on a wire, top to bottom | `Component`s in each `Segment` |

"Strap to the spreader bar by a shackle" is a member from a bar lug whose
segment holds shackle, strap, shackle. A wire through a bow port is a basket:
a path of three or more stops with one segment per span.

## 3. Headless layer

### 3.1 `rig::topo` — topology edits

`TopoEdit` + pure `topo::apply(&Rig, &TopoEdit) -> Result<Rig, TopoError>`,
in the same style as `edit::apply` for parameters:

| Group | Edits |
|---|---|
| Blocks | `AddBody { body, nodes, params }`, `RemoveBody` (cascades to its nodes and every member touching them), `RenameBody`, `SetBodyExpr`, `SetBarRating` |
| Ports | `AddNode` (a body port, or a free master link), `RemoveNode` (refused while in use; names the members), `RenameNode`, `SetNodeExpr`, `SetNodeKind` |
| Wires | `Connect { member }`, `Reroute { id, path }`, `RemoveMember`, `RenameMember` |
| Chips | `InsertComponent`, `RemoveComponent`, `MoveComponent`, `ReplaceComponent`, `SetComponentExpr` |

Edits refuse only what breaks the structure: dangling references, a path
shorter than two stops or running from a node to itself, an interior stop that
is not a bow or edge, an end on a bearing (unless a self-choker), a second hook,
quantity or parse errors in expression fields, or a negative / non-finite
weight or length. An **incomplete rig is a valid draft**: an unwired bar or a
load not yet hung is accepted, and `Rig::validate_full` reports it
(`UnsupportedBody`, `Disconnected`) in Checks.

`Reroute` keeps the segments when the count still fits. Otherwise it re-splits
them so total length and weight are unchanged (shackles are copied whole into
each run). Changing a catalog roundsling's length re-derives its weight.

### 3.2 `rig::preset` — palette presets

| Preset | Creates |
|---|---|
| Spreader bar | `bar{n}_span` (10 ft), `bar{n}_weight` (250 lb), WLL 20,000 lb; lugs Top A/B and Bottom A/B at x = ±span/2 |
| Lifting beam | `beam{n}_span`, `beam{n}_weight`; a centre top lug 1 ft up, Bottom A/B at ±span/2 |
| Load (2 points / 4 corners) | `load_weight`, `load_length`, `load_width`, `load_height` (later loads `load2_…`); CG at half height; pick points on top at ±L/2 (and ±W/2) |
| Frame / tare | `frame{n}_weight`; one Hang lug |
| Master link (junction) | a `Free` node |
| Default run | shackle 1″, RS-5 roundsling 10 ft, shackle 1″; through bows the sling is a basket split evenly, with shackles only at the ends |

Generated names go through `edit::validate_name` and take a numeric suffix on a
clash. Every generated parameter is `Assumed`, so the provisional banner shows
until the user replaces them with drawing values. `blank_rig` is the hook plus a
two-point load. `replace_with` swaps hardware while keeping a length-bearing
item's length; `resize_roundsling` changes size or hitch and re-derives weight.

### 3.3 `rig::layout` — schematic layout

`Schematic::build(&rig, &CanvasLayout)` places one block per body or master link:

- **Tiers** breadth-first from the hook (hook at 0); blocks not reached sit below the reached ones, and loads always sit below everything else.
- **Rows**: each block's x follows the mean x of the blocks above it that share a member, then the row is spread so blocks don't overlap.
- **Ports**: a port sits on a block's bottom edge when a member hangs from that block through it (by `RigTree` parent), else on the top. Unused ports go by name ("Top …") and body kind (loads and frames on top).
- **Dragged positions** (`CanvasLayout`) override the automatic centre per block.

### 3.4 Session

`RigSession::apply_edit(impl Into<RigEdit>)` applies a `RigEdit::Param` or
`RigEdit::Topo` as one undo step and recomputes. `apply(ParamEdit)` and
`apply_topo(TopoEdit)` are typed wrappers, so the parameter table keeps its
field-level `EditError`. An edit that leaves the rig unchanged is not recorded.
`rename` renames the rig as an undoable edit.

### 3.5 Persistence

Canvas positions live in a new store space, `SPACE_RIG_LAYOUT = 13`, one JSON
row per rig (`save_rig_layout` / `load_rig_layout`). They are written as soon
as a block is dropped, are not part of the undo history, and do not mark the
rig unsaved. `save_rig` leaves them alone; `delete_rig` removes them. Spaces
1–12 and `SCHEMA_VERSION` are unchanged.

## 4. Screen

The Rig document's viewport has a **Diagram / 3D / Split** toggle (Diagram for a
rig with no members, Split otherwise). The Inspector is the default right panel,
next to Live results.

### 4.1 Canvas (`ui::components::rig_canvas`)

- **Palette** above the diagram: Spreader bar, Lifting beam, Load (2 points), Load (4 corners), Frame / tare, Master link (junction); then the hardware items. Click (or press and drag) an item, then click the canvas to place a body, or click a green **+** on a wire to insert hardware. Clicking a chip with hardware armed inserts below it.
- **Connect**: drag from one port to another, or click a port then click the next. Clicking a bow or edge first makes a basket; the path continues until a lug, the hook or a master link ends it. While connecting, valid targets turn green and the rest dim. Esc cancels.
- **Wires** show the member's tag and solved tension near the upper end, and their hardware as chips. Slack members are dashed, over-capacity members red, links thin and grey.
- **Blocks** show weight, span and WLL, take the rated status colour, and carry a "not rigged" or "not hung from the hook" badge while the rig is a draft. Drag a block to move it.
- **Right-click** anything: add a body or master link at the pointer; add ports or remove a body; connect from a port or change its kind; insert a shackle at the top or bottom, route through a bow, straighten or remove a member; insert a shackle above or below a chip, move it, replace it with another kind, or remove it.
- **Keyboard**: Delete / Backspace removes the selection, Esc cancels the current gesture or clears the selection. Scroll zooms around the pointer; drag the background to pan; **Fit** frames the whole rig.

### 4.2 Shared selection (`ui::selection`)

`Selection::{Body, Node, Member, Component}` replaces the member-only
selection. The canvas, the 3D view, the Live results member list and the
inspector all read and write it, so picking a member anywhere highlights it
everywhere. `Selection::delete_edit` is the one place that knows how to delete
each kind (never the hook).

### 4.3 Inspector (`docs::rig::inspector`)

- **Nothing selected**: rig name, counts, weight below the hook, a short how-to.
- **Body**: name, weight, span / WLL or L / W / H, CG; its ports (click to select); add lug / bow / edge; remove the body and its members.
- **Port or master link**: name, kind, body-frame coordinates, bow diameter / edge radius and μ; the members using it; remove.
- **Member**: name, length, weight, lowest rating, tension; the route as one dropdown per stop, remove a bearing, pass through another bow or edge; the hardware of each run with move / remove and "add hardware at the bottom"; remove.
- **Hardware**: type (replace), catalog pickers (shackle size, roundsling size and hitch, chain grade and size), length, weight, width / diameter, take-up range and setting; rated WLL; move, add a shackle below, remove.

Every value field takes a number with units or an expression (`8'-6"`,
`bar1_span/2`), commits on Enter or blur, and shows the refusal under the field.

### 4.4 Commands

Toolbar groups **Edit** (Undo, Redo, Delete selection), **Insert** (each body
preset, master link), **Hardware** (each hardware preset: inserted below the
selected chip, otherwise armed for the canvas) and **View** (Diagram, 3D,
Split). Every tool is also in the `Ctrl+K` palette. The Projects pane has
**New blank rig**.

## 5. Tests

| Test | Covers |
|---|---|
| `topo::tests::two_leg_bridle_rebuilt_by_edits_solves_the_same` | Rebuild the bridle fixture edit by edit; weights, hook load and every tension match to 1e-9 |
| `topo::tests::bar_template_rebuilt_by_edits_solves_the_same` | Same for the two-layer spreader template from `from_layers` |
| `topo::tests::spreader_rig_from_presets_solves` | Blank rig + spreader preset + default runs validates and converges |
| `topo::tests::draft_rigs_are_accepted_and_reported` | Drafts are accepted; `UnsupportedBody` reported |
| `topo::tests::remove_body_cascades_without_dangling_refs` | Cascade; the hook can't be removed |
| `topo::tests::remove_node_in_use_is_refused_with_members` | Refusal names the members |
| `topo::tests::structural_errors_are_refused_and_leave_input_unchanged` | Lug as interior stop, self loop, bad index |
| `topo::tests::reroute_through_a_bow_keeps_total_length_and_weight` | Basket re-split |
| `topo::tests::component_edits_insert_move_replace_and_resync_weight` | Chip edits; catalog weight follows length; quantity errors |
| `preset::tests::*` | Unique names, blank rig, bridle from presets, hardware presets, `replace_with` |
| `layout::tests::*` | Hook on top, load at the bottom, no overlap in a row, dragged positions, detached blocks |
| `session::tests::topology_edits_recompute_and_undo_exactly` | Undo/redo through a sequence of topology edits |
| `store::rig::tests::canvas_layout_round_trips_survives_rig_saves_and_dies_with_the_rig` | Space 13 |
| `edit::tests::where_used_matches_serde_reference_completeness_oracle` | Now also covers a rig built only from presets |
| `rig_canvas::tests::chips_are_centred_with_a_gap_on_each_side` | Chip and insert-point layout along a wire |
