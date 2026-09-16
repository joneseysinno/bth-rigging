//! User-saved spreader and mat catalog CRUD.
//!
//! Roadmap: Step 0 foundation.

use uuid::Uuid;

use crate::domain::{SavedMat, SavedSpreader};

use super::keys::{mat_point, spreader_point};
use super::spaces::{KIND_USER_MAT, KIND_USER_SPREADER, SPACE_HARDWARE};
use super::{DbError, RiggingStore};

impl RiggingStore {
    pub fn list_spreaders(&self) -> Result<Vec<SavedSpreader>, DbError> {
        let rows = self.db.query(SPACE_HARDWARE, None)?;
        let mut out = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&row.data) else {
                continue;
            };
            let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if kind != KIND_USER_SPREADER {
                continue;
            }
            if let Ok(sp) = serde_json::from_value::<SavedSpreader>(v) {
                out.push(sp);
            }
        }
        out.sort_by(|a, b| {
            a.manufacturer
                .to_lowercase()
                .cmp(&b.manufacturer.to_lowercase())
                .then_with(|| a.model.to_lowercase().cmp(&b.model.to_lowercase()))
        });
        Ok(out)
    }

    pub fn load_spreader(&self, id: Uuid) -> Result<Option<SavedSpreader>, DbError> {
        Ok(self.list_spreaders()?.into_iter().find(|s| s.id == id))
    }

    pub fn save_spreader(&self, spreader: &SavedSpreader) -> Result<(), DbError> {
        let mut payload = serde_json::to_value(spreader)?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "kind".into(),
                serde_json::Value::String(KIND_USER_SPREADER.into()),
            );
        }
        let bytes = serde_json::to_vec(&payload)?;
        self.db
            .insert(SPACE_HARDWARE, spreader_point(spreader.id), bytes)?;
        self.db.sync()?;
        Ok(())
    }

    pub fn delete_spreader(&self, id: Uuid) -> Result<(), DbError> {
        self.db.delete(SPACE_HARDWARE, spreader_point(id))?;
        self.db.sync()?;
        Ok(())
    }

    pub fn list_mats(&self) -> Result<Vec<SavedMat>, DbError> {
        let rows = self.db.query(SPACE_HARDWARE, None)?;
        let mut out = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&row.data) else {
                continue;
            };
            let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if kind != KIND_USER_MAT {
                continue;
            }
            if let Ok(mat) = serde_json::from_value::<SavedMat>(v) {
                out.push(mat);
            }
        }
        out.sort_by(|a, b| {
            a.manufacturer
                .to_lowercase()
                .cmp(&b.manufacturer.to_lowercase())
                .then_with(|| a.model.to_lowercase().cmp(&b.model.to_lowercase()))
        });
        Ok(out)
    }

    pub fn load_mat(&self, id: Uuid) -> Result<Option<SavedMat>, DbError> {
        Ok(self.list_mats()?.into_iter().find(|m| m.id == id))
    }

    pub fn save_mat(&self, mat: &SavedMat) -> Result<(), DbError> {
        let mut payload = serde_json::to_value(mat)?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "kind".into(),
                serde_json::Value::String(KIND_USER_MAT.into()),
            );
        }
        let bytes = serde_json::to_vec(&payload)?;
        self.db.insert(SPACE_HARDWARE, mat_point(mat.id), bytes)?;
        self.db.sync()?;
        Ok(())
    }

    pub fn delete_mat(&self, id: Uuid) -> Result<(), DbError> {
        self.db.delete(SPACE_HARDWARE, mat_point(id))?;
        self.db.sync()?;
        Ok(())
    }
}
