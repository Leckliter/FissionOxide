//! Quadratic tetrahedral meshes of solids.
//!
//! [`mesh_solid`] turns a closed [`SurfaceMesh`] into 10-node tetrahedra:
//!
//! 1. A size field: the global element size, smaller near faces given a local size (growing back
//!    at the grading rate with distance from them).
//! 2. The surface is remeshed to the size field (see [`SurfaceMesh::remesh`]).
//! 3. Interior points come from an octree whose cells shrink to the size field; each leaf adds
//!    its corners and centre, which makes a body-centred cubic lattice (whose Delaunay
//!    tetrahedra are all well shaped) wherever the size is constant. Points outside the solid or
//!    too close to its surface are dropped.
//! 4. Everything is Delaunay tetrahedralized. Surface edges and triangles missing from the
//!    tetrahedralization are recovered by splitting them (conforming Delaunay), so the
//!    tetrahedra fill the solid exactly.
//! 5. Tetrahedra outside the solid go; flat ones on the surface (four surface points nearly in a
//!    plane) are peeled off.
//! 6. A node goes at the middle of every edge (straight-sided quadratic tetrahedra), and the
//!    boundary triangles keep the CAD face they lie on, for loads and constraints.

use serde::{Deserialize, Serialize};

use crate::delaunay::{Delaunay, FACES};
use crate::geom::*;
use crate::surface::SurfaceMesh;

/// Mid-edge node order of a 10-node tetrahedron: node 4 + k sits on edge `TET_EDGES[k]`.
pub const TET_EDGES: [[usize; 2]; 6] = [[0, 1], [1, 2], [0, 2], [0, 3], [1, 3], [2, 3]];
/// Mid-edge node order of a 6-node triangle: node 3 + k sits on edge `TRI_EDGES[k]`.
pub const TRI_EDGES: [[usize; 2]; 3] = [[0, 1], [1, 2], [2, 0]];

/// A local mesh size on some faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refinement {
  pub faces: Vec<u32>,
  /// Element size on and near the faces (mm).
  pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshOptions {
  /// Global element size (mm).
  pub size: f64,
  #[serde(default)]
  pub refine: Vec<Refinement>,
  /// How fast the size grows away from refined faces (size per unit distance, default 0.4).
  #[serde(default = "grading")]
  pub grading: f64,
}

fn grading() -> f64 {
  0.4
}

impl MeshOptions {
  pub fn new(size: f64) -> MeshOptions {
    MeshOptions { size, refine: Vec::new(), grading: grading() }
  }

  /// A size for a body: a twentieth of its bounding box's largest side (clamped to a tenth of its
  /// smallest so thin parts get several layers).
  pub fn auto(lo: V3, hi: V3) -> MeshOptions {
    let ext = sub(hi, lo);
    let big = ext[0].max(ext[1]).max(ext[2]);
    let small = ext[0].min(ext[1]).min(ext[2]).max(big * 1e-3);
    MeshOptions::new((big / 20.0).min(small / 2.0).max(big / 200.0))
  }
}

/// A mesh of 10-node tetrahedra.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TetMesh {
  pub nodes: Vec<V3>,
  /// Corner nodes come first: `nodes[..corners]`.
  pub corners: usize,
  /// Corners 0–3 (positive volume), then the mid-edge nodes in [`TET_EDGES`] order.
  pub tets: Vec<[u32; 10]>,
  /// Boundary triangles, wound counter-clockwise from outside: corners then the mid-edge nodes
  /// in [`TRI_EDGES`] order.
  pub boundary: Vec<[u32; 6]>,
  /// The CAD face each boundary triangle lies on.
  pub boundary_face: Vec<u32>,
}

/// What meshing made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshStats {
  pub nodes: usize,
  pub elements: usize,
  pub boundary_triangles: usize,
  pub volume: f64,
  /// Worst and mean element shape quality (1 = regular tetrahedron).
  pub min_quality: f64,
  pub mean_quality: f64,
  /// Surface edges and triangles that couldn't be recovered (0 for a mesh matching the surface).
  pub unrecovered: usize,
}

impl TetMesh {
  pub fn volume(&self) -> f64 {
    self.tets.iter().map(|t| self.tet_volume(t)).sum()
  }

  pub fn tet_volume(&self, t: &[u32; 10]) -> f64 {
    let [a, b, c, d] = [0, 1, 2, 3].map(|k| self.nodes[t[k] as usize]);
    tet_volume(a, b, c, d)
  }

