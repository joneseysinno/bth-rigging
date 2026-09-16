//! Legacy layer engine: tension, geometry, and sling splits.
//!
//! Template oracle for the future rig graph (Step 1). Graph results must match.
//! Roadmap: Step 0 foundation; kept through Steps 1–3 as regression reference.

pub mod calc;
pub mod geometry;
pub mod split;

pub use calc::{
    HardwareCheck, LayerTension, PickResult, RiggingWeight, SpreaderInfo, apex_shackle_count,
    calculate_layer, calculate_pick, layer_rigging_weight, leg_count, tension_factor,
};
pub use geometry::{
    LayerGeometry, PlanPoint, SpacingSource, UNEQUAL_DROP_TOL_IN, endpoints_below_count,
    governing_angle, layer_spreader_span, resolve_geometry, rigging_height_ft,
};
pub use split::split_slings;
