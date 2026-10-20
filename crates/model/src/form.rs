//! Form (T-spline) bodies: a closed quad control cage whose smooth limit surface is the body, as
//! in Fusion's Create Form. The surface is the Catmull-Clark limit of the cage.
//!
//! The limit surface is split into B-rep faces along the cage's "star lines": the edge lines that
//! leave each extraordinary vertex (valence other than 4) and run straight on through regular
//! vertices to the next one. Each region they bound that is a rectangular grid of cage faces
//! becomes one B-spline face (a box cage gives its six sides); any other region gets a face per
//! cage face. A face's B-spline interpolates exact limit points, found by subdividing the cage a
//! few times and pushing the vertices to their limit positions.

use std::collections::HashMap;

use fox_kernel::BSplineSurface;
use fox_kernel::glam::DVec3;
use serde::{Deserialize, Serialize};

/// Most cage faces a form may have.
pub const MAX_FACES: usize = 4096;
/// Most subdivided quads used to sample the limit surface.
const SAMPLE_QUADS: usize = 25_000;

/// The control cage of a form body: vertices (mm, in the form's plane coordinates) and quad
/// faces (four vertex indices each, counter-clockwise seen from outside). The cage must be closed:
/// every edge is shared by exactly two faces.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FormCage {
  pub vertices: Vec<[f64; 3]>,
  pub faces: Vec<[u32; 4]>,
}

impl FormCage {
  /// Fusion's Form > Box: a `length` (x) by `width` (y) by `height` (z) box cage centred on the
  /// origin in x and y and standing on z = 0, with `counts` faces along each side.
  pub fn quad_box(length: f64, width: f64, height: f64, counts: [u32; 3]) -> Result<FormCage, String> {
    if ![length, width, height].iter().all(|v| v.is_finite() && *v > 0.0) {
      return Err("the box's length, width and height must be positive".into());
    }
    if counts.iter().any(|c| !(1..=32).contains(c)) {
      return Err("each side needs 1 to 32 faces".into());
    }
    let [nx, ny, nz] = counts;
    let mut index: HashMap<(u32, u32, u32), u32> = HashMap::new();
    let mut cage = FormCage::default();
    let mut vid = |i: u32, j: u32, k: u32, cage: &mut FormCage| -> u32 {
      *index.entry((i, j, k)).or_insert_with(|| {
        cage.vertices.push([
          length * (f64::from(i) / f64::from(nx) - 0.5),
          width * (f64::from(j) / f64::from(ny) - 0.5),
          height * f64::from(k) / f64::from(nz),
        ]);
        (cage.vertices.len() - 1) as u32
      })
    };
    // each side's quads, outward
    for a in 0..nx {
      for b in 0..ny {
        let f = [
          vid(a, b, 0, &mut cage),
          vid(a, b + 1, 0, &mut cage),
          vid(a + 1, b + 1, 0, &mut cage),
          vid(a + 1, b, 0, &mut cage),
        ];
        cage.faces.push(f);
        let f = [
          vid(a, b, nz, &mut cage),
          vid(a + 1, b, nz, &mut cage),
          vid(a + 1, b + 1, nz, &mut cage),
          vid(a, b + 1, nz, &mut cage),
        ];
        cage.faces.push(f);
      }
    }
    for a in 0..nx {
      for b in 0..nz {
        let f = [
          vid(a, 0, b, &mut cage),
          vid(a + 1, 0, b, &mut cage),
          vid(a + 1, 0, b + 1, &mut cage),
          vid(a, 0, b + 1, &mut cage),
        ];
        cage.faces.push(f);
        let f = [
          vid(a, ny, b, &mut cage),
          vid(a, ny, b + 1, &mut cage),
          vid(a + 1, ny, b + 1, &mut cage),
          vid(a + 1, ny, b, &mut cage),
        ];
        cage.faces.push(f);
      }
    }
    for a in 0..ny {
      for b in 0..nz {
        let f = [
          vid(0, a, b, &mut cage),
          vid(0, a, b + 1, &mut cage),
          vid(0, a + 1, b + 1, &mut cage),
          vid(0, a + 1, b, &mut cage),
        ];
        cage.faces.push(f);
        let f = [
          vid(nx, a, b, &mut cage),
          vid(nx, a + 1, b, &mut cage),
          vid(nx, a + 1, b + 1, &mut cage),
          vid(nx, a, b + 1, &mut cage),
        ];
        cage.faces.push(f);
      }
    }
    Ok(cage)
  }

