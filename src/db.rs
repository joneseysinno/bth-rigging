//! InfiniteDb persistence for projects, picks, layers, and catalogs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use infinite_db::infinitedb_core::address::{DimensionVector, RevisionId, SpaceId};
use infinite_db::infinitedb_core::hyperedge::{
    Directionality, EndpointPolarity, EndpointRef, EndpointRole, Hyperedge, HyperedgeId,
    HyperedgeKind,
};
use infinite_db::infinitedb_core::space::SpaceConfig;
use infinite_db::{EngineError, InfiniteDb};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::catalog::{POLYESTER_ROUNDSLINGS, RoundSlingRating, SHACKLES};
use crate::domain::{MatAnalysis, Pick, Project, SavedMat, SavedSpreader, SlingLayer};

const KIND_USER_SPREADER: &str = "user_spreader";
const KIND_USER_MAT: &str = "user_mat";

pub const SPACE_CATALOG: SpaceId = SpaceId(1);
pub const SPACE_PICKS: SpaceId = SpaceId(2);
pub const SPACE_LAYERS: SpaceId = SpaceId(3);
pub const SPACE_TOPOLOGY: SpaceId = SpaceId(4);
pub const SPACE_PROJECTS: SpaceId = SpaceId(5);
pub const SPACE_HARDWARE: SpaceId = SpaceId(6);
pub const SPACE_MAT_ANALYSES: SpaceId = SpaceId(7);

