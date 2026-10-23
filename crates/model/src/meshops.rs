//! Mesh tools for faceted bodies: welding, reduction (vertex clustering) and smoothing.

use std::collections::HashMap;

use fox_kernel::TriMesh;
use fox_kernel::glam::DVec3;

/// Shared vertices (a tessellation repeats them per face) and triangles indexing them.
pub fn weld(m: &TriMesh) -> (Vec<DVec3>, Vec<u32>) {
  let mut index: HashMap<[i64; 3], u32> = HashMap::new();
  let mut pos = Vec::new();
  let q = |v: f32| (v as f64 * 1e5).round() as i64;
  let mut map = Vec::with_capacity(m.positions.len());
  for p in &m.positions {
    let key = [q(p[0]), q(p[1]), q(p[2])];
    let i = *index.entry(key).or_insert_with(|| {
      pos.push(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
      (pos.len() - 1) as u32
    });
    map.push(i);
  }
  let tris = m.indices.iter().map(|&i| map.get(i as usize).copied().unwrap_or(0)).collect();
  (pos, drop_degenerate(tris))
}

fn drop_degenerate(tris: Vec<u32>) -> Vec<u32> {
  tris.chunks_exact(3).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).flatten().copied().collect()
}

/// Keeps about `keep` (0–1) of the triangles by clustering vertices on a grid.
pub fn reduce(pos: &[DVec3], tris: &[u32], keep: f64) -> (Vec<DVec3>, Vec<u32>) {
  if keep >= 0.999 || pos.is_empty() {
    return (pos.to_vec(), tris.to_vec());
  }
  let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
  for p in pos {
    lo = lo.min(*p);
    hi = hi.max(*p);
  }
  let size = (hi - lo).max_element().max(1e-9);
  // vertices scale roughly with the square of the grid resolution on a surface
  let target = ((pos.len() as f64) * keep).max(8.0);
  let mut cells = (target.sqrt() * 2.0).max(2.0);
  let mut out = (pos.to_vec(), tris.to_vec());
  for _ in 0..12 {
    out = cluster(pos, tris, lo, size / cells);
    if (out.1.len() as f64) <= tris.len() as f64 * keep * 1.15 {
      break;
    }
    cells *= 0.8;
  }
  out
}

/// Merges the vertices in each cube of a grid (side `cell`, corner `lo`) into their average.
fn cluster(pos: &[DVec3], tris: &[u32], lo: DVec3, cell: f64) -> (Vec<DVec3>, Vec<u32>) {
  let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
  let mut sums: Vec<(DVec3, f64)> = Vec::new();
  let map: Vec<usize> = pos
    .iter()
    .map(|p| {
      let k = ((*p - lo) / cell).floor();
      let key = [k.x as i64, k.y as i64, k.z as i64];
      let i = *ids.entry(key).or_insert_with(|| {
        sums.push((DVec3::ZERO, 0.0));
        sums.len() - 1
      });
      sums[i].0 += *p;
      sums[i].1 += 1.0;
      i
    })
    .collect();
  let new_pos: Vec<DVec3> = sums.iter().map(|(s, n)| *s / *n).collect();
  let new_tris = drop_degenerate(tris.iter().map(|&i| map[i as usize] as u32).collect());
  (new_pos, new_tris)
}

/// Laplacian smoothing (each pass moves vertices halfway to their neighbours' average).
pub fn smooth(pos: &[DVec3], tris: &[u32], passes: usize) -> Vec<DVec3> {
  let mut nb: Vec<Vec<u32>> = vec![Vec::new(); pos.len()];
  for t in tris.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      if !nb[a as usize].contains(&b) {
        nb[a as usize].push(b);
      }
      if !nb[b as usize].contains(&a) {
        nb[b as usize].push(a);
      }
    }
  }
  let mut p = pos.to_vec();
  for _ in 0..passes {
    let prev = p.clone();
    for (i, n) in nb.iter().enumerate() {
      if n.is_empty() {
        continue;
      }
      let avg = n.iter().map(|&j| prev[j as usize]).sum::<DVec3>() / n.len() as f64;
      p[i] = prev[i] * 0.5 + avg * 0.5;
    }
  }
  p
}

