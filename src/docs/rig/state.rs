//! Per-rig editor session, shared by the model view, panels and status readout.

use dioxus::prelude::*;
use indexmap::IndexMap;
use uuid::Uuid;

use crate::app::store;
use crate::docs::{Registry, root_signal};
pub use crate::ui::selection::Armed;
use crate::ui::selection::Selection;
use bth_rigging::rig::gather::{GatherOptions, GatherPlan};
use bth_rigging::rig::layout::CanvasLayout;
use bth_rigging::rig::session::{HeadlineDelta, RigEdit, RigSession, Snapshot};
use bth_rigging::rig::solve::MemberForce;
use bth_rigging::rig::{MemberId, NodeId, Rig, RigError};

/// What the viewport shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Diagram,
    ThreeD,
    Split,
}

#[derive(Clone, Copy, PartialEq)]
pub struct RigState {
    pub session: Signal<RigSession>,
    pub status_message: Signal<String>,
    pub selected: Signal<Option<Selection>>,
    pub hovered: Signal<Option<Selection>>,
    pub view_mode: Signal<ViewMode>,
    pub armed: Signal<Option<Armed>>,
    pub layout: Signal<CanvasLayout>,
    /// Open points ticked for the next gather (Build panel and diagram).
    pub picked: Signal<Vec<NodeId>>,
    /// How gathers size what they create.
    pub gather_opts: Signal<GatherOptions>,
}

/// What the rig views draw, taken from the session's current snapshot.
pub struct RigView {
    pub rig: Rig,
    pub snapshot: Snapshot,
    pub delta: Option<HeadlineDelta>,
    pub dirty: bool,
}

impl RigView {
    pub fn forces(&self) -> IndexMap<MemberId, MemberForce> {
        self.snapshot
            .solved
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|solved| solved.members.clone())
            .unwrap_or_default()
    }

    pub fn solve_errors(&self) -> Option<Vec<RigError>> {
        self.snapshot
            .solved
            .as_ref()
            .and_then(|result| result.as_ref().err().cloned())
    }
}

thread_local! {
    static RIGS: Registry<RigState> = Registry::new();
}

pub fn state(rig_id: Uuid) -> Result<RigState, String> {
    RIGS.with(|rigs| rigs.get_or_load(rig_id, || load(rig_id)))
}

pub fn forget(rig_id: Uuid) {
    RIGS.with(|rigs| rigs.forget(rig_id));
}

fn load(rig_id: Uuid) -> Result<RigState, String> {
    let store = store().ok_or("Database not available.")?;
    let rig = store
        .load_rig(rig_id)
        .map_err(|e| e.to_string())?
        .ok_or("Rig not found.")?;
    let layout = store.load_rig_layout(rig_id).unwrap_or_default();
    let mode = if rig.members.is_empty() {
        ViewMode::Diagram
    } else {
        ViewMode::Split
    };
    Ok(RigState {
        session: root_signal(RigSession::new(rig)),
        status_message: root_signal(String::new()),
        selected: root_signal(None),
        hovered: root_signal(None),
        view_mode: root_signal(mode),
        armed: root_signal(None),
        layout: root_signal(layout),
        picked: root_signal(Vec::new()),
        gather_opts: root_signal(GatherOptions::default()),
    })
}

impl RigState {
    pub fn view(&self) -> RigView {
        let session = self.session.read();
        RigView {
            rig: session.rig().clone(),
            snapshot: session.snapshot().clone(),
            delta: session.delta(),
            dirty: session.is_dirty(),
        }
    }

    /// Apply an edit; report a refusal in the status line. Returns whether it
    /// was applied.
    pub fn edit(self, edit: impl Into<RigEdit>) -> bool {
        let mut session = self.session;
        let mut status = self.status_message;
        let result = session.write().apply_edit(edit);
        match result {
            Ok(()) => {
                status.set(String::new());
                self.drop_stale_selection();
                true
            }
            Err(error) => {
                status.set(error.message().to_owned());
                false
            }
        }
    }

    /// Like [`Self::edit`], but returns the refusal for a field to show.
    pub fn try_edit(self, edit: impl Into<RigEdit>) -> Option<String> {
        let mut session = self.session;
        let mut status = self.status_message;
        let result = session.write().apply_edit(edit);
        match result {
            Ok(()) => {
                status.set(String::new());
                self.drop_stale_selection();
                None
            }
            Err(error) => Some(error.message().to_owned()),
        }
    }

    /// Apply a gather plan as one undo step and report it in the status line.
    /// The points it created become the ticked points (usually the next step).
    pub fn apply_plan(self, plan: Result<GatherPlan, String>) -> bool {
        let mut status = self.status_message;
        let plan = match plan {
            Ok(plan) => plan,
            Err(message) => {
                status.set(message);
                return false;
            }
        };
        let mut session = self.session;
        let result = session.write().apply_batch(&plan.edits);
        match result {
            Ok(()) => {
                status.set(plan.summary);
                let mut picked = self.picked;
                picked.set(plan.new_points);
                self.drop_stale_selection();
                true
            }
            Err(error) => {
                status.set(error.message().to_owned());
                false
            }
        }
    }

    pub fn undo(self) {
        let mut session = self.session;
        session.write().undo();
        self.drop_stale_selection();
    }

    pub fn redo(self) {
        let mut session = self.session;
        session.write().redo();
        self.drop_stale_selection();
    }

    pub fn select(self, selection: Option<Selection>) {
        let mut selected = self.selected;
        selected.set(selection);
    }

    /// Store a dragged block position. Saved straight away and kept out of
    /// the undo history.
    pub fn move_block(self, rig_id: Uuid, key: Uuid, at: [f64; 2]) {
        let mut layout = self.layout;
        layout.write().set(key, at);
        if let Some(store) = store() {
            let mut saved = layout.peek().clone();
            saved.retain_in(self.session.peek().rig());
            let _ = store.save_rig_layout(rig_id, &saved);
        }
    }

    fn drop_stale_selection(self) {
        let rig = self.session.peek().rig().clone();
        let mut selected = self.selected;
        let mut hovered = self.hovered;
        if selected.peek().is_some_and(|s| !s.exists_in(&rig)) {
            selected.set(None);
        }
        if hovered.peek().is_some_and(|s| !s.exists_in(&rig)) {
            hovered.set(None);
        }
    }
}
