//! Sketch clean-up: Remove Duplicates (copies of curves lying on top of each other) and Clean Up
//! (an imported or projected sketch made DXF-clean: links broken, constraints and stray points
//! gone, gaps closed, splines turned into lines or arcs). The LumenArt add-in's Line Cleaner and
//! Sketch Cleaner, as core tools. Tolerances are lengths in model units, compared as distances.

use std::collections::{HashMap, HashSet};
use std::mem::{Discriminant, discriminant};

use fox_model::{FeatureKind, SketchLink};
use fox_sketch::{ConstraintKind, CurveId, CurveKind, ORIGIN, PointId, PtSpec, Seg, Sketch};
use serde_json::{Value, json};

use super::{opt_expr, opt_str, params_scope, sketch_id};
use crate::{CmdError, CmdResult, Session};

type P = [f64; 2];

/// Standard tolerance: 0.001".
const DEFAULT_TOL: &str = "0.001 in";

pub(super) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "sketch.remove_duplicates" => remove_duplicates(s, p),
    "sketch.cleanup" => cleanup(s, p),
    _ => return None,
  })
}

fn flag(p: &Value, k: &str, default: bool) -> bool {
  p.get(k).and_then(Value::as_bool).unwrap_or(default)
}

/// A non-negative length (number in mm or expression), `default` when missing.
fn tolerance(s: &Session, p: &Value, k: &str, default: &str) -> Result<f64, CmdError> {
  let e = opt_expr(p, k).unwrap_or_else(|| default.to_string());
  let v = fox_expr::eval(&e, fox_expr::Kind::Length, &params_scope(s))?;
  if !(v.is_finite() && v >= 0.0) {
    return Err(CmdError::new(format!("{k}: a length of zero or more")));
  }
  Ok(v)
}

fn dist(a: P, b: P) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Curves the tools leave alone: glyphs of editable text and spun profiles (deleting one of
/// those deletes the whole text or profile).
fn owned(sketch: &Sketch, id: CurveId) -> bool {
  sketch.text_of(id).is_some() || sketch.spun_of(id).is_some()
}

// ---------------------------------------------------------------- remove duplicates

/// What a duplicate is compared by, at the positions when the command ran.
enum Geo {
  Line { a: P, b: P, ia: PointId, ib: PointId },
  Arc { c: P, r: f64, s: P, e: P, m: P },
  Circle { c: P, r: f64 },
  /// Splines and Bézier chains: their defining points, and those that only match in order
  /// (tangent and curvature handles).
  Points { pts: Vec<P>, fixed_order: Vec<P>, degree: u8 },
  Conic { s: P, e: P, apex: P, rho: f64 },
  Ellipse { c: P, major: P, minor: f64 },
  EllArc { c: P, major: P, minor: f64, s: P, e: P },
}

struct Item {
  id: CurveId,
  kind: Discriminant<CurveKind>,
  class: &'static str,
  geo: Geo,
  gone: bool,
}

/// The report's group of a curve kind.
fn class(kind: &CurveKind) -> &'static str {
  match kind {
    CurveKind::Line { .. } => "lines",
    CurveKind::Arc { .. } => "arcs",
    CurveKind::Circle { .. } => "circles",
    CurveKind::Ellipse { .. } | CurveKind::EllipticalArc { .. } => "ellipses",
    CurveKind::Spline { .. }
    | CurveKind::ControlSpline { .. }
    | CurveKind::Bezier { .. }
    | CurveKind::QuinticSpline { .. }
    | CurveKind::Conic { .. } => "splines",
  }
}

const CLASSES: [&str; 5] = ["lines", "arcs", "circles", "splines", "ellipses"];

