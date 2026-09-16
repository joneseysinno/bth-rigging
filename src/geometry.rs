//! Rigging geometry: sling angles from fixed sling lengths and pick-point spacing.
//!
//! Plan coordinates (ft): `x` runs along the length of the load / spreader,
//! `y` across the width. Everything is centred under the hook at (0, 0).
//!
//! For each layer:
//! * **Apex points** are where the tops of the slings hang from — the hook for L1,
//!   otherwise the endpoints of the layer above (spreader lugs or pick points).
//! * **Pick points** are where the bottoms of the slings attach. With a spreader on
//!   the layer they are the bar's top lugs (spread over the span); otherwise they come
//!   from the entered adjacent spacing and optional width.
//! * **Endpoints below** are what the next layer hangs from: the bar's end lugs when a
//!   spreader is present, otherwise the pick points themselves.
//!
//! Each sling's horizontal reach `h` is the plan distance from its apex to its pick
//! point. With leg length `L`, the angle from horizontal is `θ = acos(h / L)` and the
//! vertical drop is `√(L² − h²)`. The smallest angle on the layer governs tension.

use std::collections::HashMap;

use uuid::Uuid;

use crate::hardware::split_slings;
use crate::models::{SavedSpreader, SlingLayer};

/// Leg drops on one layer that differ by more than this (in) are flagged.
pub const UNEQUAL_DROP_TOL_IN: f64 = 1.0;

/// A point in plan (ft).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanPoint {
    pub x: f64,
    pub y: f64,
}

impl PlanPoint {
    pub const ORIGIN: PlanPoint = PlanPoint { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn dist(self, other: PlanPoint) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// Where a layer's pick-point spacing came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingSource {
    /// Spreader span on this layer.
    Spreader,
    /// Entered pick-point spacing / width.
    Entered,
    /// Single sling: pick point sits directly under its apex.
    SingleSling,
}

/// Resolved geometry for one layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayerGeometry {
    /// Bottom attachment points of this layer's slings (empty = unknown).
    pub pick_points: Vec<PlanPoint>,
    /// Points the next layer hangs from (empty = unknown).
    pub endpoints_below: Vec<PlanPoint>,
    pub spacing_source: Option<SpacingSource>,
    /// Adjacent spacing along x actually used (ft).
    pub spacing_ft: Option<f64>,
    /// Width across y actually used (ft).
    pub width_ft: Option<f64>,
    /// Spreader span used (ft).
    pub spreader_span_ft: Option<f64>,
    /// Horizontal reach per sling (ft), in sling order.
    pub reaches_ft: Vec<f64>,
    /// Vertical drop per sling (ft), in sling order.
    pub drops_ft: Vec<f64>,
    /// Governing (smallest) calculated angle from horizontal, degrees.
    pub angle_deg: Option<f64>,
    /// Largest reach on the layer (ft).
    pub max_reach_ft: Option<f64>,
    /// Vertical drop of the governing sling (ft).
    pub drop_ft: Option<f64>,
    /// Spread between the longest and shortest leg drop (in), when over tolerance.
    pub unequal_drop_in: Option<f64>,
    /// Geometry that cannot be rigged (e.g. sling shorter than its reach).
    pub error: Option<String>,
    /// Why the angle could not be calculated (manual angle is used instead).
    pub missing: Option<String>,
}

impl LayerGeometry {
    pub fn is_calculated(&self) -> bool {
        self.angle_deg.is_some() && self.error.is_none()
    }

    /// Short description of where the pick points came from.
    pub fn spacing_label(&self) -> Option<String> {
        match self.spacing_source? {
            SpacingSource::Spreader => self
                .spreader_span_ft
                .map(|s| format!("Bar span {} ft", fmt_ft(s))),
            SpacingSource::SingleSling => Some("Single sling, plumb".into()),
            SpacingSource::Entered => match (self.spacing_ft, self.width_ft) {
                (Some(s), Some(w)) => {
                    Some(format!("Picks @ {} ft × {} ft wide", fmt_ft(s), fmt_ft(w)))
                }
                (Some(s), None) => Some(format!("Picks @ {} ft", fmt_ft(s))),
                (None, Some(w)) => Some(format!("Picks {} ft wide", fmt_ft(w))),
                (None, None) => None,
            },
        }
    }

    /// Human-readable geometry lines for reports.
    pub fn summary_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(label) = self.spacing_label() {
            out.push(label);
        }
        if let Some(ref e) = self.error {
            out.push(e.clone());
        } else if let (Some(_), Some(h), Some(d)) =
            (self.angle_deg, self.max_reach_ft, self.drop_ft)
        {
            out.push(format!("Reach {} ft · drop {} ft", fmt_ft(h), fmt_ft(d)));
        } else if let Some(ref m) = self.missing {
            out.push(format!("Manual angle — {m}"));
        }
        if let Some(inch) = self.unequal_drop_in {
            out.push(format!(
                "Leg drops differ by {inch:.1} in — legs will not share load equally"
            ));
        }
        out
    }
}

