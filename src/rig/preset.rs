//! Palette presets: what a new spreader bar, load, master link or piece of
//! hardware looks like before the user edits it.
//!
//! Step: G (graph builder)
//! Theory: docs/step-g-graph-builder.md.
//! Inputs: the current rig (for unique names) and a palette choice.
//! Outputs: `TopoEdit`s ready for `topo::apply`, and blank starting rigs.
//! Must not depend on: UI, dioxus, store.

use uuid::Uuid;

use crate::catalog::find_by_size;
use crate::domain::Hitch;

use super::body::{BarRating, Body, BodyKind};
use super::component::{Adjust, Component, ComponentKind};
use super::id::{BodyId, NodeId};
use super::member::{Member, Segment};
use super::node::{Node, NodeKind};
use super::param::{Coord3, Expr, Param, ParamSource, ParamTable, Quantity};
use super::topo::TopoEdit;
use super::{Rig, edit::validate_name};

/// Default shackle on a new run, inches.
pub const DEFAULT_SHACKLE: &str = "1";
/// Default roundsling size on a new run.
pub const DEFAULT_SLING_SIZE: u8 = 5;
/// Default run length, ft.
pub const DEFAULT_RUN_FT: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickPattern {
    Two,
    FourCorners,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyPreset {
    SpreaderBar,
    LiftingBeam,
    Load { pattern: PickPattern },
    Frame,
}

impl BodyPreset {
    pub const ALL: [BodyPreset; 5] = [
        Self::SpreaderBar,
        Self::LiftingBeam,
        Self::Load {
            pattern: PickPattern::Two,
        },
        Self::Load {
            pattern: PickPattern::FourCorners,
        },
        Self::Frame,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SpreaderBar => "Spreader bar",
            Self::LiftingBeam => "Lifting beam",
            Self::Load {
                pattern: PickPattern::Two,
            } => "Load (2 pick points)",
            Self::Load {
                pattern: PickPattern::FourCorners,
            } => "Load (4 corners)",
            Self::Frame => "Frame / tare",
        }
    }
}

/// Port kinds a user can add to a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortKind {
    Lug,
    Bow,
    Edge,
}

impl PortKind {
    pub const ALL: [PortKind; 3] = [Self::Lug, Self::Bow, Self::Edge];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lug => "Lug",
            Self::Bow => "Bow",
            Self::Edge => "Edge",
        }
    }

    pub fn node_kind(self) -> NodeKind {
        match self {
            Self::Lug => NodeKind::Lug {
                plate_normal: super::node::Axis::Z,
                rating: None,
            },
            Self::Bow => NodeKind::Bow {
                bow_dia: Expr::c(0.25),
                mu: Expr::c(0.0),
            },
            Self::Edge => NodeKind::Edge {
                radius: Expr::c(1.0 / 12.0),
                mu: Expr::c(0.0),
                softener: false,
            },
        }
    }
}

/// Hardware a user can drop onto a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwarePreset {
    Shackle,
    RoundSling,
    Strap,
    Chain,
    WireRope,
    MasterLink,
    Turnbuckle,
}

impl HardwarePreset {
    pub const ALL: [HardwarePreset; 7] = [
        Self::Shackle,
        Self::RoundSling,
        Self::Strap,
        Self::Chain,
        Self::WireRope,
        Self::MasterLink,
        Self::Turnbuckle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Shackle => "Shackle",
            Self::RoundSling => "Roundsling",
            Self::Strap => "Strap",
            Self::Chain => "Chain",
            Self::WireRope => "Wire rope",
            Self::MasterLink => "Master link",
            Self::Turnbuckle => "Turnbuckle",
        }
    }

    pub fn component(self) -> Component {
        match self {
            Self::Shackle => Component::shackle(DEFAULT_SHACKLE),
            Self::RoundSling => {
                Component::roundsling(DEFAULT_SLING_SIZE, Hitch::Vertical, DEFAULT_RUN_FT)
            }
            Self::Strap => Component::strap(DEFAULT_RUN_FT, DEFAULT_RUN_FT * 1.0, 4.0),
            Self::Chain => Component::chain(80, 0.5, 5.0, 5.0 * 2.5, Adjust::new(0.0, 2.0, 1.0)),
            Self::WireRope => Component::new(
                ComponentKind::WireRope {
                    dia_in: Expr::c(0.75),
                },
                DEFAULT_RUN_FT,
                DEFAULT_RUN_FT * 1.04,
            ),
            Self::MasterLink => Component::master_link(0.4, 8.0),
            Self::Turnbuckle => Component::new(ComponentKind::Turnbuckle, 2.0, 15.0)
                .with_adjust(Adjust::new(-0.5, 0.5, 0.0)),
        }
    }
}

