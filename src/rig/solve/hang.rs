//! Energy minimization; multipliers to tensions.
//!
//! Step: 3
//! Theory: roadmap Step 3 — hang solve. Let the rig find its own pose under
//! gravity: minimise the potential `Π = Σ W·z` of every body CG and lumped
//! weight subject to `ℓᵢ(q) ≤ Lᵢ` for each tension member (path length through
//! its bearings) and `x_a = x_b` for each link. The Lagrange multipliers of
//! the active constraints **are** the member forces. Each constraint is
//! regularised by its compliance (`ℓᵢ − Lᵢ = cᵢ λᵢ`), which makes the answer
//! unique for an indeterminate rig and is the elastic model itself; with the
//! rigid default stiffness the stretch is ~1e-9 of the length.
//!
//! Method: Newton on the KKT equations with an active set, symmetric
//! quasi-definite system `[H+μI, −A; −Aᵀ, −C]`, backtracking on the residual,
//! trust-region step cap. `H` is a central finite difference of the
//! Lagrangian gradient `b(q) − A(q)λ` (gravity + geometric stiffness).
//! Generalised coordinates per free body: world translation (ft) and a
//! rotation vector scaled by `l_char` (so every coordinate is in ft and every
//! generalised force in lb) — exactly the rows of `equilibrium::assemble`.
//! Inputs: evaluated graph, gravity.
//! Outputs: member tensions and the hung pose.
//! Must not depend on: UI, dioxus.

use super::elastic::column_compliance;
use super::equilibrium::{Equilibrium, Unknown, assemble};
use super::linalg::{Mat, solve_dense};
use super::model::{MemberKind, Model, State, sub};

/// Options for the hang.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HangOptions {
    pub max_iter: usize,
    /// Stationarity tolerance, relative to the total weight.
    pub force_rtol: f64,
    /// Constraint tolerance, relative to `l_char`.
    pub length_rtol: f64,
}

impl Default for HangOptions {
    fn default() -> Self {
        Self {
            max_iter: 400,
            force_rtol: 1e-10,
            length_rtol: 1e-11,
        }
    }
}

/// Hung equilibrium.
#[derive(Debug, Clone)]
pub struct Hung {
    pub state: State,
    /// Per model member: taut (constraint active) at the solution.
    pub active: Vec<bool>,
    pub eq: Equilibrium,
    /// Multipliers per `eq.unknowns` column: tension (lb) or link force component.
    pub t: Vec<f64>,
    pub iterations: usize,
    pub converged: bool,
    /// ‖b − A t‖∞ at the solution, lb.
    pub force_residual_lbs: f64,
    /// Largest constraint violation beyond the elastic stretch, ft.
    pub length_residual_ft: f64,
}

