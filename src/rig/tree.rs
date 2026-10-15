//! Rigging tree: members tagged top-down from the hook, with readable text.
//!
//! Step: screen work (Step 2 D2-1)
//! Theory: docs/rig-model.md "Rig tree and tags".
//! Inputs: an authored rig graph.
//! Outputs: per-member tag (`M1`…), tier, parent, from / via / to text, hardware.
//! Must not depend on: UI, dioxus, store, solve.

use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::IndexMap;

use crate::format::format_ft_in;

use super::Rig;
use super::component::{Component, ComponentKind};
use super::eval::ItemRef;
use super::id::{BodyId, MemberId, NodeId};
use super::member::Member;
use super::node::NodeKind;

/// Length at or below which a two-stop member is a hardware-only link.
const LINK_LENGTH_FT: f64 = 1e-6;

/// A place members hang from: a rigid body, or a free knot (master link).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeVertex {
    Body(BodyId),
    Knot(NodeId),
}

/// How the member is rigged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberShape {
    /// Two stops, end to end.
    Leg,
    /// Passes through a bearing on another body.
    Basket,
    /// Passes through a bearing on the body its ends are on.
    Choker,
    /// Two stops, hardware only, no length.
    Link,
}

impl MemberShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Leg => "leg",
            Self::Basket => "basket",
            Self::Choker => "choker",
            Self::Link => "link",
        }
    }
}

/// One member, placed in the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeMember {
    pub id: MemberId,
    /// `M1`, `M2`, … in tree order.
    pub tag: String,
    /// Hops from the hook: hook legs are 1.
    pub tier: usize,
    /// `None` when the member is not reachable from the hook.
    pub parent: Option<TreeVertex>,
    pub parent_label: String,
    /// The authored member name.
    pub label: String,
    /// Upper stop, e.g. "Bar top · Top hook LF".
    pub from: String,
    /// Interior bearings the member passes, other than `from`.
    pub via: Vec<String>,
    /// Lower end stops.
    pub to: Vec<String>,
    pub shape: MemberShape,
    /// Hardware top → bottom, e.g. `shackle 1-1/2″ · RS-11 Vertical 16'-0" · shackle 1-1/2″`.
    pub hardware: String,
}

impl TreeMember {
    /// "Hook → Bar top · Top hook LF", with bearings as "via …".
    pub fn route(&self) -> String {
        let mut out = self.from.clone();
        if !self.via.is_empty() {
            out.push_str(" → via ");
            out.push_str(&self.via.join(", "));
        }
        if !self.to.is_empty() {
            out.push_str(" → ");
            out.push_str(&self.to.join(", "));
        }
        out
    }
}

/// A group of members that hang from the same vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeGroup {
    pub parent: Option<TreeVertex>,
    pub label: String,
    pub tier: usize,
    pub members: Vec<MemberId>,
}

/// The rig as a tree rooted at the hook.
///
/// Tags are derived, never stored: parameter edits keep them, topology edits
/// may renumber them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RigTree {
    pub members: IndexMap<MemberId, TreeMember>,
    pub groups: Vec<TreeGroup>,
}

impl RigTree {
    /// Breadth-first from `rig.root`. A body is reached as a whole; a member
    /// hangs from the first reached vertex on its path.
    pub fn build(rig: &Rig) -> Self {
        let vertex_of = |node: NodeId| -> Option<TreeVertex> {
            let n = rig.nodes.get(&node)?;
            Some(match n.body {
                Some(b) => TreeVertex::Body(b),
                None => TreeVertex::Knot(node),
            })
        };

        let mut tree = RigTree::default();
        let mut tier_of: HashMap<TreeVertex, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        if let Some(root) = vertex_of(rig.root) {
            tier_of.insert(root, 0);
            queue.push_back(root);
        }

        let mut assigned: HashSet<MemberId> = HashSet::new();
        while let Some(vertex) = queue.pop_front() {
            let tier = tier_of[&vertex] + 1;
            let mut group = TreeGroup {
                parent: Some(vertex),
                label: vertex_label(rig, vertex),
                tier,
                members: Vec::new(),
            };
            for (mid, member) in &rig.members {
                if assigned.contains(mid) {
                    continue;
                }
                let Some(from) = member
                    .path
                    .iter()
                    .copied()
                    .find(|n| vertex_of(*n) == Some(vertex))
                else {
                    continue;
                };
                assigned.insert(*mid);
                for n in &member.path {
                    if let Some(v) = vertex_of(*n)
                        && !tier_of.contains_key(&v)
                    {
                        tier_of.insert(v, tier);
                        queue.push_back(v);
                    }
                }
                let tag = format!("M{}", tree.members.len() + 1);
                let item = place(rig, member, tag, tier, Some(vertex), from);
                tree.members.insert(*mid, item);
                group.members.push(*mid);
            }
            if !group.members.is_empty() {
                tree.groups.push(group);
            }
        }

        let orphans: Vec<&Member> = rig
            .members
            .values()
            .filter(|m| !assigned.contains(&m.id))
            .collect();
        if !orphans.is_empty() {
            let tier = tree.groups.iter().map(|g| g.tier).max().unwrap_or(0) + 1;
            let mut group = TreeGroup {
                parent: None,
                label: "Not connected to the hook".into(),
                tier,
                members: Vec::new(),
            };
            for member in orphans {
                let tag = format!("M{}", tree.members.len() + 1);
                let from = member.path.first().copied().unwrap_or(rig.root);
                let item = place(rig, member, tag, tier, None, from);
                tree.members.insert(member.id, item);
                group.members.push(member.id);
            }
            tree.groups.push(group);
        }
        tree
    }

