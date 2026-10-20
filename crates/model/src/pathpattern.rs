//! Pattern on Path: where each copy sits along a chained sketch path, by arc length.

use fox_sketch::Seg;
use serde::{Deserialize, Serialize};

type P2 = [f64; 2];

/// How a path pattern's copies are turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathOrientation {
  /// Every copy keeps the original's orientation; it only moves.
  #[default]
  Identical,
  /// Each copy turns with the path, by the change in its tangent from the original's position.
  AlongPath,
}

/// The spacing, start and direction of a path pattern.
#[derive(Debug, Clone, Copy)]
pub struct PathLayout {
  /// Instances, the original included.
  pub count: usize,
  /// Arc length between neighbours, or the whole pattern's length when `extent` is set.
  pub distance: f64,
  pub extent: bool,
  /// Where the original sits, as a fraction of the path length.
  pub start: f64,
  /// Run toward the path's start.
  pub flip: bool,
  /// Centre the instances on the original. An even count puts the extra one ahead of it.
  pub symmetric: bool,
}

impl PathLayout {
  /// Arc length of the original along a path `length` long.
  pub fn origin(&self, length: f64) -> f64 {
    self.start * length
  }

  /// Arc lengths of the copies (the original left out), in order. Positions past the path's
  /// ends lie on its straight extensions.
  pub fn copies(&self, length: f64) -> Vec<f64> {
    let step = match (self.extent, self.count) {
      (false, _) => self.distance,
      (true, n) if n > 1 => self.distance / (n - 1) as f64,
      (true, _) => 0.0,
    };
    let dir = if self.flip { -1.0 } else { 1.0 };
    let first = if self.symmetric { (self.count.max(1) - 1) / 2 } else { 0 };
    let s0 = self.origin(length);
    (0..self.count).filter(|&k| k != first).map(|k| s0 + dir * step * (k as f64 - first as f64)).collect()
  }
}

/// Total arc length of a chained path.
pub fn path_length(path: &[Seg]) -> f64 {
  path.iter().map(seg_length).sum()
}

/// The point and unit tangent (direction of travel) at arc length `s` along a chained path.
/// Before its start and past its end the path runs on straight, along its end tangents.
pub fn path_at(path: &[Seg], s: f64) -> Option<(P2, P2)> {
  let first = path.first()?;
  if s < 0.0 {
    let (p, t) = seg_at(first, 0.0)?;
    return Some((add(p, mul(t, s)), t));
  }
  let mut left = s;
  for (i, seg) in path.iter().enumerate() {
    let l = seg_length(seg);
    if left <= l || i + 1 == path.len() {
      if left <= l {
        return seg_at(seg, left);
      }
      let (p, t) = seg_at(seg, l)?;
      return Some((add(p, mul(t, left - l)), t));
    }
    left -= l;
  }
  None
}

fn seg_length(s: &Seg) -> f64 {
  match *s {
    Seg::Line { a, b } => dist(a, b),
    Seg::Arc { radius, sweep, .. } => radius * sweep.abs(),
    _ => polyline(s).windows(2).map(|w| dist(w[0], w[1])).sum(),
  }
}

/// Point and unit tangent `len` along a segment (clamped to it).
fn seg_at(s: &Seg, len: f64) -> Option<(P2, P2)> {
  match *s {
    Seg::Line { a, b } => {
      let t = unit([b[0] - a[0], b[1] - a[1]])?;
      Some((add(a, mul(t, len.clamp(0.0, dist(a, b)))), t))
    }
    Seg::Arc { center, radius, start_angle, sweep } => {
      if radius <= 0.0 {
        return None;
      }
      let a = start_angle + sweep.signum() * (len / radius).clamp(0.0, sweep.abs());
      let (sn, c) = a.sin_cos();
      let dir = sweep.signum();
      Some(([center[0] + radius * c, center[1] + radius * sn], [-sn * dir, c * dir]))
    }
    _ => {
      let pts = polyline(s);
      let mut left = len.max(0.0);
      for w in pts.windows(2) {
        let l = dist(w[0], w[1]);
        if l < 1e-12 {
          continue;
        }
        let t = unit([w[1][0] - w[0][0], w[1][1] - w[0][1]])?;
        if left <= l {
          return Some((add(w[0], mul(t, left)), t));
        }
        left -= l;
      }
      let n = pts.len();
      let t = unit([pts[n - 1][0] - pts[n - 2][0], pts[n - 1][1] - pts[n - 2][1]])?;
      Some((pts[n - 1], t))
    }
  }
}