fn fmt_ft(v: f64) -> String {
    if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

fn positive(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite() && *x > 0.0)
}

/// `n` points in a line along x at the given adjacent spacing, centred on 0.
fn line_points(n: u32, spacing: f64) -> Vec<PlanPoint> {
    let n = n.max(1);
    let mid = (f64::from(n) - 1.0) / 2.0;
    (0..n)
        .map(|k| PlanPoint::new((f64::from(k) - mid) * spacing, 0.0))
        .collect()
}

/// `n` points in two rows (y = ±width/2), columns along x at the adjacent spacing.
/// Ordered column by column (x ascending, then y ascending) so groups under
/// parent endpoints pick the nearest columns. An odd last point sits on y = 0.
fn grid_points(n: u32, spacing: f64, width: f64) -> Vec<PlanPoint> {
    let n = n.max(1);
    let cols = n.div_ceil(2);
    let mid = (f64::from(cols) - 1.0) / 2.0;
    let mut out = Vec::with_capacity(n as usize);
    for c in 0..cols {
        let x = (f64::from(c) - mid) * spacing;
        let remaining = n - 2 * c;
        if remaining >= 2 {
            out.push(PlanPoint::new(x, -width / 2.0));
            out.push(PlanPoint::new(x, width / 2.0));
        } else {
            out.push(PlanPoint::new(x, 0.0));
        }
    }
    out
}

/// `n` points spread evenly over `span`, outer to outer, centred on 0.
fn span_points(n: u32, span: f64) -> Vec<PlanPoint> {
    let n = n.max(1);
    if n == 1 {
        return vec![PlanPoint::ORIGIN];
    }
    line_points(n, span / (f64::from(n) - 1.0))
}

/// Spreader span for a layer: the layer's own value, else the saved bar's span.
pub fn layer_spreader_span(
    layer: &SlingLayer,
    catalog: &HashMap<Uuid, SavedSpreader>,
) -> Option<f64> {
    let has_bar = layer.spreader_id.is_some() || layer.spreader_wll_lbs.is_some();
    if !has_bar {
        return None;
    }
    positive(layer.spreader_span_ft).or_else(|| {
        layer
            .spreader_id
            .and_then(|id| catalog.get(&id))
            .and_then(|b| positive(b.span_ft))
    })
}

/// Number of endpoints the next layer hangs from.
/// A spreader always presents at least two end lugs.
pub fn endpoints_below_count(layer: &SlingLayer) -> u32 {
    let has_bar = layer.spreader_id.is_some() || layer.spreader_wll_lbs.is_some();
    if has_bar {
        layer.sling_count.max(2)
    } else {
        layer.sling_count.max(1)
    }
}

