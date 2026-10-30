//! Rig build panel: build the rig from the load up.
//!
//! Every pick point starts open. Tick the open points, press what to gather
//! them into (master link, spreader bar, lifting beam, basket, the hook), and
//! the new item's top becomes the next open point. A recipe builds a whole
//! common rig in one click. Every gather is one undo step; Ungather takes an
//! item apart again. Engine: `bth_rigging::rig::gather`.

use dioxus::prelude::*;

use super::state::{RigState, state};
use crate::docs;
use crate::ui::selection::Selection;
use bth_rigging::rig::gather::{self, Built, GatherKind, GatherOptions, OpenPoint, Recipe};
use bth_rigging::rig::{BodyId, NodeId, Rig, TreeVertex};

#[allow(non_snake_case)]
pub fn Build() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    rsx! { BuildBody { st } }
}

#[component]
fn BuildBody(st: RigState) -> Element {
    let rig = st.session.read().rig().clone();
    let mut picked = st.picked;
    let opts = st.gather_opts;

    let progress = gather::status(&rig);
    // Drop picks that are no longer open (after undo, a gather, the canvas).
    let open_ids: Vec<NodeId> = progress.open.iter().map(|p| p.node).collect();
    let picks: Vec<NodeId> = picked()
        .into_iter()
        .filter(|n| open_ids.contains(n))
        .collect();

    let loads: Vec<(BodyId, String, Vec<Recipe>)> = rig
        .bodies
        .values()
        .filter(|b| b.kind.is_load())
        .map(|b| (b.id, b.label.clone(), gather::recipes_for(&rig, b.id)))
        .filter(|(_, _, r)| !r.is_empty())
        .collect();

    let groups = group_by_owner(&progress.open);
    let built = gather::built(&rig);
    let n_open = progress.open.len();
    let n_unhung = progress.unhung.len();

    rsx! {
        section { class: "inspector build-panel app-panel-pad",
            h3 { class: "insp-title", "Build from the load up" }
            if progress.done {
                p { class: "build-progress is-done", "Rig complete: everything hangs from the hook." }
            } else {
                p { class: "build-progress",
                    "{n_open} open point(s)"
                    if n_unhung > 0 { " · {n_unhung} item(s) not hung yet" }
                }
            }
            if !(st.status_message)().is_empty() {
                p { class: "status-line", "{st.status_message}" }
            }

            // ---------- recipes ----------
            for (load , label , recipes) in loads {
                div { class: "build-section",
                    h4 { class: "insp-subtitle", "Start {label} from a recipe" }
                    div { class: "build-recipes",
                        for recipe in recipes {
                            button {
                                class: "btn btn-ghost sm",
                                title: recipe.help(),
                                onclick: move |_| {
                                    let rig = st.session.peek().rig().clone();
                                    st.apply_plan(gather::apply_recipe(&rig, load, recipe, opts()));
                                },
                                "{recipe.label()}"
                            }
                        }
                    }
                }
            }

            // ---------- open points ----------
            div { class: "build-section",
                h4 { class: "insp-subtitle", "Open points" }
                if groups.is_empty() {
                    p { class: "insp-note muted",
                        if progress.done { "Nothing left to gather." } else { "No open points. Add a load from the Insert toolbar." }
                    }
                }
                for (_ , owner_label , points) in groups {
                    div { class: "build-group", key: "{owner_label}",
                        div { class: "build-group-head",
                            span { "{owner_label}" }
                            button {
                                class: "btn btn-ghost xs",
                                title: "Tick every open point on {owner_label}",
                                onclick: {
                                    let ids: Vec<NodeId> = points.iter().map(|p| p.node).collect();
                                    move |_| {
                                        let mut now: Vec<NodeId> = picked.peek().clone();
                                        for id in &ids {
                                            if !now.contains(id) {
                                                now.push(*id);
                                            }
                                        }
                                        picked.set(now);
                                    }
                                },
                                "all"
                            }
                        }
                        for p in points.iter() {
                            PointRow {
                                key: "{p.node}",
                                st,
                                point: p.clone(),
                                checked: picks.contains(&p.node),
                                on_toggle: move |id: NodeId| {
                                    let mut now = picked.peek().clone();
                                    if let Some(i) = now.iter().position(|n| *n == id) {
                                        now.remove(i);
                                    } else {
                                        now.push(id);
                                    }
                                    picked.set(now);
                                },
                            }
                        }
                    }
                }
            }

            // ---------- gather ----------
            if !open_ids.is_empty() {
                GatherButtons { st, rig: rig.clone(), picks: picks.clone(), opts }
                Options { opts }
            }

            // ---------- spare ----------
            if !progress.spare.is_empty() {
                details { class: "insp-group build-section",
                    summary { "Unused points ({progress.spare.len()})" }
                    p { class: "insp-note muted",
                        "These are on items that already hang from the hook. Leave them, or ungather the item to use them."
                    }
                    ul { class: "insp-list",
                        for p in progress.spare.iter() {
                            li { class: "insp-row", key: "{p.node}",
                                span { class: "insp-row-main", "{p.owner_label} · {p.label}" }
                            }
                        }
                    }
                }
            }

            // ---------- built ----------
            if !built.is_empty() {
                div { class: "build-section",
                    h4 { class: "insp-subtitle", "Built (top first)" }
                    ul { class: "insp-list",
                        for (what , label) in built {
                            BuiltRow { key: "{label}", st, what, label }
                        }
                    }
                    div { class: "insp-actions insp-danger",
                        button {
                            class: "btn btn-ghost sm",
                            title: "Remove every gathered item and member, back to the bare load",
                            onclick: move |_| {
                                let rig = st.session.peek().rig().clone();
                                st.apply_plan(gather::clear(&rig));
                            },
                            "Clear rigging"
                        }
                    }
                }
            }

            div { class: "insp-help",
                h4 { "How it works" }
                ol {
                    li { "Tick the open points that go to the same thing, e.g. both pick points. On the diagram, Shift+click an open (glowing) port." }
                    li { "Press what holds them. It is placed over them and sized: bar span to the points, legs over the CG at the target angle, hardware for the load." }
                    li { "Its top is now an open point. Keep gathering until you send the last one To hook." }
                    li { "Each gather is one Undo. Fine-tune anything in the Inspector or the diagram." }
                }
            }
        }
    }
}

