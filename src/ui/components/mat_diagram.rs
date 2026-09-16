//! Top-down plan view of mat, pad, and effective bearing area (dimensions in feet).
//!
//! Roadmap: Step 0 foundation.

use dioxus::prelude::*;

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
