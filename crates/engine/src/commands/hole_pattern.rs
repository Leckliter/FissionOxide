//! Fusion's Hole and Rectangular Pattern options beyond the base commands: a single hole placed
//! on a face and positioned by offsets to edges, the tap type and drill point; a pattern
//! direction taken from an edge, and patterns of components with suppressed instances.
//!
//! Each runs as one undo step and, where it builds on a base command (`hole`,
//! `pattern.rectangular`, `component.copy`, `thread`), runs that command for the rest.

use fox_kernel::glam::DVec3;
use fox_model::{BodyId, EdgeRef, FeatureId, FeatureKind, SketchLink};
use fox_sketch::{ConstraintKind, Dim, PtSpec};
use serde_json::{Value, json};

use super::{expr_of, opt_str, params_scope, req_vec3, run as dispatch};
use crate::{CmdError, CmdResult, Session};

/// Hole options handled here; the rest go to the base `hole` command.
const HOLE_KEYS: [&str; 6] = ["face", "at", "offsets", "tap", "drill_point", "pitch"];

/// Runs `hole` and `pattern.rectangular` when they use an option of this module.
pub(super) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  match name {
    "hole" if HOLE_KEYS.iter().any(|k| p.get(k).is_some()) => Some(grouped(s, |s| hole(s, p))),
    "pattern.rectangular"
      if p.get("components").is_some()
        || p.get("suppress").is_some()
        || is_edge(p.get("direction"))
        || is_edge(p.get("direction2")) =>
    {
      Some(grouped(s, |s| pattern(s, p)))
    }
    _ => None,
  }
}

/// Runs `f` as one undo step: its edits collapse into one, and a failure undoes them all.
fn grouped(s: &mut Session, f: impl FnOnce(&mut Session) -> CmdResult) -> CmdResult {
  let own = !s.in_transaction();
  let (start, before, editing) = (s.undo.len(), s.design.clone(), s.edit_sketch);
  if own {
    s.begin_transaction()?;
  }
  let r = f(s);
  match &r {
    Ok(_) if own => {
      s.commit_transaction()?;
    }
    Ok(_) => {}
    Err(_) => {
      if own {
        s.rollback_transaction()?;
      } else if s.undo.len() > start {
        s.undo.truncate(start);
        s.design = before;
        s.rebuild();
      }
      s.edit_sketch = editing;
    }
  }
  r
}

// ---------------------------------------------------------------- hole

fn hole(s: &mut Session, p: &Value) -> CmdResult {
  let mut q = p.clone();
  if let Some(o) = q.as_object_mut() {
    for k in HOLE_KEYS {
      o.remove(k);
    }
  }
  // Drill Point: Fusion's flat bottom is a 180° tip
  match opt_str(p, "drill_point") {
    None | Some("angle") => {}
    Some("flat") => q["tip_angle"] = json!("180 deg"),
    Some(o) => return Err(CmdError::new(format!("unknown drill point '{o}' (flat, angle)"))),
  }
  let tap = opt_str(p, "tap").unwrap_or("simple");
  if !matches!(tap, "simple" | "clearance" | "tapped") {
    return Err(CmdError::new(format!("unknown tap type '{tap}' (simple, clearance, tapped)")));
  }
  let placed = match p.get("face") {
    Some(face) => {
      let (sk, pt) = place(s, face, p)?;
      q["sketch"] = json!(sk.0);
      q["points"] = json!([pt]);
      Some(sk)
    }
    None => None,
  };
  let mut r = dispatch(s, "hole", &q)?;
  if let Some(sk) = placed {
    r["sketch"] = json!(sk.0);
  }
  if tap == "tapped" {
    let faces = hole_walls(s, &q)?;
    if faces.is_empty() {
      return Err(CmdError::new("the hole has no walls to thread"));
    }
    let mut t = json!({"faces": faces});
    if let Some(pitch) = p.get("pitch") {
      t["pitch"] = pitch.clone();
    }
    let t = dispatch(s, "thread", &t)?;
    r["thread"] = t.get("feature").cloned().unwrap_or(t);
  }
  Ok(r)
}

