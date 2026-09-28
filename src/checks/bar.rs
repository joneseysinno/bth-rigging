//! Spreader compression P = V / tan θ.
//!
//! Step: 3
//! Theory: roadmap Step 3 — bar checks. A spreader bar's WLL is the total
//! load it may carry hanging below it. Demand is the vertical load delivered
//! through the bar's lower attachments (not the bar's own weight). The axial
//! compression comes straight from the solve (`P`, reported), and for a
//! symmetric two-leg top rig equals `V / tan θ` per end — kept as a
//! cross-check function.
//! Inputs: end reactions and angle.
//! Outputs: compression utilization.
//! Must not depend on: UI, dioxus.

use super::{Check, CheckKind};

/// Spreader / lifting-beam check.
pub fn bar(label: &str, wll_lbs: u32, hanging_lbs: f64, compression_lbs: f64) -> Check {
    Check::new(CheckKind::Bar, label, hanging_lbs, f64::from(wll_lbs)).note(format!(
        "axial {} {:.0} lb",
        if compression_lbs >= 0.0 {
            "compression"
        } else {
            "tension"
        },
        compression_lbs.abs()
    ))
}

/// Compression at one end of a bar held by a symmetric pair of top legs:
/// `P = V / tan θ`, θ from horizontal.
pub fn compression_from_angle(end_vertical_lbs: f64, angle_deg: f64) -> f64 {
    let t = angle_deg.to_radians().tan();
    if t.abs() < 1e-12 {
        f64::INFINITY
    } else {
        end_vertical_lbs / t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Status;

    #[test]
    fn bar_check_and_cross_check() {
        let c = bar("SB-20", 20_000, 18_000.0, 5_000.0);
        assert_eq!(c.status, Status::Ok);
        assert!((compression_from_angle(1_000.0, 45.0) - 1_000.0).abs() < 1e-9);
    }
}
