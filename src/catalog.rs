//! WSTDA-RS-1 polyester roundsling rated capacities (lbs).
//!
//! Values are representative Table 2-1 ratings. Manufacturer tags may differ —
//! always verify the identification tag for the lift.

use serde::Serialize;

use crate::models::Hitch;

/// One catalog entry for a polyester roundsling size.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoundSlingRating {
    pub size: u8,
    pub color: &'static str,
    pub vertical_lbs: u32,
    pub choker_lbs: u32,
    pub basket_vertical_lbs: u32,
    /// Basket hitch WLL at 45° from horizontal (reference).
    pub basket_45_lbs: u32,
    /// Approximate self-weight per foot of sling length (lb/ft).
    /// Representative endless polyester roundsling values (Lift-All Tuflex catalog);
    /// verify against the sling manufacturer.
    pub weight_lbs_per_ft: f64,
}

/// WSTDA-RS-1 Table 2-1 polyester roundsling ratings (lbs).
pub const POLYESTER_ROUNDSLINGS: &[RoundSlingRating] = &[
    RoundSlingRating {
        size: 1,
        color: "Purple",
        vertical_lbs: 2_600,
        choker_lbs: 2_100,
        basket_vertical_lbs: 5_200,
        basket_45_lbs: 3_700,
        weight_lbs_per_ft: 0.44,
    },
    RoundSlingRating {
        size: 2,
        color: "Green",
        vertical_lbs: 5_300,
        choker_lbs: 4_200,
        basket_vertical_lbs: 10_600,
        basket_45_lbs: 7_500,
        weight_lbs_per_ft: 0.63,
    },
    RoundSlingRating {
        size: 3,
        color: "Yellow",
        vertical_lbs: 8_400,
        choker_lbs: 6_700,
        basket_vertical_lbs: 16_800,
        basket_45_lbs: 11_900,
        weight_lbs_per_ft: 0.75,
    },
    RoundSlingRating {
        size: 4,
        color: "Tan",
        vertical_lbs: 10_600,
        choker_lbs: 8_500,
        basket_vertical_lbs: 21_200,
        basket_45_lbs: 15_000,
        weight_lbs_per_ft: 0.88,
    },
    RoundSlingRating {
        size: 5,
        color: "Red",
        vertical_lbs: 13_200,
        choker_lbs: 10_600,
        basket_vertical_lbs: 26_400,
        basket_45_lbs: 18_700,
        weight_lbs_per_ft: 1.00,
    },
    RoundSlingRating {
        size: 6,
        color: "White",
        vertical_lbs: 16_800,
        choker_lbs: 13_400,
        basket_vertical_lbs: 33_600,
        basket_45_lbs: 23_800,
        weight_lbs_per_ft: 1.13,
    },
    RoundSlingRating {
        size: 7,
        color: "Blue",
        vertical_lbs: 21_200,
        choker_lbs: 17_000,
        basket_vertical_lbs: 42_400,
        basket_45_lbs: 30_000,
        weight_lbs_per_ft: 1.19,
    },
    RoundSlingRating {
        size: 8,
        color: "Orange",
        vertical_lbs: 25_000,
        choker_lbs: 20_000,
        basket_vertical_lbs: 50_000,
        basket_45_lbs: 35_400,
        weight_lbs_per_ft: 1.25,
    },
    RoundSlingRating {
        size: 9,
        color: "Orange",
        vertical_lbs: 31_000,
        choker_lbs: 24_800,
        basket_vertical_lbs: 62_000,
        basket_45_lbs: 43_800,
        weight_lbs_per_ft: 1.50,
    },
    RoundSlingRating {
        size: 10,
        color: "Orange",
        vertical_lbs: 40_000,
        choker_lbs: 32_000,
        basket_vertical_lbs: 80_000,
        basket_45_lbs: 56_600,
        weight_lbs_per_ft: 1.62,
    },
    RoundSlingRating {
        size: 11,
        color: "Orange",
        vertical_lbs: 53_000,
        choker_lbs: 42_400,
        basket_vertical_lbs: 106_000,
        basket_45_lbs: 74_900,
        weight_lbs_per_ft: 2.00,
    },
    RoundSlingRating {
        size: 12,
        color: "Orange",
        vertical_lbs: 66_000,
        choker_lbs: 52_800,
        basket_vertical_lbs: 132_000,
        basket_45_lbs: 93_000,
        weight_lbs_per_ft: 2.13,
    },
    RoundSlingRating {
        size: 13,
        color: "Orange",
        vertical_lbs: 90_000,
        choker_lbs: 72_000,
        basket_vertical_lbs: 180_000,
        basket_45_lbs: 127_300,
        weight_lbs_per_ft: 2.50,
    },
];

impl RoundSlingRating {
    /// WLL for the selected hitch, compared against per-leg tension.
    ///
    /// Basket uses vertical WLL per leg (basket rating is 2× when both legs are vertical).
    pub fn hitch_wll_lbs(&self, hitch: Hitch) -> u32 {
        match hitch {
            Hitch::Vertical => self.vertical_lbs,
            Hitch::Choker => self.choker_lbs,
            Hitch::Basket => self.vertical_lbs,
        }
    }

    pub fn label(&self) -> String {
        format!(
            "RS-{} {} ({} lb vert)",
            self.size, self.color, self.vertical_lbs
        )
    }

    /// Self-weight of one sling of the given length (lb).
    pub fn sling_weight_lbs(&self, length_ft: f64) -> f64 {
        if length_ft.is_finite() && length_ft > 0.0 {
            self.weight_lbs_per_ft * length_ft
        } else {
            0.0
        }
    }
}

pub fn find_by_size(size: u8) -> Option<&'static RoundSlingRating> {
    POLYESTER_ROUNDSLINGS.iter().find(|r| r.size == size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_thirteen_sizes() {
        assert_eq!(POLYESTER_ROUNDSLINGS.len(), 13);
        assert_eq!(POLYESTER_ROUNDSLINGS[0].size, 1);
        assert_eq!(POLYESTER_ROUNDSLINGS[0].color, "Purple");
        assert_eq!(POLYESTER_ROUNDSLINGS[0].vertical_lbs, 2_600);
        assert_eq!(POLYESTER_ROUNDSLINGS[12].size, 13);
        assert_eq!(POLYESTER_ROUNDSLINGS[12].vertical_lbs, 90_000);
        assert_eq!(POLYESTER_ROUNDSLINGS[12].choker_lbs, 72_000);
        assert_eq!(POLYESTER_ROUNDSLINGS[12].basket_vertical_lbs, 180_000);
    }
}
