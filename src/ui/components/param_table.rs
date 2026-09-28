//! Keyed parameter table for rig editing.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §8.2.
//! Inputs: rig parameter data and edit events.
//! Outputs: editable parameter rows and where-used details.
//! Must not depend on: store, solver internals.

use dioxus::prelude::*;

use bth_rigging::format::{format_ft_in, format_num};
use bth_rigging::rig::body::{BodyKind, Placement};
use bth_rigging::rig::component::ComponentKind;
use bth_rigging::rig::edit::{ParamEdit, UseSite, apply, use_counts, uses};
use bth_rigging::rig::param::syntax::{ParseError, parse_for};
use bth_rigging::rig::param::{Expr, Param, ParamSource, ParamTable, Quantity};
use bth_rigging::rig::{NodeKind, Rig};

#[component]
pub fn ParamTableView(rig: Rig, on_edit: EventHandler<ParamEdit>) -> Element {
    let counts = use_counts(&rig);
    rsx! {
        section { class: "param-table-wrap",
            table { class: "param-table",
                thead {
                    tr {
                        th { "Name" }
                        th { "Unit" }
                        th { "Value" }
                        th { "=" }
                        th { "−tol" }
                        th { "+tol" }
                        th { "Source" }
                        th { "Used" }
                        th { "Note" }
                    }
                }
                tbody {
                    for param in rig.params.values() {
                        {
                            let param_id = param.id;
                            let param_uses = uses(&rig, param_id);
                            let count = counts.get(&param_id).copied().unwrap_or(0);
                            rsx! {
                                ParamRow {
                                    key: "{param_id}",
                                    rig: rig.clone(),
                                    param: param.clone(),
                                    param_uses,
                                    use_count: count,
                                    on_edit,
                                }
                            }
                        }
                    }
                    AddParamRow { rig: rig.clone(), on_edit }
                }
            }
        }
    }
}

