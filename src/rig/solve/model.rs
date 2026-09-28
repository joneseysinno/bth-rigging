//! Solver model: the rig reduced to rigid bodies, free knots, members and
//! point loads, plus a mutable pose (`State`) the hang can move.
//!
//! Step: 3
//! Theory: roadmap Step 3 — the same incidence structure the Step 2 descent
//! walked, with numbers attached. Built once per solve from `Rig` + `EvalRig`.
//! Must not depend on: UI, dioxus, store.

// Index loops read like the math here.
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::rig::body::Placement;
use crate::rig::component::ComponentKind;
use crate::rig::eval::{EvalRig, Frame, MISSING_LENGTH_FT, Rot};
use crate::rig::id::{BodyId, MemberId, NodeId};
use crate::rig::{Rig, RigError, RigErrorKind};

/// Default axial stiffness for a component with no `stiffness_lb`, as a
/// multiple of the total hanging weight: "rigid" — a leg carrying the whole
/// load stretches 1e-9 of its length.
pub const RIGID_EA_FACTOR: f64 = 1e9;

/// Where a node's world position comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeLoc {
    /// On free body `index` at body-local `local`.
    Body { index: usize, local: [f64; 3] },
    /// On a held body (hook, pinned, posed): fixed in world.
    Held { body: BodyId, at: [f64; 3] },
    /// Free knot `index` (master link, collector).
    Free { index: usize },
}

/// Mechanical kind of a member in the solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemberKind {
    /// Tension-only, one tension along the whole path (μ = 0 at bearings).
    Tension,
    /// Zero-length hardware (master link / shackle stack): a ball joint that
    /// carries a full 3-component force. Template bar-to-load and apex joints.
    Link,
}

#[derive(Debug, Clone)]
pub struct ModelMember {
    pub id: MemberId,
    pub label: String,
    pub path: Vec<NodeId>,
    pub kind: MemberKind,
    /// Available (nominal) length, ft — Σ segments incl. adjust settings.
    pub length: f64,
    /// Flexibility, ft/lb: Σ Lᵢ / EAᵢ over components.
    pub compliance: f64,
    /// Adjustable length currently set (Σ adjust settings), and its range.
    pub adjust: Option<AdjustRange>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdjustRange {
    pub setting: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone)]
pub struct ModelBody {
    pub id: BodyId,
    pub label: String,
    pub weight: f64,
    pub cg_local: [f64; 3],
}

/// A rigid pose: world = origin + R · local.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub origin: [f64; 3],
    pub r: [[f64; 3]; 3],
}

impl Pose {
    pub fn from_frame(f: &Frame) -> Self {
        let cols = [
            f.rot.apply([1.0, 0.0, 0.0]),
            f.rot.apply([0.0, 1.0, 0.0]),
            f.rot.apply([0.0, 0.0, 1.0]),
        ];
        let mut r = [[0.0; 3]; 3];
        for (j, c) in cols.iter().enumerate() {
            for i in 0..3 {
                r[i][j] = c[i];
            }
        }
        Self {
            origin: f.origin,
            r,
        }
    }

    pub fn apply(&self, l: [f64; 3]) -> [f64; 3] {
        add(self.origin, self.rotate(l))
    }

    pub fn rotate(&self, l: [f64; 3]) -> [f64; 3] {
        let r = &self.r;
        [
            r[0][0] * l[0] + r[0][1] * l[1] + r[0][2] * l[2],
            r[1][0] * l[0] + r[1][1] * l[1] + r[1][2] * l[2],
            r[2][0] * l[0] + r[2][1] * l[1] + r[2][2] * l[2],
        ]
    }

    /// Apply a world-frame translation `d` and small rotation vector `phi`
    /// (Rodrigues, exact for any angle).
    pub fn perturbed(&self, d: [f64; 3], phi: [f64; 3]) -> Self {
        let q = rodrigues(phi);
        let mut r = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                r[i][j] = (0..3).map(|k| q[i][k] * self.r[k][j]).sum();
            }
        }
        Self {
            origin: add(self.origin, d),
            r,
        }
    }

    /// Intrinsic z-y-x Euler (deg) matching `Rot::apply`.
    pub fn to_rot(&self) -> Rot {
        let r = &self.r;
        let pitch = (-r[2][0]).clamp(-1.0, 1.0).asin();
        let (yaw, roll) = if pitch.cos().abs() > 1e-12 {
            (r[1][0].atan2(r[0][0]), r[2][1].atan2(r[2][2]))
        } else {
            ((-r[0][1]).atan2(r[1][1]), 0.0)
        };
        Rot::from_degrees(yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees())
    }

    /// Tilt of the body's local z axis from world vertical, deg.
    pub fn tilt_deg(&self) -> f64 {
        self.r[2][2].clamp(-1.0, 1.0).acos().to_degrees()
    }
}

