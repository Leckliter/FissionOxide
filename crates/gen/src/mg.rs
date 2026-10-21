//! Geometric multigrid for the grid's stiffness: a V-cycle used as the conjugate gradient
//! preconditioner, which keeps the iteration count about flat as grids grow and stiffness
//! contrasts reach a million to one.
//!
//! Coarse levels halve the element count along each axis that has at least four elements
//! (semi-coarsening keeps thin plates thin). Their operators are Galerkin products
//! `P^T K P` with trilinear prolongation `P`, built element by element: every fine element sits
//! in one coarse element, and its nodes interpolate from that element's corners only, so each
//! coarse element gets its own 24x24 matrix. Fixed degrees of freedom are cut out of `P`. The
//! smoother is damped Jacobi, the same before and after the coarse correction (a symmetric
//! preconditioner), and the coarsest level is solved exactly by a banded Cholesky
//! factorisation.

use crate::Grid;
use crate::fe::{CORNERS, System};

/// Damped Jacobi weight and sweeps per side.
const OMEGA: f64 = 0.6;
const SWEEPS: usize = 2;
/// Coarsen until the grid has about this many nodes.
const COARSEST_NODES: usize = 700;

/// One coarse level.
struct Level {
  grid: Grid,
  /// Coarsening factor (1 or 2) per axis, from the level above.
  fac: [usize; 3],
  /// 24x24 matrix per element, row-major.
  mats: Vec<f64>,
  fixed: Vec<bool>,
  diag: Vec<f64>,
}

pub struct Multigrid<'a> {
  fine: &'a System<'a>,
  fine_diag: Vec<f64>,
  levels: Vec<Level>,
  /// Banded Cholesky factor of the coarsest level (or of the fine level when it's small).
  chol: Banded,
  threads: usize,
}

impl<'a> Multigrid<'a> {
  pub fn new(fine: &'a System<'a>) -> Multigrid<'a> {
    let threads = fine.threads.max(1);
    let fine_diag = fine.diagonal();
    let mut levels: Vec<Level> = Vec::new();
    loop {
      let g = levels.last().map(|l| l.grid).unwrap_or(*fine.grid);
      if g.nodes() <= COARSEST_NODES {
        break;
      }
      let fac = g.n.map(|n| if n >= 4 { 2 } else { 1 });
      if fac == [1, 1, 1] {
        break;
      }
      let coarse = Grid { origin: g.origin, h: g.h * 2.0, n: [0, 1, 2].map(|a| g.n[a].div_ceil(fac[a])) };
      let level = match levels.last() {
        None => galerkin(&g, coarse, fac, &|e| ElemMat::Scaled(fine.e[e]), fine.ke, fine.fixed, threads),
        Some(prev) => galerkin(&g, coarse, fac, &|e| ElemMat::Own(e), &prev.mats, &prev.fixed, threads),
      };
      levels.push(level);
    }
    let chol = match levels.last() {
      Some(l) => Banded::factor(&l.grid, &|e, r, c| l.mats[e * 576 + r * 24 + c], &l.fixed),
      None => Banded::factor(fine.grid, &|e, r, c| fine.e[e] * fine.ke[r * 24 + c], fine.fixed),
    };
    Multigrid { fine, fine_diag, levels, chol, threads }
  }

  /// z = M^-1 r (one V-cycle).
  pub fn precondition(&self, r: &[f64], z: &mut [f64]) {
    if self.levels.is_empty() {
      z.copy_from_slice(r);
      self.chol.solve(z);
      return;
    }
    self.cycle(0, r, z);
  }

  fn apply(&self, level: usize, x: &[f64], y: &mut [f64]) {
    if level == 0 {
      self.fine.apply(x, y);
    } else {
      let l = &self.levels[level - 1];
      apply_mats(&l.grid, &l.mats, &l.fixed, x, y, self.threads);
    }
  }

  fn cycle(&self, level: usize, b: &[f64], x: &mut [f64]) {
    if level == self.levels.len() {
      x.copy_from_slice(b);
      self.chol.solve(x);
      return;
    }
    let (diag, fixed, grid) = if level == 0 {
      (&self.fine_diag, self.fine.fixed, *self.fine.grid)
    } else {
      let l = &self.levels[level - 1];
      (&l.diag, &l.fixed[..], l.grid)
    };
    let n = b.len();
    x.fill(0.0);
    let mut ax = vec![0.0; n];
    let smooth = |x: &mut [f64], ax: &mut [f64]| {
      for _ in 0..SWEEPS {
        self.apply(level, x, ax);
        for i in 0..n {
          if !fixed[i] {
            x[i] += OMEGA * (b[i] - ax[i]) / diag[i];
          }
        }
      }
    };
    smooth(x, &mut ax);
    self.apply(level, x, &mut ax);
    let r: Vec<f64> = (0..n).map(|i| if fixed[i] { 0.0 } else { b[i] - ax[i] }).collect();
    let coarse = &self.levels[level];
    let mut rc = vec![0.0; 3 * coarse.grid.nodes()];
    restrict(&grid, &coarse.grid, coarse.fac, fixed, &r, &mut rc, self.threads);
    for (v, f) in rc.iter_mut().zip(&coarse.fixed) {
      if *f {
        *v = 0.0;
      }
    }
    let mut xc = vec![0.0; rc.len()];
    self.cycle(level + 1, &rc, &mut xc);
    prolong_add(&grid, &coarse.grid, coarse.fac, fixed, &xc, x, self.threads);
    smooth(x, &mut ax);
  }
}