/// Remesh's settings (Fusion's MESH > MODIFY > Remesh).
#[derive(Debug, Clone, Copy)]
pub struct Remesh {
  /// The edge length to aim for.
  pub target: f64,
  /// Adaptive keeps flat regions coarse and refines where the surface bends; Uniform makes every
  /// edge about `target` long.
  pub adaptive: bool,
  /// 0–1: how little the relaxation that evens the triangles out may move vertices.
  pub shape: f64,
  /// Vertices on sharp edges stay where they are.
  pub sharp: bool,
  /// Vertices on open boundaries stay where they are.
  pub boundaries: bool,
}

/// Fusion's Density (0–1, higher is more faces) as an edge length for a body `diagonal` across.
pub fn density_edge(diagonal: f64, density: f64) -> f64 {
  diagonal.max(1e-9) / (4.0 + 60.0 * density.clamp(0.0, 1.0))
}

/// Regenerates the triangles to edges about `opts.target` long: a dense mesh is coarsened by
/// clustering first, long edges are split (conformingly) until none is left, then the vertices
/// relax in their tangent planes.
pub fn remesh(pos: &[DVec3], tris: &[u32], opts: &Remesh) -> (Vec<DVec3>, Vec<u32>) {
  let target = opts.target.max(1e-6);
  let (mut pos, mut tris) = (pos.to_vec(), tris.to_vec());
  if pos.is_empty() || tris.is_empty() {
    return (pos, tris);
  }
  let edges = edge_lengths(&pos, &tris);
  let mean = edges.iter().sum::<f64>() / edges.len().max(1) as f64;
  // coarsen: denser than asked
  if !opts.adaptive && mean < 0.5 * target {
    let lo = pos.iter().fold(DVec3::splat(f64::INFINITY), |a, p| a.min(*p));
    let (p, t) = cluster(&pos, &tris, lo, 0.7 * target);
    if !t.is_empty() {
      (pos, tris) = (p, t);
    }
  }
  for _ in 0..12 {
    if tris.len() / 3 > 400_000 {
      break;
    }
    let normals = face_normals(&pos, &tris);
    let flat = flat_edges(&tris, &normals);
    let long = |a: u32, b: u32| {
      let l = (pos[a as usize] - pos[b as usize]).length();
      // adaptive: flat regions may keep edges three times as long
      let lim = if opts.adaptive && flat.contains(&key(a, b)) { 4.0 } else { 4.0 / 3.0 };
      l > lim * target
    };
    let marked: Vec<(u32, u32)> = tris
      .chunks_exact(3)
      .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
      .filter(|&(a, b)| long(a, b))
      .map(|(a, b)| key(a, b))
      .collect();
    if marked.is_empty() {
      break;
    }
    tris = split_edges(&mut pos, &tris, &marked);
  }
  let passes = ((1.0 - opts.shape.clamp(0.0, 1.0)) * 6.0).round() as usize;
  if passes > 0 {
    let normals = face_normals(&pos, &tris);
    let boundary: Vec<(u32, u32)> = boundary_edges(&tris).into_iter().map(|(a, b)| key(a, b)).collect();
    let sharp = sharp_edges(&tris, &normals);
    let mut fixed = vec![false; pos.len()];
    let mut features: Vec<Vec<u32>> = vec![Vec::new(); pos.len()];
    for (edges, pinned) in [(&boundary, opts.boundaries), (&sharp, opts.sharp)] {
      for &(a, b) in edges {
        fixed[a as usize] |= pinned;
        fixed[b as usize] |= pinned;
        for (u, w) in [(a, b), (b, a)] {
          if !features[u as usize].contains(&w) {
            features[u as usize].push(w);
          }
        }
      }
    }
    pos = relax(&pos, &tris, &fixed, &features, passes, 0.5 * (1.0 - opts.shape.clamp(0.0, 1.0)));
  }
  (pos, drop_degenerate(tris))
}

