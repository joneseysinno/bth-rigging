//! Projects nav pane: job folders, and the picks, rigs and mat analyses in
//! each. Clicking an item opens it as a document.

use std::collections::HashSet;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{REVISION, bump_revision, store};
use crate::docs;
use bth_rigging::domain::{MatAnalysis, Pick, Project};
use bth_rigging::format::{format_lbs, format_updated};
use bth_rigging::report::{calc_package_filename, render_project_calc_package};
use bth_rigging::rig;
use bth_rigging::store::rig::RigHeaderView;

/// Projects expanded in the tree; kept while the pane is closed.
static EXPANDED: GlobalSignal<HashSet<Uuid>> = Signal::global(HashSet::new);

#[allow(non_snake_case)]
pub fn ProjectsPane() -> Element {
    let shell = use_shell();
    let mut projects = use_signal(Vec::<(Project, usize)>::new);
    let mut new_name = use_signal(String::new);
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
                        let count = s.pick_count(p.id).unwrap_or(0);
                        (p, count)
                    })
                    .collect();
                projects.set(rows);
            }
            Err(e) => status.set(e.to_string()),
        }
    });

    let mut create = move || {
        let name = new_name().trim().to_string();
        if name.is_empty() {
            status.set("Enter a project name.".into());
            return;
        }
        let Some(s) = store() else { return };
        let project = Project::new(name);
        match s.save_project(&project) {
            Ok(()) => {
                new_name.set(String::new());
                status.set(String::new());
                EXPANDED.write().insert(project.id);
                bump_revision();
            }
            Err(err) => status.set(err.to_string()),
        }
    };

    rsx! {
        div { class: "app-pane",
            div { class: "app-pane-create",
                input {
                    class: "field-input",
                    placeholder: "New project name",
                    "aria-label": "New project name",
                    value: "{new_name}",
                    oninput: move |e| new_name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            create();
                        }
                    },
                }
                button {
                    class: "btn btn-primary sm",
                    onclick: move |_| create(),
                    "Create"
                }
            }
            if !status().is_empty() {
                p { class: "status-line app-pad", "{status}" }
            }
            if projects().is_empty() {
                p { class: "muted app-pad", "No projects yet. Create one to start adding picks." }
            }
            ul { class: "app-tree", role: "tree",
                for (project, count) in projects() {
                    {
                        let id = project.id;
                        let name = project.name.clone();
                        let open = EXPANDED.read().contains(&id);
                        let detail = if project.notes.is_empty() {
                            format!("{count} pick(s) · {}", format_updated(project.updated_at))
                        } else {
                            format!("{} · {count} pick(s)", project.notes)
                        };
                        rsx! {
                            li { key: "{id}", role: "treeitem", "aria-expanded": if open { "true" } else { "false" },
                                div { class: "app-tree-row app-project-row",
                                    button {
                                        class: "as-row",
                                        title: "{detail}",
                                        onclick: move |_| {
                                            let mut expanded = EXPANDED.write();
                                            if !expanded.remove(&id) {
                                                expanded.insert(id);
                                            }
                                        },
                                        Ico { icon: if open { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT }, size: 14 }
                                        Ico { icon: icons::FOLDER, size: 15 }
                                        span { class: "as-ellipsis", "{name}" }
                                    }
                                    button {
                                        class: "as-icon-btn as-sm app-row-x",
                                        title: "Delete project",
                                        "aria-label": "Delete project {name}",
                                        onclick: move |_| {
                                            let Some(s) = store() else { return };
                                            for doc_id in project_record_ids(id) {
                                                docs::discard(shell, doc_id);
                                            }
                                            match s.delete_project(id) {
                                                Ok(()) => status.set("Project deleted.".into()),
                                                Err(e) => status.set(e.to_string()),
                                            }
                                            EXPANDED.write().remove(&id);
                                            bump_revision();
                                        },
                                        Ico { icon: icons::CLOSE, size: 13 }
                                    }
                                }
                                if open {
                                    ProjectItems { id, name: name.clone(), status }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
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
    ids.extend(s.list_rigs_for_project(project_id).unwrap_or_default().into_iter().map(|r| r.id));
    ids.extend(
        s.list_mat_analyses_for_project(project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.id),
    );
    ids
}

#[component]
fn ProjectItems(id: Uuid, name: String, status: Signal<String>) -> Element {
    let shell = use_shell();
    let mut picks = use_signal(Vec::<Pick>::new);
    let mut rigs = use_signal(Vec::<RigHeaderView>::new);
    let mut mat_analyses = use_signal(Vec::<MatAnalysis>::new);
    let mut selected_pick = use_signal(String::new);

    use_effect(move || {
        let _ = REVISION();
        if let Some(s) = store() {
            picks.set(s.list_picks_for_project(id).unwrap_or_default());
            rigs.set(s.list_rigs_for_project(id).unwrap_or_default());
            mat_analyses.set(s.list_mat_analyses_for_project(id).unwrap_or_default());
        }
    });

    rsx! {
        div { class: "app-tree-children",
            div { class: "app-tree-actions",
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
                button {
                    class: "btn btn-ghost sm",
                    title: "Print the project calc package to PDF",
                    disabled: picks().is_empty() && mat_analyses().is_empty(),
                    onclick: {
                        let project_name = name.clone();
                        move |_| print_calc_package(id, &project_name, status)
                    },
                    "Print package"
                }
            }

            h3 { class: "app-tree-heading", "Picks" }
            if picks().is_empty() {
                p { class: "muted app-tree-empty", "No picks yet." }
            }
            for pick in picks() {
                {
                    let pick_id = pick.id;
                    let pick_name = pick.name.clone();
                    rsx! {
                        div { key: "{pick_id}", class: "app-tree-row",
                            button {
                                class: "as-row",
                                onclick: {
                                    let pick_name = pick_name.clone();
                                    move |_| docs::open(shell, docs::pick::ID, pick_id, pick_name.clone())
                                },
                                Ico { icon: icons::LAYERS, size: 14 }
                                span { class: "as-ellipsis", "{pick_name}" }
                                span { class: "muted app-tree-meta", "{format_lbs(pick.weight_lbs)} lb" }
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

            h3 { class: "app-tree-heading", "Rigs" }
            div { class: "app-tree-actions",
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
            if rigs().is_empty() {
                p { class: "muted app-tree-empty", "No rigs yet." }
            }
            for header in rigs() {
                {
                    let rig_id = header.id;
                    let rig_name = header.name.clone();
                    rsx! {
                        div { key: "{rig_id}", class: "app-tree-row",
                            button {
                                class: "as-row",
                                onclick: {
                                    let rig_name = rig_name.clone();
                                    move |_| docs::open(shell, docs::rig::ID, rig_id, rig_name.clone())
                                },
                                Ico { icon: icons::TREE, size: 14 }
                                span { class: "as-ellipsis", "{rig_name}" }
                            }
                        }
                    }
                }
            }

            h3 { class: "app-tree-heading", "Mat analyses" }
            if mat_analyses().is_empty() {
                p { class: "muted app-tree-empty", "No mat analyses yet." }
            }
            for analysis in mat_analyses() {
                {
                    let analysis_id = analysis.id;
                    let analysis_name = analysis.name.clone();
                    rsx! {
                        div { key: "{analysis_id}", class: "app-tree-row",
                            button {
                                class: "as-row",
                                onclick: {
                                    let analysis_name = analysis_name.clone();
                                    move |_| docs::open(shell, docs::mat::ID, analysis_id, analysis_name.clone())
                                },
                                Ico { icon: icons::SCALE, size: 14 }
                                span { class: "as-ellipsis", "{analysis_name}" }
                                span { class: "muted app-tree-meta", "{format_lbs(analysis.outrigger_load_lbs)} lb" }
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