fn geo(kind: &CurveKind, pos: &HashMap<PointId, P>) -> Option<Geo> {
  let at = |id: &PointId| pos.get(id).copied();
  let all = |ids: &[PointId]| ids.iter().map(at).collect::<Option<Vec<P>>>();
  Some(match kind {
    CurveKind::Line { a, b } => Geo::Line { a: at(a)?, b: at(b)?, ia: *a, ib: *b },
    CurveKind::Circle { center, radius } => Geo::Circle { c: at(center)?, r: *radius },
    CurveKind::Arc { center, start, end } => {
      let (c, s, e) = (at(center)?, at(start)?, at(end)?);
      let r = dist(c, s);
      let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
      let mut sweep = (e[1] - c[1]).atan2(e[0] - c[0]) - a0;
      if sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
      }
      let am = a0 + sweep / 2.0;
      Geo::Arc { c, r, s, e, m: [c[0] + r * am.cos(), c[1] + r * am.sin()] }
    }
    CurveKind::Spline { points, start_handle, end_handle, start_curvature, end_curvature } => {
      let handles: Vec<PointId> =
        [start_handle, end_handle, start_curvature, end_curvature].into_iter().flatten().copied().collect();
      Geo::Points { pts: all(points)?, fixed_order: all(&handles)?, degree: 0 }
    }
    CurveKind::ControlSpline { points, degree } => Geo::Points { pts: all(points)?, fixed_order: vec![], degree: *degree },
    CurveKind::Bezier { points } | CurveKind::QuinticSpline { points } => {
      Geo::Points { pts: all(points)?, fixed_order: vec![], degree: 0 }
    }
    CurveKind::Conic { start, end, apex, rho } => Geo::Conic { s: at(start)?, e: at(end)?, apex: at(apex)?, rho: *rho },
    CurveKind::Ellipse { center, major, minor } => Geo::Ellipse { c: at(center)?, major: at(major)?, minor: *minor },
    CurveKind::EllipticalArc { center, major, minor, start, end } => {
      Geo::EllArc { c: at(center)?, major: at(major)?, minor: *minor, s: at(start)?, e: at(end)? }
    }
  })
}

/// How the second of two lines relates to the first.
#[derive(Debug, PartialEq)]
enum Overlap {
  None,
  /// The second lies on the first (or is a copy of it): it goes.
  SecondInFirst,
  /// The first lies on the longer second: the first goes.
  FirstInSecond,
  /// They overlap part way: the second is cut back to the first's end `keep` (its point
  /// `moved` is replaced by it).
  Partial { moved: PointId, keep: PointId },
}

fn lines(first: (P, P, PointId, PointId), second: (P, P, PointId, PointId), tol: f64) -> Overlap {
  let (a1, b1, ia1, ib1) = first;
  let (a2, b2, ia2, ib2) = second;
  if (dist(a1, a2) <= tol && dist(b1, b2) <= tol) || (dist(a1, b2) <= tol && dist(b1, a2) <= tol) {
    return Overlap::SecondInFirst;
  }
  let (l1, l2) = (dist(a1, b1), dist(a2, b2));
  if l1 <= tol || l2 <= tol {
    return Overlap::None;
  }
  // collinear: each line's ends within tol of the other line
  let off = |p: P, a: P, b: P, l: f64| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / l;
  if off(a2, a1, b1, l1) > tol || off(b2, a1, b1, l1) > tol || off(a1, a2, b2, l2) > tol || off(b1, a2, b2, l2) > tol {
    return Overlap::None;
  }
  let u = [(b1[0] - a1[0]) / l1, (b1[1] - a1[1]) / l1];
  let t = |p: P| (p[0] - a1[0]) * u[0] + (p[1] - a1[1]) * u[1];
  let (ta, tb) = (t(a2), t(b2));
  let (lo, hi) = (ta.min(tb), ta.max(tb));
  // touching end to end (a corner or a continuation) is not an overlap
  if hi.min(l1) - lo.max(0.0) <= tol {
    return Overlap::None;
  }
  if lo >= -tol && hi <= l1 + tol {
    return Overlap::SecondInFirst;
  }
  if lo <= tol && hi >= l1 - tol {
    return Overlap::FirstInSecond;
  }
  // the second's end inside the first moves to the first's end inside the second
  let end_at_hi = |hi_end: bool| if (ta > tb) == hi_end { ia2 } else { ib2 };
  if lo < 0.0 {
    // it sticks out before a1: its end at hi is inside the first
    Overlap::Partial { moved: end_at_hi(true), keep: ia1 }
  } else {
    // it sticks out past b1: its end at lo is inside the first
    Overlap::Partial { moved: end_at_hi(false), keep: ib1 }
  }
}

fn same_pts(a: &[P], b: &[P], tol: f64) -> bool {
  a.len() == b.len() && a.iter().zip(b).all(|(p, q)| dist(*p, *q) <= tol)
}

fn same_pts_either_way(a: &[P], b: &[P], tol: f64) -> bool {
  let rev: Vec<P> = b.iter().rev().copied().collect();
  same_pts(a, b, tol) || same_pts(a, &rev, tol)
}

