//! The closed triangle surface a solid is meshed from: welded from a tessellation (each
//! triangle keeps the id of the CAD face it came from), then remeshed to the element size —
//! long edges split, edges flipped towards Delaunay inside each face and vertices smoothed
//! inside flat faces — so the volume mesher gets evenly sized, well shaped triangles. Edges
//! between different faces (feature edges) are split but never flipped or moved, so every face
//! keeps its boundary.

use std::collections::BinaryHeap;

use crate::geom::*;

#[derive(Debug, Clone, Default)]
pub struct SurfaceMesh {
  pub points: Vec<V3>,
  /// Triangles, counter-clockwise seen from outside.
  pub tris: Vec<[u32; 3]>,
  /// The CAD face each triangle lies on.
  pub tri_face: Vec<u32>,
}

type Edges = Map<(u32, u32), [u32; 2]>;

/// Triangles meeting at less than this (degrees) count as coplanar: only those are flipped, so
/// the faceted surface keeps its shape exactly.
const FLAT_DEG: f64 = 1.0;

fn key(a: u32, b: u32) -> (u32, u32) {
  if a < b { (a, b) } else { (b, a) }
}

/// Heap entry: how many times too long an edge is.
#[derive(PartialEq)]
struct Long(f64, u32, u32);
impl Eq for Long {}
impl PartialOrd for Long {
  fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(o))
  }
}
impl Ord for Long {
  fn cmp(&self, o: &Self) -> std::cmp::Ordering {
    self.0.total_cmp(&o.0)
  }
}

