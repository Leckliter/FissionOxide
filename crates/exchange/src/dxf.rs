//! DXF: 2D geometry in and out of sketches.
//!
//! Reading handles LINE, CIRCLE, ARC, LWPOLYLINE and POLYLINE (with bulge arcs), POINT, SPLINE
//! and ELLIPSE in the ENTITIES section, the `$INSUNITS` header unit, and entities drawn with a
//! flipped extrusion direction (mirrored OCS, common from CAM tools). Other entities are counted
//! in [`Drawing::skipped`]. Writing produces an R12 (AC1009) file, which every CAD/CAM/laser tool
//! reads, unless there are splines or ellipses: those need R2000 (AC1015).

use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

pub use crate::nurbs::Spline;
use crate::{ExchangeError, Result};

/// One 2D entity in millimetres. Arcs run counter-clockwise from `start` to `end` (radians).
#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
  Line {
    a: [f64; 2],
    b: [f64; 2],
  },
  Circle {
    center: [f64; 2],
    radius: f64,
  },
  Arc {
    center: [f64; 2],
    radius: f64,
    start: f64,
    end: f64,
  },
  Point {
    at: [f64; 2],
  },
  /// A cubic Bézier: start, two control points, end. DXF reading never produces one (SVG does);
  /// DXF writing flattens it to lines.
  Bezier {
    p: [[f64; 2]; 4],
  },
  /// A NURBS curve (DXF SPLINE).
  Spline(Spline),
  /// An elliptical arc: `major` is the end of the major axis relative to the centre, `ratio` the
  /// minor/major radius ratio (at most 1); it runs counter-clockwise from parameter `start` to
  /// `end` (radians; 0 to 2π is the whole ellipse).
  Ellipse {
    center: [f64; 2],
    major: [f64; 2],
    ratio: f64,
    start: f64,
    end: f64,
  },
}

impl Entity {
  pub fn arc_point(center: [f64; 2], radius: f64, angle: f64) -> [f64; 2] {
    [center[0] + radius * angle.cos(), center[1] + radius * angle.sin()]
  }

  /// The point at `t` (0..1) on a cubic Bézier.
  pub fn bezier_point(p: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    [0, 1].map(|k| w[0] * p[0][k] + w[1] * p[1][k] + w[2] * p[2][k] + w[3] * p[3][k])
  }

  /// A whole ellipse (parameters 0 to 2π) with these radii; `major_end` is the end of the
  /// `a`-radius axis relative to the centre. Swaps the axes when `b` is the larger radius, as DXF
  /// wants a ratio of at most 1.
  pub fn ellipse(center: [f64; 2], major_end: [f64; 2], b: f64) -> Entity {
    let a = major_end[0].hypot(major_end[1]);
    let (major, ratio) = if b <= a || a == 0.0 {
      (major_end, if a > 0.0 { b / a } else { 1.0 })
    } else {
      ([-major_end[1] * b / a, major_end[0] * b / a], a / b)
    };
    Entity::Ellipse { center, major, ratio, start: 0.0, end: std::f64::consts::TAU }
  }

  /// Splines and ellipses as cubic Bézier pieces (a Bézier as itself; empty for other entities).
  pub fn beziers(&self) -> Vec<[[f64; 2]; 4]> {
    match self {
      Entity::Bezier { p } => vec![*p],
      Entity::Spline(sp) => sp.beziers(),
      Entity::Ellipse { center, major, ratio, start, end } => {
        crate::nurbs::ellipse_beziers(*center, *major, *ratio, *start, *end)
      }
      _ => Vec::new(),
    }
  }

  /// A whole ellipse rather than an elliptical arc.
  pub fn is_full_ellipse(&self) -> bool {
    match *self {
      Entity::Ellipse { start, end, .. } => !(1e-9..=TAU - 1e-9).contains(&(end - start).rem_euclid(TAU)),
      _ => false,
    }
  }
}

/// A DXF drawing's geometry. `construction` marks entities on a layer named like
/// "construction" (how FissionOxide writes construction geometry).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing {
  pub entities: Vec<(Entity, bool)>,
  /// Entity types that were present but not imported, with counts.
  pub skipped: BTreeMap<String, usize>,
}