/// Points along a curved segment, start and end included.
fn polyline(s: &Seg) -> Vec<P2> {
  let mut out = Vec::new();
  match *s {
    Seg::Bezier { p } => {
      const STEPS: usize = 256;
      for i in 0..=STEPS {
        let t = i as f64 / STEPS as f64;
        let u = 1.0 - t;
        let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push([
          b0 * p[0][0] + b1 * p[1][0] + b2 * p[2][0] + b3 * p[3][0],
          b0 * p[0][1] + b1 * p[1][1] + b2 * p[2][1] + b3 * p[3][1],
        ]);
      }
    }
    _ => {
      s.flatten(&mut out);
      out.push(s.end());
    }
  }
  out
}

fn dist(a: P2, b: P2) -> f64 {
  (b[0] - a[0]).hypot(b[1] - a[1])
}

fn add(a: P2, b: P2) -> P2 {
  [a[0] + b[0], a[1] + b[1]]
}

fn mul(a: P2, k: f64) -> P2 {
  [a[0] * k, a[1] * k]
}

fn unit(v: P2) -> Option<P2> {
  let l = v[0].hypot(v[1]);
  (l > 1e-12).then(|| [v[0] / l, v[1] / l])
}

#[cfg(test)]
mod tests {
  use super::*;

  fn layout(count: usize, distance: f64) -> PathLayout {
    PathLayout { count, distance, extent: false, start: 0.0, flip: false, symmetric: false }
  }

  #[test]
  fn copies_follow_spacing_extent_symmetric_and_flip() {
    assert_eq!(layout(3, 10.0).copies(100.0), vec![10.0, 20.0]);
    assert_eq!(PathLayout { extent: true, ..layout(5, 80.0) }.copies(100.0), vec![20.0, 40.0, 60.0, 80.0]);
    assert_eq!(PathLayout { symmetric: true, ..layout(5, 15.0) }.copies(100.0), vec![-30.0, -15.0, 15.0, 30.0]);
    assert_eq!(PathLayout { start: 1.0, flip: true, ..layout(4, 25.0) }.copies(100.0), vec![75.0, 50.0, 25.0]);
  }

  #[test]
  fn path_points_turn_corners_and_extend_past_the_ends() {
    let path = [Seg::Line { a: [0.0, 0.0], b: [60.0, 0.0] }, Seg::Line { a: [60.0, 0.0], b: [60.0, 40.0] }];
    assert_eq!(path_length(&path), 100.0);
    let at = |s: f64| path_at(&path, s).unwrap();
    assert_eq!(at(25.0), ([25.0, 0.0], [1.0, 0.0]));
    assert_eq!(at(75.0), ([60.0, 15.0], [0.0, 1.0]));
    assert_eq!(at(-30.0), ([-30.0, 0.0], [1.0, 0.0]));
    assert_eq!(at(110.0), ([60.0, 50.0], [0.0, 1.0]));
    let arc = [Seg::Arc { center: [0.0, 0.0], radius: 50.0, start_angle: 0.0, sweep: std::f64::consts::FRAC_PI_2 }];
    let (p, t) = path_at(&arc, 50.0 * std::f64::consts::FRAC_PI_2).unwrap();
    assert!((p[0]).abs() < 1e-9 && (p[1] - 50.0).abs() < 1e-9 && (t[0] + 1.0).abs() < 1e-9);
  }
}
