//! Projects home: every project as a card, plus the "New project" form.
//! Opening a card shows that project in the nav pane (`panes::project`).

use app_shell::prelude::*;
use dioxus::prelude::*;

use crate::app::{CURRENT_PROJECT, REVISION, bump_revision, store};
use crate::panes;
use bth_rigging::domain::Project;
use bth_rigging::format::format_updated;
use bth_rigging::store::ProjectCounts;

pub const ID: &str = "projects";
/// Document id of the home tab.
pub const HOME_ID: &str = "home";

pub fn doc_type() -> DocType {
    DocType::new(ID, "Projects", icons::HOME, Home)
}

pub fn home_document() -> Document {
    Document::new(HOME_ID, "Projects", ID)
}

/// Reopen the home tab if it was closed, and switch to it.
pub fn open_home(mut shell: Shell) {
    let is_open = shell.documents.peek().iter().any(|d| d.id == HOME_ID);
    if !is_open {
        shell.documents.write().insert(0, home_document());
    }
    shell.activate(HOME_ID);
}

#[allow(non_snake_case)]
fn Home() -> Element {
    let shell = use_shell();
    let mut projects = use_signal(Vec::<(Project, ProjectCounts)>::new);
    let mut query = use_signal(String::new);
    let mut show_form = use_signal(|| false);
    let mut status = use_signal(String::new);

    use_effect(move || {
        let _ = REVISION();
        let Some(s) = store() else {
            status.set("Database unavailable.".into());
            return;
        };
        match s.list_projects() {
            Ok(list) => {
                let rows = list
                    .into_iter()
                    .map(|p| {
                        let counts = s.project_counts(p.id).unwrap_or_default();
                        (p, counts)
                    })
                    .collect();
                projects.set(rows);
            }
            Err(e) => status.set(e.to_string()),
        }
    });

    let total = projects.read().len();
    let needle = query().trim().to_lowercase();
    let shown: Vec<(Project, ProjectCounts)> = projects()
        .into_iter()
        .filter(|(p, _)| matches(p, &needle))
        .collect();

    rsx! {
        div { class: "app-home",
            header { class: "app-home-header",
                div {
                    p { class: "report-brand", "BTH Rigging" }
                    h1 { class: "app-home-title", "Projects" }
                    p { class: "muted",
                        if total == 1 { "1 project" } else { "{total} projects" }
                    }
                }
                div { class: "app-home-actions",
                    input {
                        class: "field-input",
                        r#type: "search",
                        placeholder: "Search name, job #, client, location",
                        "aria-label": "Search projects",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| show_form.set(!show_form()),
                        if show_form() { "Cancel" } else { "+ New project" }
                    }
                }
            }
            if show_form() {
                NewProjectForm { shell, show_form, status }
            }
            if !status().is_empty() {
                p { class: "status-line", "{status}" }
            }
            if total == 0 {
                div { class: "app-empty",
                    h2 { "No projects yet" }
                    p { "Create a project for each job. Picks, rigs and mat analyses live inside it." }
                    if !show_form() {
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| show_form.set(true),
                            "+ New project"
                        }
                    }
                }
            } else if shown.is_empty() {
                p { class: "muted", "No projects match the search." }
            }
            div { class: "app-project-grid",
                for (project, counts) in shown {
                    ProjectCard { key: "{project.id}", shell, project, counts }
                }
            }
        }
    }
}

fn matches(project: &Project, needle: &str) -> bool {
    needle.is_empty()
        || [
            &project.name,
            &project.job_number,
            &project.client,
            &project.location,
        ]
        .iter()
        .any(|field| field.to_lowercase().contains(needle))
}

#[component]
fn ProjectCard(shell: Shell, project: Project, counts: ProjectCounts) -> Element {
    let id = project.id;
    let is_open = CURRENT_PROJECT() == Some(id);
    let picks = plural(counts.picks, "pick", "picks");
    let rigs = plural(counts.rigs, "rig", "rigs");
    let mats = plural(counts.mats, "mat analysis", "mat analyses");
    rsx! {
        button {
            class: if is_open { "app-project-card is-open" } else { "app-project-card" },
            "aria-current": if is_open { Some("true") } else { None },
            onclick: move |_| panes::open_project(shell, id),
            div { class: "app-project-card-head",
                Ico { icon: icons::FOLDER, size: 18 }
                h3 { class: "as-ellipsis", "{project.name}" }
                if !project.job_number.is_empty() {
                    span { class: "app-job-tag", "#{project.job_number}" }
                }
            }
            if !project.client.is_empty() {
                p { class: "app-project-card-line", "{project.client}" }
            }
            if !project.location.is_empty() {
                p { class: "app-project-card-line muted", "{project.location}" }
            }
            div { class: "app-project-card-counts",
                span { "{picks}" }
                span { "{rigs}" }
                span { "{mats}" }
            }
            p { class: "app-project-card-updated muted", "Updated {format_updated(project.updated_at)}" }
        }
    }
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

#[component]
fn NewProjectForm(shell: Shell, show_form: Signal<bool>, status: Signal<String>) -> Element {
    let mut name = use_signal(String::new);
    let mut job_number = use_signal(String::new);
    let mut client = use_signal(String::new);
    let mut location = use_signal(String::new);
    let mut notes = use_signal(String::new);

    let mut create = move || {
        let project_name = name().trim().to_string();
        if project_name.is_empty() {
            status.set("Enter a project name.".into());
            return;
        }
        let Some(s) = store() else {
            status.set("Database not available.".into());
            return;
        };
        let mut project = Project::new(project_name);
        project.job_number = job_number().trim().to_string();
        project.client = client().trim().to_string();
        project.location = location().trim().to_string();
        project.notes = notes().trim().to_string();
        match s.save_project(&project) {
            Ok(()) => {
                status.set(String::new());
                show_form.set(false);
                bump_revision();
                panes::open_project(shell, project.id);
            }
            Err(e) => status.set(e.to_string()),
        }
    };

    rsx! {
        form {
            class: "app-project-form",
            onsubmit: move |e| {
                e.prevent_default();
                create();
            },
            h2 { "New project" }
            div { class: "field-grid",
                label { class: "field span-2",
                    span { class: "field-label", "Project name *" }
                    input {
                        class: "field-input",
                        autofocus: true,
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
                        rows: 3,
                        value: "{notes}",
                        oninput: move |e| notes.set(e.value()),
                    }
                }
            }
            div { class: "app-project-form-actions",
                button {
                    r#type: "button",
                    class: "btn btn-ghost",
                    onclick: move |_| show_form.set(false),
                    "Cancel"
                }
                button { r#type: "submit", class: "btn btn-primary", "Create project" }
            }
        }
    }
}
