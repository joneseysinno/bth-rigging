//! Step 3 solver tests: hand statics, determinacy, hang, Duplo10.

use super::*;
use crate::rig::param::Coord3;
use crate::rig::{RigBuilder, duplo10, two_leg_bridle};

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1.0)
}

#[test]
fn two_leg_bridle_matches_hand_statics() {
    let rig = two_leg_bridle(10_000.0, 10.0, 10.0);
    let ev = EvalRig::evaluate(&rig).unwrap();
    let sling = 10.0; // RS-5 × 10 ft, lumped at the load lug
    let expect = (10_000.0 + 2.0 * sling) / 2.0 / 60f64.to_radians().sin();

    let st = statics_at(&rig, &ev).unwrap();
    assert_eq!(st.determinacy.s, 0, "{:?}", st.determinacy);
    assert!(st.determinacy.consistent);
    for m in st.members.values() {
        assert!(
            (m.tension_lbs - expect).abs() < 1e-9,
            "{} vs {expect}",
            m.tension_lbs
        );
    }
    assert!(
        (st.hook_load_lbs - 10_020.0).abs() < 1e-9,
        "{}",
        st.hook_load_lbs
    );

    let hung = solve(&rig).unwrap();
    assert!(hung.converged);
    for m in hung.members.values() {
        assert!(
            rel(m.tension_lbs, expect) < 1e-6,
            "{} vs {expect}",
            m.tension_lbs
        );
    }
    assert!(rel(hung.hook_load_lbs, 10_020.0) < 1e-9);
}

fn four_leg(cg_x: f64, legs: [f64; 4]) -> Rig {
    let mut b = RigBuilder::new("4leg");
    let hook = b.hook("Hook");
    let load = b.load("Load", 8.0, 6.0, 2.0, 4_000.0);
    b.set_cg(load, Coord3::new(cg_x, 0.0, 1.0));
    let lugs = [
        b.lug(load, "a", -4.0, -3.0, 2.0),
        b.lug(load, "b", -4.0, 3.0, 2.0),
        b.lug(load, "c", 4.0, -3.0, 2.0),
        b.lug(load, "d", 4.0, 3.0, 2.0),
    ];
    for (i, lug) in lugs.iter().copied().enumerate() {
        let len = legs[i];
        b.member(format!("leg {i}"))
            .from(hook)
            .to(lug)
            .segment(|s| s.push(crate::rig::Component::strap(len, 0.0, 2.0)));
    }
    b.finish().expect("valid")
}

#[test]
fn symmetric_four_leg_is_indeterminate_by_one_and_shares_equally() {
    let rig = four_leg(0.0, [10.0; 4]);
    let ev = EvalRig::evaluate(&rig).unwrap();
    let st = statics_at(&rig, &ev).unwrap();
    assert_eq!(st.determinacy.s, 1, "{:?}", st.determinacy);
    assert!(st.determinacy.k >= 3);
    let expect = 4_000.0 / 4.0 / 60f64.to_radians().sin();
    for m in st.members.values() {
        assert!((m.tension_lbs - expect).abs() < 1e-9);
    }
    let hung = solve(&rig).unwrap();
    assert!(hung.converged);
    for m in hung.members.values() {
        assert!(rel(m.tension_lbs, expect) < 1e-6, "{}", m.tension_lbs);
    }
}

#[test]
fn short_leg_in_a_four_leg_unloads_its_neighbours() {
    // Leg 0 an inch short: the rigid hang puts it and its diagonal partner (3)
    // in charge; 1 and 2 go slack.
    let rig = four_leg(0.0, [10.0 - 1.0 / 12.0, 10.0, 10.0, 10.0]);
    let hung = solve(&rig).unwrap();
    assert!(hung.converged);
    let t = |k: usize| hung.tension_of(&rig, &format!("leg {k}")).unwrap();
    assert!(t(0) > 1_500.0 && t(3) > 1_500.0, "{} {}", t(0), t(3));
    assert!(t(1) < 1.0 && t(2) < 1.0, "{} {}", t(1), t(2));
    let v: f64 = hung
        .members
        .values()
        .map(|m| m.stop_forces.last().unwrap()[2])
        .sum();
    assert!(rel(v, 4_000.0) < 1e-8, "{v}");
}

