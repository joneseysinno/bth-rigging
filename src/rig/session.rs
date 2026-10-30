//! Headless edit session, recompute snapshots, and undo/redo history.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §6.5 and §7.
//! Inputs: validated rig edits.
//! Outputs: evaluation, solve, rating, headline deltas, and undo history.
//! Must not depend on: UI, dioxus, store.

use std::time::{Duration, Instant};

use crate::checks::Status;
use crate::rig::edit::{EditError, ParamEdit, apply};
use crate::rig::eval::EvalRig;
use crate::rig::solve::SolvedRig;
use crate::rig::topo::{TopoEdit, TopoError};
use crate::rig::views::orbit::PoseSource;
use crate::rig::{Rig, RigError, RigErrorKind, ValidationReport};

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
    /// The solver's last iterate when it did not converge; `solved` then holds
    /// a `SolveNotConverged` error.
    pub unconverged: Option<SolvedRig>,
    pub rated: Option<Vec<Rated>>,
    pub headline: Headline,
    pub headline_is_last_good: bool,
    pub elapsed: Duration,
}

impl Snapshot {
    /// Geometry to draw: the hung pose, else the unconverged iterate, else
    /// the nominal descent. `None` when evaluation failed.
    pub fn draw_pose(&self) -> Option<(&EvalRig, Option<&SolvedRig>, PoseSource)> {
        if let Some(Ok(solved)) = &self.solved {
            return Some((&solved.eval, Some(solved), PoseSource::Hung));
        }
        if let Some(solved) = &self.unconverged {
            return Some((&solved.eval, Some(solved), PoseSource::Unconverged));
        }
        self.eval
            .as_ref()
            .ok()
            .map(|eval| (eval, None, PoseSource::Descent))
    }
}

/// Any edit the session can apply: a parameter value or a change to the graph.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum RigEdit {
    Param(ParamEdit),
    Topo(TopoEdit),
}

impl From<ParamEdit> for RigEdit {
    fn from(edit: ParamEdit) -> Self {
        Self::Param(edit)
    }
}

impl From<TopoEdit> for RigEdit {
    fn from(edit: TopoEdit) -> Self {
        Self::Topo(edit)
    }
}

