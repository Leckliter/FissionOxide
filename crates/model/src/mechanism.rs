//! Cam motion: where a follower moving along a line or a curve touches a cam.
//!
//! The cam is given as triangles (its bodies' tessellation, in world coordinates); the
//! follower's tip moves along `tip + s · dir` as its joint slides by `s`, or along any path
//! (a rocker arm's arc as its joint turns). The follower rests on the side of the cam it starts
//! on (where its tip is at zero motion), so a follower above a cam rides on top of it and one
//! below hangs under it.
//!
//! A knife edge is a point, a flat follower a face square to the slide, and a roller a ball or
//! a cylinder of some radius round the tip: a cylinder (its axis and width) touches the cam
//! where a disc swept across that width would, so a roller on a plate cam lies on its profile.

use fox_kernel::glam::DVec3;

/// The slide that puts a follower in contact with a cam, `clearance` short of touching.
///
/// A knife-edge follower touches where its slide line enters the cam; a flat-faced one (a face
/// square to the slide) touches the cam's furthest point along the slide. Errors when the
/// follower's line misses the cam (knife edge) or the cam has no triangles.
pub fn cam_contact(tris: &[[DVec3; 3]], tip: DVec3, dir: DVec3, flat: bool, clearance: f64) -> Result<f64, String> {
  let len = dir.length();
  if len < 1e-12 {
    return Err("the follower's joint doesn't slide".into());
  }
  let d = dir / len;
  if tris.is_empty() {
    return Err("the cam has no bodies".into());
  }
  // which side of the cam the follower is on: compare with the cam's middle along the slide
  let (lo, hi) =
    tris.iter().flatten().map(|p| (*p - tip).dot(d)).fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t), b.max(t)));
  let above = lo + hi < 0.0;
  let t = if flat {
    if above { hi } else { lo }
  } else {
    let hits: Vec<f64> = tris.iter().filter_map(|t| ray_hit(tip, d, t)).collect();
    if hits.is_empty() {
      return Err("the follower's line misses the cam".into());
    }
    let pick = |a: f64, b: f64| if above { a.max(b) } else { a.min(b) };
    hits.into_iter().reduce(pick).unwrap_or(0.0)
  };
  let t = if above { t + clearance } else { t - clearance };
  Ok(t / len)
}

/// Where the line `o + t·d` crosses a triangle (any t), Möller–Trumbore.
fn ray_hit(o: DVec3, d: DVec3, [a, b, c]: &[DVec3; 3]) -> Option<f64> {
  let (e1, e2) = (*b - *a, *c - *a);
  let p = d.cross(e2);
  let det = e1.dot(p);
  if det.abs() < 1e-14 {
    return None;
  }
  let inv = 1.0 / det;
  let s = o - *a;
  let u = s.dot(p) * inv;
  if !(-1e-9..=1.0 + 1e-9).contains(&u) {
    return None;
  }
  let q = s.cross(e1);
  let v = d.dot(q) * inv;
  if v < -1e-9 || u + v > 1.0 + 1e-9 {
    return None;
  }
  Some(e2.dot(q) * inv)
}

/// A roller on the follower's tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Roller {
  pub radius: f64,
  /// A cylindrical roller's axis (unit) and half its width; `None` = a ball.
  pub axis: Option<(DVec3, f64)>,
}

/// The slide that puts a roller centred at `centre` (moving along `centre + s · dir`) in
/// contact with a cam, `clearance` short of touching (round the roller). A cylindrical roller
/// whose axis isn't square to the slide touches as a ball.
pub fn roller_contact(
  tris: &[[DVec3; 3]],
  centre: DVec3,
  dir: DVec3,
  roller: Roller,
  clearance: f64,
) -> Result<f64, String> {
  let len = dir.length();
  if len < 1e-12 {
    return Err("the follower's joint doesn't slide".into());
  }
  let d = dir / len;
  if tris.is_empty() {
    return Err("the cam has no bodies".into());
  }
  let r = roller.radius + clearance;
  if r <= 0.0 {
    return Err("the roller's radius must be above zero".into());
  }
  let (lo, hi) =
    tris.iter().flatten().map(|p| (*p - centre).dot(d)).fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t), b.max(t)));
  let above = lo + hi < 0.0;
  let probe = Probe::new(tris, roller.axis.filter(|(a, _)| a.dot(d).abs() < 1e-6), centre);
  let c = probe.flat(centre);
  let hits: Vec<f64> = probe.tris.iter().flat_map(|t| ball_events(c, d, t, r)).collect();
  let t = if above { hits.iter().copied().reduce(f64::max) } else { hits.iter().copied().reduce(f64::min) };
  Ok(t.ok_or("the follower's line misses the cam")? / len)
}

