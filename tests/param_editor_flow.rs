//! §8.4 editor checklist as headless tests (step 2.5.19).
//!
//! Each test mirrors one manual step of docs/step-2.5-parser-param-table.md
//! §8.4 through `RigSession`, the same API the rig editor calls.

use bth_rigging::domain::{Hitch, Pick, Project, SlingLayer};
use bth_rigging::rig::edit::{ParamEdit, UseSite};
use bth_rigging::rig::session::RigSession;
use bth_rigging::rig::template::{from_layers, tensions_match_layers};
use bth_rigging::rig::{ParamId, ParamSource, Rig, RigErrorKind, duplo10};
use bth_rigging::store::RiggingStore;
use uuid::Uuid;

fn session() -> RigSession {
    RigSession::new(duplo10())
}

fn id(session: &RigSession, name: &str) -> ParamId {
    session.rig().param_named(name).unwrap().id
}

fn set(session: &mut RigSession, name: &str, text: &str) {
    let id = id(session, name);
    session
        .apply(ParamEdit::SetValue {
            id,
            text: text.into(),
        })
        .unwrap_or_else(|error| panic!("{name} = {text}: {}", error.message));
}

fn value_text(rig: &Rig, name: &str) -> String {
    rig.param_named(name)
        .unwrap()
        .expr
        .as_ref()
        .unwrap()
        .to_text(&rig.params)
}

#[test]
fn step_1_duplo10_opens_provisional() {
    let session = session();
    assert_eq!(session.rig().params.len(), 22);
    let headline = session.snapshot().headline;
    assert!(
        (headline.hook_load_lbs - 122_983.0).abs() <= 0.5,
        "hook load {}",
        headline.hook_load_lbs
    );
    assert!(headline.assumed_count > 0);
    assert!(!session.snapshot().headline_is_last_good);
}

#[test]
fn step_2_s12_edit_moves_the_governing_angle() {
    let mut session = session();
    let before = session.snapshot().headline;
    set(&mut session, "s12", "8'-6\"");
    assert_eq!(session.rig().param_named("s12").unwrap().nominal, 8.5);
    let after = session.snapshot().headline;
    assert!(
        (after.governing_angle_deg - before.governing_angle_deg).abs() > 0.1,
        "governing angle {} -> {}",
        before.governing_angle_deg,
        after.governing_angle_deg
    );
    let delta = session.delta().unwrap();
    assert!(delta.governing_angle_deg.abs() > 0.1, "{delta:?}");
}

#[test]
fn step_3_derived_value_reprints_canonically() {
    let mut session = session();
    set(&mut session, "s12", "s23 + 6 in");
    let s12 = session.rig().param_named("s12").unwrap();
    assert_eq!(s12.source, ParamSource::Derived);
    assert_eq!(value_text(session.rig(), "s12"), "s23 + 0.5");
}