/// Fusion's single hole: a sketch on the planar `face` with one point where the face was picked
/// (`at`, a model point; default the face's centre), held at `offsets` [{edge, distance}] from
/// up to two straight edges (projected into the sketch as construction lines that follow them).
/// Returns the sketch and the point.
fn place(s: &mut Session, face: &Value, p: &Value) -> Result<(FeatureId, u32), CmdError> {
  let get = |k: &str| face.get(k).and_then(Value::as_u64).map(|v| v as u32);
  let (Some(bid), Some(fid)) = (get("body"), get("face")) else {
    return Err(CmdError::new("'face' needs {body, face}"));
  };
  let body = s.model.body(BodyId(bid)).ok_or(CmdError::new("no such body"))?;
  let info = body.faces().iter().find(|f| f.id == fid).copied().ok_or(CmdError::new("no such face"))?;
  if info.plane().is_none() {
    return Err(CmdError::new("a hole is placed on a planar face"));
  }
  let at = match p.get("at") {
    Some(_) => DVec3::from(req_vec3(p, "at")?),
    None => info.origin,
  };
  let mut refs = Vec::new();
  for o in p.get("offsets").and_then(Value::as_array).into_iter().flatten() {
    let e = o.get("edge").ok_or(CmdError::new("each offset needs an 'edge'"))?;
    let eb = BodyId(e.get("body").and_then(Value::as_u64).unwrap_or(bid as u64) as u32);
    let eid = e.get("edge").and_then(Value::as_u64).ok_or(CmdError::new("an offset edge needs {body, edge}"))? as u32;
    let b = s.model.body(eb).ok_or(CmdError::new("no such body"))?;
    let edge = b.edges().iter().find(|x| x.id == eid).copied().ok_or(CmdError::new("no such edge"))?;
    if edge.kind != fox_kernel::CurveKind::Line {
      return Err(CmdError::new("hole offsets are measured from straight edges"));
    }
    let expr = expr_of(o.get("distance").ok_or(CmdError::new("each offset needs a 'distance'"))?)?;
    let v = fox_expr::eval(&expr, fox_expr::Kind::Length, &params_scope(s))?;
    refs.push((edge.start, edge.end, EdgeRef::from_edge(eb, &edge, b.faces()), Dim::expr(expr, v)));
  }
  if refs.len() > 2 {
    return Err(CmdError::new("a hole takes at most two offset edges"));
  }
  let sk = dispatch(s, "sketch.create", &json!({"plane": {"body": bid, "face": fid}}))?;
  let sk = FeatureId(sk["sketch"].as_u64().ok_or(CmdError::new("the hole sketch failed"))? as u32);
  s.edit_sketch = None;
  let plane = s.model.sketches.get(&sk).map(|r| r.plane).ok_or(CmdError::new("the hole sketch failed to build"))?;
  let c = plane.to_local(at);
  let lines: Vec<_> = refs.into_iter().map(|(a, b, link, d)| (plane.to_local(a), plane.to_local(b), link, d)).collect();
  let pt = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let pt = sketch.add_point(c[0], c[1]);
    let mut links = Vec::new();
    for (a, b, link, dim) in lines {
      if (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-6 {
        return Err(CmdError::new("an offset edge must not be square to the face"));
      }
      let (line, ends) = sketch.add_line(PtSpec::At(a[0], a[1]), PtSpec::At(b[0], b[1]))?;
      sketch.set_construction(line, true)?;
      for e in ends {
        if let Some(q) = sketch.point_mut(e) {
          q.fixed = true;
        }
      }
      sketch.add_constraint(ConstraintKind::PointLineDistance { p: pt, line, value: dim })?;
      links.push(SketchLink { curve: line, edge: link });
    }
    if !sketch.solve().ok() {
      return Err(CmdError::new("the hole offsets can't all be met"));
    }
    if let Some(FeatureKind::Sketch { links: l, .. }) = d.feature_mut(sk).map(|f| &mut f.kind) {
      l.extend(links);
    }
    Ok(pt.0)
  })?;
  Ok((sk, pt))
}

