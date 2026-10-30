//! Load-up builder: build a rig by **gathering** open points upward.
//!
//! Step: G2 (load-up builder)
//! Theory: docs/step-g2-load-up-builder.md.
//! Inputs: an authored rig, a set of open points, a gather kind and options.
//! Outputs: a [`GatherPlan`] — the edits (one undo step) and the rig after them.
//! Must not depend on: UI, dioxus, store.
//!
//! A rigger reasons from the load up: "these two lugs go to a spreader, the
//! spreader goes to the hook". So the builder never asks for coordinates or
//! wires. Every pick point starts **open**. The user selects open points and
//! gathers them into something above — a master link, a spreader bar, a
//! lifting beam, a basket, or the hook — and the new thing's top becomes the
//! next open point. When nothing is open and everything hangs from the hook,
//! the rig is done.
//!
//! Each gather sizes what it creates from a quick **design pose** (loads level
//! at the origin, every gathered item placed over what it holds):
//!
//! * bar span = plan distance between the two clusters it spans, so the drop
//!   legs hang plumb;
//! * bridle legs meet over the **CG** of what they hold (so the lift hangs
//!   level) with the flattest leg at the target sling angle;
//! * lengths round **up** to the inch;
//! * slings and shackles are picked from the catalog for the estimated leg
//!   tension when `auto_size` is on.
//!
//! The design pose is only for sizing. The real geometry and forces always
//! come from the solver after the edits are applied.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::catalog::{POLYESTER_ROUNDSLINGS, SHACKLES};
use crate::domain::Hitch;

use super::Rig;
use super::body::BodyKind;
use super::component::Component;
use super::edit::ParamEdit;
use super::id::{BodyId, MemberId, NodeId, ParamId};
use super::member::Segment;
use super::node::{Node, NodeKind};
use super::param::{Coord3, Expr, ParamSource};
use super::preset::{self, BodyPreset, DEFAULT_SHACKLE, DEFAULT_SLING_SIZE};
use super::session::{RigEdit, apply_rig_edit};
use super::topo::TopoEdit;
use super::tree::TreeVertex;

/// Points closer than this in plan are "directly below" (ft).
const PLAN_EPS_FT: f64 = 1e-6;
/// A bar shorter than this is refused: use a master link instead (ft).
const MIN_BAR_SPAN_FT: f64 = 0.5;

// ---------------------------------------------------------------- options

/// How a gather sizes what it creates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatherOptions {
    /// Target angle of the flattest bridle leg, from horizontal (deg).
    /// 60° is the usual design target; below 30° is not allowed.
    pub angle_deg: f64,
    /// Length of a plumb drop leg, e.g. from a spreader lug to a pick point (ft).
    pub drop_ft: f64,
    /// Pick the smallest catalog roundsling and shackle for the estimated
    /// tension. Off: the default sizes (RS-5, 1″ shackle).
    pub auto_size: bool,
}

impl Default for GatherOptions {
    fn default() -> Self {
        Self {
            angle_deg: 60.0,
            drop_ft: 6.0,
            auto_size: true,
        }
    }
}

impl GatherOptions {
    fn tan(&self) -> f64 {
        self.angle_deg.clamp(30.0, 89.0).to_radians().tan()
    }

    fn drop(&self) -> f64 {
        if self.drop_ft.is_finite() && self.drop_ft > 0.0 {
            self.drop_ft
        } else {
            GatherOptions::default().drop_ft
        }
    }
}

// ---------------------------------------------------------------- kinds

/// What open points can be gathered into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GatherKind {
    /// A master link (free junction) over the points; legs to each. Its link
    /// becomes the new open point.
    MasterLink,
    /// A spreader bar over two clusters of points; its top lugs become open.
    SpreaderBar,
    /// A lifting beam over two clusters; its top lug becomes open.
    LiftingBeam,
    /// One sling through bows / edges (under the load) to a master link above.
    Basket,
    /// Legs from the hook to each point (a single master link or top lug
    /// hangs straight on the hook with a shackle).
    Hook,
}

impl GatherKind {
    pub const ALL: [GatherKind; 5] = [
        Self::MasterLink,
        Self::SpreaderBar,
        Self::LiftingBeam,
        Self::Basket,
        Self::Hook,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::MasterLink => "Master link",
            Self::SpreaderBar => "Spreader bar",
            Self::LiftingBeam => "Lifting beam",
            Self::Basket => "Basket",
            Self::Hook => "To hook",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Self::MasterLink => "Bridle legs from one master link, placed over the CG",
            Self::SpreaderBar => "A bar spanning the points, plumb drop legs",
            Self::LiftingBeam => "A beam spanning the points, one top lug",
            Self::Basket => "One sling through bows / edges to a master link",
            Self::Hook => "Hang the points from the hook",
        }
    }
}

// ---------------------------------------------------------------- open points

/// A point that still needs something above it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPoint {
    pub node: NodeId,
    pub owner: TreeVertex,
    /// "Load", "Spreader 1", "Master link 2" …
    pub owner_label: String,
    /// "P1", "Top A" … (for a master link, its own name).
    pub label: String,
    /// A bow or edge: can only be gathered into a basket.
    pub bearing: bool,
    /// On something that already hangs from the hook: an unused, optional point.
    pub spare: bool,
    /// Design-pose position (ft), for ordering and display.
    pub at: [f64; 3],
}

/// Where the build stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BuildStatus {
    /// Points still to gather, grouped by owner in rig order.
    pub open: Vec<OpenPoint>,
    /// Unused points on things that already hang from the hook.
    pub spare: Vec<OpenPoint>,
    /// Bodies (other than the hook) not yet hanging from the hook.
    pub unhung: Vec<BodyId>,
    /// Nothing open, everything hangs from the hook, and there is rigging.
    pub done: bool,
}

pub fn status(rig: &Rig) -> BuildStatus {
    let points = points(rig);
    let reached = reached(rig);
    let unhung: Vec<BodyId> = rig
        .bodies
        .values()
        .filter(|b| !matches!(b.kind, BodyKind::Hook))
        .filter(|b| !reached.contains(&TreeVertex::Body(b.id)))
        .map(|b| b.id)
        .collect();
    let (spare, open): (Vec<_>, Vec<_>) = points.into_iter().partition(|p| p.spare);
    let done = open.is_empty() && unhung.is_empty() && !rig.members.is_empty();
    BuildStatus {
        open,
        spare,
        unhung,
        done,
    }
}

