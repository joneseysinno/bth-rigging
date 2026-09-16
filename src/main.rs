#![allow(non_snake_case)]

mod app;
mod ui;

use dioxus::prelude::*;

use app::App;

fn main() {
    dioxus::launch(App);
}