    pub fn get(&self, id: MemberId) -> Option<&TreeMember> {
        self.members.get(&id)
    }

    pub fn tag_of(&self, id: MemberId) -> Option<&str> {
        self.members.get(&id).map(|m| m.tag.as_str())
    }

    /// Members hanging from `vertex`, in tag order.
    pub fn members_under(&self, vertex: TreeVertex) -> Vec<&TreeMember> {
        self.members
            .values()
            .filter(|m| m.parent == Some(vertex))
            .collect()
    }

    /// Tags a rated item or residual points at. A member gives its own tag; a
    /// node gives every member that touches it; a body is drawn by name, so it
    /// gives none.
    pub fn tags_for(&self, rig: &Rig, at: &ItemRef) -> Vec<String> {
        match at {
            ItemRef::Member { id, .. } => self.tag_of(*id).map(str::to_owned).into_iter().collect(),
            ItemRef::Node(node) => rig
                .members
                .values()
                .filter(|m| m.path.contains(node))
                .filter_map(|m| self.tag_of(m.id).map(str::to_owned))
                .collect(),
            ItemRef::Body(_) => Vec::new(),
        }
    }
}

fn place(
    rig: &Rig,
    member: &Member,
    tag: String,
    tier: usize,
    parent: Option<TreeVertex>,
    from: NodeId,
) -> TreeMember {
    let last = member.path.len().saturating_sub(1);
    let mut via = Vec::new();
    let mut to = Vec::new();
    for (i, n) in member.path.iter().enumerate() {
        if *n == from {
            continue;
        }
        if i == 0 || i == last {
            to.push(*n);
        } else {
            via.push(*n);
        }
    }
    to.dedup();
    TreeMember {
        id: member.id,
        tag,
        tier,
        parent,
        parent_label: parent
            .map(|v| vertex_label(rig, v))
            .unwrap_or_else(|| "Not connected to the hook".into()),
        label: member.label.clone(),
        from: stop_label(rig, from),
        via: grouped_stops(rig, &via),
        to: grouped_stops(rig, &to),
        shape: shape_of(rig, member),
        hardware: hardware_text(rig, member),
    }
}

fn vertex_label(rig: &Rig, vertex: TreeVertex) -> String {
    match vertex {
        TreeVertex::Body(b) => rig
            .bodies
            .get(&b)
            .map(|body| body.label.clone())
            .unwrap_or_else(|| "?".into()),
        TreeVertex::Knot(n) => rig
            .nodes
            .get(&n)
            .map(|node| node.label.clone())
            .unwrap_or_else(|| "?".into()),
    }
}

/// "Body · node"; the root, or a node named like its body, prints the body only.
fn stop_label(rig: &Rig, node: NodeId) -> String {
    let Some(n) = rig.nodes.get(&node) else {
        return "?".into();
    };
    match n.body.and_then(|b| rig.bodies.get(&b)) {
        Some(body) if n.kind.is_root() || body.label.eq_ignore_ascii_case(&n.label) => {
            body.label.clone()
        }
        Some(body) => format!("{} · {}", body.label, n.label),
        None => n.label.clone(),
    }
}

/// Consecutive stops on the same body share one body prefix:
/// "Al MDC enclosure · P1-front, P3-front".
fn grouped_stops(rig: &Rig, nodes: &[NodeId]) -> Vec<String> {
    let mut out: Vec<(Option<BodyId>, String, Vec<String>)> = Vec::new();
    for id in nodes {
        let Some(n) = rig.nodes.get(id) else {
            continue;
        };
        let body = n.body.and_then(|b| rig.bodies.get(&b));
        match (out.last_mut(), body) {
            (Some((Some(prev), _, names)), Some(b)) if *prev == b.id => names.push(n.label.clone()),
            (_, Some(b)) if b.label != n.label => {
                out.push((Some(b.id), b.label.clone(), vec![n.label.clone()]))
            }
            _ => out.push((None, n.label.clone(), Vec::new())),
        }
    }
    out.into_iter()
        .map(|(_, head, names)| {
            if names.is_empty() {
                head
            } else {
                format!("{head} · {}", names.join(", "))
            }
        })
        .collect()
}

