//! WSTDA §4.7 connection / bearing-width hardware table.
//!
//! Step: 1 / 3
//! Theory: Duplo10 Lift Readback — bearing width at connections; roadmap Step 1/3.
//! Inputs: component size, hitch / connection type.
//! Outputs: allowable bearing width and related catalog lookups.
//! Must not depend on: layers, store, UI, dioxus.

/// Approximate roundsling body diameter (in) by WSTDA size number.
/// Representative endless-polyester values; verify against the tag.
pub fn roundsling_body_dia_in(size: u8) -> Option<f64> {
    Some(match size {
        1 => 0.50,
        2 => 0.62,
        3 => 0.75,
        4 => 0.88,
        5 => 1.00,
        6 => 1.12,
        7 => 1.25,
        8 => 1.38,
        9 => 1.50,
        10 => 1.75,
        11 => 2.00,
        12 => 2.25,
        13 => 2.50,
        _ => return None,
    })
}

/// Minimum connection hardware diameter (in) from WSTDA-RS-1 §4.7 style guidance:
/// hardware should be at least as large as the sling body.
pub fn min_hardware_dia_in(size: u8) -> Option<f64> {
    roundsling_body_dia_in(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_rs_sizes() {
        assert_eq!(roundsling_body_dia_in(1), Some(0.50));
        assert_eq!(roundsling_body_dia_in(13), Some(2.50));
        assert_eq!(min_hardware_dia_in(5), Some(1.00));
        assert_eq!(roundsling_body_dia_in(99), None);
    }
}
