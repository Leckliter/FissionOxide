//! Intersect (Fusion's `Sketch.intersect`): the curves where bodies, or chosen faces of them, cut
//! the sketch plane. The kernel sections the shapes with a plane face; the pieces come back as
//! lines, arcs, circles and splines in sketch coordinates and are kept up to date the way spun
//! profiles are (`Sketch::add_section`).

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind, Plane, Shape};
use fox_sketch::SpunSeg;

use super::Ctx;
use crate::BodyId;

/// How far apart two section edges' ends may be and still count as one point (mm).
const JOIN_TOL: f64 = 1e-4;

impl Ctx<'_> {
  /// The section outline of a sketch's Intersect from the bodies as they are now.
  pub(super) fn section_outline(&self, plane: &Plane, sp: &fox_sketch::SketchSpun) -> Result<Vec<SpunSeg>, String> {
    let mut picks = Vec::new();
    for b in &sp.bodies {
      let bi = self.body_index(BodyId(*b))?;
      let faces: Vec<u32> = sp.faces.iter().filter(|f| f[0] == *b).map(|f| f[1]).collect();
      picks.push((&self.state.bodies[bi].shape, faces));
    }
    outline(&picks, plane)
  }
}

/// The curves where `shapes` (each with the faces of it to section; none = the whole shape) cut
/// `plane`, in the plane's coordinates.
pub fn outline(shapes: &[(&Shape, Vec<u32>)], plane: &Plane) -> Result<Vec<SpunSeg>, String> {
  if shapes.is_empty() {
    return Err("there are no bodies to intersect".into());
  }
  let n = plane.normal();
  // a plane face big enough to pass every body
  let mut size: f64 = 1.0;
  for (s, _) in shapes {
    if let Ok(m) = s.mass() {
      for c in [m.bbox_min, m.bbox_max] {
        size = size.max((c - plane.origin).length());
      }
      size = size.max((m.bbox_max - m.bbox_min).length());
    }
  }
  let cutter = Shape::plane_face(plane.origin, n, size * 2.0).map_err(|e| e.to_string())?;
  let mut out: Vec<SpunSeg> = Vec::new();
  let mut any = false;
  for (shape, faces) in shapes {
    let tool = if faces.is_empty() {
      (*shape).clone()
    } else {
      let mut parts = Vec::new();
      for f in faces {
        parts.push(shape.face_shape(*f).map_err(|e| format!("face {f}: {e}"))?);
      }
      Shape::compound(&parts).map_err(|e| e.to_string())?
    };
    // a body the plane misses adds nothing; only all of them missing is an error
    let Ok(wires) = cutter.section_wires(&tool, JOIN_TOL) else { continue };
    for w in &wires {
      for seg in segs(w, plane)? {
        if !out.iter().any(|o| same(o, &seg)) {
          out.push(seg);
        }
      }
    }
    any = any || !wires.is_empty();
  }
  if !any || out.is_empty() {
    return Err("the bodies don't cross the sketch plane".into());
  }
  Ok(out)
}

/// A wire's edges as outline pieces in `plane`'s coordinates.
fn segs(wire: &Shape, plane: &Plane) -> Result<Vec<SpunSeg>, String> {
  let l = |v: DVec3| plane.to_local(v);
  let edges = wire.edges().map_err(|e| e.to_string())?;
  let mut polylines = None;
  let mut out = Vec::new();
  for e in &edges {
    match e.kind {
      CurveKind::Line => out.push(SpunSeg::Line { a: l(e.start), b: l(e.end) }),
      CurveKind::Circle if (e.start - e.end).length() <= JOIN_TOL => {
        out.push(SpunSeg::Circle { center: l(e.center), radius: e.radius })
      }
      CurveKind::Circle => out.push(SpunSeg::Arc { a: l(e.start), mid: l(e.mid), b: l(e.end) }),
      CurveKind::Degenerate => {}
      CurveKind::Other => {
        if polylines.is_none() {
          let deflection = (e.length * 0.002).clamp(0.005, 0.2);
          polylines = Some(wire.edge_lines(deflection).map_err(|x| x.to_string())?);
        }
        let Some(pl) = polylines.as_ref().and_then(|p| p.iter().find(|x| x.id == e.id)) else { continue };
        let mut points: Vec<[f64; 2]> = Vec::new();
        for q in &pl.points {
          let q = l(*q);
          if points.last().is_none_or(|p| (p[0] - q[0]).hypot(p[1] - q[1]) > 1e-7) {
            points.push(q);
          }
        }
        match points.len() {
          0 | 1 => {}
          2 => out.push(SpunSeg::Line { a: points[0], b: points[1] }),
          _ => out.push(SpunSeg::Spline { points }),
        }
      }
    }
  }
  Ok(out)
}

/// The same piece, possibly traced the other way: faces that share a section edge each give it.
fn same(a: &SpunSeg, b: &SpunSeg) -> bool {
  let p = |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) <= JOIN_TOL;
  match (a, b) {
    (SpunSeg::Line { a, b }, SpunSeg::Line { a: c, b: d }) => (p(a, c) && p(b, d)) || (p(a, d) && p(b, c)),
    (SpunSeg::Arc { a, mid, b }, SpunSeg::Arc { a: c, mid: m, b: d }) => {
      p(mid, m) && ((p(a, c) && p(b, d)) || (p(a, d) && p(b, c)))
    }
    (SpunSeg::Circle { center, radius }, SpunSeg::Circle { center: c, radius: r }) => {
      p(center, c) && (radius - r).abs() <= JOIN_TOL
    }
    (SpunSeg::Spline { points }, SpunSeg::Spline { points: q }) => {
      points.len() == q.len()
        && (points.iter().zip(q).all(|(a, b)| p(a, b)) || points.iter().zip(q.iter().rev()).all(|(a, b)| p(a, b)))
    }
    _ => false,
  }
}
