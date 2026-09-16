//! Outrigger mat bearing-pressure calculations.
//!
//! Roadmap: Step 0 foundation; crane statics (Step 6) will feed loads here.

pub mod bearing;

pub use bearing::{MatBearingInput, MatBearingResult, calculate_mat_bearing};
