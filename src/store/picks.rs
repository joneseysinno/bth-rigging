//! Pick and sling-layer CRUD, including PickHasLayer hyperedges.
//!
//! Roadmap: Step 0 foundation.

use std::collections::BTreeMap;

use infinite_db::infinitedb_core::address::RevisionId;
use infinite_db::infinitedb_core::hyperedge::{
    Directionality, EndpointPolarity, EndpointRef, EndpointRole, Hyperedge, HyperedgeId,
    HyperedgeKind,
};
use uuid::Uuid;

use crate::domain::{Pick, SlingLayer};

use super::keys::{layer_edge_id, layer_point, pick_point, project_point};
use super::spaces::{
    KIND_PICK_HAS_LAYER, SPACE_LAYERS, SPACE_PICKS, SPACE_PROJECTS, SPACE_TOPOLOGY,
};
use super::{DbError, RiggingStore};

impl RiggingStore {
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