impl SurfaceMesh {
  /// Welds a triangle soup (positions shared between faces only by coordinate) into one surface:
  /// points closer than `tol` merge, and triangles that collapse are dropped. The result is
  /// turned outward if it encloses negative volume.
  pub fn weld(positions: &[V3], indices: &[u32], tri_face: &[u32], tol: f64) -> SurfaceMesh {
    let tol = tol.max(1e-12);
    let cellkey = |p: V3| [(p[0] / tol).floor() as i64, (p[1] / tol).floor() as i64, (p[2] / tol).floor() as i64];
    let mut grid: Map<[i64; 3], Vec<u32>> = Map::default();
    let mut points: Vec<V3> = Vec::new();
    let mut remap = Vec::with_capacity(positions.len());
    for &p in positions {
      let c = cellkey(p);
      let mut found = None;
      'search: for dx in -1..=1 {
        for dy in -1..=1 {
          for dz in -1..=1 {
            if let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
              for &i in list {
                if dist(points[i as usize], p) <= tol {
                  found = Some(i);
                  break 'search;
                }
              }
            }
          }
        }
      }
      let i = found.unwrap_or_else(|| {
        points.push(p);
        let i = points.len() as u32 - 1;
        grid.entry(c).or_default().push(i);
        i
      });
      remap.push(i);
    }
    let mut tris = Vec::new();
    let mut faces = Vec::new();
    for (t, c) in indices.chunks_exact(3).enumerate() {
      let tri = [remap[c[0] as usize], remap[c[1] as usize], remap[c[2] as usize]];
      if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
        continue;
      }
      let [a, b, c] = tri.map(|i| points[i as usize]);
      if norm(tri_normal(a, b, c)) <= 1e-14 * (dist(a, b) + dist(b, c)).powi(2) {
        continue;
      }
      tris.push(tri);
      faces.push(tri_face.get(t).copied().unwrap_or(0));
    }
    let mut m = SurfaceMesh { points, tris, tri_face: faces };
    if m.volume() < 0.0 {
      for t in &mut m.tris {
        t.swap(1, 2);
      }
    }
    m
  }

  /// Enclosed volume (negative when the triangles face inward).
  pub fn volume(&self) -> f64 {
    self
      .tris
      .iter()
      .map(|t| {
        let [a, b, c] = t.map(|i| self.points[i as usize]);
        dot(a, cross(b, c)) / 6.0
      })
      .sum()
  }

  pub fn area(&self) -> f64 {
    self
      .tris
      .iter()
      .map(|t| tri_area(self.points[t[0] as usize], self.points[t[1] as usize], self.points[t[2] as usize]))
      .sum()
  }

  fn edges(&self) -> Edges {
    let mut e: Edges = Map::with_capacity_and_hasher(self.tris.len() * 2, Default::default());
    for (t, tri) in self.tris.iter().enumerate() {
      for k in 0..3 {
        let slot = e.entry(key(tri[k], tri[(k + 1) % 3])).or_insert([u32::MAX; 2]);
        if slot[0] == u32::MAX {
          slot[0] = t as u32;
        } else {
          slot[1] = t as u32;
        }
      }
    }
    e
  }

  /// Every edge is shared by exactly two triangles that run along it in opposite directions.
  pub fn is_closed(&self) -> bool {
    let mut count: Map<(u32, u32), i32> = Map::default();
    for tri in &self.tris {
      for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        let dir = if a < b { 1000 } else { -1000 };
        *count.entry(key(a, b)).or_insert(0) += dir + 1;
      }
    }
    // each edge must appear once in each direction: +1000+1 and -1000+1 sum to 2
    count.values().all(|&c| c == 2)
  }

  /// The edges that aren't shared by exactly two consistently oriented triangles.
  pub fn open_edges(&self) -> usize {
    let mut count: Map<(u32, u32), i32> = Map::default();
    for tri in &self.tris {
      for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        let dir = if a < b { 1000 } else { -1000 };
        *count.entry(key(a, b)).or_insert(0) += dir + 1;
      }
    }
    count.values().filter(|&&c| c != 2).count()
  }

  /// The faces each point is on.
  pub fn point_faces(&self) -> Vec<Vec<u32>> {
    let mut out = vec![Vec::new(); self.points.len()];
    for (t, tri) in self.tris.iter().enumerate() {
      for &v in tri {
        let f = self.tri_face[t];
        if !out[v as usize].contains(&f) {
          out[v as usize].push(f);
        }
      }
    }
    out
  }

  /// Splits the edge `a`–`b` at its midpoint (or `at`); returns the new point.
  fn split(&mut self, edges: &mut Edges, a: u32, b: u32, at: Option<V3>) -> Option<u32> {
    let pair = *edges.get(&key(a, b))?;
    if pair[1] == u32::MAX {
      return None;
    }
    // t1 runs a → b, t2 runs b → a
    let (t1, t2) = if self.has_directed(pair[0], a, b) { (pair[0], pair[1]) } else { (pair[1], pair[0]) };
    let c = self.third(t1, a, b);
    let d = self.third(t2, a, b);
    let m = self.points.len() as u32;
    self.points.push(at.unwrap_or_else(|| mid(self.points[a as usize], self.points[b as usize])));
    let (f1, f2) = (self.tri_face[t1 as usize], self.tri_face[t2 as usize]);
    let t3 = self.tris.len() as u32;
    let t4 = t3 + 1;
    self.tris[t1 as usize] = [a, m, c];
    self.tris[t2 as usize] = [b, m, d];
    self.tris.push([m, b, c]);
    self.tris.push([m, a, d]);
    self.tri_face.push(f1);
    self.tri_face.push(f2);
    edges.remove(&key(a, b));
    edges.insert(key(a, m), [t1, t4]);
    edges.insert(key(m, b), [t3, t2]);
    edges.insert(key(m, c), [t1, t3]);
    edges.insert(key(m, d), [t2, t4]);
    replace(edges, key(b, c), t1, t3);
    replace(edges, key(a, d), t2, t4);
    Some(m)
  }

  fn has_directed(&self, t: u32, a: u32, b: u32) -> bool {
    let tri = self.tris[t as usize];
    (0..3).any(|k| tri[k] == a && tri[(k + 1) % 3] == b)
  }

  fn third(&self, t: u32, a: u32, b: u32) -> u32 {
    *self.tris[t as usize].iter().find(|&&v| v != a && v != b).unwrap_or(&a)
  }

  /// For each missing edge: flips it to the other diagonal when its two triangles are coplanar
  /// (the same surface either way) and `has(c, d)` accepts that diagonal, else splits it at its
  /// middle. Returns the points the splits made.
  pub fn flip_or_split(&mut self, pairs: &[(u32, u32)], has: &dyn Fn(u32, u32) -> bool) -> Vec<u32> {
    let mut e = self.edges();
    // within a face the surface may bend a little at a flip (it's faceted anyway)
    let cos_flat = FLAT_DEG.to_radians().cos();
    let mut out = Vec::new();
    let mut k = 0u32;
    for &(a, b) in pairs {
      k = k.wrapping_add(1);
      let Some(&pair) = e.get(&key(a, b)) else { continue };
      if pair[1] != u32::MAX && self.tri_face[pair[0] as usize] == self.tri_face[pair[1] as usize] {
        let (t1, t2) = if self.has_directed(pair[0], a, b) { (pair[0], pair[1]) } else { (pair[1], pair[0]) };
        let c = self.third(t1, a, b);
        let d = self.third(t2, a, b);
        if c != d && !e.contains_key(&key(c, d)) && has(c, d) {
          let [pa, pb, pc, pd] = [a, b, c, d].map(|v| self.points[v as usize]);
          let n1 = unit(tri_normal(pa, pb, pc));
          let n2 = unit(tri_normal(pb, pa, pd));
          let m1 = tri_normal(pa, pd, pc);
          let m2 = tri_normal(pd, pb, pc);
          let navg = unit(add(n1, n2));
          if dot(n1, n2) >= cos_flat && dot(m1, navg) > 1e-9 * norm(m1) && dot(m2, navg) > 1e-9 * norm(m2) {
            self.tris[t1 as usize] = [a, d, c];
            self.tris[t2 as usize] = [d, b, c];
            e.remove(&key(a, b));
            e.insert(key(c, d), [t1, t2]);
            replace(&mut e, key(b, c), t1, t2);
            replace(&mut e, key(a, d), t2, t1);
            continue;
          }
        }
      }
      // split a little off the middle: midpoints of regular grids stay cocircular
      let f = 0.5 + 0.08 * (((k.wrapping_mul(2654435761) >> 16) % 1000) as f64 / 1000.0 - 0.5);
      let (pa, pb) = (self.points[a as usize], self.points[b as usize]);
      if let Some(m) = self.split(&mut e, a, b, Some(add(pa, scale(sub(pb, pa), f)))) {
        out.push(m);
      }
    }
    out
  }

  /// Splits the edges between pairs of points at their middles, where they exist (both
  /// triangles along each get split); returns the new points.
  pub fn split_edges(&mut self, pairs: &[(u32, u32)]) -> Vec<Option<u32>> {
    let mut e = self.edges();
    pairs.iter().map(|&(a, b)| self.split(&mut e, a, b, None)).collect()
  }

  /// Splits edges longer than 4/3 of the size wanted at their middle, longest first, until none
  /// is.
  pub fn refine(&mut self, size: &dyn Fn(V3) -> f64) {
    self.refine_where(size, false);
  }

  /// Splits the edges between faces longer than the size wanted.
  pub fn refine_features(&mut self, size: &dyn Fn(V3) -> f64) {
    self.refine_where(size, true);
  }

  fn refine_where(&mut self, size: &dyn Fn(V3) -> f64, features: bool) {
    let mut edges = self.edges();
    let feature = |m: &SurfaceMesh, e: &Edges, a: u32, b: u32| {
      e.get(&key(a, b)).is_some_and(|p| p[1] == u32::MAX || m.tri_face[p[0] as usize] != m.tri_face[p[1] as usize])
    };
    let limit_ratio = if features { 1.0 } else { 4.0 / 3.0 };
    let ratio = |m: &SurfaceMesh, a: u32, b: u32| {
      let (p, q) = (m.points[a as usize], m.points[b as usize]);
      dist(p, q) / size(mid(p, q)).max(1e-9)
    };
    let mut heap = BinaryHeap::new();
    for &(a, b) in edges.keys() {
      let r = ratio(self, a, b);
      if r > limit_ratio && (!features || feature(self, &edges, a, b)) {
        heap.push(Long(r, a, b));
      }
    }
    let limit = 4_000_000usize;
    while let Some(Long(r, a, b)) = heap.pop() {
      if !edges.contains_key(&key(a, b)) || (ratio(self, a, b) - r).abs() > 1e-12 {
        continue;
      }
      if self.points.len() > limit {
        break;
      }
      let Some(m) = self.split(&mut edges, a, b, None) else { continue };
      let mut around = vec![a, b];
      for t in &self.tris[self.tris.len() - 2..] {
        around.extend(t.iter().copied());
      }
      around.sort();
      around.dedup();
      for v in around {
        if v != m && edges.contains_key(&key(m, v)) && (!features || feature(self, &edges, m, v)) {
          let r = ratio(self, m, v);
          if r > limit_ratio {
            heap.push(Long(r, m, v));
          }
        }
      }
    }
  }

  /// Flips edges inside faces towards a Delaunay triangulation, where the two triangles are
  /// close to coplanar. Returns the number of flips.
  pub fn flip_delaunay(&mut self, max_dihedral_deg: f64) -> usize {
    let mut edges = self.edges();
    let cos_max = max_dihedral_deg.to_radians().cos();
    let mut flips = 0;
    for _ in 0..8 {
      let mut changed = 0;
      let keys: Vec<(u32, u32)> = edges.keys().copied().collect();
      for (a, b) in keys {
        if self.try_flip(&mut edges, a, b, cos_max).is_some() {
          changed += 1;
        }
      }
      flips += changed;
      if changed == 0 {
        break;
      }
    }
    flips
  }

  /// Flips the edge `a`–`b` to the other diagonal if both triangles are on one face, nearly
  /// coplanar, and the flip makes them Delaunay; returns the new diagonal's ends.
  fn try_flip(&mut self, edges: &mut Edges, a: u32, b: u32, cos_max: f64) -> Option<(u32, u32)> {
    let pair = *edges.get(&key(a, b))?;
    if pair[1] == u32::MAX || self.tri_face[pair[0] as usize] != self.tri_face[pair[1] as usize] {
      return None;
    }
    let (t1, t2) = if self.has_directed(pair[0], a, b) { (pair[0], pair[1]) } else { (pair[1], pair[0]) };
    let c = self.third(t1, a, b);
    let d = self.third(t2, a, b);
    if c == d || edges.contains_key(&key(c, d)) {
      return None;
    }
    let [pa, pb, pc, pd] = [a, b, c, d].map(|v| self.points[v as usize]);
    let n1 = unit(tri_normal(pa, pb, pc));
    let n2 = unit(tri_normal(pb, pa, pd));
    if dot(n1, n2) < cos_max {
      return None;
    }
    // the angles at c and d opposite the edge; flip when they sum past π
    if angle(pc, pa, pb) + angle(pd, pa, pb) <= std::f64::consts::PI + 1e-9 {
      return None;
    }
    let navg = unit(add(n1, n2));
    let m1 = tri_normal(pa, pd, pc);
    let m2 = tri_normal(pd, pb, pc);
    if dot(m1, navg) <= 1e-12 * norm(m1) || dot(m2, navg) <= 1e-12 * norm(m2) {
      return None;
    }
    self.tris[t1 as usize] = [a, d, c];
    self.tris[t2 as usize] = [d, b, c];
    edges.remove(&key(a, b));
    edges.insert(key(c, d), [t1, t2]);
    replace(edges, key(b, c), t1, t2);
    replace(edges, key(a, d), t2, t1);
    Some((c, d))
  }

  /// Splits triangle `t` at `p` (inside it) into three; returns the new point.
  fn split_tri(&mut self, edges: &mut Edges, t: u32, p: V3) -> u32 {
    let [a, b, c] = self.tris[t as usize];
    let m = self.points.len() as u32;
    self.points.push(p);
    let f = self.tri_face[t as usize];
    let t2 = self.tris.len() as u32;
    let t3 = t2 + 1;
    self.tris[t as usize] = [a, b, m];
    self.tris.push([b, c, m]);
    self.tris.push([c, a, m]);
    self.tri_face.extend([f, f]);
    replace(edges, key(b, c), t, t2);
    replace(edges, key(c, a), t, t3);
    edges.insert(key(a, m), [t, t3]);
    edges.insert(key(b, m), [t, t2]);
    edges.insert(key(c, m), [t2, t3]);
    m
  }

  /// The triangles touching point `m` (found from the edge map around `hint`'s triangles).
  fn around(&self, edges: &Edges, m: u32, near: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut todo: Vec<u32> = near.to_vec();
    while let Some(t) = todo.pop() {
      if out.contains(&t) || !self.tris[t as usize].contains(&m) {
        continue;
      }
      out.push(t);
      let tri = self.tris[t as usize];
      for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        if a != m && b != m {
          continue;
        }
        if let Some(p) = edges.get(&key(a, b)) {
          for &n in p {
            if n != u32::MAX {
              todo.push(n);
            }
          }
        }
      }
    }
    out
  }

  /// Delaunay refinement inside faces (Chew/Ruppert): the circumcentre of every triangle too
  /// big for the size field, or too skinny, is inserted, and edges are flipped back to
  /// Delaunay. A circumcentre beyond its face's boundary splits the boundary edge in the way.
  pub fn refine_delaunay(&mut self, size: &dyn Fn(V3) -> f64) {
    let cos_max = FLAT_DEG.to_radians().cos();
    let mut edges = self.edges();
    let budget = (self.area() / size_floor(self, size).powi(2) * 8.0) as usize + 10 * self.tris.len() + 1000;
    let badness = |m: &SurfaceMesh, t: u32| -> f64 {
      let [a, b, c] = m.tris[t as usize].map(|i| m.points[i as usize]);
      let h = size(scale(add(add(a, b), c), 1.0 / 3.0)).max(1e-12);
      let r = circumradius(a, b, c);
      let short = dist(a, b).min(dist(b, c)).min(dist(c, a));
      let big = r / (0.7 * h);
      // skinny: circumradius long against the shortest edge (min angle under about 26°)
      let skinny = if r > 0.25 * h && short > 0.0 { r / short / 1.15 } else { 0.0 };
      big.max(skinny)
    };
    let mut heap: BinaryHeap<Long> = BinaryHeap::new();
    for t in 0..self.tris.len() as u32 {
      let b = badness(self, t);
      if b > 1.0 {
        heap.push(Long(b, t, 0));
      }
    }
    let mut inserted = 0usize;
    let mut failed: Set<[u32; 3]> = Set::default();
    while let Some(Long(bad, t, _)) = heap.pop() {
      if inserted > budget {
        break;
      }
      if (badness(self, t) - bad).abs() > 1e-12 * bad.max(1.0) || failed.contains(&self.tris[t as usize]) {
        continue;
      }
      let [a, b, c] = self.tris[t as usize].map(|i| self.points[i as usize]);
      let cc = circumcentre(a, b, c);
      let face = self.tri_face[t as usize];
      let new_point = match self.walk(&edges, t, cc, face) {
        Walk::Inside(tt, p) => {
          // a point next to an edge that can't flip would leave a sliver along it: split the
          // edge instead
          let tri = self.tris[tt as usize];
          let h = size(p);
          let near = (0..3).map(|k| (tri[k], tri[(k + 1) % 3])).find(|&(x, y)| {
            let (px, py) = (self.points[x as usize], self.points[y as usize]);
            self.is_segment(&edges, x, y, cos_max) && seg_dist(p, px, py) < 0.2 * h
          });
          match near {
            Some((x, y)) => {
              let (px, py) = (self.points[x as usize], self.points[y as usize]);
              if dist(px, py) > 0.3 * size(mid(px, py)) { self.split(&mut edges, x, y, None) } else { None }
            }
            None => Some(self.split_tri(&mut edges, tt, p)),
          }
        }
        Walk::OnEdge(x, y) => self.split(&mut edges, x, y, None),
        Walk::Blocked(x, y) => {
          // a boundary edge the circumcentre encroaches on (inside its diametral circle) splits
          let (px, py) = (self.points[x as usize], self.points[y as usize]);
          let l = dist(px, py);
          if dist(cc, mid(px, py)) < 0.5 * l && l > 0.3 * size(mid(px, py)) {
            self.split(&mut edges, x, y, None)
          } else {
            None
          }
        }
        Walk::Lost => None,
      };
      let Some(m) = new_point else {
        failed.insert(self.tris[t as usize]);
        continue;
      };
      inserted += 1;
      let n = self.tris.len() as u32;
      let start: Vec<u32> = (n.saturating_sub(4)..n).chain([t]).collect();
      let mut fan = self.around(&edges, m, &start);
      let mut stack: Vec<(u32, u32)> = fan
        .iter()
        .map(|&f| {
          let tri = self.tris[f as usize];
          let k = tri.iter().position(|&v| v == m).unwrap_or(0);
          (tri[(k + 1) % 3], tri[(k + 2) % 3])
        })
        .collect();
      let mut guard = 0;
      while let Some((x, y)) = stack.pop() {
        guard += 1;
        if guard > 5_000 {
          break;
        }
        if let Some((c2, d2)) = self.try_flip(&mut edges, x, y, cos_max) {
          let other = if c2 == m {
            d2
          } else if d2 == m {
            c2
          } else {
            continue;
          };
          stack.push((x, other));
          stack.push((other, y));
        }
      }
      fan = self.around(&edges, m, &fan.iter().copied().chain(start.iter().copied()).collect::<Vec<_>>());
      // the fan's triangles and their neighbours may now be bad
      for &f in &fan {
        let b = badness(self, f);
        if b > 1.0 {
          heap.push(Long(b, f, 0));
        }
      }
    }
  }

  /// Splits every edge that can't be flipped (between faces, or where the surface bends) while a
  /// triangle on either side has an obtuse angle facing it, so those edges get empty diametral
  /// spheres and a Delaunay tetrahedralization contains them. Returns the splits.
  pub fn split_encroached(&mut self, size: &dyn Fn(V3) -> f64) -> usize {
    let cos_flat = FLAT_DEG.to_radians().cos();
    let mut edges = self.edges();
    let mut total = 0;
    for _ in 0..40 {
      let mut todo = Vec::new();
      for (&(a, b), pair) in &edges {
        if pair[1] == u32::MAX {
          continue;
        }
        let (t1, t2) = (pair[0], pair[1]);
        let c = self.third(t1, a, b);
        let d = self.third(t2, a, b);
        let [pa, pb, pc, pd] = [a, b, c, d].map(|v| self.points[v as usize]);
        let segment = self.tri_face[t1 as usize] != self.tri_face[t2 as usize]
          || dot(unit(self.normal(t1)), unit(self.normal(t2))) < cos_flat;
        if !segment {
          continue;
        }
        let obtuse = angle(pc, pa, pb).max(angle(pd, pa, pb)) > std::f64::consts::FRAC_PI_2 + 1e-9;
        if obtuse && dist(pa, pb) > 0.1 * size(mid(pa, pb)) {
          todo.push((a, b));
        }
      }
      if todo.is_empty() {
        break;
      }
      let cos_max = cos_flat;
      for (a, b) in todo {
        let Some(m) = self.split(&mut edges, a, b, None) else { continue };
        total += 1;
        let n = self.tris.len() as u32;
        let fan = self.around(&edges, m, &(n - 2..n).collect::<Vec<_>>());
        let mut stack: Vec<(u32, u32)> = fan
          .iter()
          .map(|&f| {
            let tri = self.tris[f as usize];
            let k = tri.iter().position(|&v| v == m).unwrap_or(0);
            (tri[(k + 1) % 3], tri[(k + 2) % 3])
          })
          .collect();
        let mut guard = 0;
        while let Some((x, y)) = stack.pop() {
          guard += 1;
          if guard > 2_000 {
            break;
          }
          if let Some((c2, d2)) = self.try_flip(&mut edges, x, y, cos_max) {
            let other = if c2 == m {
              d2
            } else if d2 == m {
              c2
            } else {
              continue;
            };
            stack.push((x, other));
            stack.push((other, y));
          }
        }
      }
    }
    total
  }

  /// An edge that is never flipped: between faces, or where the surface bends.
  fn is_segment(&self, edges: &Edges, a: u32, b: u32, cos_flat: f64) -> bool {
    let Some(p) = edges.get(&key(a, b)) else { return false };
    p[1] == u32::MAX
      || self.tri_face[p[0] as usize] != self.tri_face[p[1] as usize]
      || dot(unit(self.normal(p[0])), unit(self.normal(p[1]))) < cos_flat
  }

  /// Walks across triangles of one face from `t` towards `p`.
  fn walk(&self, edges: &Edges, t: u32, p: V3, face: u32) -> Walk {
    let mut t = t;
    let mut prev = u32::MAX;
    for _ in 0..10_000 {
      let tri = self.tris[t as usize];
      let [a, b, c] = tri.map(|i| self.points[i as usize]);
      let n = tri_normal(a, b, c);
      let nn = dot(n, n);
      if nn <= 0.0 {
        return Walk::Lost;
      }
      // barycentric coordinates of p projected onto the triangle's plane
      let l = [dot(tri_normal(b, c, p), n) / nn, dot(tri_normal(c, a, p), n) / nn, dot(tri_normal(a, b, p), n) / nn];
      let (k, lo) = (0..3).map(|k| (k, l[k])).fold((0, f64::INFINITY), |m, x| if x.1 < m.1 { x } else { m });
      if lo >= 1e-3 {
        let q = add(add(scale(a, l[0]), scale(b, l[1])), scale(c, l[2]));
        return Walk::Inside(t, q);
      }
      // the edge opposite the most negative coordinate
      let (x, y) = (tri[(k + 1) % 3], tri[(k + 2) % 3]);
      if lo > -1e-3 {
        // on (or very near) that edge
        let other = edges.get(&key(x, y)).map(|p| if p[0] == t { p[1] } else { p[0] });
        let interior = other.is_some_and(|o| o != u32::MAX && self.tri_face[o as usize] == face);
        return if interior { Walk::OnEdge(x, y) } else { Walk::Blocked(x, y) };
      }
      let Some(pair) = edges.get(&key(x, y)) else { return Walk::Lost };
      let next = if pair[0] == t { pair[1] } else { pair[0] };
      if next == u32::MAX || self.tri_face[next as usize] != face {
        return Walk::Blocked(x, y);
      }
      if next == prev {
        return Walk::Lost;
      }
      prev = t;
      t = next;
    }
    Walk::Lost
  }

  /// Moves each point inside a flat region of a face towards the middle of its neighbours,
  /// in the plane, where that keeps every triangle around it the right way up.
  pub fn smooth(&mut self, passes: usize) {
    let mut ring: Vec<Vec<u32>> = vec![Vec::new(); self.points.len()];
    let mut vtris: Vec<Vec<u32>> = vec![Vec::new(); self.points.len()];
    for (t, tri) in self.tris.iter().enumerate() {
      for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        ring[a as usize].push(b);
        ring[b as usize].push(a);
        vtris[a as usize].push(t as u32);
      }
    }
    for r in &mut ring {
      r.sort();
      r.dedup();
    }
    let flat = (0.5f64).to_radians().cos();
    for _ in 0..passes {
      for v in 0..self.points.len() {
        let ts = &vtris[v];
        if ts.is_empty() || ts.iter().any(|&t| self.tri_face[t as usize] != self.tri_face[ts[0] as usize]) {
          continue;
        }
        let normals: Vec<V3> = ts.iter().map(|&t| self.normal(t)).collect();
        let n = unit(normals.iter().fold([0.0; 3], |s, m| add(s, *m)));
        if normals.iter().any(|m| dot(unit(*m), n) < flat) {
          continue;
        }
        let c =
          scale(ring[v].iter().fold([0.0; 3], |s, &w| add(s, self.points[w as usize])), 1.0 / ring[v].len() as f64);
        let old = self.points[v];
        let d = sub(c, old);
        let moved = add(old, sub(d, scale(n, dot(d, n))));
        self.points[v] = moved;
        let ok = ts.iter().all(|&t| {
          let m = self.normal(t);
          dot(m, n) > 0.2 * norm(m)
        });
        if !ok {
          self.points[v] = old;
        }
      }
    }
  }

  fn normal(&self, t: u32) -> V3 {
    let [a, b, c] = self.tris[t as usize].map(|i| self.points[i as usize]);
    tri_normal(a, b, c)
  }

  /// Remeshes to the size field: feature edges split to size, a Delaunay triangulation of each
  /// face refined until its triangles are small and well shaped, then smoothed a little.
  pub fn remesh(&mut self, size: &dyn Fn(V3) -> f64) {
    self.refine_features(size);
    self.flip_delaunay(FLAT_DEG);
    for _ in 0..4 {
      self.refine_delaunay(size);
      if self.split_encroached(size) == 0 {
        break;
      }
    }
    self.smooth(2);
    self.flip_delaunay(FLAT_DEG);
  }

  /// Drops points no triangle uses.
  pub fn compact(&mut self) {
    let mut used = vec![u32::MAX; self.points.len()];
    let mut pts = Vec::new();
    for t in &mut self.tris {
      for v in t.iter_mut() {
        if used[*v as usize] == u32::MAX {
          used[*v as usize] = pts.len() as u32;
          pts.push(self.points[*v as usize]);
        }
        *v = used[*v as usize];
      }
    }
    self.points = pts;
  }
}

