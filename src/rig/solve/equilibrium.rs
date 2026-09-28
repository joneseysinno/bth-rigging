//! Equilibrium matrix `A t = b` at a fixed geometry.
//!
//! Step: 3
//! Theory: Readback §3.5. Rows are the equilibrium equations of every free
//! body (3 force + 3 moment about its origin) and every free knot (3 force).
//! Columns are the unknown member forces: one tension per tension member
//! (constant along the path, μ = 0 at bearings) and three components per
//! zero-length link. `b` is minus the external load (body weights at their
//! CG, member self-weight lumped at each segment's lower stop).
//! Moment rows are divided by `Model::l_char` so every row is in lb.
//! Must not depend on: UI, dioxus, store.

use super::linalg::Mat;
use super::model::{MemberKind, Model, NodeLoc, State, add, cross, scale, sub, unit};
use crate::rig::id::{BodyId, NodeId};

/// Column meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    /// Tension in member `index` (lb, ≥ 0 when admissible).
    Tension(usize),
    /// Component `axis` (0 = x, 1 = y, 2 = z) of the force link `index`
    /// exerts on its **last** path stop (the first stop gets the opposite).
    Link(usize, u8),
}

/// Row meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// Free body index, 0–2 force, 3–5 moment/l_char.
    Body(usize, u8),
    /// Free knot index, 0–2 force.
    Knot(usize, u8),
}

#[derive(Debug, Clone)]
pub struct Equilibrium {
    pub a: Mat,
    pub b: Vec<f64>,
    pub unknowns: Vec<Unknown>,
    pub rows: Vec<Row>,
}

/// Build `A` and `b` for the members flagged `active` (others are slack / removed).
pub fn assemble(model: &Model, state: &State, active: &[bool]) -> Equilibrium {
    let nb = model.bodies.len();
    let nrows = model.dof();
    let mut rows = Vec::with_capacity(nrows);
    for i in 0..nb {
        for k in 0..6 {
            rows.push(Row::Body(i, k));
        }
    }
    for i in 0..model.free_nodes.len() {
        for k in 0..3 {
            rows.push(Row::Knot(i, k));
        }
    }

    let mut unknowns = Vec::new();
    for (i, m) in model.members.iter().enumerate() {
        if !active.get(i).copied().unwrap_or(true) {
            continue;
        }
        match m.kind {
            MemberKind::Tension => unknowns.push(Unknown::Tension(i)),
            MemberKind::Link => {
                for ax in 0..3 {
                    unknowns.push(Unknown::Link(i, ax));
                }
            }
        }
    }

    let mut a = Mat::zeros(nrows, unknowns.len());
    for (col, u) in unknowns.iter().enumerate() {
        for (node, f) in unit_forces(model, state, *u) {
            add_force(model, state, &mut a.data, a.cols, col, node, f);
        }
    }

    let mut load = vec![0.0; nrows];
    for (i, body) in model.bodies.iter().enumerate() {
        let cg = model.cg(state, i);
        add_point(
            model,
            state,
            &mut load,
            Target::Body(i),
            cg,
            [0.0, 0.0, -body.weight],
        );
    }
    for (node, w) in &model.point_loads {
        if let Some(t) = target(model, *node) {
            let x = model.pos(state, *node);
            add_point(model, state, &mut load, t, x, [0.0, 0.0, -*w]);
        }
    }
    let b = load.iter().map(|v| -v).collect();
    Equilibrium {
        a,
        b,
        unknowns,
        rows,
    }
}

