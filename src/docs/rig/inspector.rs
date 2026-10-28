//! Rig inspector panel: edit whatever is selected on the canvas — a body and
//! its ports, a port or master link, a member's route and hardware, or one
//! piece of hardware with catalog pickers.

use dioxus::prelude::*;

use super::state::{RigState, state};
use crate::docs;
use crate::ui::selection::Selection;
use bth_rigging::catalog::{CHAINS, POLYESTER_ROUNDSLINGS, SHACKLES, find_chain};
use bth_rigging::domain::Hitch;
use bth_rigging::format::{format_ft_in, format_lbs};
use bth_rigging::rig::preset::{self, HardwarePreset, PortKind};
use bth_rigging::rig::topo::{BodyField, ComponentField, NodeField, TopoEdit};
use bth_rigging::rig::{
    BodyId, BodyKind, Component, ComponentKind, Expr, MemberId, NodeId, NodeKind, Quantity, Rig,
    RigTree,
};

#[allow(non_snake_case)]
pub fn Inspector() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    rsx! { InspectorBody { st } }
}

#[component]
fn InspectorBody(st: RigState) -> Element {
    let rig = st.session.read().rig().clone();
    let selection = (st.selected)().filter(|s| s.exists_in(&rig));
    rsx! {
        section { class: "inspector app-panel-pad",
            if !(st.status_message)().is_empty() {
                p { class: "status-line", "{st.status_message}" }
            }
            match selection {
                None => rsx! { Overview { st, rig } },
                Some(Selection::Body(id)) => rsx! { BodyPanel { st, rig, id } },
                Some(Selection::Node(id)) => rsx! { NodePanel { st, rig, id } },
                Some(Selection::Member(id)) => rsx! { MemberPanel { st, rig, id } },
                Some(Selection::Component { member, segment, index }) => rsx! {
                    ComponentPanel { st, rig, member, segment, index }
                },
            }
        }
    }
}

// ---------- shared fields ----------

/// A text input that commits on Enter or blur. `commit` returns an error to
/// show under the field. Give it a `key` that includes the value so it resets
/// when the value changes elsewhere (undo, another panel).
#[component]
fn Field(
    #[props(into)] label: String,
    value: String,
    #[props(default, into)] unit: String,
    #[props(default, into)] hint: String,
    commit: Callback<String, Option<String>>,
) -> Element {
    let mut error = use_signal(|| None::<String>);
    rsx! {
        label { class: "insp-field",
            span { class: "field-label", "{label}" }
            span { class: "insp-input",
                input {
                    class: if error().is_some() { "field-input sm is-invalid" } else { "field-input sm" },
                    value: "{value}",
                    spellcheck: "false",
                    onchange: move |e| error.set(commit.call(e.value())),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            e.stop_propagation();
                        }
                    },
                }
                if !unit.is_empty() {
                    span { class: "insp-unit", "{unit}" }
                }
            }
            if let Some(message) = error() {
                span { class: "insp-error", "{message}" }
            } else if !hint.is_empty() {
                span { class: "insp-hint", "{hint}" }
            }
        }
    }
}

fn expr_text(rig: &Rig, expr: &Expr) -> String {
    expr.to_text(&rig.params)
}

/// Feet-inches readout for a length expression that isn't a plain number.
fn length_hint(rig: &Rig, expr: &Expr) -> String {
    expr.eval(&rig.params)
        .map(|v| format!("= {}", format_ft_in(v, 16)))
        .unwrap_or_default()
}

fn weight_hint(rig: &Rig, expr: &Expr) -> String {
    match expr {
        Expr::Const(_) => String::new(),
        _ => expr
            .eval(&rig.params)
            .map(|v| format!("= {} lb", format_lbs(v)))
            .unwrap_or_default(),
    }
}

fn hint_for(rig: &Rig, expr: &Expr, quantity: Quantity) -> String {
    match quantity {
        Quantity::Length => length_hint(rig, expr),
        Quantity::Weight => weight_hint(rig, expr),
        _ => String::new(),
    }
}

fn unit_for(quantity: Quantity) -> String {
    match quantity {
        Quantity::Length => "ft".into(),
        Quantity::Weight => "lb".into(),
        _ => String::new(),
    }
}

// ---------- nothing selected ----------

