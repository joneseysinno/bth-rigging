//! Layers to graph conversion; results must match the layer engine.
//!
//! Step: 1
//! Theory: roadmap Step 1 — permanent regression oracle vs layers.
//! Inputs: SlingLayer stack / pick.
//! Outputs: equivalent rig graph.
//! Must not depend on: UI, dioxus.

use crate::catalog::{find_by_size, find_shackle};
use crate::domain::{Pick, SavedSpreader, SlingLayer};
use crate::layers::geometry::{
    LayerGeometry, PlanPoint, endpoints_below_count, layer_spreader_span, resolve_geometry,
};
use crate::layers::split::split_slings;

use super::Rig;
use super::build::RigBuilder;
use super::component::Component;
use super::id::NodeId;
use super::param::{Coord3, Expr, Quantity};

/// Convert a layer-engine pick into a rig graph whose weights match `calculate_pick`.
pub fn from_layers(pick: &Pick, layers: &[SlingLayer], spreaders: &[SavedSpreader]) -> Rig {
    let mut b = RigBuilder::new(pick.name.clone());
    b.project_id(pick.project_id);
    let mut b = b.with_id(pick.id);

    let w_load = b
        .param("load_weight", Quantity::Weight, pick.weight_lbs.max(0.0))
        .drawing();

    let geoms = resolve_geometry(layers, spreaders);
    let (len, width, height) = infer_load_dims(&geoms);
    let load_length = b.param("load_length", Quantity::Length, len).assumed();
    let load_width = b.param("load_width", Quantity::Length, width).assumed();
    let load_height = b.param("load_height", Quantity::Length, height).assumed();

    let hook = b.hook("Hook");
    let load = b.load("Load", load_length, load_width, load_height, w_load);
    b.set_cg(load, Coord3::new(0.0, 0.0, Expr::from(load_height) / 2.0));

    let catalog: std::collections::HashMap<uuid::Uuid, SavedSpreader> =
        spreaders.iter().cloned().map(|s| (s.id, s)).collect();

    let mut below: Vec<NodeId> = vec![hook];
    let n_layers = layers.len();

    for (i, layer) in layers.iter().enumerate() {
        let geom = geoms.get(i).cloned().unwrap_or_default();
        let n = layer.sling_count.max(1);
        let last = i + 1 == n_layers;
        let has_bar = layer.spreader_id.is_some() || layer.spreader_wll_lbs.is_some();
        let span = layer_spreader_span(layer, &catalog);

        let pick_pts = if geom.pick_points.is_empty() {
            synth_points(n, layer.pick_spacing_ft)
        } else {
            geom.pick_points.clone()
        };

        let (picks, next_below) = if has_bar {
            let bar_w = layer
                .spreader_id
                .and_then(|id| catalog.get(&id))
                .map(|s| s.weight_lbs)
                .unwrap_or(0.0);
            let wll = layer
                .spreader_id
                .and_then(|id| catalog.get(&id))
                .map(|s| s.wll_lbs)
                .or(layer.spreader_wll_lbs)
                .unwrap_or(0);
            let span_val = span.unwrap_or(1.0);
            let span_p = b
                .param(format!("span_layer{i}"), Quantity::Length, span_val)
                .assumed();
            let bar = b.spreader(format!("Spreader L{i}"), span_p, bar_w.max(0.0), wll);
            let cx = centroid_x(&pick_pts);
            let mut pick_nodes = Vec::with_capacity(pick_pts.len());
            for (k, p) in pick_pts.iter().enumerate() {
                pick_nodes.push(b.lug(bar, format!("L{i} bar pick {k}"), p.x - cx, p.y, 0.0));
            }
            let ends = endpoints_below_count(layer) as usize;
            let below_nodes = if ends != pick_nodes.len() {
                let end_pts = if geom.endpoints_below.is_empty() {
                    span_line(ends as u32, span_val)
                } else {
                    geom.endpoints_below.clone()
                };
                let mut out = Vec::new();
                for (k, p) in end_pts.iter().enumerate() {
                    out.push(b.lug(bar, format!("L{i} bar end {k}"), p.x - cx, p.y, 0.0));
                }
                out
            } else {
                pick_nodes.clone()
            };
            if last {
                for (k, node) in below_nodes.iter().enumerate() {
                    let lug = b.lug(
                        load,
                        format!("load from bar {k}"),
                        pick_pts.get(k).map(|p| p.x).unwrap_or(0.0),
                        pick_pts.get(k).map(|p| p.y).unwrap_or(0.0),
                        load_height,
                    );
                    b.member(format!("L{i} bar to load {k}"))
                        .from(*node)
                        .to(lug)
                        .segment(|s| s.master_link(0.1, 0.0));
                }
            }
            (pick_nodes, below_nodes)
        } else if last {
            let mut pick_nodes = Vec::new();
            for (k, p) in pick_pts.iter().enumerate() {
                pick_nodes.push(b.lug(load, format!("P{k}"), p.x, p.y, load_height));
            }
            let copy = pick_nodes.clone();
            (pick_nodes, copy)
        } else {
            let mut pick_nodes = Vec::new();
            for (k, _p) in pick_pts.iter().enumerate() {
                pick_nodes.push(b.free(format!("L{i} knot {k}")));
            }
            let copy = pick_nodes.clone();
            (pick_nodes, copy)
        };

        let parents = below.clone();
        let splits = split_slings(parents.len() as u32, n);
        let path_len = if layer.sling_length_ft.is_finite() && layer.sling_length_ft > 0.0 {
            layer.sling_length_ft
        } else {
            1e-9
        };
        let _len_p = b
            .param(format!("l_layer{i}"), Quantity::Length, path_len)
            .drawing();
        let sling_w = find_by_size(layer.size)
            .map(|r| r.sling_weight_lbs(layer.sling_length_ft))
            .unwrap_or(0.0);

        let mut sling_idx = 0usize;
        for (pi, parent) in parents.iter().enumerate() {
            let count = splits.get(pi).copied().unwrap_or(0) as usize;
            let hang = if let Some(size) = layer
                .apex_shackle
                .as_deref()
                .filter(|s| find_shackle(s).is_some())
            {
                let collector = b.free(format!("L{i} collector {pi}"));
                let sz = size.to_string();
                b.member(format!("L{i} apex {pi}"))
                    .from(*parent)
                    .to(collector)
                    .segment(|s| s.shackle(&sz).master_link(0.2, 0.0));
                collector
            } else {
                *parent
            };
            for _ in 0..count {
                let dest = picks[sling_idx.min(picks.len().saturating_sub(1))];
                let mut sling = Component::roundsling(layer.size, layer.hitch, path_len);
                sling.weight = Expr::c(sling_w);
                if !(layer.sling_length_ft.is_finite() && layer.sling_length_ft > 0.0) {
                    sling.weight = Expr::c(0.0);
                }
                let leg = layer.leg_shackle.clone();
                b.member(format!("L{i} sling {sling_idx}"))
                    .from(hang)
                    .to(dest)
                    .segment(|s| {
                        let mut s = s.push(sling.clone());
                        if let Some(key) = leg.as_deref().filter(|k| find_shackle(k).is_some()) {
                            s = s.shackle(key);
                        }
                        s
                    });
                sling_idx += 1;
            }
        }

        if layer.tare_lbs.is_finite() && layer.tare_lbs.abs() > 1e-15 {
            let frame = b.frame(format!("L{i} tare"), layer.tare_lbs.max(0.0));
            let tn = b.lug(frame, format!("L{i} tare node"), 0.0, 0.0, 0.0);
            let host = picks.first().copied().unwrap_or(hook);
            b.member(format!("L{i} tare hang"))
                .from(host)
                .to(tn)
                .segment(|s| s.master_link(0.1, 0.0));
        }

        below = next_below;
        let _ = geom;
    }

    b.build_unchecked()
}

