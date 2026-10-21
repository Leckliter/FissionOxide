//! Manufacture commands: setups, milling operations, the tool library, toolpaths, the stock
//! simulation and G-code.

use fox_model::BodyId;
use fox_model::cam::{self, Entry, Library, OpKind, Operation, Setup, Tabs, Tool, ToolKind};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str, req_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "cam.setup" => setup(s, p),
    "cam.face" | "cam.pocket" | "cam.contour" | "cam.drill" | "cam.adaptive" | "cam.parallel" | "cam.scallop" => {
      operation(s, name, p)
    }
    "cam.adaptive3d" | "cam.pencil" | "cam.steep_shallow" | "cam.contour3d" => operation(s, name, p),
    "cam.generate" => generate(s, p),
    "cam.post" => post(s, p),
    "cam.list" => Ok(json!(
      s.design
        .cam
        .iter()
        .map(|st| json!({
          "setup": st.id, "name": st.name, "body": st.body.map(|b| b.0),
          "post": cam::post::find(&st.post).map(|p| p.id), "units": if st.imperial { "inch" } else { "mm" },
          "operations": st.operations.iter().map(|o| json!({
            "name": o.name, "tool": o.tool.number, "diameter": o.tool.diameter, "kind": o.tool.kind.id(),
          })).collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>()
    )),
    "cam.delete" => delete(s, p),
    "cam.tools" => library(s).map(|l| json!(l.tools.iter().map(tool_json).collect::<Vec<_>>())),
    "cam.tool.add" => tool_add(s, p),
    "cam.tool.delete" => tool_delete(s, p),
    "cam.posts" => Ok(json!(
      cam::post::POSTS
        .iter()
        .map(|p| json!({
          "id": p.id, "name": p.name, "extension": p.extension,
        }))
        .collect::<Vec<_>>()
    )),
    "cam.simulate" => simulate(s, p),
    _ => return None,
  })
}

/// A number, or an expression string ("6 mm", "0.4").
fn num(p: &Value, k: &str, default: f64) -> f64 {
  match p.get(k) {
    Some(Value::String(e)) => fox_expr::eval(e, fox_expr::Kind::Length, &std::collections::HashMap::new())
      .or_else(|_| fox_expr::eval(e, fox_expr::Kind::Unitless, &std::collections::HashMap::new()))
      .unwrap_or(default),
    Some(v) => v.as_f64().unwrap_or(default),
    None => default,
  }
}

fn flag(p: &Value, k: &str) -> bool {
  match p.get(k) {
    Some(Value::Bool(b)) => *b,
    Some(Value::String(t)) => matches!(t.trim(), "true" | "yes" | "1"),
    Some(v) => v.as_f64().is_some_and(|x| x != 0.0),
    None => false,
  }
}

/// The setup a command names, or the only/last one.
fn setup_index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  match p.get("setup").and_then(Value::as_u64) {
    Some(id) => s.design.cam.iter().position(|st| st.id as u64 == id).ok_or(CmdError::new(format!("no setup {id}"))),
    None if s.design.cam.is_empty() => Err(CmdError::new("there is no setup: cam.setup first")),
    None => Ok(s.design.cam.len() - 1),
  }
}

/// A post-processor id from `post`, checked.
fn post_of(p: &Value) -> Result<Option<String>, CmdError> {
  match opt_str(p, "post").filter(|t| !t.trim().is_empty()) {
    Some(id) => cam::post::find(id)
      .map(|p| Some(p.id.to_string()))
      .ok_or_else(|| CmdError::new(format!("unknown post-processor '{id}' ({})", cam::post::ids().join(", ")))),
    None => Ok(None),
  }
}

/// `units`: true for inches.
fn imperial_of(p: &Value) -> Result<Option<bool>, CmdError> {
  match opt_str(p, "units").filter(|t| !t.trim().is_empty()) {
    Some(u) => cam::post::Units::parse(u)
      .map(|u| Some(u == cam::post::Units::Imperial))
      .ok_or_else(|| CmdError::new(format!("unknown units '{u}' (mm or inch)"))),
    None => Ok(None),
  }
}

