//! Nominal descent: one pass, least-squares in plan, max-plus in elevation.
//!
//! Step: 2
//! Must not iterate. A case that needs a solve gets a residual and waits for Step 3.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use super::{
    Frame, ItemRef, PlacementUsed, Residual, ResidualKind, Rot, Severity, theoretical_drop,
};
use crate::rig::body::Placement;
use crate::rig::id::{BodyId, MemberId, NodeId};
use crate::rig::node::NodeKind;
use crate::rig::param::ParamTable;
use crate::rig::{Rig, RigError, RigErrorKind};

/// Lengths below this are treated as missing input (legacy `sling_length_ft = 0`),
/// not as a Short/GEOM failure.
pub const MISSING_LENGTH_FT: f64 = 1e-6;
/// Same inch threshold the layer engine uses for the LEGS flag.
pub const UNEQUAL_DROP_TOL_IN: f64 = 1.0;

const EPS: f64 = 1e-12;

pub struct Placed {
    pub nodes: IndexMap<NodeId, [f64; 3]>,
    pub frames: IndexMap<BodyId, Frame>,
    pub used: IndexMap<BodyId, PlacementUsed>,
    pub support_centroid: IndexMap<BodyId, Option<[f64; 3]>>,
    pub residuals: Vec<Residual>,
    pub split_nominal: HashMap<(MemberId, usize), f64>,
}

struct Leg {
    support: [f64; 3],
    local: [f64; 3],
    member: MemberId,
    segment: usize,
    length: f64,
    missing: bool,
}

pub fn place(rig: &Rig) -> Result<Placed, Vec<RigError>> {
    let params = &rig.params;
    let mut nodes: IndexMap<NodeId, [f64; 3]> = IndexMap::new();
    let mut frames: IndexMap<BodyId, Frame> = IndexMap::new();
    let mut used: IndexMap<BodyId, PlacementUsed> = IndexMap::new();
    let mut support_centroid: IndexMap<BodyId, Option<[f64; 3]>> = IndexMap::new();
    let mut residuals = Vec::new();
    let mut split_nominal: HashMap<(MemberId, usize), f64> = HashMap::new();
    let mut placed_bodies: HashSet<BodyId> = HashSet::new();

    let root_node = rig.nodes.get(&rig.root).ok_or_else(|| {
        vec![RigError::new(
            RigErrorKind::DanglingRef,
            "root node missing",
        )]
    })?;
    let hook_id = root_node.body.ok_or_else(|| {
        vec![RigError::new(
            RigErrorKind::NodePlacement,
            "root has no hook body",
        )]
    })?;
    let hook = rig.bodies.get(&hook_id).ok_or_else(|| {
        vec![RigError::new(
            RigErrorKind::DanglingRef,
            "hook body missing",
        )]
    })?;
    let (hook_frame, hook_used) = authored_frame(hook, params, [0.0, 0.0, 0.0], Rot::identity())?;
    apply_body(rig, hook_id, hook_frame, &mut nodes);
    frames.insert(hook_id, hook_frame);
    used.insert(hook_id, hook_used);
    support_centroid.insert(hook_id, None);
    placed_bodies.insert(hook_id);

    let mut guard = 0;
    loop {
        guard += 1;
        if guard > rig.bodies.len() + rig.nodes.len() + 4 {
            break;
        }
        let mut progress = false;
        if place_free_nodes(rig, params, &mut nodes)? {
            progress = true;
        }
        let mut candidates: Vec<BodyId> = rig
            .bodies
            .keys()
            .copied()
            .filter(|id| !placed_bodies.contains(id) && body_is_placeable(rig, *id, &nodes))
            .collect();
        // Place gear before the load so a long centre hang doesn't steal the
        // descent before the side bars exist (Duplo10: hook→Bar C→load vs baskets).
        candidates.sort_by_key(|id| rig.bodies.get(id).is_some_and(|b| b.kind.is_load()));
        for bid in candidates {
            let body = rig.bodies.get(&bid).unwrap();
            let rot = rotation_of(body, params)?;
            let raw = collect_legs(rig, params, bid, &nodes, &rot)?;
            let (legs, splits, assumed) = split_bearings(raw, &rot);
            for ((mid, si), len) in splits {
                split_nominal.insert((mid, si), len);
            }
            if assumed {
                residuals.push(Residual {
                    kind: ResidualKind::BearingSplitAssumed,
                    at: ItemRef::Body(bid),
                    value: 0.0,
                    message: format!(
                        "body '{}': strap split at a bow was assumed (proportional to reach)",
                        body.label
                    ),
                    severity: Severity::Warn,
                });
            }
            let (frame, used_p, centroid, extra) = place_from_legs(body, params, &rot, &legs)?;
            residuals.extend(extra);
            apply_body(rig, bid, frame, &mut nodes);
            frames.insert(bid, frame);
            used.insert(bid, used_p);
            support_centroid.insert(bid, centroid);
            placed_bodies.insert(bid);
            progress = true;
        }
        if !progress {
            break;
        }
    }

    for node in rig.nodes.values() {
        if !nodes.contains_key(&node.id) {
            return Err(vec![RigError::new(
                RigErrorKind::Disconnected,
                format!("node '{}' was not placed", node.label),
            )]);
        }
    }
    Ok(Placed {
        nodes,
        frames,
        used,
        support_centroid,
        residuals,
        split_nominal,
    })
}

