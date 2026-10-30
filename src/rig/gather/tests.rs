use super::*;
use crate::rig::edit::ParamEdit;
use crate::rig::preset::{self, BodyPreset, PickPattern};
use crate::rig::session::RigSession;
use crate::rig::solve::{self, SolvedRig};
use crate::rig::topo::{self, BodyField};

fn two_point() -> Rig {
    preset::blank_rig("gather", uuid::Uuid::nil())
}

fn four_corner() -> Rig {
    let rig = preset::hook_only("gather", uuid::Uuid::nil());
    let add = preset::add_body(
        &rig,
        BodyPreset::Load {
            pattern: PickPattern::FourCorners,
        },
    );
    topo::apply(&rig, &add).unwrap()
}

fn load_of(rig: &Rig) -> BodyId {
    rig.bodies.values().find(|b| b.kind.is_load()).unwrap().id
}

fn open_nodes(rig: &Rig) -> Vec<NodeId> {
    status(rig).open.iter().map(|p| p.node).collect()
}

fn named(rig: &Rig, label: &str) -> NodeId {
    rig.node_named(label).unwrap().id
}

fn solved(rig: &Rig) -> SolvedRig {
    rig.validate().expect("valid");
    let s = solve::solve(rig).expect("solves");
    assert!(s.converged, "solver converged");
    let w = rig.weights().unwrap();
    assert!(
        (s.hook_load_lbs - w.total_below_root_lbs).abs() < 1e-6 * w.total_below_root_lbs.max(1.0),
        "hook load {} vs weight {}",
        s.hook_load_lbs,
        w.total_below_root_lbs
    );
    s
}

fn load_tilt(rig: &Rig, s: &SolvedRig) -> f64 {
    s.tilt_deg.get(&load_of(rig)).copied().unwrap_or(0.0)
}

fn gathered(rig: &Rig, ends: &[NodeId], kind: GatherKind) -> GatherPlan {
    gather(rig, ends, kind, GatherOptions::default()).expect("gather")
}

#[test]
fn a_blank_rig_starts_with_its_pick_points_open() {
    let rig = two_point();
    let st = status(&rig);
    let labels: Vec<&str> = st.open.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["P1", "P2"]);
    assert_eq!(st.unhung, vec![load_of(&rig)]);
    assert!(!st.done);
}

#[test]
fn two_points_to_the_hook_hang_level_at_sixty_degrees() {
    let rig = two_point();
    let plan = gathered(&rig, &open_nodes(&rig), GatherKind::Hook);
    assert!(status(&plan.rig).done);
    // 10 ft load: h = 5, rise = 5·tan 60° → each leg exactly 10 ft.
    for m in plan.rig.members.values() {
        let len = m.nominal_length(&plan.rig.params).unwrap();
        assert!((len - 10.0).abs() < 1e-9, "leg {len}");
    }
    let s = solved(&plan.rig);
    assert!(load_tilt(&plan.rig, &s) < 0.05);
    let expect = 10_000.0 / 2.0 / 60f64.to_radians().sin();
    for m in s.members.values() {
        assert!(
            (m.tension_lbs - expect).abs() / expect < 0.01,
            "{}",
            m.tension_lbs
        );
    }
}

#[test]
fn an_offset_cg_still_hangs_level() {
    let rig = two_point();
    let load = load_of(&rig);
    let rig = topo::apply(
        &rig,
        &TopoEdit::SetBodyExpr {
            id: load,
            field: BodyField::CgX,
            text: "2".into(),
        },
    )
    .unwrap();
    let plan = gathered(&rig, &open_nodes(&rig), GatherKind::Hook);
    let lengths: Vec<f64> = plan
        .rig
        .members
        .values()
        .map(|m| m.nominal_length(&plan.rig.params).unwrap())
        .collect();
    assert!(
        (lengths[0] - lengths[1]).abs() > 1.0,
        "legs differ: {lengths:?}"
    );
    let s = solved(&plan.rig);
    // Only the inch rounding of the legs is left.
    assert!(
        load_tilt(&plan.rig, &s) < 0.5,
        "tilt {}",
        load_tilt(&plan.rig, &s)
    );
}

#[test]
fn four_corners_gather_to_a_master_link_then_the_hook() {
    let rig = four_corner();
    let plan = gathered(&rig, &open_nodes(&rig), GatherKind::MasterLink);
    assert_eq!(plan.rig.members.len(), 4);
    let link = plan.new_points[0];
    assert_eq!(open_nodes(&plan.rig), vec![link]);
    let hooked = gathered(&plan.rig, &[link], GatherKind::Hook);
    assert!(status(&hooked.rig).done);
    let s = solved(&hooked.rig);
    assert!(load_tilt(&hooked.rig, &s) < 0.05);
}

#[test]
fn a_spreader_spans_the_points_with_plumb_legs() {
    let rig = two_point();
    let plan = gathered(&rig, &open_nodes(&rig), GatherKind::SpreaderBar);
    assert!((plan.rig.param_named("bar1_span").unwrap().nominal - 10.0).abs() < 1e-9);
    assert_eq!(plan.new_points.len(), 2, "top lugs open");
    assert_eq!(open_nodes(&plan.rig), plan.new_points);
    let hooked = gathered(&plan.rig, &plan.new_points, GatherKind::Hook);
    let s = solved(&hooked.rig);
    let bottom = [
        named(&hooked.rig, "Bottom A"),
        named(&hooked.rig, "Bottom B"),
    ];
    for (id, m) in &hooked.rig.members {
        if bottom.contains(&m.path[0]) {
            let t = s.members[id].tension_lbs;
            assert!((t - 5_000.0).abs() < 60.0, "drop leg {t}");
        }
    }
    assert!(load_tilt(&hooked.rig, &s) < 0.05);
}

