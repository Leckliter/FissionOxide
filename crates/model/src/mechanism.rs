//! Cam motion: where a follower sliding along a line touches a cam.
//!
//! The cam is given as triangles (its bodies' tessellation, in world coordinates); the
//! follower's tip moves along `tip + s · dir` as its joint slides by `s`. The follower rests on
//! the side of the cam it starts on (where its tip is at zero slide), so a follower above a cam
//! rides on top of it and one below hangs under it.

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
  let (lo, hi) = tris.iter().flatten().map(|p| (*p - tip).dot(d)).fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t), b.max(t)));
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
}