/// Is the second curve a duplicate of the first (lines aside)?
fn duplicate(g1: &Geo, g2: &Geo, tol: f64) -> bool {
  let near = |a: P, b: P| dist(a, b) <= tol;
  match (g1, g2) {
    (Geo::Circle { c: c1, r: r1 }, Geo::Circle { c: c2, r: r2 }) => near(*c1, *c2) && (r1 - r2).abs() <= tol,
    (Geo::Arc { c: c1, r: r1, s: s1, e: e1, m: m1 }, Geo::Arc { c: c2, r: r2, s: s2, e: e2, m: m2 }) => {
      // either way round, as long as both cover the same side of the circle
      near(*c1, *c2)
        && (r1 - r2).abs() <= tol
        && ((near(*s1, *s2) && near(*e1, *e2)) || (near(*s1, *e2) && near(*e1, *s2)))
        && near(*m1, *m2)
    }
    (
      Geo::Points { pts: p1, fixed_order: h1, degree: d1 },
      Geo::Points { pts: p2, fixed_order: h2, degree: d2 },
    ) => {
      d1 == d2
        && if h1.is_empty() && h2.is_empty() {
          same_pts_either_way(p1, p2, tol)
        } else {
          same_pts(p1, p2, tol) && same_pts(h1, h2, tol)
        }
    }
    (Geo::Conic { s: s1, e: e1, apex: a1, rho: r1 }, Geo::Conic { s: s2, e: e2, apex: a2, rho: r2 }) => {
      near(*a1, *a2)
        && (r1 - r2).abs() <= 1e-9
        && ((near(*s1, *s2) && near(*e1, *e2)) || (near(*s1, *e2) && near(*e1, *s2)))
    }
    (Geo::Ellipse { c: c1, major: m1, minor: n1 }, Geo::Ellipse { c: c2, major: m2, minor: n2 }) => {
      // the major axis end may be either end
      let other = [2.0 * c2[0] - m2[0], 2.0 * c2[1] - m2[1]];
      near(*c1, *c2) && (n1 - n2).abs() <= tol && (near(*m1, *m2) || near(*m1, other))
    }
    (
      Geo::EllArc { c: c1, major: m1, minor: n1, s: s1, e: e1 },
      Geo::EllArc { c: c2, major: m2, minor: n2, s: s2, e: e2 },
    ) => {
      let other = [2.0 * c2[0] - m2[0], 2.0 * c2[1] - m2[1]];
      near(*c1, *c2)
        && (n1 - n2).abs() <= tol
        && (near(*m1, *m2) || near(*m1, other))
        && near(*s1, *s2)
        && near(*e1, *e2)
    }
    _ => false,
  }
}