/// Every open or spare point in rig order.
pub fn points(rig: &Rig) -> Vec<OpenPoint> {
    let used: HashSet<NodeId> = rig
        .members
        .values()
        .flat_map(|m| m.path.iter().copied())
        .collect();
    let reached = reached(rig);
    let rank = ranks(rig);
    let hook_body = rig.nodes.get(&rig.root).and_then(|n| n.body);
    let mut design = Design::new(rig);
    let mut out = Vec::new();
    for node in rig.nodes.values() {
        if node.id == rig.root || node.kind.is_root() {
            continue;
        }
        let Some(owner) = vertex_of(rig, node.id) else {
            continue;
        };
        let open = match (node.body, &node.kind) {
            (None, NodeKind::Free) => !held_from_above(rig, &rank, node.id),
            (Some(body), kind) => {
                if Some(body) == hook_body || used.contains(&node.id) {
                    false
                } else {
                    match rig.bodies.get(&body).map(|b| &b.kind) {
                        Some(BodyKind::Load { .. } | BodyKind::Frame) => true,
                        Some(BodyKind::SpreaderBar { .. } | BodyKind::LiftingBeam { .. }) => {
                            !node.label.starts_with("Bottom") && !kind.is_bearing_capable()
                        }
                        _ => false,
                    }
                }
            }
            _ => false,
        };
        if !open {
            continue;
        }
        out.push(OpenPoint {
            node: node.id,
            owner,
            owner_label: vertex_label(rig, owner),
            label: node.label.clone(),
            bearing: node.kind.is_bearing_capable(),
            spare: reached.contains(&owner),
            at: design.point(node.id),
        });
    }
    out
}

// ---------------------------------------------------------------- plans

/// The edits of one gather (apply as one undo step) and the rig after them.
#[derive(Debug, Clone, PartialEq)]
pub struct GatherPlan {
    pub edits: Vec<RigEdit>,
    pub rig: Rig,
    /// Open points the gather created (a master link, a bar's top lugs).
    pub new_points: Vec<NodeId>,
    /// One line for the status bar, e.g. "Spreader 1, span 10'-0", 2 legs".
    pub summary: String,
}

struct Plan {
    rig: Rig,
    edits: Vec<RigEdit>,
}

impl Plan {
    fn new(rig: &Rig) -> Self {
        Self {
            rig: rig.clone(),
            edits: Vec::new(),
        }
    }

    fn push(&mut self, edit: impl Into<RigEdit>) -> Result<(), String> {
        let edit = edit.into();
        self.rig = apply_rig_edit(&self.rig, &edit).map_err(|e| e.message().to_owned())?;
        self.edits.push(edit);
        Ok(())
    }

    fn finish(self, new_points: Vec<NodeId>, summary: String) -> GatherPlan {
        GatherPlan {
            edits: self.edits,
            rig: self.rig,
            new_points,
            summary,
        }
    }
}

