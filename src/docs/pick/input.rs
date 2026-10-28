//! Pick input panel: name, layers, spreaders and payload.

use dioxus::prelude::*;
use uuid::Uuid;

use super::draft::LayerDraft;
use super::state::{PickState, state};
use crate::app::store;
use crate::docs;
use bth_rigging::catalog::{POLYESTER_ROUNDSLINGS, SHACKLES};
use bth_rigging::domain::{Hitch, SavedSpreader};
use bth_rigging::format::format_num;

#[allow(non_snake_case)]
pub fn Input() -> Element {
    let Some(pick_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(pick_id) else {
        return rsx! {};
    };
    let PickState {
        name: mut pick_name,
        mut weight_lbs,
        mut layers,
        mut catalog,
        mut status,
        ..
    } = st;
    let geoms = st.derive(pick_id).geoms;
    let layer_count = layers().len();

    rsx! {
        div { class: "app-panel-pad",
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
                    class: "btn btn-secondary sm",
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
                                                                if let Some(s) = store() {
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
    }
}
