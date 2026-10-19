//! 2D helpers for toolpaths: polygon regions with exact distances to their boundary, scalar
//! grids, a Euclidean distance transform and iso-contours (marching squares).

use fox_kernel::glam::{DVec2, DVec3};

/// A planar region in XY bounded by closed loops (an outer loop and holes; inside = even-odd).
#[derive(Debug, Clone)]
pub struct Region {
  segs: Vec<(DVec2, DVec2)>,
  pub lo: DVec2,
  pub hi: DVec2,
  /// Segments by bucket, for distance queries.
  bucket: f64,
  bx: usize,
  by: usize,
  buckets: Vec<Vec<u32>>,
}

impl Region {
  pub fn new(loops: &[Vec<DVec3>]) -> Option<Region> {
    let mut segs = Vec::new();
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for l in loops.iter().filter(|l| l.len() > 2) {
      for i in 0..l.len() {
        let (a, b) = (l[i].truncate(), l[(i + 1) % l.len()].truncate());
        lo = lo.min(a);
        hi = hi.max(a);
        if (a - b).length() > 1e-12 {
          segs.push((a, b));
        }
      }
    }
    if segs.len() < 3 {
      return None;
    }
    let size = hi - lo;
    let bucket = (size.x.max(size.y) / 64.0).max((size.x * size.y / segs.len() as f64).sqrt()).max(1e-3);
    let bx = ((size.x / bucket).ceil() as usize).max(1);
    let by = ((size.y / bucket).ceil() as usize).max(1);
    let mut buckets = vec![Vec::new(); bx * by];
    for (i, (a, b)) in segs.iter().enumerate() {
      let (i0, j0) = cell(lo, bucket, bx, by, a.min(*b));
      let (i1, j1) = cell(lo, bucket, bx, by, a.max(*b));
      for j in j0..=j1 {
        for k in i0..=i1 {
          buckets[j * bx + k].push(i as u32);
        }
      }
    }
    Some(Region { segs, lo, hi, bucket, bx, by, buckets })
  }

  /// Even-odd inside test.
  pub fn contains(&self, p: DVec2) -> bool {
    let mut inside = false;
    for (a, b) in &self.segs {
      if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
        inside = !inside;
      }
    }
    inside
  }

  /// Distance to the boundary, exact up to `max` (larger distances come back as `max`).
  pub fn distance(&self, p: DVec2, max: f64) -> f64 {
    let (ci, cj) = cell(self.lo, self.bucket, self.bx, self.by, p);
    // a point outside the bucket grid starts its search from the nearest bucket
    let outside = (p - p.clamp(self.lo, self.hi)).length();
    let mut best = f64::INFINITY;
    let limit = (((max + outside) / self.bucket).ceil() as usize + 1).min(self.bx.max(self.by));
    for k in 0..=limit {
      if best.is_finite() && (k as f64 - 1.0) * self.bucket > best - outside {
        break;
      }
      let (i0, i1) = (ci.saturating_sub(k), (ci + k).min(self.bx - 1));
      let (j0, j1) = (cj.saturating_sub(k), (cj + k).min(self.by - 1));
      for j in j0..=j1 {
        for i in i0..=i1 {
          // the ring only
          if k > 0 && i != i0 && i != i1 && j != j0 && j != j1 {
            continue;
          }
          for &s in &self.buckets[j * self.bx + i] {
            let (a, b) = self.segs[s as usize];
            best = best.min(seg_distance(p, a, b));
          }
        }
      }
    }
    best.min(max)
  }

  /// Signed distance to the boundary (positive inside), clamped to ±`max`.
  pub fn sdf(&self, p: DVec2, max: f64) -> f64 {
    let d = self.distance(p, max);
    if self.contains(p) { d } else { -d }
  }

  /// Signed distances at a grid's nodes (positive inside), clamped to ±`max`; inside tests by
  /// scanlines so this stays linear in the grid size.
  pub fn sdf_grid(&self, g: &mut Grid, max: f64) {
    for j in 0..g.ny {
      let y = g.y0 + j as f64 * g.h;
      let mut xs: Vec<f64> = self
        .segs
        .iter()
        .filter(|(a, b)| (a.y > y) != (b.y > y))
        .map(|(a, b)| a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x))
        .collect();
      xs.sort_by(f64::total_cmp);
      let mut k = 0;
      for i in 0..g.nx {
        let x = g.x0 + i as f64 * g.h;
        while k < xs.len() && xs[k] <= x {
          k += 1;
        }
        let inside = k % 2 == 1;
        let d = self.distance(DVec2::new(x, y), max);
        g.v[j * g.nx + i] = if inside { d } else { -d };
      }
    }
  }

  /// The inside point farthest from the boundary (among grid nodes `h` apart) and its
  /// distance, clamped to `cap`.
  pub fn deepest(&self, h: f64, cap: f64) -> Option<(DVec2, f64)> {
    let mut g = Grid::new(self.lo, self.hi, h, 1, 0.0);
    self.sdf_grid(&mut g, cap);
    // exact distances stop at the cap: rank by a distance transform from the outside instead
    let outside: Vec<bool> = g.v.iter().map(|v| *v <= 0.0).collect();
    let depth = g.distance_to(&outside);
    let (i, _) = depth.iter().enumerate().filter(|(i, _)| g.v[*i] > 0.0).max_by(|a, b| a.1.total_cmp(b.1))?;
    Some((g.node(i), g.v[i]))
  }
}

