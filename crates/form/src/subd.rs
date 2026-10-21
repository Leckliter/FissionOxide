//! The smooth body of a cage: its Catmull-Clark limit surface, with sharp creases and open
//! boundaries (boundary edges act as creases; boundary vertices with one face stay as corners),
//! and that surface as bicubic B-spline patches, one per B-rep face.
//!
//! A cage with faces other than quads is subdivided once first (which leaves the limit surface
//! unchanged), giving a quad "base" mesh. The patches are then the rectangular regions of base
//! faces bounded by the "star lines" (the edge lines that leave each extraordinary vertex and run
//! straight on through regular vertices), creases and the boundary, as Fusion splits a T-spline
//! body into faces (a box cage gives six). Regions whose side would meet another region's corner
//! part-way along are split there, so neighbouring patches always share whole sides and sew
//! watertight; a region that isn't a rectangular grid becomes one patch per base face. Each patch
//! interpolates a grid of exact limit points, found by subdividing the base a few times and
//! pushing the vertices to their limit positions.

use std::collections::{HashMap, HashSet};

use glam::DVec3;

use crate::bspline::{Patch, interpolate};
use crate::mesh::{Cage, Topo, key};

/// Most subdivided quads used to sample the limit surface.
const SAMPLE_QUADS: usize = 25_000;

/// One subdivision level: positions, polygons and creased edges (open boundary edges are
/// found from the polygons).
#[derive(Clone)]
pub(crate) struct Level {
  pub pos: Vec<DVec3>,
  pub polys: Vec<Vec<u32>>,
  pub creases: HashSet<(u32, u32)>,
}

/// Positions after one Catmull-Clark step: old vertices first, then a point per face, then a
/// point per edge (`edge_pt`), with the creases carried onto the new edges.
struct Step {
  pos: Vec<DVec3>,
  face0: u32,
  edge_pt: HashMap<(u32, u32), u32>,
  creases: HashSet<(u32, u32)>,
}

/// Per vertex: the other ends of its sharp (creased or open) edges and its face count.
fn sharpness(pos_len: usize, polys: &[Vec<u32>], creases: &HashSet<(u32, u32)>) -> (Vec<Vec<u32>>, Vec<u32>, HashMap<(u32, u32), u32>) {
  let mut count: HashMap<(u32, u32), u32> = HashMap::new();
  let mut nfaces = vec![0u32; pos_len];
  for p in polys {
    for k in 0..p.len() {
      *count.entry(key(p[k], p[(k + 1) % p.len()])).or_default() += 1;
      nfaces[p[k] as usize] += 1;
    }
  }
  let mut sharp = vec![Vec::new(); pos_len];
  for (&(a, b), &c) in &count {
    if c != 2 || creases.contains(&(a, b)) {
      sharp[a as usize].push(b);
      sharp[b as usize].push(a);
    }
  }
  (sharp, nfaces, count)
}

