//! 2D cutting for lasers, plasma torches and waterjets.
//!
//! The input is a set of edges in a plane (a flat pattern, a face, a sketch). They are chained
//! into closed contours, nested (a contour inside an outline is a hole, one inside a hole is a
//! part again), offset by half the kerf towards the scrap with true arcs
//! ([`cavalier_contours`]), given lead-ins and lead-outs on the scrap side, broken by
//! micro-joints if asked, and ordered nearest-first with every contour cut before the contours
//! around it. [`post`] turns the result into G-code for a machine profile, or an R12 DXF;
//! [`nest`] lays copies of parts out on sheets first.
//!
//! Units are millimetres; segments are polyline-style: two points and a bulge (the tangent of a
//! quarter of the arc's sweep, positive counter-clockwise, 0 for a line).

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

pub mod nest;
pub mod plan;
pub mod post;
#[cfg(test)]
mod tests;

pub use nest::{Nest, NestParams, NestPart, NestSheet, Placement, nest, parts_of};
pub use plan::{CutParams, LEAD_SNAP, Lead, LeadKind, Pass, Plan, SegKind, plan};
pub use post::{Machine, MachineKind, Process, builtin_machines};

pub type P = [f64; 2];

/// Points closer than this are joined when chaining edges (mm).
pub const JOIN_TOL: f64 = 1e-3;

pub(crate) fn sub(a: P, b: P) -> P {
  [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn add(a: P, b: P) -> P {
  [a[0] + b[0], a[1] + b[1]]
}

pub(crate) fn scale(a: P, k: f64) -> P {
  [a[0] * k, a[1] * k]
}

pub(crate) fn dist(a: P, b: P) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

pub(crate) fn dot(a: P, b: P) -> f64 {
  a[0] * b[0] + a[1] * b[1]
}

/// Rotated a quarter turn counter-clockwise.
pub(crate) fn left(a: P) -> P {
  [-a[1], a[0]]
}

pub(crate) fn unit(a: P) -> P {
  let l = a[0].hypot(a[1]);
  if l < 1e-15 { [1.0, 0.0] } else { [a[0] / l, a[1] / l] }
}

pub(crate) fn polar(c: P, r: f64, ang: f64) -> P {
  [c[0] + r * ang.cos(), c[1] + r * ang.sin()]
}

/// A line (bulge 0) or circular arc from `a` to `b`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Seg {
  pub a: P,
  pub b: P,
  #[serde(default)]
  pub bulge: f64,
}

impl Seg {
  pub fn line(a: P, b: P) -> Seg {
    Seg { a, b, bulge: 0.0 }
  }

  /// An arc around `center` from `start_angle` through `sweep` radians (|sweep| ≤ π).
  pub fn arc(center: P, radius: f64, start_angle: f64, sweep: f64) -> Seg {
    Seg {
      a: polar(center, radius, start_angle),
      b: polar(center, radius, start_angle + sweep),
      bulge: (sweep / 4.0).tan(),
    }
  }

  pub fn is_line(&self) -> bool {
    self.bulge.abs() < 1e-9
  }

  /// Signed sweep (radians, positive counter-clockwise; 0 for a line).
  pub fn sweep(&self) -> f64 {
    4.0 * self.bulge.atan()
  }

  /// Centre and radius of an arc.
  pub fn center_radius(&self) -> (P, f64) {
    let d = dist(self.a, self.b);
    let b = self.bulge;
    let r = d * (1.0 + b * b) / (4.0 * b.abs());
    let mid = scale(add(self.a, self.b), 0.5);
    let n = left(unit(sub(self.b, self.a)));
    (add(mid, scale(n, d * (1.0 - b * b) / (4.0 * b))), r)
  }

  pub fn length(&self) -> f64 {
    if self.is_line() {
      return dist(self.a, self.b);
    }
    let (_, r) = self.center_radius();
    r * self.sweep().abs()
  }

  /// The point a fraction `f` of the way along.
  pub fn at(&self, f: f64) -> P {
    if self.is_line() {
      return add(self.a, scale(sub(self.b, self.a), f));
    }
    let (c, r) = self.center_radius();
    let a0 = (self.a[1] - c[1]).atan2(self.a[0] - c[0]);
    polar(c, r, a0 + self.sweep() * f)
  }

  /// Unit direction of travel a fraction `f` of the way along.
  pub fn tangent(&self, f: f64) -> P {
    if self.is_line() {
      return unit(sub(self.b, self.a));
    }
    let (c, _) = self.center_radius();
    let radial = unit(sub(self.at(f), c));
    if self.bulge > 0.0 { left(radial) } else { scale(left(radial), -1.0) }
  }

  /// Split at fraction `f`.
  pub fn split(&self, f: f64) -> (Seg, Seg) {
    let m = self.at(f);
    let s = self.sweep();
    (Seg { a: self.a, b: m, bulge: (s * f / 4.0).tan() }, Seg { a: m, b: self.b, bulge: (s * (1.0 - f) / 4.0).tan() })
  }

  pub fn reversed(&self) -> Seg {
    Seg { a: self.b, b: self.a, bulge: -self.bulge }
  }

  /// Closest point: (fraction along, distance).
  pub fn closest(&self, p: P) -> (f64, f64) {
    if self.is_line() {
      let d = sub(self.b, self.a);
      let l2 = dot(d, d);
      let f = if l2 < 1e-24 { 0.0 } else { (dot(sub(p, self.a), d) / l2).clamp(0.0, 1.0) };
      return (f, dist(p, self.at(f)));
    }
    let (c, r) = self.center_radius();
    let s = self.sweep();
    let a0 = (self.a[1] - c[1]).atan2(self.a[0] - c[0]);
    let ap = (p[1] - c[1]).atan2(p[0] - c[0]);
    // angle from the start in the arc's direction, in [0, 2π)
    let rel = ((ap - a0) * s.signum()).rem_euclid(TAU);
    if rel <= s.abs() {
      let f = rel / s.abs();
      return (f, (dist(p, c) - r).abs());
    }
    let (da, db) = (dist(p, self.a), dist(p, self.b));
    if da <= db { (0.0, da) } else { (1.0, db) }
  }

  /// Points along the segment, `a` excluded, `b` included, chords within `tol` of the arc.
  pub fn flatten(&self, tol: f64, out: &mut Vec<P>) {
    if self.is_line() {
      out.push(self.b);
      return;
    }
    let (_, r) = self.center_radius();
    let step = if tol >= r { PI / 2.0 } else { 2.0 * (1.0 - tol / r).acos() };
    let n = (self.sweep().abs() / step.max(1e-3)).ceil().clamp(1.0, 4096.0) as usize;
    for i in 1..=n {
      out.push(if i == n { self.b } else { self.at(i as f64 / n as f64) });
    }
  }
}

/// An edge of the input geometry, in the cutting plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Edge {
  Line {
    a: P,
    b: P,
  },
  /// From `start_angle` through `sweep` radians (positive counter-clockwise; up to a full turn).
  Arc {
    center: P,
    radius: f64,
    start_angle: f64,
    sweep: f64,
  },
  Circle {
    center: P,
    radius: f64,
  },
  /// Any other curve, as points along it.
  Polyline {
    points: Vec<P>,
  },
}

