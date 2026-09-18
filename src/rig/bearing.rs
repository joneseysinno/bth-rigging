//! Capstan band, D/d, edge radius, softener.
//!
//! Step: 1 / 3
//! Theory: roadmap Step 1/3 — soft-goods bearing geometry.
//! Inputs: contact geometry and softener.
//! Outputs: bearing factors for checks.
//! Must not depend on: UI, dioxus.

use crate::catalog::connection_hardware::{min_hardware_dia_in, roundsling_body_dia_in};

/// D/d = bearing diameter / sling body diameter. `None` if either is non-positive.
pub fn d_over_d(bearing_dia_in: f64, sling_body_dia_in: f64) -> Option<f64> {
    if bearing_dia_in > 0.0 && sling_body_dia_in > 0.0 {
        Some(bearing_dia_in / sling_body_dia_in)
    } else {
        None
    }
}

/// Catalog D/d for a roundsling size over hardware of `hardware_dia_in`.
pub fn roundsling_d_over_d(size: u8, hardware_dia_in: f64) -> Option<f64> {
    d_over_d(hardware_dia_in, roundsling_body_dia_in(size)?)
}

/// True when the hardware meets the WSTDA §4.7 minimum diameter for the size.
pub fn hardware_width_ok(size: u8, hardware_dia_in: f64) -> bool {
    min_hardware_dia_in(size)
        .map(|min| hardware_dia_in + 1e-9 >= min)
        .unwrap_or(false)
}

/// Capstan / belt-friction factor `e^{μθ}` for wrap angle `theta_rad`.
/// `mu = 0` → 1 (equal tension both sides). Forces are not applied in Step 1.
pub fn capstan_factor(mu: f64, theta_rad: f64) -> f64 {
    if !mu.is_finite() || !theta_rad.is_finite() {
        return f64::NAN;
    }
    (mu * theta_rad).exp()
}

/// Suggested wrap angle (rad) for a strap through a bow: 180° default.
pub fn default_bow_wrap_rad() -> f64 {
    std::f64::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capstan_unity_when_mu_zero() {
        assert!((capstan_factor(0.0, default_bow_wrap_rad()) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn d_over_d_and_width_table() {
        let d = roundsling_d_over_d(5, 2.0).unwrap();
        assert!(d > 1.0);
        assert!(hardware_width_ok(5, 2.0));
        assert!(!hardware_width_ok(13, 0.25));
    }
}