/// A fine element's matrix: the shared one times a modulus, or the level's own.
enum ElemMat {
  Scaled(f64),
  Own(usize),
}

/// Trilinear weights from the 8 corners of a coarse element to the 8 nodes of the child at
/// `child` (its position inside the coarse element) for coarsening factors `fac`.
fn weights(child: [usize; 3], fac: [usize; 3]) -> [[f64; 8]; 8] {
  let mut w = [[0.0; 8]; 8];
  for (a, ca) in CORNERS.iter().enumerate() {
    let t = [0, 1, 2].map(|d| (child[d] + ca[d]) as f64 / fac[d] as f64);
    for (b, cb) in CORNERS.iter().enumerate() {
      w[a][b] = (0..3).map(|d| if cb[d] == 1 { t[d] } else { 1.0 - t[d] }).product();
    }
  }
  w
}

fn galerkin(
  fine: &Grid,
  coarse: Grid,
  fac: [usize; 3],
  mat: &(dyn Fn(usize) -> ElemMat + Sync),
  base: &[f64],
  fixed: &[bool],
  threads: usize,
) -> Level {
  let nc = coarse.elements();
  let mut mats = vec![0.0; nc * 576];
  // P^T KE P for each child position, for children without fixed dofs on the finest level
  let pre: Vec<Vec<f64>> = (0..8)
    .map(|c| {
      let mut out = vec![0.0; 576];
      if fixed.len() == 3 * fine.nodes() && base.len() >= 576 {
        let pos = [c % 2, (c / 2) % 2, c / 4];
        if (0..3).all(|d| pos[d] < fac[d]) {
          product(&base[..576], &weights(pos, fac), &[[true; 3]; 8], 1.0, &mut [0.0; 576], &mut out);
        }
      }
      out
    })
    .collect();
  let pre = &pre;
  let chunk = nc.div_ceil(threads).max(1);
  std::thread::scope(|sc| {
    for (ci, part) in mats.chunks_mut(chunk * 576).enumerate() {
      sc.spawn(move || {
        let mut t = [0.0; 576];
        for (l, out) in part.chunks_mut(576).enumerate() {
          let ce = ci * chunk + l;
          let [ic, jc, kc] = coarse.elem_ijk(ce);
          for ck in 0..fac[2] {
            for cj in 0..fac[1] {
              for cii in 0..fac[0] {
                let (i, j, k) = (ic * fac[0] + cii, jc * fac[1] + cj, kc * fac[2] + ck);
                if i >= fine.n[0] || j >= fine.n[1] || k >= fine.n[2] {
                  continue;
                }
                let e = fine.elem(i, j, k);
                let child = cii + 2 * (cj + 2 * ck);
                let nodes = fine.elem_nodes(e);
                let free: [[bool; 3]; 8] = nodes.map(|n| [0, 1, 2].map(|r| !fixed[3 * n + r]));
                let all_free = free.iter().all(|f| f.iter().all(|v| *v));
                let (scale, m) = match mat(e) {
                  ElemMat::Scaled(s) if all_free => {
                    // the precomputed product
                    for (o, q) in out.iter_mut().zip(&pre[child]) {
                      *o += s * q;
                    }
                    continue;
                  }
                  ElemMat::Scaled(s) => (s, &base[..576]),
                  ElemMat::Own(id) => (1.0, &base[id * 576..id * 576 + 576]),
                };
                product(m, &weights([cii, cj, ck], fac), &free, scale, &mut t, out);
              }
            }
          }
        }
      });
    }
  });
  let mut diag = vec![0.0; 3 * coarse.nodes()];
  for e in 0..nc {
    for (a, n) in coarse.elem_nodes(e).iter().enumerate() {
      for r in 0..3 {
        diag[3 * n + r] += mats[e * 576 + (3 * a + r) * 25];
      }
    }
  }
  let peak = diag.iter().fold(0.0f64, |m, d| m.max(*d));
  // dofs with nothing left (all their fine dofs fixed, or outside the fine grid) are fixed
  let fixed: Vec<bool> = diag.iter().map(|d| *d <= 1e-12 * peak).collect();
  for (d, f) in diag.iter_mut().zip(&fixed) {
    if *f {
      *d = 1.0;
    }
  }
  Level { grid: coarse, fac, mats, fixed, diag }
}

