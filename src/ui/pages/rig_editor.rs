//! Interim route target for the Step 2.5 rig editor.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §8.
//! Inputs: project and rig identifiers.
//! Outputs: persisted rig identity and navigation until the full editor lands.
//! Must not depend on: solver implementation details.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{AppCtx, Route};

#[component]
pub fn RigEditorPage(project_id: Uuid, rig_id: Uuid) -> Element {
    let ctx = use_context::<AppCtx>();
    let mut rig_name = use_signal(|| None::<String>);

    {
        let ctx = ctx.clone();
        use_effect(move || {
            if let Some(ref store) = ctx.store {
                rig_name.set(store.load_rig(rig_id).ok().flatten().map(|rig| rig.name));
            }
        });
    }

    let title = rig_name().unwrap_or_else(|| "Rig editor".into());
    rsx! {
        div { class: "page-shell",
            header { class: "page-header compact",
                div { class: "page-header-inner",
                    Link {
                        to: Route::Project { id: project_id },
                        class: "back-link",
                        "← Project"
                    }
                    h1 { class: "brand-title", "{title}" }
                }
            }
            main { class: "page-main",
                p { class: "status-line", "The parameter table editor is being added next." }
            }
        }
    }
}