  pub fn quality(&self) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut sum = 0.0;
    for t in &self.tets {
      let [a, b, c, d] = [0, 1, 2, 3].map(|k| self.nodes[t[k] as usize]);
      let q = tet_quality(a, b, c, d);
      lo = lo.min(q);
      sum += q;
    }
    (if self.tets.is_empty() { 0.0 } else { lo }, sum / self.tets.len().max(1) as f64)
  }

  pub fn stats(&self) -> MeshStats {
    let (min_quality, mean_quality) = self.quality();
    MeshStats {
      nodes: self.nodes.len(),
      elements: self.tets.len(),
      boundary_triangles: self.boundary.len(),
      volume: self.volume(),
      min_quality,
      mean_quality,
      unrecovered: 0,
    }
  }

  /// The nodes on any of the faces.
  pub fn face_nodes(&self, faces: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = self
      .boundary
      .iter()
      .zip(&self.boundary_face)
      .filter(|(_, f)| faces.contains(f))
      .flat_map(|(t, _)| t.iter().copied())
      .collect();
    out.sort();
    out.dedup();
    out
  }

  /// The boundary triangles on any of the faces.
  pub fn face_tris(&self, faces: &[u32]) -> Vec<usize> {
    (0..self.boundary.len()).filter(|&k| faces.contains(&self.boundary_face[k])).collect()
  }

  /// The CAD faces on the boundary.
  pub fn faces(&self) -> Vec<u32> {
    let mut f = self.boundary_face.clone();
    f.sort();
    f.dedup();
    f
  }

  /// A mesh from linear tetrahedra (corner nodes only, any orientation): mid-edge nodes are added
  /// and the boundary found. `face_of` names the CAD face of a boundary triangle from its corners.
  pub fn from_linear(points: &[V3], tets: &[[u32; 4]], face_of: &dyn Fn([u32; 3]) -> u32) -> TetMesh {
    let mut used = vec![u32::MAX; points.len()];
    let mut nodes = Vec::new();
    let mut corner_tets = Vec::with_capacity(tets.len());
    for t in tets {
      let mut v = *t;
      let q = v.map(|k| points[k as usize]);
      if tet_volume(q[0], q[1], q[2], q[3]) < 0.0 {
        v.swap(0, 1);
      }
      for k in &mut v {
        if used[*k as usize] == u32::MAX {
          used[*k as usize] = nodes.len() as u32;
          nodes.push(points[*k as usize]);
        }
      }
      corner_tets.push(v);
    }
    let back: Vec<u32> = {
      let mut b = vec![0u32; nodes.len()];
      for (orig, &n) in used.iter().enumerate() {
        if n != u32::MAX {
          b[n as usize] = orig as u32;
        }
      }
      b
    };
    let corners = nodes.len();
    let mut mids: Map<(u32, u32), u32> = Map::default();
    let mut out = Vec::with_capacity(corner_tets.len());
    for v in &corner_tets {
      let n = v.map(|k| used[k as usize]);
      let mut t = [0u32; 10];
      t[..4].copy_from_slice(&n);
      for (e, [i, j]) in TET_EDGES.iter().enumerate() {
        let (a, b) = (n[*i], n[*j]);
        let key = if a < b { (a, b) } else { (b, a) };
        let id = *mids.entry(key).or_insert_with(|| {
          nodes.push(mid(nodes[a as usize], nodes[b as usize]));
          nodes.len() as u32 - 1
        });
        t[4 + e] = id;
      }
      out.push(t);
    }
    // boundary: faces used by one tetrahedron
    let mut count: Map<[u32; 3], (u32, [u32; 3])> = Map::default();
    let mut seen_twice: Set<[u32; 3]> = Set::default();
    for (ti, t) in out.iter().enumerate() {
      for f in FACES {
        let tri = [t[f[0]], t[f[1]], t[f[2]]];
        let mut k = tri;
        k.sort();
        if count.remove(&k).is_some() {
          seen_twice.insert(k);
        } else if !seen_twice.contains(&k) {
          count.insert(k, (ti as u32, tri));
        }
      }
    }
    let mut faces: Vec<(u32, [u32; 3])> = count.into_values().collect();
    faces.sort();
    let mut boundary = Vec::with_capacity(faces.len());
    let mut boundary_face = Vec::with_capacity(faces.len());
    for (_, tri) in faces {
      let mut b = [0u32; 6];
      b[..3].copy_from_slice(&tri);
      for (e, [i, j]) in TRI_EDGES.iter().enumerate() {
        let (a, c) = (tri[*i], tri[*j]);
        b[3 + e] = mids[&if a < c { (a, c) } else { (c, a) }];
      }
      boundary.push(b);
      boundary_face.push(face_of(tri.map(|k| back[k as usize])));
    }
    TetMesh { nodes, corners, tets: out, boundary, boundary_face }
  }

  /// A box split into `n` cells along each side, six tetrahedra per cell, faces numbered as in
  /// [`crate::surface::box_surface`]: a structured mesh for checks.
  pub fn structured_box(lo: V3, hi: V3, n: [usize; 3]) -> TetMesh {
    let idx = |i: usize, j: usize, k: usize| (i + (n[0] + 1) * (j + (n[1] + 1) * k)) as u32;
    let mut pts = Vec::new();
    for k in 0..=n[2] {
      for j in 0..=n[1] {
        for i in 0..=n[0] {
          pts.push([
            lo[0] + (hi[0] - lo[0]) * i as f64 / n[0] as f64,
            lo[1] + (hi[1] - lo[1]) * j as f64 / n[1] as f64,
            lo[2] + (hi[2] - lo[2]) * k as f64 / n[2] as f64,
          ]);
        }
      }
    }
    let mut tets = Vec::new();
    for k in 0..n[2] {
      for j in 0..n[1] {
        for i in 0..n[0] {
          let c = |d: usize| idx(i + (d & 1), j + ((d >> 1) & 1), k + ((d >> 2) & 1));
          // Kuhn split along the 0–7 diagonal
          for path in [[1, 3], [1, 5], [2, 3], [2, 6], [4, 5], [4, 6]] {
            tets.push([c(0), c(path[0]), c(path[1]), c(7)]);
          }
        }
      }
    }
    let p = pts.clone();
    let face_of = move |tri: [u32; 3]| {
      let q = tri.map(|k| p[k as usize]);
      let eps = 1e-9 * (dist(lo, hi) + 1.0);
      for (ax, f) in [(0, 1), (1, 3), (2, 5)] {
        if q.iter().all(|v| (v[ax] - lo[ax]).abs() < eps) {
          return f;
        }
        if q.iter().all(|v| (v[ax] - hi[ax]).abs() < eps) {
          return f + 1;
        }
      }
      0
    };
    TetMesh::from_linear(&pts, &tets, &face_of)
  }
}