fn infer_load_dims(geoms: &[LayerGeometry]) -> (f64, f64, f64) {
    let last = geoms.last();
    let pts = last.map(|g| g.pick_points.as_slice()).unwrap_or(&[]);
    if pts.is_empty() {
        return (10.0, 4.0, 2.0);
    }
    let xs: Vec<f64> = pts.iter().map(|p| p.x).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.y).collect();
    let length = (xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - xs.iter().copied().fold(f64::INFINITY, f64::min))
    .max(1.0);
    let width = (ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - ys.iter().copied().fold(f64::INFINITY, f64::min))
    .max(1.0);
    (length, width, 2.0)
}

fn centroid_x(pts: &[PlanPoint]) -> f64 {
    if pts.is_empty() {
        0.0
    } else {
        pts.iter().map(|p| p.x).sum::<f64>() / pts.len() as f64
    }
}

fn synth_points(n: u32, spacing: Option<f64>) -> Vec<PlanPoint> {
    let n = n.max(1);
    let s = spacing.filter(|x| x.is_finite() && *x > 0.0).unwrap_or(1.0);
    let mid = (f64::from(n) - 1.0) / 2.0;
    (0..n)
        .map(|k| PlanPoint::new((f64::from(k) - mid) * s, 0.0))
        .collect()
}