fn cell(lo: DVec2, size: f64, nx: usize, ny: usize, p: DVec2) -> (usize, usize) {
  let q = (p - lo) / size;
  ((q.x.max(0.0) as usize).min(nx - 1), (q.y.max(0.0) as usize).min(ny - 1))
}

pub fn seg_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
  let ab = b - a;
  let t = ((p - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
  (a + ab * t - p).length()
}

/// Values on the nodes of a regular XY grid: node (i, j) at (x0 + i h, y0 + j h).
#[derive(Debug, Clone)]
pub struct Grid {
  pub x0: f64,
  pub y0: f64,
  pub h: f64,
  pub nx: usize,
  pub ny: usize,
  pub v: Vec<f64>,
}

impl Grid {
  /// A grid covering `lo`..`hi` with `pad` nodes of margin.
  pub fn new(lo: DVec2, hi: DVec2, h: f64, pad: usize, fill: f64) -> Grid {
    let nx = ((hi.x - lo.x) / h).ceil() as usize + 1 + 2 * pad;
    let ny = ((hi.y - lo.y) / h).ceil() as usize + 1 + 2 * pad;
    Grid { x0: lo.x - pad as f64 * h, y0: lo.y - pad as f64 * h, h, nx, ny, v: vec![fill; nx * ny] }
  }

  pub fn node(&self, i: usize) -> DVec2 {
    DVec2::new(self.x0 + (i % self.nx) as f64 * self.h, self.y0 + (i / self.nx) as f64 * self.h)
  }

  /// The nearest node to a point (clamped to the grid).
  pub fn index(&self, p: DVec2) -> usize {
    let i = ((p.x - self.x0) / self.h).round().clamp(0.0, (self.nx - 1) as f64) as usize;
    let j = ((p.y - self.y0) / self.h).round().clamp(0.0, (self.ny - 1) as f64) as usize;
    j * self.nx + i
  }

  /// Bilinear interpolation.
  pub fn sample(&self, p: DVec2) -> f64 {
    let fx = ((p.x - self.x0) / self.h).clamp(0.0, (self.nx - 1) as f64);
    let fy = ((p.y - self.y0) / self.h).clamp(0.0, (self.ny - 1) as f64);
    let (i, j) = ((fx as usize).min(self.nx.saturating_sub(2)), (fy as usize).min(self.ny.saturating_sub(2)));
    let (tx, ty) = (fx - i as f64, fy - j as f64);
    let at = |i: usize, j: usize| self.v[(j.min(self.ny - 1)) * self.nx + i.min(self.nx - 1)];
    let a = at(i, j) * (1.0 - tx) + at(i + 1, j) * tx;
    let b = at(i, j + 1) * (1.0 - tx) + at(i + 1, j + 1) * tx;
    a * (1.0 - ty) + b * ty
  }

  /// Euclidean distance from every node to the nearest node of `set` (infinite if empty).
  pub fn distance_to(&self, set: &[bool]) -> Vec<f64> {
    let (nx, ny) = (self.nx, self.ny);
    let big = 1e30;
    let mut d: Vec<f64> = set.iter().map(|&s| if s { 0.0 } else { big }).collect();
    let mut f = vec![0.0; nx.max(ny)];
    let mut out = vec![0.0; nx.max(ny)];
    for j in 0..ny {
      f[..nx].copy_from_slice(&d[j * nx..(j + 1) * nx]);
      edt_1d(&f[..nx], &mut out[..nx]);
      d[j * nx..(j + 1) * nx].copy_from_slice(&out[..nx]);
    }
    for i in 0..nx {
      for j in 0..ny {
        f[j] = d[j * nx + i];
      }
      edt_1d(&f[..ny], &mut out[..ny]);
      for j in 0..ny {
        d[j * nx + i] = out[j];
      }
    }
    d.iter().map(|v| if *v >= big * 0.5 { f64::INFINITY } else { v.sqrt() * self.h }).collect()
  }

  /// Closed iso-lines where the values cross zero, oriented with the negative side on their
  /// left (counter-clockwise around negative areas). Nodes on the grid's border count as
  /// positive, so every line closes.
  pub fn contours(&self) -> Vec<Vec<DVec2>> {
    let (nx, ny) = (self.nx, self.ny);
    if nx < 2 || ny < 2 {
      return Vec::new();
    }
    let val = |i: usize, j: usize| {
      if i == 0 || j == 0 || i == nx - 1 || j == ny - 1 { self.v[j * nx + i].max(1e-9) } else { self.v[j * nx + i] }
    };
    let neg = |v: f64| v < 0.0;
    // edge keys: horizontal edge from node (i, j) = 2 (j nx + i), vertical = 2 (j nx + i) + 1
    let point = |key: usize| -> DVec2 {
      let n = key / 2;
      let (i, j) = (n % nx, n / nx);
      let (i2, j2) = if key.is_multiple_of(2) { (i + 1, j) } else { (i, j + 1) };
      let (a, b) = (val(i, j), val(i2, j2));
      let t = (a / (a - b)).clamp(0.0, 1.0);
      let pa = DVec2::new(self.x0 + i as f64 * self.h, self.y0 + j as f64 * self.h);
      let pb = DVec2::new(self.x0 + i2 as f64 * self.h, self.y0 + j2 as f64 * self.h);
      pa + (pb - pa) * t
    };
    let mut next: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for j in 0..ny - 1 {
      for i in 0..nx - 1 {
        let c = [val(i, j), val(i + 1, j), val(i + 1, j + 1), val(i, j + 1)];
        let n = c.iter().filter(|v| neg(**v)).count();
        if n == 0 || n == 4 {
          continue;
        }
        // the cell's edges counter-clockwise, as (key, corner from, corner to)
        let edges = [
          (2 * (j * nx + i), 0, 1),
          (2 * (j * nx + i + 1) + 1, 1, 2),
          (2 * ((j + 1) * nx + i), 2, 3),
          (2 * (j * nx + i) + 1, 3, 0),
        ];
        let mut ups = Vec::new(); // negative → positive crossings
        let mut downs = Vec::new(); // positive → negative
        for (k, (key, a, b)) in edges.iter().enumerate() {
          match (neg(c[*a]), neg(c[*b])) {
            (true, false) => ups.push((k, *key)),
            (false, true) => downs.push((k, *key)),
            _ => {}
          }
        }
        let centre_neg = neg(c.iter().sum::<f64>() / 4.0);
        for &(k, key) in &ups {
          // pair with the next positive → negative crossing counter-clockwise when the
          // negative corners join through the centre, else with the previous one
          let pick = if centre_neg {
            downs.iter().min_by_key(|(m, _)| (m + 4 - k) % 4)
          } else {
            downs.iter().min_by_key(|(m, _)| (k + 4 - m) % 4)
          };
          if let Some(&(_, to)) = pick {
            next.insert(key, to);
          }
        }
      }
    }
    let mut out = Vec::new();
    let mut keys: Vec<usize> = next.keys().copied().collect();
    keys.sort_unstable();
    for start in keys {
      if !next.contains_key(&start) {
        continue;
      }
      let mut l = Vec::new();
      let mut k = start;
      while let Some(n) = next.remove(&k) {
        l.push(point(k));
        k = n;
      }
      if l.len() > 2 {
        out.push(l);
      }
    }
    out
  }
}

/// Squared 1D distance transform (Felzenszwalb & Huttenlocher) in node units; values of 1e30
/// and more are "no feature".
fn edt_1d(f: &[f64], d: &mut [f64]) {
  let big = 1e30;
  let mut v: Vec<usize> = Vec::with_capacity(f.len());
  let mut z: Vec<f64> = Vec::with_capacity(f.len());
  for (q, &fq) in f.iter().enumerate() {
    if fq >= big {
      continue;
    }
    let hq = fq + (q * q) as f64;
    while let Some(&p) = v.last() {
      let s = (hq - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
      if s <= z[z.len() - 1] {
        v.pop();
        z.pop();
        continue;
      }
      v.push(q);
      z.push(s);
      break;
    }
    if v.is_empty() {
      v.push(q);
      z.push(f64::NEG_INFINITY);
    }
  }
  if v.is_empty() {
    d.fill(big);
    return;
  }
  let mut k = 0;
  for (q, out) in d.iter_mut().enumerate() {
    while k + 1 < v.len() && z[k + 1] < q as f64 {
      k += 1;
    }
    let p = v[k];
    *out = (q as f64 - p as f64).powi(2) + f[p];
  }
}

/// Drops points that deviate less than `tol` from the line through their neighbours.
pub fn simplify(points: &[DVec2], tol: f64, closed: bool) -> Vec<DVec2> {
  if points.len() < 3 {
    return points.to_vec();
  }
  let mut out = vec![points[0]];
  let mut anchor = 0;
  for i in 1..points.len() {
    let next = if i + 1 < points.len() {
      points[i + 1]
    } else if closed {
      points[0]
    } else {
      out.push(points[i]);
      break;
    };
    // keep i if any skipped point between the anchor and the next would stray too far
    let ok = (anchor + 1..=i).all(|k| seg_distance(points[k], points[anchor], next) <= tol);
    if !ok {
      out.push(points[i]);
      anchor = i;
    }
  }
  out
}

/// Signed area (positive = counter-clockwise).
pub fn area2(l: &[DVec2]) -> f64 {
  let n = l.len();
  (0..n).map(|i| l[i].perp_dot(l[(i + 1) % n])).sum::<f64>() / 2.0
}

#[cfg(test)]
mod tests {
  use super::*;

  fn square(lo: f64, hi: f64) -> Vec<DVec3> {
    vec![DVec3::new(lo, lo, 0.0), DVec3::new(hi, lo, 0.0), DVec3::new(hi, hi, 0.0), DVec3::new(lo, hi, 0.0)]
  }

  #[test]
  fn region_distances() {
    // a 20 × 20 square with a 4 × 4 hole in the middle
    let mut hole = square(8.0, 12.0);
    hole.reverse();
    let r = Region::new(&[square(0.0, 20.0), hole]).unwrap();
    assert!(r.contains(DVec2::new(1.0, 1.0)) && !r.contains(DVec2::new(10.0, 10.0)));
    assert!((r.sdf(DVec2::new(3.0, 5.0), 100.0) - 3.0).abs() < 1e-12);
    assert!((r.sdf(DVec2::new(10.0, 10.0), 100.0) + 2.0).abs() < 1e-12);
    assert!((r.sdf(DVec2::new(-5.0, 10.0), 100.0) + 5.0).abs() < 1e-12);
    assert_eq!(r.distance(DVec2::new(5.0, 5.0), 1.0), 1.0);
    let mut g = Grid::new(r.lo, r.hi, 0.5, 2, 0.0);
    r.sdf_grid(&mut g, 50.0);
    for (i, v) in g.v.iter().enumerate() {
      assert!((v - r.sdf(g.node(i), 50.0)).abs() < 1e-9, "{:?}", g.node(i));
    }
  }

  #[test]
  fn distance_transform_and_contours() {
    let mut g = Grid::new(DVec2::ZERO, DVec2::new(10.0, 10.0), 0.25, 0, 0.0);
    let set: Vec<bool> = (0..g.v.len()).map(|i| (g.node(i) - DVec2::new(5.0, 5.0)).length() < 1e-9).collect();
    let d = g.distance_to(&set);
    for (i, v) in d.iter().enumerate() {
      assert!((v - (g.node(i) - DVec2::new(5.0, 5.0)).length()).abs() < 1e-9);
    }
    // the circle of radius 3 around the centre
    for (v, d) in g.v.iter_mut().zip(&d) {
      *v = d - 3.0;
    }
    let c = g.contours();
    assert_eq!(c.len(), 1);
    assert!(area2(&c[0]) > 0.0, "counter-clockwise around the negative inside");
    for p in &c[0] {
      assert!(((*p - DVec2::new(5.0, 5.0)).length() - 3.0).abs() < 0.02, "{p:?}");
    }
    assert!((area2(&c[0]) - std::f64::consts::PI * 9.0).abs() < 0.2);
    let s = simplify(&c[0], 0.01, true);
    assert!(s.len() < c[0].len() && s.len() > 20);
    // a ring (negative between radius 2 and 4): the outer loop counter-clockwise, the inner clockwise
    for (v, d) in g.v.iter_mut().zip(&d) {
      *v = (d - 4.0).max(2.0 - d);
    }
    let mut c = g.contours();
    c.sort_by(|a, b| area2(a).abs().total_cmp(&area2(b).abs()));
    assert_eq!(c.len(), 2);
    assert!(area2(&c[0]) < 0.0 && area2(&c[1]) > 0.0);
  }
}
