//! Catalog and hardware seeding, plus orphan-pick migration.
//!
//! Seed insert loops must keep their order. Roadmap: Step 0 foundation.

use infinite_db::infinitedb_core::address::DimensionVector;
use serde::{Deserialize, Serialize};

use crate::catalog::{POLYESTER_ROUNDSLINGS, RoundSlingRating, SHACKLES};
use crate::domain::Project;

use super::spaces::{SPACE_CATALOG, SPACE_HARDWARE, UNASSIGNED_NAME};
use super::{DbError, RiggingStore};

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

impl RiggingStore {
    pub(super) fn seed_catalog_if_empty(&self) -> Result<(), DbError> {
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

    pub(super) fn seed_hardware_if_empty(&self) -> Result<(), DbError> {
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

    pub(super) fn migrate_orphan_picks(&self) -> Result<(), DbError> {
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

    pub(super) fn ensure_unassigned_project(&self) -> Result<Project, DbError> {
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
}
