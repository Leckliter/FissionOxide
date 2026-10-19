//! Sheet metal: rules, the 2D sections of contour and edge flanges, thickness measurement and
//! flat patterns. The open-path and hole-section helpers here also serve Sweep and Hole.
//!
//! Sheet metal bodies are ordinary solids tagged with a [`SheetInfo`] (thickness, bend radius,
//! K-factor). Bends are cylindrical faces, so any feature can keep working on the body, and the
//! kernel's unfolder rolls the bends out by the length of their neutral layer to make the flat
//! pattern.

use std::collections::HashMap;
use std::f64::consts::PI;

use clf_kernel::glam::DVec3;
use clf_kernel::{Bend, FaceInfo, Plane, Shape, SurfaceKind};
use clf_sketch::{CurveId, CurveKind, Loop, Profile, Seg, Sketch};

use crate::SheetMetalRule;

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
  /// The body is a flat pattern (after Unfold).
  pub flat: bool,
}

impl SheetInfo {
  /// Length of the neutral layer through a bend of `angle` radians (the bend allowance).
  pub fn bend_allowance(&self, angle: f64) -> f64 {
    angle.abs() * (self.bend_radius + self.k_factor * self.thickness)
  }
}

/// Evaluates a rule's expressions.
pub fn eval_rule(rule: &SheetMetalRule, params: &HashMap<String, f64>) -> Result<SheetInfo, String> {
  let len = |e: &str| clf_expr::eval(e, clf_expr::Kind::Length, params).map_err(|err| format!("'{e}': {err}"));
  let thickness = len(&rule.thickness)?;
  let bend_radius = len(&rule.bend_radius)?;
  let k_factor = clf_expr::eval(&rule.k_factor, clf_expr::Kind::Unitless, params)
    .map_err(|err| format!("'{}': {err}", rule.k_factor))?;
  if !(thickness > 1e-6 && thickness.is_finite()) {
    return Err(format!("rule {}: the thickness must be positive", rule.name));
  }
  if !(bend_radius >= 0.0 && bend_radius.is_finite()) {
    return Err(format!("rule {}: the bend radius must not be negative", rule.name));
  }
  if !(0.0..=1.0).contains(&k_factor) {
    return Err(format!("rule {}: the K-factor must be between 0 and 1", rule.name));
  }
  Ok(SheetInfo { rule: rule.name.clone(), thickness, bend_radius, k_factor, flat: false })
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
  }
}

// ---------------------------------------------------------------- contour flange

/// The open path of a sketch for a contour flange: the given curves (or every non-construction
/// line and arc), chained end to end.
pub fn open_path(sketch: &Sketch, curves: &[CurveId]) -> Result<Vec<Seg>, String> {
  let mut segs = Vec::new();
  for c in &sketch.curves {
    if !(curves.is_empty() && !c.construction || curves.contains(&c.id)) {
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
    });
  }
  let (Some(p0), Some(p1), Some(q0), Some(q1)) = (p.first(), p.last(), q.first(), q.last()) else {
    return Err("the contour path is empty".into());
  };
  let mut segs = p.clone();
  segs.push(Seg::Line { a: p1.end(), b: q1.end() });
  segs.extend(q.iter().rev().map(|s| s.reversed()));
  segs.push(Seg::Line { a: q0.start(), b: p0.start() });
  let mut outer = Loop { segs };
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
  let mut outer = Loop { segs };
  if outer.signed_area() < 0.0 {
    outer = outer.reversed();
  }
  Profile { outer, holes: Vec::new() }
}

/// An axis-aligned rectangle profile.
pub fn rectangle(lo: P2, hi: P2) -> Profile {
  let (a, b, c, d) = (lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]);
  let segs = vec![Seg::Line { a, b }, Seg::Line { a: b, b: c }, Seg::Line { a: c, b: d }, Seg::Line { a: d, b: a }];
  Profile { outer: Loop { segs }, holes: Vec::new() }
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
  let mut outer = Loop { segs };
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

