//! Shop checks for flat-pattern DXFs: what a laser, plasma or waterjet shop looks at before it
//! cuts a part.
//!
//! Entities are sorted by the role of their layer ([`Role`]): `CUT` (and any layer without a
//! known role) is cut, `BEND_UP` / `BEND_DOWN` hold bend centre lines, `ETCH` is marked but not
//! cut, `NOTES` holds text. The checks:
//!
//! - every cut contour is closed and doesn't branch;
//! - no cut entity is drawn twice (the head would cut it twice);
//! - no contour is narrower than the minimum feature size, and no web between two contours is;
//! - holes keep a safe distance from bend lines (they distort when the bend is formed).
//!
//! FissionOxide's flat-pattern DXFs write a `SHEET T=… R=… K=…` note, from which the default
//! rules ([`Rules::for_sheet`]) are taken when none are given.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use serde::Serialize;

use crate::Result;
use crate::dxf::{Entity, read_dxf};

/// The role of a flat-pattern layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
  Cut,
  BendUp,
  BendDown,
  Etch,
  Note,
  /// Construction geometry: ignored.
  Construction,
}

/// The role a layer name stands for (case-insensitive; unknown layers are cut).
pub fn role(layer: &str) -> Role {
  let l = layer.trim().to_ascii_uppercase();
  if l.starts_with("BEND") {
    if l.contains("DOWN") { Role::BendDown } else { Role::BendUp }
  } else if l.starts_with("ETCH") || l.starts_with("MARK") || l.starts_with("SCRIBE") {
    Role::Etch
  } else if l.starts_with("NOTE") || l.starts_with("TEXT") {
    Role::Note
  } else if l.contains("CONSTRUCTION") {
    Role::Construction
  } else {
    Role::Cut
  }
}

/// The shop rules a flat pattern is checked against (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rules {
  /// The narrowest hole, slot or web that cuts cleanly.
  pub min_feature: f64,
  /// The least distance from a hole's edge to a bend line.
  pub hole_to_bend: f64,
  /// Points closer than this are the same point.
  pub tol: f64,
}

impl Rules {
  /// Common rules for a sheet `thickness` thick bent with inside radius `bend_radius`: features
  /// at least a thickness wide, holes at least 2·t + R from a bend line.
  pub fn for_sheet(thickness: f64, bend_radius: f64) -> Rules {
    Rules { min_feature: thickness, hole_to_bend: 2.0 * thickness + bend_radius, tol: 1e-3 }
  }
}

/// What a check found wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
  /// Nothing on a cut layer forms a closed contour.
  NoOutline,
  /// A cut contour has a loose end.
  OpenContour,
  /// More than two cut entities meet at a point.
  Branch,
  /// A cut entity overlaps another one.
  Duplicate,
  /// A contour is narrower than the minimum feature size.
  SmallFeature,
  /// Two contours come closer than the minimum feature size.
  ThinWeb,
  /// A hole is too close to a bend line.
  HoleNearBend,
  /// The file holds more than one part (outer contour).
  SeveralParts,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
  pub kind: IssueKind,
  /// Where (file coordinates, mm).
  pub at: [f64; 2],
  pub message: String,
}

/// The result of checking a flat pattern.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
  pub issues: Vec<Issue>,
  /// Closed cut contours: part outlines and holes.
  pub contours: usize,
  pub holes: usize,
  pub bends: usize,
  pub etch: usize,
  /// TEXT / MTEXT entities.
  pub notes: usize,
  /// Entities per layer.
  pub layers: BTreeMap<String, usize>,
  /// The rules the file was checked against.
  pub rules: Rules,
}

impl Report {
  pub fn ok(&self) -> bool {
    self.issues.is_empty()
  }
}

/// The entities of a DXF with their layers, and its text entities (layer, text).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layered {
  pub entities: Vec<(String, Entity)>,
  pub texts: Vec<(String, String)>,
}

