//! Series stiffness, tension-only force method.
//!
//! Step: 3
//! Theory: roadmap Step 3 — elastic split of an indeterminate rig at a fixed
//! geometry. Each member is a series spring: compliance `c = Σ Lᵢ / EAᵢ`
//! (components without `stiffness_lb` use the rigid default, see
//! `model::RIGID_EA_FACTOR`). Among all force sets in equilibrium
//! (`A t = b`) the elastic one minimises complementary energy `½ Σ cᵢ tᵢ²`:
//! `t = C^½ · pinv(A C^½) · b`. For a determinate rig that is simply the
//! unique solution. Tension-only: a member that comes out in compression is
//! made slack and the split is re-solved (active set), until every tension
//! member is ≥ 0 or the load can no longer be carried in this pose.
//! Inputs: model + pose + active set.
//! Outputs: tensions with slack handling.
//! Must not depend on: UI, dioxus.

use super::equilibrium::{Equilibrium, Unknown, assemble, residual};
use super::linalg::Svd;
use super::model::{MemberKind, Model, State};
use super::rank::CONSISTENT_RTOL;

/// Link (ball-joint) flexibility relative to the stiffest tension member.
/// Small, so a link never steals load from a real member, but non-zero so
/// self-stress between two links on the same pair of bodies is resolved to 0.
pub const LINK_COMPLIANCE_FACTOR: f64 = 1e-3;

/// Compliance per unknown column of `eq`.
pub fn column_compliance(model: &Model, eq: &Equilibrium) -> Vec<f64> {
    let c_min = model
        .members
        .iter()
        .filter(|m| m.kind == MemberKind::Tension)
        .map(|m| m.compliance)
        .fold(f64::INFINITY, f64::min);
    let c_link = if c_min.is_finite() {
        c_min * LINK_COMPLIANCE_FACTOR
    } else {
        1e-12
    };
    eq.unknowns
        .iter()
        .map(|u| match u {
            Unknown::Tension(i) => model.members[*i].compliance.max(1e-300),
            Unknown::Link(..) => c_link,
        })
        .collect()
}

/// Minimum complementary-energy force set for the given equilibrium.
pub fn min_energy(model: &Model, eq: &Equilibrium) -> Vec<f64> {
    let c = column_compliance(model, eq);
    let mut a = eq.a.clone();
    let w: Vec<f64> = c.iter().map(|x| x.sqrt()).collect();
    for (j, s) in w.iter().enumerate() {
        a.scale_col(j, *s);
    }
    let y = Svd::of(&a).solve(&eq.b);
    y.iter().zip(&w).map(|(y, w)| y * w).collect()
}

/// Result of a small-displacement elastic split.
#[derive(Debug, Clone)]
pub struct Split {
    pub eq: Equilibrium,
    /// Value per `eq.unknowns` column.
    pub t: Vec<f64>,
    /// Per model member: carries load in this split.
    pub active: Vec<bool>,
    /// Members made slack because they came out in compression.
    pub dropped: Vec<usize>,
    pub residual_lbs: f64,
    pub consistent: bool,
}

/// Tension-only elastic split at a fixed pose, starting from `active`.
pub fn split(model: &Model, state: &State, active: &[bool]) -> Split {
    let mut active = active.to_vec();
    let mut dropped = Vec::new();
    let tol = 1e-9 * model.w_total;
    loop {
        let eq = assemble(model, state, &active);
        let t = min_energy(model, &eq);
        let res = residual(&eq, &t);
        let worst = eq
            .unknowns
            .iter()
            .zip(&t)
            .filter_map(|(u, v)| match u {
                Unknown::Tension(i) if *v < -tol => Some((*i, *v)),
                _ => None,
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        match worst {
            Some((i, _)) => {
                active[i] = false;
                dropped.push(i);
            }
            None => {
                return Split {
                    consistent: res <= CONSISTENT_RTOL * model.w_total,
                    residual_lbs: res,
                    eq,
                    t,
                    active,
                    dropped,
                };
            }
        }
    }
}