/// Why `ends` can't be gathered into `kind`, or `Ok`.
pub fn check(rig: &Rig, ends: &[NodeId], kind: GatherKind) -> Result<(), String> {
    if ends.is_empty() {
        return Err("select the open points to gather".into());
    }
    let mut seen = HashSet::new();
    if !ends.iter().all(|e| seen.insert(*e)) {
        return Err("a point is selected twice".into());
    }
    let open: HashMap<NodeId, OpenPoint> = points(rig).into_iter().map(|p| (p.node, p)).collect();
    let mut chosen = Vec::with_capacity(ends.len());
    for id in ends {
        let Some(p) = open.get(id) else {
            let name = rig.nodes.get(id).map_or("that point", |n| n.label.as_str());
            return Err(format!("'{name}' is already rigged"));
        };
        chosen.push(p);
    }
    let bearings = chosen.iter().filter(|p| p.bearing).count();
    match kind {
        GatherKind::Basket => {
            if bearings != chosen.len() {
                return Err("a basket passes through bows or edges only".into());
            }
        }
        _ if bearings > 0 => {
            return Err("bows and edges take a basket; pick lugs or master links".into());
        }
        _ => {}
    }
    match kind {
        GatherKind::MasterLink if ends.len() < 2 => {
            Err("a master link gathers two or more points".into())
        }
        GatherKind::SpreaderBar | GatherKind::LiftingBeam => {
            if ends.len() < 2 {
                return Err(format!("a {} spans two or more points", kind_noun(kind)));
            }
            let at: Vec<[f64; 3]> = chosen.iter().map(|p| p.at).collect();
            let (a, b) = split_along_axis(&at);
            let span = plan_dist(centroid(&at, &a), centroid(&at, &b));
            if span < MIN_BAR_SPAN_FT {
                Err(format!(
                    "those points are too close together for a {}; use a master link",
                    kind_noun(kind)
                ))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// Gather `ends` into a new item of `kind` above them.
pub fn gather(
    rig: &Rig,
    ends: &[NodeId],
    kind: GatherKind,
    opts: GatherOptions,
) -> Result<GatherPlan, String> {
    check(rig, ends, kind)?;
    let mut plan = Plan::new(rig);
    let (at, holds) = {
        let mut d = Design::new(rig);
        let at: Vec<[f64; 3]> = ends.iter().map(|n| d.point(*n)).collect();
        let owners: Vec<TreeVertex> = ends.iter().filter_map(|n| vertex_of(rig, *n)).collect();
        (at, d.mass_below(&owners))
    };
    // Over the CG when the points hold whole items; over the points themselves
    // when they are only part of something (two of four corners, one basket of
    // two), since the rest of its weight goes elsewhere.
    let all: Vec<usize> = (0..at.len()).collect();
    let apex_xy = match holds.cg_xy {
        Some(cg) if kind != GatherKind::Basket && holds_whole(rig, ends) => cg,
        _ => centroid(&at, &all),
    };
    match kind {
        GatherKind::MasterLink => {
            let link = new_master_link(&mut plan)?;
            let legs = bridle(&mut plan, link, apex_xy, ends, &at, holds.weight, opts)?;
            let name = node_label(&plan.rig, link);
            Ok(plan.finish(vec![link], format!("{name}, {legs} legs")))
        }
        GatherKind::Hook => {
            let root = plan.rig.root;
            if ends.len() == 1 {
                link_to(&mut plan, root, ends[0], holds.weight, opts)?;
                Ok(plan.finish(Vec::new(), "Hung on the hook".into()))
            } else {
                let legs = bridle(&mut plan, root, apex_xy, ends, &at, holds.weight, opts)?;
                Ok(plan.finish(Vec::new(), format!("{legs} legs to the hook")))
            }
        }
        GatherKind::SpreaderBar | GatherKind::LiftingBeam => {
            let (tops, summary) = bar(&mut plan, kind, ends, &at, &holds, opts)?;
            Ok(plan.finish(tops, summary))
        }
        GatherKind::Basket => {
            let link = new_master_link(&mut plan)?;
            basket(&mut plan, link, apex_xy, ends, &at, holds.weight, opts)?;
            let name = node_label(&plan.rig, link);
            Ok(plan.finish(vec![link], format!("Basket to {name}")))
        }
    }
}

/// Every open point of every item the ends belong to is among the ends.
fn holds_whole(rig: &Rig, ends: &[NodeId]) -> bool {
    let owners: HashSet<TreeVertex> = ends.iter().filter_map(|n| vertex_of(rig, *n)).collect();
    points(rig)
        .iter()
        .filter(|p| !p.spare && !p.bearing && owners.contains(&p.owner))
        .all(|p| ends.contains(&p.node))
}

// ---------------------------------------------------------------- ungather

/// Something a gather built, which can be taken apart again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Built {
    /// The members hanging on the hook.
    Hook,
    /// A spreader bar / lifting beam, or a master link (knot).
    Item(TreeVertex),
}

/// What can be ungathered, top (nearest the hook) first.
pub fn built(rig: &Rig) -> Vec<(Built, String)> {
    let rank = ranks(rig);
    let mut items: Vec<(usize, Built, String)> = Vec::new();
    for body in rig.bodies.values() {
        if body.kind.is_gear() {
            let v = TreeVertex::Body(body.id);
            items.push((rank_of(&rank, v), Built::Item(v), body.label.clone()));
        }
    }
    for node in rig.nodes.values() {
        if node.kind.is_free() && node.body.is_none() {
            let v = TreeVertex::Knot(node.id);
            items.push((rank_of(&rank, v), Built::Item(v), node.label.clone()));
        }
    }
    items.sort_by_key(|(r, _, _)| std::cmp::Reverse(*r));
    let mut out = Vec::new();
    if rig.members.values().any(|m| m.path.contains(&rig.root)) {
        out.push((Built::Hook, "Hook connection".to_owned()));
    }
    out.extend(items.into_iter().map(|(_, b, l)| (b, l)));
    out
}

/// Take `what` apart: its members (above and below) go, and the points it
/// held are open again. Parameters only it used are deleted.
pub fn ungather(rig: &Rig, what: Built) -> Result<GatherPlan, String> {
    let mut plan = Plan::new(rig);
    match what {
        Built::Hook => {
            let root = rig.root;
            let doomed: Vec<MemberId> = rig
                .members
                .values()
                .filter(|m| m.path.contains(&root))
                .map(|m| m.id)
                .collect();
            if doomed.is_empty() {
                return Err("nothing hangs on the hook".into());
            }
            for id in doomed {
                plan.push(TopoEdit::RemoveMember { id })?;
            }
            Ok(plan.finish(Vec::new(), "Unhooked".into()))
        }
        Built::Item(TreeVertex::Body(id)) => {
            let body = rig.bodies.get(&id).ok_or("that item no longer exists")?;
            if !body.kind.is_gear() {
                return Err(format!("{} isn't something a gather built", body.label));
            }
            let label = body.label.clone();
            let mut owned: Vec<ParamId> = body.weight.params().into_iter().collect();
            if let BodyKind::SpreaderBar { span, .. } | BodyKind::LiftingBeam { span, .. } =
                &body.kind
            {
                owned.extend(span.params());
            }
            for node in rig.nodes.values().filter(|n| n.body == Some(id)) {
                owned.extend(node.local.params());
            }
            plan.push(TopoEdit::RemoveBody { id })?;
            delete_unused(&mut plan, &owned);
            Ok(plan.finish(Vec::new(), format!("Removed {label}")))
        }
        Built::Item(TreeVertex::Knot(id)) => {
            let label = node_label(rig, id);
            for m in rig.members.values().filter(|m| m.path.contains(&id)) {
                plan.push(TopoEdit::RemoveMember { id: m.id })?;
            }
            plan.push(TopoEdit::RemoveNode { id })?;
            Ok(plan.finish(Vec::new(), format!("Removed {label}")))
        }
    }
}

/// Remove every gathered item and every member: back to the bare load(s).
pub fn clear(rig: &Rig) -> Result<GatherPlan, String> {
    let mut plan = Plan::new(rig);
    while let Some((what, _)) = built(&plan.rig).into_iter().next() {
        let step = ungather(&plan.rig, what)?;
        plan.rig = step.rig;
        plan.edits.extend(step.edits);
    }
    let rest: Vec<MemberId> = plan.rig.members.keys().copied().collect();
    for id in rest {
        plan.push(TopoEdit::RemoveMember { id })?;
    }
    Ok(plan.finish(Vec::new(), "Rigging cleared".into()))
}

fn delete_unused(plan: &mut Plan, params: &[ParamId]) {
    let mut seen = HashSet::new();
    for id in params {
        if !seen.insert(*id) {
            continue;
        }
        // Refused while still in use — that's the check we want.
        let _ = plan.push(ParamEdit::Delete { id: *id });
    }
}

// ---------------------------------------------------------------- recipes

/// Whole rigs in one click, built from gathers on one load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Recipe {
    /// Two points: two legs to the hook.
    TwoLegBridle,
    /// Four points: four legs to a master link on the hook.
    FourLegBridle,
    /// A spreader bar over the points (two or four), its top legs to the hook.
    Spreader,
    /// Four points: a bar across each end, one bar over both (an "H").
    SpreaderOverTwo,
    /// A lifting beam over the points, its top lug on the hook.
    LiftingBeam,
    /// Two baskets under the load at the quarter points, to the hook.
    TwoBaskets,
}

impl Recipe {
    pub const ALL: [Recipe; 6] = [
        Self::TwoLegBridle,
        Self::FourLegBridle,
        Self::Spreader,
        Self::SpreaderOverTwo,
        Self::LiftingBeam,
        Self::TwoBaskets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TwoLegBridle => "2-leg bridle",
            Self::FourLegBridle => "4-leg bridle",
            Self::Spreader => "Spreader bar",
            Self::SpreaderOverTwo => "Bar over two bars",
            Self::LiftingBeam => "Lifting beam",
            Self::TwoBaskets => "Two baskets",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Self::TwoLegBridle => "Two legs from the hook to the pick points",
            Self::FourLegBridle => "Four legs to a master link (B30.9: rate on two legs)",
            Self::Spreader => "Spreader bar over the points, two legs to the hook",
            Self::SpreaderOverTwo => "A bar across each end, one bar over both",
            Self::LiftingBeam => "Lifting beam over the points, hung on its top lug",
            Self::TwoBaskets => "Two slings under the load at the quarter points",
        }
    }
}

/// Recipes that fit `load` as it stands (its lugs all open).
pub fn recipes_for(rig: &Rig, load: BodyId) -> Vec<Recipe> {
    let lugs = open_lugs(rig, load);
    let all_open = rig
        .nodes
        .values()
        .filter(|n| n.body == Some(load) && !n.kind.is_bearing_capable())
        .count()
        == lugs.len();
    if !all_open
        || !matches!(
            rig.bodies.get(&load).map(|b| &b.kind),
            Some(BodyKind::Load { .. })
        )
    {
        return Vec::new();
    }
    match lugs.len() {
        2 => vec![
            Recipe::TwoLegBridle,
            Recipe::Spreader,
            Recipe::LiftingBeam,
            Recipe::TwoBaskets,
        ],
        4 => vec![
            Recipe::FourLegBridle,
            Recipe::Spreader,
            Recipe::SpreaderOverTwo,
            Recipe::LiftingBeam,
            Recipe::TwoBaskets,
        ],
        0 => vec![Recipe::TwoBaskets],
        _ => vec![Recipe::FourLegBridle, Recipe::Spreader, Recipe::LiftingBeam],
    }
}

/// Build `recipe` on `load` as one plan.
pub fn apply_recipe(
    rig: &Rig,
    load: BodyId,
    recipe: Recipe,
    opts: GatherOptions,
) -> Result<GatherPlan, String> {
    if !recipes_for(rig, load).contains(&recipe) {
        return Err(format!("'{}' doesn't fit this load", recipe.label()));
    }
    let mut plan = Plan::new(rig);
    let lugs = open_lugs(rig, load);
    let step =
        |plan: &mut Plan, ends: &[NodeId], kind: GatherKind| -> Result<Vec<NodeId>, String> {
            let g = gather(&plan.rig, ends, kind, opts)?;
            plan.rig = g.rig;
            plan.edits.extend(g.edits);
            Ok(g.new_points)
        };
    match recipe {
        Recipe::TwoLegBridle | Recipe::FourLegBridle if lugs.len() == 2 => {
            step(&mut plan, &lugs, GatherKind::Hook)?;
        }
        Recipe::TwoLegBridle | Recipe::FourLegBridle => {
            let link = step(&mut plan, &lugs, GatherKind::MasterLink)?;
            step(&mut plan, &link, GatherKind::Hook)?;
        }
        Recipe::Spreader | Recipe::LiftingBeam => {
            let kind = if recipe == Recipe::Spreader {
                GatherKind::SpreaderBar
            } else {
                GatherKind::LiftingBeam
            };
            let tops = step(&mut plan, &lugs, kind)?;
            step(&mut plan, &tops, GatherKind::Hook)?;
        }
        Recipe::SpreaderOverTwo => {
            // Pair the corners across the short side: one low bar at each end.
            let at: Vec<[f64; 3]> = {
                let mut d = Design::new(&plan.rig);
                lugs.iter().map(|n| d.point(*n)).collect()
            };
            let (a, b) = split_along_axis(&at);
            let mut tops = Vec::new();
            for side in [a, b] {
                let ends: Vec<NodeId> = side.iter().map(|i| lugs[*i]).collect();
                tops.extend(step(&mut plan, &ends, GatherKind::SpreaderBar)?);
            }
            let top = step(&mut plan, &tops, GatherKind::SpreaderBar)?;
            step(&mut plan, &top, GatherKind::Hook)?;
        }
        Recipe::TwoBaskets => {
            let groups = add_basket_edges(&mut plan, load)?;
            let mut links = Vec::new();
            for ends in groups {
                links.extend(step(&mut plan, &ends, GatherKind::Basket)?);
            }
            step(&mut plan, &links, GatherKind::Hook)?;
        }
    }
    Ok(plan.finish(Vec::new(), format!("{} built", recipe.label())))
}

fn open_lugs(rig: &Rig, load: BodyId) -> Vec<NodeId> {
    points(rig)
        .into_iter()
        .filter(|p| p.owner == TreeVertex::Body(load) && !p.bearing && !p.spare)
        .map(|p| p.node)
        .collect()
}

/// Four edges per basket — over the top long edge, under both bottom edges,
/// back over the other top edge — at x = ±L/4. Returns each basket's edges
/// in path order.
fn add_basket_edges(plan: &mut Plan, load: BodyId) -> Result<Vec<Vec<NodeId>>, String> {
    let (length, width, height) = match plan.rig.bodies.get(&load).map(|b| &b.kind) {
        Some(BodyKind::Load {
            length,
            width,
            height,
        }) => (length.clone(), width.clone(), height.clone()),
        _ => return Err("baskets go under a load".into()),
    };
    let mut groups = Vec::new();
    for (tag, sign) in [("A", -1.0), ("B", 1.0)] {
        let x = length.clone() * (0.25 * sign);
        let mut ids = Vec::new();
        for (side, y_sign, top) in [
            ("top S", -1.0, true),
            ("bottom S", -1.0, false),
            ("bottom N", 1.0, false),
            ("top N", 1.0, true),
        ] {
            let z = if top { height.clone() } else { Expr::c(0.0) };
            let y = width.clone() * (0.5 * y_sign);
            let node = Node::new(
                format!("Basket {tag} {side}"),
                NodeKind::Edge {
                    radius: Expr::c(1.0 / 12.0),
                    mu: Expr::c(0.0),
                    softener: true,
                },
            )
            .on_body(load, Coord3::new(x.clone(), y, z));
            ids.push(node.id);
            plan.push(TopoEdit::AddNode { node })?;
        }
        groups.push(ids);
    }
    Ok(groups)
}

// ---------------------------------------------------------------- builders

fn new_master_link(plan: &mut Plan) -> Result<NodeId, String> {
    let edit = preset::master_link(&plan.rig);
    let TopoEdit::AddNode { node } = &edit else {
        unreachable!("master_link adds a node")
    };
    let id = node.id;
    plan.push(edit)?;
    Ok(id)
}

/// Legs from `apex` (placed over `apex_xy`) to each end. Returns the count.
fn bridle(
    plan: &mut Plan,
    apex: NodeId,
    apex_xy: [f64; 2],
    ends: &[NodeId],
    at: &[[f64; 3]],
    weight: f64,
    opts: GatherOptions,
) -> Result<usize, String> {
    let h: Vec<f64> = at
        .iter()
        .map(|p| plan_dist(apex_xy, [p[0], p[1]]))
        .collect();
    let apex_z = at
        .iter()
        .zip(&h)
        .map(|(p, h)| p[2] + rise(*h, opts))
        .fold(f64::NEG_INFINITY, f64::max);
    let carrying = ends.len().min(2) as f64;
    for ((end, p), h) in ends.iter().zip(at).zip(&h) {
        let dz = apex_z - p[2];
        let length = round_up_inch((h * h + dz * dz).sqrt());
        let tension = weight / carrying * length / dz.max(1e-9);
        let edit = leg(
            &plan.rig,
            vec![apex, *end],
            &[length],
            tension,
            Hitch::Vertical,
            opts,
        );
        plan.push(edit)?;
    }
    Ok(ends.len())
}

/// Hang a single point straight from `upper`: a shackle for a master link or
/// a top lug, a drop leg for anything else.
fn link_to(
    plan: &mut Plan,
    upper: NodeId,
    end: NodeId,
    weight: f64,
    opts: GatherOptions,
) -> Result<(), String> {
    let edit = leg(
        &plan.rig,
        vec![upper, end],
        &[0.0],
        weight,
        Hitch::Vertical,
        opts,
    );
    plan.push(edit)
}

fn bar(
    plan: &mut Plan,
    kind: GatherKind,
    ends: &[NodeId],
    at: &[[f64; 3]],
    holds: &Mass,
    opts: GatherOptions,
) -> Result<(Vec<NodeId>, String), String> {
    let (a, b) = split_along_axis(at);
    let ca = centroid(at, &a);
    let cb = centroid(at, &b);
    let span = round_to_inch(plan_dist(ca, cb));

    let preset_kind = if kind == GatherKind::SpreaderBar {
        BodyPreset::SpreaderBar
    } else {
        BodyPreset::LiftingBeam
    };
    let mut add = preset::add_body(&plan.rig, preset_kind);
    let TopoEdit::AddBody {
        body,
        nodes,
        params,
    } = &mut add
    else {
        unreachable!("add_body adds a body")
    };
    for p in params.iter_mut() {
        if p.name.ends_with("_span") {
            p.nominal = span;
            p.source = ParamSource::Assumed;
            p.note = "Sized by Gather to sit over the points it holds".into();
        }
    }
    let label = body.label.clone();
    let find = |name: &str| nodes.iter().find(|n| n.label == name).map(|n| n.id);
    let (bottom_a, bottom_b) = (
        find("Bottom A").ok_or("bar preset has no Bottom A")?,
        find("Bottom B").ok_or("bar preset has no Bottom B")?,
    );
    let tops: Vec<NodeId> = nodes
        .iter()
        .filter(|n| n.label.starts_with("Top"))
        .map(|n| n.id)
        .collect();
    plan.push(add)?;

    // Bar level: every leg reaches the same height.
    let port_xy = |i: usize| if a.contains(&i) { ca } else { cb };
    let rises: Vec<(f64, f64)> = (0..at.len())
        .map(|i| {
            let h = plan_dist(port_xy(i), [at[i][0], at[i][1]]);
            let alone = if a.contains(&i) { a.len() } else { b.len() } == 1;
            let rise = if alone && h < PLAN_EPS_FT {
                opts.drop()
            } else {
                rise(h, opts)
            };
            (h, rise)
        })
        .collect();
    let bar_z = (0..at.len())
        .map(|i| at[i][2] + rises[i].1)
        .fold(f64::NEG_INFINITY, f64::max);

    // Share of the weight at each bar end, by where the CG sits between them.
    let cg = holds
        .cg_xy
        .unwrap_or([(ca[0] + cb[0]) / 2.0, (ca[1] + cb[1]) / 2.0]);
    let t = if span > 0.0 {
        let u = [(cb[0] - ca[0]) / span, (cb[1] - ca[1]) / span];
        (((cg[0] - ca[0]) * u[0] + (cg[1] - ca[1]) * u[1]) / span).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let end_weight = [holds.weight * (1.0 - t), holds.weight * t];

    for (side, (cluster, port)) in [(&a, bottom_a), (&b, bottom_b)].into_iter().enumerate() {
        let carrying = cluster.len().min(2) as f64;
        for &i in cluster.iter() {
            let h = rises[i].0;
            let dz = bar_z - at[i][2];
            let length = round_up_inch((h * h + dz * dz).sqrt());
            let tension = end_weight[side] / carrying * length / dz.max(1e-9);
            let edit = leg(
                &plan.rig,
                vec![port, ends[i]],
                &[length],
                tension,
                Hitch::Vertical,
                opts,
            );
            plan.push(edit)?;
        }
    }
    let summary = format!(
        "{label}, span {}, {} legs",
        crate::format::format_ft_in(span, 16),
        ends.len()
    );
    Ok((tops, summary))
}

/// One sling from `apex`, through the bearings in path order, back to `apex`.
fn basket(
    plan: &mut Plan,
    apex: NodeId,
    apex_xy: [f64; 2],
    ends: &[NodeId],
    at: &[[f64; 3]],
    weight: f64,
    opts: GatherOptions,
) -> Result<(), String> {
    let order = basket_order(at);
    let first = order[0];
    let last = *order.last().unwrap_or(&first);
    let apex_z = [first, last]
        .iter()
        .map(|&i| at[i][2] + rise(plan_dist(apex_xy, [at[i][0], at[i][1]]), opts))
        .fold(f64::NEG_INFINITY, f64::max);
    let apex_at = [apex_xy[0], apex_xy[1], apex_z];
    let mut path = vec![apex];
    let mut lengths = Vec::new();
    let mut prev = apex_at;
    for &i in &order {
        path.push(ends[i]);
        lengths.push(round_up_inch(dist3(prev, at[i])));
        prev = at[i];
    }
    path.push(apex);
    lengths.push(round_up_inch(dist3(prev, apex_at)));
    let side = dist3(apex_at, at[first]);
    let dz = (apex_z - at[first][2]).max(1e-9);
    let tension = weight / 2.0 * side / dz;
    let edit = leg(&plan.rig, path, &lengths, tension, Hitch::Basket, opts);
    plan.push(edit)
}

/// A member along `path`: shackle, a roundsling per span, shackle. A span of
/// zero length is a hardware-only link (shackle into the hook).
fn leg(
    rig: &Rig,
    path: Vec<NodeId>,
    lengths: &[f64],
    tension: f64,
    hitch: Hitch,
    opts: GatherOptions,
) -> TopoEdit {
    let (sling, shackle) = if opts.auto_size {
        (sling_for(tension), shackle_for(tension))
    } else {
        (DEFAULT_SLING_SIZE, DEFAULT_SHACKLE.to_owned())
    };
    let mut edit = preset::connect(rig, path);
    if let TopoEdit::Connect { member } = &mut edit {
        let spans = lengths.len();
        member.segments = lengths
            .iter()
            .enumerate()
            .map(|(i, len)| {
                let mut c = Vec::new();
                if i == 0 {
                    c.push(Component::shackle(shackle.clone()));
                }
                if *len > 0.0 {
                    c.push(Component::roundsling(sling, hitch, *len));
                } else {
                    // Hardware-only link, as `template` writes it.
                    c.push(Component::master_link(1e-12, 0.0));
                }
                if i + 1 == spans && (spans > 1 || *len > 0.0) {
                    c.push(Component::shackle(shackle.clone()));
                }
                Segment::new(c)
            })
            .collect();
    }
    edit
}

/// Smallest roundsling whose vertical WLL covers `lbs` (the largest if none does).
pub fn sling_for(lbs: f64) -> u8 {
    POLYESTER_ROUNDSLINGS
        .iter()
        .filter(|r| r.hitch_wll_lbs(Hitch::Vertical) as f64 >= lbs)
        .min_by_key(|r| r.vertical_lbs)
        .or_else(|| POLYESTER_ROUNDSLINGS.iter().max_by_key(|r| r.vertical_lbs))
        .map_or(DEFAULT_SLING_SIZE, |r| r.size)
}

/// Smallest shackle whose WLL covers `lbs` (the largest if none does).
pub fn shackle_for(lbs: f64) -> String {
    SHACKLES
        .iter()
        .filter(|s| s.wll_lbs as f64 >= lbs)
        .min_by_key(|s| s.wll_lbs)
        .or_else(|| SHACKLES.iter().max_by_key(|s| s.wll_lbs))
        .map_or_else(|| DEFAULT_SHACKLE.to_owned(), |s| s.size_in.to_owned())
}

/// Height a leg climbs over plan offset `h`: plumb legs take the drop length.
fn rise(h: f64, opts: GatherOptions) -> f64 {
    if h < PLAN_EPS_FT {
        opts.drop()
    } else {
        h * opts.tan()
    }
}

fn kind_noun(kind: GatherKind) -> &'static str {
    match kind {
        GatherKind::SpreaderBar => "spreader bar",
        GatherKind::LiftingBeam => "lifting beam",
        _ => "bar",
    }
}

// ---------------------------------------------------------------- graph helpers

pub fn vertex_of(rig: &Rig, node: NodeId) -> Option<TreeVertex> {
    let n = rig.nodes.get(&node)?;
    Some(match n.body {
        Some(b) => TreeVertex::Body(b),
        None => TreeVertex::Knot(node),
    })
}

fn vertex_label(rig: &Rig, v: TreeVertex) -> String {
    match v {
        TreeVertex::Body(b) => rig.bodies.get(&b).map(|b| b.label.clone()),
        TreeVertex::Knot(n) => rig.nodes.get(&n).map(|n| n.label.clone()),
    }
    .unwrap_or_default()
}

fn node_label(rig: &Rig, id: NodeId) -> String {
    rig.nodes
        .get(&id)
        .map(|n| n.label.clone())
        .unwrap_or_default()
}

/// Vertices joined by each member (in path order, deduplicated).
fn member_vertices(rig: &Rig) -> Vec<Vec<TreeVertex>> {
    rig.members
        .values()
        .map(|m| {
            let mut vs: Vec<TreeVertex> = Vec::new();
            for n in &m.path {
                if let Some(v) = vertex_of(rig, *n)
                    && !vs.contains(&v)
                {
                    vs.push(v);
                }
            }
            vs
        })
        .collect()
}

/// Vertices that hang from the hook.
fn reached(rig: &Rig) -> HashSet<TreeVertex> {
    let mut seen = HashSet::new();
    let Some(root) = vertex_of(rig, rig.root) else {
        return seen;
    };
    let links = member_vertices(rig);
    seen.insert(root);
    let mut queue = VecDeque::from([root]);
    while let Some(v) = queue.pop_front() {
        for vs in links.iter().filter(|vs| vs.contains(&v)) {
            for w in vs {
                if seen.insert(*w) {
                    queue.push_back(*w);
                }
            }
        }
    }
    seen
}

/// Hops up from the nearest load (loads are 0). Vertices with no path to a
/// load are missing (treated as top).
fn ranks(rig: &Rig) -> HashMap<TreeVertex, usize> {
    let links = member_vertices(rig);
    let mut rank = HashMap::new();
    let mut queue = VecDeque::new();
    for b in rig.bodies.values().filter(|b| b.kind.is_load()) {
        let v = TreeVertex::Body(b.id);
        rank.insert(v, 0);
        queue.push_back(v);
    }
    while let Some(v) = queue.pop_front() {
        let r = rank[&v] + 1;
        for vs in links.iter().filter(|vs| vs.contains(&v)) {
            for w in vs {
                if !rank.contains_key(w) {
                    rank.insert(*w, r);
                    queue.push_back(*w);
                }
            }
        }
    }
    rank
}

fn rank_of(rank: &HashMap<TreeVertex, usize>, v: TreeVertex) -> usize {
    rank.get(&v).copied().unwrap_or(usize::MAX)
}

/// A master link is held when some member joins it to something higher.
fn held_from_above(rig: &Rig, rank: &HashMap<TreeVertex, usize>, knot: NodeId) -> bool {
    let me = TreeVertex::Knot(knot);
    let mine = rank_of(rank, me);
    let root = vertex_of(rig, rig.root);
    member_vertices(rig).iter().any(|vs| {
        vs.contains(&me)
            && vs
                .iter()
                .any(|w| *w != me && (Some(*w) == root || rank_of(rank, *w) > mine))
    })
}

// ---------------------------------------------------------------- design pose

#[derive(Debug, Clone, Copy)]
struct Pose {
    c: [f64; 3],
    yaw: f64,
}

impl Pose {
    const ORIGIN: Pose = Pose {
        c: [0.0; 3],
        yaw: 0.0,
    };

    fn apply(&self, l: [f64; 3]) -> [f64; 3] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.c[0] + c * l[0] - s * l[1],
            self.c[1] + s * l[0] + c * l[1],
            self.c[2] + l[2],
        ]
    }
}