fn key(a: u32, b: u32) -> (u32, u32) {
  (a.min(b), a.max(b))
}

fn edge_lengths(pos: &[DVec3], tris: &[u32]) -> Vec<f64> {
  tris
    .chunks_exact(3)
    .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
    .map(|(a, b)| (pos[a as usize] - pos[b as usize]).length())
    .collect()
}

/// Unit normals of the triangles (zero for slivers).
fn face_normals(pos: &[DVec3], tris: &[u32]) -> Vec<DVec3> {
  tris
    .chunks_exact(3)
    .map(|t| {
      let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
      (b - a).cross(c - a).normalize_or_zero()
    })
    .collect()
}

/// The triangles on each undirected edge.
fn edge_faces(tris: &[u32]) -> HashMap<(u32, u32), Vec<usize>> {
  let mut m: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
  for (f, t) in tris.chunks_exact(3).enumerate() {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      m.entry(key(a, b)).or_default().push(f);
    }
  }
  m
}

/// Edges whose two triangles lie within 10 degrees of each other.
fn flat_edges(tris: &[u32], normals: &[DVec3]) -> std::collections::HashSet<(u32, u32)> {
  edge_faces(tris)
    .into_iter()
    .filter(|(_, f)| f.len() == 2 && normals[f[0]].dot(normals[f[1]]) > 0.985)
    .map(|(k, _)| k)
    .collect()
}

/// Edges whose two triangles meet at more than 30 degrees.
fn sharp_edges(tris: &[u32], normals: &[DVec3]) -> Vec<(u32, u32)> {
  edge_faces(tris)
    .into_iter()
    .filter(|(_, f)| f.len() == 2 && normals[f[0]].dot(normals[f[1]]) < 0.866)
    .map(|(k, _)| k)
    .collect()
}

/// Edges on one triangle only: the outline of an open mesh, as they run in that triangle.
pub fn boundary_edges(tris: &[u32]) -> Vec<(u32, u32)> {
  let faces = edge_faces(tris);
  tris
    .chunks_exact(3)
    .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
    .filter(|&(a, b)| faces.get(&key(a, b)).is_some_and(|f| f.len() == 1))
    .collect()
}

/// Splits every `marked` edge at its middle and the triangles around it to match, so the mesh
/// stays watertight: a triangle with one split edge becomes two, with two three, with three four.
fn split_edges(pos: &mut Vec<DVec3>, tris: &[u32], marked: &[(u32, u32)]) -> Vec<u32> {
  let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
  for &(a, b) in marked {
    mid.entry((a, b)).or_insert_with(|| {
      let m = (pos[a as usize] + pos[b as usize]) / 2.0;
      pos.push(m);
      (pos.len() - 1) as u32
    });
  }
  let mut out = Vec::with_capacity(tris.len() * 2);
  for t in tris.chunks_exact(3) {
    let v = [t[0], t[1], t[2]];
    let e: [Option<u32>; 3] = [0, 1, 2].map(|i| mid.get(&key(v[i], v[(i + 1) % 3])).copied());
    match e.iter().filter(|x| x.is_some()).count() {
      0 => out.extend(v),
      1 => {
        let i = e.iter().position(Option::is_some).unwrap_or(0);
        let (a, b, c, m) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3], e[i].unwrap_or(0));
        out.extend([a, m, c, m, b, c]);
      }
      2 => {
        // the unsplit edge runs a → b
        let i = e.iter().position(Option::is_none).unwrap_or(0);
        let (a, b, c) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
        let (mbc, mca) = (e[(i + 1) % 3].unwrap_or(0), e[(i + 2) % 3].unwrap_or(0));
        out.extend([mca, mbc, c, a, b, mbc, a, mbc, mca]);
      }
      _ => {
        let [m0, m1, m2] = e.map(|x| x.unwrap_or(0));
        out.extend([v[0], m0, m2, m0, v[1], m1, m2, m1, v[2], m0, m1, m2]);
      }
    }
  }
  out
}

