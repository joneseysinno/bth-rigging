//! SVG diagram components for lift and mat reports.
//!
//! Roadmap: Step 0 foundation.

pub mod mat_diagram;
pub mod param_table;
pub mod rigging_diagram;

pub use mat_diagram::MatBearingDiagram;
pub use param_table::ParamTableView;
pub use rigging_diagram::{DiagramLayer, RiggingDiagram};
