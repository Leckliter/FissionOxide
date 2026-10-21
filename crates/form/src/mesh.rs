//! The control cage of a form body and its topology.

use std::collections::{BTreeSet, HashMap};

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::symmetry::Symmetry;

/// Most cage faces a form may have.
pub const MAX_FACES: usize = 4096;

/// How a form body is shown, as Fusion's Alt+1/2/3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
  /// Alt+1: the control cage as flat faces.
  Box,
  /// Alt+2: the smooth body with its control cage drawn over it.
  ControlFrame,
  /// Alt+3: the smooth body alone.
  #[default]
  Smooth,
}

impl Display {
  pub fn is_smooth(&self) -> bool {
    *self == Display::Smooth
  }
}

/// The control cage of a form body: vertices (mm, in the form's plane coordinates) and polygon
/// faces (vertex indices, counter-clockwise seen from outside). Every edge has one face on each
/// side, or one face if it's on the cage's open boundary. Creased edges stay sharp in the smooth
/// body.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Cage {
  pub vertices: Vec<[f64; 3]>,
  pub faces: Vec<Vec<u32>>,
  /// Creased edges, each as its two vertices (lower index first).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub creases: Vec<[u32; 2]>,
  #[serde(default, skip_serializing_if = "Symmetry::is_none")]
  pub symmetry: Symmetry,
  #[serde(default, skip_serializing_if = "Display::is_smooth")]
  pub display: Display,
}

/// An undirected edge key.
pub(crate) fn key(a: u32, b: u32) -> (u32, u32) {
  (a.min(b), a.max(b))
}

impl Cage {
  pub fn pos(&self, v: u32) -> DVec3 {
    DVec3::from(self.vertices[v as usize])
  }

  /// Checks the cage is a manifold, consistently oriented polygon mesh.
  pub fn check(&self) -> Result<(), String> {
    Topo::of(self).map(|_| ())
  }

  /// Whether the cage has no open boundary.
  pub fn is_closed(&self) -> bool {
    Topo::of(self).is_ok_and(|t| t.half.keys().all(|(a, b)| t.half.contains_key(&(*b, *a))))
  }

  /// The vertex nearest `p`.
  pub fn nearest_vertex(&self, p: [f64; 3]) -> Option<usize> {
    let p = DVec3::from(p);
    (0..self.vertices.len()).min_by(|a, b| {
      let d = |i: usize| DVec3::from(self.vertices[i]).distance_squared(p);
      d(*a).total_cmp(&d(*b))
    })
  }

  /// The edge whose midpoint is nearest `p`.
  pub fn nearest_edge(&self, p: [f64; 3]) -> Option<[u32; 2]> {
    let p = DVec3::from(p);
    self.edges().into_iter().min_by(|a, b| {
      let d = |e: &[u32; 2]| ((self.pos(e[0]) + self.pos(e[1])) / 2.0).distance_squared(p);
      d(a).total_cmp(&d(b))
    })
  }

  /// The face whose centre is nearest `p`.
  pub fn nearest_face(&self, p: [f64; 3]) -> Option<usize> {
    let p = DVec3::from(p);
    (0..self.faces.len()).min_by(|a, b| {
      let d = |f: usize| self.centroid(f).distance_squared(p);
      d(*a).total_cmp(&d(*b))
    })
  }

  /// Every edge once, lower vertex first, sorted.
  pub fn edges(&self) -> Vec<[u32; 2]> {
    let set: BTreeSet<(u32, u32)> =
      self.faces.iter().flat_map(|f| (0..f.len()).map(move |k| key(f[k], f[(k + 1) % f.len()]))).collect();
    set.into_iter().map(|(a, b)| [a, b]).collect()
  }

  /// Edges with a face on one side only, as they run in that face.
  pub fn boundary_edges(&self) -> Vec<[u32; 2]> {
    let directed: BTreeSet<(u32, u32)> =
      self.faces.iter().flat_map(|f| (0..f.len()).map(move |k| (f[k], f[(k + 1) % f.len()]))).collect();
    directed.iter().filter(|(a, b)| !directed.contains(&(*b, *a))).map(|(a, b)| [*a, *b]).collect()
  }