/// Tangential smoothing: each pass moves the free vertices `lambda` of the way to their
/// neighbours' average, within the plane through them across their normal. A vertex on a
/// crease or an open boundary (two `features` neighbours) only slides along it, and one where
/// such lines end or meet (a corner) stays put, so the body keeps its shape.
fn relax(pos: &[DVec3], tris: &[u32], fixed: &[bool], features: &[Vec<u32>], passes: usize, lambda: f64) -> Vec<DVec3> {
  let mut nb: Vec<Vec<u32>> = vec![Vec::new(); pos.len()];
  for t in tris.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      if !nb[a as usize].contains(&b) {
        nb[a as usize].push(b);
      }
      if !nb[b as usize].contains(&a) {
        nb[b as usize].push(a);
      }
    }
  }
  let mut p = pos.to_vec();
  for _ in 0..passes {
    let normals = vertex_normals(&p, tris);
    let prev = p.clone();
    for (i, n) in nb.iter().enumerate() {
      if n.is_empty() || fixed[i] {
        continue;
      }
      match features[i].as_slice() {
        [] => {
          let avg = n.iter().map(|&j| prev[j as usize]).sum::<DVec3>() / n.len() as f64;
          let d = avg - prev[i];
          p[i] = prev[i] + (d - normals[i] * d.dot(normals[i])) * lambda;
        }
        &[a, b] => {
          let (a, b) = (prev[a as usize], prev[b as usize]);
          let along = (b - a).normalize_or_zero();
          let d = (a + b) / 2.0 - prev[i];
          p[i] = prev[i] + along * d.dot(along) * lambda;
        }
        _ => {}
      }
    }
  }
  p
}

/// Area-weighted vertex normals.
fn vertex_normals(pos: &[DVec3], tris: &[u32]) -> Vec<DVec3> {
  let mut n = vec![DVec3::ZERO; pos.len()];
  for t in tris.chunks_exact(3) {
    let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
    let f = (b - a).cross(c - a);
    for &i in t {
      n[i as usize] += f;
    }
  }
  n.iter().map(|v| v.normalize_or_zero()).collect()
}

/// The vertices moved `d` along their normals (negative: against them), far enough at corners
/// that every triangle around a vertex ends up `d` from where it was.
pub fn offset(pos: &[DVec3], tris: &[u32], d: f64) -> Vec<DVec3> {
  let normals = vertex_normals(pos, tris);
  let faces = face_normals(pos, tris);
  // the triangle around each vertex that leans furthest from its normal
  let mut lean = vec![1.0f64; pos.len()];
  for (f, t) in tris.chunks_exact(3).enumerate() {
    for &i in t {
      let c = normals[i as usize].dot(faces[f]);
      lean[i as usize] = lean[i as usize].min(c);
    }
  }
  pos.iter().enumerate().map(|(i, p)| *p + normals[i] * (d / lean[i].clamp(0.3, 1.0))).collect()
}

/// Mesh Shell of an open mesh: the surface, its copy offset `-d` (inward) turned inside out and
/// walls joining the two along the open boundary, as one closed mesh.
pub fn shell_open(pos: &[DVec3], tris: &[u32], d: f64) -> (Vec<DVec3>, Vec<u32>) {
  let n = pos.len() as u32;
  let mut out_pos = pos.to_vec();
  out_pos.extend(offset(pos, tris, -d));
  let mut out = tris.to_vec();
  for t in tris.chunks_exact(3) {
    out.extend([t[0] + n, t[2] + n, t[1] + n]);
  }
  for (a, b) in boundary_edges(tris) {
    // the wall runs b → a beside the surface's a → b
    out.extend([b, a, a + n, b, a + n, b + n]);
  }
  (out_pos, out)
}

