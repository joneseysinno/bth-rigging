//! Engineering core for BTH Rigging.
//!
//! No Dioxus or rfd. Headless tests: `cargo test --lib --no-default-features`.
//! Roadmap: Step 0 foundation; Steps 1–7 land under `rig`, `crane`, and `checks`.

pub mod catalog;
pub mod checks;
pub mod crane;
pub mod domain;
pub mod format;
pub mod layers;
pub mod mats;
pub mod report;
pub mod rig;
pub mod store;
