//! T-Spline Mesh (`.tsm`) files, what Fusion's Insert T-Spline reads, and the smooth surface of
//! their control cage.
//!
//! A TSM file stores a half-edge mesh: `f`, `e` and `v` records each name one link (half-edge),
//! `l prev next twin vertex face edge ...` records are the links, and `0g x y z w` records are
//! the control points. Fusion reads the coordinates in centimetres (its internal unit) whatever
//! the `units` line says, so they're scaled by 10 here.
//!
//! The surface is the cage's Catmull-Clark limit surface, which is what a T-spline with uniform
//! knot intervals is away from its star points (Fusion's G1 caps around star points differ
//! slightly: about 0.06% of the volume of a box cage). [`Cage::limit_patches`] samples it as one
//! grid of exact limit points per B-rep face. The faces are the cage regions between the lines
//! that run from star points (vertices with other than four edges) across regular vertices, as
//! in Fusion, where a box cage gives a six-faced body.
//!
//! Not read: knot intervals other than 1, creases, and rational weights (an error).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{ExchangeError, Result};

/// Millimetres per TSM coordinate unit (Fusion reads them as centimetres).
const MM_PER_UNIT: f64 = 10.0;

/// A T-spline control cage.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cage {
  /// Control points, mm.
  pub points: Vec<[f64; 3]>,
  /// Faces as loops of point indices, counter-clockwise seen from outside.
  pub faces: Vec<Vec<u32>>,
}

/// Points on the limit surface over one rectangular region of the cage: `rows × cols`, row by
/// row. Rows run along the region's second direction, so (column, row) is right-handed about
/// the outward normal.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
  pub rows: usize,
  pub cols: usize,
  pub points: Vec<[f64; 3]>,
}

/// Reads a `.tsm` file.
pub fn read_tsm(text: &str) -> Result<Cage> {
  let mut faces = Vec::new();
  let mut links: Vec<[usize; 6]> = Vec::new();
  let mut points = Vec::new();
  for (n, line) in text.lines().enumerate() {
    let t: Vec<&str> = line.split_whitespace().collect();
    let bad = || ExchangeError::fmt(format!("TSM line {}: can't read '{}'", n + 1, line.trim()));
    let ints = |k: usize| -> Result<Vec<usize>> {
      t.get(1..=k).ok_or_else(bad)?.iter().map(|s| s.parse::<usize>().map_err(|_| bad())).collect()
    };
    match t.first().copied() {
      Some("f") => faces.push(ints(1)?[0]),
      Some("l") => {
        let v = ints(6)?;
        links.push([v[0], v[1], v[2], v[3], v[4], v[5]]);
      }
      Some("0g") => {
        let v: Vec<f64> =
          t.get(1..=4).ok_or_else(bad)?.iter().map(|s| s.parse::<f64>().map_err(|_| bad())).collect::<Result<_>>()?;
        if (v[3] - 1.0).abs() > 1e-9 {
          return Err(ExchangeError::fmt("TSM: rational control points (weights other than 1) aren't supported"));
        }
        if v.iter().any(|c| !c.is_finite()) {
          return Err(bad());
        }
        points.push([v[0] * MM_PER_UNIT, v[1] * MM_PER_UNIT, v[2] * MM_PER_UNIT]);
      }
      _ => {}
    }
  }
  if faces.is_empty() || points.is_empty() {
    return Err(ExchangeError::fmt("TSM: no faces or control points (is this a T-Spline Mesh file?)"));
  }
  let mut loops = Vec::with_capacity(faces.len());
  for &first in &faces {
    let mut face = Vec::new();
    let mut l = first;
    loop {
      let link = links.get(l).ok_or_else(|| ExchangeError::fmt(format!("TSM: no link {l}")))?;
      let v = link[3];
      if v >= points.len() {
        return Err(ExchangeError::fmt(format!("TSM: link {l} names vertex {v}, which has no control point")));
      }
      face.push(v as u32);
      l = link[1];
      if l == first {
        break;
      }
      if face.len() > links.len() {
        return Err(ExchangeError::fmt("TSM: a face's links don't close"));
      }
    }
    if face.len() < 3 {
      return Err(ExchangeError::fmt("TSM: a face has fewer than three sides"));
    }
    loops.push(face);
  }
  Ok(Cage { points, faces: loops })
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
  [a[0] * s, a[1] * s, a[2] * s]
}

