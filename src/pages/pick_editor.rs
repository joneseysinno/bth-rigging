//! Pick editor: input pane + PDF-ready lift report.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::Route;
use crate::app_state::{AppCtx, format_lbs, format_num};
use crate::calc::calculate_pick;
use crate::catalog::POLYESTER_ROUNDSLINGS;
use crate::diagram::{DiagramLayer, RiggingDiagram};
use crate::geometry::{LayerGeometry, resolve_geometry};
use crate::hardware::SHACKLES;
use crate::models::{Hitch, Pick, Project, SavedSpreader, SlingLayer};

#[derive(Clone, PartialEq)]
struct LayerDraft {
    key: Uuid,
    size: u8,
    hitch: Hitch,
    angle_deg: String,
    sling_count: String,
    /// Sling length (ft) for self-weight and angle geometry.
    sling_length_ft: String,
    /// Adjacent pick-point spacing (ft).
    pick_spacing_ft: String,
    /// Pick-point width (ft) for rectangular patterns.
    pick_width_ft: String,
    /// Spreader span as rigged (ft).
    spreader_span_ft: String,
    apex_shackle: String,
    leg_shackle: String,
    /// Selected saved spreader id (empty = none).
    spreader_id: String,
    /// Legacy WLL if loaded without spreader_id.
    legacy_spreader_wll: Option<u32>,
    tare_lbs: String,
    /// Inline create form visibility.
    show_new_spreader: bool,
    new_mfr: String,
    new_model: String,
    new_wll: String,
    new_weight: String,
    new_span: String,
}

impl LayerDraft {
    fn new() -> Self {
        Self {
            key: Uuid::new_v4(),
            size: 2,
            hitch: Hitch::Vertical,
            angle_deg: "60".into(),
            sling_count: "2".into(),
            sling_length_ft: "10".into(),
            pick_spacing_ft: String::new(),
            pick_width_ft: String::new(),
            spreader_span_ft: String::new(),
            apex_shackle: String::new(),
            leg_shackle: String::new(),
            spreader_id: String::new(),
            legacy_spreader_wll: None,
            tare_lbs: "0".into(),
            show_new_spreader: false,
            new_mfr: String::new(),
            new_model: String::new(),
            new_wll: String::new(),
            new_weight: String::new(),
            new_span: String::new(),
        }
    }

    fn from_layer(layer: &SlingLayer) -> Self {
        let mut d = Self::new();
        d.size = layer.size;
        d.hitch = layer.hitch;
        d.angle_deg = format_num(layer.angle_deg);
        d.sling_count = layer.sling_count.to_string();
        d.sling_length_ft = format_num(layer.sling_length_ft);
        d.pick_spacing_ft = layer.pick_spacing_ft.map(format_num).unwrap_or_default();
        d.pick_width_ft = layer.pick_width_ft.map(format_num).unwrap_or_default();
        d.spreader_span_ft = layer.spreader_span_ft.map(format_num).unwrap_or_default();
        d.apex_shackle = layer.apex_shackle.clone().unwrap_or_default();
        d.leg_shackle = layer.leg_shackle.clone().unwrap_or_default();
        d.spreader_id = layer
            .spreader_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        d.legacy_spreader_wll = if layer.spreader_id.is_none() {
            layer.spreader_wll_lbs
        } else {
            None
        };
        d.tare_lbs = format_num(layer.tare_lbs);
        d
    }

