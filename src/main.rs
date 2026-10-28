//! Desktop binary entry point: launches the Dioxus app (`desktop` feature).
//!
//! The layout (title bar, rails, panels, palette) comes from `app-shell`.
//! Everything app-specific lives in `app::spec()` and `docs`.
#![allow(non_snake_case)]

mod app;
mod docs;
mod ui;

use app::App;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    let window = WindowBuilder::new()
        .with_title("BTH Rigging")
        .with_inner_size(LogicalSize::new(1440.0, 900.0));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
}