/// Hang the rig from `start`, with `allowed[i] = false` forcing member `i`
/// slack (used by the bounding cases).
pub fn hang(model: &Model, start: &State, allowed: &[bool], opts: HangOptions) -> Hung {
    let n = model.members.len();
    let lchar = model.l_char;
    let w = model.w_total;
    let mut state = start.clone();

    // Phase A: find the right basin with a soft, energy-decreasing relaxation.
    let soft_iters = soft_relax(model, &mut state, allowed);

    // Initial active set: allowed members that are taut (or short) now.
    let mut active: Vec<bool> = (0..n)
        .map(|i| {
            allowed.get(i).copied().unwrap_or(true)
                && match model.members[i].kind {
                    MemberKind::Link => true,
                    MemberKind::Tension => {
                        model.path_length(&state, i) - model.members[i].length
                            >= -1e-6 * model.members[i].length.max(1.0)
                    }
                }
        })
        .collect();

    // Initial multipliers from the elastic split at the start pose.
    let eq0 = assemble(model, &state, &active);
    let t0 = super::elastic::min_energy(model, &eq0);
    let mut lam: Vec<f64> = vec![0.0; 3 * n + n];
    store(&eq0, &t0, &mut lam, n);

    let mut total_iter = soft_iters;
    let mut converged = false;
    for _outer in 0..200 {
        let (outcome, iters) = newton(model, &mut state, &mut active, allowed, &mut lam, n, opts);
        total_iter += iters;
        if outcome == Outcome::Activated {
            continue;
        }
        let ok = outcome == Outcome::Converged;

        // Active-set update.
        let eq = assemble(model, &state, &active);
        let t = gather(&eq, &lam, n);
        let neg = eq
            .unknowns
            .iter()
            .zip(&t)
            .filter_map(|(u, v)| match u {
                Unknown::Tension(i) if *v < -1e-9 * w => Some((*i, *v)),
                _ => None,
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((i, _)) = neg {
            active[i] = false;
            lam[i] = 0.0;
            continue;
        }
        let over = (0..n)
            .filter(|&i| {
                !active[i]
                    && allowed.get(i).copied().unwrap_or(true)
                    && model.members[i].kind == MemberKind::Tension
            })
            .map(|i| (i, model.path_length(&state, i) - model.members[i].length))
            .filter(|(_, v)| *v > opts.length_rtol * lchar * 10.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((i, _)) = over {
            active[i] = true;
            lam[i] = 0.0;
            continue;
        }
        converged = ok;
        break;
    }

    let eq = assemble(model, &state, &active);
    let t = gather(&eq, &lam, n);
    let at = eq.a.mul_vec(&t);
    let force_residual_lbs = at
        .iter()
        .zip(&eq.b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    let comp = column_compliance(model, &eq);
    let cons = constraint_values(model, &state, &eq);
    let length_residual_ft = cons
        .iter()
        .zip(&comp)
        .zip(&t)
        .map(|((g, c), l)| (g - c * l).abs())
        .fold(0.0, f64::max);
    Hung {
        state,
        active,
        eq,
        t,
        iterations: total_iter,
        converged,
        force_residual_lbs,
        length_residual_ft,
    }
}

/// Soft penalty stiffness factor for phase A: `k = SOFT · W / L`.
const SOFT_FACTOR: f64 = 1e4;

fn soft_k(model: &Model, i: usize) -> f64 {
    let m = &model.members[i];
    let l = match m.kind {
        MemberKind::Tension => m.length.max(1e-3),
        MemberKind::Link => 1e-3 * model.l_char,
    };
    SOFT_FACTOR * model.w_total / l
}

/// Soft member forces at a pose (tension-only springs, links as ball springs).
fn soft_forces(model: &Model, s: &State, eq: &Equilibrium) -> Vec<f64> {
    let cons = constraint_values(model, s, eq);
    eq.unknowns
        .iter()
        .zip(&cons)
        .map(|(u, g)| match u {
            Unknown::Tension(i) => soft_k(model, *i) * g.max(0.0),
            Unknown::Link(i, _) => soft_k(model, *i) * g,
        })
        .collect()
}

/// Total potential of the soft system: gravity + spring energy.
fn soft_energy(model: &Model, s: &State, allowed: &[bool]) -> f64 {
    let mut e = 0.0;
    for (i, b) in model.bodies.iter().enumerate() {
        e += b.weight * model.cg(s, i)[2];
    }
    for (node, w) in &model.point_loads {
        if !model.is_held_node(*node) {
            e += w * model.pos(s, *node)[2];
        }
    }
    for (i, m) in model.members.iter().enumerate() {
        if !allowed.get(i).copied().unwrap_or(true) {
            continue;
        }
        let k = soft_k(model, i);
        match m.kind {
            MemberKind::Tension => {
                let g = (model.path_length(s, i) - m.length).max(0.0);
                e += 0.5 * k * g * g;
            }
            MemberKind::Link => {
                let a = model.pos(s, m.path[0]);
                let b = model.pos(s, *m.path.last().unwrap_or(&m.path[0]));
                let d = sub(a, b);
                e += 0.5 * k * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
            }
        }
    }
    e
}

fn soft_grad(model: &Model, s: &State, allowed: &[bool]) -> Vec<f64> {
    let eq = assemble(model, s, allowed);
    let t = soft_forces(model, s, &eq);
    lag_grad(&eq, &t)
}

/// Phase A: damped Newton on the soft energy with a positive-definite
/// Hessian, so every step goes downhill (a rigid body never swings "up and
/// over" to a far equilibrium). Returns iterations used.
fn soft_relax(model: &Model, state: &mut State, allowed: &[bool]) -> usize {
    let dof = model.dof();
    let lchar = model.l_char;
    let w = model.w_total;
    let mut mu = 1e-6 * w / lchar;
    for it in 0..300 {
        let g = soft_grad(model, state, allowed);
        let gmax = g.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        if gmax <= 1e-9 * w {
            return it;
        }
        let h_step = 1e-6 * lchar;
        let mut h = Mat::zeros(dof, dof);
        for j in 0..dof {
            let mut e = vec![0.0; dof];
            e[j] = h_step;
            let gp = soft_grad(model, &perturb(model, state, &e), allowed);
            e[j] = -h_step;
            let gm = soft_grad(model, &perturb(model, state, &e), allowed);
            for i in 0..dof {
                h[(i, j)] = (gp[i] - gm[i]) / (2.0 * h_step);
            }
        }
        for i in 0..dof {
            for j in (i + 1)..dof {
                let v = 0.5 * (h[(i, j)] + h[(j, i)]);
                h[(i, j)] = v;
                h[(j, i)] = v;
            }
        }
        let e0 = soft_energy(model, state, allowed);
        let mut accepted = false;
        for _try in 0..40 {
            let mut k = h.clone();
            for i in 0..dof {
                k[(i, i)] += mu;
            }
            let rhs: Vec<f64> = g.iter().map(|v| -v).collect();
            let Some(dq) = solve_dense(&k, &rhs) else {
                mu *= 10.0;
                continue;
            };
            let slope: f64 = dq.iter().zip(&g).map(|(a, b)| a * b).sum();
            if slope >= 0.0 {
                // Not a descent direction: the Hessian is indefinite here.
                mu *= 10.0;
                continue;
            }
            let big = dq.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            let mut alpha = if big > 0.05 * lchar {
                0.05 * lchar / big
            } else {
                1.0
            };
            for _ls in 0..30 {
                let step: Vec<f64> = dq.iter().map(|v| v * alpha).collect();
                let trial = perturb(model, state, &step);
                let e1 = soft_energy(model, &trial, allowed);
                if e1 <= e0 + 1e-4 * alpha * slope {
                    *state = trial;
                    accepted = true;
                    break;
                }
                alpha *= 0.5;
            }
            if accepted {
                mu = (mu * 0.3).max(1e-12 * w / lchar);
                break;
            }
            mu *= 10.0;
        }
        if !accepted {
            return it;
        }
    }
    300
}

/// Multiplier storage: index `i` for member i tension; `n + 3i + ax` for link axis.
fn store(eq: &Equilibrium, t: &[f64], lam: &mut [f64], n: usize) {
    for (u, v) in eq.unknowns.iter().zip(t) {
        match u {
            Unknown::Tension(i) => lam[*i] = *v,
            Unknown::Link(i, ax) => lam[n + 3 * i + *ax as usize] = *v,
        }
    }
}

fn gather(eq: &Equilibrium, lam: &[f64], n: usize) -> Vec<f64> {
    eq.unknowns
        .iter()
        .map(|u| match u {
            Unknown::Tension(i) => lam[*i],
            Unknown::Link(i, ax) => lam[n + 3 * i + *ax as usize],
        })
        .collect()
}

/// Constraint value per unknown column: `ℓ − L` for tension, `(x_first − x_last)[ax]` for links.
fn constraint_values(model: &Model, state: &State, eq: &Equilibrium) -> Vec<f64> {
    eq.unknowns
        .iter()
        .map(|u| match u {
            Unknown::Tension(i) => model.path_length(state, *i) - model.members[*i].length,
            Unknown::Link(i, ax) => {
                let m = &model.members[*i];
                let a = model.pos(state, m.path[0]);
                let b = model.pos(state, *m.path.last().unwrap_or(&m.path[0]));
                sub(a, b)[*ax as usize]
            }
        })
        .collect()
}

/// Apply a generalised displacement.
pub fn perturb(model: &Model, state: &State, dq: &[f64]) -> State {
    let mut s = state.clone();
    let nb = model.bodies.len();
    for i in 0..nb {
        let d = [dq[6 * i], dq[6 * i + 1], dq[6 * i + 2]];
        let phi = [
            dq[6 * i + 3] / model.l_char,
            dq[6 * i + 4] / model.l_char,
            dq[6 * i + 5] / model.l_char,
        ];
        s.poses[i] = state.poses[i].perturbed(d, phi);
    }
    for k in 0..model.free_nodes.len() {
        let base = 6 * nb + 3 * k;
        for ax in 0..3 {
            s.free[k][ax] += dq[base + ax];
        }
    }
    s
}

/// Lagrangian gradient `b(q) − A(q) t` over the equilibrium rows.
fn lag_grad(eq: &Equilibrium, t: &[f64]) -> Vec<f64> {
    let at = eq.a.mul_vec(t);
    eq.b.iter().zip(&at).map(|(b, a)| b - a).collect()
}

fn merit(model: &Model, state: &State, active: &[bool], lam: &[f64], n: usize) -> f64 {
    let eq = assemble(model, state, active);
    let t = gather(&eq, lam, n);
    let g = lag_grad(&eq, &t);
    let comp = column_compliance(model, &eq);
    let cons = constraint_values(model, state, &eq);
    let fw: f64 = g.iter().map(|v| (v / model.w_total).powi(2)).sum();
    let fl: f64 = cons
        .iter()
        .zip(&comp)
        .zip(&t)
        .map(|((g, c), l)| ((g - c * l) / model.l_char).powi(2))
        .sum();
    fw + fl
}

/// Newton on the KKT equations for a fixed active set. Returns (converged, iterations).
/// Why a Newton pass stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Converged,
    /// A slack member became taut along the step; it joined the active set.
    Activated,
    Stalled,
}

/// Largest normalised overstretch `(ℓ − L)/L` among allowed slack members.
fn worst_block(model: &Model, s: &State, active: &[bool], allowed: &[bool]) -> (f64, usize) {
    let mut worst = (f64::NEG_INFINITY, usize::MAX);
    for (i, m) in model.members.iter().enumerate() {
        if active[i] || !allowed.get(i).copied().unwrap_or(true) || m.kind != MemberKind::Tension {
            continue;
        }
        let v = (model.path_length(s, i) - m.length) / m.length.max(1e-9);
        if v > worst.0 {
            worst = (v, i);
        }
    }
    worst
}

#[allow(clippy::too_many_arguments)]
fn newton(
    model: &Model,
    state: &mut State,
    active: &mut [bool],
    allowed: &[bool],
    lam: &mut [f64],
    n: usize,
    opts: HangOptions,
) -> (Outcome, usize) {
    let dof = model.dof();
    let lchar = model.l_char;
    let w = model.w_total;
    let mut mu = 1e-9 * w / lchar;
    for it in 0..opts.max_iter {
        let eq = assemble(model, state, active);
        let t = gather(&eq, lam, n);
        let g = lag_grad(&eq, &t);
        let comp = column_compliance(model, &eq);
        let cons = constraint_values(model, state, &eq);
        let r2: Vec<f64> = cons
            .iter()
            .zip(&comp)
            .zip(&t)
            .map(|((g, c), l)| g - c * l)
            .collect();
        let gmax = g.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let rmax = r2.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        if gmax <= opts.force_rtol * w && rmax <= opts.length_rtol * lchar {
            return (Outcome::Converged, it);
        }

        // Finite-difference Hessian of the Lagrangian at fixed multipliers.
        let h_step = 1e-6 * lchar;
        let mut h = Mat::zeros(dof, dof);
        for j in 0..dof {
            let mut e = vec![0.0; dof];
            e[j] = h_step;
            let sp = perturb(model, state, &e);
            e[j] = -h_step;
            let sm = perturb(model, state, &e);
            let gp = lag_grad(&assemble(model, &sp, active), &t);
            let gm = lag_grad(&assemble(model, &sm, active), &t);
            for i in 0..dof {
                // H = ∂(∇L)/∂q: gravity + geometric stiffness.
                h[(i, j)] = (gp[i] - gm[i]) / (2.0 * h_step);
            }
        }
        // Symmetrise.
        for i in 0..dof {
            for j in (i + 1)..dof {
                let v = 0.5 * (h[(i, j)] + h[(j, i)]);
                h[(i, j)] = v;
                h[(j, i)] = v;
            }
        }

        let nl = eq.unknowns.len();
        let mut accepted = false;
        let m0 = merit(model, state, active, lam, n);
        for _try in 0..30 {
            let sz = dof + nl;
            let mut k = Mat::zeros(sz, sz);
            for i in 0..dof {
                for j in 0..dof {
                    k[(i, j)] = h[(i, j)];
                }
                k[(i, i)] += mu;
            }
            for (c, cv) in comp.iter().enumerate() {
                for i in 0..dof {
                    let a = eq.a[(i, c)];
                    k[(i, dof + c)] = -a;
                    k[(dof + c, i)] = -a;
                }
                k[(dof + c, dof + c)] = -cv;
            }
            // [H+μI, −A; −Aᵀ, −C]·[Δq; Δλ] = −[∇L; g − Cλ],  ∇L = b − A λ.
            let mut rhs = vec![0.0; sz];
            for i in 0..dof {
                rhs[i] = -g[i];
            }
            for c in 0..nl {
                rhs[dof + c] = -r2[c];
            }
            let Some(sol) = solve_dense(&k, &rhs) else {
                mu *= 10.0;
                continue;
            };
            let mut dq: Vec<f64> = sol[..dof].to_vec();
            // Trust region: 0.05 l_char translation, 0.05 rad rotation.
            let cap = 0.05 * lchar;
            let big = dq.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            let mut frac = 1.0;
            if big > cap {
                frac = cap / big;
            }
            let mut alpha = frac;
            for _ls in 0..12 {
                for i in 0..dof {
                    dq[i] = alpha * sol[i];
                }
                let trial = perturb(model, state, &dq);
                // A slack member must not be pulled past its length: stop at
                // the first one that goes taut and make it active.
                if worst_block(model, &trial, active, allowed).0 > 1e-9 {
                    let (mut lo, mut hi) = (0.0_f64, alpha);
                    for _ in 0..50 {
                        let mid = 0.5 * (lo + hi);
                        let dm: Vec<f64> = sol[..dof].iter().map(|v| v * mid).collect();
                        let s = perturb(model, state, &dm);
                        if worst_block(model, &s, active, allowed).0 > 0.0 {
                            hi = mid;
                        } else {
                            lo = mid;
                        }
                    }
                    let dm: Vec<f64> = sol[..dof].iter().map(|v| v * lo).collect();
                    *state = perturb(model, state, &dm);
                    let mut t_new = t.clone();
                    for c in 0..nl {
                        t_new[c] += lo * sol[dof + c];
                    }
                    store(&eq, &t_new, lam, n);
                    for (i, m) in model.members.iter().enumerate() {
                        if !active[i]
                            && allowed.get(i).copied().unwrap_or(true)
                            && m.kind == MemberKind::Tension
                            && model.path_length(state, i) - m.length >= -1e-7 * m.length.max(1.0)
                        {
                            active[i] = true;
                            lam[i] = 0.0;
                        }
                    }
                    return (Outcome::Activated, it);
                }
                let mut lam_t = lam.to_vec();
                let mut t_new = t.clone();
                for c in 0..nl {
                    t_new[c] += alpha * sol[dof + c];
                }
                store(&eq, &t_new, &mut lam_t, n);
                let m1 = merit(model, &trial, active, &lam_t, n);
                if m1 < m0 || m1 <= 1e-28 {
                    *state = trial;
                    lam.copy_from_slice(&lam_t);
                    accepted = true;
                    break;
                }
                alpha *= 0.5;
            }
            if accepted {
                mu = (mu * 0.3).max(1e-12 * w / lchar);
                break;
            }
            mu *= 10.0;
        }
        if !accepted {
            return (Outcome::Stalled, it);
        }
    }
    (Outcome::Stalled, opts.max_iter)
}
