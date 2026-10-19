//! Arc fitting: runs of short cutting moves that follow a circle in the XY plane become single
//! G2/G3 arcs. Toolpaths come out of the offsetter and the sketch as polylines; controls move
//! smoother and programs get far shorter when circles are sent as circles.

use fox_kernel::glam::{DVec2, DVec3};

/// A move from the previous point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
  Line(DVec3),
  /// To `to` around `center` (same Z), clockwise (G2) or counter-clockwise (G3).
  Arc {
    to: DVec3,
    center: DVec3,
    cw: bool,
  },
}

/// The fewest points that make an arc; shorter runs stay lines.
const MIN_POINTS: usize = 4;
/// Arcs larger than this are left as lines: controls round big radii badly, and they are rarely
/// arcs in the first place.
const MAX_RADIUS: f64 = 5000.0;
/// How far a step's chord may sag inside its arc (mm). Polylines that approximate circles sag
/// far less; a polygon (a 13-sided pocket, say) sags more and stays a polygon. It also bounds how
/// far the arcs stray from the polyline.
pub const MAX_SAG: f64 = 0.02;
/// How close the fitted arcs stay to the polyline's points (mm), as the posts fit them.
pub const FIT_TOL: f64 = 0.002;

/// Moves from `from` through `points` (all at one feed), with runs that lie within `tol` of a
/// circle at one height merged into arcs (a full circle becomes two halves). Every point stays on the path: only the moves between
/// them change.
pub fn fit(from: DVec3, points: &[DVec3], tol: f64) -> Vec<Seg> {
  let mut out = Vec::new();
  let mut start = from;
  let mut i = 0;
  while i < points.len() {
    // the longest run start, points[i..=j] that is one arc
    let mut best = None;
    let mut j = i + MIN_POINTS - 2;
    while j < points.len() {
      match arc_through(start, &points[i..=j], tol) {
        Some(a) => best = Some((j, a)),
        None => break,
      }
      j += 1;
    }
    match best {
      Some((j, (center, cw, sweep))) => {
        // a full turn as two halves, which every control reads the same way
        if sweep > std::f64::consts::PI * 1.9 {
          out.push(Seg::Arc { to: points[i + (j - i) / 2], center, cw });
        }
        out.push(Seg::Arc { to: points[j], center, cw });
        start = points[j];
        i = j + 1;
      }
      None => {
        out.push(Seg::Line(points[i]));
        start = points[i];
        i += 1;
      }
    }
  }
  out
}

/// The circle through `start` and `run` when every point is within `tol` of it, all at one Z,
/// turning one way by at most a full turn: (centre, clockwise, sweep).
fn arc_through(start: DVec3, run: &[DVec3], tol: f64) -> Option<(DVec3, bool, f64)> {
  if run.len() + 1 < MIN_POINTS || run.iter().any(|p| (p.z - start.z).abs() > 1e-9) {
    return None;
  }
  let flat = |p: DVec3| DVec2::new(p.x, p.y);
  // three points well apart (a closed circle ends where it starts)
  let a = flat(start);
  let c = circumcentre(a, flat(run[run.len() / 3]), flat(run[2 * run.len() / 3]))?;
  let r = (a - c).length();
  if !(r > tol * 10.0 && r < MAX_RADIUS) {
    return None;
  }
  // every point on the circle (the polyline's chords sag inside the circle it approximates; the
  // arc follows that circle), in small steps one way (a coarse polygon is not a circle)
  let mut prev = a;
  let mut turn = 0.0;
  let mut sign = 0.0;
  for p in run.iter().map(|p| flat(*p)) {
    if ((p - c).length() - r).abs() > tol {
      return None;
    }
    let step = (prev - c).perp_dot(p - c).atan2((prev - c).dot(p - c));
    let sag = r * (1.0 - (step / 2.0).cos());
    if step.abs() < 1e-12 || (sign != 0.0 && step.signum() != sign) || step.abs() > 0.5 || sag > MAX_SAG {
      return None;
    }
    sign = step.signum();
    turn += step;
    prev = p;
  }
  if turn.abs() > std::f64::consts::TAU + 1e-9 {
    return None;
  }
  Some((DVec3::new(c.x, c.y, start.z), turn < 0.0, turn.abs()))
}