    fn to_layer(&self, pick_id: Uuid, layer_index: u32) -> Option<SlingLayer> {
        let angle_deg = self.angle_deg.trim().parse::<f64>().ok()?;
        let sling_count = self.sling_count.trim().parse::<u32>().ok()?;
        if sling_count == 0 {
            return None;
        }
        let tare_lbs = self.tare_lbs.trim().parse::<f64>().unwrap_or(0.0).max(0.0);
        let sling_length_ft = self
            .sling_length_ft
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .max(0.0);
        let apex = opt_str(&self.apex_shackle);
        let leg = opt_str(&self.leg_shackle);
        let pick_spacing_ft = opt_pos(&self.pick_spacing_ft);
        let pick_width_ft = opt_pos(&self.pick_width_ft);
        let spreader_span_ft = opt_pos(&self.spreader_span_ft);
        let spreader_id = if self.spreader_id.trim().is_empty() {
            None
        } else {
            Some(Uuid::parse_str(self.spreader_id.trim()).ok()?)
        };
        let spreader_wll_lbs = if spreader_id.is_some() {
            None
        } else {
            self.legacy_spreader_wll
        };
        Some(SlingLayer {
            pick_id,
            layer_index,
            size: self.size,
            hitch: self.hitch,
            angle_deg,
            sling_count,
            sling_length_ft,
            pick_spacing_ft,
            pick_width_ft,
            spreader_span_ft,
            apex_shackle: apex,
            leg_shackle: leg,
            spreader_id,
            spreader_wll_lbs,
            tare_lbs,
        })
    }

    fn to_diagram(&self, index: u32) -> Option<DiagramLayer> {
        let layer = self.to_layer(Uuid::nil(), index)?;
        Some(DiagramLayer::from_sling(&layer))
    }

    fn has_spreader(&self) -> bool {
        !self.spreader_id.trim().is_empty() || self.legacy_spreader_wll.is_some()
    }
}