/// Open points grouped by what they're on, in rig order.
fn group_by_owner(points: &[OpenPoint]) -> Vec<(TreeVertex, String, Vec<OpenPoint>)> {
    let mut out: Vec<(TreeVertex, String, Vec<OpenPoint>)> = Vec::new();
    for p in points {
        match out.iter_mut().find(|(o, _, _)| *o == p.owner) {
            Some((_, _, list)) => list.push(p.clone()),
            None => out.push((p.owner, p.owner_label.clone(), vec![p.clone()])),
        }
    }
    out
}

#[component]
fn PointRow(
    st: RigState,
    point: OpenPoint,
    checked: bool,
    on_toggle: EventHandler<NodeId>,
) -> Element {
    let id = point.node;
    let mut hovered = st.hovered;
    let kind = if point.bearing { "bearing" } else { "" };
    let label = if matches!(point.owner, TreeVertex::Knot(_)) {
        "link".to_owned()
    } else {
        point.label.clone()
    };
    rsx! {
        label {
            class: if checked { "build-point is-picked" } else { "build-point" },
            onmouseenter: move |_| hovered.set(Some(Selection::Node(id))),
            onmouseleave: move |_| hovered.set(None),
            input {
                r#type: "checkbox",
                checked,
                onchange: move |_| on_toggle.call(id),
            }
            span { class: "build-point-name", "{label}" }
            if !kind.is_empty() {
                span { class: "tag-pill", "{kind}" }
            }
        }
    }
}

