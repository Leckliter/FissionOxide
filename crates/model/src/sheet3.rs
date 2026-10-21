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
/// rule's bend radius, the outside that plus the thickness. Where folds converge on a corner
/// (`corners`: each corner and the radius of the relief already cut there; the apex of a fan of
/// facets, or two folds meeting in a V) the bends are conical: from the relief their radius grows
/// in proportion to the distance from the corner, to the full radius at the fold's far end. A
/// fold that still can't be rounded gets a smaller bend, or one that closes towards its ends,
/// and the rest stay sharp, with a warning. The developed flat pattern stays as it is: a fold of
/// angle θ shortens the flat by ρ·(2·tan(θ/2) − θ) (ρ the neutral radius), which `bends` (the
/// folds in the flat) is checked against.
pub fn round_folds(
  shape: &Shape,
  bends: &[Bend],
  info: &SheetInfo,
  corners: &[(DVec3, f64)],
) -> Result<(Shape, Option<String>), String> {
  let (t, r) = (info.thickness, info.bend_radius);
  if r <= 1e-9 {
    return Err("rounded bends need a bend radius above zero".into());
  }
  let ke = |e: fox_kernel::KernelError| e.to_string();
  let folds = |s: &Shape| -> Result<Vec<Fold>, String> {
    let (faces, edges) = (s.faces().map_err(ke)?, s.edges().map_err(ke)?);
    Ok(
      lofted_folds(&faces, &edges, t, r)
        .into_iter()
        .filter_map(|(id, rad)| edges.iter().find(|e| e.id == id).map(|e| Fold { id, rad, a: e.start, b: e.end }))
        .collect(),
    )
  };
  let all = folds(shape)?;
  if all.is_empty() {
    return Ok((shape.clone(), None));
  }
  let mut warnings = Vec::new();
  // the corner a fold runs from: the relief one of its ends lies on (the other end clear of it)
  let corner_of = |f: &Fold| {
    let on = |p: DVec3| corners.iter().find(|(c, rr)| (p - *c).length() < rr + 2.0 * t).map(|(c, _)| *c);
    match (on(f.a), on(f.b)) {
      (Some(c), None) | (None, Some(c)) => Some(c),
      _ => None,
    }
  };
  let conical = |f: &Fold| -> Vec<(f64, f64)> {
    let Some(c) = corner_of(f) else { return constant(f.rad) };
    let far = (f.a - c).length().max((f.b - c).length());
    (0..=8)
      .map(|k| {
        let u = k as f64 / 8.0;
        (u, f.rad * ((f.a + (f.b - f.a) * u) - c).length() / far)
      })
      .collect()
  };
  // the folds clear of corners first (true cylinders), then the converging ones, a corner at a
  // time
  let mut converging: Vec<Vec<DVec3>> = Vec::new();
  for (c, _) in corners {
    let at: Vec<DVec3> = all.iter().filter(|f| corner_of(f) == Some(*c)).map(Fold::mid).collect();
    if !at.is_empty() {
      converging.push(at);
    }
  }
  let rest: Vec<Vec<DVec3>> = all.iter().filter(|f| corner_of(f).is_none()).map(|f| vec![f.mid()]).collect();
  let shape = round_batch(shape.clone(), &folds, rest, t, &|f| constant(f.rad))?;
  let shape = round_batch(shape, &folds, converging, t, &conical)?;
  let shape = round_tapered(shape, &folds, t)?;
  // edges on both faces of the sheet: each fold has two
  let sharp = folds(&shape)?.len();
  if sharp > 0 {
    warnings.push(format!(
      "{} of {} fold edges meet too closely to round (or the facets are narrower than the bend); they stay sharp",
      sharp,
      all.len()
    ));
  }
  let rho = r + info.k_factor * t;
  let deduction = bends.iter().map(|b| rho * (2.0 * (b.angle / 2.0).tan() - b.angle)).fold(0.0, f64::max);
  if deduction > 0.01 {
    warnings.push(format!("the flat pattern leaves out up to {deduction:.3} mm of bend deduction at each bend"));
  }
  Ok((shape, (!warnings.is_empty()).then(|| warnings.join("; "))))
}

/// The sharp folds of a press-brake lofted flange, with the fillet radius that turns each into a
/// bend of inside radius `r` (as [`sheet::sharp_corners`]). The sheet's faces are found more
/// loosely than there: around a corner where facets converge, the offset facets can't all stay
/// exactly parallel to their inside faces, so a face of the sheet is a planar face with a nearly
/// opposite one about `t` away.
pub fn lofted_folds(faces: &[fox_kernel::FaceInfo], edges: &[EdgeInfo], t: f64, r: f64) -> Vec<(u32, f64)> {
  use fox_kernel::SurfaceKind;
  let skin = |id: u32| {
    let f = faces.iter().find(|f| f.id == id && f.kind == SurfaceKind::Plane)?;
    faces
      .iter()
      .filter(|g| g.kind == SurfaceKind::Plane && g.normal.dot(f.normal) < -0.98)
      .any(|g| {
        let d = (f.center - g.origin).dot(g.normal).abs();
        d > 0.7 * t && d < 1.3 * t
      })
      .then_some(*f)
  };
  let mut out = Vec::new();
  for e in edges {
    if e.kind != CurveKind::Line || e.faces[1] == 0 {
      continue;
    }
    let (Some(f1), Some(f2)) = (skin(e.faces[0]), skin(e.faces[1])) else { continue };
    if f1.normal.dot(f2.normal).abs() > 0.9999 {
      continue;
    }
    let convex = (f1.center - e.mid).dot(f2.normal) < 0.0;
    out.push((e.id, if convex { r + t } else { r }));
  }
  out
}

