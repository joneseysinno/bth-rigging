//! Screw-pin anchor shackle ratings and self-weights.
//!
//! Representative Crosby G-209 style values for calculation aids.
//! Always verify manufacturer identification tags.
//! Roadmap: Step 0 foundation.

use serde::Serialize;

/// Screw-pin anchor shackle (representative Crosby G-209 style ratings).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShackleRating {
    /// Nominal size key, e.g. `"3/8"`.
    pub size_in: &'static str,
    /// Working load limit in short tons.
    pub wll_tons: f64,
    /// Working load limit in pounds (ton × 2000).
    pub wll_lbs: u32,
    /// Approximate weight each (lb), Crosby G-209 catalog.
    pub weight_lbs: f64,
}

impl ShackleRating {
    pub fn label(&self) -> String {
        format!("{}″ · {} lb WLL", self.size_in, self.wll_lbs)
    }
}

/// Representative screw-pin anchor shackles (WLL ton × 2000 → lb).
pub const SHACKLES: &[ShackleRating] = &[
    ShackleRating {
        size_in: "3/8",
        wll_tons: 1.0,
        wll_lbs: 2_000,
        weight_lbs: 0.31,
    },
    ShackleRating {
        size_in: "7/16",
        wll_tons: 1.5,
        wll_lbs: 3_000,
        weight_lbs: 0.38,
    },
    ShackleRating {
        size_in: "1/2",
        wll_tons: 2.0,
        wll_lbs: 4_000,
        weight_lbs: 0.72,
    },
    ShackleRating {
        size_in: "5/8",
        wll_tons: 3.25,
        wll_lbs: 6_500,
        weight_lbs: 1.37,
    },
    ShackleRating {
        size_in: "3/4",
        wll_tons: 4.75,
        wll_lbs: 9_500,
        weight_lbs: 2.35,
    },
    ShackleRating {
        size_in: "7/8",
        wll_tons: 6.5,
        wll_lbs: 13_000,
        weight_lbs: 3.62,
    },
    ShackleRating {
        size_in: "1",
        wll_tons: 8.5,
        wll_lbs: 17_000,
        weight_lbs: 5.03,
    },
    ShackleRating {
        size_in: "1-1/8",
        wll_tons: 9.5,
        wll_lbs: 19_000,
        weight_lbs: 7.41,
    },
    ShackleRating {
        size_in: "1-1/4",
        wll_tons: 12.0,
        wll_lbs: 24_000,
        weight_lbs: 9.50,
    },
    ShackleRating {
        size_in: "1-3/8",
        wll_tons: 13.5,
        wll_lbs: 27_000,
        weight_lbs: 13.53,
    },
    ShackleRating {
        size_in: "1-1/2",
        wll_tons: 17.0,
        wll_lbs: 34_000,
        weight_lbs: 17.20,
    },
    ShackleRating {
        size_in: "1-3/4",
        wll_tons: 25.0,
        wll_lbs: 50_000,
        weight_lbs: 27.78,
    },
    ShackleRating {
        size_in: "2",
        wll_tons: 35.0,
        wll_lbs: 70_000,
        weight_lbs: 45.00,
    },
    ShackleRating {
        size_in: "2-1/2",
        wll_tons: 55.0,
        wll_lbs: 110_000,
        weight_lbs: 85.75,
    },
];

pub fn find_shackle(size_in: &str) -> Option<&'static ShackleRating> {
    SHACKLES.iter().find(|s| s.size_in == size_in)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shackles_cover_common_sizes() {
        assert_eq!(find_shackle("3/4").unwrap().wll_lbs, 9_500);
        assert_eq!(find_shackle("2-1/2").unwrap().wll_lbs, 110_000);
        assert!((find_shackle("1").unwrap().weight_lbs - 5.03).abs() < 1e-9);
        assert!(SHACKLES.iter().all(|s| s.weight_lbs > 0.0));
    }
}