fn replace(edges: &mut Edges, k: (u32, u32), from: u32, to: u32) {
  if let Some(p) = edges.get_mut(&k) {
    for t in p.iter_mut() {
      if *t == from {
        *t = to;
        return;
      }
    }
  }
}

/// A box's surface (two triangles per side), faces numbered 1–6: −X, +X, −Y, +Y, −Z, +Z.
pub fn box_surface(lo: V3, hi: V3) -> SurfaceMesh {
  let p = |i: usize| {
    [
      if i & 1 == 0 { lo[0] } else { hi[0] },
      if i & 2 == 0 { lo[1] } else { hi[1] },
      if i & 4 == 0 { lo[2] } else { hi[2] },
    ]
  };
  let points: Vec<V3> = (0..8).map(p).collect();
  // quads counter-clockwise from outside
  let quads: [([u32; 4], u32); 6] =
    [([0, 4, 6, 2], 1), ([1, 3, 7, 5], 2), ([0, 1, 5, 4], 3), ([2, 6, 7, 3], 4), ([0, 2, 3, 1], 5), ([4, 5, 7, 6], 6)];
  let mut tris = Vec::new();
  let mut tri_face = Vec::new();
  for (q, f) in quads {
    tris.push([q[0], q[1], q[2]]);
    tris.push([q[0], q[2], q[3]]);
    tri_face.extend([f, f]);
  }
  SurfaceMesh { points, tris, tri_face }
}

