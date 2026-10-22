//! Silhouette Split: a body cut where its outline lies, seen along a direction (Fusion's SOLID >
//! MODIFY > Silhouette Split). The outline is found on the body's mesh: the points that project
//! onto the boundary of its shadow, whose middle height along the direction places the cutting
//! plane. The feature it adds is a Split Body (or Split Face) on that plane.

use fox_kernel::glam::DVec3;
use fox_model::BodyId;
use serde_json::{Value, json};

use crate::commands::req_u32;
use crate::{CmdError, CmdResult, Session};

/// `silhouette_split {direction, body, operation?}`: `shelled` (Split Shelled Body, the default)
/// and `solid` split the body in two; `face` splits its faces and keeps it one body.
pub(crate) fn split(s: &mut Session, p: &Value) -> CmdResult {
  let dir = direction(s, p.get("direction").ok_or(CmdError::new("missing 'direction' (a plane or an axis)"))?)?;
  let body = BodyId(req_u32(p, "body")?);
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let mesh = b.mesh().ok_or(CmdError::new("the body has no mesh"))?;
  let pts: Vec<DVec3> = mesh.positions.iter().map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64)).collect();
  let h = silhouette_height(&pts, dir, b.diagonal()).ok_or(CmdError::new("couldn't find the body's silhouette"))?;
  let plane = json!({"origin": (dir * h).to_array(), "normal": dir.to_array()});
  match p.get("operation").and_then(Value::as_str).unwrap_or("shelled") {
    "shelled" | "solid" => s.execute("split", &json!({"plane": plane, "bodies": [body.0]})),
    "face" => {
      // the faces the plane crosses (their mesh triangles lie on both sides of it)
      let tol = b.diagonal() * 1e-6;
      let mut range: std::collections::BTreeMap<u32, (f64, f64)> = Default::default();
      for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let Some(&face) = mesh.tri_face.get(t) else { continue };
        let r = range.entry(face).or_insert((f64::MAX, f64::MIN));
        for &i in tri {
          let z = pts.get(i as usize).map_or(h, |q| q.dot(dir));
          *r = (r.0.min(z), r.1.max(z));
        }
      }
      let faces: Vec<Value> = range
        .into_iter()
        .filter(|(_, (lo, hi))| *lo < h - tol && *hi > h + tol)
        .map(|(face, _)| json!({"body": body.0, "face": face}))
        .collect();
      if faces.is_empty() {
        return Err(CmdError::new("the silhouette crosses no face"));
      }
      s.execute("split_face", &json!({"faces": faces, "tool": plane}))
    }
    o => Err(CmdError::new(format!("unknown operation '{o}' (shelled, solid or face)"))),
  }
}

/// The pull direction a plane (its normal) or an axis (origin axis, construction axis or
/// straight edge) names, as a unit vector.
fn direction(s: &Session, v: &Value) -> Result<DVec3, CmdError> {
  let unit = |d: DVec3| d.try_normalize().ok_or(CmdError::new("the direction is zero"));
  if let Some(n) = v.as_str() {
    match n.to_ascii_lowercase().as_str() {
      "x" => return Ok(DVec3::X),
      "y" => return Ok(DVec3::Y),
      "z" => return Ok(DVec3::Z),
      _ => {}
    }
  }
  if let Some(a) = v.get("axis").and_then(Value::as_u64) {
    let (_, d) = s
      .model
      .axes
      .get(&fox_model::FeatureId(a as u32))
      .copied()
      .ok_or(CmdError::new("that feature is not a construction axis"))?;
    return unit(d);
  }
  if let (Some(b), Some(e)) = (v.get("body").and_then(Value::as_u64), v.get("edge").and_then(Value::as_u64)) {
    let body = s.model.body(BodyId(b as u32)).ok_or(CmdError::new(format!("no body {b}")))?;
    let edge = body.edges().iter().find(|x| x.id as u64 == e).ok_or(CmdError::new("no such edge"))?;
    if edge.kind != fox_kernel::CurveKind::Line {
      return Err(CmdError::new("the direction edge isn't straight"));
    }
    return unit(edge.end - edge.start);
  }
  let plane = crate::scene::view_plane(s, v, 0.0, false).map_err(CmdError::new)?;
  unit(plane.normal())
}

/// The height along `dir` of the outline of `pts` seen along `dir`: the points within a small
/// tolerance of the boundary of their shadow (its convex hull), at their median height.
pub(crate) fn silhouette_height(pts: &[DVec3], dir: DVec3, diagonal: f64) -> Option<f64> {
  let u = dir.any_orthonormal_vector();
  let v = dir.cross(u);
  let flat: Vec<[f64; 2]> = pts.iter().map(|p| [p.dot(u), p.dot(v)]).collect();
  let hull = convex_hull(&flat);
  if hull.len() < 3 {
    return None;
  }
  let tol = (diagonal * 4e-3).max(1e-6);
  let mut heights: Vec<f64> = pts
    .iter()
    .zip(&flat)
    .filter(|(_, q)| (0..hull.len()).any(|k| seg_dist(**q, hull[k], hull[(k + 1) % hull.len()]) <= tol))
    .map(|(p, _)| p.dot(dir))
    .collect();
  if heights.is_empty() {
    return None;
  }
  heights.sort_by(f64::total_cmp);
  let n = heights.len();
  Some(if n % 2 == 1 { heights[n / 2] } else { (heights[n / 2 - 1] + heights[n / 2]) / 2.0 })
}

/// The convex hull of points, counter-clockwise (Andrew's monotone chain).
fn convex_hull(pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
  let mut p: Vec<[f64; 2]> = pts.to_vec();
  p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
  p.dedup();
  if p.len() < 3 {
    return p;
  }
  let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  let mut hull: Vec<[f64; 2]> = Vec::with_capacity(2 * p.len());
  for pass in 0..2 {
    let start = hull.len();
    let iter: Box<dyn Iterator<Item = &[f64; 2]>> =
      if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
    for &q in iter {
      while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0 {
        hull.pop();
      }
      hull.push(q);
    }
    hull.pop();
  }
  hull
}

/// Distance from `p` to the segment `a`–`b`.
fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l2 = dx * dx + dy * dy;
  let t = if l2 < 1e-24 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
  (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_sphere_outline_lies_on_its_equator() {
    // points on a sphere of radius 10 about (0, 0, 5)
    let mut pts = Vec::new();
    for i in 0..=36 {
      let lat = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 36.0;
      for j in 0..48 {
        let lon = std::f64::consts::TAU * j as f64 / 48.0;
        pts.push(DVec3::new(10.0 * lat.cos() * lon.cos(), 10.0 * lat.cos() * lon.sin(), 5.0 + 10.0 * lat.sin()));
      }
    }
    let h = silhouette_height(&pts, DVec3::Z, 20.0).unwrap();
    assert!((h - 5.0).abs() < 1e-6, "{h}");
  }
}
