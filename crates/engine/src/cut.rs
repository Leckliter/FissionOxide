//! 2D cutting commands (laser, plasma, waterjet): contours from a flat pattern, faces or a
//! sketch, planned and posted by `fox-cut`, with the machine profiles in the hub's
//! `machines.json` next to the built-in ones.

use std::path::{Path, PathBuf};

use fox_cut::{Contour, CutParams, Edge, Lead, LeadKind, Machine, plan};
use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind, EdgeInfo, Plane};
use fox_model::BodyId;
use fox_sketch::{Loop, Seg as SkSeg, find_profiles};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "cut.machines" => machines(s).map(|ms| json!(ms.iter().map(machine_json).collect::<Vec<_>>())),
    "cut.machine.add" => machine_add(s, p),
    "cut.machine.delete" => machine_delete(s, p),
    "cut.contours" => contours_cmd(s, p),
    "cut.gcode" => gcode(s, p),
    _ => return None,
  })
}

fn machines_file(hub: &Path) -> PathBuf {
  hub.join("machines.json")
}

/// The hub's own machine profiles.
fn user_machines(hub: &Path) -> Result<Vec<Machine>, CmdError> {
  let f = machines_file(hub);
  if !f.exists() {
    return Ok(Vec::new());
  }
  let text = std::fs::read_to_string(&f)?;
  serde_json::from_str(&text).map_err(|e| CmdError::new(format!("{}: {e}", f.display())))
}

/// Built-in machines, then the hub's (a hub machine with a built-in's id replaces it).
fn machines(s: &Session) -> Result<Vec<Machine>, CmdError> {
  let user = user_machines(&s.hub_root())?;
  let mut all: Vec<Machine> = fox_cut::builtin_machines()
    .into_iter()
    .filter(|b| !user.iter().any(|u| u.id.eq_ignore_ascii_case(&b.id)))
    .collect();
  all.extend(user);
  Ok(all)
}

fn machine_json(m: &Machine) -> Value {
  json!({
    "id": m.id, "name": m.name, "kind": m.kind, "extension": m.extension, "arcs": m.arcs,
    "processes": m.processes.iter().map(|p| json!({
      "name": p.name, "kerf": p.params.kerf, "feed": p.params.feed, "power": p.params.power,
      "pierce_delay": p.params.pierce_delay,
    })).collect::<Vec<_>>(),
  })
}

fn machine_add(s: &mut Session, p: &Value) -> CmdResult {
  let m: Machine = serde_json::from_value(p.get("machine").cloned().unwrap_or_else(|| p.clone()))
    .map_err(|e| CmdError::new(format!("bad machine profile: {e}")))?;
  if m.id.trim().is_empty() {
    return Err(CmdError::new("a machine needs an id"));
  }
  let hub = s.hub_root();
  let mut user = user_machines(&hub)?;
  user.retain(|u| !u.id.eq_ignore_ascii_case(&m.id));
  user.push(m.clone());
  std::fs::create_dir_all(&hub)?;
  std::fs::write(machines_file(&hub), serde_json::to_string_pretty(&user).map_err(|e| CmdError::new(e.to_string()))?)?;
  Ok(machine_json(&m))
}

fn machine_delete(s: &mut Session, p: &Value) -> CmdResult {
  let id = opt_str(p, "id").ok_or(CmdError::new("'id' is required"))?;
  let hub = s.hub_root();
  let mut user = user_machines(&hub)?;
  let n = user.len();
  user.retain(|u| !u.id.eq_ignore_ascii_case(id));
  if user.len() == n {
    return Err(CmdError::new(format!("no machine '{id}' in the hub (built-in machines can't be deleted)")));
  }
  std::fs::write(machines_file(&hub), serde_json::to_string_pretty(&user).map_err(|e| CmdError::new(e.to_string()))?)?;
  Ok(json!({"deleted": id}))
}

fn machine(s: &Session, p: &Value) -> Result<Machine, CmdError> {
  match p.get("machine") {
    Some(Value::Object(_)) => {
      serde_json::from_value(p["machine"].clone()).map_err(|e| CmdError::new(format!("bad machine profile: {e}")))
    }
    Some(Value::String(id)) => {
      let all = machines(s)?;
      all.iter().find(|m| m.id.eq_ignore_ascii_case(id.trim())).cloned().ok_or_else(|| {
        CmdError::new(format!(
          "unknown machine '{id}' ({})",
          all.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ")
        ))
      })
    }
    Some(_) => Err(CmdError::new("'machine' is an id or a machine profile")),
    None => fox_cut::post::find("grbl_laser").ok_or(CmdError::new("no default machine")),
  }
}

