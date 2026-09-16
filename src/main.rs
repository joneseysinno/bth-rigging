#![allow(non_snake_case)]

mod app_state;
mod calc;
mod catalog;
mod db;
mod diagram;
mod geometry;
mod hardware;
mod mat_calc;
mod models;
mod pages;
mod print;

#[cfg(test)]
mod golden_tests;

use dioxus::prelude::*;
use uuid::Uuid;

use app_state::AppCtx;
use db::RiggingStore;
use pages::{Home, MatEditor, PickEditor, ProjectPage};

const TAILWIND: Asset = asset!("/assets/tailwind.css");
const APP_CSS: Asset = asset!("/assets/app.css");

#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/project/:id")]
    Project { id: Uuid },
    #[route("/project/:project_id/pick/:pick_id")]
    PickEditor { project_id: Uuid, pick_id: Uuid },
    #[route("/project/:project_id/mat/:analysis_id")]
    MatEditor { project_id: Uuid, analysis_id: Uuid },
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let store = use_hook(|| RiggingStore::open_default().ok());
    use_context_provider(|| AppCtx {
        store: store.clone(),
    });

    rsx! {
        document::Stylesheet { href: TAILWIND }
        document::Stylesheet { href: APP_CSS }
        document::Link {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600;700&family=Source+Serif+4:opsz,wght@8..60,500;8..60,600&display=swap",
        }
        document::Title { "BTH Rigging" }

        Router::<Route> {}
    }
}

#[component]
fn Project(id: Uuid) -> Element {
    rsx! { ProjectPage { id } }
}