fn authored_frame(
    body: &crate::rig::Body,
    params: &ParamTable,
    fallback_origin: [f64; 3],
    fallback_rot: Rot,
) -> Result<(Frame, PlacementUsed), Vec<RigError>> {
    match body.placement() {
        Placement::Derived => Ok((
            Frame {
                origin: fallback_origin,
                rot: fallback_rot,
            },
            PlacementUsed::Derived,
        )),
        Placement::Pinned { at } => {
            let origin = at.eval(params).map_err(|e| vec![e])?;
            Ok((
                Frame {
                    origin,
                    rot: fallback_rot,
                },
                PlacementUsed::Pinned,
            ))
        }
        Placement::Posed { at, rot } => {
            let origin = at.eval(params).map_err(|e| vec![e])?;
            let [yaw, pitch, roll] = rot.eval(params).map_err(|e| vec![e])?;
            Ok((
                Frame {
                    origin,
                    rot: Rot::from_degrees(yaw, pitch, roll),
                },
                PlacementUsed::Posed,
            ))
        }
        Placement::Level { rot } => {
            let [yaw, pitch, roll] = rot.eval(params).map_err(|e| vec![e])?;
            Ok((
                Frame {
                    origin: fallback_origin,
                    rot: Rot::from_degrees(yaw, pitch, roll),
                },
                PlacementUsed::Level,
            ))
        }
    }
}

fn rotation_of(body: &crate::rig::Body, params: &ParamTable) -> Result<Rot, Vec<RigError>> {
    match body.placement() {
        Placement::Posed { rot, .. } | Placement::Level { rot } => {
            let [yaw, pitch, roll] = rot.eval(params).map_err(|e| vec![e])?;
            Ok(Rot::from_degrees(yaw, pitch, roll))
        }
        _ => Ok(Rot::identity()),
    }
}

fn apply_body(rig: &Rig, bid: BodyId, frame: Frame, nodes: &mut IndexMap<NodeId, [f64; 3]>) {
    for node in rig.nodes.values() {
        if node.body == Some(bid) {
            let local = node.local.eval(&rig.params).unwrap_or([0.0, 0.0, 0.0]);
            nodes.insert(node.id, frame.transform(local));
        }
    }
}

fn body_is_placeable(rig: &Rig, bid: BodyId, nodes: &IndexMap<NodeId, [f64; 3]>) -> bool {
    for member in rig.members.values() {
        let mut has_placed = false;
        let mut has_body = false;
        for nid in &member.path {
            if nodes.contains_key(nid) {
                has_placed = true;
            }
            if rig.nodes.get(nid).and_then(|n| n.body) == Some(bid) {
                has_body = true;
            }
        }
        if has_placed && has_body {
            return true;
        }
    }
    false
}

