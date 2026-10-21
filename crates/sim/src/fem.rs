//! Element matrices of straight-sided quadratic tetrahedra (10 nodes) and triangles (6 nodes).
//!
//! The shape functions are polynomials in barycentric coordinates `L`: corners
//! `N = L(2L − 1)`, mid-edge nodes `N = 4 Li Lj`. With straight sides the gradients of `L` are
//! constant over an element, so every integral needed (stiffness, mass, loads) is a polynomial in
//! `L` integrated exactly with `∫ L0^a L1^b L2^c L3^d dV = 6V a! b! c! d! / (a + b + c + d + 3)!`
//! — no quadrature. The `L`-polynomial parts are the same for every element and are tabulated
//! once ([`Tables`]); an element only adds its gradients and size.

use std::sync::OnceLock;

use crate::geom::*;
use crate::mesh::{TET_EDGES, TRI_EDGES};

/// A polynomial in four barycentric coordinates: (coefficient, exponents).
type Poly = Vec<(f64, [u8; 4])>;

fn mul(a: &Poly, b: &Poly) -> Poly {
  let mut out: Poly = Vec::new();
  for (ca, ea) in a {
    for (cb, eb) in b {
      let e = [ea[0] + eb[0], ea[1] + eb[1], ea[2] + eb[2], ea[3] + eb[3]];
      match out.iter_mut().find(|(_, x)| *x == e) {
        Some(t) => t.0 += ca * cb,
        None => out.push((ca * cb, e)),
      }
    }
  }
  out
}

fn deriv(a: &Poly, k: usize) -> Poly {
  a.iter()
    .filter(|(_, e)| e[k] > 0)
    .map(|(c, e)| {
      let mut e2 = *e;
      e2[k] -= 1;
      (c * e[k] as f64, e2)
    })
    .collect()
}

fn fact(n: u32) -> f64 {
  (1..=n).map(f64::from).product()
}

/// ∫ over a tetrahedron of unit volume.
fn integrate_tet(a: &Poly) -> f64 {
  a.iter()
    .map(|(c, e)| {
      let s: u32 = e.iter().map(|&x| u32::from(x)).sum();
      c * 6.0 * e.iter().map(|&x| fact(u32::from(x))).product::<f64>() / fact(s + 3)
    })
    .sum()
}

/// ∫ over a triangle of unit area (exponent 3 must be 0).
fn integrate_tri(a: &Poly) -> f64 {
  a.iter()
    .map(|(c, e)| {
      let s: u32 = e[..3].iter().map(|&x| u32::from(x)).sum();
      c * 2.0 * e[..3].iter().map(|&x| fact(u32::from(x))).product::<f64>() / fact(s + 2)
    })
    .sum()
}

fn lin(k: usize) -> Poly {
  let mut e = [0u8; 4];
  e[k] = 1;
  vec![(1.0, e)]
}

/// Shape functions of the 10-node tetrahedron (or, with `vertices = 3`, the 6-node triangle).
fn shapes(vertices: usize) -> Vec<Poly> {
  let mut out = Vec::new();
  for i in 0..vertices {
    // L(2L - 1) = 2L² - L
    let mut p = mul(&lin(i), &lin(i));
    p[0].0 = 2.0;
    p.push((-1.0, lin(i)[0].1));
    out.push(p);
  }
  let edges: Vec<[usize; 2]> = if vertices == 4 { TET_EDGES.to_vec() } else { TRI_EDGES.to_vec() };
  for [i, j] in edges {
    let mut p = mul(&lin(i), &lin(j));
    p[0].0 = 4.0;
    out.push(p);
  }
  out
}