fn unit_scale(code: i64) -> f64 {
  match code {
    1 => 25.4,
    2 => 304.8,
    3 => 1_609_344.0,
    5 => 10.0,
    6 => 1000.0,
    7 => 1_000_000.0,
    8 => 25.4e-6,
    9 => 0.0254,
    10 => 914.4,
    11 => 1e-7,
    12 => 1e-6,
    13 => 1e-3,
    14 => 100.0,
    // 0 unitless, 4 millimetres
    _ => 1.0,
  }
}

/// The group-code/value pairs of an entity.
#[derive(Debug, Default)]
struct Raw {
  kind: String,
  pairs: Vec<(i32, String)>,
}

impl Raw {
  fn f(&self, code: i32) -> Option<f64> {
    self.pairs.iter().find(|(c, _)| *c == code).and_then(|(_, v)| v.trim().parse().ok())
  }

  fn s(&self, code: i32) -> Option<&str> {
    self.pairs.iter().find(|(c, _)| *c == code).map(|(_, v)| v.trim())
  }

  fn flipped(&self) -> bool {
    self.f(230).is_some_and(|z| z < 0.0)
  }
}

pub fn read_dxf(bytes: &[u8]) -> Result<Drawing> {
  let text = String::from_utf8_lossy(bytes);
  let mut lines = text.lines();
  let mut pairs: Vec<(i32, String)> = Vec::new();
  while let (Some(c), Some(v)) = (lines.next(), lines.next()) {
    let code: i32 = c.trim().parse().map_err(|_| ExchangeError::fmt(format!("DXF: bad group code '{}'", c.trim())))?;
    pairs.push((code, v.to_string()));
  }
  if pairs.is_empty() {
    return Err(ExchangeError::fmt("DXF: the file is empty (binary DXF isn't supported)"));
  }
  // header unit
  let mut scale = 1.0;
  if let Some(i) = pairs.iter().position(|(c, v)| *c == 9 && v.trim() == "$INSUNITS")
    && let Some((70, v)) = pairs.get(i + 1)
  {
    scale = unit_scale(v.trim().parse().unwrap_or(0));
  }
  // split the ENTITIES section into raw entities
  let mut raws: Vec<Raw> = Vec::new();
  let mut section = String::new();
  let mut i = 0;
  while i < pairs.len() {
    let (c, v) = (&pairs[i].0, pairs[i].1.trim());
    if *c == 0 && v == "SECTION" {
      section = pairs.get(i + 1).map(|p| p.1.trim().to_string()).unwrap_or_default();
      i += 2;
      continue;
    }
    if *c == 0 && v == "ENDSEC" {
      section.clear();
    } else if section == "ENTITIES" {
      if *c == 0 {
        raws.push(Raw { kind: v.to_string(), pairs: Vec::new() });
      } else if let Some(r) = raws.last_mut() {
        r.pairs.push((*c, pairs[i].1.clone()));
      }
    }
    i += 1;
  }

  let mut d = Drawing::default();
  let mut k = 0;
  while k < raws.len() {
    let r = &raws[k];
    let cons = r.s(8).is_some_and(|l| l.to_ascii_lowercase().contains("construction"));
    let fx = if r.flipped() { -1.0 } else { 1.0 };
    let pt = |x: i32, y: i32| -> Option<[f64; 2]> { Some([r.f(x)? * fx * scale, r.f(y)? * scale]) };
    match r.kind.as_str() {
      "LINE" => {
        if let (Some(a), Some(b)) = (pt(10, 20), pt(11, 21)) {
          d.entities.push((Entity::Line { a, b }, cons));
        }
      }
      "CIRCLE" => {
        if let (Some(center), Some(rad)) = (pt(10, 20), r.f(40)) {
          d.entities.push((Entity::Circle { center, radius: rad * scale }, cons));
        }
      }
      "ARC" => {
        if let (Some(center), Some(rad), Some(a0), Some(a1)) = (pt(10, 20), r.f(40), r.f(50), r.f(51)) {
          let (mut s, mut e) = (a0.to_radians(), a1.to_radians());
          if fx < 0.0 {
            (s, e) = (PI - e, PI - s);
          }
          d.entities.push((Entity::Arc { center, radius: rad * scale, start: s, end: e }, cons));
        }
      }
      "POINT" => {
        if let Some(at) = pt(10, 20) {
          d.entities.push((Entity::Point { at }, cons));
        }
      }
      "LWPOLYLINE" => {
        let closed = r.f(70).is_some_and(|f| (f as i64) & 1 == 1);
        let mut verts: Vec<([f64; 2], f64)> = Vec::new();
        let mut x = None;
        for (c, v) in &r.pairs {
          let v: f64 = v.trim().parse().unwrap_or(0.0);
          match c {
            10 => x = Some(v),
            20 => {
              if let Some(xv) = x.take() {
                verts.push(([xv * fx * scale, v * scale], 0.0));
              }
            }
            42 => {
              if let Some(last) = verts.last_mut() {
                last.1 = v * fx;
              }
            }
            _ => {}
          }
        }
        polyline(&mut d, &verts, closed, cons);
      }
      "POLYLINE" => {
        let closed = r.f(70).is_some_and(|f| (f as i64) & 1 == 1);
        let mut verts = Vec::new();
        while k + 1 < raws.len() && raws[k + 1].kind == "VERTEX" {
          k += 1;
          let v = &raws[k];
          if let (Some(x), Some(y)) = (v.f(10), v.f(20)) {
            verts.push(([x * fx * scale, y * scale], v.f(42).unwrap_or(0.0) * fx));
          }
        }
        if raws.get(k + 1).is_some_and(|r| r.kind == "SEQEND") {
          k += 1;
        }
        polyline(&mut d, &verts, closed, cons);
      }
      "SPLINE" => match spline(r, scale) {
        Some(sp) => d.entities.push((Entity::Spline(sp), cons)),
        None => *d.skipped.entry("SPLINE".into()).or_insert(0) += 1,
      },
      "ELLIPSE" => {
        // ELLIPSE points are in world coordinates; a flipped extrusion only reverses the direction
        if let (Some(cx), Some(cy), Some(mx), Some(my), Some(ratio)) = (r.f(10), r.f(20), r.f(11), r.f(21), r.f(40))
          && ratio > 0.0
          && ratio <= 1.0 + 1e-9
          && mx.hypot(my) > 0.0
        {
          let (a0, a1) = (r.f(41).unwrap_or(0.0), r.f(42).unwrap_or(TAU));
          let (start, end) = if fx < 0.0 { (-a1, -a0) } else { (a0, a1) };
          let (center, major) = ([cx * scale, cy * scale], [mx * scale, my * scale]);
          d.entities.push((Entity::Ellipse { center, major, ratio: ratio.min(1.0), start, end }, cons));
        } else {
          *d.skipped.entry("ELLIPSE".into()).or_insert(0) += 1;
        }
      }
      other => *d.skipped.entry(other.to_string()).or_insert(0) += 1,
    }
    k += 1;
  }
  Ok(d)
}

