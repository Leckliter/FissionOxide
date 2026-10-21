//! Face names that survive edits (the topological naming problem, answered from the kernel's
//! modification history).
//!
//! Every face of a body carries a set of names. A feature that makes faces names them from what it
//! made them from: an extrusion's side faces from the sketch curves they were swept from, its caps
//! from the profile, a fillet's round from the edge it rounds (the names of the two faces along
//! it). Faces a feature only changes (trims, splits, moves) keep their names: the kernel follows
//! them through OpenCASCADE's own history (`Modified`, `Generated`, `IsDeleted` of each
//! algorithm, the Boolean and UnifySameDomain histories; see `fox_kernel::FaceLabels`). An edge
//! is named by the faces on either side of it.
//!
//! Since sketch curves keep their ids when a dimension changes, a face is named the same after an
//! edit that moves it far away, and a reference that stored its name finds it again where the
//! geometric signature (see [`crate::refs`]) would pick whatever face now sits where it was. The
//! signature still decides among the pieces of a split face, and is the whole answer for faces
//! the history can't name (imported bodies, and kernel calls that keep no history).
//!
//! Names are stored in saved references, so how they are made must never change.

use std::sync::Arc;

use fox_kernel::glam::DVec3;
use fox_kernel::{EdgeInfo, FaceInfo, Plane, Shape};
use fox_sketch::{Profile, Seg};

use crate::FeatureId;

/// A face's names, sorted; empty when it has none.
pub type FaceNames = Vec<u64>;

/// The names of a body's faces (index `face id - 1`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Names {
  pub faces: Vec<FaceNames>,
}

impl Names {
  pub fn of(&self, face: u32) -> &[u64] {
    face.checked_sub(1).and_then(|i| self.faces.get(i as usize)).map(Vec::as_slice).unwrap_or(&[])
  }

  pub fn is_empty(&self) -> bool {
    self.faces.iter().all(Vec::is_empty)
  }
}

// What a name is made from: one tag per kind of origin.
const GENERATED: u64 = 0x67656e;
const CURVE: u64 = 0x63757276;
const PROFILE: u64 = 0x70726f66;
const COPY: u64 = 0x636f7079;
const INDEX: u64 = 0x696e6478;

/// FNV-1a over the words, kept to 53 bits (names go into JSON, where some readers hold numbers
/// as doubles); never 0.
pub fn mix(words: &[u64]) -> u64 {
  let mut h: u64 = 0xcbf2_9ce4_8422_2325;
  for w in words {
    for b in w.to_le_bytes() {
      h ^= u64::from(b);
      h = h.wrapping_mul(0x0100_0000_01b3);
    }
  }
  let h = h & ((1 << 53) - 1);
  if h == 0 { 1 } else { h }
}

/// The names of `shape`'s faces, from its history: a face modified from a labelled face takes
/// its names; one `feature` generated from a labelled face or edge is named from that label.
/// `None` when the kernel calls that made `shape` kept no history.
pub fn from_history(shape: &Shape, feature: FeatureId) -> Option<Names> {
  if !shape.has_labels() {
    return None;
  }
  let labels = shape.face_labels().ok()?;
  let faces = labels
    .faces
    .into_iter()
    .map(|f| {
      let mut n: Vec<u64> = f
        .into_iter()
        .map(|(l, generated)| if generated { mix(&[GENERATED, feature.0.into(), l]) } else { l })
        .collect();
      n.sort_unstable();
      n.dedup();
      n
    })
    .collect();
  Some(Names { faces })
}

/// The names of `shape`'s faces taken from the faces of `before` they are (the same face, or one
/// on the same surface), for kernel calls that keep no history.
pub fn from_identity(shape: &Shape, before: &Shape, names: &Names) -> Names {
  let same = shape.same_faces(before).unwrap_or_default();
  Names { faces: same.into_iter().map(|j| names.of(j).to_vec()).collect() }
}