impl Edge {
  /// The edge as segments (arcs in pieces of at most a quarter turn).
  pub fn segs(&self) -> Vec<Seg> {
    match self {
      Edge::Line { a, b } => vec![Seg::line(*a, *b)],
      Edge::Arc { center, radius, start_angle, sweep } => arc_segs(*center, *radius, *start_angle, *sweep),
      Edge::Circle { center, radius } => arc_segs(*center, *radius, 0.0, TAU),
      Edge::Polyline { points } => points.windows(2).map(|w| Seg::line(w[0], w[1])).collect(),
    }
  }
}

fn arc_segs(c: P, r: f64, start: f64, sweep: f64) -> Vec<Seg> {
  let n = (sweep.abs() / (PI / 2.0) - 1e-9).ceil().max(1.0) as usize;
  let d = sweep / n as f64;
  let mut v: Vec<Seg> = (0..n).map(|i| Seg::arc(c, r, start + d * i as f64, d)).collect();
  // close exactly
  if (sweep.abs() - TAU).abs() < 1e-9
    && let (Some(f), Some(l)) = (v.first().copied(), v.last_mut())
  {
    l.b = f.a;
  }
  v
}

/// A closed chain of segments, each starting where the one before ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contour {
  pub segs: Vec<Seg>,
}

impl Contour {
  /// Signed area (positive = counter-clockwise), arcs included.
  pub fn area(&self) -> f64 {
    let mut a = 0.0;
    for s in &self.segs {
      a += (s.a[0] * s.b[1] - s.b[0] * s.a[1]) / 2.0;
      if !s.is_line() {
        // circular segment between chord and arc
        let (_, r) = s.center_radius();
        let th = s.sweep();
        a += r * r * (th - th.sin()) / 2.0;
      }
    }
    a
  }

  pub fn length(&self) -> f64 {
    self.segs.iter().map(Seg::length).sum()
  }

  pub fn reversed(&self) -> Contour {
    Contour { segs: self.segs.iter().rev().map(Seg::reversed).collect() }
  }

  /// Points around the contour (chords within `tol`).
  pub fn polygon(&self, tol: f64) -> Vec<P> {
    let mut v = Vec::new();
    for s in &self.segs {
      s.flatten(tol, &mut v);
    }
    v
  }

