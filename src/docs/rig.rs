//! Rig document: the rig diagram (node-graph builder) and 3D rig tree in the
//! viewport, the inspector and live results on the right, checks on the left
//! and the parameter table in the bottom panel.

mod checks;
mod inspector;
mod model;
mod params;
mod results;
mod state;

use app_shell::prelude::*;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::{bump_revision, store};
use crate::docs;
use crate::ui::selection::{Armed, Selection};
use bth_rigging::checks::Status;
use bth_rigging::format::format_lbs;
use bth_rigging::rig::preset::{self, BodyPreset, HardwarePreset, PickPattern};
use bth_rigging::rig::topo::TopoEdit;
pub use state::forget;
use state::{ViewMode, state};

pub const ID: &str = "rig";
const PARAMS_PANEL: &str = "rig.params";
const INSPECTOR_PANEL: &str = "rig.inspector";

/// Toolbar command → body preset.
const ADD_BODY: [(&str, BodyPreset); 5] = [
    ("rig.add.bar", BodyPreset::SpreaderBar),
    ("rig.add.beam", BodyPreset::LiftingBeam),
    (
        "rig.add.load2",
        BodyPreset::Load {
            pattern: PickPattern::Two,
        },
    ),
    (
        "rig.add.load4",
        BodyPreset::Load {
            pattern: PickPattern::FourCorners,
        },
    ),
    ("rig.add.frame", BodyPreset::Frame),
];

/// Toolbar command → hardware preset.
const ADD_HARDWARE: [(&str, HardwarePreset); 7] = [
    ("rig.hw.shackle", HardwarePreset::Shackle),
    ("rig.hw.sling", HardwarePreset::RoundSling),
    ("rig.hw.strap", HardwarePreset::Strap),
    ("rig.hw.chain", HardwarePreset::Chain),
    ("rig.hw.rope", HardwarePreset::WireRope),
    ("rig.hw.link", HardwarePreset::MasterLink),
    ("rig.hw.turnbuckle", HardwarePreset::Turnbuckle),
];

pub fn doc_type() -> DocType {
    let mut t = DocType::new(ID, "Rig", icons::TREE, model::Model);
    t.toolbar = vec![
        ToolGroup::new(
            "Rig",
            vec![Tool::new("rig.save", "Save rig", icons::DOWNLOAD).primary()],
        ),
        ToolGroup::new(
            "Edit",
            vec![
                Tool::new("rig.undo", "Undo", icons::UNDO),
                Tool::new("rig.redo", "Redo", icons::REDO),
                Tool::new("rig.delete", "Delete selection", icons::CLOSE),
            ],
        ),
        ToolGroup::new(
            "Insert",
            vec![
                Tool::new("rig.add.bar", "Add spreader bar", icons::RULER),
                Tool::new("rig.add.beam", "Add lifting beam", icons::SCALE),
                Tool::new("rig.add.load2", "Add load (2 pick points)", icons::CUBE),
                Tool::new("rig.add.load4", "Add load (4 corners)", icons::PATTERN),
                Tool::new("rig.add.frame", "Add frame / tare", icons::GRID),
                Tool::new("rig.add.knot", "Add master link (junction)", icons::TARGET),
            ],
        ),
        ToolGroup::new(
            "Hardware",
            vec![
                Tool::new("rig.hw.shackle", "Insert shackle", icons::LINK),
                Tool::new("rig.hw.sling", "Insert roundsling", icons::REVOLVE),
                Tool::new("rig.hw.strap", "Insert strap", icons::LAYERS),
                Tool::new("rig.hw.chain", "Insert chain", icons::EXTRUDE),
                Tool::new("rig.hw.rope", "Insert wire rope", icons::FX),
                Tool::new("rig.hw.link", "Insert master link", icons::DOT),
                Tool::new("rig.hw.turnbuckle", "Insert turnbuckle", icons::SLIDERS),
            ],
        ),
        ToolGroup::new(
            "View",
            vec![
                Tool::new("rig.view.diagram", "Show the rig diagram", icons::TREE),
                Tool::new("rig.view.3d", "Show the 3D view", icons::EYE),
                Tool::new(
                    "rig.view.split",
                    "Show diagram and 3D side by side",
                    icons::MIRROR,
                ),
            ],
        ),
    ];
    t.left_rail = vec![RailItem::panel(
        "rig.checks",
        "Checks",
        icons::INFO,
        checks::Checks,
    )];
    t.right_rail = vec![
        RailItem::panel(
            INSPECTOR_PANEL,
            "Inspector",
            icons::PENCIL,
            inspector::Inspector,
        ),
        RailItem::panel(
            "rig.results",
            "Live results",
            icons::TARGET,
            results::Results,
        ),
    ];
    t.bottom_rail = vec![RailItem::panel(
        PARAMS_PANEL,
        "Parameters",
        icons::TABLE,
        params::Params,
    )];
    t.default_right = Some(INSPECTOR_PANEL);
    t.status = Some(View(StatusReadout));
    t
}

