//! Include 3D Geometry: body edges and other sketches' curves brought into a sketch at their
//! world positions, and the sweep, pipe and rail paths that run through them.
//!
//! Included curves that lie in the sketch plane become ordinary sketch curves (projected and
//! linked, so they still form profiles); the rest are kept as [`fox_sketch::Curve3d`]s.

use std::collections::BTreeMap;

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind as EdgeKind, EdgeInfo, Plane, Seg3, Shape};
use fox_sketch::{Curve3dKind, CurveId, CurveKind, Seg, Sketch};

use crate::FeatureId;
use crate::regen::SketchResult;
use crate::sheet;

/// How close path pieces' ends must be to join.
const JOIN: f64 = 1e-6;

fn p3(v: DVec3) -> [f64; 3] {
  v.to_array()
}

/// A body edge as a world-space curve (`None` for a degenerate edge). Free-form edges are
/// sampled from the shape's edge polylines at `deflection`.
pub fn edge_curve3d(shape: &Shape, e: &EdgeInfo, deflection: f64) -> Option<Curve3dKind> {
  match e.kind {
    EdgeKind::Line => Some(Curve3dKind::Line { a: p3(e.start), b: p3(e.end) }),
    EdgeKind::Circle if (e.start - e.end).length() < 1e-6 => {
      Some(Curve3dKind::Circle { center: p3(e.center), normal: p3(e.axis), radius: e.radius })
    }
    EdgeKind::Circle => Some(Curve3dKind::Arc { start: p3(e.start), mid: p3(e.mid), end: p3(e.end) }),
    EdgeKind::Degenerate => None,
    _ => {
      let lines = shape.edge_lines(deflection).ok()?;
      let pl = lines.into_iter().find(|l| l.id == e.id)?;
      (pl.points.len() >= 2).then(|| Curve3dKind::Polyline { points: pl.points.into_iter().map(p3).collect() })
    }
  }
}

/// Does every point of a curve lie in the plane?
pub fn on_plane(kind: &Curve3dKind, plane: &Plane) -> bool {
  let off = |p: [f64; 3]| (DVec3::from(p) - plane.origin).dot(plane.normal()).abs() < 1e-6;
  match kind {
    Curve3dKind::Circle { center, normal, .. } => {
      off(*center) && DVec3::from(*normal).normalize_or_zero().cross(plane.normal()).length() < 1e-6
    }
    Curve3dKind::Arc { start, mid, end } => off(*start) && off(*mid) && off(*end),
    k => k.polyline(2).into_iter().all(off),
  }
}

/// A sketch's curve (2D or included) at its world position.
pub fn sketch_curve3d(sk: &SketchResult, curve: CurveId) -> Option<Curve3dKind> {
  if let Some(c) = sk.sketch.curve3d(curve) {
    return Some(c.kind.clone());
  }
  let w = |p: [f64; 2]| p3(sk.plane.to_world(p));
  let c = sk.sketch.curve(curve)?;
  match &c.kind {
    CurveKind::Line { a, b } => Some(Curve3dKind::Line { a: w(sk.sketch.pos(*a)?), b: w(sk.sketch.pos(*b)?) }),
    CurveKind::Circle { center, radius } => {
      Some(Curve3dKind::Circle { center: w(sk.sketch.pos(*center)?), normal: p3(sk.plane.normal()), radius: *radius })
    }
    CurveKind::Arc { .. } => match sk.sketch.curve_segs(curve).first()? {
      s @ Seg::Arc { .. } => Some(Curve3dKind::Arc { start: w(s.start()), mid: w(s.mid()), end: w(s.end()) }),
      _ => None,
    },
    _ => {
      let pts = sk.sketch.spline_points(curve);
      (pts.len() >= 2).then(|| Curve3dKind::Polyline { points: pts.into_iter().map(w).collect() })
    }
  }
}

/// Moves included curves copied from other sketches onto their source curves as those sketches
/// now stand; returns how many sources could not be found.
pub fn refresh_sources(sketches: &BTreeMap<FeatureId, SketchResult>, sketch: &mut Sketch) -> usize {
  let mut lost = 0;
  for c in &mut sketch.curves3d {
    let Some(src) = c.source else { continue };
    match sketches.get(&FeatureId(src.sketch)).and_then(|s| sketch_curve3d(s, src.curve)) {
      Some(k) => c.kind = k,
      None => lost += 1,
    }
  }
  lost
}

