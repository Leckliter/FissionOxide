//! 2D cutting commands (laser, plasma, waterjet): contours from a flat pattern, faces or a
//! sketch, planned and posted by `fox-cut`, with the machine profiles in the hub's
//! `machines.json` next to the built-in ones.

use std::path::{Path, PathBuf};

use fox_cut::{Contour, CutParams, Edge, Lead, LeadKind, Machine, Nest, NestParams, NestPart, P, Plan, SegKind, plan};
use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind, EdgeInfo, Plane};
use fox_model::{BodyId, CutJob};
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
    "cut.nest" => nest_cmd(s, p),
    "cut.job.create" => job_create(s, p),
    "cut.job.edit" => job_edit(s, p),
    "cut.job.delete" => job_delete(s, p),
    "cut.job.list" => Ok(job_list(s)),
    "cut.job.preview" => job_preview(s, p),
    "cut.job.lead" => job_lead(s, p),
    _ => return None,
  })
}

/// A cut job's toolpaths for the viewport (`cut.job.preview`), for one design revision.
#[derive(Debug, Clone)]
pub struct CutPreview {
  pub job: u32,
  pub revision: u64,
  pub lines: Vec<CutLine>,
  /// The plane the cut lies on (a nest's sheets are laid out on it side by side) and how far
  /// above it the lines are drawn: where a click picks a lead point.
  pub plane: Plane,
  pub lift: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutLineKind {
  Outline,
  Hole,
  Lead,
  Rapid,
  Sheet,
}

#[derive(Debug, Clone)]
pub struct CutLine {
  pub points: Vec<DVec3>,
  pub kind: CutLineKind,
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
  if let Some(v) = p.get("lead_in").filter(|v| !v.is_null()) {
    c.lead_in = lead(v, c.lead_in)?;
  }
  if let Some(v) = p.get("lead_out").filter(|v| !v.is_null()) {
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
        SkSeg::Bezier { .. } | SkSeg::EllipseArc { .. } | SkSeg::Conic { .. } | SkSeg::Quintic { .. } => {
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

/// Where a cut's contours came from: a description, the plane they lie on and the shift that
/// put their lower-left corner at the origin (none with `origin: model`).
struct Source {
  contours: Vec<Contour>,
  what: String,
  open: usize,
  plane: Plane,
  shift: P,
}

impl Source {
  /// A point of the cut (its own coordinates) in the model.
  fn to_model(&self, q: P) -> DVec3 {
    self.plane.to_world([q[0] + self.shift[0], q[1] + self.shift[1]])
  }

  /// A model point on (or above) the source's plane in the cut's coordinates.
  fn to_cut(&self, p: DVec3) -> P {
    let l = self.plane.to_local(p);
    [l[0] - self.shift[0], l[1] - self.shift[1]]
  }
}

/// Closed contours of the source, the lower-left corner of their bounds at the origin unless
/// `origin` is "model".
fn source(s: &Session, p: &Value) -> Result<Source, CmdError> {
  let (edges, what, plane) = if let Some(id) = p.get("sketch").and_then(Value::as_u64) {
    let id = u32::try_from(id).map_err(|_| CmdError::new("bad sketch id"))?;
    let plane = s.model.sketches.get(&fox_model::FeatureId(id)).map(|r| r.plane).unwrap_or(Plane::XY);
    (sketch_edges(s, id)?, format!("sketch {id}"), plane)
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
    (edges, format!("{} face(s)", faces.len()), plane.unwrap_or(Plane::XY))
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
    (body_edges(&flat.shape, &flat.plane, None)?, format!("flat pattern of body {}", id.0), flat.plane)
  };
  let (mut cs, open) = fox_cut::chain(&edges, fox_cut::JOIN_TOL);
  if cs.is_empty() {
    return Err(CmdError::new(format!("{what} has no closed contours to cut")));
  }
  let mut shift = [0.0, 0.0];
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
    shift = lo;
  }
  Ok(Source { contours: cs, what, open, plane, shift })
}

fn contours_cmd(s: &Session, p: &Value) -> CmdResult {
  let src = source(s, p)?;
  let (depth, _) = fox_cut::nesting(&src.contours);
  Ok(json!({
    "source": src.what,
    "open_chains": src.open,
    "contours": src.contours.iter().zip(&depth).map(|(c, d)| {
      let (lo, hi) = c.bounds();
      json!({"hole": d % 2 == 1, "length": c.length(), "area": c.area().abs(), "min": lo, "max": hi, "segments": c.segs.len()})
    }).collect::<Vec<_>>(),
  }))
}

/// Lead points: `[x, y]` in the cut's coordinates or `[x, y, z]` in the model (projected onto
/// the source's plane).
fn lead_points(v: Option<&Value>, src: &Source) -> Result<Vec<P>, CmdError> {
  let mut out = Vec::new();
  for q in v.and_then(Value::as_array).into_iter().flatten() {
    let a: Vec<f64> = q.as_array().map(|a| a.iter().filter_map(num).collect()).unwrap_or_default();
    match a.len() {
      2 => out.push([a[0], a[1]]),
      3 => out.push(src.to_cut(DVec3::new(a[0], a[1], a[2]))),
      _ => return Err(CmdError::new("a lead point is [x, y] (cut coordinates) or [x, y, z] (model)")),
    }
  }
  Ok(out)
}

/// The nesting settings: `{sheet: [w, h] | "w x h" | {width, height}, spacing?, margin?,
/// rotations?, mirror?, max_sheets?}`. The spacing defaults to 3 mm or two kerfs.
fn nest_params(v: &Value, kerf: f64) -> Result<NestParams, CmdError> {
  let d = NestParams::default();
  let mut n = NestParams { spacing: d.spacing.max(2.0 * kerf), ..d };
  let size = |a: Option<&Value>, b: Option<&Value>| match (a.and_then(num), b.and_then(num)) {
    (Some(w), Some(h)) => Ok((w, h)),
    _ => Err(CmdError::new("the sheet is [width, height], \"width x height\" or {width, height}")),
  };
  let wh = match v.get("sheet") {
    Some(Value::Array(a)) if a.len() == 2 => Some(size(a.first(), a.get(1))?),
    Some(Value::Object(_)) => Some(size(v["sheet"].get("width"), v["sheet"].get("height"))?),
    Some(Value::String(t)) => {
      let mut it = t.split(['x', 'X', '×', '*']).map(|x| Value::String(x.trim().to_string()));
      Some(size(it.next().as_ref(), it.next().as_ref())?)
    }
    Some(Value::Null) | None => match (v.get("width"), v.get("height")) {
      (None, None) => None,
      (w, h) => Some(size(w, h)?),
    },
    Some(_) => return Err(CmdError::new("the sheet is [width, height], \"width x height\" or {width, height}")),
  };
  if let Some((w, h)) = wh {
    (n.sheet_width, n.sheet_height) = (w, h);
  }
  if let Some(x) = v.get("spacing").and_then(num) {
    n.spacing = x;
  }
  if let Some(x) = v.get("margin").and_then(num) {
    n.margin = x;
  }
  if let Some(x) = v.get("rotations").and_then(num) {
    if !(1.0..=360.0).contains(&x) {
      return Err(CmdError::new("'rotations' is how many orientations a part may take, 1 to 360"));
    }
    n.rotations = x.round() as u32;
  }
  if let Some(b) = v.get("mirror").and_then(Value::as_bool) {
    n.mirror = b;
  }
  if let Some(x) = v.get("max_sheets").and_then(num) {
    n.max_sheets = x.max(1.0).round() as u32;
  }
  if n.spacing < kerf {
    return Err(CmdError::new(format!("the spacing ({} mm) is less than the kerf ({kerf} mm)", n.spacing)));
  }
  Ok(n)
}

fn quantity(p: &Value) -> Result<u32, CmdError> {
  match p.get("quantity").map(num) {
    None => Ok(1),
    Some(Some(q)) if (0.0..=100_000.0).contains(&q) => Ok(q.round() as u32),
    _ => Err(CmdError::new("'quantity' is a count")),
  }
}

/// A cut worked out: one plan, or one per sheet of a nest.
struct Job {
  machine: Machine,
  params: CutParams,
  /// The main source, then those of `parts`.
  sources: Vec<Source>,
  plans: Vec<Plan>,
  /// The nest, its parts and the source each part came from.
  nest: Option<(Nest, Vec<NestPart>, Vec<usize>)>,
}

/// Gap drawn between the sheets of a nest in the preview (mm).
fn sheet_gap(n: &Nest) -> f64 {
  (n.width * 0.1).max(20.0)
}

fn job(s: &Session, p: &Value) -> Result<Job, CmdError> {
  let machine = machine(s, p)?;
  let mut params = params(&machine, p)?;
  let main = source(s, p)?;
  let nest_v = match p.get("nest") {
    None | Some(Value::Null) | Some(Value::Bool(false)) => None,
    Some(Value::Bool(true)) => Some(json!({})),
    Some(v @ Value::Object(_)) => Some(v.clone()),
    Some(_) => return Err(CmdError::new("'nest' is {sheet, spacing?, margin?, rotations?, mirror?, max_sheets?}")),
  };
  let Some(nv) = nest_v else {
    if p.get("parts").is_some() || p.get("quantity").and_then(num).is_some_and(|q| q != 1.0) {
      return Err(CmdError::new("quantities and extra parts are for nesting: pass 'nest' with a sheet size"));
    }
    params.lead_points = lead_points(p.get("lead_points"), &main)?;
    let plan = plan(&main.contours, &params);
    return Ok(Job { machine, params, sources: vec![main], plans: vec![plan], nest: None });
  };
  let np = nest_params(&nv, params.kerf)?;
  let mut items: Vec<(Source, u32, String, Vec<P>)> = Vec::new();
  let leads = lead_points(p.get("lead_points"), &main)?;
  let name = opt_str(p, "part_name").map(str::to_string).unwrap_or_else(|| main.what.clone());
  items.push((main, quantity(p)?, name, leads));
  for part in p.get("parts").and_then(Value::as_array).into_iter().flatten() {
    let src = source(s, part)?;
    let leads = lead_points(part.get("lead_points"), &src)?;
    let name = opt_str(part, "name").map(str::to_string).unwrap_or_else(|| src.what.clone());
    items.push((src, quantity(part)?, name, leads));
  }
  let mut parts = Vec::new();
  let mut part_source = Vec::new();
  for (si, (src, q, name, leads)) in items.iter().enumerate() {
    let groups = fox_cut::parts_of(&src.contours);
    let n = groups.len();
    // each lead point goes with the part whose contour is nearest
    let near = |g: &Vec<Contour>, l: P| g.iter().map(|c| c.closest(l).2).fold(f64::INFINITY, f64::min);
    for (gi, g) in groups.iter().enumerate() {
      let mine = leads
        .iter()
        .copied()
        .filter(|l| (0..n).min_by(|&a, &b| near(&groups[a], *l).total_cmp(&near(&groups[b], *l))) == Some(gi))
        .collect();
      let name = if n > 1 { format!("{name} #{}", gi + 1) } else { name.clone() };
      parts.push(NestPart { name, contours: g.clone(), quantity: *q, lead_points: mine });
      part_source.push(si);
    }
  }
  let nest = fox_cut::nest(&parts, &np).map_err(CmdError::new)?;
  let mut plans = Vec::new();
  for k in 0..nest.sheets.len() {
    let (cs, leads) = nest.sheet(&parts, k);
    let mut pk = params.clone();
    pk.lead_points = leads;
    let mut pl = plan(&cs, &pk);
    pl.sheet = Some(([0.0, 0.0], [nest.width, nest.height]));
    plans.push(pl);
  }
  let sources = items.into_iter().map(|i| i.0).collect();
  Ok(Job { machine, params, sources, plans, nest: Some((nest, parts, part_source)) })
}

fn imperial(p: &Value) -> Result<bool, CmdError> {
  match opt_str(p, "units") {
    Some(u) => match u.trim().to_ascii_lowercase().as_str() {
      "mm" | "metric" => Ok(false),
      "in" | "inch" | "inches" | "imperial" => Ok(true),
      o => Err(CmdError::new(format!("unknown units '{o}' (mm, inch)"))),
    },
    None => Ok(false),
  }
}

/// What a nest came to: sheets, utilisation and what was left out.
fn nest_report(nest: &Nest, parts: &[NestPart]) -> Value {
  let placed = |i: usize| nest.sheets.iter().flat_map(|s| &s.placements).filter(|p| p.part == i).count();
  json!({
    "sheet_size": [nest.width, nest.height],
    "sheets": nest.sheets.iter().map(|sh| json!({
      "parts": sh.placements.len(),
      "utilisation": sh.utilisation,
      "extent": sh.extent,
      "placements": sh.placements.iter().map(|p| json!({
        "part": parts[p.part].name, "rotation": p.rotation, "mirrored": p.mirrored, "offset": p.offset,
      })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>(),
    "parts": parts.iter().enumerate().map(|(i, p)| json!({
      "name": p.name, "quantity": p.quantity, "placed": placed(i), "area": p.area(),
    })).collect::<Vec<_>>(),
    "utilisation": nest.utilisation(parts),
    "unplaced": nest.unplaced.iter().map(|(i, n)| json!({"part": parts[*i].name, "count": n})).collect::<Vec<_>>(),
    "warnings": nest.warnings,
  })
}

/// The command's parameters on top of a stored job's (`job`), if it names one.
fn with_job(s: &Session, p: &Value) -> Result<(Value, Option<u32>), CmdError> {
  if p.get("job").is_none() {
    return Ok((p.clone(), None));
  }
  let i = job_index(s, p)?;
  let j = &s.design.cut_jobs[i];
  let mut merged = j.params.clone();
  if !merged.is_object() {
    merged = json!({});
  }
  for (k, v) in p.as_object().into_iter().flatten() {
    if k != "job" {
      merged[k] = v.clone();
    }
  }
  if merged.get("name").is_none() {
    merged["name"] = json!(j.name);
  }
  Ok((merged, Some(j.id)))
}

fn summary(j: &Job, sheet: Option<usize>) -> Value {
  let plans: Vec<&Plan> = match sheet {
    Some(k) => j.plans.get(k).into_iter().collect(),
    None => j.plans.iter().collect(),
  };
  let mut warnings: Vec<String> = j.nest.as_ref().map(|n| n.0.warnings.clone()).unwrap_or_default();
  for pl in &plans {
    for w in &pl.warnings {
      if !warnings.contains(w) {
        warnings.push(w.clone());
      }
    }
  }
  let sum = |f: &dyn Fn(&Plan) -> f64| plans.iter().map(|p| f(p)).sum::<f64>();
  let mut out = json!({
    "machine": j.machine.id,
    "source": j.sources.iter().map(|s| s.what.as_str()).collect::<Vec<_>>().join(", "),
    "contours": plans.iter().map(|p| p.paths.len()).sum::<usize>(),
    "holes": plans.iter().map(|p| p.paths.iter().filter(|c| c.1).count()).sum::<usize>(),
    "pierces": plans.iter().map(|p| p.pierces()).sum::<usize>(),
    "cut_length": sum(&|p| p.cut_length()),
    "rapid_length": sum(&|p| p.rapid_length()),
    "time_s": sum(&|p| p.time(&j.params, j.machine.rapid_rate)),
    "kerf": j.params.kerf,
    "feed": j.params.feed,
    "open_chains": j.sources.iter().map(|s| s.open).sum::<usize>(),
    "warnings": warnings,
    "extension": j.machine.extension,
  });
  if let Some((n, parts, _)) = &j.nest {
    out["nest"] = nest_report(n, parts);
  }
  out
}

fn gcode(s: &Session, p: &Value) -> CmdResult {
  let (p, _) = with_job(s, p)?;
  let j = job(s, &p)?;
  let imperial = imperial(&p)?;
  let sheet = match p.get("sheet").and_then(num) {
    Some(k) if k >= 1.0 && (k as usize) <= j.plans.len() => Some(k as usize - 1),
    Some(_) => return Err(CmdError::new(format!("'sheet' is 1 to {}", j.plans.len()))),
    None => None,
  };
  if j.plans.is_empty() {
    return Err(CmdError::new("nothing could be placed on a sheet"));
  }
  let name = opt_str(&p, "name").map(str::to_string).unwrap_or_else(|| s.design.name.clone());
  let program = |k: usize| {
    let n = if j.plans.len() > 1 { format!("{name} sheet {} of {}", k + 1, j.plans.len()) } else { name.clone() };
    j.machine.output(&j.plans[k], &j.params, &n, imperial)
  };
  let mut out = summary(&j, sheet);
  if j.nest.is_some() {
    out["sheets"] = json!(j.plans.len());
  }
  match opt_str(&p, "path").filter(|f| !f.trim().is_empty()) {
    Some(path) => {
      let ks: Vec<usize> = match sheet {
        Some(k) => vec![k],
        None => (0..j.plans.len()).collect(),
      };
      let mut paths = Vec::new();
      let mut bytes = 0;
      for &k in &ks {
        let f = if ks.len() > 1 { sheet_path(path, k + 1) } else { PathBuf::from(path) };
        let text = program(k);
        std::fs::write(&f, &text)?;
        bytes += text.len();
        paths.push(f.display().to_string());
      }
      out["path"] = json!(paths[0]);
      if paths.len() > 1 {
        out["paths"] = json!(paths);
      }
      out["bytes"] = json!(bytes);
    }
    None => {
      out["gcode"] = json!(program(sheet.unwrap_or(0)));
      if sheet.is_none() && j.plans.len() > 1 {
        out["programs"] = json!((0..j.plans.len()).map(program).collect::<Vec<_>>());
      }
    }
  }
  Ok(out)
}

/// `cut.dxf` → `cut-sheet2.dxf`.
fn sheet_path(path: &str, k: usize) -> PathBuf {
  let p = Path::new(path);
  let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
  let name = match p.extension() {
    Some(e) => format!("{stem}-sheet{k}.{}", e.to_string_lossy()),
    None => format!("{stem}-sheet{k}"),
  };
  p.with_file_name(name)
}

fn nest_cmd(s: &Session, p: &Value) -> CmdResult {
  let (mut p, _) = with_job(s, p)?;
  if p.get("nest").is_none_or(|n| n.is_null() || n == &json!(false)) {
    p["nest"] = json!({});
  }
  let j = job(s, &p)?;
  let (n, parts, _) = j.nest.as_ref().ok_or(CmdError::new("nothing to nest"))?;
  let mut out = nest_report(n, parts);
  out["cut"] = json!(j.plans.iter().map(|pl| json!({
    "contours": pl.paths.len(), "pierces": pl.pierces(), "cut_length": pl.cut_length(),
    "time_s": pl.time(&j.params, j.machine.rapid_rate),
  })).collect::<Vec<_>>());
  Ok(out)
}

// ---------------------------------------------------------------- cut jobs kept in the design

/// The index of the job named by `job` (id or name), or the last one.
fn job_index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  let jobs = &s.design.cut_jobs;
  if jobs.is_empty() {
    return Err(CmdError::new("the design has no cut jobs (cut.job.create)"));
  }
  match p.get("job") {
    None | Some(Value::Null) => Ok(jobs.len() - 1),
    Some(Value::String(n)) => {
      jobs.iter().position(|j| j.name.eq_ignore_ascii_case(n.trim())).ok_or(CmdError::new(format!("no cut job '{n}'")))
    }
    Some(v) => {
      let id = v.as_u64().ok_or(CmdError::new("'job' is an id or a name"))?;
      jobs.iter().position(|j| u64::from(j.id) == id).ok_or(CmdError::new(format!("no cut job {id}")))
    }
  }
}

/// Keys of `cut.gcode` that belong to one run, not to a stored job.
const RUN_KEYS: [&str; 4] = ["job", "path", "sheet", "name"];

fn job_create(s: &mut Session, p: &Value) -> CmdResult {
  let mut params = json!({});
  for (k, v) in p.as_object().into_iter().flatten() {
    if !RUN_KEYS.contains(&k.as_str()) && !v.is_null() {
      params[k] = v.clone();
    }
  }
  let j = job(s, &params)?;
  let id = s.design.cut_jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
  let name = opt_str(p, "name").filter(|n| !n.trim().is_empty()).map(str::to_string).unwrap_or_else(|| format!("Cut{id}"));
  let mut out = summary(&j, None);
  s.edit(|d| {
    d.cut_jobs.push(CutJob { id, name: name.clone(), params });
    Ok(())
  })?;
  out["job"] = json!(id);
  out["name"] = json!(name);
  Ok(out)
}

fn job_edit(s: &mut Session, p: &Value) -> CmdResult {
  let i = job_index(s, p)?;
  let mut params = s.design.cut_jobs[i].params.clone();
  if !params.is_object() {
    params = json!({});
  }
  let obj = params.as_object_mut().expect("an object");
  for (k, v) in p.as_object().into_iter().flatten() {
    if RUN_KEYS.contains(&k.as_str()) {
      continue;
    }
    if v.is_null() {
      obj.remove(k);
    } else {
      obj.insert(k.clone(), v.clone());
    }
  }
  let j = job(s, &params)?;
  let name = opt_str(p, "name").filter(|n| !n.trim().is_empty()).map(str::to_string);
  let mut out = summary(&j, None);
  s.edit(|d| {
    let job = &mut d.cut_jobs[i];
    job.params = params;
    if let Some(n) = name {
      job.name = n;
    }
    Ok(())
  })?;
  out["job"] = json!(s.design.cut_jobs[i].id);
  out["name"] = json!(s.design.cut_jobs[i].name);
  Ok(out)
}

fn job_delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = job_index(s, p)?;
  let id = s.design.cut_jobs[i].id;
  s.edit(|d| {
    d.cut_jobs.remove(i);
    Ok(())
  })?;
  if s.cut_preview.as_ref().is_some_and(|c| c.job == id) {
    s.cut_preview = None;
  }
  Ok(json!({"deleted": id}))
}

fn job_list(s: &Session) -> Value {
  json!(s.design.cut_jobs.iter().map(|j| json!({"job": j.id, "name": j.name, "params": j.params})).collect::<Vec<_>>())
}

/// Computes a job and keeps its toolpaths for the viewport (`s.cut_preview`).
fn job_preview(s: &mut Session, p: &Value) -> CmdResult {
  let i = job_index(s, p)?;
  let id = s.design.cut_jobs[i].id;
  let (params, _) = with_job(s, &json!({"job": id}))?;
  let j = match job(s, &params) {
    Ok(j) => j,
    Err(e) => {
      s.cut_preview = None;
      return Err(e);
    }
  };
  let src = &j.sources[0];
  // a hair above the source's plane, so the paths show over the face they lie on
  let lift = src.plane.normal() * 0.02;
  let mut lines = Vec::new();
  let gap = j.nest.as_ref().map(|n| n.0.width + sheet_gap(&n.0)).unwrap_or(0.0);
  for (k, pl) in j.plans.iter().enumerate() {
    let dx = gap * k as f64;
    let to3 = |q: P| src.to_model([q[0] + dx, q[1]]) + lift;
    if let Some((lo, hi)) = pl.sheet {
      let c = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]], lo];
      lines.push(CutLine { points: c.iter().map(|q| to3(*q)).collect(), kind: CutLineKind::Sheet });
    }
    let mut at = [0.0, 0.0];
    for pass in &pl.passes {
      lines.push(CutLine { points: vec![to3(at), to3(pass.start())], kind: CutLineKind::Rapid });
      let mut run: Vec<DVec3> = Vec::new();
      let mut kind = None;
      for (seg, sk) in &pass.segs {
        let k = match sk {
          SegKind::Cut if pass.hole => CutLineKind::Hole,
          SegKind::Cut => CutLineKind::Outline,
          _ => CutLineKind::Lead,
        };
        if kind != Some(k) {
          if let Some(prev) = kind
            && run.len() > 1
          {
            lines.push(CutLine { points: std::mem::take(&mut run), kind: prev });
          }
          run = vec![to3(seg.a)];
          kind = Some(k);
        }
        let mut pts = Vec::new();
        seg.flatten(0.05, &mut pts);
        run.extend(pts.into_iter().map(to3));
      }
      if let Some(k) = kind
        && run.len() > 1
      {
        lines.push(CutLine { points: run, kind: k });
      }
      at = pass.end();
    }
  }
  let out = summary(&j, None);
  s.cut_preview = Some(CutPreview { job: id, revision: s.revision, lines, plane: src.plane, lift: 0.02 });
  let mut out = out;
  out["job"] = json!(id);
  Ok(out)
}

/// Picks where one contour's lead-in starts: `point` (model, on the preview) goes to the job's
/// lead points, replacing any other on the same contour; `clear` removes them all.
fn job_lead(s: &mut Session, p: &Value) -> CmdResult {
  let i = job_index(s, p)?;
  let id = s.design.cut_jobs[i].id;
  let mut stored = s.design.cut_jobs[i].params.clone();
  if p.get("clear").and_then(Value::as_bool) == Some(true) {
    if let Some(o) = stored.as_object_mut() {
      o.remove("lead_points");
    }
    for part in stored.get_mut("parts").and_then(Value::as_array_mut).into_iter().flatten() {
      if let Some(o) = part.as_object_mut() {
        o.remove("lead_points");
      }
    }
  } else {
    let a: Vec<f64> =
      p.get("point").and_then(Value::as_array).map(|a| a.iter().filter_map(num).collect()).unwrap_or_default();
    if a.len() != 3 && a.len() != 2 {
      return Err(CmdError::new("'point' is [x, y, z] in the model (or [x, y] in the cut's coordinates)"));
    }
    let (params, _) = with_job(s, &json!({"job": id}))?;
    let j = job(s, &params)?;
    let main = &j.sources[0];
    let q = if a.len() == 3 { main.to_cut(DVec3::new(a[0], a[1], a[2])) } else { [a[0], a[1]] };
    // the source and its own coordinates the point is in
    let (si, local) = match &j.nest {
      None => (0, q),
      Some((n, parts, part_source)) => {
        let gap = n.width + sheet_gap(n);
        let k = ((q[0] / gap).floor().max(0.0) as usize).min(n.sheets.len().saturating_sub(1));
        let sq = [q[0] - gap * k as f64, q[1]];
        let mut best: Option<(f64, usize, P)> = None;
        for pl in n.sheets.get(k).map(|s| &s.placements[..]).unwrap_or(&[]) {
          for c in &parts[pl.part].contours {
            let d = pl.contour(c).closest(sq).2;
            if best.is_none_or(|b| d < b.0) {
              best = Some((d, pl.part, pl.invert(sq)));
            }
          }
        }
        let (_, part, l) = best.ok_or(CmdError::new("there is no part on that sheet"))?;
        (part_source[part], l)
      }
    };
    let src = &j.sources[si];
    let nearest = |l: P| {
      (0..src.contours.len()).min_by(|&x, &y| src.contours[x].closest(l).2.total_cmp(&src.contours[y].closest(l).2))
    };
    let target = nearest(local);
    let slot = if si == 0 { &mut stored } else { &mut stored["parts"][si - 1] };
    let old = lead_points(slot.get("lead_points"), src)?;
    let raw: Vec<Value> = slot.get("lead_points").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut keep: Vec<Value> = raw.into_iter().zip(old).filter(|(_, l)| nearest(*l) != target).map(|(v, _)| v).collect();
    // kept as a model point, so it stays put when the cut's lower-left corner moves
    let m = src.to_model(local);
    keep.push(json!([m.x, m.y, m.z]));
    slot["lead_points"] = json!(keep);
  }
  let j = job(s, &stored)?;
  s.edit(|d| {
    d.cut_jobs[i].params = stored;
    Ok(())
  })?;
  let mut out = summary(&j, None);
  out["job"] = json!(id);
  Ok(out)
}
