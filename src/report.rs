//! Project calc-package assembly and PDF export.
//!
//! Roadmap: Step 0 foundation.

pub mod calc_package;
pub mod pdf;

pub use calc_package::{
    assemble_calc_package, assemble_from_store, CalcPackage, MatRow, MatSheet, PickRow, PickSheet,
    PrintError,
};
pub use pdf::{calc_package_filename, render_calc_package_pdf, render_project_calc_package};