fn place_free_nodes(
    rig: &Rig,
    params: &ParamTable,
    nodes: &mut IndexMap<NodeId, [f64; 3]>,
) -> Result<bool, Vec<RigError>> {
    let mut progress = false;
    for member in rig.members.values() {
        for (i, w) in member.path.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let a_here = nodes.contains_key(&a);
            let b_here = nodes.contains_key(&b);
            if a_here == b_here {
                continue;
            }
            let (placed, free_id) = if a_here { (a, b) } else { (b, a) };
            let Some(free) = rig.nodes.get(&free_id) else {
                continue;
            };
            if free.body.is_some() || !matches!(free.kind, NodeKind::Free) {
                continue;
            }
            let len = member.segments[i]
                .eval_length(params)
                .map_err(|e| vec![e])?;
            let hang = if len < MISSING_LENGTH_FT { 0.0 } else { len };
            let s = nodes[&placed];
            nodes.insert(free_id, [s[0], s[1], s[2] - hang]);
            progress = true;
        }
    }
    Ok(progress)
}

enum RawLeg {
    Simple(Leg),
    Bearing {
        support: [f64; 3],
        left: (NodeId, [f64; 3], usize),
        right: (NodeId, [f64; 3], usize),
        member: MemberId,
        total: f64,
    },
}

fn collect_legs(
    rig: &Rig,
    params: &ParamTable,
    bid: BodyId,
    nodes: &IndexMap<NodeId, [f64; 3]>,
    _rot: &Rot,
) -> Result<Vec<RawLeg>, Vec<RigError>> {
    let mut out = Vec::new();
    for member in rig.members.values() {
        let n = member.path.len();
        if n < 2 {
            continue;
        }
        let mut i = 0;
        while i + 1 < n {
            let a = member.path[i];
            let b = member.path[i + 1];
            let na = rig.nodes.get(&a);
            let nb = rig.nodes.get(&b);
            let a_body = na.and_then(|n| n.body) == Some(bid);
            let b_body = nb.and_then(|n| n.body) == Some(bid);
            let a_pl = nodes.contains_key(&a);
            let b_pl = nodes.contains_key(&b);

            if i + 2 < n {
                let c = member.path[i + 2];
                let nc = rig.nodes.get(&c);
                let c_body = nc.and_then(|n| n.body) == Some(bid);
                let mid = rig.nodes.get(&b);
                let mid_placed = nodes.contains_key(&b);
                let mid_bearing = mid.is_some_and(|n| n.kind.is_bearing_capable());
                if a_body && c_body && mid_placed && mid_bearing && !a_pl && !nodes.contains_key(&c)
                {
                    let l0 = member.segments[i]
                        .eval_length(params)
                        .map_err(|e| vec![e])?;
                    let l1 = member.segments[i + 1]
                        .eval_length(params)
                        .map_err(|e| vec![e])?;
                    let la = na.unwrap().local.eval(params).map_err(|e| vec![e])?;
                    let lc = nc.unwrap().local.eval(params).map_err(|e| vec![e])?;
                    out.push(RawLeg::Bearing {
                        support: nodes[&b],
                        left: (a, la, i),
                        right: (c, lc, i + 1),
                        member: member.id,
                        total: l0 + l1,
                    });
                    i += 2;
                    continue;
                }
            }

            if a_pl && b_body && !b_pl {
                let len = member.segments[i]
                    .eval_length(params)
                    .map_err(|e| vec![e])?;
                let local = nb.unwrap().local.eval(params).map_err(|e| vec![e])?;
                out.push(RawLeg::Simple(Leg {
                    support: nodes[&a],
                    local,
                    member: member.id,
                    segment: i,
                    length: len,
                    missing: len < MISSING_LENGTH_FT,
                }));
            } else if b_pl && a_body && !a_pl {
                let len = member.segments[i]
                    .eval_length(params)
                    .map_err(|e| vec![e])?;
                let local = na.unwrap().local.eval(params).map_err(|e| vec![e])?;
                out.push(RawLeg::Simple(Leg {
                    support: nodes[&b],
                    local,
                    member: member.id,
                    segment: i,
                    length: len,
                    missing: len < MISSING_LENGTH_FT,
                }));
            }
            i += 1;
        }
    }
    Ok(out)
}

