//! Sheet metal: rules, the 2D sections of contour and edge flanges, thickness measurement and
//! flat patterns. The open-path and hole-section helpers here also serve Sweep and Hole.
//!
//! Sheet metal bodies are ordinary solids tagged with a [`SheetInfo`] (thickness, bend radius,
//! K-factor). Bends are cylindrical faces, so any feature can keep working on the body, and the
//! kernel's unfolder rolls the bends out by the length of their neutral layer to make the flat
//! pattern.

use std::collections::HashMap;
use std::f64::consts::PI;

use fox_kernel::glam::DVec3;
use fox_kernel::{Bend, FaceInfo, Plane, Shape, SurfaceKind};
use fox_sketch::{CurveId, CurveKind, Loop, Profile, Seg, Sketch};

use crate::{Corner3Relief, CornerPlacement, CornerRelief, ReliefShape, SheetMetalRule};

/// Points closer than this join a path.
const JOIN_TOL: f64 = 1e-6;

/// The sheet metal settings a body carries (evaluated from its rule).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetInfo {
  pub rule: String,
  /// mm
  pub thickness: f64,
  /// Inside bend radius, mm.
  pub bend_radius: f64,
  pub k_factor: f64,
  pub relief: ReliefShape,
  pub relief_width: f64,
  /// Past the bend.
  pub relief_depth: f64,
  /// Narrower material beside a relief goes with it.
  pub relief_remnant: f64,
  /// Mitered corners and rips.
  pub gap: f64,
  /// The body is a flat pattern (after Unfold).
  pub flat: bool,
  pub corner_relief: CornerRelief,
  /// mm; `None` = the default for the shape.
  pub corner_relief_size: Option<f64>,
  pub corner_placement: CornerPlacement,
  pub corner3_relief: Corner3Relief,
  /// mm
  pub corner3_radius: f64,
  /// Set on lofted flanges, which the B-rep unfolder can't flatten (see [`crate::sheet2`]).
  pub developed: Option<std::sync::Arc<crate::sheet2::Development>>,
}

/// What Refold needs to fold a flat pattern back up.
#[derive(Debug, Clone)]
pub struct Unfolded {
  /// The body before it was unfolded.
  pub folded: Shape,
  /// Its stationary face.
  pub face: u32,
  /// The flat pattern as unfolded (later changes are found by comparing with it).
  pub flat: Shape,
}

impl SheetInfo {
  /// Length of the neutral layer through a bend of `angle` radians (the bend allowance).
  pub fn bend_allowance(&self, angle: f64) -> f64 {
    angle.abs() * (self.bend_radius + self.k_factor * self.thickness)
  }

  /// Corner relief size: a square's side, a round or tear relief's diameter. By default a
  /// square or round relief clears both bends where they meet, a tear is a thickness wide.
  pub fn corner_relief_size(&self) -> f64 {
    let (r, t) = (self.bend_radius, self.thickness);
    self.corner_relief_size.unwrap_or(match self.corner_relief {
      CornerRelief::Round => 2.0 * std::f64::consts::SQRT_2 * (r + t),
      CornerRelief::Tear => t,
      CornerRelief::Square | CornerRelief::None => 2.0 * (r + t),
    })
  }
}

/// A rule's scope: the design's parameters, and `Thickness` once the thickness is known.
struct RuleScope<'a> {
  params: &'a HashMap<String, f64>,
  thickness: Option<f64>,
}

impl fox_expr::Scope for RuleScope<'_> {
  fn get(&self, name: &str) -> Option<f64> {
    match self.thickness {
      Some(t) if name == "Thickness" => Some(t),
      _ => self.params.get(name).copied(),
    }
  }
}

/// Evaluates a rule's expressions. Every value but the thickness may use `Thickness`.
pub fn eval_rule(rule: &SheetMetalRule, params: &HashMap<String, f64>) -> Result<SheetInfo, String> {
  let mut scope = RuleScope { params, thickness: None };
  let thickness = fox_expr::eval(&rule.thickness, fox_expr::Kind::Length, &scope)
    .map_err(|err| format!("'{}': {err}", rule.thickness))?;
  if !(thickness > 1e-6 && thickness.is_finite()) {
    return Err(format!("rule {}: the thickness must be positive", rule.name));
  }
  scope.thickness = Some(thickness);
  let len = |e: &str| fox_expr::eval(e, fox_expr::Kind::Length, &scope).map_err(|err| format!("'{e}': {err}"));
  let bend_radius = len(&rule.bend_radius)?;
  let k_factor = fox_expr::eval(&rule.k_factor, fox_expr::Kind::Unitless, &scope)
    .map_err(|err| format!("'{}': {err}", rule.k_factor))?;
  if !(bend_radius >= 0.0 && bend_radius.is_finite()) {
    return Err(format!("rule {}: the bend radius must not be negative", rule.name));
  }
  if !(0.0..=1.0).contains(&k_factor) {
    return Err(format!("rule {}: the K-factor must be between 0 and 1", rule.name));
  }
  let or = |e: &str, default: f64| -> Result<f64, String> { if e.trim().is_empty() { Ok(default) } else { len(e) } };
  let relief_width = or(&rule.relief_width, thickness)?;
  let relief_depth = or(&rule.relief_depth, thickness / 2.0)?;
  let relief_remnant = or(&rule.relief_remnant, thickness * 2.0)?;
  let gap = or(&rule.gap, thickness / 2.0)?;
  let corner3_radius = or(&rule.corner3_radius, thickness)?;
  if !(relief_width > 0.0 && relief_depth >= 0.0 && relief_remnant >= 0.0 && gap > 0.0 && corner3_radius > 0.0) {
    return Err(format!("rule {}: relief sizes and the gap must be positive", rule.name));
  }
  let corner_relief_size = if rule.corner_relief_size.trim().is_empty() {
    None
  } else {
    let v = len(&rule.corner_relief_size)?;
    if !(v > 1e-6 && v.is_finite()) {
      return Err(format!("rule {}: the corner relief size must be positive", rule.name));
    }
    Some(v)
  };
  Ok(SheetInfo {
    rule: rule.name.clone(),
    thickness,
    bend_radius,
    k_factor,
    relief: rule.relief,
    relief_width,
    relief_depth,
    relief_remnant,
    gap,
    flat: false,
    corner_relief: rule.corner_relief,
    corner_relief_size,
    corner_placement: rule.corner_placement,
    corner3_relief: rule.corner3_relief,
    corner3_radius,
    developed: None,
  })
}

