//! Sparse symmetric matrices and the solvers: sparse Cholesky ([`faer`]), Jacobi-preconditioned
//! conjugate gradients for very large systems, and shift-invert Lanczos for the lowest
//! eigenpairs of `K φ = λ M φ`.

use faer::linalg::solvers::Solve;
use faer::sparse::{SparseColMat, SymbolicSparseColMat};
use faer::{Col, Side};

/// A symmetric sparse matrix in compressed rows, both triangles stored.
#[derive(Debug, Clone, Default)]
pub struct SymMatrix {
  pub n: usize,
  pub row_ptr: Vec<usize>,
  pub cols: Vec<u32>,
  pub vals: Vec<f64>,
}

impl SymMatrix {
  /// An all-zero matrix with this pattern (each row's columns sorted, the diagonal included).
  pub fn with_pattern(rows: Vec<Vec<u32>>) -> SymMatrix {
    let n = rows.len();
    let mut row_ptr = Vec::with_capacity(n + 1);
    row_ptr.push(0);
    let mut cols = Vec::new();
    for r in rows {
      cols.extend(r);
      row_ptr.push(cols.len());
    }
    let nnz = cols.len();
    SymMatrix { n, row_ptr, cols, vals: vec![0.0; nnz] }
  }

  /// Where entry (r, c) is stored.
  #[inline]
  pub fn find(&self, r: usize, c: usize) -> Option<usize> {
    let s = &self.cols[self.row_ptr[r]..self.row_ptr[r + 1]];
    s.binary_search(&(c as u32)).ok().map(|k| self.row_ptr[r] + k)
  }

  #[inline]
  pub fn add(&mut self, r: usize, c: usize, v: f64) {
    if let Some(k) = self.find(r, c) {
      self.vals[k] += v;
    }
  }

  pub fn diag(&self) -> Vec<f64> {
    (0..self.n).map(|r| self.find(r, r).map(|k| self.vals[k]).unwrap_or(0.0)).collect()
  }

  pub fn mul(&self, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0; self.n];
    for (r, out) in y.iter_mut().enumerate() {
      let mut s = 0.0;
      for k in self.row_ptr[r]..self.row_ptr[r + 1] {
        s += self.vals[k] * x[self.cols[k] as usize];
      }
      *out = s;
    }
    y
  }

  /// The same pattern with `vals = a·self + b·other` (other must share the pattern).
  pub fn combine(&self, a: f64, other: &SymMatrix, b: f64) -> SymMatrix {
    SymMatrix {
      n: self.n,
      row_ptr: self.row_ptr.clone(),
      cols: self.cols.clone(),
      vals: self.vals.iter().zip(&other.vals).map(|(x, y)| a * x + b * y).collect(),
    }
  }

  /// The upper triangle as a faer matrix.
  fn upper(&self) -> SparseColMat<usize, f64> {
    let mut col_ptr = Vec::with_capacity(self.n + 1);
    col_ptr.push(0usize);
    let mut row_idx = Vec::new();
    let mut vals = Vec::new();
    // column r of the upper triangle = row r's entries left of the diagonal (symmetry)
    for r in 0..self.n {
      for k in self.row_ptr[r]..self.row_ptr[r + 1] {
        let c = self.cols[k] as usize;
        if c <= r {
          row_idx.push(c);
          vals.push(self.vals[k]);
        }
      }
      col_ptr.push(row_idx.len());
    }
    SparseColMat::new(SymbolicSparseColMat::new_checked(self.n, self.n, col_ptr, None, row_idx), vals)
  }
}

/// A factorized symmetric positive definite matrix.
pub struct Factor {
  llt: faer::sparse::linalg::solvers::Llt<usize, f64>,
  n: usize,
}

impl Factor {
  pub fn new(a: &SymMatrix) -> Result<Factor, String> {
    if a.n == 0 {
      return Err("there is nothing to solve".into());
    }
    let m = a.upper();
    let llt = m.sp_cholesky(Side::Upper).map_err(|_| {
      "the stiffness matrix isn't positive definite: the model is probably under-constrained (it can move or \
       spin freely); add constraints, or bond the parts together"
        .to_string()
    })?;
    Ok(Factor { llt, n: a.n })
  }

  pub fn solve(&self, b: &[f64]) -> Vec<f64> {
    let rhs = Col::<f64>::from_fn(self.n, |i| b[i]);
    let x = self.llt.solve(&rhs);
    (0..self.n).map(|i| x[i]).collect()
  }
}

