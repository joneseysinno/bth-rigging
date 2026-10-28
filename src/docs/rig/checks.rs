//! Rig checks panel: validation, evaluation and solve errors, residuals and
//! rated items to review.

use dioxus::prelude::*;

use super::state::state;
use super::status_class;
use crate::docs;
use bth_rigging::checks::Status;
use bth_rigging::format::format_lbs;
use bth_rigging::rig::solve::rate::Rated;
use bth_rigging::rig::{ResidualKind, RigTree};

#[allow(non_snake_case)]
pub fn Checks() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    let view = st.view();
    let validation = &view.snapshot.validation;
    let eval = &view.snapshot.eval;
    let solve_errors = view.solve_errors();
    let tree = RigTree::build(&view.rig);
    let rated = view.snapshot.rated.clone().unwrap_or_default();
    let failures: Vec<&Rated> = rated
        .iter()
        .filter(|item| item.check.status != Status::Ok)
        .collect();
    let all_clear = validation.errors.is_empty()
        && validation.warnings.is_empty()
        && eval.is_ok()
        && solve_errors.is_none()
        && failures.is_empty();

    rsx! {
        section { class: "results-panel app-panel-pad",
            if all_clear {
                p { class: "muted", "No validation messages or rated items to review." }
            }
            if !validation.errors.is_empty() || !validation.warnings.is_empty() {
                section { class: "results-section validation-list",
                    h3 { "Validation" }
                    for error in validation.errors.iter() {
                        p { class: "validation-error", "{error.message}" }
                    }
                    for warning in validation.warnings.iter() {
                        p { class: "validation-warning", "{warning.message}" }
                    }
                }
            }
            if let Err(errors) = eval {
                div { class: "results-error",
                    h3 { "Evaluation failed" }
                    for error in errors {
                        p { "{error.message}" }
                    }
                }
            }
            if let Some(errors) = &solve_errors {
                div { class: "results-error",
                    h3 { "Solve failed" }
                    for error in errors {
                        p { "{error.message}" }
                    }
                }
            }
            if let Ok(eval) = eval {
                section { class: "results-section residual-list",
                    h3 { "Residuals" }
                    if eval.residuals.is_empty() {
                        p { class: "muted", "No residuals." }
                    }
                    for residual in &eval.residuals {
                        p { class: "residual-item",
                            strong { "{residual_label(residual.kind)}" }
                            span { "{residual.message}" }
                        }
                    }
                }
            }
            if !failures.is_empty() {
                section { class: "results-section rating-list",
                    h3 { "Rated items to review" }
                    for (index, item) in failures.into_iter().enumerate() {
                        article { class: "rated-item", key: "{index}",
                            div { class: "rated-item-heading",
                                span {
                                    span { class: "rated-item-tags",
                                        for tag in tree.tags_for(&view.rig, &item.at) {
                                            span { class: "tag-pill", "{tag}" }
                                        }
                                    }
                                    strong { "{item.owner}" }
                                }
                                span { class: status_class(item.check.status), "{item.check.status.label()}" }
                            }
                            p {
                                "{item.check.item}: {format_lbs(item.check.demand_lbs)} lb / {format_lbs(item.check.capacity_lbs)} lb"
                            }
                            for note in &item.check.notes {
                                p { class: "muted", "{note}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn residual_label(kind: ResidualKind) -> &'static str {
    match kind {
        ResidualKind::Short => "Short",
        ResidualKind::Slack => "Slack",
        ResidualKind::UnequalDrop => "Unequal drop",
        ResidualKind::BearingSplitAssumed => "Bearing split assumed",
        ResidualKind::Underconstrained => "Underconstrained",
        ResidualKind::CgOffset => "CG offset",
    }
}
