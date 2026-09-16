//! Crane subgraph: boom head as root, capacity, pose, clearance, statics, wind.
//!
//! Step: 4–7
//! Theory: Duplo10 Lift Readback crane sections; roadmap Steps 4–7.
//! Inputs: crane config, chart, reeving, site geometry, rig reactions.
//! Outputs: capacity checks, head/hook pose, clearances, outrigger loads, wind side load.
//! Must not depend on: UI, dioxus. May depend on domain, catalog, rig outputs.

pub mod chart;
pub mod clearance;
pub mod config;
pub mod pose;
pub mod reeving;
pub mod statics;
pub mod wind;