fn key(a: u32, b: u32) -> (u32, u32) {
  (a.min(b), a.max(b))
}

/// Faces at each edge, and the edges and faces at each vertex.
struct Topology {
  edge_faces: HashMap<(u32, u32), Vec<u32>>,
  vert_edges: Vec<Vec<(u32, u32)>>,
  vert_faces: Vec<Vec<u32>>,
}

impl Topology {
  fn new(c: &Cage) -> Result<Topology> {
    let mut edge_faces: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    let mut vert_edges = vec![Vec::new(); c.points.len()];
    let mut vert_faces = vec![Vec::new(); c.points.len()];
    for (f, face) in c.faces.iter().enumerate() {
      for (i, &a) in face.iter().enumerate() {
        let b = face[(i + 1) % face.len()];
        if a as usize >= c.points.len() || a == b {
          return Err(ExchangeError::fmt("T-spline cage: a face has a bad vertex"));
        }
        let e = edge_faces.entry(key(a, b)).or_default();
        if e.len() == 2 {
          return Err(ExchangeError::fmt("T-spline cage: an edge has more than two faces"));
        }
        if e.is_empty() {
          vert_edges[a as usize].push(key(a, b));
          vert_edges[b as usize].push(key(a, b));
        }
        e.push(f as u32);
        vert_faces[a as usize].push(f as u32);
      }
    }
    Ok(Topology { edge_faces, vert_edges, vert_faces })
  }

  fn boundary(&self, e: (u32, u32)) -> bool {
    self.edge_faces.get(&e).is_none_or(|f| f.len() < 2)
  }

  fn on_boundary(&self, v: usize) -> bool {
    self.vert_edges[v].iter().any(|&e| self.boundary(e))
  }

  /// The two boundary neighbours of a boundary vertex that has exactly two boundary edges.
  fn boundary_neighbours(&self, v: usize) -> Option<(u32, u32)> {
    let ends: Vec<u32> = self.vert_edges[v]
      .iter()
      .filter(|&&e| self.boundary(e))
      .map(|&(a, b)| if a as usize == v { b } else { a })
      .collect();
    match ends[..] {
      [a, b] => Some((a, b)),
      _ => None,
    }
  }
}

/// One Catmull-Clark step: the refined cage, and where each edge's new point went (face `f`'s
/// point is `points.len() + f` of the old cage; old vertices keep their indices).
fn subdivide(c: &Cage, topo: &Topology) -> (Cage, HashMap<(u32, u32), u32>) {
  let nv = c.points.len();
  let fp: Vec<[f64; 3]> = c
    .faces
    .iter()
    .map(|f| scale(f.iter().fold([0.0; 3], |s, &v| add(s, c.points[v as usize])), 1.0 / f.len() as f64))
    .collect();
  let mut points = Vec::with_capacity(nv + fp.len() + topo.edge_faces.len());
  for v in 0..nv {
    let p = c.points[v];
    let n = topo.vert_edges[v].len();
    let q = if topo.on_boundary(v) {
      match topo.boundary_neighbours(v) {
        Some((a, b)) if topo.vert_faces[v].len() > 1 => {
          scale(add(add(c.points[a as usize], c.points[b as usize]), scale(p, 6.0)), 1.0 / 8.0)
        }
        _ => p,
      }
    } else if n < 3 {
      p
    } else {
      let fs = &topo.vert_faces[v];
      let favg = scale(fs.iter().fold([0.0; 3], |s, &f| add(s, fp[f as usize])), 1.0 / fs.len() as f64);
      let mid = topo.vert_edges[v]
        .iter()
        .fold([0.0; 3], |s, &(a, b)| add(s, scale(add(c.points[a as usize], c.points[b as usize]), 0.5)));
      let n = n as f64;
      scale(add(add(favg, scale(mid, 2.0 / n)), scale(p, n - 3.0)), 1.0 / n)
    };
    points.push(q);
  }
  points.extend_from_slice(&fp);
  let mut edges: Vec<(u32, u32)> = topo.edge_faces.keys().copied().collect();
  edges.sort_unstable();
  let mut edge_pt = HashMap::with_capacity(edges.len());
  for e in edges {
    let ends = add(c.points[e.0 as usize], c.points[e.1 as usize]);
    let q = match topo.edge_faces[&e][..] {
      [f, g] => scale(add(ends, add(fp[f as usize], fp[g as usize])), 0.25),
      _ => scale(ends, 0.5),
    };
    edge_pt.insert(e, points.len() as u32);
    points.push(q);
  }
  let mut faces = Vec::new();
  for (f, face) in c.faces.iter().enumerate() {
    let n = face.len();
    for i in 0..n {
      let (prev, v, next) = (face[(i + n - 1) % n], face[i], face[(i + 1) % n]);
      faces.push(vec![v, edge_pt[&key(v, next)], (nv + f) as u32, edge_pt[&key(prev, v)]]);
    }
  }
  (Cage { points, faces }, edge_pt)
}

