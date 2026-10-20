//! Sheet metal, continued: butt corners between edge flanges, setbacks that run through an
//! earlier flange's bend, rounded bends between a press-brake lofted flange's facets, and cuts
//! made in a lofted flange carried into its flat pattern.

use fox_kernel::glam::{DMat4, DVec3, DVec4};
use fox_kernel::{Bend, BoolOp, CurveKind, EdgeInfo, Plane, Shape};
use fox_sketch::{Loop, Profile, Seg};

type P2 = [f64; 2];

use crate::sheet::{self, FlatPattern, SheetInfo};
use crate::sheet2::Development;

// ---------------------------------------------------------------- edge flange corners

/// Where a butt corner stops flange i, as distances from the corner point along its edge
/// (`into_i` points from the corner along the edge, `wi` out of the sheet edge): (where its bend
/// ends, where its wall ends). The bend ends where the other flange's setback trims the sheet;
/// the wall, cut square, runs on until its inside corner is `gap` from flange j's. `shift` is
/// the setback (the bend section's x origin, see [`sheet::flange_section`]) and `inner` the
/// offset of the wall's inside face from the edge, both along w.
pub fn butt_corner(
  (into_i, wi): (DVec3, DVec3),
  (into_j, wj): (DVec3, DVec3),
  shift: f64,
  inner: f64,
  gap: f64,
) -> Option<(f64, f64)> {
  let c = wi.dot(wj);
  let across = into_i.dot(wj);
  if 1.0 + c < 1e-9 || across.abs() < 1e-9 {
    return None;
  }
  // the walls' inside faces meet at q (relative to the corner point)
  let q = (wi + wj) * (inner / (1.0 + c));
  let half = into_i.angle_between(into_j) / 2.0;
  if half.sin() < 1e-9 {
    return None;
  }
  let wall = q.dot(into_i) + gap / (2.0 * half.sin());
  Some((shift / across, wall))
}

/// An earlier flange's bend that the sheet runs into past the end `p` of an edge (`out` points
/// on from the edge's end, `w` out of the sheet edge): the bend's section on a plane across `w`,
/// to sweep along w over the setback so the new flange's setback trims that bend as well.
/// `r` and `t` are the rule's bend radius and thickness.
pub fn bend_past_end(edges: &[EdgeInfo], p: DVec3, out: DVec3, w: DVec3, r: f64, t: f64) -> Option<(Plane, Profile)> {
  let tol = 1e-6 * (1.0 + r + t);
  edges.iter().find_map(|e| {
    if e.kind != CurveKind::Circle || e.axis.dot(w).abs() < 1.0 - 1e-6 || (e.start - e.end).length() < tol {
      return None;
    }
    let other = if (e.start - p).length() < tol {
      e.end
    } else if (e.end - p).length() < tol {
      e.start
    } else {
      return None;
    };
    // the arc is the inside or the outside of a bend that starts tangent to the sheet at p
    if (e.radius - r).abs() > 1e-4 * (1.0 + r) && (e.radius - r - t).abs() > 1e-4 * (1.0 + r) {
      return None;
    }
    let to_center = e.center - p;
    let y = to_center.try_normalize()?;
    if y.dot(out).abs() > 1e-6 || (other - p).dot(out) < tol || y.dot(w).abs() > 1e-6 {
      return None;
    }
    let angle = e.length / e.radius;
    if !(angle > 1e-6 && angle < std::f64::consts::PI + 1e-6) {
      return None;
    }
    let plane = Plane { origin: e.center - y * r, x: out, y };
    Some((plane, sheet::flange_section(r, t, angle, 0.0)))
  })
}

// ---------------------------------------------------------------- lofted flanges

