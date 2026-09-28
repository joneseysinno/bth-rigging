//! Parameters to 3D coordinates, lengths, and weights.
//!
//! Step: 2
//! Theory: roadmap Step 2 — nominal descent, no statics.
//! Inputs: graph + parameter bindings.
//! Outputs: numeric geometry, residuals, mass props.
//! Must not depend on: UI, dioxus, store, layers.

mod descent;

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::body::{BodyKind, Placement};
use super::id::{BodyId, MemberId, NodeId, ParamId};
use super::param::ParamTable;
use super::{Rig, RigError, RigWeights};

pub use descent::{MISSING_LENGTH_FT, UNEQUAL_DROP_TOL_IN};

/// Rigid-body pose: world = origin + R · local. Right-handed, z up, ft.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub origin: [f64; 3],
    pub rot: Rot,
}

impl Frame {
    pub fn identity_at(origin: [f64; 3]) -> Self {
        Self {
            origin,
            rot: Rot::identity(),
        }
    }

    pub fn transform(&self, local: [f64; 3]) -> [f64; 3] {
        let r = self.rot.apply(local);
        [
            self.origin[0] + r[0],
            self.origin[1] + r[1],
            self.origin[2] + r[2],
        ]
    }
}

/// Intrinsic z-y-x (yaw, pitch, roll), deg. Identity is the Step 2 default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rot {
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
}

impl Rot {
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn from_degrees(yaw: f64, pitch: f64, roll: f64) -> Self {
        Self { yaw, pitch, roll }
    }

    /// Apply Rz(yaw) · Ry(pitch) · Rx(roll) to a body-local vector.
    pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        let (sr, cr) = self.roll.to_radians().sin_cos();
        let [x, y, z] = v;
        // Rx
        let y1 = y * cr - z * sr;
        let z1 = y * sr + z * cr;
        // Ry
        let x2 = x * cp + z1 * sp;
        let z2 = -x * sp + z1 * cp;
        // Rz
        let x3 = x2 * cy - y1 * sy;
        let y3 = x2 * sy + y1 * cy;
        [x3, y3, z2]
    }
}

/// Which placement rule actually sat this body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementUsed {
    Derived,
    Pinned,
    Posed,
    Level,
}

impl PlacementUsed {
    pub fn from_placement(p: &Placement) -> Self {
        match p {
            Placement::Derived => Self::Derived,
            Placement::Pinned { .. } => Self::Pinned,
            Placement::Posed { .. } => Self::Posed,
            Placement::Level { .. } => Self::Level,
        }
    }
}

/// Hook-to-geometry heights, ft.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HeightSummary {
    /// Hook down to the lowest node.
    pub hook_to_lowest_ft: f64,
    /// Hook down to the top of the load box (origin.z + height).
    pub hook_to_load_top_ft: f64,
    /// Hook down to the lowest load-lug (layer-engine rigging height).
    pub hook_to_pick_ft: f64,
}

/// Evaluated body pose and mass properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyEval {
    pub frame: Frame,
    pub weight_lbs: f64,
    pub cg_world: [f64; 3],
    pub support_centroid: Option<[f64; 3]>,
    /// Plan distance CG → support centroid (swing hint).
    pub cg_offset_ft: f64,
    pub placement: PlacementUsed,
}

/// Evaluated member: world polyline and per-segment derived lengths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberEval {
    pub points: Vec<[f64; 3]>,
    pub chords_ft: Vec<f64>,
    pub nominal_ft: Vec<f64>,
    pub slack_ft: Vec<f64>,
    pub short_ft: Vec<f64>,
    pub reach_ft: Vec<f64>,
    pub drop_ft: Vec<f64>,
    pub angle_deg: Vec<f64>,
    pub governing_angle_deg: f64,
    pub wrap_deg: Vec<f64>,
    pub taut: bool,
}

impl MemberEval {
    pub fn theoretical_drop(&self, i: usize) -> f64 {
        let l = self.nominal_ft.get(i).copied().unwrap_or(0.0);
        let h = self.reach_ft.get(i).copied().unwrap_or(0.0);
        theoretical_drop(l, h)
    }
}