#[component]
fn Overview(st: RigState, rig: Rig) -> Element {
    let mut session = st.session;
    let weights = rig.weights().ok();
    let hardware: usize = rig
        .members
        .values()
        .flat_map(|m| m.segments.iter())
        .map(|s| s.components.len())
        .sum();
    let name = rig.name.clone();
    rsx! {
        h3 { class: "insp-title", "Rig" }
        div {
            Field {
                key: "rig-name-{name}",
                label: "Name",
                value: name.clone(),
                commit: move |text: String| {
                    let text = text.trim().to_owned();
                    if text.is_empty() {
                        return Some("A rig needs a name".into());
                    }
                    session.write().rename(&text);
                    None
                },
            }
        }
        dl { class: "insp-facts",
            dt { "Bodies" }
            dd { "{rig.bodies.len()}" }
            dt { "Members" }
            dd { "{rig.members.len()}" }
            dt { "Hardware pieces" }
            dd { "{hardware}" }
            if let Some(w) = weights {
                dt { "Below the hook" }
                dd { "{format_lbs(w.total_below_root_lbs)} lb" }
            }
        }
        div { class: "insp-help",
            h4 { "Building the rig" }
            ol {
                li { "Add a spreader bar, lifting beam, load or master link from the palette above the diagram, then click where it should go." }
                li { "Drag from one port to another to hang a member between them. It starts as shackle, roundsling, shackle." }
                li { "To make a basket, click the first lug, then a bow or edge, then the other lug." }
                li { "Pick a hardware item in the palette and click a + on a member to insert it, or right-click a chip to replace or move it." }
                li { "Select anything to edit it here. Delete removes the selection; Ctrl+K finds every command." }
            }
        }
    }
}

// ---------- body ----------

