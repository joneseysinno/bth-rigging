//! Solver pipeline: hang the evaluated rig, split the load, report.
//!
//! Step: 3
//! Theory: roadmap Step 3 — determinacy (Readback §3.5, `s = m − r`,
//! `k = dof − r`) and a tension-only solve. The pipeline is
//! `EvalRig` (Step 2 descent pose) → `Model` → `hang` (pose under gravity,
//! multipliers = tensions) → determinacy at the hung pose → `SolvedRig`.
//! `statics_at` is the small-displacement split at a given pose (no hang);
//! it is what the layer-engine equivalence gate compares, because for a
//! symmetric template the descent pose already is the equilibrium.
//! Inputs: evaluated graph.
//! Outputs: tensions and reactions.
//! Must not depend on: UI, dioxus.

pub mod bounds;
pub mod elastic;
pub mod envelope;
pub mod equilibrium;
pub mod hang;
pub mod inverse;
pub mod linalg;
pub mod lp;
pub mod model;
pub mod rank;
pub mod rate;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::eval::{EvalRig, Frame};
use super::id::{BodyId, MemberId, NodeId};
use super::{Rig, RigError};
use equilibrium::{Equilibrium, Unknown, held_resultants, unit_forces};
use hang::{HangOptions, Hung};
use model::{MemberKind, Model, State, norm, scale, sub};
use rank::{Determinacy, determinacy};

pub use model::{Pose, RIGID_EA_FACTOR};

/// Force result for one member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberForce {
    pub label: String,
    /// Tension (lb). For a link, the magnitude of the joint force.
    pub tension_lbs: f64,
    /// Carries load in the solution.
    pub taut: bool,
    pub is_link: bool,
    /// Force the member applies to each path stop, world lb.
    pub stop_forces: Vec<[f64; 3]>,
    /// Actual chord angle from horizontal per segment at the solved pose, deg.
    pub angle_deg: Vec<f64>,
    /// Wrap angle at each interior bearing, deg.
    pub wrap_deg: Vec<f64>,
}

/// Support reaction on a held body (hook, pinned or posed body).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub body: BodyId,
    pub label: String,
    /// Force the support must supply, lb (hook: +z up).
    pub force: [f64; 3],
    /// Moment about the body's CG / hold point, lb·ft.
    pub moment: [f64; 3],
}

/// How a tension set was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    /// Small-displacement split at a fixed pose.
    Statics,
    /// Hung under gravity (finite displacement), multipliers.
    Hang,
}

/// A solved rig.
#[derive(Debug, Clone)]
pub struct SolvedRig {
    pub method: Method,
    /// Geometry the forces are in equilibrium with.
    pub eval: EvalRig,
    pub members: IndexMap<MemberId, MemberForce>,
    /// Determinacy of the members carrying load at the solution.
    pub determinacy: Determinacy,
    /// Determinacy of the whole graph (every member taut) at the solved pose.
    pub topology: Determinacy,
    /// Vertical pull on the hook, lb.
    pub hook_load_lbs: f64,
    pub reactions: Vec<Reaction>,
    /// Tilt of each free body's local z from vertical, deg.
    pub tilt_deg: IndexMap<BodyId, f64>,
    pub converged: bool,
    pub iterations: usize,
    /// ‖A t − b‖∞, lb.
    pub residual_lbs: f64,
}

impl SolvedRig {
    pub fn tension_of(&self, rig: &Rig, label: &str) -> Option<f64> {
        let id = rig.members.iter().find(|(_, m)| m.label == label)?.0;
        self.members.get(id).map(|m| m.tension_lbs)
    }

    pub fn max_tension_lbs(&self) -> f64 {
        self.members
            .values()
            .filter(|m| !m.is_link)
            .map(|m| m.tension_lbs)
            .fold(0.0, f64::max)
    }

    pub fn slack_members(&self) -> Vec<MemberId> {
        self.members
            .iter()
            .filter(|(_, m)| !m.taut && !m.is_link)
            .map(|(id, _)| *id)
            .collect()
    }
}

/// Hang the rig and solve it. The default Step 3 entry point.
pub fn solve(rig: &Rig) -> Result<SolvedRig, Vec<RigError>> {
    let eval = EvalRig::evaluate(rig)?;
    solve_eval(rig, &eval)
}

/// Hang from an existing evaluation.
pub fn solve_eval(rig: &Rig, eval: &EvalRig) -> Result<SolvedRig, Vec<RigError>> {
    let (model, start) = Model::build(rig, eval)?;
    let allowed = vec![true; model.members.len()];
    let hung = hang::hang(&model, &start, &allowed, HangOptions::default());
    finish_hung(rig, eval, &model, hung)
}

/// Hang with some members forced slack (bounding cases).
pub fn solve_with_slack(
    rig: &Rig,
    eval: &EvalRig,
    slack: &[MemberId],
) -> Result<SolvedRig, Vec<RigError>> {
    let (model, start) = Model::build(rig, eval)?;
    let allowed: Vec<bool> = model
        .members
        .iter()
        .map(|m| !slack.contains(&m.id))
        .collect();
    let hung = hang::hang(&model, &start, &allowed, HangOptions::default());
    finish_hung(rig, eval, &model, hung)
}

