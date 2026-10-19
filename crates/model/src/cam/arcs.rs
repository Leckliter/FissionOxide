//! Arc fitting for G-code: runs of cutting moves that lie on a circle in the XY plane (G17)
//! become one G2/G3 arc, helical when Z changes evenly along it. Toolpaths stay polylines; only
//! their output is fitted, so the simulation and the viewport see what they always did.
//!
//! An arc replaces a run of moves when every point of the run is within the tolerance of it and
//! no chord bows away from it by more than twice the tolerance. Arcs start and end on the
//! run's own end points, so controls see the same radius at both ends.

use fox_kernel::glam::{DVec2, DVec3};

use super::Move;

/// Arc tolerance when a setup doesn't set one (mm).
pub const DEFAULT_TOLERANCE: f64 = 0.01;
/// Largest radius written as an arc (mm): flatter runs stay straight moves.
const MAX_RADIUS: f64 = 1000.0;
/// Fewest moves an arc replaces.
const MIN_MOVES: usize = 3;
/// Largest turn of one move inside an arc, so coarse polygons aren't rounded off.
const MAX_STEP: f64 = 20.0 * std::f64::consts::PI / 180.0;
/// Largest sweep of one arc (just under a full turn, where start and end would meet).
const MAX_SWEEP: f64 = 350.0 * std::f64::consts::PI / 180.0;
/// Most moves one arc may replace (keeps fitting linear-ish on long runs).
const MAX_RUN: usize = 4000;

/// A piece of G-code output: a move as it was, or an arc replacing several cutting moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
  Move(Move),
  Arc(Arc),
}

/// A circular (or helical) cutting move in the XY plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
  /// Where it starts (the tool's position before it).
  pub from: DVec3,
  pub to: DVec3,
  /// Centre in XY (world coordinates).
  pub centre: DVec2,
  /// Counter-clockwise seen from above (G3); clockwise is G2.
  pub ccw: bool,
  pub feed: f64,
  /// How many moves it replaces.
  pub moves: usize,
}

impl Arc {
  pub fn radius(&self) -> f64 {
    (self.from.truncate() - self.centre).length()
  }

  /// Sweep angle (radians, positive).
  pub fn sweep(&self) -> f64 {
    let a = angle(self.from.truncate() - self.centre);
    let b = angle(self.to.truncate() - self.centre);
    let d = if self.ccw { b - a } else { a - b };
    d.rem_euclid(std::f64::consts::TAU)
  }

  /// Points along the arc no more than `step` radians apart, without its start.
  pub fn points(&self, step: f64) -> Vec<DVec3> {
    let sweep = self.sweep();
    let n = ((sweep / step.max(1e-3)).ceil() as usize).max(1);
    let a0 = angle(self.from.truncate() - self.centre);
    let r = self.radius();
    let dir = if self.ccw { 1.0 } else { -1.0 };
    (1..=n)
      .map(|k| {
        let t = k as f64 / n as f64;
        if k == n {
          return self.to;
        }
        let a = a0 + dir * sweep * t;
        let z = self.from.z + (self.to.z - self.from.z) * t;
        DVec3::new(self.centre.x + r * a.cos(), self.centre.y + r * a.sin(), z)
      })
      .collect()
  }
}

/// The moves of a toolpath with runs on circles replaced by arcs within `tol` (mm). `start`
/// is where the tool is before the first move, if known.
pub fn fit(moves: &[Move], start: Option<DVec3>, tol: f64) -> Vec<Segment> {
  let tol = if tol > 0.0 { tol } else { DEFAULT_TOLERANCE };
  let mut out = Vec::with_capacity(moves.len());
  let mut at = start;
  let mut i = 0;
  while i < moves.len() {
    let m = moves[i];
    if let Some(from) = at.filter(|_| !m.rapid) {
      // the longest run of cutting moves from here that fits an arc
      let mut best = None;
      let mut j = i + MIN_MOVES - 1;
      while j < moves.len() && j - i < MAX_RUN {
        if moves[j].rapid || (moves[j].feed - m.feed).abs() > 1e-9 {
          break;
        }
        match arc_through(from, &moves[i..=j], tol) {
          Some(a) => best = Some(a),
          None => break,
        }
        j += 1;
      }
      if let Some(a) = best.filter(|a| !straight(from, &moves[i..i + a.moves], tol)) {
        i += a.moves;
        at = Some(a.to);
        out.push(Segment::Arc(a));
        continue;
      }
    }
    out.push(Segment::Move(m));
    at = Some(m.to);
    i += 1;
  }
  out
}

