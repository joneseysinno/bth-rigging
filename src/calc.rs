//! Sling tension and hardware reaction checks (bottom-up load chain).
//!
//! Rigging self-weight (slings, shackles, spreader bars, other tare) accumulates
//! layer by layer from the payload up to the hook.

use std::collections::HashMap;

use uuid::Uuid;

use crate::catalog::{RoundSlingRating, find_by_size};
use crate::geometry::{
    LayerGeometry, endpoints_below_count, governing_angle, resolve_geometry, rigging_height_ft,
};
use crate::hardware::find_shackle;
use crate::models::{Hitch, SavedSpreader, SlingLayer};

/// Pass/fail check for one hardware component.
#[derive(Debug, Clone)]
pub struct HardwareCheck {
    pub name: String,
    pub reaction_lbs: f64,
    pub wll_lbs: u32,
    pub overloaded: bool,
}

impl HardwareCheck {
    pub fn utilization_pct(&self) -> f64 {
        if self.wll_lbs == 0 {
            f64::INFINITY
        } else {
            (self.reaction_lbs / f64::from(self.wll_lbs)) * 100.0
        }
    }
}

/// Resolved spreader used on a layer (catalog or legacy WLL-only).
#[derive(Debug, Clone)]
pub struct SpreaderInfo {
    pub name: String,
    pub wll_lbs: u32,
    pub weight_lbs: f64,
}

/// Self-weight of the rigging that belongs to one layer (lb).
///
/// Load path inside a layer, top → bottom:
/// apex shackle(s) → slings → leg shackles → spreader bar → (next layer / payload).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RiggingWeight {
    /// All slings on the layer: count × length × lb/ft.
    pub slings_lbs: f64,
    /// Leg (load-end) shackles: one per sling.
    pub leg_shackles_lbs: f64,
    /// Spreader bar self-weight.
    pub spreader_lbs: f64,
    /// User-entered other tare (hooks, links, below-the-hook devices, …).
    pub other_tare_lbs: f64,
    /// Apex shackles: one at the hook (L1) or one per parent endpoint.
    pub apex_shackles_lbs: f64,
    pub apex_shackle_count: u32,
    pub leg_shackle_count: u32,
}

impl RiggingWeight {
    /// Rigging weight hanging from the slings (everything except apex shackles).
    pub fn below_apex_lbs(&self) -> f64 {
        self.slings_lbs + self.leg_shackles_lbs + self.spreader_lbs + self.other_tare_lbs
    }

    /// Total rigging weight this layer adds to the load chain.
    pub fn total_lbs(&self) -> f64 {
        self.below_apex_lbs() + self.apex_shackles_lbs
    }

    /// Short human-readable breakdown, omitting zero items.
    pub fn breakdown_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |label: String, v: f64| {
            if v > 0.0 {
                out.push(format!("{label}: {}", fmt_lb(v)));
            }
        };
        push("Slings".into(), self.slings_lbs);
        push(
            format!("Leg shackles ×{}", self.leg_shackle_count),
            self.leg_shackles_lbs,
        );
        push("Spreader".into(), self.spreader_lbs);
        push("Other tare".into(), self.other_tare_lbs);
        push(
            format!("Apex shackles ×{}", self.apex_shackle_count),
            self.apex_shackles_lbs,
        );
        out
    }
}

fn fmt_lb(v: f64) -> String {
    if v >= 100.0 {
        format!("{:.0} lb", v)
    } else {
        format!("{:.1} lb", v)
    }
}

/// Result of tension + hardware calculation for one layer.
#[derive(Debug, Clone)]
pub struct LayerTension {
    /// Load hanging from the bottom of this layer's rigging
    /// (payload on the last layer, otherwise everything delivered up by the layer below).
    pub load_below_lbs: f64,
    /// Load this layer's slings carry: load below + this layer's slings, leg shackles,
    /// spreader, and other tare.
    pub carried_lbs: f64,
    /// Load hanging under the spreader (before adding bar self-weight).
    pub load_on_spreader_lbs: f64,
    /// Load this layer delivers upward (hook or parent endpoints) = carried + apex shackles.
    pub load_at_top_lbs: f64,
    /// Rigging self-weight added on this layer.
    pub rigging: RiggingWeight,
    /// Cumulative rigging weight from this layer down to the payload.
    pub cumulative_rigging_lbs: f64,
    /// True when no sling length was given, so sling self-weight is excluded.
    pub sling_length_missing: bool,
    /// Angle used for tension (degrees from horizontal).
    pub angle_deg: f64,
    /// True when the angle came from sling length + pick-point geometry.
    pub angle_calculated: bool,
    /// Pick-point geometry for this layer.
    pub geometry: LayerGeometry,
    pub tension_factor: f64,
    pub leg_count: u32,
    pub share_lbs: f64,
    pub tension_lbs: f64,
    pub hitch_wll_lbs: u32,
    pub utilization_pct: f64,
    pub overloaded: bool,
    pub angle_below_minimum: bool,
    pub rating: RoundSlingRating,
    pub hardware: Vec<HardwareCheck>,
    pub spreader: Option<SpreaderInfo>,
}

