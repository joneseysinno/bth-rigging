//! Headless topology edits: add, remove and rewire bodies, nodes, members and
//! the hardware inside them.
//!
//! Step: G (graph builder)
//! Theory: docs/step-g-graph-builder.md; docs/rig-model.md "Topology edits".
//! Inputs: an authored rig and one topology edit.
//! Outputs: a new rig, or an error that leaves the input unchanged.
//! Must not depend on: UI, dioxus, store.
//!
//! Edits refuse only what breaks the graph's structure (dangling references,
//! path shape, a non-bearing interior stop). An incomplete rig — an unwired
//! bar, a load not yet hung — is a valid draft; `Rig::validate_full` reports it.

use super::body::{BarRating, Body, BodyKind};
use super::component::{Component, ComponentKind};
use super::id::{BodyId, MemberId, NodeId};
use super::member::{Member, Segment};
use super::node::{Node, NodeKind};
use super::param::syntax::parse_for;
use super::param::{Expr, Param, Quantity};
use super::{Rig, edit::validate_name};
use crate::catalog::find_by_size;

#[derive(Debug, Clone, PartialEq)]
pub enum TopoEdit {
    /// Insert a body with its nodes and the parameters they reference.
    AddBody {
        body: Body,
        nodes: Vec<Node>,
        params: Vec<Param>,
    },
    /// Removes the body, its nodes and every member touching them.
    RemoveBody {
        id: BodyId,
    },
    RenameBody {
        id: BodyId,
        label: String,
    },
    SetBodyExpr {
        id: BodyId,
        field: BodyField,
        text: String,
    },
    SetBarRating {
        id: BodyId,
        wll_lbs: u32,
    },
    /// A port on a body, or a free node (master link) with `body: None`.
    AddNode {
        node: Node,
    },
    /// Refused while a member uses the node.
    RemoveNode {
        id: NodeId,
    },
    RenameNode {
        id: NodeId,
        label: String,
    },
    SetNodeExpr {
        id: NodeId,
        field: NodeField,
        text: String,
    },
    SetNodeKind {
        id: NodeId,
        kind: NodeKind,
    },
    Connect {
        member: Member,
    },
    /// New path for a member. Segments are kept when the count still fits,
    /// otherwise re-split so total length and weight are unchanged.
    Reroute {
        id: MemberId,
        path: Vec<NodeId>,
    },
    RemoveMember {
        id: MemberId,
    },
    RenameMember {
        id: MemberId,
        label: String,
    },
    InsertComponent {
        member: MemberId,
        segment: usize,
        index: usize,
        component: Component,
    },
    RemoveComponent {
        member: MemberId,
        segment: usize,
        index: usize,
    },
    MoveComponent {
        member: MemberId,
        segment: usize,
        from: usize,
        to: usize,
    },
    ReplaceComponent {
        member: MemberId,
        segment: usize,
        index: usize,
        component: Component,
    },
    SetComponentExpr {
        member: MemberId,
        segment: usize,
        index: usize,
        field: ComponentField,
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyField {
    Weight,
    CgX,
    CgY,
    CgZ,
    Span,
    Length,
    Width,
    Height,
}

impl BodyField {
    pub fn quantity(self) -> Quantity {
        match self {
            Self::Weight => Quantity::Weight,
            _ => Quantity::Length,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeField {
    X,
    Y,
    Z,
    BowDia,
    EdgeRadius,
    Mu,
}

impl NodeField {
    pub fn quantity(self) -> Quantity {
        match self {
            Self::Mu => Quantity::Ratio,
            _ => Quantity::Length,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentField {
    Length,
    Weight,
    AdjustMin,
    AdjustMax,
    AdjustSetting,
    /// Strap width, chain size or rope diameter, in inches (a plain number).
    SizeIn,
}

impl ComponentField {
    pub fn quantity(self) -> Quantity {
        match self {
            Self::Weight => Quantity::Weight,
            Self::SizeIn => Quantity::Ratio,
            _ => Quantity::Length,
        }
    }
}

/// A refused topology edit. `members` names the members in the way.
#[derive(Debug, Clone, PartialEq)]
pub struct TopoError {
    pub message: String,
    pub members: Vec<MemberId>,
}

impl TopoError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            members: Vec::new(),
        }
    }
}

impl std::fmt::Display for TopoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub fn apply(rig: &Rig, edit: &TopoEdit) -> Result<Rig, TopoError> {
    let mut trial = rig.clone();
    match edit {
        TopoEdit::AddBody {
            body,
            nodes,
            params,
        } => {
            if matches!(body.kind, BodyKind::Hook) {
                return Err(TopoError::new("a rig has exactly one hook"));
            }
            if trial.bodies.contains_key(&body.id) {
                return Err(TopoError::new("body already exists"));
            }
            for param in params {
                validate_name(&param.name, &trial.params, None).map_err(TopoError::new)?;
                trial.params.insert(param.id, param.clone());
            }
            trial.bodies.insert(body.id, body.clone());
            for node in nodes {
                if node.body != Some(body.id) {
                    return Err(TopoError::new(format!(
                        "node '{}' is not on the new body",
                        node.label
                    )));
                }
                insert_node(&mut trial, node.clone())?;
            }
            check_refs(&trial)?;
        }
        TopoEdit::RemoveBody { id } => {
            let body = trial.bodies.get(id).ok_or_else(|| missing("body"))?;
            if matches!(body.kind, BodyKind::Hook) {
                return Err(TopoError::new("the hook can't be removed"));
            }
            let doomed: Vec<NodeId> = trial
                .nodes
                .values()
                .filter(|n| n.body == Some(*id))
                .map(|n| n.id)
                .collect();
            trial
                .members
                .retain(|_, m| !m.path.iter().any(|n| doomed.contains(n)));
            trial.nodes.retain(|nid, _| !doomed.contains(nid));
            trial.bodies.shift_remove(id);
        }
        TopoEdit::RenameBody { id, label } => {
            trial
                .bodies
                .get_mut(id)
                .ok_or_else(|| missing("body"))?
                .label
                .clone_from(label);
        }
        TopoEdit::SetBodyExpr { id, field, text } => {
            let expr = parse_field(text, &trial, field.quantity())?;
            let body = trial.bodies.get_mut(id).ok_or_else(|| missing("body"))?;
            let slot = match (field, &mut body.kind) {
                (BodyField::Weight, _) => &mut body.weight,
                (BodyField::CgX, _) => &mut body.cg.x,
                (BodyField::CgY, _) => &mut body.cg.y,
                (BodyField::CgZ, _) => &mut body.cg.z,
                (
                    BodyField::Span,
                    BodyKind::SpreaderBar { span, .. } | BodyKind::LiftingBeam { span, .. },
                ) => span,
                (BodyField::Length, BodyKind::Load { length, .. }) => length,
                (BodyField::Width, BodyKind::Load { width, .. }) => width,
                (BodyField::Height, BodyKind::Load { height, .. }) => height,
                _ => return Err(TopoError::new("this body has no such field")),
            };
            *slot = expr;
            check_value(&trial)?;
        }
        TopoEdit::SetBarRating { id, wll_lbs } => {
            let body = trial.bodies.get_mut(id).ok_or_else(|| missing("body"))?;
            match &mut body.kind {
                BodyKind::SpreaderBar { rating, .. } | BodyKind::LiftingBeam { rating, .. } => {
                    *rating = BarRating::new(*wll_lbs);
                }
                _ => return Err(TopoError::new("only bars and beams carry a rating")),
            }
        }
        TopoEdit::AddNode { node } => {
            if node.kind.is_root() {
                return Err(TopoError::new("a rig has exactly one hook"));
            }
            match (node.body, &node.kind) {
                (Some(_), NodeKind::Free) => {
                    return Err(TopoError::new("a master link is not on a body"));
                }
                (None, kind) if !kind.is_free() => {
                    return Err(TopoError::new(format!("a {} needs a body", kind.label())));
                }
                (Some(b), _) if !trial.bodies.contains_key(&b) => return Err(missing("body")),
                _ => {}
            }
            insert_node(&mut trial, node.clone())?;
            check_refs(&trial)?;
        }
        TopoEdit::RemoveNode { id } => {
            let node = trial.nodes.get(id).ok_or_else(|| missing("node"))?;
            if node.kind.is_root() {
                return Err(TopoError::new("the hook can't be removed"));
            }
            let users = members_using(&trial, *id);
            if !users.is_empty() {
                let names: Vec<&str> = users
                    .iter()
                    .filter_map(|m| trial.members.get(m).map(|m| m.label.as_str()))
                    .collect();
                return Err(TopoError {
                    message: format!("'{}' is used by {}", node.label, names.join(", ")),
                    members: users,
                });
            }
            trial.nodes.shift_remove(id);
        }
        TopoEdit::RenameNode { id, label } => {
            trial
                .nodes
                .get_mut(id)
                .ok_or_else(|| missing("node"))?
                .label
                .clone_from(label);
        }
        TopoEdit::SetNodeExpr { id, field, text } => {
            let expr = parse_field(text, &trial, field.quantity())?;
            let node = trial.nodes.get_mut(id).ok_or_else(|| missing("node"))?;
            let slot = match (field, &mut node.kind) {
                (NodeField::X, _) => &mut node.local.x,
                (NodeField::Y, _) => &mut node.local.y,
                (NodeField::Z, _) => &mut node.local.z,
                (NodeField::BowDia, NodeKind::Bow { bow_dia, .. }) => bow_dia,
                (NodeField::EdgeRadius, NodeKind::Edge { radius, .. }) => radius,
                (NodeField::Mu, NodeKind::Bow { mu, .. } | NodeKind::Edge { mu, .. }) => mu,
                _ => return Err(TopoError::new("this node has no such field")),
            };
            *slot = expr;
        }
        TopoEdit::SetNodeKind { id, kind } => {
            let node = trial.nodes.get(id).ok_or_else(|| missing("node"))?;
            if node.kind.is_root() || kind.is_root() {
                return Err(TopoError::new("the hook's kind is fixed"));
            }
            if node.body.is_none() != kind.is_free() {
                return Err(TopoError::new(
                    "a master link can't become a body port, or the reverse",
                ));
            }
            trial.nodes.get_mut(id).expect("checked").kind = kind.clone();
            for member in trial.members.values() {
                check_path(&trial, &member.path)?;
            }
        }
        TopoEdit::Connect { member } => {
            if trial.members.contains_key(&member.id) {
                return Err(TopoError::new("member already exists"));
            }
            check_path(&trial, &member.path)?;
            if member.segments.len() + 1 != member.path.len() {
                return Err(TopoError::new(format!(
                    "{} stops need {} hardware runs",
                    member.path.len(),
                    member.path.len() - 1
                )));
            }
            trial.members.insert(member.id, member.clone());
        }
        TopoEdit::Reroute { id, path } => {
            check_path(&trial, path)?;
            let member = trial.members.get_mut(id).ok_or_else(|| missing("member"))?;
            let want = path.len() - 1;
            if member.segments.len() != want {
                member.segments = resplit(&member.segments, want);
            }
            member.path.clone_from(path);
        }
        TopoEdit::RemoveMember { id } => {
            trial
                .members
                .shift_remove(id)
                .ok_or_else(|| missing("member"))?;
        }
        TopoEdit::RenameMember { id, label } => {
            trial
                .members
                .get_mut(id)
                .ok_or_else(|| missing("member"))?
                .label
                .clone_from(label);
        }
        TopoEdit::InsertComponent {
            member,
            segment,
            index,
            component,
        } => {
            let seg = segment_mut(&mut trial, *member, *segment)?;
            if *index > seg.components.len() {
                return Err(TopoError::new("insert position is past the end"));
            }
            seg.components.insert(*index, component.clone());
            check_value(&trial)?;
        }
        TopoEdit::RemoveComponent {
            member,
            segment,
            index,
        } => {
            let seg = segment_mut(&mut trial, *member, *segment)?;
            if *index >= seg.components.len() {
                return Err(missing("component"));
            }
            seg.components.remove(*index);
        }
        TopoEdit::MoveComponent {
            member,
            segment,
            from,
            to,
        } => {
            let seg = segment_mut(&mut trial, *member, *segment)?;
            let n = seg.components.len();
            if *from >= n || *to >= n {
                return Err(missing("component"));
            }
            let c = seg.components.remove(*from);
            seg.components.insert(*to, c);
        }
        TopoEdit::ReplaceComponent {
            member,
            segment,
            index,
            component,
        } => {
            let seg = segment_mut(&mut trial, *member, *segment)?;
            let slot = seg
                .components
                .get_mut(*index)
                .ok_or_else(|| missing("component"))?;
            *slot = component.clone();
            check_value(&trial)?;
        }
        TopoEdit::SetComponentExpr {
            member,
            segment,
            index,
            field,
            text,
        } => {
            let expr = parse_field(text, &trial, field.quantity())?;
            let params = trial.params.clone();
            let seg = segment_mut(&mut trial, *member, *segment)?;
            let c = seg
                .components
                .get_mut(*index)
                .ok_or_else(|| missing("component"))?;
            match field {
                ComponentField::Length => {
                    c.length = expr;
                    resync_catalog_weight(c, &params);
                }
                ComponentField::Weight => c.weight = expr,
                ComponentField::AdjustMin
                | ComponentField::AdjustMax
                | ComponentField::AdjustSetting => {
                    let adjust = c
                        .adjust
                        .as_mut()
                        .ok_or_else(|| TopoError::new("this hardware has no take-up"))?;
                    match field {
                        ComponentField::AdjustMin => adjust.min = expr,
                        ComponentField::AdjustMax => adjust.max = expr,
                        _ => adjust.setting = expr,
                    }
                }
                ComponentField::SizeIn => match &mut c.kind {
                    ComponentKind::Strap { width_in: v }
                    | ComponentKind::Chain { size_in: v, .. }
                    | ComponentKind::WireRope { dia_in: v } => *v = expr,
                    _ => return Err(TopoError::new("this hardware has no size field")),
                },
            }
            check_value(&trial)?;
        }
    }
    Ok(trial)
}

/// Every member whose path touches `node`.
pub fn members_using(rig: &Rig, node: NodeId) -> Vec<MemberId> {
    rig.members
        .values()
        .filter(|m| m.path.contains(&node))
        .map(|m| m.id)
        .collect()
}

/// Whether a path is structurally acceptable (used by the canvas to light up
/// valid targets before an edit is made).
pub fn path_ok(rig: &Rig, path: &[NodeId]) -> bool {
    check_path(rig, path).is_ok()
}

/// A catalog roundsling's weight follows its length (lb/ft × ft).
fn resync_catalog_weight(c: &mut Component, params: &super::ParamTable) {
    if let (ComponentKind::RoundSling { size, .. }, Some(_)) = (&c.kind, &c.catalog)
        && let Some(rating) = find_by_size(*size)
        && let Ok(length) = c.length.eval(params)
    {
        c.weight = Expr::c(rating.sling_weight_lbs(length));
    }
}

fn insert_node(rig: &mut Rig, node: Node) -> Result<(), TopoError> {
    if rig.nodes.contains_key(&node.id) {
        return Err(TopoError::new("node already exists"));
    }
    rig.nodes.insert(node.id, node);
    Ok(())
}

fn segment_mut(rig: &mut Rig, member: MemberId, segment: usize) -> Result<&mut Segment, TopoError> {
    rig.members
        .get_mut(&member)
        .ok_or_else(|| missing("member"))?
        .segments
        .get_mut(segment)
        .ok_or_else(|| missing("hardware run"))
}

fn check_path(rig: &Rig, path: &[NodeId]) -> Result<(), TopoError> {
    if path.len() < 2 {
        return Err(TopoError::new("a member needs two ends"));
    }
    let mut nodes = Vec::with_capacity(path.len());
    for id in path {
        nodes.push(rig.nodes.get(id).ok_or_else(|| missing("node"))?);
    }
    if path.windows(2).any(|w| w[0] == w[1]) {
        return Err(TopoError::new("a member can't run from a node to itself"));
    }
    let last = nodes.len() - 1;
    for node in &nodes[1..last] {
        if !node.kind.is_bearing_capable() {
            return Err(TopoError::new(format!(
                "'{}' is a {}; a member can only pass through a bow or an edge",
                node.label,
                node.kind.label()
            )));
        }
    }
    let self_choker = path[0] == path[last] && nodes[0].kind.is_bearing_capable();
    for end in [nodes[0], nodes[last]] {
        if end.kind.is_bearing_capable() && !self_choker {
            return Err(TopoError::new(format!(
                "'{}' is a {}; a member can only end on a lug, the hook or a master link",
                end.label,
                end.kind.label()
            )));
        }
    }
    Ok(())
}

fn check_refs(rig: &Rig) -> Result<(), TopoError> {
    let report = rig.validate_full();
    if let Some(error) = report.errors.iter().find(|e| {
        matches!(
            e.code,
            super::RigErrorKind::DanglingRef | super::RigErrorKind::ParamCycle
        )
    }) {
        return Err(TopoError::new(error.message.clone()));
    }
    Ok(())
}

/// Weights and lengths stay finite and non-negative.
fn check_value(rig: &Rig) -> Result<(), TopoError> {
    let report = rig.validate_full();
    if let Some(error) = report
        .errors
        .iter()
        .find(|e| e.code == super::RigErrorKind::BadValue)
    {
        return Err(TopoError::new(error.message.clone()));
    }
    Ok(())
}

fn parse_field(text: &str, rig: &Rig, quantity: Quantity) -> Result<Expr, TopoError> {
    let parsed = parse_for(text, &rig.params, quantity).map_err(|e| TopoError::new(e.message))?;
    parsed
        .expr
        .eval(&rig.params)
        .map_err(|e| TopoError::new(e.message))?;
    Ok(parsed.expr)
}

/// Split `segments` into `want` runs with the same total length and weight.
/// Each new run copies the hardware order of the first run.
fn resplit(segments: &[Segment], want: usize) -> Vec<Segment> {
    let have = segments.len().max(1);
    let template = segments.first().cloned().unwrap_or_default();
    let ratio = have as f64 / want as f64;
    let scale = |e: &Expr| {
        if (ratio - 1.0).abs() < 1e-12 {
            e.clone()
        } else {
            match e {
                Expr::Const(v) => Expr::c(v * ratio),
                other => other.clone() * ratio,
            }
        }
    };
    (0..want)
        .map(|_| {
            let mut seg = template.clone();
            for c in &mut seg.components {
                if matches!(c.kind, ComponentKind::Shackle { .. }) {
                    continue;
                }
                c.length = scale(&c.length);
                c.weight = scale(&c.weight);
            }
            seg
        })
        .collect()
}

fn missing(what: &str) -> TopoError {
    TopoError::new(format!("that {what} no longer exists"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, Pick, SavedSpreader, SlingLayer};
    use crate::rig::preset::{self, BodyPreset, PickPattern};
    use crate::rig::{from_layers, solve, two_leg_bridle};

    /// Rebuild `source` from a hook-only rig, one edit at a time.
    fn rebuild(source: &Rig) -> Rig {
        let mut rig = preset::hook_only(&source.name, source.project_id);
        let new_root = rig.root;
        let map = |id: NodeId| if id == source.root { new_root } else { id };
        let mut first = true;
        for body in source.bodies.values() {
            if matches!(body.kind, BodyKind::Hook) {
                continue;
            }
            let nodes = source
                .nodes
                .values()
                .filter(|n| n.body == Some(body.id))
                .cloned()
                .collect();
            let params = if first {
                source.params.values().cloned().collect()
            } else {
                Vec::new()
            };
            first = false;
            rig = apply(
                &rig,
                &TopoEdit::AddBody {
                    body: body.clone(),
                    nodes,
                    params,
                },
            )
            .unwrap();
        }
        for node in source.nodes.values().filter(|n| n.kind.is_free()) {
            rig = apply(&rig, &TopoEdit::AddNode { node: node.clone() }).unwrap();
        }
        for member in source.members.values() {
            let mut member = member.clone();
            member.path = member.path.iter().map(|n| map(*n)).collect();
            rig = apply(&rig, &TopoEdit::Connect { member }).unwrap();
        }
        rig
    }

    fn assert_same_solve(a: &Rig, b: &Rig) {
        let wa = a.weights().unwrap();
        let wb = b.weights().unwrap();
        assert!((wa.total_below_root_lbs - wb.total_below_root_lbs).abs() < 1e-9);
        assert!((wa.rigging_lbs - wb.rigging_lbs).abs() < 1e-9);
        let sa = solve::solve(a).unwrap();
        let sb = solve::solve(b).unwrap();
        assert!((sa.hook_load_lbs - sb.hook_load_lbs).abs() < 1e-9);
        for member in a.members.values() {
            let ta = sa.tension_of(a, &member.label).unwrap();
            let tb = sb.tension_of(b, &member.label).unwrap();
            assert!(
                (ta - tb).abs() < 1e-9 * ta.abs().max(1.0),
                "{}",
                member.label
            );
        }
    }

    fn two_layer_template() -> Rig {
        let pick = Pick::new(uuid::Uuid::nil(), "template", 10_000.0);
        let bar = SavedSpreader::new("Test", "Bar", 30_000, 150.0);
        let top = SlingLayer {
            pick_id: pick.id,
            layer_index: 0,
            size: 7,
            hitch: Hitch::Vertical,
            angle_deg: 90.0,
            sling_count: 2,
            sling_length_ft: 12.0,
            pick_spacing_ft: None,
            pick_width_ft: None,
            spreader_span_ft: Some(10.0),
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: Some(bar.id),
            spreader_wll_lbs: None,
            tare_lbs: 0.0,
        };
        let bottom = SlingLayer {
            pick_id: pick.id,
            layer_index: 1,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 90.0,
            sling_count: 4,
            sling_length_ft: 9.0,
            pick_spacing_ft: Some(10.0),
            pick_width_ft: None,
            spreader_span_ft: None,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: None,
            tare_lbs: 0.0,
        };
        from_layers(&pick, &[top, bottom], &[bar])
    }

    #[test]
    fn two_leg_bridle_rebuilt_by_edits_solves_the_same() {
        let fixture = two_leg_bridle(1_000.0, 10.0, 8.0);
        let rebuilt = rebuild(&fixture);
        rebuilt.validate().unwrap();
        assert_same_solve(&fixture, &rebuilt);
    }

    #[test]
    fn bar_template_rebuilt_by_edits_solves_the_same() {
        let template = two_layer_template();
        let rebuilt = rebuild(&template);
        rebuilt.validate().unwrap();
        assert_same_solve(&template, &rebuilt);
    }

    #[test]
    fn spreader_rig_from_presets_solves() {
        let mut rig = preset::blank_rig("bar rig", uuid::Uuid::nil());
        let load = rig.bodies.values().find(|b| b.kind.is_load()).unwrap().id;
        let picks: Vec<NodeId> = ports_of(&rig, load);
        let add = preset::add_body(&rig, BodyPreset::SpreaderBar);
        let TopoEdit::AddBody { body, .. } = &add else {
            panic!()
        };
        let bar = body.id;
        rig = apply(&rig, &add).unwrap();
        let bar_ports = ports_of(&rig, bar);
        // Top A, Top B, Bottom A, Bottom B.
        for top in &bar_ports[..2] {
            rig = apply(&rig, &preset::connect(&rig, vec![rig.root, *top])).unwrap();
        }
        for (bottom, pick) in bar_ports[2..].iter().zip(&picks) {
            rig = apply(&rig, &preset::connect(&rig, vec![*bottom, *pick])).unwrap();
        }
        rig.validate().unwrap();
        let solved = solve::solve(&rig).unwrap();
        assert!(solved.converged);
        let weights = rig.weights().unwrap();
        assert!((solved.hook_load_lbs - weights.total_below_root_lbs).abs() < 1e-6);
    }

    fn ports_of(rig: &Rig, body: BodyId) -> Vec<NodeId> {
        rig.nodes
            .values()
            .filter(|n| n.body == Some(body))
            .map(|n| n.id)
            .collect()
    }

    #[test]
    fn draft_rigs_are_accepted_and_reported() {
        let rig = preset::hook_only("draft", uuid::Uuid::nil());
        let rig = apply(
            &rig,
            &preset::add_body(
                &rig,
                BodyPreset::Load {
                    pattern: PickPattern::FourCorners,
                },
            ),
        )
        .unwrap();
        let report = rig.validate_full();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.code == crate::rig::RigErrorKind::UnsupportedBody)
        );
    }

    #[test]
    fn remove_body_cascades_without_dangling_refs() {
        let template = two_layer_template();
        let bar = template
            .bodies
            .values()
            .find(|b| b.kind.is_gear())
            .unwrap()
            .id;
        let removed = apply(&template, &TopoEdit::RemoveBody { id: bar }).unwrap();
        assert!(!removed.bodies.contains_key(&bar));
        assert!(removed.nodes.values().all(|n| n.body != Some(bar)));
        let report = removed.validate_full();
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.code == crate::rig::RigErrorKind::DanglingRef)
        );
        let hook = template
            .bodies
            .values()
            .find(|b| matches!(b.kind, BodyKind::Hook))
            .unwrap()
            .id;
        assert!(apply(&template, &TopoEdit::RemoveBody { id: hook }).is_err());
    }

    #[test]
    fn remove_node_in_use_is_refused_with_members() {
        let rig = two_leg_bridle(1_000.0, 10.0, 8.0);
        let lug = rig.node_named("A").unwrap().id;
        let error = apply(&rig, &TopoEdit::RemoveNode { id: lug }).unwrap_err();
        assert_eq!(error.members.len(), 1);
        assert!(error.message.contains("leg A"));
    }

    #[test]
    fn structural_errors_are_refused_and_leave_input_unchanged() {
        let rig = two_leg_bridle(1_000.0, 10.0, 8.0);
        let original = rig.clone();
        let a = rig.node_named("A").unwrap().id;
        let b = rig.node_named("B").unwrap().id;
        // A lug can't be an interior stop.
        let error = apply(&rig, &preset::connect(&rig, vec![rig.root, a, b])).unwrap_err();
        assert!(error.message.contains("bow"));
        assert!(apply(&rig, &preset::connect(&rig, vec![a, a])).is_err());
        let leg = rig.members.values().next().unwrap().id;
        assert!(
            apply(
                &rig,
                &TopoEdit::RemoveComponent {
                    member: leg,
                    segment: 0,
                    index: 5
                }
            )
            .is_err()
        );
        assert_eq!(rig, original);
    }

    #[test]
    fn reroute_through_a_bow_keeps_total_length_and_weight() {
        let mut rig = preset::blank_rig("basket", uuid::Uuid::nil());
        let add = preset::add_body(&rig, BodyPreset::SpreaderBar);
        let TopoEdit::AddBody { body, .. } = &add else {
            panic!()
        };
        let bar = body.id;
        rig = apply(&rig, &add).unwrap();
        let bow = Node::bow("Bow", bar, crate::rig::Coord3::origin(), 0.25, 0.0);
        let bow_id = bow.id;
        rig = apply(&rig, &TopoEdit::AddNode { node: bow }).unwrap();
        let load = rig.bodies.values().find(|b| b.kind.is_load()).unwrap().id;
        let picks = ports_of(&rig, load);
        rig = apply(&rig, &preset::connect(&rig, vec![picks[0], picks[1]])).unwrap();
        let id = *rig.members.keys().last().unwrap();
        let before_len = rig.members[&id].nominal_length(&rig.params).unwrap();
        let before_w = rig.members[&id].weight(&rig.params).unwrap();
        rig = apply(
            &rig,
            &TopoEdit::Reroute {
                id,
                path: vec![picks[0], bow_id, picks[1]],
            },
        )
        .unwrap();
        let m = &rig.members[&id];
        assert_eq!(m.segments.len(), 2);
        assert!((m.nominal_length(&rig.params).unwrap() - before_len).abs() < 1e-9);
        // Shackles are copied whole into the new run; everything else is split.
        let extra_shackles: f64 = m.segments[1]
            .components
            .iter()
            .filter(|c| matches!(c.kind, ComponentKind::Shackle { .. }))
            .map(|c| c.eval_weight(&rig.params).unwrap())
            .sum();
        assert!((m.weight(&rig.params).unwrap() - before_w - extra_shackles).abs() < 1e-9);
    }

    #[test]
    fn component_edits_insert_move_replace_and_resync_weight() {
        let rig = two_leg_bridle(1_000.0, 10.0, 8.0);
        let leg = *rig.members.keys().next().unwrap();
        let rig = apply(
            &rig,
            &TopoEdit::InsertComponent {
                member: leg,
                segment: 0,
                index: 0,
                component: Component::shackle("1"),
            },
        )
        .unwrap();
        assert_eq!(rig.members[&leg].segments[0].components.len(), 2);
        let rig = apply(
            &rig,
            &TopoEdit::MoveComponent {
                member: leg,
                segment: 0,
                from: 0,
                to: 1,
            },
        )
        .unwrap();
        assert!(matches!(
            rig.members[&leg].segments[0].components[1].kind,
            ComponentKind::Shackle { .. }
        ));
        let rig = apply(
            &rig,
            &TopoEdit::SetComponentExpr {
                member: leg,
                segment: 0,
                index: 0,
                field: ComponentField::Length,
                text: "12 ft".into(),
            },
        )
        .unwrap();
        let sling = &rig.members[&leg].segments[0].components[0];
        let expected = find_by_size(5).unwrap().sling_weight_lbs(12.0);
        assert!((sling.eval_weight(&rig.params).unwrap() - expected).abs() < 1e-9);
        let bad = apply(
            &rig,
            &TopoEdit::SetComponentExpr {
                member: leg,
                segment: 0,
                index: 0,
                field: ComponentField::Length,
                text: "100 lb".into(),
            },
        );
        assert!(bad.is_err());
    }
}