/// Small-displacement elastic split at the pose `eval` already has (no hang).
pub fn statics_at(rig: &Rig, eval: &EvalRig) -> Result<SolvedRig, Vec<RigError>> {
    let (model, state) = Model::build(rig, eval)?;
    let active: Vec<bool> = (0..model.members.len())
        .map(|i| match model.members[i].kind {
            MemberKind::Link => true,
            MemberKind::Tension => {
                let m = &model.members[i];
                model.path_length(&state, i) >= m.length - 1e-6 * m.length.max(1.0)
            }
        })
        .collect();
    let split = elastic::split(&model, &state, &active);
    let det = determinacy(&split.eq, model.w_total);
    let det = Determinacy {
        consistent: split.consistent,
        residual_lbs: split.residual_lbs,
        ..det
    };
    report(
        rig,
        eval.clone(),
        &model,
        &state,
        &split.eq,
        &split.t,
        &split.active,
        det,
        Method::Statics,
        true,
        0,
    )
}

fn finish_hung(
    rig: &Rig,
    base: &EvalRig,
    model: &Model,
    hung: Hung,
) -> Result<SolvedRig, Vec<RigError>> {
    let (frames, nodes) = world(rig, base, model, &hung.state);
    let eval = EvalRig::with_pose(rig, base, frames, nodes)?;
    let det = determinacy(&hung.eq, model.w_total);
    let det = Determinacy {
        consistent: hung.force_residual_lbs <= rank::CONSISTENT_RTOL * model.w_total,
        residual_lbs: hung.force_residual_lbs,
        ..det
    };
    report(
        rig,
        eval,
        model,
        &hung.state,
        &hung.eq,
        &hung.t,
        &hung.active,
        det,
        Method::Hang,
        hung.converged,
        hung.iterations,
    )
}

/// World frames and node positions for a model state.
pub fn world(
    rig: &Rig,
    base: &EvalRig,
    model: &Model,
    state: &State,
) -> (IndexMap<BodyId, Frame>, IndexMap<NodeId, [f64; 3]>) {
    let mut frames = IndexMap::new();
    for (id, be) in &base.bodies {
        let f = match model.body_index.get(id) {
            Some(&i) => {
                let p = state.poses[i];
                Frame {
                    origin: p.origin,
                    rot: p.to_rot(),
                }
            }
            None => be.frame,
        };
        frames.insert(*id, f);
    }
    let mut nodes = IndexMap::new();
    for nid in rig.nodes.keys() {
        nodes.insert(*nid, model.pos(state, *nid));
    }
    (frames, nodes)
}

#[allow(clippy::too_many_arguments)]
fn report(
    rig: &Rig,
    eval: EvalRig,
    model: &Model,
    state: &State,
    eq: &Equilibrium,
    t: &[f64],
    active: &[bool],
    det: Determinacy,
    method: Method,
    converged: bool,
    iterations: usize,
) -> Result<SolvedRig, Vec<RigError>> {
    let mut members = IndexMap::new();
    for (i, m) in model.members.iter().enumerate() {
        let me = eval.members.get(&m.id);
        let mut stop_forces = vec![[0.0; 3]; m.path.len()];
        let mut tension = 0.0;
        let mut link = [0.0; 3];
        for (col, u) in eq.unknowns.iter().enumerate() {
            let hit = match u {
                Unknown::Tension(k) | Unknown::Link(k, _) => *k == i,
            };
            if !hit {
                continue;
            }
            if let Unknown::Tension(_) = u {
                tension = t[col];
            }
            if let Unknown::Link(_, ax) = u {
                link[*ax as usize] = t[col];
            }
            for (node, f) in unit_forces(model, state, *u) {
                if let Some(k) = m.path.iter().position(|n| *n == node) {
                    stop_forces[k] = model::add(stop_forces[k], scale(f, t[col]));
                }
            }
        }
        let is_link = m.kind == MemberKind::Link;
        if is_link {
            tension = norm(link);
        }
        let angle_deg = m
            .path
            .windows(2)
            .map(|w| {
                let d = sub(model.pos(state, w[1]), model.pos(state, w[0]));
                let h = (d[0] * d[0] + d[1] * d[1]).sqrt();
                if norm(d) < 1e-12 {
                    90.0
                } else {
                    d[2].abs().atan2(h).to_degrees()
                }
            })
            .collect();
        members.insert(
            m.id,
            MemberForce {
                label: m.label.clone(),
                tension_lbs: tension,
                taut: active.get(i).copied().unwrap_or(false) && (is_link || tension > 0.0),
                is_link,
                stop_forces,
                angle_deg,
                wrap_deg: me.map(|e| e.wrap_deg.clone()).unwrap_or_default(),
            },
        );
    }

    let mut hook_load_lbs = 0.0;
    let mut reactions = Vec::new();
    for (body, f, mo) in held_resultants(model, state, eq, t) {
        if body == model.root_body {
            hook_load_lbs = -f[2];
        }
        reactions.push(Reaction {
            body,
            label: rig
                .bodies
                .get(&body)
                .map(|b| b.label.clone())
                .unwrap_or_default(),
            force: scale(f, -1.0),
            moment: scale(mo, -1.0),
        });
    }

    let all = vec![true; model.members.len()];
    let topology = determinacy(&equilibrium::assemble(model, state, &all), model.w_total);

    let tilt_deg = model
        .bodies
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, state.poses[i].tilt_deg()))
        .collect();

    Ok(SolvedRig {
        method,
        eval,
        members,
        residual_lbs: det.residual_lbs,
        determinacy: det,
        topology,
        hook_load_lbs,
        reactions,
        tilt_deg,
        converged,
        iterations,
    })
}

#[cfg(test)]
mod tests;
