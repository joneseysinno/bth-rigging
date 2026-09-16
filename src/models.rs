//! Domain models for projects, picks, and sling layers.

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

/// A job folder that holds one or more picks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    /// Unix millis for sorting.
    pub updated_at: u64,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            notes: String::new(),
            updated_at: now_millis(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_millis();
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

pub fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
