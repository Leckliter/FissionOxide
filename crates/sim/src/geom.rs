//! Small 3D helpers: vector arithmetic on `[f64; 3]`, a k-d tree for nearest points and an
//! inside/outside test for closed triangle meshes.

pub type V3 = [f64; 3];

#[inline]
pub fn add(a: V3, b: V3) -> V3 {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
pub fn scale(a: V3, s: f64) -> V3 {
  [a[0] * s, a[1] * s, a[2] * s]
}
#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
#[inline]
pub fn norm(a: V3) -> f64 {
  dot(a, a).sqrt()
}
#[inline]
pub fn dist(a: V3, b: V3) -> f64 {
  norm(sub(a, b))
}
#[inline]
pub fn mid(a: V3, b: V3) -> V3 {
  scale(add(a, b), 0.5)
}
/// `a` scaled to unit length (zero stays zero).
pub fn unit(a: V3) -> V3 {
  let n = norm(a);
  if n > 0.0 { scale(a, 1.0 / n) } else { a }
}

/// Twice the area vector of a triangle (its normal by the right-hand rule).
pub fn tri_normal(a: V3, b: V3, c: V3) -> V3 {
  cross(sub(b, a), sub(c, a))
}

pub fn tri_area(a: V3, b: V3, c: V3) -> f64 {
  0.5 * norm(tri_normal(a, b, c))
}

/// Signed volume of a tetrahedron (positive when `d` is on the side `a, b, c` turn
/// counter-clockwise around).
pub fn tet_volume(a: V3, b: V3, c: V3, d: V3) -> f64 {
  dot(cross(sub(b, a), sub(c, a)), sub(d, a)) / 6.0
}

/// A tetrahedron's shape quality: 1 for a regular one, 0 for a flat one (volume over the cube of
/// the root-mean-square edge length, normalised).
pub fn tet_quality(a: V3, b: V3, c: V3, d: V3) -> f64 {
  let v = tet_volume(a, b, c, d);
  let l2 = [(a, b), (a, c), (a, d), (b, c), (b, d), (c, d)].iter().map(|(p, q)| dot(sub(*p, *q), sub(*p, *q))).sum::<f64>()
    / 6.0;
  if l2 <= 0.0 { 0.0 } else { 6.0 * std::f64::consts::SQRT_2 * v / (l2 * l2.sqrt()) }
}

/// Closest point on triangle `abc` to `p`, with its barycentric coordinates.
pub fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> (V3, [f64; 3]) {
  // Ericson, Real-Time Collision Detection 5.1.5
  let ab = sub(b, a);
  let ac = sub(c, a);
  let ap = sub(p, a);
  let d1 = dot(ab, ap);
  let d2 = dot(ac, ap);
  if d1 <= 0.0 && d2 <= 0.0 {
    return (a, [1.0, 0.0, 0.0]);
  }
  let bp = sub(p, b);
  let d3 = dot(ab, bp);
  let d4 = dot(ac, bp);
  if d3 >= 0.0 && d4 <= d3 {
    return (b, [0.0, 1.0, 0.0]);
  }
  let vc = d1 * d4 - d3 * d2;
  if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
    let v = d1 / (d1 - d3);
    return (add(a, scale(ab, v)), [1.0 - v, v, 0.0]);
  }
  let cp = sub(p, c);
  let d5 = dot(ab, cp);
  let d6 = dot(ac, cp);
  if d6 >= 0.0 && d5 <= d6 {
    return (c, [0.0, 0.0, 1.0]);
  }
  let vb = d5 * d2 - d1 * d6;
  if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
    let w = d2 / (d2 - d6);
    return (add(a, scale(ac, w)), [1.0 - w, 0.0, w]);
  }
  let va = d3 * d6 - d5 * d4;
  if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
    let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
    return (add(b, scale(sub(c, b), w)), [0.0, 1.0 - w, w]);
  }
  let denom = 1.0 / (va + vb + vc);
  let v = vb * denom;
  let w = vc * denom;
  (add(a, add(scale(ab, v), scale(ac, w))), [1.0 - v - w, v, w])
}

/// Axis-aligned bounding box of points.
pub fn bbox(points: &[V3]) -> (V3, V3) {
  let mut lo = [f64::INFINITY; 3];
  let mut hi = [f64::NEG_INFINITY; 3];
  for p in points {
    for k in 0..3 {
      lo[k] = lo[k].min(p[k]);
      hi[k] = hi[k].max(p[k]);
    }
  }
  (lo, hi)
}