fn step(pos: &[DVec3], polys: &[Vec<u32>], creases: &HashSet<(u32, u32)>) -> Step {
  let nv = pos.len();
  let (sharp, nfaces, count) = sharpness(nv, polys, creases);
  let face_pts: Vec<DVec3> =
    polys.iter().map(|q| q.iter().map(|&v| pos[v as usize]).sum::<DVec3>() / q.len() as f64).collect();
  let mut edge_faces: HashMap<(u32, u32), DVec3> = HashMap::new();
  let mut fsum = vec![DVec3::ZERO; nv];
  for (qi, q) in polys.iter().enumerate() {
    for k in 0..q.len() {
      *edge_faces.entry(key(q[k], q[(k + 1) % q.len()])).or_default() += face_pts[qi];
      fsum[q[k] as usize] += face_pts[qi];
    }
  }
  let mut keys: Vec<(u32, u32)> = count.keys().copied().collect();
  keys.sort_unstable();
  let edge0 = (nv + polys.len()) as u32;
  let mut edge_pt = HashMap::with_capacity(keys.len());
  let mut edge_pts = Vec::with_capacity(keys.len());
  let mut esum = vec![DVec3::ZERO; nv];
  for (i, &(a, b)) in keys.iter().enumerate() {
    let (pa, pb) = (pos[a as usize], pos[b as usize]);
    let is_sharp = count[&(a, b)] != 2 || creases.contains(&(a, b));
    edge_pts.push(if is_sharp { (pa + pb) / 2.0 } else { (pa + pb + edge_faces[&(a, b)]) / 4.0 });
    edge_pt.insert((a, b), edge0 + i as u32);
    esum[a as usize] += (pa + pb) / 2.0;
    esum[b as usize] += (pa + pb) / 2.0;
  }
  let mut out = Vec::with_capacity(nv + polys.len() + keys.len());
  for v in 0..nv {
    let p = pos[v];
    let s = &sharp[v];
    out.push(if nfaces[v] == 0 || s.len() >= 3 || (s.len() == 2 && nfaces[v] == 1) {
      p
    } else if s.len() == 2 {
      (pos[s[0] as usize] + 6.0 * p + pos[s[1] as usize]) / 8.0
    } else {
      let n = f64::from(nfaces[v]);
      let q = fsum[v] / n;
      let r = esum[v] / n;
      (q + 2.0 * r + (n - 3.0) * p) / n
    });
  }
  out.extend(&face_pts);
  out.extend(&edge_pts);
  let mut new_creases = HashSet::new();
  for &(a, b) in creases {
    if let Some(&e) = edge_pt.get(&(a, b)) {
      new_creases.insert(key(a, e));
      new_creases.insert(key(e, b));
    }
  }
  Step { pos: out, face0: nv as u32, edge_pt, creases: new_creases }
}

impl Level {
  pub fn of(cage: &Cage) -> Level {
    Level {
      pos: cage.vertices.iter().map(|v| DVec3::from(*v)).collect(),
      polys: cage.faces.clone(),
      creases: cage.creases.iter().map(|c| key(c[0], c[1])).collect(),
    }
  }

  /// One Catmull-Clark step on any polygons: each n-sided face becomes n quads.
  pub fn subdivide(&self) -> Level {
    let st = step(&self.pos, &self.polys, &self.creases);
    let e = |a: u32, b: u32| st.edge_pt[&key(a, b)];
    let mut polys = Vec::new();
    for (fi, f) in self.polys.iter().enumerate() {
      let n = f.len();
      for i in 0..n {
        let (prev, v, next) = (f[(i + n - 1) % n], f[i], f[(i + 1) % n]);
        polys.push(vec![v, e(v, next), st.face0 + fi as u32, e(prev, v)]);
      }
    }
    Level { pos: st.pos, polys, creases: st.creases }
  }

  /// Every vertex pushed to its position on the limit surface (the polygons must be quads).
  pub fn limit(&self) -> Vec<DVec3> {
    let nv = self.pos.len();
    let (sharp, _, _) = sharpness(nv, &self.polys, &self.creases);
    let (mut edge2, mut diag, mut count) = (vec![DVec3::ZERO; nv], vec![DVec3::ZERO; nv], vec![0u32; nv]);
    for q in &self.polys {
      for k in 0..4 {
        let v = q[k] as usize;
        edge2[v] += self.pos[q[(k + 1) % 4] as usize] + self.pos[q[(k + 3) % 4] as usize];
        diag[v] += self.pos[q[(k + 2) % 4] as usize];
        count[v] += 1;
      }
    }
    (0..nv)
      .map(|v| {
        let (p, s) = (self.pos[v], &sharp[v]);
        if count[v] == 0 || s.len() >= 3 || (s.len() == 2 && count[v] == 1) {
          return p;
        }
        if s.len() == 2 {
          return (self.pos[s[0] as usize] + 4.0 * p + self.pos[s[1] as usize]) / 6.0;
        }
        let n = f64::from(count[v]);
        // each edge neighbour was counted by both faces beside it
        (n * n * p + 2.0 * edge2[v] + diag[v]) / (n * (n + 5.0))
      })
      .collect()
  }

  fn as_cage(&self) -> Cage {
    Cage { vertices: self.pos.iter().map(|p| p.to_array()).collect(), faces: self.polys.clone(), ..Cage::default() }
  }
}