  /// Whether `p` is inside (even-odd on the flattened contour).
  pub fn contains(&self, p: P) -> bool {
    let poly = self.polygon(1e-3);
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
      let (a, b) = (poly[i], poly[(i + 1) % n]);
      if (a[1] > p[1]) != (b[1] > p[1]) {
        let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        if p[0] < x {
          inside = !inside;
        }
      }
    }
    inside
  }

  /// Bounding box (min, max), arcs bulging past their ends included.
  pub fn bounds(&self) -> (P, P) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in self.polygon(1e-3) {
      for k in 0..2 {
        lo[k] = lo[k].min(p[k]);
        hi[k] = hi[k].max(p[k]);
      }
    }
    (lo, hi)
  }

  /// Closest point: (segment index, fraction along it, distance).
  pub fn closest(&self, p: P) -> (usize, f64, f64) {
    let mut best = (0, 0.0, f64::INFINITY);
    for (i, s) in self.segs.iter().enumerate() {
      let (f, d) = s.closest(p);
      if d < best.2 {
        best = (i, f, d);
      }
    }
    best
  }

  /// The same loop starting at fraction `f` of segment `i`.
  pub fn start_at(&self, i: usize, f: f64) -> Contour {
    let n = self.segs.len();
    let mut segs = Vec::with_capacity(n + 1);
    let s = self.segs[i];
    let len = s.length();
    if f * len < 1e-6 {
      segs.extend((0..n).map(|k| self.segs[(i + k) % n]));
    } else if (1.0 - f) * len < 1e-6 {
      segs.extend((1..=n).map(|k| self.segs[(i + k) % n]));
    } else {
      let (x, y) = s.split(f);
      segs.push(y);
      segs.extend((1..n).map(|k| self.segs[(i + k) % n]));
      segs.push(x);
    }
    Contour { segs }
  }
}

/// Joins edges end to end into closed contours. Returns the contours and how many open chains
/// were left over (they can't be cut as closed shapes and are dropped).
pub fn chain(edges: &[Edge], tol: f64) -> (Vec<Contour>, usize) {
  let mut pieces: Vec<Vec<Seg>> =
    edges.iter().map(Edge::segs).filter(|v| v.iter().any(|s| s.length() > tol * 0.01)).collect();
  // drop zero-length segments inside pieces
  for p in &mut pieces {
    p.retain(|s| dist(s.a, s.b) > 1e-9 || !s.is_line());
  }
  pieces.retain(|p| !p.is_empty());
  let mut used = vec![false; pieces.len()];
  let mut out = Vec::new();
  let mut open = 0;
  for i in 0..pieces.len() {
    if used[i] {
      continue;
    }
    used[i] = true;
    let mut segs = pieces[i].clone();
    let start = segs[0].a;
    loop {
      let end = segs[segs.len() - 1].b;
      if dist(end, start) <= tol && (segs.len() > 1 || !segs[0].is_line()) {
        break;
      }
      let mut next = None;
      for (j, p) in pieces.iter().enumerate() {
        if used[j] {
          continue;
        }
        if dist(p[0].a, end) <= tol {
          next = Some((j, false));
          break;
        }
        if dist(p[p.len() - 1].b, end) <= tol {
          next = Some((j, true));
          break;
        }
      }
      let Some((j, rev)) = next else { break };
      used[j] = true;
      let mut more: Vec<Seg> =
        if rev { pieces[j].iter().rev().map(Seg::reversed).collect() } else { pieces[j].clone() };
      more[0].a = end;
      segs.extend(more);
    }
    let n = segs.len();
    if dist(segs[n - 1].b, start) <= tol && (n > 1 || !segs[0].is_line()) {
      segs[n - 1].b = start;
      let c = Contour { segs };
      if c.area().abs() > 1e-9 {
        out.push(c);
      }
    } else {
      open += 1;
    }
  }
  (out, open)
}

/// Nesting depth of each contour: how many others enclose it (even = part outline, odd = hole).
/// Also each contour's innermost container.
pub fn nesting(contours: &[Contour]) -> (Vec<usize>, Vec<Option<usize>>) {
  let areas: Vec<f64> = contours.iter().map(|c| c.area().abs()).collect();
  let mut depth = vec![0; contours.len()];
  let mut parent = vec![None; contours.len()];
  for (i, c) in contours.iter().enumerate() {
    // a point inside contour i's material side but on it: its first segment's middle
    let probe = c.segs[0].at(0.5);
    for (j, o) in contours.iter().enumerate() {
      if i == j || areas[j] <= areas[i] {
        continue;
      }
      if o.contains(probe) {
        depth[i] += 1;
        if parent[i].is_none_or(|p: usize| areas[j] < areas[p]) {
          parent[i] = Some(j);
        }
      }
    }
  }
  (depth, parent)
}