impl LayerTension {
    /// Sling status stamp `(label, kind)` where kind is `ok` / `warn` / `over`.
    pub fn status(&self) -> (&'static str, &'static str) {
        if self.geometry.error.is_some() {
            ("GEOM", "over")
        } else if self.overloaded {
            ("OVER", "over")
        } else if self.angle_below_minimum {
            ("ANGLE", "warn")
        } else if self.geometry.unequal_drop_in.is_some() {
            ("LEGS", "warn")
        } else if self.sling_length_missing {
            ("NO LEN", "warn")
        } else {
            ("OK", "ok")
        }
    }
}

/// Full pick result: layers top→bottom plus hook load.
#[derive(Debug, Clone)]
pub struct PickResult {
    pub payload_lbs: f64,
    /// Total rigging self-weight across all layers.
    pub rigging_weight_lbs: f64,
    /// Total at the hook = payload + all rigging weight.
    pub hook_load_lbs: f64,
    /// Approximate hook-to-pick-point rigging height (ft) when every layer's
    /// geometry is calculated. Excludes shackle and spreader depth.
    pub rigging_height_ft: Option<f64>,
    pub layers: Vec<LayerTension>,
}

/// Tension factor = 1 / sin(θ) where θ is degrees from horizontal.
pub fn tension_factor(angle_deg: f64) -> f64 {
    let rad = angle_deg.to_radians();
    let s = rad.sin();
    if s.abs() < 1e-12 {
        f64::INFINITY
    } else {
        1.0 / s
    }
}

/// Number of load-sharing legs for the hitch and sling count.
/// Basket: each sling contributes two legs.
pub fn leg_count(hitch: Hitch, sling_count: u32) -> u32 {
    let n = sling_count.max(1);
    match hitch {
        Hitch::Vertical | Hitch::Choker => n,
        Hitch::Basket => 2 * n,
    }
}

/// Number of apex shackles on a layer: one at the hook for L1,
/// otherwise one at each endpoint of the parent layer.
pub fn apex_shackle_count(layer_index: usize, parent_endpoint_count: Option<u32>) -> u32 {
    if layer_index == 0 {
        1
    } else {
        parent_endpoint_count.unwrap_or(1).max(1)
    }
}

fn check(name: impl Into<String>, reaction_lbs: f64, wll_lbs: u32) -> HardwareCheck {
    HardwareCheck {
        name: name.into(),
        reaction_lbs,
        wll_lbs,
        overloaded: !reaction_lbs.is_finite() || reaction_lbs > f64::from(wll_lbs),
    }
}

fn resolve_spreader(
    layer: &SlingLayer,
    catalog: &HashMap<Uuid, SavedSpreader>,
) -> Option<SpreaderInfo> {
    if let Some(id) = layer.spreader_id {
        if let Some(sp) = catalog.get(&id) {
            return Some(SpreaderInfo {
                name: sp.label(),
                wll_lbs: sp.wll_lbs,
                weight_lbs: sp.weight_lbs,
            });
        }
    }
    // Legacy unnamed bar: WLL only, zero self-weight.
    layer.spreader_wll_lbs.map(|wll| SpreaderInfo {
        name: format!("Spreader ({wll} lb WLL)"),
        wll_lbs: wll,
        weight_lbs: 0.0,
    })
}