/// A rig holding only the hook.
pub fn hook_only(name: &str, project_id: Uuid) -> Rig {
    let hook = Body::new("Hook", BodyKind::Hook, 0.0);
    let root = Node::root("hook", hook.id);
    let mut rig = Rig::new(name, root.id);
    rig.project_id = project_id;
    rig.bodies.insert(hook.id, hook);
    rig.nodes.insert(root.id, root);
    rig
}

/// A new rig: the hook and a load with two pick points, nothing wired yet.
pub fn blank_rig(name: &str, project_id: Uuid) -> Rig {
    let rig = hook_only(name, project_id);
    let edit = add_body(
        &rig,
        BodyPreset::Load {
            pattern: PickPattern::Two,
        },
    );
    super::topo::apply(&rig, &edit).expect("load preset fits a hook-only rig")
}

/// `AddBody` for a palette body, with uniquely named `Assumed` parameters.
pub fn add_body(rig: &Rig, preset: BodyPreset) -> TopoEdit {
    let mut params = Params::new(rig);
    let (body, nodes) = match preset {
        BodyPreset::SpreaderBar => {
            let n = next_number(rig, |k| matches!(k, BodyKind::SpreaderBar { .. }));
            let p = params.prefix("bar", n);
            let span = params.add(&format!("{p}_span"), Quantity::Length, 10.0);
            let weight = params.add(&format!("{p}_weight"), Quantity::Weight, 250.0);
            let body = Body::new(
                format!("Spreader {n}"),
                BodyKind::SpreaderBar {
                    span: span.into(),
                    rating: BarRating::new(20_000),
                },
                weight,
            );
            let half = || Expr::from(span) / 2.0;
            let nodes = vec![
                Node::lug("Top A", body.id, Coord3::new(-half(), 0.0, 0.0)),
                Node::lug("Top B", body.id, Coord3::new(half(), 0.0, 0.0)),
                Node::lug("Bottom A", body.id, Coord3::new(-half(), 0.0, 0.0)),
                Node::lug("Bottom B", body.id, Coord3::new(half(), 0.0, 0.0)),
            ];
            (body, nodes)
        }
        BodyPreset::LiftingBeam => {
            let n = next_number(rig, |k| matches!(k, BodyKind::LiftingBeam { .. }));
            let p = params.prefix("beam", n);
            let span = params.add(&format!("{p}_span"), Quantity::Length, 10.0);
            let weight = params.add(&format!("{p}_weight"), Quantity::Weight, 400.0);
            let body = Body::new(
                format!("Lifting beam {n}"),
                BodyKind::LiftingBeam {
                    span: span.into(),
                    rating: BarRating::new(20_000),
                },
                weight,
            );
            let half = || Expr::from(span) / 2.0;
            let nodes = vec![
                Node::lug("Top", body.id, Coord3::new(0.0, 0.0, 1.0)),
                Node::lug("Bottom A", body.id, Coord3::new(-half(), 0.0, 0.0)),
                Node::lug("Bottom B", body.id, Coord3::new(half(), 0.0, 0.0)),
            ];
            (body, nodes)
        }
        BodyPreset::Load { pattern } => {
            let n = next_number(rig, BodyKind::is_load);
            let p = if n == 1 {
                params.prefix("load", 0)
            } else {
                params.prefix("load", n)
            };
            let weight = params.add(&format!("{p}_weight"), Quantity::Weight, 10_000.0);
            let length = params.add(&format!("{p}_length"), Quantity::Length, 10.0);
            let width = params.add(&format!("{p}_width"), Quantity::Length, 4.0);
            let height = params.add(&format!("{p}_height"), Quantity::Length, 4.0);
            let label = if n == 1 {
                "Load".to_owned()
            } else {
                format!("Load {n}")
            };
            let body = Body::new(
                label,
                BodyKind::Load {
                    length: length.into(),
                    width: width.into(),
                    height: height.into(),
                },
                weight,
            )
            .with_cg(Coord3::new(0.0, 0.0, Expr::from(height) / 2.0));
            let hl = || Expr::from(length) / 2.0;
            let hw = || Expr::from(width) / 2.0;
            let top = || Expr::from(height);
            let nodes = match pattern {
                PickPattern::Two => vec![
                    Node::lug("P1", body.id, Coord3::new(-hl(), 0.0, top())),
                    Node::lug("P2", body.id, Coord3::new(hl(), 0.0, top())),
                ],
                PickPattern::FourCorners => vec![
                    Node::lug("P1", body.id, Coord3::new(-hl(), -hw(), top())),
                    Node::lug("P2", body.id, Coord3::new(hl(), -hw(), top())),
                    Node::lug("P3", body.id, Coord3::new(-hl(), hw(), top())),
                    Node::lug("P4", body.id, Coord3::new(hl(), hw(), top())),
                ],
            };
            (body, nodes)
        }
        BodyPreset::Frame => {
            let n = next_number(rig, |k| matches!(k, BodyKind::Frame));
            let p = params.prefix("frame", n);
            let weight = params.add(&format!("{p}_weight"), Quantity::Weight, 100.0);
            let body = Body::new(format!("Frame {n}"), BodyKind::Frame, weight);
            let nodes = vec![Node::lug("Hang", body.id, Coord3::origin())];
            (body, nodes)
        }
    };
    TopoEdit::AddBody {
        body,
        nodes,
        params: params.into_new(),
    }
}

