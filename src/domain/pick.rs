//! Pick, sling layer, and hitch types.
//!
//! Must not depend on catalog, store, or calculation modules.
//! Roadmap: Step 0 foundation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Hitch configuration for a round sling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Hitch {
    #[default]
    Vertical,
    Choker,
    Basket,
}

impl Hitch {
    pub fn label(self) -> &'static str {
        match self {
            Hitch::Vertical => "Vertical",
            Hitch::Choker => "Choker",
            Hitch::Basket => "Basket",
        }
    }

    pub fn all() -> [Hitch; 3] {
        [Hitch::Vertical, Hitch::Choker, Hitch::Basket]
    }
}

/// A saved pick (lift) with a named load inside a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pick {
    pub id: Uuid,
    /// Owning project; `Uuid::nil()` means unassigned (migrated on open).
    #[serde(default = "Uuid::nil")]
    pub project_id: Uuid,
    pub name: String,
    /// Payload weight (lb) entered on the final layer.
    pub weight_lbs: f64,
}

impl Pick {
    pub fn new(project_id: Uuid, name: impl Into<String>, weight_lbs: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            weight_lbs,
        }
    }
}

/// One layer/level of slings sharing angle and hitch on a pick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlingLayer {
    pub pick_id: Uuid,
    pub layer_index: u32,
    /// WSTDA-RS-1 roundsling size number (1–13).
    pub size: u8,
    pub hitch: Hitch,
    /// Manual sling angle in degrees from horizontal (90 = vertical).
    /// Used only when the angle cannot be calculated from pick-point geometry;
    /// on save it is updated to the calculated angle when one is available.
    pub angle_deg: f64,
    /// Number of slings on this layer.
    pub sling_count: u32,
    /// Sling length (ft) used for self-weight. `0` on older saves = sling weight excluded.
    #[serde(default)]
    pub sling_length_ft: f64,
    /// Apex (top) shackle catalog key, e.g. `"3/4"`.
    #[serde(default)]
    pub apex_shackle: Option<String>,
    /// Per-leg / load-end shackle catalog key.
    #[serde(default)]
    pub leg_shackle: Option<String>,
    /// Reference to a user-saved spreader bar.
    #[serde(default)]
    pub spreader_id: Option<Uuid>,
    /// Legacy unnamed spreader WLL (lb) when no `spreader_id`.
    #[serde(default)]
    pub spreader_wll_lbs: Option<u32>,
    /// Adjacent spacing (ft) between this layer's pick points along the length,
    /// i.e. where the bottoms of these slings attach. Ignored when a spreader with a
    /// span is on this layer (the bar lugs set the spacing).
    #[serde(default)]
    pub pick_spacing_ft: Option<f64>,
    /// Optional spacing (ft) across the width for rectangular patterns
    /// (pick points in two rows at ±width/2).
    #[serde(default)]
    pub pick_width_ft: Option<f64>,
    /// Spreader span (ft) as rigged for this pick, outer lug to outer lug.
    /// Prefilled from the saved bar; `None` falls back to the saved bar's span.
    #[serde(default)]
    pub spreader_span_ft: Option<f64>,
    /// Extra rigging tare on this layer (lb), beyond the automatic sling,
    /// shackle, and spreader self-weights.
    #[serde(default)]
    pub tare_lbs: f64,
}
