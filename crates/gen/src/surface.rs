//! The outcome's surface: the 0.5 iso-surface of the density field (averaged onto the nodes) by
//! marching cubes, each cube split into six tetrahedra around its main diagonal (the Kuhn
//! split, consistent between neighbours, so the mesh is closed and manifold without the
//! ambiguous cases of the classic tables), then Taubin smoothing, which takes the voxel steps
//! off without shrinking the part.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Grid;
use crate::grid::{V3, add, cross, dot, norm, scale, sub};

/// A closed triangle mesh, outward facing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SurfaceMesh {
  pub positions: Vec<[f32; 3]>,
  pub indices: Vec<u32>,
}

impl SurfaceMesh {
  pub fn triangles(&self) -> usize {
    self.indices.len() / 3
  }

  /// Enclosed volume (divergence theorem).
  pub fn volume(&self) -> f64 {
    self
      .indices
      .chunks_exact(3)
      .map(|t| {
        let p = |i: u32| self.positions[i as usize].map(f64::from);
        dot(p(t[0]), cross(p(t[1]), p(t[2]))) / 6.0
      })
      .sum()
  }

  /// Bounding box.
  pub fn bounds(&self) -> (V3, V3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &self.positions {
      for a in 0..3 {
        lo[a] = lo[a].min(p[a] as f64);
        hi[a] = hi[a].max(p[a] as f64);
      }
    }
    (lo, hi)
  }

  /// Edges used by other than exactly two triangles (0 for a closed manifold mesh).
  pub fn open_edges(&self) -> usize {
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for t in self.indices.chunks_exact(3) {
      for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
        *count.entry((a.min(b), a.max(b))).or_default() += 1;
      }
    }
    count.values().filter(|c| **c != 2).count()
  }
}

/// The six tetrahedra of a cube (corner indices as in [`crate::fe::CORNERS`]).
const TETS: [[usize; 4]; 6] = [[0, 1, 2, 6], [0, 2, 3, 6], [0, 3, 7, 6], [0, 7, 4, 6], [0, 4, 5, 6], [0, 5, 1, 6]];

/// Element densities averaged onto the nodes.
pub fn nodal(grid: &Grid, density: &[f32]) -> Vec<f64> {
  let mut v = vec![0.0; grid.nodes()];
  for (e, d) in density.iter().enumerate() {
    for n in grid.elem_nodes(e) {
      v[n] += *d as f64 / 8.0;
    }
  }
  v
}

/// The iso-surface of element densities at `iso`, smoothed by `passes` Taubin steps.
/// The grid is padded with an empty layer first, so the surface is closed.
pub fn extract(grid: &Grid, density: &[f32], iso: f64, passes: usize) -> SurfaceMesh {
  let padded = Grid { origin: grid.origin.map(|o| o - grid.h), h: grid.h, n: grid.n.map(|n| n + 2) };
  let mut dens = vec![0.0f32; padded.elements()];
  for (e, d) in density.iter().enumerate() {
    let [i, j, k] = grid.elem_ijk(e);
    dens[padded.elem(i + 1, j + 1, k + 1)] = *d;
  }
  let grid = &padded;
  let f = nodal(grid, &dens);
  let mut verts: Vec<V3> = Vec::new();
  let mut index: HashMap<(usize, usize), u32> = HashMap::new();
  let mut tris: Vec<u32> = Vec::new();
  let mut vertex = |a: usize, b: usize, verts: &mut Vec<V3>| -> u32 {
    let key = (a.min(b), a.max(b));
    *index.entry(key).or_insert_with(|| {
      let (fa, fb) = (f[key.0], f[key.1]);
      let t = ((iso - fa) / (fb - fa)).clamp(0.0, 1.0);
      let (pa, pb) = (grid.node_pos(key.0), grid.node_pos(key.1));
      verts.push(add(pa, scale(sub(pb, pa), t)));
      (verts.len() - 1) as u32
    })
  };
  for e in 0..grid.elements() {
    let nodes = grid.elem_nodes(e);
    let inside = nodes.map(|n| f[n] > iso);
    if inside.iter().all(|i| *i) || inside.iter().all(|i| !*i) {
      continue;
    }
    for tet in TETS {
      let ids = tet.map(|c| nodes[c]);
      let ins: Vec<usize> = ids.into_iter().filter(|n| f[*n] > iso).collect();
      let outs: Vec<usize> = ids.into_iter().filter(|n| f[*n] <= iso).collect();
      // outward: from the inside corners towards the outside ones
      let centre = |v: &[usize]| scale(v.iter().fold([0.0; 3], |s, n| add(s, grid.node_pos(*n))), 1.0 / v.len() as f64);
      let poly: Vec<u32> = match (ins.len(), outs.len()) {
        (1, 3) => outs.iter().map(|o| vertex(ins[0], *o, &mut verts)).collect(),
        (3, 1) => ins.iter().map(|i| vertex(*i, outs[0], &mut verts)).collect(),
        (2, 2) => vec![
          vertex(ins[0], outs[0], &mut verts),
          vertex(ins[0], outs[1], &mut verts),
          vertex(ins[1], outs[1], &mut verts),
          vertex(ins[1], outs[0], &mut verts),
        ],
        _ => continue,
      };
      let out_dir = sub(centre(&outs), centre(&ins));
      let mut emit = |a: u32, b: u32, c: u32| {
        if a == b || b == c || a == c {
          return;
        }
        let (pa, pb, pc) = (verts[a as usize], verts[b as usize], verts[c as usize]);
        if dot(cross(sub(pb, pa), sub(pc, pa)), out_dir) < 0.0 {
          tris.extend([a, c, b]);
        } else {
          tris.extend([a, b, c]);
        }
      };
      emit(poly[0], poly[1], poly[2]);
      if poly.len() == 4 {
        emit(poly[0], poly[2], poly[3]);
      }
    }
  }
  let verts = taubin(&verts, &tris, passes);
  SurfaceMesh { positions: verts.iter().map(|p| p.map(|v| v as f32)).collect(), indices: tris }
}