/// Weight held and where its CG sits in plan.
#[derive(Debug, Clone, Copy, Default)]
struct Mass {
    weight: f64,
    cg_xy: Option<[f64; 2]>,
}

/// One segment from a node on a vertex to a node on a lower vertex.
struct Hang {
    upper: NodeId,
    lower: NodeId,
    length: f64,
}

/// Quick design pose: loads level at the origin, each gathered item over
/// what it holds at the height its legs reach.
struct Design<'a> {
    rig: &'a Rig,
    rank: HashMap<TreeVertex, usize>,
    poses: HashMap<TreeVertex, Pose>,
}

impl<'a> Design<'a> {
    fn new(rig: &'a Rig) -> Self {
        Self {
            rig,
            rank: ranks(rig),
            poses: HashMap::new(),
        }
    }

    fn local(&self, node: NodeId) -> [f64; 3] {
        self.rig
            .nodes
            .get(&node)
            .and_then(|n| n.local.eval(&self.rig.params).ok())
            .unwrap_or([0.0; 3])
    }

    fn point(&mut self, node: NodeId) -> [f64; 3] {
        let Some(v) = vertex_of(self.rig, node) else {
            return [0.0; 3];
        };
        let local = match v {
            TreeVertex::Knot(_) => [0.0; 3],
            TreeVertex::Body(_) => self.local(node),
        };
        self.pose(v).apply(local)
    }