/// Whether every point of a run is within `tol` of the line from its start to its end (then it
/// is better left to straight moves).
fn straight(from: DVec3, run: &[Move], tol: f64) -> bool {
  let to = run[run.len() - 1].to;
  run.iter().all(|m| super::region::seg_distance(m.to.truncate(), from.truncate(), to.truncate()) <= tol)
}

/// The arc from `from` through the run's points, if they all fit it.
fn arc_through(from: DVec3, run: &[Move], tol: f64) -> Option<Arc> {
  let n = run.len();
  let to = run[n - 1].to;
  let mid = run[(n - 1) / 2].to;
  let (a, b, c) = (from.truncate(), mid.truncate(), to.truncate());
  let centre = circle(a, b, c)?;
  let r = (a - centre).length();
  if !(r > 1e-4 && r <= MAX_RADIUS) {
    return None;
  }
  let ccw = (b - a).perp_dot(c - b) > 0.0;
  let dir = if ccw { 1.0 } else { -1.0 };
  // walk the points around the centre: always the same way, in small steps
  let mut angles = Vec::with_capacity(n);
  let mut prev = angle(a - centre);
  let mut total = 0.0;
  let mut last = a;
  for m in run {
    let p = m.to.truncate();
    if ((p - centre).length() - r).abs() > tol {
      return None;
    }
    let ang = angle(p - centre);
    let step = dir * wrap(ang - prev);
    if !(-1e-9..=MAX_STEP).contains(&step) {
      return None;
    }
    // the chord bows in from the arc by its sagitta
    let chord = (p - last).length();
    if r - (r * r - chord * chord / 4.0).max(0.0).sqrt() > 2.0 * tol {
      return None;
    }
    total += step.max(0.0);
    angles.push(total);
    prev = ang;
    last = p;
  }
  if total > MAX_SWEEP || total < 1e-6 {
    return None;
  }
  // Z evenly along the sweep (flat or helical)
  for (m, t) in run.iter().zip(&angles) {
    let z = from.z + (to.z - from.z) * (t / total);
    if (m.to.z - z).abs() > tol {
      return None;
    }
  }
  Some(Arc { from, to, centre, ccw, feed: run[0].feed, moves: n })
}

/// The direction of a vector (radians from +X).
fn angle(v: DVec2) -> f64 {
  v.y.atan2(v.x)
}

/// An angle wrapped into (−π, π].
fn wrap(a: f64) -> f64 {
  let t = std::f64::consts::TAU;
  let a = a.rem_euclid(t);
  if a > std::f64::consts::PI { a - t } else { a }
}

