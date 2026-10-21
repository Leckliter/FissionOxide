//! Incremental 3D Delaunay tetrahedralization (Bowyer–Watson) with exact orientation and
//! in-sphere predicates (Shewchuk's, from the `robust` crate).
//!
//! Points go in one at a time: a stochastic walk finds the tetrahedron holding the new point,
//! the tetrahedra whose circumspheres contain it are removed and the cavity is refilled with
//! tetrahedra joining its boundary faces to the point. Everything sits inside a large "super"
//! tetrahedron whose four corners are vertices 0–3; [`Delaunay::tets`] leaves out the
//! tetrahedra touching them. Callers should keep points in general position (e.g. jitter them
//! very slightly): exactly cospherical or coplanar sets are not specially handled.

use crate::geom::{Map, V3};

pub const NONE: u32 = u32::MAX;

/// The vertices of the face opposite each vertex of a positive tetrahedron, wound so its normal
/// points out.
pub const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 3, 2], [0, 1, 3], [0, 2, 1]];

fn c3(p: V3) -> robust::Coord3D<f64> {
  robust::Coord3D { x: p[0], y: p[1], z: p[2] }
}

/// Positive when `d` is on the side of `a, b, c` they turn counter-clockwise around.
pub fn orient(a: V3, b: V3, c: V3, d: V3) -> f64 {
  -robust::orient3d(c3(a), c3(b), c3(c), c3(d))
}

/// Positive when `e` is inside the circumsphere of the positive tetrahedron `a, b, c, d`.
pub fn in_sphere(a: V3, b: V3, c: V3, d: V3, e: V3) -> f64 {
  -robust::insphere(c3(a), c3(b), c3(c), c3(d), c3(e))
}

#[derive(Debug, Clone)]
pub struct Delaunay {
  /// Vertices; 0–3 are the super tetrahedron's corners.
  pub pts: Vec<V3>,
  pub tets: Vec<[u32; 4]>,
  /// The tetrahedron across the face opposite each vertex ([`NONE`] outside).
  pub nbr: Vec<[u32; 4]>,
  pub dead: Vec<bool>,
  free: Vec<u32>,
  last: u32,
  rng: u64,
  mark: Vec<u32>,
  stamp: u32,
}

impl Delaunay {
  /// An empty triangulation able to hold points within the box `lo`–`hi`.
  pub fn new(lo: V3, hi: V3) -> Delaunay {
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt().max(1e-6) * 50.0;
    let pts = vec![
      [c[0] + r, c[1] + r, c[2] + r],
      [c[0] - r, c[1] - r, c[2] + r],
      [c[0] - r, c[1] + r, c[2] - r],
      [c[0] + r, c[1] - r, c[2] - r],
    ];
    let mut tet = [0, 1, 2, 3];
    if orient(pts[0], pts[1], pts[2], pts[3]) < 0.0 {
      tet.swap(0, 1);
    }
    Delaunay {
      pts,
      tets: vec![tet],
      nbr: vec![[NONE; 4]],
      dead: vec![false],
      free: Vec::new(),
      last: 0,
      rng: 0x9E37_79B9_7F4A_7C15,
      mark: vec![0],
      stamp: 0,
    }
  }

  fn rand(&mut self) -> u64 {
    // xorshift64*
    self.rng ^= self.rng >> 12;
    self.rng ^= self.rng << 25;
    self.rng ^= self.rng >> 27;
    self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
  }