/// Reads a DXF keeping each entity's layer.
pub fn read_layered(bytes: &[u8]) -> Result<Layered> {
  let text = String::from_utf8_lossy(bytes);
  let mut lines = text.lines();
  let mut pairs: Vec<(i32, String)> = Vec::new();
  while let (Some(c), Some(v)) = (lines.next(), lines.next()) {
    let Ok(code) = c.trim().parse::<i32>() else {
      return Err(crate::ExchangeError::fmt(format!("DXF: bad group code '{}'", c.trim())));
    };
    pairs.push((code, v.trim_end_matches('\r').to_string()));
  }
  let units = pairs
    .iter()
    .position(|(c, v)| *c == 9 && v.trim() == "$INSUNITS")
    .and_then(|i| pairs.get(i + 1))
    .filter(|(c, _)| *c == 70)
    .map(|(_, v)| v.trim().to_string())
    .unwrap_or_else(|| "4".into());
  // the ENTITIES section, one group per entity (a POLYLINE takes its VERTEXes and SEQEND)
  let mut groups: Vec<Vec<(i32, String)>> = Vec::new();
  let mut section = String::new();
  let mut i = 0;
  while i < pairs.len() {
    let (c, v) = (pairs[i].0, pairs[i].1.trim());
    if c == 0 && v == "SECTION" {
      section = pairs.get(i + 1).map(|p| p.1.trim().to_string()).unwrap_or_default();
      i += 2;
      continue;
    }
    if c == 0 && v == "ENDSEC" {
      section.clear();
    } else if section == "ENTITIES" {
      let joins =
        c == 0 && matches!(v, "VERTEX" | "SEQEND") && groups.last().is_some_and(|g| g[0].1.trim() == "POLYLINE");
      if c == 0 && !joins {
        groups.push(Vec::new());
      }
      if let Some(g) = groups.last_mut() {
        g.push(pairs[i].clone());
      }
    }
    i += 1;
  }
  let mut out = Layered::default();
  for g in groups {
    let kind = g[0].1.trim().to_string();
    let layer = g.iter().find(|(c, _)| *c == 8).map(|(_, v)| v.trim().to_string()).unwrap_or_else(|| "0".into());
    if kind == "TEXT" || kind == "MTEXT" {
      let s = g.iter().find(|(c, _)| *c == 1).map(|(_, v)| v.clone()).unwrap_or_default();
      out.texts.push((layer, s));
      continue;
    }
    // each entity on its own through the full reader, so its layer stays known
    let mut mini = format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (c, v) in &g {
      mini.push_str(&format!("{c}\n{v}\n"));
    }
    mini.push_str("0\nENDSEC\n0\nEOF\n");
    for (e, _) in read_dxf(mini.as_bytes())?.entities {
      out.entities.push((layer.clone(), e));
    }
  }
  Ok(out)
}

/// The `SHEET T=… R=…` note FissionOxide writes: (thickness, bend radius).
pub fn sheet_note(texts: &[(String, String)]) -> Option<(f64, f64)> {
  texts.iter().find_map(|(_, s)| {
    let s = s.trim();
    let rest = s.strip_prefix("SHEET")?;
    let value = |key: &str| -> Option<f64> {
      rest.split_whitespace().find_map(|w| w.strip_prefix(key)).and_then(|v| v.trim_end_matches("mm").parse().ok())
    };
    Some((value("T=")?, value("R=")?))
  })
}

/// Checks a flat-pattern DXF. Without `rules`, they come from the file's `SHEET` note (or a 1 mm
/// sheet when it has none).
pub fn check_dxf(bytes: &[u8], rules: Option<Rules>) -> Result<Report> {
  let layered = read_layered(bytes)?;
  let rules = rules.unwrap_or_else(|| {
    let (t, r) = sheet_note(&layered.texts).unwrap_or((1.0, 1.0));
    Rules::for_sheet(t, r)
  });
  Ok(check(&layered, rules))
}

// ---------------------------------------------------------------- geometry

type P2 = [f64; 2];

fn dist(a: P2, b: P2) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Distance from p to the segment ab.
fn seg_point(a: P2, b: P2, p: P2) -> f64 {
  let d = [b[0] - a[0], b[1] - a[1]];
  let l2 = d[0] * d[0] + d[1] * d[1];
  if l2 < 1e-24 {
    return dist(a, p);
  }
  let k = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
  dist([a[0] + d[0] * k, a[1] + d[1] * k], p)
}

