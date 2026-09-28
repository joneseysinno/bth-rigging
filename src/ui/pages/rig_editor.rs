//! Rig editor: parameter table and live numeric results.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §8.
//! Inputs: a persisted rig, parameter edits, and explicit save requests.
//! Outputs: keyed parameter editor and live evaluation/solve/rating details.
//! Must not depend on: layer calculation or solver internals beyond snapshots.

use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{AppCtx, Route};
use crate::ui::components::ParamTableView;
use bth_rigging::checks::Status;
use bth_rigging::format::{format_lbs, format_num};
use bth_rigging::rig::edit::ParamEdit;
use bth_rigging::rig::session::{HeadlineDelta, RigSession};
use bth_rigging::rig::solve::{MemberForce, rate::Rated};
use bth_rigging::rig::{EvalRig, ItemRef, MemberId, RigError, ValidationReport};

#[component]
pub fn RigEditorPage(project_id: Uuid, rig_id: Uuid) -> Element {
    let ctx = use_context::<AppCtx>();
    let navigator = use_navigator();
    let mut session = use_signal(|| None::<RigSession>);
    let mut status_message = use_signal(String::new);
    let mut load_error = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);
    let mut show_leave_confirm = use_signal(|| false);
    let poller_active = use_signal(|| false);

    {
        let ctx = ctx.clone();
        use_effect(move || {
            if loaded() {
                return;
            }
            if let Some(ref store) = ctx.store {
                match store.load_rig(rig_id) {
                    Ok(Some(rig)) if rig.project_id == project_id => {
                        session.set(Some(RigSession::new(rig)));
                        loaded.set(true);
                    }
                    Ok(Some(_)) => {
                        load_error.set(Some("Rig does not belong to this project.".into()));
                        loaded.set(true);
                    }
                    Ok(None) => {
                        load_error.set(Some("Rig not found.".into()));
                        loaded.set(true);
                    }
                    Err(error) => {
                        load_error.set(Some(error.to_string()));
                        loaded.set(true);
                    }
                }
            } else {
                load_error.set(Some("Database not available.".into()));
                loaded.set(true);
            }
        });
    }

    {
        let mut poller_active = poller_active;
        use_effect(move || {
            if session.read().as_ref().is_some_and(RigSession::is_solving) && !poller_active() {
                poller_active.set(true);
                let mut session = session;
                let mut poller_active = poller_active;
                spawn(async move {
                    loop {
                        Delay::new(Duration::from_millis(16)).await;
                        let finished = session.write().as_mut().is_none_or(|current| {
                            current.poll();
                            !current.is_solving()
                        });
                        if finished {
                            break;
                        }
                    }
                    poller_active.set(false);
                });
            }
        });
    }

    let view = session.read().as_ref().map(|current| {
        (
            current.rig().clone(),
            current.snapshot().clone(),
            current.delta(),
            current.is_dirty(),
            current.is_solving(),
        )
    });
    let Some((rig, snapshot, delta, dirty, solving)) = view else {
        if let Some(error) = load_error() {
            return rsx! {
                div { class: "page-shell",
                    main { class: "page-main",
                        Link {
                            to: Route::Project { id: project_id },
                            class: "back-link",
                            "← Project"
                        }
                        p { class: "status-line", "{error}" }
                    }
                }
            };
        }
        return rsx! {
            div { class: "page-shell",
                main { class: "page-main",
                    p { class: "status-line", "Loading rig…" }
                }
            }
        };
    };

    let title = rig.name.clone();
    let solved_members = snapshot
        .solved
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|solved| {
            rig.members
                .iter()
                .filter_map(|(id, member)| {
                    solved
                        .members
                        .get(id)
                        .map(|force| (member.label.clone(), *id, force.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    let solve_errors = snapshot
        .solved
        .as_ref()
        .and_then(|result| result.as_ref().err().cloned());
    let weight_breakdown = snapshot
        .eval
        .as_ref()
        .ok()
        .map(|evaluation| evaluation.weights.clone());
    let on_edit = move |edit: ParamEdit| {
        let result = session
            .write()
            .as_mut()
            .map(|current| current.apply_async(edit));
        match result {
            Some(Ok(())) => status_message.set(String::new()),
            Some(Err(error)) => status_message.set(error.message),
            None => status_message.set("Rig session is not available.".into()),
        }
    };

    rsx! {
        div { class: "page-shell rig-editor-shell",
            header { class: "page-header compact",
                div { class: "page-header-inner rig-editor-header",
                    button {
                        class: "back-link rig-back-button",
                        onclick: move |_| {
                            if session.read().as_ref().is_some_and(RigSession::is_dirty) {
                                show_leave_confirm.set(true);
                            } else {
                                navigator.push(Route::Project { id: project_id });
                            }
                        },
                        "← Project"
                    }
                    div { class: "rig-title-block",
                        h1 { class: "brand-title", "{title}" }
                        span {
                            class: if dirty { "dirty-indicator is-dirty" } else { "dirty-indicator" },
                            title: if dirty { "Unsaved changes" } else { "Saved" },
                        }
                    }
                    div { class: "toolbar rig-toolbar",
                        button {
                            class: "btn btn-ghost",
                            disabled: !dirty,
                            onclick: {
                                let ctx = ctx.clone();
                                move |_| {
                                    if let Some(ref store) = ctx.store {
                                        if let Some(current) = session.write().as_mut() {
                                            match store.save_rig(current.rig()) {
                                                Ok(()) => {
                                                    current.mark_saved();
                                                    status_message.set("Rig saved.".into());
                                                }
                                                Err(error) => status_message.set(error.to_string()),
                                            }
                                        }
                                    }
                                }
                            },
                            "Save"
                        }
                        button {
                            class: "btn btn-ghost",
                            onclick: move |_| {
                                if let Some(current) = session.write().as_mut() {
                                    current.undo();
                                }
                            },
                            "Undo"
                        }
                        button {
                            class: "btn btn-ghost",
                            onclick: move |_| {
                                if let Some(current) = session.write().as_mut() {
                                    current.redo();
                                }
                            },
                            "Redo"
                        }
                    }
                }
            }
            main { class: "editor-grid rig-editor-grid",
                section { class: "panel param-panel",
                    div { class: "layer-toolbar",
                        h2 { class: "panel-title", "Parameters" }
                        span { class: "muted", "{rig.params.len()} inputs" }
                    }
                    ParamTableView { rig: rig.clone(), on_edit }
                    if !status_message().is_empty() {
                        p { class: "status-line", "{status_message}" }
                    }
                }
                ResultsPanel {
                    validation: snapshot.validation,
                    eval: snapshot.eval,
                    solve_errors,
                    solved_members,
                    rated: snapshot.rated.unwrap_or_default(),
                    headline: snapshot.headline,
                    headline_is_last_good: snapshot.headline_is_last_good,
                    weight_breakdown,
                    delta,
                    solving,
                }
            }
            if show_leave_confirm() {
                div { class: "modal-backdrop",
                    section { class: "leave-confirm panel",
                        h2 { class: "panel-title", "Unsaved rig changes" }
                        p { "Leave without saving these parameter edits?" }
                        div { class: "toolbar",
                            button {
                                class: "btn btn-ghost",
                                onclick: move |_| show_leave_confirm.set(false),
                                "Stay"
                            }
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    navigator.push(Route::Project { id: project_id });
                                },
                                "Leave"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ResultsPanel(
    validation: ValidationReport,
    eval: Result<EvalRig, Vec<RigError>>,
    solve_errors: Option<Vec<RigError>>,
    solved_members: Vec<(String, MemberId, MemberForce)>,
    rated: Vec<Rated>,
    headline: bth_rigging::rig::session::Headline,
    headline_is_last_good: bool,
    weight_breakdown: Option<bth_rigging::rig::RigWeights>,
    delta: Option<HeadlineDelta>,
    solving: bool,
) -> Element {
    rsx! {
        section { class: "results-panel",
            div { class: "results-heading",
                h2 { class: "panel-title", "Live results" }
                if solving {
                    span { class: "solving-indicator", "Solving…" }
                }
            }
            if headline.assumed_count > 0 {
                div { class: "provisional-banner",
                    strong { "Provisional" }
                    span { "{headline.assumed_count} assumed inputs" }
                }
            }
            if !validation.errors.is_empty() || !validation.warnings.is_empty() {
                section { class: "results-section validation-list",
                    h3 { "Validation" }
                    for error in validation.errors.iter() {
                        p { class: "validation-error", "{error}" }
                    }
                    for warning in validation.warnings.iter() {
                        p { class: "validation-warning", "{warning}" }
                    }
                }
            }
            if let Err(errors) = &eval {
                div { class: "results-error",
                    h3 { "Evaluation failed" }
                    for error in errors {
                        p { "{error}" }
                    }
                }
            }
            if let Some(errors) = &solve_errors {
                div { class: "results-error",
                    h3 { "Solve failed" }
                    for error in errors {
                        p { "{error}" }
                    }
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
            if !solved_members.is_empty() {
                section { class: "results-section",
                    h3 { "Members" }
                    div { class: "result-table-wrap",
                        table { class: "result-table",
                            thead {
                                tr {
                                    th { "Member" }
                                    th { "Tension" }
                                    th { "Angle" }
                                    th { "State" }
                                    th { "Status" }
                                }
                            }
                            tbody {
                                for (label , member_id , force) in &solved_members {
                                    {
                                        let member_status = rated_for_member(&rated, *member_id);
                                        let angle = force.angle_deg.iter().copied().fold(0.0, f64::max);
                                        rsx! {
                                            tr { key: "{member_id}",
                                                td { "{label}" }
                                                td { "{format_lbs(force.tension_lbs)} lb" }
                                                td { "{format_num(angle)}°" }
                                                td {
                                                    if force.taut {
                                                        "Taut"
                                                    } else {
                                                        "Slack"
                                                    }
                                                }
                                                td {
                                                    span { class: status_class(member_status), "{member_status.label()}" }
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
            if let Ok(eval) = &eval {
                section { class: "results-section residual-list",
                    h3 { "Residuals" }
                    if eval.residuals.is_empty() {
                        p { class: "muted", "No residuals." }
                    }
                    for residual in &eval.residuals {
                        p { class: "residual-item",
                            strong { "{residual.kind:?}" }
                            span { "{residual.message}" }
                        }
                    }
                }
            }
            {
                let failures: Vec<&Rated> = rated
                    .iter()
                    .filter(|item| item.check.status != Status::Ok)
                    .collect();
                if !failures.is_empty() {
                    rsx! {
                        section { class: "results-section rating-list",
                            h3 { "Rated items to review" }
                            for item in failures {
                                article { class: "rated-item", key: "{item.owner}-{item.check.item}",
                                    div { class: "rated-item-heading",
                                        strong { "{item.owner}" }
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
                } else {
                    rsx! {}
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

fn status_class(status: Status) -> &'static str {
    match status {
        Status::Ok => "status-tag status-ok",
        Status::Warn => "status-tag status-warn",
        Status::Over => "status-tag status-over",
    }
}

fn rated_for_member(rated: &[Rated], member: bth_rigging::rig::MemberId) -> Status {
    rated
        .iter()
        .filter(|item| matches!(item.at, ItemRef::Member { id, .. } if id == member))
        .map(|item| item.check.status)
        .max()
        .unwrap_or(Status::Ok)
}

struct Delay {
    duration: Duration,
    complete: Arc<AtomicBool>,
    started: bool,
}

impl Delay {
    fn new(duration: Duration) -> Self {
        Self {
            duration,
            complete: Arc::new(AtomicBool::new(false)),
            started: false,
        }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.complete.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        if !self.started {
            self.started = true;
            let complete = Arc::clone(&self.complete);
            let duration = self.duration;
            let waker = context.waker().clone();
            thread::spawn(move || {
                thread::sleep(duration);
                complete.store(true, Ordering::Release);
                waker.wake();
            });
        }
        Poll::Pending
    }
}
