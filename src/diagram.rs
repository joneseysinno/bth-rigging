//! Simple SVG schematic of stacked, connected sling layers.

use dioxus::prelude::*;

use crate::app_state::format_lbs;
use crate::catalog::find_by_size;
use crate::geometry::LayerGeometry;
use crate::hardware::{sling_stroke_color, split_slings};
use crate::models::SlingLayer;

/// Layer data needed to draw the schematic.
#[derive(Clone, PartialEq)]
pub struct DiagramLayer {
    pub index: u32,
    pub angle_deg: f64,
    pub sling_count: u32,
    pub size: u8,
    pub has_apex_shackle: bool,
    pub has_leg_shackle: bool,
    pub has_spreader: bool,
    /// Angle came from sling length + pick spacing.
    pub angle_calculated: bool,
    /// Rigging cannot reach the pick points.
    pub geometry_error: bool,
    /// Pick spacing / bar span annotation.
    pub spacing_label: Option<String>,
}

impl DiagramLayer {
    pub fn from_sling(layer: &SlingLayer) -> Self {
        Self {
            index: layer.layer_index,
            angle_deg: layer.angle_deg.clamp(5.0, 90.0),
            sling_count: layer.sling_count.max(1),
            size: layer.size,
            has_apex_shackle: layer.apex_shackle.is_some(),
            has_leg_shackle: layer.leg_shackle.is_some(),
            has_spreader: layer.spreader_id.is_some() || layer.spreader_wll_lbs.is_some(),
            angle_calculated: false,
            geometry_error: false,
            spacing_label: None,
        }
    }

    /// Like [`from_sling`](Self::from_sling) but draws the calculated angle.
    pub fn from_sling_geometry(layer: &SlingLayer, geom: &LayerGeometry) -> Self {
        let mut d = Self::from_sling(layer);
        if geom.error.is_some() {
            d.geometry_error = true;
            d.angle_deg = 5.0;
        } else if let Some(a) = geom.angle_deg {
            d.angle_deg = a.clamp(5.0, 90.0);
            d.angle_calculated = true;
        }
        d.spacing_label = geom.spacing_label();
        d
    }
}

fn foot_xs(cx: f64, half_span: f64, count: u32) -> Vec<f64> {
    let n = count.max(1);
    if n == 1 {
        return vec![cx];
    }
    (0..n)
        .map(|k| {
            let t = (k as f64 / (n - 1) as f64) * 2.0 - 1.0;
            cx + t * half_span
        })
        .collect()
}

fn half_span_for(angle: f64, rise: f64) -> f64 {
    if angle >= 89.5 {
        18.0
    } else {
        let rad = angle.to_radians();
        (rise / rad.tan()).clamp(20.0, 150.0)
    }
}

