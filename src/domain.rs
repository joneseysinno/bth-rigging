//! Domain models for projects, picks, hardware catalogs, and mat analyses.
//!
//! Depends on nothing else in the crate. Serde field names are frozen for stored data.
//! Roadmap: Step 0 foundation.

pub mod mat;
pub mod pick;
pub mod project;
pub mod spreader;

pub use mat::{MatAnalysis, SavedMat};
pub use pick::{Hitch, Pick, SlingLayer};
pub use project::{Project, now_millis};
pub use spreader::SavedSpreader;