fn circumcentre(a: DVec2, b: DVec2, c: DVec2) -> Option<DVec2> {
  let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
  if d.abs() < 1e-12 {
    return None;
  }
  let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
  Some(DVec2::new(
    (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
    (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
  ))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::f64::consts::TAU;

  fn circle(c: DVec3, r: f64, n: usize, from: f64, turn: f64) -> Vec<DVec3> {
    (0..=n)
      .map(|k| {
        c + DVec3::new(
          r * (from + turn * k as f64 / n as f64).cos(),
          r * (from + turn * k as f64 / n as f64).sin(),
          0.0,
        )
      })
      .collect()
  }

  #[test]
  fn a_polygonal_circle_becomes_two_half_arcs() {
    let pts = circle(DVec3::new(10.0, 5.0, -2.0), 8.0, 72, 0.0, TAU);
    let segs = fit(pts[0], &pts[1..], 0.002);
    assert_eq!(segs.len(), 2, "{segs:?}");
    for s in &segs {
      let Seg::Arc { center, cw, .. } = s else { panic!("{s:?}") };
      assert!((*center - DVec3::new(10.0, 5.0, -2.0)).length() < 1e-9);
      assert!(!cw, "counter-clockwise points make G3");
    }
    assert!(matches!(segs[1], Seg::Arc { to, .. } if to == pts[72]), "ends where the circle closes");
  }

  #[test]
  fn lines_stay_lines_and_corners_split_arcs() {
    // a square: no arcs
    let sq = [DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 10.0, 0.0), DVec3::new(0.0, 10.0, 0.0), DVec3::ZERO];
    assert!(fit(DVec3::ZERO, &sq, 0.002).iter().all(|s| matches!(s, Seg::Line(_))));
    // a clockwise quarter round, then a straight run: one G2 and lines
    let mut pts = circle(DVec3::ZERO, 5.0, 12, TAU / 4.0, -TAU / 4.0);
    pts.extend([DVec3::new(5.0, -10.0, 0.0), DVec3::new(5.0, -20.0, 0.0)]);
    let segs = fit(pts[0], &pts[1..], 0.002);
    assert!(matches!(segs[0], Seg::Arc { cw: true, .. }), "{segs:?}");
    assert_eq!(segs.iter().filter(|s| matches!(s, Seg::Arc { .. })).count(), 1);
    assert_eq!(segs.last(), Some(&Seg::Line(DVec3::new(5.0, -20.0, 0.0))));
  }

  #[test]
  fn ramps_and_wobbles_stay_lines() {
    // a helix (Z changes) is not an XY arc
    let mut helix = circle(DVec3::ZERO, 5.0, 24, 0.0, TAU / 2.0);
    for (k, p) in helix.iter_mut().enumerate() {
      p.z = -0.1 * k as f64;
    }
    assert!(fit(helix[0], &helix[1..], 0.002).iter().all(|s| matches!(s, Seg::Line(_))));
    // points off the circle by more than the tolerance
    let mut rough = circle(DVec3::ZERO, 5.0, 24, 0.0, TAU / 2.0);
    rough[6].x += 0.05;
    let segs = fit(rough[0], &rough[1..], 0.002);
    let on = |p: DVec3| segs.iter().any(|s| matches!(s, Seg::Line(q) | Seg::Arc { to: q, .. } if *q == p));
    assert!(on(rough[6]), "the off point is still visited");
  }

  #[test]
  fn polygons_stay_polygons() {
    // a 13-sided pocket outline (each side sags 0.6 mm from its circle) is not a circle
    let pts = circle(DVec3::ZERO, 20.0, 13, 0.0, TAU);
    assert!(fit(pts[0], &pts[1..], 0.002).iter().all(|s| matches!(s, Seg::Line(_))));
  }
}
