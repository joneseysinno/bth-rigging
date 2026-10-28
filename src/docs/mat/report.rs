//! Mat analysis viewport: the Mat Bearing Report.

use dioxus::prelude::*;

use super::state::state;
use crate::docs::{self, LoadError, NotARecord};
use crate::ui::components::MatBearingDiagram;
use bth_rigging::format::{format_lbs, format_num};

#[allow(non_snake_case)]
pub fn Report() -> Element {
    let Some(analysis_id) = docs::use_record_id() else {
        return rsx! { NotARecord { label: "mat analysis" } };
    };
    let st = match state(analysis_id) {
        Ok(st) => st,
        Err(message) => return rsx! { LoadError { message } },
    };
    let calc = st.derive();
    let project_name = (st.project_name)();
    let name = st.name;
    let selected_mat = calc.selected_mat;
    let load = calc.load;
    let pad_l = calc.pad_l;
    let pad_w = calc.pad_w;
    let allow = calc.allow;
    let result = calc.result;

    rsx! {
        div { class: "app-report-wrap",
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