/// Command handler for this document type.
pub fn handle(shell: Shell, rig_id: Uuid, id: &'static str) {
    let Ok(st) = state(rig_id) else { return };
    let mut mode = st.view_mode;
    let mut armed = st.armed;
    let show_diagram = |mut mode: Signal<ViewMode>| {
        if *mode.peek() == ViewMode::ThreeD {
            mode.set(ViewMode::Split);
        }
    };
    if let Some((_, body)) = ADD_BODY.iter().find(|(cmd, _)| *cmd == id) {
        let edit = preset::add_body(st.session.peek().rig(), *body);
        let TopoEdit::AddBody { body, .. } = &edit else {
            return;
        };
        let select = Selection::Body(body.id);
        if st.edit(edit) {
            st.select(Some(select));
            show_diagram(mode);
        }
        return;
    }
    if let Some((_, hardware)) = ADD_HARDWARE.iter().find(|(cmd, _)| *cmd == id) {
        show_diagram(mode);
        let selected = *st.selected.peek();
        if let Some(Selection::Component {
            member,
            segment,
            index,
        }) = selected
        {
            let edit = TopoEdit::InsertComponent {
                member,
                segment,
                index: index + 1,
                component: hardware.component(),
            };
            if st.edit(edit) {
                st.select(Some(Selection::Component {
                    member,
                    segment,
                    index: index + 1,
                }));
            }
        } else {
            armed.set(Some(Armed::Hardware(*hardware)));
        }
        return;
    }
    match id {
        "rig.save" => save(shell, rig_id),
        "rig.undo" => st.undo(),
        "rig.redo" => st.redo(),
        "rig.delete" => {
            let selected = *st.selected.peek();
            let edit = selected.and_then(|s| s.delete_edit(st.session.peek().rig()));
            if let Some(edit) = edit {
                st.edit(edit);
            }
        }
        "rig.add.knot" => {
            let edit = preset::master_link(st.session.peek().rig());
            let TopoEdit::AddNode { node } = &edit else {
                return;
            };
            let select = Selection::Node(node.id);
            if st.edit(edit) {
                st.select(Some(select));
                show_diagram(mode);
            }
        }
        "rig.view.diagram" => mode.set(ViewMode::Diagram),
        "rig.view.3d" => mode.set(ViewMode::ThreeD),
        "rig.view.split" => mode.set(ViewMode::Split),
        _ => {}
    }
}

fn save(shell: Shell, rig_id: Uuid) {
    let Ok(st) = state(rig_id) else { return };
    let mut session = st.session;
    let mut status_message = st.status_message;
    let Some(store) = store() else {
        status_message.set("Database not available.".into());
        return;
    };
    let mut current = session.write();
    match store.save_rig(current.rig()) {
        Ok(()) => {
            current.mark_saved();
            let name = current.rig().name.clone();
            drop(current);
            status_message.set("Rig saved.".into());
            docs::rename(shell, &rig_id.to_string(), &name);
            bump_revision();
        }
        Err(error) => status_message.set(error.to_string()),
    }
}

fn status_class(status: Status) -> &'static str {
    match status {
        Status::Ok => "status-tag status-ok",
        Status::Warn => "status-tag status-warn",
        Status::Over => "status-tag status-over",
    }
}

#[allow(non_snake_case)]
fn StatusReadout() -> Element {
    let Some(rig_id) = docs::use_record_id() else {
        return rsx! {};
    };
    let Ok(st) = state(rig_id) else {
        return rsx! {};
    };
    let session = st.session.read();
    let dirty = session.is_dirty();
    let headline = session.snapshot().headline;
    let solved = matches!(session.snapshot().solved, Some(Ok(_)));
    rsx! {
        span {
            class: if dirty { "dirty-indicator is-dirty" } else { "dirty-indicator" },
            title: if dirty { "Unsaved changes" } else { "Saved" },
        }
        span { if dirty { "unsaved" } else { "saved" } }
        if solved || headline.hook_load_lbs > 0.0 {
            span { "hook {format_lbs(headline.hook_load_lbs)} lb" }
            span { class: status_class(headline.worst_status), "{headline.worst_status.label()}" }
        } else {
            span { class: "muted", "draft: not solved yet" }
        }
        if !(st.status_message)().is_empty() {
            span { "{st.status_message}" }
        }
    }
}