/// What Remove Duplicates did (or would do) to a sketch.
#[derive(Default)]
struct Found {
  removed: Vec<(CurveId, &'static str)>,
  trimmed: Vec<CurveId>,
}

/// Finds the duplicates in `sketch` and removes them from it, keeping the first of each pair.
fn find_duplicates(sketch: &mut Sketch, tol: f64, construction: bool) -> Result<Found, CmdError> {
  let pos: HashMap<PointId, P> = sketch.points.iter().map(|p| (p.id, [p.x, p.y])).collect();
  let mut items: Vec<Item> = sketch
    .curves
    .iter()
    .filter(|c| (construction || !c.construction) && !owned(sketch, c.id))
    .filter_map(|c| {
      Some(Item { id: c.id, kind: discriminant(&c.kind), class: class(&c.kind), geo: geo(&c.kind, &pos)?, gone: false })
    })
    .collect();
  let mut found = Found::default();
  // the second line of a partial overlap: its new end
  let mut cut: Vec<(usize, PointId, PointId)> = Vec::new();
  for i in 0..items.len() {
    if items[i].gone {
      continue;
    }
    for j in i + 1..items.len() {
      if items[j].gone || items[j].kind != items[i].kind {
        continue;
      }
      if let (Geo::Line { a: a1, b: b1, ia: ia1, ib: ib1 }, Geo::Line { a: a2, b: b2, ia: ia2, ib: ib2 }) =
        (&items[i].geo, &items[j].geo)
      {
        match lines((*a1, *b1, *ia1, *ib1), (*a2, *b2, *ia2, *ib2), tol) {
          Overlap::None => {}
          Overlap::SecondInFirst => items[j].gone = true,
          Overlap::FirstInSecond => {
            items[i].gone = true;
            break;
          }
          Overlap::Partial { moved, keep } => {
            let k = pos[&keep];
            if let Geo::Line { a, b, ia, ib } = &mut items[j].geo {
              if *ia == moved {
                (*a, *ia) = (k, keep);
              } else {
                (*b, *ib) = (k, keep);
              }
            }
            cut.push((j, moved, keep));
          }
        }
      } else if duplicate(&items[i].geo, &items[j].geo, tol) {
        items[j].gone = true;
      }
    }
  }
  for (j, moved, keep) in cut {
    if items[j].gone {
      continue;
    }
    let id = items[j].id;
    if let Some(CurveKind::Line { a, b }) = sketch.curve_mut(id).map(|c| &mut c.kind) {
      if *a == moved {
        *a = keep;
      } else if *b == moved {
        *b = keep;
      }
    }
    // its length changed: a dimension on it no longer holds
    sketch.constraints.retain(|c| !(c.kind.dim().is_some() && c.kind.curves().contains(&id)));
    let used = moved == ORIGIN || sketch.curves.iter().any(|c| fox_sketch::curve_points(&c.kind).contains(&moved));
    if !used {
      sketch.remove_point(moved)?;
    }
    found.trimmed.push(id);
  }
  for it in items.iter().filter(|it| it.gone) {
    sketch.remove_curve(it.id)?;
    found.removed.push((it.id, it.class));
  }
  found.trimmed.retain(|id| sketch.curve(*id).is_some());
  Ok(found)
}

fn remove_duplicates(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let tol = tolerance(s, p, "tolerance", DEFAULT_TOL)?;
  let construction = flag(p, "include_construction", false);
  let dry_run = flag(p, "dry_run", false);
  let mut sketch = s.design.sketch(sk)?.clone();
  let found = find_duplicates(&mut sketch, tol, construction)?;
  let changed = !found.removed.is_empty() || !found.trimmed.is_empty();
  if changed && !dry_run {
    s.edit(move |d| {
      if !sketch.solve().ok() {
        return Err(CmdError::new("the result conflicts with the sketch's constraints"));
      }
      if let Some(FeatureKind::Sketch { sketch: old, links, .. }) = d.feature_mut(sk).map(|f| &mut f.kind) {
        links.retain(|l| sketch.curve(l.curve).is_some() || sketch.curve3d(l.curve).is_some());
        *old = sketch;
      }
      Ok(())
    })?;
  }
  let mut counts = serde_json::Map::new();
  for c in CLASSES {
    counts.insert(c.into(), json!(found.removed.iter().filter(|r| r.1 == c).count()));
  }
  let parts: Vec<String> = CLASSES
    .iter()
    .map(|c| (c, found.removed.iter().filter(|r| r.1 == *c).count()))
    .filter(|(_, n)| *n > 0)
    .map(|(c, n)| format!("{n} {}", if n == 1 { c.trim_end_matches('s') } else { c }))
    .collect();
  let mut summary = if parts.is_empty() { "No duplicates".to_string() } else { parts.join(", ") };
  if !found.trimmed.is_empty() {
    summary.push_str(&format!("; {} overlapping line(s) cut back", found.trimmed.len()));
  }
  if !parts.is_empty() || !found.trimmed.is_empty() {
    summary.push_str(if dry_run { " to remove" } else { " removed" });
  }
  Ok(json!({
    "removed": found.removed.len(),
    "counts": counts,
    "curves": found.removed.iter().map(|r| r.0.0).collect::<Vec<_>>(),
    "trimmed": found.trimmed.iter().map(|c| c.0).collect::<Vec<_>>(),
    "dry_run": dry_run,
    "tolerance": tol,
    "summary": summary,
  }))
}

// ---------------------------------------------------------------- clean up

#[derive(Clone, Copy, PartialEq)]
enum Constraints {
  Projected,
  All,
  None,
}

#[derive(Clone, Copy, PartialEq)]
enum SplinesTo {
  Lines,
  Arcs,
}

struct Options {
  break_links: bool,
  constraints: Constraints,
  dimensions: bool,
  purge_points: bool,
  construction_to_normal: bool,
  merge_gaps: Option<f64>,
  splines_to: Option<SplinesTo>,
  segments: usize,
  spline_tolerance: f64,
}

fn options(s: &Session, p: &Value) -> Result<Options, CmdError> {
  let constraints = match opt_str(p, "delete_constraints").unwrap_or("projected") {
    "projected" => Constraints::Projected,
    "all" => Constraints::All,
    "none" => Constraints::None,
    o => return Err(CmdError::new(format!("delete_constraints '{o}': projected, all or none"))),
  };
  // merge_gaps: off (null, false), on with the standard tolerance (true, then gap_tolerance), or
  // the tolerance itself
  let merge_gaps = match p.get("merge_gaps") {
    None | Some(Value::Null) | Some(Value::Bool(false)) => None,
    Some(Value::Bool(true)) => Some(tolerance(s, p, "gap_tolerance", DEFAULT_TOL)?),
    Some(_) => Some(tolerance(s, p, "merge_gaps", DEFAULT_TOL)?),
  };
  let splines_to = match p.get("splines_to") {
    None | Some(Value::Null) => None,
    Some(v) => match v.as_str() {
      Some("none") | Some("") => None,
      Some("lines") => Some(SplinesTo::Lines),
      Some("arcs") => Some(SplinesTo::Arcs),
      _ => return Err(CmdError::new("splines_to: lines, arcs or null")),
    },
  };
  let segments = match p.get("segments") {
    None | Some(Value::Null) => 64,
    Some(v) => {
      let n = v.as_u64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok()));
      match n {
        Some(n @ 4..=512) => n as usize,
        _ => return Err(CmdError::new("segments: a whole number from 4 to 512")),
      }
    }
  };
  let spline_tolerance = tolerance(s, p, "spline_tolerance", DEFAULT_TOL)?;
  if splines_to == Some(SplinesTo::Arcs) && spline_tolerance <= 0.0 {
    return Err(CmdError::new("spline_tolerance: more than zero"));
  }
  Ok(Options {
    break_links: flag(p, "break_links", true),
    constraints,
    dimensions: flag(p, "delete_dimensions", false),
    purge_points: flag(p, "purge_points", true),
    construction_to_normal: flag(p, "construction_to_normal", false),
    merge_gaps,
    splines_to,
    segments,
    spline_tolerance,
  })
}

