//! Small dense linear algebra for the solver: one-sided Jacobi SVD, rank,
//! least-squares / minimum-norm solve.
//!
//! Step: 3
//! Theory: the rigs are tiny (tens of equations), so a dense SVD is the whole
//! story: rank for §3.5 determinacy, pseudo-inverse for the force method, and
//! an orthonormal row basis for the bounds LP. No external crate.
//! Must not depend on: UI, dioxus, store.

// Index loops read like the math here.
#![allow(clippy::needless_range_loop)]

/// Row-major dense matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct Mat {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Mat {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m[(i, i)] = 1.0;
        }
        m
    }

    pub fn transpose(&self) -> Self {
        let mut t = Self::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                t[(j, i)] = self[(i, j)];
            }
        }
        t
    }

    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        debug_assert_eq!(x.len(), self.cols);
        (0..self.rows)
            .map(|i| {
                let row = &self.data[i * self.cols..(i + 1) * self.cols];
                row.iter().zip(x).map(|(a, b)| a * b).sum()
            })
            .collect()
    }

    pub fn col(&self, j: usize) -> Vec<f64> {
        (0..self.rows).map(|i| self[(i, j)]).collect()
    }

    /// Scale column `j` by `s`.
    pub fn scale_col(&mut self, j: usize, s: f64) {
        for i in 0..self.rows {
            self[(i, j)] *= s;
        }
    }

    pub fn max_abs(&self) -> f64 {
        self.data.iter().fold(0.0_f64, |m, v| m.max(v.abs()))
    }
}

impl std::ops::Index<(usize, usize)> for Mat {
    type Output = f64;
    fn index(&self, (i, j): (usize, usize)) -> &f64 {
        &self.data[i * self.cols + j]
    }
}

impl std::ops::IndexMut<(usize, usize)> for Mat {
    fn index_mut(&mut self, (i, j): (usize, usize)) -> &mut f64 {
        &mut self.data[i * self.cols + j]
    }
}

/// Thin SVD `A = U · diag(s) · Vᵀ`, `k = min(rows, cols)` triplets,
/// singular values sorted descending.
#[derive(Debug, Clone)]
pub struct Svd {
    /// rows × k
    pub u: Mat,
    pub s: Vec<f64>,
    /// cols × k
    pub v: Mat,
}

/// Relative singular-value cut-off for the pseudo-inverse (numerical zero).
pub const RANK_RTOL: f64 = 1e-10;

/// Relative cut-off for **physical** rank decisions (determinacy, the bounds
/// LP). An equation whose only coefficients are a few 1e-8 of a leg's force
/// — a slack chain hanging 0.000002° off plumb under a bar that is free to
/// swing that way — is a mechanism direction: an imperceptible pose change
/// satisfies it. Counting it as a constraint would pin that leg at zero.
pub const MECH_RTOL: f64 = 1e-6;

impl Svd {
    pub fn of(a: &Mat) -> Self {
        if a.rows >= a.cols {
            jacobi(a)
        } else {
            let t = jacobi(&a.transpose());
            Self {
                u: t.v,
                s: t.s,
                v: t.u,
            }
        }
    }

    pub fn tol(&self) -> f64 {
        self.s.first().copied().unwrap_or(0.0) * RANK_RTOL
    }

    pub fn rank(&self) -> usize {
        self.rank_with(RANK_RTOL)
    }

    /// Rank with a relative singular-value cut-off.
    pub fn rank_with(&self, rtol: f64) -> usize {
        let tol = self.s.first().copied().unwrap_or(0.0) * rtol;
        self.s.iter().filter(|s| **s > tol && **s > 0.0).count()
    }

    /// Minimum-norm least-squares solution of `A x = b`.
    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        let k = self.s.len();
        let tol = self.tol();
        let mut x = vec![0.0; self.v.rows];
        for c in 0..k {
            let s = self.s[c];
            if s <= tol || s == 0.0 {
                continue;
            }
            let mut ub = 0.0;
            for i in 0..self.u.rows {
                ub += self.u[(i, c)] * b[i];
            }
            let coef = ub / s;
            for (j, xj) in x.iter_mut().enumerate() {
                *xj += coef * self.v[(j, c)];
            }
        }
        x
    }

    /// Orthonormal basis (as rows) of the row space: `Uᵣᵀ A` has `r` independent rows.
    /// Returns `(Uᵣᵀ A, Uᵣᵀ b)`.
    pub fn reduce_rows(&self, a: &Mat, b: &[f64]) -> (Mat, Vec<f64>) {
        self.reduce_rows_with(a, b, RANK_RTOL)
    }

    /// [`Self::reduce_rows`] keeping singular directions above `rtol`.
    pub fn reduce_rows_with(&self, a: &Mat, b: &[f64], rtol: f64) -> (Mat, Vec<f64>) {
        let r = self.rank_with(rtol);
        let mut ra = Mat::zeros(r, a.cols);
        let mut rb = vec![0.0; r];
        for c in 0..r {
            for i in 0..a.rows {
                let u = self.u[(i, c)];
                if u == 0.0 {
                    continue;
                }
                for j in 0..a.cols {
                    ra[(c, j)] += u * a[(i, j)];
                }
                rb[c] += u * b[i];
            }
        }
        (ra, rb)
    }
}