const KIND_PICK_HAS_LAYER: &str = "PickHasLayer";
const UNASSIGNED_NAME: &str = "Unassigned";

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database engine: {0}")]
    Engine(#[from] EngineError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("data directory unavailable")]
    NoDataDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogRow {
    size: u8,
    color: String,
    vertical_lbs: u32,
    choker_lbs: u32,
    basket_vertical_lbs: u32,
    basket_45_lbs: u32,
}

impl From<&RoundSlingRating> for CatalogRow {
    fn from(r: &RoundSlingRating) -> Self {
        Self {
            size: r.size,
            color: r.color.to_string(),
            vertical_lbs: r.vertical_lbs,
            choker_lbs: r.choker_lbs,
            basket_vertical_lbs: r.basket_vertical_lbs,
            basket_45_lbs: r.basket_45_lbs,
        }
    }
}

/// Shared store wrapping InfiniteDb.
#[derive(Clone)]
pub struct RiggingStore {
    db: Arc<InfiniteDb>,
    path: PathBuf,
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

    fn ensure_spaces(&self) -> Result<(), DbError> {
        let spaces = [
            (SPACE_CATALOG, "catalog", 2usize),
            (SPACE_PICKS, "picks", 2),
            (SPACE_LAYERS, "layers", 3),
            (SPACE_TOPOLOGY, "topology", 2),
            (SPACE_PROJECTS, "projects", 2),
            (SPACE_HARDWARE, "hardware", 2),
            (SPACE_MAT_ANALYSES, "mat_analyses", 2),
        ];
        for (id, name, dims) in spaces {
            if let Err(err) = self.db.register_space(SpaceConfig::new(id, name, dims)) {
                let msg = err.to_string().to_lowercase();
                if !(msg.contains("already") || msg.contains("exist") || msg.contains("duplicate"))
                {
                    return Err(DbError::Engine(err));
                }
            }
        }
        Ok(())
    }

    fn seed_catalog_if_empty(&self) -> Result<(), DbError> {
        let rows = self.db.query(SPACE_CATALOG, None)?;
        if !rows.is_empty() {
            return Ok(());
        }
        for rating in POLYESTER_ROUNDSLINGS {
            let payload = serde_json::to_vec(&CatalogRow::from(rating))?;
            self.db.insert(
                SPACE_CATALOG,
                DimensionVector::new(vec![u32::from(rating.size), 0]),
                payload,
            )?;
        }
        self.db.sync()?;
        Ok(())
    }

    fn seed_hardware_if_empty(&self) -> Result<(), DbError> {
        let rows = self.db.query(SPACE_HARDWARE, None)?;
        let has_shackle = rows.iter().any(|row| {
            !row.tombstone
                && serde_json::from_slice::<serde_json::Value>(&row.data)
                    .ok()
                    .and_then(|v| {
                        v.get("kind")
                            .and_then(|k| k.as_str())
                            .map(|s| s == "shackle")
                    })
                    .unwrap_or(false)
        });
        if has_shackle {
            return Ok(());
        }
        for (i, sh) in SHACKLES.iter().enumerate() {
            let payload = serde_json::to_vec(&serde_json::json!({
                "kind": "shackle",
                "size_in": sh.size_in,
                "wll_lbs": sh.wll_lbs,
            }))?;
            self.db.insert(
                SPACE_HARDWARE,
                DimensionVector::new(vec![1, i as u32]),
                payload,
            )?;
        }
        self.db.sync()?;
        Ok(())
    }

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

    fn migrate_orphan_picks(&self) -> Result<(), DbError> {
        let picks = self.list_picks()?;
        let orphans: Vec<_> = picks
            .into_iter()
            .filter(|p| p.project_id.is_nil())
            .collect();
        if orphans.is_empty() {
            return Ok(());
        }

        let unassigned = self.ensure_unassigned_project()?;
        for mut pick in orphans {
            pick.project_id = unassigned.id;
            let layers = self.load_layers(pick.id)?;
            self.save_pick(&pick, &layers)?;
        }
        Ok(())
    }

    fn ensure_unassigned_project(&self) -> Result<Project, DbError> {
        if let Some(p) = self
            .list_projects()?
            .into_iter()
            .find(|p| p.name == UNASSIGNED_NAME)
        {
            return Ok(p);
        }
        let project = Project::new(UNASSIGNED_NAME);
        self.save_project(&project)?;
        Ok(project)
    }

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
        self.db.delete(SPACE_PROJECTS, project_point(id))?;
        self.db.sync()?;
        Ok(())
    }

    pub fn pick_count(&self, project_id: Uuid) -> Result<usize, DbError> {
        Ok(self.list_picks_for_project(project_id)?.len())
    }

    pub fn list_picks(&self) -> Result<Vec<Pick>, DbError> {
        let rows = self.db.query(SPACE_PICKS, None)?;
        let mut picks: Vec<Pick> = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            if let Ok(pick) = serde_json::from_slice::<Pick>(&row.data) {
                picks.push(pick);
            }
        }
        picks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(picks)
    }

    pub fn list_picks_for_project(&self, project_id: Uuid) -> Result<Vec<Pick>, DbError> {
        Ok(self
            .list_picks()?
            .into_iter()
            .filter(|p| p.project_id == project_id)
            .collect())
    }

    pub fn load_pick(&self, id: Uuid) -> Result<Option<(Pick, Vec<SlingLayer>)>, DbError> {
        let pick = self.list_picks()?.into_iter().find(|p| p.id == id);
        let Some(pick) = pick else {
            return Ok(None);
        };
        let mut layers = self.load_layers(id)?;
        layers.sort_by_key(|l| l.layer_index);
        Ok(Some((pick, layers)))
    }

    pub fn load_layers(&self, pick_id: Uuid) -> Result<Vec<SlingLayer>, DbError> {
        let rows = self.db.query(SPACE_LAYERS, None)?;
        let mut layers = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            let layer: SlingLayer = match serde_json::from_slice(&row.data) {
                Ok(l) => l,
                Err(_) => continue,
            };
            if layer.pick_id == pick_id {
                layers.push(layer);
            }
        }
        layers.sort_by_key(|l| l.layer_index);
        Ok(layers)
    }

    pub fn save_pick(&self, pick: &Pick, layers: &[SlingLayer]) -> Result<(), DbError> {
        let pick_payload = serde_json::to_vec(pick)?;
        self.db
            .insert(SPACE_PICKS, pick_point(pick.id), pick_payload)?;

        self.clear_layers_for_pick(pick.id)?;

        for layer in layers {
            let mut layer = layer.clone();
            layer.pick_id = pick.id;
            let payload = serde_json::to_vec(&layer)?;
            self.db.insert(
                SPACE_LAYERS,
                layer_point(pick.id, layer.layer_index),
                payload,
            )?;

            let edge_id = layer_edge_id(pick.id, layer.layer_index);
            let edge = Hyperedge {
                id: HyperedgeId(edge_id),
                kind: HyperedgeKind::new(KIND_PICK_HAS_LAYER),
                endpoints: vec![
                    EndpointRef::new(EndpointRole::new("pick"), SPACE_PICKS, pick_point(pick.id))
                        .with_polarity(EndpointPolarity::Tail),
                    EndpointRef::new(
                        EndpointRole::new("layer"),
                        SPACE_LAYERS,
                        layer_point(pick.id, layer.layer_index),
                    )
                    .with_polarity(EndpointPolarity::Head),
                ],
                weight_milli: None,
                metadata: BTreeMap::new(),
                valid_from: RevisionId::ZERO,
                valid_to: None,
                directionality: Directionality::Directed,
                authoring_frame: None,
                computation: None,
            };
            self.db.insert_hyperedge(SPACE_TOPOLOGY, edge)?;
        }

        if !pick.project_id.is_nil() {
            if let Some(mut project) = self.load_project(pick.project_id)? {
                project.touch();
                let payload = serde_json::to_vec(&project)?;
                self.db
                    .insert(SPACE_PROJECTS, project_point(project.id), payload)?;
            }
        }

        self.db.sync()?;
        Ok(())
    }

    pub fn delete_pick(&self, id: Uuid) -> Result<(), DbError> {
        self.clear_layers_for_pick(id)?;
        self.db.delete(SPACE_PICKS, pick_point(id))?;
        self.db.sync()?;
        Ok(())
    }

    fn clear_layers_for_pick(&self, pick_id: Uuid) -> Result<(), DbError> {
        let existing = self.load_layers(pick_id)?;
        for layer in &existing {
            let point = layer_point(pick_id, layer.layer_index);
            self.db.delete(SPACE_LAYERS, point)?;
            let _ = self.db.delete_hyperedge(
                SPACE_TOPOLOGY,
                HyperedgeId(layer_edge_id(pick_id, layer.layer_index)),
            );
        }
        Ok(())
    }
}

fn uuid_coords(id: Uuid) -> (u32, u32) {
    let bytes = id.as_bytes();
    let hi = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let lo = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (hi, lo)
}

fn project_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

fn pick_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

fn layer_point(pick_id: Uuid, layer_index: u32) -> DimensionVector {
    let (hi, lo) = uuid_coords(pick_id);
    DimensionVector::new(vec![hi, lo, layer_index])
}

fn layer_edge_id(pick_id: Uuid, layer_index: u32) -> u64 {
    let (hi, lo) = uuid_coords(pick_id);
    ((u64::from(hi) << 32) | u64::from(lo))
        .wrapping_mul(1_000_003)
        .wrapping_add(u64::from(layer_index))
}

fn spreader_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

fn mat_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

fn mat_analysis_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Hitch;
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