/// The motion that brings a follower's tip moving along `path` (motion → tip centre and, for
/// a cylindrical roller, its axis) into contact with a cam, the tip `clearance` short of it.
///
/// The motion is looked for within half a turn either side of where the tip comes nearest the
/// cam's middle, approaching from the side the tip is on at zero motion, as a straight
/// follower does; the path is followed in quarter-degree steps and the touch bisected.
pub fn path_contact(
  tris: &[[DVec3; 3]],
  path: &dyn Fn(f64) -> (DVec3, DVec3),
  roller: Option<Roller>,
  clearance: f64,
) -> Result<f64, String> {
  use std::f64::consts::PI;
  if tris.is_empty() {
    return Err("the cam has no bodies".into());
  }
  let r = roller.map_or(0.0, |r| r.radius) + clearance;
  let (c0, a0) = path(0.0);
  // a cylinder keeps its contact plane only if the path keeps its axis and plane
  let cylinder = roller.and_then(|r| r.axis).filter(|(a, _)| {
    (1..8).all(|k| {
      let (c, ax) = path(PI * k as f64 / 4.0 - PI);
      ax.cross(a0).length() < 1e-6 && (c - c0).dot(*a).abs() < 1e-6
    })
  });
  let cylinder = cylinder.map(|(_, w)| (a0.normalize(), w));
  let probe = Probe::new(tris, cylinder, c0);
  let at = |s: f64| probe.flat(path(s).0);
  let n = probe.tris.len().max(1) as f64;
  let middle = probe.tris.iter().flatten().fold(DVec3::ZERO, |a, p| a + *p) / (3.0 * n);
  let near = (0..360)
    .map(|k| -PI + 2.0 * PI * k as f64 / 360.0)
    .min_by(|a, b| (at(*a) - middle).length_squared().total_cmp(&(at(*b) - middle).length_squared()))
    .unwrap_or(0.0);
  let side = if near <= 0.0 { 1.0 } else { -1.0 };
  let far = near + side * PI;
  let bounds = probe.tris.iter().map(|t| bounds(t, r)).collect::<Vec<_>>();
  let first = |p: DVec3, q: DVec3| -> Option<f64> {
    let (lo, hi) = (p.min(q), p.max(q));
    let d = q - p;
    probe
      .tris
      .iter()
      .zip(&bounds)
      .filter(|(_, (a, b))| a.cmple(hi).all() && b.cmpge(lo).all())
      .flat_map(|(t, _)| if r > 0.0 { ball_events(p, d, t, r) } else { ray_hit(p, d, t).into_iter().collect() })
      .filter(|u| (-1e-12..=1.0 + 1e-12).contains(u))
      .reduce(f64::min)
  };
  const STEPS: usize = 720;
  for k in 0..STEPS {
    let s0 = far - side * PI * k as f64 / STEPS as f64;
    let s1 = far - side * PI * (k + 1) as f64 / STEPS as f64;
    let p = at(s0);
    if first(p, at(s1)).is_none() {
      continue;
    }
    // bisect along the path itself (not its chord)
    let (mut a, mut b) = (0.0, 1.0);
    for _ in 0..30 {
      let m = (a + b) / 2.0;
      if first(p, at(s0 + (s1 - s0) * m)).is_some() {
        b = m;
      } else {
        a = m;
      }
    }
    return Ok(s0 + (s1 - s0) * b);
  }
  Err("the follower's path misses the cam".into())
}

/// The cam's triangles as a follower's tip sees them: as they are for a ball or a knife edge;
/// for a cylindrical roller, clipped to the roller's width and flattened onto its middle plane.
struct Probe {
  tris: Vec<[DVec3; 3]>,
  plane: Option<(DVec3, DVec3)>,
}

