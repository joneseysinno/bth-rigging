//! Project page: job details, and the picks, rigs and mat analyses in the job.
//! Clicking an item opens it as a document.

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{REVISION, bump_revision, store};
use crate::docs::{self, LoadError, NotARecord, Registry, root_signal};
use bth_rigging::domain::{MatAnalysis, Pick};
use bth_rigging::format::{format_lbs, format_updated};
use bth_rigging::report::{calc_package_filename, render_project_calc_package};
use bth_rigging::rig;
use bth_rigging::store::rig::RigHeaderView;

pub const ID: &str = "project";

pub fn doc_type() -> DocType {
    let mut t = DocType::new(ID, "Project", icons::FOLDER, Page);
    t.toolbar = vec![ToolGroup::new(
        "Project",
        vec![
            Tool::new("prj.save", "Save details", icons::DOWNLOAD).primary(),
            Tool::new("prj.print", "Print package", icons::FILES),
            Tool::new("prj.delete", "Delete project", icons::CLOSE),
        ],
    )];
    t
}

/// Editable job details, shared by the page and the toolbar commands.
#[derive(Clone, Copy, PartialEq)]
pub struct ProjectState {
    pub name: Signal<String>,
    pub job_number: Signal<String>,
    pub client: Signal<String>,
    pub location: Signal<String>,
    pub notes: Signal<String>,
    pub updated_at: Signal<u64>,
    pub status: Signal<String>,
    pub confirm_delete: Signal<bool>,
}

thread_local! {
    static PROJECTS: Registry<ProjectState> = Registry::new();
}

fn state(project_id: Uuid) -> Result<ProjectState, String> {
    PROJECTS.with(|projects| projects.get_or_load(project_id, || load(project_id)))
}

pub fn forget(project_id: Uuid) {
    PROJECTS.with(|projects| projects.forget(project_id));
}

fn load(project_id: Uuid) -> Result<ProjectState, String> {
    let s = store().ok_or("Database not available.")?;
    let p = s
        .load_project(project_id)
        .map_err(|e| e.to_string())?
        .ok_or("Project not found.")?;
    Ok(ProjectState {
        name: root_signal(p.name),
        job_number: root_signal(p.job_number),
        client: root_signal(p.client),
        location: root_signal(p.location),
        notes: root_signal(p.notes),
        updated_at: root_signal(p.updated_at),
        status: root_signal(String::new()),
        confirm_delete: root_signal(false),
    })
}

/// Command handler for this document type.
pub fn handle(shell: Shell, project_id: Uuid, id: &'static str) {
    let Ok(st) = state(project_id) else { return };
    let mut confirm_delete = st.confirm_delete;
    match id {
        "prj.save" => save(shell, project_id, st),
        "prj.print" => print_calc_package(project_id, &(st.name)(), st.status),
        "prj.delete" => confirm_delete.set(true),
        _ => {}
    }
}

fn save(shell: Shell, project_id: Uuid, st: ProjectState) {
    let mut status = st.status;
    let mut updated_at = st.updated_at;
    let name = (st.name)().trim().to_string();
    if name.is_empty() {
        status.set("Enter a project name.".into());
        return;
    }
    let Some(s) = store() else {
        status.set("Database not available.".into());
        return;
    };
    let mut project = match s.load_project(project_id) {
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
    project.name = name;
    project.job_number = (st.job_number)().trim().to_string();
    project.client = (st.client)().trim().to_string();
    project.location = (st.location)().trim().to_string();
    project.notes = (st.notes)().trim().to_string();
    project.touch();
    match s.save_project(&project) {
        Ok(()) => {
            updated_at.set(project.updated_at);
            status.set("Project saved.".into());
            docs::rename(shell, &project_id.to_string(), &project.name);
            bump_revision();
        }
        Err(e) => status.set(format!("Save failed: {e}")),
    }
}

fn delete(shell: Shell, project_id: Uuid, mut status: Signal<String>) {
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
    docs::projects::open_home(shell);
    docs::discard(shell, project_id);
    bump_revision();
}

/// Every record id in a project, so their documents can be closed on delete.
fn project_record_ids(project_id: Uuid) -> Vec<Uuid> {
    let Some(s) = store() else { return Vec::new() };
    let mut ids: Vec<Uuid> = s
        .list_picks_for_project(project_id)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.id)
        .collect();
    ids.extend(
        s.list_rigs_for_project(project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.id),
    );
    ids.extend(
        s.list_mat_analyses_for_project(project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.id),
    );
    ids
}

