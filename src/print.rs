//! Project calc-package PDF export via Typst (US Letter).

use chrono::Local;
use serde::Serialize;
use thiserror::Error;
use typst::foundations::{Dict, IntoValue};
use typst_as_lib::TypstEngine;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;
use uuid::Uuid;

use crate::store::RiggingStore;
use crate::domain::{MatAnalysis, Pick, Project, SavedMat, SavedSpreader, SlingLayer};
use crate::format::{format_lbs, format_num};
use crate::layers::calculate_pick;
use crate::mats::{MatBearingInput, calculate_mat_bearing};

const TEMPLATE: &str = include_str!("../assets/calc-package.typ");

#[derive(Debug, Error)]
pub enum PrintError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl PrintError {
    fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CalcPackage {
    pub project: String,
    pub printed_at: String,
    pub picks: Vec<PickSheet>,
    pub mats: Vec<MatSheet>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PickSheet {
    pub name: String,
    pub payload: String,
    pub rigging_weight: String,
    /// Approximate hook-to-pick-point height, when all angles are calculated.
    pub rigging_height: Option<String>,
    pub hook_load: String,
    pub warn: Option<String>,
    pub rows: Vec<PickRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PickRow {
    pub layer: String,
    pub config: String,
    /// Pick-point geometry notes (spacing source, reach/drop, warnings).
    pub geometry: Vec<String>,
    /// Layer rigging weight: first line is the total, then the breakdown.
    pub rigging: Vec<String>,
    pub carried: String,
    /// Load delivered up from the top of this layer (after apex shackles).
    pub top: String,
    pub tension: String,
    pub sling_wll: String,
    pub util: String,
    pub hardware: Vec<String>,
    pub status: String,
    pub status_kind: String,
    pub is_spreader: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatSheet {
    pub name: String,
    pub outrigger: String,
    pub allowable: String,
    pub warn: Option<String>,
    pub rows: Vec<MatRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatRow {
    pub label: String,
    pub value: String,
    pub status: Option<String>,
    pub status_kind: Option<String>,
}

/// Sanitize a project name for use as a Windows filename stem.
pub fn calc_package_filename(project_name: &str) -> String {
    let mut stem: String = project_name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    stem = stem.trim().trim_matches('.').to_string();
    if stem.is_empty() {
        stem = "project".into();
    }
    format!("{stem} calc package.pdf")
}

/// Assemble calc-package DTO from store data for one project.
pub fn assemble_calc_package(
    project: &Project,
    picks: &[(Pick, Vec<SlingLayer>)],
    spreaders: &[SavedSpreader],
    mats: &[(MatAnalysis, Option<SavedMat>)],
) -> CalcPackage {
    let printed_at = Local::now().format("%Y-%m-%d %H:%M").to_string();
    CalcPackage {
        project: project.name.clone(),
        printed_at,
        picks: picks
            .iter()
            .map(|(pick, layers)| assemble_pick_sheet(pick, layers, spreaders))
            .collect(),
        mats: mats
            .iter()
            .map(|(analysis, mat)| assemble_mat_sheet(analysis, mat.as_ref()))
            .collect(),
    }
}

/// Load project data from the store and assemble the package DTO.
pub fn assemble_from_store(
    store: &RiggingStore,
    project_id: Uuid,
) -> Result<CalcPackage, PrintError> {
    let project = store
        .load_project(project_id)
        .map_err(|e| PrintError::msg(e.to_string()))?
        .ok_or_else(|| PrintError::msg("Project not found."))?;

    let pick_metas = store
        .list_picks_for_project(project_id)
        .map_err(|e| PrintError::msg(e.to_string()))?;
    let mut picks = Vec::with_capacity(pick_metas.len());
    for meta in pick_metas {
        let (pick, layers) = store
            .load_pick(meta.id)
            .map_err(|e| PrintError::msg(e.to_string()))?
            .unwrap_or((meta, Vec::new()));
        picks.push((pick, layers));
    }

    let spreaders = store
        .list_spreaders()
        .map_err(|e| PrintError::msg(e.to_string()))?;

    let analyses = store
        .list_mat_analyses_for_project(project_id)
        .map_err(|e| PrintError::msg(e.to_string()))?;
    let mut mats = Vec::with_capacity(analyses.len());
    for analysis in analyses {
        let mat = if analysis.mat_id.is_nil() {
            None
        } else {
            store
                .load_mat(analysis.mat_id)
                .map_err(|e| PrintError::msg(e.to_string()))?
        };
        mats.push((analysis, mat));
    }

    if picks.is_empty() && mats.is_empty() {
        return Err(PrintError::msg(
            "Add a pick or mat analysis before printing.",
        ));
    }

    Ok(assemble_calc_package(&project, &picks, &spreaders, &mats))
}

/// Compile the calc package to PDF bytes.
pub fn render_calc_package_pdf(package: &CalcPackage) -> Result<Vec<u8>, PrintError> {
    let json = serde_json::to_string(package)?;
    let mut dict = Dict::new();
    dict.insert("data".into(), json.into_value());

    let engine = TypstEngine::builder()
        .main_file(TEMPLATE)
        .search_fonts_with(
            TypstKitFontOptions::default()
                .include_system_fonts(false)
                .include_embedded_fonts(true),
        )
        .build();

    let compiled = engine.compile_with_input(dict);
    let doc = compiled
        .output
        .map_err(|err| PrintError::msg(err.to_string()))?;

    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|err| PrintError::msg(format!("PDF export failed: {err:?}")))
}

/// Full pipeline: assemble from store and render PDF bytes.
pub fn render_project_calc_package(
    store: &RiggingStore,
    project_id: Uuid,
) -> Result<Vec<u8>, PrintError> {
    let package = assemble_from_store(store, project_id)?;
    render_calc_package_pdf(&package)
}

fn assemble_pick_sheet(
    pick: &Pick,
    layers: &[SlingLayer],
    spreaders: &[SavedSpreader],
) -> PickSheet {
    let weight = pick.weight_lbs;
    if !weight.is_finite() || weight < 0.0 {
        return PickSheet {
            name: pick.name.clone(),
            payload: "—".into(),
            rigging_weight: "—".into(),
            rigging_height: None,
            hook_load: "—".into(),
            warn: Some("Enter a valid payload on the final layer to compute reactions.".into()),
            rows: Vec::new(),
        };
    }

    let Some(pr) = calculate_pick(weight, layers, spreaders) else {
        return PickSheet {
            name: pick.name.clone(),
            payload: format!("{} lb", format_lbs(weight)),
            rigging_weight: "—".into(),
            rigging_height: None,
            hook_load: "—".into(),
            warn: Some("Check layer inputs to compute reactions.".into()),
            rows: Vec::new(),
        };
    };

    let mut rows = Vec::new();
    for (idx, r) in pr.layers.iter().enumerate() {
        let l = &layers[idx];
        let (status, status_kind) = r.status();
        let angle_text = if r.geometry.error.is_some() {
            "—°".to_string()
        } else if r.angle_calculated {
            format!("{:.1}° calc", r.angle_deg)
        } else {
            format!("{:.0}°", r.angle_deg)
        };

        let hw_lines: Vec<String> = r
            .hardware
            .iter()
            .map(|h| {
                let stamp = if h.overloaded { "OVER" } else { "OK" };
                format!(
                    "{}: {} / {} lb ({stamp})",
                    h.name,
                    format_lbs(h.reaction_lbs),
                    format_lbs(f64::from(h.wll_lbs)),
                )
            })
            .collect();

        rows.push(PickRow {
            layer: format!("L{}", idx + 1),
            config: format!(
                "RS-{} · {} · {} · {} sling(s) × {} ft",
                l.size,
                l.hitch.label(),
                angle_text,
                l.sling_count,
                format_num(l.sling_length_ft)
            ),
            geometry: r.geometry.summary_lines(),
            rigging: {
                let mut lines = vec![format!("+{} lb", format_lbs(r.rigging.total_lbs()))];
                lines.extend(r.rigging.breakdown_lines());
                if r.sling_length_missing {
                    lines.push("Sling length not entered".into());
                }
                lines
            },
            carried: format!("{} lb", format_lbs(r.carried_lbs)),
            top: format!("top {} lb", format_lbs(r.load_at_top_lbs)),
            tension: format!("{} lb", format_lbs(r.tension_lbs)),
            sling_wll: format!("{} lb", format_lbs(f64::from(r.hitch_wll_lbs))),
            util: if r.utilization_pct.is_finite() {
                format!("{:.0}%", r.utilization_pct)
            } else {
                "—".into()
            },
            hardware: hw_lines,
            status: status.into(),
            status_kind: status_kind.into(),
            is_spreader: false,
        });

        if let Some(ref sp) = r.spreader {
            let ends = l.sling_count.max(2);
            let end_r = r.load_on_spreader_lbs / f64::from(ends);
            let over = end_r > f64::from(sp.wll_lbs);
            let (sp_status, sp_kind) = if over { ("OVER", "over") } else { ("OK", "ok") };
            rows.push(PickRow {
                layer: "↳ bar".into(),
                config: sp.name.clone(),
                geometry: r
                    .geometry
                    .spreader_span_ft
                    .map(|span| vec![format!("Span as rigged {} ft", format_num(span))])
                    .unwrap_or_default(),
                rigging: vec!["(in layer)".into()],
                carried: format!("{} lb under", format_lbs(r.load_on_spreader_lbs)),
                top: String::new(),
                tension: format!("self {} lb", format_lbs(sp.weight_lbs)),
                sling_wll: format!("{} lb WLL", format_lbs(f64::from(sp.wll_lbs))),
                util: "—".into(),
                hardware: vec![format!(
                    "End rxn: {} / {} lb ({})",
                    format_lbs(end_r),
                    format_lbs(f64::from(sp.wll_lbs)),
                    if over { "OVER" } else { "OK" }
                )],
                status: sp_status.into(),
                status_kind: sp_kind.into(),
                is_spreader: true,
            });
        }
    }

    PickSheet {
        name: pick.name.clone(),
        payload: format!("{} lb", format_lbs(weight)),
        rigging_weight: format!("{} lb", format_lbs(pr.rigging_weight_lbs)),
        rigging_height: pr.rigging_height_ft.map(|h| {
            format!(
                "≈ {} ft (excl. shackle / bar depth)",
                format_num((h * 100.0).round() / 100.0)
            )
        }),
        hook_load: format!("{} lb", format_lbs(pr.hook_load_lbs)),
        warn: None,
        rows,
    }
}

fn assemble_mat_sheet(analysis: &MatAnalysis, mat: Option<&SavedMat>) -> MatSheet {
    let load = analysis.outrigger_load_lbs;
    let allow = analysis.allowable_psf;
    let outrigger = if load.is_finite() {
        format!("{} lb", format_lbs(load))
    } else {
        "—".into()
    };
    let allowable = if allow.is_finite() {
        format!("{} psf", format_lbs(allow))
    } else {
        "—".into()
    };

    let Some(mat) = mat else {
        return MatSheet {
            name: analysis.name.clone(),
            outrigger,
            allowable,
            warn: Some("Select or create a mat to compute effective bearing length.".into()),
            rows: Vec::new(),
        };
    };

    let Some(r) = calculate_mat_bearing(MatBearingInput::from_mat(
        mat,
        analysis.outrigger_load_lbs,
        analysis.pad_length_in,
        analysis.pad_width_in,
        analysis.allowable_psf,
    )) else {
        return MatSheet {
            name: analysis.name.clone(),
            outrigger,
            allowable,
            warn: Some(
                "Enter valid outrigger load, pad size, and allowable pressure. \
                 Mat must have a positive manufacturer allowable load."
                    .into(),
            ),
            rows: Vec::new(),
        };
    };

    let soil_kind = if r.soil_overloaded { "over" } else { "ok" };
    let soil_status = if r.soil_overloaded { "OVER" } else { "OK" };
    let mat_kind = if r.mat_overloaded { "over" } else { "ok" };
    let mat_status = if r.mat_overloaded { "OVER" } else { "OK" };

    let mut rows = vec![
        MatRow {
            label: "Mat".into(),
            value: mat.label(),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Thickness".into(),
            value: format!("{} in", format_num(mat.thickness_in)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Mat weight W".into(),
            value: format!("{} lb", format_lbs(mat.weight_lbs)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Mfr. allowable P".into(),
            value: format!("{} lb", format_lbs(r.manufacturer_allowable_lbs)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Pad C × width".into(),
            value: format!(
                "{} × {} in ({} × {} ft)",
                format_num(analysis.pad_length_in),
                format_num(analysis.pad_width_in),
                format_num(r.pad_length_ft),
                format_num(r.pad_width_ft)
            ),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Areqd".into(),
            value: format!("{} ft²", format_num(r.areqd_ft2)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Lreqd".into(),
            value: format!("{} ft", format_num(r.lreqd_ft)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Leff × B".into(),
            value: format!(
                "{} × {} ft = {} ft²",
                format_num(r.leff_ft),
                format_num(r.effective_width_ft),
                format_num(r.area_ft2)
            ),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Lc (cantilever)".into(),
            value: format!("{} ft each side", format_num(r.lc_ft)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Total load P+W".into(),
            value: format!("{} lb", format_lbs(r.total_load_lbs)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "q (crane only)".into(),
            value: format!("{} psf", format_lbs(r.q_psf)),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "qt vs qa".into(),
            value: format!(
                "{} / {} psf",
                format_lbs(r.qt_psf),
                format_lbs(r.allowable_psf)
            ),
            status: None,
            status_kind: None,
        },
        MatRow {
            label: "Soil utilization".into(),
            value: {
                let mut s = format!("{}%", format_num(r.soil_utilization_pct));
                if r.mat_too_short {
                    s.push_str(" · SHORT");
                }
                s
            },
            status: Some(soil_status.into()),
            status_kind: Some(soil_kind.into()),
        },
        MatRow {
            label: "Mat capacity usage".into(),
            value: format!(
                "{}% ({} / {} lb)",
                format_num(r.mat_utilization_pct),
                format_lbs(load),
                format_lbs(r.manufacturer_allowable_lbs)
            ),
            status: Some(mat_status.into()),
            status_kind: Some(mat_kind.into()),
        },
    ];

    if r.pad_geometry_warn {
        rows.push(MatRow {
            label: "Geometry".into(),
            value: "Pad length C ≥ Leff — check pad / mat sizing.".into(),
            status: Some("WARN".into()),
            status_kind: Some("warn".into()),
        });
    }

    MatSheet {
        name: analysis.name.clone(),
        outrigger,
        allowable,
        warn: None,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, MatAnalysis, Pick, Project, SavedMat, SlingLayer};

    fn sample_layer(pick_id: Uuid, index: u32) -> SlingLayer {
        SlingLayer {
            pick_id,
            layer_index: index,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 60.0,
            sling_count: 2,
            sling_length_ft: 10.0,
            pick_spacing_ft: None,
            pick_width_ft: None,
            spreader_span_ft: None,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: None,
            tare_lbs: 0.0,
        }
    }

    #[test]
    fn filename_strips_illegal_chars() {
        assert_eq!(
            calc_package_filename("Job <A>/B:?\"*"),
            "Job -A--B---- calc package.pdf"
        );
        assert_eq!(calc_package_filename("   "), "project calc package.pdf");
    }

    #[test]
    fn assemble_pick_and_mat_sheets() {
        let project = Project::new("Tower A");
        let pick = Pick::new(project.id, "Main hoist", 10_000.0);
        let layers = vec![sample_layer(pick.id, 0)];

        let mut mat = SavedMat::new("Acme", "M8", 8.0, 4.0, 6.0);
        mat.weight_lbs = 1_200.0;
        mat.manufacturer_allowable_lbs = 100_000.0;
        let analysis = MatAnalysis {
            id: Uuid::new_v4(),
            project_id: project.id,
            name: "Outrigger NE".into(),
            mat_id: mat.id,
            outrigger_load_lbs: 50_000.0,
            pad_length_in: 24.0,
            pad_width_in: 24.0,
            allowable_psf: 3_000.0,
            spread_angle_deg: 45.0,
        };

        let pkg = assemble_calc_package(&project, &[(pick, layers)], &[], &[(analysis, Some(mat))]);

        assert_eq!(pkg.project, "Tower A");
        assert_eq!(pkg.picks.len(), 1);
        assert_eq!(pkg.mats.len(), 1);
        assert!(pkg.picks[0].warn.is_none());
        assert!(!pkg.picks[0].rows.is_empty());
        assert_eq!(pkg.picks[0].rows[0].status_kind, "ok");
        // 2 × RS-5 @ 10 ft × 1.00 lb/ft = 20 lb of sling weight on top of 10,000 lb.
        assert_eq!(pkg.picks[0].rigging_weight, "20 lb");
        assert_eq!(pkg.picks[0].hook_load, "10,020 lb");
        assert!(pkg.mats[0].warn.is_none());
        assert!(
            pkg.mats[0]
                .rows
                .iter()
                .any(|r| r.label == "Soil utilization")
        );
    }

    #[test]
    fn incomplete_pick_gets_warning() {
        let project = Project::new("Empty");
        let pick = Pick::new(project.id, "No layers", 10_000.0);
        let pkg = assemble_calc_package(&project, &[(pick, vec![])], &[], &[]);
        assert!(pkg.picks[0].warn.is_some());
        assert!(pkg.picks[0].rows.is_empty());
    }

    #[test]
    fn incomplete_mat_gets_warning() {
        let project = Project::new("Empty");
        let analysis = MatAnalysis::new(project.id, "No mat", Uuid::nil());
        let pkg = assemble_calc_package(&project, &[], &[], &[(analysis, None)]);
        assert!(pkg.mats[0].warn.is_some());
        assert!(pkg.mats[0].rows.is_empty());
    }

    #[test]
    fn compile_smoke_produces_pdf() {
        let project = Project::new("Smoke");
        let pick = Pick::new(project.id, "Lift 1", 10_000.0);
        let layers = vec![sample_layer(pick.id, 0)];
        let mut mat = SavedMat::new("Acme", "M8", 8.0, 4.0, 6.0);
        mat.weight_lbs = 1_200.0;
        mat.manufacturer_allowable_lbs = 100_000.0;
        let analysis = MatAnalysis {
            id: Uuid::new_v4(),
            project_id: project.id,
            name: "Mat 1".into(),
            mat_id: mat.id,
            outrigger_load_lbs: 50_000.0,
            pad_length_in: 24.0,
            pad_width_in: 24.0,
            allowable_psf: 3_000.0,
            spread_angle_deg: 45.0,
        };
        let pkg = assemble_calc_package(&project, &[(pick, layers)], &[], &[(analysis, Some(mat))]);
        let pdf = render_calc_package_pdf(&pkg).expect("pdf");
        assert!(pdf.starts_with(b"%PDF"));
    }
}
