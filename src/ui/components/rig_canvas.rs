//! Node-graph canvas for building a rig: bodies and master links are blocks,
//! their lugs and bows are ports, members are wires carrying hardware chips.
//!
//! Step: G (graph builder)
//! Theory: docs/step-g-graph-builder.md.
//! Inputs: the rig session, canvas layout, shared selection, the armed palette item,
//! the points ticked for the next gather (step G2).
//! Outputs: topology edits through `on_edit`, dragged block positions through `on_move`,
//! gather plans through `on_plan`.
//!
//! Open points (step G2) glow; Shift/Ctrl+click ticks one, and a gather bar over
//! the diagram gathers the ticked points like the Build panel does.
//! Must not depend on: store.

use std::collections::HashSet;
use std::rc::Rc;

use dioxus::html::geometry::ClientPoint;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use indexmap::IndexMap;
use uuid::Uuid;

use crate::ui::selection::{Armed, Selection};
use bth_rigging::checks::Status;
use bth_rigging::format::{format_ft_in, format_lbs};
use bth_rigging::rig::gather::{self, GatherKind, GatherOptions, GatherPlan};
use bth_rigging::rig::layout::{Block, CanvasLayout, Edge, PlacedBlock, Schematic};
use bth_rigging::rig::preset::{self, BodyPreset, HardwarePreset, PickPattern, PortKind};
use bth_rigging::rig::session::RigSession;
use bth_rigging::rig::solve::MemberForce;
use bth_rigging::rig::solve::rate::Rated;
use bth_rigging::rig::topo::{self, TopoEdit};
use bth_rigging::rig::{
    BodyId, BodyKind, Component, ComponentKind, ItemRef, MemberId, MemberShape, NodeId, NodeKind,
    ParamTable, Rig, RigTree,
};

