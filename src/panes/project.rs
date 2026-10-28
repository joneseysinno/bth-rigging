//! Project nav pane: the picks, rigs and mat analyses in the open project.
//! Clicking an item opens it as a document.

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use super::NoProject;
use crate::app::{CURRENT_PROJECT, REVISION, bump_revision, store};
use crate::docs;
use bth_rigging::domain::{MatAnalysis, Pick};
use bth_rigging::format::format_lbs;
use bth_rigging::report::{calc_package_filename, render_project_calc_package};
use bth_rigging::rig;
use bth_rigging::store::rig::RigHeaderView;

#[allow(non_snake_case)]
pub fn ProjectPane() -> Element {
    let Some(id) = CURRENT_PROJECT() else {
        return rsx! { NoProject {} };
    };
    rsx! { ProjectTree { key: "{id}", id } }
}

#[component]
fn ProjectTree(id: Uuid) -> Element {
    let shell = use_shell();
    let mut project = use_signal(|| store().and_then(|s| s.load_project(id).ok().flatten()));
    let mut picks = use_signal(Vec::<Pick>::new);
    let mut rigs = use_signal(Vec::<RigHeaderView>::new);
    let mut mat_analyses = use_signal(Vec::<MatAnalysis>::new);
    let mut selected_pick = use_signal(String::new);
    let mut status = use_signal(String::new);

    use_effect(move || {
        let _ = REVISION();
        let Some(s) = store() else {
            status.set("Database unavailable.".into());
            return;
        };
        project.set(s.load_project(id).ok().flatten());
        picks.set(s.list_picks_for_project(id).unwrap_or_default());
        rigs.set(s.list_rigs_for_project(id).unwrap_or_default());
        mat_analyses.set(s.list_mat_analyses_for_project(id).unwrap_or_default());
    });

    let Some(current) = project() else {
        return rsx! { NoProject {} };
    };
    let name = current.name.clone();
    let meta: Vec<&str> = [current.job_number.as_str(), current.client.as_str()]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    let meta = meta.join(" · ");

    rsx! {
        div { class: "app-pane",
            header { class: "app-pane-header",
                button {
                    class: "app-crumb",
                    onclick: move |_| docs::projects::open_home(shell),
                    "← All projects"
                }
                div { class: "app-pane-title",
                    Ico { icon: icons::FOLDER, size: 15 }
                    strong { class: "as-ellipsis", title: "{name}", "{name}" }
                }
                if !meta.is_empty() {
                    p { class: "muted app-pane-meta", "{meta}" }
                }
            }
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
            if !status().is_empty() {
                p { class: "status-line app-pad", "{status}" }
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
                button {
                    class: "btn btn-ghost sm",
                    title: "Start a rig from a hook and a load, then build it on the diagram",
                    onclick: move |_| match blank_rig(id, rigs().len() + 1) {
                        Ok((rig_id, rig_name)) => {
                            bump_revision();
                            docs::open(shell, docs::rig::ID, rig_id, rig_name);
                        }
                        Err(e) => status.set(e),
                    },
                    "New blank rig"
                }
            }
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

/// Every record id in a project, so their documents can be closed on delete.
pub fn project_record_ids(project_id: Uuid) -> Vec<Uuid> {
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

/// Save a new rig holding a hook and an unrigged load. Returns its id and name.
fn blank_rig(project_id: Uuid, number: usize) -> Result<(Uuid, String), String> {
    let store = store().ok_or("Database not available.")?;
    let rig = rig::preset::blank_rig(&format!("Rig {number}"), project_id);
    store.save_rig(&rig).map_err(|e| e.to_string())?;
    Ok((rig.id, rig.name))
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