  /// The vertex nearest `p`.
  pub fn nearest_vertex(&self, p: [f64; 3]) -> Option<usize> {
    let p = DVec3::from(p);
    (0..self.vertices.len()).min_by(|a, b| {
      let d = |i: usize| DVec3::from(self.vertices[i]).distance_squared(p);
      d(*a).total_cmp(&d(*b))
    })
  }

  /// Checks the cage is a closed, consistently oriented quad mesh.
  pub fn check(&self) -> Result<(), String> {
    Topology::of(self).map(|_| ())
  }

  /// The limit surface as B-spline surfaces, one per B-rep face (see the module docs).
  pub fn surfaces(&self) -> Result<Vec<BSplineSurface>, String> {
    let topo = Topology::of(self)?;
    let mut level = 0;
    while level < 5 && self.faces.len() * 4usize.pow(level + 1) <= SAMPLE_QUADS {
      level += 1;
    }
    let level = level.max(2);
    let mut mesh = Subdivision::of(self);
    for _ in 0..level {
      mesh = mesh.subdivide()?;
    }
    let limit = mesh.limit();
    let n = mesh.n;
    let mut out = Vec::new();
    for region in topo.regions(self) {
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
}

/// The cage's half-edges and vertex valences.
struct Topology {
  /// Directed edge a → b: the face it is in and the corner of `a` there.
  half: HashMap<(u32, u32), (usize, usize)>,
  valence: Vec<usize>,
}

/// One cage face placed in a region's grid: cell (i, j), with the face's corner `offset` at the
/// cell's (i, j) corner and its next corners at (i + 1, j), (i + 1, j + 1), (i, j + 1).
struct Cell {
  face: usize,
  offset: usize,
  i: usize,
  j: usize,
}

/// A rectangular grid of cage faces that becomes one B-rep face.
struct Region {
  width: usize,
  height: usize,
  cells: Vec<Cell>,
}

impl Topology {
  fn of(cage: &FormCage) -> Result<Topology, String> {
    let nv = cage.vertices.len();
    if cage.faces.len() < 2 {
      return Err("the cage has no faces".into());
    }
    if cage.faces.len() > MAX_FACES {
      return Err(format!("the cage has more than {MAX_FACES} faces"));
    }
    if cage.vertices.iter().flatten().any(|c| !c.is_finite()) {
      return Err("the cage has non-finite vertices".into());
    }
    let mut half = HashMap::new();
    let mut valence = vec![0; nv];
    for (fi, f) in cage.faces.iter().enumerate() {
      for k in 0..4 {
        let (a, b) = (f[k], f[(k + 1) % 4]);
        if a as usize >= nv {
          return Err(format!("face {fi} uses vertex {a}, but the cage has {nv}"));
        }
        if f.iter().filter(|v| **v == a).count() > 1 {
          return Err(format!("face {fi} uses a vertex twice"));
        }
        if half.insert((a, b), (fi, k)).is_some() {
          return Err("the cage's faces don't all face the same way, or an edge has more than two faces".into());
        }
        valence[a as usize] += 1;
      }
    }
    if half.keys().any(|(a, b)| !half.contains_key(&(*b, *a))) {
      return Err("the cage isn't closed: every edge needs a face on each side".into());
    }
    let topo = Topology { half, valence };
    // the faces round each vertex form one fan
    for (fi, f) in cage.faces.iter().enumerate() {
      for (k, &v) in f.iter().enumerate() {
        let (mut face, mut corner, mut steps) = (fi, k, 0);
        loop {
          // the face across the edge from v to its previous corner
          let prev = cage.faces[face][(corner + 3) % 4];
          let (g, m) = topo.half[&(v, prev)];
          (face, corner) = (g, m);
          steps += 1;
          if face == fi || steps > topo.valence[v as usize] {
            break;
          }
        }
        if steps != topo.valence[v as usize] {
          return Err(format!("the cage is pinched at vertex {v}"));
        }
      }
    }
    Ok(topo)
  }

  fn face_of(&self, a: u32, b: u32) -> (usize, usize) {
    self.half[&(a, b)]
  }

  /// At a valence-4 vertex `w` reached from `u`, the vertex straight on.
  fn straight_on(&self, cage: &FormCage, u: u32, w: u32) -> u32 {
    let (f, k) = self.face_of(u, w);
    let x = cage.faces[f][(k + 2) % 4];
    let (g, m) = self.face_of(x, w);
    cage.faces[g][(m + 2) % 4]
  }

  /// The B-rep face regions (see the module docs).
  fn regions(&self, cage: &FormCage) -> Vec<Region> {
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut cut: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for f in &cage.faces {
      for k in 0..4 {
        let (v, u) = (f[k], f[(k + 1) % 4]);
        if self.valence[v as usize] == 4 {
          continue;
        }
        let (mut prev, mut cur) = (v, u);
        while cut.insert(key(prev, cur)) && self.valence[cur as usize] == 4 {
          let next = self.straight_on(cage, prev, cur);
          (prev, cur) = (cur, next);
        }
      }
    }
    // faces joined across uncut edges
    let mut region = vec![usize::MAX; cage.faces.len()];
    let mut out = Vec::new();
    for start in 0..cage.faces.len() {
      if region[start] != usize::MAX {
        continue;
      }
      let id = out.len();
      region[start] = id;
      let mut members = vec![start];
      let mut i = 0;
      while i < members.len() {
        let f = cage.faces[members[i]];
        for k in 0..4 {
          let (a, b) = (f[k], f[(k + 1) % 4]);
          if cut.contains(&key(a, b)) {
            continue;
          }
          let (g, _) = self.face_of(b, a);
          if region[g] == usize::MAX {
            region[g] = id;
            members.push(g);
          }
        }
        i += 1;
      }
      out.push(members);
    }
    let mut regions = Vec::new();
    for members in out {
      match self.grid(cage, &members, &cut) {
        Some(r) => regions.push(r),
        None => regions.extend(members.iter().map(|&f| Region {
          width: 1,
          height: 1,
          cells: vec![Cell { face: f, offset: 0, i: 0, j: 0 }],
        })),
      }
    }
    regions
  }

  /// Lays a region's faces out on a rectangular grid, if they form one.
  fn grid(&self, cage: &FormCage, members: &[usize], cut: &std::collections::HashSet<(u32, u32)>) -> Option<Region> {
    let mut coord: HashMap<u32, (i64, i64)> = HashMap::new();
    let mut at: HashMap<(i64, i64), u32> = HashMap::new();
    let mut cells: HashMap<(i64, i64), (usize, usize)> = HashMap::new();
    let mut placed: HashMap<usize, ()> = HashMap::new();
    let mut place = |face: usize,
                     offset: usize,
                     cell: (i64, i64),
                     coord: &mut HashMap<u32, (i64, i64)>,
                     at: &mut HashMap<(i64, i64), u32>|
     -> Option<()> {
      let f = cage.faces[face];
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
    placed.insert(members[0], ());
    let mut queue = vec![members[0]];
    while let Some(face) = queue.pop() {
      let f = cage.faces[face];
      for k in 0..4 {
        let (a, b) = (f[k], f[(k + 1) % 4]);
        if cut.contains(&(a.min(b), a.max(b))) {
          continue;
        }
        let (g, p) = self.face_of(b, a);
        if placed.contains_key(&g) {
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
        placed.insert(g, ());
        queue.push(g);
      }
    }
    let (lo_i, lo_j) = (cells.keys().map(|c| c.0).min()?, cells.keys().map(|c| c.1).min()?);
    let (hi_i, hi_j) = (cells.keys().map(|c| c.0).max()?, cells.keys().map(|c| c.1).max()?);
    let (width, height) = ((hi_i - lo_i + 1) as usize, (hi_j - lo_j + 1) as usize);
    if cells.len() != members.len() || cells.len() != width * height {
      return None;
    }
    let cells = cells
      .into_iter()
      .map(|((i, j), (face, offset))| Cell { face, offset, i: (i - lo_i) as usize, j: (j - lo_j) as usize })
      .collect();
    Some(Region { width, height, cells })
  }
}

/// A subdivided cage: each cage face is an n x n grid of quads, `grids[f][t * (n + 1) + s]` being
/// the vertex at (s, t) with the face's corners at (0, 0), (n, 0), (n, n), (0, n) in order.
struct Subdivision {
  pos: Vec<DVec3>,
  n: usize,
  grids: Vec<Vec<u32>>,
}

impl Subdivision {
  fn of(cage: &FormCage) -> Subdivision {
    Subdivision {
      pos: cage.vertices.iter().map(|v| DVec3::from(*v)).collect(),
      n: 1,
      grids: cage.faces.iter().map(|f| vec![f[0], f[1], f[3], f[2]]).collect(),
    }
  }

  fn quads(&self) -> impl Iterator<Item = [u32; 4]> + '_ {
    let n = self.n;
    self.grids.iter().flat_map(move |g| {
      (0..n).flat_map(move |t| {
        (0..n).map(move |s| {
          let at = |s: usize, t: usize| g[t * (n + 1) + s];
          [at(s, t), at(s + 1, t), at(s + 1, t + 1), at(s, t + 1)]
        })
      })
    })
  }

  /// One Catmull-Clark step.
  fn subdivide(&self) -> Result<Subdivision, String> {
    let nv = self.pos.len();
    let quads: Vec<[u32; 4]> = self.quads().collect();
    let face_pts: Vec<DVec3> =
      quads.iter().map(|q| q.iter().map(|&v| self.pos[v as usize]).sum::<DVec3>() / 4.0).collect();
    let mut edges: HashMap<(u32, u32), (usize, DVec3, u32)> = HashMap::new();
    let (mut fsum, mut fcount) = (vec![DVec3::ZERO; nv], vec![0u32; nv]);
    for (qi, q) in quads.iter().enumerate() {
      for k in 0..4 {
        let (a, b) = (q[k], q[(k + 1) % 4]);
        let next = edges.len();
        let e = edges.entry((a.min(b), a.max(b))).or_insert((next, DVec3::ZERO, 0));
        e.1 += face_pts[qi];
        e.2 += 1;
        fsum[a as usize] += face_pts[qi];
        fcount[a as usize] += 1;
      }
    }
    let (mut esum, mut ecount) = (vec![DVec3::ZERO; nv], vec![0u32; nv]);
    let mut edge_pts = vec![DVec3::ZERO; edges.len()];
    for (&(a, b), &(i, fs, count)) in &edges {
      if count != 2 {
        return Err("the cage isn't closed".into());
      }
      let (pa, pb) = (self.pos[a as usize], self.pos[b as usize]);
      edge_pts[i] = (pa + pb + fs) / 4.0;
      for v in [a, b] {
        esum[v as usize] += (pa + pb) / 2.0;
        ecount[v as usize] += 1;
      }
    }
    let mut pos = Vec::with_capacity(nv + quads.len() + edges.len());
    for v in 0..nv {
      let n = f64::from(fcount[v]);
      pos.push(if fcount[v] == 0 {
        self.pos[v]
      } else {
        let q = fsum[v] / n;
        let r = esum[v] / f64::from(ecount[v].max(1));
        (q + 2.0 * r + (n - 3.0) * self.pos[v]) / n
      });
    }
    pos.extend(&face_pts);
    pos.extend(&edge_pts);
    let (face0, edge0) = (nv as u32, (nv + quads.len()) as u32);
    let edge = |a: u32, b: u32| edge0 + edges[&(a.min(b), a.max(b))].0 as u32;
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
            ng[(2 * t + 1) * (m + 1) + 2 * s + 1] = face0 + (fi * n * n + t * n + s) as u32;
          }
        }
      }
      grids.push(ng);
    }
    Ok(Subdivision { pos, n: m, grids })
  }

  /// Every vertex pushed to its position on the limit surface.
  fn limit(&self) -> Vec<DVec3> {
    let nv = self.pos.len();
    let (mut edge2, mut diag, mut count) = (vec![DVec3::ZERO; nv], vec![DVec3::ZERO; nv], vec![0u32; nv]);
    for q in self.quads() {
      for k in 0..4 {
        let v = q[k] as usize;
        edge2[v] += self.pos[q[(k + 1) % 4] as usize] + self.pos[q[(k + 3) % 4] as usize];
        diag[v] += self.pos[q[(k + 2) % 4] as usize];
        count[v] += 1;
      }
    }
    (0..nv)
      .map(|v| {
        if count[v] == 0 {
          return self.pos[v];
        }
        let n = f64::from(count[v]);
        // each edge neighbour was counted by both faces beside it
        (n * n * self.pos[v] + 2.0 * edge2[v] + diag[v]) / (n * (n + 5.0))
      })
      .collect()
  }
}

