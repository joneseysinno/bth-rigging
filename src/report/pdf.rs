//! Typst PDF rendering for the calc package.
//!
//! Roadmap: Step 0 foundation.

use typst::foundations::{Dict, IntoValue};
use typst_as_lib::TypstEngine;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;
use uuid::Uuid;

use crate::store::RiggingStore;

use super::calc_package::{CalcPackage, PrintError, assemble_from_store};

const TEMPLATE: &str = include_str!("../../assets/calc-package.typ");

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, MatAnalysis, Pick, Project, SavedMat, SlingLayer};
    use crate::report::calc_package::assemble_calc_package;

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
