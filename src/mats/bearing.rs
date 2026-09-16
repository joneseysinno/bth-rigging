//! Duerr soil-bearing effective length (Leff) for crane mats.
//!
//! Based on Eqs. 1–4 / 15 from David Duerr, P.E.,
//! “Effective Bearing Length of Crane Mats” (2010).
//! Full bending/shear MST quadratics are out of scope for this slice.
//! Roadmap: Step 0 foundation.

use crate::domain::SavedMat;

/// Result of a Duerr soil-based effective-length check.
#[derive(Debug, Clone)]
pub struct MatBearingResult {
    /// Total vertical load including mat self-weight (lb).
    pub total_load_lbs: f64,
    /// Required bearing area (ft²) from Eq. 1.
    pub areqd_ft2: f64,
    /// Required effective length (ft) from Eq. 2.
    pub lreqd_ft: f64,
    /// Effective length used for pressure: min(L_mat, Lreqd) (ft).
    pub leff_ft: f64,
    /// Cantilever each side of pad: max(0, (Leff − C) / 2) (ft).
    pub lc_ft: f64,
    /// Pad length along mat (ft), clamped to mat length.
    pub pad_length_ft: f64,
    /// Pad width across mat (ft), clamped to mat width (display).
    pub pad_width_ft: f64,
    /// Mat width B (ft) — effective contact width.
    pub effective_width_ft: f64,
    /// Leff × B (ft²).
    pub area_ft2: f64,
    /// Ground pressure from crane load only: P / (Leff B) (psf).
    pub q_psf: f64,
    /// Total pressure incl. mat weight: (P+W) / (Leff B) (psf).
    pub qt_psf: f64,
    /// Allowable GBP (psf).
    pub allowable_psf: f64,
    /// Manufacturer allowable outrigger load (lb).
    pub manufacturer_allowable_lbs: f64,
    /// qt / qa × 100.
    pub soil_utilization_pct: f64,
    /// P / P_allow_mfr × 100.
    pub mat_utilization_pct: f64,
    pub soil_overloaded: bool,
    pub mat_overloaded: bool,
    /// True when Lreqd exceeds mat length (soil check forced onto short mat).
    pub mat_too_short: bool,
    /// True when pad C ≥ Leff (no valid cantilever).
    pub pad_geometry_warn: bool,
}

impl MatBearingResult {
    /// Alias for diagram: effective length along mat.
    pub fn effective_length_ft(&self) -> f64 {
        self.leff_ft
    }

    pub fn overloaded(&self) -> bool {
        self.soil_overloaded || self.mat_overloaded
    }
}

/// Inputs for Duerr soil-bearing effective length.
#[derive(Debug, Clone, Copy)]
pub struct MatBearingInput {
    pub mat_length_ft: f64,
    pub mat_width_ft: f64,
    pub mat_weight_lbs: f64,
    pub outrigger_load_lbs: f64,
    /// Pad length along the mat (in) → C.
    pub pad_length_in: f64,
    /// Pad width across the mat (in) — display / clamp only.
    pub pad_width_in: f64,
    pub allowable_psf: f64,
    /// Manufacturer-rated allowable outrigger load (lb).
    pub manufacturer_allowable_lbs: f64,
}

impl MatBearingInput {
    pub fn from_mat(
        mat: &SavedMat,
        outrigger_load_lbs: f64,
        pad_length_in: f64,
        pad_width_in: f64,
        allowable_psf: f64,
    ) -> Self {
        Self {
            mat_length_ft: mat.length_ft,
            mat_width_ft: mat.width_ft,
            mat_weight_lbs: mat.weight_lbs.max(0.0),
            outrigger_load_lbs,
            pad_length_in,
            pad_width_in,
            allowable_psf,
            manufacturer_allowable_lbs: mat.manufacturer_allowable_lbs.max(0.0),
        }
    }
}