#[derive(Default)]
struct Cleaned {
  links_broken: usize,
  constraints: usize,
  dimensions: usize,
  construction: usize,
  points: usize,
  gaps: usize,
  splines: usize,
  added: Vec<CurveId>,
}

fn cleanup(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let o = options(s, p)?;
  let r = s.edit(|d| {
    let Some(FeatureKind::Sketch { sketch, links, .. }) = d.feature_mut(sk).map(|f| &mut f.kind) else {
      return Err(CmdError::new("not a sketch"));
    };
    clean(sketch, links, &o)
  })?;
  let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0);
  Ok(json!({
    "links_broken": r.links_broken,
    "constraints_deleted": r.constraints,
    "dimensions_deleted": r.dimensions,
    "construction_to_normal": r.construction,
    "points_purged": r.points,
    "gaps_merged": r.gaps,
    "splines_converted": r.splines,
    "curves": r.added.iter().map(|c| c.0).collect::<Vec<_>>(),
    "profiles": profiles,
  }))
}

fn clean(sketch: &mut Sketch, links: &mut Vec<SketchLink>, o: &Options) -> Result<Cleaned, CmdError> {
  let mut r = Cleaned::default();
  // projected geometry: linked to a body edge, a spun profile's, or held where it is by fixed
  // points only (a silhouette projected without a link)
  let fixed: HashSet<PointId> = sketch.points.iter().filter(|p| p.fixed && p.id != ORIGIN).map(|p| p.id).collect();
  let linked: HashSet<CurveId> = links.iter().map(|l| l.curve).collect();
  let projected: HashSet<CurveId> = sketch
    .curves
    .iter()
    .filter(|c| {
      let pts: Vec<PointId> = fox_sketch::curve_points(&c.kind).into_iter().filter(|p| *p != ORIGIN).collect();
      linked.contains(&c.id)
        || sketch.spun_of(c.id).is_some()
        || (!pts.is_empty() && pts.iter().all(|p| fixed.contains(p)))
    })
    .map(|c| c.id)
    .collect();
  let projected_points: HashSet<PointId> = sketch
    .curves
    .iter()
    .filter(|c| projected.contains(&c.id))
    .flat_map(|c| fox_sketch::curve_points(&c.kind))
    .filter(|p| *p != ORIGIN)
    .collect();

  if o.break_links {
    r.links_broken = links.len() + sketch.spuns.len();
    links.clear();
    // spun profiles' curves stay as plain curves
    sketch.spuns.clear();
    for c in &mut sketch.curves3d {
      if c.source.take().is_some() {
        r.links_broken += 1;
      }
    }
    for pt in sketch.points.iter_mut().filter(|p| projected_points.contains(&p.id)) {
      pt.fixed = false;
    }
  }

  let doomed: Vec<(fox_sketch::ConstraintId, bool)> = sketch
    .constraints
    .iter()
    .filter_map(|c| {
      let dim = c.kind.dim().is_some();
      let touches = || {
        c.kind.curves().iter().any(|x| projected.contains(x))
          || c.kind.points().iter().any(|x| projected_points.contains(x))
      };
      let go = match o.constraints {
        // everything holding projected geometry, its dimensions too
        Constraints::Projected => touches(),
        Constraints::All => !dim,
        Constraints::None => false,
      } || (dim && o.dimensions);
      go.then_some((c.id, dim))
    })
    .collect();
  for (id, dim) in doomed {
    // deleting a polygon constraint can take others with it
    if sketch.constraint(id).is_some() {
      sketch.remove_constraint(id)?;
      if dim {
        r.dimensions += 1;
      } else {
        r.constraints += 1;
      }
    }
  }

  if o.construction_to_normal {
    for c in sketch.curves.iter_mut().filter(|c| c.construction) {
      c.construction = false;
      r.construction += 1;
    }
  }

  if o.purge_points {
    let mut used: HashSet<PointId> = sketch.curves.iter().flat_map(|c| fox_sketch::curve_points(&c.kind)).collect();
    used.insert(ORIGIN);
    for t in &sketch.texts {
      used.insert(t.anchor);
      used.extend(t.points.iter().copied());
    }
    let stray: Vec<PointId> = sketch.points.iter().map(|p| p.id).filter(|p| !used.contains(p)).collect();
    for pt in stray {
      if sketch.point(pt).is_some() {
        sketch.remove_point(pt)?;
        r.points += 1;
      }
    }
  }

  if let Some(tol) = o.merge_gaps {
    r.gaps = merge_gaps(sketch, tol)?;
  }

  if let Some(to) = o.splines_to {
    let splines: Vec<CurveId> = sketch
      .curves
      .iter()
      .filter(|c| {
        matches!(
          c.kind,
          CurveKind::Spline { .. }
            | CurveKind::ControlSpline { .. }
            | CurveKind::Bezier { .. }
            | CurveKind::QuinticSpline { .. }
            | CurveKind::Conic { .. }
        ) && !owned(sketch, c.id)
      })
      .map(|c| c.id)
      .collect();
    for id in splines {
      if let Some(made) = convert_spline(sketch, id, to, o)? {
        r.splines += 1;
        r.added.extend(made);
      }
    }
  }

  if (r.gaps > 0 || r.splines > 0) && !sketch.solve().ok() {
    return Err(CmdError::new("the cleaned sketch conflicts with its constraints (delete them as well)"));
  }
  Ok(r)
}