/// Apply one parameter or topology edit to a copy of `rig`.
#[allow(clippy::result_large_err)]
pub fn apply_rig_edit(rig: &Rig, edit: &RigEdit) -> Result<Rig, RigEditError> {
    match edit {
        RigEdit::Param(edit) => apply(rig, edit).map_err(RigEditError::Param),
        RigEdit::Topo(edit) => super::topo::apply(rig, edit).map_err(RigEditError::Topo),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RigEditError {
    Param(EditError),
    Topo(TopoError),
}

impl RigEditError {
    pub fn message(&self) -> &str {
        match self {
            Self::Param(error) => &error.message,
            Self::Topo(error) => &error.message,
        }
    }
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
        }
    }

    /// Apply a parameter or topology edit as one undo step, then recompute.
    /// A refused edit changes nothing.
    #[allow(clippy::result_large_err)]
    pub fn apply_edit(&mut self, edit: impl Into<RigEdit>) -> Result<(), RigEditError> {
        let updated = apply_rig_edit(&self.rig, &edit.into())?;
        self.commit(updated);
        Ok(())
    }

    /// Apply several edits in order as **one** undo step (a Gather from the
    /// load-up builder is many topology edits). If any edit is refused the
    /// rig is unchanged and that refusal is returned.
    #[allow(clippy::result_large_err)]
    pub fn apply_batch(&mut self, edits: &[RigEdit]) -> Result<(), RigEditError> {
        let mut trial = self.rig.clone();
        for edit in edits {
            trial = apply_rig_edit(&trial, edit)?;
        }
        self.commit(trial);
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub fn apply(&mut self, edit: ParamEdit) -> Result<(), EditError> {
        self.apply_edit(edit).map_err(|error| match error {
            RigEditError::Param(error) => error,
            RigEditError::Topo(_) => unreachable!("a parameter edit"),
        })
    }

    pub fn apply_topo(&mut self, edit: TopoEdit) -> Result<(), TopoError> {
        self.apply_edit(edit).map_err(|error| match error {
            RigEditError::Topo(error) => error,
            RigEditError::Param(_) => unreachable!("a topology edit"),
        })
    }

    /// Rename the rig itself (part of the undo history, like any edit).
    pub fn rename(&mut self, name: &str) {
        let mut updated = self.rig.clone();
        updated.name = name.to_owned();
        self.commit(updated);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn commit(&mut self, updated: Rig) {
        if updated == self.rig {
            return;
        }
        self.undo.push(std::mem::replace(&mut self.rig, updated));
        self.redo.clear();
        self.dirty = self.rig != self.saved_rig;
        let previous_good = self.last_good;
        let (snapshot, good) = recompute(&self.rig, previous_good);
        self.finish_recompute(snapshot, good, previous_good);
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
        let previous_good = self.last_good;
        let (snapshot, good) = recompute(&self.rig, previous_good);
        self.finish_recompute(snapshot, good, previous_good);
        self.dirty = self.rig != self.saved_rig;
    }

    fn finish_recompute(
        &mut self,
        snapshot: Snapshot,
        good: Option<Headline>,
        previous_good: Option<Headline>,
    ) {
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
    let (solved, unconverged) = match solved {
        Some(Ok(result)) if !result.converged => {
            let error = RigError::new(
                RigErrorKind::SolveNotConverged,
                format!(
                    "solver did not converge ({} iterations) — geometry may be infeasible; check lengths and spans",
                    result.iterations
                ),
            );
            (Some(Err(vec![error])), Some(result))
        }
        other => (other, None),
    };
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
        unconverged,
        rated,
        headline,
        headline_is_last_good: current_good.is_none() && previous_good.is_some(),
        elapsed: start.elapsed(),
    };
    (snapshot, current_good)
}

fn make_headline(rig: &Rig, solved: &SolvedRig, rated: &[Rated]) -> Headline {
    let governing_angle_deg = solved
        .members
        .values()
        .filter(|member| member.taut && !member.is_link)
        .flat_map(|member| member.angle_deg.iter().copied())
        .map(f64::abs)
        .fold(90.0, f64::min);
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
    fn topology_edits_recompute_and_undo_exactly() {
        use crate::rig::preset;
        use crate::rig::topo::TopoEdit;
        let blank = preset::blank_rig("blank", uuid::Uuid::nil());
        let picks: Vec<_> = blank
            .nodes
            .values()
            .filter(|n| !n.kind.is_root())
            .map(|n| n.id)
            .collect();
        let mut session = RigSession::new(blank.clone());
        assert!(session.snapshot().solved.is_none());
        let mut history = vec![session.rig().clone()];
        for pick in &picks {
            let edit = preset::connect(session.rig(), vec![session.rig().root, *pick]);
            session.apply_topo(edit).unwrap();
            history.push(session.rig().clone());
        }
        assert!(session.snapshot().solved.as_ref().unwrap().is_ok());
        let bar = preset::add_body(session.rig(), preset::BodyPreset::SpreaderBar);
        session.apply_topo(bar).unwrap();
        history.push(session.rig().clone());
        let first = *session.rig().members.keys().next().unwrap();
        session
            .apply_topo(TopoEdit::RemoveMember { id: first })
            .unwrap();
        history.push(session.rig().clone());

        let refused = session.apply_topo(TopoEdit::RemoveMember { id: first });
        assert!(refused.is_err());
        assert_eq!(session.rig(), history.last().unwrap());

        for expected in history.iter().rev().skip(1) {
            assert!(session.undo());
            assert_eq!(session.rig(), expected);
        }
        assert!(!session.can_undo());
        assert!(!session.is_dirty());
        for expected in history.iter().skip(1) {
            assert!(session.redo());
            assert_eq!(session.rig(), expected);
        }
        assert!(session.is_dirty());
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
        load.cg.x = crate::rig::Expr::c(1.0) / (crate::rig::Expr::Param(s12) / s12);
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
    fn non_converged_solve_is_not_a_result() {
        let mut session = RigSession::new(duplo10());
        let headline = session.snapshot().headline;
        let s12 = session.rig().param_named("s12").unwrap().id;
        session
            .apply(ParamEdit::SetValue {
                id: s12,
                text: "17 ft".into(),
            })
            .unwrap();
        let snapshot = session.snapshot();
        assert!(snapshot.headline_is_last_good);
        assert_eq!(snapshot.headline, headline);
        assert!(snapshot.rated.is_none());
        assert!(!snapshot.unconverged.as_ref().unwrap().converged);
        let errors = snapshot.solved.as_ref().unwrap().as_ref().unwrap_err();
        assert_eq!(errors[0].code, RigErrorKind::SolveNotConverged);
        assert!(errors[0].message.starts_with("solver did not converge ("));
        assert!(
            errors[0]
                .message
                .ends_with("iterations) — geometry may be infeasible; check lengths and spans")
        );

        assert!(session.undo());
        let snapshot = session.snapshot();
        assert!(!snapshot.headline_is_last_good);
        assert!(snapshot.solved.as_ref().unwrap().is_ok());
        assert!(snapshot.unconverged.is_none());
        assert!(snapshot.rated.is_some());
        assert_eq!(snapshot.headline, headline);
    }

    /// Recorded in 2.5.14a from the solver before the phase-A stall exit
    /// (302 iterations): the `load_weight` edit right after `s12 = 9'-0"` in
    /// `duplo10_edit_latency`.
    const OUTLIER_HOOK_LOAD_LBS: f64 = 123_882.760;
    const OUTLIER_TENSIONS_LBS: [(&str, f64); 20] = [
        ("Basket strap P1–P3 (front)", 23_369.032_715_953),
        ("Basket strap P1–P3 (back)", 23_369.032_715_883),
        ("Basket strap P5–P7 (front)", 23_369.032_715_883),
        ("Basket strap P5–P7 (back)", 23_369.032_715_953),
        ("Chain leg P2 front", 0.0),
        ("Chain leg P2 back", 0.0),
        ("Chain leg P4 front", 0.0),
        ("Chain leg P4 back", 0.0),
        ("Chain leg P6 front", 0.0),
        ("Chain leg P6 back", 0.0),
        ("Hook leg LF", 36_320.577_060_447),
        ("Hook leg LB", 36_320.563_306_673),
        ("Hook leg RF", 36_320.563_306_673),
        ("Hook leg RB", 36_320.577_060_447),
        ("Hook to Bar C front", 610.986_883_358),
        ("Hook to Bar C back", 610.986_883_358),
        ("V leg L front", 30_207.748_274_873),
        ("V leg L back", 30_207.748_274_784),
        ("V leg R front", 30_207.748_274_784),
        ("V leg R back", 30_207.748_274_873),
    ];

    #[test]
    fn stall_exit_keeps_outlier_edit_answer() {
        let rig = duplo10();
        let s12 = rig.param_named("s12").unwrap().id;
        let load_weight = rig.param_named("load_weight").unwrap().id;
        let mut session = RigSession::new(rig);
        session
            .apply(ParamEdit::SetValue {
                id: s12,
                text: "9'-0\"".into(),
            })
            .unwrap();
        session
            .apply(ParamEdit::SetValue {
                id: load_weight,
                text: "118200 lb".into(),
            })
            .unwrap();
        let solved = session
            .snapshot()
            .solved
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap();
        assert!(solved.converged);
        assert!(
            solved.iterations < 100,
            "outlier took {} iterations",
            solved.iterations
        );
        assert!((solved.hook_load_lbs - OUTLIER_HOOK_LOAD_LBS).abs() < 0.5);
        let tolerance = 1e-6 * OUTLIER_HOOK_LOAD_LBS;
        for (label, expected) in OUTLIER_TENSIONS_LBS {
            let tension = solved.tension_of(session.rig(), label).unwrap();
            assert!(
                (tension - expected).abs() < tolerance,
                "{label}: {tension} vs {expected}"
            );
        }
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
                let step = index / 2;
                let inches = 3 * if step <= 5 { step } else { 10 - step };
                (s12, format!("{}'-{}\"", 8 + inches / 12, inches % 12))
            } else {
                (load_weight, format!("{} lb", 117_300 + index * 100))
            };
            let start = Instant::now();
            session.apply(ParamEdit::SetValue { id, text }).unwrap();
            elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
            let solved = session
                .snapshot()
                .solved
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap();
            assert!(solved.converged, "edit {index} did not converge");
        }
        elapsed.sort_by(f64::total_cmp);
        let median = (elapsed[9] + elapsed[10]) / 2.0;
        let max = elapsed[19];
        println!("Duplo10 edit latency: median={median:.3} ms max={max:.3} ms");
    }
}
