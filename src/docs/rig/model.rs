//! Rig viewport: the node-graph canvas for building the rig and the
//! rotatable 3D rig tree, alone or side by side.

use app_shell::prelude::*;
use dioxus::prelude::*;

use super::state::{ViewMode, state};
use super::{ID, PARAMS_PANEL};
use crate::docs::{self, LoadError, NotARecord};
use crate::ui::components::{RigCanvas, RigTreeView};
use bth_rigging::rig::RigTree;
use bth_rigging::rig::topo::TopoEdit;

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
    let mut mode = st.view_mode;
    let current = mode();
    let tree = RigTree::build(&view.rig);
    let rated = view.snapshot.rated.clone().unwrap_or_default();
    let draw_pose = view.snapshot.draw_pose().map(|(eval, solved, source)| {
        (
            eval.clone(),
            solved.map(|solved| solved.members.clone()),
            source,
        )
    });
    let modes = [
        ("Diagram", ViewMode::Diagram),
        ("3D", ViewMode::ThreeD),
        ("Split", ViewMode::Split),
    ];

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
                div { class: "rig-view-modes",
                    for (label , value) in modes {
                        button {
                            key: "{label}",
                            class: if current == value { "btn btn-ghost sm is-active" } else { "btn btn-ghost sm" },
                            onclick: move |_| mode.set(value),
                            "{label}"
                        }
                    }
                }
            }
            div { class: if current == ViewMode::Split { "rig-views is-split" } else { "rig-views" },
                if current != ViewMode::ThreeD {
                    RigCanvas {
                        session: st.session,
                        layout: st.layout,
                        selected: st.selected,
                        hovered: st.hovered,
                        armed: st.armed,
                        on_edit: move |edit: TopoEdit| st.edit(edit),
                        on_move: move |(key, at)| st.move_block(rig_id, key, at),
                        picked: st.picked,
                        gather_opts: st.gather_opts,
                        on_plan: move |plan| st.apply_plan(plan),
                    }
                }
                if current != ViewMode::Diagram {
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
                        div { class: "rig-3d-empty",
                            p { class: "muted",
                                "The 3D view appears once the rig evaluates. Hang the load from the hook in the diagram; open Checks to see what is missing."
                            }
                        }
                    }
                }
            }
        }
    }
}