/// The element-independent integrals.
pub struct Tables {
  /// `stiff[a][b][k][l]` = ∫ ∂Na/∂Lk ∂Nb/∂Ll dV over a unit-volume tetrahedron.
  pub stiff: [[[[f64; 4]; 4]; 10]; 10],
  /// ∫ Na Nb dV (unit volume).
  pub mass: [[f64; 10]; 10],
  /// ∫ Na dV (unit volume).
  pub load: [f64; 10],
  /// ∫ Na Nb dA over a unit-area triangle.
  pub tri_mass: [[f64; 6]; 6],
  /// ∫ Na dA (unit area).
  pub tri_load: [f64; 6],
  /// `grad_at[n][a][k]` = ∂Na/∂Lk at node n of the tetrahedron.
  pub grad_at: [[[f64; 4]; 10]; 10],
}

pub fn tables() -> &'static Tables {
  static T: OnceLock<Tables> = OnceLock::new();
  T.get_or_init(|| {
    let n = shapes(4);
    let dn: Vec<Vec<Poly>> = n.iter().map(|p| (0..4).map(|k| deriv(p, k)).collect()).collect();
    let mut t = Tables {
      stiff: [[[[0.0; 4]; 4]; 10]; 10],
      mass: [[0.0; 10]; 10],
      load: [0.0; 10],
      tri_mass: [[0.0; 6]; 6],
      tri_load: [0.0; 6],
      grad_at: [[[0.0; 4]; 10]; 10],
    };
    for a in 0..10 {
      t.load[a] = integrate_tet(&n[a]);
      for b in 0..10 {
        t.mass[a][b] = integrate_tet(&mul(&n[a], &n[b]));
        for k in 0..4 {
          for l in 0..4 {
            t.stiff[a][b][k][l] = integrate_tet(&mul(&dn[a][k], &dn[b][l]));
          }
        }
      }
    }
    let m = shapes(3);
    for a in 0..6 {
      t.tri_load[a] = integrate_tri(&m[a]);
      for b in 0..6 {
        t.tri_mass[a][b] = integrate_tri(&mul(&m[a], &m[b]));
      }
    }
    for (node, l) in node_coords().iter().enumerate() {
      for a in 0..10 {
        for k in 0..4 {
          t.grad_at[node][a][k] = eval(&dn[a][k], *l);
        }
      }
    }
    t
  })
}

fn eval(p: &Poly, l: [f64; 4]) -> f64 {
  p.iter().map(|(c, e)| c * (0..4).map(|k| l[k].powi(i32::from(e[k]))).product::<f64>()).sum()
}

/// Barycentric coordinates of the 10 nodes.
pub fn node_coords() -> [[f64; 4]; 10] {
  let mut out = [[0.0; 4]; 10];
  for (k, row) in out.iter_mut().enumerate().take(4) {
    row[k] = 1.0;
  }
  for (e, [i, j]) in TET_EDGES.iter().enumerate() {
    out[4 + e][*i] = 0.5;
    out[4 + e][*j] = 0.5;
  }
  out
}

/// The 10 shape function values at barycentric coordinates `l`.
pub fn shape_values(l: [f64; 4]) -> [f64; 10] {
  let mut n = [0.0; 10];
  for k in 0..4 {
    n[k] = l[k] * (2.0 * l[k] - 1.0);
  }
  for (e, [i, j]) in TET_EDGES.iter().enumerate() {
    n[4 + e] = 4.0 * l[*i] * l[*j];
  }
  n
}

/// The 6 triangle shape function values at barycentric coordinates `l`.
pub fn tri_shape_values(l: [f64; 3]) -> [f64; 6] {
  let mut n = [0.0; 6];
  for k in 0..3 {
    n[k] = l[k] * (2.0 * l[k] - 1.0);
  }
  for (e, [i, j]) in TRI_EDGES.iter().enumerate() {
    n[3 + e] = 4.0 * l[*i] * l[*j];
  }
  n
}

/// ∂Na/∂Lk at barycentric coordinates `l`.
pub fn shape_grads(l: [f64; 4]) -> [[f64; 4]; 10] {
  let mut g = [[0.0; 4]; 10];
  for k in 0..4 {
    g[k][k] = 4.0 * l[k] - 1.0;
  }
  for (e, [i, j]) in TET_EDGES.iter().enumerate() {
    g[4 + e][*i] = 4.0 * l[*j];
    g[4 + e][*j] = 4.0 * l[*i];
  }
  g
}

