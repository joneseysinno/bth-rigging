//! Rig parameters panel: the keyed parameter table.

use dioxus::prelude::*;

use super::state::state;
use crate::docs;
use crate::ui::components::ParamTableView;
use bth_rigging::rig::edit::ParamEdit;

#[allow(non_snake_case)]
pub fn Params() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    let mut session = st.session;
    let mut status_message = st.status_message;
    let rig = session.read().rig().clone();
    let on_edit = move |edit: ParamEdit| match session.write().apply(edit) {
        Ok(()) => status_message.set(String::new()),
        Err(error) => status_message.set(error.message),
    };

    rsx! {
        div { class: "param-panel app-panel-pad",
            if !status_message().is_empty() {
                p { class: "status-line", "{status_message}" }
            }
            ParamTableView { rig, on_edit }
        }
    }
}