#[test]
fn cg_offset_swings_under_the_hook() {
    // Two legs, CG 1 ft toward leg B: the load tilts until the CG is under the hook.
    let mut b = RigBuilder::new("offset");
    let hook = b.hook("Hook");
    let load = b.load("Load", 10.0, 2.0, 2.0, 5_000.0);
    b.set_cg(load, Coord3::new(1.0, 0.0, 0.0));
    let a = b.lug(load, "A", -5.0, 0.0, 2.0);
    let c = b.lug(load, "B", 5.0, 0.0, 2.0);
    for (l, n) in [("leg A", a), ("leg B", c)] {
        b.member(l)
            .from(hook)
            .to(n)
            .segment(|s| s.push(crate::rig::Component::strap(10.0, 0.0, 2.0)));
    }
    let rig = b.finish().unwrap();
    let hung = solve(&rig).unwrap();
    assert!(hung.converged, "iters {}", hung.iterations);
    let be = hung.eval.bodies.get(&load).unwrap();
    assert!(be.cg_world[0].abs() < 1e-7, "cg {:?}", be.cg_world);
    assert!(be.cg_world[1].abs() < 1e-7);
    let tilt = hung.tilt_deg.get(&load).copied().unwrap();
    assert!(tilt > 1.0, "tilt {tilt}");
    // B (closer to the CG) carries more.
    let ta = hung.tension_of(&rig, "leg A").unwrap();
    let tb = hung.tension_of(&rig, "leg B").unwrap();
    assert!(tb > ta, "{ta} {tb}");
    assert!(rel(hung.hook_load_lbs, 5_000.0) < 1e-9);
}

#[test]
fn duplo10_hangs_and_balances() {
    let rig = duplo10();
    let hung = solve(&rig).unwrap();
    assert!(
        hung.converged,
        "iters {} residual {}",
        hung.iterations, hung.residual_lbs
    );
    let w = hung.eval.weights.total_below_root_lbs;
    assert!(
        rel(hung.hook_load_lbs, w) < 1e-8,
        "{} vs {w}",
        hung.hook_load_lbs
    );
    assert!(hung.determinacy.s > 0, "{:?}", hung.determinacy);
    for m in hung.members.values() {
        assert!(m.tension_lbs >= -1e-6, "{} {}", m.label, m.tension_lbs);
    }
}

#[test]
fn duplo10_nominal_hangs_on_the_baskets_with_chains_slack() {
    // Provisional fixture: 8 ft chain settings leave every chain slack, so
    // the enclosure hangs on the four baskets; bar C carries only itself.
    let rig = duplo10();
    let s = solve(&rig).unwrap();
    for m in s.members.values() {
        if m.label.starts_with("Chain") {
            assert!(!m.taut && m.tension_lbs.abs() < 1e-6, "{}", m.label);
        }
        if m.label.starts_with("Basket") || m.label.starts_with("Hook leg") {
            assert!(m.taut && m.tension_lbs > 1_000.0, "{}", m.label);
        }
    }
    let t = |l: &str| s.tension_of(&rig, l).unwrap();
    // Four baskets share equally; so do the four hook legs.
    assert!(
        rel(
            t("Basket strap P1–P3 (front)"),
            t("Basket strap P5–P7 (back)")
        ) < 1e-9
    );
    assert!(rel(t("Hook leg LF"), t("Hook leg RB")) < 1e-9);
    // Topology: 20 members, 13 independent equations carry, 7 self-stress states.
    assert_eq!((s.topology.m, s.topology.r, s.topology.s), (20, 13, 7));
    for tilt in s.tilt_deg.values() {
        assert!(*tilt < 1e-6);
    }
}