/// A SPLINE's degree, knots, weights, control and fit points (world coordinates), or `None` when
/// they don't make a curve.
fn spline(r: &Raw, scale: f64) -> Option<Spline> {
  let mut sp = Spline { degree: r.f(71)? as usize, ..Spline::default() };
  let (mut cx, mut fx) = (None, None);
  for (c, v) in &r.pairs {
    let Ok(v) = v.trim().parse::<f64>() else { continue };
    match c {
      40 => sp.knots.push(v),
      41 => sp.weights.push(v),
      10 => cx = Some(v),
      20 => {
        if let Some(x) = cx.take() {
          sp.control.push([x * scale, v * scale]);
        }
      }
      11 => fx = Some(v),
      21 => {
        if let Some(x) = fx.take() {
          sp.fit.push([x * scale, v * scale]);
        }
      }
      _ => {}
    }
  }
  if sp.weights.len() != sp.control.len() {
    sp.weights.clear();
  }
  if sp.control.is_empty() {
    // fit points only: the curve is interpolated through them
    sp.knots.clear();
    return (sp.fit.len() >= 2).then_some(sp);
  }
  if sp.knots.is_empty() && sp.control.len() > sp.degree {
    sp = Spline { fit: sp.fit, weights: sp.weights, ..Spline::clamped_uniform(sp.degree, sp.control) };
  }
  sp.is_valid().then_some(sp)
}

/// Splits a polyline with bulges into lines and arcs.
fn polyline(d: &mut Drawing, verts: &[([f64; 2], f64)], closed: bool, cons: bool) {
  let n = verts.len();
  if n < 2 {
    return;
  }
  let segs = if closed { n } else { n - 1 };
  for i in 0..segs {
    let (a, bulge) = verts[i];
    let b = verts[(i + 1) % n].0;
    if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9 {
      continue;
    }
    match bulge_arc(a, b, bulge) {
      Some(arc) => d.entities.push((arc, cons)),
      None => d.entities.push((Entity::Line { a, b }, cons)),
    }
  }
}