/// A number, or an expression string ("1.5 mm").
fn num(v: &Value) -> Option<f64> {
  match v {
    Value::String(e) => fox_expr::eval(e, fox_expr::Kind::Length, &std::collections::HashMap::new())
      .or_else(|_| fox_expr::eval(e, fox_expr::Kind::Unitless, &std::collections::HashMap::new()))
      .ok(),
    v => v.as_f64(),
  }
}

/// A lead: "arc" | "line" | "none" | a length | {kind?, length?}.
fn lead(v: &Value, base: Lead) -> Result<Lead, CmdError> {
  let kind = |k: &str| LeadKind::parse(k).ok_or(CmdError::new(format!("unknown lead '{k}' (arc, line, none)")));
  Ok(match v {
    Value::String(k) => match num(v) {
      Some(l) if k.trim().starts_with(|c: char| c.is_ascii_digit() || c == '.') => Lead { length: l, ..base },
      _ => Lead { kind: kind(k)?, ..base },
    },
    Value::Number(_) => Lead { length: num(v).unwrap_or(base.length), ..base },
    Value::Object(_) => Lead {
      kind: match v.get("kind").and_then(Value::as_str) {
        Some(k) => kind(k)?,
        None => base.kind,
      },
      length: v.get("length").and_then(num).unwrap_or(base.length),
    },
    _ => return Err(CmdError::new("a lead is arc|line|none, a length, or {kind, length}")),
  })
}

/// The machine's process with the parameters given in the command on top.
fn params(m: &Machine, p: &Value) -> Result<CutParams, CmdError> {
  let process = match p.get("process") {
    Some(Value::Number(n)) => Some(n.to_string()),
    Some(v) => v.as_str().filter(|n| !n.trim().is_empty()).map(str::to_string),
    None => None,
  };
  let mut c = m.process(process.as_deref()).map_err(CmdError::new)?;
  let set = |k: &str, f: &mut f64| {
    if let Some(v) = p.get(k).and_then(num) {
      *f = v;
    }
  };
  set("kerf", &mut c.kerf);
  set("feed", &mut c.feed);
  set("power", &mut c.power);
  set("pierce_delay", &mut c.pierce_delay);
  set("pierce_height", &mut c.pierce_height);
  set("cut_height", &mut c.cut_height);
  set("safe_height", &mut c.safe_height);
  set("overcut", &mut c.overcut);
  set("tab_width", &mut c.tab_width);
  set("small_hole_diameter", &mut c.small_hole_diameter);
  set("small_hole_feed", &mut c.small_hole_feed);
  for (k, f) in [
    ("compensate", &mut c.compensate),
    ("tabs_on_holes", &mut c.tabs_on_holes),
    ("part_on_right", &mut c.part_on_right),
  ] {
    if let Some(b) = p.get(k).and_then(Value::as_bool) {
      *f = b;
    }
  }
  for (k, f) in [("tabs", &mut c.tabs), ("material", &mut c.material)] {
    if let Some(v) = p.get(k).and_then(num) {
      if !(0.0..=1e6).contains(&v) {
        return Err(CmdError::new(format!("bad '{k}'")));
      }
      *f = v.round() as u32;
    }
  }
  if let Some(v) = p.get("lead_in") {
    c.lead_in = lead(v, c.lead_in)?;
  }
  if let Some(v) = p.get("lead_out") {
    c.lead_out = lead(v, c.lead_out)?;
  }
  if c.feed <= 0.0 || c.kerf < 0.0 {
    return Err(CmdError::new("feed must be positive and kerf not negative"));
  }
  Ok(c)
}

