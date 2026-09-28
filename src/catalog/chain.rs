//! Alloy lifting chain working load limits (lb) by grade and size.
//!
//! Step: 3
//! Theory: representative NACM *Welded Steel Chain Specifications* values for
//! Grade 80 and Grade 100 alloy chain, straight (vertical) pull. Manufacturer
//! tags govern — always verify the tag for the lift. Grade 70 transport
//! chain is not for overhead lifting and is deliberately absent.
//! Must not depend on: layers, store, UI, dioxus.

use serde::Serialize;

/// One chain size's rating.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ChainRating {
    pub grade: u8,
    /// Nominal size, in.
    pub size_in: f64,
    pub label: &'static str,
    pub wll_lbs: u32,
}

const fn c(grade: u8, size_in: f64, label: &'static str, wll_lbs: u32) -> ChainRating {
    ChainRating {
        grade,
        size_in,
        label,
        wll_lbs,
    }
}

/// Grade 80 and Grade 100 alloy chain, lb.
pub const CHAINS: &[ChainRating] = &[
    c(80, 7.0 / 32.0, "7/32", 2_100),
    c(80, 9.0 / 32.0, "9/32", 3_500),
    c(80, 5.0 / 16.0, "5/16", 4_500),
    c(80, 3.0 / 8.0, "3/8", 7_100),
    c(80, 1.0 / 2.0, "1/2", 12_000),
    c(80, 5.0 / 8.0, "5/8", 18_100),
    c(80, 3.0 / 4.0, "3/4", 28_300),
    c(80, 7.0 / 8.0, "7/8", 34_200),
    c(80, 1.0, "1", 47_700),
    c(80, 1.25, "1-1/4", 72_300),
    c(100, 7.0 / 32.0, "7/32", 2_700),
    c(100, 9.0 / 32.0, "9/32", 4_300),
    c(100, 5.0 / 16.0, "5/16", 5_700),
    c(100, 3.0 / 8.0, "3/8", 8_800),
    c(100, 1.0 / 2.0, "1/2", 15_000),
    c(100, 5.0 / 8.0, "5/8", 22_600),
    c(100, 3.0 / 4.0, "3/4", 35_300),
    c(100, 7.0 / 8.0, "7/8", 42_700),
];

/// Rating for `grade` and nominal `size_in` (matched to 1/64 in). The
/// shorthand grades `8` and `10` (as stamped "G8", "G10") mean 80 and 100.
pub fn find_chain(grade: u8, size_in: f64) -> Option<&'static ChainRating> {
    let grade = match grade {
        8 => 80,
        10 => 100,
        g => g,
    };
    CHAINS
        .iter()
        .find(|r| r.grade == grade && (r.size_in - size_in).abs() < 1.0 / 128.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_by_grade_and_size() {
        assert_eq!(find_chain(80, 0.5).unwrap().wll_lbs, 12_000);
        assert_eq!(find_chain(100, 0.375).unwrap().wll_lbs, 8_800);
        assert!(find_chain(70, 0.5).is_none());
        assert_eq!(find_chain(8, 0.5).unwrap().wll_lbs, 12_000);
        assert!(find_chain(80, 0.55).is_none());
    }
}
