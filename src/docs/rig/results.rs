//! Rig live results panel: headline numbers and the member list.

use dioxus::prelude::*;

use super::state::state;
use super::status_class;
use crate::docs;
use bth_rigging::checks::Status;
use bth_rigging::format::{format_lbs, format_num};
use bth_rigging::rig::solve::{MemberForce, rate::Rated};
use bth_rigging::rig::{ItemRef, MemberId, MemberShape, RigTree};

#[allow(non_snake_case)]
pub fn Results() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    let view = st.view();
    let forces = view.forces();
    let tree = RigTree::build(&view.rig);
    let rated = view.snapshot.rated.clone().unwrap_or_default();
    let headline = view.snapshot.headline;
    let headline_is_last_good = view.snapshot.headline_is_last_good;
    let weight_breakdown = view
        .snapshot
        .eval
        .as_ref()
        .ok()
        .map(|evaluation| evaluation.weights.clone());
    let delta = view.delta;
    let selected = st.selected;
    let hovered = st.hovered;

    rsx! {
        section { class: "results-panel app-panel-pad",
            if headline.assumed_count > 0 {
                div { class: "provisional-banner",
                    strong { "Provisional" }
                    span { "{headline.assumed_count} assumed inputs" }
                }
            }
            if headline_is_last_good {
                p { class: "last-good-label", "Last good results" }
            }
            div { class: if headline_is_last_good { "headline-grid is-last-good" } else { "headline-grid" },
                HeadlineValue {
                    label: "Hook load",
                    value: format!("{} lb", format_lbs(headline.hook_load_lbs)),
                    delta: delta.map(|change| change.hook_load_lbs),
                    unit: "lb",
                }
                if let Some(weights) = &weight_breakdown {
                    div { class: "headline-breakdown",
                        span { "Load {format_lbs(weights.load_lbs)} lb" }
                        span { "Gear {format_lbs(weights.gear_lbs)} lb" }
                        span { "Rigging {format_lbs(weights.rigging_lbs)} lb" }
                    }
                }
                HeadlineValue {
                    label: "Governing angle",
                    value: format!("{}°", format_num(headline.governing_angle_deg)),
                    delta: delta.map(|change| change.governing_angle_deg),
                    unit: "°",
                }
                HeadlineValue {
                    label: "Maximum tension",
                    value: format!("{} lb", format_lbs(headline.max_tension_lbs)),
                    delta: delta.map(|change| change.max_tension_lbs),
                    unit: "lb",
                }
                HeadlineValue {
                    label: "Maximum utilization",
                    value: format!("{}%", format_num(headline.max_utilization * 100.0)),
                    delta: delta.map(|change| change.max_utilization * 100.0),
                    unit: "%",
                }
                div { class: "headline-value headline-status",
                    span { class: "field-label", "Worst status" }
                    span { class: status_class(headline.worst_status), "{headline.worst_status.label()}" }
                }
            }
            if !tree.members.is_empty() {
                section { class: "results-section member-tree",
                    h3 { "Members" }
                    for (group_index , group) in tree.groups.iter().enumerate() {
                        div { class: "member-group", key: "group-{group_index}",
                            p { class: "member-group-heading",
                                span { "From {group.label}" }
                                span { "{group.members.len()}" }
                            }
                            for member_id in group.members.iter().copied() {
                                if let Some(member) = tree.get(member_id) {
                                    MemberRow {
                                        key: "{member_id}",
                                        tag: member.tag.clone(),
                                        label: member.label.clone(),
                                        route: member.route(),
                                        hardware: member.hardware.clone(),
                                        is_link: member.shape == MemberShape::Link,
                                        force: forces.get(&member_id).cloned(),
                                        checks: rated
                                            .iter()
                                            .filter(|item| matches!(item.at, ItemRef::Member { id, .. } if id == member_id))
                                            .cloned()
                                            .collect::<Vec<_>>(),
                                        member_id,
                                        selected,
                                        hovered,
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MemberRow(
    member_id: MemberId,
    tag: String,
    label: String,
    route: String,
    hardware: String,
    is_link: bool,
    force: Option<MemberForce>,
    checks: Vec<Rated>,
    selected: Signal<Option<MemberId>>,
    hovered: Signal<Option<MemberId>>,
) -> Element {
    let is_selected = selected() == Some(member_id);
    let hot = is_selected || hovered() == Some(member_id);
    let status = rated_for_member(&checks, member_id);
    let mut class = String::from("member-row");
    if hot {
        class.push_str(" is-hot");
    }
    if is_link {
        class.push_str(" is-link");
    }
    rsx! {
        div {
            class: "{class}",
            onmouseenter: move |_| hovered.set(Some(member_id)),
            onmouseleave: move |_| hovered.set(None),
            onclick: move |_| {
                selected.set(if is_selected { None } else { Some(member_id) });
            },
            div { class: "member-row-main",
                span { class: if hot { "tag-pill is-hot" } else { "tag-pill" }, "{tag}" }
                span { class: "member-name", title: "{label}", "{label}" }
                span { class: "member-numbers",
                    if let Some(force) = &force {
                        span { "{format_lbs(force.tension_lbs)} lb" }
                        span {
                            "{format_num(force.angle_deg.iter().copied().fold(0.0, f64::max))}°"
                        }
                        if !force.taut && !force.is_link {
                            span { class: "muted", "Slack" }
                        }
                    } else {
                        span { class: "muted", "—" }
                    }
                    span { class: status_class(status), "{status.label()}" }
                }
            }
            p { class: "member-route", "{route}" }
            if !hardware.is_empty() {
                p { class: "member-hardware", "{hardware}" }
            }
            if is_selected && !checks.is_empty() {
                ul { class: "member-checks",
                    for (index , item) in checks.iter().enumerate() {
                        li { key: "{index}",
                            span { "{item.check.item}" }
                            span {
                                "{format_lbs(item.check.demand_lbs)} / {format_lbs(item.check.capacity_lbs)} lb "
                                span { class: status_class(item.check.status), "{item.check.status.label()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HeadlineValue(
    label: &'static str,
    value: String,
    delta: Option<f64>,
    unit: &'static str,
) -> Element {
    rsx! {
        div { class: "headline-value",
            span { class: "field-label", "{label}" }
            strong { "{value}" }
            if let Some(change) = delta {
                span { class: if change > 0.0 { "delta-chip delta-up" } else if change < 0.0 { "delta-chip delta-down" } else { "delta-chip" },
                    "{format_delta(change, unit)}"
                }
            }
        }
    }
}

fn format_delta(value: f64, unit: &str) -> String {
    let sign = if value > 0.0 { "+" } else { "−" };
    let number = if unit == "lb" {
        format_lbs(value.abs())
    } else {
        format_num(value.abs())
    };
    format!("{sign}{number} {unit}")
}

fn rated_for_member(rated: &[Rated], member: MemberId) -> Status {
    rated
        .iter()
        .filter(|item| matches!(item.at, ItemRef::Member { id, .. } if id == member))
        .map(|item| item.check.status)
        .max()
        .unwrap_or(Status::Ok)
}
