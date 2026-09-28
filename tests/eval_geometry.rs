//! Step 2 geometry equivalence vs `layers::geometry` (decision D2-9: lives in tests/).

use bth_rigging::domain::{Hitch, Pick, SavedSpreader, SlingLayer};
use bth_rigging::rig::eval::EvalRig;
use bth_rigging::rig::template::{from_layers, geometry_match_layers};
use uuid::Uuid;

fn layer(hitch: Hitch, angle: f64, count: u32, size: u8) -> SlingLayer {
    SlingLayer {
        pick_id: Uuid::nil(),
        layer_index: 0,
        size,
        hitch,
        angle_deg: angle,
        sling_count: count,
        sling_length_ft: 0.0,
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

fn check(pick: &Pick, layers: &[SlingLayer], spreaders: &[SavedSpreader]) {
    let rig = from_layers(pick, layers, spreaders);
    rig.validate()
        .unwrap_or_else(|e| panic!("{}: graph invalid: {e:?}", pick.name));
    let ev = EvalRig::evaluate(&rig).unwrap_or_else(|e| panic!("{}: eval: {e:?}", pick.name));
    geometry_match_layers(&rig, &ev, layers, spreaders)
        .unwrap_or_else(|e| panic!("{}: {e}", pick.name));
}

#[test]
fn print_sample_single_manual() {
    let pick = Pick::new(Uuid::nil(), "Single manual", 10_000.0);
    let mut l = layer(Hitch::Vertical, 60.0, 2, 5);
    l.sling_length_ft = 10.0;
    check(&pick, &[l], &[]);
}

#[test]
fn print_sample_two_layer_spreader() {
    let mut bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
    bar.span_ft = Some(10.0);
    let pick = Pick::new(Uuid::nil(), "Two-layer spreader", 20_000.0);
    let mut top = layer(Hitch::Vertical, 90.0, 2, 7);
    top.sling_length_ft = 12.0;
    top.spreader_id = Some(bar.id);
    top.spreader_span_ft = Some(10.0);
    let mut bot = layer(Hitch::Vertical, 90.0, 2, 5);
    bot.layer_index = 1;
    bot.sling_length_ft = 10.0;
    bot.pick_spacing_ft = Some(10.0);
    check(&pick, &[top, bot], &[bar]);
}

#[test]
fn print_sample_impossible_geom() {
    let pick = Pick::new(Uuid::nil(), "Impossible geom", 5_000.0);
    let mut l = layer(Hitch::Vertical, 60.0, 2, 5);
    l.sling_length_ft = 10.0;
    l.pick_spacing_ft = Some(30.0);
    check(&pick, &[l], &[]);
}

#[test]
fn db_fixture_pick() {
    let mut bar = SavedSpreader::new("Acme", "SB-20", 20_000, 180.0);
    bar.span_ft = Some(8.0);
    let pick = Pick::new(Uuid::nil(), "Pick 1", 10_000.0);
    let l = SlingLayer {
        pick_id: pick.id,
        layer_index: 0,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 60.0,
        sling_count: 2,
        sling_length_ft: 12.0,
        pick_spacing_ft: Some(8.0),
        pick_width_ft: Some(6.0),
        spreader_span_ft: Some(12.0),
        apex_shackle: Some("1".into()),
        leg_shackle: Some("3/4".into()),
        spreader_id: Some(bar.id),
        spreader_wll_lbs: None,
        tare_lbs: 25.0,
    };
    check(&pick, &[l], &[bar]);
}

#[test]
fn duplo10ish_two_over_four() {
    let bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
    let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
    l1.spreader_id = Some(bar.id);
    l1.apex_shackle = Some("1-1/4".into());
    let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
    l2.layer_index = 1;
    l2.tare_lbs = 50.0;
    let pick = Pick::new(Uuid::nil(), "2-over-4", 10_000.0);
    check(&pick, &[l1, l2], &[bar]);
}

#[test]
fn two_over_four_rectangle_calculated() {
    let mut bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
    bar.span_ft = Some(12.0);
    let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
    l1.sling_length_ft = 12.0;
    l1.spreader_id = Some(bar.id);
    l1.spreader_span_ft = Some(12.0);
    let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
    l2.layer_index = 1;
    l2.sling_length_ft = 10.0;
    l2.pick_spacing_ft = Some(8.0);
    l2.pick_width_ft = Some(6.0);
    let pick = Pick::new(Uuid::nil(), "2-over-4 calculated", 10_000.0);
    check(&pick, &[l1, l2], &[bar]);
}

#[test]
fn legacy_zero_sling_length() {
    let pick = Pick::new(Uuid::nil(), "legacy", 10_000.0);
    let l = layer(Hitch::Vertical, 60.0, 2, 5);
    assert_eq!(l.sling_length_ft, 0.0);
    check(&pick, &[l], &[]);
}

#[test]
fn corpus_with_shackles_and_lengths() {
    let mut bar = SavedSpreader::new("Test", "Bar", 40_000, 300.0);
    bar.span_ft = Some(10.0);
    let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
    l1.sling_length_ft = 12.0;
    l1.apex_shackle = Some("1-1/2".into());
    l1.leg_shackle = Some("1".into());
    l1.spreader_id = Some(bar.id);
    l1.spreader_span_ft = Some(10.0);
    let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
    l2.layer_index = 1;
    l2.sling_length_ft = 8.0;
    l2.apex_shackle = Some("3/4".into());
    l2.leg_shackle = Some("5/8".into());
    l2.tare_lbs = 20.0;
    l2.pick_spacing_ft = Some(6.0);
    l2.pick_width_ft = Some(4.0);
    let pick = Pick::new(Uuid::nil(), "tree", 10_000.0);
    check(&pick, &[l1, l2], &[bar]);
}

#[test]
fn three_in_line_flags_legs() {
    let pick = Pick::new(Uuid::nil(), "three-line", 8_000.0);
    let mut l = layer(Hitch::Vertical, 60.0, 3, 5);
    l.sling_length_ft = 10.0;
    l.pick_spacing_ft = Some(5.0);
    check(&pick, &[l], &[]);
}

#[test]
fn two_leg_sixty_degrees() {
    let pick = Pick::new(Uuid::nil(), "60deg", 10_000.0);
    let mut l = layer(Hitch::Vertical, 45.0, 2, 5);
    l.sling_length_ft = 12.0;
    l.pick_spacing_ft = Some(12.0);
    check(&pick, &[l], &[]);
}
