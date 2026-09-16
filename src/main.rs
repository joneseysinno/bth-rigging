#![allow(non_snake_case)]

mod app;
mod ui;

use app::App;

fn main() {
    dioxus::launch(App);
}