/// The size field: the global size, shrunk near refined faces.
pub struct SizeField {
  global: f64,
  grading: f64,
  near: Vec<(f64, KdTree)>,
}

impl SizeField {
  pub fn new(surface: &SurfaceMesh, opts: &MeshOptions) -> SizeField {
    let mut near = Vec::new();
    for r in &opts.refine {
      if !(r.size > 0.0) || r.size >= opts.size {
        continue;
      }
      // sample the faces' triangles at about half the local size
      let mut samples = Vec::new();
      for (t, tri) in surface.tris.iter().enumerate() {
        if !r.faces.contains(&surface.tri_face[t]) {
          continue;
        }
        let [a, b, c] = tri.map(|i| surface.points[i as usize]);
        let l = dist(a, b).max(dist(b, c)).max(dist(c, a));
        let n = ((l / (r.size * 0.5)).ceil() as usize).clamp(1, 200);
        for i in 0..=n {
          for j in 0..=n - i {
            let (u, v) = (i as f64 / n as f64, j as f64 / n as f64);
            samples.push(add(a, add(scale(sub(b, a), u), scale(sub(c, a), v))));
          }
        }
      }
      if !samples.is_empty() {
        near.push((r.size, KdTree::new(&samples)));
      }
    }
    SizeField { global: opts.size, grading: opts.grading.clamp(0.05, 5.0), near }
  }

  pub fn at(&self, p: V3) -> f64 {
    let mut h = self.global;
    for (s, tree) in &self.near {
      if let Some((_, d)) = tree.nearest(p) {
        h = h.min(s + self.grading * (d - s * 0.5).max(0.0));
      }
    }
    h
  }

  pub fn min(&self) -> f64 {
    self.near.iter().map(|n| n.0).fold(self.global, f64::min)
  }
}