#[component]
fn ParamRow(
    rig: Rig,
    param: Param,
    param_uses: Vec<UseSite>,
    use_count: usize,
    on_edit: EventHandler<ParamEdit>,
) -> Element {
    let initial = value_text(&param, &rig.params);
    let mut value_draft = use_signal(|| initial.clone());
    let mut value_error = use_signal(|| None::<ParseError>);
    let mut edit_message = use_signal(String::new);
    let mut expanded = use_signal(|| false);
    let mut name_draft = use_signal(|| param.name.clone());
    let mut minus_draft = use_signal(|| tolerance_text(param.minus, param.quantity));
    let mut plus_draft = use_signal(|| tolerance_text(param.plus, param.quantity));
    let mut note_draft = use_signal(|| param.note.clone());
    let derived = param.source == ParamSource::Derived;

    rsx! {
        tr {
            key: "{param.id}",
            class: if value_error().is_some() { "param-row param-row--error" } else if derived { "param-row param-row--derived" } else { "param-row" },
            td {
                input {
                    class: "param-input param-name-input",
                    value: "{name_draft}",
                    onchange: move |event| {
                        let name = event.value();
                        name_draft.set(name.clone());
                        on_edit
                            .call(ParamEdit::Rename {
                                id: param.id,
                                name,
                            });
                    },
                }
            }
            td { class: "param-unit", "{param.quantity.unit_label()}" }
            td {
                div { class: "param-value-cell",
                    input {
                        class: if value_error().is_some() { "param-input param-value-input is-invalid" } else { "param-input param-value-input" },
                        value: "{value_draft}",
                        placeholder: "8'-6\"  ·  s12 + 2*lug_gauge_G  ·  12 kip",
                        title: value_error().as_ref().map(|error| error.caret(&value_draft())).unwrap_or_default(),
                        oninput: {
                            let table = rig.params.clone();
                            move |event| {
                                let text = event.value();
                                value_draft.set(text.clone());
                                value_error.set(parse_for(&text, &table, param.quantity).err());
                            }
                        },
                        onchange: move |event| {
                            let text = event.value();
                            let edit = ParamEdit::SetValue {
                                id: param.id,
                                text: text.clone(),
                            };
                            match apply(&rig, &edit) {
                                Ok(_) => {
                                    on_edit.call(edit);
                                    value_error.set(None);
                                    edit_message.set(String::new());
                                }
                                Err(error) => {
                                    value_error.set(error.parse);
                                    edit_message.set(error.message);
                                    value_draft.set(text);
                                }
                            }
                        },
                        onkeydown: {
                            let default_text = initial.clone();
                            move |event| {
                                if event.key() == Key::Escape {
                                    value_draft.set(default_text.clone());
                                    value_error.set(None);
                                }
                            }
                        },
                    }
                    if derived {
                        span { class: "tag-fx", "fx" }
                    }
                }
                if let Some(error) = value_error() {
                    p { class: "param-error-message", "{error.message}" }
                }
                if !edit_message().is_empty() {
                    p { class: "param-error-message", "{edit_message}" }
                }
            }
            td { class: "param-evaluated",
                span { "{format_num(param.nominal)} {param.quantity.unit_label()}" }
                if param.quantity == Quantity::Length {
                    small { "{format_ft_in(param.nominal, 16)}" }
                }
            }
            td {
                input {
                    class: "param-input param-tolerance-input",
                    value: "{minus_draft}",
                    disabled: derived,
                    onchange: move |event| {
                        let text = event.value();
                        minus_draft.set(text.clone());
                        on_edit
                            .call(ParamEdit::SetTol {
                                id: param.id,
                                minus: text,
                                plus: plus_draft(),
                            });
                    },
                }
            }
            td {
                input {
                    class: "param-input param-tolerance-input",
                    value: "{plus_draft}",
                    disabled: derived,
                    onchange: move |event| {
                        let text = event.value();
                        plus_draft.set(text.clone());
                        on_edit
                            .call(ParamEdit::SetTol {
                                id: param.id,
                                minus: minus_draft(),
                                plus: text,
                            });
                    },
                }
            }
            td {
                select {
                    class: "param-input param-source-select",
                    value: "{source_key(param.source)}",
                    onchange: move |event| {
                        if let Some(source) = parse_source(&event.value()) {
                            on_edit
                                .call(ParamEdit::SetSource {
                                    id: param.id,
                                    source,
                                });
                        }
                    },
                    option { value: "Drawing", "Drawing" }
                    option { value: "Catalog", "Catalog" }
                    option { value: "Measured", "Measured" }
                    option { value: "Assumed", "Assumed" }
                    option { value: "Derived", "Derived" }
                }
                if param.source == ParamSource::Assumed {
                    span { class: "tag-assumed", "Assumed" }
                }
            }
            td {
                div { class: "used-cell",
                    button {
                        class: "used-count",
                        title: "Show parameter uses",
                        onclick: move |_| expanded.set(!expanded()),
                        "{use_count}"
                    }
                    button {
                        class: "delete-param-button",
                        title: "Delete parameter",
                        onclick: move |_| on_edit.call(ParamEdit::Delete { id: param.id }),
                        "×"
                    }
                }
            }
            td {
                input {
                    class: "param-input param-note-input",
                    value: "{note_draft}",
                    onchange: move |event| {
                        let note = event.value();
                        note_draft.set(note.clone());
                        on_edit
                            .call(ParamEdit::SetNote {
                                id: param.id,
                                note,
                            });
                    },
                }
            }
        }
        if expanded() {
            tr { class: "param-uses-row",
                td { colspan: "9",
                    for site in param_uses.iter() {
                        for expression in use_expressions(&rig, site) {
                            p { class: "param-use-item",
                                code { "{use_label(&rig, site)}" }
                                span { " = " }
                                code { "{expression.to_text(&rig.params)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddParamRow(rig: Rig, on_edit: EventHandler<ParamEdit>) -> Element {
    let mut name = use_signal(String::new);
    let mut quantity = use_signal(|| Quantity::Length);
    let mut value = use_signal(String::new);
    rsx! {
        tr { class: "param-add-row",
            td {
                input {
                    class: "param-input",
                    placeholder: "new_name",
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                }
            }
            td {
                select {
                    class: "param-input",
                    value: "{quantity_key(quantity())}",
                    onchange: move |event| {
                        if let Some(parsed) = parse_quantity(&event.value()) {
                            quantity.set(parsed)
                        }
                    },
                    option { value: "Length", "ft" }
                    option { value: "Weight", "lb" }
                    option { value: "Angle", "deg" }
                    option { value: "Ratio", "1" }
                    option { value: "Count", "ea" }
                }
            }
            td {
                input {
                    class: "param-input",
                    value: "{value}",
                    placeholder: "8'-6\"  ·  s12 + 2*lug_gauge_G  ·  12 kip",
                    oninput: move |event| value.set(event.value()),
                }
            }
            td { "—" }
            td { "—" }
            td { "—" }
            td { "Assumed" }
            td { "0" }
            td {
                button {
                    class: "btn btn-ghost sm",
                    disabled: name().trim().is_empty() || value().trim().is_empty(),
                    onclick: {
                        let table = rig.params.clone();
                        move |_| {
                            let name_text = name().trim().to_owned();
                            let value_text = value();
                            if parse_for(&value_text, &table, quantity()).is_ok() {
                                on_edit
                                    .call(ParamEdit::Add {
                                        name: name_text,
                                        quantity: quantity(),
                                        value: value_text,
                                    });
                                name.set(String::new());
                                value.set(String::new());
                            }
                        }
                    },
                    "+ Add"
                }
            }
        }
    }
}

fn value_text(param: &Param, table: &ParamTable) -> String {
    param
        .expr
        .as_ref()
        .map(|expression| expression.to_text(table))
        .unwrap_or_else(|| format_num(param.nominal))
}

fn tolerance_text(value: f64, quantity: Quantity) -> String {
    if value == 0.0 {
        String::new()
    } else {
        format!("{} {}", format_num(value), quantity.unit_label())
    }
}

fn source_key(source: ParamSource) -> &'static str {
    match source {
        ParamSource::Drawing => "Drawing",
        ParamSource::Catalog => "Catalog",
        ParamSource::Measured => "Measured",
        ParamSource::Assumed => "Assumed",
        ParamSource::Derived => "Derived",
    }
}

fn parse_source(value: &str) -> Option<ParamSource> {
    match value {
        "Drawing" => Some(ParamSource::Drawing),
        "Catalog" => Some(ParamSource::Catalog),
        "Measured" => Some(ParamSource::Measured),
        "Assumed" => Some(ParamSource::Assumed),
        "Derived" => Some(ParamSource::Derived),
        _ => None,
    }
}

fn quantity_key(quantity: Quantity) -> &'static str {
    match quantity {
        Quantity::Length => "Length",
        Quantity::Weight => "Weight",
        Quantity::Angle => "Angle",
        Quantity::Ratio => "Ratio",
        Quantity::Count => "Count",
    }
}

fn parse_quantity(value: &str) -> Option<Quantity> {
    match value {
        "Length" => Some(Quantity::Length),
        "Weight" => Some(Quantity::Weight),
        "Angle" => Some(Quantity::Angle),
        "Ratio" => Some(Quantity::Ratio),
        "Count" => Some(Quantity::Count),
        _ => None,
    }
}

fn use_label(rig: &Rig, site: &UseSite) -> String {
    match site {
        UseSite::Param { id } => rig
            .params
            .get(id)
            .map(|param| format!("Parameter {}", param.name))
            .unwrap_or_else(|| "Parameter".into()),
        UseSite::NodeCoord { node, axis } => rig
            .nodes
            .get(node)
            .map(|item| format!("Node {}.{axis}", item.label))
            .unwrap_or_else(|| format!("Node {axis}")),
        UseSite::BodyWeight { body } => rig
            .bodies
            .get(body)
            .map(|item| format!("Body {} weight", item.label))
            .unwrap_or_else(|| "Body weight".into()),
        UseSite::BodyCg { body, axis } => rig
            .bodies
            .get(body)
            .map(|item| format!("Body {} CG.{axis}", item.label))
            .unwrap_or_else(|| format!("Body CG.{axis}")),
        UseSite::BodyGeometry { body, field } => rig
            .bodies
            .get(body)
            .map(|item| format!("Body {} {field}", item.label))
            .unwrap_or_else(|| format!("Body {field}")),
        UseSite::Placement { body } => rig
            .bodies
            .get(body)
            .map(|item| format!("Body {} placement", item.label))
            .unwrap_or_else(|| "Body placement".into()),
        UseSite::Component {
            member,
            segment,
            index,
            field,
        }
        | UseSite::ComponentKind {
            member,
            segment,
            index,
            field,
        } => rig
            .members
            .get(member)
            .map(|item| {
                format!(
                    "{} segment {} component {} {field}",
                    item.label,
                    segment + 1,
                    index + 1
                )
            })
            .unwrap_or_else(|| format!("Component {field}")),
        UseSite::Bearing { node, field } => rig
            .nodes
            .get(node)
            .map(|item| format!("Node {} {field}", item.label))
            .unwrap_or_else(|| format!("Bearing {field}")),
    }
}

fn use_expressions<'a>(rig: &'a Rig, site: &UseSite) -> Vec<&'a Expr> {
    match site {
        UseSite::Param { id } => rig
            .params
            .get(id)
            .and_then(|param| param.expr.as_ref())
            .into_iter()
            .collect(),
        UseSite::NodeCoord { node, axis } => rig
            .nodes
            .get(node)
            .map(|item| match axis {
                'x' => &item.local.x,
                'y' => &item.local.y,
                _ => &item.local.z,
            })
            .into_iter()
            .collect(),
        UseSite::BodyWeight { body } => rig
            .bodies
            .get(body)
            .map(|item| &item.weight)
            .into_iter()
            .collect(),
        UseSite::BodyCg { body, axis } => rig
            .bodies
            .get(body)
            .map(|item| match axis {
                'x' => &item.cg.x,
                'y' => &item.cg.y,
                _ => &item.cg.z,
            })
            .into_iter()
            .collect(),
        UseSite::BodyGeometry { body, field } => rig
            .bodies
            .get(body)
            .and_then(|item| match &item.kind {
                BodyKind::SpreaderBar { span, .. } | BodyKind::LiftingBeam { span, .. }
                    if *field == "span" =>
                {
                    Some(span)
                }
                BodyKind::Load {
                    length,
                    width,
                    height,
                } => match *field {
                    "length" => Some(length),
                    "width" => Some(width),
                    "height" => Some(height),
                    _ => None,
                },
                _ => None,
            })
            .into_iter()
            .collect(),
        UseSite::Placement { body } => rig
            .bodies
            .get(body)
            .map(|item| match item.placement.as_ref() {
                Some(Placement::Pinned { at }) => vec![&at.x, &at.y, &at.z],
                Some(Placement::Posed { at, rot }) => {
                    vec![&at.x, &at.y, &at.z, &rot.yaw, &rot.pitch, &rot.roll]
                }
                Some(Placement::Level { rot }) => vec![&rot.yaw, &rot.pitch, &rot.roll],
                _ => Vec::new(),
            })
            .unwrap_or_default(),
        UseSite::Component {
            member,
            segment,
            index,
            field,
        } => rig
            .members
            .get(member)
            .and_then(|item| item.segments.get(*segment))
            .and_then(|segment| segment.components.get(*index))
            .map(|component| match *field {
                "length" => vec![&component.length],
                "weight" => vec![&component.weight],
                "stiffness_lb" => component.stiffness_lb.iter().collect(),
                "adjust.min" => component
                    .adjust
                    .as_ref()
                    .map(|adjust| vec![&adjust.min])
                    .unwrap_or_default(),
                "adjust.max" => component
                    .adjust
                    .as_ref()
                    .map(|adjust| vec![&adjust.max])
                    .unwrap_or_default(),
                _ => component
                    .adjust
                    .as_ref()
                    .map(|adjust| vec![&adjust.setting])
                    .unwrap_or_default(),
            })
            .unwrap_or_default(),
        UseSite::ComponentKind {
            member,
            segment,
            index,
            field,
        } => rig
            .members
            .get(member)
            .and_then(|item| item.segments.get(*segment))
            .and_then(|segment| segment.components.get(*index))
            .and_then(|component| match (&component.kind, *field) {
                (ComponentKind::Strap { width_in }, "width_in") => Some(width_in),
                (ComponentKind::Chain { size_in, .. }, "size_in") => Some(size_in),
                (ComponentKind::WireRope { dia_in }, "dia_in") => Some(dia_in),
                _ => None,
            })
            .into_iter()
            .collect(),
        UseSite::Bearing { node, field } => rig
            .nodes
            .get(node)
            .and_then(|item| match &item.kind {
                NodeKind::Bow { bow_dia, mu } => {
                    Some(if *field == "bow_dia" { bow_dia } else { mu })
                }
                NodeKind::Edge { radius, mu, .. } => {
                    Some(if *field == "radius" { radius } else { mu })
                }
                _ => None,
            })
            .into_iter()
            .collect(),
    }
}
