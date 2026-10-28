//! Per-pick editor state, shared by the input panel, report and status readout.

use dioxus::prelude::*;
use uuid::Uuid;

use super::draft::LayerDraft;
use crate::app::store;
use crate::docs::{Registry, root_signal};
use crate::ui::components::DiagramLayer;
use bth_rigging::domain::{SavedSpreader, SlingLayer};
use bth_rigging::format::format_num;
use bth_rigging::layers::{LayerGeometry, PickResult, calculate_pick, resolve_geometry};

#[derive(Clone, Copy)]
pub struct PickState {
    pub project_id: Uuid,
    pub project_name: Signal<String>,
    pub name: Signal<String>,
    pub weight_lbs: Signal<String>,
    pub layers: Signal<Vec<LayerDraft>>,
    pub catalog: Signal<Vec<SavedSpreader>>,
    pub status: Signal<String>,
}

/// Results derived from the current inputs.
pub struct PickCalc {
    pub weight: f64,
    pub built_layers: Vec<SlingLayer>,
    /// Geometry (calculated angles) per layer; only when every layer parses.
    pub geoms: Vec<LayerGeometry>,
    pub diagram_layers: Vec<DiagramLayer>,
    pub pick_result: Option<PickResult>,
    /// Load passing through the top of each layer (payload + rigging below that point).
    pub level_loads: Vec<f64>,
}

thread_local! {
    static PICKS: Registry<PickState> = Registry::new();
}

pub fn state(pick_id: Uuid) -> Result<PickState, String> {
    PICKS.with(|picks| picks.get_or_load(pick_id, || load(pick_id)))
}

pub fn forget(pick_id: Uuid) {
    PICKS.with(|picks| picks.forget(pick_id));
}

fn load(pick_id: Uuid) -> Result<PickState, String> {
    let s = store().ok_or("Database not available.")?;
    let (pick, saved_layers) = s
        .load_pick(pick_id)
        .map_err(|e| e.to_string())?
        .ok_or("Pick not found.")?;
    let project_name = s
        .load_project(pick.project_id)
        .ok()
        .flatten()
        .map(|p| p.name)
        .unwrap_or_else(|| "Project".into());
    let layers = if saved_layers.is_empty() {
        vec![LayerDraft::new()]
    } else {
        saved_layers.iter().map(LayerDraft::from_layer).collect()
    };
    Ok(PickState {
        project_id: pick.project_id,
        project_name: root_signal(project_name),
        name: root_signal(pick.name),
        weight_lbs: root_signal(format_num(pick.weight_lbs)),
        layers: root_signal(layers),
        catalog: root_signal(s.list_spreaders().unwrap_or_default()),
        status: root_signal(String::new()),
    })
}

impl PickState {
    pub fn derive(&self, pick_id: Uuid) -> PickCalc {
        let layers = (self.layers)();
        let catalog = (self.catalog)();
        let weight: f64 = (self.weight_lbs)().trim().parse().unwrap_or(f64::NAN);

        let built_layers: Vec<SlingLayer> = layers
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.to_layer(pick_id, i as u32))
            .collect();
        let all_layers_valid = built_layers.len() == layers.len();
        let geoms: Vec<LayerGeometry> = if all_layers_valid {
            resolve_geometry(&built_layers, &catalog)
        } else {
            Vec::new()
        };
        let diagram_layers: Vec<DiagramLayer> = if all_layers_valid {
            built_layers
                .iter()
                .zip(geoms.iter())
                .map(|(l, g)| DiagramLayer::from_sling_geometry(l, g))
                .collect()
        } else {
            layers
                .iter()
                .enumerate()
                .filter_map(|(i, d)| d.to_diagram(i as u32))
                .collect()
        };
        let pick_result = if weight.is_finite() && weight >= 0.0 && all_layers_valid {
            calculate_pick(weight, &built_layers, &catalog)
        } else {
            None
        };
        let level_loads: Vec<f64> = pick_result
            .as_ref()
            .map(|pr| pr.layers.iter().map(|l| l.load_at_top_lbs).collect())
            .unwrap_or_default();

        PickCalc {
            weight,
            built_layers,
            geoms,
            diagram_layers,
            pick_result,
            level_loads,
        }
    }
}