fn span_line(n: u32, span: f64) -> Vec<PlanPoint> {
    let n = n.max(1);
    if n == 1 {
        return vec![PlanPoint::ORIGIN];
    }
    let step = span / (f64::from(n) - 1.0);
    synth_points(n, Some(step))
}

/// `rigging_lbs + gear_lbs` vs layer-engine `rigging_weight_lbs`, and hook load.
pub fn weights_match_layers(
    rig: &Rig,
    pick: &Pick,
    layers: &[SlingLayer],
    spreaders: &[SavedSpreader],
) -> Result<(), String> {
    use crate::layers::calculate_pick;
    let Some(pr) = calculate_pick(pick.weight_lbs, layers, spreaders) else {
        return Err("layer engine returned None".into());
    };
    let w = rig.weights().map_err(|e| e.to_string())?;
    let gear_and_rig = w.rigging_lbs + w.gear_lbs;
    if (gear_and_rig - pr.rigging_weight_lbs).abs() > 1e-9 {
        return Err(format!(
            "rigging+gear {gear_and_rig} != layers {} (load {} gear {} rigging {})",
            pr.rigging_weight_lbs, w.load_lbs, w.gear_lbs, w.rigging_lbs
        ));
    }
    if (w.total_below_root_lbs - pr.hook_load_lbs).abs() > 1e-9 {
        return Err(format!(
            "hook {} != layers {}",
            w.total_below_root_lbs, pr.hook_load_lbs
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, Pick};
    use crate::layers::calculate_pick;
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
            .unwrap_or_else(|e| panic!("graph invalid: {e:?}"));
        weights_match_layers(&rig, pick, layers, spreaders).expect("weights");
        let pr = calculate_pick(pick.weight_lbs, layers, spreaders).unwrap();
        let w = rig.weights().unwrap();
        assert!((w.load_lbs - pick.weight_lbs).abs() < 1e-9);
        assert!((w.total_below_root_lbs - pr.hook_load_lbs).abs() < 1e-9);
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
    fn legacy_zero_sling_length() {
        let pick = Pick::new(Uuid::nil(), "legacy", 10_000.0);
        let l = layer(Hitch::Vertical, 60.0, 2, 5);
        assert_eq!(l.sling_length_ft, 0.0);
        check(&pick, &[l], &[]);
    }

    #[test]
    fn corpus_with_shackles_and_lengths() {
        let bar = SavedSpreader::new("Test", "Bar", 40_000, 300.0);
        let mut l1 = layer(Hitch::Vertical, 60.0, 2, 7);
        l1.sling_length_ft = 12.0;
        l1.apex_shackle = Some("1-1/2".into());
        l1.leg_shackle = Some("1".into());
        l1.spreader_id = Some(bar.id);
        let mut l2 = layer(Hitch::Vertical, 60.0, 4, 5);
        l2.layer_index = 1;
        l2.sling_length_ft = 8.0;
        l2.apex_shackle = Some("3/4".into());
        l2.leg_shackle = Some("5/8".into());
        l2.tare_lbs = 20.0;
        let pick = Pick::new(Uuid::nil(), "tree", 10_000.0);
        check(&pick, &[l1, l2], &[bar]);
    }
}
