//! DXF: 2D geometry in and out of sketches.
//!
//! Reading handles LINE, CIRCLE, ARC, LWPOLYLINE and POLYLINE (with bulge arcs) and POINT in the
//! ENTITIES section, the `$INSUNITS` header unit, and entities drawn with a flipped extrusion
//! direction (mirrored OCS, common from CAM tools). Other entities are counted in
//! [`Drawing::skipped`]. Writing produces an R12 (AC1009) file, which every CAD/CAM/laser tool
//! reads.

use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

use crate::{ExchangeError, Result};

/// One 2D entity in millimetres. Arcs run counter-clockwise from `start` to `end` (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Entity {
  Line { a: [f64; 2], b: [f64; 2] },
  Circle { center: [f64; 2], radius: f64 },
  Arc { center: [f64; 2], radius: f64, start: f64, end: f64 },
  Point { at: [f64; 2] },
}

impl Entity {
  pub fn arc_point(center: [f64; 2], radius: f64, angle: f64) -> [f64; 2] {
    [center[0] + radius * angle.cos(), center[1] + radius * angle.sin()]
  }
}

/// A DXF drawing's geometry. `construction` marks entities on a layer named like
/// "construction" (how CLFusion writes construction geometry).
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
      other => *d.skipped.entry(other.to_string()).or_insert(0) += 1,
    }
    k += 1;
  }
  Ok(d)
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

/// Writes entities (millimetres) as an R12 DXF. Construction entities go on layer
/// "CONSTRUCTION".
pub fn write_dxf(entities: &[(Entity, bool)]) -> String {
  use std::fmt::Write as _;
  let mut s = String::new();
  let mut pair = |c: i32, v: &str| {
    let _ = write!(s, "{c:>3}\n{v}\n");
  };
  pair(999, "CLFusion DXF export (mm)");
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
    }
  }
  pair(0, "ENDSEC");
  pair(0, "EOF");
  s
}