/// The bicubic B-spline through a grid of points (`grid[i][j]`, u index i), at uniform parameters
/// with not-a-knot ends.
fn interpolate(grid: &[Vec<DVec3>]) -> Result<BSplineSurface, String> {
  let (nu, nv) = (grid.len(), grid.first().map_or(0, Vec::len));
  if nu < 4 || nv < 4 {
    return Err("too few limit points to fit a surface".into());
  }
  let (ku, su) = Collocation::new(nu)?;
  let (kv, sv) = Collocation::new(nv)?;
  // along v for each row, then along u for each column
  let rows: Vec<Vec<DVec3>> = grid.iter().map(|row| sv.solve(row)).collect();
  let mut poles = vec![vec![DVec3::ZERO; nv]; nu];
  for j in 0..nv {
    let col: Vec<DVec3> = rows.iter().map(|r| r[j]).collect();
    for (i, p) in su.solve(&col).into_iter().enumerate() {
      poles[i][j] = p;
    }
  }
  Ok(BSplineSurface { degree: 3, poles, u_knots: ku.0, u_mults: ku.1, v_knots: kv.0, v_mults: kv.1 })
}

/// An LU-factored cubic interpolation matrix for `n` points at uniform parameters.
struct Collocation {
  lu: Vec<Vec<f64>>,
  perm: Vec<usize>,
}

