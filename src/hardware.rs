//! Diagram stroke colors for WSTDA cover names.
//!
//! Moves to `ui::components::rigging_diagram` in Step 0.5.
//! Roadmap: Step 0 foundation.

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
