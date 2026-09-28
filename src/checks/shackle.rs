//! WLL and side-load reduction.
//!
//! Step: 3
//! Theory: roadmap Step 3 — shackle checks. Screw-pin anchor shackle WLL
//! (`catalog::shackle`) reduced for side load per the manufacturer's chart:
//! in-line 100 %, 45° side load 70 %, 90° side load 50 %. Angles between chart
//! points take the next larger angle's reduction (conservative); an angle
//! within [`INLINE_TOL_DEG`] of in-line counts as in-line.
//! Inputs: reaction and shackle size.
//! Outputs: utilization / overload.
//! Must not depend on: UI, dioxus.

use super::{Check, CheckKind};
use crate::catalog::find_shackle;

/// Side-load angle still treated as in-line, deg.
pub const INLINE_TOL_DEG: f64 = 5.0;

/// Fraction of WLL available at a side-load angle (deg from the shackle's plane).
pub fn side_load_factor(angle_deg: f64) -> f64 {
    let a = angle_deg.abs();
    if a <= INLINE_TOL_DEG {
        1.0
    } else if a <= 45.0 {
        0.70
    } else {
        0.50
    }
}

/// Shackle check. `None` if the size is not in the catalog.
pub fn shackle(size: &str, reaction_lbs: f64, side_angle_deg: f64) -> Option<Check> {
    let r = find_shackle(size)?;
    let f = side_load_factor(side_angle_deg);
    let mut c = Check::new(
        CheckKind::Shackle,
        format!("shackle {size}″"),
        reaction_lbs,
        f64::from(r.wll_lbs) * f,
    );
    if f < 1.0 {
        c = c.warn(format!(
            "side load {side_angle_deg:.1}° → {:.0}% WLL",
            f * 100.0
        ));
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Status;

    #[test]
    fn side_load_steps() {
        assert_eq!(side_load_factor(0.0), 1.0);
        assert_eq!(side_load_factor(30.0), 0.70);
        assert_eq!(side_load_factor(60.0), 0.50);
        let c = shackle("1-1/4", 20_000.0, 0.0).unwrap();
        assert_eq!(c.status, Status::Ok);
        let c = shackle("1-1/4", 20_000.0, 20.0).unwrap();
        assert_eq!(c.status, Status::Over, "24 000 × 0.7 = 16 800");
    }
}