/// The free ends of a curve (none for closed curves).
fn ends(kind: &CurveKind) -> Vec<PointId> {
  let first_last = |pts: &[PointId]| match (pts.first(), pts.last()) {
    (Some(a), Some(b)) if a != b => vec![*a, *b],
    _ => vec![],
  };
  match kind {
    CurveKind::Line { a, b } => vec![*a, *b],
    CurveKind::Arc { start, end, .. }
    | CurveKind::EllipticalArc { start, end, .. }
    | CurveKind::Conic { start, end, .. } => vec![*start, *end],
    CurveKind::Spline { points, .. }
    | CurveKind::ControlSpline { points, .. }
    | CurveKind::Bezier { points }
    | CurveKind::QuinticSpline { points } => first_last(points),
    CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => vec![],
  }
}

/// Moves curve ends within `tol` of each other to one place (their middle, or a fixed one's
/// position) and makes them coincident. Returns how many ends joined another.
fn merge_gaps(sketch: &mut Sketch, tol: f64) -> Result<usize, CmdError> {
  // each end point with the curves it ends
  let mut owners: HashMap<PointId, Vec<CurveId>> = HashMap::new();
  for c in sketch.curves.iter().filter(|c| !owned(sketch, c.id)) {
    for e in ends(&c.kind) {
      owners.entry(e).or_default().push(c.id);
    }
  }
  let pos: HashMap<PointId, (P, bool)> = sketch.points.iter().map(|p| (p.id, ([p.x, p.y], p.fixed))).collect();
  let mut pts: Vec<PointId> = owners.keys().copied().filter(|p| pos.contains_key(p)).collect();
  pts.sort_by_key(|p| p.0);
  let joined: HashSet<(PointId, PointId)> = sketch
    .constraints
    .iter()
    .filter_map(|c| match c.kind {
      ConstraintKind::Coincident { a, b } => Some((a.min(b), a.max(b))),
      _ => None,
    })
    .collect();
  // union-find over a grid of tol-sized cells
  let cell = tol.max(1e-9);
  let key = |p: P| ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
  let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
  for (i, p) in pts.iter().enumerate() {
    grid.entry(key(pos[p].0)).or_default().push(i);
  }
  let mut parent: Vec<usize> = (0..pts.len()).collect();
  fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    i
  }
  for (i, p) in pts.iter().enumerate() {
    let (q, _) = pos[p];
    let (kx, ky) = key(q);
    for dx in -1..=1 {
      for dy in -1..=1 {
        for &j in grid.get(&(kx + dx, ky + dy)).into_iter().flatten() {
          if j <= i || dist(q, pos[&pts[j]].0) > tol {
            continue;
          }
          // never join a curve's two ends (that would collapse it)
          if owners[p].iter().any(|c| owners[&pts[j]].contains(c)) {
            continue;
          }
          let (a, b) = (root(&mut parent, i), root(&mut parent, j));
          if a != b {
            parent[a.max(b)] = a.min(b);
          }
        }
      }
    }
  }
  let mut groups: HashMap<usize, Vec<PointId>> = HashMap::new();
  for i in 0..pts.len() {
    let r = root(&mut parent, i);
    groups.entry(r).or_default().push(pts[i]);
  }
  let mut groups: Vec<Vec<PointId>> = groups.into_values().filter(|g| g.len() > 1).collect();
  groups.sort_by_key(|g| g[0].0);
  let mut merged = 0;
  for g in groups {
    // a chain of short curves can still bring two ends of one curve together: leave it
    let mut seen = HashSet::new();
    if g.iter().flat_map(|p| owners[p].iter()).any(|c| !seen.insert(*c)) {
      continue;
    }
    let fixed: Vec<P> = g.iter().filter(|p| pos[p].1).map(|p| pos[p].0).collect();
    let target = match fixed.first() {
      Some(f) if fixed.iter().all(|q| dist(*q, *f) <= 1e-9) => *f,
      Some(_) => continue,
      None => {
        let n = g.len() as f64;
        let sx: f64 = g.iter().map(|p| pos[p].0[0]).sum();
        let sy: f64 = g.iter().map(|p| pos[p].0[1]).sum();
        [sx / n, sy / n]
      }
    };
    for p in &g {
      if let Some(pt) = sketch.point_mut(*p) {
        (pt.x, pt.y) = (target[0], target[1]);
      }
    }
    let keep = g[0];
    for other in &g[1..] {
      if !joined.contains(&(keep.min(*other), keep.max(*other))) {
        sketch.add_constraint(ConstraintKind::Coincident { a: keep, b: *other })?;
      }
      merged += 1;
    }
  }
  Ok(merged)
}