/// Self-weight of the rigging on one layer.
pub fn layer_rigging_weight(
    layer: &SlingLayer,
    rating: &RoundSlingRating,
    spreader: Option<&SpreaderInfo>,
    apex_count: u32,
) -> RiggingWeight {
    let slings = layer.sling_count.max(1);
    let slings_lbs = f64::from(slings) * rating.sling_weight_lbs(layer.sling_length_ft);

    let (leg_shackle_count, leg_shackles_lbs) =
        match layer.leg_shackle.as_deref().and_then(find_shackle) {
            Some(sh) => (slings, f64::from(slings) * sh.weight_lbs),
            None => (0, 0.0),
        };

    let (apex_shackle_count, apex_shackles_lbs) =
        match layer.apex_shackle.as_deref().and_then(find_shackle) {
            Some(sh) => (apex_count, f64::from(apex_count) * sh.weight_lbs),
            None => (0, 0.0),
        };

    let other = layer.tare_lbs;
    RiggingWeight {
        slings_lbs,
        leg_shackles_lbs,
        spreader_lbs: spreader.map(|s| s.weight_lbs.max(0.0)).unwrap_or(0.0),
        other_tare_lbs: if other.is_finite() {
            other.max(0.0)
        } else {
            0.0
        },
        apex_shackles_lbs,
        apex_shackle_count,
        leg_shackle_count,
    }
}

/// Compute one layer for the load hanging below it.
fn calculate_layer_at(
    load_below: f64,
    angle_deg: f64,
    layer: &SlingLayer,
    layer_index: usize,
    parent_endpoint_count: Option<u32>,
    spreader: Option<SpreaderInfo>,
) -> Option<LayerTension> {
    let rating = find_by_size(layer.size)?.clone();
    let apex_count = apex_shackle_count(layer_index, parent_endpoint_count);
    let rigging = layer_rigging_weight(layer, &rating, spreader.as_ref(), apex_count);

    // Spreader carries only what hangs under it; its self-weight goes to the slings.
    let load_on_spreader = load_below;
    // Slings carry everything below plus this layer's own rigging (conservatively
    // the full sling self-weight is taken at the sling).
    let sling_carried = load_below + rigging.below_apex_lbs();
    let load_at_top = sling_carried + rigging.apex_shackles_lbs;

    let factor = tension_factor(angle_deg);
    let legs = leg_count(layer.hitch, layer.sling_count);
    let share = sling_carried / f64::from(legs);
    let tension = share * factor;
    let hitch_wll = rating.hitch_wll_lbs(layer.hitch);
    let utilization = if hitch_wll == 0 {
        f64::INFINITY
    } else {
        (tension / f64::from(hitch_wll)) * 100.0
    };

    let mut hardware = Vec::new();

    if let Some(key) = layer.apex_shackle.as_deref() {
        if let Some(sh) = find_shackle(key) {
            // Single hook connection on L1; otherwise one apex shackle per parent endpoint.
            let reaction = sling_carried / f64::from(apex_count);
            hardware.push(check(
                format!("Apex shackle {}″", sh.size_in),
                reaction,
                sh.wll_lbs,
            ));
        }
    }

    if let Some(key) = layer.leg_shackle.as_deref() {
        if let Some(sh) = find_shackle(key) {
            hardware.push(check(
                format!("Leg shackle {}″", sh.size_in),
                tension,
                sh.wll_lbs,
            ));
        }
    }

    if let Some(ref sp) = spreader {
        let ends = layer.sling_count.max(2);
        let end_reaction = load_on_spreader / f64::from(ends);
        hardware.push(check(
            format!("Spreader ({})", sp.name),
            end_reaction,
            sp.wll_lbs,
        ));
    }

    Some(LayerTension {
        load_below_lbs: load_below,
        carried_lbs: sling_carried,
        load_on_spreader_lbs: load_on_spreader,
        load_at_top_lbs: load_at_top,
        rigging,
        cumulative_rigging_lbs: 0.0, // filled in by calculate_pick
        sling_length_missing: !(layer.sling_length_ft.is_finite() && layer.sling_length_ft > 0.0),
        angle_deg,
        angle_calculated: false, // filled in by calculate_pick
        geometry: LayerGeometry::default(),
        tension_factor: factor,
        leg_count: legs,
        share_lbs: share,
        tension_lbs: tension,
        hitch_wll_lbs: hitch_wll,
        utilization_pct: utilization,
        overloaded: !tension.is_finite() || tension > f64::from(hitch_wll),
        angle_below_minimum: angle_deg < 30.0,
        rating,
        hardware,
        spreader,
    })
}