// ---------------------------------------------------------------- 2D helpers

type P2 = [f64; 2];

fn sub(a: P2, b: P2) -> P2 {
  [a[0] - b[0], a[1] - b[1]]
}

fn add(a: P2, b: P2) -> P2 {
  [a[0] + b[0], a[1] + b[1]]
}

fn mul(a: P2, k: f64) -> P2 {
  [a[0] * k, a[1] * k]
}

fn len2(a: P2) -> f64 {
  a[0].hypot(a[1])
}

fn unit(a: P2) -> Option<P2> {
  let l = len2(a);
  (l > 1e-12).then(|| mul(a, 1.0 / l))
}

/// Left normal of a direction.
fn left(d: P2) -> P2 {
  [-d[1], d[0]]
}

fn cross(a: P2, b: P2) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

fn dot(a: P2, b: P2) -> f64 {
  a[0] * b[0] + a[1] * b[1]
}

fn arc(center: P2, radius: f64, from: P2, sweep: f64) -> Seg {
  let d = sub(from, center);
  Seg::Arc { center, radius, start_angle: d[1].atan2(d[0]), sweep }
}

/// Unit tangent at the start / end of a segment, in travel direction.
fn tangents(s: &Seg) -> Option<(P2, P2)> {
  match *s {
    Seg::Line { a, b } => unit(sub(b, a)).map(|d| (d, d)),
    Seg::Arc { start_angle, sweep, .. } => {
      let sg = sweep.signum();
      let t = |a: f64| [-a.sin() * sg, a.cos() * sg];
      Some((t(start_angle), t(start_angle + sweep)))
    }
    Seg::Circle { .. } => None,
    Seg::Bezier { p } => {
      let a = unit(sub(if len2(sub(p[1], p[0])) > 1e-12 { p[1] } else { p[2] }, p[0]))?;
      let b = unit(sub(p[3], if len2(sub(p[3], p[2])) > 1e-12 { p[2] } else { p[1] }))?;
      Some((a, b))
    }
    Seg::EllipseArc { .. } | Seg::Conic { .. } | Seg::Quintic { .. } => {
      let (a, b) = s.end_tangents();
      Some((unit(a)?, unit(b)?))
    }
  }
}

// ---------------------------------------------------------------- contour flange

/// The open path of a sketch for a contour flange: the given curves (or every non-construction
/// line and arc), chained end to end.
pub fn open_path(sketch: &Sketch, curves: &[CurveId]) -> Result<Vec<Seg>, String> {
  let mut segs = Vec::new();
  for c in &sketch.curves {
    if !(curves.is_empty() && !c.reference() || curves.contains(&c.id)) {
      continue;
    }
    match &c.kind {
      CurveKind::Line { a, b } => {
        let (Some(a), Some(b)) = (sketch.pos(*a), sketch.pos(*b)) else { continue };
        if len2(sub(b, a)) > JOIN_TOL {
          segs.push(Seg::Line { a, b });
        }
      }
      CurveKind::Arc { center, start, end } => {
        let (Some(c), Some(s), Some(e)) = (sketch.pos(*center), sketch.pos(*start), sketch.pos(*end)) else { continue };
        let a0 = sub(s, c)[1].atan2(sub(s, c)[0]);
        let a1 = sub(e, c)[1].atan2(sub(e, c)[0]);
        let mut sweep = (a1 - a0).rem_euclid(2.0 * PI);
        if sweep < 1e-12 {
          sweep = 2.0 * PI;
        }
        segs.push(Seg::Arc { center: c, radius: len2(sub(s, c)), start_angle: a0, sweep });
      }
      CurveKind::Spline { .. } | CurveKind::ControlSpline { .. } | CurveKind::Bezier { .. } => {
        segs.extend(sketch.beziers(c.id).into_iter().map(|p| Seg::Bezier { p }));
      }
      CurveKind::EllipticalArc { .. } | CurveKind::Conic { .. } | CurveKind::QuinticSpline { .. } => {
        segs.extend(sketch.curve_segs(c.id))
      }
      CurveKind::Ellipse { .. } => {
        if !curves.is_empty() {
          return Err("a path can't include a full ellipse".into());
        }
      }
      CurveKind::Circle { .. } => {
        if !curves.is_empty() {
          return Err("a contour flange path can't include a full circle".into());
        }
      }
    }
  }
  if segs.is_empty() {
    return Err("the sketch has no lines or arcs for the contour".into());
  }
  // endpoints shared by fewer than two segments start the path
  let near = |a: P2, b: P2| len2(sub(a, b)) < JOIN_TOL;
  let degree =
    |p: P2| segs.iter().filter(|s| near(s.start(), p)).count() + segs.iter().filter(|s| near(s.end(), p)).count();
  for s in &segs {
    if degree(s.start()) > 2 || degree(s.end()) > 2 {
      return Err("the contour path branches; pick one chain of curves".into());
    }
  }
  let start = segs
    .iter()
    .flat_map(|s| [s.start(), s.end()])
    .find(|p| degree(*p) == 1)
    .ok_or("the contour is closed; use Flange for closed profiles")?;
  let mut path = Vec::new();
  let mut at = start;
  let mut left_over = segs.clone();
  while let Some(i) = left_over.iter().position(|s| near(s.start(), at) || near(s.end(), at)) {
    let s = left_over.swap_remove(i);
    let s = if near(s.start(), at) { s } else { s.reversed() };
    at = s.end();
    path.push(s);
  }
  if !left_over.is_empty() {
    return Err("the contour curves don't form one connected path".into());
  }
  Ok(path)
}

