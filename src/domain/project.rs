//! Project folder and wall-clock helper.
//!
//! Must not depend on catalog, store, or calculation modules.
//! Roadmap: Step 0 foundation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A job folder that holds one or more picks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub job_number: String,
    #[serde(default)]
    pub client: String,
    /// Job site location.
    #[serde(default)]
    pub location: String,
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
            job_number: String::new(),
            client: String::new(),
            location: String::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_projects_saved_before_job_details() {
        let json = r#"{"id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","name":"Old","notes":"n","updated_at":5}"#;
        let p: Project = serde_json::from_str(json).expect("old shape");
        assert_eq!(p.name, "Old");
        assert_eq!(p.notes, "n");
        assert!(p.job_number.is_empty());
        assert!(p.client.is_empty());
        assert!(p.location.is_empty());
        assert_eq!(p.updated_at, 5);
    }

    #[test]
    fn job_details_round_trip() {
        let mut p = Project::new("Tower A");
        p.job_number = "24-117".into();
        p.client = "Acme".into();
        p.location = "Houston, TX".into();
        let back: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(back.job_number, "24-117");
        assert_eq!(back.client, "Acme");
        assert_eq!(back.location, "Houston, TX");
    }
}