/// A k-d tree over points, for nearest-point and radius queries.
#[derive(Debug, Clone, Default)]
pub struct KdTree {
  points: Vec<V3>,
  /// Point indices in tree order: each subtree is a contiguous range, its median at the middle.
  order: Vec<u32>,
}

impl KdTree {
  pub fn new(points: &[V3]) -> KdTree {
    let mut order: Vec<u32> = (0..points.len() as u32).collect();
    fn build(points: &[V3], idx: &mut [u32], depth: usize) {
      if idx.len() <= 1 {
        return;
      }
      let axis = depth % 3;
      let m = idx.len() / 2;
      idx.select_nth_unstable_by(m, |a, b| points[*a as usize][axis].total_cmp(&points[*b as usize][axis]));
      let (l, r) = idx.split_at_mut(m);
      build(points, l, depth + 1);
      build(points, &mut r[1..], depth + 1);
    }
    build(points, &mut order, 0);
    KdTree { points: points.to_vec(), order }
  }

  pub fn is_empty(&self) -> bool {
    self.order.is_empty()
  }

  /// The nearest point's index and distance.
  pub fn nearest(&self, p: V3) -> Option<(usize, f64)> {
    let mut best = (usize::MAX, f64::INFINITY);
    self.nearest_in(&self.order, 0, p, &mut best);
    (best.0 != usize::MAX).then(|| (best.0, best.1.sqrt()))
  }

  fn nearest_in(&self, idx: &[u32], depth: usize, p: V3, best: &mut (usize, f64)) {
    if idx.is_empty() {
      return;
    }
    let m = idx.len() / 2;
    let i = idx[m] as usize;
    let q = self.points[i];
    let d2 = dot(sub(p, q), sub(p, q));
    if d2 < best.1 {
      *best = (i, d2);
    }
    let axis = depth % 3;
    let diff = p[axis] - q[axis];
    let (near, far) = if diff < 0.0 { (&idx[..m], &idx[m + 1..]) } else { (&idx[m + 1..], &idx[..m]) };
    self.nearest_in(near, depth + 1, p, best);
    if diff * diff < best.1 {
      self.nearest_in(far, depth + 1, p, best);
    }
  }

  /// Indices of the points within `r` of `p`.
  pub fn within(&self, p: V3, r: f64) -> Vec<usize> {
    let mut out = Vec::new();
    self.within_in(&self.order, 0, p, r * r, &mut out);
    out
  }

  fn within_in(&self, idx: &[u32], depth: usize, p: V3, r2: f64, out: &mut Vec<usize>) {
    if idx.is_empty() {
      return;
    }
    let m = idx.len() / 2;
    let i = idx[m] as usize;
    let q = self.points[i];
    if dot(sub(p, q), sub(p, q)) <= r2 {
      out.push(i);
    }
    let axis = depth % 3;
    let diff = p[axis] - q[axis];
    if diff <= 0.0 || diff * diff <= r2 {
      self.within_in(&idx[..m], depth + 1, p, r2, out);
    }
    if diff >= 0.0 || diff * diff <= r2 {
      self.within_in(&idx[m + 1..], depth + 1, p, r2, out);
    }
  }
}

/// Inside/outside tests against a closed triangle mesh: a ray along +X from the point counts the
/// triangles it crosses, found through a grid over the YZ plane.
pub struct InsideTest<'a> {
  points: &'a [V3],
  tris: &'a [[u32; 3]],
  lo: [f64; 2],
  cell: f64,
  n: [usize; 2],
  cells: Vec<Vec<u32>>,
  /// Offset added to every query so rays don't pass exactly through mesh edges and vertices.
  jitter: [f64; 2],
}

impl<'a> InsideTest<'a> {
  pub fn new(points: &'a [V3], tris: &'a [[u32; 3]]) -> InsideTest<'a> {
    let (lo, hi) = bbox(points);
    let size = (hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-9);
    let n_side = ((tris.len() as f64).sqrt().ceil() as usize).clamp(1, 512);
    let cell = size / n_side as f64 * 1.0001;
    let n = [(((hi[1] - lo[1]) / cell).floor() as usize + 1), (((hi[2] - lo[2]) / cell).floor() as usize + 1)];
    let mut cells = vec![Vec::new(); n[0] * n[1]];
    for (t, tri) in tris.iter().enumerate() {
      let ps = tri.map(|i| points[i as usize]);
      let (mut a0, mut a1, mut b0, mut b1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
      for p in ps {
        a0 = a0.min(p[1]);
        a1 = a1.max(p[1]);
        b0 = b0.min(p[2]);
        b1 = b1.max(p[2]);
      }
      let i0 = (((a0 - lo[1]) / cell).floor().max(0.0) as usize).min(n[0] - 1);
      let i1 = (((a1 - lo[1]) / cell).floor().max(0.0) as usize).min(n[0] - 1);
      let j0 = (((b0 - lo[2]) / cell).floor().max(0.0) as usize).min(n[1] - 1);
      let j1 = (((b1 - lo[2]) / cell).floor().max(0.0) as usize).min(n[1] - 1);
      for i in i0..=i1 {
        for j in j0..=j1 {
          cells[i * n[1] + j].push(t as u32);
        }
      }
    }
    InsideTest { points, tris, lo: [lo[1], lo[2]], cell, n, cells, jitter: [size * 3.1e-9, size * 1.7e-9] }
  }

