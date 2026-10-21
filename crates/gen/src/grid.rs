//! The voxel grid the optimisation runs on: cubic elements of side `h`, nodes at their corners,
//! and the geometry queries that turn bodies and faces into elements and nodes.

use serde::{Deserialize, Serialize};

pub type V3 = [f64; 3];

pub(crate) fn sub(a: V3, b: V3) -> V3 {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn add(a: V3, b: V3) -> V3 {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn scale(a: V3, s: f64) -> V3 {
  [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn dot(a: V3, b: V3) -> f64 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(crate) fn norm(a: V3) -> f64 {
  dot(a, a).sqrt()
}

/// A box of `n[0] x n[1] x n[2]` cubic elements of side `h` from `origin` (its min corner).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Grid {
  pub origin: V3,
  pub h: f64,
  pub n: [usize; 3],
}

impl Grid {
  /// A grid covering `lo..hi` with about `elements` elements (at least 1 per axis), or elements of
  /// side `h` when given, plus `pad` empty elements on every side.
  pub fn around(lo: V3, hi: V3, h: Option<f64>, elements: usize, pad: usize) -> Grid {
    let size = sub(hi, lo).map(|d| d.max(1e-9));
    let h = h.filter(|h| *h > 0.0).unwrap_or_else(|| (size[0] * size[1] * size[2] / elements.max(1) as f64).cbrt());
    let n = size.map(|d| (d / h).ceil().max(1.0) as usize + 2 * pad);
    let origin = [0, 1, 2].map(|a| {
      let span = (n[a] - 2 * pad) as f64 * h;
      // centred, so the grid is symmetric about the box's middle
      lo[a] + 0.5 * (size[a] - span) - pad as f64 * h
    });
    Grid { origin, h, n }
  }

  pub fn elements(&self) -> usize {
    self.n[0] * self.n[1] * self.n[2]
  }

  pub fn nodes(&self) -> usize {
    (self.n[0] + 1) * (self.n[1] + 1) * (self.n[2] + 1)
  }

  pub fn elem(&self, i: usize, j: usize, k: usize) -> usize {
    i + self.n[0] * (j + self.n[1] * k)
  }

  pub fn elem_ijk(&self, e: usize) -> [usize; 3] {
    [e % self.n[0], (e / self.n[0]) % self.n[1], e / (self.n[0] * self.n[1])]
  }

  pub fn node(&self, i: usize, j: usize, k: usize) -> usize {
    i + (self.n[0] + 1) * (j + (self.n[1] + 1) * k)
  }

  pub fn node_ijk(&self, n: usize) -> [usize; 3] {
    let (a, b) = (self.n[0] + 1, self.n[1] + 1);
    [n % a, (n / a) % b, n / (a * b)]
  }

  pub fn node_pos(&self, n: usize) -> V3 {
    let ijk = self.node_ijk(n);
    [0, 1, 2].map(|a| self.origin[a] + ijk[a] as f64 * self.h)
  }

  pub fn elem_center(&self, e: usize) -> V3 {
    let ijk = self.elem_ijk(e);
    [0, 1, 2].map(|a| self.origin[a] + (ijk[a] as f64 + 0.5) * self.h)
  }

  /// The 8 nodes of an element, in the order of [`crate::fe::CORNERS`].
  pub fn elem_nodes(&self, e: usize) -> [usize; 8] {
    let [i, j, k] = self.elem_ijk(e);
    crate::fe::CORNERS.map(|c| self.node(i + c[0], j + c[1], k + c[2]))
  }

  /// The far corner of the grid.
  pub fn max(&self) -> V3 {
    [0, 1, 2].map(|a| self.origin[a] + self.n[a] as f64 * self.h)
  }

  /// Elements whose centres are inside a closed triangle mesh (parity of crossings of a ray along
  /// +Z through each column of centres).
  pub fn inside(&self, positions: &[[f32; 3]], indices: &[u32]) -> Vec<bool> {
    let mut out = vec![false; self.elements()];
    let h = self.h;
    // crossings of each (i, j) column, as z values
    let mut cols: Vec<Vec<f64>> = vec![Vec::new(); self.n[0] * self.n[1]];
    let p = |i: u32| positions.get(i as usize).map(|v| v.map(f64::from)).unwrap_or([0.0; 3]);
    // tiny offsets so rays don't run through vertices and edges
    let (dx, dy) = (h * 1.234e-4, h * 2.345e-4);
    for t in indices.chunks_exact(3) {
      let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
      let lo = [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1])];
      let hi = [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1])];
      let i0 = (((lo[0] - self.origin[0]) / h - 0.5).ceil().max(0.0)) as usize;
      let j0 = (((lo[1] - self.origin[1]) / h - 0.5).ceil().max(0.0)) as usize;
      let i1 = (((hi[0] - self.origin[0]) / h - 0.5).floor()).min(self.n[0] as f64 - 1.0);
      let j1 = (((hi[1] - self.origin[1]) / h - 0.5).floor()).min(self.n[1] as f64 - 1.0);
      if i1 < 0.0 || j1 < 0.0 {
        continue;
      }
      let (i1, j1) = (i1 as usize, j1 as usize);
      for j in j0..=j1 {
        for i in i0..=i1 {
          let x = self.origin[0] + (i as f64 + 0.5) * h + dx;
          let y = self.origin[1] + (j as f64 + 0.5) * h + dy;
          if let Some(z) = ray_z(a, b, c, x, y) {
            cols[i + self.n[0] * j].push(z);
          }
        }
      }
    }
    for j in 0..self.n[1] {
      for i in 0..self.n[0] {
        let col = &mut cols[i + self.n[0] * j];
        if col.is_empty() {
          continue;
        }
        col.sort_by(f64::total_cmp);
        for k in 0..self.n[2] {
          let z = self.origin[2] + (k as f64 + 0.5) * h;
          let below = col.partition_point(|c| *c < z);
          if below % 2 == 1 {
            out[self.elem(i, j, k)] = true;
          }
        }
      }
    }
    out
  }

  /// Nodes within `tol` of any of the triangles.
  pub fn nodes_near(&self, positions: &[[f32; 3]], indices: &[u32], tol: f64) -> Vec<usize> {
    let mut hit = vec![false; self.nodes()];
    let p = |i: u32| positions.get(i as usize).map(|v| v.map(f64::from)).unwrap_or([0.0; 3]);
    for t in indices.chunks_exact(3) {
      let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
      let lo = [0, 1, 2].map(|k| a[k].min(b[k]).min(c[k]) - tol);
      let hi = [0, 1, 2].map(|k| a[k].max(b[k]).max(c[k]) + tol);
      let r = |k: usize| {
        let l = ((lo[k] - self.origin[k]) / self.h).ceil().max(0.0) as usize;
        let u = ((hi[k] - self.origin[k]) / self.h).floor().min(self.n[k] as f64);
        (l, u)
      };
      let ((i0, i1), (j0, j1), (k0, k1)) = (r(0), r(1), r(2));
      if i1 < 0.0 || j1 < 0.0 || k1 < 0.0 {
        continue;
      }
      for k in k0..=k1 as usize {
        for j in j0..=j1 as usize {
          for i in i0..=i1 as usize {
            let n = self.node(i, j, k);
            if !hit[n] && dist_point_triangle(self.node_pos(n), a, b, c) <= tol {
              hit[n] = true;
            }
          }
        }
      }
    }
    hit.iter().enumerate().filter(|(_, h)| **h).map(|(i, _)| i).collect()
  }

  /// Nodes within `r` of a point.
  pub fn nodes_within(&self, at: V3, r: f64) -> Vec<usize> {
    (0..self.nodes()).filter(|n| norm(sub(self.node_pos(*n), at)) <= r).collect()
  }

  /// The node nearest a point.
  pub fn nearest_node(&self, at: V3) -> usize {
    let ijk = [0, 1, 2].map(|a| (((at[a] - self.origin[a]) / self.h).round().max(0.0) as usize).min(self.n[a]));
    self.node(ijk[0], ijk[1], ijk[2])
  }
}