/// Resolve pick points, reaches, and calculated angles for every layer (top → bottom).
pub fn resolve_geometry(layers: &[SlingLayer], spreaders: &[SavedSpreader]) -> Vec<LayerGeometry> {
    let catalog: HashMap<Uuid, SavedSpreader> =
        spreaders.iter().cloned().map(|s| (s.id, s)).collect();

    let mut out = Vec::with_capacity(layers.len());
    let mut apexes: Vec<PlanPoint> = vec![PlanPoint::ORIGIN];

    for layer in layers {
        let n = layer.sling_count.max(1);
        let mut g = LayerGeometry::default();
        let span = layer_spreader_span(layer, &catalog);
        let spacing = positive(layer.pick_spacing_ft);
        let width = positive(layer.pick_width_ft);

        // --- Pick points (bottoms of this layer's slings) ---
        if let Some(span) = span {
            g.spacing_source = Some(SpacingSource::Spreader);
            g.spreader_span_ft = Some(span);
            g.pick_points = span_points(n, span);
            g.spacing_ft = if n > 1 {
                Some(span / (f64::from(n) - 1.0))
            } else {
                None
            };
            g.endpoints_below = span_points(endpoints_below_count(layer), span);
        } else if n == 1 && apexes.len() == 1 {
            g.spacing_source = Some(SpacingSource::SingleSling);
            g.pick_points = vec![apexes[0]];
            g.endpoints_below = g.pick_points.clone();
        } else if let Some(s) = spacing {
            g.spacing_source = Some(SpacingSource::Entered);
            g.spacing_ft = Some(s);
            g.pick_points = match width {
                Some(w) => {
                    g.width_ft = Some(w);
                    grid_points(n, s, w)
                }
                None => line_points(n, s),
            };
            g.endpoints_below = g.pick_points.clone();
        } else if let (Some(w), true) = (width, n == 2) {
            // Two pick points straight across the width.
            g.spacing_source = Some(SpacingSource::Entered);
            g.width_ft = Some(w);
            g.pick_points = grid_points(2, 0.0, w);
            g.endpoints_below = g.pick_points.clone();
        }

        // --- Reaches and angles ---
        let length = layer.sling_length_ft;
        if g.pick_points.is_empty() {
            g.missing = Some(
                if layer.spreader_id.is_some() || layer.spreader_wll_lbs.is_some() {
                    "Enter the spreader span to calculate the angle.".into()
                } else {
                    "Enter pick-point spacing to calculate the angle.".into()
                },
            );
        } else if apexes.is_empty() {
            g.missing = Some("Pick points of the layer above are unknown.".into());
        } else if !(length.is_finite() && length > 0.0) {
            g.missing = Some("Enter sling length to calculate the angle.".into());
        } else {
            let splits = split_slings(apexes.len() as u32, n);
            let mut idx = 0usize;
            for (ai, apex) in apexes.iter().enumerate() {
                let count = splits.get(ai).copied().unwrap_or(0) as usize;
                for _ in 0..count {
                    if let Some(p) = g.pick_points.get(idx) {
                        g.reaches_ft.push(apex.dist(*p));
                    }
                    idx += 1;
                }
            }

            let max_reach = g.reaches_ft.iter().copied().fold(0.0_f64, f64::max);
            g.max_reach_ft = Some(max_reach);
            if max_reach >= length - 1e-9 {
                g.error = Some(format!(
                    "Sling length {:.2} ft is too short for a {:.2} ft horizontal reach.",
                    length, max_reach
                ));
            } else {
                g.drops_ft = g
                    .reaches_ft
                    .iter()
                    .map(|h| (length * length - h * h).sqrt())
                    .collect();
                let angle = (max_reach / length).acos().to_degrees();
                g.angle_deg = Some(angle);
                let min_drop = g.drops_ft.iter().copied().fold(f64::INFINITY, f64::min);
                let max_drop = g.drops_ft.iter().copied().fold(0.0_f64, f64::max);
                g.drop_ft = Some(min_drop);
                let spread_in = (max_drop - min_drop) * 12.0;
                if spread_in > UNEQUAL_DROP_TOL_IN {
                    g.unequal_drop_in = Some(spread_in);
                }
            }
        }

        apexes = g.endpoints_below.clone();
        out.push(g);
    }

    out
}

/// Angle used for tension on a layer and whether it was calculated.
/// Invalid geometry returns 0° (infinite tension) so the layer reads as overloaded.
pub fn governing_angle(layer: &SlingLayer, geom: Option<&LayerGeometry>) -> (f64, bool) {
    match geom {
        Some(g) if g.error.is_some() => (0.0, true),
        Some(g) => match g.angle_deg {
            Some(a) => (a, true),
            None => (layer.angle_deg, false),
        },
        None => (layer.angle_deg, false),
    }
}