/// Compute Duerr soil-based Leff and capacity usages.
pub fn calculate_mat_bearing(input: MatBearingInput) -> Option<MatBearingResult> {
    let MatBearingInput {
        mat_length_ft,
        mat_width_ft,
        mat_weight_lbs,
        outrigger_load_lbs,
        pad_length_in,
        pad_width_in,
        allowable_psf,
        manufacturer_allowable_lbs,
    } = input;

    if !(mat_length_ft.is_finite() && mat_width_ft.is_finite()) {
        return None;
    }
    if mat_length_ft <= 0.0 || mat_width_ft <= 0.0 {
        return None;
    }
    if !(outrigger_load_lbs.is_finite() && pad_length_in.is_finite() && pad_width_in.is_finite()) {
        return None;
    }
    if outrigger_load_lbs < 0.0 || pad_length_in <= 0.0 || pad_width_in <= 0.0 {
        return None;
    }
    if !(allowable_psf.is_finite() && allowable_psf > 0.0) {
        return None;
    }
    if !(manufacturer_allowable_lbs.is_finite() && manufacturer_allowable_lbs > 0.0) {
        return None;
    }
    if !mat_weight_lbs.is_finite() || mat_weight_lbs < 0.0 {
        return None;
    }

    let b = mat_width_ft;
    let l_mat = mat_length_ft;
    let w = mat_weight_lbs;
    let p = outrigger_load_lbs;

    let pad_l = (pad_length_in / 12.0).min(l_mat);
    let pad_w = (pad_width_in / 12.0).min(b);
    let c = pad_l;

    // Eqs. 1–2
    let areqd = (p + w) / allowable_psf;
    let lreqd = areqd / b;
    let mat_too_short = lreqd > l_mat + 1e-9;
    let leff = lreqd.min(l_mat);
    if leff <= 0.0 || !leff.is_finite() {
        return None;
    }

    let lc = ((leff - c) / 2.0).max(0.0);
    let pad_geometry_warn = c + 1e-9 >= leff;

    let area = leff * b;
    let q = p / area;
    let qt = (p + w) / area;

    let soil_util = qt / allowable_psf * 100.0;
    let mat_util = p / manufacturer_allowable_lbs * 100.0;

    Some(MatBearingResult {
        total_load_lbs: p + w,
        areqd_ft2: areqd,
        lreqd_ft: lreqd,
        leff_ft: leff,
        lc_ft: lc,
        pad_length_ft: pad_l,
        pad_width_ft: pad_w,
        effective_width_ft: b,
        area_ft2: area,
        q_psf: q,
        qt_psf: qt,
        allowable_psf,
        manufacturer_allowable_lbs,
        soil_utilization_pct: soil_util,
        mat_utilization_pct: mat_util,
        soil_overloaded: qt > allowable_psf,
        mat_overloaded: p > manufacturer_allowable_lbs,
        mat_too_short,
        pad_geometry_warn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Duerr Table 1 soil-bearing method values.
    fn table1() -> MatBearingInput {
        MatBearingInput {
            mat_length_ft: 20.0,
            mat_width_ft: 4.0,
            mat_weight_lbs: 4_000.0,
            outrigger_load_lbs: 100_000.0,
            pad_length_in: 24.0, // C = 2 ft
            pad_width_in: 24.0,
            allowable_psf: 3_000.0,
            manufacturer_allowable_lbs: 150_000.0,
        }
    }

    #[test]
    fn duerr_table1_soil_method() {
        // Areqd = 104000/3000 = 34.666… ft²
        // Lreqd = 34.666…/4 = 8.666… ft
        // Lc = (8.666… − 2)/2 = 3.333… ft
        let r = calculate_mat_bearing(table1()).expect("ok");
        assert!((r.areqd_ft2 - 34.666_666_7).abs() < 1e-4);
        assert!((r.lreqd_ft - 8.666_666_7).abs() < 1e-4);
        assert!((r.leff_ft - 8.666_666_7).abs() < 1e-4);
        assert!((r.lc_ft - 3.333_333_3).abs() < 1e-4);
        assert!(!r.mat_too_short);
        // q = 100000/(8.666…*4) ≈ 2884.6 psf
        assert!((r.q_psf - 100_000.0 / r.area_ft2).abs() < 1e-6);
        assert!((r.qt_psf - 104_000.0 / r.area_ft2).abs() < 1e-6);
        // qt ≈ 3000 when Leff = Lreqd
        assert!((r.qt_psf - 3_000.0).abs() < 0.01);
        assert!(!r.soil_overloaded);
        assert!((r.mat_utilization_pct - 100_000.0 / 150_000.0 * 100.0).abs() < 1e-6);
        assert!(!r.mat_overloaded);
    }

    #[test]
    fn clamps_leff_to_mat_length() {
        let mut i = table1();
        i.mat_length_ft = 6.0; // shorter than Lreqd 8.67
        let r = calculate_mat_bearing(i).expect("ok");
        assert!((r.leff_ft - 6.0).abs() < 1e-9);
        assert!(r.mat_too_short);
        assert!(r.soil_overloaded);
        assert!(r.qt_psf > 3_000.0);
    }

    #[test]
    fn mat_overloaded_when_p_exceeds_allowable() {
        let mut i = table1();
        i.manufacturer_allowable_lbs = 80_000.0;
        let r = calculate_mat_bearing(i).expect("ok");
        assert!(r.mat_overloaded);
        assert!(r.mat_utilization_pct > 100.0);
    }

    #[test]
    fn pad_geometry_warn_when_c_ge_leff() {
        let mut i = table1();
        i.pad_length_in = 120.0; // 10 ft > Leff
        let r = calculate_mat_bearing(i).expect("ok");
        assert!(r.pad_geometry_warn);
        assert!((r.lc_ft - 0.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_invalid_inputs() {
        let mut i = table1();
        i.mat_length_ft = 0.0;
        assert!(calculate_mat_bearing(i).is_none());

        let mut i = table1();
        i.manufacturer_allowable_lbs = 0.0;
        assert!(calculate_mat_bearing(i).is_none());

        let mut i = table1();
        i.allowable_psf = 0.0;
        assert!(calculate_mat_bearing(i).is_none());
    }
}