#[test]
fn every_recipe_builds_a_rig_that_solves() {
    for rig in [two_point(), four_corner()] {
        let load = load_of(&rig);
        let recipes = recipes_for(&rig, load);
        assert!(recipes.len() >= 4);
        for recipe in recipes {
            let plan = apply_recipe(&rig, load, recipe, GatherOptions::default())
                .unwrap_or_else(|e| panic!("{}: {e}", recipe.label()));
            let st = status(&plan.rig);
            assert!(st.done, "{} leaves {:?}", recipe.label(), st.open);
            let s = solved(&plan.rig);
            let tilt = load_tilt(&plan.rig, &s);
            assert!(tilt < 0.5, "{} tilts {tilt}", recipe.label());
        }
    }
}

#[test]
fn ungather_puts_the_rig_back_exactly() {
    let rig = two_point();
    let plan = gathered(&rig, &open_nodes(&rig), GatherKind::SpreaderBar);
    let bar = plan
        .rig
        .bodies
        .values()
        .find(|b| b.kind.is_gear())
        .unwrap()
        .id;
    let undone = ungather(&plan.rig, Built::Item(TreeVertex::Body(bar))).unwrap();
    assert_eq!(undone.rig, rig);

    let full = apply_recipe(
        &rig,
        load_of(&rig),
        Recipe::Spreader,
        GatherOptions::default(),
    )
    .unwrap();
    assert_eq!(built(&full.rig)[0].0, Built::Hook);
    assert_eq!(clear(&full.rig).unwrap().rig, rig);
}

#[test]
fn a_gather_is_one_undo_step() {
    let rig = four_corner();
    let plan = apply_recipe(
        &rig,
        load_of(&rig),
        Recipe::SpreaderOverTwo,
        GatherOptions::default(),
    )
    .unwrap();
    let mut session = RigSession::new(rig.clone());
    session.apply_batch(&plan.edits).unwrap();
    assert_eq!(session.rig(), &plan.rig);
    assert!(session.undo());
    assert_eq!(session.rig(), &rig);
    assert!(!session.can_undo());
}

#[test]
fn gathers_refuse_what_does_not_fit() {
    let rig = two_point();
    let ends = open_nodes(&rig);
    assert!(check(&rig, &ends[..1], GatherKind::SpreaderBar).is_err());
    assert!(check(&rig, &ends[..1], GatherKind::MasterLink).is_err());
    assert!(check(&rig, &ends, GatherKind::Basket).is_err());
    assert!(check(&rig, &[], GatherKind::Hook).is_err());

    let plan = gathered(&rig, &ends[..1], GatherKind::Hook);
    let err = check(&plan.rig, &ends, GatherKind::Hook).unwrap_err();
    assert!(err.contains("already rigged"), "{err}");

    // A bow takes a basket only.
    let load = load_of(&rig);
    let bow = preset::port(&rig, load, preset::PortKind::Bow);
    let TopoEdit::AddNode { node } = &bow else {
        unreachable!()
    };
    let bow_id = node.id;
    let with_bow = topo::apply(&rig, &bow).unwrap();
    let err = check(&with_bow, &[ends[0], bow_id], GatherKind::MasterLink).unwrap_err();
    assert!(err.contains("basket"), "{err}");
    assert!(check(&with_bow, &[bow_id], GatherKind::Basket).is_ok());
}

#[test]
fn auto_size_picks_hardware_that_carries_the_solved_tension() {
    let rig = two_point();
    let w = rig.param_named("load_weight").unwrap().id;
    let rig = crate::rig::edit::apply(
        &rig,
        &ParamEdit::SetValue {
            id: w,
            text: "60000".into(),
        },
    )
    .unwrap();
    let plan = gathered(&rig, &open_nodes(&rig), GatherKind::Hook);
    let s = solved(&plan.rig);
    for (id, m) in &plan.rig.members {
        let rating = m.min_rating().expect("rated hardware");
        let t = s.members[id].tension_lbs;
        assert!(
            f64::from(rating.wll_lbs) >= t,
            "{} WLL {} < {t}",
            rating.label,
            rating.wll_lbs
        );
    }
    // And not the default size, which is far too small for 60 kip.
    let off = GatherOptions {
        auto_size: false,
        ..GatherOptions::default()
    };
    let plain = gather(&rig, &open_nodes(&rig), GatherKind::Hook, off).unwrap();
    assert_ne!(plain.rig, plan.rig);
}

#[test]
fn a_flatter_target_angle_gives_longer_legs() {
    let rig = two_point();
    let ends = open_nodes(&rig);
    let len = |angle: f64| {
        let opts = GatherOptions {
            angle_deg: angle,
            ..GatherOptions::default()
        };
        let plan = gather(&rig, &ends, GatherKind::Hook, opts).unwrap();
        let m = plan.rig.members.values().next().unwrap().clone();
        m.nominal_length(&plan.rig.params).unwrap()
    };
    // 45°: h = 5 → 7.07 ft rounded up to the inch.
    assert!((len(45.0) - 85.0 / 12.0).abs() < 1e-9, "{}", len(45.0));
    assert!(len(45.0) < len(60.0));
    assert!(len(75.0) > len(60.0));
}

#[test]
fn principal_axis_runs_along_the_long_side() {
    let rect = [
        [-5.0, -2.0, 0.0],
        [5.0, -2.0, 0.0],
        [-5.0, 2.0, 0.0],
        [5.0, 2.0, 0.0],
    ];
    let u = principal_axis(&rect);
    assert!((u[0] - 1.0).abs() < 1e-12 && u[1].abs() < 1e-12);
    let (a, b) = split_along_axis(&rect);
    assert_eq!(a, vec![0, 2]);
    assert_eq!(b, vec![1, 3]);
}
