//! Cut-service checks and cut estimates (see [`fox_exchange::cutservice`]): a sketch, a sheet
//! metal body's flat pattern or a DXF file checked against a cutting service's profile, and
//! measured for a quote. Service profiles are the built-in ones and the hub's
//! `cut_services.json`.

use std::path::{Path, PathBuf};

use fox_exchange::cutservice::{self, CheckOptions, CutService};
use fox_exchange::flatcheck::{self, Layered, P2};
use fox_kernel::Plane;
use fox_model::{BodyId, FeatureId};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str, sheet_body};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "cut.service.list" => list(s),
    "cut.service.add" => add(s, p),
    "cut.service.delete" => delete(s, p),
    "cut.service.check" => check(s, p),
    "cut.estimate" => estimate(s, p),
    _ => return None,
  })
}

fn services_file(hub: &Path) -> PathBuf {
  hub.join("cut_services.json")
}

/// The hub's own service profiles.
fn user_services(hub: &Path) -> Result<Vec<CutService>, CmdError> {
  let f = services_file(hub);
  if !f.exists() {
    return Ok(Vec::new());
  }
  let text = std::fs::read_to_string(&f)?;
  serde_json::from_str(&text).map_err(|e| CmdError::new(format!("{}: {e}", f.display())))
}

fn write_services(hub: &Path, all: &[CutService]) -> Result<(), CmdError> {
  std::fs::create_dir_all(hub)?;
  std::fs::write(services_file(hub), serde_json::to_string_pretty(all).map_err(|e| CmdError::new(e.to_string()))?)?;
  Ok(())
}

/// Built-in services, then the hub's (a hub service with a built-in's id replaces it), each with
/// whether it is built in.
fn services(s: &Session) -> Result<Vec<(CutService, bool)>, CmdError> {
  let user = user_services(&s.hub_root())?;
  let mut all: Vec<(CutService, bool)> = cutservice::builtin_services()
    .into_iter()
    .filter(|b| !user.iter().any(|u| u.id.eq_ignore_ascii_case(&b.id)))
    .map(|b| (b, true))
    .collect();
  all.extend(user.into_iter().map(|u| (u, false)));
  Ok(all)
}

fn list(s: &Session) -> CmdResult {
  let all = services(s)?;
  Ok(json!(
    all
      .iter()
      .map(|(svc, builtin)| {
        let mut v = serde_json::to_value(svc).unwrap_or_default();
        v["builtin"] = json!(builtin);
        v
      })
      .collect::<Vec<_>>()
  ))
}

fn add(s: &mut Session, p: &Value) -> CmdResult {
  let svc: CutService = serde_json::from_value(p.get("service").cloned().unwrap_or_else(|| p.clone()))
    .map_err(|e| CmdError::new(format!("bad service profile: {e}")))?;
  if svc.id.trim().is_empty() {
    return Err(CmdError::new("a service needs an id"));
  }
  if svc.processes.is_empty() {
    return Err(CmdError::new("a service needs at least one process"));
  }
  let hub = s.hub_root();
  let mut user = user_services(&hub)?;
  user.retain(|u| !u.id.eq_ignore_ascii_case(&svc.id));
  user.push(svc.clone());
  write_services(&hub, &user)?;
  serde_json::to_value(&svc).map_err(|e| CmdError::new(e.to_string()))
}

fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let id = opt_str(p, "id").ok_or(CmdError::new("'id' is required"))?;
  let hub = s.hub_root();
  let mut user = user_services(&hub)?;
  let n = user.len();
  user.retain(|u| !u.id.eq_ignore_ascii_case(id));
  if user.len() == n {
    return Err(CmdError::new(format!("no service '{id}' in the hub (built-in services can't be deleted)")));
  }
  write_services(&hub, &user)?;
  Ok(json!({"deleted": id}))
}

/// What is checked or measured: the drawing, and how its coordinates map to the model.
struct Source {
  layered: Layered,
  what: String,
  /// Drawing coordinates → world: a plane and the shift added before it.
  frame: Option<(Plane, P2)>,
  /// Thickness and bend radius the source knows.
  sheet: Option<(f64, f64)>,
  /// Notes are FissionOxide's own (a flat pattern's bend and sheet notes).
  own_notes: bool,
  sketch: Option<u32>,
  body: Option<BodyId>,
}

impl Source {
  fn world(&self, at: P2) -> Option<[f64; 3]> {
    self.frame.as_ref().map(|(pl, o)| pl.to_world([at[0] + o[0], at[1] + o[1]]).to_array())
  }
}

fn id_of(v: &Value, what: &str) -> Result<u32, CmdError> {
  v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or(CmdError::new(format!("bad {what} id")))
}