/// Points along a curve, close enough together to fit lines and arcs to.
fn sample(sketch: &Sketch, id: CurveId, per_seg: usize) -> Vec<P> {
  let mut out = Vec::new();
  for seg in sketch.curve_segs(id) {
    match seg {
      Seg::Bezier { p } => out.extend((0..per_seg).map(|i| fox_sketch::bezier_at(&p, i as f64 / per_seg as f64))),
      Seg::Conic { p, w } => out.extend((0..per_seg).map(|i| fox_sketch::conic_at(&p, w, i as f64 / per_seg as f64))),
      Seg::Quintic { p } => {
        out.extend((0..per_seg * 4).map(|i| fox_sketch::quintic_at(&p, i as f64 / (per_seg * 4) as f64)))
      }
      other => other.flatten(&mut out),
    }
  }
  if let Some(last) = sketch.curve_segs(id).last() {
    out.push(last.end());
  }
  out.dedup_by(|a, b| dist(*a, *b) < 1e-12);
  out
}

/// `n` + 1 points evenly spaced along a polyline (by length).
fn resample(pts: &[P], n: usize) -> Vec<P> {
  let mut acc = vec![0.0];
  for w in pts.windows(2) {
    acc.push(acc.last().unwrap_or(&0.0) + dist(w[0], w[1]));
  }
  let total = *acc.last().unwrap_or(&0.0);
  let mut out = Vec::with_capacity(n + 1);
  let mut k = 0;
  for i in 0..=n {
    let target = total * i as f64 / n as f64;
    while k + 2 < acc.len() && acc[k + 1] < target {
      k += 1;
    }
    let span = acc[k + 1] - acc[k];
    let t = if span > 0.0 { ((target - acc[k]) / span).clamp(0.0, 1.0) } else { 0.0 };
    out.push([pts[k][0] + (pts[k + 1][0] - pts[k][0]) * t, pts[k][1] + (pts[k + 1][1] - pts[k][1]) * t]);
  }
  out
}