fn split_bearings(
    raw: Vec<RawLeg>,
    rot: &Rot,
) -> (Vec<Leg>, HashMap<(MemberId, usize), f64>, bool) {
    // Plan offset from every attachment, independent of lengths.
    let mut supports = Vec::new();
    let mut rlocals = Vec::new();
    let mut peek: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for r in &raw {
        match r {
            RawLeg::Simple(l) => {
                peek.push((l.support, l.local));
            }
            RawLeg::Bearing {
                support,
                left,
                right,
                ..
            } => {
                peek.push((*support, left.1));
                peek.push((*support, right.1));
            }
        }
    }
    for (s, l) in &peek {
        supports.push(*s);
        rlocals.push(rot.apply(*l));
    }
    let t_xy = plan_offset(&supports, &rlocals);

    let mut legs = Vec::new();
    let mut splits = HashMap::new();
    let mut assumed = false;
    for r in raw {
        match r {
            RawLeg::Simple(l) => legs.push(l),
            RawLeg::Bearing {
                support,
                left,
                right,
                member,
                total,
            } => {
                let missing = total < MISSING_LENGTH_FT;
                let rl = rot.apply(left.1);
                let rr = rot.apply(right.1);
                let h1 = hypot(
                    support[0] - (rl[0] + t_xy[0]),
                    support[1] - (rl[1] + t_xy[1]),
                );
                let h2 = hypot(
                    support[0] - (rr[0] + t_xy[0]),
                    support[1] - (rr[1] + t_xy[1]),
                );
                let (l1, l2, flag) = if missing {
                    (total / 2.0, total / 2.0, false)
                } else if (h1 - h2).abs() < 1e-9 {
                    (total / 2.0, total / 2.0, false)
                } else if h1 + h2 < EPS {
                    (total / 2.0, total / 2.0, false)
                } else {
                    (total * h1 / (h1 + h2), total * h2 / (h1 + h2), true)
                };
                assumed |= flag;
                splits.insert((member, left.2), l1);
                splits.insert((member, right.2), l2);
                legs.push(Leg {
                    support,
                    local: left.1,
                    member,
                    segment: left.2,
                    length: l1,
                    missing,
                });
                legs.push(Leg {
                    support,
                    local: right.1,
                    member,
                    segment: right.2,
                    length: l2,
                    missing,
                });
            }
        }
    }
    (legs, splits, assumed)
}

