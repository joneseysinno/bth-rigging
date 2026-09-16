//! Engineering core for BTH Rigging.
//!
//! No Dioxus or rfd. Headless tests: `cargo test --lib --no-default-features`.
//! Roadmap: Step 0 foundation; Steps 1–7 land under `rig`, `crane`, and `checks`.

pub mod calc;
pub mod catalog;
pub mod db;
pub mod format;
pub mod geometry;
pub mod hardware;
pub mod mat_calc;
pub mod models;
pub mod print;
