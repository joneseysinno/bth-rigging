//! Grade/size WLL; grab-hook reduction.
//!
//! Step: 3
//! Theory: roadmap Step 3 — chain checks. Capacity is the catalog WLL
//! (`catalog::chain`) times a hook efficiency supplied by the caller
//! (1.0 for a cradle grab hook or a connecting link; use the hook
//! manufacturer's reduction for a standard grab hook — it is not guessed here).
//! Inputs: tension and chain catalog.
//! Outputs: utilization / overload.
//! Must not depend on: UI, dioxus.

use super::{Check, CheckKind};
use crate::catalog::find_chain;

/// Chain leg check. `None` if the grade/size is not in the catalog.
pub fn chain(grade: u8, size_in: f64, tension_lbs: f64, hook_efficiency: f64) -> Option<Check> {
    let r = find_chain(grade, size_in)?;
    let eff = hook_efficiency.clamp(0.0, 1.0);
    let mut c = Check::new(
        CheckKind::Chain,
        format!("chain G{grade} {}″", r.label),
        tension_lbs,
        f64::from(r.wll_lbs) * eff,
    );
    if eff < 1.0 {
        c = c.note(format!("hook efficiency {:.0}%", eff * 100.0));
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Status;

    #[test]
    fn grade_80_half_inch() {
        let c = chain(80, 0.5, 11_000.0, 1.0).unwrap();
        assert_eq!(c.capacity_lbs, 12_000.0);
        assert_eq!(c.status, Status::Ok);
        let c = chain(80, 0.5, 11_000.0, 0.8).unwrap();
        assert_eq!(c.status, Status::Over);
    }
}