/// out += P^T (scale M) P for a child with corner weights `w` (fine node a from coarse corner b)
/// and free degrees of freedom `free` (P is w on each component, cut at fixed dofs).
fn product(m: &[f64], w: &[[f64; 8]; 8], free: &[[bool; 3]; 8], scale: f64, t: &mut [f64; 576], out: &mut [f64]) {
  // t = M P: t[row][3b + s] = sum_a M[row][3a + s] w[a][b] free[a][s]
  for row in 0..24 {
    let mr = &m[row * 24..row * 24 + 24];
    for b in 0..8 {
      for s in 0..3 {
        let mut sum = 0.0;
        for a in 0..8 {
          if free[a][s] {
            sum += mr[3 * a + s] * w[a][b];
          }
        }
        t[row * 24 + 3 * b + s] = sum * scale;
      }
    }
  }
  // out[3b + s][col] += sum_a w[a][b] free[a][s] t[3a + s][col]
  for b in 0..8 {
    for s in 0..3 {
      let o = &mut out[(3 * b + s) * 24..(3 * b + s) * 24 + 24];
      for a in 0..8 {
        if !free[a][s] || w[a][b] == 0.0 {
          continue;
        }
        let wa = w[a][b];
        let tr = &t[(3 * a + s) * 24..(3 * a + s) * 24 + 24];
        for (ov, tv) in o.iter_mut().zip(tr) {
          *ov += wa * tv;
        }
      }
    }
  }
}