  pub fn centroid(&self, f: usize) -> DVec3 {
    let face = &self.faces[f];
    face.iter().map(|&v| self.pos(v)).sum::<DVec3>() / face.len().max(1) as f64
  }

  /// A face's unit normal (Newell's method), zero if degenerate.
  pub fn face_normal(&self, f: usize) -> DVec3 {
    let face = &self.faces[f];
    let mut n = DVec3::ZERO;
    for k in 0..face.len() {
      let (a, b) = (self.pos(face[k]), self.pos(face[(k + 1) % face.len()]));
      n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    n.normalize_or_zero()
  }

  /// A vertex's normal: the mean of its faces' normals.
  pub fn vertex_normal(&self, v: u32) -> DVec3 {
    self.faces.iter().enumerate().filter(|(_, f)| f.contains(&v)).map(|(i, _)| self.face_normal(i)).sum::<DVec3>().normalize_or_zero()
  }

  pub fn is_creased(&self, a: u32, b: u32) -> bool {
    let k = key(a, b);
    self.creases.iter().any(|c| (c[0], c[1]) == k)
  }

  /// Size of the cage's bounding box diagonal (1 if empty).
  pub fn size(&self) -> f64 {
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for v in &self.vertices {
      lo = lo.min(DVec3::from(*v));
      hi = hi.max(DVec3::from(*v));
    }
    if self.vertices.is_empty() { 1.0 } else { (hi - lo).length().max(1e-9) }
  }

  /// Drops vertices no face uses (renumbering the rest) and creases that are no longer edges.
  pub fn compact(&mut self) {
    let mut used = vec![false; self.vertices.len()];
    for f in &self.faces {
      for &v in f {
        used[v as usize] = true;
      }
    }
    let mut map = vec![u32::MAX; self.vertices.len()];
    let mut verts = Vec::new();
    for (i, u) in used.iter().enumerate() {
      if *u {
        map[i] = verts.len() as u32;
        verts.push(self.vertices[i]);
      }
    }
    self.vertices = verts;
    for f in &mut self.faces {
      for v in f.iter_mut() {
        *v = map[*v as usize];
      }
    }
    let edges: BTreeSet<[u32; 2]> = self.edges().into_iter().collect();
    let mut creases: Vec<[u32; 2]> = self
      .creases
      .iter()
      .filter(|c| (c[0] as usize) < map.len() && (c[1] as usize) < map.len())
      .map(|c| {
        let (a, b) = key(map[c[0] as usize], map[c[1] as usize]);
        [a, b]
      })
      .filter(|c| edges.contains(c))
      .collect();
    creases.sort();
    creases.dedup();
    self.creases = creases;
  }

  /// Adds a vertex, returning its index.
  pub(crate) fn add_vertex(&mut self, p: DVec3) -> u32 {
    self.vertices.push(p.to_array());
    (self.vertices.len() - 1) as u32
  }

  /// Marks or clears the crease on an edge.
  pub(crate) fn set_crease(&mut self, a: u32, b: u32, on: bool) {
    let (a, b) = key(a, b);
    self.creases.retain(|c| (c[0], c[1]) != (a, b));
    if on {
      self.creases.push([a, b]);
      self.creases.sort();
    }
  }
}

/// The cage's directed edges and the faces around each vertex.
pub(crate) struct Topo {
  /// Directed edge a → b: the face it is in and the corner of `a` there.
  pub half: HashMap<(u32, u32), (usize, usize)>,
  /// Neighbouring vertices of each vertex.
  pub nbrs: Vec<Vec<u32>>,
  /// Faces using each vertex.
  pub vfaces: Vec<Vec<usize>>,
}

impl Topo {
  pub fn of(cage: &Cage) -> Result<Topo, String> {
    let nv = cage.vertices.len();
    if cage.faces.is_empty() {
      return Err("the cage has no faces".into());
    }
    if cage.faces.len() > MAX_FACES {
      return Err(format!("the cage has more than {MAX_FACES} faces"));
    }
    if cage.vertices.iter().flatten().any(|c| !c.is_finite()) {
      return Err("the cage has non-finite vertices".into());
    }
    let mut half = HashMap::new();
    let mut nbrs: Vec<Vec<u32>> = vec![Vec::new(); nv];
    let mut vfaces: Vec<Vec<usize>> = vec![Vec::new(); nv];
    for (fi, f) in cage.faces.iter().enumerate() {
      if f.len() < 3 {
        return Err(format!("face {fi} has fewer than three corners"));
      }
      for k in 0..f.len() {
        let (a, b) = (f[k], f[(k + 1) % f.len()]);
        if a as usize >= nv {
          return Err(format!("face {fi} uses vertex {a}, but the cage has {nv}"));
        }
        if f.iter().filter(|v| **v == a).count() > 1 {
          return Err(format!("face {fi} uses a vertex twice"));
        }
        if half.insert((a, b), (fi, k)).is_some() {
          return Err("the cage's faces don't all face the same way, or an edge has more than two faces".into());
        }
        vfaces[a as usize].push(fi);
        for (x, y) in [(a, b), (b, a)] {
          if !nbrs[x as usize].contains(&y) {
            nbrs[x as usize].push(y);
          }
        }
      }
    }
    let topo = Topo { half, nbrs, vfaces };
    // the faces round each vertex form one fan
    for v in 0..nv as u32 {
      let faces = &topo.vfaces[v as usize];
      let Some(&start) = faces.first() else { continue };
      let mut seen = vec![start];
      for dir in [false, true] {
        let mut f = start;
        loop {
          let Some(g) = topo.turn(cage, f, v, dir) else { break };
          if g == start || seen.contains(&g) {
            break;
          }
          seen.push(g);
          f = g;
        }
      }
      if seen.len() != faces.len() {
        return Err(format!("the cage is pinched at vertex {v}"));
      }
    }
    Ok(topo)
  }