/// Rounds the folds between a press-brake flange's facets: the inside of each fold gets the
/// rule's bend radius, the outside that plus the thickness. Folds that meet too closely to round
/// (a fan of facets converging on a corner) stay sharp, with a warning. The developed flat
/// pattern stays as it is: a fold of angle θ shortens the flat by ρ·(2·tan(θ/2) − θ) (ρ the
/// neutral radius), which `bends` (the folds in the flat) is checked against.
pub fn round_folds(shape: &Shape, bends: &[Bend], info: &SheetInfo) -> Result<(Shape, Option<String>), String> {
  let (t, r) = (info.thickness, info.bend_radius);
  if r <= 1e-9 {
    return Err("rounded bends need a bend radius above zero".into());
  }
  let ke = |e: fox_kernel::KernelError| e.to_string();
  let folds = |s: &Shape| -> Result<Vec<(u32, f64, DVec3)>, String> {
    let (faces, edges) = (s.faces().map_err(ke)?, s.edges().map_err(ke)?);
    Ok(
      sheet::sharp_corners(&faces, &edges, t, r)
        .into_iter()
        .filter_map(|(id, rad)| edges.iter().find(|e| e.id == id).map(|e| (id, rad, e.mid)))
        .collect(),
    )
  };
  let all = folds(shape)?;
  if all.is_empty() {
    return Ok((shape.clone(), None));
  }
  let mut warnings = Vec::new();
  let shape = match shape.fillet_each(&all.iter().map(|(id, rad, _)| (*id, *rad)).collect::<Vec<_>>()) {
    Ok(s) => s,
    Err(_) => {
      // one at a time, skipping those that fail
      let mut s = shape.clone();
      let mut failed: Vec<DVec3> = Vec::new();
      for _ in 0..2 * all.len() {
        let near = |m: DVec3| failed.iter().any(|f| (*f - m).length() < 1e-6 * (1.0 + t));
        let Some((id, rad, mid)) = folds(&s)?.into_iter().find(|(_, _, m)| !near(*m)) else { break };
        match s.fillet_each(&[(id, rad)]) {
          Ok(n) if n.is_valid() => s = n,
          _ => failed.push(mid),
        }
      }
      // edges on both faces of the sheet: each fold has two
      let sharp = folds(&s)?.len();
      warnings.push(format!(
        "{} of {} fold edges meet too closely to round (or the facets are narrower than the bend); they stay sharp",
        sharp,
        all.len()
      ));
      s
    }
  };
  let rho = r + info.k_factor * t;
  let deduction = bends.iter().map(|b| rho * (2.0 * (b.angle / 2.0).tan() - b.angle)).fold(0.0, f64::max);
  if deduction > 0.01 {
    warnings.push(format!("the flat pattern leaves out up to {deduction:.3} mm of bend deduction at each bend"));
  }
  Ok((shape, (!warnings.is_empty()).then(|| warnings.join("; "))))
}

