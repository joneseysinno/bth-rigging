//! Built-in rigs used across tests: Duplo10 and small shapes.
//!
//! Step: 1
//! Theory: Duplo10 Lift Readback — provisional geometry until drawings arrive.
//! Inputs: placeholder parameters tagged Assumed / Drawing.
//! Outputs: a validating `Rig`.
//! Must not depend on: UI, dioxus, store.

use crate::domain::Hitch;

use super::Rig;
use super::build::RigBuilder;
use super::component::Adjust;
use super::node::Axis;
use super::param::{Coord3, Expr, Quantity};

/// Duplo10 enclosure under a 4-bar frame. Dimensions are placeholders
/// (`ParamSource::Assumed`) except the lift-plan load and EGL split.
pub fn duplo10() -> Rig {
    let mut b = RigBuilder::new("Duplo10");

    let load_length = b.param("load_length", Quantity::Length, 48.0).assumed();
    let load_width = b.param("load_width", Quantity::Length, 12.0).assumed();
    let load_height = b.param("load_height", Quantity::Length, 11.0).assumed();
    let load_weight = b
        .param("load_weight", Quantity::Weight, 117_300.0)
        .drawing();
    let cg_x = b
        .param("cg_x", Quantity::Length, 0.0)
        .tol(2.0, 2.0)
        .assumed();
    let cg_y = b
        .param("cg_y", Quantity::Length, 0.0)
        .tol(2.0, 2.0)
        .assumed();
    let cg_z = b
        .param("cg_z", Quantity::Length, 5.5)
        .tol(2.0, 2.0)
        .assumed();
    let s12 = b.param("s12", Quantity::Length, 8.0).assumed();
    let s23 = b.param("s23", Quantity::Length, 8.0).assumed();
    let s34 = b.param("s34", Quantity::Length, 8.0).assumed();
    let g = b.param("lug_gauge_G", Quantity::Length, 12.0).assumed();
    let lug_z = b.param("lug_z", Quantity::Length, 11.0).assumed();
    let span_a = b.param("span_A", Quantity::Length, 34.0).assumed();
    let span_b = b.param("span_B", Quantity::Length, 12.0).assumed();
    let bar_weight_top = b
        .param("bar_weight_top", Quantity::Weight, 1_673.0)
        .drawing();
    let bar_weight_cross = b
        .param("bar_weight_cross", Quantity::Weight, 1_000.0)
        .drawing();
    let basket_len = b.param("basket_len", Quantity::Length, 22.0).assumed();
    let chain_leg_len = b.param("chain_leg_len", Quantity::Length, 9.0).assumed();
    let chain_adjust = b.param("chain_adjust", Quantity::Length, 8.0).assumed();
    let mu_bow = b.param("mu_bow", Quantity::Ratio, 0.0).assumed();
    let strap_wpf = b.param("strap_wpf", Quantity::Ratio, 0.5).assumed();
    let chain_w = b.param("chain_weight", Quantity::Weight, 35.0).assumed();

    let hook = b.hook("Hook");
    let unit = b.load(
        "Al MDC enclosure",
        load_length,
        load_width,
        load_height,
        load_weight,
    );
    b.set_cg(unit, Coord3::new(cg_x, cg_y, cg_z));

    let half_g = Expr::from(g) / 2.0;
    // P1 at -3(s12): with 8 ft bays, P1..P7 at x = -24,-16,-8,0,8,16,24.
    let x_p = |k: i32| -> Expr {
        // k = 0..6 → P1..P7. x = (k as f64 - 3.0) * 8, built from s12/s23/s34.
        // P1 = -(s12+s23+s34), P2 = -(s23+s34), P3 = -s34, P4 = 0,
        // P5 = s34, P6 = s34+s23, P7 = s34+s23+s12.
        match k {
            0 => -(Expr::from(s12) + s23 + s34),
            1 => -(Expr::from(s23) + s34),
            2 => -Expr::from(s34),
            3 => Expr::c(0.0),
            4 => Expr::from(s34),
            5 => Expr::from(s34) + s23,
            6 => Expr::from(s34) + s23 + s12,
            _ => Expr::c(0.0),
        }
    };

    let mut load_lugs = Vec::new();
    for i in 0..7 {
        let x = x_p(i);
        let front = b.lug_axis(
            unit,
            format!("P{}-front", i + 1),
            x.clone(),
            -half_g.clone(),
            lug_z,
            Axis::Y,
        );
        let back = b.lug_axis(
            unit,
            format!("P{}-back", i + 1),
            x,
            half_g.clone(),
            lug_z,
            Axis::NegY,
        );
        load_lugs.push((front, back));
    }

    let bar_top = b.bar("Bar top", span_a, bar_weight_top);
    let bar_l = b.bar("Bar L", span_b, bar_weight_cross);
    let bar_c = b.bar("Bar C", span_b, bar_weight_cross);
    let bar_r = b.bar("Bar R", span_b, bar_weight_cross);

    let half_a = Expr::from(span_a) / 2.0;
    let half_b = Expr::from(span_b) / 2.0;
    let bow_dia = Expr::c(3.0);

    // Top bar along X: end bows at ±span_A/2.
    let top_bow_a = b.bow(
        bar_top,
        "Bar top end A",
        -half_a.clone(),
        0.0,
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let top_bow_b = b.bow(
        bar_top,
        "Bar top end B",
        half_a,
        0.0,
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let top_lug_l_f = b.lug(bar_top, "Top→L front", x_p(1), -half_g.clone(), 0.0);
    let top_lug_l_b = b.lug(bar_top, "Top→L back", x_p(1), half_g.clone(), 0.0);
    let top_lug_r_f = b.lug(bar_top, "Top→R front", x_p(5), -half_g.clone(), 0.0);
    let top_lug_r_b = b.lug(bar_top, "Top→R back", x_p(5), half_g.clone(), 0.0);
    let top_hook_lf = b.lug(
        bar_top,
        "Top hook LF",
        -(Expr::from(span_a) / 4.0),
        -1.0,
        0.0,
    );
    let top_hook_lb = b.lug(
        bar_top,
        "Top hook LB",
        -(Expr::from(span_a) / 4.0),
        1.0,
        0.0,
    );
    let top_hook_rf = b.lug(bar_top, "Top hook RF", Expr::from(span_a) / 4.0, -1.0, 0.0);
    let top_hook_rb = b.lug(bar_top, "Top hook RB", Expr::from(span_a) / 4.0, 1.0, 0.0);

    // Cross bars along Y: end bows at ±span_B/2, at body origin.
    let bow_l_f = b.bow(
        bar_l,
        "Bar L bow F",
        0.0,
        -half_b.clone(),
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let bow_l_b = b.bow(
        bar_l,
        "Bar L bow B",
        0.0,
        half_b.clone(),
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let lug_l_f = b.lug(bar_l, "Bar L lug F", 0.0, -half_b.clone(), 0.1);
    let lug_l_b = b.lug(bar_l, "Bar L lug B", 0.0, half_b.clone(), 0.1);

    let bow_c_f = b.bow(
        bar_c,
        "Bar C bow F",
        0.0,
        -half_b.clone(),
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let bow_c_b = b.bow(
        bar_c,
        "Bar C bow B",
        0.0,
        half_b.clone(),
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let lug_c_f = b.lug(bar_c, "Bar C lug F", 0.0, -half_b.clone(), 0.1);
    let lug_c_b = b.lug(bar_c, "Bar C lug B", 0.0, half_b.clone(), 0.1);

    let bow_r_f = b.bow(
        bar_r,
        "Bar R bow F",
        0.0,
        -half_b.clone(),
        0.0,
        bow_dia.clone(),
        mu_bow,
    );
    let bow_r_b = b.bow(bar_r, "Bar R bow B", 0.0, half_b, 0.0, bow_dia, mu_bow);
    let lug_r_f = b.lug(bar_r, "Bar R lug F", 0.0, -(Expr::from(span_b) / 2.0), 0.1);
    let lug_r_b = b.lug(bar_r, "Bar R lug B", 0.0, Expr::from(span_b) / 2.0, 0.1);

    let _ = (top_bow_a, top_bow_b, bow_c_f, bow_c_b); // bows counted; used as bearings below

    let half_basket = Expr::from(basket_len) / 2.0;
    let basket_half_w = half_basket.clone() * strap_wpf;

    let mut basket = |label: &str, a, bow, c| {
        b.member(label)
            .from(a)
            .through(bow)
            .to(c)
            .segment(|s| {
                s.shackle("1-1/4")
                    .strap_w(half_basket.clone(), basket_half_w.clone())
                    .shackle("1-1/4")
            })
            .segment(|s| {
                s.shackle("1-1/4")
                    .strap_w(half_basket.clone(), basket_half_w.clone())
                    .shackle("1-1/4")
            });
    };
    basket(
        "Basket strap P1–P3 (front)",
        load_lugs[0].0,
        bow_l_f,
        load_lugs[2].0,
    );
    basket(
        "Basket strap P1–P3 (back)",
        load_lugs[0].1,
        bow_l_b,
        load_lugs[2].1,
    );
    basket(
        "Basket strap P5–P7 (front)",
        load_lugs[4].0,
        bow_r_f,
        load_lugs[6].0,
    );
    basket(
        "Basket strap P5–P7 (back)",
        load_lugs[4].1,
        bow_r_b,
        load_lugs[6].1,
    );

    let adj = Adjust::new(6.0, 10.0, chain_adjust);
    let mut chain_leg = |label: &str, bar_lug, pick| {
        let adj = adj.clone();
        b.member(label).from(bar_lug).to(pick).segment(|s| {
            s.shackle("1")
                .strap_w(3.0, Expr::c(3.0) * strap_wpf)
                .chain(8, 0.5, chain_leg_len, chain_w, adj)
                .shackle("1")
        });
    };
    chain_leg("Chain leg P2 front", lug_l_f, load_lugs[1].0);
    chain_leg("Chain leg P2 back", lug_l_b, load_lugs[1].1);
    chain_leg("Chain leg P4 front", lug_c_f, load_lugs[3].0);
    chain_leg("Chain leg P4 back", lug_c_b, load_lugs[3].1);
    chain_leg("Chain leg P6 front", lug_r_f, load_lugs[5].0);
    chain_leg("Chain leg P6 back", lug_r_b, load_lugs[5].1);

    let mut hook_leg = |label: &str, dest| {
        b.member(label).from(hook).to(dest).segment(|s| {
            s.shackle("1-1/2")
                .roundsling(11, Hitch::Vertical, 16.0)
                .shackle("1-1/2")
        });
    };
    hook_leg("Hook leg LF", top_hook_lf);
    hook_leg("Hook leg LB", top_hook_lb);
    hook_leg("Hook leg RF", top_hook_rf);
    hook_leg("Hook leg RB", top_hook_rb);
    hook_leg("Hook to Bar C front", lug_c_f);
    hook_leg("Hook to Bar C back", lug_c_b);
    drop(hook_leg);

    b.member("V leg L front")
        .from(top_lug_l_f)
        .to(lug_l_f)
        .segment(|s| {
            s.shackle("1-1/4")
                .roundsling(9, Hitch::Vertical, 10.0)
                .shackle("1-1/4")
        });
    b.member("V leg L back")
        .from(top_lug_l_b)
        .to(lug_l_b)
        .segment(|s| {
            s.shackle("1-1/4")
                .roundsling(9, Hitch::Vertical, 10.0)
                .shackle("1-1/4")
        });
    b.member("V leg R front")
        .from(top_lug_r_f)
        .to(lug_r_f)
        .segment(|s| {
            s.shackle("1-1/4")
                .roundsling(9, Hitch::Vertical, 10.0)
                .shackle("1-1/4")
        });
    b.member("V leg R back")
        .from(top_lug_r_b)
        .to(lug_r_b)
        .segment(|s| {
            s.shackle("1-1/4")
                .roundsling(9, Hitch::Vertical, 10.0)
                .shackle("1-1/4")
        });

    b.finish().expect("Duplo10 fixture should validate")
}

/// Two-leg vertical bridle, no spreader — used as a tiny corpus / builder check.
pub fn two_leg_bridle(weight_lbs: f64, length_ft: f64, spacing_ft: f64) -> Rig {
    let mut b = RigBuilder::new("two-leg");
    let w = b
        .param("load_weight", Quantity::Weight, weight_lbs)
        .drawing();
    let hook = b.hook("Hook");
    let unit = b.load("Load", spacing_ft, 4.0, 2.0, w);
    let a = b.lug(unit, "A", -(spacing_ft / 2.0), 0.0, 2.0);
    let c = b.lug(unit, "B", spacing_ft / 2.0, 0.0, 2.0);
    b.member("leg A")
        .from(hook)
        .to(a)
        .segment(|s| s.roundsling(5, Hitch::Vertical, length_ft));
    b.member("leg B")
        .from(hook)
        .to(c)
        .segment(|s| s.roundsling(5, Hitch::Vertical, length_ft));
    b.finish().expect("two-leg")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::body::BodyKind;
    use crate::rig::node::NodeKind;
    use crate::rig::param::sweep;

    fn is_bow(n: &crate::rig::Node) -> bool {
        matches!(n.kind, NodeKind::Bow { .. })
    }

    #[test]
    fn duplo10_counts() {
        let rig = duplo10();
        rig.validate().expect("valid");

        let n_load_bars = rig
            .bodies
            .values()
            .filter(|b| matches!(b.kind, BodyKind::Load { .. } | BodyKind::SpreaderBar { .. }))
            .count();
        assert_eq!(n_load_bars, 5, "enclosure + 4 bars");

        let n_root = rig.nodes.values().filter(|n| n.kind.is_root()).count();
        assert_eq!(n_root, 1);

        let n_load_lugs = rig
            .nodes
            .values()
            .filter(|n| {
                n.label.starts_with('P')
                    && n.label.contains('-')
                    && matches!(n.kind, NodeKind::Lug { .. })
            })
            .count();
        assert_eq!(n_load_lugs, 14, "7 pick points × 2 faces");

        let n_bows = rig.nodes.values().filter(|n| is_bow(n)).count();
        assert_eq!(n_bows, 8, "bar-end bows");

        let baskets = rig
            .members
            .values()
            .filter(|m| m.label.starts_with("Basket"))
            .count();
        assert_eq!(baskets, 4);
        let chains = rig
            .members
            .values()
            .filter(|m| m.label.starts_with("Chain"))
            .count();
        assert_eq!(chains, 6);
        let hook_legs = rig
            .members
            .values()
            .filter(|m| m.label.starts_with("Hook leg"))
            .count();
        assert_eq!(hook_legs, 4);
        let v_legs = rig
            .members
            .values()
            .filter(|m| m.label.starts_with("V leg"))
            .count();
        assert_eq!(v_legs, 4);
        let hook_center = rig
            .members
            .values()
            .filter(|m| m.label.starts_with("Hook to Bar C"))
            .count();
        assert_eq!(hook_center, 2);
    }

    #[test]
    fn duplo10_weights_match_lift_plan_inputs() {
        let rig = duplo10();
        let w = rig.weights().unwrap();
        assert!((w.load_lbs - 117_300.0).abs() < 1e-9);
        assert!((w.gear_lbs - 4_673.0).abs() < 1e-9);
        assert!(w.assumed);
        assert!((w.total_below_root_lbs - (w.load_lbs + w.gear_lbs + w.rigging_lbs)).abs() < 1e-9);
        assert!(w.rigging_lbs > 0.0);
    }

    #[test]
    fn duplo10_sweep_weight() {
        let rig = duplo10();
        let id = rig.param_named("load_weight").unwrap().id;
        let s = sweep(&rig.params, |tab| {
            // weight roll-up at this table: only load_weight moves
            crate::rig::param::Expr::p(id).eval(tab)
        })
        .unwrap();
        assert!((s.nominal - 117_300.0).abs() < 1e-9);
        // no tolerance on load_weight → min = max = nominal
        assert!((s.min - s.nominal).abs() < 1e-9);

        let cg = rig.param_named("cg_x").unwrap().id;
        let s = sweep(&rig.params, |tab| crate::rig::param::Expr::p(cg).eval(tab)).unwrap();
        assert!((s.min - (-2.0)).abs() < 1e-9);
        assert!((s.max - 2.0).abs() < 1e-9);
    }
}