/// A tetrahedron's corner geometry: volume and the (constant) gradients of its barycentric
/// coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Geo {
  pub volume: f64,
  pub grad: [V3; 4],
}

impl Geo {
  pub fn new(x: [V3; 4]) -> Geo {
    let e1 = sub(x[1], x[0]);
    let e2 = sub(x[2], x[0]);
    let e3 = sub(x[3], x[0]);
    let det = dot(e1, cross(e2, e3));
    // rows of the inverse Jacobian are the gradients of L1, L2, L3
    let g1 = scale(cross(e2, e3), 1.0 / det);
    let g2 = scale(cross(e3, e1), 1.0 / det);
    let g3 = scale(cross(e1, e2), 1.0 / det);
    let g0 = scale(add(add(g1, g2), g3), -1.0);
    Geo { volume: det / 6.0, grad: [g0, g1, g2, g3] }
  }

  /// Barycentric coordinates of a point.
  pub fn bary(&self, x0: V3, p: V3) -> [f64; 4] {
    let d = sub(p, x0);
    let l1 = dot(self.grad[1], d);
    let l2 = dot(self.grad[2], d);
    let l3 = dot(self.grad[3], d);
    [1.0 - l1 - l2 - l3, l1, l2, l3]
  }

  /// Spatial gradients of the 10 shape functions at barycentric coordinates `l`.
  pub fn shape_gradients(&self, l: [f64; 4]) -> [V3; 10] {
    let dl = shape_grads(l);
    let mut out = [[0.0; 3]; 10];
    for a in 0..10 {
      for k in 0..4 {
        out[a] = add(out[a], scale(self.grad[k], dl[a][k]));
      }
    }
    out
  }

  /// `G[a][b][i][j]` = ∫ ∂iNa ∂jNb dV.
  pub fn gradient_products(&self) -> [[[[f64; 3]; 3]; 10]; 10] {
    let t = tables();
    let mut out = [[[[0.0; 3]; 3]; 10]; 10];
    for a in 0..10 {
      for b in 0..10 {
        let mut g = [[0.0; 3]; 3];
        for k in 0..4 {
          for l in 0..4 {
            let c = t.stiff[a][b][k][l] * self.volume;
            if c == 0.0 {
              continue;
            }
            for i in 0..3 {
              for j in 0..3 {
                g[i][j] += c * self.grad[k][i] * self.grad[l][j];
              }
            }
          }
        }
        out[a][b] = g;
      }
    }
    out
  }
}

/// The 30 × 30 stiffness matrix of an isotropic elastic tetrahedron (rows and columns ordered
/// node by node, x y z), with Lamé parameters λ and μ.
pub fn stiffness(geo: &Geo, lambda: f64, mu: f64) -> Box<[[f64; 30]; 30]> {
  let g = geo.gradient_products();
  let mut k = Box::new([[0.0; 30]; 30]);
  for a in 0..10 {
    for b in 0..10 {
      let gab = &g[a][b];
      let tr = gab[0][0] + gab[1][1] + gab[2][2];
      for i in 0..3 {
        for j in 0..3 {
          let mut v = lambda * gab[i][j] + mu * gab[j][i];
          if i == j {
            v += mu * tr;
          }
          k[3 * a + i][3 * b + j] = v;
        }
      }
    }
  }
  k
}

/// The 10 × 10 conduction matrix of a tetrahedron with conductivity `k`.
pub fn conduction(geo: &Geo, k: f64) -> [[f64; 10]; 10] {
  let g = geo.gradient_products();
  let mut out = [[0.0; 10]; 10];
  for a in 0..10 {
    for b in 0..10 {
      out[a][b] = k * (g[a][b][0][0] + g[a][b][1][1] + g[a][b][2][2]);
    }
  }
  out
}

