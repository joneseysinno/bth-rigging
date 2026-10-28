//! Editable layer rows for the pick input panel, and their conversion to
//! saved `SlingLayer`s.

use uuid::Uuid;

use crate::ui::components::DiagramLayer;
use bth_rigging::domain::{Hitch, SlingLayer};
use bth_rigging::format::format_num;

#[derive(Clone, PartialEq)]
pub struct LayerDraft {
    pub key: Uuid,
    pub size: u8,
    pub hitch: Hitch,
    pub angle_deg: String,
    pub sling_count: String,
    /// Sling length (ft) for self-weight and angle geometry.
    pub sling_length_ft: String,
    /// Adjacent pick-point spacing (ft).
    pub pick_spacing_ft: String,
    /// Pick-point width (ft) for rectangular patterns.
    pub pick_width_ft: String,
    /// Spreader span as rigged (ft).
    pub spreader_span_ft: String,
    pub apex_shackle: String,
    pub leg_shackle: String,
    /// Selected saved spreader id (empty = none).
    pub spreader_id: String,
    /// Legacy WLL if loaded without spreader_id.
    pub legacy_spreader_wll: Option<u32>,
    pub tare_lbs: String,
    /// Inline create form visibility.
    pub show_new_spreader: bool,
    pub new_mfr: String,
    pub new_model: String,
    pub new_wll: String,
    pub new_weight: String,
    pub new_span: String,
}

impl LayerDraft {
    pub fn new() -> Self {
        Self {
            key: Uuid::new_v4(),
            size: 2,
            hitch: Hitch::Vertical,
            angle_deg: "60".into(),
            sling_count: "2".into(),
            sling_length_ft: "10".into(),
            pick_spacing_ft: String::new(),
            pick_width_ft: String::new(),
            spreader_span_ft: String::new(),
            apex_shackle: String::new(),
            leg_shackle: String::new(),
            spreader_id: String::new(),
            legacy_spreader_wll: None,
            tare_lbs: "0".into(),
            show_new_spreader: false,
            new_mfr: String::new(),
            new_model: String::new(),
            new_wll: String::new(),
            new_weight: String::new(),
            new_span: String::new(),
        }
    }

    pub fn from_layer(layer: &SlingLayer) -> Self {
        let mut d = Self::new();
        d.size = layer.size;
        d.hitch = layer.hitch;
        d.angle_deg = format_num(layer.angle_deg);
        d.sling_count = layer.sling_count.to_string();
        d.sling_length_ft = format_num(layer.sling_length_ft);
        d.pick_spacing_ft = layer.pick_spacing_ft.map(format_num).unwrap_or_default();
        d.pick_width_ft = layer.pick_width_ft.map(format_num).unwrap_or_default();
        d.spreader_span_ft = layer.spreader_span_ft.map(format_num).unwrap_or_default();
        d.apex_shackle = layer.apex_shackle.clone().unwrap_or_default();
        d.leg_shackle = layer.leg_shackle.clone().unwrap_or_default();
        d.spreader_id = layer
            .spreader_id
            .map(|id| id.to_string())
            .unwrap_or_default();
        d.legacy_spreader_wll = if layer.spreader_id.is_none() {
            layer.spreader_wll_lbs
        } else {
            None
        };
        d.tare_lbs = format_num(layer.tare_lbs);
        d
    }

    pub fn to_layer(&self, pick_id: Uuid, layer_index: u32) -> Option<SlingLayer> {
        let angle_deg = self.angle_deg.trim().parse::<f64>().ok()?;
        let sling_count = self.sling_count.trim().parse::<u32>().ok()?;
        if sling_count == 0 {
            return None;
        }
        let tare_lbs = self.tare_lbs.trim().parse::<f64>().unwrap_or(0.0).max(0.0);
        let sling_length_ft = self
            .sling_length_ft
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
            .max(0.0);
        let apex = opt_str(&self.apex_shackle);
        let leg = opt_str(&self.leg_shackle);
        let pick_spacing_ft = opt_pos(&self.pick_spacing_ft);
        let pick_width_ft = opt_pos(&self.pick_width_ft);
        let spreader_span_ft = opt_pos(&self.spreader_span_ft);
        let spreader_id = if self.spreader_id.trim().is_empty() {
            None
        } else {
            Some(Uuid::parse_str(self.spreader_id.trim()).ok()?)
        };
        let spreader_wll_lbs = if spreader_id.is_some() {
            None
        } else {
            self.legacy_spreader_wll
        };
        Some(SlingLayer {
            pick_id,
            layer_index,
            size: self.size,
            hitch: self.hitch,
            angle_deg,
            sling_count,
            sling_length_ft,
            pick_spacing_ft,
            pick_width_ft,
            spreader_span_ft,
            apex_shackle: apex,
            leg_shackle: leg,
            spreader_id,
            spreader_wll_lbs,
            tare_lbs,
        })
    }

    pub fn to_diagram(&self, index: u32) -> Option<DiagramLayer> {
        let layer = self.to_layer(Uuid::nil(), index)?;
        Some(DiagramLayer::from_sling(&layer))
    }

    pub fn has_spreader(&self) -> bool {
        !self.spreader_id.trim().is_empty() || self.legacy_spreader_wll.is_some()
    }
}

/// Parse a positive number; blank / zero / invalid → `None`.
pub fn opt_pos(s: &str) -> Option<f64> {
    s.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

pub fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