/// The base quad mesh of a cage: the cage itself if it's all quads, else the cage subdivided once.
pub(crate) fn base(cage: &Cage) -> Result<Level, String> {
  cage.check()?;
  let level = Level::of(cage);
  Ok(if cage.faces.iter().all(|f| f.len() == 4) { level } else { level.subdivide() })
}

/// The limit surface as a quad mesh: the cage subdivided `levels` times (at least once) with
/// every vertex on the limit surface.
pub fn limit_mesh(cage: &Cage, levels: u32) -> Result<(Vec<DVec3>, Vec<[u32; 4]>), String> {
  let mut l = base(cage)?;
  for _ in 1..levels.max(1) {
    l = l.subdivide();
  }
  let pos = l.limit();
  Ok((pos, l.polys.iter().map(|q| [q[0], q[1], q[2], q[3]]).collect()))
}

/// A subdivided base mesh: each base face is an n x n grid of quads, `grids[f][t * (n + 1) + s]`
/// being the vertex at (s, t) with the face's corners at (0, 0), (n, 0), (n, n), (0, n) in order.
struct Grids {
  level: Level,
  n: usize,
  grids: Vec<Vec<u32>>,
}

impl Grids {
  fn of(base: &Level) -> Grids {
    Grids { level: base.clone(), n: 1, grids: base.polys.iter().map(|f| vec![f[0], f[1], f[3], f[2]]).collect() }
  }

  fn quads(&self) -> Vec<Vec<u32>> {
    let n = self.n;
    self
      .grids
      .iter()
      .flat_map(|g| {
        (0..n).flat_map(move |t| {
          (0..n).map(move |s| {
            let at = |s: usize, t: usize| g[t * (n + 1) + s];
            vec![at(s, t), at(s + 1, t), at(s + 1, t + 1), at(s, t + 1)]
          })
        })
      })
      .collect()
  }

  fn subdivide(&self) -> Grids {
    let quads = self.quads();
    let st = step(&self.level.pos, &quads, &self.level.creases);
    let edge = |a: u32, b: u32| st.edge_pt[&key(a, b)];
    let (n, m) = (self.n, 2 * self.n);
    let mut grids = Vec::with_capacity(self.grids.len());
    for (fi, g) in self.grids.iter().enumerate() {
      let at = |s: usize, t: usize| g[t * (n + 1) + s];
      let mut ng = vec![0u32; (m + 1) * (m + 1)];
      for t in 0..=n {
        for s in 0..=n {
          ng[2 * t * (m + 1) + 2 * s] = at(s, t);
          if s < n {
            ng[2 * t * (m + 1) + 2 * s + 1] = edge(at(s, t), at(s + 1, t));
          }
          if t < n {
            ng[(2 * t + 1) * (m + 1) + 2 * s] = edge(at(s, t), at(s, t + 1));
          }
          if s < n && t < n {
            ng[(2 * t + 1) * (m + 1) + 2 * s + 1] = st.face0 + (fi * n * n + t * n + s) as u32;
          }
        }
      }
      grids.push(ng);
    }
    let mut level = Level { pos: st.pos, polys: Vec::new(), creases: st.creases };
    level.polys = Grids { level: level.clone(), n: m, grids: grids.clone() }.quads();
    Grids { level, n: m, grids }
  }
}

/// One base face placed in a region's grid: cell (i, j), with the face's corner `offset` at the
/// cell's (i, j) corner and its next corners at (i + 1, j), (i + 1, j + 1), (i, j + 1).
struct Cell {
  face: usize,
  offset: usize,
  i: usize,
  j: usize,
}

/// A rectangular grid of base faces that becomes one patch.
struct Region {
  width: usize,
  height: usize,
  cells: Vec<Cell>,
  /// The vertex at each grid point.
  at: HashMap<(usize, usize), u32>,
}

impl Region {
  fn corners(&self) -> [u32; 4] {
    let (w, h) = (self.width, self.height);
    [self.at[&(0, 0)], self.at[&(w, 0)], self.at[&(w, h)], self.at[&(0, h)]]
  }
}

/// The base mesh's topology and the edges patches may not cross.
struct Layout<'a> {
  base: &'a Cage,
  topo: Topo,
  cut: HashSet<(u32, u32)>,
}

