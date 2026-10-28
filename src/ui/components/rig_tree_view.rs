//! Rotatable 3D view of the rig with a tag on every member.
//!
//! Step: screen work (Step 2 D2-1)
//! Theory: docs/rig-model.md "Rig tree and tags".
//! Inputs: rig, its tree, a pose with optional member forces, rated items.
//! Outputs: SVG with drag-to-orbit; hover / select a member by id.
//! Must not depend on: store, solver internals.

use dioxus::prelude::*;
use indexmap::IndexMap;

use crate::ui::selection::Selection;
use bth_rigging::format::format_lbs;
use bth_rigging::rig::solve::MemberForce;
use bth_rigging::rig::solve::rate::Rated;
use bth_rigging::rig::{
    Camera, EvalRig, Item, MemberId, OrbitScene, PoseSource, Rig, RigTree, Role, Transform,
};

const VIEW_W: f64 = 960.0;
const VIEW_H: f64 = 460.0;
const MARGIN: f64 = 40.0;
const PILL_W: f64 = 34.0;
const PILL_H: f64 = 18.0;
/// Degrees of orbit per pixel of drag.
const DRAG_DEG_PER_PX: f64 = 0.4;
/// Pointer travel (px) above which a press is a drag, not a click.
const CLICK_SLOP_PX: f64 = 3.0;

