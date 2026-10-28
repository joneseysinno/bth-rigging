//! App shell: root component and the app described as data.
//!
//! Bin / UI only. Layout comes from `app-shell`; document types live in `docs`.

pub mod context;

pub use context::{REVISION, bump_revision, store};

use app_shell::prelude::*;
use dioxus::prelude::*;

use crate::docs;

const TAILWIND: Asset = asset!("/assets/tailwind.css");
const APP_CSS: Asset = asset!("/assets/app.css");

#[component]
pub fn App() -> Element {
    rsx! {
        document::Stylesheet { href: TAILWIND }
        document::Stylesheet { href: APP_CSS }
        document::Link {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600;700&family=Source+Serif+4:opsz,wght@8..60,500;8..60,600&display=swap",
        }
        document::Title { "BTH Rigging" }

        AppShell { spec }
    }
}

/// The whole app, described as data.
fn spec() -> ShellSpec {
    let mut s = ShellSpec::new("BTH Rigging");
    s.theme = Theme::Light;
    s.nav_rail = vec![
        RailItem::command(docs::NAV_HOME, "Projects", icons::HOME),
        RailItem::panel("nav.docs", "Open documents", icons::FILES, DocumentsPane),
        RailItem::panel("nav.settings", "Settings", icons::SLIDERS, SettingsPane).at_end(),
    ];
    s.default_nav = None;
    s.doc_types = vec![
        docs::projects::doc_type(),
        docs::project::doc_type(),
        docs::pick::doc_type(),
        docs::mat::doc_type(),
        docs::rig::doc_type(),
    ];
    s.documents = vec![docs::projects::home_document()];
    s.active_document = Some(docs::projects::HOME_ID.into());
    s.on_command = Some(docs::on_command);
    s
}
