//! Project folder and wall-clock helper.
//!
//! Must not depend on catalog, store, or calculation modules.
//! Roadmap: Step 0 foundation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A job folder that holds one or more picks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    /// Unix millis for sorting.
    pub updated_at: u64,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            notes: String::new(),
            updated_at: now_millis(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_millis();
    }
}

pub fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