/// Where each vertex of an all-quad cage lands on the limit surface.
fn limit_points(c: &Cage, topo: &Topology) -> Vec<[f64; 3]> {
  (0..c.points.len())
    .map(|v| {
      let p = c.points[v];
      if topo.on_boundary(v) {
        return match topo.boundary_neighbours(v) {
          Some((a, b)) if topo.vert_faces[v].len() > 1 => {
            scale(add(add(c.points[a as usize], c.points[b as usize]), scale(p, 4.0)), 1.0 / 6.0)
          }
          _ => p,
        };
      }
      let n = topo.vert_edges[v].len();
      if n < 3 {
        return p;
      }
      let edges = topo.vert_edges[v]
        .iter()
        .fold([0.0; 3], |s, &(a, b)| add(s, c.points[(if a as usize == v { b } else { a }) as usize]));
      let diag = topo.vert_faces[v].iter().fold([0.0; 3], |s, &f| {
        let face = &c.faces[f as usize];
        let i = face.iter().position(|&w| w as usize == v).unwrap_or(0);
        add(s, c.points[face[(i + 2) % 4] as usize])
      });
      let n = n as f64;
      scale(add(add(scale(p, n * n), scale(edges, 4.0)), diag), 1.0 / (n * (n + 5.0)))
    })
    .collect()
}

/// A rectangular region of quads: each face with the region coordinates of its four corners.
type Region = Vec<(u32, [(i64, i64); 4])>;

