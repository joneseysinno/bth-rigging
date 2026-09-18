//! Project CRUD.
//!
//! Roadmap: Step 0 foundation.

use uuid::Uuid;

use crate::domain::Project;

use super::keys::project_point;
use super::spaces::SPACE_PROJECTS;
use super::{DbError, RiggingStore};

impl RiggingStore {
    pub fn list_projects(&self) -> Result<Vec<Project>, DbError> {
        let rows = self.db.query(SPACE_PROJECTS, None)?;
        let mut projects = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            if let Ok(p) = serde_json::from_slice::<Project>(&row.data) {
                projects.push(p);
            }
        }
        projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(projects)
    }

    pub fn load_project(&self, id: Uuid) -> Result<Option<Project>, DbError> {
        Ok(self.list_projects()?.into_iter().find(|p| p.id == id))
    }

    pub fn save_project(&self, project: &Project) -> Result<(), DbError> {
        let payload = serde_json::to_vec(project)?;
        self.db
            .insert(SPACE_PROJECTS, project_point(project.id), payload)?;
        self.db.sync()?;
        Ok(())
    }

    pub fn delete_project(&self, id: Uuid) -> Result<(), DbError> {
        let picks = self.list_picks_for_project(id)?;
        for pick in picks {
            self.delete_pick(pick.id)?;
        }
        let analyses = self.list_mat_analyses_for_project(id)?;
        for analysis in analyses {
            self.delete_mat_analysis(analysis.id)?;
        }
        let rigs = self.list_rigs_for_project(id)?;
        for rig in rigs {
            self.delete_rig(rig.id)?;
        }
        self.db.delete(SPACE_PROJECTS, project_point(id))?;
        self.db.sync()?;
        Ok(())
    }

    pub fn pick_count(&self, project_id: Uuid) -> Result<usize, DbError> {
        Ok(self.list_picks_for_project(project_id)?.len())
    }
}
