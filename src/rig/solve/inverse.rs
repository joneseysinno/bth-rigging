//! Chain setting for level / take-up.
//!
//! Step: 3
//! Theory: roadmap Step 3 — inverse setting. Two questions a rigger asks of
//! the adjusters (chains, turnbuckles — any component with `Adjust`):
//!
//! * **Level.** What settings make every free body hang at its target
//!   attitude (level, or the authored `Level` rotation)? Hang, rotate each
//!   tilted body back to its target about its own CG (the hang already put the
//!   CG under its supports), read the chord each adjustable member needs at
//!   that pose, set it, hang again. For a rig whose adjusters are enough to
//!   level it this converges in one or two passes.
//! * **Take-up.** How much must each slack adjustable member be shortened to
//!   come just snug at the solved pose? With inextensible (rigid) rigging a
//!   snug chain still carries nothing; how much it picks up past snug depends
//!   on stiffness — that is the target-split question, deferred until members
//!   carry real `stiffness_lb` (Step 7 tolerances).
//!
//! Settings outside the adjuster's `[min, max]` are reported, not clamped.
//! Inputs: target geometry.
//! Outputs: required chain / length settings.
//! Must not depend on: UI, dioxus.

use serde::{Deserialize, Serialize};

use super::model::{Model, Pose, State, norm, sub};
use super::{SolvedRig, solve};
use crate::rig::body::Placement;
use crate::rig::id::MemberId;
use crate::rig::param::Expr;
use crate::rig::{Rig, RigError};

/// One adjuster change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingChange {
    pub member: MemberId,
    pub label: String,
    /// Segment and component index of the adjuster that takes the change.
    pub segment: usize,
    pub component: usize,
    pub from_ft: f64,
    pub to_ft: f64,
    pub min_ft: f64,
    pub max_ft: f64,
}

impl SettingChange {
    pub fn in_range(&self) -> bool {
        self.to_ft >= self.min_ft - 1e-9 && self.to_ft <= self.max_ft + 1e-9
    }
}

#[derive(Debug, Clone)]
pub struct InverseResult {
    pub changes: Vec<SettingChange>,
    /// The rig with the new settings written in as constants.
    pub rig: Rig,
    /// Hung with the new settings.
    pub solved: SolvedRig,
    /// Largest remaining tilt from target over free bodies, deg.
    pub max_tilt_deg: f64,
    pub passes: usize,
    /// False when a required setting falls outside its adjuster's range;
    /// `solved` is then the last pose that could be hung.
    pub feasible: bool,
}

/// Tilt tolerance for "level", deg.
pub const LEVEL_TOL_DEG: f64 = 0.01;

/// Settings that hang every free body at its target attitude.
pub fn level(rig: &Rig) -> Result<InverseResult, Vec<RigError>> {
    let original = settings_of(rig)?;
    let mut work = rig.clone();
    let mut passes = 0;
    let mut solved = solve(&work)?;
    loop {
        let tilt = max_tilt(&work, &solved)?;
        if tilt <= LEVEL_TOL_DEG || passes >= 10 {
            let changes = diff(&original, &settings_of(&work)?);
            return Ok(InverseResult {
                feasible: changes.iter().all(SettingChange::in_range),
                changes,
                rig: work,
                solved,
                max_tilt_deg: tilt,
                passes,
            });
        }
        passes += 1;
        let (model, hung_state) = Model::build(&work, &solved.eval)?;
        let target = level_state(&work, &model, &hung_state)?;
        let mut changed = false;
        for (i, mm) in model.members.iter().enumerate() {
            if mm.adjust.is_none() {
                continue;
            }
            let need = model.path_length(&target, i);
            let delta = need - mm.length;
            if delta.abs() > 1e-9 {
                set_delta(&mut work, mm.id, delta)?;
                changed = true;
            }
        }
        let changes = diff(&original, &settings_of(&work)?);
        let in_range = changes.iter().all(SettingChange::in_range);
        if !changed || !in_range {
            return Ok(InverseResult {
                feasible: in_range,
                changes,
                rig: work,
                solved,
                max_tilt_deg: tilt,
                passes,
            });
        }
        solved = solve(&work)?;
    }
}