    /// Segments hanging from `v` to strictly lower vertices.
    fn hangs(&self, v: TreeVertex) -> Vec<Hang> {
        let mine = rank_of(&self.rank, v);
        let mut out = Vec::new();
        for m in self.rig.members.values() {
            for (i, w) in m.path.windows(2).enumerate() {
                let length = m.segments[i].eval_length(&self.rig.params).unwrap_or(0.0);
                for (upper, lower) in [(w[0], w[1]), (w[1], w[0])] {
                    let (Some(vu), Some(vl)) =
                        (vertex_of(self.rig, upper), vertex_of(self.rig, lower))
                    else {
                        continue;
                    };
                    if vu == v && vl != v && rank_of(&self.rank, vl) < mine {
                        out.push(Hang {
                            upper,
                            lower,
                            length,
                        });
                    }
                }
            }
        }
        out
    }

    fn pose(&mut self, v: TreeVertex) -> Pose {
        if let Some(p) = self.poses.get(&v) {
            return *p;
        }
        // Guard against cycles while this vertex is being placed.
        self.poses.insert(v, Pose::ORIGIN);
        let hangs = self.hangs(v);
        let pose = if hangs.is_empty() {
            Pose::ORIGIN
        } else {
            self.place(v, &hangs)
        };
        self.poses.insert(v, pose);
        pose
    }