/// A lofted flange's flat pattern with the cuts made since it was built (holes, notches, any
/// material missing from the as-built flange): each facet's share of what was removed is moved
/// from the folded flange onto the facet's place in the flat, then cut from it.
pub fn developed_with_cuts(dev: &Development, t: f64, current: &Shape) -> Result<FlatPattern, String> {
  let ke = |e: fox_kernel::KernelError| e.to_string();
  let mut flat = dev.flat_pattern(t)?;
  let built = dev.built.as_ref().ok_or("the lofted flange's flat pattern only holds while the body is as built")?;
  let built = built.lock().map_err(|_| "the lofted flange's built shape is unavailable")?.clone();
  let removed = match built.boolean(current, BoolOp::Subtract) {
    Ok(r) => r,
    Err(fox_kernel::KernelError::Empty) => return Ok(flat),
    Err(e) => return Err(e.to_string()),
  };
  let cur = current.mass().map_err(ke)?.volume;
  let rem = removed.mass().map_err(ke)?;
  if cur > dev.volume + 1e-6 * dev.volume.max(1.0) + 1e-9 {
    return Err("material was added to the lofted flange: its flat pattern can't follow".into());
  }
  let (lo, hi) = (rem.bbox_min, rem.bbox_max);
  // the flat's top face is the profiles' side of the sheet, the material behind it
  let n_flat = dev.plane.normal();
  let mut shape = flat.shape.clone();
  let before = shape.mass().map_err(ke)?.volume;
  for (p3, p2, toward) in &dev.facets {
    let (e1, e2) = (p3[1] - p3[0], p3[2] - p3[0]);
    let (Some(x3), Some(z3)) = (e1.try_normalize(), e1.cross(e2).try_normalize()) else { continue };
    let y3 = z3.cross(x3);
    // skip facets whose slab misses what was removed
    let pad = DVec3::splat(2.0 * t);
    let fmin = p3.iter().fold(DVec3::splat(f64::INFINITY), |a, p| a.min(*p - pad));
    let fmax = p3.iter().fold(DVec3::splat(f64::NEG_INFINITY), |a, p| a.max(*p + pad));
    if fmin.cmpgt(hi).any() || fmax.cmplt(lo).any() {
      continue;
    }
    // the facet's prism through the sheet
    let pl = Plane { origin: p3[0] - z3 * (2.0 * t), x: x3, y: y3 };
    let loc = |p: DVec3| [(p - p3[0]).dot(x3), (p - p3[0]).dot(y3)];
    let (a, b, c) = (loc(p3[0]), loc(p3[1]), loc(p3[2]));
    let mut tri = Loop { segs: vec![Seg::Line { a, b }, Seg::Line { a: b, b: c }, Seg::Line { a: c, b: a }] };
    if tri.signed_area() < 0.0 {
      tri = tri.reversed();
    }
    let face = Shape::face(&pl, &Profile { outer: tri, holes: Vec::new() }).map_err(ke)?;
    let prism = face.extrude(z3 * (4.0 * t)).map_err(ke)?;
    let Ok(piece) = removed.boolean(&prism, BoolOp::Intersect) else { continue };
    let Ok(pm) = piece.mass() else { continue };
    if pm.volume < 1e-9 {
      continue;
    }
    // the rigid map from the facet onto its place in the flat (its neutral layer `depth` under
    // the top), mirrored when that puts the material on the wrong side
    let w = |q: P2| dev.plane.to_world(q) - n_flat * dev.depth;
    let (q0, q1, q2) = (w(p2[0]), w(p2[1]), w(p2[2]));
    let x2 = (q1 - q0).try_normalize().ok_or("a developed facet has no width")?;
    let y2 = (q2 - q0 - x2 * (q2 - q0).dot(x2)).try_normalize().ok_or("a developed facet has no width")?;
    let y3 = (e2 - x3 * e2.dot(x3)).normalize();
    let (z3, z2) = (x3.cross(y3), x2.cross(y2));
    let from = DMat4::from_cols(x3.extend(0.0), y3.extend(0.0), z3.extend(0.0), p3[0].extend(1.0));
    let to = DMat4::from_cols(x2.extend(0.0), y2.extend(0.0), z2.extend(0.0), DVec4::new(q0.x, q0.y, q0.z, 1.0));
    let mut piece = piece.transformed(to * from.inverse()).map_err(ke)?;
    if (z3.dot(*toward) > 0.0) != (z2.dot(-n_flat) > 0.0) {
      piece = piece.mirrored(q0, n_flat).map_err(ke)?;
    }
    if let Ok(s) = shape.boolean(&piece, BoolOp::Subtract) {
      shape = s;
    }
  }
  let moved = before - shape.mass().map_err(ke)?.volume;
  if moved < 0.5 * rem.volume {
    return Err("the cuts in the lofted flange could not be placed in its flat pattern".into());
  }
  flat.shape = shape.unified().map_err(ke)?;
  Ok(flat)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn butt_corner_of_a_tray() {
    // the corner at the origin of a tray: flange i along +x (out of the sheet towards −y),
    // flange j along +y (out towards −x); inside bends R 2.5, t 2.5, gap 2.5
    let (shift, inner, gap) = (-5.0, -2.5, 2.5);
    let (bend, wall) = butt_corner((DVec3::X, -DVec3::Y), (DVec3::Y, -DVec3::X), shift, inner, gap).unwrap();
    assert!((bend - 5.0).abs() < 1e-12, "{bend}");
    assert!((wall - (2.5 + 2.5 / 2f64.sqrt())).abs() < 1e-12, "{wall}");
  }
}