/// Splits an all-quad cage into rectangular regions bounded by the boundary and by the edge
/// lines from irregular vertices. A region that doesn't come out rectangular (a torus with no
/// irregular vertex, say) falls back to one region per face.
fn regions(c: &Cage, topo: &Topology) -> Vec<Region> {
  let regular = |v: usize| {
    let faces = topo.vert_faces[v].len();
    if topo.on_boundary(v) { faces <= 2 } else { faces == 4 }
  };
  let mut cut: HashSet<(u32, u32)> = topo.edge_faces.keys().copied().filter(|&e| topo.boundary(e)).collect();
  for v in 0..c.points.len() {
    if regular(v) {
      continue;
    }
    for &start in &topo.vert_edges[v] {
      let (mut from, mut e) = (v as u32, start);
      while cut.insert(e) {
        let to = if e.0 == from { e.1 } else { e.0 };
        if !regular(to as usize) || topo.on_boundary(to as usize) {
          break;
        }
        // straight on: the edge at `to` that shares no face with `e`
        let faces = &topo.edge_faces[&e];
        let next = topo.vert_edges[to as usize]
          .iter()
          .find(|&&g| g != e && topo.edge_faces[&g].iter().all(|f| !faces.contains(f)));
        match next {
          Some(&g) => (from, e) = (to, g),
          None => break,
        }
      }
    }
  }
  let mut seen = vec![false; c.faces.len()];
  let mut out = Vec::new();
  for f0 in 0..c.faces.len() {
    if seen[f0] {
      continue;
    }
    seen[f0] = true;
    let mut region: Region = vec![(f0 as u32, [(0, 0), (1, 0), (1, 1), (0, 1)])];
    let mut queue = VecDeque::from([0usize]);
    while let Some(k) = queue.pop_front() {
      let (f, corners) = region[k];
      let face = &c.faces[f as usize];
      for i in 0..4 {
        let (p, q) = (face[i], face[(i + 1) % 4]);
        if cut.contains(&key(p, q)) {
          continue;
        }
        let Some(&g) = topo.edge_faces[&key(p, q)].iter().find(|&&g| g != f) else { continue };
        if seen[g as usize] {
          continue;
        }
        let gf = &c.faces[g as usize];
        let Some(j) = (0..4).find(|&j| gf[j] == q && gf[(j + 1) % 4] == p) else { continue };
        seen[g as usize] = true;
        let (cp, cq, cin) = (corners[i], corners[(i + 1) % 4], corners[(i + 3) % 4]);
        let d = (cin.0 - cp.0, cin.1 - cp.1);
        let mut gc = [(0, 0); 4];
        gc[j] = cq;
        gc[(j + 1) % 4] = cp;
        gc[(j + 2) % 4] = (cp.0 - d.0, cp.1 - d.1);
        gc[(j + 3) % 4] = (cq.0 - d.0, cq.1 - d.1);
        region.push((g, gc));
        queue.push_back(region.len() - 1);
      }
    }
    if rectangular(c, &region) {
      out.push(region);
    } else {
      out.extend(region.iter().map(|&(f, _)| vec![(f, [(0, 0), (1, 0), (1, 1), (0, 1)])]));
    }
  }
  out
}

/// Whether a region's faces tile a rectangle, one vertex per grid point.
fn rectangular(c: &Cage, region: &Region) -> bool {
  let mut cells = HashSet::new();
  let mut at: HashMap<u32, (i64, i64)> = HashMap::new();
  let (mut lo, mut hi) = ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN));
  for &(f, corners) in region {
    let (x, y) = (corners.iter().map(|p| p.0).min().unwrap_or(0), corners.iter().map(|p| p.1).min().unwrap_or(0));
    if !cells.insert((x, y)) {
      return false;
    }
    (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x + 1), hi.1.max(y + 1)));
    for (k, &v) in c.faces[f as usize].iter().enumerate() {
      if *at.entry(v).or_insert(corners[k]) != corners[k] {
        return false;
      }
    }
  }
  cells.len() as i64 == (hi.0 - lo.0) * (hi.1 - lo.1)
}

