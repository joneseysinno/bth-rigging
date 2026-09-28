//! In-plane / out-of-plane lug checks.
//!
//! Step: 3
//! Theory: roadmap Step 3 — lug checks. A lug here carries only a rated WLL
//! (`LugRating`); plate geometry for an ASME BTH-1 pin-connection check is not
//! in the model yet. So: resultant vs WLL, and a WARN with the out-of-plane
//! angle whenever the load leaves the plate plane by more than
//! [`OUT_OF_PLANE_TOL_DEG`] — the lug's rating is for in-plane load.
//! Inputs: lug geometry and load direction.
//! Outputs: utilization / overload.
//! Must not depend on: UI, dioxus.

use super::{Check, CheckKind};

/// Out-of-plane angle that triggers a WARN, deg.
pub const OUT_OF_PLANE_TOL_DEG: f64 = 5.0;

/// Lug check from the resultant force (lb) and the out-of-plane angle (deg).
pub fn lug(label: &str, wll_lbs: u32, resultant_lbs: f64, out_of_plane_deg: f64) -> Check {
    let mut c = Check::new(CheckKind::Lug, label, resultant_lbs, f64::from(wll_lbs));
    if out_of_plane_deg > OUT_OF_PLANE_TOL_DEG {
        c = c.warn(format!(
            "{out_of_plane_deg:.1}° out of plane — verify the lug for side load"
        ));
    }
    c
}

/// Angle between a force and a plate (deg): 0 = in the plate's plane.
pub fn out_of_plane_deg(force: [f64; 3], plate_normal: [f64; 3]) -> f64 {
    let nf = (force[0] * force[0] + force[1] * force[1] + force[2] * force[2]).sqrt();
    let nn = (plate_normal[0] * plate_normal[0]
        + plate_normal[1] * plate_normal[1]
        + plate_normal[2] * plate_normal[2])
        .sqrt();
    if nf < 1e-12 || nn < 1e-12 {
        return 0.0;
    }
    let s = (force[0] * plate_normal[0] + force[1] * plate_normal[1] + force[2] * plate_normal[2])
        / (nf * nn);
    s.abs().clamp(0.0, 1.0).asin().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Status;

    #[test]
    fn in_plane_and_side_load() {
        assert!(out_of_plane_deg([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]).abs() < 1e-12);
        assert!((out_of_plane_deg([0.0, 1.0, 1.0], [0.0, 1.0, 0.0]) - 45.0).abs() < 1e-9);
        assert_eq!(lug("P1", 10_000, 9_000.0, 2.0).status, Status::Ok);
        assert_eq!(lug("P1", 10_000, 9_000.0, 20.0).status, Status::Warn);
        assert_eq!(lug("P1", 10_000, 11_000.0, 20.0).status, Status::Over);
    }
}
