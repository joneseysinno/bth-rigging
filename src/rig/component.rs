//! Strap, chain, shackle, link; order within a segment.
//!
//! Step: 1
//! Theory: roadmap Step 1 — components on members.
//! Inputs: component kind and catalog refs.
//! Outputs: ordered Component list on a segment.
//! Must not depend on: solve, UI, dioxus.

use serde::{Deserialize, Serialize};

use crate::catalog::{find_by_size, find_shackle};
use crate::domain::Hitch;

use super::RigError;
use super::param::{Expr, ParamTable};

/// Pointer into a catalog table; numbers stay in `catalog/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRef {
    pub kind: CatalogKind,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogKind {
    RoundSling,
    Shackle,
    Chain,
    WireRope,
    Bar,
}

impl CatalogRef {
    pub fn roundsling(size: u8) -> Self {
        Self {
            kind: CatalogKind::RoundSling,
            key: size.to_string(),
        }
    }

    pub fn shackle(size: impl Into<String>) -> Self {
        Self {
            kind: CatalogKind::Shackle,
            key: size.into(),
        }
    }
}

/// Adjustable take-up on a chain or turnbuckle. `setting` ∈ `[min, max]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Adjust {
    pub min: Expr,
    pub max: Expr,
    pub setting: Expr,
}

impl Adjust {
    pub fn new(min: impl Into<Expr>, max: impl Into<Expr>, setting: impl Into<Expr>) -> Self {
        Self {
            min: min.into(),
            max: max.into(),
            setting: setting.into(),
        }
    }

    pub fn eval(&self, params: &ParamTable) -> Result<(f64, f64, f64), RigError> {
        Ok((
            self.min.eval(params)?,
            self.max.eval(params)?,
            self.setting.eval(params)?,
        ))
    }
}

/// Physical hardware kind sitting in a segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentKind {
    RoundSling { size: u8, hitch: Hitch },
    Strap { width_in: Expr },
    Chain { grade: u8, size_in: Expr },
    WireRope { dia_in: Expr },
    Shackle { size: String },
    MasterLink,
    Turnbuckle,
}

impl ComponentKind {
    pub fn label(&self) -> String {
        match self {
            Self::RoundSling { size, hitch } => {
                format!("RS-{size} {}", hitch.label())
            }
            Self::Strap { .. } => "strap".into(),
            Self::Chain { grade, .. } => format!("chain G{grade}"),
            Self::WireRope { .. } => "wire rope".into(),
            Self::Shackle { size } => format!("shackle {size}″"),
            Self::MasterLink => "master link".into(),
            Self::Turnbuckle => "turnbuckle".into(),
        }
    }
}

/// One piece of hardware in a segment, top → bottom in the user's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub kind: ComponentKind,
    /// ft, pin-to-pin contribution (before adjust setting).
    pub length: Expr,
    /// lb.
    pub weight: Expr,
    /// Some for chains and turnbuckles.
    pub adjust: Option<Adjust>,
    /// EA; None = treat as inextensible (Step 3).
    pub stiffness_lb: Option<Expr>,
    pub catalog: Option<CatalogRef>,
}

impl Component {
    pub fn new(kind: ComponentKind, length: impl Into<Expr>, weight: impl Into<Expr>) -> Self {
        Self {
            kind,
            length: length.into(),
            weight: weight.into(),
            adjust: None,
            stiffness_lb: None,
            catalog: None,
        }
    }

    pub fn roundsling(size: u8, hitch: Hitch, length_ft: f64) -> Self {
        let w = find_by_size(size)
            .map(|r| r.sling_weight_lbs(length_ft))
            .unwrap_or(0.0);
        Self {
            kind: ComponentKind::RoundSling { size, hitch },
            length: Expr::c(length_ft),
            weight: Expr::c(w),
            adjust: None,
            stiffness_lb: None,
            catalog: Some(CatalogRef::roundsling(size)),
        }
    }

    pub fn shackle(size: impl Into<String>) -> Self {
        let size = size.into();
        let w = find_shackle(&size).map(|s| s.weight_lbs).unwrap_or(0.0);
        Self {
            kind: ComponentKind::Shackle { size: size.clone() },
            length: Expr::c(0.0),
            weight: Expr::c(w),
            adjust: None,
            stiffness_lb: None,
            catalog: Some(CatalogRef::shackle(size)),
        }
    }

    pub fn strap(
        length: impl Into<Expr>,
        weight: impl Into<Expr>,
        width_in: impl Into<Expr>,
    ) -> Self {
        Self::new(
            ComponentKind::Strap {
                width_in: width_in.into(),
            },
            length,
            weight,
        )
    }

    pub fn chain(
        grade: u8,
        size_in: impl Into<Expr>,
        length: impl Into<Expr>,
        weight: impl Into<Expr>,
        adjust: Adjust,
    ) -> Self {
        Self {
            kind: ComponentKind::Chain {
                grade,
                size_in: size_in.into(),
            },
            length: length.into(),
            weight: weight.into(),
            adjust: Some(adjust),
            stiffness_lb: None,
            catalog: None,
        }
    }

    pub fn master_link(length: impl Into<Expr>, weight: impl Into<Expr>) -> Self {
        Self::new(ComponentKind::MasterLink, length, weight)
    }

    pub fn with_adjust(mut self, adjust: Adjust) -> Self {
        self.adjust = Some(adjust);
        self
    }

    pub fn eval_length(&self, params: &ParamTable) -> Result<f64, RigError> {
        let base = self.length.eval(params)?;
        let extra = if let Some(adj) = &self.adjust {
            adj.setting.eval(params)?
        } else {
            0.0
        };
        Ok(base + extra)
    }

    pub fn eval_weight(&self, params: &ParamTable) -> Result<f64, RigError> {
        self.weight.eval(params)
    }

    /// Catalog WLL in lb, when the component has a known rating.
    pub fn catalog_wll_lbs(&self) -> Option<u32> {
        match &self.kind {
            ComponentKind::RoundSling { size, hitch } => {
                find_by_size(*size).map(|r| r.hitch_wll_lbs(*hitch))
            }
            ComponentKind::Shackle { size } => find_shackle(size).map(|s| s.wll_lbs),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::param::ParamTable;

    #[test]
    fn shackle_and_roundsling_pull_catalog_weight() {
        let t = ParamTable::new();
        let sh = Component::shackle("1-1/4");
        assert!((sh.eval_weight(&t).unwrap() - 9.50).abs() < 1e-9);
        assert_eq!(sh.catalog_wll_lbs(), Some(24_000));
        let rs = Component::roundsling(5, Hitch::Vertical, 10.0);
        assert!((rs.eval_weight(&t).unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(rs.catalog_wll_lbs(), Some(13_200));
    }

    #[test]
    fn chain_length_includes_adjust_setting() {
        let t = ParamTable::new();
        let c = Component::chain(8, 0.5, 9.0, 40.0, Adjust::new(6.0, 10.0, 8.0));
        assert!((c.eval_length(&t).unwrap() - 17.0).abs() < 1e-12);
    }
}