/// A sharp fold edge of a shape: its id, the fillet radius that makes it a bend, its ends.
#[derive(Debug, Clone, Copy)]
struct Fold {
  id: u32,
  rad: f64,
  a: DVec3,
  b: DVec3,
}

impl Fold {
  fn mid(&self) -> DVec3 {
    (self.a + self.b) / 2.0
  }
}

/// The fold edges of a shape.
type Folds<'a> = dyn Fn(&Shape) -> Result<Vec<Fold>, String> + 'a;

/// A radius law holding `rad` along the edge.
fn constant(rad: f64) -> Vec<(f64, f64)> {
  vec![(0.0, rad), (1.0, rad)]
}

/// A fillet that comes out as one valid solid (constant radii as true cylinders).
fn fillet_ok(s: &Shape, edges: &[(u32, Vec<(f64, f64)>)]) -> Option<Shape> {
  let flat = |l: &[(f64, f64)]| l.iter().all(|(_, r)| (r - l[0].1).abs() < 1e-12);
  let made = if edges.iter().all(|(_, l)| flat(l)) {
    s.fillet_each(&edges.iter().map(|(id, l)| (*id, l[0].1)).collect::<Vec<_>>())
  } else {
    s.fillet_laws(edges)
  };
  made.ok().filter(|n| n.is_valid() && n.solid_count() == 1)
}

/// Rounds the folds whose midpoints are in `groups` with the radius laws `law` gives them: all
/// in one fillet, else group by group, else one by one, a fold that fails on its own getting a
/// slightly smaller law before it is given up (OCCT's fillets fail now and then at one size).
fn round_batch(
  shape: Shape,
  folds: &Folds,
  groups: Vec<Vec<DVec3>>,
  t: f64,
  law: &dyn Fn(&Fold) -> Vec<(f64, f64)>,
) -> Result<Shape, String> {
  let tol = 1e-6 * (1.0 + t);
  // edge ids change with every fillet: find the folds again by their midpoints
  let find = |s: &Shape, mids: &[DVec3]| -> Result<Vec<(u32, Vec<(f64, f64)>)>, String> {
    Ok(
      folds(s)?.iter().filter(|f| mids.iter().any(|q| (*q - f.mid()).length() < tol)).map(|f| (f.id, law(f))).collect(),
    )
  };
  let everything: Vec<DVec3> = groups.iter().flatten().copied().collect();
  let now = find(&shape, &everything)?;
  if now.is_empty() {
    return Ok(shape);
  }
  if let Some(n) = fillet_ok(&shape, &now) {
    return Ok(n);
  }
  let mut s = shape;
  for g in &groups {
    let now = find(&s, g)?;
    if now.is_empty() {
      continue;
    }
    if let Some(n) = fillet_ok(&s, &now) {
      s = n;
      continue;
    }
    for m in g {
      let Some((id, l)) = find(&s, &[*m])?.into_iter().next() else { continue };
      let made = [1.0, 0.9].iter().find_map(|k| {
        let l: Vec<(f64, f64)> = l.iter().map(|(u, r)| (*u, r * k)).collect();
        fillet_ok(&s, &[(id, l)])
      });
      if let Some(n) = made {
        s = n;
      }
    }
  }
  Ok(s)
}

/// The last try at folds still sharp: the bend at 75 % and half its radius, then bends
/// that close to nothing at one end, at the other, or at both.
fn round_tapered(shape: Shape, folds: &Folds, t: f64) -> Result<Shape, String> {
  let tol = 1e-6 * (1.0 + t);
  let mut s = shape;
  let mut failed: Vec<DVec3> = Vec::new();
  let start = folds(&s)?.len();
  for _ in 0..2 * start {
    let near = |m: DVec3| failed.iter().any(|f| (*f - m).length() < tol);
    let Some(f) = folds(&s)?.into_iter().find(|f| !near(f.mid())) else { break };
    let steps =
      |g: &dyn Fn(f64) -> f64| (0..=8).map(|k| (k as f64 / 8.0, f.rad * g(k as f64 / 8.0))).collect::<Vec<_>>();
    let mut laws: Vec<Vec<(f64, f64)>> = [0.75, 0.5].iter().map(|k| steps(&|_| *k)).collect();
    laws.push(steps(&|u| u));
    laws.push(steps(&|u| 1.0 - u));
    laws.push(steps(&|u| 1.0 - (2.0 * u - 1.0).abs()));
    match laws.into_iter().find_map(|l| fillet_ok(&s, &[(f.id, l)])) {
      Some(n) => s = n,
      None => failed.push(f.mid()),
    }
  }
  Ok(s)
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
    let mut tri = Loop::new(vec![Seg::Line { a, b }, Seg::Line { a: b, b: c }, Seg::Line { a: c, b: a }]);
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