/// Taubin (lambda | mu) smoothing with the umbrella operator.
pub fn taubin(verts: &[V3], tris: &[u32], passes: usize) -> Vec<V3> {
  let mut nb: Vec<Vec<u32>> = vec![Vec::new(); verts.len()];
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
  let mut v = verts.to_vec();
  let step = |v: &[V3], k: f64| -> Vec<V3> {
    v.iter()
      .enumerate()
      .map(|(i, p)| {
        let n = &nb[i];
        if n.is_empty() {
          return *p;
        }
        let avg = scale(n.iter().fold([0.0; 3], |s, j| add(s, v[*j as usize])), 1.0 / n.len() as f64);
        add(*p, scale(sub(avg, *p), k))
      })
      .collect()
  };
  for _ in 0..passes {
    v = step(&v, 0.5);
    v = step(&v, -0.53);
  }
  v
}

/// Total area.
pub fn area(m: &SurfaceMesh) -> f64 {
  m.indices
    .chunks_exact(3)
    .map(|t| {
      let p = |i: u32| m.positions[i as usize].map(f64::from);
      0.5 * norm(cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0]))))
    })
    .sum()
}

/// Drops pieces of the mesh enclosing less than `fraction` of the largest piece's volume
/// (islands of material the optimisation left floating).
pub fn drop_islands(m: &SurfaceMesh, fraction: f64) -> SurfaceMesh {
  let nv = m.positions.len();
  let mut parent: Vec<usize> = (0..nv).collect();
  fn find(p: &mut [usize], mut i: usize) -> usize {
    while p[i] != i {
      p[i] = p[p[i]];
      i = p[i];
    }
    i
  }
  for t in m.indices.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2])] {
      let (ra, rb) = (find(&mut parent, a as usize), find(&mut parent, b as usize));
      if ra != rb {
        parent[ra] = rb;
      }
    }
  }
  let mut vol: std::collections::HashMap<usize, f64> = HashMap::new();
  let p = |i: u32| m.positions[i as usize].map(f64::from);
  for t in m.indices.chunks_exact(3) {
    let r = find(&mut parent, t[0] as usize);
    *vol.entry(r).or_default() += dot(p(t[0]), cross(p(t[1]), p(t[2]))) / 6.0;
  }
  let largest = vol.values().fold(0.0f64, |a, b| a.max(*b));
  let keep: Vec<u32> = m
    .indices
    .chunks_exact(3)
    .filter(|t| {
      let r = find(&mut parent, t[0] as usize);
      vol.get(&r).copied().unwrap_or(0.0) >= fraction * largest
    })
    .flatten()
    .copied()
    .collect();
  compact(&m.positions, &keep)
}

