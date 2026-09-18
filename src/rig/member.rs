//! Member path and segments between nodes.
//!
//! Step: 1
//! Theory: roadmap Step 1 — load paths.
//! Inputs: endpoint nodes and segment list.
//! Outputs: Member path records.
//! Must not depend on: solve, UI, dioxus.

use serde::{Deserialize, Serialize};

use super::RigError;
use super::component::Component;
use super::id::{MemberId, NodeId};
use super::param::ParamTable;

/// Ordered hardware between two consecutive path stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Segment {
    /// Top → bottom, the user's order.
    pub components: Vec<Component>,
}

impl Segment {
    pub fn new(components: Vec<Component>) -> Self {
        Self { components }
    }

    pub fn eval_length(&self, params: &ParamTable) -> Result<f64, RigError> {
        self.components.iter().map(|c| c.eval_length(params)).sum()
    }

    pub fn eval_weight(&self, params: &ParamTable) -> Result<f64, RigError> {
        self.components.iter().map(|c| c.eval_weight(params)).sum()
    }

    pub fn has_positive_length(&self, params: &ParamTable) -> Result<bool, RigError> {
        for c in &self.components {
            if c.eval_length(params)? > 0.0 {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Catalog rating that governed `Member::min_rating`, naming the segment it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct MinRating {
    pub wll_lbs: u32,
    pub segment_index: usize,
    pub component_index: usize,
    pub label: String,
}

/// One physical assembly: a path of nodes with a segment between each pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub label: String,
    /// len ≥ 2; interior stops must be bearing-capable.
    pub path: Vec<NodeId>,
    /// len == path.len() - 1.
    pub segments: Vec<Segment>,
}

impl Member {
    pub fn new(label: impl Into<String>, path: Vec<NodeId>, segments: Vec<Segment>) -> Self {
        Self {
            id: MemberId::new(),
            label: label.into(),
            path,
            segments,
        }
    }

    /// Σ component lengths + chain/turnbuckle settings.
    pub fn nominal_length(&self, params: &ParamTable) -> Result<f64, RigError> {
        self.segments.iter().map(|s| s.eval_length(params)).sum()
    }

    pub fn weight(&self, params: &ParamTable) -> Result<f64, RigError> {
        self.segments.iter().map(|s| s.eval_weight(params)).sum()
    }

    /// Lowest catalog WLL on the member, and the segment it came from.
    pub fn min_rating(&self) -> Option<MinRating> {
        let mut best: Option<MinRating> = None;
        for (si, seg) in self.segments.iter().enumerate() {
            for (ci, comp) in seg.components.iter().enumerate() {
                if let Some(wll) = comp.catalog_wll_lbs() {
                    let candidate = MinRating {
                        wll_lbs: wll,
                        segment_index: si,
                        component_index: ci,
                        label: comp.kind.label(),
                    };
                    match &best {
                        Some(b) if b.wll_lbs <= wll => {}
                        _ => best = Some(candidate),
                    }
                }
            }
        }
        best
    }

    pub fn start(&self) -> Option<NodeId> {
        self.path.first().copied()
    }

    pub fn end(&self) -> Option<NodeId> {
        self.path.last().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Hitch;
    use crate::rig::component::{Adjust, Component};
    use crate::rig::param::ParamTable;

    fn two_node_path() -> (NodeId, NodeId, Vec<NodeId>) {
        let a = NodeId::new();
        let b = NodeId::new();
        (a, b, vec![a, b])
    }

    #[test]
    fn hook_leg_path_is_two_nodes_one_segment() {
        let (_, _, path) = two_node_path();
        let seg = Segment::new(vec![
            Component::shackle("1-1/4"),
            Component::roundsling(7, Hitch::Vertical, 12.0),
            Component::shackle("1-1/4"),
        ]);
        let m = Member::new("Hook leg", path, vec![seg]);
        assert_eq!(m.path.len(), 2);
        assert_eq!(m.segments.len(), 1);
        let t = ParamTable::new();
        let len = m.nominal_length(&t).unwrap();
        // shackles contribute 0; sling 12 ft
        assert!((len - 12.0).abs() < 1e-12);
        let w = m.weight(&t).unwrap();
        assert!((w - (9.50 + 12.0 * 1.19 + 9.50)).abs() < 1e-9);
        let rating = m.min_rating().unwrap();
        // 1-1/4 shackle is 24_000; RS-7 vertical is 21_200
        assert_eq!(rating.wll_lbs, 21_200);
        assert_eq!(rating.segment_index, 0);
    }

    #[test]
    fn basket_strap_path_has_two_segments() {
        let p1 = NodeId::new();
        let bow = NodeId::new();
        let p3 = NodeId::new();
        let strap = |half: f64| {
            Segment::new(vec![
                Component::shackle("1-1/4"),
                Component::strap(half, 5.0, 4.0),
                Component::shackle("1-1/4"),
            ])
        };
        let m = Member::new(
            "Basket strap P1–P3 (front)",
            vec![p1, bow, p3],
            vec![strap(11.0), strap(11.0)],
        );
        assert_eq!(m.path.len(), 3);
        assert_eq!(m.segments.len(), 2);
        let t = ParamTable::new();
        assert!((m.nominal_length(&t).unwrap() - 22.0).abs() < 1e-12);
    }

    #[test]
    fn basket_under_unit_has_three_segments() {
        let lug_a = NodeId::new();
        let e1 = NodeId::new();
        let e2 = NodeId::new();
        let lug_b = NodeId::new();
        let seg = Segment::new(vec![Component::strap(4.0, 2.0, 6.0)]);
        let m = Member::new(
            "Under-unit basket",
            vec![lug_a, e1, e2, lug_b],
            vec![seg.clone(), seg.clone(), seg],
        );
        assert_eq!(m.path.len(), 4);
        assert_eq!(m.segments.len(), 3);
    }

    #[test]
    fn strap_and_adjustable_chain_one_segment() {
        let (_, _, path) = two_node_path();
        let seg = Segment::new(vec![
            Component::shackle("1"),
            Component::strap(3.0, 8.0, 3.0),
            Component::chain(8, 0.5, 9.0, 30.0, Adjust::new(6.0, 10.0, 8.0)),
            Component::shackle("1"),
        ]);
        let m = Member::new("Chain leg P2", path, vec![seg]);
        let t = ParamTable::new();
        // 3 + 9 + 8 setting
        assert!((m.nominal_length(&t).unwrap() - 20.0).abs() < 1e-12);
    }

    #[test]
    fn choker_path_is_lug_bow_lug() {
        let lug_a = NodeId::new();
        let choke = NodeId::new();
        let lug_b = NodeId::new();
        let seg = Segment::new(vec![Component::roundsling(5, Hitch::Choker, 8.0)]);
        let m = Member::new("Choker", vec![lug_a, choke, lug_b], vec![seg.clone(), seg]);
        assert_eq!(m.path.len(), 3);
        assert_eq!(m.min_rating().unwrap().wll_lbs, 10_600);
    }
}