enum Walk {
  Inside(u32, V3),
  OnEdge(u32, u32),
  Blocked(u32, u32),
  Lost,
}

fn angle(o: V3, p: V3, q: V3) -> f64 {
  let (u, v) = (sub(p, o), sub(q, o));
  norm(cross(u, v)).atan2(dot(u, v))
}

pub fn circumcentre(a: V3, b: V3, c: V3) -> V3 {
  let ab = sub(b, a);
  let ac = sub(c, a);
  let n = cross(ab, ac);
  let nn = dot(n, n);
  if nn <= 0.0 {
    return scale(add(add(a, b), c), 1.0 / 3.0);
  }
  let t = add(scale(cross(n, ab), dot(ac, ac)), scale(cross(ac, n), dot(ab, ab)));
  add(a, scale(t, 0.5 / nn))
}

pub fn circumradius(a: V3, b: V3, c: V3) -> f64 {
  let n = norm(tri_normal(a, b, c));
  if n <= 0.0 { f64::INFINITY } else { dist(a, b) * dist(b, c) * dist(c, a) / (2.0 * n) }
}

/// The smallest size the field asks for at the mesh's points.
fn size_floor(m: &SurfaceMesh, size: &dyn Fn(V3) -> f64) -> f64 {
  m.points.iter().map(|p| size(*p)).fold(f64::INFINITY, f64::min).max(1e-9)
}

fn seg_dist(p: V3, a: V3, b: V3) -> f64 {
  let ab = sub(b, a);
  let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-300)).clamp(0.0, 1.0);
  dist(p, add(a, scale(ab, t)))
}
