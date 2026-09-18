//! Graph persistence: spaces, hyperedge roles, and versioned rig documents.
//!
//! Step: 1
//! Theory: roadmap Step 1 — bodies, nodes, members as stored topology.
//! Inputs: evaluated / authored rig graph.
//! Outputs: InfiniteDb spaces and hyperedges for the graph.
//! Must not depend on: layers calc, UI, dioxus. May depend on domain and catalog.

use std::collections::BTreeMap;

use infinite_db::infinitedb_core::address::RevisionId;
use infinite_db::infinitedb_core::hyperedge::{
    Directionality, EndpointPolarity, EndpointRef, EndpointRole, Hyperedge, HyperedgeId,
    HyperedgeKind,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rig::{Body, Member, Node, NodeId, Param, Rig, SCHEMA_VERSION};

use super::keys::{member_edge_id, rig_item_point, rig_point};
use super::spaces::{
    KIND_RIG_MEMBER_PATH, SPACE_RIG_BODIES, SPACE_RIG_MEMBERS, SPACE_RIG_NODES, SPACE_RIG_PARAMS,
    SPACE_RIGS, SPACE_TOPOLOGY,
};
use super::{DbError, RiggingStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RigHeader {
    id: Uuid,
    project_id: Uuid,
    name: String,
    schema_version: u16,
    root: NodeId,
}

impl RiggingStore {
    pub fn list_rigs(&self) -> Result<Vec<RigHeaderView>, DbError> {
        let rows = self.db.query(SPACE_RIGS, None)?;
        let mut out = Vec::new();
        for row in rows {
            if row.tombstone {
                continue;
            }
            let Ok(h) = serde_json::from_slice::<RigHeader>(&row.data) else {
                continue;
            };
            if h.schema_version > SCHEMA_VERSION {
                return Err(DbError::SchemaTooNew {
                    found: h.schema_version,
                    max: SCHEMA_VERSION,
                });
            }
            out.push(RigHeaderView {
                id: h.id,
                project_id: h.project_id,
                name: h.name,
                schema_version: h.schema_version,
            });
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    pub fn list_rigs_for_project(&self, project_id: Uuid) -> Result<Vec<RigHeaderView>, DbError> {
        Ok(self
            .list_rigs()?
            .into_iter()
            .filter(|r| r.project_id == project_id)
            .collect())
    }

    pub fn load_rig(&self, id: Uuid) -> Result<Option<Rig>, DbError> {
        let rows = self.db.query(SPACE_RIGS, None)?;
        let mut header: Option<RigHeader> = None;
        for row in rows {
            if row.tombstone {
                continue;
            }
            let Ok(h) = serde_json::from_slice::<RigHeader>(&row.data) else {
                continue;
            };
            if h.id == id {
                header = Some(h);
                break;
            }
        }
        let Some(header) = header else {
            return Ok(None);
        };
        if header.schema_version > SCHEMA_VERSION {
            return Err(DbError::SchemaTooNew {
                found: header.schema_version,
                max: SCHEMA_VERSION,
            });
        }

        let params = load_indexed::<Param>(self, SPACE_RIG_PARAMS, id)?;
        let bodies = load_indexed::<Body>(self, SPACE_RIG_BODIES, id)?;
        let nodes = load_indexed::<Node>(self, SPACE_RIG_NODES, id)?;
        let members = load_indexed::<Member>(self, SPACE_RIG_MEMBERS, id)?;

        Ok(Some(Rig {
            id: header.id,
            project_id: header.project_id,
            name: header.name,
            schema_version: header.schema_version,
            params: params.into_iter().map(|p| (p.id, p)).collect(),
            bodies: bodies.into_iter().map(|b| (b.id, b)).collect(),
            nodes: nodes.into_iter().map(|n| (n.id, n)).collect(),
            members: members.into_iter().map(|m| (m.id, m)).collect(),
            root: header.root,
        }))
    }

    pub fn save_rig(&self, rig: &Rig) -> Result<(), DbError> {
        self.clear_rig_rows(rig.id)?;

        let header = RigHeader {
            id: rig.id,
            project_id: rig.project_id,
            name: rig.name.clone(),
            schema_version: SCHEMA_VERSION,
            root: rig.root,
        };
        self.db
            .insert(SPACE_RIGS, rig_point(rig.id), serde_json::to_vec(&header)?)?;

        for (i, param) in rig.params.values().enumerate() {
            self.db.insert(
                SPACE_RIG_PARAMS,
                rig_item_point(rig.id, i as u32),
                serde_json::to_vec(param)?,
            )?;
        }
        for (i, body) in rig.bodies.values().enumerate() {
            self.db.insert(
                SPACE_RIG_BODIES,
                rig_item_point(rig.id, i as u32),
                serde_json::to_vec(body)?,
            )?;
        }
        let node_index: BTreeMap<NodeId, u32> = rig
            .nodes
            .values()
            .enumerate()
            .map(|(i, n)| (n.id, i as u32))
            .collect();
        for (i, node) in rig.nodes.values().enumerate() {
            self.db.insert(
                SPACE_RIG_NODES,
                rig_item_point(rig.id, i as u32),
                serde_json::to_vec(node)?,
            )?;
        }
        for (i, member) in rig.members.values().enumerate() {
            self.db.insert(
                SPACE_RIG_MEMBERS,
                rig_item_point(rig.id, i as u32),
                serde_json::to_vec(member)?,
            )?;
            self.insert_member_edge(rig.id, member, &node_index)?;
        }

        self.db.sync()?;
        Ok(())
    }

    pub fn delete_rig(&self, id: Uuid) -> Result<(), DbError> {
        self.clear_rig_rows(id)?;
        self.db.sync()?;
        Ok(())
    }

    /// Hyperedge paths for a rig, in member-payload order. Used to assert
    /// payload / topology agreement.
    pub fn member_hyperedge_paths(
        &self,
        rig_id: Uuid,
    ) -> Result<Vec<(Uuid, Vec<NodeId>)>, DbError> {
        let Some(rig) = self.load_rig(rig_id)? else {
            return Ok(Vec::new());
        };
        let node_index: BTreeMap<u32, NodeId> = rig
            .nodes
            .values()
            .enumerate()
            .map(|(i, n)| (i as u32, n.id))
            .collect();
        let mut out = Vec::new();
        let edges = self
            .db
            .query_hyperedges_by_kind(SPACE_TOPOLOGY, KIND_RIG_MEMBER_PATH, None)?;
        for edge in edges {
            let Some(rid) = edge.metadata.get("rig_id") else {
                continue;
            };
            let Ok(rid) = Uuid::parse_str(rid) else {
                continue;
            };
            if rid != rig_id {
                continue;
            }
            let Some(mid) = edge.metadata.get("member_id") else {
                continue;
            };
            let Ok(mid) = Uuid::parse_str(mid) else {
                continue;
            };
            let mut path = Vec::new();
            for ep in &edge.endpoints {
                if ep.space != SPACE_RIG_NODES {
                    continue;
                }
                if ep.node.coords.len() != 3 {
                    continue;
                }
                let idx = ep.node.coords[2];
                if let Some(nid) = node_index.get(&idx) {
                    path.push(*nid);
                }
            }
            out.push((mid, path));
        }
        Ok(out)
    }

    fn insert_member_edge(
        &self,
        rig_id: Uuid,
        member: &Member,
        node_index: &BTreeMap<NodeId, u32>,
    ) -> Result<(), DbError> {
        let n = member.path.len();
        if n < 2 {
            return Ok(());
        }
        let mut endpoints = Vec::with_capacity(n);
        for (i, nid) in member.path.iter().enumerate() {
            let Some(&idx) = node_index.get(nid) else {
                continue;
            };
            let role = if i == 0 || i + 1 == n {
                EndpointRole::new("end")
            } else {
                EndpointRole::new("bearing")
            };
            let polarity = if i == 0 {
                EndpointPolarity::Tail
            } else if i + 1 == n {
                EndpointPolarity::Head
            } else {
                EndpointPolarity::Neutral
            };
            endpoints.push(
                EndpointRef::new(role, SPACE_RIG_NODES, rig_item_point(rig_id, idx))
                    .with_polarity(polarity),
            );
        }
        if endpoints.len() < 2 {
            return Ok(());
        }
        let mut metadata = BTreeMap::new();
        metadata.insert("rig_id".into(), rig_id.to_string());
        metadata.insert("member_id".into(), member.id.0.to_string());
        let edge = Hyperedge {
            id: HyperedgeId(member_edge_id(member.id.0)),
            kind: HyperedgeKind::new(KIND_RIG_MEMBER_PATH),
            endpoints,
            weight_milli: None,
            metadata,
            valid_from: RevisionId::ZERO,
            valid_to: None,
            directionality: Directionality::Directed,
            authoring_frame: None,
            computation: None,
        };
        self.db.insert_hyperedge(SPACE_TOPOLOGY, edge)?;
        Ok(())
    }

    fn clear_rig_rows(&self, id: Uuid) -> Result<(), DbError> {
        self.db.delete(SPACE_RIGS, rig_point(id))?;
        for space in [
            SPACE_RIG_PARAMS,
            SPACE_RIG_BODIES,
            SPACE_RIG_NODES,
            SPACE_RIG_MEMBERS,
        ] {
            let rows = self.db.query(space, None)?;
            for row in rows {
                if row.tombstone {
                    continue;
                }
                let coords = &row.address.point.coords;
                if coords.len() >= 2 {
                    let (hi, lo) = super::keys::uuid_coords(id);
                    if coords[0] == hi && coords[1] == lo {
                        self.db.delete(space, row.address.point.clone())?;
                    }
                }
            }
        }
        let edges = self
            .db
            .query_hyperedges_by_kind(SPACE_TOPOLOGY, KIND_RIG_MEMBER_PATH, None)?;
        for edge in edges {
            if edge.metadata.get("rig_id").map(String::as_str) == Some(&id.to_string()) {
                let _ = self.db.delete_hyperedge(SPACE_TOPOLOGY, edge.id);
            }
        }
        Ok(())
    }
}

/// Lightweight row returned by `list_rigs` (full graph is `load_rig`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigHeaderView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub schema_version: u16,
}

fn load_indexed<T: for<'de> Deserialize<'de>>(
    store: &RiggingStore,
    space: infinite_db::infinitedb_core::address::SpaceId,
    rig_id: Uuid,
) -> Result<Vec<T>, DbError> {
    let rows = store.db.query(space, None)?;
    let (hi, lo) = super::keys::uuid_coords(rig_id);
    let mut pairs: Vec<(u32, T)> = Vec::new();
    for row in rows {
        if row.tombstone {
            continue;
        }
        let coords = &row.address.point.coords;
        if coords.len() != 3 || coords[0] != hi || coords[1] != lo {
            continue;
        }
        let Ok(item) = serde_json::from_slice::<T>(&row.data) else {
            continue;
        };
        pairs.push((coords[2], item));
    }
    pairs.sort_by_key(|(i, _)| *i);
    Ok(pairs.into_iter().map(|(_, v)| v).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, Pick, Project, SavedSpreader, SlingLayer};
    use crate::rig::{duplo10, from_layers};
    use crate::store::RiggingStore;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bth-rigging-{name}-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn rig_round_trip() {
        let dir = tmp("rig-rt");
        let store = RiggingStore::open(&dir).expect("open");
        let project = Project::new("Job");
        store.save_project(&project).unwrap();
        let mut rig = duplo10();
        rig.project_id = project.id;
        store.save_rig(&rig).unwrap();

        let store2 = RiggingStore::open(&dir).expect("reopen");
        let loaded = store2.load_rig(rig.id).unwrap().expect("present");
        assert_eq!(loaded.name, rig.name);
        assert_eq!(loaded.params.len(), rig.params.len());
        assert_eq!(loaded.bodies.len(), rig.bodies.len());
        assert_eq!(loaded.nodes.len(), rig.nodes.len());
        assert_eq!(loaded.members.len(), rig.members.len());
        assert_eq!(loaded, rig);
        loaded.validate().expect("still valid");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hyperedge_agrees_with_member_payload() {
        let dir = tmp("rig-he");
        let store = RiggingStore::open(&dir).expect("open");
        let mut rig = crate::rig::two_leg_bridle(1_000.0, 8.0, 6.0);
        let project = Project::new("Job");
        store.save_project(&project).unwrap();
        rig.project_id = project.id;
        store.save_rig(&rig).unwrap();

        let edges = store.member_hyperedge_paths(rig.id).unwrap();
        assert_eq!(edges.len(), rig.members.len());
        for member in rig.members.values() {
            let found = edges.iter().find(|(id, _)| *id == member.id.0);
            let (_, path) = found.expect("edge");
            assert_eq!(
                path, &member.path,
                "payload vs hyperedge for {}",
                member.label
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cascade_delete_removes_rigs() {
        let dir = tmp("rig-cas");
        let store = RiggingStore::open(&dir).expect("open");
        let project = Project::new("Job");
        store.save_project(&project).unwrap();
        let mut rig = crate::rig::two_leg_bridle(500.0, 6.0, 4.0);
        rig.project_id = project.id;
        store.save_rig(&rig).unwrap();
        assert_eq!(store.list_rigs_for_project(project.id).unwrap().len(), 1);
        store.delete_project(project.id).unwrap();
        assert!(store.list_rigs_for_project(project.id).unwrap().is_empty());
        assert!(store.load_rig(rig.id).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_data_folder_still_loads_and_new_spaces_start_empty() {
        let dir = tmp("rig-old");
        let store = RiggingStore::open(&dir).expect("open");
        let project = Project::new("Job A");
        store.save_project(&project).unwrap();
        let mut bar = SavedSpreader::new("Acme", "SB-20", 20_000, 180.0);
        bar.span_ft = Some(8.0);
        store.save_spreader(&bar).unwrap();
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
        store.save_pick(&pick, &layers).unwrap();
        let mut mat = crate::domain::SavedMat::new("DICA", "OUTRIGGER-8x4", 8.0, 4.0, 6.0);
        mat.weight_lbs = 1_200.0;
        store.save_mat(&mat).unwrap();

        let store2 = RiggingStore::open(&dir).expect("reopen");
        assert_eq!(store2.list_projects().unwrap().len(), 1);
        assert_eq!(store2.list_picks().unwrap().len(), 1);
        assert_eq!(store2.list_spreaders().unwrap().len(), 1);
        assert_eq!(store2.list_mats().unwrap().len(), 1);
        assert!(store2.list_rigs().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_too_new_is_rejected() {
        let dir = tmp("rig-ver");
        let store = RiggingStore::open(&dir).expect("open");
        let header = RigHeader {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            name: "future".into(),
            schema_version: SCHEMA_VERSION + 1,
            root: NodeId::new(),
        };
        store
            .db
            .insert(
                SPACE_RIGS,
                rig_point(header.id),
                serde_json::to_vec(&header).unwrap(),
            )
            .unwrap();
        store.db.sync().unwrap();
        let err = store.list_rigs().unwrap_err();
        assert!(matches!(err, DbError::SchemaTooNew { found, .. } if found == SCHEMA_VERSION + 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn template_graph_round_trips() {
        let dir = tmp("rig-tpl");
        let store = RiggingStore::open(&dir).expect("open");
        let pick = Pick::new(Uuid::nil(), "bridle", 10_000.0);
        let layer = SlingLayer {
            pick_id: pick.id,
            layer_index: 0,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 60.0,
            sling_count: 2,
            sling_length_ft: 10.0,
            pick_spacing_ft: Some(8.0),
            pick_width_ft: None,
            spreader_span_ft: None,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: None,
            tare_lbs: 0.0,
        };
        let rig = from_layers(&pick, &[layer], &[]);
        store.save_rig(&rig).unwrap();
        let loaded = store.load_rig(rig.id).unwrap().unwrap();
        assert_eq!(loaded.members.len(), rig.members.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