impl Probe {
  fn new(tris: &[[DVec3; 3]], cylinder: Option<(DVec3, f64)>, centre: DVec3) -> Probe {
    let Some((a, w)) = cylinder else { return Probe { tris: tris.to_vec(), plane: None } };
    let flat = |p: DVec3| p - a * (p - centre).dot(a);
    let mut out = Vec::new();
    for t in tris {
      let poly = clip(&clip(t, a, centre.dot(a) + w), -a, -centre.dot(a) + w);
      for k in 1..poly.len().saturating_sub(1) {
        out.push([flat(poly[0]), flat(poly[k]), flat(poly[k + 1])]);
      }
    }
    Probe { tris: out, plane: Some((a, centre)) }
  }

  /// A tip centre on the probe's plane (as it is without one).
  fn flat(&self, p: DVec3) -> DVec3 {
    match self.plane {
      Some((a, c)) => p - a * (p - c).dot(a),
      None => p,
    }
  }
}

/// The part of a triangle where `p · n <= k`.
fn clip(poly: &[DVec3], n: DVec3, k: f64) -> Vec<DVec3> {
  let mut out = Vec::new();
  for (i, p) in poly.iter().enumerate() {
    let q = poly[(i + 1) % poly.len()];
    let (dp, dq) = (p.dot(n) - k, q.dot(n) - k);
    if dp <= 0.0 {
      out.push(*p);
    }
    if (dp < 0.0) != (dq < 0.0) && (dp - dq).abs() > 1e-15 {
      out.push(*p + (q - *p) * (dp / (dp - dq)));
    }
  }
  out
}

fn bounds(t: &[DVec3; 3], r: f64) -> (DVec3, DVec3) {
  let lo = t[0].min(t[1]).min(t[2]) - DVec3::splat(r + 1e-9);
  let hi = t[0].max(t[1]).max(t[2]) + DVec3::splat(r + 1e-9);
  (lo, hi)
}

/// Where a ball of radius `r` moving along `c + t·d` (any t) starts or stops touching a
/// triangle: its face (either side), its edges and its corners.
fn ball_events(c: DVec3, d: DVec3, [a, b, k]: &[DVec3; 3], r: f64) -> Vec<f64> {
  let mut out = Vec::new();
  let n = (*b - *a).cross(*k - *a);
  if let Some(n) = n.try_normalize() {
    let dn = d.dot(n);
    if dn.abs() > 1e-14 {
      for side in [-1.0, 1.0] {
        let t = (side * r - (c - *a).dot(n)) / dn;
        if inside(c + d * t - n * side * r, [*a, *b, *k], n) {
          out.push(t);
        }
      }
    }
  }
  for (p, q) in [(*a, *b), (*b, *k), (*k, *a)] {
    let e = q - p;
    let el = e.length();
    if el < 1e-14 {
      continue;
    }
    let u = e / el;
    let (w, dd) = (c - p, d);
    let (wp, dp) = (w - u * w.dot(u), dd - u * dd.dot(u));
    for t in roots(dp.dot(dp), 2.0 * wp.dot(dp), wp.dot(wp) - r * r) {
      let along = (c + d * t - p).dot(u);
      if (0.0..=el).contains(&along) {
        out.push(t);
      }
    }
  }
  for v in [*a, *b, *k] {
    let w = c - v;
    out.extend(roots(d.dot(d), 2.0 * w.dot(d), w.dot(w) - r * r));
  }
  out
}

/// Is `p` (on the triangle's plane) inside it?
fn inside(p: DVec3, [a, b, c]: [DVec3; 3], n: DVec3) -> bool {
  let e = -1e-12;
  (b - a).cross(p - a).dot(n) >= e && (c - b).cross(p - b).dot(n) >= e && (a - c).cross(p - c).dot(n) >= e
}

/// Real roots of `a t² + b t + c`.
fn roots(a: f64, b: f64, c: f64) -> Vec<f64> {
  if a.abs() < 1e-18 {
    return Vec::new();
  }
  let disc = b * b - 4.0 * a * c;
  if disc < 0.0 {
    return Vec::new();
  }
  let q = disc.sqrt();
  vec![(-b - q) / (2.0 * a), (-b + q) / (2.0 * a)]
}