/// Bottom-up pick calculation.
///
/// The payload hangs from the last layer. Walking up the tree, each layer adds its
/// own rigging self-weight (slings, leg shackles, spreader, other tare, apex shackles),
/// so every component is checked against the payload plus all rigging below it.
pub fn calculate_pick(
    payload_lbs: f64,
    layers: &[SlingLayer],
    spreaders: &[SavedSpreader],
) -> Option<PickResult> {
    if layers.is_empty() {
        return None;
    }

    let catalog: HashMap<Uuid, SavedSpreader> =
        spreaders.iter().cloned().map(|s| (s.id, s)).collect();

    let n = layers.len();
    let geoms = resolve_geometry(layers, spreaders);
    let mut results: Vec<Option<LayerTension>> = vec![None; n];
    let mut hanging = payload_lbs;
    let mut cumulative_rigging = 0.0;

    for i in (0..n).rev() {
        let layer = &layers[i];
        let spreader = resolve_spreader(layer, &catalog);

        let parent_ends = if i == 0 {
            None
        } else {
            Some(endpoints_below_count(&layers[i - 1]))
        };

        let (angle, calculated) = governing_angle(layer, geoms.get(i));
        let mut lt = calculate_layer_at(hanging, angle, layer, i, parent_ends, spreader)?;
        lt.angle_calculated = calculated;
        lt.geometry = geoms.get(i).cloned().unwrap_or_default();
        cumulative_rigging += lt.rigging.total_lbs();
        lt.cumulative_rigging_lbs = cumulative_rigging;
        hanging = lt.load_at_top_lbs;
        results[i] = Some(lt);
    }

    let layers_out: Vec<LayerTension> = results.into_iter().flatten().collect();
    if layers_out.len() != n {
        return None;
    }

    Some(PickResult {
        payload_lbs,
        rigging_weight_lbs: cumulative_rigging,
        hook_load_lbs: hanging,
        rigging_height_ft: rigging_height_ft(&geoms),
        layers: layers_out,
    })
}

