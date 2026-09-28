//! Equilibrium matrix rank: s = m − r, k = dof − r.
//!
//! Step: 3
//! Theory: Readback §3.5 determinacy. `m` unknown member forces, `dof`
//! equilibrium equations, `r = rank(A)`. `s > 0`: statically indeterminate —
//! `s` independent self-stress states, the split depends on stiffness and
//! fit-up (→ `elastic`, `bounds`). `k > 0`: mechanisms — the rig can move
//! without stretching anything (pendulum sway, a bar spinning on its own axis).
//! A hanging rig is normally a mechanism held by gravity; what matters is
//! whether the load is **consistent** (`b ∈ range(A)`) at the current pose.
//! If not, the pose is not an equilibrium and the rig must `hang`.
//! Inputs: connectivity and free DOF.
//! Outputs: rank, static indeterminacy, kinematic DOF.
//! Must not depend on: UI, dioxus.

use serde::{Deserialize, Serialize};

use super::equilibrium::{Equilibrium, residual};
use super::linalg::{MECH_RTOL, Svd};

/// Determinacy of one equilibrium system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Determinacy {
    /// Unknown member forces (1 per tension member, 3 per link).
    pub m: usize,
    /// Equilibrium equations (6 per free body, 3 per free knot).
    pub dof: usize,
    pub r: usize,
    /// Static indeterminacy `m − r`.
    pub s: usize,
    /// Mechanisms `dof − r`.
    pub k: usize,
    /// `b ∈ range(A)`: this pose can be in equilibrium (ignoring signs).
    pub consistent: bool,
    /// ‖A t − b‖∞ of the least-squares solution, lb.
    pub residual_lbs: f64,
}

impl Determinacy {
    pub fn is_determinate(&self) -> bool {
        self.s == 0
    }
}

/// Consistency tolerance relative to the total hanging weight.
pub const CONSISTENT_RTOL: f64 = 1e-8;

pub fn determinacy(eq: &Equilibrium, w_total: f64) -> Determinacy {
    let svd = Svd::of(&eq.a);
    let r = svd.rank_with(MECH_RTOL);
    let t = svd.solve(&eq.b);
    let res = residual(eq, &t);
    let m = eq.a.cols;
    let dof = eq.a.rows;
    Determinacy {
        m,
        dof,
        r,
        s: m.saturating_sub(r),
        k: dof.saturating_sub(r),
        consistent: res <= CONSISTENT_RTOL * w_total.max(1.0),
        residual_lbs: res,
    }
}