pub fn dot(a: &[f64], b: &[f64]) -> f64 {
  a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Conjugate gradients preconditioned by the diagonal; returns the solution and the iterations.
pub fn pcg(a: &SymMatrix, b: &[f64], tol: f64, max_iter: usize) -> Result<(Vec<f64>, usize), String> {
  let d: Vec<f64> = a.diag().iter().map(|&x| if x > 0.0 { 1.0 / x } else { 1.0 }).collect();
  let mut x = vec![0.0; a.n];
  let mut r = b.to_vec();
  let bn = dot(b, b).sqrt();
  if bn == 0.0 {
    return Ok((x, 0));
  }
  let mut z: Vec<f64> = r.iter().zip(&d).map(|(r, d)| r * d).collect();
  let mut p = z.clone();
  let mut rz = dot(&r, &z);
  for it in 0..max_iter {
    let ap = a.mul(&p);
    let pap = dot(&p, &ap);
    if !(pap > 0.0) {
      return Err("conjugate gradients broke down: the model is probably under-constrained".into());
    }
    let alpha = rz / pap;
    for i in 0..a.n {
      x[i] += alpha * p[i];
      r[i] -= alpha * ap[i];
    }
    if dot(&r, &r).sqrt() <= tol * bn {
      return Ok((x, it + 1));
    }
    for i in 0..a.n {
      z[i] = r[i] * d[i];
    }
    let rz2 = dot(&r, &z);
    let beta = rz2 / rz;
    rz = rz2;
    for i in 0..a.n {
      p[i] = z[i] + beta * p[i];
    }
  }
  Err(format!("conjugate gradients didn't converge in {max_iter} iterations"))
}

/// Solves `a x = b`: sparse Cholesky up to `direct_limit` unknowns, conjugate gradients above.
pub fn solve(a: &SymMatrix, b: &[f64], direct_limit: usize) -> Result<Vec<f64>, String> {
  if a.n <= direct_limit { Ok(Factor::new(a)?.solve(b)) } else { pcg(a, b, 1e-10, 20 * a.n.max(1000)).map(|r| r.0) }
}

/// The lowest `count` eigenpairs of `K φ = λ M φ` (K symmetric semi-definite, M positive definite):
/// shift-invert Lanczos with full reorthogonalization about `shift` (below the lowest
/// eigenvalue). Eigenvectors are M-normalized (φᵀ M φ = 1). Eigenvalues come out ascending.
pub fn eigen_lowest(k: &SymMatrix, m: &SymMatrix, count: usize, shift: f64) -> Result<Vec<(f64, Vec<f64>)>, String> {
  let n = k.n;
  let count = count.min(n);
  if count == 0 {
    return Ok(Vec::new());
  }
  let factor = Factor::new(&k.combine(1.0, m, -shift))?;
  let mut steps = (2 * count + 20).min(n);
  loop {
    let r = lanczos(&factor, m, n, count, steps, shift);
    match r {
      Some(v) => return Ok(v),
      None if steps >= n || steps > 2000 => {
        return lanczos_all(&factor, m, n, count, steps.min(n), shift)
          .ok_or_else(|| "the frequencies didn't converge".to_string());
      }
      None => steps = (steps * 2).min(n),
    }
  }
}

/// One Lanczos run of `steps`; `None` when the wanted pairs haven't converged.
fn lanczos(
  f: &Factor,
  m: &SymMatrix,
  n: usize,
  count: usize,
  steps: usize,
  shift: f64,
) -> Option<Vec<(f64, Vec<f64>)>> {
  let pairs = lanczos_run(f, m, n, steps)?;
  let (theta, s, q, beta_last) = pairs;
  let mut out = Vec::new();
  // largest θ first: θ = 1 / (λ − σ)
  let mut order: Vec<usize> = (0..theta.len()).collect();
  order.sort_by(|a, b| theta[*b].total_cmp(&theta[*a]));
  for &i in order.iter().take(count) {
    let resid = (beta_last * s[(s.nrows() - 1, i)]).abs();
    if resid > 1e-8 * theta[i].abs().max(1e-300) && q.len() < n {
      return None;
    }
    out.push(ritz(theta[i], &s, i, &q, shift));
  }
  Some(out)
}

fn lanczos_all(
  f: &Factor,
  m: &SymMatrix,
  n: usize,
  count: usize,
  steps: usize,
  shift: f64,
) -> Option<Vec<(f64, Vec<f64>)>> {
  let (theta, s, q, _) = lanczos_run(f, m, n, steps)?;
  let mut order: Vec<usize> = (0..theta.len()).collect();
  order.sort_by(|a, b| theta[*b].total_cmp(&theta[*a]));
  Some(order.iter().take(count).map(|&i| ritz(theta[i], &s, i, &q, shift)).collect())
}

fn ritz(theta: f64, s: &faer::Mat<f64>, i: usize, q: &[Vec<f64>], shift: f64) -> (f64, Vec<f64>) {
  let lambda = shift + 1.0 / theta;
  let n = q[0].len();
  let mut v = vec![0.0; n];
  for (j, qj) in q.iter().enumerate() {
    let c = s[(j, i)];
    for t in 0..n {
      v[t] += c * qj[t];
    }
  }
  (lambda, v)
}

type Run = (Vec<f64>, faer::Mat<f64>, Vec<Vec<f64>>, f64);

fn lanczos_run(f: &Factor, m: &SymMatrix, n: usize, steps: usize) -> Option<Run> {
  let mut seed = 0x1234_5678_9abc_def0u64;
  let mut rnd = || {
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    (seed as f64 / u64::MAX as f64) - 0.5
  };
  let mut q: Vec<Vec<f64>> = Vec::with_capacity(steps);
  let mut mq: Vec<Vec<f64>> = Vec::with_capacity(steps);
  let mut alpha = Vec::with_capacity(steps);
  let mut beta: Vec<f64> = Vec::with_capacity(steps);
  let start: Vec<f64> = (0..n).map(|_| rnd()).collect();
  let mut w = start;
  let mut beta_last = 0.0;
  for j in 0..steps {
    // orthogonalize w against the basis (M inner product) and normalize
    for _ in 0..2 {
      for (qi, mqi) in q.iter().zip(&mq) {
        let c = dot(mqi, &w);
        for t in 0..n {
          w[t] -= c * qi[t];
        }
      }
    }
    let mw = m.mul(&w);
    let nrm = dot(&w, &mw).max(0.0).sqrt();
    if j > 0 {
      beta.push(nrm);
    }
    if nrm < 1e-300 || (j > 0 && nrm < 1e-12 * alpha.iter().fold(0.0f64, |a: f64, b: &f64| a.max(b.abs()))) {
      // invariant subspace: restart with a fresh random vector
      if j > 0 {
        *beta.last_mut()? = 0.0;
      }
      w = (0..n).map(|_| rnd()).collect();
      for _ in 0..2 {
        for (qi, mqi) in q.iter().zip(&mq) {
          let c = dot(mqi, &w);
          for t in 0..n {
            w[t] -= c * qi[t];
          }
        }
      }
      let mw = m.mul(&w);
      let nrm = dot(&w, &mw).sqrt();
      if nrm < 1e-300 {
        break;
      }
      q.push(w.iter().map(|x| x / nrm).collect());
      mq.push(mw.iter().map(|x| x / nrm).collect());
    } else {
      q.push(w.iter().map(|x| x / nrm).collect());
      mq.push(mw.iter().map(|x| x / nrm).collect());
    }
    let qj = &q[j];
    let mut v = f.solve(&mq[j]);
    let a = dot(&mq[j], &v);
    alpha.push(a);
    for t in 0..n {
      v[t] -= a * qj[t];
    }
    if j > 0 {
      let b = beta[j - 1];
      for t in 0..n {
        v[t] -= b * q[j - 1][t];
      }
    }
    w = v;
    if j + 1 == steps {
      for _ in 0..2 {
        for (qi, mqi) in q.iter().zip(&mq) {
          let c = dot(mqi, &w);
          for t in 0..n {
            w[t] -= c * qi[t];
          }
        }
      }
      beta_last = dot(&w, &m.mul(&w)).max(0.0).sqrt();
    }
  }
  let k = q.len();
  if k == 0 {
    return None;
  }
  let t = faer::Mat::<f64>::from_fn(k, k, |i, j| {
    if i == j {
      alpha[i]
    } else if i + 1 == j || j + 1 == i {
      beta[i.min(j)]
    } else {
      0.0
    }
  });
  let e = t.self_adjoint_eigen(Side::Lower).ok()?;
  let theta: Vec<f64> = (0..k).map(|i| e.S()[i]).collect();
  let s = e.U().to_owned();
  Some((theta, s, q, beta_last))
}
