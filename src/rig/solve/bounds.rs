//! Slack-chain / slack-basket bounding cases and the tension envelope.
//!
//! Step: 3
//! Theory: roadmap Step 3 — bounding cases. An indeterminate rig (`s > 0`)
//! has no single "true" split without exact lengths and stiffnesses: a chain
//! set a link long, a basket that beds in, and the load moves to other legs.
//! Two views, both reported, neither replaces the nominal solve:
//!
//! 1. **Envelope.** At the solved pose, every force set in equilibrium with
//!    tension-only members (`A t = b`, `t ≥ 0`, links free) is admissible for
//!    *some* combination of length errors. The range of each member tension
//!    over that set is one LP per bound. It answers "how bad can this leg get
//!    if the others are off" — the honest worst case for a rigid rig.
//! 2. **Named cases.** Hang the rig again with a group of members forced
//!    slack: all chains/adjusters (`ChainsSlack`), all bearing members —
//!    baskets and chokers (`BasketsSlack`). A case that leaves a body
//!    unsupported is reported as not carrying.
//!
//! Inputs: graph with possible slack members.
//! Outputs: bounding tension envelopes.
//! Must not depend on: UI, dioxus.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::equilibrium::{Unknown, assemble};
use super::linalg::{MECH_RTOL, Mat, Svd};
use super::lp::{LpResult, maximize};
use super::model::Model;
use super::{SolvedRig, solve_with_slack};
use crate::rig::component::ComponentKind;
use crate::rig::eval::EvalRig;
use crate::rig::id::MemberId;
use crate::rig::{Rig, RigError};

/// Range of one member's tension over all admissible splits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TensionRange {
    pub min_lbs: f64,
    /// `f64::INFINITY` when a self-stress state can pre-tension it without limit.
    pub max_lbs: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub members: IndexMap<MemberId, TensionRange>,
    /// False when no tension-only split exists at this pose.
    pub feasible: bool,
}

impl Envelope {
    pub fn max_of(&self, id: MemberId) -> Option<f64> {
        self.members.get(&id).map(|r| r.max_lbs)
    }
}

/// LP envelope at the pose of `solved` (all tension members may carry).
pub fn envelope(rig: &Rig, solved: &SolvedRig) -> Result<Envelope, Vec<RigError>> {
    let (model, state) = Model::build(rig, &solved.eval)?;
    let all = vec![true; model.members.len()];
    let eq = assemble(&model, &state, &all);
    let svd = Svd::of(&eq.a);
    let (ra, rb) = svd.reduce_rows_with(&eq.a, &eq.b, MECH_RTOL);
    let w = model.w_total.max(1.0);
    let rb: Vec<f64> = rb.iter().map(|v| v / w).collect();

    // Variables: tension columns as-is (≥ 0); link columns split into ±.
    let mut cols: Vec<(usize, f64)> = Vec::new(); // (eq column, sign)
    for (c, u) in eq.unknowns.iter().enumerate() {
        match u {
            Unknown::Tension(_) => cols.push((c, 1.0)),
            Unknown::Link(..) => {
                cols.push((c, 1.0));
                cols.push((c, -1.0));
            }
        }
    }
    let mut a = Mat::zeros(ra.rows, cols.len());
    for i in 0..ra.rows {
        for (k, (c, s)) in cols.iter().enumerate() {
            a[(i, k)] = ra[(i, *c)] * s;
        }
    }

    let mut members = IndexMap::new();
    let mut feasible = true;
    for (k, (c, _)) in cols.iter().enumerate() {
        let Unknown::Tension(mi) = eq.unknowns[*c] else {
            continue;
        };
        let mut obj = vec![0.0; cols.len()];
        obj[k] = 1.0;
        let max = match maximize(&a, &rb, &obj) {
            LpResult::Optimal { value, .. } => value * w,
            LpResult::Unbounded => f64::INFINITY,
            LpResult::Infeasible => {
                feasible = false;
                f64::NAN
            }
        };
        obj[k] = -1.0;
        let min = match maximize(&a, &rb, &obj) {
            LpResult::Optimal { value, .. } => (-value * w).max(0.0),
            _ => f64::NAN,
        };
        members.insert(
            model.members[mi].id,
            TensionRange {
                min_lbs: min,
                max_lbs: max,
            },
        );
    }
    Ok(Envelope { members, feasible })
}

/// Which group a named bounding case forces slack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseKind {
    /// Every member with a chain or an adjustable component.
    ChainsSlack,
    /// Every member that reeves through a bearing (basket, choker).
    BasketsSlack,
}

impl CaseKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ChainsSlack => "chains slack",
            Self::BasketsSlack => "baskets slack",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoundingCase {
    pub kind: CaseKind,
    pub slack: Vec<MemberId>,
    /// `None` when the remaining members cannot carry the rig.
    pub solved: Option<SolvedRig>,
}

/// Members in each named group.
pub fn case_members(rig: &Rig, kind: CaseKind) -> Vec<MemberId> {
    rig.members
        .iter()
        .filter(|(_, m)| match kind {
            CaseKind::ChainsSlack => m.segments.iter().any(|s| {
                s.components
                    .iter()
                    .any(|c| c.adjust.is_some() || matches!(c.kind, ComponentKind::Chain { .. }))
            }),
            CaseKind::BasketsSlack => m.path.len() > 2,
        })
        .map(|(id, _)| *id)
        .collect()
}

/// Solve every named case whose group is non-empty.
pub fn named_cases(rig: &Rig, eval: &EvalRig) -> Result<Vec<BoundingCase>, Vec<RigError>> {
    let mut out = Vec::new();
    for kind in [CaseKind::ChainsSlack, CaseKind::BasketsSlack] {
        let slack = case_members(rig, kind);
        if slack.is_empty() {
            continue;
        }
        let solved = if leaves_body_hanging(rig, &slack) {
            None
        } else {
            let s = solve_with_slack(rig, eval, &slack)?;
            let w = s.eval.weights.total_below_root_lbs;
            let ok = s.converged && (s.hook_load_lbs - w).abs() <= 1e-6 * w.max(1.0);
            ok.then_some(s)
        };
        out.push(BoundingCase {
            kind,
            slack,
            solved,
        });
    }
    Ok(out)
}

/// A body with no remaining member attached cannot be carried.
fn leaves_body_hanging(rig: &Rig, slack: &[MemberId]) -> bool {
    for (bid, _) in rig.bodies.iter() {
        let root_body = rig.nodes.get(&rig.root).and_then(|n| n.body);
        if Some(*bid) == root_body {
            continue;
        }
        let touched = rig.members.iter().any(|(mid, m)| {
            !slack.contains(mid)
                && m.path
                    .iter()
                    .any(|n| rig.nodes.get(n).and_then(|nd| nd.body) == Some(*bid))
        });
        if !touched {
            return true;
        }
    }
    false
}