#[component]
fn BodyPanel(st: RigState, rig: Rig, id: BodyId) -> Element {
    let Some(body) = rig.bodies.get(&id).cloned() else {
        return rsx! {};
    };
    let is_hook = matches!(body.kind, BodyKind::Hook);
    let mut fields: Vec<(&'static str, BodyField, Expr)> = Vec::new();
    if !is_hook {
        fields.push(("Weight", BodyField::Weight, body.weight.clone()));
    }
    match &body.kind {
        BodyKind::SpreaderBar { span, .. } | BodyKind::LiftingBeam { span, .. } => {
            fields.push(("Span", BodyField::Span, span.clone()));
        }
        BodyKind::Load {
            length,
            width,
            height,
        } => {
            fields.push(("Length", BodyField::Length, length.clone()));
            fields.push(("Width", BodyField::Width, width.clone()));
            fields.push(("Height", BodyField::Height, height.clone()));
        }
        _ => {}
    }
    let cg = [
        ("CG x", BodyField::CgX, body.cg.x.clone()),
        ("CG y", BodyField::CgY, body.cg.y.clone()),
        ("CG z", BodyField::CgZ, body.cg.z.clone()),
    ];
    let rating = match &body.kind {
        BodyKind::SpreaderBar { rating, .. } | BodyKind::LiftingBeam { rating, .. } => {
            Some(rating.wll_lbs)
        }
        _ => None,
    };
    let ports: Vec<NodeId> = rig
        .nodes
        .values()
        .filter(|n| n.body == Some(id))
        .map(|n| n.id)
        .collect();
    let label = body.label.clone();
    rsx! {
        h3 { class: "insp-title",
            "{label}"
            span { class: "insp-kind", "{body.kind.label()}" }
        }
        if !is_hook {
            Field {
                key: "body-name-{id}-{label}",
                label: "Name",
                value: label.clone(),
                commit: move |text: String| st.try_edit(TopoEdit::RenameBody { id, label: text.trim().to_owned() }),
            }
        }
        for (name , field , expr) in fields {
            Field {
                key: "body-{id}-{name}-{expr_text(&rig, &expr)}",
                label: name,
                value: expr_text(&rig, &expr),
                unit: unit_for(field.quantity()),
                hint: hint_for(&rig, &expr, field.quantity()),
                commit: move |text: String| st.try_edit(TopoEdit::SetBodyExpr { id, field, text }),
            }
        }
        if let Some(wll) = rating {
            Field {
                key: "body-wll-{id}-{wll}",
                label: "WLL",
                value: wll.to_string(),
                unit: "lb",
                commit: move |text: String| match text.trim().replace(',', "").parse::<u32>() {
                    Ok(wll_lbs) => st.try_edit(TopoEdit::SetBarRating { id, wll_lbs }),
                    Err(_) => Some("Enter a whole number of pounds".into()),
                },
            }
        }
        if !is_hook {
            details { class: "insp-group",
                summary { "Centre of gravity (body frame)" }
                for (name , field , expr) in cg {
                    Field {
                        key: "body-{id}-{name}-{expr_text(&rig, &expr)}",
                        label: name,
                        value: expr_text(&rig, &expr),
                        unit: "ft",
                        commit: move |text: String| st.try_edit(TopoEdit::SetBodyExpr { id, field, text }),
                    }
                }
            }
        }
        h4 { class: "insp-subtitle", "Ports" }
        ul { class: "insp-list",
            for node in ports {
                {
                    let n = rig.nodes[&node].clone();
                    let used = rig.members.values().filter(|m| m.path.contains(&node)).count();
                    rsx! {
                        li {
                            key: "{node}",
                            class: "insp-row",
                            onclick: move |_| st.select(Some(Selection::Node(node))),
                            span { class: "insp-row-main", "{n.label}" }
                            span { class: "muted", "{n.kind.label()} · {used} member(s)" }
                        }
                    }
                }
            }
        }
        if !is_hook {
            div { class: "insp-actions",
                for kind in PortKind::ALL {
                    button {
                        class: "btn btn-ghost sm",
                        onclick: move |_| {
                            let edit = preset::port(st.session.peek().rig(), id, kind);
                            st.edit(edit);
                        },
                        "+ {kind.label()}"
                    }
                }
            }
            div { class: "insp-actions insp-danger",
                button {
                    class: "btn btn-ghost sm",
                    onclick: move |_| {
                        st.edit(TopoEdit::RemoveBody { id });
                    },
                    "Remove {label} and its members"
                }
            }
        }
    }
}

// ---------- port / master link ----------

#[component]
fn NodePanel(st: RigState, rig: Rig, id: NodeId) -> Element {
    let Some(node) = rig.nodes.get(&id).cloned() else {
        return rsx! {};
    };
    let body = node.body.and_then(|b| rig.bodies.get(&b)).cloned();
    let tree = RigTree::build(&rig);
    let users: Vec<(MemberId, String)> = rig
        .members
        .values()
        .filter(|m| m.path.contains(&id))
        .map(|m| {
            let tag = tree.tag_of(m.id).unwrap_or("").to_owned();
            (m.id, format!("{tag} {}", m.label))
        })
        .collect();
    let mut coords: Vec<(&'static str, NodeField, Expr)> = Vec::new();
    if body.is_some() && !node.kind.is_root() {
        coords.push(("x", NodeField::X, node.local.x.clone()));
        coords.push(("y", NodeField::Y, node.local.y.clone()));
        coords.push(("z", NodeField::Z, node.local.z.clone()));
    }
    match &node.kind {
        NodeKind::Bow { bow_dia, mu } => {
            coords.push(("Bow diameter", NodeField::BowDia, bow_dia.clone()));
            coords.push(("Friction μ", NodeField::Mu, mu.clone()));
        }
        NodeKind::Edge { radius, mu, .. } => {
            coords.push(("Edge radius", NodeField::EdgeRadius, radius.clone()));
            coords.push(("Friction μ", NodeField::Mu, mu.clone()));
        }
        _ => {}
    }
    let kind_editable = body.is_some() && !node.kind.is_root();
    let current_kind = node.kind.label();
    let label = node.label.clone();
    rsx! {
        h3 { class: "insp-title",
            "{label}"
            span { class: "insp-kind",
                if let Some(b) = &body {
                    "{node.kind.label()} on {b.label}"
                } else {
                    "master link (junction)"
                }
            }
        }
        if let Some(b) = &body {
            {
                let bid = b.id;
                rsx! {
                    button { class: "btn btn-ghost sm", onclick: move |_| st.select(Some(Selection::Body(bid))), "Select {b.label}" }
                }
            }
        }
        if !node.kind.is_root() {
            Field {
                key: "node-name-{id}-{label}",
                label: "Name",
                value: label.clone(),
                commit: move |text: String| st.try_edit(TopoEdit::RenameNode { id, label: text.trim().to_owned() }),
            }
        }
        if kind_editable {
            label { class: "insp-field",
                span { class: "field-label", "Kind" }
                select {
                    class: "field-input sm",
                    onchange: move |e| {
                        if let Some(kind) = PortKind::ALL.iter().find(|k| k.label() == e.value()) {
                            st.edit(TopoEdit::SetNodeKind { id, kind: kind.node_kind() });
                        }
                    },
                    for kind in PortKind::ALL {
                        option {
                            value: "{kind.label()}",
                            selected: kind.node_kind().label() == current_kind,
                            "{kind.label()}"
                        }
                    }
                }
            }
        }
        if !coords.is_empty() {
            p { class: "insp-note muted", "Coordinates are in the body frame, ft. Type a number or an expression such as bar_span/2." }
        }
        for (name , field , expr) in coords {
            Field {
                key: "node-{id}-{name}-{expr_text(&rig, &expr)}",
                label: name,
                value: expr_text(&rig, &expr),
                unit: if field == NodeField::Mu { String::new() } else { "ft".to_owned() },
                commit: move |text: String| st.try_edit(TopoEdit::SetNodeExpr { id, field, text }),
            }
        }
        h4 { class: "insp-subtitle", "Members here" }
        if users.is_empty() {
            p { class: "muted", "None yet. Drag from this port to another on the diagram." }
        }
        ul { class: "insp-list",
            for (mid , text) in users {
                li {
                    key: "{mid}",
                    class: "insp-row",
                    onclick: move |_| st.select(Some(Selection::Member(mid))),
                    span { class: "insp-row-main", "{text}" }
                }
            }
        }
        if !node.kind.is_root() {
            div { class: "insp-actions insp-danger",
                button {
                    class: "btn btn-ghost sm",
                    onclick: move |_| {
                        st.edit(TopoEdit::RemoveNode { id });
                    },
                    "Remove {label}"
                }
            }
        }
    }
}

// ---------- member ----------

#[component]
fn MemberPanel(st: RigState, rig: Rig, id: MemberId) -> Element {
    let Some(member) = rig.members.get(&id).cloned() else {
        return rsx! {};
    };
    let tree = RigTree::build(&rig);
    let tag = tree.tag_of(id).unwrap_or("").to_owned();
    let shape = tree.get(id).map(|t| t.shape.label()).unwrap_or("member");
    let force = st
        .session
        .read()
        .snapshot()
        .solved
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|s| s.members.get(&id).cloned());
    let length = member.nominal_length(&rig.params).unwrap_or(0.0);
    let weight = member.weight(&rig.params).unwrap_or(0.0);
    let rating = member.min_rating();
    let stops: Vec<(NodeId, String)> = rig
        .nodes
        .values()
        .map(|n| (n.id, preset::stop_label(&rig, n.id)))
        .collect();
    let bows: Vec<(NodeId, String)> = rig
        .nodes
        .values()
        .filter(|n| n.kind.is_bearing_capable() && !member.path.contains(&n.id))
        .map(|n| (n.id, preset::stop_label(&rig, n.id)))
        .collect();
    let path = member.path.clone();
    let label = member.label.clone();
    rsx! {
        h3 { class: "insp-title",
            span { class: "tag-pill", "{tag}" }
            " {label}"
            span { class: "insp-kind", "{shape}" }
        }
        div {
            Field {
                key: "member-name-{id}-{label}",
                label: "Name",
                value: label.clone(),
                commit: move |text: String| st.try_edit(TopoEdit::RenameMember { id, label: text.trim().to_owned() }),
            }
        }
        dl { class: "insp-facts",
            dt { "Length" }
            dd { "{format_ft_in(length, 16)}" }
            dt { "Weight" }
            dd { "{format_lbs(weight)} lb" }
            if let Some(r) = &rating {
                dt { "Lowest rating" }
                dd { "{format_lbs(f64::from(r.wll_lbs))} lb ({r.label})" }
            }
            if let Some(f) = &force {
                dt { "Tension" }
                dd {
                    "{format_lbs(f.tension_lbs)} lb"
                    if !f.taut && !f.is_link {
                        span { class: "muted", " · slack" }
                    }
                }
            }
        }
        h4 { class: "insp-subtitle", "Route" }
        for (i , stop) in path.iter().copied().enumerate() {
            {
                let path_now = path.clone();
                let interior = i > 0 && i + 1 < path.len();
                let path_cut = path.clone();
                rsx! {
                    div { key: "stop-{i}", class: "insp-stop",
                        span { class: "insp-stop-index", "{i + 1}" }
                        select {
                            class: "field-input sm",
                            onchange: move |e| {
                                let Ok(uuid) = uuid::Uuid::parse_str(&e.value()) else { return };
                                let mut next = path_now.clone();
                                next[i] = NodeId(uuid);
                                st.edit(TopoEdit::Reroute { id, path: next });
                            },
                            for (nid , text) in stops.iter() {
                                option { value: "{nid.0}", selected: *nid == stop, "{text}" }
                            }
                        }
                        if interior {
                            button {
                                class: "btn btn-ghost sm",
                                title: "Stop passing through this bearing",
                                onclick: move |_| {
                                    let mut next = path_cut.clone();
                                    next.remove(i);
                                    st.edit(TopoEdit::Reroute { id, path: next });
                                },
                                "✕"
                            }
                        }
                    }
                }
            }
        }
        if !bows.is_empty() {
            {
                let path_add = path.clone();
                rsx! {
                    label { class: "insp-field",
                        span { class: "field-label", "Pass through a bow or edge" }
                        select {
                            class: "field-input sm",
                            onchange: move |e| {
                                let Ok(uuid) = uuid::Uuid::parse_str(&e.value()) else { return };
                                let mut next = path_add.clone();
                                next.insert(next.len() - 1, NodeId(uuid));
                                st.edit(TopoEdit::Reroute { id, path: next });
                            },
                            option { value: "", selected: true, "Choose…" }
                            for (nid , text) in bows.iter() {
                                option { value: "{nid.0}", "{text}" }
                            }
                        }
                    }
                }
            }
        }
        h4 { class: "insp-subtitle", "Hardware (top to bottom)" }
        for (si , seg) in member.segments.iter().enumerate() {
            div { key: "seg-{si}", class: "insp-run",
                if member.segments.len() > 1 {
                    p { class: "insp-run-title muted",
                        "{preset::stop_label(&rig, member.path[si])} → {preset::stop_label(&rig, member.path[si + 1])}"
                    }
                }
                ul { class: "insp-list",
                    for (ci , c) in seg.components.iter().enumerate() {
                        {
                            let (text, title) = crate::ui::components::rig_canvas::chip_text(c, &rig.params);
                            let count = seg.components.len();
                            rsx! {
                                li { key: "c-{si}-{ci}", class: "insp-row", title: "{title}",
                                    span {
                                        class: "insp-row-main",
                                        onclick: move |_| st.select(Some(Selection::Component { member: id, segment: si, index: ci })),
                                        "{text}"
                                    }
                                    span { class: "insp-row-tools",
                                        button {
                                            class: "btn btn-ghost xs",
                                            disabled: ci == 0,
                                            title: "Move up",
                                            onclick: move |_| {
                                                st.edit(TopoEdit::MoveComponent { member: id, segment: si, from: ci, to: ci - 1 });
                                            },
                                            "↑"
                                        }
                                        button {
                                            class: "btn btn-ghost xs",
                                            disabled: ci + 1 >= count,
                                            title: "Move down",
                                            onclick: move |_| {
                                                st.edit(TopoEdit::MoveComponent { member: id, segment: si, from: ci, to: ci + 1 });
                                            },
                                            "↓"
                                        }
                                        button {
                                            class: "btn btn-ghost xs",
                                            title: "Remove",
                                            onclick: move |_| {
                                                st.edit(TopoEdit::RemoveComponent { member: id, segment: si, index: ci });
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                {
                    let end = seg.components.len();
                    rsx! {
                        select {
                            class: "field-input sm",
                            onchange: move |e| {
                                if let Some(h) = HardwarePreset::ALL.iter().find(|h| h.label() == e.value()) {
                                    st.edit(TopoEdit::InsertComponent {
                                        member: id,
                                        segment: si,
                                        index: end,
                                        component: h.component(),
                                    });
                                }
                            },
                            option { value: "", selected: true, "+ Add hardware at the bottom…" }
                            for h in HardwarePreset::ALL {
                                option { value: "{h.label()}", "{h.label()}" }
                            }
                        }
                    }
                }
            }
        }
        div { class: "insp-actions insp-danger",
            button {
                class: "btn btn-ghost sm",
                onclick: move |_| {
                    st.edit(TopoEdit::RemoveMember { id });
                },
                "Remove member"
            }
        }
    }
}

// ---------- one piece of hardware ----------

#[component]
fn ComponentPanel(
    st: RigState,
    rig: Rig,
    member: MemberId,
    segment: usize,
    index: usize,
) -> Element {
    let Some(c) = rig
        .members
        .get(&member)
        .and_then(|m| m.segments.get(segment))
        .and_then(|s| s.components.get(index))
        .cloned()
    else {
        return rsx! {};
    };
    let count = rig.members[&member].segments[segment].components.len();
    let tree = RigTree::build(&rig);
    let tag = tree.tag_of(member).unwrap_or("").to_owned();
    let member_label = rig.members[&member].label.clone();
    let replace = move |next: Component| {
        st.edit(TopoEdit::ReplaceComponent {
            member,
            segment,
            index,
            component: next,
        });
    };
    let params = rig.params.clone();
    let current_kind = c.kind.label();
    let kind_name = HardwarePreset::ALL
        .iter()
        .find(|h| std::mem::discriminant(&h.component().kind) == std::mem::discriminant(&c.kind))
        .map(|h| h.label())
        .unwrap_or("");
    let mut fields: Vec<(&'static str, ComponentField, Expr)> = Vec::new();
    if !matches!(c.kind, ComponentKind::Shackle { .. }) {
        fields.push(("Length", ComponentField::Length, c.length.clone()));
    }
    fields.push(("Weight", ComponentField::Weight, c.weight.clone()));
    match &c.kind {
        ComponentKind::Strap { width_in } => {
            fields.push(("Width (in)", ComponentField::SizeIn, width_in.clone()))
        }
        ComponentKind::WireRope { dia_in } => {
            fields.push(("Diameter (in)", ComponentField::SizeIn, dia_in.clone()))
        }
        _ => {}
    }
    if let Some(adjust) = &c.adjust {
        fields.push(("Take-up min", ComponentField::AdjustMin, adjust.min.clone()));
        fields.push(("Take-up max", ComponentField::AdjustMax, adjust.max.clone()));
        fields.push((
            "Take-up setting",
            ComponentField::AdjustSetting,
            adjust.setting.clone(),
        ));
    }
    let wll = c.catalog_wll_lbs().or_else(|| match &c.kind {
        ComponentKind::Chain { grade, size_in } => size_in
            .eval(&rig.params)
            .ok()
            .and_then(|s| find_chain(*grade, s))
            .map(|r| r.wll_lbs),
        _ => None,
    });
    let c_for_kind = c.clone();
    let params_for_kind = params.clone();
    rsx! {
        h3 { class: "insp-title",
            "{current_kind}"
            span { class: "insp-kind", "in {tag} {member_label}" }
        }
        button {
            class: "btn btn-ghost sm",
            onclick: move |_| st.select(Some(Selection::Member(member))),
            "Select the whole member"
        }
        label { class: "insp-field",
            span { class: "field-label", "Type" }
            select {
                class: "field-input sm",
                onchange: move |e| {
                    if let Some(h) = HardwarePreset::ALL.iter().find(|h| h.label() == e.value()) {
                        replace(preset::replace_with(&c_for_kind, *h, &params_for_kind));
                    }
                },
                for h in HardwarePreset::ALL {
                    option { value: "{h.label()}", selected: h.label() == kind_name, "{h.label()}" }
                }
            }
        }
        {catalog_pickers(&c, &params, replace)}
        for (name , field , expr) in fields {
            Field {
                key: "comp-{member}-{segment}-{index}-{name}-{expr_text(&rig, &expr)}",
                label: name,
                value: expr_text(&rig, &expr),
                unit: unit_for(field.quantity()),
                hint: hint_for(&rig, &expr, field.quantity()),
                commit: move |text: String| st.try_edit(TopoEdit::SetComponentExpr { member, segment, index, field, text }),
            }
        }
        if let Some(wll) = wll {
            dl { class: "insp-facts",
                dt { "Rated WLL" }
                dd { "{format_lbs(f64::from(wll))} lb" }
            }
        }
        div { class: "insp-actions",
            button {
                class: "btn btn-ghost sm",
                disabled: index == 0,
                onclick: move |_| {
                    if st.edit(TopoEdit::MoveComponent { member, segment, from: index, to: index - 1 }) {
                        st.select(Some(Selection::Component { member, segment, index: index - 1 }));
                    }
                },
                "Move up"
            }
            button {
                class: "btn btn-ghost sm",
                disabled: index + 1 >= count,
                onclick: move |_| {
                    if st.edit(TopoEdit::MoveComponent { member, segment, from: index, to: index + 1 }) {
                        st.select(Some(Selection::Component { member, segment, index: index + 1 }));
                    }
                },
                "Move down"
            }
            button {
                class: "btn btn-ghost sm",
                onclick: move |_| {
                    if st.edit(TopoEdit::InsertComponent {
                        member,
                        segment,
                        index: index + 1,
                        component: Component::shackle(preset::DEFAULT_SHACKLE),
                    }) {
                        st.select(Some(Selection::Component { member, segment, index: index + 1 }));
                    }
                },
                "Add shackle below"
            }
        }
        div { class: "insp-actions insp-danger",
            button {
                class: "btn btn-ghost sm",
                onclick: move |_| {
                    st.edit(TopoEdit::RemoveComponent { member, segment, index });
                },
                "Remove"
            }
        }
    }
}

/// Size, hitch and grade pickers backed by the hardware catalogs.
fn catalog_pickers(
    c: &Component,
    params: &bth_rigging::rig::ParamTable,
    replace: impl FnMut(Component) + Copy + 'static,
) -> Element {
    match &c.kind {
        ComponentKind::Shackle { size } => {
            let current = size.clone();
            let mut replace = replace;
            rsx! {
                label { class: "insp-field",
                    span { class: "field-label", "Shackle size" }
                    select {
                        class: "field-input sm",
                        onchange: move |e| replace(Component::shackle(e.value())),
                        for s in SHACKLES {
                            option { value: "{s.size_in}", selected: s.size_in == current, "{s.label()}" }
                        }
                    }
                }
            }
        }
        ComponentKind::RoundSling { size, hitch } => {
            let (size, hitch) = (*size, *hitch);
            let c1 = c.clone();
            let c2 = c.clone();
            let p1 = params.clone();
            let p2 = params.clone();
            let mut replace1 = replace;
            let mut replace2 = replace;
            rsx! {
                label { class: "insp-field",
                    span { class: "field-label", "Roundsling size" }
                    select {
                        class: "field-input sm",
                        onchange: move |e| {
                            if let Ok(next) = e.value().parse::<u8>() {
                                replace1(preset::resize_roundsling(&c1, next, hitch, &p1));
                            }
                        },
                        for r in POLYESTER_ROUNDSLINGS {
                            option { value: "{r.size}", selected: r.size == size, "{r.label()}" }
                        }
                    }
                }
                label { class: "insp-field",
                    span { class: "field-label", "Hitch" }
                    select {
                        class: "field-input sm",
                        onchange: move |e| {
                            if let Some(next) = Hitch::all().into_iter().find(|h| h.label() == e.value()) {
                                replace2(preset::resize_roundsling(&c2, size, next, &p2));
                            }
                        },
                        for h in Hitch::all() {
                            option { value: "{h.label()}", selected: h == hitch, "{h.label()}" }
                        }
                    }
                }
            }
        }
        ComponentKind::Chain { grade, size_in } => {
            let grade = *grade;
            let size = size_in.eval(params).unwrap_or(0.0);
            let c1 = c.clone();
            let c2 = c.clone();
            let mut replace1 = replace;
            let mut replace2 = replace;
            rsx! {
                label { class: "insp-field",
                    span { class: "field-label", "Grade" }
                    select {
                        class: "field-input sm",
                        onchange: move |e| {
                            if let Ok(next) = e.value().parse::<u8>() {
                                let mut c = c1.clone();
                                c.kind = ComponentKind::Chain { grade: next, size_in: Expr::c(size) };
                                replace1(c);
                            }
                        },
                        for g in [80u8, 100] {
                            option { value: "{g}", selected: g == grade, "Grade {g}" }
                        }
                    }
                }
                label { class: "insp-field",
                    span { class: "field-label", "Chain size" }
                    select {
                        class: "field-input sm",
                        onchange: move |e| {
                            if let Ok(next) = e.value().parse::<f64>() {
                                let mut c = c2.clone();
                                c.kind = ComponentKind::Chain { grade, size_in: Expr::c(next) };
                                replace2(c);
                            }
                        },
                        for r in CHAINS.iter().filter(|r| r.grade == grade) {
                            option {
                                value: "{r.size_in}",
                                selected: (r.size_in - size).abs() < 1.0 / 128.0,
                                "{r.label}″ · {format_lbs(f64::from(r.wll_lbs))} lb"
                            }
                        }
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::rig::state::ViewMode;
    use bth_rigging::rig::layout::CanvasLayout;
    use bth_rigging::rig::session::RigSession;
    use bth_rigging::rig::{duplo10, preset::blank_rig};

    #[derive(Props, Clone)]
    struct P {
        rig: fn() -> Rig,
        select: fn(&Rig) -> Option<Selection>,
    }

    impl PartialEq for P {
        fn eq(&self, _: &Self) -> bool {
            false
        }
    }

    #[allow(non_snake_case)]
    fn Host(p: P) -> Element {
        let session = use_signal(|| RigSession::new((p.rig)()));
        let selected = use_signal(|| (p.select)(session.peek().rig()));
        let st = RigState {
            session,
            status_message: use_signal(String::new),
            selected,
            hovered: use_signal(|| None),
            view_mode: use_signal(|| ViewMode::Diagram),
            armed: use_signal(|| None),
            layout: use_signal(CanvasLayout::default),
        };
        rsx! { InspectorBody { st } }
    }

    fn mount(rig: fn() -> Rig, select: fn(&Rig) -> Option<Selection>) {
        let mut dom = VirtualDom::new_with_props(Host, P { rig, select });
        dom.rebuild_in_place();
    }

    fn blank() -> Rig {
        blank_rig("blank", uuid::Uuid::nil())
    }

    fn component_where(rig: &Rig, pick: fn(&Component) -> bool) -> Option<Selection> {
        rig.members.values().find_map(|m| {
            m.segments.iter().enumerate().find_map(|(si, s)| {
                s.components
                    .iter()
                    .position(pick)
                    .map(|ci| Selection::Component {
                        member: m.id,
                        segment: si,
                        index: ci,
                    })
            })
        })
    }

    #[test]
    fn every_panel_renders_on_duplo10_and_a_blank_rig() {
        mount(blank, |_| None);
        mount(duplo10, |_| None);
        mount(blank, |rig| {
            rig.bodies
                .values()
                .find(|b| b.kind.is_load())
                .map(|b| Selection::Body(b.id))
        });
        mount(duplo10, |rig| {
            rig.bodies
                .values()
                .find(|b| b.kind.is_gear())
                .map(|b| Selection::Body(b.id))
        });
        mount(duplo10, |rig| {
            Some(Selection::Body(rig.nodes[&rig.root].body.unwrap()))
        });
        mount(duplo10, |rig| Some(Selection::Node(rig.root)));
        mount(duplo10, |rig| {
            rig.nodes
                .values()
                .find(|n| n.kind.is_bearing_capable())
                .map(|n| Selection::Node(n.id))
        });
        mount(duplo10, |rig| {
            rig.nodes
                .values()
                .find(|n| matches!(n.kind, NodeKind::Lug { .. }))
                .map(|n| Selection::Node(n.id))
        });
        mount(duplo10, |rig| {
            rig.members
                .values()
                .find(|m| m.path.len() > 2)
                .map(|m| Selection::Member(m.id))
        });
        mount(duplo10, |rig| {
            component_where(rig, |c| matches!(c.kind, ComponentKind::Shackle { .. }))
        });
        mount(duplo10, |rig| {
            component_where(rig, |c| matches!(c.kind, ComponentKind::Chain { .. }))
        });
        mount(duplo10, |rig| {
            component_where(rig, |c| {
                matches!(
                    c.kind,
                    ComponentKind::RoundSling { .. } | ComponentKind::Strap { .. }
                )
            })
        });
    }
}