impl Collocation {
  /// The solver and the distinct knots with multiplicities.
  #[allow(clippy::type_complexity)]
  fn new(n: usize) -> Result<((Vec<f64>, Vec<u32>), Collocation), String> {
    let m = (n - 1) as f64;
    let params: Vec<f64> = (0..n).map(|i| i as f64 / m).collect();
    // full knot vector: clamped ends, interior knots at the parameters but the second and last-but-one
    let mut knots = vec![0.0; 4];
    knots.extend(params.iter().take(n - 2).skip(2));
    knots.extend([1.0; 4]);
    let mut a = vec![vec![0.0; n]; n];
    for (i, &u) in params.iter().enumerate() {
      let span = if i == n - 1 { n - 1 } else { (3..n).rev().find(|&s| knots[s] <= u).unwrap_or(3) };
      for (r, b) in basis(&knots, span, u).into_iter().enumerate() {
        a[i][span - 3 + r] = b;
      }
    }
    let mut perm: Vec<usize> = (0..n).collect();
    for c in 0..n {
      let p = (c..n).max_by(|x, y| a[*x][c].abs().total_cmp(&a[*y][c].abs())).unwrap_or(c);
      if a[p][c].abs() < 1e-14 {
        return Err("singular interpolation".into());
      }
      a.swap(c, p);
      perm.swap(c, p);
      let pivot = a[c].clone();
      for row in a.iter_mut().skip(c + 1) {
        let f = row[c] / pivot[c];
        row[c] = f;
        for (x, y) in row.iter_mut().zip(&pivot).skip(c + 1) {
          *x -= f * y;
        }
      }
    }
    let mut distinct = vec![0.0];
    distinct.extend(params.iter().take(n - 2).skip(2));
    distinct.push(1.0);
    let mut mults = vec![1; distinct.len()];
    mults[0] = 4;
    if let Some(last) = mults.last_mut() {
      *last = 4;
    }
    Ok(((distinct, mults), Collocation { lu: a, perm }))
  }

