//! Shared app context (bin / UI only).
//!
//! Formatters live in `bth_rigging::format`. Must not be imported by the lib.

use bth_rigging::db::RiggingStore;

#[derive(Clone)]
pub struct AppCtx {
    pub store: Option<RiggingStore>,
}
