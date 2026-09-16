//! InfiniteDb persistence for projects, picks, layers, and catalogs.
//!
//! Depends on `domain` and `catalog` only — never on calculation modules.
//! Roadmap: Step 0 foundation; graph persistence lands in `store::rig` (Step 1).

mod hardware;
mod keys;
mod mat_analyses;
mod picks;
mod projects;
mod seed;
mod spaces;
pub mod rig;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use infinite_db::InfiniteDb;
use thiserror::Error;

pub use spaces::{
    SPACE_CATALOG, SPACE_HARDWARE, SPACE_LAYERS, SPACE_MAT_ANALYSES, SPACE_PICKS, SPACE_PROJECTS,
    SPACE_TOPOLOGY,
};

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database engine: {0}")]
    Engine(#[from] infinite_db::EngineError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("data directory unavailable")]
    NoDataDir,
}

/// Shared store wrapping InfiniteDb.
#[derive(Clone)]
pub struct RiggingStore {
    pub(super) db: Arc<InfiniteDb>,
    pub(super) path: PathBuf,
}

impl RiggingStore {
    pub fn open_default() -> Result<Self, DbError> {
        let base = dirs::data_dir().ok_or(DbError::NoDataDir)?;
        let path = base.join("bth-rigging").join("db");
        Self::open(&path)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;
        let db = InfiniteDb::open(&path)?;
        let store = Self {
            db: Arc::new(db),
            path,
        };
        store.ensure_spaces()?;
        store.seed_catalog_if_empty()?;
        store.seed_hardware_if_empty()?;
        store.migrate_orphan_picks()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, MatAnalysis, Pick, Project, SavedMat, SavedSpreader, SlingLayer};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn project_pick_save_reload() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bth-rigging-proj-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);

        let store = RiggingStore::open(&dir).expect("open");
        let project = Project::new("Job A");
        store.save_project(&project).expect("project");

        let mut bar = SavedSpreader::new("Acme", "SB-20", 20_000, 180.0);
        bar.span_ft = Some(8.0);
        store.save_spreader(&bar).expect("spreader");

        let pick = Pick::new(project.id, "Pick 1", 10_000.0);
        let layers = vec![SlingLayer {
            pick_id: pick.id,
            layer_index: 0,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 60.0,
            sling_count: 2,
            sling_length_ft: 12.0,
            pick_spacing_ft: Some(8.0),
            pick_width_ft: Some(6.0),
            spreader_span_ft: Some(12.0),
            apex_shackle: Some("1".into()),
            leg_shackle: Some("3/4".into()),
            spreader_id: Some(bar.id),
            spreader_wll_lbs: None,
            tare_lbs: 25.0,
        }];
        store.save_pick(&pick, &layers).expect("save");

        let store2 = RiggingStore::open(&dir).expect("reopen");
        assert_eq!(store2.list_projects().unwrap().len(), 1);
        let loaded = store2.load_pick(pick.id).unwrap().unwrap();
        assert_eq!(loaded.0.project_id, project.id);
        assert_eq!(loaded.1[0].apex_shackle.as_deref(), Some("1"));
        assert_eq!(loaded.1[0].spreader_id, Some(bar.id));
        assert!((loaded.1[0].tare_lbs - 25.0).abs() < 1e-9);
        assert!((loaded.1[0].sling_length_ft - 12.0).abs() < 1e-9);
        assert_eq!(loaded.1[0].pick_spacing_ft, Some(8.0));
        assert_eq!(loaded.1[0].pick_width_ft, Some(6.0));
        assert_eq!(loaded.1[0].spreader_span_ft, Some(12.0));

        let listed = store2.list_spreaders().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].model, "SB-20");
        assert!((listed[0].weight_lbs - 180.0).abs() < 1e-9);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mat_and_analysis_save_reload_and_cascade() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bth-rigging-mat-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);

        let store = RiggingStore::open(&dir).expect("open");
        let project = Project::new("Job Mat");
        store.save_project(&project).expect("project");

        let mut mat = SavedMat::new("DICA", "OUTRIGGER-8x4", 8.0, 4.0, 6.0);
        mat.weight_lbs = 1_200.0;
        mat.manufacturer_allowable_lbs = 80_000.0;
        store.save_mat(&mat).expect("mat");

        let analysis = MatAnalysis::new(project.id, "Pad A", mat.id);
        store.save_mat_analysis(&analysis).expect("analysis");

        let store2 = RiggingStore::open(&dir).expect("reopen");
        let mats = store2.list_mats().unwrap();
        assert_eq!(mats.len(), 1);
        assert_eq!(mats[0].model, "OUTRIGGER-8x4");
        assert!((mats[0].thickness_in - 6.0).abs() < 1e-9);
        assert!((mats[0].manufacturer_allowable_lbs - 80_000.0).abs() < 1e-9);

        let listed = store2.list_mat_analyses_for_project(project.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Pad A");
        assert_eq!(listed[0].mat_id, mat.id);

        store2.delete_project(project.id).expect("delete");
        assert!(
            store2
                .list_mat_analyses_for_project(project.id)
                .unwrap()
                .is_empty()
        );
        // Catalog mats are global — not deleted with the project.
        assert_eq!(store2.list_mats().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