/// `level_loads[i]` is the load passing through the top of layer `i`
/// (payload + all rigging at and below that layer). Empty when not computed.
#[component]
pub fn RiggingDiagram(
    layers: Vec<DiagramLayer>,
    weight_lbs: f64,
    level_loads: Vec<f64>,
) -> Element {
    let n = layers.len().max(1) as f64;
    let slot = 160.0;
    let height = 100.0 + n * slot + 40.0;
    let width = 420.0;
    let cx = width / 2.0;

    // Precompute endpoint x positions so lower layers attach to upper feet.
    let mut parent_xs: Vec<f64> = vec![cx];
    let mut layer_geometry: Vec<(Vec<f64>, Vec<f64>, f64, f64)> = Vec::new();
    // (apex_xs, foot_xs, top_y, foot_y)

    for (i, layer) in layers.iter().enumerate() {
        let top_y = 48.0 + i as f64 * slot;
        let rise = 88.0;
        let angle = layer.angle_deg;
        let half = half_span_for(angle, rise);
        let count = layer.sling_count.min(8);

        let apex_xs = if i == 0 { vec![cx] } else { parent_xs.clone() };

        let foot_y = top_y + rise;
        let mut feet = Vec::new();

        if i == 0 {
            feet = foot_xs(cx, half, count);
        } else {
            let splits = split_slings(apex_xs.len() as u32, count);
            for (ai, &apex_x) in apex_xs.iter().enumerate() {
                let n_here = splits.get(ai).copied().unwrap_or(1).max(1);
                let group_half = if n_here == 1 {
                    0.0
                } else {
                    (half * 0.55).clamp(12.0, 55.0)
                };
                for k in 0..n_here {
                    let t = if n_here == 1 {
                        0.0
                    } else {
                        (k as f64 / (n_here - 1) as f64) * 2.0 - 1.0
                    };
                    feet.push(apex_x + t * group_half);
                }
            }
        }

        // Spreader endpoints = feet (even if fewer drawn for clutter).
        parent_xs = feet.clone();
        layer_geometry.push((apex_xs, feet, top_y, foot_y));
    }

    rsx! {
        svg {
            class: "rig-diagram w-full h-auto transition-opacity duration-300",
            view_box: "0 0 {width} {height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0",
                y: "0",
                width: "{width}",
                height: "{height}",
                fill: "#faf8f5",
                rx: "4",
            }

            // Top hook
            {
                let hook_y = 28.0;
                rsx! {
                    line {
                        x1: "{cx}",
                        y1: "8",
                        x2: "{cx}",
                        y2: "{hook_y - 10.0}",
                        stroke: "#1e293b",
                        stroke_width: "2.5",
                        stroke_linecap: "round",
                    }
                    circle {
                        cx: "{cx}",
                        cy: "{hook_y}",
                        r: "9",
                        fill: "none",
                        stroke: "#1e293b",
                        stroke_width: "2.5",
                    }
                    if let Some(&hook) = level_loads.first() {
                        if hook.is_finite() {
                            text {
                                x: "{cx + 16.0}",
                                y: "{hook_y + 4.0}",
                                fill: "#b45309",
                                font_size: "11",
                                font_family: "Outfit, system-ui, sans-serif",
                                font_weight: "700",
                                "Hook {format_lbs(hook)} lb"
                            }
                        }
                    }
                }
            }

            for (i, layer) in layers.iter().enumerate() {
                {
                    let layer = layer.clone();
                    let (apex_xs, feet, top_y, foot_y) = layer_geometry[i].clone();
                    let color = if layer.geometry_error {
                        "#dc2626"
                    } else {
                        find_by_size(layer.size)
                            .map(|r| sling_stroke_color(r.color))
                            .unwrap_or("#334155")
                    };
                    let label_y = top_y - 6.0;
                    let angle = layer.angle_deg;
                    let shackle_y = foot_y + 8.0;
                    let spreader_y = if layer.has_leg_shackle {
                        shackle_y + 14.0
                    } else {
                        foot_y + 4.0
                    };
                    let is_last = i + 1 == layers.len();

                    rsx! {
                        g { key: "{layer.index}",
                            text {
                                x: "16",
                                y: "{label_y}",
                                fill: "#64748b",
                                font_size: "11",
                                font_family: "Outfit, system-ui, sans-serif",
                                font_weight: "600",
                                "L{layer.index + 1}"
                            }
                            // Cumulative load at the top of this layer (payload + rigging below)
                            if let Some(&load) = level_loads.get(i) {
                                if load.is_finite() {
                                    text {
                                        x: "16",
                                        y: "{label_y + 13.0}",
                                        fill: "#b45309",
                                        font_size: "9.5",
                                        font_family: "Outfit, system-ui, sans-serif",
                                        font_weight: "600",
                                        "{format_lbs(load)} lb"
                                    }
                                }
                            }

                            // Apex shackles at each attachment point from above
                            for (ai, &ax) in apex_xs.iter().enumerate() {
                                {
                                    let ax = ax;
                                    rsx! {
                                        if layer.has_apex_shackle {
                                            circle {
                                                key: "apex-{ai}",
                                                cx: "{ax}",
                                                cy: "{top_y}",
                                                r: "11",
                                                fill: "#faf8f5",
                                                stroke: "#0f172a",
                                                stroke_width: "2.2",
                                            }
                                            line {
                                                x1: "{ax - 11.0}",
                                                y1: "{top_y}",
                                                x2: "{ax + 11.0}",
                                                y2: "{top_y}",
                                                stroke: "#0f172a",
                                                stroke_width: "2.2",
                                                stroke_linecap: "round",
                                            }
                                        } else {
                                            circle {
                                                key: "apex-dot-{ai}",
                                                cx: "{ax}",
                                                cy: "{top_y}",
                                                r: "5",
                                                fill: "#0f172a",
                                            }
                                        }
                                    }
                                }
                            }

                            // Sling lines: each foot connects to its parent apex
                            {
                                let splits = if i == 0 {
                                    vec![feet.len() as u32]
                                } else {
                                    split_slings(apex_xs.len() as u32, layer.sling_count.min(8))
                                };
                                let mut segments: Vec<(usize, usize, f64, f64)> = Vec::new();
                                let mut foot_idx = 0usize;
                                for (ai, &ax) in apex_xs.iter().enumerate() {
                                    let n_here = splits.get(ai).copied().unwrap_or(1) as usize;
                                    for k in 0..n_here {
                                        let fx = feet.get(foot_idx + k).copied().unwrap_or(ax);
                                        segments.push((ai, k, ax, fx));
                                    }
                                    foot_idx += n_here;
                                }
                                rsx! {
                                    for (ai, k, ax, fx) in segments {
                                        line {
                                            key: "sling-{ai}-{k}",
                                            x1: "{ax}",
                                            y1: "{top_y + 12.0}",
                                            x2: "{fx}",
                                            y2: "{foot_y}",
                                            stroke: "{color}",
                                            stroke_width: "3",
                                            stroke_linecap: "round",
                                            class: "transition-all duration-300",
                                        }
                                    }
                                }
                            }

                            // Leg shackles at feet
                            if layer.has_leg_shackle {
                                for (fi, &fx) in feet.iter().enumerate() {
                                    {
                                        let fx = fx;
                                        rsx! {
                                            circle {
                                                key: "leg-{fi}",
                                                cx: "{fx}",
                                                cy: "{shackle_y}",
                                                r: "7",
                                                fill: "#faf8f5",
                                                stroke: "#0f172a",
                                                stroke_width: "1.8",
                                            }
                                            line {
                                                x1: "{fx - 7.0}",
                                                y1: "{shackle_y}",
                                                x2: "{fx + 7.0}",
                                                y2: "{shackle_y}",
                                                stroke: "#0f172a",
                                                stroke_width: "1.8",
                                            }
                                        }
                                    }
                                }
                            }

                            // Spreader below shackles
                            if layer.has_spreader && feet.len() >= 2 {
                                {
                                    let x_left = feet.first().copied().unwrap_or(cx);
                                    let x_right = feet.last().copied().unwrap_or(cx);
                                    rsx! {
                                        line {
                                            x1: "{x_left}",
                                            y1: "{spreader_y}",
                                            x2: "{x_right}",
                                            y2: "{spreader_y}",
                                            stroke: "#475569",
                                            stroke_width: "6",
                                            stroke_linecap: "round",
                                        }
                                    }
                                }
                            }

                            // Angle annotation
                            if let (Some(&ax), Some(&fx)) = (apex_xs.first(), feet.last()) {
                                {
                                    let mid_x = (ax + fx) / 2.0 + 14.0;
                                    let mid_y = (top_y + 12.0 + foot_y) / 2.0;
                                    rsx! {
                                        text {
                                            x: "{mid_x}",
                                            y: "{mid_y}",
                                            fill: "#78716c",
                                            font_size: "10",
                                            font_family: "Outfit, system-ui, sans-serif",
                                            if layer.geometry_error {
                                                "too short"
                                            } else if layer.angle_calculated {
                                                "{angle:.1}°"
                                            } else {
                                                "{angle:.0}°"
                                            }
                                        }
                                    }
                                }
                            }

                            // Pick spacing / bar span annotation
                            if let Some(ref label) = layer.spacing_label {
                                text {
                                    x: "{width - 10.0}",
                                    y: "{foot_y + 22.0}",
                                    text_anchor: "end",
                                    fill: "#57534e",
                                    font_size: "9.5",
                                    font_family: "Outfit, system-ui, sans-serif",
                                    "{label}"
                                }
                            }

                            // Load stub under last layer
                            if is_last {
                                {
                                    let ly = if layer.has_spreader {
                                        spreader_y + 18.0
                                    } else if layer.has_leg_shackle {
                                        shackle_y + 18.0
                                    } else {
                                        foot_y + 12.0
                                    };
                                    rsx! {
                                        rect {
                                            x: "{cx - 36.0}",
                                            y: "{ly}",
                                            width: "72",
                                            height: "22",
                                            rx: "2",
                                            fill: "#e7e5e4",
                                            stroke: "#57534e",
                                            stroke_width: "1.5",
                                        }
                                        text {
                                            x: "{cx}",
                                            y: "{ly + 15.0}",
                                            text_anchor: "middle",
                                            fill: "#292524",
                                            font_size: "10",
                                            font_family: "Outfit, system-ui, sans-serif",
                                            font_weight: "600",
                                            if weight_lbs.is_finite() && weight_lbs > 0.0 {
                                                "{format_lbs(weight_lbs)} lb"
                                            } else {
                                                "LOAD"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Top-down plan view of mat, pad, and effective bearing area (all dimensions in feet).
#[component]
pub fn MatBearingDiagram(
    mat_length_ft: f64,
    mat_width_ft: f64,
    pad_length_ft: f64,
    pad_width_ft: f64,
    effective_length_ft: f64,
    effective_width_ft: f64,
) -> Element {
    let mat_l = mat_length_ft.max(0.1);
    let mat_w = mat_width_ft.max(0.1);
    let view_w = 420.0_f64;
    let view_h = 280.0_f64;
    let margin = 36.0;
    let scale = ((view_w - 2.0 * margin) / mat_l).min((view_h - 2.0 * margin) / mat_w);

    let mx = (view_w - mat_l * scale) / 2.0;
    let my = (view_h - mat_w * scale) / 2.0;
    let mw = mat_l * scale;
    let mh = mat_w * scale;

    let pad_l = pad_length_ft.clamp(0.0, mat_l);
    let pad_w = pad_width_ft.clamp(0.0, mat_w);
    let pw = pad_l * scale;
    let ph = pad_w * scale;
    let px = mx + (mw - pw) / 2.0;
    let py = my + (mh - ph) / 2.0;

    let eff_l = effective_length_ft.clamp(0.0, mat_l);
    let eff_w = effective_width_ft.clamp(0.0, mat_w);
    let ew = eff_l * scale;
    let eh = eff_w * scale;
    let ex = mx + (mw - ew) / 2.0;
    let ey = my + (mh - eh) / 2.0;

    let label = format!("{mat_l:.2} × {mat_w:.2} ft mat");

    rsx! {
        svg {
            class: "mat-diagram",
            view_box: "0 0 {view_w} {view_h}",
            width: "100%",
            height: "260",
            role: "img",
            "aria-label": "Mat bearing plan view",

            rect {
                x: "0",
                y: "0",
                width: "{view_w}",
                height: "{view_h}",
                fill: "#faf8f5",
            }

            // Mat footprint
            rect {
                x: "{mx}",
                y: "{my}",
                width: "{mw}",
                height: "{mh}",
                fill: "#d6d3d1",
                stroke: "#44403c",
                stroke_width: "2",
            }

            // Effective area (dashed)
            if ew > 0.5 && eh > 0.5 {
                rect {
                    x: "{ex}",
                    y: "{ey}",
                    width: "{ew}",
                    height: "{eh}",
                    fill: "rgba(180, 83, 9, 0.12)",
                    stroke: "#b45309",
                    stroke_width: "1.75",
                    stroke_dasharray: "6 4",
                }
            }

            // Pad
            if pw > 0.5 && ph > 0.5 {
                rect {
                    x: "{px}",
                    y: "{py}",
                    width: "{pw}",
                    height: "{ph}",
                    fill: "#1e293b",
                    stroke: "#0f172a",
                    stroke_width: "1.5",
                }
            }

            text {
                x: "{view_w / 2.0}",
                y: "{view_h - 12.0}",
                text_anchor: "middle",
                fill: "#57534e",
                font_size: "11",
                font_family: "Outfit, system-ui, sans-serif",
                "{label}"
            }
            text {
                x: "{mx + 8.0}",
                y: "{my + 16.0}",
                fill: "#292524",
                font_size: "10",
                font_family: "Outfit, system-ui, sans-serif",
                font_weight: "600",
                "MAT"
            }
            if pw > 0.5 {
                text {
                    x: "{px + pw / 2.0}",
                    y: "{py + ph / 2.0 + 4.0}",
                    text_anchor: "middle",
                    fill: "#f8fafc",
                    font_size: "9",
                    font_family: "Outfit, system-ui, sans-serif",
                    font_weight: "700",
                    "PAD"
                }
            }
        }
    }
}
