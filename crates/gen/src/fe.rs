//! Linear elastic finite elements on the voxel grid: trilinear 8-node bricks (all the same size,
//! so one element stiffness matrix scaled by each element's Young's modulus), a matrix-free
//! stiffness product split across threads, and Jacobi-preconditioned conjugate gradients.

use crate::grid::Grid;

/// Element corner offsets, counter-clockwise on the bottom then the top.
pub const CORNERS: [[usize; 3]; 8] =
  [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]];

/// The 24x24 stiffness matrix of a cubic element of side `h` with Young's modulus 1 and
/// Poisson's ratio `nu`, by 2x2x2 Gauss quadrature.
pub fn element_stiffness(h: f64, nu: f64) -> Vec<f64> {
  let d = elasticity(1.0, nu);
  let g = 1.0 / 3f64.sqrt();
  let mut k = vec![0.0; 24 * 24];
  for gz in [-g, g] {
    for gy in [-g, g] {
      for gx in [-g, g] {
        let b = strain_matrix([gx, gy, gz], h);
        let w = (h / 2.0).powi(3);
        // k += B^T D B w
        let mut db = [[0.0; 24]; 6];
        for r in 0..6 {
          for c in 0..24 {
            db[r][c] = (0..6).map(|s| d[r][s] * b[s][c]).sum();
          }
        }
        for r in 0..24 {
          for c in 0..24 {
            k[r * 24 + c] += w * (0..6).map(|s| b[s][r] * db[s][c]).sum::<f64>();
          }
        }
      }
    }
  }
  k
}

/// Isotropic elasticity matrix (Voigt order xx, yy, zz, xy, yz, zx; engineering shear).
pub fn elasticity(e: f64, nu: f64) -> [[f64; 6]; 6] {
  let c = e / ((1.0 + nu) * (1.0 - 2.0 * nu));
  let mut d = [[0.0; 6]; 6];
  for r in 0..3 {
    for s in 0..3 {
      d[r][s] = c * if r == s { 1.0 - nu } else { nu };
    }
    d[r + 3][r + 3] = c * (1.0 - 2.0 * nu) / 2.0;
  }
  d
}

/// Strain-displacement matrix at natural coordinates `xi` of a cube of side `h`.
pub fn strain_matrix(xi: [f64; 3], h: f64) -> [[f64; 24]; 6] {
  let mut b = [[0.0; 24]; 6];
  for (a, c) in CORNERS.iter().enumerate() {
    let s = c.map(|v| if v == 0 { -1.0 } else { 1.0 });
    let f = [0, 1, 2].map(|i| 1.0 + s[i] * xi[i]);
    // dN/dx = dN/dxi * 2/h
    let dn = [s[0] * f[1] * f[2], f[0] * s[1] * f[2], f[0] * f[1] * s[2]].map(|v| v / 8.0 * 2.0 / h);
    let (x, y, z) = (3 * a, 3 * a + 1, 3 * a + 2);
    b[0][x] = dn[0];
    b[1][y] = dn[1];
    b[2][z] = dn[2];
    b[3][x] = dn[1];
    b[3][y] = dn[0];
    b[4][y] = dn[2];
    b[4][z] = dn[1];
    b[5][z] = dn[0];
    b[5][x] = dn[2];
  }
  b
}

/// The assembled (implicit) stiffness of the grid for the elements' Young's moduli `e`, with
/// `fixed` degrees of freedom held at zero.
pub struct System<'a> {
  pub grid: &'a Grid,
  pub ke: &'a [f64],
  pub e: &'a [f64],
  pub fixed: &'a [bool],
  pub threads: usize,
}

impl System<'_> {
  /// y = K x (rows of fixed degrees of freedom are the identity; x is zero on them).
  pub fn apply(&self, x: &[f64], y: &mut [f64]) {
    gather(self.grid, x, y, self.threads, &|e| (self.ke, self.e[e]));
    for (d, f) in self.fixed.iter().enumerate() {
      if *f {
        y[d] = x[d];
      }
    }
  }

  /// The diagonal of K.
  pub fn diagonal(&self) -> Vec<f64> {
    let g = self.grid;
    let mut d = vec![0.0; 3 * g.nodes()];
    for e in 0..g.elements() {
      for (a, n) in g.elem_nodes(e).iter().enumerate() {
        for r in 0..3 {
          d[3 * n + r] += self.e[e] * self.ke[(3 * a + r) * 25];
        }
      }
    }
    for (i, f) in self.fixed.iter().enumerate() {
      if *f {
        d[i] = 1.0;
      }
    }
    d
  }

  /// Solves K u = f by conjugate gradients preconditioned with a multigrid V-cycle, from the
  /// guess in `u`. Returns the iterations used.
  pub fn solve(&self, f: &[f64], u: &mut [f64], tol: f64, max_iter: usize) -> usize {
    let mg = crate::mg::Multigrid::new(self);
    self.pcg(f, u, tol, max_iter, &|r: &[f64], z: &mut [f64]| mg.precondition(r, z))
  }

  /// The same with the Jacobi (diagonal) preconditioner.
  pub fn solve_jacobi(&self, f: &[f64], u: &mut [f64], tol: f64, max_iter: usize) -> usize {
    let diag = self.diagonal();
    self.pcg(f, u, tol, max_iter, &|r: &[f64], z: &mut [f64]| {
      for i in 0..r.len() {
        z[i] = r[i] / diag[i];
      }
    })
  }

  fn pcg(&self, f: &[f64], u: &mut [f64], tol: f64, max_iter: usize, m: &dyn Fn(&[f64], &mut [f64])) -> usize {
    let n = f.len();
    for (i, fx) in self.fixed.iter().enumerate() {
      if *fx {
        u[i] = 0.0;
      }
    }
    let mut r = vec![0.0; n];
    self.apply(u, &mut r);
    for i in 0..n {
      r[i] = if self.fixed[i] { 0.0 } else { f[i] - r[i] };
    }
    let fnorm = f.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-300);
    let mut z = vec![0.0; n];
    m(&r, &mut z);
    let mut p = z.clone();
    let mut rz: f64 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
    let mut q = vec![0.0; n];
    for it in 0..max_iter {
      let rn = r.iter().map(|v| v * v).sum::<f64>().sqrt();
      if rn <= tol * fnorm {
        return it;
      }
      self.apply(&p, &mut q);
      let pq: f64 = p.iter().zip(&q).map(|(a, b)| a * b).sum();
      if pq.abs() < 1e-300 {
        return it;
      }
      let alpha = rz / pq;
      for i in 0..n {
        u[i] += alpha * p[i];
        r[i] -= alpha * q[i];
      }
      m(&r, &mut z);
      let rz2: f64 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
      let beta = rz2 / rz;
      rz = rz2;
      for i in 0..n {
        p[i] = z[i] + beta * p[i];
      }
    }
    max_iter
  }
}