fn cross(o: P2, a: P2, b: P2) -> f64 {
  (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Distance between segments ab and cd.
fn seg_seg(a: P2, b: P2, c: P2, d: P2) -> f64 {
  let (d1, d2, d3, d4) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
  if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
    return 0.0;
  }
  seg_point(a, b, c).min(seg_point(a, b, d)).min(seg_point(c, d, a)).min(seg_point(c, d, b))
}

/// Least distance between two polylines (and the point on `a` where it is).
fn poly_poly(a: &[P2], b: &[P2]) -> (f64, P2) {
  let mut best = (f64::INFINITY, a.first().copied().unwrap_or([0.0, 0.0]));
  for wa in a.windows(2) {
    for wb in b.windows(2) {
      let d = seg_seg(wa[0], wa[1], wb[0], wb[1]);
      if d < best.0 {
        best = (d, wa[0]);
      }
    }
  }
  best
}

fn inside(poly: &[P2], p: P2) -> bool {
  let mut c = false;
  let n = poly.len();
  for i in 0..n {
    let (a, b) = (poly[i], poly[(i + 1) % n]);
    if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
      c = !c;
    }
  }
  c
}

fn area(poly: &[P2]) -> f64 {
  let n = poly.len();
  (0..n).map(|i| poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1]).sum::<f64>().abs() / 2.0
}

/// The least width of a point set over all directions (1° steps, then refined).
fn min_width(pts: &[P2]) -> f64 {
  let width = |a: f64| {
    let (c, s) = (a.cos(), a.sin());
    let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
      let v = p[0] * c + p[1] * s;
      (lo.min(v), hi.max(v))
    });
    hi - lo
  };
  let step = std::f64::consts::PI / 180.0;
  let (mut best_a, mut best) = (0.0, f64::INFINITY);
  for k in 0..180 {
    let a = k as f64 * step;
    let w = width(a);
    if w < best {
      (best_a, best) = (a, w);
    }
  }
  for k in -20..=20 {
    best = best.min(width(best_a + k as f64 * step / 20.0));
  }
  best
}

/// Sweep of an arc running counter-clockwise from `start` to `end`.
fn sweep(start: f64, end: f64) -> f64 {
  let s = (end - start).rem_euclid(TAU);
  if s < 1e-12 { TAU } else { s }
}

/// A cut entity: its ends (None when closed by itself) and points along it.
struct Piece {
  ends: Option<(P2, P2)>,
  pts: Vec<P2>,
}

fn piece(e: &Entity) -> Option<Piece> {
  let arc_pts = |c: P2, r: f64, a0: f64, sw: f64| -> Vec<P2> {
    let n = ((sw / (TAU / 144.0)).ceil() as usize).max(4);
    (0..=n).map(|k| Entity::arc_point(c, r, a0 + sw * k as f64 / n as f64)).collect()
  };
  Some(match e {
    Entity::Line { a, b } => Piece { ends: Some((*a, *b)), pts: vec![*a, *b] },
    Entity::Circle { center, radius } => Piece { ends: None, pts: arc_pts(*center, *radius, 0.0, TAU) },
    Entity::Arc { center, radius, start, end } => {
      let pts = arc_pts(*center, *radius, *start, sweep(*start, *end));
      let (a, b) = (pts[0], pts[pts.len() - 1]);
      Piece { ends: if dist(a, b) < 1e-9 { None } else { Some((a, b)) }, pts }
    }
    Entity::Point { .. } => return None,
    Entity::Bezier { .. } | Entity::Spline(_) | Entity::Ellipse { .. } => {
      let mut pts: Vec<P2> = Vec::new();
      for p in e.beziers() {
        for k in 0..=16 {
          let q = Entity::bezier_point(&p, k as f64 / 16.0);
          if pts.last().is_none_or(|l| dist(*l, q) > 1e-12) {
            pts.push(q);
          }
        }
      }
      let (a, b) = (*pts.first()?, *pts.last()?);
      Piece { ends: if dist(a, b) < 1e-9 { None } else { Some((a, b)) }, pts }
    }
  })
}

