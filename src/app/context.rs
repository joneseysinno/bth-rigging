//! Shared app context (bin / UI only).
//!
//! Formatters live in `bth_rigging::format`. Must not be imported by the lib.
//! Roadmap: Step 0 foundation.

use bth_rigging::store::RiggingStore;

#[derive(Clone)]
pub struct AppCtx {
    pub store: Option<RiggingStore>,
}