impl Layout<'_> {
  fn regular(&self, v: u32) -> bool {
    self.topo.nbrs[v as usize].len() == 4 && !self.topo.is_boundary_vertex(v)
  }

  /// At a regular vertex `w` reached from `u`, the vertex straight on.
  fn straight_on(&self, u: u32, w: u32) -> Option<u32> {
    let (f, k) = *self.topo.half.get(&(u, w))?;
    let x = self.base.faces[f][(k + 2) % 4];
    let (g, m) = *self.topo.half.get(&(x, w))?;
    Some(self.base.faces[g][(m + 2) % 4])
  }

  /// Cuts along the edge line through a → b, both ways, until it meets an irregular vertex, the
  /// boundary or an edge already cut.
  fn cut_line(&mut self, a: u32, b: u32) {
    for (mut prev, mut cur, first) in [(a, b, true), (b, a, false)] {
      if first && !self.cut.insert(key(prev, cur)) {
        continue;
      }
      while self.regular(cur) {
        let Some(next) = self.straight_on(prev, cur) else { break };
        (prev, cur) = (cur, next);
        if !self.cut.insert(key(prev, cur)) {
          break;
        }
      }
    }
  }

  /// Faces joined across uncut edges, each group laid out as a grid if it is one, else a region
  /// per face. `Err(face)` if a group wraps round on itself (and `seams`): cut seams there.
  fn regions(&self, seams: bool) -> Result<Vec<Region>, usize> {
    let faces = &self.base.faces;
    let mut region = vec![usize::MAX; faces.len()];
    let mut groups = Vec::new();
    for start in 0..faces.len() {
      if region[start] != usize::MAX {
        continue;
      }
      let id = groups.len();
      region[start] = id;
      let mut members = vec![start];
      let mut i = 0;
      while i < members.len() {
        let f = &faces[members[i]];
        for k in 0..4 {
          let (a, b) = (f[k], f[(k + 1) % 4]);
          if self.cut.contains(&key(a, b)) {
            continue;
          }
          let Some(&(g, _)) = self.topo.half.get(&(b, a)) else { continue };
          if region[g] == usize::MAX {
            region[g] = id;
            members.push(g);
          }
        }
        i += 1;
      }
      groups.push(members);
    }
    let mut out = Vec::new();
    for members in groups {
      match self.grid(&members) {
        Some(r) => out.push(r),
        None => {
          // a group with no cut edge inside that still isn't a grid wraps round: cut seams
          let inner = members.iter().any(|&m| {
            let g = &faces[m];
            (0..4).any(|k| {
              let (a, b) = (g[k], g[(k + 1) % 4]);
              self.cut.contains(&key(a, b)) && self.topo.half.get(&(b, a)).is_some_and(|h| region[h.0] == region[m])
            })
          });
          if seams && !inner && members.len() > 1 {
            return Err(members[0]);
          }
          out.extend(members.iter().map(|&m| {
            let g = &faces[m];
            let at = HashMap::from([((0, 0), g[0]), ((1, 0), g[1]), ((1, 1), g[2]), ((0, 1), g[3])]);
            Region { width: 1, height: 1, cells: vec![Cell { face: m, offset: 0, i: 0, j: 0 }], at }
          }));
        }
      }
    }
    Ok(out)
  }

  /// Lays a group's faces out on a rectangular grid, if they form one with no cut edge inside.
  fn grid(&self, members: &[usize]) -> Option<Region> {
    let faces = &self.base.faces;
    let mut coord: HashMap<u32, (i64, i64)> = HashMap::new();
    let mut at: HashMap<(i64, i64), u32> = HashMap::new();
    let mut cells: HashMap<(i64, i64), (usize, usize)> = HashMap::new();
    let mut placed: HashSet<usize> = HashSet::new();
    let mut place = |face: usize,
                     offset: usize,
                     cell: (i64, i64),
                     coord: &mut HashMap<u32, (i64, i64)>,
                     at: &mut HashMap<(i64, i64), u32>|
     -> Option<()> {
      let f = &faces[face];
      let corners = [(0, 0), (1, 0), (1, 1), (0, 1)];
      for (m, c) in corners.iter().enumerate() {
        let v = f[(offset + m) % 4];
        let p = (cell.0 + c.0, cell.1 + c.1);
        if *coord.entry(v).or_insert(p) != p || *at.entry(p).or_insert(v) != v {
          return None;
        }
      }
      if cells.insert(cell, (face, offset)).is_some() {
        return None;
      }
      Some(())
    };
    place(members[0], 0, (0, 0), &mut coord, &mut at)?;
    placed.insert(members[0]);
    let mut queue = vec![members[0]];
    while let Some(face) = queue.pop() {
      let f = &faces[face];
      for k in 0..4 {
        let (a, b) = (f[k], f[(k + 1) % 4]);
        if self.cut.contains(&key(a, b)) {
          continue;
        }
        let Some(&(g, p)) = self.topo.half.get(&(b, a)) else { continue };
        if placed.contains(&g) {
          continue;
        }
        let (pa, pb) = (coord[&a], coord[&b]);
        let t = (pb.0 - pa.0, pb.1 - pa.1);
        let d = (t.1, -t.0);
        // g runs b, a, a + d, b + d
        let quad = [pb, pa, (pa.0 + d.0, pa.1 + d.1), (pb.0 + d.0, pb.1 + d.1)];
        let min = quad.iter().copied().min()?;
        let m = quad.iter().position(|q| *q == min)?;
        place(g, (p + m) % 4, min, &mut coord, &mut at)?;
        placed.insert(g);
        queue.push(g);
      }
    }
    let (lo_i, lo_j) = (cells.keys().map(|c| c.0).min()?, cells.keys().map(|c| c.1).min()?);
    let (hi_i, hi_j) = (cells.keys().map(|c| c.0).max()?, cells.keys().map(|c| c.1).max()?);
    let (width, height) = ((hi_i - lo_i + 1) as usize, (hi_j - lo_j + 1) as usize);
    if cells.len() != members.len() || cells.len() != width * height {
      return None;
    }
    // no cut edge between two of its cells
    for (&(i, j), _) in cells.iter() {
      for (di, dj, a, b) in [(1, 0, (i + 1, j), (i + 1, j + 1)), (0, 1, (i, j + 1), (i + 1, j + 1))] {
        if cells.contains_key(&(i + di, j + dj)) && self.cut.contains(&key(at[&a], at[&b])) {
          return None;
        }
      }
    }
    let at = at.into_iter().map(|((i, j), v)| (((i - lo_i) as usize, (j - lo_j) as usize), v)).collect();
    let cells = cells
      .into_iter()
      .map(|((i, j), (face, offset))| Cell { face, offset, i: (i - lo_i) as usize, j: (j - lo_j) as usize })
      .collect();
    Some(Region { width, height, cells, at })
  }

  /// The regions, split until no region's side passes another region's corner.
  fn conforming_regions(&mut self) -> Vec<Region> {
    let mut seams = true;
    loop {
      let regions = match self.regions(seams) {
        Ok(r) => r,
        Err(face) => {
          // the lines along all four sides of one of its faces
          let before = self.cut.len();
          let f = self.base.faces[face].clone();
          for k in 0..4 {
            self.cut_line(f[k], f[(k + 1) % 4]);
          }
          seams = self.cut.len() > before;
          continue;
        }
      };
      let corners: HashSet<u32> = regions.iter().flat_map(Region::corners).collect();
      let mut lines = Vec::new();
      for r in &regions {
        let (w, h) = (r.width, r.height);
        for i in 1..w {
          if corners.contains(&r.at[&(i, 0)]) || corners.contains(&r.at[&(i, h)]) {
            lines.push((r.at[&(i, 0)], r.at[&(i, 1)]));
          }
        }
        for j in 1..h {
          if corners.contains(&r.at[&(0, j)]) || corners.contains(&r.at[&(w, j)]) {
            lines.push((r.at[&(0, j)], r.at[&(1, j)]));
          }
        }
      }
      if lines.is_empty() {
        return regions;
      }
      for (a, b) in lines {
        self.cut_line(a, b);
      }
    }
  }
}

