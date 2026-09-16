//! Space IDs, kind strings, and space registration.
//!
//! Frozen for Step 0 — stored InfiniteDb layout must not change.
//! Roadmap: Step 0 foundation.

use infinite_db::infinitedb_core::address::SpaceId;
use infinite_db::infinitedb_core::space::SpaceConfig;

use super::{DbError, RiggingStore};

pub const SPACE_CATALOG: SpaceId = SpaceId(1);
pub const SPACE_PICKS: SpaceId = SpaceId(2);
pub const SPACE_LAYERS: SpaceId = SpaceId(3);
pub const SPACE_TOPOLOGY: SpaceId = SpaceId(4);
pub const SPACE_PROJECTS: SpaceId = SpaceId(5);
pub const SPACE_HARDWARE: SpaceId = SpaceId(6);
pub const SPACE_MAT_ANALYSES: SpaceId = SpaceId(7);

pub(super) const KIND_PICK_HAS_LAYER: &str = "PickHasLayer";
pub(super) const KIND_USER_SPREADER: &str = "user_spreader";
pub(super) const KIND_USER_MAT: &str = "user_mat";
pub(super) const UNASSIGNED_NAME: &str = "Unassigned";

impl RiggingStore {
    pub(super) fn ensure_spaces(&self) -> Result<(), DbError> {
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
}
