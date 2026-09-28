//! Walk a solved rig and stamp every rated item.
//!
//! Step: 3
//! Theory: roadmap Step 3 — "checks stamp OK/OVER". The glue between the
//! solve and the pure `checks` functions: each roundsling, chain and shackle
//! on a member gets the member tension (a shackle at a bearing gets the
//! resultant of both legs; a shackle on a lug gets the side-load angle from
//! the lug's plate normal); each rated lug gets the resultant of every member
//! at it; each rated spreader / lifting beam gets the load hanging below it
//! and its axial force. `with_bounds` re-rates the member hardware at the
//! envelope maximum and raises a nominal OK to WARN when the bound is OVER —
//! "OK as rigged, OVER if the load shifts".
//! Must not depend on: UI, dioxus, store.

use serde::{Deserialize, Serialize};

use super::bounds::Envelope;
use super::model::{add, dot, norm, sub, unit};
use super::{MemberForce, SolvedRig};
use crate::checks::{self, Check, Status};
use crate::rig::body::BodyKind;
use crate::rig::component::ComponentKind;
use crate::rig::eval::ItemRef;
use crate::rig::id::{MemberId, NodeId};
use crate::rig::node::{Axis, NodeKind};
use crate::rig::{Member, Rig, RigError};

/// A check pinned to the item it rates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rated {
    pub at: ItemRef,
    /// Member / node / body label.
    pub owner: String,
    pub check: Check,
}

/// Worst stamp over a set of checks.
pub fn governing(rated: &[Rated]) -> Status {
    rated
        .iter()
        .map(|r| r.check.status)
        .max()
        .unwrap_or(Status::Ok)
}

/// Rate every item at the solved tensions.
pub fn rate(rig: &Rig, solved: &SolvedRig) -> Result<Vec<Rated>, Vec<RigError>> {
    let mut out = Vec::new();
    for (mid, m) in &rig.members {
        let Some(mf) = solved.members.get(mid) else {
            continue;
        };
        out.extend(member_checks(rig, solved, *mid, m, mf, mf.tension_lbs)?);
    }
    out.extend(lug_checks(rig, solved));
    out.extend(bar_checks(rig, solved));
    Ok(out)
}

/// Nominal checks, with member hardware also rated at the envelope maximum.
pub fn with_bounds(
    rig: &Rig,
    solved: &SolvedRig,
    env: &Envelope,
) -> Result<Vec<Rated>, Vec<RigError>> {
    let mut out = Vec::new();
    for (mid, m) in &rig.members {
        let Some(mf) = solved.members.get(mid) else {
            continue;
        };
        let mut nominal = member_checks(rig, solved, *mid, m, mf, mf.tension_lbs)?;
        if let Some(range) = env.members.get(mid)
            && range.max_lbs.is_finite()
            && range.max_lbs > mf.tension_lbs * (1.0 + 1e-9)
        {
            let bound = member_checks(rig, solved, *mid, m, mf, range.max_lbs)?;
            for (n, b) in nominal.iter_mut().zip(bound) {
                if b.check.status == Status::Over && n.check.status == Status::Ok {
                    n.check = n.check.clone().warn(format!(
                        "OVER if the load shifts: {:.0} lb possible vs {:.0} lb rated",
                        b.check.demand_lbs, b.check.capacity_lbs
                    ));
                }
            }
        }
        out.extend(nominal);
    }
    out.extend(lug_checks(rig, solved));
    out.extend(bar_checks(rig, solved));
    Ok(out)
}