    fn place(&mut self, v: TreeVertex, hangs: &[Hang]) -> Pose {
        let lower: Vec<[f64; 3]> = hangs.iter().map(|h| self.point(h.lower)).collect();
        let local: Vec<[f64; 3]> = hangs
            .iter()
            .map(|h| match v {
                TreeVertex::Knot(_) => [0.0; 3],
                TreeVertex::Body(_) => self.local(h.upper),
            })
            .collect();
        let mut pose = Pose::ORIGIN;
        match v {
            TreeVertex::Knot(_) => {
                let all: Vec<usize> = (0..lower.len()).collect();
                let below = self.below_vertices(v);
                let xy = self
                    .mass_of(&below)
                    .cg_xy
                    .unwrap_or_else(|| centroid(&lower, &all));
                pose.c = [xy[0], xy[1], 0.0];
            }
            TreeVertex::Body(_) => {
                // Ports at the two ends of the body (by local x).
                let lo = (0..hangs.len()).min_by(|&i, &j| local[i][0].total_cmp(&local[j][0]));
                let hi = (0..hangs.len()).max_by(|&i, &j| local[i][0].total_cmp(&local[j][0]));
                let (lo, hi) = (lo.unwrap_or(0), hi.unwrap_or(0));
                let group = |port: NodeId| -> Vec<usize> {
                    (0..hangs.len())
                        .filter(|&i| hangs[i].upper == port)
                        .collect()
                };
                if (local[hi][0] - local[lo][0]).abs() > 1e-9 {
                    let ca = centroid(&lower, &group(hangs[lo].upper));
                    let cb = centroid(&lower, &group(hangs[hi].upper));
                    pose.yaw = (cb[1] - ca[1]).atan2(cb[0] - ca[0]);
                    let mid_local = [
                        (local[lo][0] + local[hi][0]) / 2.0,
                        (local[lo][1] + local[hi][1]) / 2.0,
                        0.0,
                    ];
                    let off = Pose {
                        c: [0.0; 3],
                        yaw: pose.yaw,
                    }
                    .apply(mid_local);
                    pose.c = [
                        (ca[0] + cb[0]) / 2.0 - off[0],
                        (ca[1] + cb[1]) / 2.0 - off[1],
                        0.0,
                    ];
                } else {
                    let c = centroid(&lower, &group(hangs[lo].upper));
                    pose.c = [c[0] - local[lo][0], c[1] - local[lo][1], 0.0];
                }
            }
        }
        // Height: the highest point any leg reaches, less its port's local z.
        let mut z = f64::NEG_INFINITY;
        for ((h, low), loc) in hangs.iter().zip(&lower).zip(&local) {
            let at = pose.apply([loc[0], loc[1], 0.0]);
            let off = plan_dist([at[0], at[1]], [low[0], low[1]]);
            let rise = (h.length * h.length - off * off).max(0.0).sqrt();
            z = z.max(low[2] + rise - loc[2]);
        }
        pose.c[2] = if z.is_finite() { z } else { 0.0 };
        pose
    }