/// Interior points: octree leaves' corners and centres, inside the solid and away from its
/// surface.
fn interior_points(surface: &SurfaceMesh, size: &SizeField) -> Vec<V3> {
  let (lo, hi) = bbox(&surface.points);
  let h0 = size.global;
  let depth = ((h0 / size.min()).log2().ceil().max(0.0) as u32 + 1).min(10);
  let unit = h0 / f64::from(1u32 << (depth + 1));
  let n = [0, 1, 2].map(|k| ((hi[k] - lo[k]) / h0).ceil().max(1.0) as i64);
  let origin = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0 - n[k] as f64 * h0 / 2.0);
  let mut keys: Set<[i64; 3]> = Set::default();
  let mut stack: Vec<([i64; 3], u32)> = Vec::new();
  let root = 1i64 << (depth + 1);
  for i in 0..n[0] {
    for j in 0..n[1] {
      for k in 0..n[2] {
        stack.push(([i * root, j * root, k * root], 0));
      }
    }
  }
  let to_p = |q: [i64; 3]| [0, 1, 2].map(|k| origin[k] + q[k] as f64 * unit);
  while let Some((c, level)) = stack.pop() {
    let side = root >> level;
    let half = side / 2;
    let centre = [c[0] + half, c[1] + half, c[2] + half];
    let s = side as f64 * unit;
    if level < depth && size.at(to_p(centre)) < s * 0.75 {
      for d in 0..8 {
        stack.push(([c[0] + (d & 1) * half, c[1] + ((d >> 1) & 1) * half, c[2] + ((d >> 2) & 1) * half], level + 1));
      }
      continue;
    }
    keys.insert(centre);
    for d in 0..8 {
      keys.insert([c[0] + (d & 1) * side, c[1] + ((d >> 1) & 1) * side, c[2] + ((d >> 2) & 1) * side]);
    }
  }
  let inside = InsideTest::new(&surface.points, &surface.tris);
  let near = KdTree::new(&surface.points);
  let mut keys: Vec<[i64; 3]> = keys.into_iter().collect();
  keys.sort();
  keys
    .into_iter()
    .map(to_p)
    .filter(|&p| {
      let h = size.at(p);
      near.nearest(p).is_some_and(|(_, d)| d > 0.7 * h) && inside.inside(p)
    })
    .collect()
}

/// Spreads points in Morton order so each insertion starts its walk close by.
fn morton_order(points: &[V3]) -> Vec<usize> {
  let (lo, hi) = bbox(points);
  let ext = (0..3).map(|k| hi[k] - lo[k]).fold(1e-12, f64::max);
  let spread = |x: u64| {
    let mut x = x & 0x1f_ffff;
    x = (x | x << 32) & 0x1f00000000ffff;
    x = (x | x << 16) & 0x1f0000ff0000ff;
    x = (x | x << 8) & 0x100f00f00f00f00f;
    x = (x | x << 4) & 0x10c30c30c30c30c3;
    (x | x << 2) & 0x1249249249249249
  };
  let mut keyed: Vec<(u64, usize)> = points
    .iter()
    .enumerate()
    .map(|(i, p)| {
      let q = [0, 1, 2].map(|k| (((p[k] - lo[k]) / ext) * 2_000_000.0) as u64);
      (spread(q[0]) | spread(q[1]) << 1 | spread(q[2]) << 2, i)
    })
    .collect();
  keyed.sort_unstable();
  keyed.into_iter().map(|k| k.1).collect()
}

