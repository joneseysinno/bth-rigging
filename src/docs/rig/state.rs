//! Per-rig editor session, shared by the model view, panels and status readout.

use dioxus::prelude::*;
use indexmap::IndexMap;
use uuid::Uuid;

use crate::app::store;
use crate::docs::{Registry, root_signal};
use bth_rigging::rig::session::{HeadlineDelta, RigSession, Snapshot};
use bth_rigging::rig::solve::MemberForce;
use bth_rigging::rig::{MemberId, Rig, RigError};

#[derive(Clone, Copy)]
pub struct RigState {
    pub session: Signal<RigSession>,
    pub status_message: Signal<String>,
    pub selected: Signal<Option<MemberId>>,
    pub hovered: Signal<Option<MemberId>>,
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
    Ok(RigState {
        session: root_signal(RigSession::new(rig)),
        status_message: root_signal(String::new()),
        selected: root_signal(None),
        hovered: root_signal(None),
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
}
