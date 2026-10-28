//! Details nav pane: edit, save or delete the open project's job details.

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use super::NoProject;
use super::project::project_record_ids;
use crate::app::{CURRENT_PROJECT, bump_revision, store};
use crate::docs;
use bth_rigging::domain::Project;
use bth_rigging::format::format_updated;

#[allow(non_snake_case)]
pub fn DetailsPane() -> Element {
    let Some(id) = CURRENT_PROJECT() else {
        return rsx! { NoProject {} };
    };
    let Some(project) = store().and_then(|s| s.load_project(id).ok().flatten()) else {
        return rsx! { NoProject {} };
    };
    rsx! { DetailsForm { key: "{id}", project } }
}

/// The fields load once per project, so other saves don't wipe edits in progress.
#[component]
fn DetailsForm(project: Project) -> Element {
    let shell = use_shell();
    let id = project.id;
    let mut name = use_signal(|| project.name.clone());
    let mut job_number = use_signal(|| project.job_number.clone());
    let mut client = use_signal(|| project.client.clone());
    let mut location = use_signal(|| project.location.clone());
    let mut notes = use_signal(|| project.notes.clone());
    let mut updated_at = use_signal(|| project.updated_at);
    let mut status = use_signal(String::new);
    let mut confirm_delete = use_signal(|| false);

    let save = move |_| {
        let trimmed = name().trim().to_string();
        if trimmed.is_empty() {
            status.set("Enter a project name.".into());
            return;
        }
        let Some(s) = store() else {
            status.set("Database not available.".into());
            return;
        };
        let mut project = match s.load_project(id) {
            Ok(Some(p)) => p,
            Ok(None) => {
                status.set("Project no longer exists.".into());
                return;
            }
            Err(e) => {
                status.set(e.to_string());
                return;
            }
        };
        project.name = trimmed;
        project.job_number = job_number().trim().to_string();
        project.client = client().trim().to_string();
        project.location = location().trim().to_string();
        project.notes = notes().trim().to_string();
        project.touch();
        match s.save_project(&project) {
            Ok(()) => {
                updated_at.set(project.updated_at);
                status.set("Details saved.".into());
                bump_revision();
            }
            Err(e) => status.set(format!("Save failed: {e}")),
        }
    };

    rsx! {
        div { class: "app-panel-pad",
            div { class: "field-grid",
                label { class: "field span-2",
                    span { class: "field-label", "Project name" }
                    input {
                        class: "field-input",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                label { class: "field",
                    span { class: "field-label", "Job number" }
                    input {
                        class: "field-input",
                        value: "{job_number}",
                        oninput: move |e| job_number.set(e.value()),
                    }
                }
                label { class: "field",
                    span { class: "field-label", "Client" }
                    input {
                        class: "field-input",
                        value: "{client}",
                        oninput: move |e| client.set(e.value()),
                    }
                }
                label { class: "field span-2",
                    span { class: "field-label", "Site location" }
                    input {
                        class: "field-input",
                        value: "{location}",
                        oninput: move |e| location.set(e.value()),
                    }
                }
                label { class: "field span-2",
                    span { class: "field-label", "Notes" }
                    textarea {
                        class: "field-input",
                        rows: 4,
                        value: "{notes}",
                        oninput: move |e| notes.set(e.value()),
                    }
                }
            }
            p { class: "hint", "Updated {format_updated(updated_at())}" }
            div { class: "app-project-form-actions",
                button {
                    class: "btn btn-ghost danger sm",
                    onclick: move |_| confirm_delete.set(true),
                    "Delete project"
                }
                button { class: "btn btn-primary sm", onclick: save, "Save details" }
            }
            if !status().is_empty() {
                p { class: "status-line", "{status}" }
            }
            if confirm_delete() {
                div { class: "app-confirm", role: "alertdialog",
                    p {
                        strong { "Delete {name}?" }
                        " This also deletes its picks, rigs and mat analyses. It cannot be undone."
                    }
                    div { class: "app-project-form-actions",
                        button {
                            class: "btn btn-ghost sm",
                            onclick: move |_| confirm_delete.set(false),
                            "Cancel"
                        }
                        button {
                            class: "btn btn-ghost danger sm",
                            onclick: move |_| {
                                confirm_delete.set(false);
                                delete_project(shell, id, status);
                            },
                            "Delete project"
                        }
                    }
                }
            }
        }
    }
}

fn delete_project(shell: Shell, project_id: Uuid, mut status: Signal<String>) {
    let Some(s) = store() else {
        status.set("Database not available.".into());
        return;
    };
    for doc_id in project_record_ids(project_id) {
        docs::discard(shell, doc_id);
    }
    if let Err(e) = s.delete_project(project_id) {
        status.set(e.to_string());
        return;
    }
    *CURRENT_PROJECT.write() = None;
    docs::projects::open_home(shell);
    bump_revision();
}
