//! Shared app state (bin / UI only).
//!
//! Formatters live in `bth_rigging::format`. Must not be imported by the lib.
//! `ShellSpec::on_command` is a plain `fn`, so the store is global rather than
//! in a context provider.

use bth_rigging::store::RiggingStore;
use dioxus::prelude::*;

/// The database, opened on first use. `None` when the data directory is unavailable.
pub static STORE: GlobalSignal<Option<RiggingStore>> =
    Signal::global(|| RiggingStore::open_default().ok());

/// Bumped after every save or delete so lists re-fetch.
pub static REVISION: GlobalSignal<u64> = Signal::global(|| 0);

pub fn store() -> Option<RiggingStore> {
    STORE.peek().clone()
}

pub fn bump_revision() {
    *REVISION.write() += 1;
}