fn place_from_legs(
    body: &crate::rig::Body,
    params: &ParamTable,
    rot: &Rot,
    legs: &[Leg],
) -> Result<(Frame, PlacementUsed, Option<[f64; 3]>, Vec<Residual>), Vec<RigError>> {
    let mut residuals = Vec::new();
    let used = PlacementUsed::from_placement(&body.placement());

    let rlocals: Vec<[f64; 3]> = legs.iter().map(|l| rot.apply(l.local)).collect();
    let supports: Vec<[f64; 3]> = legs.iter().map(|l| l.support).collect();
    let t_xy = plan_offset(&supports, &rlocals);

    let mut drops = Vec::new();
    let mut tzs = Vec::new();
    for (leg, rl) in legs.iter().zip(rlocals.iter()) {
        let h = hypot(
            leg.support[0] - (rl[0] + t_xy[0]),
            leg.support[1] - (rl[1] + t_xy[1]),
        );
        let (d, short) = if leg.missing {
            (0.0, 0.0)
        } else if leg.length < h - 1e-9 {
            (0.0, h - leg.length)
        } else {
            (theoretical_drop(leg.length, h), 0.0)
        };
        if short > 0.0 {
            residuals.push(Residual {
                kind: ResidualKind::Short,
                at: ItemRef::Member {
                    id: leg.member,
                    segment: Some(leg.segment),
                },
                value: short,
                message: format!(
                    "body '{}': segment is {short:.4} ft short of a {h:.4} ft reach",
                    body.label
                ),
                severity: Severity::Warn,
            });
        }
        drops.push(d);
        tzs.push(leg.support[2] - d - rl[2]);
    }

    let derived_z = tzs
        .iter()
        .copied()
        .filter(|z| z.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    let derived_z = if derived_z.is_finite() {
        derived_z
    } else {
        0.0
    };
    let derived_origin = [t_xy[0], t_xy[1], derived_z];

    let (frame, _) = authored_frame(body, params, derived_origin, *rot)?;
    // Level keeps authored rot and derived origin; authored_frame for Level uses fallback origin.
    let frame = match body.placement() {
        Placement::Level { .. } => Frame {
            origin: derived_origin,
            rot: frame.rot,
        },
        Placement::Derived => Frame {
            origin: derived_origin,
            rot: *rot,
        },
        _ => frame,
    };

    // Slack against the origin we actually used.
    for (i, leg) in legs.iter().enumerate() {
        if leg.missing {
            continue;
        }
        let slack = frame.origin[2] - tzs[i];
        if slack > 1e-6 {
            residuals.push(Residual {
                kind: ResidualKind::Slack,
                at: ItemRef::Member {
                    id: leg.member,
                    segment: Some(leg.segment),
                },
                value: slack,
                message: format!("body '{}': leg hangs {slack:.4} ft slack", body.label),
                severity: Severity::Warn,
            });
        }
    }

    let usable: Vec<f64> = legs
        .iter()
        .zip(drops.iter())
        .filter(|(l, _)| !l.missing)
        .map(|(_, d)| *d)
        .collect();
    if usable.len() >= 2 {
        let min_d = usable.iter().copied().fold(f64::INFINITY, f64::min);
        let max_d = usable.iter().copied().fold(0.0_f64, f64::max);
        let spread_in = (max_d - min_d) * 12.0;
        if spread_in > UNEQUAL_DROP_TOL_IN {
            residuals.push(Residual {
                kind: ResidualKind::UnequalDrop,
                at: ItemRef::Body(body.id),
                value: spread_in,
                message: format!(
                    "body '{}': leg drops differ by {spread_in:.2} in",
                    body.label
                ),
                severity: Severity::Warn,
            });
        }
    }

    if underconstrained(legs, &rlocals) {
        residuals.push(Residual {
            kind: ResidualKind::Underconstrained,
            at: ItemRef::Body(body.id),
            value: legs.len() as f64,
            message: format!(
                "body '{}': pose is not determined by its supports (yaw assumed identity)",
                body.label
            ),
            severity: Severity::Warn,
        });
    }

    let centroid = if supports.is_empty() {
        None
    } else {
        let n = supports.len() as f64;
        Some([
            supports.iter().map(|s| s[0]).sum::<f64>() / n,
            supports.iter().map(|s| s[1]).sum::<f64>() / n,
            supports.iter().map(|s| s[2]).sum::<f64>() / n,
        ])
    };

    Ok((frame, used, centroid, residuals))
}

fn plan_offset(supports: &[[f64; 3]], rlocals: &[[f64; 3]]) -> [f64; 2] {
    if supports.is_empty() || rlocals.is_empty() {
        return [0.0, 0.0];
    }
    let n = supports.len().min(rlocals.len()) as f64;
    let sx: f64 = supports.iter().map(|s| s[0]).sum::<f64>() / n;
    let sy: f64 = supports.iter().map(|s| s[1]).sum::<f64>() / n;
    let lx: f64 = rlocals.iter().map(|s| s[0]).sum::<f64>() / n;
    let ly: f64 = rlocals.iter().map(|s| s[1]).sum::<f64>() / n;
    [sx - lx, sy - ly]
}

fn underconstrained(legs: &[Leg], rlocals: &[[f64; 3]]) -> bool {
    if legs.len() < 2 {
        return true;
    }
    let pts: Vec<[f64; 2]> = rlocals.iter().map(|p| [p[0], p[1]]).collect();
    collinear(&pts)
}

fn collinear(pts: &[[f64; 2]]) -> bool {
    if pts.len() < 3 {
        return true;
    }
    let o = pts[0];
    let mut dir: Option<[f64; 2]> = None;
    for p in &pts[1..] {
        let v = [p[0] - o[0], p[1] - o[1]];
        let n2 = v[0] * v[0] + v[1] * v[1];
        if n2 < 1e-16 {
            continue;
        }
        match dir {
            None => dir = Some(v),
            Some(d) => {
                let cross = d[0] * v[1] - d[1] * v[0];
                if cross.abs() > 1e-8 {
                    return false;
                }
            }
        }
    }
    true
}

fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}