/// The 10 × 10 scalar consistent mass matrix (multiply by density, per direction).
pub fn mass(geo: &Geo) -> [[f64; 10]; 10] {
  let t = tables();
  let mut out = [[0.0; 10]; 10];
  for a in 0..10 {
    for b in 0..10 {
      out[a][b] = t.mass[a][b] * geo.volume;
    }
  }
  out
}

/// ∫ Na dV.
pub fn volume_load(geo: &Geo) -> [f64; 10] {
  tables().load.map(|w| w * geo.volume)
}

/// Strain (xx, yy, zz, xy, yz, zx — engineering shears) at barycentric coordinates `l` from the 10
/// nodal displacements.
pub fn strain(geo: &Geo, l: [f64; 4], u: &[V3; 10]) -> [f64; 6] {
  let g = geo.shape_gradients(l);
  let mut du = [[0.0; 3]; 3];
  for a in 0..10 {
    for i in 0..3 {
      for j in 0..3 {
        du[i][j] += u[a][i] * g[a][j];
      }
    }
  }
  [du[0][0], du[1][1], du[2][2], du[0][1] + du[1][0], du[1][2] + du[2][1], du[2][0] + du[0][2]]
}

/// Stress (xx, yy, zz, xy, yz, zx) from strain.
pub fn stress(lambda: f64, mu: f64, e: [f64; 6]) -> [f64; 6] {
  let tr = e[0] + e[1] + e[2];
  [
    lambda * tr + 2.0 * mu * e[0],
    lambda * tr + 2.0 * mu * e[1],
    lambda * tr + 2.0 * mu * e[2],
    mu * e[3],
    mu * e[4],
    mu * e[5],
  ]
}

pub fn von_mises(s: [f64; 6]) -> f64 {
  (0.5 * ((s[0] - s[1]).powi(2) + (s[1] - s[2]).powi(2) + (s[2] - s[0]).powi(2))
    + 3.0 * (s[3] * s[3] + s[4] * s[4] + s[5] * s[5]))
    .sqrt()
}

/// Principal stresses, largest first.
pub fn principal(s: [f64; 6]) -> [f64; 3] {
  // eigenvalues of a symmetric 3×3 (Smith's trigonometric method)
  let (a, b, c, d, e, f) = (s[0], s[1], s[2], s[3], s[4], s[5]);
  let p1 = d * d + e * e + f * f;
  let q = (a + b + c) / 3.0;
  let p2 = (a - q).powi(2) + (b - q).powi(2) + (c - q).powi(2) + 2.0 * p1;
  let p = (p2 / 6.0).sqrt();
  if p < 1e-300 {
    return [a, b, c];
  }
  let m = [[(a - q) / p, d / p, f / p], [d / p, (b - q) / p, e / p], [f / p, e / p, (c - q) / p]];
  let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
    + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
  let r = (det / 2.0).clamp(-1.0, 1.0);
  let phi = r.acos() / 3.0;
  let e1 = q + 2.0 * p * phi.cos();
  let e3 = q + 2.0 * p * (phi + 2.0 * std::f64::consts::PI / 3.0).cos();
  [e1, 3.0 * q - e1 - e3, e3]
}

/// A 6-node boundary triangle's area and outward unit normal (from its corners).
pub fn tri_geo(x: [V3; 3]) -> (f64, V3) {
  let n = tri_normal(x[0], x[1], x[2]);
  let l = norm(n);
  (0.5 * l, if l > 0.0 { scale(n, 1.0 / l) } else { n })
}

/// ∫ Na dA over a triangle of area `area`.
pub fn tri_load(area: f64) -> [f64; 6] {
  tables().tri_load.map(|w| w * area)
}

/// ∫ Na Nb dA over a triangle of area `area`.
pub fn tri_mass(area: f64) -> [[f64; 6]; 6] {
  tables().tri_mass.map(|r| r.map(|w| w * area))
}