/// The cross-section of a contour flange: the path is one face of the sheet, the material lies
/// on its left (right when `flip`), and sharp corners between lines become bends with inside
/// radius `r`.
pub fn contour_section(path: &[Seg], t: f64, r: f64, flip: bool) -> Result<Profile, String> {
  let side = if flip { -1.0 } else { 1.0 };
  // 1. round sharp corners
  let mut p: Vec<Seg> = Vec::new();
  for (i, s) in path.iter().enumerate() {
    let mut s = *s;
    if let Some(prev) = p.last_mut() {
      let (_, t_in) = tangents(prev).ok_or("bad path segment")?;
      let (t_out, _) = tangents(&s).ok_or("bad path segment")?;
      let turn = cross(t_in, t_out).atan2(dot(t_in, t_out));
      if turn.abs() > 1e-7 {
        let (Seg::Line { a: pa, .. }, Seg::Line { b: sb, .. }) = (*prev, s) else {
          return Err(format!("corner {i} next to an arc must be tangent"));
        };
        if turn.abs() > PI * 179.0 / 180.0 {
          return Err(format!("corner {i} folds back on itself"));
        }
        // turning towards the material puts the path on the outside of the bend
        let rho = if turn * side > 0.0 { r + t } else { r };
        let cut = rho * (turn.abs() / 2.0).tan();
        let corner = s.start();
        let e = sub(corner, mul(t_in, cut));
        let f = add(corner, mul(t_out, cut));
        if dot(sub(e, pa), t_in) < -1e-9 || dot(sub(sb, f), t_out) < -1e-9 {
          return Err(format!("the bend at corner {i} doesn't fit (segments too short for the bend radius)"));
        }
        *prev = Seg::Line { a: pa, b: e };
        let center = add(e, mul(left(t_in), rho * turn.signum()));
        if rho > 1e-9 {
          p.push(arc(center, rho, e, turn));
        }
        s = Seg::Line { a: f, b: sb };
      }
    }
    p.push(s);
  }
  p.retain(|s| match s {
    Seg::Line { a, b } => len2(sub(*b, *a)) > 1e-9,
    _ => true,
  });
  // 2. the other face: the path offset by t towards the material
  let mut q = Vec::with_capacity(p.len());
  for s in &p {
    q.push(match *s {
      Seg::Line { a, b } => {
        let n = mul(left(unit(sub(b, a)).ok_or("bad path segment")?), side * t);
        Seg::Line { a: add(a, n), b: add(b, n) }
      }
      Seg::Arc { center, radius, start_angle, sweep } => {
        let r2 = radius - side * sweep.signum() * t;
        if r2 < 1e-9 {
          return Err("an arc in the contour is tighter than the sheet thickness".into());
        }
        Seg::Arc { center, radius: r2, start_angle, sweep }
      }
      Seg::Circle { .. } => return Err("bad path segment".into()),
      Seg::Bezier { .. } | Seg::Quintic { .. } => {
        return Err("contour flanges follow lines and arcs, not splines".into());
      }
      Seg::EllipseArc { .. } => return Err("contour flanges follow lines and arcs, not ellipses".into()),
      Seg::Conic { .. } => return Err("contour flanges follow lines and arcs, not conics".into()),
    });
  }
  let (Some(p0), Some(p1), Some(q0), Some(q1)) = (p.first(), p.last(), q.first(), q.last()) else {
    return Err("the contour path is empty".into());
  };
  let mut segs = p.clone();
  segs.push(Seg::Line { a: p1.end(), b: q1.end() });
  segs.extend(q.iter().rev().map(|s| s.reversed()));
  segs.push(Seg::Line { a: q0.start(), b: p0.start() });
  let mut outer = Loop::new(segs);
  if outer.signed_area() < 0.0 {
    outer = outer.reversed();
  }
  Ok(Profile { outer, holes: Vec::new() })
}

// ---------------------------------------------------------------- edge flange

/// Section of an edge flange in its own plane: x points out of the sheet edge, y the way the
/// flange bends. The sheet's bend-side face is y = 0, its other face y = −t, and the bend starts
/// at x = 0. `straight` is the flat length after the bend.
pub fn flange_section(r: f64, t: f64, angle: f64, straight: f64) -> Profile {
  let c = [0.0, r];
  let (sa, ca) = angle.sin_cos();
  let dir = [ca, sa];
  let end = [sa, -ca];
  let i0 = [0.0, 0.0];
  let o0 = [0.0, -t];
  let i1 = add(c, mul(end, r));
  let o1 = add(c, mul(end, r + t));
  let mut segs = Vec::new();
  if r > 1e-9 {
    segs.push(arc(c, r, i0, angle));
  }
  if straight > 1e-9 {
    let (i2, o2) = (add(i1, mul(dir, straight)), add(o1, mul(dir, straight)));
    segs.push(Seg::Line { a: i1, b: i2 });
    segs.push(Seg::Line { a: i2, b: o2 });
    segs.push(Seg::Line { a: o2, b: o1 });
  } else {
    segs.push(Seg::Line { a: i1, b: o1 });
  }
  segs.push(arc(c, r + t, o1, -angle));
  segs.push(Seg::Line { a: o0, b: i0 });
  let mut outer = Loop::new(segs);
  if outer.signed_area() < 0.0 {
    outer = outer.reversed();
  }
  Profile { outer, holes: Vec::new() }
}

/// An axis-aligned rectangle profile.
pub fn rectangle(lo: P2, hi: P2) -> Profile {
  let (a, b, c, d) = (lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]);
  let segs = vec![Seg::Line { a, b }, Seg::Line { a: b, b: c }, Seg::Line { a: c, b: d }, Seg::Line { a: d, b: a }];
  Profile { outer: Loop::new(segs), holes: Vec::new() }
}

