//! Routed pages.
//!
//! Roadmap: Step 0 foundation; pick editor internals deferred to Step 2.

pub mod home;
pub mod mat_editor;
pub mod pick_editor;
pub mod project;
pub mod rig_editor;

pub use home::Home;
pub use mat_editor::MatEditor;
pub use pick_editor::PickEditor;
pub use project::ProjectPage;
pub use rig_editor::RigEditorPage;