/// Flat pattern outline and bend lines as a DXF (R12, millimetres) in the stationary face's
/// plane, its lower-left corner at the origin.
pub fn flat_dxf(flat: &FlatPattern) -> Result<String, String> {
  use std::fmt::Write as _;
  let pl = &flat.plane;
  let n = pl.normal();
  let edges = flat.shape.edges().map_err(|e| e.to_string())?;
  let on_plane = |p: DVec3| (p - pl.origin).dot(n).abs() < 1e-4;
  let lines = flat.shape.edge_lines(0.01).map_err(|e| e.to_string())?;
  let mut ents: Vec<Ent> = Vec::new();
  for e in &edges {
    if !(on_plane(e.start) && on_plane(e.end) && on_plane(e.mid)) || e.kind == clf_kernel::CurveKind::Degenerate {
      continue;
    }
    let p = |v: DVec3| pl.to_local(v);
    match e.kind {
      clf_kernel::CurveKind::Line => ents.push(Ent::Line(p(e.start), p(e.end), "OUTLINE")),
      clf_kernel::CurveKind::Circle if (e.start - e.end).length() < 1e-6 => {
        ents.push(Ent::Circle(p(e.center), e.radius))
      }
      clf_kernel::CurveKind::Circle => {
        let (a, b) = if e.axis.dot(n) > 0.0 { (e.start, e.end) } else { (e.end, e.start) };
        let c = p(e.center);
        let ang = |v: DVec3| {
          let d = sub(p(v), c);
          d[1].atan2(d[0]).to_degrees()
        };
        ents.push(Ent::Arc(c, e.radius, ang(a), ang(b)));
      }
      _ => {
        if let Some(l) = lines.iter().find(|l| l.id == e.id) {
          for w in l.points.windows(2) {
            ents.push(Ent::Line(p(w[0]), p(w[1]), "OUTLINE"));
          }
        }
      }
    }
  }
  if ents.is_empty() {
    return Err("the flat pattern has no outline".into());
  }
  for b in &flat.bends {
    ents.push(Ent::Line(pl.to_local(b.start), pl.to_local(b.end), if b.inner { "BEND_UP" } else { "BEND_DOWN" }));
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
  let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n");
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
        let _ = write!(s, "0\nCIRCLE\n8\nOUTLINE\n10\n{}\n20\n{}\n30\n0\n40\n{r}\n", c[0], c[1]);
      }
      Ent::Arc(c, r, a0, a1) => {
        let c = sub(c, o);
        let _ = write!(s, "0\nARC\n8\nOUTLINE\n10\n{}\n20\n{}\n30\n0\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]);
      }
    }
  }
  s.push_str("0\nENDSEC\n0\nEOF\n");
  Ok(s)
}

enum Ent {
  Line(P2, P2, &'static str),
  Circle(P2, f64),
  Arc(P2, f64, f64, f64),
}

impl Ent {
  fn extent(&self) -> Vec<P2> {
    match *self {
      Ent::Line(a, b, _) => vec![a, b],
      Ent::Circle(c, r) | Ent::Arc(c, r, ..) => vec![sub(c, [r, r]), add(c, [r, r])],
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
    s.add_line(clf_sketch::PtSpec::At(50.0, 0.0), clf_sketch::PtSpec::At(50.0, 30.0)).unwrap();
    s.add_line(clf_sketch::PtSpec::At(0.0, 0.0), clf_sketch::PtSpec::At(50.0, 0.0)).unwrap();
    let p = open_path(&s, &[]).unwrap();
    assert_eq!(p.len(), 2);
    assert!(len2(sub(p[0].end(), p[1].start())) < 1e-9);
    s.add_line(clf_sketch::PtSpec::At(0.0, 0.0), clf_sketch::PtSpec::At(50.0, 30.0)).unwrap();
    assert!(open_path(&s, &[]).is_err(), "closed triangle");
  }
}