impl Cage {
  /// The limit surface, sampled as one grid of points per B-rep face. Each cage quad gives
  /// `2^levels + 1` points a side; `levels` is clamped to 1..=6.
  pub fn limit_patches(&self, levels: u32) -> Result<Vec<Patch>> {
    let levels = levels.clamp(1, 6);
    let mut cage = self.clone();
    let mut topo = Topology::new(&cage)?;
    let mut levels_left = levels;
    if cage.faces.iter().any(|f| f.len() != 4) {
      // one step turns every face into quads
      cage = subdivide(&cage, &topo).0;
      topo = Topology::new(&cage)?;
      levels_left -= 1;
    }
    let regions = regions(&cage, &topo);
    // each base quad's points at the finest level: grid[a][b], a from corner 0 toward corner 1,
    // b from corner 0 toward corner 3
    let mut grids: Vec<Vec<Vec<u32>>> = cage.faces.iter().map(|f| vec![vec![f[0], f[3]], vec![f[1], f[2]]]).collect();
    for _ in 0..levels_left {
      let (next, edge_pt) = subdivide(&cage, &topo);
      let nv = cage.points.len() as u32;
      let face_of: HashMap<[u32; 4], u32> = cage
        .faces
        .iter()
        .enumerate()
        .map(|(f, q)| {
          let mut k = [q[0], q[1], q[2], q[3]];
          k.sort_unstable();
          (k, nv + f as u32)
        })
        .collect();
      for g in &mut grids {
        let n = g.len() - 1;
        let mut r = vec![vec![0u32; 2 * n + 1]; 2 * n + 1];
        for a in 0..=n {
          for b in 0..=n {
            r[2 * a][2 * b] = g[a][b];
            if a < n {
              r[2 * a + 1][2 * b] = edge_pt[&key(g[a][b], g[a + 1][b])];
            }
            if b < n {
              r[2 * a][2 * b + 1] = edge_pt[&key(g[a][b], g[a][b + 1])];
            }
            if a < n && b < n {
              let mut k = [g[a][b], g[a + 1][b], g[a + 1][b + 1], g[a][b + 1]];
              k.sort_unstable();
              r[2 * a + 1][2 * b + 1] =
                *face_of.get(&k).ok_or_else(|| ExchangeError::fmt("T-spline cage: lost a face while subdividing"))?;
            }
          }
        }
        *g = r;
      }
      cage = next;
      topo = Topology::new(&cage)?;
    }
    let limit = limit_points(&cage, &topo);
    let n = 1i64 << levels_left;
    let mut out = Vec::with_capacity(regions.len());
    for region in regions {
      let lo = region.iter().flat_map(|r| r.1).fold((i64::MAX, i64::MAX), |m, p| (m.0.min(p.0), m.1.min(p.1)));
      let hi = region.iter().flat_map(|r| r.1).fold((i64::MIN, i64::MIN), |m, p| (m.0.max(p.0), m.1.max(p.1)));
      let (cols, rows) = (((hi.0 - lo.0) * n + 1) as usize, ((hi.1 - lo.1) * n + 1) as usize);
      let mut points = vec![[0.0; 3]; rows * cols];
      for (f, corners) in region {
        let g = &grids[f as usize];
        let (c0, c1, c3) = (corners[0], corners[1], corners[3]);
        let (du, dv) = ((c1.0 - c0.0, c1.1 - c0.1), (c3.0 - c0.0, c3.1 - c0.1));
        for (a, line) in g.iter().enumerate() {
          for (b, &v) in line.iter().enumerate() {
            let (a, b) = (a as i64, b as i64);
            let x = (c0.0 - lo.0) * n + a * du.0 + b * dv.0;
            let y = (c0.1 - lo.1) * n + a * du.1 + b * dv.1;
            points[y as usize * cols + x as usize] = limit[v as usize];
          }
        }
      }
      out.push(Patch { rows, cols, points });
    }
    Ok(out)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The 80 × 60 × 20 mm box cage (2 × 2 quads a side) that Fusion's Form > Box makes.
  const BOX: &str = include_str!("../testdata/box-cage.tsm");

  fn signed_volume(a: [f64; 3], b: [f64; 3], d: [f64; 3]) -> f64 {
    (a[0] * (b[1] * d[2] - b[2] * d[1]) - a[1] * (b[0] * d[2] - b[2] * d[0]) + a[2] * (b[0] * d[1] - b[1] * d[0])) / 6.0
  }

  /// Signed volume and area of a patch's points, as triangles.
  fn patch_volume_area(p: &Patch) -> (f64, f64) {
    let (mut vol, mut area) = (0.0, 0.0);
    let at = |r: usize, c: usize| p.points[r * p.cols + c];
    for r in 0..p.rows - 1 {
      for c in 0..p.cols - 1 {
        for [a, b, d] in [[at(r, c), at(r, c + 1), at(r + 1, c + 1)], [at(r, c), at(r + 1, c + 1), at(r + 1, c)]] {
          let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [d[0] - a[0], d[1] - a[1], d[2] - a[2]]);
          let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
          area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
          vol += signed_volume(a, b, d);
        }
      }
    }
    (vol, area)
  }