fn setup(s: &mut Session, p: &Value) -> CmdResult {
  let body = p.get("body").and_then(Value::as_u64).map(|b| BodyId(b as u32));
  if let Some(b) = body
    && s.model.body(b).is_none()
  {
    return Err(CmdError::new(format!("no body {}", b.0)));
  }
  if s.model.bodies.is_empty() {
    return Err(CmdError::new("there is nothing to machine"));
  }
  let id = s.design.cam.iter().map(|st| st.id).max().unwrap_or(0) + 1;
  let st = Setup {
    id,
    name: opt_str(p, "name").filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Setup{id}")),
    body,
    stock_side: num(p, "stock_side", 2.0),
    stock_top: num(p, "stock_top", 1.0),
    operations: Vec::new(),
    post: post_of(p)?.unwrap_or_default(),
    imperial: imperial_of(p)?.unwrap_or(false),
    straight: p.get("arcs").is_some() && !flag(p, "arcs"),
    arc_tolerance: num(p, "arc_tolerance", 0.0).max(0.0),
  };
  let (lo, hi) = cam::stock(&st, &s.model).map_err(CmdError::new)?;
  s.edit(|d| {
    d.cam.push(st);
    Ok(())
  })?;
  Ok(json!({"setup": id, "stock": [lo.to_array(), hi.to_array()]}))
}

fn faces_of(s: &Session, p: &Value, k: &str) -> Result<Vec<fox_model::FaceRef>, CmdError> {
  let mut out = Vec::new();
  for f in p.get(k).and_then(Value::as_array).into_iter().flatten() {
    out.push(face_ref(s, f)?);
  }
  Ok(out)
}

/// The hub's tool library.
fn library(s: &Session) -> Result<Library, CmdError> {
  Library::load(&s.hub_dir()).map_err(CmdError::new)
}

fn tool_json(t: &Tool) -> Value {
  json!({
    "number": t.number, "name": t.name, "kind": t.kind.id(), "diameter": t.diameter,
    "corner_radius": t.corner_radius, "angle": t.angle, "tip_diameter": t.tip_diameter,
    "flute_length": t.flute_length, "flutes": t.flutes, "rpm": t.rpm, "feed": t.feed, "plunge": t.plunge,
    "holder_diameter": t.holder_diameter, "stickout": t.stickout,
    "description": t.describe(),
  })
}

/// A tool's fields from JSON over `base`.
fn tool_fields(t: &Value, base: Tool) -> Result<Tool, CmdError> {
  let kind = match opt_str(t, "kind").filter(|k| !k.trim().is_empty()) {
    Some(k) => ToolKind::parse(k).ok_or_else(|| {
      CmdError::new(format!(
        "unknown tool kind '{k}' ({})",
        ToolKind::ALL.iter().map(|k| k.id()).collect::<Vec<_>>().join(", ")
      ))
    })?,
    None => base.kind,
  };
  let tool = Tool {
    kind,
    name: opt_str(t, "name").map(str::to_string).unwrap_or(base.name.clone()),
    diameter: num(t, "diameter", base.diameter),
    corner_radius: num(t, "corner_radius", base.corner_radius),
    angle: num(t, "angle", base.angle),
    tip_diameter: num(t, "tip_diameter", base.tip_diameter),
    flute_length: num(t, "flute_length", base.flute_length),
    flutes: num(t, "flutes", base.flutes as f64).max(0.0) as u32,
    rpm: num(t, "rpm", base.rpm),
    feed: num(t, "feed", base.feed),
    plunge: num(t, "plunge", base.plunge),
    holder_diameter: num(t, "holder_diameter", base.holder_diameter),
    stickout: num(t, "stickout", base.stickout),
    ..base
  };
  tool.check().map_err(CmdError::new)?;
  Ok(tool)
}

fn tool_add(s: &mut Session, p: &Value) -> CmdResult {
  let mut lib = library(s)?;
  let number = match p.get("number").map(|_| num(p, "number", 0.0)) {
    Some(n) if n >= 1.0 && n.fract() == 0.0 => n as u32,
    Some(_) => return Err(CmdError::new("the tool number must be a whole number from 1")),
    None => lib.tools.iter().map(|t| t.number).max().unwrap_or(0) + 1,
  };
  let base = lib.get(number).cloned().unwrap_or_else(|| Tool::flat(number, 0.0));
  if base.diameter == 0.0 && p.get("diameter").is_none() {
    return Err(CmdError::new("missing 'diameter'"));
  }
  let tool = tool_fields(p, Tool { number, ..base })?;
  let out = tool_json(&tool);
  lib.put(tool);
  lib.save(&s.hub_dir()).map_err(CmdError::new)?;
  Ok(out)
}