/// Meshes a closed surface into quadratic tetrahedra.
pub fn mesh_solid(surface: &SurfaceMesh, opts: &MeshOptions) -> Result<(TetMesh, MeshStats), String> {
  if surface.tris.len() < 4 {
    return Err("the body has no surface to mesh".into());
  }
  if !surface.is_closed() {
    return Err(format!(
      "the body's surface isn't closed ({} open edges): only solids can be meshed",
      surface.open_edges()
    ));
  }
  if !(opts.size > 0.0) || !opts.size.is_finite() {
    return Err("the mesh size must be positive".into());
  }
  let size = SizeField::new(surface, opts);
  let mut surf = surface.clone();
  surf.remesh(&|p| size.at(p));
  let (lo, hi) = bbox(&surf.points);
  let diag = dist(lo, hi);
  let interior = interior_points(&surf, &size);
  let estimate = surf.points.len() + interior.len();
  if estimate > 3_000_000 {
    return Err(format!("the mesh would have about {estimate} corner nodes: use a larger mesh size"));
  }
  // every point, surface ones first; the triangulation sees them jittered very slightly
  let mut all: Vec<V3> = surf.points.clone();
  let ns0 = all.len();
  all.extend(interior.iter().copied());
  let mut rng = 0x2545_F491_4F6C_DD1Du64;
  let mut jitter = |p: V3| {
    let mut r = || {
      rng ^= rng << 13;
      rng ^= rng >> 7;
      rng ^= rng << 17;
      (rng as f64 / u64::MAX as f64) * 2.0 - 1.0
    };
    let j = diag * 1e-9;
    [p[0] + r() * j, p[1] + r() * j, p[2] + r() * j]
  };
  let mut dt = Delaunay::new(sub(lo, [diag; 3]), add(hi, [diag; 3]));
  // dt vertex of each point in `all`
  let mut vert = vec![u32::MAX; all.len()];
  let mut owner: Vec<u32> = vec![u32::MAX; 4];
  let eps = diag * 1e-12;
  for i in morton_order(&all) {
    if let Some(v) = dt.insert(jitter(all[i]), eps) {
      vert[i] = v;
      owner.push(i as u32);
    }
  }
  // surface point s is all[surf_all[s]]
  let mut surf_all: Vec<u32> = (0..ns0 as u32).collect();
  let mut unrecovered = 0;
  for _round in 0..12 {
    let mut edges: Set<(u32, u32)> = Set::with_capacity_and_hasher(dt.tets.len() * 7, Default::default());
    let mut faces: Set<[u32; 3]> = Set::with_capacity_and_hasher(dt.tets.len() * 4, Default::default());
    for (t, v) in dt.tets.iter().enumerate() {
      if dt.dead[t] {
        continue;
      }
      for i in 0..4 {
        for j in i + 1..4 {
          edges.insert(if v[i] < v[j] { (v[i], v[j]) } else { (v[j], v[i]) });
        }
        let f = FACES[i];
        let mut k = [v[f[0]], v[f[1]], v[f[2]]];
        k.sort();
        faces.insert(k);
      }
    }
    let dv = |s: u32| vert[surf_all[s as usize] as usize];
    let mut split: Vec<(u32, u32)> = Vec::new();
    let mut seen: Set<(u32, u32)> = Set::default();
    for tri in &surf.tris {
      let mut missing_edge = false;
      for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        let (va, vb) = (dv(a), dv(b));
        if va == u32::MAX || vb == u32::MAX {
          continue;
        }
        let key = if va < vb { (va, vb) } else { (vb, va) };
        if !edges.contains(&key) {
          missing_edge = true;
          if seen.insert(if a < b { (a, b) } else { (b, a) }) {
            split.push((a, b));
          }
        }
      }
      if !missing_edge {
        let mut k = tri.map(dv);
        if k.contains(&u32::MAX) {
          continue;
        }
        k.sort();
        if !faces.contains(&k) {
          // split the triangle's longest edge
          let mut best = (0.0, 0, 0);
          for e in 0..3 {
            let (a, b) = (tri[e], tri[(e + 1) % 3]);
            let l = dist(surf.points[a as usize], surf.points[b as usize]);
            if l > best.0 {
              best = (l, a, b);
            }
          }
          let key = if best.1 < best.2 { (best.1, best.2) } else { (best.2, best.1) };
          if seen.insert(key) {
            split.push((best.1, best.2));
          }
        }
      }
    }
    // tiny pieces (where a curved face's facets fold) aren't worth chasing: what's left is
    // classified by element centroids and only shifts the boundary by a sliver
    split.retain(|&(a, b)| {
      let (pa, pb) = (surf.points[a as usize], surf.points[b as usize]);
      dist(pa, pb) > 0.15 * size.at(mid(pa, pb))
    });
    unrecovered = split.len();
    if split.is_empty() {
      break;
    }
    let has = |c: u32, d: u32| {
      let (Some(&ac), Some(&ad)) = (surf_all.get(c as usize), surf_all.get(d as usize)) else { return false };
      let (vc, vd) = (vert[ac as usize], vert[ad as usize]);
      vc != u32::MAX && vd != u32::MAX && edges.contains(&if vc < vd { (vc, vd) } else { (vd, vc) })
    };
    let before = surf.points.len();
    surf.flip_or_split(&split, &has);
    // splits leave skinny triangles on the faces next to them, whose big circumspheres would go
    // missing next: refine those too
    surf.refine_delaunay(&|p| size.at(p));
    surf.split_encroached(&|p| size.at(p));
    for m in before..surf.points.len() {
      let p = surf.points[m];
      all.push(p);
      let ai = all.len() as u32 - 1;
      surf_all.push(ai);
      let v = dt.insert(jitter(p), eps).unwrap_or(u32::MAX);
      vert.push(v);
      if v != u32::MAX {
        owner.push(ai);
      }
    }
  }
  // keep the tetrahedra inside the solid
  let inside = InsideTest::new(&surf.points, &surf.tris);
  let on_surface: Vec<bool> = {
    let mut s = vec![false; all.len()];
    for &a in &surf_all {
      s[a as usize] = true;
    }
    s
  };
  let mut keep: Vec<[u32; 4]> = Vec::new();
  for (_, v) in dt.tets() {
    let p = v.map(|x| all[owner[x as usize] as usize]);
    let c = scale(add(add(p[0], p[1]), add(p[2], p[3])), 0.25);
    if inside.inside(c) {
      keep.push(v.map(|x| owner[x as usize]));
    }
  }
  let point_faces_all: Map<u32, Vec<u32>> =
    surf.point_faces().into_iter().enumerate().map(|(k, f)| (surf_all[k], f)).collect();
  // peel flat tetrahedra off the surface
  for _ in 0..20 {
    let mut count: Map<[u32; 3], u32> = Map::default();
    for t in &keep {
      for f in FACES {
        let mut k = [t[f[0]], t[f[1]], t[f[2]]];
        k.sort();
        *count.entry(k).or_insert(0) += 1;
      }
    }
    let before = keep.len();
    keep.retain(|t| {
      let p = t.map(|x| all[x as usize]);
      let q = tet_quality(p[0], p[1], p[2], p[3]);
      if q > 0.02 {
        return true;
      }
      // only tetrahedra flat on one face come off (others would leave a dent)
      let surface_only = t.iter().all(|&x| on_surface[x as usize]) && {
        let lists: Vec<&Vec<u32>> = t.iter().filter_map(|x| point_faces_all.get(x)).collect();
        lists.len() == 4 && lists[0].iter().any(|f| lists.iter().all(|l| l.contains(f)))
      };
      let exposed = FACES.iter().any(|f| {
        let mut k = [t[f[0]], t[f[1]], t[f[2]]];
        k.sort();
        count[&k] == 1
      });
      !(surface_only && exposed)
    });
    if keep.len() == before {
      break;
    }
  }
  if keep.is_empty() {
    return Err("meshing made no elements: the mesh size may be too large for the body".into());
  }
  improve(&mut all, &mut keep, &on_surface, 6);
  // what is still flat can't carry load: drop it if it's on the surface (flat on a face)
  keep.retain(|t| {
    let p = t.map(|x| all[x as usize]);
    tet_volume(p[0], p[1], p[2], p[3]).abs() > 1e-12 * diag.powi(3) || !t.iter().all(|&x| on_surface[x as usize])
  });
  let mut tri_faces: Map<[u32; 3], u32> = Map::with_capacity_and_hasher(surf.tris.len(), Default::default());
  for (t, tri) in surf.tris.iter().enumerate() {
    let mut k = tri.map(|s| surf_all[s as usize]);
    k.sort();
    tri_faces.insert(k, surf.tri_face[t]);
  }
  let mut point_faces: Map<u32, Vec<u32>> = Map::default();
  for (s, faces) in surf.point_faces().into_iter().enumerate() {
    point_faces.insert(surf_all[s], faces);
  }
  let face_of = |tri: [u32; 3]| {
    let mut k = tri;
    k.sort();
    if let Some(f) = tri_faces.get(&k) {
      return *f;
    }
    let lists: Vec<&Vec<u32>> = tri.iter().filter_map(|v| point_faces.get(v)).collect();
    if let Some(first) = lists.first() {
      if let Some(f) = first.iter().find(|f| lists.iter().all(|l| l.contains(f))) {
        return *f;
      }
      if let Some(f) = first.first() {
        return *f;
      }
    }
    0
  };
  let mesh = TetMesh::from_linear(&all, &keep, &face_of);
  let mut stats = mesh.stats();
  stats.unrecovered = unrecovered;
  Ok((mesh, stats))
}