/// The mesh with only the vertices its triangles use.
fn compact(positions: &[[f32; 3]], tris: &[u32]) -> SurfaceMesh {
  let mut map = vec![u32::MAX; positions.len()];
  let mut out = SurfaceMesh::default();
  for &i in tris {
    if map[i as usize] == u32::MAX {
      map[i as usize] = out.positions.len() as u32;
      out.positions.push(positions[i as usize]);
    }
    out.indices.push(map[i as usize]);
  }
  out
}

/// Quadric error metric.
#[derive(Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
  fn plane(n: V3, d: f64) -> Quadric {
    let [a, b, c] = n;
    Quadric([a * a, a * b, a * c, a * d, b * b, b * c, b * d, c * c, c * d, d * d])
  }
  fn add(&self, o: &Quadric) -> Quadric {
    let mut q = self.0;
    for (a, b) in q.iter_mut().zip(o.0) {
      *a += b;
    }
    Quadric(q)
  }
  fn error(&self, p: V3) -> f64 {
    let q = &self.0;
    let [x, y, z] = p;
    q[0] * x * x
      + 2.0 * q[1] * x * y
      + 2.0 * q[2] * x * z
      + 2.0 * q[3] * x
      + q[4] * y * y
      + 2.0 * q[5] * y * z
      + 2.0 * q[6] * y
      + q[7] * z * z
      + 2.0 * q[8] * z
      + q[9]
  }
  /// The point of least error, if the quadric is well conditioned.
  fn optimum(&self) -> Option<V3> {
    let q = &self.0;
    let m = [[q[0], q[1], q[2]], [q[1], q[4], q[5]], [q[2], q[5], q[7]]];
    let b = [-q[3], -q[6], -q[8]];
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
      + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let scale = q[0] + q[4] + q[7];
    if det.abs() <= 1e-9 * scale.powi(3).max(1e-300) {
      return None;
    }
    let solve = |col: usize| {
      let mut a = m;
      for r in 0..3 {
        a[r][col] = b[r];
      }
      (a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
        / det
    };
    Some([solve(0), solve(1), solve(2)])
  }
}

/// The vertices sharing a triangle with `v`, sorted.
fn ring(faces: &[Vec<u32>], tris: &[[u32; 3]], v: usize) -> Vec<u32> {
  let mut r: Vec<u32> = faces[v].iter().flat_map(|f| tris[*f as usize]).filter(|w| *w as usize != v).collect();
  r.sort_unstable();
  r.dedup();
  r
}

