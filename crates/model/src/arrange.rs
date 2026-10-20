//! Arrange: pack copies of components flat on a rectangular envelope for manufacturing (nesting
//! for laser, plasma or router cutting, or a 3D printer's bed).
//!
//! Each part is laid on its largest flat face and seen from above as a 2D outline (its bounding
//! rectangle for the rectangular solver, the down face's outer loop for true shape). Parts go
//! in largest first; each takes the allowed rotation and position that keeps it furthest left,
//! then lowest (the envelope fills column by column), at least the object spacing from every
//! part already placed and inside the frame.

use serde::{Deserialize, Serialize};

use crate::ComponentId;

/// How parts' outlines are packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrangeSolver {
  /// Each part's bounding rectangle.
  #[default]
  Rectangular,
  /// The parts' real outlines, so they can nest into each other's notches.
  TrueShape,
}

/// Turns a part may take in the envelope's plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrangeRotation {
  /// As laid flat.
  #[serde(rename = "none")]
  None,
  /// As laid flat or turned half way round.
  #[serde(rename = "180")]
  Half,
  /// Quarter turns.
  #[serde(rename = "90")]
  Quarter,
  /// Any: quarter turns, and eighth turns for true shape.
  #[default]
  #[serde(rename = "all")]
  All,
}

impl ArrangeRotation {
  /// The angles (radians) tried, in order of preference.
  pub fn angles(self, solver: ArrangeSolver) -> Vec<f64> {
    let steps: &[u32] = match (self, solver) {
      (ArrangeRotation::None, _) => &[0],
      (ArrangeRotation::Half, _) => &[0, 4],
      (ArrangeRotation::Quarter, _) | (ArrangeRotation::All, ArrangeSolver::Rectangular) => &[0, 2, 4, 6],
      (ArrangeRotation::All, ArrangeSolver::TrueShape) => &[0, 2, 4, 6, 1, 3, 5, 7],
    };
    steps.iter().map(|k| *k as f64 * std::f64::consts::FRAC_PI_4).collect()
  }
}

/// A component to arrange.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrangeItem {
  pub component: ComponentId,
  #[serde(default)]
  pub rotation: ArrangeRotation,
  /// How many copies to place.
  #[serde(default = "one")]
  pub quantity: u32,
}

fn one() -> u32 {
  1
}

/// Where a part goes: turned by `angle` about the envelope's origin, then moved by `offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
  pub angle: f64,
  pub offset: [f64; 2],
}

/// One part to pack: its outline (a closed polygon, either winding) and the angles it may take.
#[derive(Debug, Clone)]
pub struct Part {
  pub outline: Vec<[f64; 2]>,
  pub angles: Vec<f64>,
}

/// The envelope and spacing rules.
#[derive(Debug, Clone, Copy)]
pub struct Envelope {
  /// Size along the plane's X and Y.
  pub size: [f64; 2],
  /// Smallest gap between parts.
  pub spacing: f64,
  /// Margin kept clear along the envelope's edges.
  pub frame: f64,
  pub solver: ArrangeSolver,
}

const EPS: f64 = 1e-7;

/// A part's outline at one angle, moved so its bounding box starts at the origin.
struct Shape2 {
  angle: f64,
  pts: Vec<[f64; 2]>,
  /// Where the turned outline's bounding box started (subtracted from `pts`).
  min: [f64; 2],
  size: [f64; 2],
}

impl Shape2 {
  fn new(outline: &[[f64; 2]], angle: f64, rect: bool) -> Shape2 {
    let (s, c) = angle.sin_cos();
    let turned: Vec<[f64; 2]> = outline.iter().map(|p| [c * p[0] - s * p[1], s * p[0] + c * p[1]]).collect();
    let (min, max) = bounds(&turned);
    let size = [max[0] - min[0], max[1] - min[1]];
    let pts = if rect {
      vec![[0.0, 0.0], [size[0], 0.0], [size[0], size[1]], [0.0, size[1]]]
    } else {
      turned.iter().map(|p| [p[0] - min[0], p[1] - min[1]]).collect()
    };
    Shape2 { angle, pts, min, size }
  }
}

/// A part already in the envelope.
struct Placed {
  pts: Vec<[f64; 2]>,
  min: [f64; 2],
  max: [f64; 2],
}