#[test]
fn step_4_cycle_is_refused_with_readable_path() {
    let mut session = session();
    set(&mut session, "s12", "s23 + 6 in");
    let before = session.rig().clone();
    let s23 = id(&session, "s23");
    let error = session
        .apply(ParamEdit::SetValue {
            id: s23,
            text: "s12".into(),
        })
        .unwrap_err();
    assert!(
        error.message.contains("s23 → s12 → s23"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("ParamCycle"), "{}", error.message);
    assert_eq!(session.rig(), &before);
}

#[test]
fn step_5_quantity_mismatch_points_at_the_operator() {
    let mut session = session();
    let before = session.rig().clone();
    let span_b = id(&session, "span_B");
    let text = "span_A + load_weight";
    let error = session
        .apply(ParamEdit::SetValue {
            id: span_b,
            text: text.into(),
        })
        .unwrap_err();
    let parse = error.parse.expect("parse error");
    assert_eq!(&text[parse.span.start..parse.span.end], "+");
    assert_eq!(session.rig(), &before);
}

#[test]
fn step_6_rename_reprints_dependents() {
    let mut session = session();
    set(&mut session, "s12", "s23 + 6 in");
    let s23 = id(&session, "s23");
    session
        .apply(ParamEdit::Rename {
            id: s23,
            name: "span_23".into(),
        })
        .unwrap();
    assert_eq!(value_text(session.rig(), "s12"), "span_23 + 0.5");
}

#[test]
fn step_7_delete_in_use_lists_node_coordinates() {
    let mut session = session();
    let before = session.rig().clone();
    let gauge = id(&session, "lug_gauge_G");
    let error = session.apply(ParamEdit::Delete { id: gauge }).unwrap_err();
    assert!(!error.uses.is_empty());
    assert!(
        error
            .uses
            .iter()
            .any(|site| matches!(site, UseSite::NodeCoord { .. })),
        "{:?}",
        error.uses
    );
    assert_eq!(session.rig(), &before);
}

#[test]
fn step_8_undo_restores_and_store_round_trips() {
    let mut session = session();
    let original = session.rig().clone();
    set(&mut session, "s12", "9 ft");
    set(&mut session, "load_weight", "118300 lb");
    set(&mut session, "span_B", "13 ft");
    assert!(session.undo());
    assert!(session.undo());
    assert!(session.undo());
    assert_eq!(session.rig(), &original);
    assert!(!session.is_dirty());

    set(&mut session, "s12", "8'-6\"");
    let dir = std::env::temp_dir().join(format!("bth-rigging-flow-{}", Uuid::new_v4()));
    let store = RiggingStore::open(&dir).unwrap();
    let project = Project::new("Flow");
    store.save_project(&project).unwrap();
    let mut rig = session.rig().clone();
    rig.project_id = project.id;
    store.save_rig(&rig).unwrap();
    drop(store);
    let reloaded = RiggingStore::open(&dir)
        .unwrap()
        .load_rig(rig.id)
        .unwrap()
        .unwrap();
    assert_eq!(reloaded, rig);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn step_9_rig_from_pick_matches_layers_hook_load() {
    let pick = Pick::new(Uuid::nil(), "Flow pick", 10_000.0);
    let layers = [SlingLayer {
        pick_id: pick.id,
        layer_index: 0,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 60.0,
        sling_count: 2,
        sling_length_ft: 10.0,
        pick_spacing_ft: Some(10.0),
        pick_width_ft: None,
        spreader_span_ft: None,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: None,
        spreader_wll_lbs: None,
        tare_lbs: 0.0,
    }];
    let session = RigSession::new(from_layers(&pick, &layers, &[]));
    let solved = session
        .snapshot()
        .solved
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    tensions_match_layers(session.rig(), solved, &pick, &layers, &[], 1e-6).unwrap();
    assert_eq!(
        session.snapshot().headline.hook_load_lbs,
        solved.hook_load_lbs
    );
}

#[test]
fn step_10_fractional_inches() {
    let mut session = session();
    set(&mut session, "s12", "8'-6 1/2\"");
    let nominal = session.rig().param_named("s12").unwrap().nominal;
    assert!((nominal - (8.0 + 6.5 / 12.0)).abs() < 1e-12, "{nominal}");
}

#[test]
fn step_11_non_convergence_keeps_last_good_then_undo_is_clean() {
    let mut session = session();
    let headline = session.snapshot().headline;
    set(&mut session, "s12", "17 ft");
    let snapshot = session.snapshot();
    assert!(snapshot.headline_is_last_good);
    assert_eq!(snapshot.headline, headline);
    assert!(snapshot.rated.is_none());
    let errors = snapshot.solved.as_ref().unwrap().as_ref().unwrap_err();
    assert_eq!(errors[0].code, RigErrorKind::SolveNotConverged);
    assert!(errors[0].message.contains("solver did not converge"));

    assert!(session.undo());
    let snapshot = session.snapshot();
    assert!(!snapshot.headline_is_last_good);
    assert!(snapshot.solved.as_ref().unwrap().is_ok());
    assert_eq!(snapshot.headline, headline);
    assert!(!session.is_dirty());
}