fn tool_delete(s: &mut Session, p: &Value) -> CmdResult {
  let mut lib = library(s)?;
  let number = num(p, "number", 0.0) as u32;
  if !lib.remove(number) {
    return Err(CmdError::new(format!("the library has no tool {number}")));
  }
  lib.save(&s.hub_dir()).map_err(CmdError::new)?;
  Ok(json!({"deleted": number}))
}

/// An operation's tool: a library tool (`tool_number`, `tool: 3` or `tool: {number}` without a
/// diameter), its fields overridden by `tool: {...}` and the top-level feeds; or a tool given
/// in full (`tool: {diameter, ...}` or `tool_diameter`), numbered like an earlier tool of the
/// same diameter.
fn op_tool(s: &Session, p: &Value) -> Result<Tool, CmdError> {
  let mut t = match p.get("tool") {
    Some(Value::Number(n)) => json!({"number": n}),
    Some(v) => v.clone(),
    None => json!({}),
  };
  // a library tool keeps its own shape
  if let Some(k) = p.get("tool_kind")
    && t.get("kind").is_none()
    && t.is_object()
    && !(p.get("tool_number").is_some() && num(p, "tool_number", 0.0) >= 1.0)
  {
    t["kind"] = k.clone();
  }
  let wanted = match (p.get("tool_number").map(|_| num(p, "tool_number", 0.0)), t.get("number")) {
    (Some(n), _) if n >= 1.0 => Some(n as u32),
    (_, Some(_)) if t.get("diameter").is_none() && p.get("tool_diameter").is_none() => {
      Some(num(&t, "number", 0.0) as u32)
    }
    _ => None,
  };
  let base = match wanted {
    Some(n) => library(s)?.get(n).cloned().ok_or_else(|| CmdError::new(format!("the tool library has no tool {n}")))?,
    None => {
      let diameter = match (t.get("diameter"), p.get("tool_diameter")) {
        (Some(_), _) => num(&t, "diameter", 0.0),
        (None, Some(_)) => num(p, "tool_diameter", 0.0),
        _ => {
          return Err(CmdError::new(
            "missing the tool ('tool_number' from the library, 'tool': {diameter} or 'tool_diameter')",
          ));
        }
      };
      // tools are numbered by diameter unless given
      let used: Vec<&Tool> = s.design.cam.iter().flat_map(|st| st.operations.iter().map(|o| &o.tool)).collect();
      let number = t.get("number").and_then(Value::as_u64).map(|n| n as u32).unwrap_or_else(|| {
        used
          .iter()
          .find(|x| (x.diameter - diameter).abs() < 1e-9)
          .map(|x| x.number)
          .unwrap_or_else(|| used.iter().map(|x| x.number).max().unwrap_or(0) + 1)
      });
      Tool { rpm: 10000.0, feed: 1000.0, plunge: 300.0, ..Tool::flat(number, diameter) }
    }
  };
  // feeds given with the operation win over the tool's
  let mut tool = tool_fields(&t, base)?;
  if p.get("feed").is_some() && t.get("feed").is_none() {
    tool.feed = num(p, "feed", tool.feed);
  }
  if p.get("plunge").is_some() && t.get("plunge").is_none() {
    tool.plunge = num(p, "plunge", tool.plunge);
  }
  if p.get("rpm").is_some() && t.get("rpm").is_none() {
    tool.rpm = num(p, "rpm", tool.rpm);
  }
  tool.check().map_err(CmdError::new)?;
  Ok(tool)
}