/// Packs the parts; `None` for a part that doesn't fit.
pub fn pack(parts: &[Part], env: Envelope) -> Vec<Option<Placement>> {
  let rect = env.solver == ArrangeSolver::Rectangular;
  // a zero spacing still keeps parts from overlapping: they may touch to within a hair
  let gap = env.spacing.max(1e-6);
  let lo = [env.frame, env.frame];
  let hi = [env.size[0] - env.frame, env.size[1] - env.frame];
  let mut order: Vec<usize> = (0..parts.len()).collect();
  let area = |p: &Part| if rect { bbox_area(&p.outline) } else { polygon_area(&p.outline).abs() };
  order.sort_by(|a, b| area(&parts[*b]).total_cmp(&area(&parts[*a])));
  let mut placed: Vec<Placed> = Vec::new();
  let mut out = vec![None; parts.len()];
  for i in order {
    let part = &parts[i];
    // (x max, y max, shape, x, y)
    let mut best: Option<(f64, f64, Shape2, f64, f64)> = None;
    for &angle in &part.angles {
      let sh = Shape2::new(&part.outline, angle, rect);
      let xs = candidates(lo[0], &placed, &sh, gap, 0);
      let ys = candidates(lo[1], &placed, &sh, gap, 1);
      'x: for &x in &xs {
        if x + sh.size[0] > hi[0] + EPS {
          break;
        }
        if let Some(b) = &best
          && x + sh.size[0] > b.0 + EPS
        {
          break;
        }
        for &y in &ys {
          if y + sh.size[1] > hi[1] + EPS {
            continue 'x;
          }
          if let Some(b) = &best
            && (x + sh.size[0] - b.0).abs() <= EPS
            && y + sh.size[1] >= b.1 - EPS
          {
            continue 'x;
          }
          if fits(&sh, [x, y], &placed, gap) {
            best = Some((x + sh.size[0], y + sh.size[1], Shape2 { pts: sh.pts.clone(), ..sh }, x, y));
            continue 'x;
          }
        }
      }
    }
    let Some((_, _, sh, x, y)) = best else { continue };
    let pts: Vec<[f64; 2]> = sh.pts.iter().map(|p| [p[0] + x, p[1] + y]).collect();
    placed.push(Placed { pts, min: [x, y], max: [x + sh.size[0], y + sh.size[1]] });
    out[i] = Some(Placement { angle: sh.angle, offset: [x - sh.min[0], y - sh.min[1]] });
  }
  out
}

/// Positions worth trying along one axis: against the frame, or a vertex the spacing away from
/// a placed part's vertex, sorted.
fn candidates(lo: f64, placed: &[Placed], sh: &Shape2, gap: f64, axis: usize) -> Vec<f64> {
  let mut v = vec![lo];
  for p in placed {
    for q in &p.pts {
      for s in &sh.pts {
        for d in [gap, -gap] {
          let c = q[axis] - s[axis] + d;
          if c >= lo - EPS {
            v.push(c.max(lo));
          }
        }
      }
    }
  }
  v.sort_by(f64::total_cmp);
  v.dedup_by(|a, b| (*a - *b).abs() <= EPS);
  v
}

/// Whether the shape at `at` keeps `gap` from every placed part.
fn fits(sh: &Shape2, at: [f64; 2], placed: &[Placed], gap: f64) -> bool {
  let (min, max) = ([at[0], at[1]], [at[0] + sh.size[0], at[1] + sh.size[1]]);
  let pts: Vec<[f64; 2]> = sh.pts.iter().map(|p| [p[0] + at[0], p[1] + at[1]]).collect();
  placed.iter().all(|q| {
    // bounding boxes far enough apart settle it without looking at edges
    if min[0] >= q.max[0] + gap - EPS
      || q.min[0] >= max[0] + gap - EPS
      || min[1] >= q.max[1] + gap - EPS
      || q.min[1] >= max[1] + gap - EPS
    {
      return true;
    }
    polygon_distance(&pts, &q.pts) >= gap - EPS && !inside(pts[0], &q.pts) && !inside(q.pts[0], &pts)
  })
}

fn bounds(pts: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
  let mut lo = [f64::INFINITY; 2];
  let mut hi = [f64::NEG_INFINITY; 2];
  for p in pts {
    for k in 0..2 {
      lo[k] = lo[k].min(p[k]);
      hi[k] = hi[k].max(p[k]);
    }
  }
  (lo, hi)
}

fn bbox_area(pts: &[[f64; 2]]) -> f64 {
  let (lo, hi) = bounds(pts);
  (hi[0] - lo[0]).max(0.0) * (hi[1] - lo[1]).max(0.0)
}

