//! References to faces and edges of bodies that survive edits.
//!
//! Kernel face/edge indices change whenever the model changes (the "topological naming
//! problem"). A reference therefore stores a geometric signature taken when the user picked the
//! face or edge — a point on it, its direction, the normals around it — and is resolved on every
//! regeneration by scoring candidates against that signature. After resolving, the signature is
//! refreshed so references follow gradual changes (a block getting taller drags the sketch on its
//! top face up with it).

use clf_kernel::glam::DVec3;
use clf_kernel::{CurveKind, EdgeInfo, FaceInfo, SurfaceKind, TriMesh};
use serde::{Deserialize, Serialize};

use crate::BodyId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceRef {
  pub body: BodyId,
  /// A point on the face when it was picked.
  pub point: [f64; 3],
  /// Outward normal (planes) or axis (cylinders).
  pub normal: [f64; 3],
  #[serde(default)]
  pub planar: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeRef {
  pub body: BodyId,
  /// The edge's midpoint when it was picked.
  pub point: [f64; 3],
  /// Unit direction for straight edges, axis for circular ones.
  pub dir: [f64; 3],
  /// Normals of the planar faces meeting at the edge (zero for curved faces).
  pub n1: [f64; 3],
  pub n2: [f64; 3],
  #[serde(default)]
  pub circular: bool,
  #[serde(default)]
  pub radius: f64,
}

impl FaceRef {
  pub fn from_face(body: BodyId, f: &FaceInfo, at: DVec3) -> FaceRef {
    FaceRef { body, point: at.to_array(), normal: f.normal.to_array(), planar: f.kind == SurfaceKind::Plane }
  }

  /// Picks the best matching face (1-based id). `mesh` is the body's tessellation, used to check
  /// that the stored point projects inside the candidate face.
  pub fn resolve(&self, faces: &[FaceInfo], mesh: Option<&TriMesh>, scale: f64) -> Option<u32> {
    let p = DVec3::from(self.point);
    let n0 = DVec3::from(self.normal);
    let scale = scale.max(1.0);
    let mut best: Option<(u32, f64)> = None;
    for f in faces {
      let planar = f.kind == SurfaceKind::Plane;
      if planar != self.planar {
        continue;
      }
      let align = f.normal.dot(n0);
      let score = if planar {
        if align < 0.999 {
          continue;
        }
        let q = p - f.normal * (p - f.origin).dot(f.normal);
        let inside = mesh.is_some_and(|m| point_in_face(m, f.id, q));
        // inside wins outright; then the closest plane, then the closest centroid
        let off = (p - f.origin).dot(f.normal).abs();
        (if inside { 0.0 } else { 10.0 * scale }) + off + 0.01 * (q - f.center).length()
      } else {
        if align.abs() < 0.999 {
          continue;
        }
        (p - f.center).length()
      };
      if best.is_none_or(|(_, s)| score < s) {
        best = Some((f.id, score));
      }
    }
    best.map(|(id, _)| id)
  }
}

impl EdgeRef {
  pub fn from_edge(body: BodyId, e: &EdgeInfo, faces: &[FaceInfo]) -> EdgeRef {
    let (n1, n2) = adjacent_normals(e, faces);
    EdgeRef {
      body,
      point: e.mid.to_array(),
      dir: edge_dir(e).to_array(),
      n1: n1.to_array(),
      n2: n2.to_array(),
      circular: e.kind == CurveKind::Circle,
      radius: e.radius,
    }
  }

  /// Picks the best matching edge (1-based id).
  pub fn resolve(&self, edges: &[EdgeInfo], faces: &[FaceInfo], scale: f64) -> Option<u32> {
    let p = DVec3::from(self.point);
    let d0 = DVec3::from(self.dir);
    let (a0, b0) = (DVec3::from(self.n1), DVec3::from(self.n2));
    let scale = scale.max(1.0);
    let mut best: Option<(u32, f64)> = None;
    for e in edges {
      if e.kind == CurveKind::Degenerate || (e.kind == CurveKind::Circle) != self.circular {
        continue;
      }
      let dir_mis = 1.0 - edge_dir(e).dot(d0).abs();
      if dir_mis > 0.01 {
        continue;
      }
      let (a, b) = adjacent_normals(e, faces);
      // normals may come in either order
      let m1 = normal_mismatch(a, a0) + normal_mismatch(b, b0);
      let m2 = normal_mismatch(a, b0) + normal_mismatch(b, a0);
      let face_mis = m1.min(m2);
      let rad_mis = if self.circular { (e.radius - self.radius).abs() / scale } else { 0.0 };
      let dist = (e.mid - p).length();
      let score = dist + scale * (5.0 * face_mis + dir_mis * 10.0 + rad_mis);
      if best.is_none_or(|(_, s)| score < s) {
        best = Some((e.id, score));
      }
    }
    best.map(|(id, _)| id)
  }
}

fn normal_mismatch(a: DVec3, b: DVec3) -> f64 {
  match (a.length_squared() > 0.5, b.length_squared() > 0.5) {
    (true, true) => 1.0 - a.dot(b),
    (false, false) => 0.0,
    _ => 1.0,
  }
}

fn edge_dir(e: &EdgeInfo) -> DVec3 {
  match e.kind {
    CurveKind::Circle => e.axis.normalize_or_zero(),
    _ => (e.end - e.start).normalize_or_zero(),
  }
}

fn adjacent_normals(e: &EdgeInfo, faces: &[FaceInfo]) -> (DVec3, DVec3) {
  let n = |id: u32| {
    faces.iter().find(|f| f.id == id && f.kind == SurfaceKind::Plane).map(|f| f.normal).unwrap_or(DVec3::ZERO)
  };
  let (a, b) = (n(e.faces[0]), n(e.faces[1]));
  // canonical order so signatures compare regardless of face numbering
  if (a.x, a.y, a.z) <= (b.x, b.y, b.z) { (a, b) } else { (b, a) }
}

/// Is `p` (on the face's plane) inside one of the face's triangles?
pub fn point_in_face(m: &TriMesh, face: u32, p: DVec3) -> bool {
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if m.tri_face.get(t) != Some(&face) {
      continue;
    }
    let v = |i: u32| m.positions.get(i as usize).map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64));
    let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) else { continue };
    let n = (b - a).cross(c - a);
    let nn = n.length_squared();
    if nn < 1e-18 {
      continue;
    }
    let w0 = (b - p).cross(c - p).dot(n) / nn;
    let w1 = (c - p).cross(a - p).dot(n) / nn;
    let w2 = 1.0 - w0 - w1;
    let eps = -1e-6;
    if w0 >= eps && w1 >= eps && w2 >= eps {
      return true;
    }
  }
  false
}
