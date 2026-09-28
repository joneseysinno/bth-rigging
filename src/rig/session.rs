//! Headless edit session, recompute snapshots, and undo/redo history.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §6.5 and §7.
//! Inputs: validated rig edits.
//! Outputs: evaluation, solve, rating, headline deltas, and undo history.
//! Must not depend on: UI, dioxus, store.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crate::checks::Status;
use crate::rig::edit::{EditError, ParamEdit, apply};
use crate::rig::eval::EvalRig;
use crate::rig::solve::SolvedRig;
use crate::rig::{Rig, RigError, ValidationReport};

use super::solve;
use super::solve::rate::Rated;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Headline {
    pub hook_load_lbs: f64,
    pub governing_angle_deg: f64,
    pub max_tension_lbs: f64,
    pub max_utilization: f64,
    pub worst_status: Status,
    pub assumed_count: usize,
}

impl Default for Headline {
    fn default() -> Self {
        Self {
            hook_load_lbs: 0.0,
            governing_angle_deg: 0.0,
            max_tension_lbs: 0.0,
            max_utilization: 0.0,
            worst_status: Status::Ok,
            assumed_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadlineDelta {
    pub hook_load_lbs: f64,
    pub governing_angle_deg: f64,
    pub max_tension_lbs: f64,
    pub max_utilization: f64,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub validation: ValidationReport,
    pub eval: Result<EvalRig, Vec<RigError>>,
    pub solved: Option<Result<SolvedRig, Vec<RigError>>>,
    pub rated: Option<Vec<Rated>>,
    pub headline: Headline,
    pub headline_is_last_good: bool,
    pub solving: bool,
    pub elapsed: Duration,
}

struct RecomputeResult {
    generation: u64,
    snapshot: Snapshot,
    good: Option<Headline>,
    previous_good: Option<Headline>,
}

pub struct RigSession {
    rig: Rig,
    saved_rig: Rig,
    undo: Vec<Rig>,
    redo: Vec<Rig>,
    snapshot: Snapshot,
    last_good: Option<Headline>,
    delta: Option<HeadlineDelta>,
    dirty: bool,
    generation: u64,
    pending: Option<Receiver<RecomputeResult>>,
}

impl RigSession {
    pub fn new(rig: Rig) -> Self {
        let saved_rig = rig.clone();
        let (snapshot, last_good) = recompute(&rig, None);
        Self {
            rig,
            saved_rig,
            undo: Vec::new(),
            redo: Vec::new(),
            snapshot,
            last_good,
            delta: None,
            dirty: false,
            generation: 0,
            pending: None,
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn apply(&mut self, edit: ParamEdit) -> Result<(), EditError> {
        self.accept_edit(edit)?;
        self.invalidate_pending();
        let previous_good = self.last_good;
        let (snapshot, good) = recompute(&self.rig, previous_good);
        self.finish_recompute(snapshot, good, previous_good);
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub fn apply_async(&mut self, edit: ParamEdit) -> Result<(), EditError> {
        self.accept_edit(edit)?;
        self.schedule_recompute();
        Ok(())
    }

    pub fn poll(&mut self) -> bool {
        let result = match self.pending.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Empty)) | None => return false,
            Some(Err(TryRecvError::Disconnected)) => {
                self.pending = None;
                self.snapshot.solving = false;
                return false;
            }
        };
        self.pending = None;
        if result.generation != self.generation {
            return false;
        }
        self.finish_recompute(result.snapshot, result.good, result.previous_good);
        true
    }

    pub fn is_solving(&self) -> bool {
        self.snapshot.solving
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.rig.clone());
        self.rig = previous;
        self.recompute_after_history_change();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.rig.clone());
        self.rig = next;
        self.recompute_after_history_change();
        true
    }

    pub fn rig(&self) -> &Rig {
        &self.rig
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn delta(&self) -> Option<HeadlineDelta> {
        self.delta
    }

    pub fn mark_saved(&mut self) {
        self.saved_rig.clone_from(&self.rig);
        self.dirty = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn recompute_after_history_change(&mut self) {
        self.invalidate_pending();
        let previous_good = self.last_good;
        let (snapshot, good) = recompute(&self.rig, previous_good);
        self.finish_recompute(snapshot, good, previous_good);
        self.dirty = self.rig != self.saved_rig;
    }

    #[allow(clippy::result_large_err)]
    fn accept_edit(&mut self, edit: ParamEdit) -> Result<(), EditError> {
        let updated = apply(&self.rig, &edit)?;
        self.undo.push(self.rig.clone());
        self.redo.clear();
        self.rig = updated;
        self.dirty = self.rig != self.saved_rig;
        Ok(())
    }

    fn schedule_recompute(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let rig = self.rig.clone();
        let previous_good = self.last_good;
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.snapshot.solving = true;
        thread::spawn(move || {
            let (snapshot, good) = recompute(&rig, previous_good);
            let _ = sender.send(RecomputeResult {
                generation,
                snapshot,
                good,
                previous_good,
            });
        });
    }

    fn invalidate_pending(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.snapshot.solving = false;
    }

    fn finish_recompute(
        &mut self,
        mut snapshot: Snapshot,
        good: Option<Headline>,
        previous_good: Option<Headline>,
    ) {
        snapshot.solving = false;
        self.snapshot = snapshot;
        self.delta = good.zip(previous_good).map(|(new, old)| HeadlineDelta {
            hook_load_lbs: new.hook_load_lbs - old.hook_load_lbs,
            governing_angle_deg: new.governing_angle_deg - old.governing_angle_deg,
            max_tension_lbs: new.max_tension_lbs - old.max_tension_lbs,
            max_utilization: new.max_utilization - old.max_utilization,
        });
        if let Some(headline) = good {
            self.last_good = Some(headline);
        }
    }
}

fn recompute(rig: &Rig, previous_good: Option<Headline>) -> (Snapshot, Option<Headline>) {
    let start = Instant::now();
    let validation = rig.validate_full();
    let eval = EvalRig::evaluate(rig);
    let solved = eval
        .as_ref()
        .ok()
        .map(|evaluation| solve::solve_eval(rig, evaluation));
    let rated = solved
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|result| super::solve::rate::rate(rig, result).ok());
    let current_good = solved
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .zip(rated.as_deref())
        .map(|(solved, rated)| make_headline(rig, solved, rated));
    let headline = current_good.or(previous_good).unwrap_or_default();
    let snapshot = Snapshot {
        validation,
        eval,
        solved,
        rated,
        headline,
        headline_is_last_good: current_good.is_none() && previous_good.is_some(),
        solving: false,
        elapsed: start.elapsed(),
    };
    (snapshot, current_good)
}

fn make_headline(rig: &Rig, solved: &SolvedRig, rated: &[Rated]) -> Headline {
    let governing_angle_deg = solved
        .members
        .values()
        .flat_map(|member| member.angle_deg.iter().copied())
        .map(f64::abs)
        .fold(0.0, f64::max);
    let max_utilization = rated
        .iter()
        .map(|item| item.check.utilization())
        .fold(0.0, f64::max);
    let worst_status = super::solve::rate::governing(rated);
    Headline {
        hook_load_lbs: solved.hook_load_lbs,
        governing_angle_deg,
        max_tension_lbs: solved.max_tension_lbs(),
        max_utilization,
        worst_status,
        assumed_count: rig
            .params
            .values()
            .filter(|param| param.source == super::ParamSource::Assumed)
            .count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::edit::ParamField;
    use crate::rig::{ParamSource, duplo10};

    #[test]
    fn duplo10_session_edits_recompute_and_undo_exactly() {
        let rig = duplo10();
        let original = rig.clone();
        let s12 = rig.param_named("s12").unwrap().id;
        let load_weight = rig.param_named("load_weight").unwrap().id;
        let mut session = RigSession::new(rig);
        assert!(session.snapshot().solved.as_ref().unwrap().is_ok());
        let initial_angle = session.snapshot().headline.governing_angle_deg;
        let initial_nodes = session
            .snapshot()
            .solved
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .eval
            .nodes
            .clone();

        session
            .apply(ParamEdit::SetValue {
                id: s12,
                text: "9 ft".into(),
            })
            .unwrap();
        assert_ne!(
            session.snapshot().headline.governing_angle_deg,
            initial_angle
        );
        assert_ne!(
            session
                .snapshot()
                .solved
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .eval
                .nodes,
            initial_nodes
        );
        assert!(session.delta().is_some());

        let prior_hook_load = session.snapshot().headline.hook_load_lbs;
        session
            .apply(ParamEdit::SetValue {
                id: load_weight,
                text: "118300 lb".into(),
            })
            .unwrap();
        assert!(
            (session.snapshot().headline.hook_load_lbs - prior_hook_load - 1000.0).abs() < 1e-6
        );
        assert!((session.delta().unwrap().hook_load_lbs - 1000.0).abs() < 1e-6);

        assert!(session.undo());
        assert!(session.undo());
        assert_eq!(session.rig(), &original);
        assert!(session.redo());
        assert!(session.redo());
        assert_eq!(
            session.rig().params[&load_weight].source,
            ParamSource::Drawing
        );
    }

    #[test]
    fn failed_edit_changes_nothing_and_dirty_tracks_save_point() {
        let mut session = RigSession::new(duplo10());
        let original = session.rig().clone();
        let id = session.rig().param_named("s12").unwrap().id;
        let error = session
            .apply(ParamEdit::SetValue {
                id,
                text: "bad name".into(),
            })
            .unwrap_err();
        assert_eq!(error.field, ParamField::Value);
        assert_eq!(session.rig(), &original);
        assert!(!session.is_dirty());

        session
            .apply(ParamEdit::SetValue {
                id,
                text: "9 ft".into(),
            })
            .unwrap();
        assert!(session.is_dirty());
        session.mark_saved();
        assert!(!session.is_dirty());
        assert!(session.undo());
        assert!(session.is_dirty());
        assert!(session.redo());
        assert!(!session.is_dirty());
    }

    #[test]
    fn failed_solve_preserves_last_good_headline() {
        let mut rig = duplo10();
        let s12 = rig.param_named("s12").unwrap().id;
        let load = rig
            .bodies
            .values_mut()
            .find(|body| matches!(body.kind, super::super::body::BodyKind::Load { .. }))
            .unwrap();
        load.cg.x = crate::rig::Expr::c(1.0) / s12;
        let mut session = RigSession::new(rig);
        assert!(session.snapshot().eval.is_ok());
        let headline = session.snapshot().headline;
        session
            .apply(ParamEdit::SetValue {
                id: s12,
                text: "0 ft".into(),
            })
            .unwrap();
        assert!(session.snapshot().eval.is_err());
        assert!(session.snapshot().headline_is_last_good);
        assert_eq!(session.snapshot().headline, headline);
    }

    #[test]
    fn async_recompute_accepts_only_the_latest_generation() {
        let mut session = RigSession::new(duplo10());
        let baseline = session.snapshot().headline.hook_load_lbs;
        let s12 = session.rig().param_named("s12").unwrap().id;
        let load_weight = session.rig().param_named("load_weight").unwrap().id;

        session
            .apply_async(ParamEdit::SetValue {
                id: s12,
                text: "9 ft".into(),
            })
            .unwrap();
        assert!(session.is_solving());
        session
            .apply_async(ParamEdit::SetValue {
                id: load_weight,
                text: "118300 lb".into(),
            })
            .unwrap();
        assert!(session.is_solving());
        wait_for_result(&mut session);

        assert!(!session.is_solving());
        assert!((session.snapshot().headline.hook_load_lbs - baseline - 1000.0).abs() < 1e-6);
        assert_eq!(session.rig().params[&s12].nominal, 9.0);
        assert_eq!(session.rig().params[&load_weight].nominal, 118300.0);
        assert!(session.delta().is_some());
    }

    #[test]
    fn history_navigation_cancels_pending_recompute() {
        let original = duplo10();
        let mut session = RigSession::new(original.clone());
        let s12 = session.rig().param_named("s12").unwrap().id;
        session
            .apply_async(ParamEdit::SetValue {
                id: s12,
                text: "9 ft".into(),
            })
            .unwrap();
        assert!(session.is_solving());
        assert!(session.undo());
        assert!(!session.is_solving());
        assert_eq!(session.rig(), &original);
        assert!(!session.poll());
        assert_eq!(session.rig(), &original);
    }

    fn wait_for_result(session: &mut RigSession) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.is_solving() && Instant::now() < deadline {
            session.poll();
            thread::yield_now();
        }
        assert!(!session.is_solving(), "background recompute did not finish");
    }

    #[test]
    #[ignore = "release latency measurement for §7.2"]
    fn duplo10_edit_latency() {
        let rig = duplo10();
        let s12 = rig.param_named("s12").unwrap().id;
        let load_weight = rig.param_named("load_weight").unwrap().id;
        let mut session = RigSession::new(rig);
        let mut elapsed = Vec::with_capacity(20);
        for index in 0..20 {
            let (id, text) = if index % 2 == 0 {
                (s12, format!("{} ft", 8 + index / 2))
            } else {
                (load_weight, format!("{} lb", 117_300 + index * 100))
            };
            session.apply(ParamEdit::SetValue { id, text }).unwrap();
            elapsed.push(session.snapshot().elapsed.as_secs_f64() * 1000.0);
        }
        elapsed.sort_by(f64::total_cmp);
        let median = (elapsed[9] + elapsed[10]) / 2.0;
        let max = elapsed[19];
        println!("Duplo10 edit latency: median={median:.3} ms max={max:.3} ms");
    }
}