fn operation(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let tool = op_tool(s, p)?;
  let kind = match name {
    "cam.face" => OpKind::Face,
    "cam.pocket" | "cam.adaptive" => {
      let floors = faces_of(s, p, "floors")?;
      if floors.is_empty() {
        return Err(CmdError::new("missing 'floors' (the pocket floor faces)"));
      }
      if name == "cam.adaptive" {
        OpKind::Adaptive { floors }
      } else {
        OpKind::Pocket { floors, rest: flag(p, "rest") }
      }
    }
    "cam.contour" => {
      let tabs = match p.get("tabs") {
        Some(Value::Object(_)) => Some(Tabs {
          count: num(&p["tabs"], "count", 0.0).max(0.0) as u32,
          width: num(&p["tabs"], "width", 5.0),
          height: num(&p["tabs"], "height", 2.0),
        }),
        Some(_) => Some(Tabs {
          count: num(p, "tabs", 0.0).max(0.0) as u32,
          width: num(p, "tab_width", 5.0),
          height: num(p, "tab_height", 2.0),
        }),
        None => None,
      }
      .filter(|t| t.count > 0);
      OpKind::Contour { faces: faces_of(s, p, "faces")?, inside: flag(p, "inside"), tabs }
    }
    "cam.parallel" => OpKind::Parallel { faces: faces_of(s, p, "faces")?, angle: num(p, "angle", 0.0) },
    "cam.scallop" => OpKind::Scallop { faces: faces_of(s, p, "faces")? },
    "cam.adaptive3d" => OpKind::Adaptive3d,
    "cam.pencil" => OpKind::Pencil { faces: faces_of(s, p, "faces")? },
    "cam.steep_shallow" => {
      OpKind::SteepShallow { faces: faces_of(s, p, "faces")?, threshold: num(p, "threshold", 45.0) }
    }
    "cam.contour3d" => OpKind::Contour3d { faces: faces_of(s, p, "faces")?, min_slope: num(p, "min_slope", 0.0) },
    _ => OpKind::Drill { holes: faces_of(s, p, "holes")?, peck: num(p, "peck", 0.0) },
  };
  let label = match name {
    "cam.face" => "Face",
    "cam.pocket" => "2D Pocket",
    "cam.contour" => "2D Contour",
    "cam.adaptive" => "2D Adaptive",
    "cam.parallel" => "Parallel",
    "cam.scallop" => "Scallop",
    "cam.adaptive3d" => "Adaptive",
    "cam.pencil" => "Pencil",
    "cam.steep_shallow" => "Steep and Shallow",
    "cam.contour3d" => "Contour",
    _ => "Drill",
  };
  // adaptive clearing takes light, deep cuts and spirals in by default
  let (stepover, stepdown, entry) = match name {
    "cam.adaptive" => (0.1, 10.0, Entry::Helix),
    "cam.parallel" | "cam.scallop" | "cam.pencil" => (0.1, 2.0, Entry::Plunge),
    "cam.adaptive3d" => (0.15, 3.0, Entry::Helix),
    "cam.steep_shallow" | "cam.contour3d" => (0.1, 0.5, Entry::Plunge),
    _ => (0.4, 2.0, Entry::Plunge),
  };
  // roughing leaves some for finishing
  let stock_to_leave = if name == "cam.adaptive3d" { 0.3 } else { 0.0 };
  let entry = match opt_str(p, "entry").filter(|e| !e.trim().is_empty()) {
    Some(e) => Entry::parse(e).ok_or_else(|| CmdError::new(format!("unknown entry '{e}' (plunge, ramp, helix)")))?,
    None => entry,
  };
  let n = s.design.cam[i].operations.iter().filter(|o| o.name.starts_with(label)).count() + 1;
  let op = Operation {
    name: opt_str(p, "name").filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("{label}{n}")),
    kind,
    tool,
    stepover: num(p, "stepover", stepover),
    stepdown: num(p, "stepdown", stepdown),
    stock_to_leave: num(p, "stock_to_leave", stock_to_leave),
    entry,
    ramp_angle: num(p, "ramp_angle", 5.0),
    helix_diameter: num(p, "helix_diameter", 0.0),
  };
  // check it computes, after the setup's other operations, before keeping it
  let mut probe = s.design.cam[i].clone();
  probe.operations.push(op.clone());
  let paths = cam::toolpaths(&probe, &s.model).map_err(CmdError::new)?;
  let moves = paths.last().map(|t| t.moves.len()).unwrap_or(0);
  let opname = op.name.clone();
  let number = op.tool.number;
  s.edit(|d| {
    d.cam[i].operations.push(op);
    Ok(())
  })?;
  Ok(json!({"operation": opname, "moves": moves, "tool": number}))
}

