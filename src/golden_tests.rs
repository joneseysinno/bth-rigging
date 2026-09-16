//! Golden snapshots for stored-layer JSON and calc-package DTOs.
//!
//! Run with `UPDATE_GOLDEN=1` to regenerate fixtures after an intentional change.
//!
//! Included as a unit-test module from `main.rs` until the lib+bin split (0.3);
//! then this file becomes a standalone integration test under `tests/`.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{
    Hitch, MatAnalysis, Pick, Project, SavedMat, SavedSpreader, SlingLayer,
};
use crate::print::{assemble_calc_package, CalcPackage};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn update_golden() -> bool {
    matches!(
        std::env::var("UPDATE_GOLDEN").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

fn read_or_write(path: &PathBuf, fresh: &str) {
    if update_golden() {
        fs::create_dir_all(path.parent().unwrap()).expect("fixtures dir");
        fs::write(path, fresh).expect("write golden");
        return;
    }
    let expected = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("missing golden {}: {e}", path.display()));
    assert_eq!(
        fresh, expected,
        "golden mismatch for {} — set UPDATE_GOLDEN=1 to regenerate",
        path.display()
    );
}

/// Canonical pretty JSON (sorted keys not required; stable field order from serde).
fn pretty<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("serialize");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

// --- layers_v1: exact JSON shape written by save_pick / save_spreader / save_mat ---

#[derive(Debug, Serialize, Deserialize)]
struct LayersFixture {
    pick_full: Pick,
    pick_minimal: Pick,
    layer_full: SlingLayer,
    layer_minimal: SlingLayer,
    layer_legacy_spreader_wll: SlingLayer,
    spreader_full: SavedSpreader,
    spreader_minimal: SavedSpreader,
    mat_full: SavedMat,
    mat_minimal: SavedMat,
    mat_analysis: MatAnalysis,
}

fn layers_fixture() -> LayersFixture {
    let project_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let pick_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
    let spreader_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let mat_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
    let analysis_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();

    LayersFixture {
        pick_full: Pick {
            id: pick_id,
            project_id,
            name: "Main hoist".into(),
            weight_lbs: 10_000.0,
        },
        pick_minimal: Pick {
            id: Uuid::parse_str("22222222-2222-4222-8222-222222222200").unwrap(),
            project_id: Uuid::nil(),
            name: "Orphan".into(),
            weight_lbs: 0.0,
        },
        layer_full: SlingLayer {
            pick_id,
            layer_index: 0,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 60.0,
            sling_count: 2,
            sling_length_ft: 12.0,
            apex_shackle: Some("3/4".into()),
            leg_shackle: Some("5/8".into()),
            spreader_id: Some(spreader_id),
            spreader_wll_lbs: None,
            pick_spacing_ft: Some(8.0),
            pick_width_ft: Some(4.0),
            spreader_span_ft: Some(10.0),
            tare_lbs: 25.0,
        },
        layer_minimal: SlingLayer {
            pick_id,
            layer_index: 1,
            size: 3,
            hitch: Hitch::Choker,
            angle_deg: 90.0,
            sling_count: 1,
            sling_length_ft: 0.0,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: None,
            pick_spacing_ft: None,
            pick_width_ft: None,
            spreader_span_ft: None,
            tare_lbs: 0.0,
        },
        layer_legacy_spreader_wll: SlingLayer {
            pick_id,
            layer_index: 2,
            size: 7,
            hitch: Hitch::Basket,
            angle_deg: 45.0,
            sling_count: 2,
            sling_length_ft: 8.0,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: Some(20_000),
            pick_spacing_ft: Some(6.0),
            pick_width_ft: None,
            spreader_span_ft: None,
            tare_lbs: 0.0,
        },
        spreader_full: SavedSpreader {
            id: spreader_id,
            manufacturer: "Lift-All".into(),
            model: "SB-20".into(),
            wll_lbs: 40_000,
            weight_lbs: 280.0,
            span_ft: Some(10.0),
            notes: "shop bar".into(),
        },
        spreader_minimal: SavedSpreader {
            id: Uuid::parse_str("33333333-3333-4333-8333-333333333300").unwrap(),
            manufacturer: "Generic".into(),
            model: "Beam".into(),
            wll_lbs: 10_000,
            weight_lbs: 0.0,
            span_ft: None,
            notes: String::new(),
        },
        mat_full: SavedMat {
            id: mat_id,
            manufacturer: "Duerr".into(),
            model: "D8".into(),
            length_ft: 8.0,
            width_ft: 4.0,
            thickness_in: 6.0,
            weight_lbs: 1_200.0,
            manufacturer_allowable_lbs: 100_000.0,
            notes: "yard stock".into(),
        },
        mat_minimal: SavedMat {
            id: Uuid::parse_str("44444444-4444-4444-8444-444444444400").unwrap(),
            manufacturer: "Acme".into(),
            model: "M4".into(),
            length_ft: 4.0,
            width_ft: 4.0,
            thickness_in: 4.0,
            weight_lbs: 0.0,
            manufacturer_allowable_lbs: 0.0,
            notes: String::new(),
        },
        mat_analysis: MatAnalysis {
            id: analysis_id,
            project_id,
            name: "Outrigger NE".into(),
            mat_id,
            outrigger_load_lbs: 50_000.0,
            pad_length_in: 24.0,
            pad_width_in: 24.0,
            allowable_psf: 3_000.0,
            spread_angle_deg: 45.0,
        },
    }
}

#[test]
fn layers_v1_roundtrip_and_golden() {
    let fixture = layers_fixture();
    let json = pretty(&fixture);

    // Deserialize → re-serialize must match (stable serde shape).
    let parsed: LayersFixture = serde_json::from_str(&json).expect("deserialize fixture");
    let again = pretty(&parsed);
    assert_eq!(json, again, "serde roundtrip changed JSON");

    let path = fixtures_dir().join("layers_v1.json");
    read_or_write(&path, &json);
}

// --- calc_package_v1: assembled DTO for sample picks (printed_at blanked) ---

fn calc_package_fixture() -> CalcPackage {
    let project_id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
    let mut project = Project::new("Golden Project");
    project.id = project_id;
    project.updated_at = 1_700_000_000_000;

    let spreader_id = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap();
    let mut bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
    bar.id = spreader_id;
    bar.span_ft = Some(10.0);

    // 1. Single layer, manual angle (no spacing → angle not calculated).
    let pick1_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-ccccccccccc1").unwrap();
    let pick1 = Pick {
        id: pick1_id,
        project_id,
        name: "Single manual".into(),
        weight_lbs: 10_000.0,
    };
    let layer1 = SlingLayer {
        pick_id: pick1_id,
        layer_index: 0,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 60.0,
        sling_count: 2,
        sling_length_ft: 10.0,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: None,
        spreader_wll_lbs: None,
        pick_spacing_ft: None,
        pick_width_ft: None,
        spreader_span_ft: None,
        tare_lbs: 0.0,
    };

    // 2. Two-layer with spreader span and calculated angles.
    let pick2_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-ccccccccccc2").unwrap();
    let pick2 = Pick {
        id: pick2_id,
        project_id,
        name: "Two-layer spreader".into(),
        weight_lbs: 20_000.0,
    };
    // Top layer: 2 slings over 10 ft span → reach 5 ft each, L=12 → calculated angle.
    let l2_top = SlingLayer {
        pick_id: pick2_id,
        layer_index: 0,
        size: 7,
        hitch: Hitch::Vertical,
        angle_deg: 90.0,
        sling_count: 2,
        sling_length_ft: 12.0,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: Some(spreader_id),
        spreader_wll_lbs: None,
        pick_spacing_ft: None,
        pick_width_ft: None,
        spreader_span_ft: Some(10.0),
        tare_lbs: 0.0,
    };
    let l2_bot = SlingLayer {
        pick_id: pick2_id,
        layer_index: 1,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 90.0,
        sling_count: 2,
        sling_length_ft: 10.0,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: None,
        spreader_wll_lbs: None,
        pick_spacing_ft: Some(10.0),
        pick_width_ft: None,
        spreader_span_ft: None,
        tare_lbs: 0.0,
    };

    // 3. Impossible geometry (sling shorter than reach) → GEOM.
    let pick3_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-ccccccccccc3").unwrap();
    let pick3 = Pick {
        id: pick3_id,
        project_id,
        name: "Impossible geom".into(),
        weight_lbs: 5_000.0,
    };
    let layer3 = SlingLayer {
        pick_id: pick3_id,
        layer_index: 0,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 60.0,
        sling_count: 2,
        sling_length_ft: 10.0,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: None,
        spreader_wll_lbs: None,
        pick_spacing_ft: Some(30.0),
        pick_width_ft: None,
        spreader_span_ft: None,
        tare_lbs: 0.0,
    };

    let mat_id = Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap();
    let mut mat = SavedMat::new("Acme", "M8", 8.0, 4.0, 6.0);
    mat.id = mat_id;
    mat.weight_lbs = 1_200.0;
    mat.manufacturer_allowable_lbs = 100_000.0;

    let analysis = MatAnalysis {
        id: Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap(),
        project_id,
        name: "Outrigger NE".into(),
        mat_id,
        outrigger_load_lbs: 50_000.0,
        pad_length_in: 24.0,
        pad_width_in: 24.0,
        allowable_psf: 3_000.0,
        spread_angle_deg: 45.0,
    };

    let mut pkg = assemble_calc_package(
        &project,
        &[
            (pick1, vec![layer1]),
            (pick2, vec![l2_top, l2_bot]),
            (pick3, vec![layer3]),
        ],
        &[bar],
        &[(analysis, Some(mat))],
    );
    pkg.printed_at.clear();
    pkg
}

#[test]
fn calc_package_v1_golden() {
    let pkg = calc_package_fixture();
    assert_eq!(pkg.picks.len(), 3);
    assert_eq!(pkg.mats.len(), 1);
    // Manual angle path: no calc suffix expected in config for pick 1.
    assert!(pkg.picks[0].warn.is_none());
    // Calculated angles on pick 2.
    assert!(
        pkg.picks[1].rows.iter().any(|r| r.config.contains("calc")),
        "expected calculated angle on two-layer pick: {:?}",
        pkg.picks[1].rows
    );
    // GEOM on impossible geometry.
    assert!(
        pkg.picks[2]
            .rows
            .iter()
            .any(|r| r.status == "GEOM" || r.status_kind == "over"),
        "expected GEOM on impossible pick: {:?}",
        pkg.picks[2].rows
    );

    let json = pretty(&pkg);
    let path = fixtures_dir().join("calc_package_v1.json");
    read_or_write(&path, &json);
}
