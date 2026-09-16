//! Mat bearing-pressure editor: Duerr Leff + manufacturer capacity.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::Route;
use crate::app_state::AppCtx;
use crate::diagram::MatBearingDiagram;
use bth_rigging::format::{format_lbs, format_num};
use bth_rigging::mats::{MatBearingInput, calculate_mat_bearing};
use bth_rigging::domain::{MatAnalysis, Project, SavedMat};

#[component]
pub fn MatEditor(project_id: Uuid, analysis_id: Uuid) -> Element {
    let ctx = use_context::<AppCtx>();
    let mut project = use_signal(|| None::<Project>);
    let mut name = use_signal(|| "New mat analysis".to_string());
    let mut mat_id = use_signal(|| String::new());
    let mut outrigger_load = use_signal(|| "50000".to_string());
    let mut pad_length = use_signal(|| "24".to_string());
    let mut pad_width = use_signal(|| "24".to_string());
    let mut allowable = use_signal(|| "3000".to_string());
    let mut mats = use_signal(Vec::<SavedMat>::new);
    let mut status = use_signal(|| String::new());
    let mut loaded = use_signal(|| false);
    let mut show_new_mat = use_signal(|| false);
    let mut new_mfr = use_signal(|| String::new());
    let mut new_model = use_signal(|| String::new());
    let mut new_length = use_signal(|| "8".to_string());
    let mut new_width = use_signal(|| "4".to_string());
    let mut new_thickness = use_signal(|| "6".to_string());
    let mut new_weight = use_signal(|| "0".to_string());
    let mut new_allow_load = use_signal(|| "100000".to_string());
    let navigator = use_navigator();

    {
        let ctx = ctx.clone();
        use_effect(move || {
            if loaded() {
                return;
            }
            if let Some(ref s) = ctx.store {
                project.set(s.load_project(project_id).ok().flatten());
                mats.set(s.list_mats().unwrap_or_default());
                if let Ok(Some(a)) = s.load_mat_analysis(analysis_id) {
                    name.set(a.name);
                    mat_id.set(if a.mat_id.is_nil() {
                        String::new()
                    } else {
                        a.mat_id.to_string()
                    });
                    outrigger_load.set(format_num(a.outrigger_load_lbs));
                    pad_length.set(format_num(a.pad_length_in));
                    pad_width.set(format_num(a.pad_width_in));
                    allowable.set(format_num(a.allowable_psf));
                }
                loaded.set(true);
            }
        });
    }

    let project_name = project()
        .map(|p| p.name)
        .unwrap_or_else(|| "Project".into());

    let selected_mat = {
        let id_str = mat_id();
        mats().into_iter().find(|m| m.id.to_string() == id_str)
    };

    let load: f64 = outrigger_load().trim().parse().unwrap_or(f64::NAN);
    let pad_l: f64 = pad_length().trim().parse().unwrap_or(f64::NAN);
    let pad_w: f64 = pad_width().trim().parse().unwrap_or(f64::NAN);
    let allow: f64 = allowable().trim().parse().unwrap_or(f64::NAN);

    let result = selected_mat.as_ref().and_then(|mat| {
        calculate_mat_bearing(MatBearingInput::from_mat(mat, load, pad_l, pad_w, allow))
    });

    rsx! {
        div { class: "page-shell editor-shell",
            header { class: "page-header compact",
                div { class: "page-header-inner",
                    Link {
                        to: Route::Project { id: project_id },
                        class: "back-link",
                        "← {project_name}"
                    }
                    h1 { class: "brand-title", "Mat bearing editor" }
                }
            }

            main { class: "editor-grid",
                section { class: "panel input-panel",
                    h2 { class: "panel-title", "Input" }

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
                                            let ctx = ctx.clone();
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
                                                if let Some(ref s) = ctx.store {
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

                    div { class: "sticky-actions",
                        button {
                            class: "btn btn-primary",
                            onclick: {
                                let ctx = ctx.clone();
                                move |_| {
                                    let Ok(load_v) = outrigger_load().trim().parse::<f64>() else {
                                        status.set("Enter a valid outrigger load.".into());
                                        return;
                                    };
                                    let Ok(pad_l_v) = pad_length().trim().parse::<f64>() else {
                                        status.set("Enter a valid pad length.".into());
                                        return;
                                    };
                                    let Ok(pad_w_v) = pad_width().trim().parse::<f64>() else {
                                        status.set("Enter a valid pad width.".into());
                                        return;
                                    };
                                    let Ok(allow_v) = allowable().trim().parse::<f64>() else {
                                        status.set("Enter a valid allowable pressure.".into());
                                        return;
                                    };
                                    let mid = if mat_id().trim().is_empty() {
                                        Uuid::nil()
                                    } else {
                                        match Uuid::parse_str(mat_id().trim()) {
                                            Ok(u) => u,
                                            Err(_) => {
                                                status.set("Select a saved mat.".into());
                                                return;
                                            }
                                        }
                                    };
                                    let analysis = MatAnalysis {
                                        id: analysis_id,
                                        project_id,
                                        name: name(),
                                        mat_id: mid,
                                        outrigger_load_lbs: load_v,
                                        pad_length_in: pad_l_v,
                                        pad_width_in: pad_w_v,
                                        allowable_psf: allow_v,
                                        spread_angle_deg: 45.0,
                                    };
                                    if let Some(ref s) = ctx.store {
                                        match s.save_mat_analysis(&analysis) {
                                            Ok(()) => status.set("Analysis saved.".into()),
                                            Err(e) => status.set(format!("Save failed: {e}")),
                                        }
                                    }
                                }
                            },
                            "Save analysis"
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
                        "Effective bearing length Leff from Duerr soil-bearing method: "
                        "Areqd = (P+W)/qa, Lreqd = Areqd/B, Leff = min(L, Lreqd). "
                        "Pad centered on the mat. Mat capacity usage uses the manufacturer allowable outrigger load."
                    }
                }

                section { class: "report-canvas",
                    article { class: "lift-report",
                        header { class: "report-header",
                            div {
                                p { class: "report-brand", "BTH Rigging" }
                                h2 { class: "report-title", "Mat Bearing Report" }
                            }
                            div { class: "report-meta",
                                p { strong { "Project: " } "{project_name}" }
                                p { strong { "Analysis: " } "{name}" }
                                p { strong { "Outrigger: " }
                                    if load.is_finite() {
                                        "{format_lbs(load)} lb"
                                    } else {
                                        "—"
                                    }
                                }
                                p { strong { "Allowable GBP: " }
                                    if allow.is_finite() {
                                        "{format_lbs(allow)} psf"
                                    } else {
                                        "—"
                                    }
                                }
                            }
                        }

                        if let Some(ref mat) = selected_mat {
                            div { class: "report-diagram",
                                MatBearingDiagram {
                                    mat_length_ft: mat.length_ft,
                                    mat_width_ft: mat.width_ft,
                                    pad_length_ft: result.as_ref().map(|r| r.pad_length_ft).unwrap_or(pad_l / 12.0),
                                    pad_width_ft: result.as_ref().map(|r| r.pad_width_ft).unwrap_or(pad_w / 12.0),
                                    effective_length_ft: result.as_ref().map(|r| r.leff_ft).unwrap_or(0.0),
                                    effective_width_ft: result.as_ref().map(|r| r.effective_width_ft).unwrap_or(mat.width_ft),
                                }
                            }

                            if let Some(ref r) = result {
                                table { class: "report-table",
                                    tbody {
                                        tr {
                                            th { "Mat" }
                                            td { "{mat.label()}" }
                                        }
                                        tr {
                                            th { "Thickness" }
                                            td { "{format_num(mat.thickness_in)} in" }
                                        }
                                        tr {
                                            th { "Mat weight W" }
                                            td { "{format_lbs(mat.weight_lbs)} lb" }
                                        }
                                        tr {
                                            th { "Mfr. allowable P" }
                                            td { "{format_lbs(r.manufacturer_allowable_lbs)} lb" }
                                        }
                                        tr {
                                            th { "Pad C × width" }
                                            td {
                                                "{format_num(pad_l)} × {format_num(pad_w)} in "
                                                "({format_num(r.pad_length_ft)} × {format_num(r.pad_width_ft)} ft)"
                                            }
                                        }
                                        tr {
                                            th { "Areqd" }
                                            td { "{format_num(r.areqd_ft2)} ft²" }
                                        }
                                        tr {
                                            th { "Lreqd" }
                                            td { "{format_num(r.lreqd_ft)} ft" }
                                        }
                                        tr {
                                            th { "Leff × B" }
                                            td {
                                                "{format_num(r.leff_ft)} × {format_num(r.effective_width_ft)} ft "
                                                "= {format_num(r.area_ft2)} ft²"
                                            }
                                        }
                                        tr {
                                            th { "Lc (cantilever)" }
                                            td { "{format_num(r.lc_ft)} ft each side" }
                                        }
                                        tr {
                                            th { "Total load P+W" }
                                            td { "{format_lbs(r.total_load_lbs)} lb" }
                                        }
                                        tr {
                                            th { "q (crane only)" }
                                            td { "{format_lbs(r.q_psf)} psf" }
                                        }
                                        tr {
                                            th { "qt vs qa" }
                                            td {
                                                "{format_lbs(r.qt_psf)} / {format_lbs(r.allowable_psf)} psf"
                                            }
                                        }
                                        tr {
                                            th { "Soil utilization" }
                                            td {
                                                "{format_num(r.soil_utilization_pct)}% "
                                                span {
                                                    class: if r.soil_overloaded { "stamp over" } else { "stamp ok" },
                                                    if r.soil_overloaded { "OVER" } else { "OK" }
                                                }
                                                if r.mat_too_short {
                                                    " "
                                                    span { class: "stamp warn", "SHORT" }
                                                }
                                            }
                                        }
                                        tr {
                                            th { "Mat capacity usage" }
                                            td {
                                                "{format_num(r.mat_utilization_pct)}% "
                                                "({format_lbs(load)} / {format_lbs(r.manufacturer_allowable_lbs)} lb) "
                                                span {
                                                    class: if r.mat_overloaded { "stamp over" } else { "stamp ok" },
                                                    if r.mat_overloaded { "OVER" } else { "OK" }
                                                }
                                            }
                                        }
                                        if r.pad_geometry_warn {
                                            tr {
                                                th { "Geometry" }
                                                td {
                                                    span { class: "stamp warn", "WARN" }
                                                    " Pad length C ≥ Leff — check pad / mat sizing."
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                p { class: "report-warn",
                                    "Enter valid outrigger load, pad size, and allowable pressure. "
                                    "Mat must have a positive manufacturer allowable load."
                                }
                            }
                        } else {
                            p { class: "report-warn",
                                "Select or create a mat to compute effective bearing length."
                            }
                        }

                        footer { class: "report-footer",
                            "Calculation aid only. Effective length uses Duerr (2010) soil-bearing method. "
                            "Manufacturer allowable outrigger load and identification tags govern mat capacity. "
                            "Site geotechnical values and a qualified person govern crane setup."
                        }
                    }
                }
            }
        }
    }
}
