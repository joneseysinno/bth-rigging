//! User-saved spreader bar catalog entries.
//!
//! Must not depend on catalog, store, or calculation modules.
//! Roadmap: Step 0 foundation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User-saved spreader bar for quick selection across picks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSpreader {
    pub id: Uuid,
    pub manufacturer: String,
    pub model: String,
    pub wll_lbs: u32,
    /// Self-weight of the bar (lb); added into the load chain going up.
    pub weight_lbs: f64,
    #[serde(default)]
    pub span_ft: Option<f64>,
    #[serde(default)]
    pub notes: String,
}

impl SavedSpreader {
    pub fn new(
        manufacturer: impl Into<String>,
        model: impl Into<String>,
        wll_lbs: u32,
        weight_lbs: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            manufacturer: manufacturer.into(),
            model: model.into(),
            wll_lbs,
            weight_lbs,
            span_ft: None,
            notes: String::new(),
        }
    }

    pub fn label(&self) -> String {
        let mut s = format!(
            "{} {} · {} lb WLL",
            self.manufacturer.trim(),
            self.model.trim(),
            self.wll_lbs
        );
        if self.weight_lbs > 0.0 {
            s.push_str(&format!(" · {} lb bar", self.weight_lbs.round() as u64));
        }
        if let Some(span) = self.span_ft {
            s.push_str(&format!(" · {span} ft"));
        }
        s
    }
}
