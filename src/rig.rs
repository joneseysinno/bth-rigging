//! Rigging graph: bodies, nodes, members, components, and solve pipeline.
//!
//! Step: 1–3, 7 (envelope)
//! Theory: Duplo10 Lift Readback Parts 3–4; roadmap Steps 1–3.
//! Inputs: authored graph + parameters.
//! Outputs: tensions, reactions, views; must match `layers` for template cases.
//! Must not depend on: UI, dioxus, store (persistence is `store::rig`).

use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub mod bearing;
pub mod body;
pub mod build;
pub mod component;
pub mod eval;
pub mod fixtures;
pub mod id;
pub mod member;
pub mod node;
pub mod param;
pub mod solve;
pub mod template;
pub mod views;

pub use body::{BarRating, Body, BodyKind};
pub use build::RigBuilder;
pub use component::{Adjust, CatalogKind, CatalogRef, Component, ComponentKind};
pub use fixtures::{duplo10, two_leg_bridle};
pub use id::{BodyId, MemberId, NodeId, ParamId};
pub use member::{Member, MinRating, Segment};
pub use node::{Axis, LugRating, Node, NodeKind};
pub use param::{
    Coord3, Expr, Param, ParamSource, ParamTable, Quantity, SweepResult, check_param_cycles, sweep,
};
pub use template::from_layers;

/// Current rig document schema. Loading a newer version is `DbError::SchemaTooNew`.
pub const SCHEMA_VERSION: u16 = 1;

/// Validation / eval failure, named after the item it points at.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code:?}: {message}")]
pub struct RigError {
    pub code: RigErrorKind,
    pub message: String,
}

