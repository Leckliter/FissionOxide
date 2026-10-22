//! Included 3D geometry: sketch curves that keep their world positions instead of lying on the
//! sketch plane (Fusion's Include 3D Geometry). They take part in sweep paths and rails, not in
//! profiles or the solver.

use serde::{Deserialize, Serialize};

use crate::{CurveId, Sketch};

/// A world-space point.
pub type P3 = [f64; 3];

/// The other sketch's curve an included curve copies; it follows that curve when the model
/// rebuilds. (Included body edges follow their edge through the model's sketch links instead.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurveSource {
  /// The source sketch's feature id.
  pub sketch: u32,
  pub curve: CurveId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Curve3d {
  pub id: CurveId,
  pub kind: Curve3dKind,
  #[serde(default)]
  pub construction: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<CurveSource>,
  /// Drawn with 3D Sketch on (a 3D sketch line), not included from other geometry: drawn as the
  /// sketch's own curves are, whatever the palette's Projected Geometries says.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub drawn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Curve3dKind {
  Line {
    a: P3,
    b: P3,
  },
  /// A circular arc from `start` through `mid` to `end`.
  Arc {
    start: P3,
    mid: P3,
    end: P3,
  },
  Circle {
    center: P3,
    normal: P3,
    radius: f64,
  },
  /// A sampled free-form curve (splines and other edges), start to end.
  Polyline {
    points: Vec<P3>,
  },
}

fn sub(a: P3, b: P3) -> P3 {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: P3, b: P3) -> P3 {
  [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: P3, k: f64) -> P3 {
  [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: P3, b: P3) -> f64 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: P3) -> Option<P3> {
  let l = dot(a, a).sqrt();
  (l > 1e-12).then(|| scale(a, 1.0 / l))
}

/// The centre of the circle through three points, with the unit normal of their plane (the
/// direction that makes start → mid → end counter-clockwise); `None` when they are collinear.
pub fn circle_through(a: P3, m: P3, b: P3) -> Option<(P3, P3)> {
  let (u, v) = (sub(m, a), sub(b, a));
  let w = cross(u, v);
  let ww = dot(w, w);
  if ww < 1e-18 {
    return None;
  }
  // circumcentre of a triangle in 3D
  let t = add(scale(cross(w, u), dot(v, v)), scale(cross(v, w), dot(u, u)));
  Some((add(a, scale(t, 0.5 / ww)), unit(w)?))
}

impl Curve3dKind {
  /// Start and end points (equal for a circle).
  pub fn ends(&self) -> Option<(P3, P3)> {
    match self {
      Curve3dKind::Line { a, b } => Some((*a, *b)),
      Curve3dKind::Arc { start, end, .. } => Some((*start, *end)),
      Curve3dKind::Circle { .. } => None,
      Curve3dKind::Polyline { points } => Some((*points.first()?, *points.last()?)),
    }
  }

  /// Points along the curve for drawing; arcs and circles get `n` pieces.
  pub fn polyline(&self, n: usize) -> Vec<P3> {
    let n = n.max(2);
    let ring = |c: P3, u: P3, v: P3, sweep: f64| -> Vec<P3> {
      (0..=n)
        .map(|i| {
          let t = sweep * i as f64 / n as f64;
          add(c, add(scale(u, t.cos()), scale(v, t.sin())))
        })
        .collect()
    };
    match self {
      Curve3dKind::Line { a, b } => vec![*a, *b],
      Curve3dKind::Arc { start, mid, end } => {
        let Some((c, nrm)) = circle_through(*start, *mid, *end) else { return vec![*start, *end] };
        let u = sub(*start, c);
        let v = cross(nrm, u);
        let e = sub(*end, c);
        let mut sweep = dot(e, v).atan2(dot(e, u)).rem_euclid(std::f64::consts::TAU);
        if sweep < 1e-12 {
          sweep = std::f64::consts::TAU;
        }
        ring(c, u, v, sweep)
      }
      Curve3dKind::Circle { center, normal, radius } => {
        let Some(nrm) = unit(*normal) else { return Vec::new() };
        let a = if nrm[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let Some(u) = unit(sub(a, scale(nrm, dot(a, nrm)))) else { return Vec::new() };
        let v = cross(nrm, u);
        ring(*center, scale(u, *radius), scale(v, *radius), std::f64::consts::TAU)
      }
      Curve3dKind::Polyline { points } => points.clone(),
    }
  }
}

impl Sketch {
  /// Adds an included 3D curve and returns its id (shared with the 2D curves' ids).
  pub fn add_curve3d(&mut self, kind: Curve3dKind, source: Option<CurveSource>) -> CurveId {
    let id = CurveId(self.alloc());
    self.curves3d.push(Curve3d { id, kind, construction: false, source, drawn: false });
    id
  }

  /// Adds a 3D sketch line from `a` to `b` (world positions), as 3D Sketch draws it.
  pub fn add_line3d(&mut self, a: P3, b: P3) -> CurveId {
    let id = CurveId(self.alloc());
    self.curves3d.push(Curve3d {
      id,
      kind: Curve3dKind::Line { a, b },
      construction: false,
      source: None,
      drawn: true,
    });
    id
  }

  pub fn curve3d(&self, id: CurveId) -> Option<&Curve3d> {
    self.curves3d.iter().find(|c| c.id == id)
  }

  pub fn curve3d_mut(&mut self, id: CurveId) -> Option<&mut Curve3d> {
    self.curves3d.iter_mut().find(|c| c.id == id)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn included_curves_take_ids_and_delete() {
    let mut s = Sketch::new();
    let l = s.add_curve3d(Curve3dKind::Line { a: [0.0, 0.0, 0.0], b: [0.0, 0.0, 5.0] }, None);
    let arc = Curve3dKind::Arc { start: [1.0, 0.0, 0.0], mid: [0.0, 1.0, 0.0], end: [-1.0, 0.0, 0.0] };
    let a = s.add_curve3d(arc.clone(), None);
    assert_ne!(l, a);
    let pts = arc.polyline(8);
    assert_eq!(pts.len(), 9);
    assert!(pts.iter().all(|p| (dot(*p, *p) - 1.0).abs() < 1e-9 && p[1] >= -1e-9));
    assert!(s.remove_curve(l).is_ok());
    assert!(s.curve3d(l).is_none() && s.curve3d(a).is_some());
    let json = serde_json::to_string(&s).unwrap_or_default();
    let back: Sketch = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(back, s);
  }
}
