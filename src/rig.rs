//! Rigging graph: bodies, nodes, members, components, and solve pipeline.
//!
//! Step: 1–3, 7 (envelope)
//! Theory: Duplo10 Lift Readback Parts 3–4; roadmap Steps 1–3.
//! Inputs: authored graph + parameters.
//! Outputs: tensions, reactions, views; must match `layers` for template cases.
//! Must not depend on: UI, dioxus, store (persistence is `store::rig`).

pub mod bearing;
pub mod body;
pub mod component;
pub mod eval;
pub mod member;
pub mod node;
pub mod param;
pub mod solve;
pub mod template;
pub mod views;
