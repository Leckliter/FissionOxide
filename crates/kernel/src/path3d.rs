//! Paths that leave the sketch plane: chains of world-space pieces (included 3D geometry) and
//! planar sketch segments, as sweep paths and rails.

use fox_sketch::Seg;
use glam::DVec3;

use crate::{KernelError, Plane, Result, Shape, push_seg, sys};

/// One piece of a 3D path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg3 {
  Line {
    a: DVec3,
    b: DVec3,
  },
  /// A circular arc from `start` through `mid` to `end`.
  Arc {
    start: DVec3,
    mid: DVec3,
    end: DVec3,
  },
  /// A sketch segment on its sketch plane.
  Planar {
    plane: Plane,
    seg: Seg,
  },
}

impl Seg3 {
  pub fn start(&self) -> DVec3 {
    match *self {
      Seg3::Line { a, .. } => a,
      Seg3::Arc { start, .. } => start,
      Seg3::Planar { plane, seg } => plane.to_world(seg.start()),
    }
  }

  pub fn end(&self) -> DVec3 {
    match *self {
      Seg3::Line { b, .. } => b,
      Seg3::Arc { end, .. } => end,
      Seg3::Planar { plane, seg } => plane.to_world(seg.end()),
    }
  }

  pub fn reversed(&self) -> Seg3 {
    match *self {
      Seg3::Line { a, b } => Seg3::Line { a: b, b: a },
      Seg3::Arc { start, mid, end } => Seg3::Arc { start: end, mid, end: start },
      Seg3::Planar { plane, seg } => Seg3::Planar { plane, seg: seg.reversed() },
    }
  }

  /// Direction of travel leaving the start (not unit length).
  pub fn start_tangent(&self) -> DVec3 {
    match *self {
      Seg3::Line { a, b } => b - a,
      Seg3::Arc { start, mid, end } => {
        let (u, v) = (mid - start, end - start);
        let w = u.cross(v);
        if w.length_squared() < 1e-18 {
          return mid - start;
        }
        let c =
          start + (w.cross(u) * v.length_squared() + v.cross(w) * u.length_squared()) / (2.0 * w.length_squared());
        w.cross(start - c)
      }
      Seg3::Planar { plane, seg } => {
        let t = seg.end_tangents().0;
        plane.x * t[0] + plane.y * t[1]
      }
    }
  }
}

impl Shape {
  /// An open chain of world-space pieces, in order, as a wire (sweep paths through included 3D
  /// geometry).
  pub fn path3d(segs: &[Seg3]) -> Result<Shape> {
    let mut data = Vec::new();
    let put = |out: &mut Vec<f64>, kind: f64, p: [DVec3; 3]| {
      out.push(kind);
      for v in p.into_iter().chain([DVec3::ZERO]) {
        out.extend_from_slice(&[v.x, v.y, v.z]);
      }
    };
    for s in segs {
      match *s {
        Seg3::Line { a, b } => put(&mut data, 0.0, [a, b, DVec3::ZERO]),
        Seg3::Arc { start, mid, end } => put(&mut data, 1.0, [start, mid, end]),
        Seg3::Planar { plane, seg } => push_seg(&mut data, &plane, &seg),
      }
    }
    if data.is_empty() || data.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("the path is empty or not finite".into()));
    }
    Shape::wrap(sys::make_wire(&data)?)
  }
}