/// The centre of the circle through three points (None when they are in a line).
fn circle(a: DVec2, b: DVec2, c: DVec2) -> Option<DVec2> {
  let (ab, ac) = (b - a, c - a);
  let d = 2.0 * ab.perp_dot(ac);
  let scale = ab.length_squared().max(ac.length_squared()).max(1e-300);
  if d.abs() < 1e-12 * scale {
    return None;
  }
  let (b2, c2) = (ab.length_squared(), ac.length_squared());
  Some(a + DVec2::new(ac.y * b2 - ab.y * c2, ab.x * c2 - ac.x * b2) / d)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn cut(p: DVec3) -> Move {
    Move { rapid: false, to: p, feed: 800.0 }
  }

  /// Points on a circle (radius `r` round `c`) from angle `a0` to `a1` (degrees) in `n` steps.
  fn ring(c: DVec2, r: f64, a0: f64, a1: f64, n: usize, z0: f64, z1: f64) -> Vec<DVec3> {
    (0..=n)
      .map(|k| {
        let t = k as f64 / n as f64;
        let a = (a0 + (a1 - a0) * t).to_radians();
        DVec3::new(c.x + r * a.cos(), c.y + r * a.sin(), z0 + (z1 - z0) * t)
      })
      .collect()
  }

  #[test]
  fn circles_become_arcs_and_lines_stay() {
    // a straight lead-in, a quarter circle counter-clockwise, a straight lead-out
    let c = DVec2::new(10.0, 5.0);
    let q = ring(c, 4.0, 0.0, 90.0, 18, -2.0, -2.0);
    let mut moves = vec![cut(DVec3::new(14.0, -3.0, -2.0)), cut(DVec3::new(14.0, 5.0, -2.0))];
    moves.extend(q[1..].iter().map(|p| cut(*p)));
    moves.push(cut(DVec3::new(0.0, 9.0, -2.0)));
    let segs = fit(&moves, Some(DVec3::new(14.0, -10.0, -2.0)), 0.01);
    let arcs: Vec<&Arc> = segs
      .iter()
      .filter_map(|s| match s {
        Segment::Arc(a) => Some(a),
        _ => None,
      })
      .collect();
    assert_eq!(arcs.len(), 1, "{segs:?}");
    let a = arcs[0];
    assert!(a.ccw && (a.centre - c).length() < 1e-9 && (a.radius() - 4.0).abs() < 1e-9);
    assert!((a.sweep() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    assert_eq!(a.moves, 18);
    assert_eq!(segs.len(), 2 + 1 + 1);
    // clockwise
    let q = ring(c, 4.0, 90.0, -45.0, 30, 0.0, 0.0);
    let moves: Vec<Move> = q[1..].iter().map(|p| cut(*p)).collect();
    let segs = fit(&moves, Some(q[0]), 0.01);
    assert!(matches!(segs[..], [Segment::Arc(Arc { ccw: false, moves: 30, .. })]), "{segs:?}");
  }

  #[test]
  fn helices_noise_and_polygons() {
    // a helix of two turns: arcs of up to 350° each, Z falling evenly
    let c = DVec2::new(0.0, 0.0);
    let q = ring(c, 3.0, 0.0, 720.0, 144, 0.0, -4.0);
    let moves: Vec<Move> = q[1..].iter().map(|p| cut(*p)).collect();
    let segs = fit(&moves, Some(q[0]), 0.01);
    assert!(segs.len() <= 4 && segs.iter().all(|s| matches!(s, Segment::Arc(_))), "{segs:?}");
    // they replay the helix
    let mut at = q[0];
    let mut k = 0;
    for s in &segs {
      let Segment::Arc(a) = s else { unreachable!() };
      assert_eq!(a.from, at);
      for p in a.points(1f64.to_radians()) {
        let r = p.truncate().length();
        assert!((r - 3.0).abs() < 1e-9);
        let ang = p.y.atan2(p.x);
        // Z goes down 2 per turn
        let turn = (-p.z / 2.0) * std::f64::consts::TAU;
        assert!(wrap(turn - ang).abs() < 1e-6, "{p:?}");
      }
      k += a.moves;
      at = a.to;
    }
    assert_eq!(k, 144);
    // noise beyond the tolerance keeps the moves
    let mut q = ring(c, 10.0, 0.0, 60.0, 12, 0.0, 0.0);
    q[5].x += 0.05;
    let moves: Vec<Move> = q[1..].iter().map(|p| cut(*p)).collect();
    let segs = fit(&moves, Some(q[0]), 0.01);
    // the noisy point is never inside an arc, and every point an arc replaces is on it
    let mut k = 0;
    for s in &segs {
      match s {
        Segment::Move(_) => k += 1,
        Segment::Arc(a) => {
          assert!(!(k..k + a.moves - 1).contains(&4), "{segs:?}");
          for m in &moves[k..k + a.moves] {
            assert!(((m.to.truncate() - a.centre).length() - a.radius()).abs() <= 0.01);
          }
          k += a.moves;
        }
      }
    }
    assert_eq!(k, moves.len());
    // a square's corners lie on a circle but stay corners
    let sq = [DVec3::new(1.0, 0.0, 0.0), DVec3::new(1.0, 1.0, 0.0), DVec3::new(0.0, 1.0, 0.0), DVec3::ZERO];
    let segs = fit(&sq.map(cut), Some(DVec3::ZERO), 0.01);
    assert!(segs.iter().all(|s| matches!(s, Segment::Move(_))));
    // collinear runs and rapids stay as they are
    let line: Vec<Move> = (1..10).map(|k| cut(DVec3::new(k as f64, 0.0, 0.0))).collect();
    assert!(fit(&line, Some(DVec3::ZERO), 0.01).iter().all(|s| matches!(s, Segment::Move(_))));
    let q = ring(c, 3.0, 0.0, 90.0, 10, 0.0, 0.0);
    let mut moves: Vec<Move> = q[1..].iter().map(|p| cut(*p)).collect();
    moves[4].rapid = true;
    let segs = fit(&moves, Some(q[0]), 0.01);
    assert!(segs.iter().any(|s| matches!(s, Segment::Move(m) if m.rapid)));
    // a feed change splits the run
    let mut moves: Vec<Move> = q[1..].iter().map(|p| cut(*p)).collect();
    for m in &mut moves[5..] {
      m.feed = 400.0;
    }
    let segs = fit(&moves, Some(q[0]), 0.01);
    assert_eq!(segs.iter().filter(|s| matches!(s, Segment::Arc(_))).count(), 2, "{segs:?}");
  }
}