/// Triangles of a tessellation, in f64.
pub fn triangles(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[DVec3; 3]> {
  let at = |i: u32| positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
  indices.chunks_exact(3).filter_map(|t| Some([at(t[0])?, at(t[1])?, at(t[2])?])).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A unit-ish cube [−5, 5]³ as triangles.
  fn cube() -> Vec<[DVec3; 3]> {
    let v = |x: f64, y: f64, z: f64| DVec3::new(x, y, z) * 5.0;
    let quad = |a: DVec3, b: DVec3, c: DVec3, d: DVec3| [[a, b, c], [a, c, d]];
    let mut out = Vec::new();
    for s in [-1.0, 1.0] {
      out.extend(quad(v(-1.0, -1.0, s), v(1.0, -1.0, s), v(1.0, 1.0, s), v(-1.0, 1.0, s)));
      out.extend(quad(v(-1.0, s, -1.0), v(1.0, s, -1.0), v(1.0, s, 1.0), v(-1.0, s, 1.0)));
      out.extend(quad(v(s, -1.0, -1.0), v(s, 1.0, -1.0), v(s, 1.0, 1.0), v(s, -1.0, 1.0)));
    }
    out
  }

  #[test]
  fn followers_rest_on_the_side_they_start() {
    let tris = cube();
    // from above, sliding along +Z: touches the top face at z = 5
    let s = cam_contact(&tris, DVec3::new(1.0, 2.0, 20.0), DVec3::Z, false, 0.0).unwrap();
    assert!((s - -15.0).abs() < 1e-9, "{s}");
    // from below with the slide pointing down (−Z per unit): touches the bottom at z = −5
    let s = cam_contact(&tris, DVec3::new(0.0, 0.0, -20.0), -DVec3::Z, false, 1.0).unwrap();
    assert!((s - -14.0).abs() < 1e-9, "{s}");
    // flat follower off to the side still rests on the cube's top
    let s = cam_contact(&tris, DVec3::new(30.0, 0.0, 20.0), DVec3::Z * 2.0, true, 0.0).unwrap();
    assert!((s - -7.5).abs() < 1e-9, "{s}");
    // a knife edge off to the side misses
    assert!(cam_contact(&tris, DVec3::new(30.0, 0.0, 20.0), DVec3::Z, false, 0.0).is_err());
  }

  #[test]
  fn rollers_touch_round_their_radius() {
    let tris = cube();
    // a ball above the cube's edge (x = 5), sliding down: it touches the edge, not the face
    let ball = Roller { radius: 2.0, axis: None };
    let s = roller_contact(&tris, DVec3::new(6.0, 0.0, 20.0), -DVec3::Z, ball, 0.0).unwrap();
    let z = 20.0 - s;
    assert!((z - (5.0 + (4.0_f64 - 1.0).sqrt())).abs() < 1e-9, "{z}");
    // straight above the face it lies on it (plus the clearance)
    let s = roller_contact(&tris, DVec3::new(0.0, 0.0, 20.0), -DVec3::Z, ball, 0.5).unwrap();
    assert!((20.0 - s - 7.5).abs() < 1e-9, "{s}");
    // a cylinder across the cube (axis Y) as wide as it touches the top face too; one off its
    // end misses
    let roller = Roller { radius: 2.0, axis: Some((DVec3::Y, 1.0)) };
    let s = roller_contact(&tris, DVec3::new(0.0, 3.0, 20.0), -DVec3::Z, roller, 0.0).unwrap();
    assert!((20.0 - s - 7.0).abs() < 1e-9, "{s}");
    assert!(roller_contact(&tris, DVec3::new(0.0, 7.0, 20.0), -DVec3::Z, roller, 0.0).is_err());
  }

  #[test]
  fn curved_paths_touch_from_their_side() {
    let tris = cube();
    // a tip swinging about (20, 0, 0) at radius 18, starting at a = 0.6 (above the cube): it
    // comes down onto the top face y = 5 where 18 sin a = 5
    let path = |a: f64| {
      let a = a + 0.6;
      (DVec3::new(20.0 - 18.0 * a.cos(), 18.0 * a.sin(), 0.0), DVec3::Z)
    };
    let s = path_contact(&tris, &path, None, 0.0).unwrap();
    assert!((18.0 * (s + 0.6).sin() - 5.0).abs() < 1e-6, "{s}");
    // a roller on it keeps its radius off the face
    let s = path_contact(&tris, &path, Some(Roller { radius: 1.0, axis: Some((DVec3::Z, 0.5)) }), 0.0).unwrap();
    assert!((18.0 * (s + 0.6).sin() - 6.0).abs() < 1e-6, "{s}");
    // a path that never comes near misses
    let far = |a: f64| (DVec3::new(100.0 + a.cos(), a.sin(), 0.0), DVec3::Z);
    assert!(path_contact(&tris, &far, None, 0.0).is_err());
  }
}