fn sketch_source(s: &Session, id: u32) -> Result<Source, CmdError> {
  let r = s
    .model
    .sketches
    .get(&FeatureId(id))
    .ok_or(CmdError::new(format!("no computed sketch {id} (rolled back, or not a sketch?)")))?;
  let entities = crate::files::sketch_entities(&r.sketch, false)
    .into_iter()
    .map(|(e, construction)| (if construction { "CONSTRUCTION" } else { "0" }.to_string(), e))
    .collect();
  Ok(Source {
    layered: Layered { entities, texts: Vec::new() },
    what: format!("sketch {id}"),
    frame: Some((r.plane, [0.0, 0.0])),
    sheet: None,
    own_notes: true,
    sketch: Some(id),
    body: None,
  })
}

fn body_source(s: &Session, p: &Value, id: BodyId) -> Result<Source, CmdError> {
  let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
  let sheet = body.sheet.as_ref().ok_or(CmdError::new(format!("body {} is not sheet metal", id.0)))?;
  let face = match p.get("face") {
    Some(f) => {
      let r = face_ref(s, f)?;
      Some(r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?)
    }
    None => None,
  };
  let flat = body.flat_pattern(face).map_err(CmdError::new)?;
  let dxf = crate::sheet2::flat_dxf(body, &flat)?;
  let layered = flatcheck::read_layered(dxf.as_bytes()).map_err(|e| CmdError::new(e.to_string()))?;
  // the DXF has its lower-left corner at the origin: where that is on the flat's plane
  let mut o = [f64::INFINITY; 2];
  for l in flat.shape.edge_lines(0.01)? {
    for q in l.points {
      let q = flat.plane.to_local(q);
      o = [o[0].min(q[0]), o[1].min(q[1])];
    }
  }
  for b in &flat.bends {
    for q in [b.start, b.end] {
      let q = flat.plane.to_local(q);
      o = [o[0].min(q[0]), o[1].min(q[1])];
    }
  }
  let o = if o[0].is_finite() { o } else { [0.0, 0.0] };
  Ok(Source {
    layered,
    what: format!("flat pattern of body {}", id.0),
    frame: Some((flat.plane, o)),
    sheet: Some((sheet.thickness, sheet.bend_radius)),
    own_notes: true,
    sketch: None,
    body: Some(id),
  })
}

/// The source in `p`: `sketch`, `body` (a sheet metal body's flat pattern) or `dxf` (a file);
/// else the sketch being edited, else the only sheet metal body.
fn source(s: &Session, p: &Value) -> Result<Source, CmdError> {
  if let Some(path) = opt_str(p, "dxf").or_else(|| opt_str(p, "path")).filter(|f| !f.trim().is_empty()) {
    let bytes = std::fs::read(path)?;
    let layered = flatcheck::read_layered(&bytes).map_err(|e| CmdError::new(e.to_string()))?;
    let sheet = flatcheck::sheet_note(&layered.texts);
    return Ok(Source {
      layered,
      what: path.to_string(),
      frame: None,
      sheet,
      own_notes: false,
      sketch: None,
      body: None,
    });
  }
  if let Some(v) = p.get("sketch") {
    return sketch_source(s, id_of(v, "sketch")?);
  }
  // forms pass picked bodies as a list
  let body = match p.get("body") {
    Some(Value::Array(a)) => a.first().map(|v| id_of(v, "body")).transpose()?,
    Some(v) => Some(id_of(v, "body")?),
    None => None,
  };
  if let Some(b) = body {
    return body_source(s, p, BodyId(b));
  }
  if let Some(id) = s.edit_sketch {
    return sketch_source(s, id.0);
  }
  let id = sheet_body(s, &json!({})).map_err(|e| CmdError::new(format!("{}: pass 'sketch', 'body' or 'dxf'", e.0)))?;
  body_source(s, p, id)
}

/// A length in `p` (a number is mm, a string an expression such as "0.125 in").
fn length(s: &Session, p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::String(e)) if e.trim().is_empty() => Ok(None),
    Some(v) => {
      let e = v.as_f64().map(|x| x.to_string()).or_else(|| v.as_str().map(str::to_string)).unwrap_or_default();
      Ok(Some(fox_expr::eval(&e, fox_expr::Kind::Length, &s.model.params)?))
    }
  }
}

fn source_json(src: &Source) -> Value {
  let mut v = json!({"source": src.what});
  if let Some(id) = src.sketch {
    v["sketch"] = json!(id);
  }
  if let Some(id) = src.body {
    v["body"] = json!(id.0);
  }
  v
}

