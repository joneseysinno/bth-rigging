//! Schematic (canvas) layout of the rig graph: one block per body or master
//! link, ports along block edges, tiers from the hook down.
//!
//! Step: G (graph builder)
//! Theory: docs/step-g-graph-builder.md "Canvas layout".
//! Inputs: an authored rig and any positions the user dragged.
//! Outputs: block centres, sizes and port positions in canvas units (px at zoom 1).
//! Must not depend on: UI, dioxus, store.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Rig;
use super::body::BodyKind;
use super::id::{BodyId, NodeId};
use super::tree::{RigTree, TreeVertex};

pub const TIER_GAP: f64 = 200.0;
pub const BLOCK_GAP: f64 = 70.0;
pub const BLOCK_H: f64 = 46.0;
pub const PORT_PITCH: f64 = 56.0;
pub const MIN_BLOCK_W: f64 = 150.0;
pub const KNOT_R: f64 = 14.0;

/// Positions the user dragged, keyed by body or master-link (free node) id.
/// Anything missing falls back to the automatic layout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CanvasLayout {
    pub positions: Vec<(Uuid, f64, f64)>,
}

impl CanvasLayout {
    pub fn get(&self, id: Uuid) -> Option<[f64; 2]> {
        self.positions
            .iter()
            .find(|(k, ..)| *k == id)
            .map(|(_, x, y)| [*x, *y])
    }

    pub fn set(&mut self, id: Uuid, at: [f64; 2]) {
        match self.positions.iter_mut().find(|(k, ..)| *k == id) {
            Some(slot) => {
                slot.1 = at[0];
                slot.2 = at[1];
            }
            None => self.positions.push((id, at[0], at[1])),
        }
    }

    /// Drop positions for blocks that no longer exist.
    pub fn retain_in(&mut self, rig: &Rig) {
        self.positions.retain(|(id, ..)| {
            rig.bodies.contains_key(&BodyId(*id))
                || rig
                    .nodes
                    .get(&NodeId(*id))
                    .is_some_and(|n| n.kind.is_free())
        });
    }
}

/// A block on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Block {
    Body(BodyId),
    Knot(NodeId),
}

