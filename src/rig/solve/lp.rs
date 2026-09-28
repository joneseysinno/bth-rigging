//! Dense two-phase simplex (Bland's rule) for the bounds envelope.
//!
//! Step: 3
//! Theory: `max cᵀx  s.t.  A x = b,  x ≥ 0`. Problems here have tens of rows
//! and columns, so a textbook tableau is plenty and has no dependencies.
//! Must not depend on: UI, dioxus.

// Index loops read like the math here.
#![allow(clippy::needless_range_loop)]

use super::linalg::Mat;

#[derive(Debug, Clone, PartialEq)]
pub enum LpResult {
    Optimal { x: Vec<f64>, value: f64 },
    Unbounded,
    Infeasible,
}

const EPS: f64 = 1e-10;

/// Maximise `c·x` subject to `A x = b`, `x ≥ 0`.
pub fn maximize(a: &Mat, b: &[f64], c: &[f64]) -> LpResult {
    let m = a.rows;
    let n = a.cols;
    let width = n + m + 1; // x, artificials, rhs
    let mut t = vec![vec![0.0; width]; m];
    for i in 0..m {
        let flip = if b[i] < 0.0 { -1.0 } else { 1.0 };
        for j in 0..n {
            t[i][j] = flip * a[(i, j)];
        }
        t[i][n + i] = 1.0;
        t[i][width - 1] = flip * b[i];
    }
    let mut basis: Vec<usize> = (n..n + m).collect();

    // Phase 1: maximise −Σ artificials.
    let mut obj1 = vec![0.0; width];
    for j in n..n + m {
        obj1[j] = -1.0;
    }
    if !run(&mut t, &mut basis, &obj1, n + m) {
        return LpResult::Infeasible; // cannot be unbounded in phase 1
    }
    let art: f64 = basis
        .iter()
        .enumerate()
        .filter(|(_, bj)| **bj >= n)
        .map(|(i, _)| t[i][width - 1])
        .sum();
    let scale = b.iter().fold(1.0_f64, |s, v| s.max(v.abs()));
    if art > 1e-8 * scale {
        return LpResult::Infeasible;
    }
    // Drive remaining artificials out of the basis; drop redundant rows.
    let mut i = 0;
    while i < t.len() {
        if basis[i] >= n {
            let best = (0..n)
                .map(|j| (j, t[i][j].abs()))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((j, v)) = best
                && v > 1e-9
            {
                pivot(&mut t, &mut basis, i, j);
            } else {
                t.remove(i);
                basis.remove(i);
                continue;
            }
        }
        i += 1;
    }
    // Phase 2 on the x columns only (artificial columns frozen out).
    for row in t.iter_mut() {
        for v in row.iter_mut().take(n + m).skip(n) {
            *v = 0.0;
        }
    }
    let mut obj = vec![0.0; width];
    obj[..n].copy_from_slice(c);
    if !run(&mut t, &mut basis, &obj, n) {
        return LpResult::Unbounded;
    }
    let mut x = vec![0.0; n];
    for (r, bj) in basis.iter().enumerate() {
        if *bj < n {
            x[*bj] = t[r][width - 1];
        }
    }
    let value = c.iter().zip(&x).map(|(a, b)| a * b).sum();
    LpResult::Optimal { x, value }
}

fn pivot(t: &mut [Vec<f64>], basis: &mut [usize], r: usize, col: usize) {
    let p = t[r][col];
    for v in t[r].iter_mut() {
        *v /= p;
    }
    let prow = t[r].clone();
    for (i, row) in t.iter_mut().enumerate() {
        if i == r {
            continue;
        }
        let f = row[col];
        if f == 0.0 {
            continue;
        }
        for (v, pv) in row.iter_mut().zip(&prow) {
            *v -= f * pv;
        }
    }
    basis[r] = col;
}

/// Simplex iterations maximising `obj` over columns `0..ncols`. `false` = unbounded.
fn run(t: &mut [Vec<f64>], basis: &mut [usize], obj: &[f64], ncols: usize) -> bool {
    let width = obj.len();
    for _ in 0..10_000 {
        // Reduced cost r_j = c_j − c_B · column_j.
        let mut enter = None;
        for j in 0..ncols {
            if basis.contains(&j) {
                continue;
            }
            let mut rc = obj[j];
            for (i, bi) in basis.iter().enumerate() {
                rc -= obj[*bi] * t[i][j];
            }
            if rc > EPS {
                enter = Some(j);
                break; // Bland: smallest index
            }
        }
        let Some(j) = enter else {
            return true;
        };
        let mut leave: Option<(usize, f64)> = None;
        for (i, row) in t.iter().enumerate() {
            if row[j] > EPS {
                let ratio = row[width - 1] / row[j];
                match leave {
                    None => leave = Some((i, ratio)),
                    Some((li, lr)) => {
                        if ratio < lr - 1e-12 || (ratio <= lr + 1e-12 && basis[i] < basis[li]) {
                            leave = Some((i, ratio));
                        }
                    }
                }
            }
        }
        let Some((r, _)) = leave else {
            return false;
        };
        pivot(t, basis, r, j);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_lp() {
        // max x0  s.t. x0 + x1 = 4, x0 − x2 = 1  → x0 = 4
        let a = Mat {
            rows: 2,
            cols: 3,
            data: vec![1.0, 1.0, 0.0, 1.0, 0.0, -1.0],
        };
        match maximize(&a, &[4.0, 1.0], &[1.0, 0.0, 0.0]) {
            LpResult::Optimal { x, value } => {
                assert!((value - 4.0).abs() < 1e-12);
                assert!((x[0] - 4.0).abs() < 1e-12);
            }
            other => panic!("{other:?}"),
        }
        // min x0 → x0 = 1
        match maximize(&a, &[4.0, 1.0], &[-1.0, 0.0, 0.0]) {
            LpResult::Optimal { x, .. } => assert!((x[0] - 1.0).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unbounded_and_infeasible() {
        let a = Mat {
            rows: 1,
            cols: 2,
            data: vec![1.0, -1.0],
        };
        assert_eq!(maximize(&a, &[1.0], &[1.0, 0.0]), LpResult::Unbounded);
        let a = Mat {
            rows: 1,
            cols: 2,
            data: vec![1.0, 1.0],
        };
        assert_eq!(maximize(&a, &[-1.0], &[1.0, 0.0]), LpResult::Infeasible);
    }
}
