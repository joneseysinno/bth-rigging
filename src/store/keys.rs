//! Dimension-vector keys and PickHasLayer hyperedge IDs.
//!
//! Formulas are frozen for Step 0 stored topology.
//! Roadmap: Step 0 foundation.

use infinite_db::infinitedb_core::address::DimensionVector;
use uuid::Uuid;

pub(super) fn uuid_coords(id: Uuid) -> (u32, u32) {
    let bytes = id.as_bytes();
    let hi = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let lo = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (hi, lo)
}

pub(super) fn project_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

pub(super) fn pick_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

pub(super) fn layer_point(pick_id: Uuid, layer_index: u32) -> DimensionVector {
    let (hi, lo) = uuid_coords(pick_id);
    DimensionVector::new(vec![hi, lo, layer_index])
}

pub(super) fn layer_edge_id(pick_id: Uuid, layer_index: u32) -> u64 {
    let (hi, lo) = uuid_coords(pick_id);
    ((u64::from(hi) << 32) | u64::from(lo))
        .wrapping_mul(1_000_003)
        .wrapping_add(u64::from(layer_index))
}

pub(super) fn spreader_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

pub(super) fn mat_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

pub(super) fn mat_analysis_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

pub(super) fn rig_point(id: Uuid) -> DimensionVector {
    let (hi, lo) = uuid_coords(id);
    DimensionVector::new(vec![hi, lo])
}

pub(super) fn rig_item_point(rig_id: Uuid, index: u32) -> DimensionVector {
    let (hi, lo) = uuid_coords(rig_id);
    DimensionVector::new(vec![hi, lo, index])
}

pub(super) fn member_edge_id(member_id: Uuid) -> u64 {
    let (hi, lo) = uuid_coords(member_id);
    ((u64::from(hi) << 32) | u64::from(lo))
        .wrapping_mul(1_000_003)
        .wrapping_add(0x524D) // "RM"
}
