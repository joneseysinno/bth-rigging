//! Shared rating checks used by layers, rig, and crane.
//!
//! Step: 3
//! Theory: WSTDA ratings and manufacturer reductions; roadmap Step 3.
//! Inputs: tensions / reactions and catalog ratings.
//! Outputs: pass/fail utilization stamps.
//! Must not depend on: UI, dioxus, store. May depend on catalog and domain.

pub mod bar;
pub mod chain;
pub mod lug;
pub mod shackle;
pub mod sling;