#[component]
fn GatherButtons(
    st: RigState,
    rig: Rig,
    picks: Vec<NodeId>,
    opts: Signal<GatherOptions>,
) -> Element {
    let n = picks.len();
    rsx! {
        div { class: "build-section",
            h4 { class: "insp-subtitle",
                if n == 0 { "Gather (tick some points first)" } else { "Gather {n} point(s) into" }
            }
            div { class: "build-gather",
                for kind in GatherKind::ALL {
                    {
                        let verdict = gather::check(&rig, &picks, kind);
                        let title = match &verdict {
                            Ok(()) => kind.help().to_owned(),
                            Err(why) => why.clone(),
                        };
                        let ends = picks.clone();
                        rsx! {
                            button {
                                class: if kind == GatherKind::Hook { "btn btn-primary sm" } else { "btn btn-ghost sm" },
                                disabled: verdict.is_err(),
                                title: "{title}",
                                onclick: move |_| {
                                    let rig = st.session.peek().rig().clone();
                                    // The points it creates come back ticked: usually the next step.
                                    st.apply_plan(gather::gather(&rig, &ends, kind, opts()));
                                },
                                "{kind.label()}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Options(opts: Signal<GatherOptions>) -> Element {
    let o = opts();
    let hardware = if o.auto_size { "auto" } else { "default" };
    let mut angle_err = use_signal(|| None::<String>);
    let mut drop_err = use_signal(|| None::<String>);
    rsx! {
        details { class: "insp-group build-section",
            summary { "Sizing: {o.angle_deg:.0}° legs, {o.drop_ft:.1} ft drops, hardware {hardware}" }
            label { class: "insp-field",
                span { class: "field-label", "Sling angle from horizontal" }
                span { class: "insp-input",
                    input {
                        class: "field-input sm",
                        value: "{o.angle_deg}",
                        onchange: move |e| match e.value().trim().parse::<f64>() {
                            Ok(v) if (30.0..=89.0).contains(&v) => {
                                angle_err.set(None);
                                opts.write().angle_deg = v;
                            }
                            _ => angle_err.set(Some("30° to 89° (below 30° is not allowed)".into())),
                        },
                    }
                    span { class: "insp-unit", "°" }
                }
                if let Some(e) = angle_err() {
                    span { class: "insp-error", "{e}" }
                } else {
                    span { class: "insp-hint", "The flattest bridle leg is set to this angle." }
                }
            }
            label { class: "insp-field",
                span { class: "field-label", "Plumb drop leg" }
                span { class: "insp-input",
                    input {
                        class: "field-input sm",
                        value: "{o.drop_ft}",
                        onchange: move |e| match e.value().trim().parse::<f64>() {
                            Ok(v) if v > 0.0 && v.is_finite() => {
                                drop_err.set(None);
                                opts.write().drop_ft = v;
                            }
                            _ => drop_err.set(Some("a length in feet, more than 0".into())),
                        },
                    }
                    span { class: "insp-unit", "ft" }
                }
                if let Some(e) = drop_err() {
                    span { class: "insp-error", "{e}" }
                }
            }
            label { class: "build-point",
                input {
                    r#type: "checkbox",
                    checked: o.auto_size,
                    onchange: move |_| {
                        let now = opts.peek().auto_size;
                        opts.write().auto_size = !now;
                    },
                }
                span { "Pick sling and shackle sizes for the estimated tension" }
            }
        }
    }
}

#[component]
fn BuiltRow(st: RigState, what: Built, label: String) -> Element {
    let mut hovered = st.hovered;
    let hover = match what {
        Built::Item(TreeVertex::Body(id)) => Some(Selection::Body(id)),
        Built::Item(TreeVertex::Knot(id)) => Some(Selection::Node(id)),
        Built::Hook => None,
    };
    rsx! {
        li {
            class: "insp-row",
            onmouseenter: move |_| hovered.set(hover),
            onmouseleave: move |_| hovered.set(None),
            onclick: move |_| {
                if let Some(s) = hover {
                    st.select(Some(s));
                }
            },
            span { class: "insp-row-main", "{label}" }
            span { class: "insp-row-tools",
                button {
                    class: "btn btn-ghost xs",
                    title: "Take this apart; what it held becomes open again",
                    onclick: move |e| {
                        e.stop_propagation();
                        let rig = st.session.peek().rig().clone();
                        st.apply_plan(gather::ungather(&rig, what));
                    },
                    "Ungather"
                }
            }
        }
    }
}