/// Improves the worst tetrahedra in place: 2–3 and 3–2 flips where they raise the smallest
/// shape quality around them, then interior points moved towards the middle of their neighbours
/// where that helps. The boundary (triangles and points flagged `fixed`) doesn't change.
pub fn improve(points: &mut [V3], tets: &mut Vec<[u32; 4]>, fixed: &[bool], passes: usize) {
  let q = |p: &[V3], t: &[u32; 4]| {
    let x = t.map(|v| p[v as usize]);
    tet_quality(x[0], x[1], x[2], x[3])
  };
  // positive orientation throughout
  for t in tets.iter_mut() {
    let x = t.map(|v| points[v as usize]);
    if tet_volume(x[0], x[1], x[2], x[3]) < 0.0 {
      t.swap(0, 1);
    }
  }
  let face_key = |t: &[u32; 4], i: usize| {
    let f = FACES[i];
    let mut k = [t[f[0]], t[f[1]], t[f[2]]];
    k.sort();
    k
  };
  for _pass in 0..passes {
    let mut dead = vec![false; tets.len()];
    let mut faces: Map<[u32; 3], Vec<u32>> = Map::with_capacity_and_hasher(tets.len() * 2, Default::default());
    for (ti, t) in tets.iter().enumerate() {
      for i in 0..4 {
        faces.entry(face_key(t, i)).or_default().push(ti as u32);
      }
    }
    let mut bad: Vec<(f64, u32)> =
      tets.iter().enumerate().map(|(i, t)| (q(points, t), i as u32)).filter(|x| x.0 < 0.25).collect();
    bad.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut changed = 0;
    let mut new_tets: Vec<[u32; 4]> = Vec::new();
    for &(_, ti) in &bad {
      if dead[ti as usize] {
        continue;
      }
      let t = tets[ti as usize];
      let old = q(points, &t);
      // 2–3: through each interior face
      let mut done = false;
      for i in 0..4 {
        let k = face_key(&t, i);
        let Some(list) = faces.get(&k) else { continue };
        if list.iter().filter(|&&o| !dead[o as usize]).count() != 2 {
          continue;
        }
        let Some(&o) = list.iter().find(|&&o| o != ti && !dead[o as usize]) else { continue };
        let ot = get(tets, &new_tets, o);
        let d = t[i];
        let Some(&e) = ot.iter().find(|v| !k.contains(v)) else { continue };
        let f = FACES[i];
        // the face wound outward from t, and the three tetrahedra around d–e replacing the two
        let (a, b, c) = (t[f[0]], t[f[1]], t[f[2]]);
        let made = [[a, b, e, d], [b, c, e, d], [c, a, e, d]];
        let before = old.min(q(points, &ot));
        let vols_ok = made.iter().all(|m| {
          let x = m.map(|v| points[v as usize]);
          tet_volume(x[0], x[1], x[2], x[3]) > 0.0
        });
        if !vols_ok {
          continue;
        }
        let after = made.iter().map(|m| q(points, m)).fold(f64::INFINITY, f64::min);
        if after > before + 1e-6 {
          kill_and_add(tets, &mut new_tets, &mut dead, &mut faces, &[ti, o], &made, &face_key);
          changed += 1;
          done = true;
          break;
        }
      }
      if done {
        continue;
      }
      // 3–2: remove an edge with exactly three tetrahedra around it
      for (i, j) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
        let (d, e) = (t[i], t[j]);
        let Some(ring) = edge_ring(tets, &new_tets, &dead, &faces, ti, d, e) else { continue };
        if ring.len() != 3 {
          continue;
        }
        let others: Vec<u32> = ring
          .iter()
          .flat_map(|&r| get(tets, &new_tets, r))
          .filter(|v| *v != d && *v != e)
          .collect::<std::collections::BTreeSet<u32>>()
          .into_iter()
          .collect();
        if others.len() != 3 {
          continue;
        }
        let (a, b, c) = (others[0], others[1], others[2]);
        // d and e must be on opposite sides of a b c
        let x = [a, b, c].map(|v| points[v as usize]);
        let sd = tet_volume(x[0], x[1], x[2], points[d as usize]);
        let se = tet_volume(x[0], x[1], x[2], points[e as usize]);
        if sd * se >= 0.0 {
          continue;
        }
        let made = if sd > 0.0 { [[a, b, c, d], [b, a, c, e]] } else { [[b, a, c, d], [a, b, c, e]] };
        let before = ring.iter().map(|&r| q(points, &get(tets, &new_tets, r))).fold(f64::INFINITY, f64::min);
        let after = made.iter().map(|m| q(points, m)).fold(f64::INFINITY, f64::min);
        if after > before + 1e-6 {
          kill_and_add(tets, &mut new_tets, &mut dead, &mut faces, &ring, &made, &face_key);
          changed += 1;
          break;
        }
      }
    }
    let mut kept: Vec<[u32; 4]> =
      tets.iter().chain(new_tets.iter()).enumerate().filter(|(i, _)| !dead[*i]).map(|(_, t)| *t).collect();
    std::mem::swap(tets, &mut kept);
    // smoothing of free points
    let mut around: Vec<Vec<u32>> = vec![Vec::new(); points.len()];
    for (ti, t) in tets.iter().enumerate() {
      for &v in t {
        around[v as usize].push(ti as u32);
      }
    }
    let mut moved = 0;
    for v in 0..points.len() {
      if fixed.get(v).copied().unwrap_or(true) || around[v].is_empty() {
        continue;
      }
      let ts = &around[v];
      let worst = ts.iter().map(|&t| q(points, &tets[t as usize])).fold(f64::INFINITY, f64::min);
      if worst > 0.4 {
        continue;
      }
      let mut nb: Vec<u32> = ts.iter().flat_map(|&t| tets[t as usize]).filter(|&x| x as usize != v).collect();
      nb.sort();
      nb.dedup();
      let c = scale(nb.iter().fold([0.0; 3], |s, &w| add(s, points[w as usize])), 1.0 / nb.len() as f64);
      let old = points[v];
      let mut best = (worst, old);
      for f in [1.0, 0.5, 0.25] {
        points[v] = add(old, scale(sub(c, old), f));
        let w = ts.iter().map(|&t| q(points, &tets[t as usize])).fold(f64::INFINITY, f64::min);
        if w > best.0 {
          best = (w, points[v]);
        }
      }
      points[v] = best.1;
      if best.1 != old {
        moved += 1;
      }
    }
    if changed == 0 && moved == 0 {
      break;
    }
  }
}