/// Faces named by their place in the shape: for shapes whose topology the feature fixes (a box
/// always has its faces in the same order).
pub fn by_index(count: usize, feature: FeatureId) -> Names {
  Names { faces: (1..=count as u64).map(|i| vec![mix(&[INDEX, feature.0.into(), i])]).collect() }
}

/// The same names, made distinct for copy `k` of a pattern or mirror made by `feature`.
pub fn copy(names: &Names, feature: FeatureId, k: u64) -> Names {
  Names {
    faces: names
      .faces
      .iter()
      .map(|f| {
        let mut n: Vec<u64> = f.iter().map(|l| mix(&[COPY, feature.0.into(), k, *l])).collect();
        n.sort_unstable();
        n
      })
      .collect(),
  }
}

/// Makes a copy's names (from its history) distinct, as copy `k` of `feature`: a pattern's
/// instances are not the faces they were copied from.
pub fn relabel_copy(shape: &mut Shape, feature: FeatureId, k: u64) {
  if let Some(n) = from_history(shape, feature) {
    label(shape, &copy(&n, feature, k));
  }
}

/// Puts `names` on `shape` as labels for the next kernel calls to follow.
pub fn label(shape: &mut Shape, names: &Names) {
  let pairs: Vec<(u32, u64)> =
    names.faces.iter().enumerate().flat_map(|(i, f)| f.iter().map(move |l| (i as u32 + 1, *l))).collect();
  if shape.set_labels(&pairs, &[]).is_err() {
    log::debug!("could not label a shape's faces");
  }
}

/// Labels a sketch profile's face for the solid `feature` makes from it: each edge by the sketch
/// curve it was cut from, the face by the profile (its first outer curve), so the faces swept from
/// them are named the same however the sketch's dimensions change.
pub fn label_profile(face: &mut Shape, plane: &Plane, profile: &Profile, feature: FeatureId) {
  let f = u64::from(feature.0);
  let loops = || std::iter::once(&profile.outer).chain(profile.holes.iter());
  let mut segs: Vec<(Seg, u64)> = Vec::new();
  for lp in loops() {
    if lp.src.len() != lp.segs.len() {
      return;
    }
    segs.extend(lp.segs.iter().zip(&lp.src).map(|(s, src)| (*s, u64::from(src.curve.0))));
  }
  let Some(first) = profile.outer.src.iter().map(|s| u64::from(s.curve.0)).min() else { return };
  let Ok(edges) = face.edges() else { return };
  let curve_of = |e: &EdgeInfo| {
    let p = plane.to_local(e.mid);
    segs.iter().map(|(s, c)| (seg_distance(s, p), *c)).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, c)| c)
  };
  let mut labels = vec![0u64; edges.iter().map(|e| e.id as usize).max().unwrap_or(0)];
  for e in &edges {
    if let (Some(c), Some(slot)) = (curve_of(e), labels.get_mut(e.id as usize - 1)) {
      *slot = mix(&[CURVE, f, c]);
    }
  }
  let faces: Vec<(u32, u64)> =
    face.faces().unwrap_or_default().iter().map(|fi| (fi.id, mix(&[PROFILE, f, first]))).collect();
  if face.set_labels(&faces, &labels).is_err() {
    log::debug!("could not label a profile");
  }
}

/// Distance from `p` to a profile segment, in the sketch plane.
fn seg_distance(s: &Seg, p: [f64; 2]) -> f64 {
  let line = |a: [f64; 2], b: [f64; 2]| {
    let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
    let ll = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if ll > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1]) / ll).clamp(0.0, 1.0) } else { 0.0 };
    ((ap[0] - ab[0] * t).powi(2) + (ap[1] - ab[1] * t).powi(2)).sqrt()
  };
  match *s {
    Seg::Line { a, b } => line(a, b),
    Seg::Circle { center, radius, .. } => {
      (((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)).sqrt() - radius).abs()
    }
    _ => {
      let mut pts = Vec::new();
      s.flatten(&mut pts);
      pts.push(s.end());
      pts.windows(2).map(|w| line(w[0], w[1])).fold(f64::INFINITY, f64::min)
    }
  }
}

/// A face's names held by a body.
pub type Shared = Arc<Names>;

