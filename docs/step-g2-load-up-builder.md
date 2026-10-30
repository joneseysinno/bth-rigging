# Step G2: Load-up builder ("gather")

> **Status:** implemented · **Applies to:** `bth-rigging`
> **Prerequisite:** Step G (topology edits, presets, canvas, session).
> **Why:** the Step G canvas asks the user to *draw a graph* — place blocks,
> drag wires, insert chips. A rigger doesn't think that way. A rigger thinks
> "the load has these pick points; what holds them?" G2 adds a builder that
> works that way and leaves the canvas for fine-tuning.

---

## 1. The idea

Rigging is a tree: the hook is the root, the pick points are the leaves. G2
builds it **from the leaves up**:

1. Every pick point starts **open**.
2. Tick some open points and press what holds them: **Master link**, **Spreader
   bar**, **Lifting beam**, **Basket** or **To hook**.
3. The new item is placed over them, sized, and wired. Its top becomes the
   next open point (a master link, a bar's top lugs, a beam's top lug).
4. Repeat until the last open point goes **To hook**. Done.

The tree can only grow upward and every gather removes open points, so the user
can't make a cycle or orphan a piece. Each gather is **one undo step**, and
**Ungather** reverses it exactly.

**Recipes** are stored gather sequences on one load: 2-leg bridle, 4-leg
bridle (master link), spreader bar, bar over two bars (an "H"), lifting beam,
two baskets.

## 2. Engine (`rig::gather`, headless)

| API | What it does |
|---|---|
| `status(&rig) → BuildStatus` | `open` points, `spare` points (unused, on items already hung), `unhung` bodies, `done` |
| `points(&rig)` | Every open or spare point with its owner and design-pose position |
| `check(&rig, ends, kind)` | Why `ends` can't be gathered into `kind` (or `Ok`) — drives the button states |
| `gather(&rig, ends, kind, opts) → GatherPlan` | The edits (`Vec<RigEdit>`), the rig after them, the new open points, a one-line summary |
| `built(&rig)` / `ungather(&rig, Built)` / `clear(&rig)` | Take an item (or the hook connection, or everything) apart; params only it used are deleted |
| `recipes_for(&rig, load)` / `apply_recipe(…)` | One-click rigs |
| `sling_for(lbs)` / `shackle_for(lbs)` | Smallest catalog item that carries `lbs` |

`RigSession::apply_batch(&[RigEdit])` applies a plan as one undo step;
`session::apply_rig_edit` is the shared single-edit function.

### 2.1 What is "open"

- A lug, bow or edge on a **load** or **frame** that no member uses.
- A **top** port (label not `Bottom…`, not a bearing) on a spreader bar or
  lifting beam that no member uses.
- A **master link** (free node) that nothing holds from above. "Above" is by
  *rank*: hops up from the nearest load (loads are 0); the hook counts as above
  everything.
- A point on something already hanging from the hook is **spare**: listed
  separately, never required.

### 2.2 Sizing — the design pose

Gathers size what they create from a quick **design pose**: loads level at the
origin; every gathered item placed over what it holds at the height its legs
reach (`Design` in `gather.rs`, recomputed from the rig each time — nothing is
stored). The design pose is only for sizing; geometry and forces always come
from the solver.

| Item | Placement and size |
|---|---|
| Bridle (master link / hook) | Apex over the **CG** of what the points hold when they hold whole items, else over the points' centroid. The flattest leg is at the target angle (default **60°** from horizontal); every leg reaches the apex. |
| Spreader bar / lifting beam | Points split in two along their **principal axis** (PCA of plan positions, +x on ties). Span = plan distance between the cluster centroids, rounded to the inch, so single drop legs hang **plumb** (default drop **6 ft**). A cluster of 2+ points gets a bridle from its lug at the target angle. All legs end at one height, so the bar hangs level. |
| Basket | Path from a new master link, through the bearings in order (up the low side top→bottom, across, up the high side bottom→top), back to the link; apex over the bearings' centroid. |
| One point to the hook | A shackle-only link (zero length). |