/// Parse a positive number; blank / zero / invalid → `None`.
fn opt_pos(s: &str) -> Option<f64> {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

#[component]
pub fn PickEditor(project_id: Uuid, pick_id: Uuid) -> Element {
    let ctx = use_context::<AppCtx>();
    let mut project = use_signal(|| None::<Project>);
    let mut pick_name = use_signal(|| "New Pick".to_string());
    let mut weight_lbs = use_signal(|| "10000".to_string());
    let mut layers = use_signal(|| vec![LayerDraft::new()]);
    let mut catalog = use_signal(Vec::<SavedSpreader>::new);
    let mut status = use_signal(|| String::new());
    let mut loaded = use_signal(|| false);
    let navigator = use_navigator();

    {
        let ctx = ctx.clone();
        use_effect(move || {
            if loaded() {
                return;
            }
            if let Some(ref s) = ctx.store {
                project.set(s.load_project(project_id).ok().flatten());
                catalog.set(s.list_spreaders().unwrap_or_default());
                if let Ok(Some((pick, saved_layers))) = s.load_pick(pick_id) {
                    pick_name.set(pick.name);
                    weight_lbs.set(format_num(pick.weight_lbs));
                    if saved_layers.is_empty() {
                        layers.set(vec![LayerDraft::new()]);
                    } else {
                        layers.set(saved_layers.iter().map(LayerDraft::from_layer).collect());
                    }
                }
                loaded.set(true);
            }
        });
    }

    let project_name = project()
        .map(|p| p.name)
        .unwrap_or_else(|| "Project".into());

    let weight: f64 = weight_lbs().trim().parse().unwrap_or(f64::NAN);

    let built_layers: Vec<SlingLayer> = layers()
        .iter()
        .enumerate()
        .filter_map(|(i, d)| d.to_layer(pick_id, i as u32))
        .collect();
    let all_layers_valid = built_layers.len() == layers().len();
    // Geometry (calculated angles) per layer; only when every layer parses.
    let geoms: Vec<LayerGeometry> = if all_layers_valid {
        resolve_geometry(&built_layers, &catalog())
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
        layers()
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.to_diagram(i as u32))
            .collect()
    };
    let pick_result = if weight.is_finite() && weight >= 0.0 && built_layers.len() == layers().len()
    {
        calculate_pick(weight, &built_layers, &catalog())
    } else {
        None
    };

    let layer_count = layers().len();
    // Load passing through the top of each layer (payload + rigging below that point).
    let level_loads: Vec<f64> = pick_result
        .as_ref()
        .map(|pr| pr.layers.iter().map(|l| l.load_at_top_lbs).collect())
        .unwrap_or_default();

    rsx! {
        div { class: "page-shell editor-shell",
            header { class: "page-header compact",
                div { class: "page-header-inner",
                    Link {
                        to: Route::Project { id: project_id },
                        class: "back-link",
                        "← {project_name}"
                    }
                    h1 { class: "brand-title", "Pick editor" }
                }
            }

            main { class: "editor-grid",
                // ——— INPUT ———
                section { class: "panel input-panel",
                    h2 { class: "panel-title", "Input" }

                    div { class: "field-row",
                        label { class: "field grow",
                            span { class: "field-label", "Pick name" }
                            input {
                                class: "field-input",
                                value: "{pick_name}",
                                oninput: move |e| pick_name.set(e.value()),
                            }
                        }
                    }

                    div { class: "layer-toolbar",
                        h3 { class: "section-label", "Layers" }
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| layers.write().push(LayerDraft::new()),
                            "Add layer"
                        }
                    }

                    for (idx, draft) in layers().into_iter().enumerate() {
                        {
                            let key = draft.key;
                            let is_last = idx + 1 == layer_count;
                            let geom = geoms.get(idx).cloned().unwrap_or_default();
                            let angle_calc = geom.is_calculated();
                            let angle_error = geom.error.is_some();
                            let angle_label = if angle_error {
                                "Angle (° horiz.) — check geometry"
                            } else if angle_calc {
                                "Angle (° horiz., calculated)"
                            } else {
                                "Angle (° horiz., manual)"
                            };
                            let angle_display = match geom.angle_deg {
                                Some(a) if !angle_error => format!("{a:.1}"),
                                _ if angle_error => "—".to_string(),
                                _ => draft.angle_deg.clone(),
                            };
                            let geom_lines = geom.summary_lines();
                            let spacing_from_bar = draft.has_spreader() && geom.spreader_span_ft.is_some();
                            let catalog_now = catalog();
                            rsx! {
                                div { key: "{key}", class: "layer-card",
                                    div { class: "layer-card-head",
                                        span { class: "layer-badge", "L{idx + 1}" }
                                        button {
                                            class: "btn btn-ghost danger sm",
                                            disabled: layers().len() <= 1,
                                            onclick: move |_| {
                                                layers.write().retain(|l| l.key != key);
                                            },
                                            "Remove"
                                        }
                                    }

                                    div { class: "field-grid",
                                        label { class: "field",
                                            span { class: "field-label", "Round sling" }
                                            select {
                                                class: "field-input",
                                                value: "{draft.size}",
                                                onchange: move |e| {
                                                    if let Ok(size) = e.value().parse::<u8>() {
                                                        if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                            l.size = size;
                                                        }
                                                    }
                                                },
                                                for r in POLYESTER_ROUNDSLINGS {
                                                    option {
                                                        value: "{r.size}",
                                                        selected: draft.size == r.size,
                                                        "{r.label()}"
                                                    }
                                                }
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Hitch" }
                                            select {
                                                class: "field-input",
                                                value: "{draft.hitch.label()}",
                                                onchange: move |e| {
                                                    let hitch = match e.value().as_str() {
                                                        "Choker" => Hitch::Choker,
                                                        "Basket" => Hitch::Basket,
                                                        _ => Hitch::Vertical,
                                                    };
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.hitch = hitch;
                                                    }
                                                },
                                                for h in Hitch::all() {
                                                    option {
                                                        value: "{h.label()}",
                                                        selected: draft.hitch == h,
                                                        "{h.label()}"
                                                    }
                                                }
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "{angle_label}" }
                                            input {
                                                class: if angle_calc || angle_error { "field-input calc" } else { "field-input" },
                                                r#type: if angle_error { "text" } else { "number" },
                                                min: "1",
                                                max: "90",
                                                step: "any",
                                                readonly: angle_calc || angle_error,
                                                title: if angle_calc { "Calculated from sling length and pick-point spacing" } else { "" },
                                                value: "{angle_display}",
                                                oninput: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.angle_deg = e.value();
                                                    }
                                                },
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "# Slings" }
                                            input {
                                                class: "field-input",
                                                r#type: "number",
                                                min: "1",
                                                step: "1",
                                                value: "{draft.sling_count}",
                                                oninput: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.sling_count = e.value();
                                                    }
                                                },
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Sling length (ft)" }
                                            input {
                                                class: "field-input",
                                                r#type: "number",
                                                min: "0",
                                                step: "any",
                                                value: "{draft.sling_length_ft}",
                                                oninput: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.sling_length_ft = e.value();
                                                    }
                                                },
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label",
                                                if spacing_from_bar { "Pick spacing (ft) — from bar span" } else { "Pick spacing (ft, adjacent)" }
                                            }
                                            input {
                                                class: if spacing_from_bar { "field-input calc" } else { "field-input" },
                                                r#type: "number",
                                                min: "0",
                                                step: "any",
                                                placeholder: "center to center",
                                                readonly: spacing_from_bar,
                                                value: if spacing_from_bar {
                                                    geom.spacing_ft.map(format_num).unwrap_or_default()
                                                } else {
                                                    draft.pick_spacing_ft.clone()
                                                },
                                                oninput: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.pick_spacing_ft = e.value();
                                                    }
                                                },
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Pick width (ft, optional)" }
                                            input {
                                                class: if spacing_from_bar { "field-input calc" } else { "field-input" },
                                                r#type: "number",
                                                min: "0",
                                                step: "any",
                                                placeholder: "rectangular pattern",
                                                disabled: spacing_from_bar,
                                                value: "{draft.pick_width_ft}",
                                                oninput: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.pick_width_ft = e.value();
                                                    }
                                                },
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Apex shackle" }
                                            select {
                                                class: "field-input",
                                                value: "{draft.apex_shackle}",
                                                onchange: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.apex_shackle = e.value();
                                                    }
                                                },
                                                option { value: "", selected: draft.apex_shackle.is_empty(), "— none —" }
                                                for sh in SHACKLES {
                                                    option {
                                                        value: "{sh.size_in}",
                                                        selected: draft.apex_shackle == sh.size_in,
                                                        "{sh.label()}"
                                                    }
                                                }
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Leg shackle" }
                                            select {
                                                class: "field-input",
                                                value: "{draft.leg_shackle}",
                                                onchange: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.leg_shackle = e.value();
                                                    }
                                                },
                                                option { value: "", selected: draft.leg_shackle.is_empty(), "— none —" }
                                                for sh in SHACKLES {
                                                    option {
                                                        value: "{sh.size_in}",
                                                        selected: draft.leg_shackle == sh.size_in,
                                                        "{sh.label()}"
                                                    }
                                                }
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Other tare (lb)" }
                                            input {
                                                class: "field-input",
                                                r#type: "number",
                                                min: "0",
                                                step: "any",
                                                value: "{draft.tare_lbs}",
                                                oninput: move |e| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.tare_lbs = e.value();
                                                    }
                                                },
                                            }
                                        }
                                        if is_last {
                                            label { class: "field",
                                                span { class: "field-label", "Payload / load (lb)" }
                                                input {
                                                    class: "field-input",
                                                    r#type: "number",
                                                    min: "0",
                                                    step: "any",
                                                    value: "{weight_lbs}",
                                                    oninput: move |e| weight_lbs.set(e.value()),
                                                }
                                            }
                                        }
                                    }

                                    if !geom_lines.is_empty() {
                                        div { class: if angle_error { "geom-readout error" } else if geom.unequal_drop_in.is_some() { "geom-readout warn" } else { "geom-readout" },
                                            for line in geom_lines.iter() {
                                                div { "{line}" }
                                            }
                                        }
                                    }

                                    // Spreader quasi-layer under shackles
                                    div { class: "spreader-card",
                                        div { class: "spreader-card-head",
                                            span { class: "spreader-badge", "Spreader" }
                                            button {
                                                class: "btn btn-ghost sm",
                                                onclick: move |_| {
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.show_new_spreader = !l.show_new_spreader;
                                                    }
                                                },
                                                if draft.show_new_spreader { "Cancel new" } else { "+ New bar" }
                                            }
                                        }
                                        label { class: "field",
                                            span { class: "field-label", "Saved spreader" }
                                            select {
                                                class: "field-input",
                                                value: "{draft.spreader_id}",
                                                onchange: move |e| {
                                                    let id = e.value();
                                                    let bar_span = Uuid::parse_str(id.trim())
                                                        .ok()
                                                        .and_then(|uid| catalog().into_iter().find(|b| b.id == uid))
                                                        .and_then(|b| b.span_ft);
                                                    if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                        l.spreader_id = id;
                                                        l.legacy_spreader_wll = None;
                                                        // Prefill span from the saved bar; keep it editable per pick.
                                                        if let Some(span) = bar_span {
                                                            l.spreader_span_ft = format_num(span);
                                                        }
                                                    }
                                                },
                                                option {
                                                    value: "",
                                                    selected: draft.spreader_id.is_empty(),
                                                    "— none —"
                                                }
                                                if let Some(wll) = draft.legacy_spreader_wll {
                                                    option {
                                                        value: "",
                                                        selected: false,
                                                        disabled: true,
                                                        "Legacy {wll} lb WLL (pick a saved bar)"
                                                    }
                                                }
                                                for sp in catalog_now.iter() {
                                                    option {
                                                        value: "{sp.id}",
                                                        selected: draft.spreader_id == sp.id.to_string(),
                                                        "{sp.label()}"
                                                    }
                                                }
                                            }
                                        }
                                        if draft.has_spreader() {
                                            label { class: "field",
                                                span { class: "field-label", "Span as rigged (ft, lug to lug)" }
                                                input {
                                                    class: "field-input",
                                                    r#type: "number",
                                                    min: "0",
                                                    step: "any",
                                                    placeholder: "sets pick spacing",
                                                    value: "{draft.spreader_span_ft}",
                                                    oninput: move |e| {
                                                        if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                            l.spreader_span_ft = e.value();
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                        if draft.show_new_spreader {
                                            div { class: "spreader-new field-grid",
                                                label { class: "field",
                                                    span { class: "field-label", "Manufacturer" }
                                                    input {
                                                        class: "field-input",
                                                        value: "{draft.new_mfr}",
                                                        oninput: move |e| {
                                                            if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                                l.new_mfr = e.value();
                                                            }
                                                        },
                                                    }
                                                }
                                                label { class: "field",
                                                    span { class: "field-label", "Model" }
                                                    input {
                                                        class: "field-input",
                                                        value: "{draft.new_model}",
                                                        oninput: move |e| {
                                                            if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                                l.new_model = e.value();
                                                            }
                                                        },
                                                    }
                                                }
                                                label { class: "field",
                                                    span { class: "field-label", "WLL (lb)" }
                                                    input {
                                                        class: "field-input",
                                                        r#type: "number",
                                                        min: "1",
                                                        step: "1",
                                                        value: "{draft.new_wll}",
                                                        oninput: move |e| {
                                                            if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                                l.new_wll = e.value();
                                                            }
                                                        },
                                                    }
                                                }
                                                label { class: "field",
                                                    span { class: "field-label", "Self-weight (lb)" }
                                                    input {
                                                        class: "field-input",
                                                        r#type: "number",
                                                        min: "0",
                                                        step: "any",
                                                        value: "{draft.new_weight}",
                                                        oninput: move |e| {
                                                            if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                                l.new_weight = e.value();
                                                            }
                                                        },
                                                    }
                                                }
                                                label { class: "field",
                                                    span { class: "field-label", "Span (ft, optional)" }
                                                    input {
                                                        class: "field-input",
                                                        r#type: "number",
                                                        min: "0",
                                                        step: "any",
                                                        value: "{draft.new_span}",
                                                        oninput: move |e| {
                                                            if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                                l.new_span = e.value();
                                                            }
                                                        },
                                                    }
                                                }
                                                div { class: "field span-2",
                                                    button {
                                                        class: "btn btn-secondary",
                                                        onclick: {
                                                            let ctx = ctx.clone();
                                                            move |_| {
                                                                let snapshot = layers()
                                                                    .iter()
                                                                    .find(|l| l.key == key)
                                                                    .cloned();
                                                                let Some(d) = snapshot else { return };
                                                                let wll: u32 = match d.new_wll.trim().parse() {
                                                                    Ok(v) if v > 0 => v,
                                                                    _ => {
                                                                        status.set("Enter a valid spreader WLL.".into());
                                                                        return;
                                                                    }
                                                                };
                                                                let bar_w: f64 = match d.new_weight.trim().parse() {
                                                                    Ok(v) if v >= 0.0 => v,
                                                                    _ => {
                                                                        status.set("Enter a valid bar self-weight.".into());
                                                                        return;
                                                                    }
                                                                };
                                                                if d.new_mfr.trim().is_empty() || d.new_model.trim().is_empty() {
                                                                    status.set("Manufacturer and model are required.".into());
                                                                    return;
                                                                }
                                                                let mut bar = SavedSpreader::new(
                                                                    d.new_mfr.trim(),
                                                                    d.new_model.trim(),
                                                                    wll,
                                                                    bar_w,
                                                                );
                                                                if let Ok(span) = d.new_span.trim().parse::<f64>() {
                                                                    if span > 0.0 {
                                                                        bar.span_ft = Some(span);
                                                                    }
                                                                }
                                                                if let Some(ref s) = ctx.store {
                                                                    match s.save_spreader(&bar) {
                                                                        Ok(()) => {
                                                                            catalog.set(s.list_spreaders().unwrap_or_default());
                                                                            if let Some(l) = layers.write().iter_mut().find(|l| l.key == key) {
                                                                                l.spreader_id = bar.id.to_string();
                                                                                l.legacy_spreader_wll = None;
                                                                                if let Some(span) = bar.span_ft {
                                                                                    l.spreader_span_ft = format_num(span);
                                                                                }
                                                                                l.show_new_spreader = false;
                                                                                l.new_mfr.clear();
                                                                                l.new_model.clear();
                                                                                l.new_wll.clear();
                                                                                l.new_weight.clear();
                                                                                l.new_span.clear();
                                                                            }
                                                                            status.set("Spreader saved to catalog.".into());
                                                                        }
                                                                        Err(e) => status.set(format!("Spreader save failed: {e}")),
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        "Save to catalog"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "toolbar sticky-actions",
                        button {
                            class: "btn btn-primary",
                            onclick: {
                                let ctx = ctx.clone();
                                move |_| {
                                    let weight: f64 = match weight_lbs().trim().parse() {
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
                                                status.set(format!(
                                                    "Layer {} has invalid inputs.",
                                                    i + 1
                                                ));
                                                return;
                                            }
                                        }
                                    }
                                    // Persist calculated angles so the manual fallback stays current.
                                    let calc_geoms = resolve_geometry(&built, &catalog());
                                    for (l, g) in built.iter_mut().zip(calc_geoms.iter()) {
                                        if let (true, Some(a)) = (g.is_calculated(), g.angle_deg) {
                                            l.angle_deg = (a * 100.0).round() / 100.0;
                                        }
                                    }
                                    let pick = Pick {
                                        id: pick_id,
                                        project_id,
                                        name: pick_name(),
                                        weight_lbs: weight,
                                    };
                                    if let Some(ref s) = ctx.store {
                                        match s.save_pick(&pick, &built) {
                                            Ok(()) => {
                                                {
                                                    let mut drafts = layers.write();
                                                    for (d, l) in drafts.iter_mut().zip(built.iter()) {
                                                        d.angle_deg = format_num(l.angle_deg);
                                                    }
                                                }
                                                status.set("Pick saved.".into());
                                            }
                                            Err(e) => status.set(format!("Save failed: {e}")),
                                        }
                                    }
                                }
                            },
                            "Save pick"
                        }
                        button {
                            class: "btn btn-ghost",
                            onclick: move |_| {
                                navigator.push(Route::Project { id: project_id });
                            },
                            "Back"
                        }
                    }
                    if !status().is_empty() {
                        p { class: "status-line", "{status}" }
                    }
                    p { class: "hint",
                        "Payload is entered on the final layer. Rigging self-weight (slings by length, "
                        "apex and leg shackles, spreader bars, other tare) is added layer by layer, so "
                        "each layer carries the payload plus all rigging below it. "
                        "Lower-layer slings attach to the endpoints above. "
                        "Angle is from horizontal (90° = vertical) and is calculated from sling length and "
                        "pick-point spacing (center to center; a spreader's span sets it on its layer). "
                        "Add a width for 4-leg rectangular patterns. Without spacing, enter the angle manually."
                    }
                }

                // ——— OUTPUT / REPORT ———
                section { class: "report-canvas",
                    article { class: "lift-report",
                        header { class: "report-header",
                            div {
                                p { class: "report-brand", "BTH Rigging" }
                                h2 { class: "report-title", "Lift Report" }
                            }
                            div { class: "report-meta",
                                p { strong { "Project: " } "{project_name}" }
                                p { strong { "Pick: " } "{pick_name}" }
                                p { strong { "Payload: " }
                                    if weight.is_finite() {
                                        "{format_lbs(weight)} lb"
                                    } else {
                                        "—"
                                    }
                                }
                                p { strong { "Rigging weight: " }
                                    if let Some(ref pr) = pick_result {
                                        "{format_lbs(pr.rigging_weight_lbs)} lb"
                                    } else {
                                        "—"
                                    }
                                }
                                if let Some(h) = pick_result.as_ref().and_then(|pr| pr.rigging_height_ft) {
                                    p { strong { "Rigging height: " } "≈ {format_num((h * 100.0).round() / 100.0)} ft" }
                                }
                                p { strong { "Hook load: " }
                                    if let Some(ref pr) = pick_result {
                                        "{format_lbs(pr.hook_load_lbs)} lb"
                                    } else {
                                        "—"
                                    }
                                }
                            }
                        }

                        div { class: "report-diagram",
                            RiggingDiagram {
                                layers: diagram_layers.clone(),
                                weight_lbs: if weight.is_finite() { weight } else { 0.0 },
                                level_loads: level_loads.clone(),
                            }
                        }

                        if !weight.is_finite() || weight < 0.0 {
                            p { class: "report-warn", "Enter a valid payload on the final layer to compute reactions." }
                        } else if let Some(ref pr) = pick_result {
                            table { class: "report-table",
                                thead {
                                    tr {
                                        th { "Layer" }
                                        th { "Config" }
                                        th { "Rigging wt" }
                                        th { "Carried" }
                                        th { "Tension / leg" }
                                        th { "Sling WLL" }
                                        th { "Util." }
                                        th { "Hardware" }
                                        th { "Status" }
                                    }
                                }
                                tbody {
                                    for (idx, r) in pr.layers.iter().enumerate() {
                                        {
                                            let l = &built_layers[idx];
                                            let (sling_status, status_kind) = r.status();
                                            let sling_class = format!("stamp {status_kind}");
                                            let angle_text = if r.geometry.error.is_some() {
                                                "—°".to_string()
                                            } else if r.angle_calculated {
                                                format!("{:.1}° calc", r.angle_deg)
                                            } else {
                                                format!("{:.0}°", r.angle_deg)
                                            };
                                            let cfg_geom = r.geometry.summary_lines();
                                            let util_text = if r.utilization_pct.is_finite() {
                                                format!("{:.0}%", r.utilization_pct)
                                            } else {
                                                "—".to_string()
                                            };
                                            let hw_lines: Vec<String> = r.hardware.iter().map(|h| {
                                                let stamp = if h.overloaded { "OVER" } else { "OK" };
                                                format!(
                                                    "{}: {} / {} lb ({stamp})",
                                                    h.name,
                                                    format_lbs(h.reaction_lbs),
                                                    format_lbs(f64::from(h.wll_lbs)),
                                                )
                                            }).collect();
                                            let row_key = layers().get(idx).map(|d| d.key).unwrap_or(Uuid::nil());
                                            rsx! {
                                                tr { key: "{row_key}-row",
                                                    td { "L{idx + 1}" }
                                                    td { class: "cfg",
                                                        div { "RS-{l.size} · {l.hitch.label()} · {angle_text} · {l.sling_count} sling(s) × {format_num(l.sling_length_ft)} ft" }
                                                        for line in cfg_geom {
                                                            div { class: "muted geom-line", "{line}" }
                                                        }
                                                    }
                                                    td { class: "rig",
                                                        div { class: "num", strong { "+{format_lbs(r.rigging.total_lbs())} lb" } }
                                                        for line in r.rigging.breakdown_lines() {
                                                            div { class: "muted", "{line}" }
                                                        }
                                                        if r.sling_length_missing {
                                                            div { class: "muted", "Sling length not entered" }
                                                        }
                                                    }
                                                    td { class: "num",
                                                        div { "{format_lbs(r.carried_lbs)} lb" }
                                                        div { class: "muted", "top {format_lbs(r.load_at_top_lbs)} lb" }
                                                    }
                                                    td { class: "num", "{format_lbs(r.tension_lbs)} lb" }
                                                    td { class: "num", "{format_lbs(f64::from(r.hitch_wll_lbs))} lb" }
                                                    td { class: "num", "{util_text}" }
                                                    td { class: "hw",
                                                        if hw_lines.is_empty() {
                                                            span { class: "muted", "—" }
                                                        } else {
                                                            for line in hw_lines {
                                                                div { "{line}" }
                                                            }
                                                        }
                                                    }
                                                    td {
                                                        span { class: "{sling_class}", "{sling_status}" }
                                                        if r.sling_length_missing && sling_status != "NO LEN" {
                                                            div { span { class: "stamp warn", "NO LEN" } }
                                                        }
                                                        for h in r.hardware.iter().filter(|h| h.overloaded) {
                                                            div {
                                                                span { class: "stamp over", "HW OVER" }
                                                                span { class: "sr-only", "{h.name}" }
                                                            }
                                                        }
                                                    }
                                                }
                                                if let Some(ref sp) = r.spreader {
                                                    tr { key: "{row_key}-sp", class: "spreader-row",
                                                        td { class: "muted", "↳ bar" }
                                                        td { class: "cfg", "{sp.name}" }
                                                        td { class: "num muted", "(in layer)" }
                                                        td { class: "num",
                                                            "{format_lbs(r.load_on_spreader_lbs)} lb under"
                                                        }
                                                        td { class: "num muted",
                                                            "self {format_lbs(sp.weight_lbs)} lb"
                                                        }
                                                        td { class: "num", "{format_lbs(f64::from(sp.wll_lbs))} lb WLL" }
                                                        td { class: "num muted", "—" }
                                                        td { class: "hw",
                                                            {
                                                                let ends = l.sling_count.max(2);
                                                                let end_r = r.load_on_spreader_lbs / f64::from(ends);
                                                                let over = end_r > f64::from(sp.wll_lbs);
                                                                let stamp = if over { "OVER" } else { "OK" };
                                                                format!(
                                                                    "End rxn: {} / {} lb ({stamp})",
                                                                    format_lbs(end_r),
                                                                    format_lbs(f64::from(sp.wll_lbs)),
                                                                )
                                                            }
                                                        }
                                                        td {
                                                            {
                                                                let ends = l.sling_count.max(2);
                                                                let end_r = r.load_on_spreader_lbs / f64::from(ends);
                                                                if end_r > f64::from(sp.wll_lbs) {
                                                                    rsx! { span { class: "stamp over", "OVER" } }
                                                                } else {
                                                                    rsx! { span { class: "stamp ok", "OK" } }
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
                        } else {
                            p { class: "report-warn", "Check layer inputs to compute reactions." }
                        }

                        footer { class: "report-footer",
                            "Calculation aid only. Manufacturer identification tags, ASME B30.9 / B30.26, "
                            "and a qualified person govern the lift. Always verify spreader and shackle tags. "
                            "Rigging self-weight accumulates from the payload up to the hook; sling and "
                            "shackle weights are representative catalog values — verify with the manufacturer."
                        }
                    }
                }
            }
        }
    }
}
