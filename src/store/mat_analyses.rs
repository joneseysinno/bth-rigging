//! Mat bearing-pressure analysis CRUD.
//!
//! Roadmap: Step 0 foundation.

use uuid::Uuid;

use crate::domain::MatAnalysis;

use super::keys::{mat_analysis_point, project_point};
use super::spaces::{SPACE_MAT_ANALYSES, SPACE_PROJECTS};
use super::{DbError, RiggingStore};

impl RiggingStore {
    pub fn list_mat_analyses(&self) -> Result<Vec<MatAnalysis>, DbError> {
        let rows = self.db.query(SPACE_MAT_ANALYSES, None)?;
        let mut out = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            if let Ok(a) = serde_json::from_slice::<MatAnalysis>(&row.data) {
                out.push(a);
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    pub fn list_mat_analyses_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<MatAnalysis>, DbError> {
        Ok(self
            .list_mat_analyses()?
            .into_iter()
            .filter(|a| a.project_id == project_id)
            .collect())
    }

    pub fn load_mat_analysis(&self, id: Uuid) -> Result<Option<MatAnalysis>, DbError> {
        Ok(self.list_mat_analyses()?.into_iter().find(|a| a.id == id))
    }

    pub fn save_mat_analysis(&self, analysis: &MatAnalysis) -> Result<(), DbError> {
        let payload = serde_json::to_vec(analysis)?;
        self.db
            .insert(SPACE_MAT_ANALYSES, mat_analysis_point(analysis.id), payload)?;

        if !analysis.project_id.is_nil() {
            if let Some(mut project) = self.load_project(analysis.project_id)? {
                project.touch();
                let payload = serde_json::to_vec(&project)?;
                self.db
                    .insert(SPACE_PROJECTS, project_point(project.id), payload)?;
            }
        }

        self.db.sync()?;
        Ok(())
    }

    pub fn delete_mat_analysis(&self, id: Uuid) -> Result<(), DbError> {
        self.db.delete(SPACE_MAT_ANALYSES, mat_analysis_point(id))?;
        self.db.sync()?;
        Ok(())
    }
}
