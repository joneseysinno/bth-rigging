//! Roundsling by hitch/angle; basket legs.
//!
//! Step: 3
//! Theory: WSTDA-RS-1 / roadmap Step 3. Demand is the **leg** tension from
//! the solve, so the sling angle is already in it — no angle factor is
//! applied twice. Capacity per leg: vertical WLL for vertical and basket legs
//! (a basket's rating is two vertical legs), choker WLL for a choker. Legs
//! flatter than 30° from horizontal get a WARN, matching the layer engine's
//! ANGLE stamp. A basket over a bow checks the bow diameter against the
//! §4.7 minimum hardware size.
//! Inputs: tension, hitch, angle, size.
//! Outputs: utilization / overload.
//! Must not depend on: UI, dioxus.

use super::{Check, CheckKind};
use crate::catalog::connection_hardware::min_hardware_dia_in;
use crate::catalog::find_by_size;
use crate::domain::Hitch;

/// Minimum leg angle from horizontal before WARN, deg.
pub const MIN_ANGLE_DEG: f64 = 30.0;

/// Roundsling leg check. `bow_dia_in` is the bearing diameter when the leg
/// reeves over a bow (basket / choke point). `None` if the size is unknown.
pub fn roundsling(
    size: u8,
    hitch: Hitch,
    tension_lbs: f64,
    angle_deg: f64,
    bow_dia_in: Option<f64>,
) -> Option<Check> {
    let r = find_by_size(size)?;
    let cap = f64::from(r.hitch_wll_lbs(hitch));
    let mut c = Check::new(
        CheckKind::Sling,
        format!("RS-{size} {}", hitch.label()),
        tension_lbs,
        cap,
    );
    if angle_deg < MIN_ANGLE_DEG {
        c = c.warn(format!(
            "leg {angle_deg:.1}° from horizontal (< {MIN_ANGLE_DEG}°)"
        ));
    }
    if let (Some(d), Some(min)) = (bow_dia_in, min_hardware_dia_in(size))
        && d + 1e-9 < min
    {
        c = c.warn(format!(
            "bearing Ø{d:.2} in below the §4.7 minimum Ø{min:.2} in"
        ));
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Status;

    #[test]
    fn vertical_and_choker_capacities() {
        let c = roundsling(5, Hitch::Vertical, 13_000.0, 60.0, None).unwrap();
        assert_eq!(c.status, Status::Ok);
        assert_eq!(c.capacity_lbs, 13_200.0);
        let c = roundsling(5, Hitch::Choker, 11_000.0, 60.0, None).unwrap();
        assert_eq!(c.status, Status::Over);
        let c = roundsling(5, Hitch::Basket, 13_000.0, 60.0, None).unwrap();
        assert_eq!(c.capacity_lbs, 13_200.0, "basket is checked per leg");
    }

    #[test]
    fn flat_leg_and_small_bow_warn() {
        let c = roundsling(5, Hitch::Vertical, 1_000.0, 25.0, None).unwrap();
        assert_eq!(c.status, Status::Warn);
        let c = roundsling(13, Hitch::Basket, 1_000.0, 60.0, Some(1.0)).unwrap();
        assert_eq!(c.status, Status::Warn);
        assert!(roundsling(99, Hitch::Vertical, 1.0, 90.0, None).is_none());
    }
}