pub fn rodrigues(phi: [f64; 3]) -> [[f64; 3]; 3] {
    let th = norm(phi);
    let mut q = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    if th < 1e-300 {
        return q;
    }
    let k = [phi[0] / th, phi[1] / th, phi[2] / th];
    let (s, c) = th.sin_cos();
    let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
    for i in 0..3 {
        for j in 0..3 {
            let kk: f64 = (0..3).map(|m| kx[i][m] * kx[m][j]).sum();
            q[i][j] += s * kx[i][j] + (1.0 - c) * kk;
        }
    }
    q
}

/// Poses of the free bodies and positions of the free knots.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub poses: Vec<Pose>,
    pub free: Vec<[f64; 3]>,
}

/// The rig as the solver sees it.
#[derive(Debug, Clone)]
pub struct Model {
    pub bodies: Vec<ModelBody>,
    pub body_index: HashMap<BodyId, usize>,
    pub free_nodes: Vec<NodeId>,
    pub nodes: IndexMap<NodeId, NodeLoc>,
    pub members: Vec<ModelMember>,
    /// Lumped segment self-weight: (node, lb). Applied at the lower stop.
    pub point_loads: Vec<(NodeId, f64)>,
    /// Held bodies (hook, Pinned, Posed) with their weight and world CG.
    pub held: Vec<(BodyId, f64, [f64; 3])>,
    pub root_body: BodyId,
    /// Characteristic length for moment-row scaling, ft.
    pub l_char: f64,
    /// Total weight below the root (for tolerances), lb.
    pub w_total: f64,
}

impl Model {
    /// Build from an evaluated rig. `initial` is the state matching `eval`.
    pub fn build(rig: &Rig, eval: &EvalRig) -> Result<(Self, State), Vec<RigError>> {
        let params = &rig.params;
        let root_body = rig
            .nodes
            .get(&rig.root)
            .and_then(|n| n.body)
            .ok_or_else(|| {
                vec![RigError::new(
                    RigErrorKind::NodePlacement,
                    "root has no body",
                )]
            })?;

        let w_total = eval.weights.total_below_root_lbs.max(1.0);
        let ea_rigid = RIGID_EA_FACTOR * w_total;

        let mut bodies = Vec::new();
        let mut body_index = HashMap::new();
        let mut poses = Vec::new();
        let mut held = Vec::new();
        for (id, body) in &rig.bodies {
            let be = eval.bodies.get(id).ok_or_else(|| {
                vec![RigError::new(
                    RigErrorKind::Disconnected,
                    format!("body '{}' was not placed", body.label),
                )]
            })?;
            let is_held = *id == root_body
                || matches!(
                    body.placement(),
                    Placement::Pinned { .. } | Placement::Posed { .. }
                );
            if is_held {
                let w = if *id == root_body { 0.0 } else { be.weight_lbs };
                held.push((*id, w, be.cg_world));
                continue;
            }
            body_index.insert(*id, bodies.len());
            bodies.push(ModelBody {
                id: *id,
                label: body.label.clone(),
                weight: be.weight_lbs,
                cg_local: body.eval_cg(params).map_err(|e| vec![e])?,
            });
            poses.push(Pose::from_frame(&be.frame));
        }

        let mut nodes = IndexMap::new();
        let mut free_nodes = Vec::new();
        let mut free = Vec::new();
        for (nid, node) in &rig.nodes {
            let world = eval.nodes.get(nid).copied();
            let loc = match node.body {
                Some(bid) => match body_index.get(&bid) {
                    Some(&index) => NodeLoc::Body {
                        index,
                        local: node.local.eval(params).map_err(|e| vec![e])?,
                    },
                    None => NodeLoc::Held {
                        body: bid,
                        at: world.unwrap_or([0.0; 3]),
                    },
                },
                None => {
                    let index = free_nodes.len();
                    free_nodes.push(*nid);
                    free.push(world.unwrap_or([0.0; 3]));
                    NodeLoc::Free { index }
                }
            };
            nodes.insert(*nid, loc);
        }

        let mut members = Vec::new();
        let mut point_loads: Vec<(NodeId, f64)> = Vec::new();
        for (mid, m) in &rig.members {
            let length = m.nominal_length(params).map_err(|e| vec![e])?;
            let hardware_only = m.segments.iter().all(|s| {
                s.components.iter().all(|c| {
                    matches!(
                        c.kind,
                        ComponentKind::MasterLink
                            | ComponentKind::Shackle { .. }
                            | ComponentKind::Turnbuckle
                    )
                })
            });
            let kind = if length <= MISSING_LENGTH_FT && hardware_only && m.path.len() == 2 {
                MemberKind::Link
            } else if length <= MISSING_LENGTH_FT {
                return Err(vec![RigError::new(
                    RigErrorKind::EmptySegment,
                    format!(
                        "member '{}' has no length (legacy NO LEN) — the solve needs sling lengths",
                        m.label
                    ),
                )]);
            } else {
                MemberKind::Tension
            };

            let mut compliance = 0.0;
            let mut adjust: Option<AdjustRange> = None;
            for seg in &m.segments {
                for c in &seg.components {
                    let l = c.eval_length(params).map_err(|e| vec![e])?;
                    let ea = match &c.stiffness_lb {
                        Some(e) => e.eval(params).map_err(|e| vec![e])?,
                        None => ea_rigid,
                    };
                    if l > 0.0 && ea > 0.0 {
                        compliance += l / ea;
                    }
                    if let Some(a) = &c.adjust {
                        let (min, max, setting) = a.eval(params).map_err(|e| vec![e])?;
                        let acc = adjust.get_or_insert(AdjustRange {
                            setting: 0.0,
                            min: 0.0,
                            max: 0.0,
                        });
                        acc.setting += setting;
                        acc.min += min;
                        acc.max += max;
                    }
                }
            }
            if compliance <= 0.0 {
                compliance = length.max(MISSING_LENGTH_FT) / ea_rigid;
            }

            // Lumped self-weight: each segment's weight at its lower stop.
            for (si, seg) in m.segments.iter().enumerate() {
                let w = seg.eval_weight(params).map_err(|e| vec![e])?;
                if w == 0.0 {
                    continue;
                }
                let a = m.path[si];
                let b = m.path[si + 1];
                let za = eval.nodes.get(&a).map(|p| p[2]).unwrap_or(0.0);
                let zb = eval.nodes.get(&b).map(|p| p[2]).unwrap_or(0.0);
                let b_held = matches!(nodes.get(&b), Some(NodeLoc::Held { .. }));
                // Tie: the later path stop (the builder writes top → bottom),
                // unless that one is held.
                let lower = if (za - zb).abs() <= 1e-9 {
                    if b_held { a } else { b }
                } else if za < zb {
                    a
                } else {
                    b
                };
                point_loads.push((lower, w));
            }

            members.push(ModelMember {
                id: *mid,
                label: m.label.clone(),
                path: m.path.clone(),
                kind,
                length,
                compliance,
                adjust,
            });
        }

        let mut l_char: f64 = 1.0;
        for p in eval.nodes.values() {
            let d = sub(*p, eval.root_at);
            l_char = l_char.max(norm(d));
        }

        Ok((
            Self {
                bodies,
                body_index,
                free_nodes,
                nodes,
                members,
                point_loads,
                held,
                root_body,
                l_char,
                w_total,
            },
            State { poses, free },
        ))
    }

