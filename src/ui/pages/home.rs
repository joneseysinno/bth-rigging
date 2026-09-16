//! Projects home page.

use dioxus::prelude::*;

use crate::app::{AppCtx, Route};
use bth_rigging::domain::Project;
use bth_rigging::format::format_updated;

#[component]
pub fn Home() -> Element {
    let ctx = use_context::<AppCtx>();
    let mut projects = use_signal(Vec::<(Project, usize)>::new);
    let mut new_name = use_signal(|| String::new());
    let mut status = use_signal(|| String::new());
    let mut tick = use_signal(|| 0u32);
    let navigator = use_navigator();

    {
        let ctx = ctx.clone();
        use_effect(move || {
            let _ = tick();
            if let Some(ref s) = ctx.store {
                match s.list_projects() {
                    Ok(list) => {
                        let mut rows = Vec::new();
                        for p in list {
                            let count = s.pick_count(p.id).unwrap_or(0);
                            rows.push((p, count));
                        }
                        projects.set(rows);
                    }
                    Err(e) => status.set(e.to_string()),
                }
            } else {
                status.set("Database unavailable.".into());
            }
        });
    }

    rsx! {
        div { class: "page-shell",
            header { class: "page-header",
                div { class: "page-header-inner",
                    div {
                        p { class: "brand-kicker", "BTH Rigging" }
                        h1 { class: "brand-title", "Projects" }
                        p { class: "brand-sub", "Job folders for pick planning and lift reports" }
                    }
                }
            }

            main { class: "page-main",
                section { class: "panel create-bar",
                    label { class: "field grow",
                        span { class: "field-label", "New project" }
                        input {
                            class: "field-input",
                            placeholder: "e.g. Plant outage — Unit 3",
                            value: "{new_name}",
                            oninput: move |e| new_name.set(e.value()),
                        }
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: {
                            let ctx = ctx.clone();
                            move |_| {
                                let name = new_name().trim().to_string();
                                if name.is_empty() {
                                    status.set("Enter a project name.".into());
                                    return;
                                }
                                if let Some(ref s) = ctx.store {
                                    let project = Project::new(name);
                                    match s.save_project(&project) {
                                        Ok(()) => {
                                            new_name.set(String::new());
                                            status.set("Project created.".into());
                                            tick.set(tick() + 1);
                                            navigator.push(Route::Project { id: project.id });
                                        }
                                        Err(err) => status.set(err.to_string()),
                                    }
                                }
                            }
                        },
                        "Create"
                    }
                }

                if !status().is_empty() {
                    p { class: "status-line", "{status}" }
                }

                if projects().is_empty() {
                    div { class: "empty-state",
                        h2 { "No projects yet" }
                        p { "Create a project to start adding picks and building lift reports." }
                    }
                } else {
                    div { class: "project-grid",
                        for (project, count) in projects() {
                            {
                                let id = project.id;
                                let name = project.name.clone();
                                let notes = project.notes.clone();
                                let updated = format_updated(project.updated_at);
                                rsx! {
                                    article { key: "{id}", class: "project-card",
                                        button {
                                            class: "project-card-main",
                                            onclick: move |_| {
                                                navigator.push(Route::Project { id });
                                            },
                                            h2 { "{name}" }
                                            p { class: "muted",
                                                if notes.is_empty() {
                                                    "{count} pick(s) · {updated}"
                                                } else {
                                                    "{notes} · {count} pick(s)"
                                                }
                                            }
                                        }
                                        button {
                                            class: "btn btn-ghost danger",
                                            onclick: {
                                                let ctx = ctx.clone();
                                                move |_| {
                                                    if let Some(ref s) = ctx.store {
                                                        let _ = s.delete_project(id);
                                                        status.set("Project deleted.".into());
                                                        tick.set(tick() + 1);
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