/// Convenience: single layer carrying the payload (no tare / chain).
pub fn calculate_layer(weight_lbs: f64, layer: &SlingLayer) -> Option<LayerTension> {
    calculate_pick(weight_lbs, &[layer.clone()], &[])?
        .layers
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn factor_at_90_is_one() {
        assert!((tension_factor(90.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn factor_at_30_is_two() {
        assert!((tension_factor(30.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn factor_at_60_matches_wstda() {
        let f = tension_factor(60.0);
        assert!((f - 1.154_700_538).abs() < 1e-6);
    }

    #[test]
    fn ten_thousand_two_vertical_at_60() {
        let l = layer(Hitch::Vertical, 60.0, 2, 5);
        let r = calculate_layer(10_000.0, &l).expect("calc");
        assert!((r.tension_lbs - 5_773.502_691).abs() < 0.01);
        assert_eq!(r.leg_count, 2);
        assert!(!r.angle_below_minimum);
        assert!((r.carried_lbs - 10_000.0).abs() < 1e-9);
    }

    #[test]
    fn basket_doubles_legs() {
        let l = layer(Hitch::Basket, 90.0, 2, 2);
        let r = calculate_layer(10_000.0, &l).expect("calc");
        assert_eq!(r.leg_count, 4);
        assert!((r.tension_lbs - 2_500.0).abs() < 1e-9);
        assert_eq!(r.hitch_wll_lbs, 5_300);
        assert!(!r.overloaded);
    }

    #[test]
    fn choker_uses_choker_wll() {
        let l = layer(Hitch::Choker, 90.0, 1, 1);
        let r = calculate_layer(2_000.0, &l).expect("calc");
        assert_eq!(r.hitch_wll_lbs, 2_100);
        assert!(!r.overloaded);
        let overloaded = calculate_layer(2_200.0, &l).expect("calc");
        assert!(overloaded.overloaded);
    }

    #[test]
    fn warns_below_30_degrees() {
        let l = layer(Hitch::Vertical, 25.0, 2, 7);
        let r = calculate_layer(1_000.0, &l).expect("calc");
        assert!(r.angle_below_minimum);
    }

    #[test]
    fn hardware_checks_apex_leg_spreader() {
        let mut l = layer(Hitch::Vertical, 90.0, 2, 5);
        l.apex_shackle = Some("3/4".into()); // 9500 — overloaded for 10000
        l.leg_shackle = Some("5/8".into()); // 6500 — ok for 5000 tension
        l.spreader_wll_lbs = Some(10_000); // end = 5000 — ok

        let r = calculate_layer(10_000.0, &l).expect("calc");
        assert_eq!(r.hardware.len(), 3);
        assert!(r.hardware[0].overloaded); // apex
        assert!(!r.hardware[1].overloaded); // leg
        // Two 5/8 leg shackles (1.37 lb ea) now ride on the slings.
        assert!((r.hardware[1].reaction_lbs - 5_001.37).abs() < 1e-9);
        assert!(!r.hardware[2].overloaded); // spreader
        assert!((r.hardware[2].reaction_lbs - 5_000.0).abs() < 1e-9);
    }

    #[test]
    fn two_over_four_accumulates_spreader_weight() {
        let bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
        let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
        l1.layer_index = 0;
        l1.spreader_id = Some(bar.id);
        l1.apex_shackle = Some("1-1/4".into());

        let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
        l2.layer_index = 1;
        l2.tare_lbs = 50.0;

        let pick = calculate_pick(10_000.0, &[l1, l2], &[bar]).expect("pick");
        // L2: carried = payload + tare = 10050; no spreader on L2
        assert!((pick.layers[1].carried_lbs - 10_050.0).abs() < 1e-6);
        // L1: hanging from L2 (10050) + spreader 200 = 10250
        assert!((pick.layers[0].carried_lbs - 10_250.0).abs() < 1e-6);
        assert!((pick.layers[0].load_on_spreader_lbs - 10_050.0).abs() < 1e-6);
        // Hook adds the 1-1/4 apex shackle (9.50 lb).
        assert!((pick.hook_load_lbs - 10_259.5).abs() < 1e-6);
        assert!((pick.rigging_weight_lbs - 259.5).abs() < 1e-6);

        // Spreader end reaction: load under bar / 2 ends
        let sp_hw = pick.layers[0]
            .hardware
            .iter()
            .find(|h| h.name.starts_with("Spreader"))
            .expect("spreader check");
        assert!((sp_hw.reaction_lbs - 5_025.0).abs() < 1e-6);
    }

    #[test]
    fn layer2_apex_uses_per_end_share() {
        let mut l1 = layer(Hitch::Vertical, 90.0, 2, 7);
        l1.layer_index = 0;

        let mut l2 = layer(Hitch::Vertical, 90.0, 4, 5);
        l2.layer_index = 1;
        l2.apex_shackle = Some("5/8".into()); // 6500

        let pick = calculate_pick(10_000.0, &[l1, l2], &[]).expect("pick");
        // L2 apex at each of 2 parent ends: 10000/2 = 5000
        let apex = pick.layers[1]
            .hardware
            .iter()
            .find(|h| h.name.starts_with("Apex"))
            .expect("apex");
        assert!((apex.reaction_lbs - 5_000.0).abs() < 1e-6);
        assert!(!apex.overloaded);
        // Two 5/8 apex shackles (1.37 lb ea) are added on the way up.
        assert_eq!(pick.layers[1].rigging.apex_shackle_count, 2);
        assert!((pick.layers[1].load_at_top_lbs - 10_002.74).abs() < 1e-6);
        assert!((pick.hook_load_lbs - 10_002.74).abs() < 1e-6);
    }

    /// Full tree: every sling, shackle, and bar weight is carried by everything above it.
    #[test]
    fn rigging_weight_accumulates_through_tree() {
        let bar = SavedSpreader::new("Test", "Bar", 40_000, 300.0);

        // L1: 2 × RS-7 blue @ 12 ft (1.19 lb/ft), 1-1/2 apex (17.20), 1 leg shackles (5.03), spreader 300
        let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
        l1.layer_index = 0;
        l1.sling_length_ft = 12.0;
        l1.apex_shackle = Some("1-1/2".into());
        l1.leg_shackle = Some("1".into());
        l1.spreader_id = Some(bar.id);

        // L2: 4 × RS-5 red @ 8 ft (1.00 lb/ft), 3/4 apex at 2 parent ends (2.35), 5/8 legs (1.37), tare 20
        let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
        l2.layer_index = 1;
        l2.sling_length_ft = 8.0;
        l2.apex_shackle = Some("3/4".into());
        l2.leg_shackle = Some("5/8".into());
        l2.tare_lbs = 20.0;

        let payload = 10_000.0;
        let pick = calculate_pick(payload, &[l1, l2], &[bar]).expect("pick");

        // --- L2 (bottom) ---
        let r2 = &pick.layers[1];
        let l2_slings = 4.0 * 8.0 * 1.00;
        let l2_legs = 4.0 * 1.37;
        let l2_apex = 2.0 * 2.35;
        assert!((r2.rigging.slings_lbs - l2_slings).abs() < 1e-9);
        assert!((r2.rigging.leg_shackles_lbs - l2_legs).abs() < 1e-9);
        assert!((r2.rigging.apex_shackles_lbs - l2_apex).abs() < 1e-9);
        assert!((r2.load_below_lbs - payload).abs() < 1e-9);
        let l2_carried = payload + l2_slings + l2_legs + 20.0;
        assert!((r2.carried_lbs - l2_carried).abs() < 1e-9);
        let l2_top = l2_carried + l2_apex;
        assert!((r2.load_at_top_lbs - l2_top).abs() < 1e-9);
        assert!(!r2.sling_length_missing);

        // --- L1 (top) ---
        let r1 = &pick.layers[0];
        let l1_slings = 2.0 * 12.0 * 1.19;
        let l1_legs = 2.0 * 5.03;
        let l1_apex = 17.20;
        assert!((r1.load_below_lbs - l2_top).abs() < 1e-9);
        assert!((r1.load_on_spreader_lbs - l2_top).abs() < 1e-9);
        let l1_carried = l2_top + l1_slings + l1_legs + 300.0;
        assert!((r1.carried_lbs - l1_carried).abs() < 1e-9);
        let hook = l1_carried + l1_apex;
        assert!((pick.hook_load_lbs - hook).abs() < 1e-9);
        assert!((pick.rigging_weight_lbs - (hook - payload)).abs() < 1e-9);
        assert!((r1.cumulative_rigging_lbs - pick.rigging_weight_lbs).abs() < 1e-9);
        assert!((r2.cumulative_rigging_lbs - (l2_top - payload)).abs() < 1e-9);

        // Tension on L1 legs is driven by payload + all rigging below the L1 apex.
        let expected_t = l1_carried / 2.0 * tension_factor(60.0);
        assert!((r1.tension_lbs - expected_t).abs() < 1e-9);

        // L1 apex shackle sees everything hanging from it.
        let apex = r1
            .hardware
            .iter()
            .find(|h| h.name.starts_with("Apex"))
            .unwrap();
        assert!((apex.reaction_lbs - l1_carried).abs() < 1e-9);
    }

    #[test]
    fn tension_uses_angle_from_geometry() {
        // 12 ft slings to pick points 12 ft apart → 60°, regardless of the stale manual 45°
        let mut l = layer(Hitch::Vertical, 45.0, 2, 5);
        l.sling_length_ft = 12.0;
        l.pick_spacing_ft = Some(12.0);
        let pick = calculate_pick(10_000.0, &[l], &[]).expect("pick");
        let r = &pick.layers[0];
        assert!(r.angle_calculated);
        assert!((r.angle_deg - 60.0).abs() < 1e-9);
        let expected = r.carried_lbs / 2.0 * tension_factor(60.0);
        assert!((r.tension_lbs - expected).abs() < 1e-9);
        assert!(pick.rigging_height_ft.is_some());
    }

    #[test]
    fn impossible_geometry_reads_as_overloaded() {
        let mut l = layer(Hitch::Vertical, 60.0, 2, 5);
        l.sling_length_ft = 4.0;
        l.pick_spacing_ft = Some(12.0);
        let pick = calculate_pick(1_000.0, &[l], &[]).expect("pick");
        let r = &pick.layers[0];
        assert!(r.geometry.error.is_some());
        assert!(r.overloaded);
    }

    #[test]
    fn older_saved_layers_load_without_sling_length() {
        let json = r#"{"pick_id":"00000000-0000-0000-0000-000000000000","layer_index":0,
            "size":5,"hitch":"Vertical","angle_deg":60.0,"sling_count":2}"#;
        let l: SlingLayer = serde_json::from_str(json).expect("legacy layer");
        assert_eq!(l.sling_length_ft, 0.0);
    }

    #[test]
    fn missing_sling_length_is_flagged() {
        let l = layer(Hitch::Vertical, 90.0, 2, 5);
        let r = calculate_layer(1_000.0, &l).expect("calc");
        assert!(r.sling_length_missing);
        assert_eq!(r.rigging.slings_lbs, 0.0);
    }
}