fn shape_of(rig: &Rig, member: &Member) -> MemberShape {
    let n = member.path.len();
    if n == 2 {
        let length = member.nominal_length(&rig.params).unwrap_or(f64::INFINITY);
        return if length <= LINK_LENGTH_FT {
            MemberShape::Link
        } else {
            MemberShape::Leg
        };
    }
    let body_of = |id: &NodeId| rig.nodes.get(id).and_then(|node| node.body);
    let ends = [member.path[0], member.path[n - 1]];
    let interior_on_end_body = member.path[1..n - 1].iter().any(|id| {
        rig.nodes
            .get(id)
            .is_some_and(|node| matches!(node.kind, NodeKind::Bow { .. } | NodeKind::Edge { .. }))
            && body_of(id).is_some()
            && ends.iter().all(|e| body_of(e) == body_of(id))
    });
    if interior_on_end_body {
        MemberShape::Choker
    } else {
        MemberShape::Basket
    }
}

/// Segment hardware joined with " · ". Identical segments print once with
/// "(each side)"; different segments are separated by " / ".
pub fn hardware_text(rig: &Rig, member: &Member) -> String {
    let segments: Vec<String> = member
        .segments
        .iter()
        .map(|s| {
            s.components
                .iter()
                .map(|c| component_text(rig, c))
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .collect();
    match segments.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [first, rest @ ..] if rest.iter().all(|s| s == first) => format!("{first} (each side)"),
        _ => segments.join(" / "),
    }
}

fn component_text(rig: &Rig, c: &Component) -> String {
    let label = c.kind.label();
    let length = c
        .eval_length(&rig.params)
        .ok()
        .filter(|l| l.is_finite() && *l > LINK_LENGTH_FT);
    match (&c.kind, length) {
        (ComponentKind::Shackle { .. }, _) | (_, None) => label,
        (_, Some(l)) if c.adjust.is_some() => format!("{label} {} adj", format_ft_in(l, 16)),
        (_, Some(l)) => format!("{label} {}", format_ft_in(l, 16)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Hitch, Pick, SavedSpreader, SlingLayer};
    use crate::rig::edit::{ParamEdit, apply};
    use crate::rig::{duplo10, from_layers, two_leg_bridle};
    use uuid::Uuid;

    fn by_label<'a>(tree: &'a RigTree, label: &str) -> &'a TreeMember {
        tree.members
            .values()
            .find(|m| m.label == label)
            .unwrap_or_else(|| panic!("no member {label}"))
    }

    #[test]
    fn duplo10_tags_every_member_once_top_down() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        assert_eq!(tree.members.len(), 20);
        let tags: HashSet<&str> = tree.members.values().map(|m| m.tag.as_str()).collect();
        assert_eq!(tags.len(), 20);

        let hook = rig.nodes[&rig.root].body.unwrap();
        for k in 1..=6 {
            let m = tree
                .members
                .values()
                .find(|m| m.tag == format!("M{k}"))
                .unwrap();
            assert_eq!(m.parent, Some(TreeVertex::Body(hook)), "{}", m.label);
            assert_eq!(m.tier, 1);
            assert!(m.label.starts_with("Hook"), "{}", m.label);
        }
        assert_eq!(by_label(&tree, "Hook leg LF").tag, "M1");
        assert_eq!(by_label(&tree, "Hook leg LF").from, "Hook");
        assert_eq!(
            by_label(&tree, "Hook leg LF").to,
            vec!["Bar top · Top hook LF"]
        );
        assert_eq!(by_label(&tree, "V leg L front").tier, 2);
        assert_eq!(by_label(&tree, "V leg L front").parent_label, "Bar top");
        assert_eq!(by_label(&tree, "Chain leg P4 front").parent_label, "Bar C");
        assert_eq!(by_label(&tree, "Chain leg P2 front").tier, 3);

        let groups: Vec<&str> = tree.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(groups, ["Hook", "Bar top", "Bar C", "Bar L", "Bar R"]);
        let in_groups: usize = tree.groups.iter().map(|g| g.members.len()).sum();
        assert_eq!(in_groups, 20);
    }

    #[test]
    fn duplo10_baskets_hang_from_a_bar_bow() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        let baskets: Vec<&TreeMember> = tree
            .members
            .values()
            .filter(|m| m.label.starts_with("Basket"))
            .collect();
        assert_eq!(baskets.len(), 4);
        for b in baskets {
            assert_eq!(b.shape, MemberShape::Basket);
            assert!(b.from.contains(" bow "), "{}", b.from);
            assert!(b.via.is_empty());
            assert_eq!(b.to.len(), 1, "both ends on the enclosure: {:?}", b.to);
            assert!(b.to[0].starts_with("Al MDC enclosure · P"), "{:?}", b.to);
            assert!(b.hardware.ends_with("(each side)"), "{}", b.hardware);
        }
        let basket = by_label(&tree, "Basket strap P1–P3 (front)");
        assert_eq!(basket.from, "Bar L · Bar L bow F");
        assert_eq!(basket.to, vec!["Al MDC enclosure · P1-front, P3-front"]);
    }

    #[test]
    fn duplo10_hook_leg_hardware_text() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        assert_eq!(
            by_label(&tree, "Hook leg LF").hardware,
            "shackle 1-1/2″ · RS-11 Vertical 16'-0\" · shackle 1-1/2″"
        );
        assert_eq!(
            by_label(&tree, "Chain leg P2 front").hardware,
            "shackle 1″ · strap 3'-0\" · chain G8 17'-0\" adj · shackle 1″"
        );
        assert_eq!(
            by_label(&tree, "Hook leg LF").route(),
            "Hook → Bar top · Top hook LF"
        );
    }

    #[test]
    fn tags_survive_a_parameter_edit() {
        let rig = duplo10();
        let before = RigTree::build(&rig);
        let s12 = rig.param_named("s12").unwrap().id;
        let edited = apply(
            &rig,
            &ParamEdit::SetValue {
                id: s12,
                text: "8'-6\"".into(),
            },
        )
        .expect("edit applies");
        let after = RigTree::build(&edited);
        for (id, m) in &before.members {
            assert_eq!(after.tag_of(*id), Some(m.tag.as_str()), "{}", m.label);
        }
    }

    #[test]
    fn two_leg_bridle_is_m1_m2() {
        let rig = two_leg_bridle(1_000.0, 10.0, 8.0);
        let tree = RigTree::build(&rig);
        let tags: Vec<&str> = tree.members.values().map(|m| m.tag.as_str()).collect();
        assert_eq!(tags, ["M1", "M2"]);
        assert!(tree.members.values().all(|m| m.tier == 1));
        assert!(tree.members.values().all(|m| m.shape == MemberShape::Leg));
        assert_eq!(by_label(&tree, "leg A").route(), "Hook → Load · A");
    }

    #[test]
    fn from_layers_two_over_four_tiers_and_links() {
        let bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
        let layer = |index: u32, count: u32| SlingLayer {
            pick_id: Uuid::nil(),
            layer_index: index,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 60.0,
            sling_count: count,
            sling_length_ft: 10.0,
            pick_spacing_ft: None,
            pick_width_ft: None,
            spreader_span_ft: None,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: None,
            tare_lbs: 0.0,
        };
        let mut top = layer(0, 2);
        top.spreader_id = Some(bar.id);
        top.apex_shackle = Some("1-1/4".into());
        let bottom = layer(1, 4);
        let pick = Pick::new(Uuid::nil(), "2-over-4", 10_000.0);
        let rig = from_layers(&pick, &[top, bottom], &[bar]);
        let tree = RigTree::build(&rig);

        let apex = by_label(&tree, "L0 apex 0");
        assert_eq!(apex.tag, "M1");
        assert_eq!(apex.tier, 1);
        assert_eq!(apex.shape, MemberShape::Link);
        for k in 0..2 {
            let sling = by_label(&tree, &format!("L0 sling {k}"));
            assert_eq!(sling.tier, 2, "hangs from the collector knot");
            assert_eq!(sling.parent_label, "L0 collector 0");
            assert_eq!(sling.shape, MemberShape::Leg);
        }
        for k in 0..4 {
            let sling = by_label(&tree, &format!("L1 sling {k}"));
            assert_eq!(sling.tier, 3);
            assert_eq!(sling.parent_label, "Spreader L0");
        }
        assert_eq!(tree.members.len(), rig.members.len());
    }

    #[test]
    fn tags_for_points_lugs_at_touching_members() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        let lug = rig.node_named("Top hook LF").unwrap().id;
        let tags = tree.tags_for(&rig, &ItemRef::Node(lug));
        assert_eq!(tags, ["M1"]);
        let bar = rig.body_named("Bar top").unwrap().id;
        assert!(tree.tags_for(&rig, &ItemRef::Body(bar)).is_empty());
    }
}