/// Signed area (positive counter-clockwise).
pub fn polygon_area(pts: &[[f64; 2]]) -> f64 {
  let n = pts.len();
  (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>() / 2.0
}

/// Smallest distance between two closed polylines' edges (0 when they cross).
fn polygon_distance(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
  let mut d = f64::INFINITY;
  for i in 0..a.len() {
    let (a0, a1) = (a[i], a[(i + 1) % a.len()]);
    for j in 0..b.len() {
      d = d.min(segment_distance(a0, a1, b[j], b[(j + 1) % b.len()]));
      if d <= 0.0 {
        return 0.0;
      }
    }
  }
  d
}

fn segment_distance(p0: [f64; 2], p1: [f64; 2], q0: [f64; 2], q1: [f64; 2]) -> f64 {
  let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  let (d1, d2) = (cross(q0, q1, p0), cross(q0, q1, p1));
  let (d3, d4) = (cross(p0, p1, q0), cross(p0, p1, q1));
  if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
    return 0.0;
  }
  point_segment(p0, q0, q1).min(point_segment(p1, q0, q1)).min(point_segment(q0, p0, p1)).min(point_segment(q1, p0, p1))
}

fn point_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l2 = dx * dx + dy * dy;
  let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
  ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

/// Even-odd point in polygon.
fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
  let mut c = false;
  let n = poly.len();
  for i in 0..n {
    let (a, b) = (poly[i], poly[(i + 1) % n]);
    if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
      c = !c;
    }
  }
  c
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rect(w: f64, h: f64) -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
  }

  fn world(part: &Part, pl: Placement) -> ([f64; 2], [f64; 2]) {
    let (s, c) = pl.angle.sin_cos();
    let pts: Vec<[f64; 2]> =
      part.outline.iter().map(|p| [c * p[0] - s * p[1] + pl.offset[0], s * p[0] + c * p[1] + pl.offset[1]]).collect();
    bounds(&pts)
  }

  #[test]
  fn rectangles_fill_a_column_first() {
    let all = ArrangeRotation::All.angles(ArrangeSolver::Rectangular);
    let parts =
      vec![Part { outline: rect(20.0, 20.0), angles: all.clone() }, Part { outline: rect(40.0, 30.0), angles: all }];
    let env = Envelope { size: [200.0, 100.0], spacing: 5.0, frame: 0.0, solver: ArrangeSolver::Rectangular };
    let out = pack(&parts, env);
    // the big one goes first, turned upright; the small one above it
    let (lo, hi) = world(&parts[1], out[1].unwrap());
    assert!((lo[0]).abs() < 1e-9 && (lo[1]).abs() < 1e-9 && (hi[0] - 30.0).abs() < 1e-9 && (hi[1] - 40.0).abs() < 1e-9);
    let (lo, hi) = world(&parts[0], out[0].unwrap());
    assert!((lo[0]).abs() < 1e-9 && (lo[1] - 45.0).abs() < 1e-9 && (hi[1] - 65.0).abs() < 1e-9);
  }

  #[test]
  fn true_shape_nests_l_plates() {
    let l = vec![[0.0, 0.0], [50.0, 0.0], [50.0, 15.0], [20.0, 15.0], [20.0, 40.0], [0.0, 40.0]];
    let all = ArrangeRotation::All.angles(ArrangeSolver::TrueShape);
    let parts = vec![Part { outline: l.clone(), angles: all.clone() }, Part { outline: l, angles: all }];
    let env = Envelope { size: [150.0, 120.0], spacing: 3.0, frame: 5.0, solver: ArrangeSolver::TrueShape };
    let out = pack(&parts, env);
    let (a, b) = (world(&parts[0], out[0].unwrap()), world(&parts[1], out[1].unwrap()));
    let lo = [a.0[0].min(b.0[0]), a.0[1].min(b.0[1])];
    let hi = [a.1[0].max(b.1[0]), a.1[1].max(b.1[1])];
    for (got, want) in [lo[0], lo[1], hi[0], hi[1]].into_iter().zip([5.0, 5.0, 45.0, 78.0]) {
      assert!((got - want).abs() < 1e-6, "{lo:?} {hi:?}");
    }
  }

  #[test]
  fn parts_that_dont_fit_are_left_out() {
    let parts = vec![
      Part { outline: rect(60.0, 60.0), angles: vec![0.0] },
      Part { outline: rect(60.0, 60.0), angles: vec![0.0] },
    ];
    let env = Envelope { size: [100.0, 100.0], spacing: 0.0, frame: 0.0, solver: ArrangeSolver::TrueShape };
    let out = pack(&parts, env);
    assert!(out[0].is_some() && out[1].is_none());
  }
}