/// Pointer travel (px) above which a press is a drag, not a click.
const SLOP: f64 = 4.0;
const CHIP_H: f64 = 16.0;
const CHIP_GAP: f64 = 14.0;
const PORT_R: f64 = 6.0;
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    x: f64,
    y: f64,
    zoom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    left: f64,
    top: f64,
    w: f64,
    h: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    Idle,
    Pan {
        last: [f64; 2],
        moved: f64,
    },
    Block {
        key: Uuid,
        block: Block,
        grab: [f64; 2],
        at: [f64; 2],
        moved: f64,
    },
    /// Building a member's path. `pressed` while the button is still down
    /// from the first port, so a drag-and-release also connects.
    Connect {
        path: Vec<NodeId>,
        pressed: bool,
        moved: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum MenuTarget {
    Background([f64; 2]),
    Body(BodyId),
    Port(NodeId),
    Member(MemberId),
    Component {
        member: MemberId,
        segment: usize,
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct Menu {
    at: [f64; 2],
    target: MenuTarget,
}

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Heading(&'static str),
    Edit(TopoEdit),
    Insert {
        label: &'static str,
        member: MemberId,
        segment: usize,
        index: usize,
        component: Component,
    },
    Place(Armed, [f64; 2]),
    Connect(NodeId),
    Select(Selection),
    Fit,
}

/// Signals and callbacks every handler needs; `Copy` so closures can own it.
#[derive(Clone, Copy)]
struct Ctx {
    session: Signal<RigSession>,
    selected: Signal<Option<Selection>>,
    hovered: Signal<Option<Selection>>,
    armed: Signal<Option<Armed>>,
    gesture: Signal<Gesture>,
    menu: Signal<Option<Menu>>,
    view: Signal<Option<View>>,
    area: Signal<Area>,
    cursor: Signal<[f64; 2]>,
    wrapper: Signal<Option<Rc<MountedData>>>,
    fit: View,
    on_edit: Callback<TopoEdit, bool>,
    on_move: Callback<(Uuid, [f64; 2])>,
    picked: Signal<Vec<NodeId>>,
}

impl Ctx {
    /// Tick or untick an open point for the next gather.
    fn toggle_pick(self, node: NodeId) {
        let mut picked = self.picked;
        let mut now = picked.peek().clone();
        if let Some(i) = now.iter().position(|n| *n == node) {
            now.remove(i);
        } else {
            now.push(node);
        }
        picked.set(now);
    }

    fn view(self) -> View {
        self.view.peek().unwrap_or(self.fit)
    }

    fn to_canvas(self, c: ClientPoint) -> [f64; 2] {
        let v = self.view();
        let a = *self.area.peek();
        [v.x + (c.x - a.left) / v.zoom, v.y + (c.y - a.top) / v.zoom]
    }

    fn local(self, c: ClientPoint) -> [f64; 2] {
        let a = *self.area.peek();
        [c.x - a.left, c.y - a.top]
    }

    fn rig(self) -> Rig {
        self.session.peek().rig().clone()
    }

    fn gesture(self) -> Gesture {
        self.gesture.peek().clone()
    }

    fn set_gesture(self, g: Gesture) {
        let mut s = self.gesture;
        s.set(g);
    }

    fn select(self, sel: Option<Selection>) {
        let mut s = self.selected;
        s.set(sel);
    }

    fn hover(self, sel: Option<Selection>) {
        let mut s = self.hovered;
        if *s.peek() != sel {
            s.set(sel);
        }
    }

    fn arm(self, a: Option<Armed>) {
        let mut s = self.armed;
        s.set(a);
    }

    fn close_menu(self) {
        let mut m = self.menu;
        if m.peek().is_some() {
            m.set(None);
        }
    }

    fn open_menu(self, at: [f64; 2], target: MenuTarget) {
        let mut m = self.menu;
        m.set(Some(Menu { at, target }));
    }

    fn fit_view(self) {
        let mut v = self.view;
        v.set(None);
    }

    fn focus(self) {
        if let Some(el) = self.wrapper.peek().clone() {
            spawn(async move {
                let _ = el.set_focus(true).await;
            });
        }
    }

    fn edit(self, edit: TopoEdit) -> bool {
        self.on_edit.call(edit)
    }

    /// Put an armed body or master link at `at` and select it.
    fn place(self, armed: Armed, at: [f64; 2]) {
        let rig = self.rig();
        let (edit, select, key) = match armed {
            Armed::Body(p) => {
                let edit = preset::add_body(&rig, p);
                let TopoEdit::AddBody { body, .. } = &edit else {
                    return;
                };
                let id = body.id;
                (edit, Selection::Body(id), id.0)
            }
            Armed::MasterLink => {
                let edit = preset::master_link(&rig);
                let TopoEdit::AddNode { node } = &edit else {
                    return;
                };
                let id = node.id;
                (edit, Selection::Node(id), id.0)
            }
            Armed::Hardware(_) => return,
        };
        if self.edit(edit) {
            self.on_move.call((key, at));
            self.select(Some(select));
            self.arm(None);
        }
    }

    fn insert(self, member: MemberId, segment: usize, index: usize, component: Component) {
        let edit = TopoEdit::InsertComponent {
            member,
            segment,
            index,
            component,
        };
        if self.edit(edit) {
            self.select(Some(Selection::Component {
                member,
                segment,
                index,
            }));
            self.arm(None);
        }
    }

    /// Insert the armed hardware, if any. Returns whether something was armed.
    fn insert_armed(self, member: MemberId, segment: usize, index: usize) -> bool {
        let Some(Armed::Hardware(h)) = *self.armed.peek() else {
            return false;
        };
        self.insert(member, segment, index, h.component());
        true
    }

    fn connect_to(self, node: NodeId) {
        let Gesture::Connect { mut path, .. } = self.gesture() else {
            return;
        };
        let rig = self.rig();
        let Some(&last) = path.last() else {
            return;
        };
        if node == last {
            if path.len() == 1 {
                self.select(Some(Selection::Node(node)));
            }
            self.set_gesture(Gesture::Connect {
                path,
                pressed: false,
                moved: 0.0,
            });
            return;
        }
        let mut candidate = path.clone();
        candidate.push(node);
        let is_bearing = rig
            .nodes
            .get(&node)
            .is_some_and(|n| n.kind.is_bearing_capable());
        if topo::path_ok(&rig, &candidate) {
            let edit = preset::connect(&rig, candidate);
            let TopoEdit::Connect { member } = &edit else {
                return;
            };
            let id = member.id;
            if self.edit(edit) {
                self.select(Some(Selection::Member(id)));
            }
            self.set_gesture(Gesture::Idle);
        } else if is_bearing && !path.contains(&node) {
            path.push(node);
            self.set_gesture(Gesture::Connect {
                path,
                pressed: false,
                moved: 0.0,
            });
        } else {
            // Let the edit explain why the connection is refused.
            self.edit(preset::connect(&rig, candidate));
            self.set_gesture(Gesture::Connect {
                path,
                pressed: false,
                moved: 0.0,
            });
        }
    }

    fn run(self, action: Action) {
        self.close_menu();
        match action {
            Action::Heading(_) => {}
            Action::Edit(edit) => {
                self.edit(edit);
            }
            Action::Insert {
                member,
                segment,
                index,
                component,
                ..
            } => self.insert(member, segment, index, component),
            Action::Place(armed, at) => self.place(armed, at),
            Action::Connect(node) => {
                self.select(Some(Selection::Node(node)));
                self.set_gesture(Gesture::Connect {
                    path: vec![node],
                    pressed: false,
                    moved: 0.0,
                });
            }
            Action::Select(sel) => self.select(Some(sel)),
            Action::Fit => self.fit_view(),
        }
    }

    fn cancel(self) {
        if matches!(*self.gesture.peek(), Gesture::Connect { .. }) {
            self.set_gesture(Gesture::Idle);
        } else if self.armed.peek().is_some() {
            self.arm(None);
        } else if self.menu.peek().is_some() {
            self.close_menu();
        } else if !self.picked.peek().is_empty() {
            let mut picked = self.picked;
            picked.set(Vec::new());
        } else {
            self.select(None);
        }
    }
}

#[component]
pub fn RigCanvas(
    session: Signal<RigSession>,
    layout: Signal<CanvasLayout>,
    selected: Signal<Option<Selection>>,
    hovered: Signal<Option<Selection>>,
    armed: Signal<Option<Armed>>,
    on_edit: Callback<TopoEdit, bool>,
    on_move: Callback<(Uuid, [f64; 2])>,
    picked: Signal<Vec<NodeId>>,
    gather_opts: Signal<GatherOptions>,
    on_plan: Callback<Result<GatherPlan, String>, bool>,
) -> Element {
    let gesture = use_signal(|| Gesture::Idle);
    let menu = use_signal(|| None::<Menu>);
    let view = use_signal(|| None::<View>);
    let mut area = use_signal(|| Area {
        left: 0.0,
        top: 0.0,
        w: 900.0,
        h: 560.0,
    });
    let cursor = use_signal(|| [0.0, 0.0]);
    let mut svg_el = use_signal(|| None::<Rc<MountedData>>);
    let mut wrapper = use_signal(|| None::<Rc<MountedData>>);

    let refresh_area = move || {
        if let Some(el) = svg_el.peek().clone() {
            spawn(async move {
                if let Ok(r) = el.get_client_rect().await {
                    let next = Area {
                        left: r.origin.x,
                        top: r.origin.y,
                        w: r.size.width.max(1.0),
                        h: r.size.height.max(1.0),
                    };
                    if *area.peek() != next {
                        area.set(next);
                    }
                }
            });
        }
    };

    let (rig, forces, rated) = {
        let s = session.read();
        let snapshot = s.snapshot();
        let forces = snapshot
            .solved
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|solved| solved.members.clone())
            .or_else(|| snapshot.unconverged.as_ref().map(|u| u.members.clone()))
            .unwrap_or_default();
        (
            s.rig().clone(),
            forces,
            snapshot.rated.clone().unwrap_or_default(),
        )
    };
    let tree = RigTree::build(&rig);
    let current_gesture = gesture();
    let mut shown_layout = layout.read().clone();
    if let Gesture::Block { key, at, moved, .. } = &current_gesture
        && *moved > SLOP
    {
        shown_layout.set(*key, *at);
    }
    let schematic = Schematic::build(&rig, &shown_layout);
    let fit = fit_view(schematic.bounds(), area());
    let ctx = Ctx {
        session,
        selected,
        hovered,
        armed,
        gesture,
        menu,
        view,
        area,
        cursor,
        wrapper,
        fit,
        on_edit,
        on_move,
        picked,
    };
    let open: HashSet<NodeId> = gather::status(&rig).open.iter().map(|p| p.node).collect();
    let picks: Vec<NodeId> = picked().into_iter().filter(|n| open.contains(n)).collect();
    let v = view().unwrap_or(fit);
    let a = area();
    let view_box = format!("{} {} {} {}", v.x, v.y, a.w / v.zoom, a.h / v.zoom);

    let wires = wire_views(
        &rig,
        &schematic,
        &tree,
        &forces,
        &rated,
        selected(),
        hovered(),
    );
    let connect_path = match &current_gesture {
        Gesture::Connect { path, .. } => Some(path.clone()),
        _ => None,
    };
    let targets: Option<Vec<NodeId>> = connect_path.as_ref().map(|path| {
        rig.nodes
            .keys()
            .copied()
            .filter(|n| {
                let mut c = path.clone();
                c.push(*n);
                topo::path_ok(&rig, &c)
                    || (rig.nodes[n].kind.is_bearing_capable() && !path.contains(n))
            })
            .collect()
    });
    let preview = connect_path.as_ref().and_then(|path| {
        let mut pts: Vec<[f64; 2]> = path
            .iter()
            .filter_map(|n| schematic.port(*n).map(|p| p.at))
            .collect();
        pts.push(cursor());
        (pts.len() >= 2).then(|| points(&pts))
    });
    let armed_now = armed();
    let hardware_armed = matches!(armed_now, Some(Armed::Hardware(_)));
    let menu_items = menu().map(|m| (m.at, menu_actions(&rig, &m.target)));
    let hint = hint_text(&current_gesture, armed_now, rig.members.is_empty());
    let rigged = |id: BodyId| {
        rig.nodes
            .values()
            .any(|n| n.body == Some(id) && !topo::members_using(&rig, n.id).is_empty())
    };

    rsx! {
        div {
            class: "rig-canvas",
            tabindex: "0",
            onmounted: move |e| wrapper.set(Some(e.data())),
            onkeydown: move |e| match e.key() {
                Key::Escape => ctx.cancel(),
                Key::Delete | Key::Backspace => {
                    if let Some(sel) = *ctx.selected.peek() {
                        e.prevent_default();
                        if let Some(edit) = sel.delete_edit(&ctx.rig()) {
                            ctx.edit(edit);
                        }
                    }
                }
                _ => {}
            },
            div { class: "rc-toolbar",
                Palette { ctx_armed: armed }
                div { class: "rc-zoom",
                    button {
                        class: "btn btn-ghost sm",
                        title: "Zoom out",
                        onclick: move |_| zoom_by(ctx, 1.0 / 1.2),
                        "−"
                    }
                    span { class: "muted", "{(v.zoom * 100.0).round()}%" }
                    button {
                        class: "btn btn-ghost sm",
                        title: "Zoom in",
                        onclick: move |_| zoom_by(ctx, 1.2),
                        "+"
                    }
                    button {
                        class: "btn btn-ghost sm",
                        title: "Fit the whole rig",
                        onclick: move |_| ctx.fit_view(),
                        "Fit"
                    }
                }
            }
            div { class: "rc-stage",
                svg {
                    class: match &current_gesture {
                        Gesture::Pan { .. } | Gesture::Block { .. } => "rc-svg is-dragging",
                        Gesture::Connect { .. } => "rc-svg is-connecting",
                        _ if armed_now.is_some() => "rc-svg is-armed",
                        _ => "rc-svg",
                    },
                    view_box: "{view_box}",
                    xmlns: "http://www.w3.org/2000/svg",
                    onmounted: move |e| {
                        svg_el.set(Some(e.data()));
                        refresh_area();
                    },
                    onmouseenter: move |_| refresh_area(),
                    onmousedown: move |e| {
                        refresh_area();
                        ctx.focus();
                        ctx.close_menu();
                        if e.trigger_button() != Some(MouseButton::Primary) {
                            return;
                        }
                        if matches!(ctx.gesture(), Gesture::Connect { .. }) {
                            ctx.set_gesture(Gesture::Idle);
                            return;
                        }
                        let c = e.client_coordinates();
                        ctx.set_gesture(Gesture::Pan { last: [c.x, c.y], moved: 0.0 });
                    },
                    onmousemove: move |e| on_move_pointer(ctx, e.client_coordinates()),
                    onmouseup: move |e| on_release(ctx, e.client_coordinates()),
                    onmouseleave: move |_| {
                        ctx.hover(None);
                        match ctx.gesture() {
                            Gesture::Pan { .. } => ctx.set_gesture(Gesture::Idle),
                            Gesture::Block { key, at, moved, .. } => {
                                ctx.set_gesture(Gesture::Idle);
                                if moved > SLOP {
                                    ctx.on_move.call((key, at));
                                }
                            }
                            _ => {}
                        }
                    },
                    onwheel: move |e| {
                        let dy = e.delta().strip_units().y;
                        if dy.abs() > 0.0 {
                            let factor = if dy > 0.0 { 1.0 / 1.12 } else { 1.12 };
                            zoom_at(ctx, e.client_coordinates(), factor);
                        }
                    },
                    oncontextmenu: move |e| {
                        e.prevent_default();
                        let c = e.client_coordinates();
                        ctx.open_menu(ctx.local(c), MenuTarget::Background(ctx.to_canvas(c)));
                    },
                    defs {
                        pattern {
                            id: "rc-grid",
                            width: "40",
                            height: "40",
                            pattern_units: "userSpaceOnUse",
                            path { class: "rc-grid-line", d: "M 40 0 L 0 0 0 40" }
                        }
                    }
                    rect {
                        class: "rc-bg",
                        x: "{v.x - 4000.0}",
                        y: "{v.y - 4000.0}",
                        width: "{a.w / v.zoom + 8000.0}",
                        height: "{a.h / v.zoom + 8000.0}",
                        fill: "url(#rc-grid)",
                    }
                    for wire in wires.iter() {
                        {wire_el(ctx, wire)}
                    }
                    for block in schematic.blocks.iter() {
                        {block_el(ctx, &rig, block, &rated, selected(), hovered(), matches!(block.block, Block::Body(id) if !rigged(id)))}
                    }
                    for wire in wires.iter() {
                        {chips_el(ctx, wire, hardware_armed)}
                    }
                    for block in schematic.blocks.iter() {
                        for port in block.ports.iter() {
                            {port_el(ctx, &rig, block, port.node, port.edge, port.at, &rated, selected(), hovered(), targets.as_deref(), connect_path.as_deref(), PickState { open: open.contains(&port.node), picked: picks.contains(&port.node) })}
                        }
                    }
                    if let Some(pts) = preview {
                        polyline { class: "rc-preview", points: "{pts}" }
                    }
                    if let Some(item) = armed_now {
                        {
                            let [x, y] = cursor();
                            rsx! {
                                text { class: "rc-ghost", x: "{x + 12.0}", y: "{y - 10.0}", "+ {item.label()}" }
                            }
                        }
                    }
                }
                if !picks.is_empty() {
                    GatherBar { rig: rig.clone(), picks: picks.clone(), picked, gather_opts, on_plan }
                }
                if let Some((at, actions)) = menu_items {
                    div {
                        class: "rc-menu",
                        style: "left: {at[0]}px; top: {at[1]}px;",
                        onmousedown: move |e| e.stop_propagation(),
                        for (index , action) in actions.into_iter().enumerate() {
                            {
                                match action {
                                    Action::Heading(text) => rsx! {
                                        p { key: "{index}", class: "rc-menu-heading", "{text}" }
                                    },
                                    other => {
                                        let label = action_label(&rig, &other);
                                        let danger = matches!(
                                            other,
                                            Action::Edit(
                                                TopoEdit::RemoveBody { .. }
                                                | TopoEdit::RemoveNode { .. }
                                                | TopoEdit::RemoveMember { .. }
                                                | TopoEdit::RemoveComponent { .. }
                                            )
                                        );
                                        rsx! {
                                            button {
                                                key: "{index}",
                                                class: if danger { "rc-menu-item is-danger" } else { "rc-menu-item" },
                                                onclick: move |_| ctx.run(other.clone()),
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p { class: "rc-hint muted",
                if picks.is_empty() {
                    "{hint}"
                } else {
                    "{picks.len()} point(s) ticked for Gather · Shift+click open ports to add or remove · Esc clears"
                }
            }
        }
    }
}

#[component]
fn Palette(ctx_armed: Signal<Option<Armed>>) -> Element {
    let mut armed = ctx_armed;
    let bodies = [
        Armed::Body(BodyPreset::SpreaderBar),
        Armed::Body(BodyPreset::LiftingBeam),
        Armed::Body(BodyPreset::Load {
            pattern: PickPattern::Two,
        }),
        Armed::Body(BodyPreset::Load {
            pattern: PickPattern::FourCorners,
        }),
        Armed::Body(BodyPreset::Frame),
        Armed::MasterLink,
    ];
    let item = move |a: Armed| {
        let on = armed() == Some(a);
        rsx! {
            button {
                class: if on { "rc-chip-btn is-armed" } else { "rc-chip-btn" },
                title: palette_title(a),
                onmousedown: move |e| {
                    e.stop_propagation();
                    armed.set(if armed.peek().as_ref() == Some(&a) { None } else { Some(a) });
                },
                "{a.label()}"
            }
        }
    };
    rsx! {
        div { class: "rc-palette",
            span { class: "rc-palette-label", "Add" }
            for a in bodies {
                {item(a)}
            }
            span { class: "rc-palette-label", "Hardware" }
            for h in HardwarePreset::ALL {
                {item(Armed::Hardware(h))}
            }
        }
    }
}

fn palette_title(a: Armed) -> String {
    match a {
        Armed::Hardware(h) => format!(
            "{}: click (or drag) here, then click a + on a member to insert it",
            h.label()
        ),
        other => format!(
            "{}: click (or drag) here, then click the canvas to place it",
            other.label()
        ),
    }
}

fn hint_text(gesture: &Gesture, armed: Option<Armed>, empty: bool) -> String {
    if let Gesture::Connect { path, .. } = gesture {
        return if path.len() > 1 {
            "Click a lug, the hook or a master link to finish the member, or another bow to pass through it. Esc cancels.".into()
        } else {
            "Click another port to connect. Click a bow or edge first to make a basket. Esc cancels.".into()
        };
    }
    match armed {
        Some(Armed::Hardware(h)) => format!(
            "Click a + on a member to insert the {}. Esc cancels.",
            h.label().to_lowercase()
        ),
        Some(other) => format!(
            "Click the canvas to place the {}. Esc cancels.",
            other.label().to_lowercase()
        ),
        None if empty => "Drag from the hook's port to a pick point to add a leg, or add a spreader bar from the palette. Right-click anything for more.".into(),
        None => "Drag from port to port to add a member · click a wire or chip to edit it · right-click for more · scroll to zoom, drag the background to pan · Delete removes the selection.".into(),
    }
}

fn on_move_pointer(ctx: Ctx, c: ClientPoint) {
    let p = ctx.to_canvas(c);
    let mut cursor = ctx.cursor;
    cursor.set(p);
    match ctx.gesture() {
        Gesture::Pan { last, moved } => {
            let v = ctx.view();
            let (dx, dy) = ((c.x - last[0]) / v.zoom, (c.y - last[1]) / v.zoom);
            let mut view = ctx.view;
            view.set(Some(View {
                x: v.x - dx,
                y: v.y - dy,
                zoom: v.zoom,
            }));
            ctx.set_gesture(Gesture::Pan {
                last: [c.x, c.y],
                moved: moved + (dx.abs() + dy.abs()) * v.zoom,
            });
        }
        Gesture::Block {
            key,
            block,
            grab,
            at,
            moved,
        } => {
            let next = [p[0] - grab[0], p[1] - grab[1]];
            let step = ((next[0] - at[0]).abs() + (next[1] - at[1]).abs()) * ctx.view().zoom;
            ctx.set_gesture(Gesture::Block {
                key,
                block,
                grab,
                at: next,
                moved: moved + step,
            });
        }
        Gesture::Connect {
            path,
            pressed: true,
            moved,
        } => {
            ctx.set_gesture(Gesture::Connect {
                path,
                pressed: true,
                moved: moved + 1.0,
            });
        }
        _ => {}
    }
}

/// Button released over the background (or anything that let the event through).
fn on_release(ctx: Ctx, c: ClientPoint) {
    let p = ctx.to_canvas(c);
    match ctx.gesture() {
        Gesture::Pan { moved, .. } => {
            ctx.set_gesture(Gesture::Idle);
            if moved < SLOP {
                match *ctx.armed.peek() {
                    Some(Armed::Hardware(_)) => {}
                    Some(armed) => ctx.place(armed, p),
                    None => ctx.select(None),
                }
            }
        }
        Gesture::Block {
            key,
            block,
            at,
            moved,
            ..
        } => {
            ctx.set_gesture(Gesture::Idle);
            if moved > SLOP {
                ctx.on_move.call((key, at));
            } else {
                ctx.select(Some(match block {
                    Block::Body(id) => Selection::Body(id),
                    Block::Knot(id) => Selection::Node(id),
                }));
            }
        }
        Gesture::Connect {
            path,
            pressed: true,
            moved,
        } => {
            if moved > SLOP {
                ctx.set_gesture(Gesture::Idle);
            } else {
                ctx.set_gesture(Gesture::Connect {
                    path,
                    pressed: false,
                    moved: 0.0,
                });
            }
        }
        _ => {
            if let Some(armed @ (Armed::Body(_) | Armed::MasterLink)) = *ctx.armed.peek() {
                ctx.place(armed, p);
            }
        }
    }
}

/// An armed body or master link is placed by the background's release
/// handler, so wires and chips let the event through.
fn places_on_release(ctx: Ctx) -> bool {
    matches!(*ctx.armed.peek(), Some(Armed::Body(_) | Armed::MasterLink))
}

fn zoom_by(ctx: Ctx, factor: f64) {
    let a = *ctx.area.peek();
    let c = ClientPoint::new(a.left + a.w / 2.0, a.top + a.h / 2.0);
    zoom_at(ctx, c, factor);
}

fn zoom_at(ctx: Ctx, c: ClientPoint, factor: f64) {
    let v = ctx.view();
    let a = *ctx.area.peek();
    let p = ctx.to_canvas(c);
    let zoom = (v.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let mut view = ctx.view;
    view.set(Some(View {
        x: p[0] - (c.x - a.left) / zoom,
        y: p[1] - (c.y - a.top) / zoom,
        zoom,
    }));
}

fn fit_view(bounds: [f64; 4], a: Area) -> View {
    let pad = 70.0;
    let (x0, y0, x1, y1) = (
        bounds[0] - pad,
        bounds[1] - pad,
        bounds[2] + pad,
        bounds[3] + pad,
    );
    let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let zoom = (a.w / bw).min(a.h / bh).clamp(MIN_ZOOM, 1.2);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    View {
        x: cx - a.w / zoom / 2.0,
        y: cy - a.h / zoom / 2.0,
        zoom,
    }
}

// ---------- wires and chips ----------

#[derive(Debug, Clone, PartialEq)]
struct ChipView {
    segment: usize,
    index: usize,
    center: [f64; 2],
    w: f64,
    text: String,
    title: String,
    class: String,
}

#[derive(Debug, Clone, PartialEq)]
struct GapView {
    segment: usize,
    index: usize,
    at: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
struct WireView {
    id: MemberId,
    points: String,
    class: String,
    tag: String,
    tag_at: [f64; 2],
    tension: Option<String>,
    hot: bool,
    chips: Vec<ChipView>,
    gaps: Vec<GapView>,
    title: String,
}

fn wire_views(
    rig: &Rig,
    schematic: &Schematic,
    tree: &RigTree,
    forces: &IndexMap<MemberId, MemberForce>,
    rated: &[Rated],
    selected: Option<Selection>,
    hovered: Option<Selection>,
) -> Vec<WireView> {
    let mut out = Vec::new();
    for member in rig.members.values() {
        let pts: Vec<[f64; 2]> = member
            .path
            .iter()
            .filter_map(|n| schematic.port(*n).map(|p| p.at))
            .collect();
        if pts.len() != member.path.len() || pts.len() < 2 {
            continue;
        }
        let force = forces.get(&member.id);
        let status = worst(
            rated
                .iter()
                .filter(|r| matches!(r.at, ItemRef::Member { id, .. } if id == member.id)),
        );
        let tree_member = tree.get(member.id);
        let is_link = tree_member.is_some_and(|t| t.shape == MemberShape::Link);
        let hot = selected.and_then(Selection::member) == Some(member.id)
            || hovered.and_then(Selection::member) == Some(member.id);
        let mut class = String::from("rc-wire");
        if is_link {
            class.push_str(" rc-link");
        } else if force.is_some_and(|f| !f.taut) {
            class.push_str(" rc-slack");
        }
        match status {
            Status::Over => class.push_str(" rc-over"),
            Status::Warn => class.push_str(" rc-warn"),
            Status::Ok => {}
        }
        if hot {
            class.push_str(" is-hot");
        }

        let mut chips = Vec::new();
        let mut gaps = Vec::new();
        for (si, seg) in member.segments.iter().enumerate() {
            let (a, b) = (pts[si], pts[si + 1]);
            let texts: Vec<(String, String)> = seg
                .components
                .iter()
                .map(|c| chip_text(c, &rig.params))
                .collect();
            let widths: Vec<f64> = texts.iter().map(|(t, _)| chip_width(t)).collect();
            let (centers, gap_pts) = lay_chips(a, b, &widths);
            for (ci, ((text, title), center)) in texts.into_iter().zip(centers).enumerate() {
                let sel = selected
                    == Some(Selection::Component {
                        member: member.id,
                        segment: si,
                        index: ci,
                    });
                let kind = chip_kind(&seg.components[ci].kind);
                chips.push(ChipView {
                    segment: si,
                    index: ci,
                    center,
                    w: widths[ci],
                    text,
                    title,
                    class: if sel {
                        format!("rc-chip rc-chip-{kind} is-selected")
                    } else {
                        format!("rc-chip rc-chip-{kind}")
                    },
                });
            }
            for (gi, at) in gap_pts.into_iter().enumerate() {
                gaps.push(GapView {
                    segment: si,
                    index: gi,
                    at,
                });
            }
        }

        let (a, b) = (pts[0], pts[1]);
        let len = dist(a, b).max(1e-6);
        let u = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
        let step = (len * 0.18).min(26.0);
        let tag = tree_member.map(|t| t.tag.clone()).unwrap_or_default();
        out.push(WireView {
            id: member.id,
            points: points(&pts),
            class,
            tag_at: [a[0] + u[0] * step, a[1] + u[1] * step],
            tension: force
                .filter(|f| !f.is_link)
                .map(|f| format!("{} lb", format_lbs(f.tension_lbs))),
            title: tree_member
                .map(|t| format!("{} · {}\n{}", t.tag, member.label, t.hardware))
                .unwrap_or_else(|| member.label.clone()),
            tag,
            hot,
            chips,
            gaps,
        });
    }
    out
}

/// Chip centres along `a → b`, centred on the midpoint, and the insert
/// positions before, between and after them.
fn lay_chips(a: [f64; 2], b: [f64; 2], widths: &[f64]) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
    let len = dist(a, b);
    let u = if len < 1e-6 {
        [0.0, 1.0]
    } else {
        [(b[0] - a[0]) / len, (b[1] - a[1]) / len]
    };
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    if widths.is_empty() {
        return (Vec::new(), vec![mid]);
    }
    let ext: Vec<f64> = widths
        .iter()
        .map(|w| u[0].abs() * w + u[1].abs() * CHIP_H)
        .collect();
    let pitches: Vec<f64> = ext
        .windows(2)
        .map(|w| (w[0] + w[1]) / 2.0 + CHIP_GAP)
        .collect();
    let total: f64 = pitches.iter().sum();
    let along = |p: [f64; 2], d: f64| [p[0] + u[0] * d, p[1] + u[1] * d];
    let mut centers = vec![along(mid, -total / 2.0)];
    for p in &pitches {
        let last = *centers.last().expect("one centre");
        centers.push(along(last, *p));
    }
    let mut gaps = vec![along(centers[0], -(ext[0] / 2.0 + CHIP_GAP / 2.0 + 4.0))];
    for w in centers.windows(2) {
        gaps.push([(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0]);
    }
    let n = centers.len() - 1;
    gaps.push(along(centers[n], ext[n] / 2.0 + CHIP_GAP / 2.0 + 4.0));
    (centers, gaps)
}

fn chip_width(text: &str) -> f64 {
    text.chars().count() as f64 * 6.1 + 14.0
}

fn chip_kind(kind: &ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Shackle { .. } => "shackle",
        ComponentKind::RoundSling { .. } => "sling",
        ComponentKind::Strap { .. } => "strap",
        ComponentKind::Chain { .. } => "chain",
        ComponentKind::WireRope { .. } => "rope",
        ComponentKind::MasterLink => "link",
        ComponentKind::Turnbuckle => "turnbuckle",
    }
}

/// Short text on the chip, and a longer tooltip.
pub fn chip_text(c: &Component, params: &ParamTable) -> (String, String) {
    let length = c.eval_length(params).ok().filter(|l| *l > 1e-6);
    let len = length.map(|l| format_ft_in(l, 16)).unwrap_or_default();
    let adj = if c.adjust.is_some() { " adj" } else { "" };
    let text = match &c.kind {
        ComponentKind::Shackle { size } => format!("{size}″ shackle"),
        ComponentKind::RoundSling { size, hitch } => {
            let hitch = match hitch {
                bth_rigging::domain::Hitch::Vertical => "",
                bth_rigging::domain::Hitch::Choker => " choker",
                bth_rigging::domain::Hitch::Basket => " basket",
            };
            format!("RS-{size} {len}{hitch}")
        }
        ComponentKind::Strap { .. } => format!("Strap {len}"),
        ComponentKind::Chain { grade, .. } => format!("G{grade} chain {len}{adj}"),
        ComponentKind::WireRope { .. } => format!("Wire rope {len}"),
        ComponentKind::MasterLink => "Master link".into(),
        ComponentKind::Turnbuckle => format!("Turnbuckle{adj}"),
    };
    let weight = c.eval_weight(params).unwrap_or(0.0);
    let mut title = format!("{} · {} lb", c.kind.label(), format_lbs(weight));
    if let Some(l) = length {
        title.push_str(&format!(" · {}", format_ft_in(l, 16)));
    }
    if let Some(wll) = c.catalog_wll_lbs() {
        title.push_str(&format!(" · WLL {} lb", format_lbs(f64::from(wll))));
    }
    (text.trim().to_owned(), title)
}

fn wire_el(ctx: Ctx, wire: &WireView) -> Element {
    let id = wire.id;
    let gaps = wire.gaps.clone();
    rsx! {
        g { key: "wire-{id}",
            polyline { class: "{wire.class}", points: "{wire.points}" }
            polyline {
                class: "rc-wire-hit",
                points: "{wire.points}",
                onmouseenter: move |_| ctx.hover(Some(Selection::Member(id))),
                onmouseleave: move |_| ctx.hover(None),
                onmousedown: move |e| {
                    if e.trigger_button() == Some(MouseButton::Primary)
                        && !matches!(ctx.gesture(), Gesture::Connect { .. })
                    {
                        e.stop_propagation();
                        ctx.focus();
                        ctx.close_menu();
                    }
                },
                onmouseup: move |e| {
                    if ctx.gesture() != Gesture::Idle || places_on_release(ctx) {
                        return;
                    }
                    e.stop_propagation();
                    let p = ctx.to_canvas(e.client_coordinates());
                    let nearest = gaps
                        .iter()
                        .min_by(|a, b| dist(a.at, p).total_cmp(&dist(b.at, p)));
                    if let Some(gap) = nearest
                        && ctx.insert_armed(id, gap.segment, gap.index)
                    {
                        return;
                    }
                    ctx.select(Some(Selection::Member(id)));
                },
                oncontextmenu: move |e| {
                    e.prevent_default();
                    e.stop_propagation();
                    ctx.select(Some(Selection::Member(id)));
                    ctx.open_menu(ctx.local(e.client_coordinates()), MenuTarget::Member(id));
                },
                title { "{wire.title}" }
            }
        }
    }
}

fn chips_el(ctx: Ctx, wire: &WireView, show_gaps: bool) -> Element {
    let id = wire.id;
    let [tx, ty] = wire.tag_at;
    let tag_w = wire.tag.chars().count() as f64 * 7.0 + 10.0;
    rsx! {
        g { key: "chips-{id}",
            if !wire.tag.is_empty() {
                g {
                    class: if wire.hot { "rc-tag is-hot" } else { "rc-tag" },
                    onmousedown: move |e| e.stop_propagation(),
                    onmouseup: move |e| {
                        if ctx.gesture() == Gesture::Idle {
                            e.stop_propagation();
                            ctx.select(Some(Selection::Member(id)));
                        }
                    },
                    rect {
                        x: "{tx - tag_w / 2.0}",
                        y: "{ty - 8.0}",
                        width: "{tag_w}",
                        height: "16",
                        rx: "4",
                    }
                    text { x: "{tx}", y: "{ty + 4.0}", text_anchor: "middle", "{wire.tag}" }
                    if let Some(t) = &wire.tension {
                        text { class: "rc-tension", x: "{tx + tag_w / 2.0 + 4.0}", y: "{ty + 4.0}", "{t}" }
                    }
                }
            }
            for chip in wire.chips.iter() {
                {
                    let (segment, index) = (chip.segment, chip.index);
                    let sel = Selection::Component { member: id, segment, index };
                    let [cx, cy] = chip.center;
                    rsx! {
                        g {
                            key: "chip-{segment}-{index}",
                            class: "{chip.class}",
                            onmouseenter: move |_| ctx.hover(Some(sel)),
                            onmouseleave: move |_| ctx.hover(None),
                            onmousedown: move |e| {
                                if e.trigger_button() == Some(MouseButton::Primary) {
                                    e.stop_propagation();
                                    ctx.focus();
                                    ctx.close_menu();
                                }
                            },
                            onmouseup: move |e| {
                                if ctx.gesture() != Gesture::Idle || places_on_release(ctx) {
                                    return;
                                }
                                e.stop_propagation();
                                if !ctx.insert_armed(id, segment, index + 1) {
                                    ctx.select(Some(sel));
                                }
                            },
                            oncontextmenu: move |e| {
                                e.prevent_default();
                                e.stop_propagation();
                                ctx.select(Some(sel));
                                ctx.open_menu(
                                    ctx.local(e.client_coordinates()),
                                    MenuTarget::Component { member: id, segment, index },
                                );
                            },
                            rect {
                                x: "{cx - chip.w / 2.0}",
                                y: "{cy - CHIP_H / 2.0}",
                                width: "{chip.w}",
                                height: "{CHIP_H}",
                                rx: "4",
                            }
                            text { x: "{cx}", y: "{cy + 3.8}", text_anchor: "middle", "{chip.text}" }
                            title { "{chip.title}" }
                        }
                    }
                }
            }
            if show_gaps {
                for gap in wire.gaps.iter() {
                    {
                        let (segment, index) = (gap.segment, gap.index);
                        let [gx, gy] = gap.at;
                        rsx! {
                            g {
                                key: "gap-{segment}-{index}",
                                class: "rc-gap",
                                onmousedown: move |e| e.stop_propagation(),
                                onmouseup: move |e| {
                                    if ctx.gesture() == Gesture::Idle {
                                        e.stop_propagation();
                                        ctx.insert_armed(id, segment, index);
                                    }
                                },
                                circle { cx: "{gx}", cy: "{gy}", r: "7" }
                                path { d: "M {gx - 3.5} {gy} H {gx + 3.5} M {gx} {gy - 3.5} V {gy + 3.5}" }
                                title { "Insert here" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------- blocks and ports ----------

fn block_el(
    ctx: Ctx,
    rig: &Rig,
    block: &PlacedBlock,
    rated: &[Rated],
    selected: Option<Selection>,
    hovered: Option<Selection>,
    unrigged: bool,
) -> Element {
    let [cx, cy] = block.center;
    let (w, h) = (block.w, block.h);
    let key = block.block.key();
    let kind = block.block;
    let grab_start = move |e: MouseEvent| {
        if e.trigger_button() != Some(MouseButton::Primary) || ctx.armed.peek().is_some() {
            return;
        }
        e.stop_propagation();
        ctx.focus();
        ctx.close_menu();
        if matches!(ctx.gesture(), Gesture::Connect { .. }) {
            ctx.set_gesture(Gesture::Idle);
            return;
        }
        let p = ctx.to_canvas(e.client_coordinates());
        ctx.set_gesture(Gesture::Block {
            key,
            block: kind,
            grab: [p[0] - cx, p[1] - cy],
            at: [cx, cy],
            moved: 0.0,
        });
    };
    match block.block {
        Block::Knot(node) => {
            let label = rig
                .nodes
                .get(&node)
                .map(|n| n.label.clone())
                .unwrap_or_default();
            let lw = label.chars().count() as f64 * 6.5 + 14.0;
            let sel = selected == Some(Selection::Node(node));
            rsx! {
                g { key: "block-{key}", class: if sel { "rc-knot-label is-selected" } else { "rc-knot-label" },
                    onmousedown: grab_start,
                    oncontextmenu: move |e| {
                        e.prevent_default();
                        e.stop_propagation();
                        ctx.select(Some(Selection::Node(node)));
                        ctx.open_menu(ctx.local(e.client_coordinates()), MenuTarget::Port(node));
                    },
                    rect { x: "{cx + 18.0}", y: "{cy - 9.0}", width: "{lw}", height: "18", rx: "4" }
                    text { x: "{cx + 25.0}", y: "{cy + 4.0}", "{label}" }
                    if block.detached {
                        text { class: "rc-badge", x: "{cx + 18.0}", y: "{cy + 24.0}", "not hung from the hook" }
                    }
                    title { "Drag to move" }
                }
            }
        }
        Block::Body(id) => {
            let Some(body) = rig.bodies.get(&id) else {
                return rsx! {};
            };
            let status = worst(rated.iter().filter(|r| r.at == ItemRef::Body(id)));
            let mut class = format!("rc-block rc-block-{}", body_kind_class(&body.kind));
            if block.detached {
                class.push_str(" is-detached");
            }
            if selected == Some(Selection::Body(id)) {
                class.push_str(" is-selected");
            } else if hovered == Some(Selection::Body(id)) {
                class.push_str(" is-hover");
            }
            match status {
                Status::Over => class.push_str(" rc-over"),
                Status::Warn => class.push_str(" rc-warn"),
                Status::Ok => {}
            }
            let sub = body_subtitle(rig, id);
            let badge = if unrigged && !matches!(body.kind, BodyKind::Hook) {
                Some("not rigged")
            } else if block.detached {
                Some("not hung from the hook")
            } else {
                None
            };
            let label = body.label.clone();
            rsx! {
                g {
                    key: "block-{key}",
                    class: "{class}",
                    onmousedown: grab_start,
                    onmouseenter: move |_| ctx.hover(Some(Selection::Body(id))),
                    onmouseleave: move |_| ctx.hover(None),
                    oncontextmenu: move |e| {
                        e.prevent_default();
                        e.stop_propagation();
                        ctx.select(Some(Selection::Body(id)));
                        ctx.open_menu(ctx.local(e.client_coordinates()), MenuTarget::Body(id));
                    },
                    rect { x: "{cx - w / 2.0}", y: "{cy - h / 2.0}", width: "{w}", height: "{h}", rx: "7" }
                    text { class: "rc-block-label", x: "{cx}", y: "{cy - 2.0}", text_anchor: "middle", "{label}" }
                    if !sub.is_empty() {
                        text { class: "rc-block-sub", x: "{cx}", y: "{cy + 13.0}", text_anchor: "middle", "{sub}" }
                    }
                    if let Some(text) = badge {
                        text { class: "rc-badge", x: "{cx + w / 2.0 + 8.0}", y: "{cy + 4.0}", "{text}" }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn port_el(
    ctx: Ctx,
    rig: &Rig,
    block: &PlacedBlock,
    node: NodeId,
    edge: Edge,
    at: [f64; 2],
    rated: &[Rated],
    selected: Option<Selection>,
    hovered: Option<Selection>,
    targets: Option<&[NodeId]>,
    path: Option<&[NodeId]>,
    pick: PickState,
) -> Element {
    let Some(n) = rig.nodes.get(&node) else {
        return rsx! {};
    };
    let [x, y] = at;
    let knot = matches!(block.block, Block::Knot(_));
    let mut class = String::from(match &n.kind {
        NodeKind::Root => "rc-port rc-port-root",
        NodeKind::Bow { .. } => "rc-port rc-port-bow",
        NodeKind::Edge { .. } => "rc-port rc-port-edge",
        NodeKind::Free => "rc-port rc-port-knot",
        NodeKind::Lug { .. } => "rc-port rc-port-lug",
    });
    let status = worst(rated.iter().filter(|r| r.at == ItemRef::Node(node)));
    if status == Status::Over {
        class.push_str(" rc-over");
    }
    if path.is_some_and(|p| p.contains(&node)) {
        class.push_str(" is-path");
    } else if let Some(t) = targets {
        class.push_str(if t.contains(&node) {
            " is-target"
        } else {
            " is-dim"
        });
    }
    if pick.picked {
        class.push_str(" is-picked");
    } else if pick.open {
        class.push_str(" is-open");
    }
    if selected == Some(Selection::Node(node)) {
        class.push_str(" is-selected");
    } else if hovered == Some(Selection::Node(node)) {
        class.push_str(" is-hover");
    }
    let r = if knot { 12.0 } else { PORT_R };
    let label = (!knot && !n.kind.is_root()).then(|| n.label.clone());
    let label_y = match edge {
        Edge::Top => y - 10.0,
        _ => y + 18.0,
    };
    let title = format!(
        "{} ({}){} · drag to another port to connect · right-click for more",
        n.label,
        n.kind.label(),
        if pick.open {
            " · open: Shift+click to tick for Gather"
        } else {
            ""
        }
    );
    let open = pick.open;
    rsx! {
        g { key: "port-{node}",
            class: "{class}",
            onmouseenter: move |_| ctx.hover(Some(Selection::Node(node))),
            onmouseleave: move |_| ctx.hover(None),
            onmousedown: move |e| {
                if e.trigger_button() != Some(MouseButton::Primary) || ctx.armed.peek().is_some() {
                    return;
                }
                e.stop_propagation();
                if open
                    && e.modifiers().intersects(Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::META)
                    && !matches!(ctx.gesture(), Gesture::Connect { .. })
                {
                    ctx.toggle_pick(node);
                    return;
                }
                ctx.focus();
                ctx.close_menu();
                if !matches!(ctx.gesture(), Gesture::Connect { .. }) {
                    ctx.set_gesture(Gesture::Connect { path: vec![node], pressed: true, moved: 0.0 });
                }
            },
            onmouseup: move |e| {
                if matches!(ctx.gesture(), Gesture::Connect { .. }) {
                    e.stop_propagation();
                    ctx.connect_to(node);
                }
            },
            oncontextmenu: move |e| {
                e.prevent_default();
                e.stop_propagation();
                ctx.select(Some(Selection::Node(node)));
                ctx.open_menu(ctx.local(e.client_coordinates()), MenuTarget::Port(node));
            },
            circle { class: "rc-port-hit", cx: "{x}", cy: "{y}", r: "{r + 6.0}" }
            if matches!(n.kind, NodeKind::Edge { .. }) {
                rect { class: "rc-port-dot", x: "{x - r}", y: "{y - r}", width: "{2.0 * r}", height: "{2.0 * r}", transform: "rotate(45 {x} {y})" }
            } else {
                circle { class: "rc-port-dot", cx: "{x}", cy: "{y}", r: "{r}" }
            }
            if let Some(text) = label {
                text { class: "rc-port-label", x: "{x}", y: "{label_y}", text_anchor: "middle", "{text}" }
            }
            title { "{title}" }
        }
    }
}

/// Whether a port is open for gathering, and ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PickState {
    open: bool,
    picked: bool,
}

/// Floating bar over the diagram while points are ticked: gather them here
/// without going to the Build panel.
#[component]
fn GatherBar(
    rig: Rig,
    picks: Vec<NodeId>,
    picked: Signal<Vec<NodeId>>,
    gather_opts: Signal<GatherOptions>,
    on_plan: Callback<Result<GatherPlan, String>, bool>,
) -> Element {
    let n = picks.len();
    rsx! {
        div {
            class: "rc-gather-bar",
            onmousedown: move |e| e.stop_propagation(),
            span { class: "rc-gather-count", "Gather {n} into" }
            for kind in GatherKind::ALL {
                {
                    let verdict = gather::check(&rig, &picks, kind);
                    let title = match &verdict {
                        Ok(()) => kind.help().to_owned(),
                        Err(why) => why.clone(),
                    };
                    let ends = picks.clone();
                    let rig = rig.clone();
                    rsx! {
                        button {
                            key: "{kind.label()}",
                            class: if kind == GatherKind::Hook { "btn btn-primary sm" } else { "btn btn-ghost sm" },
                            disabled: verdict.is_err(),
                            title: "{title}",
                            onclick: move |_| {
                                on_plan.call(gather::gather(&rig, &ends, kind, gather_opts()));
                            },
                            "{kind.label()}"
                        }
                    }
                }
            }
            button {
                class: "btn btn-ghost sm",
                title: "Untick all (Esc)",
                onclick: move |_| picked.set(Vec::new()),
                "×"
            }
        }
    }
}

fn body_kind_class(kind: &BodyKind) -> &'static str {
    match kind {
        BodyKind::Hook => "hook",
        BodyKind::SpreaderBar { .. } => "bar",
        BodyKind::LiftingBeam { .. } => "beam",
        BodyKind::Load { .. } => "load",
        BodyKind::Frame => "frame",
    }
}

fn body_subtitle(rig: &Rig, id: BodyId) -> String {
    let Some(body) = rig.bodies.get(&id) else {
        return String::new();
    };
    let weight = body.eval_weight(&rig.params).unwrap_or(0.0);
    match &body.kind {
        BodyKind::Hook => String::new(),
        BodyKind::SpreaderBar { span, rating } | BodyKind::LiftingBeam { span, rating } => {
            let span = span
                .eval(&rig.params)
                .map(|s| format_ft_in(s, 16))
                .unwrap_or_default();
            format!(
                "span {span} · {} lb · WLL {} lb",
                format_lbs(weight),
                format_lbs(f64::from(rating.wll_lbs))
            )
        }
        _ => format!("{} lb", format_lbs(weight)),
    }
}

// ---------- context menu ----------

fn menu_actions(rig: &Rig, target: &MenuTarget) -> Vec<Action> {
    let mut out = Vec::new();
    match target {
        MenuTarget::Background(at) => {
            out.push(Action::Heading("Add here"));
            for p in BodyPreset::ALL {
                out.push(Action::Place(Armed::Body(p), *at));
            }
            out.push(Action::Place(Armed::MasterLink, *at));
            out.push(Action::Heading("View"));
            out.push(Action::Fit);
        }
        MenuTarget::Body(id) => {
            let Some(body) = rig.bodies.get(id) else {
                return out;
            };
            out.push(Action::Select(Selection::Body(*id)));
            if !matches!(body.kind, BodyKind::Hook) {
                out.push(Action::Heading("Ports"));
                for kind in PortKind::ALL {
                    out.push(Action::Edit(preset::port(rig, *id, kind)));
                }
                out.push(Action::Heading("Body"));
                out.push(Action::Edit(TopoEdit::RemoveBody { id: *id }));
            }
        }
        MenuTarget::Port(node) => {
            let Some(n) = rig.nodes.get(node) else {
                return out;
            };
            if !n.kind.is_bearing_capable() {
                out.push(Action::Connect(*node));
            }
            if n.body.is_some() && !n.kind.is_root() {
                out.push(Action::Heading("Port kind"));
                for kind in PortKind::ALL {
                    if kind.node_kind().label() != n.kind.label() {
                        out.push(Action::Edit(TopoEdit::SetNodeKind {
                            id: *node,
                            kind: kind.node_kind(),
                        }));
                    }
                }
            }
            if !n.kind.is_root() {
                out.push(Action::Heading("Port"));
                out.push(Action::Edit(TopoEdit::RemoveNode { id: *node }));
            }
        }
        MenuTarget::Member(id) => {
            let Some(member) = rig.members.get(id) else {
                return out;
            };
            let last = member.segments.len().saturating_sub(1);
            let shackle = || Component::shackle(preset::DEFAULT_SHACKLE);
            out.push(Action::Insert {
                label: "Insert shackle at the top",
                member: *id,
                segment: 0,
                index: 0,
                component: shackle(),
            });
            out.push(Action::Insert {
                label: "Insert shackle at the bottom",
                member: *id,
                segment: last,
                index: member.segments.get(last).map_or(0, |s| s.components.len()),
                component: shackle(),
            });
            let bows: Vec<NodeId> = rig
                .nodes
                .values()
                .filter(|n| n.kind.is_bearing_capable() && !member.path.contains(&n.id))
                .map(|n| n.id)
                .collect();
            if !bows.is_empty() {
                out.push(Action::Heading("Route through"));
                for bow in bows {
                    let mut path = member.path.clone();
                    path.insert(path.len() - 1, bow);
                    if topo::path_ok(rig, &path) {
                        out.push(Action::Edit(TopoEdit::Reroute { id: *id, path }));
                    }
                }
            }
            if member.path.len() > 2 {
                let first = member.path[0];
                let end = member.path[member.path.len() - 1];
                if topo::path_ok(rig, &[first, end]) {
                    out.push(Action::Heading("Route"));
                    out.push(Action::Edit(TopoEdit::Reroute {
                        id: *id,
                        path: vec![first, end],
                    }));
                }
            }
            out.push(Action::Heading("Member"));
            out.push(Action::Edit(TopoEdit::RemoveMember { id: *id }));
        }
        MenuTarget::Component {
            member,
            segment,
            index,
        } => {
            let Some(c) = rig
                .members
                .get(member)
                .and_then(|m| m.segments.get(*segment))
                .and_then(|s| s.components.get(*index))
            else {
                return out;
            };
            let count = rig.members[member].segments[*segment].components.len();
            let shackle = || Component::shackle(preset::DEFAULT_SHACKLE);
            out.push(Action::Insert {
                label: "Insert shackle above",
                member: *member,
                segment: *segment,
                index: *index,
                component: shackle(),
            });
            out.push(Action::Insert {
                label: "Insert shackle below",
                member: *member,
                segment: *segment,
                index: index + 1,
                component: shackle(),
            });
            if *index > 0 {
                out.push(Action::Edit(TopoEdit::MoveComponent {
                    member: *member,
                    segment: *segment,
                    from: *index,
                    to: index - 1,
                }));
            }
            if index + 1 < count {
                out.push(Action::Edit(TopoEdit::MoveComponent {
                    member: *member,
                    segment: *segment,
                    from: *index,
                    to: index + 1,
                }));
            }
            out.push(Action::Heading("Replace with"));
            for h in HardwarePreset::ALL {
                out.push(Action::Edit(TopoEdit::ReplaceComponent {
                    member: *member,
                    segment: *segment,
                    index: *index,
                    component: preset::replace_with(c, h, &rig.params),
                }));
            }
            out.push(Action::Heading("Hardware"));
            out.push(Action::Edit(TopoEdit::RemoveComponent {
                member: *member,
                segment: *segment,
                index: *index,
            }));
        }
    }
    out
}

fn action_label(rig: &Rig, action: &Action) -> String {
    let node_label = |id: &NodeId| preset::stop_label(rig, *id);
    match action {
        Action::Heading(t) => (*t).to_owned(),
        Action::Place(armed, _) => armed.label().to_owned(),
        Action::Connect(_) => "Connect from here".into(),
        Action::Select(_) => "Select".into(),
        Action::Fit => "Fit the whole rig".into(),
        Action::Insert { label, .. } => (*label).to_owned(),
        Action::Edit(edit) => match edit {
            TopoEdit::AddNode { node } => format!("Add {}", node.kind.label()),
            TopoEdit::RemoveBody { id } => format!(
                "Remove {}",
                rig.bodies
                    .get(id)
                    .map(|b| b.label.as_str())
                    .unwrap_or("body")
            ),
            TopoEdit::SetNodeKind { kind, .. } => format!("Make it a {}", kind.label()),
            TopoEdit::RemoveNode { id } => format!(
                "Remove {}",
                rig.nodes
                    .get(id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("port")
            ),
            TopoEdit::Reroute { path, .. } if path.len() == 2 => "Straighten (remove bows)".into(),
            TopoEdit::Reroute { path, .. } => {
                let bow = path[path.len() - 2];
                format!("Through {}", node_label(&bow))
            }
            TopoEdit::RemoveMember { .. } => "Remove member".into(),
            TopoEdit::MoveComponent { from, to, .. } if to < from => "Move up".into(),
            TopoEdit::MoveComponent { .. } => "Move down".into(),
            TopoEdit::ReplaceComponent { component, .. } => component.kind.label(),
            TopoEdit::RemoveComponent { .. } => "Remove".into(),
            _ => "Apply".into(),
        },
    }
}

// ---------- helpers ----------

fn worst<'a>(items: impl Iterator<Item = &'a Rated>) -> Status {
    items.map(|r| r.check.status).max().unwrap_or(Status::Ok)
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn points(pts: &[[f64; 2]]) -> String {
    pts.iter()
        .map(|p| format!("{:.1},{:.1}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bth_rigging::rig::{duplo10, preset::blank_rig};

    fn mount(rig: fn() -> Rig, select: fn(&Rig) -> Option<Selection>, arm: Option<Armed>) {
        #[derive(Props, Clone)]
        struct P {
            rig: fn() -> Rig,
            select: fn(&Rig) -> Option<Selection>,
            arm: Option<Armed>,
        }
        impl PartialEq for P {
            fn eq(&self, _: &Self) -> bool {
                false
            }
        }
        #[allow(non_snake_case)]
        fn Host(p: P) -> Element {
            let session = use_signal(|| RigSession::new((p.rig)()));
            let layout = use_signal(CanvasLayout::default);
            let selected = use_signal(|| (p.select)(session.peek().rig()));
            let hovered = use_signal(|| None);
            let armed = use_signal(|| p.arm);
            rsx! {
                RigCanvas {
                    session,
                    layout,
                    selected,
                    hovered,
                    armed,
                    on_edit: move |_: TopoEdit| false,
                    on_move: move |_| {},
                }
            }
        }
        let mut dom = VirtualDom::new_with_props(Host, P { rig, select, arm });
        dom.rebuild_in_place();
    }

    #[test]
    fn canvas_renders_finished_and_draft_rigs_with_every_selection() {
        mount(duplo10, |_| None, None);
        mount(
            blank_rig_fn,
            |_| None,
            Some(Armed::Body(BodyPreset::SpreaderBar)),
        );
        mount(
            duplo10,
            |rig| rig.members.keys().next().map(|id| Selection::Member(*id)),
            Some(Armed::Hardware(HardwarePreset::Shackle)),
        );
        mount(
            duplo10,
            |rig| {
                rig.members.keys().next().map(|id| Selection::Component {
                    member: *id,
                    segment: 0,
                    index: 0,
                })
            },
            None,
        );
        mount(
            duplo10,
            |rig| rig.bodies.keys().nth(1).map(|id| Selection::Body(*id)),
            None,
        );
        mount(
            duplo10,
            |rig| rig.nodes.keys().nth(3).map(|id| Selection::Node(*id)),
            None,
        );
    }

    fn blank_rig_fn() -> Rig {
        blank_rig("blank", Uuid::nil())
    }

    #[test]
    fn every_menu_has_labelled_actions() {
        let rig = duplo10();
        let member = *rig.members.keys().next().unwrap();
        let body = *rig.bodies.keys().nth(1).unwrap();
        let node = *rig.nodes.keys().nth(3).unwrap();
        for target in [
            MenuTarget::Background([0.0, 0.0]),
            MenuTarget::Body(body),
            MenuTarget::Port(node),
            MenuTarget::Member(member),
            MenuTarget::Component {
                member,
                segment: 0,
                index: 0,
            },
        ] {
            let actions = menu_actions(&rig, &target);
            assert!(!actions.is_empty(), "{target:?}");
            for action in &actions {
                let label = action_label(&rig, action);
                assert!(!label.is_empty() && label != "Apply", "{action:?}");
            }
        }
    }

    #[test]
    fn chips_are_centred_with_a_gap_on_each_side() {
        let (centers, gaps) = lay_chips([0.0, 0.0], [0.0, 200.0], &[60.0, 80.0, 60.0]);
        assert_eq!(centers.len(), 3);
        assert_eq!(gaps.len(), 4);
        assert!((centers[1][1] - 100.0).abs() < 1e-9);
        assert!(gaps[0][1] < centers[0][1] && gaps[3][1] > centers[2][1]);
        let (none, one) = lay_chips([0.0, 0.0], [0.0, 100.0], &[]);
        assert!(none.is_empty());
        assert_eq!(one, vec![[0.0, 50.0]]);
    }
}