impl Block {
    pub fn key(self) -> Uuid {
        match self {
            Self::Body(id) => id.0,
            Self::Knot(id) => id.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    /// Master links: members meet at the centre.
    Centre,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPort {
    pub node: NodeId,
    pub edge: Edge,
    pub at: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBlock {
    pub block: Block,
    pub center: [f64; 2],
    pub w: f64,
    pub h: f64,
    pub tier: usize,
    /// Not reachable from the hook.
    pub detached: bool,
    pub ports: Vec<PlacedPort>,
}

impl PlacedBlock {
    pub fn contains(&self, p: [f64; 2]) -> bool {
        (p[0] - self.center[0]).abs() <= self.w / 2.0
            && (p[1] - self.center[1]).abs() <= self.h / 2.0
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Schematic {
    pub blocks: Vec<PlacedBlock>,
}

impl Schematic {
    pub fn build(rig: &Rig, layout: &CanvasLayout) -> Self {
        let tree = RigTree::build(rig);
        let blocks = blocks_of(rig);
        let edges = port_edges(rig, &tree);
        let (tier, detached) = tiers(rig, &blocks);
        let sizes: HashMap<Block, (f64, f64)> = blocks
            .iter()
            .map(|b| (*b, block_size(rig, *b, &edges)))
            .collect();
        let auto = auto_centres(rig, &blocks, &tier, &sizes);
        let placed = blocks
            .iter()
            .map(|b| {
                let center = layout.get(b.key()).unwrap_or(auto[b]);
                let (w, h) = sizes[b];
                PlacedBlock {
                    block: *b,
                    center,
                    w,
                    h,
                    tier: tier[b],
                    detached: detached.contains(b),
                    ports: place_ports(rig, *b, center, w, h, &edges),
                }
            })
            .collect();
        Self { blocks: placed }
    }

    pub fn port(&self, node: NodeId) -> Option<&PlacedPort> {
        self.blocks
            .iter()
            .flat_map(|b| b.ports.iter())
            .find(|p| p.node == node)
    }

    pub fn block(&self, block: Block) -> Option<&PlacedBlock> {
        self.blocks.iter().find(|b| b.block == block)
    }

    /// Bounding box `[min_x, min_y, max_x, max_y]` of every block.
    pub fn bounds(&self) -> [f64; 4] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for block in &self.blocks {
            b[0] = b[0].min(block.center[0] - block.w / 2.0);
            b[1] = b[1].min(block.center[1] - block.h / 2.0);
            b[2] = b[2].max(block.center[0] + block.w / 2.0);
            b[3] = b[3].max(block.center[1] + block.h / 2.0);
        }
        if b[0].is_finite() {
            b
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
}

fn blocks_of(rig: &Rig) -> Vec<Block> {
    let mut out: Vec<Block> = rig.bodies.keys().map(|id| Block::Body(*id)).collect();
    out.extend(
        rig.nodes
            .values()
            .filter(|n| n.kind.is_free())
            .map(|n| Block::Knot(n.id)),
    );
    out
}

fn block_of(rig: &Rig, node: NodeId) -> Option<Block> {
    let n = rig.nodes.get(&node)?;
    Some(match n.body {
        Some(b) => Block::Body(b),
        None => Block::Knot(node),
    })
}

/// Which edge each body port sits on. A port is on the bottom when a member
/// hangs from its body through it; unused ports go by name and body kind.
fn port_edges(rig: &Rig, tree: &RigTree) -> HashMap<NodeId, Edge> {
    let mut out = HashMap::new();
    for node in rig.nodes.values() {
        let Some(body_id) = node.body else {
            out.insert(node.id, Edge::Centre);
            continue;
        };
        let Some(body) = rig.bodies.get(&body_id) else {
            out.insert(node.id, Edge::Top);
            continue;
        };
        if node.kind.is_root() {
            out.insert(node.id, Edge::Bottom);
            continue;
        }
        let mut hangs_from = false;
        let mut hung_by = false;
        for member in rig.members.values().filter(|m| m.path.contains(&node.id)) {
            match tree.get(member.id).and_then(|t| t.parent) {
                Some(TreeVertex::Body(b)) if b == body_id => hangs_from = true,
                _ => hung_by = true,
            }
        }
        let edge = if hangs_from && !hung_by {
            Edge::Bottom
        } else if hung_by
            || body.kind.is_load()
            || node.label.to_lowercase().starts_with("top")
            || matches!(body.kind, BodyKind::Frame)
        {
            Edge::Top
        } else {
            Edge::Bottom
        };
        out.insert(node.id, edge);
    }
    out
}

fn block_size(rig: &Rig, block: Block, edges: &HashMap<NodeId, Edge>) -> (f64, f64) {
    match block {
        Block::Knot(_) => (KNOT_R * 2.0, KNOT_R * 2.0),
        Block::Body(id) => {
            let count = |edge: Edge| {
                rig.nodes
                    .values()
                    .filter(|n| n.body == Some(id) && edges.get(&n.id) == Some(&edge))
                    .count()
            };
            let ports = count(Edge::Top).max(count(Edge::Bottom)) as f64;
            let label = rig.bodies[&id].label.chars().count() as f64 * 7.5 + 40.0;
            (
                (ports * PORT_PITCH + 20.0).max(MIN_BLOCK_W).max(label),
                BLOCK_H,
            )
        }
    }
}

fn place_ports(
    rig: &Rig,
    block: Block,
    center: [f64; 2],
    w: f64,
    h: f64,
    edges: &HashMap<NodeId, Edge>,
) -> Vec<PlacedPort> {
    let Block::Body(id) = block else {
        let Block::Knot(node) = block else {
            unreachable!()
        };
        return vec![PlacedPort {
            node,
            edge: Edge::Centre,
            at: center,
        }];
    };
    let mut out = Vec::new();
    for edge in [Edge::Top, Edge::Bottom] {
        let nodes: Vec<NodeId> = rig
            .nodes
            .values()
            .filter(|n| n.body == Some(id) && edges.get(&n.id) == Some(&edge))
            .map(|n| n.id)
            .collect();
        let n = nodes.len() as f64;
        let y = match edge {
            Edge::Top => center[1] - h / 2.0,
            _ => center[1] + h / 2.0,
        };
        let pitch = (w / n.max(1.0)).min(PORT_PITCH * 1.6);
        for (i, node) in nodes.into_iter().enumerate() {
            let x = center[0] + (i as f64 - (n - 1.0) / 2.0) * pitch;
            out.push(PlacedPort {
                node,
                edge,
                at: [x, y],
            });
        }
    }
    out
}

/// Breadth-first tiers from the hook; blocks not reached go below, loads last.
fn tiers(rig: &Rig, blocks: &[Block]) -> (HashMap<Block, usize>, HashSet<Block>) {
    let mut tier: HashMap<Block, usize> = HashMap::new();
    let mut queue = VecDeque::new();
    if let Some(root) = block_of(rig, rig.root) {
        tier.insert(root, 0);
        queue.push_back(root);
    }
    while let Some(block) = queue.pop_front() {
        let t = tier[&block];
        for member in rig.members.values() {
            if !member.path.iter().any(|n| block_of(rig, *n) == Some(block)) {
                continue;
            }
            for n in &member.path {
                if let Some(next) = block_of(rig, *n)
                    && !tier.contains_key(&next)
                {
                    tier.insert(next, t + 1);
                    queue.push_back(next);
                }
            }
        }
    }
    let reached_max = tier.values().copied().max().unwrap_or(0);
    let mut detached = HashSet::new();
    for block in blocks {
        if tier.contains_key(block) {
            continue;
        }
        detached.insert(*block);
        let is_load = matches!(block, Block::Body(b) if rig.bodies[b].kind.is_load());
        let t = if is_load {
            (reached_max + 1).max(2)
        } else {
            reached_max.max(1)
        };
        tier.insert(*block, t);
    }
    // Loads always sit below everything that is not a load.
    let lowest_other = blocks
        .iter()
        .filter(|b| !matches!(b, Block::Body(id) if rig.bodies[id].kind.is_load()))
        .map(|b| tier[b])
        .max()
        .unwrap_or(0);
    for block in blocks {
        if let Block::Body(id) = block
            && rig.bodies[id].kind.is_load()
        {
            let t = tier[block].max(lowest_other + 1);
            tier.insert(*block, t);
        }
    }
    (tier, detached)
}

/// Each tier is a row; a block's x follows the mean x of the blocks it is
/// connected to above, then the row is spread so blocks don't overlap.
fn auto_centres(
    rig: &Rig,
    blocks: &[Block],
    tier: &HashMap<Block, usize>,
    sizes: &HashMap<Block, (f64, f64)>,
) -> HashMap<Block, [f64; 2]> {
    let max_tier = tier.values().copied().max().unwrap_or(0);
    let mut out: HashMap<Block, [f64; 2]> = HashMap::new();
    for t in 0..=max_tier {
        let row: Vec<Block> = blocks.iter().copied().filter(|b| tier[b] == t).collect();
        if row.is_empty() {
            continue;
        }
        let mut wanted: Vec<(Block, f64)> = row
            .iter()
            .map(|b| {
                let xs: Vec<f64> = rig
                    .members
                    .values()
                    .filter(|m| m.path.iter().any(|n| block_of(rig, *n) == Some(*b)))
                    .flat_map(|m| m.path.iter())
                    .filter_map(|n| block_of(rig, *n))
                    .filter(|o| tier.get(o).is_some_and(|ot| *ot < t))
                    .filter_map(|o| out.get(&o).map(|c| c[0]))
                    .collect();
                let x = if xs.is_empty() {
                    0.0
                } else {
                    xs.iter().sum::<f64>() / xs.len() as f64
                };
                (*b, x)
            })
            .collect();
        wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
        let total: f64 = wanted.iter().map(|(b, _)| sizes[b].0).sum::<f64>()
            + BLOCK_GAP * (wanted.len() as f64 - 1.0);
        let mean = wanted.iter().map(|(_, x)| *x).sum::<f64>() / wanted.len() as f64;
        let mut x = mean - total / 2.0;
        let y = t as f64 * TIER_GAP;
        for (b, _) in wanted {
            let w = sizes[&b].0;
            out.insert(b, [x + w / 2.0, y]);
            x += w + BLOCK_GAP;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::{duplo10, preset};

    #[test]
    fn hook_is_on_top_and_load_on_the_bottom() {
        let rig = duplo10();
        let s = Schematic::build(&rig, &CanvasLayout::default());
        let hook = rig
            .bodies
            .values()
            .find(|b| matches!(b.kind, BodyKind::Hook))
            .unwrap()
            .id;
        let load = rig.bodies.values().find(|b| b.kind.is_load()).unwrap().id;
        let y = |id| s.block(Block::Body(id)).unwrap().center[1];
        assert_eq!(y(hook), 0.0);
        for block in &s.blocks {
            assert!(block.center[1] <= y(load));
            assert!(!block.detached);
        }
        // Every node on a body or knot gets exactly one port.
        let ports: usize = s.blocks.iter().map(|b| b.ports.len()).sum();
        assert_eq!(ports, rig.nodes.len());
    }

    #[test]
    fn blocks_in_a_row_do_not_overlap() {
        let rig = duplo10();
        let s = Schematic::build(&rig, &CanvasLayout::default());
        for a in &s.blocks {
            for b in &s.blocks {
                if a.block == b.block || a.center[1] != b.center[1] {
                    continue;
                }
                assert!((a.center[0] - b.center[0]).abs() >= (a.w + b.w) / 2.0 - 1e-9);
            }
        }
    }

    #[test]
    fn dragged_positions_override_and_unwired_blocks_are_detached() {
        let rig = preset::blank_rig("blank", Uuid::nil());
        let load = rig.bodies.values().find(|b| b.kind.is_load()).unwrap().id;
        let mut layout = CanvasLayout::default();
        layout.set(load.0, [123.0, 456.0]);
        let s = Schematic::build(&rig, &layout);
        let block = s.block(Block::Body(load)).unwrap();
        assert_eq!(block.center, [123.0, 456.0]);
        assert!(block.detached);
        assert!(block.ports.iter().all(|p| p.edge == Edge::Top));
        layout.retain_in(&preset::hook_only("x", Uuid::nil()));
        assert!(layout.positions.is_empty());
    }
}