/// Forces a unit value of unknown `u` applies, as (node, force) pairs.
pub fn unit_forces(model: &Model, state: &State, u: Unknown) -> Vec<(NodeId, [f64; 3])> {
    match u {
        Unknown::Tension(i) => {
            let m = &model.members[i];
            let pts: Vec<[f64; 3]> = m.path.iter().map(|n| model.pos(state, *n)).collect();
            let mut out = Vec::with_capacity(m.path.len());
            for (k, node) in m.path.iter().enumerate() {
                let mut f = [0.0; 3];
                if k > 0
                    && let Some(d) = unit(sub(pts[k - 1], pts[k]))
                {
                    f = add(f, d);
                }
                if k + 1 < pts.len()
                    && let Some(d) = unit(sub(pts[k + 1], pts[k]))
                {
                    f = add(f, d);
                }
                out.push((*node, f));
            }
            out
        }
        Unknown::Link(i, ax) => {
            let m = &model.members[i];
            let mut e = [0.0; 3];
            e[ax as usize] = 1.0;
            let first = m.path[0];
            let last = *m.path.last().unwrap_or(&first);
            vec![(last, e), (first, scale(e, -1.0))]
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Target {
    Body(usize),
    Knot(usize),
}

fn target(model: &Model, node: NodeId) -> Option<Target> {
    match model.nodes.get(&node)? {
        NodeLoc::Body { index, .. } => Some(Target::Body(*index)),
        NodeLoc::Free { index } => Some(Target::Knot(*index)),
        NodeLoc::Held { .. } => None,
    }
}

fn add_force(
    model: &Model,
    state: &State,
    a: &mut [f64],
    cols: usize,
    col: usize,
    node: NodeId,
    f: [f64; 3],
) {
    let Some(t) = target(model, node) else {
        return;
    };
    let x = model.pos(state, node);
    let mut rowvals = [0.0; 6];
    let (base, n) = row_block(model, state, t, x, f, &mut rowvals);
    for k in 0..n {
        a[(base + k) * cols + col] += rowvals[k];
    }
}

fn add_point(model: &Model, state: &State, load: &mut [f64], t: Target, x: [f64; 3], f: [f64; 3]) {
    let mut rowvals = [0.0; 6];
    let (base, n) = row_block(model, state, t, x, f, &mut rowvals);
    for k in 0..n {
        load[base + k] += rowvals[k];
    }
}

fn row_block(
    model: &Model,
    state: &State,
    t: Target,
    x: [f64; 3],
    f: [f64; 3],
    out: &mut [f64; 6],
) -> (usize, usize) {
    match t {
        Target::Body(i) => {
            let o = state.poses[i].origin;
            let m = cross(sub(x, o), f);
            out[0] = f[0];
            out[1] = f[1];
            out[2] = f[2];
            out[3] = m[0] / model.l_char;
            out[4] = m[1] / model.l_char;
            out[5] = m[2] / model.l_char;
            (6 * i, 6)
        }
        Target::Knot(i) => {
            out[0] = f[0];
            out[1] = f[1];
            out[2] = f[2];
            (6 * model.bodies.len() + 3 * i, 3)
        }
    }
}

/// Force and moment (about the body's world CG / held point) that the rig
/// applies to each held body for unknown values `t`, plus lumped loads that
/// land on held nodes. The support **reaction** is the negative.
pub fn held_resultants(
    model: &Model,
    state: &State,
    eq: &Equilibrium,
    t: &[f64],
) -> Vec<(BodyId, [f64; 3], [f64; 3])> {
    let mut out: Vec<(BodyId, [f64; 3], [f64; 3])> = model
        .held
        .iter()
        .map(|(id, w, _)| (*id, [0.0, 0.0, -*w], [0.0; 3]))
        .collect();
    let mut push = |node: NodeId, f: [f64; 3]| {
        if let Some(NodeLoc::Held { body, at }) = model.nodes.get(&node)
            && let Some(slot) = out.iter_mut().find(|(b, _, _)| b == body)
        {
            let r = model
                .held
                .iter()
                .find(|(b, _, _)| b == body)
                .map(|(_, _, c)| *c)
                .unwrap_or(*at);
            slot.1 = add(slot.1, f);
            slot.2 = add(slot.2, cross(sub(*at, r), f));
        }
    };
    for (col, u) in eq.unknowns.iter().enumerate() {
        for (node, f) in unit_forces(model, state, *u) {
            push(node, scale(f, t[col]));
        }
    }
    for (node, w) in &model.point_loads {
        push(*node, [0.0, 0.0, -*w]);
    }
    out
}

/// ‖A t − b‖∞ in lb.
pub fn residual(eq: &Equilibrium, t: &[f64]) -> f64 {
    let at = eq.a.mul_vec(t);
    at.iter()
        .zip(&eq.b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}
