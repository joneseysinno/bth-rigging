//! Saved mats and project-owned mat bearing analyses.
//!
//! Must not depend on catalog, store, or calculation modules.
//! Roadmap: Step 0 foundation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User-saved outrigger mat for bearing-pressure analyses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMat {
    pub id: Uuid,
    pub manufacturer: String,
    pub model: String,
    /// Plan length (ft).
    pub length_ft: f64,
    /// Plan width (ft).
    pub width_ft: f64,
    /// Thickness (in).
    pub thickness_in: f64,
    /// Self-weight (lb); added to outrigger load when used.
    #[serde(default)]
    pub weight_lbs: f64,
    /// Manufacturer allowable outrigger load on this mat (lb).
    #[serde(default)]
    pub manufacturer_allowable_lbs: f64,
    #[serde(default)]
    pub notes: String,
}

impl SavedMat {
    pub fn new(
        manufacturer: impl Into<String>,
        model: impl Into<String>,
        length_ft: f64,
        width_ft: f64,
        thickness_in: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            manufacturer: manufacturer.into(),
            model: model.into(),
            length_ft,
            width_ft,
            thickness_in,
            weight_lbs: 0.0,
            manufacturer_allowable_lbs: 0.0,
            notes: String::new(),
        }
    }

    pub fn label(&self) -> String {
        let mut s = format!(
            "{} {} · {}×{} ft × {} in",
            self.manufacturer.trim(),
            self.model.trim(),
            format_dim(self.length_ft),
            format_dim(self.width_ft),
            format_dim(self.thickness_in)
        );
        if self.weight_lbs > 0.0 {
            s.push_str(&format!(" · {} lb", self.weight_lbs.round() as u64));
        }
        if self.manufacturer_allowable_lbs > 0.0 {
            s.push_str(&format!(
                " · allow {} lb",
                self.manufacturer_allowable_lbs.round() as u64
            ));
        }
        s
    }
}

fn format_dim(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

/// A project-owned mat bearing-pressure analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatAnalysis {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    /// Selected saved mat.
    pub mat_id: Uuid,
    /// Outrigger reaction / float load (lb).
    pub outrigger_load_lbs: f64,
    /// Outrigger pad length (in).
    pub pad_length_in: f64,
    /// Outrigger pad width (in).
    pub pad_width_in: f64,
    /// Allowable ground bearing pressure (psf).
    pub allowable_psf: f64,
    /// Retained for older saves; unused by Duerr Leff calc.
    #[serde(default = "default_spread_angle")]
    pub spread_angle_deg: f64,
}

fn default_spread_angle() -> f64 {
    45.0
}

impl MatAnalysis {
    pub fn new(project_id: Uuid, name: impl Into<String>, mat_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            name: name.into(),
            mat_id,
            outrigger_load_lbs: 50_000.0,
            pad_length_in: 24.0,
            pad_width_in: 24.0,
            allowable_psf: 3_000.0,
            spread_angle_deg: 45.0,
        }
    }
}