/// Approximate vertical rigging height from hook to the lowest pick points (ft),
/// summing each layer's governing drop. Excludes shackle and spreader depth.
pub fn rigging_height_ft(geoms: &[LayerGeometry]) -> Option<f64> {
    if geoms.is_empty() {
        return None;
    }
    geoms
        .iter()
        .map(|g| if g.error.is_none() { g.drop_ft } else { None })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Hitch;

    fn layer(count: u32, length: f64) -> SlingLayer {
        SlingLayer {
            pick_id: Uuid::nil(),
            layer_index: 0,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 45.0,
            sling_count: count,
            sling_length_ft: length,
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

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn two_leg_bridle_sixty_degrees() {
        // 12 ft slings, pick points 12 ft apart → reach 6 → acos(0.5) = 60°
        let mut l = layer(2, 12.0);
        l.pick_spacing_ft = Some(12.0);
        let g = &resolve_geometry(&[l], &[])[0];
        assert!(close(g.angle_deg.unwrap(), 60.0));
        assert!(close(g.max_reach_ft.unwrap(), 6.0));
        assert!(close(g.drop_ft.unwrap(), 108.0_f64.sqrt()));
        assert!(g.unequal_drop_in.is_none());
    }

    #[test]
    fn spreader_span_sets_pick_points_and_lower_layer_hangs_from_lugs() {
        let mut bar = SavedSpreader::new("Acme", "SB", 20_000, 150.0);
        bar.span_ft = Some(10.0);
        // L1: 12 ft slings to a bar rigged at 12 ft (layer value overrides saved 10 ft)
        let mut l1 = layer(2, 12.0);
        l1.spreader_id = Some(bar.id);
        l1.spreader_span_ft = Some(12.0);
        l1.pick_spacing_ft = Some(99.0); // ignored when a span is known
        // L2: vertical drops to pick points 12 ft apart
        let mut l2 = layer(2, 10.0);
        l2.pick_spacing_ft = Some(12.0);

        let g = resolve_geometry(&[l1, l2], &[bar]);
        assert_eq!(g[0].spacing_source, Some(SpacingSource::Spreader));
        assert!(close(g[0].angle_deg.unwrap(), 60.0));
        assert!(close(g[1].angle_deg.unwrap(), 90.0));
        assert!(close(g[1].drop_ft.unwrap(), 10.0));
        let h = rigging_height_ft(&g).unwrap();
        assert!(close(h, 108.0_f64.sqrt() + 10.0));
    }

    #[test]
    fn saved_bar_span_used_when_layer_span_blank() {
        let mut bar = SavedSpreader::new("Acme", "SB", 20_000, 150.0);
        bar.span_ft = Some(12.0);
        let mut l1 = layer(2, 12.0);
        l1.spreader_id = Some(bar.id);
        let g = &resolve_geometry(&[l1], &[bar])[0];
        assert_eq!(g.spreader_span_ft, Some(12.0));
        assert!(close(g.angle_deg.unwrap(), 60.0));
    }

    #[test]
    fn four_leg_bridle_to_rectangle_uses_true_reach() {
        // 8 ft × 6 ft rectangle: half-diagonal = 5 ft; 10 ft legs → 60°
        let mut l = layer(4, 10.0);
        l.pick_spacing_ft = Some(8.0);
        l.pick_width_ft = Some(6.0);
        let g = &resolve_geometry(&[l], &[])[0];
        assert!(g.reaches_ft.iter().all(|h| close(*h, 5.0)));
        assert!(close(g.angle_deg.unwrap(), 60.0));
    }

    #[test]
    fn two_over_four_rectangle_under_spreader() {
        // Bar lugs at x = ±6; pick columns at x = ±4, rows y = ±3 → reach √(2² + 3²)
        let mut l1 = layer(2, 12.0);
        l1.spreader_wll_lbs = Some(20_000);
        l1.spreader_span_ft = Some(12.0);
        let mut l2 = layer(4, 10.0);
        l2.pick_spacing_ft = Some(8.0);
        l2.pick_width_ft = Some(6.0);
        let g = resolve_geometry(&[l1, l2], &[]);
        let h = 13.0_f64.sqrt();
        assert!(g[1].reaches_ft.iter().all(|r| close(*r, h)));
        assert!(close(
            g[1].angle_deg.unwrap(),
            (h / 10.0).acos().to_degrees()
        ));
    }

    #[test]
    fn adjacent_spacing_three_in_line_flags_unequal_drops() {
        // 3 points at 5 ft adjacent spacing: x = −5, 0, 5 → reaches 5, 0, 5
        let mut l = layer(3, 10.0);
        l.pick_spacing_ft = Some(5.0);
        let g = &resolve_geometry(&[l], &[])[0];
        assert!(close(g.reaches_ft[0], 5.0));
        assert!(close(g.reaches_ft[1], 0.0));
        assert!(close(g.angle_deg.unwrap(), 60.0));
        assert!(g.unequal_drop_in.is_some());
    }

    #[test]
    fn too_short_sling_is_an_error() {
        let mut l = layer(2, 10.0);
        l.pick_spacing_ft = Some(30.0);
        let g = &resolve_geometry(&[l.clone()], &[])[0];
        assert!(g.error.is_some());
        assert_eq!(governing_angle(&l, Some(g)), (0.0, true));
    }

    #[test]
    fn falls_back_to_manual_angle_without_spacing() {
        let l = layer(2, 10.0);
        let g = &resolve_geometry(&[l.clone()], &[])[0];
        assert!(g.angle_deg.is_none());
        assert!(g.missing.is_some());
        assert_eq!(governing_angle(&l, Some(g)), (45.0, false));
    }

    #[test]
    fn lifting_beam_single_top_sling_two_bottom_lugs() {
        // One sling to a beam's centre lug, beam lugs 10 ft apart, vertical drops below.
        let mut l1 = layer(1, 6.0);
        l1.spreader_wll_lbs = Some(20_000);
        l1.spreader_span_ft = Some(10.0);
        let mut l2 = layer(2, 8.0);
        l2.pick_spacing_ft = Some(10.0);
        let g = resolve_geometry(&[l1, l2], &[]);
        assert!(close(g[0].angle_deg.unwrap(), 90.0));
        assert_eq!(g[0].endpoints_below.len(), 2);
        assert!(close(g[1].angle_deg.unwrap(), 90.0));
    }
}