fn member_checks(
    rig: &Rig,
    solved: &SolvedRig,
    mid: MemberId,
    m: &Member,
    mf: &MemberForce,
    tension: f64,
) -> Result<Vec<Rated>, Vec<RigError>> {
    let params = &rig.params;
    let pts: Vec<[f64; 3]> = m
        .path
        .iter()
        .map(|n| solved.eval.nodes.get(n).copied().unwrap_or([0.0; 3]))
        .collect();
    // |Σ unit vectors| at each stop: 1 at an end, 2 cos(wrap/2) at a bearing.
    let stop_factor: Vec<f64> = (0..pts.len())
        .map(|k| {
            let mut f = [0.0; 3];
            if k > 0
                && let Some(u) = unit(sub(pts[k - 1], pts[k]))
            {
                f = add(f, u);
            }
            if k + 1 < pts.len()
                && let Some(u) = unit(sub(pts[k + 1], pts[k]))
            {
                f = add(f, u);
            }
            norm(f)
        })
        .collect();
    let bow_dia_in = m.path[1..m.path.len().saturating_sub(1)]
        .iter()
        .filter_map(|n| rig.nodes.get(n))
        .find_map(|n| match &n.kind {
            NodeKind::Bow { bow_dia, .. } => bow_dia.eval(params).ok().map(|d| d * 12.0),
            _ => None,
        });

    let mut out = Vec::new();
    let mut shackle_stops_done: Vec<usize> = Vec::new();
    for (k, seg) in m.segments.iter().enumerate() {
        let angle = mf.angle_deg.get(k).copied().unwrap_or(90.0);
        let last = seg.components.len().saturating_sub(1);
        for (ci, c) in seg.components.iter().enumerate() {
            let at = ItemRef::Member {
                id: mid,
                segment: Some(k),
            };
            let check = match &c.kind {
                ComponentKind::RoundSling { size, hitch } => checks::sling::roundsling(
                    *size,
                    *hitch,
                    tension,
                    angle,
                    if m.path.len() > 2 { bow_dia_in } else { None },
                ),
                ComponentKind::Chain { grade, size_in } => {
                    let size = size_in.eval(params).map_err(|e| vec![e])?;
                    checks::chain::chain(*grade, size, tension, 1.0)
                }
                ComponentKind::Shackle { size } => {
                    let stop = if ci == 0 {
                        Some(k)
                    } else if ci == last {
                        Some(k + 1)
                    } else {
                        None
                    };
                    if let Some(s) = stop {
                        if shackle_stops_done.contains(&s) {
                            continue;
                        }
                        shackle_stops_done.push(s);
                    }
                    let (demand, side) = match stop {
                        _ if mf.is_link => (tension, 0.0),
                        Some(s) => {
                            let f = mf.stop_forces.get(s).copied().unwrap_or([0.0; 3]);
                            let demand = tension * stop_factor.get(s).copied().unwrap_or(1.0);
                            let side = plate_normal_world(rig, solved, m.path[s])
                                .map(|n| checks::lug::out_of_plane_deg(f, n))
                                .unwrap_or(0.0);
                            (demand, side)
                        }
                        None => (tension, 0.0),
                    };
                    checks::shackle::shackle(size, demand, side)
                }
                _ => None,
            };
            if let Some(check) = check {
                out.push(Rated {
                    at,
                    owner: m.label.clone(),
                    check,
                });
            }
        }
    }
    Ok(out)
}

/// World plate normal of a lug node, if it is a lug on a placed body with a
/// **horizontal** plate normal (a vertical plate — a padeye). `Z` / `NegZ` is
/// the builder's default and reads as "orientation not given": no side-load
/// or out-of-plane reduction is applied for it (decision D3-6).
fn plate_normal_world(rig: &Rig, solved: &SolvedRig, node: NodeId) -> Option<[f64; 3]> {
    let n = rig.nodes.get(&node)?;
    let NodeKind::Lug { plate_normal, .. } = &n.kind else {
        return None;
    };
    let local = match plate_normal {
        Axis::X => [1.0, 0.0, 0.0],
        Axis::Y => [0.0, 1.0, 0.0],
        Axis::NegX => [-1.0, 0.0, 0.0],
        Axis::NegY => [0.0, -1.0, 0.0],
        Axis::Z | Axis::NegZ => return None,
    };
    let frame = solved.eval.bodies.get(&n.body?)?.frame;
    Some(frame.rot.apply(local))
}

/// Resultant member force at each node.
fn node_resultants(rig: &Rig, solved: &SolvedRig) -> indexmap::IndexMap<NodeId, [f64; 3]> {
    let mut out: indexmap::IndexMap<NodeId, [f64; 3]> = indexmap::IndexMap::new();
    for (mid, m) in &rig.members {
        let Some(mf) = solved.members.get(mid) else {
            continue;
        };
        for (k, n) in m.path.iter().enumerate() {
            let f = mf.stop_forces.get(k).copied().unwrap_or([0.0; 3]);
            let e = out.entry(*n).or_insert([0.0; 3]);
            *e = add(*e, f);
        }
    }
    out
}