/// One piece of a fitted chain: a line to point index `to`, or an arc through `mid`.
enum Piece {
  Line { to: usize },
  Arc { mid: usize, to: usize },
}

/// Lines and arcs within `tol` of the polyline `pts`, greedily as long as each can be.
fn fit_arcs(pts: &[P], tol: f64) -> Vec<Piece> {
  let line_fits = |i: usize, j: usize| {
    let (a, b) = (pts[i], pts[j]);
    let l = dist(a, b);
    l > 0.0
      && pts[i + 1..j]
        .iter()
        .all(|p| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / l <= tol)
  };
  let arc_fits = |i: usize, j: usize| {
    let m = (i + j) / 2;
    let Some(c) = fox_sketch::circumcenter(pts[i], pts[m], pts[j]) else { return false };
    let r = dist(c, pts[i]);
    // a nearly straight run is a line
    r < 1e6 * dist(pts[i], pts[j]).max(tol)
      && pts[i + 1..j].iter().all(|p| (dist(*p, c) - r).abs() <= tol)
  };
  let mut out = Vec::new();
  let mut i = 0;
  while i + 1 < pts.len() {
    let mut best = Piece::Line { to: i + 1 };
    for j in i + 2..pts.len() {
      if line_fits(i, j) {
        best = Piece::Line { to: j };
      } else if arc_fits(i, j) {
        best = Piece::Arc { mid: (i + j) / 2, to: j };
      } else {
        break;
      }
    }
    i = match best {
      Piece::Line { to } | Piece::Arc { to, .. } => to,
    };
    out.push(best);
  }
  out
}

/// Replaces a spline with lines or arcs joined end to end, keeping its end points (so what it
/// was joined to stays joined). Returns the new curves, or None if it can't be converted.
fn convert_spline(sketch: &mut Sketch, id: CurveId, to: SplinesTo, o: &Options) -> Result<Option<Vec<CurveId>>, CmdError> {
  let Some(curve) = sketch.curve(id).cloned() else { return Ok(None) };
  let (first, last) = match &curve.kind {
    CurveKind::Spline { points, .. }
    | CurveKind::ControlSpline { points, .. }
    | CurveKind::Bezier { points }
    | CurveKind::QuinticSpline { points } => match (points.first(), points.last()) {
      (Some(a), Some(b)) => (*a, *b),
      _ => return Ok(None),
    },
    CurveKind::Conic { start, end, .. } => (*start, *end),
    _ => return Ok(None),
  };
  let dense = sample(sketch, id, 48);
  if dense.len() < 2 {
    return Ok(None);
  }
  let mut pts = match to {
    SplinesTo::Lines => resample(&dense, o.segments),
    SplinesTo::Arcs => dense,
  };
  // the ends are the spline's own points
  let (Some(pa), Some(pb)) = (sketch.pos(first), sketch.pos(last)) else { return Ok(None) };
  let n = pts.len();
  pts[0] = pa;
  pts[n - 1] = pb;
  let pieces: Vec<Piece> = match to {
    SplinesTo::Lines => (1..n).map(|to| Piece::Line { to }).collect(),
    SplinesTo::Arcs => fit_arcs(&pts, o.spline_tolerance),
  };
  // the chain's points: the spline's ends, new points between
  let mut ids: HashMap<usize, PointId> = HashMap::from([(0, first), (n - 1, last)]);
  let mut made = Vec::new();
  let mut from = 0;
  for piece in &pieces {
    let to = match piece {
      Piece::Line { to } | Piece::Arc { to, .. } => *to,
    };
    if dist(pts[from], pts[to]) <= 1e-9 {
      continue;
    }
    let a = *ids.entry(from).or_insert_with(|| sketch.add_point(pts[from][0], pts[from][1]));
    let b = match ids.get(&to) {
      Some(b) => *b,
      None => {
        let b = sketch.add_point(pts[to][0], pts[to][1]);
        ids.insert(to, b);
        b
      }
    };
    let c = match piece {
      Piece::Line { .. } => sketch.add_line(PtSpec::Point(a), PtSpec::Point(b))?.0,
      Piece::Arc { mid, .. } => sketch.add_arc_3pt(PtSpec::Point(a), pts[*mid], PtSpec::Point(b))?,
    };
    sketch.set_construction(c, curve.construction)?;
    made.push(c);
    from = to;
  }
  if made.is_empty() {
    return Ok(None);
  }
  sketch.remove_curve(id)?;
  Ok(Some(made))
}