/// Adds world positions next to drawing ones (`at` → `world`, part `min`/`max` → `world_center`).
fn add_world(src: &Source, v: &mut Value) {
  if let Some(issues) = v.get_mut("issues").and_then(Value::as_array_mut) {
    for i in issues {
      let at = i.get("at").and_then(|a| Some([a.get(0)?.as_f64()?, a.get(1)?.as_f64()?]));
      if let Some(w) = at.and_then(|a| src.world(a)) {
        i["world"] = json!(w);
      }
    }
  }
  let parts =
    if v.get("parts").is_some() { v.get_mut("parts") } else { v.get_mut("estimate").and_then(|e| e.get_mut("parts")) };
  if let Some(parts) = parts.and_then(Value::as_array_mut) {
    for part in parts {
      let pt = |k: &str| part.get(k).and_then(|a| Some([a.get(0)?.as_f64()?, a.get(1)?.as_f64()?]));
      let center = pt("min").zip(pt("max")).map(|(lo, hi)| [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]);
      if let Some(w) = center.and_then(|c| src.world(c)) {
        part["world_center"] = json!(w);
      }
    }
  }
}

/// Writes `v` (JSON) or the estimate's CSV to `path`, by its extension.
fn export(path: &str, v: &Value, est: &cutservice::Estimate) -> Result<(), CmdError> {
  let csv = Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
  let text = if csv {
    cutservice::estimate_csv(est)
  } else {
    serde_json::to_string_pretty(v).map_err(|e| CmdError::new(e.to_string()))?
  };
  std::fs::write(path, text)?;
  Ok(())
}

fn check(s: &mut Session, p: &Value) -> CmdResult {
  let id = opt_str(p, "service").unwrap_or("sendcutsend");
  let all = services(s)?;
  let svc = all.iter().map(|(svc, _)| svc).find(|svc| svc.id.eq_ignore_ascii_case(id.trim())).ok_or_else(|| {
    CmdError::new(format!(
      "unknown service '{id}' ({})",
      all.iter().map(|(svc, _)| svc.id.as_str()).collect::<Vec<_>>().join(", ")
    ))
  })?;
  let material = opt_str(p, "material").filter(|m| !m.trim().is_empty());
  let process = svc.process_for(material, opt_str(p, "process")).map_err(CmdError::new)?;
  let src = source(s, p)?;
  let given = length(s, p, "thickness")?;
  if given.is_some_and(|t| t <= 0.0) {
    return Err(CmdError::new("the thickness must be positive"));
  }
  // the given thickness, the body's or the file's, else 1/8"
  let (thickness, assumed) = match (given, src.sheet) {
    (Some(t), _) => (t, false),
    (None, Some((t, _))) => (t, false),
    (None, None) => (0.125 * 25.4, true),
  };
  let o = CheckOptions {
    thickness,
    bend_radius: src.sheet.map(|(_, r)| r),
    own_notes: src.own_notes,
    material: material.map(str::to_string),
  };
  let report = cutservice::check(&src.layered, svc, process, &o);
  let mut out = serde_json::to_value(&report).map_err(|e| CmdError::new(e.to_string()))?;
  for (k, v) in source_json(&src).as_object().into_iter().flatten() {
    out[k] = v.clone();
  }
  out["thickness_assumed"] = json!(assumed);
  out["summary"] = json!(match report.status {
    cutservice::Status::Pass => format!("PASS: ready for {} ({})", svc.name, process.label()),
    cutservice::Status::Warnings => format!("WARNINGS: {} to review for {}", report.warnings, svc.name),
    cutservice::Status::Errors => {
      format!("ERRORS: {} error(s), {} warning(s) for {}", report.errors, report.warnings, svc.name)
    }
  });
  add_world(&src, &mut out);
  if let Some(path) = opt_str(p, "export").filter(|f| !f.trim().is_empty()) {
    export(path, &out, &report.estimate)?;
    out["export"] = json!(path);
  }
  Ok(out)
}

fn estimate(s: &mut Session, p: &Value) -> CmdResult {
  let src = source(s, p)?;
  let est = cutservice::estimate(&src.layered);
  if est.closed_contours == 0 {
    return Err(CmdError::new(format!("{} has no closed contours to cut", src.what)));
  }
  let mut out = serde_json::to_value(&est).map_err(|e| CmdError::new(e.to_string()))?;
  for (k, v) in source_json(&src).as_object().into_iter().flatten() {
    out[k] = v.clone();
  }
  add_world(&src, &mut out);
  if let Some(path) = opt_str(p, "export").filter(|f| !f.trim().is_empty()) {
    export(path, &out, &est)?;
    out["export"] = json!(path);
  }
  Ok(out)
}
