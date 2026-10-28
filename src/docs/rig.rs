//! Rig document: the 3D rig tree in the viewport, live results on the right,
//! checks on the left and the parameter table in the bottom panel.

mod checks;
mod model;
mod params;
mod results;
mod state;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{bump_revision, store};
use crate::docs;
use bth_rigging::checks::Status;
use bth_rigging::format::format_lbs;
pub use state::forget;
use state::state;

pub const ID: &str = "rig";
const PARAMS_PANEL: &str = "rig.params";

pub fn doc_type() -> DocType {
    let mut t = DocType::new(ID, "Rig", icons::TREE, model::Model);
    t.toolbar = vec![
        ToolGroup::new("Rig", vec![Tool::new("rig.save", "Save rig", icons::DOWNLOAD).primary()]),
        ToolGroup::new("Edit", vec![
            Tool::new("rig.undo", "Undo", icons::UNDO),
            Tool::new("rig.redo", "Redo", icons::REDO),
        ]),
    ];
    t.left_rail = vec![RailItem::panel("rig.checks", "Checks", icons::INFO, checks::Checks)];
    t.right_rail = vec![RailItem::panel("rig.results", "Live results", icons::TARGET, results::Results)];
    t.bottom_rail = vec![RailItem::panel(PARAMS_PANEL, "Parameters", icons::TABLE, params::Params)];
    t.default_right = Some("rig.results");
    t.status = Some(View(StatusReadout));
    t
}

/// Command handler for this document type.
pub fn handle(shell: Shell, rig_id: Uuid, id: &'static str) {
    let Ok(st) = state(rig_id) else { return };
    let mut session = st.session;
    match id {
        "rig.save" => save(shell, rig_id),
        "rig.undo" => {
            session.write().undo();
        }
        "rig.redo" => {
            session.write().redo();
        }
        _ => {}
    }
}

fn save(shell: Shell, rig_id: Uuid) {
    let Ok(st) = state(rig_id) else { return };
    let mut session = st.session;
    let mut status_message = st.status_message;
    let Some(store) = store() else {
        status_message.set("Database not available.".into());
        return;
    };
    let mut current = session.write();
    match store.save_rig(current.rig()) {
        Ok(()) => {
            current.mark_saved();
            let name = current.rig().name.clone();
            drop(current);
            status_message.set("Rig saved.".into());
            docs::rename(shell, &rig_id.to_string(), &name);
            bump_revision();
        }
        Err(error) => status_message.set(error.to_string()),
    }
}

fn status_class(status: Status) -> &'static str {
    match status {
        Status::Ok => "status-tag status-ok",
        Status::Warn => "status-tag status-warn",
        Status::Over => "status-tag status-over",
    }
}

#[allow(non_snake_case)]
fn StatusReadout() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    let session = st.session.read();
    let dirty = session.is_dirty();
    let headline = session.snapshot().headline;
    rsx! {
        span {
            class: if dirty { "dirty-indicator is-dirty" } else { "dirty-indicator" },
            title: if dirty { "Unsaved changes" } else { "Saved" },
        }
        span { if dirty { "unsaved" } else { "saved" } }
        span { "hook {format_lbs(headline.hook_load_lbs)} lb" }
        span { class: status_class(headline.worst_status), "{headline.worst_status.label()}" }
        if !(st.status_message)().is_empty() {
            span { "{st.status_message}" }
        }
    }
}
