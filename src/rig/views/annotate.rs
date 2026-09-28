//! Automatic dimensions: spans, pick spacing, hook height, angle arcs, CG offset.

use crate::format::format_num;
use crate::rig::Rig;
use crate::rig::body::BodyKind;
use crate::rig::eval::{EvalRig, ResidualKind};
use crate::rig::node::NodeKind;
use crate::rig::param::ParamSource;

use super::{Item, Role, ViewKind, project_pt};

pub fn annotate(items: &mut Vec<Item>, eval: &EvalRig, rig: &Rig, kind: ViewKind) {
    let assumed = eval.assumed
        || rig
            .params
            .values()
            .any(|p| p.source == ParamSource::Assumed);
    let tag = |s: String| {
        if assumed { format!("{s} (assumed)") } else { s }
    };

    spreader_spans(items, eval, rig, kind, &tag);
    pick_spacing(items, eval, rig, kind, &tag);
    if kind != ViewKind::Plan {
        hook_height(items, eval, rig, kind, &tag);
        governing_arc(items, eval, rig, kind, &tag);
    } else {
        cg_offset(items, eval, rig, &tag);
    }

    if eval.has_residual(ResidualKind::Short)
        || eval.has_residual(ResidualKind::UnequalDrop)
        || eval.has_residual(ResidualKind::BearingSplitAssumed)
    {
        items.push(Item::Text {
            at: [eval.root_at[0], eval.root_at[2]],
            text: tag("check residuals".into()),
            role: Role::Warning,
        });
    }
}

fn spreader_spans(
    items: &mut Vec<Item>,
    eval: &EvalRig,
    rig: &Rig,
    kind: ViewKind,
    tag: &dyn Fn(String) -> String,
) {
    for (id, body) in &rig.bodies {
        if !matches!(
            body.kind,
            BodyKind::SpreaderBar { .. } | BodyKind::LiftingBeam { .. }
        ) {
            continue;
        }
        let pts: Vec<[f64; 3]> = rig
            .nodes
            .values()
            .filter(|n| n.body == Some(*id))
            .filter_map(|n| eval.nodes.get(&n.id).copied())
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let mut best = (0usize, 1usize, 0.0_f64);
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let d = plan_dist(pts[i], pts[j]);
                if d > best.2 {
                    best = (i, j, d);
                }
            }
        }
        if best.2 < 1e-6 {
            continue;
        }
        let a = project_pt(pts[best.0], kind).0;
        let b = project_pt(pts[best.1], kind).0;
        items.push(Item::Dim {
            from: a,
            to: b,
            text: tag(format!("{} ft", format_num(best.2))),
            offset_ft: 1.0,
        });
    }
}

fn pick_spacing(
    items: &mut Vec<Item>,
    eval: &EvalRig,
    rig: &Rig,
    kind: ViewKind,
    tag: &dyn Fn(String) -> String,
) {
    let mut lugs: Vec<[f64; 3]> = rig
        .nodes
        .values()
        .filter(|n| {
            matches!(n.kind, NodeKind::Lug { .. })
                && n.body
                    .and_then(|b| rig.bodies.get(&b))
                    .is_some_and(|b| b.kind.is_load())
        })
        .filter_map(|n| eval.nodes.get(&n.id).copied())
        .collect();
    if lugs.len() < 2 {
        return;
    }
    lugs.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
    let mut xs: Vec<f64> = Vec::new();
    for p in &lugs {
        if xs.last().is_none_or(|q| (p[0] - q).abs() > 1e-6) {
            xs.push(p[0]);
        }
    }
    for w in xs.windows(2) {
        let span = (w[1] - w[0]).abs();
        if span < 1e-6 {
            continue;
        }
        // Dimension along the load, at the pick elevation.
        let z = lugs[0][2];
        let y = lugs[0][1];
        let a = project_pt([w[0], y, z], kind).0;
        let b = project_pt([w[1], y, z], kind).0;
        items.push(Item::Dim {
            from: a,
            to: b,
            text: tag(format!("{} ft", format_num(span))),
            offset_ft: 0.6,
        });
    }
}

fn hook_height(
    items: &mut Vec<Item>,
    eval: &EvalRig,
    rig: &Rig,
    kind: ViewKind,
    tag: &dyn Fn(String) -> String,
) {
    let hook = project_pt(eval.root_at, kind).0;
    let mut pick = eval.root_at;
    let mut saw = false;
    for (nid, node) in &rig.nodes {
        let Some(bid) = node.body else { continue };
        if !rig.bodies.get(&bid).is_some_and(|b| b.kind.is_load()) {
            continue;
        }
        if !matches!(node.kind, NodeKind::Lug { .. }) {
            continue;
        }
        if let Some(p) = eval.nodes.get(nid) {
            if !saw || p[2] < pick[2] {
                pick = *p;
                saw = true;
            }
        }
    }
    if !saw {
        return;
    }
    let to = project_pt(pick, kind).0;
    items.push(Item::Dim {
        from: hook,
        to,
        text: tag(format!("{} ft", format_num(eval.height.hook_to_pick_ft))),
        offset_ft: 1.2,
    });
}

fn governing_arc(
    items: &mut Vec<Item>,
    eval: &EvalRig,
    rig: &Rig,
    kind: ViewKind,
    tag: &dyn Fn(String) -> String,
) {
    let mut best: Option<(f64, [f64; 3], [f64; 3], f64)> = None;
    for (id, m) in &eval.members {
        if m.points.len() < 2 {
            continue;
        }
        for i in 0..m.chords_ft.len() {
            let ang = m.angle_deg[i];
            let better = best.map(|(a, _, _, _)| ang < a).unwrap_or(true);
            if better && m.nominal_ft[i] > crate::rig::eval::MISSING_LENGTH_FT {
                best = Some((ang, m.points[i], m.points[i + 1], m.reach_ft[i]));
            }
        }
        let _ = id;
        let _ = rig;
    }
    let Some((ang, a, b, _reach)) = best else {
        return;
    };
    // Arc at the lower end, from horizontal up to the member.
    let lower = if a[2] <= b[2] { a } else { b };
    let (at, _) = project_pt(lower, kind);
    items.push(Item::Arc {
        at,
        from_deg: 0.0,
        to_deg: ang,
        radius_ft: 1.5,
        text: tag(format!("{}°", format_num(ang))),
    });
}

fn cg_offset(items: &mut Vec<Item>, eval: &EvalRig, rig: &Rig, tag: &dyn Fn(String) -> String) {
    for (id, body) in &rig.bodies {
        if !body.kind.is_load() {
            continue;
        }
        let Some(be) = eval.bodies.get(id) else {
            continue;
        };
        let Some(s) = be.support_centroid else {
            continue;
        };
        if be.cg_offset_ft < 1e-4 {
            continue;
        }
        let a = project_pt(be.cg_world, ViewKind::Plan).0;
        let b = project_pt(s, ViewKind::Plan).0;
        items.push(Item::Dim {
            from: a,
            to: b,
            text: tag(format!("{} ft", format_num(be.cg_offset_ft))),
            offset_ft: 0.4,
        });
    }
}

fn plan_dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}