#[component]
pub fn RigTreeView(
    rig: Rig,
    tree: RigTree,
    eval: EvalRig,
    forces: Option<IndexMap<MemberId, MemberForce>>,
    pose: PoseSource,
    rated: Vec<Rated>,
    selected: Signal<Option<Selection>>,
    hovered: Signal<Option<Selection>>,
) -> Element {
    let mut camera = use_signal(Camera::default);
    let mut drag_from = use_signal(|| None::<(f64, f64)>);
    let mut drag_travel = use_signal(|| 0.0_f64);

    let scene = OrbitScene::project(&eval, &rig, &tree, camera(), forces.as_ref(), &rated);
    let t = scene.fit(VIEW_W, VIEW_H, MARGIN);
    let labels = scene.layout_labels(&t, PILL_W, PILL_H);
    let hot = |id: MemberId| {
        selected().and_then(Selection::member) == Some(id)
            || hovered().and_then(Selection::member) == Some(id)
    };

    let presets = [
        ("Iso", Camera::ISO),
        ("Side", Camera::SIDE),
        ("End", Camera::END),
        ("Plan", Camera::PLAN),
    ];

    rsx! {
        section { class: "rig-tree-view",
            div { class: "rig-tree-toolbar",
                div { class: "rig-tree-presets",
                    for (name , preset) in presets {
                        button {
                            key: "{name}",
                            class: if camera() == preset { "btn btn-ghost sm is-active" } else { "btn btn-ghost sm" },
                            onclick: move |_| camera.set(preset),
                            "{name}"
                        }
                    }
                }
                span { class: "muted rig-tree-hint", "Drag to rotate · {pose.label()}" }
            }
            svg {
                class: if drag_from().is_some() { "rig-tree-svg is-dragging" } else { "rig-tree-svg" },
                view_box: "0 0 {VIEW_W} {VIEW_H}",
                xmlns: "http://www.w3.org/2000/svg",
                onmousedown: move |event| {
                    let p = event.client_coordinates();
                    drag_from.set(Some((p.x, p.y)));
                    drag_travel.set(0.0);
                },
                onmousemove: move |event| {
                    let Some((x0, y0)) = drag_from() else {
                        return;
                    };
                    let p = event.client_coordinates();
                    let (dx, dy) = (p.x - x0, p.y - y0);
                    drag_travel.set(drag_travel() + dx.abs() + dy.abs());
                    camera.set(camera().orbit(-dx * DRAG_DEG_PER_PX, dy * DRAG_DEG_PER_PX));
                    drag_from.set(Some((p.x, p.y)));
                },
                onmouseup: move |_| drag_from.set(None),
                onmouseleave: move |_| {
                    drag_from.set(None);
                    hovered.set(None);
                },
                rect {
                    class: "rig-tree-bg",
                    x: "0",
                    y: "0",
                    width: "{VIEW_W}",
                    height: "{VIEW_H}",
                    rx: "6",
                }
                for (index , item) in scene.bodies.iter().enumerate() {
                    {body_item(index, item, &t)}
                }
                for member in scene.members.iter() {
                    {
                        let id = member.id;
                        let points = polyline_points(&member.pts, &t);
                        let class = member_class(member.role, member.is_link, hot(id));
                        let title = match member.tension_lbs {
                            Some(tension) => format!("{} · {} lb", member.tag, format_lbs(tension)),
                            None => member.tag.clone(),
                        };
                        rsx! {
                            g { key: "{id}",
                                polyline { class: "{class}", points: "{points}" }
                                polyline {
                                    class: "rt-hit",
                                    points: "{points}",
                                    onmouseenter: move |_| hovered.set(Some(Selection::Member(id))),
                                    onmouseleave: move |_| hovered.set(None),
                                    onclick: move |_| {
                                        if drag_travel() <= CLICK_SLOP_PX {
                                            let next = if selected() == Some(Selection::Member(id)) {
                                                None
                                            } else {
                                                Some(Selection::Member(id))
                                            };
                                            selected.set(next);
                                        }
                                    },
                                    title { "{title}" }
                                }
                            }
                        }
                    }
                }
                for (index , item) in scene.nodes.iter().enumerate() {
                    {node_item(index, item, &t)}
                }
                for label in labels.iter() {
                    {
                        let id = label.id;
                        let tag = scene
                            .members
                            .iter()
                            .find(|m| m.id == id)
                            .map(|m| m.tag.clone())
                            .unwrap_or_default();
                        let [cx, cy] = label.center;
                        let [ax, ay] = label.anchor;
                        let moved = (cx - ax).hypot(cy - ay) > PILL_H * 0.5;
                        let class = if hot(id) { "rt-pill is-hot" } else { "rt-pill" };
                        rsx! {
                            g {
                                key: "pill-{id}",
                                class: "{class}",
                                onmouseenter: move |_| hovered.set(Some(Selection::Member(id))),
                                onmouseleave: move |_| hovered.set(None),
                                onclick: move |_| {
                                    if drag_travel() <= CLICK_SLOP_PX {
                                        let next = if selected() == Some(Selection::Member(id)) {
                                                None
                                            } else {
                                                Some(Selection::Member(id))
                                            };
                                        selected.set(next);
                                    }
                                },
                                if moved {
                                    line {
                                        class: "rt-leader",
                                        x1: "{ax:.1}",
                                        y1: "{ay:.1}",
                                        x2: "{cx:.1}",
                                        y2: "{cy:.1}",
                                    }
                                }
                                rect {
                                    x: "{cx - PILL_W / 2.0:.1}",
                                    y: "{cy - PILL_H / 2.0:.1}",
                                    width: "{PILL_W}",
                                    height: "{PILL_H}",
                                    rx: "4",
                                }
                                text {
                                    x: "{cx:.1}",
                                    y: "{cy + 4.0:.1}",
                                    text_anchor: "middle",
                                    "{tag}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "rig-tree-legend",
                span { class: "legend-item legend-member", "Member" }
                span { class: "legend-item legend-slack", "Slack" }
                span { class: "legend-item legend-short", "Short" }
                span { class: "legend-item legend-over", "Over" }
                span { class: "legend-item legend-link", "Link" }
            }
        }
    }
}

fn body_item(index: usize, item: &Item, t: &Transform) -> Element {
    match item {
        Item::Polyline {
            pts, role, label, ..
        } => {
            let points = polyline_points(pts, t);
            let class = match role {
                Role::Bar => "rt-bar",
                _ => "rt-load",
            };
            let text_at = (*role == Role::Bar && pts.len() >= 2).then(|| {
                let a = t.apply(pts[0]);
                let b = t.apply(pts[pts.len() - 1]);
                [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
            });
            rsx! {
                g { key: "body-{index}",
                    polyline { class: "{class}", points: "{points}" }
                    if let (Some([x, y]), Some(text)) = (text_at, label.as_ref()) {
                        text {
                            class: "rt-body-label",
                            x: "{x:.1}",
                            y: "{y - 8.0:.1}",
                            text_anchor: "middle",
                            "{text}"
                        }
                    }
                }
            }
        }
        Item::Marker {
            at, role, label, ..
        } => {
            let [x, y] = t.apply(*at);
            let hook = *role == Role::Hook;
            rsx! {
                g { key: "body-{index}",
                    if hook {
                        circle {
                            class: "rt-hook",
                            cx: "{x:.1}",
                            cy: "{y:.1}",
                            r: "7",
                        }
                    } else {
                        rect {
                            class: "rt-frame",
                            x: "{x - 4.0:.1}",
                            y: "{y - 4.0:.1}",
                            width: "8",
                            height: "8",
                        }
                    }
                    if let Some(text) = label {
                        text {
                            class: "rt-body-label",
                            x: "{x + 11.0:.1}",
                            y: "{y + 4.0:.1}",
                            "{text}"
                        }
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

fn node_item(index: usize, item: &Item, t: &Transform) -> Element {
    let Item::Marker {
        at, role, label, ..
    } = item
    else {
        return rsx! {};
    };
    if *role == Role::Hook {
        return rsx! {};
    }
    let [x, y] = t.apply(*at);
    let class = match role {
        Role::Bow | Role::Edge => "rt-node rt-bow",
        _ => "rt-node",
    };
    rsx! {
        circle {
            key: "node-{index}",
            class: "{class}",
            cx: "{x:.1}",
            cy: "{y:.1}",
            r: "2.6",
            if let Some(text) = label {
                title { "{text}" }
            }
        }
    }
}

fn polyline_points(pts: &[[f64; 2]], t: &Transform) -> String {
    pts.iter()
        .map(|p| {
            let [x, y] = t.apply(*p);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn member_class(role: Role, is_link: bool, hot: bool) -> String {
    let base = match role {
        Role::Overloaded => "rt-member rt-over",
        Role::SlackMember => "rt-member rt-slack",
        Role::ShortMember => "rt-member rt-short",
        _ if is_link => "rt-member rt-link",
        _ => "rt-member",
    };
    if hot {
        format!("{base} is-hot")
    } else {
        base.to_owned()
    }
}