/// The connected pieces of a mesh (Separate's mesh shells), each with its own vertices, largest
/// first.
pub fn components(pos: &[DVec3], tris: &[u32]) -> Vec<(Vec<DVec3>, Vec<u32>)> {
  let mut parent: Vec<u32> = (0..pos.len() as u32).collect();
  fn root(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
      parent[i as usize] = parent[parent[i as usize] as usize];
      i = parent[i as usize];
    }
    i
  }
  for t in tris.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2])] {
      let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
      if ra != rb {
        parent[ra as usize] = rb;
      }
    }
  }
  let mut groups: HashMap<u32, (Vec<DVec3>, Vec<u32>, HashMap<u32, u32>)> = HashMap::new();
  let mut order = Vec::new();
  for t in tris.chunks_exact(3) {
    let r = root(&mut parent, t[0]);
    let g = groups.entry(r).or_insert_with(|| {
      order.push(r);
      (Vec::new(), Vec::new(), HashMap::new())
    });
    for &i in t {
      let j = *g.2.entry(i).or_insert_with(|| {
        g.0.push(pos[i as usize]);
        (g.0.len() - 1) as u32
      });
      g.1.push(j);
    }
  }
  let mut out: Vec<(Vec<DVec3>, Vec<u32>)> =
    order.into_iter().filter_map(|r| groups.remove(&r)).map(|(p, t, _)| (p, t)).collect();
  out.sort_by_key(|(_, t)| std::cmp::Reverse(t.len()));
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  fn grid(n: usize) -> (Vec<DVec3>, Vec<u32>) {
    let mut pos = Vec::new();
    for j in 0..=n {
      for i in 0..=n {
        pos.push(DVec3::new(i as f64, j as f64, 0.0));
      }
    }
    let mut tris = Vec::new();
    let w = (n + 1) as u32;
    for j in 0..n as u32 {
      for i in 0..n as u32 {
        let a = j * w + i;
        tris.extend([a, a + 1, a + w + 1, a, a + w + 1, a + w]);
      }
    }
    (pos, tris)
  }

  #[test]
  fn reduce_keeps_about_the_fraction() {
    let (pos, tris) = grid(40);
    let (_, t) = reduce(&pos, &tris, 0.25);
    assert!(t.len() < tris.len() / 2, "{} of {}", t.len() / 3, tris.len() / 3);
    assert!(!t.is_empty());
  }

  #[test]
  fn smoothing_keeps_a_flat_grid_flat() {
    let (pos, tris) = grid(5);
    let p = smooth(&pos, &tris, 3);
    assert!(p.iter().all(|v| v.z.abs() < 1e-12));
  }

  #[test]
  fn remesh_splits_long_edges_and_keeps_a_flat_grid_flat() {
    let (pos, tris) = grid(2);
    // full shape preservation: no relaxation, every edge within the split limit
    let opts = Remesh { target: 0.25, adaptive: false, shape: 1.0, sharp: false, boundaries: true };
    let (p, t) = remesh(&pos, &tris, &opts);
    assert!(t.len() > tris.len() * 8, "{} → {}", tris.len() / 3, t.len() / 3);
    assert!(edge_lengths(&p, &t).iter().all(|l| *l <= 4.0 / 3.0 * 0.25 + 1e-9));
    // relaxing keeps it flat and its outline where it was
    let opts = Remesh { shape: 0.5, ..opts };
    let (p, t) = remesh(&pos, &tris, &opts);
    assert!(p.iter().all(|v| v.z.abs() < 1e-12));
    // watertight: every inner edge still has two triangles, the outline is the grid's
    assert_eq!(boundary_edges(&t).iter().map(|(a, b)| (p[*a as usize] - p[*b as usize]).length()).sum::<f64>(), 8.0);
    // adaptive leaves a flat region coarser
    let (_, ta) = remesh(&pos, &tris, &Remesh { adaptive: true, ..opts });
    assert!(ta.len() < t.len());
  }

  /// A closed 30 x 10 x 5 box of 12 triangles, facing out.
  fn block() -> (Vec<DVec3>, Vec<u32>) {
    let pos = (0..8).map(|i| DVec3::new(30.0 * (i & 1) as f64, 10.0 * (i >> 1 & 1) as f64, 5.0 * (i >> 2) as f64));
    let tris =
      [0, 2, 3, 0, 3, 1, 4, 5, 7, 4, 7, 6, 0, 1, 5, 0, 5, 4, 2, 6, 7, 2, 7, 3, 0, 4, 6, 0, 6, 2, 1, 3, 7, 1, 7, 5];
    (pos.collect(), tris.to_vec())
  }

  /// Fusion's default Remesh (Adaptive, Density 0.25, Shape Preservation 0.50, nothing
  /// preserved) refines a box and leaves it a box: its creases slide along themselves, its
  /// corners stay put.
  #[test]
  fn remesh_at_the_defaults_keeps_a_box_a_box() {
    let (pos, tris) = block();
    let v0 = crate::meshtools::volume(&pos, &tris);
    assert!((v0.abs() - 1500.0).abs() < 1e-9, "{v0}");
    let diagonal = (30.0f64 * 30.0 + 10.0 * 10.0 + 5.0 * 5.0).sqrt();
    let opts =
      Remesh { target: density_edge(diagonal, 0.25), adaptive: true, shape: 0.5, sharp: false, boundaries: false };
    let (p, t) = remesh(&pos, &tris, &opts);
    assert!(t.len() > tris.len() * 4, "{} → {}", tris.len() / 3, t.len() / 3);
    assert!(boundary_edges(&t).is_empty(), "still closed");
    assert!((crate::meshtools::volume(&p, &t) - v0).abs() < 1e-6, "the volume stays");
    // every vertex is still on the box's surface
    let on_face = |v: &DVec3| {
      [v.x, 30.0 - v.x, v.y, 10.0 - v.y, v.z, 5.0 - v.z].iter().any(|d| d.abs() < 1e-9)
        && v.cmpge(DVec3::splat(-1e-9)).all()
        && v.cmple(DVec3::new(30.0, 10.0, 5.0) + 1e-9).all()
    };
    assert!(p.iter().all(on_face), "{:?}", p.iter().find(|v| !on_face(v)));
    // and every corner is still there
    for c in &pos {
      assert!(p.iter().any(|v| (*v - *c).length() < 1e-9), "corner {c}");
    }
  }

  #[test]
  fn separate_finds_each_piece() {
    let (mut pos, mut tris) = grid(3);
    let (p2, t2) = grid(1);
    let n = pos.len() as u32;
    pos.extend(p2.iter().map(|p| *p + DVec3::new(10.0, 0.0, 0.0)));
    tris.extend(t2.iter().map(|i| i + n));
    let parts = components(&pos, &tris);
    assert_eq!(parts.len(), 2);
    assert_eq!((parts[0].1.len() / 3, parts[1].1.len() / 3), (18, 2));
    assert_eq!(parts[1].0.len(), 4);
  }

  #[test]
  fn shelling_an_open_mesh_closes_it() {
    let (pos, tris) = grid(2);
    let (p, t) = shell_open(&pos, &tris, 0.5);
    assert!(boundary_edges(&t).is_empty());
    assert_eq!(p.len(), 2 * pos.len());
    // the grid faces +z: the copy lies 0.5 below
    assert!(p[pos.len()..].iter().all(|v| (v.z + 0.5).abs() < 1e-12));
  }
}