/// Where the vertical line through (x, y) crosses triangle abc, if it does.
fn ray_z(a: V3, b: V3, c: V3, x: f64, y: f64) -> Option<f64> {
  let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
  if d.abs() < 1e-30 {
    return None;
  }
  let l1 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
  let l2 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
  let l3 = 1.0 - l1 - l2;
  (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * a[2] + l2 * b[2] + l3 * c[2])
}

/// Distance from p to triangle abc.
pub fn dist_point_triangle(p: V3, a: V3, b: V3, c: V3) -> f64 {
  let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
  let (d1, d2) = (dot(ab, ap), dot(ac, ap));
  let closest = if d1 <= 0.0 && d2 <= 0.0 {
    a
  } else {
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
      b
    } else {
      let vc = d1 * d4 - d3 * d2;
      if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        add(a, scale(ab, d1 / (d1 - d3)))
      } else {
        let cp = sub(p, c);
        let (d5, d6) = (dot(ab, cp), dot(ac, cp));
        if d6 >= 0.0 && d5 <= d6 {
          c
        } else {
          let vb = d5 * d2 - d1 * d6;
          if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            add(a, scale(ac, d2 / (d2 - d6)))
          } else {
            let va = d3 * d6 - d5 * d4;
            if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
              add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))))
            } else {
              let denom = 1.0 / (va + vb + vc);
              add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
            }
          }
        }
      }
    }
  };
  norm(sub(p, closest))
}