/// A bend relief notch in (along the edge, across it; + = outward) coordinates: `along` is the
/// notch's span, it reaches from `outer` in to `inner`, with a round inner end for round reliefs.
pub fn relief_profile(along: (f64, f64), inner: f64, outer: f64, round: bool) -> Profile {
  let (a0, a1) = along;
  let w = a1 - a0;
  if !round || inner + w / 2.0 >= outer {
    return rectangle([a0, inner], [a1, outer]);
  }
  let c = [(a0 + a1) / 2.0, inner + w / 2.0];
  let segs = vec![
    Seg::Line { a: [a1, outer], b: [a0, outer] },
    Seg::Line { a: [a0, outer], b: [a0, c[1]] },
    Seg::Arc { center: c, radius: w / 2.0, start_angle: PI, sweep: PI },
    Seg::Line { a: [a1, c[1]], b: [a1, outer] },
  ];
  Profile { outer: Loop::new(segs), holes: Vec::new() }
}

/// A slab standing in for the half-space in front of a plane (towards `normal`).
pub fn half_space(origin: DVec3, normal: DVec3, size: f64) -> Result<Shape, String> {
  let n = normal.try_normalize().ok_or("zero normal")?;
  let pl = Plane::from_normal(origin, n, DVec3::X).map_err(|e| e.to_string())?;
  let face = Shape::face(&pl, &rectangle([-size, -size], [size, size])).map_err(|e| e.to_string())?;
  face.extrude(n * size).map_err(|e| e.to_string())
}

/// The two planar faces at a sheet edge: (the sheet face, the side wall). The sheet face is the
/// one with the other side of the sheet `t` behind it.
pub fn edge_faces(faces: &[FaceInfo], ids: [u32; 2], edge_len: f64, t: f64) -> Option<(FaceInfo, FaceInfo)> {
  let a = *faces.iter().find(|f| f.id == ids[0] && f.kind == SurfaceKind::Plane)?;
  let b = *faces.iter().find(|f| f.id == ids[1] && f.kind == SurfaceKind::Plane)?;
  if a.normal.dot(b.normal).abs() > 1e-6 {
    return None;
  }
  let skin = |f: &FaceInfo| measure_thickness(faces, f.id).is_some_and(|d| (d - t).abs() < 1e-3 * t.max(1.0));
  match (skin(&a), skin(&b)) {
    (true, false) => Some((a, b)),
    (false, true) => Some((b, a)),
    (false, false) => None,
    // a strip as narrow as it is thick: the narrower face is the wall
    (true, true) => {
      let width = |f: &FaceInfo| f.area / edge_len.max(1e-9);
      if (width(&a) - t).abs() <= (width(&b) - t).abs() { Some((b, a)) } else { Some((a, b)) }
    }
  }
}

// ---------------------------------------------------------------- holes

/// The top of a hole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HoleHead {
  None,
  Counterbore {
    radius: f64,
    depth: f64,
  },
  /// `angle` is the included angle of the cone.
  Countersink {
    radius: f64,
    angle: f64,
  },
}