/// Fewer triangles by quadric error edge collapses (Garland and Heckbert), down to about
/// `target`, keeping the mesh closed and manifold: a collapse is skipped when the edge's ends
/// share more than its two triangles' third corners, or when it would flip a triangle.
pub fn decimate(m: &SurfaceMesh, target: usize) -> SurfaceMesh {
  use std::cmp::Ordering;
  use std::collections::BinaryHeap;
  let mut pos: Vec<V3> = m.positions.iter().map(|p| p.map(f64::from)).collect();
  let mut tris: Vec<[u32; 3]> = m.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
  let mut alive = vec![true; tris.len()];
  let mut live = tris.len();
  if live <= target {
    return m.clone();
  }
  let nv = pos.len();
  let mut faces: Vec<Vec<u32>> = vec![Vec::new(); nv];
  for (f, t) in tris.iter().enumerate() {
    for v in t {
      faces[*v as usize].push(f as u32);
    }
  }
  let normal = |pos: &[V3], t: &[u32; 3]| {
    let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
    cross(sub(b, a), sub(c, a))
  };
  let mut quad = vec![Quadric::default(); nv];
  for t in &tris {
    let n = normal(&pos, t);
    let l = norm(n);
    if l < 1e-30 {
      continue;
    }
    let n = scale(n, 1.0 / l);
    // area weighted
    let q = Quadric::plane(n, -dot(n, pos[t[0] as usize]));
    let q = Quadric(q.0.map(|v| v * l * 0.5));
    for v in t {
      quad[*v as usize] = quad[*v as usize].add(&q);
    }
  }
  #[derive(PartialEq)]
  struct Cand(f64, u32, u32, u32, u32);
  impl Eq for Cand {}
  impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
      Some(self.cmp(o))
    }
  }
  impl Ord for Cand {
    fn cmp(&self, o: &Self) -> Ordering {
      o.0.total_cmp(&self.0)
    }
  }
  let mut version = vec![0u32; nv];
  let mut removed = vec![false; nv];
  let place = |quad: &[Quadric], pos: &[V3], a: u32, b: u32| -> (f64, V3) {
    let q = quad[a as usize].add(&quad[b as usize]);
    let (pa, pb) = (pos[a as usize], pos[b as usize]);
    let mid = scale(add(pa, pb), 0.5);
    let mut best = (q.error(mid), mid);
    if let Some(o) = q.optimum() {
      // stay near the edge
      if norm(sub(o, mid)) <= norm(sub(pb, pa)) {
        best = (q.error(o), o);
      }
    }
    for p in [pa, pb] {
      let e = q.error(p);
      if e < best.0 {
        best = (e, p);
      }
    }
    best
  };
  let mut heap = BinaryHeap::new();
  let push_edges = |heap: &mut BinaryHeap<Cand>,
                    v: u32,
                    faces: &[Vec<u32>],
                    tris: &[[u32; 3]],
                    quad: &[Quadric],
                    pos: &[V3],
                    version: &[u32]| {
    for f in &faces[v as usize] {
      for w in tris[*f as usize] {
        if w != v {
          let (e, _) = place(quad, pos, v, w);
          heap.push(Cand(e, v, w, version[v as usize], version[w as usize]));
        }
      }
    }
  };
  for v in 0..nv as u32 {
    push_edges(&mut heap, v, &faces, &tris, &quad, &pos, &version);
  }
  while live > target {
    let Some(Cand(_, a, b, va, vb)) = heap.pop() else { break };
    if removed[a as usize] || removed[b as usize] || version[a as usize] != va || version[b as usize] != vb {
      continue;
    }
    let (ai, bi) = (a as usize, b as usize);
    // the faces on the edge, and the link condition
    let shared: Vec<u32> = faces[ai].iter().copied().filter(|f| tris[*f as usize].contains(&b)).collect();
    if shared.len() != 2 {
      continue;
    }
    let (ra, rb) = (ring(&faces, &tris, ai), ring(&faces, &tris, bi));
    let common = ra.iter().filter(|w| rb.binary_search(w).is_ok()).count();
    if common != 2 {
      continue;
    }
    let (_, p) = place(&quad, &pos, a, b);
    // no flipped or degenerate triangles around the new vertex
    let ok = faces[ai].iter().chain(&faces[bi]).all(|f| {
      if shared.contains(f) {
        return true;
      }
      let t = tris[*f as usize];
      let before = normal(&pos, &t);
      let moved = t.map(|w| if w == a || w == b { p } else { pos[w as usize] });
      let after = cross(sub(moved[1], moved[0]), sub(moved[2], moved[0]));
      let (lb, la) = (norm(before), norm(after));
      la > 1e-12 * lb.max(1e-30) && dot(before, after) > 0.2 * lb * la
    });
    if !ok {
      continue;
    }
    // collapse b into a
    pos[ai] = p;
    quad[ai] = quad[ai].add(&quad[bi]);
    for f in &shared {
      alive[*f as usize] = false;
      live -= 1;
      for w in tris[*f as usize] {
        faces[w as usize].retain(|x| x != f);
      }
    }
    let moved: Vec<u32> = std::mem::take(&mut faces[bi]);
    for f in moved {
      for w in tris[f as usize].iter_mut() {
        if *w == b {
          *w = a;
        }
      }
      faces[ai].push(f);
    }
    removed[bi] = true;
    version[ai] += 1;
    let ring_a = ring(&faces, &tris, ai);
    for w in &ring_a {
      version[*w as usize] += 1;
    }
    push_edges(&mut heap, a, &faces, &tris, &quad, &pos, &version);
    for w in ring_a {
      push_edges(&mut heap, w, &faces, &tris, &quad, &pos, &version);
    }
  }
  let kept: Vec<u32> = tris.iter().zip(&alive).filter(|(_, a)| **a).flat_map(|(t, _)| *t).collect();
  let positions: Vec<[f32; 3]> = pos.iter().map(|p| p.map(|v| v as f32)).collect();
  compact(&positions, &kept)
}