/// The pieces of a path through a sketch's chosen curves (empty `curves` = every
/// non-construction curve), ordered into one chain, when the choice includes 3D geometry;
/// `None` for a path of plane curves only, which [`sheet::open_path`] handles.
pub fn path_segs(sk: &SketchResult, curves: &[CurveId]) -> Result<Option<Vec<Seg3>>, String> {
  let pick = |id: CurveId, construction: bool| curves.is_empty() && !construction || curves.contains(&id);
  let three: Vec<_> = sk.sketch.curves3d.iter().filter(|c| pick(c.id, c.construction)).collect();
  if three.is_empty() {
    return Ok(None);
  }
  let whole = || Err::<Option<Vec<Seg3>>, String>("a path can't include a whole circle or ellipse".into());
  let mut segs = Vec::new();
  for c in three {
    match &c.kind {
      Curve3dKind::Line { a, b } => segs.push(Seg3::Line { a: DVec3::from(*a), b: DVec3::from(*b) }),
      Curve3dKind::Arc { start, mid, end } => {
        segs.push(Seg3::Arc { start: DVec3::from(*start), mid: DVec3::from(*mid), end: DVec3::from(*end) })
      }
      Curve3dKind::Circle { .. } if curves.is_empty() => {}
      Curve3dKind::Circle { .. } => return whole(),
      Curve3dKind::Polyline { points } => {
        segs.extend(points.windows(2).map(|w| Seg3::Line { a: DVec3::from(w[0]), b: DVec3::from(w[1]) }))
      }
    }
  }
  for c in sk.sketch.curves.iter().filter(|c| pick(c.id, c.construction)) {
    if matches!(c.kind, CurveKind::Circle { .. } | CurveKind::Ellipse { .. }) {
      if curves.is_empty() {
        continue;
      }
      return whole();
    }
    segs.extend(sk.sketch.curve_segs(c.id).into_iter().map(|seg| Seg3::Planar { plane: sk.plane, seg }));
  }
  segs.retain(|s| s.start().distance(s.end()) > JOIN || matches!(s, Seg3::Planar { .. }));
  chain(segs).map(Some)
}

/// Orders path pieces into one open chain, end to end.
fn chain(segs: Vec<Seg3>) -> Result<Vec<Seg3>, String> {
  if segs.is_empty() {
    return Err("the path has no curves".into());
  }
  let near = |a: DVec3, b: DVec3| a.distance(b) < JOIN;
  let degree = |p: DVec3| segs.iter().map(|s| near(s.start(), p) as usize + near(s.end(), p) as usize).sum::<usize>();
  if segs.iter().any(|s| degree(s.start()) > 2 || degree(s.end()) > 2) {
    return Err("the path branches; pick one chain of curves".into());
  }
  let start = segs
    .iter()
    .flat_map(|s| [s.start(), s.end()])
    .find(|p| degree(*p) == 1)
    .ok_or("the path is closed; a sweep path must have two ends")?;
  let mut out = Vec::new();
  let mut at = start;
  let mut left = segs;
  while let Some(i) = left.iter().position(|s| near(s.start(), at) || near(s.end(), at)) {
    let s = left.swap_remove(i);
    let s = if near(s.start(), at) { s } else { s.reversed() };
    at = s.end();
    out.push(s);
  }
  if !left.is_empty() {
    return Err("the path curves don't form one connected chain".into());
  }
  Ok(out)
}

/// A sketch's chosen curves as a path wire, through included 3D geometry where chosen.
pub fn sketch_path(sk: &SketchResult, curves: &[CurveId]) -> Result<Shape, String> {
  match path_segs(sk, curves)? {
    Some(segs) => Shape::path3d(&segs).map_err(|e| e.to_string()),
    None => Shape::path(&sk.plane, &sheet::open_path(&sk.sketch, curves)?).map_err(|e| e.to_string()),
  }
}