/// The cylindrical walls of the holes `q` (a base `hole` command) drilled: faces of the hole's
/// diameter whose axis runs through one of its centres.
fn hole_walls(s: &Session, q: &Value) -> Result<Vec<Value>, CmdError> {
  let r = fox_expr::eval(&expr_of(&q["diameter"])?, fox_expr::Kind::Length, &params_scope(s))? / 2.0;
  let sk = match q.get("sketch").and_then(Value::as_u64) {
    Some(v) => FeatureId(v as u32),
    None => s
      .design
      .features
      .iter()
      .take(s.design.active_len())
      .rev()
      .find(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
      .map(|f| f.id)
      .ok_or(CmdError::new("there is no sketch"))?,
  };
  let rs = s.model.sketches.get(&sk).ok_or(CmdError::new("the hole's sketch failed"))?;
  let wanted: Vec<u32> = super::ids(q, "points");
  let used: Vec<fox_sketch::PointId> = rs.sketch.curves.iter().flat_map(|c| rs.sketch.curve_points(c.id)).collect();
  let centres: Vec<DVec3> = rs
    .sketch
    .points
    .iter()
    .filter(|pt| {
      if wanted.is_empty() { pt.id != fox_sketch::ORIGIN && !used.contains(&pt.id) } else { wanted.contains(&pt.id.0) }
    })
    .map(|pt| rs.plane.to_world([pt.x, pt.y]))
    .collect();
  let mut out = Vec::new();
  for b in &s.model.bodies {
    for f in b.faces() {
      if f.kind != fox_kernel::SurfaceKind::Cylinder || (f.radius - r).abs() > 1e-4 {
        continue;
      }
      let axis = f.normal.normalize_or_zero();
      let through = centres.iter().any(|c| {
        let d = *c - f.origin;
        (d - axis * d.dot(axis)).length() < 1e-4
      });
      if through {
        out.push(json!({"body": b.id.0, "face": f.id}));
      }
    }
  }
  Ok(out)
}

// ---------------------------------------------------------------- rectangular pattern

/// A pattern direction given as an edge: `{body, edge, flip?}`.
fn is_edge(v: Option<&Value>) -> bool {
  v.is_some_and(|v| v.get("edge").is_some())
}

/// The unit direction of a straight edge `{body, edge, flip?}`.
fn edge_direction(s: &Session, v: &Value) -> Result<[f64; 3], CmdError> {
  let body =
    BodyId(v.get("body").and_then(Value::as_u64).ok_or(CmdError::new("an edge direction needs a body"))? as u32);
  let id = v.get("edge").and_then(Value::as_u64).ok_or(CmdError::new("an edge direction needs an edge"))? as u32;
  let b = s.model.body(body).ok_or(CmdError::new("no such body"))?;
  let e = b.edges().iter().find(|e| e.id == id).copied().ok_or(CmdError::new("no such edge"))?;
  if e.kind != fox_kernel::CurveKind::Line {
    return Err(CmdError::new("a pattern direction must be a straight edge"));
  }
  let mut d = (e.end - e.start).normalize_or_zero();
  if v.get("flip").and_then(Value::as_bool).unwrap_or(false) {
    d = -d;
  }
  Ok([d.x, d.y, d.z])
}

fn pattern(s: &mut Session, p: &Value) -> CmdResult {
  let mut q = p.clone();
  for k in ["direction", "direction2"] {
    if is_edge(p.get(k)) {
      q[k] = json!(edge_direction(s, &p[k])?);
    }
  }
  if q.get("components").is_some() {
    return component_pattern(s, &q);
  }
  if q.get("suppress").is_some() {
    return Err(CmdError::new("suppressing instances works on component patterns only (not bodies yet)"));
  }
  dispatch(s, "pattern.rectangular", &q)
}

/// The instances a pattern leaves out: `suppress` [[i, j]] (or [i] in the first direction),
/// counted from the original at [0, 0].
fn suppressed(p: &Value) -> Result<Vec<[u64; 2]>, CmdError> {
  let bad = || CmdError::new("'suppress' takes instances as [i, j] (or i)");
  let mut out = Vec::new();
  for v in p.get("suppress").and_then(Value::as_array).into_iter().flatten() {
    out.push(match v {
      Value::Number(_) => [v.as_u64().ok_or_else(bad)?, 0],
      Value::Array(a) => [
        a.first().and_then(Value::as_u64).ok_or_else(bad)?,
        a.get(1).map(|j| j.as_u64().ok_or_else(bad)).transpose()?.unwrap_or(0),
      ],
      _ => return Err(bad()),
    });
  }
  Ok(out)
}

/// Fusion's Components pattern type: a copy of each component at every instance but the
/// original and the suppressed ones.
fn component_pattern(s: &mut Session, p: &Value) -> CmdResult {
  let comps: Vec<Value> = p["components"].as_array().cloned().unwrap_or_default();
  if comps.is_empty() {
    return Err(CmdError::new("pick the components to pattern"));
  }
  let scope = params_scope(s);
  let count = |k: &str, default: f64| -> Result<u64, CmdError> {
    let n = match p.get(k) {
      None => default,
      Some(v) => fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Unitless, &scope)?,
    };
    if n.is_nan() || n < 1.0 || n.fract().abs() > 1e-9 || n > 1000.0 {
      return Err(CmdError::new(format!("'{k}' must be a whole number from 1 to 1000")));
    }
    Ok(n as u64)
  };
  let length = |k: &str| -> Result<f64, CmdError> {
    match p.get(k) {
      None => Ok(0.0),
      Some(v) => Ok(fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, &scope)?),
    }
  };
  let unit = |k: &str| -> Result<DVec3, CmdError> {
    let d = DVec3::from(req_vec3(p, k)?).normalize_or_zero();
    if d == DVec3::ZERO {
      return Err(CmdError::new(format!("'{k}' has no length")));
    }
    Ok(d)
  };
  let (d1, n1, s1) = (unit("direction")?, count("count", 1.0)?, length("spacing")?);
  let (d2, n2, s2) = match p.get("direction2") {
    Some(_) => (unit("direction2")?, count("count2", 1.0)?, length("spacing2")?),
    None => (DVec3::ZERO, 1, 0.0),
  };
  let skip = suppressed(p)?;
  let mut copies = Vec::new();
  for j in 0..n2 {
    for i in 0..n1 {
      if (i, j) == (0, 0) || skip.contains(&[i, j]) {
        continue;
      }
      let t = d1 * s1 * i as f64 + d2 * s2 * j as f64;
      for c in &comps {
        let r = dispatch(s, "component.copy", &json!({"component": c, "translate": [t.x, t.y, t.z]}))?;
        copies.push(r.get("component").cloned().unwrap_or(r));
      }
    }
  }
  Ok(json!({"copies": copies}))
}

