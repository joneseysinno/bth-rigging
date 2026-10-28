//! Pick document: input layers on the left, PDF-ready Lift Report in the viewport.

mod draft;
mod input;
mod report;
mod state;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{bump_revision, store};
use crate::docs;
use bth_rigging::domain::Pick;
use bth_rigging::format::{format_lbs, format_num};
use bth_rigging::layers::resolve_geometry;
use draft::LayerDraft;
pub use state::forget;
use state::state;

pub const ID: &str = "pick";

pub fn doc_type() -> DocType {
    let mut t = DocType::new(ID, "Pick", icons::LAYERS, report::Report);
    t.toolbar = vec![
        ToolGroup::new("Pick", vec![
            Tool::new("pk.save", "Save pick", icons::DOWNLOAD).primary(),
            Tool::new("pk.add_layer", "Add layer", icons::PLUS),
        ]),
    ];
    t.left_rail = vec![RailItem::panel("pk.input", "Input", icons::SLIDERS, input::Input)];
    t.default_left = Some("pk.input");
    t.status = Some(View(Status));
    t
}

/// Command handler for this document type.
pub fn handle(shell: Shell, pick_id: Uuid, id: &'static str) {
    let Ok(st) = state(pick_id) else { return };
    let mut layers = st.layers;
    match id {
        "pk.save" => save(shell, pick_id),
        "pk.add_layer" => layers.write().push(LayerDraft::new()),
        _ => {}
    }
}

fn save(shell: Shell, pick_id: Uuid) {
    let Ok(st) = state(pick_id) else { return };
    let mut status = st.status;
    let mut layers = st.layers;
    let weight: f64 = match (st.weight_lbs)().trim().parse() {
        Ok(w) if w >= 0.0 => w,
        _ => {
            status.set("Enter a valid payload weight on the final layer.".into());
            return;
        }
    };
    let mut built = Vec::new();
    for (i, draft) in layers().iter().enumerate() {
        match draft.to_layer(pick_id, i as u32) {
            Some(l) => built.push(l),
            None => {
                status.set(format!("Layer {} has invalid inputs.", i + 1));
                return;
            }
        }
    }
    // Persist calculated angles so the manual fallback stays current.
    let calc_geoms = resolve_geometry(&built, &(st.catalog)());
    for (l, g) in built.iter_mut().zip(calc_geoms.iter()) {
        if let (true, Some(a)) = (g.is_calculated(), g.angle_deg) {
            l.angle_deg = (a * 100.0).round() / 100.0;
        }
    }
    let pick = Pick {
        id: pick_id,
        project_id: st.project_id,
        name: (st.name)(),
        weight_lbs: weight,
    };
    let Some(s) = store() else {
        status.set("Database not available.".into());
        return;
    };
    match s.save_pick(&pick, &built) {
        Ok(()) => {
            {
                let mut drafts = layers.write();
                for (d, l) in drafts.iter_mut().zip(built.iter()) {
                    d.angle_deg = format_num(l.angle_deg);
                }
            }
            status.set("Pick saved.".into());
            docs::rename(shell, &pick_id.to_string(), &pick.name);
            bump_revision();
        }
        Err(e) => status.set(format!("Save failed: {e}")),
    }
}

#[allow(non_snake_case)]
fn Status() -> Element {
    let Some(pick_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(pick_id) else {
        return rsx! {};
    };
    let calc = st.derive(pick_id);
    let Some(result) = calc.pick_result else {
        return rsx! { span { "Check layer inputs" } };
    };
    let worst = result
        .layers
        .iter()
        .map(|l| l.status())
        .max_by_key(|(_, kind)| match *kind {
            "over" => 2,
            "warn" => 1,
            _ => 0,
        })
        .unwrap_or(("OK", "ok"));
    rsx! {
        span { "hook {format_lbs(result.hook_load_lbs)} lb" }
        span { "rigging {format_lbs(result.rigging_weight_lbs)} lb" }
        span { class: "stamp {worst.1}", "{worst.0}" }
        if !(st.status)().is_empty() {
            span { "{st.status}" }
        }
    }
}