/// y = K x for a level with a matrix per element (fixed rows are the identity).
fn apply_mats(g: &Grid, mats: &[f64], fixed: &[bool], x: &[f64], y: &mut [f64], threads: usize) {
  // element by element (each matrix read once), a buffer per thread, summed
  let ne = g.elements();
  let threads = if ne < 1024 { 1 } else { threads.max(1) };
  let chunk = ne.div_ceil(threads);
  let work = |first: usize, last: usize, out: &mut [f64]| {
    let mut xe = [0.0; 24];
    for e in first..last {
      let nodes = g.elem_nodes(e);
      for (a, n) in nodes.iter().enumerate() {
        xe[3 * a..3 * a + 3].copy_from_slice(&x[3 * n..3 * n + 3]);
      }
      let m = &mats[e * 576..e * 576 + 576];
      for (a, n) in nodes.iter().enumerate() {
        for r in 0..3 {
          let row = &m[(3 * a + r) * 24..(3 * a + r + 1) * 24];
          out[3 * n + r] += row.iter().zip(&xe).map(|(k, v)| k * v).sum::<f64>();
        }
      }
    }
  };
  y.fill(0.0);
  if threads == 1 {
    work(0, ne, y);
  } else {
    let parts: Vec<Vec<f64>> = std::thread::scope(|sc| {
      let hs: Vec<_> = (0..threads)
        .map(|t| {
          let work = &work;
          sc.spawn(move || {
            let mut out = vec![0.0; x.len()];
            work(t * chunk, ((t + 1) * chunk).min(ne), &mut out);
            out
          })
        })
        .collect();
      hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    for p in parts {
      for (a, b) in y.iter_mut().zip(p) {
        *a += b;
      }
    }
  }
  for (d, f) in fixed.iter().enumerate() {
    if *f {
      y[d] = x[d];
    }
  }
}

/// The coarse nodes (and weights) a fine node interpolates from, per axis.
fn parents(i: usize, fac: usize) -> [(usize, f64); 2] {
  if fac == 1 || i.is_multiple_of(2) { [(i / fac, 1.0), (0, 0.0)] } else { [(i / 2, 0.5), (i / 2 + 1, 0.5)] }
}

/// rc = P^T r, a coarse node at a time (gathering from the fine nodes around it), threaded.
fn restrict(fine: &Grid, coarse: &Grid, fac: [usize; 3], fixed: &[bool], r: &[f64], rc: &mut [f64], threads: usize) {
  // the fine nodes (and weights) a coarse node gathers from along one axis
  let nodes = |c: usize, a: usize| -> ([(usize, f64); 3], usize) {
    if fac[a] == 1 {
      return ([(c, 1.0), (0, 0.0), (0, 0.0)], 1);
    }
    let mut v = [(2 * c, 1.0), (0, 0.0), (0, 0.0)];
    let mut n = 1;
    if c > 0 {
      v[n] = (2 * c - 1, 0.5);
      n += 1;
    }
    if 2 * c < fine.n[a] {
      v[n] = (2 * c + 1, 0.5);
      n += 1;
    }
    if 2 * c > fine.n[a] {
      // beyond the fine grid: only its odd neighbour below is there
      v[0] = v[1];
      n -= 1;
    }
    (v, n)
  };
  let work = |first: usize, out: &mut [f64]| {
    for (l, o) in out.chunks_mut(3).enumerate() {
      let [ci, cj, ck] = coarse.node_ijk(first + l);
      let mut acc = [0.0; 3];
      let ((ni, ci_n), (nj, cj_n), (nk, ck_n)) = (nodes(ci, 0), nodes(cj, 1), nodes(ck, 2));
      for &(i, wi) in &ni[..ci_n] {
        for &(j, wj) in &nj[..cj_n] {
          for &(k, wk) in &nk[..ck_n] {
            let n = fine.node(i, j, k);
            let w = wi * wj * wk;
            for d in 0..3 {
              if !fixed[3 * n + d] {
                acc[d] += w * r[3 * n + d];
              }
            }
          }
        }
      }
      o.copy_from_slice(&acc);
    }
  };
  split(rc, coarse.nodes(), threads, &work);
}

/// x += P xc, a fine node at a time, threaded.
fn prolong_add(fine: &Grid, coarse: &Grid, fac: [usize; 3], fixed: &[bool], xc: &[f64], x: &mut [f64], threads: usize) {
  let work = |first: usize, out: &mut [f64]| {
    for (l, o) in out.chunks_mut(3).enumerate() {
      let n = first + l;
      let [i, j, k] = fine.node_ijk(n);
      let (pi, pj, pk) = (parents(i, fac[0]), parents(j, fac[1]), parents(k, fac[2]));
      for (ci, wi) in pi {
        for (cj, wj) in pj {
          for (ck, wk) in pk {
            let w = wi * wj * wk;
            if w == 0.0 {
              continue;
            }
            let c = coarse.node(ci, cj, ck);
            for d in 0..3 {
              if !fixed[3 * n + d] {
                o[d] += w * xc[3 * c + d];
              }
            }
          }
        }
      }
    }
  };
  split(x, fine.nodes(), threads, &work);
}

/// Runs `work(first node, its rows)` over chunks of a per-node vector, on threads when large.
fn split(v: &mut [f64], nodes: usize, threads: usize, work: &(dyn Fn(usize, &mut [f64]) + Sync)) {
  let threads = if nodes < 2048 { 1 } else { threads.max(1) };
  if threads == 1 {
    work(0, v);
    return;
  }
  let chunk = nodes.div_ceil(threads);
  std::thread::scope(|sc| {
    for (c, out) in v.chunks_mut(chunk * 3).enumerate() {
      sc.spawn(move || work(c * chunk, out));
    }
  });
}

/// A banded symmetric positive definite matrix's Cholesky factor (lower band, row-major by
/// distance below the diagonal), with the nodes numbered along the grid's shortest axes first
/// to keep the band narrow.
struct Banded {
  n: usize,
  bw: usize,
  l: Vec<f64>,
  /// Banded index of each grid dof.
  perm: Vec<usize>,
}

impl Banded {
  fn factor(g: &Grid, entry: &dyn Fn(usize, usize, usize) -> f64, fixed: &[bool]) -> Banded {
    let n = 3 * g.nodes();
    let counts = g.n.map(|c| c + 1);
    let mut axes = [0, 1, 2];
    axes.sort_by_key(|a| counts[*a]);
    let perm: Vec<usize> = (0..n)
      .map(|d| {
        let ijk = g.node_ijk(d / 3);
        let node = ijk[axes[0]] + counts[axes[0]] * (ijk[axes[1]] + counts[axes[1]] * ijk[axes[2]]);
        3 * node + d % 3
      })
      .collect();
    let bw = 3 * (1 + counts[axes[0]] + counts[axes[0]] * counts[axes[1]]) + 2;
    // a[i][j] for j in i-bw..=i, stored at i*(bw+1) + (i-j)
    let w = bw + 1;
    let mut a = vec![0.0; n * w];
    let mut fixed_p = vec![false; n];
    for (d, f) in fixed.iter().enumerate() {
      fixed_p[perm[d]] = *f;
    }
    for e in 0..g.elements() {
      let nodes = g.elem_nodes(e);
      for (ra, na) in nodes.iter().enumerate() {
        for rr in 0..3 {
          let i = perm[3 * na + rr];
          if fixed_p[i] {
            continue;
          }
          for (cb, nb) in nodes.iter().enumerate() {
            for cc in 0..3 {
              let j = perm[3 * nb + cc];
              if j > i || fixed_p[j] {
                continue;
              }
              a[i * w + (i - j)] += entry(e, 3 * ra + rr, 3 * cb + cc);
            }
          }
        }
      }
    }
    let peak = (0..n).map(|i| a[i * w]).fold(0.0f64, f64::max).max(1e-300);
    for i in 0..n {
      if fixed_p[i] || a[i * w] <= 1e-12 * peak {
        for k in 0..w {
          a[i * w + k] = 0.0;
        }
        a[i * w] = 1.0;
        for r in i + 1..(i + w).min(n) {
          a[r * w + (r - i)] = 0.0;
        }
      }
    }
    for i in 0..n {
      let j0 = i.saturating_sub(bw);
      for j in j0..=i {
        let k0 = j0.max(j.saturating_sub(bw));
        let (ri, rj) = (&a[i * w..i * w + w], &a[j * w..j * w + w]);
        let mut s = ri[i - j];
        for k in k0..j {
          s -= ri[i - k] * rj[j - k];
        }
        if i == j {
          a[i * w] = s.max(1e-300 * peak).sqrt();
        } else {
          a[i * w + (i - j)] = s / a[j * w];
        }
      }
    }
    Banded { n, bw, l: a, perm }
  }

  fn solve(&self, x: &mut [f64]) {
    let (n, w) = (self.n, self.bw + 1);
    let mut y = vec![0.0; n];
    for (d, p) in self.perm.iter().enumerate() {
      y[*p] = x[d];
    }
    for i in 0..n {
      let mut s = y[i];
      for k in i.saturating_sub(self.bw)..i {
        s -= self.l[i * w + (i - k)] * y[k];
      }
      y[i] = s / self.l[i * w];
    }
    for i in (0..n).rev() {
      let mut s = y[i];
      for k in i + 1..(i + w).min(n) {
        s -= self.l[k * w + (k - i)] * y[k];
      }
      y[i] = s / self.l[i * w];
    }
    for (d, p) in self.perm.iter().enumerate() {
      x[d] = y[*p];
    }
  }
}
