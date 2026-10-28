//! Navigation panes shown from the nav rail, for the open project.

pub mod details;
pub mod project;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::CURRENT_PROJECT;
use crate::docs;

pub use details::DetailsPane;
pub use project::ProjectPane;

pub const NAV_PROJECT: &str = "nav.project";
pub const NAV_DETAILS: &str = "nav.details";

/// Make `id` the open project and show its documents in the nav pane.
pub fn open_project(mut shell: Shell, id: Uuid) {
    *CURRENT_PROJECT.write() = Some(id);
    shell.set_panel(Placement::Nav, Some(NAV_PROJECT));
}

/// Pane body when no project is open.
#[component]
fn NoProject() -> Element {
    let shell = use_shell();
    rsx! {
        div { class: "app-pane app-pad",
            p { class: "muted", "No project open. Choose one on the Projects page." }
            button {
                class: "btn btn-primary sm",
                onclick: move |_| docs::projects::open_home(shell),
                "Open Projects"
            }
        }
    }
}