/// The faces whose names include `name`.
pub fn faces_named(names: &Names, name: u64) -> Vec<u32> {
  if name == 0 {
    return Vec::new();
  }
  names.faces.iter().enumerate().filter(|(_, f)| f.binary_search(&name).is_ok()).map(|(i, _)| i as u32 + 1).collect()
}

/// The edges between faces named `pair` (either way round).
pub fn edges_named(names: &Names, edges: &[EdgeInfo], pair: [u64; 2]) -> Vec<u32> {
  if pair[0] == 0 || pair[1] == 0 {
    return Vec::new();
  }
  let has = |f: u32, n: u64| names.of(f).binary_search(&n).is_ok();
  edges
    .iter()
    .filter(|e| {
      let [a, b] = e.faces;
      let b = if b == 0 { a } else { b };
      (has(a, pair[0]) && has(b, pair[1])) || (has(a, pair[1]) && has(b, pair[0]))
    })
    .map(|e| e.id)
    .collect()
}

/// The name a reference keeps for `face`: the one it had, if the face still has it, else the
/// face's first.
pub fn pick(names: &Names, face: u32, had: u64) -> u64 {
  let n = names.of(face);
  if had != 0 && n.binary_search(&had).is_ok() { had } else { n.first().copied().unwrap_or(0) }
}

/// The names a reference keeps for `edge`: one per face along it (the ones it had, if the faces
/// still have them), sorted.
pub fn pick_pair(names: &Names, e: &EdgeInfo, had: [u64; 2]) -> [u64; 2] {
  let [a, b] = e.faces;
  let b = if b == 0 { a } else { b };
  let has = |f: u32, n: u64| n != 0 && names.of(f).binary_search(&n).is_ok();
  let [x, y] = if has(a, had[0]) && has(b, had[1]) {
    had
  } else if has(a, had[1]) && has(b, had[0]) {
    [had[1], had[0]]
  } else {
    let first = |f: u32| names.of(f).first().copied().unwrap_or(0);
    [first(a), first(b)]
  };
  if x <= y { [x, y] } else { [y, x] }
}

/// Whether two faces lie on one surface (the pieces of a split face do), to `tol`.
pub fn same_surface(a: &FaceInfo, b: &FaceInfo, tol: f64) -> bool {
  use fox_kernel::SurfaceKind::*;
  if a.kind != b.kind {
    return false;
  }
  let parallel = a.normal.cross(b.normal).length() <= 1e-9;
  match a.kind {
    Plane => a.normal.dot(b.normal) > 0.0 && parallel && (a.origin - b.origin).dot(b.normal).abs() <= tol,
    Cylinder | Cone => {
      parallel && (a.origin - b.origin).cross(b.normal).length() <= tol && (a.radius - b.radius).abs() <= tol
    }
    Sphere | Torus => (a.origin - b.origin).length() <= tol && (a.radius - b.radius).abs() <= tol,
    _ => a.id == b.id,
  }
}

/// Whether two edges lie on one curve (the pieces of a split edge do), to `tol`.
pub fn same_curve(a: &EdgeInfo, b: &EdgeInfo, tol: f64) -> bool {
  use fox_kernel::CurveKind::*;
  match (a.kind, b.kind) {
    (Line, Line) => {
      let d = (b.end - b.start).normalize_or_zero();
      (a.start - b.start).cross(d).length() <= tol && (a.end - b.start).cross(d).length() <= tol
    }
    (Circle, Circle) => {
      (a.center - b.center).length() <= tol
        && a.axis.cross(b.axis).length() <= 1e-9
        && (a.radius - b.radius).abs() <= tol
    }
    _ => a.id == b.id,
  }
}

/// The face of `faces` among `ids` nearest `p`.
pub fn nearest(faces: &[FaceInfo], ids: &[u32], p: DVec3) -> Option<u32> {
  faces
    .iter()
    .filter(|f| ids.contains(&f.id))
    .min_by(|a, b| (a.center - p).length().total_cmp(&(b.center - p).length()))
    .map(|f| f.id)
}