/// A model edge lying in `plane` as a 2D edge (None if it leaves the plane).
fn edge_2d(e: &EdgeInfo, plane: &Plane, lines: &[fox_kernel::EdgeLine]) -> Option<Edge> {
  let n = plane.normal();
  let on = |p: DVec3| (p - plane.origin).dot(n).abs() < 1e-4;
  if !(on(e.start) && on(e.end) && on(e.mid)) || e.kind == CurveKind::Degenerate {
    return None;
  }
  let l = |p: DVec3| plane.to_local(p);
  Some(match e.kind {
    CurveKind::Line => Edge::Line { a: l(e.start), b: l(e.end) },
    CurveKind::Circle if (e.start - e.end).length() < 1e-6 => Edge::Circle { center: l(e.center), radius: e.radius },
    CurveKind::Circle => {
      let c = l(e.center);
      let ang = |p: DVec3| {
        let q = l(p);
        (q[1] - c[1]).atan2(q[0] - c[0])
      };
      let (a0, am, a1) = (ang(e.start), ang(e.mid), ang(e.end));
      let tau = std::f64::consts::TAU;
      let to_end = (a1 - a0).rem_euclid(tau);
      let to_mid = (am - a0).rem_euclid(tau);
      let sweep = if to_mid < to_end { to_end } else { to_end - tau };
      Edge::Arc { center: c, radius: e.radius, start_angle: a0, sweep }
    }
    _ => {
      let pts = lines.iter().find(|x| x.id == e.id)?.points.iter().map(|p| l(*p)).collect();
      Edge::Polyline { points: pts }
    }
  })
}

fn body_edges(shape: &fox_kernel::Shape, plane: &Plane, face: Option<u32>) -> Result<Vec<Edge>, CmdError> {
  let edges = shape.edges()?;
  let lines = shape.edge_lines(0.01)?;
  Ok(
    edges
      .iter()
      .filter(|e| face.is_none_or(|f| e.faces.contains(&f)))
      .filter_map(|e| edge_2d(e, plane, &lines))
      .collect(),
  )
}

fn sketch_edges(s: &Session, id: u32) -> Result<Vec<Edge>, CmdError> {
  let sk = s.design.sketch(fox_model::FeatureId(id))?;
  let mut loops: Vec<Loop> = Vec::new();
  for pr in find_profiles(sk) {
    for l in std::iter::once(pr.outer).chain(pr.holes) {
      // a ring's hole is also the disc inside it: keep each loop once
      let a = l.signed_area().abs();
      if !loops.iter().any(|o| (o.signed_area().abs() - a).abs() < 1e-6 && same_loop(o, &l)) {
        loops.push(l);
      }
    }
  }
  let mut edges = Vec::new();
  for l in &loops {
    for seg in &l.segs {
      edges.push(match *seg {
        SkSeg::Line { a, b } => Edge::Line { a, b },
        SkSeg::Arc { center, radius, start_angle, sweep } => Edge::Arc { center, radius, start_angle, sweep },
        SkSeg::Circle { center, radius, .. } => Edge::Circle { center, radius },
        SkSeg::Bezier { .. } | SkSeg::EllipseArc { .. } => {
          let mut pts = Vec::new();
          seg.flatten(&mut pts);
          pts.push(seg.end());
          Edge::Polyline { points: pts }
        }
      });
    }
  }
  Ok(edges)
}

/// Whether two loops run through the same points (either direction).
fn same_loop(a: &Loop, b: &Loop) -> bool {
  let (pa, pb) = (a.polygon(), b.polygon());
  pa.iter().all(|p| pb.iter().any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-6))
}