/// The limit surface as bicubic B-spline patches, one per B-rep face (see the module docs).
pub fn patches(cage: &Cage) -> Result<Vec<Patch>, String> {
  let base = base(cage)?;
  let base_cage = base.as_cage();
  let topo = Topo::of(&base_cage)?;
  let mut layout = Layout { base: &base_cage, topo, cut: HashSet::new() };
  // creases and the boundary
  for f in &base_cage.faces {
    for k in 0..4 {
      let (a, b) = (f[k], f[(k + 1) % 4]);
      if base.creases.contains(&key(a, b)) || layout.topo.is_boundary_edge(a, b) {
        layout.cut.insert(key(a, b));
      }
    }
  }
  // star lines
  for v in 0..base.pos.len() as u32 {
    let valence = layout.topo.nbrs[v as usize].len();
    let star = if layout.topo.is_boundary_vertex(v) { valence > 3 } else { valence != 4 };
    if star {
      for u in layout.topo.nbrs[v as usize].clone() {
        layout.cut_line(v, u);
      }
    }
  }
  let regions = layout.conforming_regions();

  let mut level = 0;
  while level < 5 && base.polys.len() * 4usize.pow(level + 1) <= SAMPLE_QUADS {
    level += 1;
  }
  let mut mesh = Grids::of(&base);
  for _ in 0..level.max(2) {
    mesh = mesh.subdivide();
  }
  let limit = mesh.level.limit();
  let n = mesh.n;
  let mut out = Vec::with_capacity(regions.len());
  for region in regions {
    let (w, h) = (region.width, region.height);
    let mut grid = vec![vec![DVec3::ZERO; h * n + 1]; w * n + 1];
    for cell in &region.cells {
      let g = &mesh.grids[cell.face];
      let corners = [(0usize, 0usize), (n, 0), (n, n), (0, n)];
      let c0 = corners[cell.offset];
      let c1 = corners[(cell.offset + 1) % 4];
      let c3 = corners[(cell.offset + 3) % 4];
      let step = |a: usize, b: usize| (b as isize - a as isize) / n as isize;
      let (du, dv) = ((step(c0.0, c1.0), step(c0.1, c1.1)), (step(c0.0, c3.0), step(c0.1, c3.1)));
      for s in 0..=n {
        for t in 0..=n {
          let ls = (c0.0 as isize + du.0 * s as isize + dv.0 * t as isize) as usize;
          let lt = (c0.1 as isize + du.1 * s as isize + dv.1 * t as isize) as usize;
          grid[cell.i * n + s][cell.j * n + t] = limit[g[lt * (n + 1) + ls] as usize];
        }
      }
    }
    out.push(interpolate(&grid)?);
  }
  Ok(out)
}