/// One-sided Jacobi (Hestenes) for `rows >= cols`.
fn jacobi(a: &Mat) -> Svd {
    let m = a.rows;
    let n = a.cols;
    // Work on columns: w = A (m×n), v = I (n×n).
    let mut w: Vec<Vec<f64>> = (0..n).map(|j| a.col(j)).collect();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|j| {
            let mut e = vec![0.0; n];
            e[j] = 1.0;
            e
        })
        .collect();

    for _sweep in 0..80 {
        let mut rotated = false;
        for p in 0..n {
            for q in (p + 1)..n {
                let alpha: f64 = w[p].iter().map(|x| x * x).sum();
                let beta: f64 = w[q].iter().map(|x| x * x).sum();
                let gamma: f64 = w[p].iter().zip(&w[q]).map(|(x, y)| x * y).sum();
                if gamma == 0.0 || gamma.abs() <= 1e-15 * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for i in 0..m {
                    let xp = w[p][i];
                    let xq = w[q][i];
                    w[p][i] = c * xp - s * xq;
                    w[q][i] = s * xp + c * xq;
                }
                for i in 0..n {
                    let xp = v[p][i];
                    let xq = v[q][i];
                    v[p][i] = c * xp - s * xq;
                    v[q][i] = s * xp + c * xq;
                }
            }
        }
        if !rotated {
            break;
        }
    }

    let mut triplets: Vec<(f64, Vec<f64>, Vec<f64>)> = (0..n)
        .map(|j| {
            let norm = w[j].iter().map(|x| x * x).sum::<f64>().sqrt();
            let u = if norm > 0.0 {
                w[j].iter().map(|x| x / norm).collect()
            } else {
                vec![0.0; m]
            };
            (norm, u, v[j].clone())
        })
        .collect();
    triplets.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut u = Mat::zeros(m, n);
    let mut vv = Mat::zeros(n, n);
    let mut s = Vec::with_capacity(n);
    for (c, (sv, uc, vc)) in triplets.into_iter().enumerate() {
        s.push(sv);
        for i in 0..m {
            u[(i, c)] = uc[i];
        }
        for i in 0..n {
            vv[(i, c)] = vc[i];
        }
    }
    Svd { u, s, v: vv }
}

/// Solve a small dense symmetric system `(H + λI) x = g` by Gaussian
/// elimination with partial pivoting. `None` if singular.
pub fn solve_dense(h: &Mat, g: &[f64]) -> Option<Vec<f64>> {
    let n = h.rows;
    let mut a = h.clone();
    let mut b = g.to_vec();
    for k in 0..n {
        let mut piv = k;
        let mut best = a[(k, k)].abs();
        for i in (k + 1)..n {
            if a[(i, k)].abs() > best {
                best = a[(i, k)].abs();
                piv = i;
            }
        }
        if best < 1e-300 {
            return None;
        }
        if piv != k {
            for j in 0..n {
                let t = a[(k, j)];
                a[(k, j)] = a[(piv, j)];
                a[(piv, j)] = t;
            }
            b.swap(k, piv);
        }
        for i in (k + 1)..n {
            let f = a[(i, k)] / a[(k, k)];
            if f == 0.0 {
                continue;
            }
            for j in k..n {
                a[(i, j)] -= f * a[(k, j)];
            }
            b[i] -= f * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let mut s = b[k];
        for j in (k + 1)..n {
            s -= a[(k, j)] * x[j];
        }
        x[k] = s / a[(k, k)];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mat(rows: usize, cols: usize, v: &[f64]) -> Mat {
        Mat {
            rows,
            cols,
            data: v.to_vec(),
        }
    }

    #[test]
    fn svd_reconstructs_and_ranks() {
        // Rank 2, 3×3.
        let a = mat(3, 3, &[1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 0.0, 1.0]);
        let svd = Svd::of(&a);
        assert_eq!(svd.rank(), 2);
        for i in 0..3 {
            for j in 0..3 {
                let mut x = 0.0;
                for c in 0..3 {
                    x += svd.u[(i, c)] * svd.s[c] * svd.v[(j, c)];
                }
                assert!((x - a[(i, j)]).abs() < 1e-12, "{i},{j}");
            }
        }
    }

    #[test]
    fn wide_matrix_min_norm() {
        // x + y = 2 → min-norm (1, 1)
        let a = mat(1, 2, &[1.0, 1.0]);
        let svd = Svd::of(&a);
        assert_eq!(svd.rank(), 1);
        let x = svd.solve(&[2.0]);
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn reduce_rows_drops_duplicates() {
        let a = mat(3, 2, &[1.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
        let svd = Svd::of(&a);
        let (ra, rb) = svd.reduce_rows(&a, &[2.0, 2.0, 3.0]);
        assert_eq!(ra.rows, 2);
        let x = Svd::of(&ra).solve(&rb);
        assert!((x[0] - 2.0).abs() < 1e-12 && (x[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn dense_solve() {
        let h = mat(2, 2, &[4.0, 1.0, 1.0, 3.0]);
        let x = solve_dense(&h, &[1.0, 2.0]).unwrap();
        assert!((4.0 * x[0] + x[1] - 1.0).abs() < 1e-12);
        assert!((x[0] + 3.0 * x[1] - 2.0).abs() < 1e-12);
    }
}
