//! Mat analysis document: inputs on the left, Mat Bearing Report in the viewport.

mod input;
mod report;
mod state;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{bump_revision, store};
use crate::docs;
use bth_rigging::domain::MatAnalysis;
use bth_rigging::format::{format_lbs, format_num};
pub use state::forget;
use state::state;

pub const ID: &str = "mat";

pub fn doc_type() -> DocType {
    let mut t = DocType::new(ID, "Mat analysis", icons::SCALE, report::Report);
    t.toolbar = vec![ToolGroup::new("Analysis", vec![
        Tool::new("mat.save", "Save analysis", icons::DOWNLOAD).primary(),
    ])];
    t.left_rail = vec![RailItem::panel("mat.input", "Input", icons::SLIDERS, input::Input)];
    t.default_left = Some("mat.input");
    t.status = Some(View(Status));
    t
}

/// Command handler for this document type.
pub fn handle(shell: Shell, analysis_id: Uuid, id: &'static str) {
    if id == "mat.save" {
        save(shell, analysis_id);
    }
}

fn save(shell: Shell, analysis_id: Uuid) {
    let Ok(st) = state(analysis_id) else { return };
    let mut status = st.status;
    let Ok(load_v) = (st.outrigger_load)().trim().parse::<f64>() else {
        status.set("Enter a valid outrigger load.".into());
        return;
    };
    let Ok(pad_l_v) = (st.pad_length)().trim().parse::<f64>() else {
        status.set("Enter a valid pad length.".into());
        return;
    };
    let Ok(pad_w_v) = (st.pad_width)().trim().parse::<f64>() else {
        status.set("Enter a valid pad width.".into());
        return;
    };
    let Ok(allow_v) = (st.allowable)().trim().parse::<f64>() else {
        status.set("Enter a valid allowable pressure.".into());
        return;
    };
    let mat_id = (st.mat_id)();
    let mid = if mat_id.trim().is_empty() {
        Uuid::nil()
    } else {
        match Uuid::parse_str(mat_id.trim()) {
            Ok(u) => u,
            Err(_) => {
                status.set("Select a saved mat.".into());
                return;
            }
        }
    };
    let analysis = MatAnalysis {
        id: analysis_id,
        project_id: st.project_id,
        name: (st.name)(),
        mat_id: mid,
        outrigger_load_lbs: load_v,
        pad_length_in: pad_l_v,
        pad_width_in: pad_w_v,
        allowable_psf: allow_v,
        spread_angle_deg: 45.0,
    };
    let Some(s) = store() else {
        status.set("Database not available.".into());
        return;
    };
    match s.save_mat_analysis(&analysis) {
        Ok(()) => {
            status.set("Analysis saved.".into());
            docs::rename(shell, &analysis_id.to_string(), &analysis.name);
            bump_revision();
        }
        Err(e) => status.set(format!("Save failed: {e}")),
    }
}

#[allow(non_snake_case)]
fn Status() -> Element {
    let Some(analysis_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(analysis_id) else {
        return rsx! {};
    };
    let Some(r) = st.derive().result else {
        return rsx! { span { "Select a mat and enter valid inputs" } };
    };
    let over = r.soil_overloaded || r.mat_overloaded;
    rsx! {
        span { "qt {format_lbs(r.qt_psf)} / {format_lbs(r.allowable_psf)} psf" }
        span { "soil {format_num(r.soil_utilization_pct)}%" }
        span { "mat {format_num(r.mat_utilization_pct)}%" }
        span { class: if over { "stamp over" } else { "stamp ok" }, if over { "OVER" } else { "OK" } }
        if !(st.status)().is_empty() {
            span { "{st.status}" }
        }
    }
}
