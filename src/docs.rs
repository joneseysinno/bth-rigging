//! Document types: one module per `DocType`, registered in `app::spec()`.
//!
//! A document's id is its record's Uuid string. Ids the shell generates
//! itself (its "+ New" menu and palette) are not records; those documents
//! show [`NotARecord`].

use std::cell::RefCell;
use std::collections::HashMap;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

pub mod mat;
pub mod pick;
pub mod project;
pub mod projects;
pub mod rig;

/// Nav rail command that opens the Projects home tab.
pub const NAV_HOME: &str = "nav.home";

/// Open the record `id` as a document of `kind`, or switch to it if it is
/// already open.
pub fn open(mut shell: Shell, kind: &'static str, id: Uuid, name: impl Into<String>) {
    let id = id.to_string();
    let is_open = shell.documents.peek().iter().any(|d| d.id == id);
    if !is_open {
        shell
            .documents
            .write()
            .push(Document::new(id.clone(), name, kind));
    }
    shell.activate(&id);
}

pub fn close_if_open(mut shell: Shell, id: Uuid) {
    let id = id.to_string();
    let is_open = shell.documents.peek().iter().any(|d| d.id == id);
    if is_open {
        shell.close_document(&id);
    }
}

/// Keep the title bar and Documents pane in step with a saved name.
pub fn rename(mut shell: Shell, id: &str, name: &str) {
    let stale = shell
        .documents
        .peek()
        .iter()
        .any(|d| d.id == id && d.name != name);
    if stale {
        if let Some(doc) = shell.documents.write().iter_mut().find(|d| d.id == id) {
            doc.name = name.to_string();
        }
    }
}

/// Close a deleted record's document and drop its unsaved state.
pub fn discard(shell: Shell, id: Uuid) {
    close_if_open(shell, id);
    pick::forget(id);
    mat::forget(id);
    rig::forget(id);
    project::forget(id);
}

/// Every command from every document type lands here (after the shell has
/// handled its own built-ins). Routed by the id prefix of each doc type.
pub fn on_command(shell: Shell, ev: &CommandEvent) {
    if ev.id == NAV_HOME {
        projects::open_home(shell);
        return;
    }
    let Some(doc) = ev.doc.as_deref() else { return };
    let Ok(id) = Uuid::parse_str(doc) else { return };
    if ev.id.starts_with("pk.") {
        pick::handle(shell, id, ev.id);
    } else if ev.id.starts_with("mat.") {
        mat::handle(shell, id, ev.id);
    } else if ev.id.starts_with("rig.") {
        rig::handle(shell, id, ev.id);
    } else if ev.id.starts_with("prj.") {
        project::handle(shell, id, ev.id);
    }
}

/// The record id of the document this view was mounted for.
pub fn use_record_id() -> Option<Uuid> {
    use_document().and_then(|d| Uuid::parse_str(&d.id).ok())
}

/// A signal owned by the root scope, so it outlives the views that use it.
pub fn root_signal<T: 'static>(value: T) -> Signal<T> {
    Signal::new_in_scope(value, ScopeId::ROOT)
}

/// Per-document state handles, keyed by record id. The signals inside are
/// root-owned, so the viewport, panels and status readout of one document
/// share them, and unsaved edits survive closing and reopening it.
pub struct Registry<T: Copy + 'static> {
    map: RefCell<HashMap<Uuid, T>>,
}

impl<T: Copy + 'static> Registry<T> {
    pub fn new() -> Self {
        Self {
            map: RefCell::new(HashMap::new()),
        }
    }

    pub fn get_or_load(
        &self,
        id: Uuid,
        load: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        if let Some(state) = self.map.borrow().get(&id).copied() {
            return Ok(state);
        }
        let state = load()?;
        self.map.borrow_mut().insert(id, state);
        Ok(state)
    }

    pub fn forget(&self, id: Uuid) {
        self.map.borrow_mut().remove(&id);
    }
}

/// Viewport for a document that is not a saved record.
#[component]
pub fn NotARecord(label: &'static str) -> Element {
    let mut shell = use_shell();
    let id = use_document().map(|d| d.id).unwrap_or_default();
    rsx! {
        div { class: "app-empty",
            h2 { "Not a saved record" }
            p { "Open or create a {label} from the Projects page so it is saved with a job." }
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    projects::open_home(shell);
                    shell.close_document(&id);
                },
                "Close and open Projects"
            }
        }
    }
}

/// Viewport body when a record can't be loaded.
#[component]
pub fn LoadError(message: String) -> Element {
    rsx! {
        div { class: "app-empty",
            h2 { "Can't open this document" }
            p { "{message}" }
        }
    }
}