Leg lengths round **up** to the inch. With **auto-size** on, each leg's
roundsling and shackles are the smallest catalog items whose WLL covers the
estimated tension: weight held ÷ min(legs, 2) ÷ sin(angle) — two legs carry for
3- and 4-leg bridles (B30.9 practice); bar-end shares follow the CG between the
ends. Bar WLL stays the preset's assumed value; enter the real bar.

Because the apex sits over the CG, **an off-centre CG still hangs level**: the
legs come out at different lengths (test `an_offset_cg_still_hangs_level`).

## 3. Screen (`docs::rig::build`)

A **Build** panel is the first and default tab of the Rig document's right rail:

- Progress line ("3 open point(s) · 1 item not hung" / "Rig complete").
- **Recipes** for each load whose lugs are all open.
- **Open points**, grouped by owner, as tick boxes ("all" ticks an owner's
  points). Hovering a point highlights it on the diagram and 3D view.
- **Gather into** buttons, each disabled with the reason as its tooltip until
  the ticked points fit. After a gather the new open points are pre-ticked,
  so the next step is usually one click.
- **Sizing** options: target angle (30–89°), drop length, auto-size.
- **Unused points** (collapsed), **Built** list with **Ungather** per item, and
  **Clear rigging**.

### 3.1 On the diagram (`ui::components::rig_canvas`)

The same flow works without leaving the diagram:

- **Open ports glow** (dashed accent ring). **Shift+click** (or Ctrl/⌘+click) an
  open port to tick or untick it. A plain press still starts a connection, so
  the Step G gestures are unchanged.
- Ticked ports fill solid. While any are ticked, a **gather bar** floats at the
  bottom of the diagram: *Gather N into* Master link · Spreader bar · Lifting
  beam · Basket · To hook · ×. Disabled buttons give the reason on hover.
- **Esc** unticks everything (after it cancels a connection, an armed item or
  a menu, as before).

The ticked points and the sizing options live in the rig's editor state
(`RigState::picked`, `RigState::gather_opts`), so the panel and the diagram
always show the same picks; `RigState::apply_plan` commits a plan (one undo
step) and pre-ticks the points it created.

The 3D view, inspector and parameter table are unchanged and work on the
gathered rig like any other.

## 4. Tests (`rig::gather::tests`)

| Test | Covers |
|---|---|
| `a_blank_rig_starts_with_its_pick_points_open` | Open points / unhung on a blank rig |
| `two_points_to_the_hook_hang_level_at_sixty_degrees` | 10 ft legs, tension W/2/sin 60°, level, `done` |
| `an_offset_cg_still_hangs_level` | Apex over CG → unequal legs, tilt < 0.5° |
| `four_corners_gather_to_a_master_link_then_the_hook` | Master link becomes the open point |
| `a_spreader_spans_the_points_with_plumb_legs` | Span = point spacing, drop legs carry W/2 |
| `every_recipe_builds_a_rig_that_solves` | Every recipe on 2- and 4-point loads validates, converges, hook load = weight, level |
| `ungather_puts_the_rig_back_exactly` | Ungather and Clear restore the rig `==`, params included |
| `a_gather_is_one_undo_step` | `apply_batch` + one Undo |
| `gathers_refuse_what_does_not_fit` | Single-point bar, bearing in a bridle, already rigged |
| `auto_size_picks_hardware_that_carries_the_solved_tension` | Solved tension ≤ lowest WLL on every leg |
| `a_flatter_target_angle_gives_longer_legs` | 45° leg = 7′-1″ on a 10 ft load |
| `principal_axis_runs_along_the_long_side` | Bar clusters on a rectangle |

## 5. Not in G2 (next)

- **Target solving** beyond levelling (`solve::inverse` already levels with
  adjusters): e.g. "solve leg 3 for 60°", snap to stock lengths from an
  inventory.
- **Chokers** as a gather kind.
