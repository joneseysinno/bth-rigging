//! Mat analysis input panel: mat selection, outrigger load, pad and allowable GBP.

use dioxus::prelude::*;

use super::state::{MatState, NewMatForm, state};
use crate::app::store;
use crate::docs;
use bth_rigging::domain::SavedMat;

#[allow(non_snake_case)]
pub fn Input() -> Element {
    let Some(analysis_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(analysis_id) else {
        return rsx! {};
    };
    let MatState {
        mut name,
        mut mat_id,
        mut outrigger_load,
        mut pad_length,
        mut pad_width,
        mut allowable,
        mut mats,
        mut status,
        new_mat,
        ..
    } = st;
    let NewMatForm {
        show: mut show_new_mat,
        mfr: mut new_mfr,
        model: mut new_model,
        length: mut new_length,
        width: mut new_width,
        thickness: mut new_thickness,
        weight: mut new_weight,
        allow_load: mut new_allow_load,
    } = new_mat;

    rsx! {
        div { class: "app-panel-pad",
                    div { class: "field-row",
                        label { class: "field grow",
                            span { class: "field-label", "Analysis name" }
                            input {
                                class: "field-input",
                                value: "{name}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                    }

                    div { class: "spreader-card",
                        div { class: "spreader-card-head",
                            span { class: "spreader-badge", "Mat" }
                            button {
                                class: "btn btn-ghost sm",
                                onclick: move |_| show_new_mat.set(!show_new_mat()),
                                if show_new_mat() { "Cancel new" } else { "+ New mat" }
                            }
                        }

                        label { class: "field",
                            span { class: "field-label", "Saved mat" }
                            select {
                                class: "field-input",
                                value: "{mat_id}",
                                onchange: move |e| mat_id.set(e.value()),
                                option {
                                    value: "",
                                    selected: mat_id().is_empty(),
                                    "— select mat —"
                                }
                                for m in mats() {
                                    option {
                                        value: "{m.id}",
                                        selected: mat_id() == m.id.to_string(),
                                        "{m.label()}"
                                    }
                                }
                            }
                        }

                        if show_new_mat() {
                            div { class: "spreader-new field-grid",
                                label { class: "field",
                                    span { class: "field-label", "Manufacturer" }
                                    input {
                                        class: "field-input",
                                        value: "{new_mfr}",
                                        oninput: move |e| new_mfr.set(e.value()),
                                    }
                                }
                                label { class: "field",
                                    span { class: "field-label", "Model" }
                                    input {
                                        class: "field-input",
                                        value: "{new_model}",
                                        oninput: move |e| new_model.set(e.value()),
                                    }
                                }
                                label { class: "field",
                                    span { class: "field-label", "Length (ft)" }
                                    input {
                                        class: "field-input",
                                        r#type: "number",
                                        min: "0.1",
                                        step: "any",
                                        value: "{new_length}",
                                        oninput: move |e| new_length.set(e.value()),
                                    }
                                }
                                label { class: "field",
                                    span { class: "field-label", "Width (ft)" }
                                    input {
                                        class: "field-input",
                                        r#type: "number",
                                        min: "0.1",
                                        step: "any",
                                        value: "{new_width}",
                                        oninput: move |e| new_width.set(e.value()),
                                    }
                                }
                                label { class: "field",
                                    span { class: "field-label", "Thickness (in)" }
                                    input {
                                        class: "field-input",
                                        r#type: "number",
                                        min: "0",
                                        step: "any",
                                        value: "{new_thickness}",
                                        oninput: move |e| new_thickness.set(e.value()),
                                    }
                                }
                                label { class: "field",
                                    span { class: "field-label", "Weight (lb)" }
                                    input {
                                        class: "field-input",
                                        r#type: "number",
                                        min: "0",
                                        step: "any",
                                        value: "{new_weight}",
                                        oninput: move |e| new_weight.set(e.value()),
                                    }
                                }
                                label { class: "field span-2",
                                    span { class: "field-label", "Mfr. allowable outrigger load (lb)" }
                                    input {
                                        class: "field-input",
                                        r#type: "number",
                                        min: "0.1",
                                        step: "any",
                                        value: "{new_allow_load}",
                                        oninput: move |e| new_allow_load.set(e.value()),
                                    }
                                }
                                div { class: "field grow",
                                    button {
                                        class: "btn btn-secondary",
                                        onclick: {
                                            move |_| {
                                                let mfr = new_mfr().trim().to_string();
                                                let model = new_model().trim().to_string();
                                                if mfr.is_empty() || model.is_empty() {
                                                    status.set("Enter manufacturer and model.".into());
                                                    return;
                                                }
                                                let Ok(len) = new_length().trim().parse::<f64>() else {
                                                    status.set("Enter a valid mat length.".into());
                                                    return;
                                                };
                                                let Ok(wid) = new_width().trim().parse::<f64>() else {
                                                    status.set("Enter a valid mat width.".into());
                                                    return;
                                                };
                                                let Ok(thk) = new_thickness().trim().parse::<f64>() else {
                                                    status.set("Enter a valid thickness.".into());
                                                    return;
                                                };
                                                if len <= 0.0 || wid <= 0.0 || thk < 0.0 {
                                                    status.set("Mat dimensions must be positive.".into());
                                                    return;
                                                }
                                                let wt = new_weight().trim().parse::<f64>().unwrap_or(0.0);
                                                let Ok(allow_p) = new_allow_load().trim().parse::<f64>() else {
                                                    status.set("Enter manufacturer allowable outrigger load.".into());
                                                    return;
                                                };
                                                if allow_p <= 0.0 {
                                                    status.set("Manufacturer allowable load must be positive.".into());
                                                    return;
                                                }
                                                let mut mat = SavedMat::new(mfr, model, len, wid, thk);
                                                mat.weight_lbs = wt.max(0.0);
                                                mat.manufacturer_allowable_lbs = allow_p;
                                                if let Some(s) = store() {
                                                    match s.save_mat(&mat) {
                                                        Ok(()) => {
                                                            mats.set(s.list_mats().unwrap_or_default());
                                                            mat_id.set(mat.id.to_string());
                                                            show_new_mat.set(false);
                                                            new_mfr.set(String::new());
                                                            new_model.set(String::new());
                                                            status.set("Mat saved.".into());
                                                        }
                                                        Err(e) => status.set(format!("Save mat failed: {e}")),
                                                    }
                                                }
                                            }
                                        },
                                        "Save mat to catalog"
                                    }
                                }
                            }
                        }
                    }

                    div { class: "field-grid",
                        label { class: "field",
                            span { class: "field-label", "Outrigger load (lb)" }
                            input {
                                class: "field-input",
                                r#type: "number",
                                min: "0",
                                step: "any",
                                value: "{outrigger_load}",
                                oninput: move |e| outrigger_load.set(e.value()),
                            }
                        }
                        label { class: "field",
                            span { class: "field-label", "Pad length along mat (in)" }
                            input {
                                class: "field-input",
                                r#type: "number",
                                min: "0.1",
                                step: "any",
                                value: "{pad_length}",
                                oninput: move |e| pad_length.set(e.value()),
                            }
                        }
                        label { class: "field",
                            span { class: "field-label", "Pad width (in)" }
                            input {
                                class: "field-input",
                                r#type: "number",
                                min: "0.1",
                                step: "any",
                                value: "{pad_width}",
                                oninput: move |e| pad_width.set(e.value()),
                            }
                        }
                        label { class: "field",
                            span { class: "field-label", "Allowable GBP (psf)" }
                            input {
                                class: "field-input",
                                r#type: "number",
                                min: "0.1",
                                step: "any",
                                value: "{allowable}",
                                oninput: move |e| allowable.set(e.value()),
                            }
                        }
                    }

                    if !status().is_empty() {
                        p { class: "status-line", "{status}" }
                    }
                    p { class: "hint",
                        "Effective bearing length Leff from Duerr soil-bearing method: "
                        "Areqd = (P+W)/qa, Lreqd = Areqd/B, Leff = min(L, Lreqd). "
                        "Pad centered on the mat. Mat capacity usage uses the manufacturer allowable outrigger load."
                    }
        }
    }
}