/// `AddNode` for a free master link (a knot several members meet at).
pub fn master_link(rig: &Rig) -> TopoEdit {
    let n = rig.nodes.values().filter(|n| n.kind.is_free()).count() + 1;
    TopoEdit::AddNode {
        node: Node::free(format!("Master link {n}")),
    }
}

/// `AddNode` for a new port on `body`, at the body origin.
pub fn port(rig: &Rig, body: BodyId, kind: PortKind) -> TopoEdit {
    let n = rig
        .nodes
        .values()
        .filter(|n| n.body == Some(body) && n.kind.label() == kind.node_kind().label())
        .count()
        + 1;
    let mut node = Node::new(format!("{} {n}", kind.label()), kind.node_kind());
    node.body = Some(body);
    TopoEdit::AddNode { node }
}

/// `Connect` along `path` with the default run: shackle, roundsling, shackle.
/// Through bows the sling is a basket, split evenly, with shackles only at the ends.
pub fn connect(rig: &Rig, path: Vec<NodeId>) -> TopoEdit {
    let spans = path.len().saturating_sub(1).max(1);
    let hitch = if spans > 1 {
        Hitch::Basket
    } else {
        Hitch::Vertical
    };
    let piece = DEFAULT_RUN_FT / spans as f64;
    let segments = (0..spans)
        .map(|i| {
            let mut components = Vec::new();
            if i == 0 {
                components.push(Component::shackle(DEFAULT_SHACKLE));
            }
            components.push(Component::roundsling(DEFAULT_SLING_SIZE, hitch, piece));
            if i + 1 == spans {
                components.push(Component::shackle(DEFAULT_SHACKLE));
            }
            Segment::new(components)
        })
        .collect();
    let label = match (path.first(), path.last()) {
        (Some(a), Some(b)) => format!("{} to {}", stop_label(rig, *a), stop_label(rig, *b)),
        _ => "Member".into(),
    };
    TopoEdit::Connect {
        member: Member::new(label, path, segments),
    }
}

/// The same roundsling at a new size or hitch; weight follows the catalog.
pub fn resize_roundsling(c: &Component, size: u8, hitch: Hitch, params: &ParamTable) -> Component {
    let length = c.eval_length(params).unwrap_or(0.0);
    let mut next = Component::roundsling(size, hitch, length);
    next.length = c.length.clone();
    next.weight = Expr::c(
        find_by_size(size)
            .map(|r| r.sling_weight_lbs(length))
            .unwrap_or(0.0),
    );
    next
}

/// `preset` in place of `old`. Length-bearing hardware keeps the old length;
/// a catalog roundsling's weight follows it, other weights scale per foot.
pub fn replace_with(old: &Component, preset: HardwarePreset, params: &ParamTable) -> Component {
    let mut next = preset.component();
    let old_len = old.eval_length(params).unwrap_or(0.0);
    let base_len = next.length.eval(params).unwrap_or(0.0);
    if old_len > 0.0 && base_len > 0.0 && next.adjust.is_none() {
        let per_ft = next.weight.eval(params).unwrap_or(0.0) / base_len;
        next.length = old.length.clone();
        next.weight = Expr::c(per_ft * old_len);
    }
    next
}

/// "Body · port", or just the body for the hook.
pub fn stop_label(rig: &Rig, node: NodeId) -> String {
    let Some(n) = rig.nodes.get(&node) else {
        return "?".into();
    };
    match n.body.and_then(|b| rig.bodies.get(&b)) {
        Some(body) if n.kind.is_root() => body.label.clone(),
        Some(body) => format!("{} {}", body.label, n.label),
        None => n.label.clone(),
    }
}

fn next_number(rig: &Rig, is_kind: impl Fn(&BodyKind) -> bool) -> usize {
    rig.bodies.values().filter(|b| is_kind(&b.kind)).count() + 1
}