    /// `v` and every vertex below it.
    fn below_vertices(&self, v: TreeVertex) -> Vec<TreeVertex> {
        let mut out = vec![v];
        let mut i = 0;
        while i < out.len() {
            let cur = out[i];
            for h in self.hangs(cur) {
                if let Some(w) = vertex_of(self.rig, h.lower)
                    && !out.contains(&w)
                {
                    out.push(w);
                }
            }
            i += 1;
        }
        out
    }

    /// Body weight held by `tops` and everything below them.
    fn mass_below(&mut self, tops: &[TreeVertex]) -> Mass {
        let mut all: Vec<TreeVertex> = Vec::new();
        for t in tops {
            for v in self.below_vertices(*t) {
                if !all.contains(&v) {
                    all.push(v);
                }
            }
        }
        self.mass_of(&all)
    }

    fn mass_of(&mut self, vertices: &[TreeVertex]) -> Mass {
        let mut w = 0.0;
        let mut m = [0.0; 2];
        for v in vertices {
            let TreeVertex::Body(id) = v else { continue };
            let Some(body) = self.rig.bodies.get(id) else {
                continue;
            };
            if matches!(body.kind, BodyKind::Hook) {
                continue;
            }
            let weight = body.eval_weight(&self.rig.params).unwrap_or(0.0);
            let cg = body.eval_cg(&self.rig.params).unwrap_or([0.0; 3]);
            let at = self.pose(*v).apply(cg);
            w += weight;
            m[0] += weight * at[0];
            m[1] += weight * at[1];
        }
        Mass {
            weight: w,
            cg_xy: (w > 0.0).then(|| [m[0] / w, m[1] / w]),
        }
    }
}

