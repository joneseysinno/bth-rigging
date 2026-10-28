//! Pick viewport: the PDF-ready Lift Report.

use dioxus::prelude::*;
use uuid::Uuid;

use super::state::state;
use crate::docs::{self, LoadError, NotARecord};
use crate::ui::components::RiggingDiagram;
use bth_rigging::format::{format_lbs, format_num};

#[allow(non_snake_case)]
pub fn Report() -> Element {
    let Some(pick_id) = docs::use_record_id() else {
        return rsx! { NotARecord { label: "pick" } };
    };
    let st = match state(pick_id) {
        Ok(st) => st,
        Err(message) => return rsx! { LoadError { message } },
    };
    let calc = st.derive(pick_id);
    let project_name = (st.project_name)();
    let pick_name = st.name;
    let layers = st.layers;
    let weight = calc.weight;
    let built_layers = calc.built_layers;
    let diagram_layers = calc.diagram_layers;
    let pick_result = calc.pick_result;
    let level_loads = calc.level_loads;

    rsx! {
        div { class: "app-report-wrap",
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
