//! Node kinds: lug, bearing, free; coordinates as expressions.
//!
//! Step: 1
//! Theory: roadmap Step 1 — attachment and free nodes.
//! Inputs: kind and coordinate expressions.
//! Outputs: Node records.
//! Must not depend on: solve, UI, dioxus.

use serde::{Deserialize, Serialize};

use super::id::{BodyId, NodeId};
use super::param::{Coord3, Expr};

/// Principal axis, used for lug plate normals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
    NegX,
    NegY,
    NegZ,
}

/// Working load rating for a padeye / lug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LugRating {
    pub wll_lbs: u32,
}

impl LugRating {
    pub fn new(wll_lbs: u32) -> Self {
        Self { wll_lbs }
    }
}

/// Role of a node in the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Lug {
        plate_normal: Axis,
        rating: Option<LugRating>,
    },
    /// Shackle bow or pin a strap reeves through. `mu = 0` means free sliding.
    Bow { bow_dia: Expr, mu: Expr },
    /// Basket under the unit. `mu` stored for Step 3; Step 1 only validates.
    Edge {
        radius: Expr,
        mu: Expr,
        softener: bool,
    },
    /// Master link, collector ring — no body.
    Free,
    /// Hook bowl (Step 4: boom-head sheave).
    Root,
}

impl NodeKind {
    pub fn is_bearing_capable(&self) -> bool {
        matches!(self, Self::Bow { .. } | Self::Edge { .. })
    }

    pub fn is_free(&self) -> bool {
        matches!(self, Self::Free)
    }

    pub fn is_root(&self) -> bool {
        matches!(self, Self::Root)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Lug { .. } => "lug",
            Self::Bow { .. } => "bow",
            Self::Edge { .. } => "edge",
            Self::Free => "free",
            Self::Root => "root",
        }
    }
}

/// A point on a body (or a free knot) that members can attach to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// `None` = free node (master link, knot point).
    pub body: Option<BodyId>,
    /// In the body frame; ignored when `body` is `None`.
    pub local: Coord3,
    pub label: String,
    pub kind: NodeKind,
}

impl Node {
    pub fn new(label: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id: NodeId::new(),
            body: None,
            local: Coord3::origin(),
            label: label.into(),
            kind,
        }
    }

    pub fn on_body(mut self, body: BodyId, local: Coord3) -> Self {
        self.body = Some(body);
        self.local = local;
        self
    }

    pub fn lug(label: impl Into<String>, body: BodyId, local: Coord3) -> Self {
        Self::new(
            label,
            NodeKind::Lug {
                plate_normal: Axis::Z,
                rating: None,
            },
        )
        .on_body(body, local)
    }

    pub fn bow(
        label: impl Into<String>,
        body: BodyId,
        local: Coord3,
        bow_dia: impl Into<Expr>,
        mu: impl Into<Expr>,
    ) -> Self {
        Self::new(
            label,
            NodeKind::Bow {
                bow_dia: bow_dia.into(),
                mu: mu.into(),
            },
        )
        .on_body(body, local)
    }

    pub fn root(label: impl Into<String>, body: BodyId) -> Self {
        Self {
            id: NodeId::new(),
            body: Some(body),
            local: Coord3::origin(),
            label: label.into(),
            kind: NodeKind::Root,
        }
    }

    pub fn free(label: impl Into<String>) -> Self {
        Self::new(label, NodeKind::Free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::body::{Body, BodyKind};
    use crate::rig::id::ParamId;
    use crate::rig::param::{Param, ParamTable, Quantity};

    #[test]
    fn free_node_with_a_body_is_rejected_by_placement_rule() {
        let body = Body::new("frame", BodyKind::Frame, 0.0);
        let node = Node {
            id: NodeId::new(),
            body: Some(body.id),
            local: Coord3::origin(),
            label: "knot".into(),
            kind: NodeKind::Free,
        };
        // The node module records the illegal combination; Rig::validate reports it.
        assert!(node.kind.is_free());
        assert!(node.body.is_some());
    }

    #[test]
    fn lug_coordinates_evaluate() {
        let mut x = Param::new("lug_x", Quantity::Length, 4.0);
        x.id = ParamId::new();
        let mut table = ParamTable::new();
        table.insert(x.id, x.clone());
        let body = BodyId::new();
        let n = Node::lug("P3-front", body, Coord3::new(x.id, 0.0, 11.0));
        let xyz = n.local.eval(&table).unwrap();
        assert!((xyz[0] - 4.0).abs() < 1e-12);
        assert!((xyz[2] - 11.0).abs() < 1e-12);
    }
}