  /// The X coordinates where the ray line through (y, z) crosses the mesh, unsorted.
  pub fn crossings(&self, y: f64, z: f64) -> Vec<f64> {
    let (y, z) = (y + self.jitter[0], z + self.jitter[1]);
    let i = ((y - self.lo[0]) / self.cell).floor();
    let j = ((z - self.lo[1]) / self.cell).floor();
    let mut out = Vec::new();
    if i < 0.0 || j < 0.0 || i as usize >= self.n[0] || j as usize >= self.n[1] {
      return out;
    }
    let q = robust::Coord { x: y, y: z };
    for &t in &self.cells[i as usize * self.n[1] + j as usize] {
      let [a, b, c] = self.tris[t as usize].map(|k| self.points[k as usize]);
      let pa = robust::Coord { x: a[1], y: a[2] };
      let pb = robust::Coord { x: b[1], y: b[2] };
      let pc = robust::Coord { x: c[1], y: c[2] };
      let w0 = robust::orient2d(pb, pc, q);
      let w1 = robust::orient2d(pc, pa, q);
      let w2 = robust::orient2d(pa, pb, q);
      let inside = (w0 > 0.0 && w1 > 0.0 && w2 > 0.0) || (w0 < 0.0 && w1 < 0.0 && w2 < 0.0);
      if !inside {
        continue;
      }
      let s = w0 + w1 + w2;
      out.push((w0 * a[0] + w1 * b[0] + w2 * c[0]) / s);
    }
    out
  }

  /// Whether `p` is inside the mesh.
  pub fn inside(&self, p: V3) -> bool {
    self.crossings(p[1], p[2]).iter().filter(|x| **x > p[0]).count() % 2 == 1
  }
}

/// Eigenvalues (descending) and unit eigenvectors (columns `v[..][k]`) of a symmetric 3 × 3 matrix,
/// by Jacobi rotations.
pub fn sym_eigen3(m: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
  let mut a = m;
  let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
  for _ in 0..50 {
    let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
    if off < 1e-15 * (a[0][0].abs() + a[1][1].abs() + a[2][2].abs() + 1e-300) {
      break;
    }
    for (p, q) in [(0, 1), (0, 2), (1, 2)] {
      if a[p][q].abs() < 1e-300 {
        continue;
      }
      let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
      let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
      let t = if theta == 0.0 { 1.0 } else { t };
      let c = 1.0 / (t * t + 1.0).sqrt();
      let s = t * c;
      for k in 0..3 {
        let (akp, akq) = (a[k][p], a[k][q]);
        a[k][p] = c * akp - s * akq;
        a[k][q] = s * akp + c * akq;
      }
      for k in 0..3 {
        let (apk, aqk) = (a[p][k], a[q][k]);
        a[p][k] = c * apk - s * aqk;
        a[q][k] = s * apk + c * aqk;
      }
      for row in &mut v {
        let (vp, vq) = (row[p], row[q]);
        row[p] = c * vp - s * vq;
        row[q] = s * vp + c * vq;
      }
    }
  }
  let mut order = [0, 1, 2];
  order.sort_by(|x, y| a[*y][*y].total_cmp(&a[*x][*x]));
  let vals = order.map(|k| a[k][k]);
  let mut vecs = [[0.0; 3]; 3];
  for (col, &k) in order.iter().enumerate() {
    for r in 0..3 {
      vecs[r][col] = v[r][k];
    }
  }
  (vals, vecs)
}

/// Column `k` of a 3 × 3 matrix.
pub fn column(m: &[[f64; 3]; 3], k: usize) -> V3 {
  [m[0][k], m[1][k], m[2][k]]
}

/// Hash maps and sets with a fixed hasher: iteration order (and so every mesh) is the same
/// from run to run.
pub type Map<K, V> = std::collections::HashMap<K, V, std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>>;
pub type Set<K> = std::collections::HashSet<K, std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>>;