/// How well the patches join: the largest gap between neighbouring patches' sides and the largest
/// angle (degrees) between their normals across uncreased seams, from `samples` points per side.
/// Also the number of side points with no neighbour (open boundary or a gap).
pub fn seam_report(patches: &[Patch], creased: impl Fn(DVec3) -> bool, samples: usize) -> (f64, f64, usize) {
  let mut pts: Vec<(usize, DVec3, DVec3)> = Vec::new();
  for (pi, p) in patches.iter().enumerate() {
    for k in 1..samples {
      let t = k as f64 / samples as f64;
      for (u, v) in [(t, 0.0), (t, 1.0), (0.0, t), (1.0, t)] {
        pts.push((pi, p.eval(u, v), p.normal(u, v)));
      }
    }
  }
  let (mut gap, mut angle, mut lonely) = (0.0f64, 0.0f64, 0);
  for (i, a) in pts.iter().enumerate() {
    let best = pts
      .iter()
      .enumerate()
      .filter(|(j, b)| *j != i && b.0 != a.0)
      .map(|(_, b)| (a.1.distance(b.1), b))
      .min_by(|x, y| x.0.total_cmp(&y.0));
    match best {
      Some((d, b)) if d < 1e-3 => {
        gap = gap.max(d);
        if !creased(a.1) {
          angle = angle.max(a.2.angle_between(b.2).to_degrees());
        }
      }
      _ => lonely += 1,
    }
  }
  (gap, angle, lonely)
}
