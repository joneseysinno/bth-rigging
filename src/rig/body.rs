//! Body kinds, frame, weight, and CG.
//!
//! Step: 1
//! Theory: roadmap Step 1 — rigid bodies in the graph.
//! Inputs: body kind and mass properties.
//! Outputs: Body records in the graph.
//! Must not depend on: solve, UI, dioxus.

use serde::{Deserialize, Serialize};

use super::RigError;
use super::id::BodyId;
use super::param::{Coord3, Expr, ParamTable};

/// Working load rating for a spreader bar or lifting beam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarRating {
    pub wll_lbs: u32,
}

impl BarRating {
    pub fn new(wll_lbs: u32) -> Self {
        Self { wll_lbs }
    }
}

/// Kind of rigid body in the rig graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BodyKind {
    /// The fixed root's body (hook today; boom head in Step 4).
    Hook,
    /// Strut between end lugs.
    SpreaderBar { span: Expr, rating: BarRating },
    /// Top lug(s) not at the ends.
    LiftingBeam { span: Expr, rating: BarRating },
    Load {
        length: Expr,
        width: Expr,
        height: Expr,
    },
    /// Catch-all rigid body (tare frames, etc.).
    Frame,
}

impl BodyKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::SpreaderBar { .. } => "spreader bar",
            Self::LiftingBeam { .. } => "lifting beam",
            Self::Load { .. } => "load",
            Self::Frame => "frame",
        }
    }

    pub fn is_gear(&self) -> bool {
        matches!(self, Self::SpreaderBar { .. } | Self::LiftingBeam { .. })
    }

    pub fn is_load(&self) -> bool {
        matches!(self, Self::Load { .. })
    }
}

/// Authored yaw-pitch-roll in degrees (expressions, so a parameter can drive them).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotExpr {
    pub yaw: Expr,
    pub pitch: Expr,
    pub roll: Expr,
}

impl RotExpr {
    pub fn identity() -> Self {
        Self {
            yaw: Expr::c(0.0),
            pitch: Expr::c(0.0),
            roll: Expr::c(0.0),
        }
    }

    pub fn degrees(yaw: f64, pitch: f64, roll: f64) -> Self {
        Self {
            yaw: Expr::c(yaw),
            pitch: Expr::c(pitch),
            roll: Expr::c(roll),
        }
    }

    pub fn eval(&self, params: &ParamTable) -> Result<[f64; 3], RigError> {
        Ok([
            self.yaw.eval(params)?,
            self.pitch.eval(params)?,
            self.roll.eval(params)?,
        ])
    }
}

/// Optional author override for where a body sits. `None` / `Derived` means the
/// nominal descent places it. `#[serde(default)]` so a v1 row loads as derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Placement {
    #[default]
    Derived,
    /// Origin fixed by expression (load on the ground, a set piece).
    Pinned { at: Coord3 },
    /// Origin and rotation both authored.
    Posed { at: Coord3, rot: RotExpr },
    /// Descent picks the origin; the author fixes the rotation.
    Level { rot: RotExpr },
}

impl Placement {
    pub fn is_derived(&self) -> bool {
        matches!(self, Self::Derived)
    }
}

/// One rigid body: mass properties in a body-local frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub id: BodyId,
    pub label: String,
    pub kind: BodyKind,
    /// lb; 0 for a massless frame.
    pub weight: Expr,
    /// Body-local CG, ft.
    pub cg: Coord3,
    /// Author override; omitted on v1 rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
}

impl Body {
    pub fn new(label: impl Into<String>, kind: BodyKind, weight: impl Into<Expr>) -> Self {
        Self {
            id: BodyId::new(),
            label: label.into(),
            kind,
            weight: weight.into(),
            cg: Coord3::origin(),
            placement: None,
        }
    }

    pub fn placement(&self) -> Placement {
        self.placement.clone().unwrap_or(Placement::Derived)
    }

    pub fn with_cg(mut self, cg: Coord3) -> Self {
        self.cg = cg;
        self
    }

    pub fn eval_weight(&self, params: &ParamTable) -> Result<f64, RigError> {
        self.weight.eval(params)
    }

    pub fn eval_cg(&self, params: &ParamTable) -> Result<[f64; 3], RigError> {
        self.cg.eval(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::id::ParamId;
    use crate::rig::param::{Param, Quantity};
    use indexmap::IndexMap;

    #[test]
    fn body_local_coordinates_evaluate() {
        let mut x = Param::new("cg_x", Quantity::Length, 1.0);
        let mut y = Param::new("cg_y", Quantity::Length, 2.0);
        let mut z = Param::new("cg_z", Quantity::Length, 3.0);
        // Stable ids for the table.
        x.id = ParamId::new();
        y.id = ParamId::new();
        z.id = ParamId::new();
        let mut table = IndexMap::new();
        table.insert(x.id, x.clone());
        table.insert(y.id, y.clone());
        table.insert(z.id, z.clone());

        let mut body = Body::new(
            "Al MDC enclosure",
            BodyKind::Load {
                length: Expr::c(48.0),
                width: Expr::c(12.0),
                height: Expr::c(11.0),
            },
            117_300.0,
        );
        body.cg = Coord3::new(x.id, y.id, z.id);
        let cg = body.eval_cg(&table).unwrap();
        assert!((cg[0] - 1.0).abs() < 1e-12);
        assert!((cg[1] - 2.0).abs() < 1e-12);
        assert!((cg[2] - 3.0).abs() < 1e-12);
        assert!((body.eval_weight(&table).unwrap() - 117_300.0).abs() < 1e-12);
    }

    #[test]
    fn v1_body_json_without_placement_deserializes_as_derived() {
        let json = r#"{
            "id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "label": "Load",
            "kind": {"Load": {"length": {"Const": 10.0}, "width": {"Const": 4.0}, "height": {"Const": 2.0}}},
            "weight": {"Const": 1000.0},
            "cg": {"x": {"Const": 0.0}, "y": {"Const": 0.0}, "z": {"Const": 1.0}}
        }"#;
        let body: Body = serde_json::from_str(json).expect("v1 body");
        assert!(body.placement.is_none());
        assert!(matches!(body.placement(), Placement::Derived));
        let again = serde_json::to_value(&body).unwrap();
        assert!(again.get("placement").is_none());
    }
}
