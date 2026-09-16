//! Hardware catalogs: screw-pin shackles (ratings and self-weights).
//!
//! Values are representative industry ratings for calculation aids.
//! Always verify manufacturer identification tags.
//! Spreader bars are user-entered and stored in InfiniteDb.

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

/// Even-split child sling counts onto parent endpoints.
/// Remainder goes to the outer ends first.
///
/// Example: 4 slings onto 2 ends → `[2, 2]`; 5 onto 2 → `[3, 2]`.
pub fn split_slings(parent_ends: u32, child_count: u32) -> Vec<u32> {
    let ends = parent_ends.max(1) as usize;
    let child = child_count.max(1);
    let base = child / ends as u32;
    let rem = child % ends as u32;
    let mut out = vec![base; ends];
    // Distribute remainder to outer ends (first, then last, then inward).
    let mut left = 0usize;
    let mut right = ends.saturating_sub(1);
    let mut r = rem;
    let mut from_left = true;
    while r > 0 && left <= right {
        if from_left {
            out[left] += 1;
            if left == right {
                break;
            }
            left += 1;
        } else {
            out[right] += 1;
            if right == 0 {
                break;
            }
            right -= 1;
        }
        from_left = !from_left;
        r -= 1;
    }
    out
}

/// Approximate CSS stroke color for a WSTDA cover color name.
pub fn sling_stroke_color(wstda_color: &str) -> &'static str {
    match wstda_color {
        "Purple" => "#7c3aed",
        "Green" => "#16a34a",
        "Yellow" => "#ca8a04",
        "Tan" => "#a16207",
        "Red" => "#dc2626",
        "White" => "#94a3b8",
        "Blue" => "#2563eb",
        "Orange" => "#ea580c",
        _ => "#334155",
    }
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

    #[test]
    fn split_four_onto_two() {
        assert_eq!(split_slings(2, 4), vec![2, 2]);
    }

    #[test]
    fn split_five_onto_two_outer_bias() {
        assert_eq!(split_slings(2, 5), vec![3, 2]);
    }

    #[test]
    fn split_three_onto_two() {
        assert_eq!(split_slings(2, 3), vec![2, 1]);
    }
}