/// Half-section of a hole (x = radius, y = depth into the material), revolved about the y axis.
/// `tip` is the drill point's included angle (0 = flat bottom).
pub fn hole_section(r: f64, depth: f64, tip: f64, head: HoleHead) -> Result<Profile, String> {
  let mut pts: Vec<P2> = vec![[0.0, 0.0]];
  match head {
    HoleHead::None => pts.push([r, 0.0]),
    HoleHead::Counterbore { radius, depth: d } => {
      if !(radius > r && d > 0.0 && d < depth) {
        return Err("the counterbore must be wider than the hole and shallower than it".into());
      }
      pts.extend([[radius, 0.0], [radius, d], [r, d]]);
    }
    HoleHead::Countersink { radius, angle } => {
      if !(radius > r && angle > 0.0 && angle < PI) {
        return Err("the countersink must be wider than the hole, with an angle under 180°".into());
      }
      let d = (radius - r) / (angle / 2.0).tan();
      if d >= depth {
        return Err("the countersink is deeper than the hole".into());
      }
      pts.extend([[radius, 0.0], [r, d]]);
    }
  }
  pts.push([r, depth]);
  if tip > 1e-6 && tip < PI {
    pts.push([0.0, depth + r / (tip / 2.0).tan()]);
  } else {
    pts.push([0.0, depth]);
  }
  let segs = (0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect();
  let mut outer = Loop::new(segs);
  if outer.signed_area() < 0.0 {
    outer = outer.reversed();
  }
  Ok(Profile { outer, holes: Vec::new() })
}

// ---------------------------------------------------------------- thickness and flat patterns

/// The largest planar face (the default stationary face).
pub fn default_face(faces: &[FaceInfo]) -> Option<u32> {
  faces.iter().filter(|f| f.kind == SurfaceKind::Plane).max_by(|a, b| a.area.total_cmp(&b.area)).map(|f| f.id)
}

/// Distance from a planar face to the nearest parallel face behind it (the sheet thickness).
pub fn measure_thickness(faces: &[FaceInfo], face: u32) -> Option<f64> {
  let f = faces.iter().find(|f| f.id == face && f.kind == SurfaceKind::Plane)?;
  faces
    .iter()
    .filter(|g| g.kind == SurfaceKind::Plane && g.normal.dot(f.normal) < -0.9999)
    .map(|g| (f.origin - g.origin).dot(f.normal))
    .filter(|d| *d > 1e-6)
    .min_by(f64::total_cmp)
}

/// Sharp straight corners between two faces of a sheet `t` thick, with the fillet radius that
/// turns each into a bend of inside radius `r`: outside corners get r + t, inside corners r.
pub fn sharp_corners(faces: &[FaceInfo], edges: &[fox_kernel::EdgeInfo], t: f64, r: f64) -> Vec<(u32, f64)> {
  let skin = |id: u32| {
    faces
      .iter()
      .find(|f| f.id == id && f.kind == SurfaceKind::Plane)
      .copied()
      .filter(|f| measure_thickness(faces, f.id).is_some_and(|d| (d - t).abs() < 1e-3 * t.max(1.0)))
  };
  let mut out = Vec::new();
  for e in edges {
    if e.kind != fox_kernel::CurveKind::Line || e.faces[1] == 0 {
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

/// Square corner reliefs where two sharp outside corners of a sheet (future bends) meet, sized to
/// clear both bends: boxes at the corner reaching `r + t + relief width` along each edge and up
/// the third direction.
pub fn corner_reliefs(
  faces: &[FaceInfo],
  edges: &[fox_kernel::EdgeInfo],
  sheet: &SheetInfo,
  diagonal: f64,
) -> Result<Vec<Shape>, String> {
  let (t, r) = (sheet.thickness, sheet.bend_radius);
  let convex: Vec<&fox_kernel::EdgeInfo> = sharp_corners(faces, edges, t, r)
    .iter()
    .filter(|(_, rad)| *rad > r + t * 0.5)
    .filter_map(|(id, _)| edges.iter().find(|e| e.id == *id))
    .collect();
  let centre = faces.iter().fold(DVec3::ZERO, |a, f| a + f.center * f.area) / faces.iter().map(|f| f.area).sum::<f64>();
  let size = r + t + sheet.relief_width;
  let reach = 2.0 * (r + t) + 4.0 * sheet.gap + t;
  let mut out = Vec::new();
  let mut done: Vec<DVec3> = Vec::new();
  for (i, a) in convex.iter().enumerate() {
    for b in convex.iter().skip(i + 1) {
      let (da, db) = ((a.end - a.start).normalize_or_zero(), (b.end - b.start).normalize_or_zero());
      if da.dot(db).abs() > 0.999 {
        continue;
      }
      // the nearest pair of ends
      let Some((pa, pb)) = [a.start, a.end]
        .iter()
        .flat_map(|p| [b.start, b.end].map(|q| (*p, q)))
        .min_by(|x, y| (x.0 - x.1).length().total_cmp(&(y.0 - y.1).length()))
      else {
        continue;
      };
      if (pa - pb).length() > reach {
        continue;
      }
      // where the two edge lines meet (closest points)
      let w = a.start - b.start;
      let (aa, bb, cc, dd, ee) = (da.dot(da), da.dot(db), db.dot(db), da.dot(w), db.dot(w));
      let den = aa * cc - bb * bb;
      if den.abs() < 1e-12 {
        continue;
      }
      let sa = (bb * ee - cc * dd) / den;
      let sb = (aa * ee - bb * dd) / den;
      let v = ((a.start + da * sa) + (b.start + db * sb)) / 2.0;
      if done.iter().any(|q| (*q - v).length() < 1e-6) {
        continue;
      }
      done.push(v);
      let x = da * (centre - v).dot(da).signum();
      let y = (db - x * db.dot(x)).normalize_or_zero();
      let y = y * (centre - v).dot(y).signum();
      let z = x.cross(y);
      let z = z * (centre - v).dot(z).signum();
      let m = t + diagonal * 1e-3;
      let pl = Plane { origin: v - (x + y + z) * m, x, y };
      let face = Shape::face(&pl, &rectangle([0.0, 0.0], [size + m, size + m])).map_err(|e| e.to_string())?;
      out.push(face.extrude(z * (size + m)).map_err(|e| e.to_string())?);
    }
  }
  Ok(out)
}

/// A flattened sheet metal body.
#[derive(Debug, Clone)]
pub struct FlatPattern {
  pub shape: Shape,
  pub bends: Vec<Bend>,
  /// The stationary face's plane (outward normal); the flat lies on it, material behind it.
  pub plane: Plane,
}

/// Unfolds a sheet metal shape onto face `face` (1-based, planar).
pub fn flat_pattern(shape: &Shape, faces: &[FaceInfo], face: u32, sheet: &SheetInfo) -> Result<FlatPattern, String> {
  let info = faces.iter().find(|f| f.id == face).ok_or("the stationary face could not be found")?;
  let plane = info.plane().ok_or("the stationary face must be planar")?;
  let (flat, bends) = shape.unfold(face, sheet.thickness, sheet.k_factor).map_err(|e| e.to_string())?;
  Ok(FlatPattern { shape: flat, bends, plane })
}

/// The flat pattern as a cut file sees it: its outline and holes, and its bend lines, in the
/// coordinates of its DXF (see [`flat_dxf`]: the stationary plane, lower-left corner at the
/// origin).
pub fn flat_contours(flat: &FlatPattern) -> Result<(Vec<crate::cutcheck::Contour>, Vec<(P2, P2)>), String> {
  let pl = &flat.plane;
  let n = pl.normal();
  let faces = flat.shape.faces().map_err(|e| e.to_string())?;
  let mut loops: Vec<Vec<P2>> = Vec::new();
  for f in faces.iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Plane && f.normal.dot(n).abs() > 1.0 - 1e-6) {
    if (f.origin - pl.origin).dot(n).abs() > 1e-4 {
      continue;
    }
    let face = flat.shape.face_shape(f.id).map_err(|e| e.to_string())?;
    for l in face.face_loops(0.01).map_err(|e| e.to_string())? {
      loops.push(l.into_iter().map(|q| pl.to_local(q)).collect());
    }
  }
  let area = |l: &[P2]| crate::cutcheck::area(l).abs();
  let outer = (0..loops.len())
    .max_by(|a, b| area(&loops[*a]).total_cmp(&area(&loops[*b])))
    .ok_or("the flat pattern has no face in its plane")?;
  let bends: Vec<(P2, P2)> = flat.bends.iter().map(|b| (pl.to_local(b.start), pl.to_local(b.end))).collect();
  let o = loops
    .iter()
    .flatten()
    .chain(bends.iter().flat_map(|(a, b)| [a, b]))
    .fold([f64::INFINITY; 2], |o, p| [o[0].min(p[0]), o[1].min(p[1])]);
  let shift = |p: P2| [p[0] - o[0], p[1] - o[1]];
  let contours = loops
    .into_iter()
    .enumerate()
    .map(|(i, points)| crate::cutcheck::Contour { points: points.into_iter().map(shift).collect(), hole: i != outer })
    .collect();
  Ok((contours, bends.into_iter().map(|(a, b)| (shift(a), shift(b))).collect()))
}

/// Flat pattern outline and bend lines as a DXF (R12, millimetres) in the stationary face's
/// plane, its lower-left corner at the origin. See [`flat_dxf_for`] for the layers.
pub fn flat_dxf(flat: &FlatPattern) -> Result<String, String> {
  write_flat_dxf(flat, None, &FlatDxfOptions::default())
}

/// The flat pattern DXF of a body with sheet settings `sheet`, layered by role: the outline and
/// holes on `CUT`, bend centre lines on `BEND_UP` / `BEND_DOWN` (dashed), an empty `ETCH` layer
/// for marking, and on `NOTES` a note at each bend (direction, angle, inside radius) and a
/// `SHEET T=… R=… K=…` note below the part (which the flat pattern checks read).
pub fn flat_dxf_for(flat: &FlatPattern, sheet: &SheetInfo) -> Result<String, String> {
  write_flat_dxf(flat, Some(sheet), &FlatDxfOptions::default())
}

/// [`flat_dxf_for`] (or [`flat_dxf`] without a sheet) with Export Flat Pattern's options.
pub fn flat_dxf_with(flat: &FlatPattern, sheet: Option<&SheetInfo>, opts: &FlatDxfOptions) -> Result<String, String> {
  write_flat_dxf(flat, sheet, opts)
}

/// Export Flat Pattern's choices for a DXF: the unit, splines as polylines, and which bend
/// lines go in (Fusion's Visible Geometry: Center Lines and Extent Lines).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatDxfOptions {
  /// Millimetres per drawing unit.
  pub mm_per_unit: f64,
  /// The DXF `$INSUNITS` code of the unit (4 = millimetres).
  pub insunits: u32,
  /// Splines and ellipses written as chains of lines rather than SPLINE entities.
  pub polylines: bool,
  /// Bend centre lines on `BEND_UP` / `BEND_DOWN`.
  pub center_lines: bool,
  /// Bend extent lines (where each bend zone meets the flat faces) on `BEND_EXTENT`; they need
  /// the sheet's bend radius and K factor.
  pub extent_lines: bool,
}

impl Default for FlatDxfOptions {
  /// What the flat pattern DXF has always held: millimetres, splines, centre lines only.
  fn default() -> FlatDxfOptions {
    FlatDxfOptions { mm_per_unit: 1.0, insunits: 4, polylines: false, center_lines: true, extent_lines: false }
  }
}

/// The bend extent lines of a flat pattern: where each bend zone meets the flat faces, either
/// side of its centre line by half the bend allowance (angle x (bend radius + K x thickness)),
/// end to end in model coordinates.
pub fn flat_extent_lines(flat: &FlatPattern, sheet: &SheetInfo) -> Vec<(DVec3, DVec3)> {
  let n = flat.plane.normal();
  let mut out = Vec::new();
  for b in &flat.bends {
    let half = b.angle.abs() * (sheet.bend_radius + sheet.k_factor * sheet.thickness) / 2.0;
    let Some(across) = n.cross(b.end - b.start).try_normalize() else { continue };
    for side in [-half, half] {
      out.push((b.start + across * side, b.end + across * side));
    }
  }
  out
}

/// The layers of a flat pattern DXF: name, colour, line type.
const FLAT_LAYERS: [(&str, u8, &str); 5] = [
  ("CUT", 7, "CONTINUOUS"),
  ("BEND_UP", 1, "DASHED"),
  ("BEND_DOWN", 5, "DASHED"),
  ("ETCH", 3, "CONTINUOUS"),
  ("NOTES", 2, "CONTINUOUS"),
];

fn write_flat_dxf(flat: &FlatPattern, sheet: Option<&SheetInfo>, opts: &FlatDxfOptions) -> Result<String, String> {
  use std::fmt::Write as _;
  let pl = &flat.plane;
  let n = pl.normal();
  let edges = flat.shape.edges().map_err(|e| e.to_string())?;
  let on_plane = |p: DVec3| (p - pl.origin).dot(n).abs() < 1e-4;
  let lines = flat.shape.edge_lines(0.01).map_err(|e| e.to_string())?;
  let faces = flat.shape.faces().map_err(|e| e.to_string())?;
  // a seam between two faces of the flat's top isn't cut
  let top = |id: u32| {
    faces
      .iter()
      .any(|f| f.id == id && f.kind == SurfaceKind::Plane && f.normal.dot(n) > 1.0 - 1e-9 && on_plane(f.origin))
  };
  let mut ents: Vec<Ent> = Vec::new();
  for e in &edges {
    if !(on_plane(e.start) && on_plane(e.end) && on_plane(e.mid)) || e.kind == fox_kernel::CurveKind::Degenerate {
      continue;
    }
    if e.faces[1] != 0 && top(e.faces[0]) && top(e.faces[1]) {
      continue;
    }
    let p = |v: DVec3| pl.to_local(v);
    match e.kind {
      fox_kernel::CurveKind::Line => ents.push(Ent::Line(p(e.start), p(e.end), "CUT")),
      fox_kernel::CurveKind::Circle if (e.start - e.end).length() < 1e-6 => {
        ents.push(Ent::Circle(p(e.center), e.radius))
      }
      fox_kernel::CurveKind::Circle => {
        let (a, b) = if e.axis.dot(n) > 0.0 { (e.start, e.end) } else { (e.end, e.start) };
        let c = p(e.center);
        let ang = |v: DVec3| {
          let d = sub(p(v), c);
          d[1].atan2(d[0]).to_degrees()
        };
        ents.push(Ent::Arc(c, e.radius, ang(a), ang(b)));
      }
      _ if opts.polylines => {
        // Splines to Polylines: the edge's tessellation as lines
        let pts: Vec<P2> = lines.iter().filter(|l| l.id == e.id).flat_map(|l| l.points.iter().map(|q| p(*q))).collect();
        let pts = if pts.len() < 2 { vec![p(e.start), p(e.end)] } else { pts };
        ents.extend(pts.windows(2).map(|w| Ent::Line(w[0], w[1], "CUT")));
      }
      _ => {
        // splines and ellipses as their exact NURBS (SPLINE entities)
        let pts: Vec<P2> = lines.iter().filter(|l| l.id == e.id).flat_map(|l| l.points.iter().map(|q| p(*q))).collect();
        let nurbs = flat.shape.edge_nurbs(e.id).map_err(|e| e.to_string())?;
        let sp = fox_exchange::dxf::Spline {
          degree: nurbs.degree,
          knots: nurbs.knots,
          control: nurbs.poles.iter().map(|q| p(*q)).collect(),
          weights: nurbs.weights,
          fit: Vec::new(),
        };
        if !sp.is_valid() {
          return Err(format!("edge {} of the flat pattern has a bad NURBS", e.id));
        }
        ents.push(Ent::Spline(sp, if pts.is_empty() { vec![p(e.start), p(e.end)] } else { pts }));
      }
    }
  }
  if ents.is_empty() {
    return Err("the flat pattern has no outline".into());
  }
  if opts.center_lines {
    for b in &flat.bends {
      ents.push(Ent::Line(pl.to_local(b.start), pl.to_local(b.end), if b.inner { "BEND_UP" } else { "BEND_DOWN" }));
    }
  }
  if let Some(sheet) = sheet.filter(|_| opts.extent_lines) {
    for (a, b) in flat_extent_lines(flat, sheet) {
      ents.push(Ent::Line(pl.to_local(a), pl.to_local(b), "BEND_EXTENT"));
    }
  }
  // shift to the first quadrant
  let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
  for e in &ents {
    for q in e.extent() {
      x0 = x0.min(q[0]);
      y0 = y0.min(q[1]);
    }
  }
  let o = [x0, y0];
  // notes: one along each bend, and the sheet below the part
  let h = 2.5;
  let mut notes: Vec<(P2, f64, String)> = Vec::new();
  for b in flat.bends.iter().filter(|_| opts.center_lines) {
    let (mut a, mut c) = (pl.to_local(b.start), pl.to_local(b.end));
    // text reads left to right
    if c[0] < a[0] - 1e-9 || ((c[0] - a[0]).abs() <= 1e-9 && c[1] < a[1]) {
      (a, c) = (c, a);
    }
    let l = len2(sub(c, a));
    let Some(u) = unit(sub(c, a)) else { continue };
    let ang = u[1].atan2(u[0]).to_degrees();
    let at = add(add(a, mul(u, (l * 0.1).min(5.0))), mul(left(u), h * 0.4));
    let mut text = format!("{} {}%%d", if b.inner { "UP" } else { "DOWN" }, round3(b.angle.to_degrees()));
    if let Some(sh) = sheet {
      let _ = write!(text, " R{}", round3(sh.bend_radius));
    }
    notes.push((at, ang, text));
  }
  if let Some(sh) = sheet {
    let text = format!("SHEET T={} R={} K={}", round3(sh.thickness), round3(sh.bend_radius), round3(sh.k_factor));
    notes.push(([x0, y0 - 2.0 * h], 0.0, text));
  }
  // in the chosen unit: everything scales about the corner the file starts at
  let (ents, notes, o, h) = if opts.mm_per_unit == 1.0 {
    (ents, notes, o, h)
  } else {
    let k = 1.0 / opts.mm_per_unit;
    let ents = ents.into_iter().map(|e| e.scaled(k)).collect();
    let notes = notes.into_iter().map(|(at, ang, t)| (mul(at, k), ang, t)).collect();
    (ents, notes, mul(o, k), h * k)
  };
  let layers = flat_layers(opts);
  if ents.iter().any(|e| matches!(e, Ent::Spline(..))) {
    return Ok(flat_dxf2000(&layers, &ents, &notes, o, h));
  }
  let mut s =
    format!("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n{}\n0\nENDSEC\n", opts.insunits);
  s.push_str("0\nSECTION\n2\nTABLES\n");
  s.push_str("0\nTABLE\n2\nLTYPE\n70\n2\n");
  s.push_str("0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0\n");
  s.push_str("0\nLTYPE\n2\nDASHED\n70\n0\n3\nDashed __ __\n72\n65\n73\n2\n40\n6\n49\n4\n49\n-2\n");
  s.push_str("0\nENDTAB\n");
  let _ = write!(s, "0\nTABLE\n2\nLAYER\n70\n{}\n", layers.len());
  for &(name, color, ltype) in &layers {
    let _ = write!(s, "0\nLAYER\n2\n{name}\n70\n0\n62\n{color}\n6\n{ltype}\n");
  }
  s.push_str("0\nENDTAB\n0\nENDSEC\n");
  s.push_str("0\nSECTION\n2\nENTITIES\n");
  for e in &ents {
    match *e {
      Ent::Line(a, b, layer) => {
        let (a, b) = (sub(a, o), sub(b, o));
        let _ =
          write!(s, "0\nLINE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", a[0], a[1], b[0], b[1]);
      }
      Ent::Circle(c, r) => {
        let c = sub(c, o);
        let _ = write!(s, "0\nCIRCLE\n8\nCUT\n10\n{}\n20\n{}\n30\n0\n40\n{r}\n", c[0], c[1]);
      }
      Ent::Arc(c, r, a0, a1) => {
        let c = sub(c, o);
        let _ = write!(s, "0\nARC\n8\nCUT\n10\n{}\n20\n{}\n30\n0\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]);
      }
      // flat patterns with splines are written as R2000 (above)
      Ent::Spline(..) => {}
    }
  }
  for (at, ang, text) in &notes {
    let at = sub(*at, o);
    let _ = write!(s, "0\nTEXT\n8\nNOTES\n10\n{}\n20\n{}\n30\n0\n40\n{h}\n1\n{text}\n50\n{ang}\n", at[0], at[1]);
  }
  s.push_str("0\nENDSEC\n0\nEOF\n");
  Ok(s)
}

/// A value rounded to 3 decimals for notes (no trailing zeros).
fn round3(v: f64) -> String {
  let s = format!("{:.3}", v);
  s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The flat pattern DXF as R2000 (AC1015), which SPLINE entities need: the same layers, line
/// types and notes as the R12 file, moved by `-o`.
fn flat_dxf2000(
  layers: &[(&'static str, u8, &'static str)],
  ents: &[Ent],
  notes: &[(P2, f64, String)],
  o: P2,
  h: f64,
) -> String {
  use fox_exchange::dxf::{Entity, Layer, Text};
  let layers: Vec<Layer> =
    layers.iter().map(|(name, color, lt)| Layer { name, color: *color, dashed: *lt == "DASHED" }).collect();
  let out: Vec<(Entity, &str)> = ents
    .iter()
    .map(|e| match e {
      Ent::Line(a, b, layer) => (Entity::Line { a: sub(*a, o), b: sub(*b, o) }, *layer),
      Ent::Circle(c, r) => (Entity::Circle { center: sub(*c, o), radius: *r }, "CUT"),
      Ent::Arc(c, r, a0, a1) => {
        (Entity::Arc { center: sub(*c, o), radius: *r, start: a0.to_radians(), end: a1.to_radians() }, "CUT")
      }
      Ent::Spline(sp, _) => {
        let mut sp = sp.clone();
        sp.control.iter_mut().for_each(|q| *q = sub(*q, o));
        (Entity::Spline(sp), "CUT")
      }
    })
    .collect();
  let texts: Vec<Text> = notes
    .iter()
    .map(|(at, angle, text)| Text { layer: "NOTES", at: sub(*at, o), height: h, angle: *angle, text: text.clone() })
    .collect();
  fox_exchange::dxf::write_dxf2000_layered(&layers, &out, &texts)
}

enum Ent {
  Line(P2, P2, &'static str),
  Circle(P2, f64),
  Arc(P2, f64, f64, f64),
  /// A spline (exact NURBS) and points along it.
  Spline(fox_exchange::dxf::Spline, Vec<P2>),
}

/// The layers a flat pattern DXF declares: [`FLAT_LAYERS`], and `BEND_EXTENT` when it has
/// extent lines.
fn flat_layers(opts: &FlatDxfOptions) -> Vec<(&'static str, u8, &'static str)> {
  let mut out = FLAT_LAYERS.to_vec();
  if opts.extent_lines {
    out.push(("BEND_EXTENT", 8, "CONTINUOUS"));
  }
  out
}

impl Ent {
  /// The entity scaled by `k` about the origin.
  fn scaled(self, k: f64) -> Ent {
    match self {
      Ent::Line(a, b, layer) => Ent::Line(mul(a, k), mul(b, k), layer),
      Ent::Circle(c, r) => Ent::Circle(mul(c, k), r * k),
      Ent::Arc(c, r, a0, a1) => Ent::Arc(mul(c, k), r * k, a0, a1),
      Ent::Spline(mut sp, pts) => {
        sp.control.iter_mut().for_each(|q| *q = mul(*q, k));
        Ent::Spline(sp, pts.into_iter().map(|q| mul(q, k)).collect())
      }
    }
  }

  fn extent(&self) -> Vec<P2> {
    match self {
      Ent::Line(a, b, _) => vec![*a, *b],
      Ent::Circle(c, r) | Ent::Arc(c, r, ..) => vec![sub(*c, [*r, *r]), add(*c, [*r, *r])],
      Ent::Spline(_, pts) => pts.clone(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn contour_l_section_area() {
    let path = [Seg::Line { a: [0.0, 0.0], b: [50.0, 0.0] }, Seg::Line { a: [50.0, 0.0], b: [50.0, 30.0] }];
    let (t, r) = (2.0, 1.0);
    let prof = contour_section(&path, t, r, false).unwrap();
    // straight parts + quarter ring
    let rho = r + t;
    let flats = (50.0 - rho) * t + (30.0 - rho) * t;
    let ring = PI / 4.0 * (rho * rho - r * r);
    assert!((prof.area() - (flats + ring)).abs() < 0.05, "{}", prof.area());
  }

  #[test]
  fn flange_section_area() {
    let (r, t, l) = (1.0, 2.0, 10.0);
    let p = flange_section(r, t, PI / 2.0, l);
    let expect = PI / 4.0 * ((r + t) * (r + t) - r * r) + l * t;
    assert!((p.area() - expect).abs() < 0.05, "{}", p.area());
  }

  #[test]
  fn open_path_orders_segments() {
    let mut s = Sketch::new();
    s.add_line(fox_sketch::PtSpec::At(50.0, 0.0), fox_sketch::PtSpec::At(50.0, 30.0)).unwrap();
    s.add_line(fox_sketch::PtSpec::At(0.0, 0.0), fox_sketch::PtSpec::At(50.0, 0.0)).unwrap();
    let p = open_path(&s, &[]).unwrap();
    assert_eq!(p.len(), 2);
    assert!(len2(sub(p[0].end(), p[1].start())) < 1e-9);
    s.add_line(fox_sketch::PtSpec::At(0.0, 0.0), fox_sketch::PtSpec::At(50.0, 30.0)).unwrap();
    assert!(open_path(&s, &[]).is_err(), "closed triangle");
  }
}