fn lug_checks(rig: &Rig, solved: &SolvedRig) -> Vec<Rated> {
    let res = node_resultants(rig, solved);
    let mut out = Vec::new();
    for (nid, n) in &rig.nodes {
        let NodeKind::Lug {
            rating: Some(r), ..
        } = &n.kind
        else {
            continue;
        };
        let f = res.get(nid).copied().unwrap_or([0.0; 3]);
        let oop = plate_normal_world(rig, solved, *nid)
            .map(|pn| checks::lug::out_of_plane_deg(f, pn))
            .unwrap_or(0.0);
        out.push(Rated {
            at: ItemRef::Node(*nid),
            owner: n.label.clone(),
            check: checks::lug::lug(&n.label, r.wll_lbs, norm(f), oop),
        });
    }
    out
}

/// Axial force in a bar body at the solve, lb (+ compression).
pub fn bar_axial(rig: &Rig, solved: &SolvedRig, body: crate::rig::id::BodyId) -> Option<f64> {
    let res = node_resultants(rig, solved);
    let nodes: Vec<(NodeId, [f64; 3])> = rig
        .nodes
        .iter()
        .filter(|(_, n)| n.body == Some(body))
        .filter_map(|(id, _)| solved.eval.nodes.get(id).map(|p| (*id, *p)))
        .collect();
    let (mid, u) = principal_axis(&nodes.iter().map(|(_, p)| *p).collect::<Vec<_>>())?;
    Some(
        nodes
            .iter()
            .filter(|(_, p)| dot(sub(*p, mid), u) < 0.0)
            .map(|(id, _)| res.get(id).map(|f| dot(*f, u)).unwrap_or(0.0))
            .sum(),
    )
}

fn bar_checks(rig: &Rig, solved: &SolvedRig) -> Vec<Rated> {
    let res = node_resultants(rig, solved);
    let mut out = Vec::new();
    for (bid, b) in &rig.bodies {
        let rating = match &b.kind {
            BodyKind::SpreaderBar { rating, .. } | BodyKind::LiftingBeam { rating, .. } => rating,
            _ => continue,
        };
        let nodes: Vec<(NodeId, [f64; 3])> = rig
            .nodes
            .iter()
            .filter(|(_, n)| n.body == Some(*bid))
            .filter_map(|(id, _)| solved.eval.nodes.get(id).map(|p| (*id, *p)))
            .collect();
        // Load hanging below: every member pull on this body that points down.
        let hanging: f64 = rig
            .members
            .iter()
            .filter_map(|(mid, m)| solved.members.get(mid).map(|mf| (m, mf)))
            .flat_map(|(m, mf)| {
                m.path.iter().zip(&mf.stop_forces).filter_map(|(n, f)| {
                    (rig.nodes.get(n).and_then(|nd| nd.body) == Some(*bid))
                        .then_some((-f[2]).max(0.0))
                })
            })
            .sum();
        // Bar axis: principal direction of its nodes (the span).
        let compression = match principal_axis(&nodes.iter().map(|(_, p)| *p).collect::<Vec<_>>()) {
            Some((mid, u)) => nodes
                .iter()
                .filter(|(_, p)| dot(sub(*p, mid), u) < 0.0)
                .map(|(id, _)| res.get(id).map(|f| dot(*f, u)).unwrap_or(0.0))
                .sum(),
            None => 0.0,
        };
        let check = if rating.wll_lbs == 0 {
            let mut c = checks::bar::bar(&b.label, 0, hanging, compression);
            c.status = Status::Warn;
            c.notes.push("no WLL entered for this bar".into());
            c
        } else {
            checks::bar::bar(&b.label, rating.wll_lbs, hanging, compression)
        };
        out.push(Rated {
            at: ItemRef::Body(*bid),
            owner: b.label.clone(),
            check,
        });
    }
    out
}

/// Centroid and dominant direction of a point set (power iteration on the
/// scatter matrix). `None` for fewer than two distinct points.
fn principal_axis(pts: &[[f64; 3]]) -> Option<([f64; 3], [f64; 3])> {
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len() as f64;
    let mut c = [0.0; 3];
    for p in pts {
        c = add(c, *p);
    }
    c = [c[0] / n, c[1] / n, c[2] / n];
    let mut s = [[0.0; 3]; 3];
    for p in pts {
        let d = sub(*p, c);
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += d[i] * d[j];
            }
        }
    }
    let mut v = [1.0, 0.7, 0.3];
    for _ in 0..200 {
        let w = [
            s[0][0] * v[0] + s[0][1] * v[1] + s[0][2] * v[2],
            s[1][0] * v[0] + s[1][1] * v[1] + s[1][2] * v[2],
            s[2][0] * v[0] + s[2][1] * v[1] + s[2][2] * v[2],
        ];
        v = unit(w)?;
    }
    Some((c, v))
}