#[allow(non_snake_case)]
fn Page() -> Element {
    let Some(id) = docs::use_record_id() else {
        return rsx! { NotARecord { label: "project" } };
    };
    match state(id) {
        Ok(st) => rsx! { ProjectPage { id, st } },
        Err(message) => rsx! { LoadError { message } },
    }
}

#[component]
fn ProjectPage(id: Uuid, st: ProjectState) -> Element {
    let shell = use_shell();
    let mut picks = use_signal(Vec::<Pick>::new);
    let mut rigs = use_signal(Vec::<RigHeaderView>::new);
    let mut mat_analyses = use_signal(Vec::<MatAnalysis>::new);
    let mut selected_pick = use_signal(String::new);
    let mut status = st.status;
    let mut confirm_delete = st.confirm_delete;
    let ProjectState {
        mut name,
        mut job_number,
        mut client,
        mut location,
        mut notes,
        ..
    } = st;

    use_effect(move || {
        let _ = REVISION();
        if let Some(s) = store() {
            picks.set(s.list_picks_for_project(id).unwrap_or_default());
            rigs.set(s.list_rigs_for_project(id).unwrap_or_default());
            mat_analyses.set(s.list_mat_analyses_for_project(id).unwrap_or_default());
        }
    });

    let meta: Vec<String> = [job_number(), client(), location()]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    let meta = meta.join(" · ");

    rsx! {
        div { class: "app-home app-project-page",
            header { class: "app-home-header",
                div {
                    button {
                        class: "app-crumb",
                        onclick: move |_| docs::projects::open_home(shell),
                        "← All projects"
                    }
                    h1 { class: "app-home-title", "{name}" }
                    if !meta.is_empty() {
                        p { class: "muted", "{meta}" }
                    }
                    p { class: "muted", "Updated {format_updated((st.updated_at)())}" }
                }
                div { class: "app-home-actions",
                    button {
                        class: "btn btn-ghost",
                        title: "Print the project calc package to PDF",
                        disabled: picks().is_empty() && mat_analyses().is_empty(),
                        onclick: move |_| print_calc_package(id, &name(), status),
                        "Print package"
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| save(shell, id, st),
                        "Save details"
                    }
                }
            }

            if confirm_delete() {
                div { class: "app-confirm", role: "alertdialog",
                    p {
                        strong { "Delete {name}?" }
                        " This also deletes its picks, rigs and mat analyses. It cannot be undone."
                    }
                    div { class: "app-project-form-actions",
                        button {
                            class: "btn btn-ghost",
                            onclick: move |_| confirm_delete.set(false),
                            "Cancel"
                        }
                        button {
                            class: "btn btn-ghost danger",
                            onclick: move |_| {
                                confirm_delete.set(false);
                                delete(shell, id, status);
                            },
                            "Delete project"
                        }
                    }
                }
            }
            if !status().is_empty() {
                p { class: "status-line", "{status}" }
            }

            section { class: "app-project-section",
                h2 { "Job details" }
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
                            rows: 3,
                            value: "{notes}",
                            oninput: move |e| notes.set(e.value()),
                        }
                    }
                }
            }

            section { class: "app-project-section",
                div { class: "app-section-head",
                    h2 { "Picks" }
                    button {
                        class: "btn btn-ghost sm",
                        onclick: move |_| {
                            let Some(s) = store() else { return };
                            let pick = Pick::new(id, "New Pick", 10_000.0);
                            match s.save_pick(&pick, &[]) {
                                Ok(()) => {
                                    bump_revision();
                                    docs::open(shell, docs::pick::ID, pick.id, pick.name);
                                }
                                Err(e) => status.set(e.to_string()),
                            }
                        },
                        "+ Pick"
                    }
                }
                if picks().is_empty() {
                    p { class: "muted", "No picks yet." }
                }
                ul { class: "app-record-list",
                    for pick in picks() {
                        {
                            let pick_id = pick.id;
                            let pick_name = pick.name.clone();
                            rsx! {
                                li { key: "{pick_id}", class: "app-record-row",
                                    button {
                                        class: "app-record-open",
                                        onclick: {
                                            let pick_name = pick_name.clone();
                                            move |_| docs::open(shell, docs::pick::ID, pick_id, pick_name.clone())
                                        },
                                        Ico { icon: icons::LAYERS, size: 15 }
                                        span { class: "as-ellipsis", "{pick_name}" }
                                        span { class: "muted app-record-meta", "{format_lbs(pick.weight_lbs)} lb" }
                                    }
                                    button {
                                        class: "as-icon-btn as-sm app-row-x",
                                        title: "Delete pick",
                                        "aria-label": "Delete pick {pick_name}",
                                        onclick: move |_| {
                                            let Some(s) = store() else { return };
                                            docs::discard(shell, pick_id);
                                            if let Err(e) = s.delete_pick(pick_id) {
                                                status.set(e.to_string());
                                            }
                                            bump_revision();
                                        },
                                        Ico { icon: icons::CLOSE, size: 13 }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            section { class: "app-project-section",
                div { class: "app-section-head",
                    h2 { "Rigs" }
                    div { class: "app-section-actions",
                        select {
                            class: "field-input sm",
                            "aria-label": "Pick to build a rig from",
                            value: "{selected_pick}",
                            onchange: move |event| selected_pick.set(event.value()),
                            option { value: "", "Rig from pick…" }
                            for pick in picks() {
                                option { value: "{pick.id}", "{pick.name}" }
                            }
                        }
                        button {
                            class: "btn btn-ghost sm",
                            disabled: selected_pick().is_empty(),
                            onclick: move |_| {
                                let Ok(pick_id) = Uuid::parse_str(&selected_pick()) else {
                                    status.set("Select a pick first.".into());
                                    return;
                                };
                                match rig_from_pick(id, pick_id) {
                                    Ok((rig_id, rig_name)) => {
                                        selected_pick.set(String::new());
                                        bump_revision();
                                        docs::open(shell, docs::rig::ID, rig_id, rig_name);
                                    }
                                    Err(e) => status.set(e),
                                }
                            },
                            "Create"
                        }
                        {debug_demo_rig_button(shell, id, status)}
                    }
                }
                if rigs().is_empty() {
                    p { class: "muted", "No rigs yet." }
                }
                ul { class: "app-record-list",
                    for header in rigs() {
                        {
                            let rig_id = header.id;
                            let rig_name = header.name.clone();
                            rsx! {
                                li { key: "{rig_id}", class: "app-record-row",
                                    button {
                                        class: "app-record-open",
                                        onclick: {
                                            let rig_name = rig_name.clone();
                                            move |_| docs::open(shell, docs::rig::ID, rig_id, rig_name.clone())
                                        },
                                        Ico { icon: icons::TREE, size: 15 }
                                        span { class: "as-ellipsis", "{rig_name}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            section { class: "app-project-section",
                div { class: "app-section-head",
                    h2 { "Mat analyses" }
                    button {
                        class: "btn btn-ghost sm",
                        onclick: move |_| {
                            let Some(s) = store() else { return };
                            let analysis = MatAnalysis::new(id, "New mat analysis", Uuid::nil());
                            match s.save_mat_analysis(&analysis) {
                                Ok(()) => {
                                    bump_revision();
                                    docs::open(shell, docs::mat::ID, analysis.id, analysis.name);
                                }
                                Err(e) => status.set(e.to_string()),
                            }
                        },
                        "+ Mat analysis"
                    }
                }
                if mat_analyses().is_empty() {
                    p { class: "muted", "No mat analyses yet." }
                }
                ul { class: "app-record-list",
                    for analysis in mat_analyses() {
                        {
                            let analysis_id = analysis.id;
                            let analysis_name = analysis.name.clone();
                            rsx! {
                                li { key: "{analysis_id}", class: "app-record-row",
                                    button {
                                        class: "app-record-open",
                                        onclick: {
                                            let analysis_name = analysis_name.clone();
                                            move |_| docs::open(shell, docs::mat::ID, analysis_id, analysis_name.clone())
                                        },
                                        Ico { icon: icons::SCALE, size: 15 }
                                        span { class: "as-ellipsis", "{analysis_name}" }
                                        span { class: "muted app-record-meta", "{format_lbs(analysis.outrigger_load_lbs)} lb" }
                                    }
                                    button {
                                        class: "as-icon-btn as-sm app-row-x",
                                        title: "Delete mat analysis",
                                        "aria-label": "Delete mat analysis {analysis_name}",
                                        onclick: move |_| {
                                            let Some(s) = store() else { return };
                                            docs::discard(shell, analysis_id);
                                            if let Err(e) = s.delete_mat_analysis(analysis_id) {
                                                status.set(e.to_string());
                                            }
                                            bump_revision();
                                        },
                                        Ico { icon: icons::CLOSE, size: 13 }
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

/// Build and save a rig from a pick's layers. Returns the new rig's id and name.
fn rig_from_pick(project_id: Uuid, pick_id: Uuid) -> Result<(Uuid, String), String> {
    let store = store().ok_or("Database not available.")?;
    let (pick, pick_layers) = store
        .load_pick(pick_id)
        .map_err(|e| e.to_string())?
        .ok_or("Pick no longer exists.")?;
    let spreaders = store.list_spreaders().map_err(|e| e.to_string())?;
    let mut rig = rig::from_layers(&pick, &pick_layers, &spreaders);
    rig.project_id = project_id;
    store.save_rig(&rig).map_err(|e| e.to_string())?;
    Ok((rig.id, rig.name))
}

fn print_calc_package(project_id: Uuid, project_name: &str, mut status: Signal<String>) {
    let Some(s) = store() else {
        status.set("Database not available.".into());
        return;
    };
    let default_name = calc_package_filename(project_name);
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .add_filter("PDF", &["pdf"])
        .save_file()
    else {
        status.set("Save cancelled.".into());
        return;
    };
    match render_project_calc_package(&s, project_id) {
        Ok(bytes) => match std::fs::write(&path, bytes) {
            Ok(()) => status.set("Saved calc package.".into()),
            Err(e) => status.set(format!("Save failed: {e}")),
        },
        Err(e) => status.set(e.to_string()),
    }
}

#[cfg(debug_assertions)]
fn debug_demo_rig_button(shell: Shell, project_id: Uuid, mut status: Signal<String>) -> Element {
    rsx! {
        button {
            class: "btn btn-ghost sm",
            title: "Add the Duplo10 demo rig (debug builds only)",
            onclick: move |_| {
                let Some(store) = store() else {
                    status.set("Database not available.".into());
                    return;
                };
                let mut rig = rig::duplo10();
                rig.project_id = project_id;
                match store.save_rig(&rig) {
                    Ok(()) => {
                        bump_revision();
                        docs::open(shell, docs::rig::ID, rig.id, rig.name);
                    }
                    Err(error) => status.set(error.to_string()),
                }
            },
            "Demo"
        }
    }
}

#[cfg(not(debug_assertions))]
fn debug_demo_rig_button(_shell: Shell, _project_id: Uuid, _status: Signal<String>) -> Element {
    rsx! {}
}