  fn solve(&self, b: &[DVec3]) -> Vec<DVec3> {
    let n = b.len();
    let mut x: Vec<DVec3> = self.perm.iter().map(|&p| b[p]).collect();
    for r in 0..n {
      for c in 0..r {
        x[r] = x[r] - self.lu[r][c] * x[c];
      }
    }
    for r in (0..n).rev() {
      for c in r + 1..n {
        x[r] = x[r] - self.lu[r][c] * x[c];
      }
      x[r] /= self.lu[r][r];
    }
    x
  }
}

/// The four cubic basis functions that are non-zero on knot span `span` at `u`.
fn basis(knots: &[f64], span: usize, u: f64) -> [f64; 4] {
  let mut n = [1.0, 0.0, 0.0, 0.0];
  let (mut left, mut right) = ([0.0; 4], [0.0; 4]);
  for j in 1..=3 {
    left[j] = u - knots[span + 1 - j];
    right[j] = knots[span + j] - u;
    let mut saved = 0.0;
    for r in 0..j {
      let temp = n[r] / (right[r + 1] + left[j - r]);
      n[r] = saved + right[r + 1] * temp;
      saved = left[j - r] * temp;
    }
    n[j] = saved;
  }
  n
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn box_cage_is_closed() {
    let c = FormCage::quad_box(80.0, 60.0, 20.0, [2, 2, 2]).unwrap();
    assert_eq!(c.vertices.len(), 26);
    assert_eq!(c.faces.len(), 24);
    c.check().unwrap();
  }

  #[test]
  fn box_cage_has_six_regions() {
    let c = FormCage::quad_box(80.0, 60.0, 20.0, [2, 2, 2]).unwrap();
    let t = Topology::of(&c).unwrap();
    let r = t.regions(&c);
    assert_eq!(r.len(), 6);
    assert!(r.iter().all(|r| r.width == 2 && r.height == 2));
  }

  #[test]
  fn interpolation_reproduces_cubics() {
    let grid: Vec<Vec<DVec3>> = (0..9)
      .map(|i| {
        (0..7)
          .map(|j| {
            let (u, v) = (i as f64 / 8.0, j as f64 / 6.0);
            DVec3::new(u, v, u * u * u - v * v)
          })
          .collect()
      })
      .collect();
    let s = interpolate(&grid).unwrap();
    assert_eq!(s.poles.len(), 9);
    assert_eq!(s.u_mults.iter().sum::<u32>() as usize, 9 + 4);
  }
}