/// Shorten each slack adjustable member until it is just snug at the solved pose.
pub fn take_up(rig: &Rig) -> Result<InverseResult, Vec<RigError>> {
    let original = settings_of(rig)?;
    let solved = solve(rig)?;
    let (model, state) = Model::build(rig, &solved.eval)?;
    let mut work = rig.clone();
    for (i, mm) in model.members.iter().enumerate() {
        if mm.adjust.is_none() {
            continue;
        }
        let slack = mm.length - model.path_length(&state, i);
        if slack > 1e-9 {
            set_delta(&mut work, mm.id, -slack)?;
        }
    }
    let changes = diff(&original, &settings_of(&work)?);
    if !changes.iter().all(SettingChange::in_range) {
        let tilt = max_tilt(rig, &solved)?;
        return Ok(InverseResult {
            changes,
            rig: work,
            solved,
            max_tilt_deg: tilt,
            passes: 1,
            feasible: false,
        });
    }
    let solved = solve(&work)?;
    let tilt = max_tilt(&work, &solved)?;
    Ok(InverseResult {
        changes,
        rig: work,
        solved,
        max_tilt_deg: tilt,
        passes: 1,
        feasible: true,
    })
}

/// Target pose: each free body rotated to its target attitude about its CG.
fn level_state(rig: &Rig, model: &Model, hung: &State) -> Result<State, Vec<RigError>> {
    let mut s = hung.clone();
    for (i, b) in model.bodies.iter().enumerate() {
        let target = target_pose(rig, b.id)?;
        let cg = model.cg(hung, i);
        let rc = target.rotate(b.cg_local);
        s.poses[i] = Pose {
            origin: sub(cg, rc),
            r: target.r,
        };
    }
    Ok(s)
}

fn target_pose(rig: &Rig, body: crate::rig::id::BodyId) -> Result<Pose, Vec<RigError>> {
    let rot = match rig.bodies.get(&body).map(|b| b.placement()) {
        Some(Placement::Level { rot }) => {
            let [y, p, r] = rot.eval(&rig.params).map_err(|e| vec![e])?;
            crate::rig::eval::Rot::from_degrees(y, p, r)
        }
        _ => crate::rig::eval::Rot::identity(),
    };
    Ok(Pose::from_frame(&crate::rig::eval::Frame {
        origin: [0.0; 3],
        rot,
    }))
}

/// Largest angle between a free body's attitude and its target, deg.
fn max_tilt(rig: &Rig, solved: &SolvedRig) -> Result<f64, Vec<RigError>> {
    let (model, state) = Model::build(rig, &solved.eval)?;
    let mut worst: f64 = 0.0;
    for (i, b) in model.bodies.iter().enumerate() {
        let target = target_pose(rig, b.id)?;
        let p = state.poses[i];
        // Angle of R_targetᵀ R: tilt of each local axis, take the largest.
        for ax in 0..3 {
            let mut e = [0.0; 3];
            e[ax] = 1.0;
            let a = p.rotate(e);
            let t = target.rotate(e);
            let c = (a[0] * t[0] + a[1] * t[1] + a[2] * t[2]) / (norm(a) * norm(t));
            worst = worst.max(c.clamp(-1.0, 1.0).acos().to_degrees());
        }
    }
    Ok(worst)
}

/// (member, segment, component, setting, min, max) for every adjuster.
type Setting = (MemberId, String, usize, usize, f64, f64, f64);

fn settings_of(rig: &Rig) -> Result<Vec<Setting>, Vec<RigError>> {
    let mut out = Vec::new();
    for (id, m) in &rig.members {
        for (si, seg) in m.segments.iter().enumerate() {
            for (ci, c) in seg.components.iter().enumerate() {
                if let Some(a) = &c.adjust {
                    let (min, max, set) = a.eval(&rig.params).map_err(|e| vec![e])?;
                    out.push((*id, m.label.clone(), si, ci, set, min, max));
                }
            }
        }
    }
    Ok(out)
}

fn diff(before: &[Setting], after: &[Setting]) -> Vec<SettingChange> {
    before
        .iter()
        .zip(after)
        .filter(|(a, b)| (a.4 - b.4).abs() > 1e-12)
        .map(|(a, b)| SettingChange {
            member: a.0,
            label: a.1.clone(),
            segment: a.2,
            component: a.3,
            from_ft: a.4,
            to_ft: b.4,
            min_ft: b.5,
            max_ft: b.6,
        })
        .collect()
}

/// Add `delta` ft to the first adjuster on `member` (written as a constant).
fn set_delta(rig: &mut Rig, member: MemberId, delta: f64) -> Result<(), Vec<RigError>> {
    let params = rig.params.clone();
    let Some(m) = rig.members.get_mut(&member) else {
        return Ok(());
    };
    for seg in m.segments.iter_mut() {
        for c in seg.components.iter_mut() {
            if let Some(a) = c.adjust.as_mut() {
                let now = a.setting.eval(&params).map_err(|e| vec![e])?;
                a.setting = Expr::c(now + delta);
                return Ok(());
            }
        }
    }
    Ok(())
}