fn get(tets: &[[u32; 4]], new: &[[u32; 4]], i: u32) -> [u32; 4] {
  if (i as usize) < tets.len() { tets[i as usize] } else { new[i as usize - tets.len()] }
}

fn kill_and_add(
  tets: &[[u32; 4]],
  new: &mut Vec<[u32; 4]>,
  dead: &mut Vec<bool>,
  faces: &mut Map<[u32; 3], Vec<u32>>,
  kill: &[u32],
  add: &[[u32; 4]],
  key: &dyn Fn(&[u32; 4], usize) -> [u32; 3],
) {
  for &k in kill {
    dead[k as usize] = true;
    let t = get(tets, new, k);
    for i in 0..4 {
      if let Some(l) = faces.get_mut(&key(&t, i)) {
        l.retain(|x| *x != k);
      }
    }
  }
  for t in add {
    let id = (tets.len() + new.len()) as u32;
    new.push(*t);
    dead.push(false);
    for i in 0..4 {
      faces.entry(key(t, i)).or_default().push(id);
    }
  }
}

/// The live tetrahedra around edge d–e, starting from `start`; `None` on the boundary.
fn edge_ring(
  tets: &[[u32; 4]],
  new: &[[u32; 4]],
  dead: &[bool],
  faces: &Map<[u32; 3], Vec<u32>>,
  start: u32,
  d: u32,
  e: u32,
) -> Option<Vec<u32>> {
  let mut ring = vec![start];
  let t0 = get(tets, new, start);
  let others: Vec<u32> = t0.iter().copied().filter(|v| *v != d && *v != e).collect();
  let (first, mut cur_v) = (others[0], others[1]);
  let mut cur = start;
  for _ in 0..64 {
    let mut k = [d, e, cur_v];
    k.sort();
    let list = faces.get(&k)?;
    let next = *list.iter().find(|&&o| o != cur && !dead[o as usize])?;
    if next == start {
      return Some(ring);
    }
    let nt = get(tets, new, next);
    let w = *nt.iter().find(|v| **v != d && **v != e && **v != cur_v)?;
    ring.push(next);
    cur = next;
    cur_v = w;
    if cur_v == first {
      let mut k = [d, e, first];
      k.sort();
      return faces.get(&k)?.contains(&start).then_some(ring);
    }
  }
  None
}