/// The arc from `a` to `b` with DXF bulge `tan(angle / 4)` (positive = counter-clockwise).
pub fn bulge_arc(a: [f64; 2], b: [f64; 2], bulge: f64) -> Option<Entity> {
  if bulge.abs() < 1e-9 {
    return None;
  }
  let d = (b[0] - a[0]).hypot(b[1] - a[1]);
  let u = [(b[0] - a[0]) / d, (b[1] - a[1]) / d];
  let left = [-u[1], u[0]];
  let off = d * (1.0 - bulge * bulge) / (4.0 * bulge);
  let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
  let center = [m[0] + left[0] * off, m[1] + left[1] * off];
  let radius = d * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
  let ang = |p: [f64; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
  let (start, end) = if bulge > 0.0 { (ang(a), ang(b)) } else { (ang(b), ang(a)) };
  Some(Entity::Arc { center, radius, start, end })
}

/// Writes entities (millimetres) as a DXF: R12 when lines, circles, arcs and points do, R2000
/// when there are splines or ellipses (written as SPLINE and ELLIPSE entities). Construction
/// entities go on layer "CONSTRUCTION".
pub fn write_dxf(entities: &[(Entity, bool)]) -> String {
  if entities.iter().any(|(e, _)| matches!(e, Entity::Spline(_) | Entity::Ellipse { .. })) {
    return write_dxf2000(entities);
  }
  use std::fmt::Write as _;
  let mut s = String::new();
  let mut pair = |c: i32, v: &str| {
    let _ = write!(s, "{c:>3}\n{v}\n");
  };
  pair(999, "FissionOxide DXF export (mm)");
  pair(0, "SECTION");
  pair(2, "HEADER");
  pair(9, "$ACADVER");
  pair(1, "AC1009");
  pair(9, "$INSUNITS");
  pair(70, "4");
  pair(0, "ENDSEC");
  pair(0, "SECTION");
  pair(2, "TABLES");
  pair(0, "TABLE");
  pair(2, "LAYER");
  pair(70, "2");
  for (name, color) in [("0", "7"), ("CONSTRUCTION", "3")] {
    pair(0, "LAYER");
    pair(2, name);
    pair(70, "0");
    pair(62, color);
    pair(6, "CONTINUOUS");
  }
  pair(0, "ENDTAB");
  pair(0, "ENDSEC");
  pair(0, "SECTION");
  pair(2, "ENTITIES");
  let num = |v: f64| {
    let v = if v.abs() < 1e-12 { 0.0 } else { v };
    format!("{v:.9}").trim_end_matches('0').trim_end_matches('.').to_string()
  };
  for (e, cons) in entities {
    let layer = if *cons { "CONSTRUCTION" } else { "0" };
    match *e {
      Entity::Line { a, b } => {
        pair(0, "LINE");
        pair(8, layer);
        for (c, v) in [(10, a[0]), (20, a[1]), (30, 0.0), (11, b[0]), (21, b[1]), (31, 0.0)] {
          pair(c, &num(v));
        }
      }
      Entity::Circle { center, radius } => {
        pair(0, "CIRCLE");
        pair(8, layer);
        for (c, v) in [(10, center[0]), (20, center[1]), (30, 0.0), (40, radius)] {
          pair(c, &num(v));
        }
      }
      Entity::Arc { center, radius, start, end } => {
        pair(0, "ARC");
        pair(8, layer);
        let deg = |a: f64| a.rem_euclid(TAU).to_degrees();
        for (c, v) in [(10, center[0]), (20, center[1]), (30, 0.0), (40, radius), (50, deg(start)), (51, deg(end))] {
          pair(c, &num(v));
        }
      }
      Entity::Point { at } => {
        pair(0, "POINT");
        pair(8, layer);
        for (c, v) in [(10, at[0]), (20, at[1]), (30, 0.0)] {
          pair(c, &num(v));
        }
      }
      Entity::Bezier { .. } | Entity::Spline(_) | Entity::Ellipse { .. } => {
        // R12 has no splines: short lines
        const STEPS: usize = 16;
        for p in e.beziers() {
          for i in 0..STEPS {
            let a = Entity::bezier_point(&p, i as f64 / STEPS as f64);
            let b = Entity::bezier_point(&p, (i + 1) as f64 / STEPS as f64);
            pair(0, "LINE");
            pair(8, layer);
            for (c, v) in [(10, a[0]), (20, a[1]), (30, 0.0), (11, b[0]), (21, b[1]), (31, 0.0)] {
              pair(c, &num(v));
            }
          }
        }
      }
    }
  }
  pair(0, "ENDSEC");
  pair(0, "EOF");
  s
}

fn num(v: f64) -> String {
  let v = if v.abs() < 1e-12 { 0.0 } else { v };
  format!("{v:.12}").trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Group-code/value output with a handle counter, for R2000 files.
struct Out {
  s: String,
  next: u32,
}

impl Out {
  fn pair(&mut self, c: i32, v: &str) {
    use std::fmt::Write as _;
    let _ = write!(self.s, "{c:>3}\n{v}\n");
  }

  fn pairs(&mut self, list: &[(i32, &str)]) {
    for (c, v) in list {
      self.pair(*c, v);
    }
  }

  fn handle(&mut self) -> String {
    self.next += 1;
    format!("{:X}", self.next - 1)
  }

  fn point(&mut self, c: i32, p: [f64; 2]) {
    self.pair(c, &num(p[0]));
    self.pair(c + 10, &num(p[1]));
    self.pair(c + 20, "0");
  }

  fn table(&mut self, name: &str, h: &str, n: usize) {
    self.pairs(&[(0, "TABLE"), (2, name), (5, h), (330, "0"), (100, "AcDbSymbolTable"), (70, &n.to_string())]);
  }

  /// A table record's common part: type, handle, owner (the table) and subclass markers.
  fn record(&mut self, kind: &str, owner: &str, class: &str) {
    let h = self.handle();
    let code = if kind == "DIMSTYLE" { 105 } else { 5 };
    self.pairs(&[(0, kind), (code, &h), (330, owner), (100, "AcDbSymbolTableRecord"), (100, class)]);
  }

  /// An entity's common part, owned by model space.
  fn entity(&mut self, kind: &str, owner: &str, layer: &str, class: &str) {
    let h = self.handle();
    self.pairs(&[(0, kind), (5, &h), (330, owner), (100, "AcDbEntity"), (8, layer), (100, class)]);
  }

  fn spline(&mut self, sp: &Spline) {
    self.pairs(&[(210, "0"), (220, "0"), (230, "1")]);
    let rational = sp.is_rational();
    self.pair(70, if rational { "12" } else { "8" });
    self.pair(71, &sp.degree.to_string());
    self.pair(72, &sp.knots.len().to_string());
    self.pair(73, &sp.control.len().to_string());
    self.pair(74, &sp.fit.len().to_string());
    self.pairs(&[(42, "0.0000000001"), (43, "0.0000000001")]);
    if !sp.fit.is_empty() {
      self.pair(44, "0.0000000001");
    }
    for k in &sp.knots {
      self.pair(40, &num(*k));
    }
    if rational {
      for w in &sp.weights {
        self.pair(41, &num(*w));
      }
    }
    for p in &sp.control {
      self.point(10, *p);
    }
    for p in &sp.fit {
      self.point(11, *p);
    }
  }
}

/// An R2000 (AC1015) file: the tables, blocks and objects AutoCAD expects of that version, with
/// handles and owners, and SPLINE / ELLIPSE entities.
fn write_dxf2000(entities: &[(Entity, bool)]) -> String {
  let layers = [Layer { name: "0", color: 7, dashed: false }, Layer { name: "CONSTRUCTION", color: 3, dashed: false }];
  let ents: Vec<(Entity, &str)> =
    entities.iter().map(|(e, cons)| (e.clone(), if *cons { "CONSTRUCTION" } else { "0" })).collect();
  write_dxf2000_layered(&layers, &ents, &[])
}

/// A layer of a layered DXF: its name, colour (AutoCAD colour index) and line type (`dashed`:
/// DASHED, else continuous).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer<'a> {
  pub name: &'a str,
  pub color: u8,
  pub dashed: bool,
}

/// A single-line TEXT entity: its layer, insertion point, height, angle (degrees) and text.
#[derive(Debug, Clone, PartialEq)]
pub struct Text<'a> {
  pub layer: &'a str,
  pub at: [f64; 2],
  pub height: f64,
  pub angle: f64,
  pub text: String,
}