/// Where two cut entities overlap (along a stretch longer than `tol`), if they do.
fn overlap(x: &Entity, y: &Entity, tol: f64) -> Option<P2> {
  match (x, y) {
    (Entity::Line { a, b }, Entity::Line { a: c, b: d }) => {
      let l = dist(*a, *b);
      if l < tol || seg_point(*a, *b, *c).max(seg_point(*a, *b, *d)) > tol {
        return None;
      }
      // both on one line: overlap of the projections
      let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
      let along = |p: &P2| (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
      let (c0, c1) = (along(c).min(along(d)), along(c).max(along(d)));
      let (lo, hi) = (c0.max(0.0), c1.min(l));
      // collinear but apart, or perpendicular-ish within tol: not an overlap
      if hi - lo <= tol || (along(c) - along(d)).abs() < tol {
        return None;
      }
      let m = (lo + hi) / 2.0;
      Some([a[0] + u[0] * m, a[1] + u[1] * m])
    }
    (Entity::Circle { .. } | Entity::Arc { .. }, Entity::Circle { .. } | Entity::Arc { .. }) => {
      let span = |e: &Entity| match *e {
        Entity::Circle { center, radius } => (center, radius, 0.0, TAU),
        Entity::Arc { center, radius, start, end } => (center, radius, start, sweep(start, end)),
        _ => ([0.0, 0.0], 0.0, 0.0, 0.0),
      };
      let (c1, r1, s1, w1) = span(x);
      let (c2, r2, s2, w2) = span(y);
      if dist(c1, c2) > tol || (r1 - r2).abs() > tol || r1 < tol {
        return None;
      }
      // angular overlap of the two ranges
      let eps = tol / r1;
      let off = (s2 - s1).rem_euclid(TAU);
      let a = if off < w1 - eps {
        Some(s2 + (w1 - off).min(w2) / 2.0)
      } else if (TAU - off) < w2 - eps {
        Some(s1 + (w2 - (TAU - off)).min(w1) / 2.0)
      } else {
        None
      };
      a.map(|a| Entity::arc_point(c1, r1, a))
    }
    _ => None,
  }
}

/// Checks layered flat-pattern geometry against `rules`.
pub fn check(layered: &Layered, rules: Rules) -> Report {
  let tol = rules.tol;
  let mut issues = Vec::new();
  let mut layers: BTreeMap<String, usize> = BTreeMap::new();
  for (l, _) in &layered.entities {
    *layers.entry(l.clone()).or_default() += 1;
  }
  for (l, _) in &layered.texts {
    *layers.entry(l.clone()).or_default() += 1;
  }
  let cut: Vec<&Entity> = layered.entities.iter().filter(|(l, _)| role(l) == Role::Cut).map(|(_, e)| e).collect();
  let bends: Vec<(P2, P2)> = layered
    .entities
    .iter()
    .filter(|(l, _)| matches!(role(l), Role::BendUp | Role::BendDown))
    .filter_map(|(_, e)| match e {
      Entity::Line { a, b } => Some((*a, *b)),
      _ => None,
    })
    .collect();
  let etch = layered.entities.iter().filter(|(l, _)| role(l) == Role::Etch).count();

  // duplicates
  for i in 0..cut.len() {
    for j in i + 1..cut.len() {
      if let Some(at) = overlap(cut[i], cut[j], tol) {
        issues.push(Issue { kind: IssueKind::Duplicate, at, message: "two cut entities overlap here".into() });
      }
    }
  }

  // contours: join the pieces' ends
  let pieces: Vec<Piece> = cut.iter().filter_map(|e| piece(e)).collect();
  let mut nodes: Vec<P2> = Vec::new();
  let mut node = |p: P2| -> usize {
    match nodes.iter().position(|q| dist(*q, p) <= tol) {
      Some(i) => i,
      None => {
        nodes.push(p);
        nodes.len() - 1
      }
    }
  };
  let ends: Vec<Option<(usize, usize)>> = pieces.iter().map(|p| p.ends.map(|(a, b)| (node(a), node(b)))).collect();
  let mut degree = vec![0usize; nodes.len()];
  for (a, b) in ends.iter().flatten() {
    degree[*a] += 1;
    degree[*b] += 1;
  }
  for (i, d) in degree.iter().enumerate() {
    if *d == 1 {
      issues.push(Issue { kind: IssueKind::OpenContour, at: nodes[i], message: "a cut contour is open here".into() });
    } else if *d > 2 {
      issues.push(Issue { kind: IssueKind::Branch, at: nodes[i], message: format!("{d} cut entities meet here") });
    }
  }
  // walk the closed loops
  let mut loops: Vec<Vec<P2>> = pieces.iter().filter(|p| p.ends.is_none()).map(|p| p.pts.clone()).collect();
  let mut used = vec![false; pieces.len()];
  for start in 0..pieces.len() {
    let Some((first, mut at)) = ends[start] else { continue };
    if used[start] || degree[first] != 2 {
      continue;
    }
    let mut pts = pieces[start].pts.clone();
    let mut chain = vec![start];
    used[start] = true;
    let mut closed = false;
    while degree[at] == 2 {
      if at == first {
        closed = true;
        break;
      }
      let Some(k) = (0..pieces.len()).find(|&k| !used[k] && ends[k].is_some_and(|(a, b)| a == at || b == at)) else {
        break;
      };
      let Some((a, b)) = ends[k] else { break };
      used[k] = true;
      chain.push(k);
      if a == at {
        pts.extend(pieces[k].pts.iter().skip(1));
        at = b;
      } else {
        pts.extend(pieces[k].pts.iter().rev().skip(1));
        at = a;
      }
    }
    if closed {
      loops.push(pts);
    }
  }
  if loops.is_empty() {
    issues.push(Issue { kind: IssueKind::NoOutline, at: [0.0, 0.0], message: "no closed cut contour".into() });
  }
  // nesting: an even depth is a part outline, an odd one a hole
  let areas: Vec<f64> = loops.iter().map(|l| area(l)).collect();
  let depth: Vec<usize> = (0..loops.len())
    .map(|i| {
      (0..loops.len())
        .filter(|&j| j != i && areas[j] > areas[i] && loops[i].iter().all(|p| inside(&loops[j], *p)))
        .count()
    })
    .collect();
  let parts = depth.iter().filter(|d| **d % 2 == 0).count();
  if parts > 1 {
    let at = loops.iter().zip(&depth).filter(|(_, d)| **d % 2 == 0).nth(1).map(|(l, _)| l[0]).unwrap_or([0.0, 0.0]);
    issues.push(Issue {
      kind: IssueKind::SeveralParts,
      at,
      message: format!("{parts} parts in one file (one flat pattern per file)"),
    });
  }
  let holes = depth.iter().filter(|d| **d % 2 == 1).count();
  for (i, l) in loops.iter().enumerate() {
    let w = min_width(l);
    if w < rules.min_feature - tol {
      let what = if depth[i] % 2 == 1 { "a hole" } else { "a part" };
      issues.push(Issue {
        kind: IssueKind::SmallFeature,
        at: l[0],
        message: format!("{what} {w:.3} mm across is under the minimum feature size {:.3} mm", rules.min_feature),
      });
    }
    for (j, m) in loops.iter().enumerate().skip(i + 1) {
      let (d, at) = poly_poly(l, m);
      if d < rules.min_feature - tol {
        issues.push(Issue {
          kind: IssueKind::ThinWeb,
          at,
          message: format!(
            "contours {} and {} are {d:.3} mm apart (minimum feature size {:.3} mm)",
            i + 1,
            j + 1,
            rules.min_feature
          ),
        });
      }
    }
    if depth[i] % 2 == 1 {
      for (a, b) in &bends {
        let (d, at) = poly_poly(l, &[*a, *b]);
        if d < rules.hole_to_bend - tol {
          issues.push(Issue {
            kind: IssueKind::HoleNearBend,
            at,
            message: if d <= tol {
              "a hole crosses a bend line".to_string()
            } else {
              format!("a hole is {d:.3} mm from a bend line (at least {:.3} mm)", rules.hole_to_bend)
            },
          });
        }
      }
    }
  }
  Report { issues, contours: loops.len(), holes, bends: bends.len(), etch, notes: layered.texts.len(), layers, rules }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn line(a: P2, b: P2) -> (String, Entity) {
    ("CUT".into(), Entity::Line { a, b })
  }

  fn rect(lo: P2, hi: P2) -> Vec<(String, Entity)> {
    let (a, b, c, d) = (lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]);
    vec![line(a, b), line(b, c), line(c, d), line(d, a)]
  }

  fn plate() -> Layered {
    let mut entities = rect([0.0, 0.0], [100.0, 50.0]);
    entities.push(("CUT".into(), Entity::Circle { center: [30.0, 25.0], radius: 3.0 }));
    entities.push(("BEND_UP".into(), Entity::Line { a: [0.0, 40.0], b: [100.0, 40.0] }));
    Layered { entities, texts: vec![("NOTES".into(), "SHEET T=2 R=2 K=0.44".into())] }
  }

  #[test]
  fn a_clean_plate_passes() {
    let r = check(&plate(), Rules::for_sheet(2.0, 2.0));
    assert!(r.ok(), "{:?}", r.issues);
    assert_eq!((r.contours, r.holes, r.bends, r.notes), (2, 1, 1, 1));
  }

  #[test]
  fn finds_each_problem() {
    let rules = Rules::for_sheet(2.0, 2.0);
    let kinds = |l: &Layered| check(l, rules).issues.iter().map(|i| i.kind).collect::<Vec<_>>();
    // open: drop an edge
    let mut l = plate();
    l.entities.remove(1);
    assert_eq!(kinds(&l).iter().filter(|k| **k == IssueKind::OpenContour).count(), 2);
    // duplicate: a line drawn twice (reversed) and an arc over the circle
    let mut l = plate();
    l.entities.push(line([100.0, 0.0], [0.0, 0.0]));
    l.entities.push(("CUT".into(), Entity::Arc { center: [30.0, 25.0], radius: 3.0, start: 1.0, end: 2.0 }));
    let k = kinds(&l);
    assert_eq!(k.iter().filter(|k| **k == IssueKind::Duplicate).count(), 2, "{k:?}");
    assert!(k.contains(&IssueKind::Branch));
    // a hole under the minimum size, one near the bend, one too close to the edge
    let mut l = plate();
    l.entities.push(("CUT".into(), Entity::Circle { center: [60.0, 20.0], radius: 0.5 }));
    l.entities.push(("CUT".into(), Entity::Circle { center: [80.0, 36.0], radius: 1.5 }));
    l.entities.push(("CUT".into(), Entity::Circle { center: [90.0, 2.5], radius: 1.5 }));
    let k = kinds(&l);
    assert_eq!(k.iter().filter(|k| **k == IssueKind::SmallFeature).count(), 1, "{k:?}");
    assert_eq!(k.iter().filter(|k| **k == IssueKind::HoleNearBend).count(), 1, "{k:?}");
    assert_eq!(k.iter().filter(|k| **k == IssueKind::ThinWeb).count(), 1, "{k:?}");
    // two parts
    let mut l = plate();
    l.entities.extend(rect([200.0, 0.0], [220.0, 20.0]));
    assert_eq!(kinds(&l), vec![IssueKind::SeveralParts]);
    // nothing closed
    let l = Layered { entities: vec![line([0.0, 0.0], [1.0, 0.0])], texts: Vec::new() };
    assert!(kinds(&l).contains(&IssueKind::NoOutline));
  }

  #[test]
  fn reads_layers_and_the_sheet_note() {
    let dxf = "0\nSECTION\n2\nENTITIES\n\
      0\nLINE\n8\nCUT\n10\n0\n20\n0\n11\n10\n21\n0\n\
      0\nLWPOLYLINE\n8\nCUT\n90\n3\n70\n0\n10\n10\n20\n0\n10\n10\n20\n10\n10\n0\n20\n10\n\
      0\nLINE\n8\nCUT\n10\n0\n20\n10\n11\n0\n21\n0\n\
      0\nLINE\n8\nBEND_DOWN\n10\n0\n20\n5\n11\n10\n21\n5\n\
      0\nTEXT\n8\nNOTES\n10\n1\n20\n1\n40\n1\n1\nSHEET T=0.5 R=0.25 K=0.4\n\
      0\nENDSEC\n0\nEOF\n";
    let l = read_layered(dxf.as_bytes()).unwrap();
    assert_eq!(l.entities.len(), 5);
    assert_eq!(role(&l.entities[4].0), Role::BendDown);
    assert_eq!(sheet_note(&l.texts), Some((0.5, 0.25)));
    let r = check_dxf(dxf.as_bytes(), None).unwrap();
    assert!(r.ok(), "{:?}", r.issues);
    assert_eq!(r.rules.min_feature, 0.5);
    assert_eq!((r.contours, r.bends), (1, 1));
  }
}