#[test]
fn duplo10_envelope_and_named_cases() {
    let rig = duplo10();
    let s = solve(&rig).unwrap();
    let env = bounds::envelope(&rig, &s).unwrap();
    assert!(env.feasible);
    let id = |l: &str| rig.members.iter().find(|(_, m)| m.label == l).unwrap().0;
    let max = |l: &str| env.max_of(*id(l)).unwrap();
    // Front/back symmetry of the envelope.
    assert!(rel(max("Chain leg P2 front"), max("Chain leg P2 back")) < 1e-6);
    assert!(
        rel(
            max("Basket strap P1–P3 (front)"),
            max("Basket strap P5–P7 (back)")
        ) < 1e-6
    );
    // Any one chain pair could end up with the whole enclosure + its rigging.
    assert!(max("Chain leg P4 front") > 50_000.0);
    // A hook leg can see twice its nominal share when its partner goes slack.
    assert!(
        rel(
            max("Hook leg LF"),
            2.0 * s.tension_of(&rig, "Hook leg LF").unwrap()
        ) < 1e-6
    );

    let cases = bounds::named_cases(&rig, &s.eval).unwrap();
    assert_eq!(cases.len(), 2);
    let baskets = cases
        .iter()
        .find(|c| c.kind == bounds::CaseKind::BasketsSlack)
        .unwrap();
    let b = baskets
        .solved
        .as_ref()
        .expect("chains carry when baskets are slack");
    // With the baskets slack the centre chains (P4) find the load first.
    assert!(b.tension_of(&rig, "Chain leg P4 front").unwrap() > 50_000.0);
    assert!(rel(b.hook_load_lbs, s.hook_load_lbs) < 1e-9);
}

#[test]
fn four_leg_envelope_is_the_diagonal_pair() {
    let rig = four_leg(0.0, [10.0; 4]);
    let hung = solve(&rig).unwrap();
    let env = bounds::envelope(&rig, &hung).unwrap();
    assert!(env.feasible);
    let pair = 4_000.0 / 2.0 / 60f64.to_radians().sin();
    for r in env.members.values() {
        assert!(rel(r.max_lbs, pair) < 1e-6, "{r:?} vs {pair}");
        assert!(r.min_lbs.abs() < 1e-6, "{r:?}");
    }
}

fn offset_with_chains(cg_x: f64) -> Rig {
    use crate::rig::Adjust;
    let mut b = RigBuilder::new("offset-chains");
    let hook = b.hook("Hook");
    let load = b.load("Load", 10.0, 2.0, 2.0, 5_000.0);
    b.set_cg(load, Coord3::new(cg_x, 0.0, 0.0));
    let a = b.lug(load, "A", -5.0, 0.0, 2.0);
    let c = b.lug(load, "B", 5.0, 0.0, 2.0);
    for (l, n) in [("leg A", a), ("leg B", c)] {
        b.member(l).from(hook).to(n).segment(|s| {
            s.strap_w(6.0, 0.0)
                .chain(8, 0.5, 2.0, 0.0, Adjust::new(0.0, 4.0, 2.0))
        });
    }
    b.finish().unwrap()
}

#[test]
fn level_sets_chains_so_an_offset_cg_hangs_level() {
    let rig = offset_with_chains(1.0);
    let before = solve(&rig).unwrap();
    assert!(before.tilt_deg.values().any(|t| *t > 1.0));
    let inv = inverse::level(&rig).unwrap();
    assert!(inv.feasible, "{:?}", inv.changes);
    assert!(
        inv.max_tilt_deg < inverse::LEVEL_TOL_DEG,
        "{}",
        inv.max_tilt_deg
    );
    // Level with the CG under the hook: lugs at x = -6 and +4, same height.
    // Leg lengths are the distances hook → lug; B (nearer the CG) is shorter.
    let a = inv.changes.iter().find(|c| c.label == "leg A").unwrap();
    let b = inv.changes.iter().find(|c| c.label == "leg B").unwrap();
    assert!(b.to_ft < a.to_ft, "{a:?} {b:?}");
    // Check the geometry: hook height h with 8 + a = sqrt(36 + h²), 8 + b = sqrt(16 + h²).
    let la = 8.0 + a.to_ft;
    let lb = 8.0 + b.to_ft;
    let ha = (la * la - 36.0).sqrt();
    let hb = (lb * lb - 16.0).sqrt();
    assert!((ha - hb).abs() < 1e-6, "{ha} {hb}");
    // Statics: tension ratio from moments about the CG.
    let ta = inv.solved.tension_of(&inv.rig, "leg A").unwrap();
    let tb = inv.solved.tension_of(&inv.rig, "leg B").unwrap();
    let va = ta * ha / la;
    let vb = tb * hb / lb;
    assert!(rel(va * 6.0, vb * 4.0) < 1e-6, "{va} {vb}");
    assert!(rel(va + vb, 5_000.0) < 1e-8);
}