/// Closed contours of the source, the lower-left corner of their bounds at the origin unless
/// `origin` is "model". Also the source's description and how many open chains were dropped.
fn contours(s: &Session, p: &Value) -> Result<(Vec<Contour>, String, usize), CmdError> {
  let (edges, what) = if let Some(id) = p.get("sketch").and_then(Value::as_u64) {
    let id = u32::try_from(id).map_err(|_| CmdError::new("bad sketch id"))?;
    (sketch_edges(s, id)?, format!("sketch {id}"))
  } else if let Some(faces) = p.get("faces").and_then(Value::as_array) {
    let mut edges = Vec::new();
    let mut plane: Option<Plane> = None;
    for f in faces {
      let r = face_ref(s, f)?;
      let body = s.model.body(r.body).ok_or(CmdError::new(format!("no body {}", r.body.0)))?;
      let fid = r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?;
      let info = body.faces().iter().find(|x| x.id == fid).ok_or(CmdError::new("no such face"))?;
      let pl = info.plane().ok_or(CmdError::new("cutting needs flat faces"))?;
      let pl = *plane.get_or_insert(pl);
      if pl.normal().cross(info.plane().map(|q| q.normal()).unwrap_or(DVec3::Z)).length() > 1e-6 {
        return Err(CmdError::new("the faces must be parallel"));
      }
      edges.extend(body_edges(&body.shape, &pl, Some(fid))?);
    }
    (edges, format!("{} face(s)", faces.len()))
  } else {
    let id = match p.get("body").and_then(Value::as_u64) {
      Some(b) => BodyId(u32::try_from(b).map_err(|_| CmdError::new("bad body id"))?),
      None => {
        let mut it = s.model.bodies.iter().filter(|b| b.sheet.as_ref().is_some_and(|i| !i.flat));
        match (it.next(), it.next()) {
          (Some(b), None) => b.id,
          (None, _) => {
            return Err(CmdError::new("nothing to cut: pass 'sketch', 'faces' or a sheet metal 'body'"));
          }
          _ => return Err(CmdError::new("there are several sheet metal bodies; pass 'body'")),
        }
      }
    };
    let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
    let face = match p.get("face") {
      Some(f) => {
        let r = face_ref(s, f)?;
        Some(r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?)
      }
      None => None,
    };
    let flat = body.flat_pattern(face).map_err(CmdError::new)?;
    (body_edges(&flat.shape, &flat.plane, None)?, format!("flat pattern of body {}", id.0))
  };
  let (mut cs, open) = fox_cut::chain(&edges, fox_cut::JOIN_TOL);
  if cs.is_empty() {
    return Err(CmdError::new(format!("{what} has no closed contours to cut")));
  }
  if opt_str(p, "origin") != Some("model") {
    let (mut lo, _) = cs[0].bounds();
    for c in &cs {
      let (l, _) = c.bounds();
      lo = [lo[0].min(l[0]), lo[1].min(l[1])];
    }
    for c in &mut cs {
      for seg in &mut c.segs {
        seg.a = [seg.a[0] - lo[0], seg.a[1] - lo[1]];
        seg.b = [seg.b[0] - lo[0], seg.b[1] - lo[1]];
      }
    }
  }
  Ok((cs, what, open))
}

fn contours_cmd(s: &Session, p: &Value) -> CmdResult {
  let (cs, what, open) = contours(s, p)?;
  let (depth, _) = fox_cut::nesting(&cs);
  Ok(json!({
    "source": what,
    "open_chains": open,
    "contours": cs.iter().zip(&depth).map(|(c, d)| {
      let (lo, hi) = c.bounds();
      json!({"hole": d % 2 == 1, "length": c.length(), "area": c.area().abs(), "min": lo, "max": hi, "segments": c.segs.len()})
    }).collect::<Vec<_>>(),
  }))
}

fn gcode(s: &Session, p: &Value) -> CmdResult {
  let m = machine(s, p)?;
  let params = params(&m, p)?;
  let (cs, what, open) = contours(s, p)?;
  let imperial = match opt_str(p, "units") {
    Some(u) => match u.trim().to_ascii_lowercase().as_str() {
      "mm" | "metric" => false,
      "in" | "inch" | "inches" | "imperial" => true,
      o => return Err(CmdError::new(format!("unknown units '{o}' (mm, inch)"))),
    },
    None => false,
  };
  let plan = plan(&cs, &params);
  let name = opt_str(p, "name").map(str::to_string).unwrap_or_else(|| s.design.name.clone());
  let text = m.output(&plan, &params, &name, imperial);
  let mut out = json!({
    "machine": m.id,
    "source": what,
    "contours": plan.paths.len(),
    "holes": plan.paths.iter().filter(|c| c.1).count(),
    "pierces": plan.pierces(),
    "cut_length": plan.cut_length(),
    "rapid_length": plan.rapid_length(),
    "time_s": plan.time(&params, m.rapid_rate),
    "kerf": params.kerf,
    "feed": params.feed,
    "open_chains": open,
    "warnings": plan.warnings,
    "extension": m.extension,
  });
  match opt_str(p, "path").filter(|f| !f.trim().is_empty()) {
    Some(path) => {
      std::fs::write(path, &text)?;
      out["path"] = json!(path);
      out["bytes"] = json!(text.len());
    }
    None => out["gcode"] = json!(text),
  }
  Ok(out)
}