/// y = sum over elements of (scale M) x, with `elem` giving each element's 24x24 matrix and
/// scale; a node at a time (each thread writes its own rows), threaded on larger grids.
pub(crate) fn gather<'m>(
  g: &Grid,
  x: &[f64],
  y: &mut [f64],
  threads: usize,
  elem: &(dyn Fn(usize) -> (&'m [f64], f64) + Sync),
) {
  let nn = g.nodes();
  let threads = if nn < 2048 { 1 } else { threads.max(1) };
  let chunk = nn.div_ceil(threads);
  let (sx, sy) = (g.n[0] + 1, (g.n[0] + 1) * (g.n[1] + 1));
  let off = CORNERS.map(|c| c[0] + sx * c[1] + sy * c[2]);
  let eoff = CORNERS.map(|c| c[0] + g.n[0] * c[1] + g.n[0] * g.n[1] * c[2]);
  let rows = |first: usize, out: &mut [f64]| {
    for (local, yn) in out.chunks_mut(3).enumerate() {
      let n = first + local;
      let [i, j, k] = g.node_ijk(n);
      let mut acc = [0.0; 3];
      for (a, corner) in CORNERS.iter().enumerate() {
        // the element that has this node as its corner `a`
        if i < corner[0] || j < corner[1] || k < corner[2] {
          continue;
        }
        if i - corner[0] >= g.n[0] || j - corner[1] >= g.n[1] || k - corner[2] >= g.n[2] {
          continue;
        }
        // (the index is linear in i, j, k)
        let e = g.elem(i, j, k) - eoff[a];
        let (m, w) = elem(e);
        if w == 0.0 {
          continue;
        }
        let base = n - off[a];
        for r in 0..3 {
          let row = &m[(3 * a + r) * 24..(3 * a + r + 1) * 24];
          let mut s = 0.0;
          for b in 0..8 {
            let nb = 3 * (base + off[b]);
            s += row[3 * b] * x[nb] + row[3 * b + 1] * x[nb + 1] + row[3 * b + 2] * x[nb + 2];
          }
          acc[r] += w * s;
        }
      }
      yn.copy_from_slice(&acc);
    }
  };
  if threads == 1 {
    rows(0, y);
  } else {
    let rows = &rows;
    std::thread::scope(|sc| {
      for (c, out) in y.chunks_mut(chunk * 3).enumerate() {
        sc.spawn(move || rows(c * chunk, out));
      }
    });
  }
}

/// u_e^T KE u_e for every element (strain energy with unit modulus, times two).
pub fn element_energies(grid: &Grid, ke: &[f64], u: &[f64], threads: usize) -> Vec<f64> {
  let ne = grid.elements();
  let mut out = vec![0.0; ne];
  let chunk = ne.div_ceil(threads.max(1)).max(64);
  std::thread::scope(|sc| {
    for (c, part) in out.chunks_mut(chunk).enumerate() {
      sc.spawn(move || {
        for (l, o) in part.iter_mut().enumerate() {
          let nodes = grid.elem_nodes(c * chunk + l);
          let mut ue = [0.0; 24];
          for (a, n) in nodes.iter().enumerate() {
            ue[3 * a..3 * a + 3].copy_from_slice(&u[3 * n..3 * n + 3]);
          }
          let mut s = 0.0;
          for r in 0..24 {
            let row = &ke[r * 24..r * 24 + 24];
            s += ue[r] * row.iter().zip(&ue).map(|(k, v)| k * v).sum::<f64>();
          }
          *o = s;
        }
      });
    }
  });
  out
}

/// Von Mises stress at the centre of each element with Young's modulus `e0` (MPa for mm and N).
pub fn von_mises(grid: &Grid, u: &[f64], e0: f64, nu: f64) -> Vec<f64> {
  let d = elasticity(e0, nu);
  let b = strain_matrix([0.0; 3], grid.h);
  (0..grid.elements())
    .map(|e| {
      let nodes = grid.elem_nodes(e);
      let mut eps = [0.0; 6];
      for (r, er) in eps.iter_mut().enumerate() {
        for (a, n) in nodes.iter().enumerate() {
          for c in 0..3 {
            *er += b[r][3 * a + c] * u[3 * n + c];
          }
        }
      }
      let s: Vec<f64> = (0..6).map(|r| (0..6).map(|c| d[r][c] * eps[c]).sum()).collect();
      (0.5 * ((s[0] - s[1]).powi(2) + (s[1] - s[2]).powi(2) + (s[2] - s[0]).powi(2))
        + 3.0 * (s[3] * s[3] + s[4] * s[4] + s[5] * s[5]))
        .sqrt()
    })
    .collect()
}