#[cfg(test)]
mod tests {
  use std::f64::consts::PI;

  use fox_model::FeatureKind;
  use serde_json::{Value, json};

  use crate::Session;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    match s.execute(cmd, &p) {
      Ok(v) => v,
      Err(e) => panic!("{cmd} {p}: {e}"),
    }
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  /// An 80 × 50 × 10 block from the origin; returns its body and top face.
  fn block(s: &mut Session) -> (u32, u32) {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 50]}));
    run(s, "extrude", json!({"distance": 10}));
    let b = &s.model.bodies[0];
    (b.id.0, b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id)
  }

  /// The straight top edge of the block running along `axis` (0 = X) at the other coordinate `at`.
  fn top_edge(s: &Session, axis: usize, at: f64) -> u32 {
    let b = &s.model.bodies[0];
    b.edges()
      .iter()
      .find(|e| {
        let d = e.end - e.start;
        e.start.z > 9.9 && e.end.z > 9.9 && d[axis].abs() > 1.0 && (e.start[1 - axis] - at).abs() < 1e-6
      })
      .unwrap()
      .id
  }

  /// Where the hole wall of radius `r` is (its axis, in X and Y).
  fn wall_at(s: &Session, r: f64) -> Option<[f64; 2]> {
    let b = &s.model.bodies[0];
    b.faces()
      .iter()
      .find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder && (f.radius - r).abs() < 1e-6)
      .map(|f| [f.origin.x, f.origin.y])
  }

  #[test]
  fn single_hole_on_a_face_held_by_edge_offsets() {
    let mut s = Session::default();
    let (body, top) = block(&mut s);
    let before = (volume(&s), s.design.features.len());
    let (front, left) = (top_edge(&s, 0, 0.0), top_edge(&s, 1, 0.0));
    // clicked near (20, 20); the offsets move it to 25 from the left edge, 15 from the front
    let r = run(
      &mut s,
      "hole",
      json!({
        "face": {"body": body, "face": top}, "at": [20, 20, 10],
        "offsets": [{"edge": {"body": body, "edge": front}, "distance": "15 mm"},
                    {"edge": {"body": body, "edge": left}, "distance": 25}],
        "diameter": 6, "through_all": true,
      }),
    );
    assert!(r.get("sketch").is_some(), "{r}");
    assert!((before.0 - volume(&s) - PI * 9.0 * 10.0).abs() < 1e-2, "{}", before.0 - volume(&s));
    let c = wall_at(&s, 3.0).expect("a hole wall");
    assert!((c[0] - 25.0).abs() < 1e-6 && (c[1] - 15.0).abs() < 1e-6, "{c:?}");
    // a sketch and a hole, undone in one step
    assert_eq!(s.design.features.len(), before.1 + 2);
    assert!(s.edit_sketch.is_none());
    assert!(s.undo());
    assert_eq!(s.design.features.len(), before.1);
    assert!((volume(&s) - before.0).abs() < 1e-6);
  }

  #[test]
  fn single_hole_defaults_to_the_click_and_fails_cleanly() {
    let mut s = Session::default();
    let (body, top) = block(&mut s);
    run(&mut s, "hole", json!({"face": {"body": body, "face": top}, "at": [30, 20, 10], "diameter": 4, "depth": 5}));
    let c = wall_at(&s, 2.0).expect("a hole wall");
    assert!((c[0] - 30.0).abs() < 1e-6 && (c[1] - 20.0).abs() < 1e-6, "{c:?}");
    // a curved face can't take a hole; nothing is left behind
    let n = s.design.features.len();
    let side = s.model.bodies[0].faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap().id;
    assert!(s.execute("hole", &json!({"face": {"body": body, "face": side}, "diameter": 4, "depth": 5})).is_err());
    assert_eq!(s.design.features.len(), n);
  }

  #[test]
  fn drill_point_and_tap_type() {
    let mut s = Session::default();
    let (body, top) = block(&mut s);
    let v = volume(&s);
    // a flat drill point: just the cylinder
    run(
      &mut s,
      "hole",
      json!({"face": {"body": body, "face": top}, "at": [20, 20, 10], "diameter": 6, "depth": 5, "drill_point": "flat"}),
    );
    assert!((v - volume(&s) - PI * 9.0 * 5.0).abs() < 1e-2, "{}", v - volume(&s));
    // tapped: a thread on the wall
    let r = run(
      &mut s,
      "hole",
      json!({"face": {"body": body, "face": top}, "at": [60, 30, 10], "diameter": 8, "through_all": true, "tap": "tapped"}),
    );
    assert!(r.get("thread").is_some(), "{r}");
    assert!(s.design.features.iter().any(|f| matches!(f.kind, FeatureKind::Thread { .. })));
    assert!(s.execute("hole", &json!({"diameter": 6, "depth": 2, "tap": "taper"})).is_err());
    assert!(s.execute("hole", &json!({"diameter": 6, "depth": 2, "drill_point": "round"})).is_err());
  }

  #[test]
  fn pattern_along_an_edge() {
    let mut s = Session::default();
    let (body, _) = block(&mut s);
    let left = top_edge(&s, 1, 0.0);
    let r = run(
      &mut s,
      "pattern.rectangular",
      json!({"bodies": [body], "direction": {"body": body, "edge": left}, "count": 3, "spacing": 60, "join": false}),
    );
    assert!(r["error"].is_null(), "{r}");
    // the edge runs along Y (one way or the other): the last copy is 120 along it
    let (lo, hi) = s
      .model
      .bodies
      .iter()
      .map(|b| b.shape.mass().unwrap())
      .fold((f64::MAX, f64::MIN), |(lo, hi), m| (lo.min(m.bbox_min.y), hi.max(m.bbox_max.y)));
    assert!((hi - 170.0).abs() < 1e-6 || (lo + 120.0).abs() < 1e-6, "{lo} {hi}");
    let bad = json!({"bodies": [body], "direction": {"body": body, "edge": 999}, "count": 2, "spacing": 5});
    assert!(s.execute("pattern.rectangular", &bad).is_err());
  }

  #[test]
  fn component_pattern_with_suppressed_instances() {
    let mut s = Session::default();
    let (body, _) = block(&mut s);
    let c = run(&mut s, "component.create", json!({"name": "Block", "bodies": [body]}))["component"].clone();
    let n = s.design.features.len();
    let r = run(
      &mut s,
      "pattern.rectangular",
      json!({"components": [c], "direction": [1, 0, 0], "count": 3, "spacing": 100,
             "direction2": [0, 1, 0], "count2": 2, "spacing2": "60 mm", "suppress": [[1, 1], 2]}),
    );
    // 3 × 2 less the original and the two suppressed
    assert_eq!(r["copies"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(s.model.bodies.len(), 4);
    let at = |x: f64, y: f64| {
      s.model.bodies.iter().any(|b| {
        let m = b.shape.mass().unwrap();
        (m.bbox_min.x - x).abs() < 1e-6 && (m.bbox_min.y - y).abs() < 1e-6
      })
    };
    assert!(at(200.0, 60.0) && at(100.0, 0.0) && at(0.0, 60.0));
    assert!(!at(200.0, 0.0) && !at(100.0, 60.0), "the suppressed instances are left out");
    assert!(s.undo());
    assert_eq!(s.design.features.len(), n);
    // bodies can't suppress instances yet
    let bad = json!({"bodies": [body], "direction": [1, 0, 0], "count": 2, "spacing": 5, "suppress": [1]});
    assert!(s.execute("pattern.rectangular", &bad).is_err());
  }
}