pub fn theoretical_drop(nominal: f64, reach: f64) -> f64 {
    if nominal > reach {
        (nominal * nominal - reach * reach).sqrt()
    } else {
        0.0
    }
}

/// Angle from horizontal (90 = vertical), matching the layer engine.
pub fn angle_from_horizontal(nominal: f64, reach: f64) -> f64 {
    if nominal <= MISSING_LENGTH_FT {
        return 90.0;
    }
    if reach >= nominal - 1e-9 {
        return 0.0;
    }
    (reach / nominal).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Named pointer at a residual.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemRef {
    Node(NodeId),
    Body(BodyId),
    Member {
        id: MemberId,
        segment: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResidualKind {
    Short,
    Slack,
    UnequalDrop,
    BearingSplitAssumed,
    Underconstrained,
    CgOffset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Residual {
    pub kind: ResidualKind,
    pub at: ItemRef,
    pub value: f64,
    pub message: String,
    pub severity: Severity,
}

/// One immutable snapshot of the graph evaluated at a parameter binding.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalRig {
    pub rig_id: uuid::Uuid,
    pub bindings: HashMap<ParamId, f64>,
    pub root_at: [f64; 3],
    pub bodies: IndexMap<BodyId, BodyEval>,
    pub nodes: IndexMap<NodeId, [f64; 3]>,
    pub members: IndexMap<MemberId, MemberEval>,
    pub weights: RigWeights,
    pub height: HeightSummary,
    pub residuals: Vec<Residual>,
    pub assumed: bool,
}

impl EvalRig {
    pub fn evaluate(rig: &Rig) -> Result<Self, Vec<RigError>> {
        Self::evaluate_with(rig, &rig.params)
    }

    pub fn evaluate_with(rig: &Rig, table: &ParamTable) -> Result<Self, Vec<RigError>> {
        let mut scratch = rig.clone();
        scratch.params = table.clone();
        scratch.validate()?;

        let mut placed = descent::place(&scratch)?;
        let weights = scratch.weights().map_err(|e| vec![e])?;

        let mut members = IndexMap::new();
        for (id, member) in &scratch.members {
            let ev = member_eval(&scratch, &placed, member)?;
            members.insert(*id, ev);
        }

        let mut bodies = IndexMap::new();
        for (id, body) in &scratch.bodies {
            let frame = placed
                .frames
                .get(id)
                .copied()
                .unwrap_or_else(|| Frame::identity_at([0.0, 0.0, 0.0]));
            let used = placed
                .used
                .get(id)
                .copied()
                .unwrap_or(PlacementUsed::Derived);
            let cg_local = body.eval_cg(&scratch.params).map_err(|e| vec![e])?;
            let cg_world = frame.transform(cg_local);
            let support_centroid = placed.support_centroid.get(id).copied().flatten();
            let cg_offset_ft = match support_centroid {
                Some(s) => plan_dist(cg_world, s),
                None => 0.0,
            };
            if cg_offset_ft > 1e-3 {
                placed.residuals.push(Residual {
                    kind: ResidualKind::CgOffset,
                    at: ItemRef::Body(*id),
                    value: cg_offset_ft,
                    message: format!(
                        "body '{}' CG is {cg_offset_ft:.3} ft from its support centroid",
                        body.label
                    ),
                    severity: Severity::Warn,
                });
            }
            let weight_lbs = body.eval_weight(&scratch.params).map_err(|e| vec![e])?;
            bodies.insert(
                *id,
                BodyEval {
                    frame,
                    weight_lbs,
                    cg_world,
                    support_centroid,
                    cg_offset_ft,
                    placement: used,
                },
            );
        }

        let root_at = placed
            .nodes
            .get(&scratch.root)
            .copied()
            .unwrap_or([0.0, 0.0, 0.0]);
        let height = height_summary(&scratch, &placed.nodes, &bodies, root_at);
        let assumed = weights.assumed;
        let bindings = scratch
            .params
            .iter()
            .map(|(id, p)| {
                let v = p
                    .expr
                    .as_ref()
                    .and_then(|e| e.eval(&scratch.params).ok())
                    .unwrap_or(p.nominal);
                (*id, v)
            })
            .collect();

        Ok(Self {
            rig_id: scratch.id,
            bindings,
            root_at,
            bodies,
            nodes: placed.nodes,
            members,
            weights,
            height,
            residuals: placed.residuals,
            assumed,
        })
    }

    /// Re-evaluate member and body geometry at a pose that came from a solve
    /// (Step 3 hang) instead of the nominal descent. Residuals keep only what
    /// still applies to a hung pose: `Short` and `Slack` per member.
    pub fn with_pose(
        rig: &Rig,
        base: &EvalRig,
        frames: IndexMap<BodyId, Frame>,
        nodes: IndexMap<NodeId, [f64; 3]>,
    ) -> Result<Self, Vec<RigError>> {
        let mut split_nominal = HashMap::new();
        for (id, member) in &rig.members {
            if member.segments.len() < 2 {
                continue;
            }
            let pts: Vec<[f64; 3]> = member
                .path
                .iter()
                .map(|n| nodes.get(n).copied().unwrap_or([0.0; 3]))
                .collect();
            let chords: Vec<f64> = pts.windows(2).map(|w| dist3(w[0], w[1])).collect();
            let total: f64 = chords.iter().sum();
            let avail = member.available_length(&rig.params).map_err(|e| vec![e])?;
            if total > 1e-12 && total >= avail - 1e-6 {
                for (i, c) in chords.iter().enumerate() {
                    split_nominal.insert((*id, i), c * avail / total);
                }
            }
        }
        let placed = descent::Placed {
            nodes,
            frames,
            used: base
                .bodies
                .iter()
                .map(|(id, b)| (*id, b.placement))
                .collect(),
            support_centroid: IndexMap::new(),
            residuals: Vec::new(),
            split_nominal,
        };
        let mut members = IndexMap::new();
        let mut residuals = Vec::new();
        for (id, member) in &rig.members {
            let ev = member_eval(rig, &placed, member)?;
            for (si, s) in ev.short_ft.iter().enumerate() {
                if *s > 1e-6 {
                    residuals.push(Residual {
                        kind: ResidualKind::Short,
                        at: ItemRef::Member {
                            id: *id,
                            segment: Some(si),
                        },
                        value: *s,
                        message: format!(
                            "member '{}' is {s:.4} ft short after the hang",
                            member.label
                        ),
                        severity: Severity::Warn,
                    });
                }
            }
            members.insert(*id, ev);
        }
        let mut bodies = IndexMap::new();
        for (id, body) in &rig.bodies {
            let frame = placed
                .frames
                .get(id)
                .copied()
                .or_else(|| base.bodies.get(id).map(|b| b.frame))
                .unwrap_or_else(|| Frame::identity_at([0.0; 3]));
            let cg_local = body.eval_cg(&rig.params).map_err(|e| vec![e])?;
            let weight_lbs = base.bodies.get(id).map(|b| b.weight_lbs).unwrap_or(0.0);
            bodies.insert(
                *id,
                BodyEval {
                    frame,
                    weight_lbs,
                    cg_world: frame.transform(cg_local),
                    support_centroid: None,
                    cg_offset_ft: 0.0,
                    placement: base
                        .bodies
                        .get(id)
                        .map(|b| b.placement)
                        .unwrap_or(PlacementUsed::Derived),
                },
            );
        }
        let height = height_summary(rig, &placed.nodes, &bodies, base.root_at);
        Ok(Self {
            rig_id: base.rig_id,
            bindings: base.bindings.clone(),
            root_at: base.root_at,
            bodies,
            nodes: placed.nodes,
            members,
            weights: base.weights.clone(),
            height,
            residuals,
            assumed: base.assumed,
        })
    }

    pub fn governing_angle_deg(&self) -> f64 {
        self.members
            .values()
            .map(|m| m.governing_angle_deg)
            .fold(90.0_f64, f64::min)
    }

    pub fn max_short_ft(&self) -> f64 {
        self.members
            .values()
            .flat_map(|m| m.short_ft.iter().copied())
            .fold(0.0_f64, f64::max)
    }

    pub fn has_residual(&self, kind: ResidualKind) -> bool {
        self.residuals.iter().any(|r| r.kind == kind)
    }

    pub fn member_named<'a>(&'a self, rig: &'a Rig, label: &str) -> Option<&'a MemberEval> {
        let id = rig.members.iter().find(|(_, m)| m.label == label)?.0;
        self.members.get(id)
    }
}

fn member_eval(
    rig: &Rig,
    placed: &descent::Placed,
    member: &super::Member,
) -> Result<MemberEval, Vec<RigError>> {
    let mut points = Vec::with_capacity(member.path.len());
    for nid in &member.path {
        let p = placed.nodes.get(nid).copied().ok_or_else(|| {
            vec![RigError::new(
                super::RigErrorKind::Disconnected,
                format!("member '{}' node {nid} was not placed", member.label),
            )]
        })?;
        points.push(p);
    }
    let nseg = member.segments.len();
    let mut chords_ft = Vec::with_capacity(nseg);
    let mut nominal_ft = Vec::with_capacity(nseg);
    let mut slack_ft = Vec::with_capacity(nseg);
    let mut short_ft = Vec::with_capacity(nseg);
    let mut reach_ft = Vec::with_capacity(nseg);
    let mut drop_ft = Vec::with_capacity(nseg);
    let mut angle_deg = Vec::with_capacity(nseg);
    for i in 0..nseg {
        let a = points[i];
        let b = points[i + 1];
        let chord = dist3(a, b);
        let authored = member.segments[i]
            .eval_length(&rig.params)
            .map_err(|e| vec![e])?;
        let nominal = placed
            .split_nominal
            .get(&(member.id, i))
            .copied()
            .unwrap_or(authored);
        let reach = plan_dist(a, b);
        let drop = (a[2] - b[2]).abs();
        let slack = (nominal - chord).max(0.0);
        let short = (chord - nominal).max(0.0);
        chords_ft.push(chord);
        nominal_ft.push(nominal);
        slack_ft.push(slack);
        short_ft.push(short);
        reach_ft.push(reach);
        drop_ft.push(drop);
        angle_deg.push(angle_from_horizontal(nominal, reach));
    }
    let wrap_deg = (1..points.len().saturating_sub(1))
        .map(|i| wrap_at(points[i - 1], points[i], points[i + 1]))
        .collect();
    let governing_angle_deg = angle_deg.iter().copied().fold(90.0_f64, f64::min);
    let taut = short_ft.iter().all(|s| *s < 1e-9) && slack_ft.iter().all(|s| *s < 1e-6);
    Ok(MemberEval {
        points,
        chords_ft,
        nominal_ft,
        slack_ft,
        short_ft,
        reach_ft,
        drop_ft,
        angle_deg,
        governing_angle_deg,
        wrap_deg,
        taut,
    })
}

fn height_summary(
    rig: &Rig,
    nodes: &IndexMap<NodeId, [f64; 3]>,
    bodies: &IndexMap<BodyId, BodyEval>,
    root_at: [f64; 3],
) -> HeightSummary {
    let mut lowest = root_at[2];
    for p in nodes.values() {
        lowest = lowest.min(p[2]);
    }
    let mut pick_z = root_at[2];
    let mut saw_pick = false;
    for (nid, node) in &rig.nodes {
        let Some(bid) = node.body else { continue };
        let Some(body) = rig.bodies.get(&bid) else {
            continue;
        };
        if !body.kind.is_load() {
            continue;
        }
        if !matches!(node.kind, super::NodeKind::Lug { .. }) {
            continue;
        }
        if let Some(p) = nodes.get(nid) {
            if !saw_pick {
                pick_z = p[2];
                saw_pick = true;
            } else {
                pick_z = pick_z.min(p[2]);
            }
        }
    }
    if !saw_pick {
        pick_z = lowest;
    }
    let mut load_top = root_at[2];
    let mut saw_load = false;
    for (id, body) in &rig.bodies {
        if let BodyKind::Load { height, .. } = &body.kind
            && let (Some(be), Ok(h)) = (bodies.get(id), height.eval(&rig.params))
        {
            let top = be.frame.origin[2] + be.frame.rot.apply([0.0, 0.0, h])[2];
            if !saw_load {
                load_top = top;
                saw_load = true;
            } else {
                load_top = load_top.max(top);
            }
        }
    }
    HeightSummary {
        hook_to_lowest_ft: (root_at[2] - lowest).max(0.0),
        hook_to_load_top_ft: (root_at[2] - load_top).max(0.0),
        hook_to_pick_ft: (root_at[2] - pick_z).max(0.0),
    }
}

fn plan_dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn wrap_at(prev: [f64; 3], at: [f64; 3], next: [f64; 3]) -> f64 {
    let u = [prev[0] - at[0], prev[1] - at[1], prev[2] - at[2]];
    let v = [next[0] - at[0], next[1] - at[1], next[2] - at[2]];
    let nu = dist3(prev, at);
    let nv = dist3(next, at);
    if nu < 1e-15 || nv < 1e-15 {
        return 0.0;
    }
    let c = (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]) / (nu * nv);
    c.clamp(-1.0, 1.0).acos().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Hitch;
    use crate::rig::body::RotExpr;
    use crate::rig::param::{Coord3, Quantity, sweep};
    use crate::rig::{RigBuilder, duplo10};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn pinned_body_lugs_land_where_the_drawing_says() {
        let mut b = RigBuilder::new("pinned");
        let hook = b.hook("Hook");
        let load = b.load("Load", 10.0, 4.0, 2.0, 1_000.0);
        b.pin(load, Coord3::new(1.0, 2.0, -3.0));
        let lug = b.lug(load, "A", 4.0, 0.0, 1.0);
        b.member("leg")
            .from(hook)
            .to(lug)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 20.0));
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        let p = ev.nodes.get(&lug).copied().unwrap();
        assert!(close(p[0], 5.0), "{p:?}");
        assert!(close(p[1], 2.0), "{p:?}");
        assert!(close(p[2], -2.0), "{p:?}");
        let be = ev.bodies.get(&load).unwrap();
        assert_eq!(be.placement, PlacementUsed::Pinned);
    }

    #[test]
    fn posed_rotation_yaws_a_lug() {
        let mut b = RigBuilder::new("posed");
        let hook = b.hook("Hook");
        let load = b.load("Load", 10.0, 4.0, 2.0, 1_000.0);
        b.pose(
            load,
            Coord3::new(0.0, 0.0, 0.0),
            RotExpr::degrees(90.0, 0.0, 0.0),
        );
        let lug = b.lug(load, "A", 4.0, 0.0, 0.0);
        b.member("leg")
            .from(hook)
            .to(lug)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 20.0));
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        let p = ev.nodes.get(&lug).copied().unwrap();
        // +90° yaw about z: (x,y) → (−y, x) = (0, 4)
        assert!(close(p[0], 0.0), "{p:?}");
        assert!(close(p[1], 4.0), "{p:?}");
    }

    #[test]
    fn symmetric_four_leg_centres_under_the_hook() {
        let mut b = RigBuilder::new("4leg");
        let hook = b.hook("Hook");
        let load = b.load("Load", 8.0, 6.0, 2.0, 4_000.0);
        let lugs = [
            b.lug(load, "a", -4.0, -3.0, 2.0),
            b.lug(load, "b", -4.0, 3.0, 2.0),
            b.lug(load, "c", 4.0, -3.0, 2.0),
            b.lug(load, "d", 4.0, 3.0, 2.0),
        ];
        for (i, lug) in lugs.iter().copied().enumerate() {
            b.member(format!("leg {i}"))
                .from(hook)
                .to(lug)
                .segment(|s| s.roundsling(5, Hitch::Vertical, 10.0));
        }
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        let origin = ev.bodies.get(&load).unwrap().frame.origin;
        assert!(close(origin[0], 0.0) && close(origin[1], 0.0), "{origin:?}");
        let h = 5.0;
        let drop = (100.0 - 25.0_f64).sqrt();
        for id in lugs {
            let p = ev.nodes.get(&id).copied().unwrap();
            assert!(close(plan_dist(p, [0.0, 0.0, 0.0]), h));
            assert!(close(p[2], -drop));
        }
        let m = ev.member_named(&rig, "leg 0").unwrap();
        assert!(close(m.angle_deg[0], 60.0));
        assert!(close(m.reach_ft[0], 5.0));
        assert!(!ev.has_residual(ResidualKind::Short));
    }

    #[test]
    fn three_leg_asymmetric_centres_on_the_centroid() {
        let mut b = RigBuilder::new("3leg");
        let hook = b.hook("Hook");
        let load = b.load("Load", 12.0, 12.0, 2.0, 3_000.0);
        let a = b.lug(load, "a", 0.0, 0.0, 2.0);
        let c = b.lug(load, "c", 6.0, 0.0, 2.0);
        let e = b.lug(load, "e", 0.0, 6.0, 2.0);
        for (label, n) in [("leg a", a), ("leg c", c), ("leg e", e)] {
            b.member(label)
                .from(hook)
                .to(n)
                .segment(|s| s.roundsling(5, Hitch::Vertical, 12.0));
        }
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        let origin = ev.bodies.get(&load).unwrap().frame.origin;
        // mean(l_xy) = (2, 2); t_xy = (0,0) − (2,2) = (−2, −2)
        assert!(close(origin[0], -2.0), "{origin:?}");
        assert!(close(origin[1], -2.0), "{origin:?}");
        let pa = ev.nodes.get(&a).unwrap();
        let pc = ev.nodes.get(&c).unwrap();
        let pe = ev.nodes.get(&e).unwrap();
        let cx = (pa[0] + pc[0] + pe[0]) / 3.0;
        let cy = (pa[1] + pc[1] + pe[1]) / 3.0;
        assert!(close(cx, 0.0) && close(cy, 0.0), "centroid {cx} {cy}");
    }

    #[test]
    fn shorter_leg_binds_and_longer_carries_slack() {
        let mut b = RigBuilder::new("unequal");
        let hook = b.hook("Hook");
        let load = b.load("Load", 8.0, 4.0, 2.0, 1_000.0);
        let a = b.lug(load, "A", -3.0, 0.0, 2.0);
        let c = b.lug(load, "B", 3.0, 0.0, 2.0);
        b.member("short")
            .from(hook)
            .to(a)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 8.0));
        b.member("long")
            .from(hook)
            .to(c)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 12.0));
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        let short = ev.member_named(&rig, "short").unwrap();
        let long = ev.member_named(&rig, "long").unwrap();
        assert!(short.slack_ft[0] < 1e-6, "{}", short.slack_ft[0]);
        assert!(long.slack_ft[0] > 1e-3, "{}", long.slack_ft[0]);
        assert!(ev.has_residual(ResidualKind::Slack));
        let d_short = theoretical_drop(8.0, 3.0);
        let d_long = theoretical_drop(12.0, 3.0);
        let unequal_in = (d_long - d_short) * 12.0;
        let r = ev
            .residuals
            .iter()
            .find(|r| r.kind == ResidualKind::UnequalDrop)
            .expect("unequal");
        assert!(close(r.value, unequal_in));
        let pa = ev.nodes.get(&a).unwrap();
        assert!(close(pa[2], -d_short));
    }

    #[test]
    fn short_leg_is_geom() {
        let mut b = RigBuilder::new("short");
        let hook = b.hook("Hook");
        let load = b.load("Load", 30.0, 4.0, 2.0, 1_000.0);
        let a = b.lug(load, "A", -15.0, 0.0, 2.0);
        let c = b.lug(load, "B", 15.0, 0.0, 2.0);
        b.member("leg A")
            .from(hook)
            .to(a)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 10.0));
        b.member("leg B")
            .from(hook)
            .to(c)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 10.0));
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        assert!(ev.has_residual(ResidualKind::Short));
        assert!(ev.max_short_ft() > 0.0);
        let m = ev.member_named(&rig, "leg A").unwrap();
        assert!(close(m.angle_deg[0], 0.0));
        assert!(close(m.drop_ft[0], 0.0) || m.short_ft[0] > 0.0);
    }

    #[test]
    fn single_leg_is_underconstrained() {
        let mut b = RigBuilder::new("one");
        let hook = b.hook("Hook");
        let load = b.load("Load", 4.0, 4.0, 2.0, 500.0);
        let a = b.lug(load, "A", 0.0, 0.0, 2.0);
        b.member("leg")
            .from(hook)
            .to(a)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 8.0));
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        assert!(ev.has_residual(ResidualKind::Underconstrained));
        let p = ev.nodes.get(&a).unwrap();
        assert!(close(p[0], 0.0) && close(p[1], 0.0));
        assert!(close(p[2], -8.0));
    }

    #[test]
    fn offset_bow_raises_bearing_split_assumed() {
        let mut b = RigBuilder::new("offset-bow");
        let hook = b.hook("Hook");
        let bar = b.bar("Bar", 8.0, 100.0);
        b.pin(bar, Coord3::new(4.0, 0.0, -1.0));
        let bow = b.bow(bar, "bow", 0.0, 0.0, 0.0, 2.0, 0.0);
        let hang = b.lug(bar, "hang", 0.0, 0.0, 0.1);
        b.member("bar hang")
            .from(hook)
            .to(hang)
            .segment(|s| s.roundsling(7, Hitch::Vertical, 16.0));
        let load = b.load("Load", 12.0, 4.0, 2.0, 2_000.0);
        let p1 = b.lug(load, "P1", -6.0, 0.0, 2.0);
        let p3 = b.lug(load, "P3", 6.0, 0.0, 2.0);
        b.member("hook A")
            .from(hook)
            .to(p1)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 20.0));
        b.member("hook B")
            .from(hook)
            .to(p3)
            .segment(|s| s.roundsling(5, Hitch::Vertical, 20.0));
        b.member("basket")
            .from(p1)
            .through(bow)
            .to(p3)
            .segment(|s| s.strap(11.0))
            .segment(|s| s.strap(11.0));
        let rig = b.finish().expect("valid");
        let ev = EvalRig::evaluate(&rig).expect("eval");
        assert!(
            ev.has_residual(ResidualKind::BearingSplitAssumed),
            "{:?}",
            ev.residuals
        );
        let m = ev.member_named(&rig, "basket").unwrap();
        assert_eq!(m.wrap_deg.len(), 1);
        assert!(m.wrap_deg[0].is_finite() && m.wrap_deg[0] > 0.0);
        assert!((m.nominal_ft[0] - m.nominal_ft[1]).abs() > 1e-6);
    }

    #[test]
    fn two_leg_bridle_matches_hand_calc() {
        // 12 ft slings, picks 12 ft apart → reach 6, acos(0.5) = 60°, drop √108
        let rig = crate::rig::two_leg_bridle(1_000.0, 12.0, 12.0);
        let ev = EvalRig::evaluate(&rig).expect("eval");
        let m = ev.member_named(&rig, "leg A").unwrap();
        assert!(close(m.reach_ft[0], 6.0));
        assert!(close(m.angle_deg[0], 60.0));
        assert!(close(m.drop_ft[0], 108.0_f64.sqrt()));
        assert!(close(ev.height.hook_to_pick_ft, 108.0_f64.sqrt()));
    }

    #[test]
    fn duplo10_evaluates_and_bars_sit_below_the_hook() {
        let rig = duplo10();
        let ev = EvalRig::evaluate(&rig).expect("eval");
        assert!(ev.nodes.len() == rig.nodes.len());
        for (id, p) in &ev.nodes {
            assert!(
                p.iter().all(|c| c.is_finite()),
                "NaN at {}",
                rig.nodes.get(id).unwrap().label
            );
        }
        let hook_z = ev.root_at[2];
        let top = rig.body_named("Bar top").unwrap();
        let bar_l = rig.body_named("Bar L").unwrap();
        let bar_c = rig.body_named("Bar C").unwrap();
        let bar_r = rig.body_named("Bar R").unwrap();
        let load = rig.body_named("Al MDC enclosure").unwrap();
        let z_top = ev.bodies.get(&top.id).unwrap().frame.origin[2];
        let z_l = ev.bodies.get(&bar_l.id).unwrap().frame.origin[2];
        let z_c = ev.bodies.get(&bar_c.id).unwrap().frame.origin[2];
        let z_r = ev.bodies.get(&bar_r.id).unwrap().frame.origin[2];
        let z_load = ev.bodies.get(&load.id).unwrap().frame.origin[2];
        assert!(z_top < hook_z - 1.0, "top bar {z_top}");
        assert!(z_l < z_top - 1.0, "bar L {z_l} vs top {z_top}");
        assert!(z_r < z_top - 1.0, "bar R {z_r}");
        assert!(z_c < hook_z - 1.0, "bar C {z_c}");
        assert!(z_load < z_l.min(z_c).min(z_r), "load {z_load}");
        // Provisional: 16 ft hook legs, reach √(8.5²+1) → drop √182.75 ≈ 13.519
        assert!((z_top + (182.75_f64).sqrt()).abs() < 0.05, "top {z_top}");
        assert!(!ev.has_residual(ResidualKind::Short));
        // Symmetric baskets split evenly.
        for label in [
            "Basket strap P1–P3 (front)",
            "Basket strap P1–P3 (back)",
            "Basket strap P5–P7 (front)",
            "Basket strap P5–P7 (back)",
        ] {
            let m = ev.member_named(&rig, label).unwrap();
            assert!(
                (m.nominal_ft[0] - m.nominal_ft[1]).abs() < 1e-9,
                "{label} split {} {}",
                m.nominal_ft[0],
                m.nominal_ft[1]
            );
            assert_eq!(m.wrap_deg.len(), 1);
        }
        assert!(
            !ev.residuals
                .iter()
                .any(|r| r.kind == ResidualKind::BearingSplitAssumed)
        );
    }

    #[test]
    fn duplo10_sweep_angle_height_and_short() {
        let rig = duplo10();
        let ang = sweep(&rig.params, |t| {
            EvalRig::evaluate_with(&rig, t)
                .map(|e| e.governing_angle_deg())
                .map_err(|es| es.into_iter().next().unwrap())
        })
        .unwrap();
        assert!(ang.nominal.is_finite() && ang.min.is_finite() && ang.max.is_finite());
        assert!(ang.min <= ang.nominal && ang.nominal <= ang.max);
        let h = sweep(&rig.params, |t| {
            EvalRig::evaluate_with(&rig, t)
                .map(|e| e.height.hook_to_pick_ft)
                .map_err(|es| es.into_iter().next().unwrap())
        })
        .unwrap();
        assert!(h.nominal > 0.0);
        assert!(h.min <= h.nominal && h.nominal <= h.max);
        let short = sweep(&rig.params, |t| {
            EvalRig::evaluate_with(&rig, t)
                .map(|e| e.max_short_ft())
                .map_err(|es| es.into_iter().next().unwrap())
        })
        .unwrap();
        assert!(
            short.nominal.abs() < 1e-9,
            "nominal short {}",
            short.nominal
        );
    }

    #[test]
    fn sweep_angle_and_height_on_two_leg() {
        let mut b = RigBuilder::new("sweep");
        let len = b
            .param("sling_len", Quantity::Length, 12.0)
            .tol(1.0, 1.0)
            .assumed();
        let hook = b.hook("Hook");
        let load = b.load("Load", 12.0, 4.0, 2.0, 1_000.0);
        let a = b.lug(load, "A", -6.0, 0.0, 2.0);
        let c = b.lug(load, "B", 6.0, 0.0, 2.0);
        b.member("leg A")
            .from(hook)
            .to(a)
            .segment(|s| s.roundsling_len(5, Hitch::Vertical, len));
        b.member("leg B")
            .from(hook)
            .to(c)
            .segment(|s| s.roundsling_len(5, Hitch::Vertical, len));
        let rig = b.finish().expect("valid");
        let ang = sweep(&rig.params, |t| {
            EvalRig::evaluate_with(&rig, t)
                .map(|e| e.governing_angle_deg())
                .map_err(|es| es.into_iter().next().unwrap())
        })
        .unwrap();
        assert!(close(ang.nominal, 60.0));
        assert!(ang.min < ang.nominal && ang.max > ang.nominal);
        let h = sweep(&rig.params, |t| {
            EvalRig::evaluate_with(&rig, t)
                .map(|e| e.height.hook_to_pick_ft)
                .map_err(|es| es.into_iter().next().unwrap())
        })
        .unwrap();
        assert!(h.min < h.nominal && h.max > h.nominal);
        let short = sweep(&rig.params, |t| {
            EvalRig::evaluate_with(&rig, t)
                .map(|e| e.max_short_ft())
                .map_err(|es| es.into_iter().next().unwrap())
        })
        .unwrap();
        assert!(close(short.nominal, 0.0));
    }
}