  #[test]
  fn reads_the_box_cage() {
    let c = read_tsm(BOX).unwrap();
    assert_eq!((c.points.len(), c.faces.len()), (26, 24));
    assert!(c.faces.iter().all(|f| f.len() == 4));
    let max = c.points.iter().fold([f64::MIN; 3], |m, p| [m[0].max(p[0]), m[1].max(p[1]), m[2].max(p[2])]);
    assert_eq!(max, [40.0, 30.0, 20.0]);
    // faces wind counter-clockwise from outside: the cage's signed volume is the box's
    let mut vol = 0.0;
    for f in &c.faces {
      let p = |i: usize| c.points[f[i] as usize];
      vol += signed_volume(p(0), p(1), p(2)) + signed_volume(p(0), p(2), p(3));
    }
    assert!((vol - 96000.0).abs() < 1e-6, "{vol}");
  }

  #[test]
  fn box_cage_limit_surface_has_six_faces() {
    let patches = read_tsm(BOX).unwrap().limit_patches(4).unwrap();
    assert_eq!(patches.len(), 6);
    assert!(patches.iter().all(|p| p.rows == 33 && p.cols == 33));
    let (vol, area) = patches.iter().map(patch_volume_area).fold((0.0, 0.0), |s, x| (s.0 + x.0, s.1 + x.1));
    // the Catmull-Clark limit is 75538 mm³ and 11463.6 mm² (Fusion's T-spline: 75587, 11470)
    assert!((vol - 75538.0).abs() < 150.0, "{vol}");
    assert!((area - 11463.6).abs() < 20.0, "{area}");
    // face centres stay on the cage's faces
    let top = patches.iter().map(|p| p.points[16 * 33 + 16]).find(|q| q[2] > 19.0).unwrap();
    assert!(top[0].abs() < 1e-9 && top[1].abs() < 1e-9 && (top[2] - 20.0).abs() < 1e-9, "{top:?}");
  }

  #[test]
  fn triangles_and_open_cages() {
    // a tetrahedron: one step makes quads, and every edge then lies on a line between irregular
    // vertices, so each quad is its own face
    let tet = Cage {
      points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]],
      faces: vec![vec![0, 2, 1], vec![0, 1, 3], vec![1, 2, 3], vec![0, 3, 2]],
    };
    let patches = tet.limit_patches(3).unwrap();
    assert_eq!(patches.len(), 12);
    let vol: f64 = patches.iter().map(|p| patch_volume_area(p).0).sum();
    assert!(vol > 0.0 && vol < 1000.0 / 6.0, "{vol}");
    // an open 2 × 2 grid: one face whose corners stay put
    let mut points = Vec::new();
    for y in 0..3 {
      for x in 0..3 {
        points.push([x as f64 * 10.0, y as f64 * 10.0, if x == 1 && y == 1 { 5.0 } else { 0.0 }]);
      }
    }
    let faces = vec![vec![0, 1, 4, 3], vec![1, 2, 5, 4], vec![3, 4, 7, 6], vec![4, 5, 8, 7]];
    let patches = Cage { points, faces }.limit_patches(2).unwrap();
    assert_eq!(patches.len(), 1);
    let p = &patches[0];
    assert_eq!((p.rows, p.cols), (9, 9));
    assert_eq!(p.points[0], [0.0, 0.0, 0.0]);
    assert_eq!(p.points[80], [20.0, 20.0, 0.0]);
    assert!(p.points[40][2] > 0.0 && p.points[40][2] < 5.0);
  }

  #[test]
  fn bad_files() {
    assert!(read_tsm("hello").is_err());
    // a link naming a vertex with no control point
    assert!(read_tsm("f 0 0\nl 0 0 0 5 0 0 0\n0g 0 0 0 1\n").is_err());
    // a weighted control point
    assert!(read_tsm("f 0 0\nl 0 0 0 0 0 0 0\n0g 0 0 0 2\n").is_err());
  }
}
