//! Step 3 tension equivalence vs the layer engine (decision D2-9 pattern: lives in tests/).
//!
//! Statics at the descent pose must match `layers::calculate_pick` to 1e-9
//! (relative) wherever the layer model is exact; the full hang to 1e-6
//! (inextensible members stretch ~1e-9 of their length under load).

use bth_rigging::domain::{Hitch, Pick, SavedSpreader, SlingLayer};
use bth_rigging::rig::eval::EvalRig;
use bth_rigging::rig::solve::{solve_eval, statics_at};
use bth_rigging::rig::template::{TensionGate, from_layers, tensions_match_layers};
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

fn gate(pick: &Pick, layers: &[SlingLayer], spreaders: &[SavedSpreader]) -> TensionGate {
    let rig = from_layers(pick, layers, spreaders);
    let ev = EvalRig::evaluate(&rig).unwrap_or_else(|e| panic!("{}: eval {e:?}", pick.name));
    let hung = solve_eval(&rig, &ev).unwrap_or_else(|e| panic!("{}: hang {e:?}", pick.name));
    assert!(hung.converged, "{}: hang did not converge", pick.name);
    let h = tensions_match_layers(&rig, &hung, pick, layers, spreaders, 1e-6)
        .unwrap_or_else(|e| panic!("{} hang: {e}", pick.name));
    // A tare layer is hung from one pick by the template, so the descent pose
    // is not an equilibrium; only the hang is gated for it.
    if layers.iter().any(|l| l.tare_lbs.abs() > 1e-15) {
        return h;
    }
    let st = statics_at(&rig, &ev).unwrap_or_else(|e| panic!("{}: statics {e:?}", pick.name));
    assert!(
        st.determinacy.consistent,
        "{}: {:?}",
        pick.name, st.determinacy
    );
    let g = tensions_match_layers(&rig, &st, pick, layers, spreaders, 1e-9)
        .unwrap_or_else(|e| panic!("{} statics: {e}", pick.name));
    assert_eq!(g.compared, h.compared, "{}", pick.name);
    g
}

#[test]
fn single_layer_two_leg() {
    let pick = Pick::new(Uuid::nil(), "Single", 10_000.0);
    let mut l = layer(Hitch::Vertical, 60.0, 2, 5);
    l.sling_length_ft = 10.0;
    l.pick_spacing_ft = Some(10.0);
    let g = gate(&pick, &[l], &[]);
    assert_eq!(g.compared, vec![0]);
}

#[test]
fn two_leg_with_shackles() {
    let pick = Pick::new(Uuid::nil(), "Shackles", 12_000.0);
    let mut l = layer(Hitch::Vertical, 60.0, 2, 7);
    l.sling_length_ft = 12.0;
    l.pick_spacing_ft = Some(12.0);
    l.apex_shackle = Some("1-1/4".into());
    l.leg_shackle = Some("1".into());
    let g = gate(&pick, &[l], &[]);
    assert_eq!(g.compared, vec![0]);
}

#[test]
fn basket_hitch_layer() {
    let pick = Pick::new(Uuid::nil(), "Basket", 8_000.0);
    let mut l = layer(Hitch::Basket, 60.0, 2, 5);
    l.sling_length_ft = 10.0;
    l.pick_spacing_ft = Some(8.0);
    let g = gate(&pick, &[l], &[]);
    // The template draws a basket sling as one straight member; hook load is
    // still gated, per-leg tension is not comparable.
    assert_eq!(g.skipped, vec![(0, "basket drawn as single legs")]);
}

#[test]
fn two_layer_spreader_vertical_top() {
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
    let g = gate(&pick, &[top, bot], &[bar]);
    assert_eq!(g.compared, vec![0, 1], "{g:?}");
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
    let g = gate(&pick, &[l1, l2], &[bar]);
    assert_eq!(g.compared, vec![0, 1], "{g:?}");
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
    l2.pick_spacing_ft = Some(6.0);
    l2.pick_width_ft = Some(4.0);
    let pick = Pick::new(Uuid::nil(), "tree", 10_000.0);
    let g = gate(&pick, &[l1, l2], &[bar]);
    assert_eq!(g.compared, vec![0, 1], "{g:?}");
}

#[test]
fn three_in_line_is_skipped_as_unequal() {
    let pick = Pick::new(Uuid::nil(), "three-line", 8_000.0);
    let mut l = layer(Hitch::Vertical, 60.0, 3, 5);
    l.sling_length_ft = 10.0;
    l.pick_spacing_ft = Some(5.0);
    let g = gate(&pick, &[l], &[]);
    assert!(g.compared.is_empty(), "{g:?}");
}

#[test]
fn tare_layer_gates_hook_load_and_other_layers() {
    let mut bar = SavedSpreader::new("Test", "Bar", 40_000, 300.0);
    bar.span_ft = Some(10.0);
    let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
    l1.sling_length_ft = 12.0;
    l1.spreader_id = Some(bar.id);
    l1.spreader_span_ft = Some(10.0);
    let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
    l2.layer_index = 1;
    l2.sling_length_ft = 8.0;
    l2.tare_lbs = 20.0;
    l2.pick_spacing_ft = Some(6.0);
    l2.pick_width_ft = Some(4.0);
    let pick = Pick::new(Uuid::nil(), "tare", 10_000.0);
    let g = gate(&pick, &[l1, l2], &[bar]);
    // Hook load is gated; the off-centre tare tips the hung rig a hair, so
    // the top slings are no longer at equal angles either.
    assert_eq!(
        g.skipped,
        vec![(0, "unequal sling angles"), (1, "tare hung at one pick")]
    );
}
