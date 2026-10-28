//! Rig viewport: the rotatable 3D rig tree.

use app_shell::prelude::*;
use dioxus::prelude::*;

use super::state::state;
use super::{ID, PARAMS_PANEL};
use crate::docs::{self, LoadError, NotARecord};
use crate::ui::components::RigTreeView;
use bth_rigging::rig::RigTree;

#[allow(non_snake_case)]
pub fn Model() -> Element {
    let mut shell = use_shell();
    // The parameter table is the main input, so show it when a rig opens.
    // The bottom slot has no per-type default, and slots can't be set during render.
    use_hook(move || {
        spawn(async move {
            if shell.active_type().is_some_and(|t| t.id == ID) {
                shell.set_panel(Placement::Bottom, Some(PARAMS_PANEL));
            }
        });
    });
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! { NotARecord { label: "rig" } };
    };
    let st = match state(rig_id) {
        Ok(st) => st,
        Err(message) => return rsx! { LoadError { message } },
    };
    let view = st.view();
    let tree = RigTree::build(&view.rig);
    let rated = view.snapshot.rated.clone().unwrap_or_default();
    let draw_pose = view.snapshot.draw_pose().map(|(eval, solved, source)| {
        (
            eval.clone(),
            solved.map(|solved| solved.members.clone()),
            source,
        )
    });

    rsx! {
        div { class: "app-rig-model",
            div { class: "layer-toolbar",
                h2 { class: "panel-title app-rig-title",
                    "{view.rig.name}"
                    span {
                        class: if view.dirty { "dirty-indicator is-dirty" } else { "dirty-indicator" },
                        title: if view.dirty { "Unsaved changes" } else { "Saved" },
                    }
                }
                span { class: "muted", "{tree.members.len()} members" }
            }
            if let Some((eval, pose_forces, source)) = draw_pose {
                RigTreeView {
                    rig: view.rig.clone(),
                    tree,
                    eval,
                    forces: pose_forces,
                    pose: source,
                    rated,
                    selected: st.selected,
                    hovered: st.hovered,
                }
            } else {
                p { class: "muted", "The rig can't be drawn until it evaluates. See the errors under Checks." }
            }
        }
    }
}