impl RigError {
    pub fn new(code: RigErrorKind, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigErrorKind {
    DanglingRef,
    PathShape,
    NotBearingCapable,
    BadEnd,
    NodePlacement,
    ParamCycle,
    UnknownParam,
    QuantityMismatch,
    EmptySegment,
    AdjustOutOfRange,
    RootCount,
    Disconnected,
    UnsupportedBody,
    SelfMember,
    BadValue,
}

/// Errors block the solve; warnings surface in the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub errors: Vec<RigError>,
    pub warnings: Vec<RigError>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Weight roll-up straight from the graph (no solve).
#[derive(Debug, Clone, PartialEq)]
pub struct RigWeights {
    /// Load bodies.
    pub load_lbs: f64,
    /// Spreader bar / lifting beam bodies (the "EGL" row).
    pub gear_lbs: f64,
    /// Member components plus Frame (tare) bodies.
    pub rigging_lbs: f64,
    /// What the hook sees: load + gear + rigging. Hook body weight is excluded.
    pub total_below_root_lbs: f64,
    pub by_body: Vec<(BodyId, f64)>,
    pub by_member: Vec<(MemberId, f64)>,
    /// Any input came from an `Assumed` parameter.
    pub assumed: bool,
}

/// Authored rig graph: parameters, bodies, nodes, members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub schema_version: u16,
    pub params: ParamTable,
    pub bodies: IndexMap<BodyId, Body>,
    pub nodes: IndexMap<NodeId, Node>,
    pub members: IndexMap<MemberId, Member>,
    /// Hook today; boom head in Step 4.
    pub root: NodeId,
}

impl Rig {
    pub fn new(name: impl Into<String>, root: NodeId) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            name: name.into(),
            schema_version: SCHEMA_VERSION,
            params: IndexMap::new(),
            bodies: IndexMap::new(),
            nodes: IndexMap::new(),
            members: IndexMap::new(),
            root,
        }
    }

    pub fn param_named(&self, name: &str) -> Option<&Param> {
        self.params.values().find(|p| p.name == name)
    }

    pub fn node_named(&self, name: &str) -> Option<&Node> {
        self.nodes.values().find(|n| n.label == name)
    }

    pub fn body_named(&self, name: &str) -> Option<&Body> {
        self.bodies.values().find(|b| b.label == name)
    }

    pub fn validate(&self) -> Result<(), Vec<RigError>> {
        let report = self.validate_full();
        if report.errors.is_empty() {
            Ok(())
        } else {
            Err(report.errors)
        }
    }

    pub fn validate_full(&self) -> ValidationReport {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        self.check_dangling(&mut errors);
        self.check_params(&mut errors);
        self.check_nodes(&mut errors);
        self.check_root(&mut errors);
        self.check_members(&mut errors, &mut warnings);
        self.check_connectivity(&mut errors);
        self.check_values(&mut errors);

        ValidationReport { errors, warnings }
    }

    fn check_dangling(&self, errors: &mut Vec<RigError>) {
        if !self.nodes.contains_key(&self.root) {
            errors.push(RigError::new(
                RigErrorKind::DanglingRef,
                format!("root {} is not a node in this rig", self.root),
            ));
        }
        for node in self.nodes.values() {
            if let Some(bid) = node.body
                && !self.bodies.contains_key(&bid)
            {
                errors.push(RigError::new(
                    RigErrorKind::DanglingRef,
                    format!("node '{}' references missing body {bid}", node.label),
                ));
            }
        }
        for member in self.members.values() {
            for nid in &member.path {
                if !self.nodes.contains_key(nid) {
                    errors.push(RigError::new(
                        RigErrorKind::DanglingRef,
                        format!("member '{}' references missing node {nid}", member.label),
                    ));
                }
            }
        }
        for expr in self.all_exprs() {
            for pid in expr.params() {
                if !self.params.contains_key(&pid) {
                    errors.push(RigError::new(
                        RigErrorKind::DanglingRef,
                        format!("expression references missing parameter {pid}"),
                    ));
                }
            }
        }
    }

    fn check_params(&self, errors: &mut Vec<RigError>) {
        let mut seen = HashSet::new();
        for p in self.params.values() {
            if !seen.insert(p.name.clone()) {
                errors.push(RigError::new(
                    RigErrorKind::DanglingRef,
                    format!("duplicate parameter name '{}'", p.name),
                ));
            }
            if p.minus < 0.0 || p.plus < 0.0 {
                errors.push(RigError::new(
                    RigErrorKind::BadValue,
                    format!("parameter '{}' has negative tolerance", p.name),
                ));
            }
        }
        if let Err(e) = check_param_cycles(&self.params) {
            errors.push(e);
        }
        for expr in self.all_exprs() {
            if let Err(e) = expr.eval(&self.params) {
                if matches!(
                    e.code,
                    RigErrorKind::ParamCycle
                        | RigErrorKind::UnknownParam
                        | RigErrorKind::QuantityMismatch
                        | RigErrorKind::BadValue
                ) && !errors.iter().any(|x| x == &e)
                {
                    errors.push(e);
                }
            }
        }
    }

    fn check_nodes(&self, errors: &mut Vec<RigError>) {
        for node in self.nodes.values() {
            match (&node.kind, node.body) {
                (NodeKind::Free, Some(_)) => {
                    errors.push(RigError::new(
                        RigErrorKind::NodePlacement,
                        format!("free node '{}' must not have a body", node.label),
                    ));
                }
                (kind, None) if !kind.is_free() && !kind.is_root() => {
                    // Root and Free may be body-less; Root should have a hook body.
                    if !kind.is_free() {
                        errors.push(RigError::new(
                            RigErrorKind::NodePlacement,
                            format!("node '{}' ({}) needs a body", node.label, kind.label()),
                        ));
                    }
                }
                (NodeKind::Root, None) => {
                    errors.push(RigError::new(
                        RigErrorKind::NodePlacement,
                        format!("root node '{}' needs a hook body", node.label),
                    ));
                }
                (kind, Some(_)) if kind.is_free() => {}
                _ => {}
            }
        }
    }

    fn check_root(&self, errors: &mut Vec<RigError>) {
        let roots: Vec<&Node> = self.nodes.values().filter(|n| n.kind.is_root()).collect();
        if roots.len() != 1 {
            errors.push(RigError::new(
                RigErrorKind::RootCount,
                format!("expected exactly one Root node, found {}", roots.len()),
            ));
            return;
        }
        if roots[0].id != self.root {
            errors.push(RigError::new(
                RigErrorKind::RootCount,
                format!(
                    "Rig::root is {} but the Root node is {}",
                    self.root, roots[0].id
                ),
            ));
        }
    }

    fn check_members(&self, errors: &mut Vec<RigError>, warnings: &mut Vec<RigError>) {
        for member in self.members.values() {
            if member.path.len() < 2 || member.segments.len() != member.path.len().saturating_sub(1)
            {
                errors.push(RigError::new(
                    RigErrorKind::PathShape,
                    format!(
                        "member '{}': path len {} with {} segments",
                        member.label,
                        member.path.len(),
                        member.segments.len()
                    ),
                ));
                continue;
            }

            let nodes: Vec<Option<&Node>> =
                member.path.iter().map(|id| self.nodes.get(id)).collect();
            if nodes.iter().any(|n| n.is_none()) {
                continue; // dangling already reported
            }

            let n = nodes.len();
            for (i, node) in nodes.iter().flatten().enumerate() {
                if i == 0 || i + 1 == n {
                    continue;
                }
                if !node.kind.is_bearing_capable() {
                    errors.push(RigError::new(
                        RigErrorKind::NotBearingCapable,
                        format!(
                            "member '{}': interior stop '{}' is not Bow/Edge",
                            member.label, node.label
                        ),
                    ));
                }
            }

            let first = nodes[0].unwrap();
            let last = nodes[n - 1].unwrap();
            let loop_choker = first.id == last.id && first.kind.is_bearing_capable();
            for end in [first, last] {
                if end.kind.is_bearing_capable() && !loop_choker {
                    errors.push(RigError::new(
                        RigErrorKind::BadEnd,
                        format!(
                            "member '{}': end '{}' is {} (only a self-choker may end on a bearing)",
                            member.label,
                            end.label,
                            end.kind.label()
                        ),
                    ));
                }
            }

            if first.body.is_some() && first.body == last.body && first.id != last.id {
                warnings.push(RigError::new(
                    RigErrorKind::SelfMember,
                    format!(
                        "member '{}' has both ends on the same body (allowed for a choker)",
                        member.label
                    ),
                ));
            }

            for (si, seg) in member.segments.iter().enumerate() {
                match seg.has_positive_length(&self.params) {
                    Ok(true) => {}
                    Ok(false) => errors.push(RigError::new(
                        RigErrorKind::EmptySegment,
                        format!(
                            "member '{}' segment {si} has no positive-length component",
                            member.label
                        ),
                    )),
                    Err(e) => errors.push(e),
                }
                for comp in &seg.components {
                    if let Some(adj) = &comp.adjust {
                        match adj.eval(&self.params) {
                            Ok((min, max, setting)) => {
                                if setting < min - 1e-9 || setting > max + 1e-9 {
                                    errors.push(RigError::new(
                                        RigErrorKind::AdjustOutOfRange,
                                        format!(
                                            "member '{}' adjust setting {setting} not in [{min}, {max}]",
                                            member.label
                                        ),
                                    ));
                                }
                            }
                            Err(e) => errors.push(e),
                        }
                    }
                }
            }
        }
    }

    fn check_connectivity(&self, errors: &mut Vec<RigError>) {
        if !self.nodes.contains_key(&self.root) {
            return;
        }
        // Nodes on the same body are rigidly connected; member paths connect stops.
        let mut adj: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        let mut by_body: HashMap<BodyId, Vec<NodeId>> = HashMap::new();
        for node in self.nodes.values() {
            adj.entry(node.id).or_default();
            if let Some(b) = node.body {
                by_body.entry(b).or_default().push(node.id);
            }
        }
        for ids in by_body.values() {
            for w in ids.windows(2) {
                adj.entry(w[0]).or_default().insert(w[1]);
                adj.entry(w[1]).or_default().insert(w[0]);
            }
            if ids.len() > 2 {
                let a = *ids.first().unwrap();
                let b = *ids.last().unwrap();
                adj.entry(a).or_default().insert(b);
                adj.entry(b).or_default().insert(a);
            }
        }
        for member in self.members.values() {
            for w in member.path.windows(2) {
                adj.entry(w[0]).or_default().insert(w[1]);
                adj.entry(w[1]).or_default().insert(w[0]);
            }
        }

        let mut seen = HashSet::new();
        let mut q = VecDeque::new();
        q.push_back(self.root);
        seen.insert(self.root);
        while let Some(id) = q.pop_front() {
            if let Some(nbrs) = adj.get(&id) {
                for n in nbrs {
                    if seen.insert(*n) {
                        q.push_back(*n);
                    }
                }
            }
        }
        for node in self.nodes.values() {
            if !seen.contains(&node.id) {
                errors.push(RigError::new(
                    RigErrorKind::Disconnected,
                    format!("node '{}' is not connected from the root", node.label),
                ));
            }
        }

        for body in self.bodies.values() {
            let touched = self.nodes.values().any(|n| {
                n.body == Some(body.id) && self.members.values().any(|m| m.path.contains(&n.id))
            });
            if !touched {
                errors.push(RigError::new(
                    RigErrorKind::UnsupportedBody,
                    format!("body '{}' has no node touched by a member", body.label),
                ));
            }
        }
    }

    fn check_values(&self, errors: &mut Vec<RigError>) {
        for body in self.bodies.values() {
            match body.eval_weight(&self.params) {
                Ok(w) if w.is_finite() && w >= 0.0 => {}
                Ok(w) => errors.push(RigError::new(
                    RigErrorKind::BadValue,
                    format!("body '{}' weight {w} is not finite and ≥ 0", body.label),
                )),
                Err(e) => {
                    if e.code == RigErrorKind::BadValue {
                        errors.push(e);
                    }
                }
            }
        }
        for member in self.members.values() {
            match member.weight(&self.params) {
                Ok(w) if w.is_finite() && w >= 0.0 => {}
                Ok(w) => errors.push(RigError::new(
                    RigErrorKind::BadValue,
                    format!("member '{}' weight {w} is not finite and ≥ 0", member.label),
                )),
                Err(e) => {
                    if e.code == RigErrorKind::BadValue {
                        errors.push(e);
                    }
                }
            }
            match member.nominal_length(&self.params) {
                Ok(l) if l.is_finite() && l >= 0.0 => {}
                Ok(l) => errors.push(RigError::new(
                    RigErrorKind::BadValue,
                    format!("member '{}' length {l} is not finite and ≥ 0", member.label),
                )),
                Err(e) => {
                    if e.code == RigErrorKind::BadValue {
                        errors.push(e);
                    }
                }
            }
        }
    }

    fn all_exprs(&self) -> Vec<&Expr> {
        let mut out = Vec::new();
        for p in self.params.values() {
            if let Some(e) = &p.expr {
                out.push(e);
            }
        }
        for body in self.bodies.values() {
            out.push(&body.weight);
            out.push(&body.cg.x);
            out.push(&body.cg.y);
            out.push(&body.cg.z);
            match &body.kind {
                BodyKind::SpreaderBar { span, .. } | BodyKind::LiftingBeam { span, .. } => {
                    out.push(span);
                }
                BodyKind::Load {
                    length,
                    width,
                    height,
                } => {
                    out.push(length);
                    out.push(width);
                    out.push(height);
                }
                BodyKind::Hook | BodyKind::Frame => {}
            }
        }
        for node in self.nodes.values() {
            out.push(&node.local.x);
            out.push(&node.local.y);
            out.push(&node.local.z);
            match &node.kind {
                NodeKind::Bow { bow_dia, mu } => {
                    out.push(bow_dia);
                    out.push(mu);
                }
                NodeKind::Edge { radius, mu, .. } => {
                    out.push(radius);
                    out.push(mu);
                }
                _ => {}
            }
        }
        for member in self.members.values() {
            for seg in &member.segments {
                for c in &seg.components {
                    out.push(&c.length);
                    out.push(&c.weight);
                    if let Some(k) = &c.stiffness_lb {
                        out.push(k);
                    }
                    if let Some(adj) = &c.adjust {
                        out.push(&adj.min);
                        out.push(&adj.max);
                        out.push(&adj.setting);
                    }
                    if let ComponentKind::Strap { width_in }
                    | ComponentKind::Chain {
                        size_in: width_in, ..
                    }
                    | ComponentKind::WireRope { dia_in: width_in } = &c.kind
                    {
                        out.push(width_in);
                    }
                }
            }
        }
        out
    }

    /// Total rigging weight and hook load from the graph (no forces).
    pub fn weights(&self) -> Result<RigWeights, RigError> {
        let mut load_lbs = 0.0;
        let mut gear_lbs = 0.0;
        let mut frame_lbs = 0.0;
        let mut by_body = Vec::new();
        let mut assumed = self.params.values().any(|p| p.is_assumed());

        for body in self.bodies.values() {
            let w = body.eval_weight(&self.params)?;
            by_body.push((body.id, w));
            assumed = assumed || body.weight.is_assumed(&self.params);
            match body.kind {
                BodyKind::Load { .. } => load_lbs += w,
                BodyKind::SpreaderBar { .. } | BodyKind::LiftingBeam { .. } => gear_lbs += w,
                BodyKind::Frame => frame_lbs += w,
                BodyKind::Hook => {}
            }
        }

        let mut rigging_lbs = frame_lbs;
        let mut by_member = Vec::new();
        for member in self.members.values() {
            let w = member.weight(&self.params)?;
            by_member.push((member.id, w));
            rigging_lbs += w;
        }

        Ok(RigWeights {
            load_lbs,
            gear_lbs,
            rigging_lbs,
            total_below_root_lbs: load_lbs + gear_lbs + rigging_lbs,
            by_body,
            by_member,
            assumed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Hitch;
    use crate::rig::component::Component;
    use crate::rig::param::Param;

    fn push_param(rig: &mut Rig, p: Param) -> ParamId {
        let id = p.id;
        rig.params.insert(id, p);
        id
    }

    fn minimal() -> Rig {
        let mut hook_body = Body::new("Hook", BodyKind::Hook, 0.0);
        hook_body.id = BodyId::new();
        let mut load = Body::new(
            "Load",
            BodyKind::Load {
                length: Expr::c(10.0),
                width: Expr::c(4.0),
                height: Expr::c(2.0),
            },
            1_000.0,
        );
        load.id = BodyId::new();
        let root = Node::root("hook", hook_body.id);
        let lug = Node::lug("pick", load.id, Coord3::origin());
        let member = Member::new(
            "leg",
            vec![root.id, lug.id],
            vec![Segment::new(vec![Component::roundsling(
                5,
                Hitch::Vertical,
                8.0,
            )])],
        );
        let mut rig = Rig::new("minimal", root.id);
        rig.bodies.insert(hook_body.id, hook_body);
        rig.bodies.insert(load.id, load);
        rig.nodes.insert(root.id, root);
        rig.nodes.insert(lug.id, lug);
        rig.members.insert(member.id, member);
        rig
    }

    fn has(code: RigErrorKind, errs: &[RigError]) -> bool {
        errs.iter().any(|e| e.code == code)
    }

    #[test]
    fn v1_dangling_ref() {
        let mut rig = minimal();
        rig.members[0].path[1] = NodeId::new();
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::DanglingRef, &err));
    }

    #[test]
    fn v2_path_shape() {
        let mut rig = minimal();
        rig.members[0].segments.clear();
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::PathShape, &err));
    }

    #[test]
    fn v3_not_bearing_capable() {
        let mut rig = minimal();
        let extra = Node::lug("mid", *rig.bodies.keys().nth(1).unwrap(), Coord3::origin());
        let mid_id = extra.id;
        let start = rig.members[0].path[0];
        let end = rig.members[0].path[1];
        rig.nodes.insert(mid_id, extra);
        let m = &mut rig.members[0];
        m.path = vec![start, mid_id, end];
        m.segments.push(Segment::new(vec![Component::roundsling(
            5,
            Hitch::Vertical,
            4.0,
        )]));
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::NotBearingCapable, &err));
    }

    #[test]
    fn v4_bad_end() {
        let mut rig = minimal();
        let bar_id = {
            let bar = Body::new(
                "Bar",
                BodyKind::SpreaderBar {
                    span: Expr::c(10.0),
                    rating: BarRating::new(20_000),
                },
                100.0,
            );
            let id = bar.id;
            rig.bodies.insert(id, bar);
            id
        };
        let bow = Node::bow("bow", bar_id, Coord3::origin(), 2.0, 0.0);
        let bow_id = bow.id;
        let end = rig.members[0].path[1];
        rig.nodes.insert(bow_id, bow);
        // Hang the bar so V12 doesn't fire; the path still starts on a bow.
        let hang = Member::new(
            "hang bar",
            vec![rig.root, bow_id],
            vec![Segment::new(vec![Component::roundsling(
                5,
                Hitch::Vertical,
                4.0,
            )])],
        );
        rig.members.insert(hang.id, hang);
        let m = &mut rig.members[0];
        m.path[0] = bow_id;
        let _ = end;
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::BadEnd, &err));
    }

    #[test]
    fn v5_free_node_with_body() {
        let mut rig = minimal();
        let body = *rig.bodies.keys().next().unwrap();
        let mut free = Node::free("knot");
        free.body = Some(body);
        let id = free.id;
        // Connect it so we only trigger placement.
        let start = rig.root;
        rig.members.insert(
            MemberId::new(),
            Member::new(
                "to knot",
                vec![start, id],
                vec![Segment::new(vec![Component::roundsling(
                    5,
                    Hitch::Vertical,
                    3.0,
                )])],
            ),
        );
        rig.nodes.insert(id, free);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::NodePlacement, &err));
    }

    #[test]
    fn v6_param_cycle() {
        let mut rig = minimal();
        let mut a = Param::new("a", Quantity::Length, 1.0);
        let mut b = Param::new("b", Quantity::Length, 1.0);
        a.source = ParamSource::Derived;
        b.source = ParamSource::Derived;
        a.expr = Some(Expr::p(b.id));
        b.expr = Some(Expr::p(a.id));
        let aid = push_param(&mut rig, a);
        push_param(&mut rig, b);
        rig.bodies[1].weight = Expr::p(aid); // keep it referenced
        // cycle is in the table even if weight would also quantity-mismatch
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::ParamCycle, &err));
    }

    #[test]
    fn v6_unknown_param() {
        let mut rig = minimal();
        rig.bodies[1].weight = Expr::p(ParamId::new());
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::DanglingRef, &err) || has(RigErrorKind::UnknownParam, &err));
    }

    #[test]
    fn v7_quantity_mismatch() {
        let mut rig = minimal();
        let len = push_param(&mut rig, Param::new("span", Quantity::Length, 10.0));
        let w = push_param(&mut rig, Param::new("wt", Quantity::Weight, 100.0));
        rig.bodies[1].weight = len + w;
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::QuantityMismatch, &err));
    }

    #[test]
    fn v8_empty_segment() {
        let mut rig = minimal();
        rig.members[0].segments[0] = Segment::new(vec![Component::shackle("3/4")]);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::EmptySegment, &err));
    }

    #[test]
    fn v9_adjust_out_of_range() {
        let mut rig = minimal();
        rig.members[0].segments[0] = Segment::new(vec![Component::chain(
            8,
            0.5,
            9.0,
            10.0,
            Adjust::new(6.0, 10.0, 12.0),
        )]);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::AdjustOutOfRange, &err));
    }

    #[test]
    fn v10_root_count() {
        let mut rig = minimal();
        let hook = *rig.nodes.get(&rig.root).unwrap().body.as_ref().unwrap();
        let extra = Node::root("hook2", hook);
        rig.nodes.insert(extra.id, extra);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::RootCount, &err));
    }

    #[test]
    fn v11_disconnected() {
        let mut rig = minimal();
        let island_body = Body::new("island", BodyKind::Frame, 1.0);
        let bid = island_body.id;
        let n1 = Node::lug("a", bid, Coord3::origin());
        let n2 = Node::lug("b", bid, Coord3::new(1.0, 0.0, 0.0));
        let m = Member::new(
            "island member",
            vec![n1.id, n2.id],
            vec![Segment::new(vec![Component::roundsling(
                5,
                Hitch::Vertical,
                4.0,
            )])],
        );
        rig.bodies.insert(bid, island_body);
        rig.nodes.insert(n1.id, n1);
        rig.nodes.insert(n2.id, n2);
        rig.members.insert(m.id, m);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::Disconnected, &err));
    }

    #[test]
    fn v12_unsupported_body() {
        let mut rig = minimal();
        let lonely = Body::new("unused bar", BodyKind::Frame, 10.0);
        let n = Node::lug("unused lug", lonely.id, Coord3::origin());
        rig.bodies.insert(lonely.id, lonely);
        rig.nodes.insert(n.id, n);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::UnsupportedBody, &err) || has(RigErrorKind::Disconnected, &err));
    }

    #[test]
    fn v13_self_member_is_warning() {
        let mut rig = minimal();
        let load = *rig
            .nodes
            .values()
            .find(|n| n.label == "pick")
            .unwrap()
            .body
            .as_ref()
            .unwrap();
        let lug2 = Node::lug("pick2", load, Coord3::new(4.0, 0.0, 0.0));
        let choke = Node::bow("choke", load, Coord3::new(2.0, 0.0, 0.0), 1.0, 0.0);
        let start = rig.nodes.values().find(|n| n.label == "pick").unwrap().id;
        let m = Member::new(
            "choker",
            vec![start, choke.id, lug2.id],
            vec![
                Segment::new(vec![Component::roundsling(5, Hitch::Choker, 4.0)]),
                Segment::new(vec![Component::roundsling(5, Hitch::Choker, 4.0)]),
            ],
        );
        rig.nodes.insert(lug2.id, lug2);
        rig.nodes.insert(choke.id, choke);
        rig.members.insert(m.id, m);
        let report = rig.validate_full();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(has(RigErrorKind::SelfMember, &report.warnings));
    }

    #[test]
    fn v14_bad_value() {
        let mut rig = minimal();
        rig.bodies[1].weight = Expr::c(-5.0);
        let err = rig.validate().unwrap_err();
        assert!(has(RigErrorKind::BadValue, &err));
    }

    #[test]
    fn minimal_validates_and_rolls_up_weight() {
        let rig = minimal();
        rig.validate().expect("valid");
        let w = rig.weights().unwrap();
        assert!((w.load_lbs - 1_000.0).abs() < 1e-12);
        assert!((w.gear_lbs).abs() < 1e-12);
        assert!((w.rigging_lbs - 8.0).abs() < 1e-12); // RS-5 @ 8 ft × 1 lb/ft
        assert!((w.total_below_root_lbs - 1_008.0).abs() < 1e-12);
        assert!(!w.assumed);
    }
}
