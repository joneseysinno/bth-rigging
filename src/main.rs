//! Desktop binary entry point: launches the Dioxus app (`desktop` feature).
#![allow(non_snake_case)]

mod app;
mod ui;

use app::App;

fn main() {
    dioxus::launch(App);
}