  /// The tetrahedron containing `p`.
  pub fn locate(&mut self, p: V3) -> Option<u32> {
    let mut t = self.last;
    if self.dead.get(t as usize).copied().unwrap_or(true) {
      t = self.dead.iter().position(|d| !d)? as u32;
    }
    let mut steps = 0usize;
    'walk: loop {
      steps += 1;
      if steps > 10 * self.tets.len() + 1000 {
        return None;
      }
      let start = (self.rand() % 4) as usize;
      let v = self.tets[t as usize];
      for k in 0..4 {
        let i = (start + k) % 4;
        let mut q = v.map(|x| self.pts[x as usize]);
        q[i] = p;
        if orient(q[0], q[1], q[2], q[3]) < 0.0 {
          let n = self.nbr[t as usize][i];
          if n == NONE {
            return None;
          }
          t = n;
          continue 'walk;
        }
      }
      return Some(t);
    }
  }

  fn conflict(&self, t: u32, p: V3) -> bool {
    let v = self.tets[t as usize].map(|x| self.pts[x as usize]);
    in_sphere(v[0], v[1], v[2], v[3], p) > 0.0
  }

  fn alloc(&mut self, tet: [u32; 4], nbr: [u32; 4]) -> u32 {
    if let Some(t) = self.free.pop() {
      self.tets[t as usize] = tet;
      self.nbr[t as usize] = nbr;
      self.dead[t as usize] = false;
      t
    } else {
      self.tets.push(tet);
      self.nbr.push(nbr);
      self.dead.push(false);
      self.mark.push(0);
      self.tets.len() as u32 - 1
    }
  }

  /// Inserts a point; returns its vertex index, or `None` if it falls on an existing vertex (within
  /// `eps`) or outside the super tetrahedron.
  pub fn insert(&mut self, p: V3, eps: f64) -> Option<u32> {
    let t0 = self.locate(p)?;
    for &v in &self.tets[t0 as usize] {
      if crate::geom::dist(self.pts[v as usize], p) <= eps {
        return None;
      }
    }
    self.stamp = self.stamp.wrapping_add(1);
    if self.stamp == 0 {
      self.mark.iter_mut().for_each(|m| *m = 0);
      self.stamp = 1;
    }
    let stamp = self.stamp;
    let mut cavity = vec![t0];
    self.mark[t0 as usize] = stamp;
    let mut boundary: Vec<(u32, usize, u32)> = Vec::new();
    let mut k = 0;
    while k < cavity.len() {
      let t = cavity[k];
      k += 1;
      for i in 0..4 {
        let n = self.nbr[t as usize][i];
        if n == NONE {
          boundary.push((t, i, NONE));
        } else if self.mark[n as usize] == stamp {
          continue;
        } else if self.conflict(n, p) {
          self.mark[n as usize] = stamp;
          cavity.push(n);
        } else {
          boundary.push((t, i, n));
        }
      }
    }
    let pi = self.pts.len() as u32;
    self.pts.push(p);
    // build the new tetrahedra before freeing the cavity (their vertices come from it)
    let mut made: Vec<([u32; 4], usize, u32)> = Vec::with_capacity(boundary.len());
    for &(t, i, n) in &boundary {
      let mut v = self.tets[t as usize];
      v[i] = pi;
      made.push((v, i, n));
    }
    for &t in &cavity {
      self.dead[t as usize] = true;
      self.free.push(t);
    }
    let mut ids = Vec::with_capacity(made.len());
    for &(v, i, n) in &made {
      let mut nb = [NONE; 4];
      nb[i] = n;
      ids.push(self.alloc(v, nb));
    }
    // outside neighbours pointed at cavity tetrahedra (whose slots may be reused by now): match
    // them up by the shared face's vertices
    for (k, &(_, _, n)) in boundary.iter().enumerate() {
      if n == NONE {
        continue;
      }
      let id = ids[k];
      if let Some(j) = (0..4).find(|&j| self.shares_face(n, j, id)) {
        self.nbr[n as usize][j] = id;
      }
    }
    // link the new tetrahedra to each other across the faces through the new point
    let mut open: Map<(u32, u32), (u32, usize)> = Map::with_capacity_and_hasher(ids.len() * 3, Default::default());
    for (k, &(v, i, _)) in made.iter().enumerate() {
      let id = ids[k];
      for j in 0..4 {
        if j == i {
          continue;
        }
        let others: Vec<u32> = (0..4).filter(|&m| m != i && m != j).map(|m| v[m]).collect();
        let key = if others[0] < others[1] { (others[0], others[1]) } else { (others[1], others[0]) };
        if let Some((o, oj)) = open.remove(&key) {
          self.nbr[id as usize][j] = o;
          self.nbr[o as usize][oj] = id;
        } else {
          open.insert(key, (id, j));
        }
      }
    }
    self.last = ids[0];
    Some(pi)
  }

  /// Whether face `j` of tetrahedron `n` has the same vertices as the face of `id` opposite the new
  /// point.
  fn shares_face(&self, n: u32, j: usize, id: u32) -> bool {
    let mut a: Vec<u32> = (0..4).filter(|&m| m != j).map(|m| self.tets[n as usize][m]).collect();
    let pi = self.pts.len() as u32 - 1;
    let mut b: Vec<u32> = self.tets[id as usize].iter().copied().filter(|&x| x != pi).collect();
    a.sort();
    b.sort();
    a == b
  }

  /// Live tetrahedra not touching the super tetrahedron.
  pub fn tets(&self) -> impl Iterator<Item = (u32, [u32; 4])> + '_ {
    self
      .tets
      .iter()
      .enumerate()
      .filter(move |(t, v)| !self.dead[*t] && v.iter().all(|&x| x >= 4))
      .map(|(t, v)| (t as u32, *v))
  }

  /// Checks the neighbour links and orientations (for tests).
  pub fn check(&self) -> Result<(), String> {
    for (t, v) in self.tets.iter().enumerate() {
      if self.dead[t] {
        continue;
      }
      let q = v.map(|x| self.pts[x as usize]);
      if orient(q[0], q[1], q[2], q[3]) <= 0.0 {
        return Err(format!("tetrahedron {t} is not positive"));
      }
      for i in 0..4 {
        let n = self.nbr[t][i];
        if n == NONE {
          continue;
        }
        if self.dead[n as usize] {
          return Err(format!("tetrahedron {t} links to a dead one"));
        }
        if !self.nbr[n as usize].contains(&(t as u32)) {
          return Err(format!("tetrahedra {t} and {n} disagree"));
        }
        let mut a: Vec<u32> = (0..4).filter(|&m| m != i).map(|m| v[m]).collect();
        a.sort();
        let w = self.tets[n as usize];
        if a.iter().any(|x| !w.contains(x)) {
          return Err(format!("tetrahedra {t} and {n} don't share a face"));
        }
      }
    }
    Ok(())
  }
}