/// An R2000 (AC1015) file with named layers (the `0` layer is always there), entities on them and
/// TEXT entities: for writers that need SPLINEs and layers both (flat pattern DXFs).
pub fn write_dxf2000_layered(layers: &[Layer], entities: &[(Entity, &str)], texts: &[Text]) -> String {
  let mut o = Out { s: String::new(), next: 0x10 };
  let [vport, ltype, layer, style, view, ucs, appid, dimstyle, block_rec] = std::array::from_fn(|_| o.handle());
  let (ms, ps, root, groups) = (o.handle(), o.handle(), o.handle(), o.handle());

  o.pairs(&[(0, "SECTION"), (2, "CLASSES"), (0, "ENDSEC")]);
  o.pairs(&[(0, "SECTION"), (2, "TABLES")]);
  o.table("VPORT", &vport, 1);
  o.record("VPORT", &vport, "AcDbViewportTableRecord");
  o.pairs(&[
    (2, "*Active"),
    (70, "0"),
    (10, "0"),
    (20, "0"),
    (11, "1"),
    (21, "1"),
    (12, "0"),
    (22, "0"),
    (13, "0"),
    (23, "0"),
    (14, "10"),
    (24, "10"),
    (15, "10"),
    (25, "10"),
    (16, "0"),
    (26, "0"),
    (36, "1"),
    (17, "0"),
    (27, "0"),
    (37, "0"),
    (40, "1000"),
    (41, "1.5"),
    (42, "50"),
    (43, "0"),
    (44, "0"),
    (50, "0"),
    (51, "0"),
    (71, "0"),
    (72, "100"),
    (73, "1"),
    (74, "3"),
    (75, "0"),
    (76, "0"),
    (77, "0"),
    (78, "0"),
    (0, "ENDTAB"),
  ]);
  let dashed = layers.iter().any(|l| l.dashed);
  o.table("LTYPE", &ltype, 3 + usize::from(dashed));
  for (name, desc) in [("ByBlock", ""), ("ByLayer", ""), ("Continuous", "Solid line")] {
    o.record("LTYPE", &ltype, "AcDbLinetypeTableRecord");
    o.pairs(&[(2, name), (70, "0"), (3, desc), (72, "65"), (73, "0"), (40, "0")]);
  }
  if dashed {
    o.record("LTYPE", &ltype, "AcDbLinetypeTableRecord");
    o.pairs(&[(2, "DASHED"), (70, "0"), (3, "Dashed __ __"), (72, "65"), (73, "2"), (40, "6")]);
    o.pairs(&[(49, "4"), (74, "0"), (49, "-2"), (74, "0")]);
  }
  o.pair(0, "ENDTAB");
  let mut all = vec![Layer { name: "0", color: 7, dashed: false }];
  all.extend(layers.iter().filter(|l| l.name != "0"));
  if let Some(l) = layers.iter().find(|l| l.name == "0") {
    all[0] = *l;
  }
  o.table("LAYER", &layer, all.len());
  for l in &all {
    o.record("LAYER", &layer, "AcDbLayerTableRecord");
    let color = l.color.to_string();
    let lt = if l.dashed { "DASHED" } else { "Continuous" };
    o.pairs(&[(2, l.name), (70, "0"), (62, &color), (6, lt), (370, "-3")]);
  }
  o.pair(0, "ENDTAB");
  o.table("STYLE", &style, 1);
  o.record("STYLE", &style, "AcDbTextStyleTableRecord");
  o.pairs(&[(2, "Standard"), (70, "0"), (40, "0"), (41, "1"), (50, "0"), (71, "0"), (42, "2.5"), (3, "txt"), (4, "")]);
  o.pair(0, "ENDTAB");
  for (name, h) in [("VIEW", &view), ("UCS", &ucs)] {
    o.table(name, h, 0);
    o.pair(0, "ENDTAB");
  }
  o.table("APPID", &appid, 1);
  o.record("APPID", &appid, "AcDbRegAppTableRecord");
  o.pairs(&[(2, "ACAD"), (70, "0"), (0, "ENDTAB")]);
  o.table("DIMSTYLE", &dimstyle, 1);
  o.pairs(&[(100, "AcDbDimStyleTable"), (71, "0")]);
  o.record("DIMSTYLE", &dimstyle, "AcDbDimStyleTableRecord");
  o.pairs(&[(2, "Standard"), (70, "0"), (0, "ENDTAB")]);
  o.table("BLOCK_RECORD", &block_rec, 2);
  for (name, h) in [("*Model_Space", &ms), ("*Paper_Space", &ps)] {
    o.pairs(&[
      (0, "BLOCK_RECORD"),
      (5, h),
      (330, &block_rec),
      (100, "AcDbSymbolTableRecord"),
      (100, "AcDbBlockTableRecord"),
      (2, name),
    ]);
  }
  o.pairs(&[(0, "ENDTAB"), (0, "ENDSEC")]);

  o.pairs(&[(0, "SECTION"), (2, "BLOCKS")]);
  for (name, owner) in [("*Model_Space", &ms), ("*Paper_Space", &ps)] {
    o.entity("BLOCK", owner, "0", "AcDbBlockBegin");
    o.pairs(&[(2, name), (70, "0"), (10, "0"), (20, "0"), (30, "0"), (3, name), (1, "")]);
    o.entity("ENDBLK", owner, "0", "AcDbBlockEnd");
  }
  o.pair(0, "ENDSEC");

  o.pairs(&[(0, "SECTION"), (2, "ENTITIES")]);
  for (e, layer) in entities {
    let layer = *layer;
    match e {
      Entity::Line { a, b } => {
        o.entity("LINE", &ms, layer, "AcDbLine");
        o.point(10, *a);
        o.point(11, *b);
      }
      Entity::Circle { center, radius } => {
        o.entity("CIRCLE", &ms, layer, "AcDbCircle");
        o.point(10, *center);
        o.pair(40, &num(*radius));
      }
      Entity::Arc { center, radius, start, end } => {
        o.entity("ARC", &ms, layer, "AcDbCircle");
        o.point(10, *center);
        o.pair(40, &num(*radius));
        o.pair(100, "AcDbArc");
        o.pair(50, &num(start.rem_euclid(TAU).to_degrees()));
        o.pair(51, &num(end.rem_euclid(TAU).to_degrees()));
      }
      Entity::Point { at } => {
        o.entity("POINT", &ms, layer, "AcDbPoint");
        o.point(10, *at);
      }
      Entity::Ellipse { center, major, ratio, start, end } => {
        o.entity("ELLIPSE", &ms, layer, "AcDbEllipse");
        o.point(10, *center);
        o.point(11, *major);
        o.pairs(&[(210, "0"), (220, "0"), (230, "1")]);
        o.pair(40, &num(*ratio));
        let a0 = start.rem_euclid(TAU);
        let sweep = (end - start).rem_euclid(TAU);
        let sweep = if sweep < 1e-12 { TAU } else { sweep };
        o.pair(41, &num(a0));
        o.pair(42, &num(a0 + sweep));
      }
      Entity::Bezier { p } => {
        o.entity("SPLINE", &ms, layer, "AcDbSpline");
        o.spline(&Spline::from_beziers(&[*p]));
      }
      Entity::Spline(sp) => {
        o.entity("SPLINE", &ms, layer, "AcDbSpline");
        o.spline(sp);
      }
    }
  }
  for t in texts {
    o.entity("TEXT", &ms, t.layer, "AcDbText");
    o.point(10, t.at);
    o.pair(40, &num(t.height));
    o.pair(1, &t.text);
    o.pair(50, &num(t.angle));
    o.pair(100, "AcDbText");
  }
  o.pair(0, "ENDSEC");

  o.pairs(&[(0, "SECTION"), (2, "OBJECTS")]);
  o.pairs(&[(0, "DICTIONARY"), (5, &root), (330, "0"), (100, "AcDbDictionary"), (281, "1")]);
  o.pairs(&[(3, "ACAD_GROUP"), (350, &groups)]);
  o.pairs(&[(0, "DICTIONARY"), (5, &groups), (330, &root), (100, "AcDbDictionary"), (281, "1")]);
  o.pairs(&[(0, "ENDSEC"), (0, "EOF")]);

  let mut head = Out { s: String::new(), next: 0 };
  head.pair(999, "FissionOxide DXF export (mm)");
  head.pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1015"), (9, "$HANDSEED")]);
  head.pair(5, &format!("{:X}", o.next));
  head.pairs(&[(9, "$INSUNITS"), (70, "4"), (9, "$MEASUREMENT"), (70, "1"), (0, "ENDSEC")]);
  head.s + &o.s
}
