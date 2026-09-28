//! Shared rating checks used by layers, rig, and crane.
//!
//! Step: 3
//! Theory: WSTDA ratings and manufacturer reductions; roadmap Step 3.
//! Every check is a pure function of numbers (demand, catalog key, angle) and
//! returns a [`Check`] stamped `OK` / `WARN` / `OVER`. The glue that walks a
//! solved rig and calls these lives in `rig::solve::rate`.
//! Inputs: tensions / reactions and catalog ratings.
//! Outputs: pass/fail utilization stamps.
//! Must not depend on: UI, dioxus, store. May depend on catalog and domain.

use serde::{Deserialize, Serialize};

pub mod bar;
pub mod chain;
pub mod lug;
pub mod shackle;
pub mod sling;

/// Stamp on a check, worst last so `max` picks the governing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Status {
    Ok,
    /// Within capacity but something needs a look (angle, side load, bound).
    Warn,
    Over,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Over => "OVER",
        }
    }
}

/// What kind of item was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CheckKind {
    Sling,
    Chain,
    Shackle,
    Lug,
    Bar,
}

/// One rating check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    /// Human label, e.g. `"RS-7 Vertical"`, `"shackle 1-1/4″"`.
    pub item: String,
    /// Force on the item, lb.
    pub demand_lbs: f64,
    /// Rated capacity after reductions, lb.
    pub capacity_lbs: f64,
    pub status: Status,
    /// Why it is WARN/OVER, or the reduction applied.
    pub notes: Vec<String>,
}

impl Check {
    pub fn new(
        kind: CheckKind,
        item: impl Into<String>,
        demand_lbs: f64,
        capacity_lbs: f64,
    ) -> Self {
        let status = if !demand_lbs.is_finite() || demand_lbs > capacity_lbs {
            Status::Over
        } else {
            Status::Ok
        };
        Self {
            kind,
            item: item.into(),
            demand_lbs,
            capacity_lbs,
            status,
            notes: Vec::new(),
        }
    }

    pub fn utilization(&self) -> f64 {
        if self.capacity_lbs <= 0.0 {
            f64::INFINITY
        } else {
            self.demand_lbs / self.capacity_lbs
        }
    }

    /// Raise to WARN (never lowers an OVER) with a note.
    pub fn warn(mut self, note: impl Into<String>) -> Self {
        self.status = self.status.max(Status::Warn);
        self.notes.push(note.into());
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}
