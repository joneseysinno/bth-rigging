//! Project picks and mat analyses list.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::Route;
use crate::app_state::AppCtx;
use bth_rigging::format::format_lbs;
use bth_rigging::models::{MatAnalysis, Pick, Project};
use bth_rigging::print::{calc_package_filename, render_project_calc_package};

#[component]
pub fn ProjectPage(id: Uuid) -> Element {
    let ctx = use_context::<AppCtx>();
    let mut project = use_signal(|| None::<Project>);
    let mut picks = use_signal(Vec::<Pick>::new);
    let mut mat_analyses = use_signal(Vec::<MatAnalysis>::new);
    let mut status = use_signal(|| String::new());
    let navigator = use_navigator();

    {
        let ctx = ctx.clone();
        use_effect(move || {
            if let Some(ref s) = ctx.store {
                project.set(s.load_project(id).ok().flatten());
                picks.set(s.list_picks_for_project(id).unwrap_or_default());
                mat_analyses.set(s.list_mat_analyses_for_project(id).unwrap_or_default());
            }
        });
    }

    let Some(proj) = project() else {
        return rsx! {
            div { class: "page-shell",
                main { class: "page-main",
                    Link { to: Route::Home {}, class: "back-link", "← Projects" }
                    p { class: "status-line", "Project not found." }
                }
            }
        };
    };

    rsx! {
        div { class: "page-shell",
            header { class: "page-header",
                div { class: "page-header-inner",
                    Link { to: Route::Home {}, class: "back-link", "← Projects" }
                    h1 { class: "brand-title", "{proj.name}" }
                    p { class: "brand-sub", "Picks and mat analyses in this job" }
                }
            }

            main { class: "page-main",
                div { class: "toolbar",
                    button {
                        class: "btn btn-primary",
                        onclick: {
                            let ctx = ctx.clone();
                            move |_| {
                                let pick = Pick::new(id, "New Pick", 10_000.0);
                                if let Some(ref s) = ctx.store {
                                    if let Err(e) = s.save_pick(&pick, &[]) {
                                        status.set(e.to_string());
                                        return;
                                    }
                                    navigator.push(Route::PickEditor {
                                        project_id: id,
                                        pick_id: pick.id,
                                    });
                                }
                            }
                        },
                        "New pick"
                    }
                    button {
                        class: "btn btn-secondary",
                        onclick: {
                            let ctx = ctx.clone();
                            move |_| {
                                let analysis =
                                    MatAnalysis::new(id, "New mat analysis", Uuid::nil());
                                if let Some(ref s) = ctx.store {
                                    if let Err(e) = s.save_mat_analysis(&analysis) {
                                        status.set(e.to_string());
                                        return;
                                    }
                                    navigator.push(Route::MatEditor {
                                        project_id: id,
                                        analysis_id: analysis.id,
                                    });
                                }
                            }
                        },
                        "New mat analysis"
                    }
                    button {
                        class: "btn btn-secondary",
                        disabled: picks().is_empty() && mat_analyses().is_empty(),
                        onclick: {
                            let ctx = ctx.clone();
                            let proj_name = proj.name.clone();
                            move |_| {
                                if picks().is_empty() && mat_analyses().is_empty() {
                                    status.set(
                                        "Add a pick or mat analysis before printing.".into(),
                                    );
                                    return;
                                }
                                let Some(ref s) = ctx.store else {
                                    status.set("Database not available.".into());
                                    return;
                                };
                                let default_name = calc_package_filename(&proj_name);
                                let Some(path) = rfd::FileDialog::new()
                                    .set_file_name(&default_name)
                                    .add_filter("PDF", &["pdf"])
                                    .save_file()
                                else {
                                    status.set("Save cancelled.".into());
                                    return;
                                };
                                match render_project_calc_package(s, id) {
                                    Ok(bytes) => match std::fs::write(&path, bytes) {
                                        Ok(()) => status.set("Saved calc package.".into()),
                                        Err(e) => status.set(format!("Save failed: {e}")),
                                    },
                                    Err(e) => status.set(e.to_string()),
                                }
                            }
                        },
                        "Print calc package"
                    }
                }

                if !status().is_empty() {
                    p { class: "status-line", "{status}" }
                }

                h2 { class: "section-label", "Picks" }
                if picks().is_empty() {
                    div { class: "empty-state compact",
                        h2 { "No picks yet" }
                        p { "Add a pick to enter load, layers, and hardware." }
                    }
                } else {
                    ul { class: "pick-list",
                        for pick in picks() {
                            {
                                let pick_id = pick.id;
                                let name = pick.name.clone();
                                let weight = pick.weight_lbs;
                                rsx! {
                                    li { key: "{pick_id}", class: "pick-row",
                                        button {
                                            class: "pick-row-main",
                                            onclick: move |_| {
                                                navigator.push(Route::PickEditor {
                                                    project_id: id,
                                                    pick_id,
                                                });
                                            },
                                            span { class: "pick-name", "{name}" }
                                            span { class: "muted", "{weight as u64} lb" }
                                        }
                                        button {
                                            class: "btn btn-ghost danger",
                                            onclick: {
                                                let ctx = ctx.clone();
                                                move |_| {
                                                    if let Some(ref s) = ctx.store {
                                                        let _ = s.delete_pick(pick_id);
                                                        picks.set(
                                                            s.list_picks_for_project(id).unwrap_or_default(),
                                                        );
                                                    }
                                                }
                                            },
                                            "Delete"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                h2 { class: "section-label list-section", "Mat analyses" }
                if mat_analyses().is_empty() {
                    div { class: "empty-state compact",
                        h2 { "No mat analyses yet" }
                        p { "Add a mat analysis to check outrigger ground bearing pressure." }
                    }
                } else {
                    ul { class: "pick-list",
                        for analysis in mat_analyses() {
                            {
                                let analysis_id = analysis.id;
                                let name = analysis.name.clone();
                                let load = analysis.outrigger_load_lbs;
                                rsx! {
                                    li { key: "{analysis_id}", class: "pick-row",
                                        button {
                                            class: "pick-row-main",
                                            onclick: move |_| {
                                                navigator.push(Route::MatEditor {
                                                    project_id: id,
                                                    analysis_id,
                                                });
                                            },
                                            span { class: "pick-name", "{name}" }
                                            span { class: "muted", "{format_lbs(load)} lb outrigger" }
                                        }
                                        button {
                                            class: "btn btn-ghost danger",
                                            onclick: {
                                                let ctx = ctx.clone();
                                                move |_| {
                                                    if let Some(ref s) = ctx.store {
                                                        let _ = s.delete_mat_analysis(analysis_id);
                                                        mat_analyses.set(
                                                            s.list_mat_analyses_for_project(id)
                                                                .unwrap_or_default(),
                                                        );
                                                    }
                                                }
                                            },
                                            "Delete"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