#[test]
fn take_up_reports_duplo10_chain_settings() {
    let rig = duplo10();
    let inv = inverse::take_up(&rig).unwrap();
    // Provisional fixture numbers: strap + chain + 8 ft setting is far longer
    // than the bar-lug-to-pick gap, so the P2/P6 take-up runs past the 6 ft
    // minimum setting. The inverse reports it instead of clamping.
    assert!(!inv.feasible);
    assert_eq!(inv.changes.len(), 6, "{:?}", inv.changes);
    for c in &inv.changes {
        assert!(c.to_ft < c.from_ft, "{c:?}");
    }
    assert!(inv.changes.iter().any(|c| !c.in_range()));
}

#[test]
fn duplo10_rates_and_bounds_warn() {
    use crate::checks::Status;
    let rig = duplo10();
    let solved = solve(&rig).unwrap();
    let rated = rate::rate(&rig, &solved).unwrap();
    assert!(!rated.is_empty());
    let env = bounds::envelope(&rig, &solved).unwrap();
    let bounded = rate::with_bounds(&rig, &solved, &env).unwrap();
    // Chains are slack as rigged; the envelope says a chain could see far more
    // than a 1/2" G80's 12 000 lb.
    assert!(
        bounded
            .iter()
            .any(|r| r.owner.starts_with("Chain") && r.check.status == Status::Warn)
    );
}

#[test]
fn bar_compression_is_v_over_tan_theta() {
    use crate::checks::bar::compression_from_angle;
    // Hook → two 60° legs to the bar ends; the load hangs plumb from the ends.
    let mut b = RigBuilder::new("bar");
    let hook = b.hook("Hook");
    let bar = b.spreader("Bar", 10.0, 200.0, 20_000);
    let e1 = b.lug(bar, "end 1", -5.0, 0.0, 0.0);
    let e2 = b.lug(bar, "end 2", 5.0, 0.0, 0.0);
    for (l, n) in [("top 1", e1), ("top 2", e2)] {
        b.member(l)
            .from(hook)
            .to(n)
            .segment(|s| s.push(crate::rig::Component::strap(10.0, 0.0, 2.0)));
    }
    let load = b.load("Load", 10.0, 2.0, 2.0, 10_000.0);
    let p1 = b.lug(load, "P1", -5.0, 0.0, 2.0);
    let p2 = b.lug(load, "P2", 5.0, 0.0, 2.0);
    for (l, a, c) in [("drop 1", e1, p1), ("drop 2", e2, p2)] {
        b.member(l)
            .from(a)
            .to(c)
            .segment(|s| s.push(crate::rig::Component::strap(6.0, 0.0, 2.0)));
    }
    let rig = b.finish().unwrap();
    let s = solve(&rig).unwrap();
    let rated = rate::rate(&rig, &s).unwrap();
    let bar_check = rated.iter().find(|r| r.owner == "Bar").unwrap();
    assert!(rel(bar_check.check.demand_lbs, 10_000.0) < 1e-8);
    let p = rate::bar_axial(&rig, &s, bar).unwrap();
    let expect = compression_from_angle((10_000.0 + 200.0) / 2.0, 60.0);
    assert!(rel(p, expect) < 1e-6, "{p} vs {expect}");
}

#[test]
fn solved_scene_labels_tensions_and_flags_overload() {
    use crate::rig::views::{Item, Role, Scene, ViewKind};
    let rig = duplo10();
    let s = solve(&rig).unwrap();
    let rated = rate::rate(&rig, &s).unwrap();
    let scene = Scene::project_solved(&s, &rig, ViewKind::Side, &rated);
    assert!(!scene.has_nan());
    assert!(scene.count_role(Role::Overloaded) > 0);
    assert!(scene.count_role(Role::SlackMember) >= 6, "chains");
    assert!(scene.items.iter().any(|i| matches!(i,
        Item::Polyline { label: Some(l), .. } if l.ends_with(" lb"))));
}