/// Collects new parameters, keeping names unique against the rig and each other.
struct Params {
    table: ParamTable,
    new: Vec<Param>,
}

impl Params {
    fn new(rig: &Rig) -> Self {
        Self {
            table: rig.params.clone(),
            new: Vec::new(),
        }
    }

    /// `base{n}` (or `base` when `n` is 0), bumped until no parameter uses it.
    fn prefix(&self, base: &str, n: usize) -> String {
        let mut k = n;
        loop {
            let p = if k == 0 {
                base.to_owned()
            } else {
                format!("{base}{k}")
            };
            let prefix = format!("{p}_");
            if !self.table.values().any(|x| x.name.starts_with(&prefix)) {
                return p;
            }
            k = if k == 0 { 2 } else { k + 1 };
        }
    }

    fn add(&mut self, name: &str, quantity: Quantity, nominal: f64) -> super::ParamId {
        let mut candidate = name.to_owned();
        let mut k = 2;
        while validate_name(&candidate, &self.table, None).is_err() {
            candidate = format!("{name}_{k}");
            k += 1;
        }
        let name = candidate;
        let mut param = Param::new(name, quantity, nominal);
        param.source = ParamSource::Assumed;
        let id = param.id;
        self.table.insert(id, param.clone());
        self.new.push(param);
        id
    }

    fn into_new(self) -> Vec<Param> {
        self.new
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::topo::apply;

    #[test]
    fn repeated_presets_get_unique_names() {
        let mut rig = blank_rig("names", Uuid::nil());
        for _ in 0..3 {
            rig = apply(&rig, &add_body(&rig, BodyPreset::SpreaderBar)).unwrap();
            rig = apply(
                &rig,
                &add_body(
                    &rig,
                    BodyPreset::Load {
                        pattern: PickPattern::FourCorners,
                    },
                ),
            )
            .unwrap();
        }
        let mut names: Vec<&str> = rig.params.values().map(|p| p.name.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count);
        assert!(rig.param_named("load_weight").is_some());
        assert!(rig.param_named("bar3_span").is_some());
        assert!(
            rig.params
                .values()
                .all(|p| p.source == ParamSource::Assumed)
        );
    }

    #[test]
    fn blank_rig_is_a_draft_with_hook_and_load() {
        let rig = blank_rig("blank", Uuid::nil());
        assert_eq!(rig.bodies.len(), 2);
        assert_eq!(rig.nodes.len(), 3);
        assert!(rig.members.is_empty());
        assert!(!rig.validate_full().errors.is_empty());
    }

    #[test]
    fn hook_to_two_points_solves_like_the_bridle_fixture() {
        let mut rig = blank_rig("bridle", Uuid::nil());
        let picks: Vec<NodeId> = rig
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Lug { .. }))
            .map(|n| n.id)
            .collect();
        for pick in picks {
            rig = apply(&rig, &connect(&rig, vec![rig.root, pick])).unwrap();
        }
        rig.validate().unwrap();
        let solved = crate::rig::solve::solve(&rig).unwrap();
        assert!(solved.converged);
        assert!((solved.hook_load_lbs - rig.weights().unwrap().total_below_root_lbs).abs() < 1e-6);
    }

    #[test]
    fn replacing_hardware_keeps_length_and_scales_weight() {
        let t = ParamTable::new();
        let sling = Component::roundsling(5, Hitch::Vertical, 14.0);
        let strap = replace_with(&sling, HardwarePreset::Strap, &t);
        assert!(matches!(strap.kind, ComponentKind::Strap { .. }));
        assert!((strap.eval_length(&t).unwrap() - 14.0).abs() < 1e-12);
        assert!((strap.eval_weight(&t).unwrap() - 14.0).abs() < 1e-12);
        let back = replace_with(&strap, HardwarePreset::RoundSling, &t);
        let expected = find_by_size(DEFAULT_SLING_SIZE)
            .unwrap()
            .sling_weight_lbs(14.0);
        assert!((back.eval_weight(&t).unwrap() - expected).abs() < 1e-9);
        let shackle = replace_with(&sling, HardwarePreset::Shackle, &t);
        assert_eq!(shackle.eval_length(&t).unwrap(), 0.0);
    }

    #[test]
    fn every_hardware_preset_has_positive_length_or_is_a_shackle() {
        let t = ParamTable::new();
        for preset in HardwarePreset::ALL {
            let c = preset.component();
            let length = c.eval_length(&t).unwrap();
            assert!(
                length > 0.0 || matches!(c.kind, ComponentKind::Shackle { .. }),
                "{}",
                preset.label()
            );
        }
    }
}