/// Computes the toolpaths of a setup (shown in the viewport) and reports their length and time.
fn generate(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let st = &s.design.cam[i];
  let paths = cam::toolpaths(st, &s.model).map_err(CmdError::new)?;
  let mut at = cam::origin(st, &s.model).map_err(CmdError::new)?;
  let mut report = Vec::new();
  let mut total = 0.0;
  for t in &paths {
    let (cut, rapid, time) = t.stats(at);
    total += time;
    at = t.moves.last().map(|m| m.to).unwrap_or(at);
    report.push(json!({
      "operation": t.name, "tool": t.tool.number, "diameter": t.tool.diameter, "moves": t.moves.len(),
      "cutting_mm": cut, "rapid_mm": rapid, "minutes": time,
    }));
  }
  let id = st.id;
  s.toolpaths = paths;
  s.toolpaths_of = Some((id, s.revision));
  Ok(json!({"setup": id, "operations": report, "minutes": total}))
}

/// Writes a setup's G-code with its post-processor (or `post`/`units`); returns it when no
/// path is given.
fn post(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let mut st = s.design.cam[i].clone();
  if let Some(id) = post_of(p)? {
    st.post = id;
  }
  if let Some(imperial) = imperial_of(p)? {
    st.imperial = imperial;
  }
  if p.get("arcs").is_some() {
    st.straight = !flag(p, "arcs");
  }
  if p.get("arc_tolerance").is_some() {
    st.arc_tolerance = num(p, "arc_tolerance", 0.0).max(0.0);
  }
  let paths = cam::toolpaths(&st, &s.model).map_err(CmdError::new)?;
  if paths.is_empty() {
    return Err(CmdError::new("the setup has no operations"));
  }
  let origin = cam::origin(&st, &s.model).map_err(CmdError::new)?;
  let code = cam::gcode(&st, &paths, origin);
  let used = cam::post::find(&st.post).map(|p| p.id).unwrap_or("fanuc");
  let units = if st.imperial { "inch" } else { "mm" };
  let arcs = code
    .lines()
    .filter(|l| l.starts_with("G2 ") || l.starts_with("G3 ") || l.starts_with("G02 ") || l.starts_with("G03 "))
    .count();
  match opt_str(p, "path") {
    Some(path) => {
      std::fs::write(path, &code)?;
      Ok(json!({"path": path, "lines": code.lines().count(), "post": used, "units": units, "arcs": arcs}))
    }
    None => Ok(json!({"gcode": code, "post": used, "units": units, "arcs": arcs})),
  }
}

/// Runs a setup's toolpaths through its stock: removed volume and warnings (gouges, rapids
/// into the stock, cuts deeper than the flutes). The result is drawn in the viewport.
fn simulate(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let st = &s.design.cam[i];
  let paths = cam::toolpaths(st, &s.model).map_err(CmdError::new)?;
  if paths.is_empty() {
    return Err(CmdError::new("the setup has no operations"));
  }
  let sim = cam::sim::simulate(st, &s.model, &paths).map_err(CmdError::new)?;
  let warnings: Vec<Value> = sim
    .warnings
    .iter()
    .map(|w| {
      json!({
        "operation": w.operation, "kind": w.kind.id(), "message": w.message(), "at": w.at.to_array(),
        "depth": w.depth, "moves": w.moves,
      })
    })
    .collect();
  let out = json!({
    "setup": st.id,
    "cell": sim.stock.h,
    "removed": sim.removed.iter().map(|(n, v)| json!({"operation": n, "mm3": v})).collect::<Vec<_>>(),
    "remaining_mm3": sim.remaining(),
    "warnings": warnings,
  });
  let id = st.id;
  s.toolpaths = paths;
  s.toolpaths_of = Some((id, s.revision));
  s.simulation = Some((id, s.revision, sim));
  Ok(out)
}

fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let op = p.get("operation").map(|_| req_str(p, "operation").map(str::to_string)).transpose()?;
  s.edit(|d| {
    match &op {
      Some(name) => {
        let ops = &mut d.cam[i].operations;
        let before = ops.len();
        ops.retain(|o| o.name != *name);
        if ops.len() == before {
          return Err(CmdError::new(format!("no operation '{name}'")));
        }
      }
      None => {
        d.cam.remove(i);
      }
    }
    Ok(())
  })?;
  s.toolpaths.clear();
  s.toolpaths_of = None;
  s.simulation = None;
  Ok(json!({}))
}