  /// The face next to `f` round vertex `v`: across the edge into `v` (`back`) or out of it.
  pub fn turn(&self, cage: &Cage, f: usize, v: u32, back: bool) -> Option<usize> {
    let face = &cage.faces[f];
    let k = face.iter().position(|x| *x == v)?;
    let n = face.len();
    if back {
      let prev = face[(k + n - 1) % n];
      self.half.get(&(v, prev)).map(|h| h.0)
    } else {
      let next = face[(k + 1) % n];
      self.half.get(&(next, v)).map(|h| h.0)
    }
  }

  pub fn is_boundary_edge(&self, a: u32, b: u32) -> bool {
    !(self.half.contains_key(&(a, b)) && self.half.contains_key(&(b, a)))
  }

  pub fn is_boundary_vertex(&self, v: u32) -> bool {
    self.nbrs[v as usize].iter().any(|&u| self.is_boundary_edge(v, u))
  }

  /// The face on the other side of edge a-b from face `f`.
  pub fn across(&self, f: usize, a: u32, b: u32) -> Option<usize> {
    [self.half.get(&(a, b)), self.half.get(&(b, a))].into_iter().flatten().map(|h| h.0).find(|g| *g != f)
  }

  /// The boundary loop (or chain) through boundary edge a → b, in boundary order.
  pub fn boundary_loop(&self, cage: &Cage, a: u32, b: u32) -> Vec<u32> {
    let mut out = vec![a];
    let mut cur = b;
    while cur != a && out.len() <= cage.vertices.len() {
      out.push(cur);
      let next = self.nbrs[cur as usize]
        .iter()
        .copied()
        .find(|&n| self.half.contains_key(&(cur, n)) && !self.half.contains_key(&(n, cur)));
      match next {
        Some(n) => cur = n,
        None => break,
      }
    }
    out
  }
}