// ---------------------------------------------------------------- geometry

fn plan_dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn centroid(at: &[[f64; 3]], idx: &[usize]) -> [f64; 2] {
    if idx.is_empty() {
        return [0.0; 2];
    }
    let n = idx.len() as f64;
    [
        idx.iter().map(|&i| at[i][0]).sum::<f64>() / n,
        idx.iter().map(|&i| at[i][1]).sum::<f64>() / n,
    ]
}

/// The plan direction along which the points spread most (principal axis of
/// their plan positions), pointing toward +x (then +y). +x when the spread is
/// the same every way, so a square load's bars run along x.
fn principal_axis(at: &[[f64; 3]]) -> [f64; 2] {
    let all: Vec<usize> = (0..at.len()).collect();
    let c = centroid(at, &all);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in at {
        let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let scale = (sxx + syy).max(1e-12);
    if ((sxx - syy).abs() + sxy.abs()) / scale < 1e-9 {
        return [1.0, 0.0];
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let mut u = [theta.cos(), theta.sin()];
    if u[0] < -1e-9 || (u[0].abs() <= 1e-9 && u[1] < 0.0) {
        u = [-u[0], -u[1]];
    }
    u
}

/// Split points into the two ends of the principal axis.
fn split_along_axis(at: &[[f64; 3]]) -> (Vec<usize>, Vec<usize>) {
    let u = principal_axis(at);
    let all: Vec<usize> = (0..at.len()).collect();
    let c = centroid(at, &all);
    let s = |i: usize| (at[i][0] - c[0]) * u[0] + (at[i][1] - c[1]) * u[1];
    let (a, b): (Vec<usize>, Vec<usize>) = all.iter().partition(|&&i| s(i) < 0.0);
    if a.is_empty() || b.is_empty() {
        // Degenerate (all on the centre line): split in half by order.
        let mut sorted = all.clone();
        sorted.sort_by(|&i, &j| s(i).total_cmp(&s(j)));
        let mid = sorted.len() / 2;
        return (sorted[..mid].to_vec(), sorted[mid..].to_vec());
    }
    (a, b)
}

/// Path order for a basket: up the low side (top before bottom), across the
/// bottom, up the high side (bottom before top).
fn basket_order(at: &[[f64; 3]]) -> Vec<usize> {
    let u = principal_axis(at);
    let all: Vec<usize> = (0..at.len()).collect();
    let c = centroid(at, &all);
    let s = |i: usize| (at[i][0] - c[0]) * u[0] + (at[i][1] - c[1]) * u[1];
    let mut idx = all;
    idx.sort_by(|&i, &j| {
        let (si, sj) = (s(i), s(j));
        if (si - sj).abs() > 1e-9 {
            return si.total_cmp(&sj);
        }
        if si < 0.0 {
            at[j][2].total_cmp(&at[i][2])
        } else {
            at[i][2].total_cmp(&at[j][2])
        }
    });
    idx
}

fn round_up_inch(ft: f64) -> f64 {
    ((ft * 12.0) - 1e-9).ceil().max(0.0) / 12.0
}

fn round_to_inch(ft: f64) -> f64 {
    (ft * 12.0).round() / 12.0
}

#[cfg(test)]
mod tests;
