//! Solver pipeline overview.
//!
//! Step: 3
//! Theory: roadmap Step 3 — determinacy and tension solve.
//! Inputs: evaluated graph.
//! Outputs: tensions and reactions.
//! Must not depend on: UI, dioxus.

pub mod bounds;
pub mod elastic;
pub mod envelope;
pub mod hang;
pub mod inverse;
pub mod rank;