    pub fn pos(&self, s: &State, n: NodeId) -> [f64; 3] {
        match self.nodes.get(&n) {
            Some(NodeLoc::Body { index, local }) => s.poses[*index].apply(*local),
            Some(NodeLoc::Held { at, .. }) => *at,
            Some(NodeLoc::Free { index }) => s.free[*index],
            None => [0.0; 3],
        }
    }

    pub fn cg(&self, s: &State, body: usize) -> [f64; 3] {
        s.poses[body].apply(self.bodies[body].cg_local)
    }

    /// Current path length (Σ chords) of member `i`.
    pub fn path_length(&self, s: &State, i: usize) -> f64 {
        let m = &self.members[i];
        m.path
            .windows(2)
            .map(|w| norm(sub(self.pos(s, w[1]), self.pos(s, w[0]))))
            .sum()
    }

    /// Number of equilibrium equations (6 per free body, 3 per free knot).
    pub fn dof(&self) -> usize {
        6 * self.bodies.len() + 3 * self.free_nodes.len()
    }

    pub fn member_index(&self, id: MemberId) -> Option<usize> {
        self.members.iter().position(|m| m.id == id)
    }

    pub fn is_held_node(&self, n: NodeId) -> bool {
        matches!(self.nodes.get(&n), Some(NodeLoc::Held { .. }))
    }
}

pub fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
pub fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(a);
    if n > 1e-12 {
        Some(scale(a, 1.0 / n))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_round_trips_euler() {
        let f = Frame {
            origin: [1.0, 2.0, 3.0],
            rot: Rot::from_degrees(30.0, -10.0, 5.0),
        };
        let p = Pose::from_frame(&f);
        let r = p.to_rot();
        assert!((r.yaw - 30.0).abs() < 1e-9);
        assert!((r.pitch + 10.0).abs() < 1e-9);
        assert!((r.roll - 5.0).abs() < 1e-9);
        let l = [0.3, -1.2, 2.0];
        let a = p.apply(l);
        let b = f.transform(l);
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn small_rotation_moves_like_cross_product() {
        let p = Pose::from_frame(&Frame::identity_at([0.0; 3]));
        let q = p.perturbed([0.0; 3], [0.0, 0.0, 1e-7]);
        let x = q.apply([1.0, 0.0, 0.0]);
        assert!((x[1] - 1e-7).abs() < 1e-15);
    }
}
