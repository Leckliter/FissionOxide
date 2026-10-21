//! Manufacture commands: milling and lathe setups, their operations (2.5D, 3D, 3+2, rotary,
//! swarf and turning), the tool library, toolpaths, the stock simulation and G-code.

use fox_kernel::glam::DVec3;
use fox_model::BodyId;
use fox_model::cam::axes::{Kinematics, Machine};
use fox_model::cam::turn::{self, Lathe, LatheOrigin};
use fox_model::cam::{self, Entry, HolderStep, Library, OpKind, Operation, Setup, Tabs, Tool, ToolKind};
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
    "cam.rotary" | "cam.swarf" => operation(s, name, p),
    "cam.turn_face" | "cam.turn_rough" | "cam.turn_finish" | "cam.groove" | "cam.part_off" | "cam.thread" => {
      operation(s, name, p)
    }
    "cam.generate" => generate(s, p),
    "cam.fixtures" => set_fixtures(s, p),
    "cam.post" => post(s, p),
    "cam.list" => Ok(json!(
      s.design
        .cam
        .iter()
        .map(|st| json!({
          "setup": st.id, "name": st.name, "body": st.body.map(|b| b.0),
          "type": if st.lathe.is_some() { "turning" } else { "milling" },
          "machine": st.machine.map(|m| m.kinematics.id()),
          "post": post_id(st), "units": if st.imperial { "inch" } else { "mm" },
          "fixtures": st.fixtures.iter().map(|b| b.0).collect::<Vec<_>>(),
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
          "id": p.id, "name": p.name, "extension": p.extension, "machine": "mill", "rotary": p.rotary,
        }))
        .chain(turn::LATHE_POSTS.iter().map(|p| json!({
          "id": p.id, "name": p.name, "extension": p.extension, "machine": "lathe", "rotary": false,
        })))
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

/// A post-processor id from `post`, checked: a lathe post for a lathe setup.
fn post_of(p: &Value, lathe: bool) -> Result<Option<String>, CmdError> {
  match opt_str(p, "post").filter(|t| !t.trim().is_empty()) {
    Some(id) if lathe => turn::find_post(id).map(|p| Some(p.id.to_string())).ok_or_else(|| {
      let ids: Vec<&str> = turn::LATHE_POSTS.iter().map(|p| p.id).collect();
      CmdError::new(format!("unknown lathe post-processor '{id}' ({})", ids.join(", ")))
    }),
    Some(id) => cam::post::find(id)
      .map(|p| Some(p.id.to_string()))
      .ok_or_else(|| CmdError::new(format!("unknown post-processor '{id}' ({})", cam::post::ids().join(", ")))),
    None => Ok(None),
  }
}

/// The id of the post a setup's G-code is written with.
fn post_id(st: &Setup) -> Option<&'static str> {
  if st.lathe.is_some() { turn::find_post(&st.post).map(|p| p.id) } else { cam::post::find(&st.post).map(|p| p.id) }
}

/// A direction: "x", "-z" or [x, y, z].
fn direction(v: &Value) -> Result<DVec3, CmdError> {
  let d = match v {
    Value::String(t) => match t.trim().to_ascii_lowercase().as_str() {
      "x" | "+x" => DVec3::X,
      "-x" => DVec3::NEG_X,
      "y" | "+y" => DVec3::Y,
      "-y" => DVec3::NEG_Y,
      "z" | "+z" => DVec3::Z,
      "-z" => DVec3::NEG_Z,
      other => return Err(CmdError::new(format!("unknown direction '{other}' (x, -x, y, -y, z, -z or [x, y, z])"))),
    },
    Value::Array(a) if a.len() == 3 => DVec3::new(
      a[0].as_f64().unwrap_or(f64::NAN),
      a[1].as_f64().unwrap_or(f64::NAN),
      a[2].as_f64().unwrap_or(f64::NAN),
    ),
    _ => return Err(CmdError::new("a direction is \"x\", \"-z\" or [x, y, z]")),
  };
  d.try_normalize().filter(|d| d.is_finite()).ok_or_else(|| CmdError::new("the direction can't be zero"))
}

/// A flat face's normal pointing out of its body (as its mesh shows it).
fn outward(s: &Session, f: &fox_model::FaceRef) -> DVec3 {
  let n = DVec3::from(f.normal);
  let Some(b) = s.model.body(f.body) else { return n };
  let (Some(id), Some(m)) = (f.resolve(b.faces(), b.mesh(), b.diagonal()), b.mesh()) else { return n };
  let mut sum = DVec3::ZERO;
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if m.tri_face.get(t) == Some(&id) {
      for &i in tri {
        let v = m.normals[i as usize];
        sum += DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64);
      }
    }
  }
  if sum.dot(n) < 0.0 { -n } else { n }
}

/// A lathe setup's settings: the spindle axis (a direction, or a cylindrical face's axis;
/// default the axis the part is round about), the bar, the chuck and the work origin.
fn lathe_of(s: &Session, p: &Value, probe: &Setup) -> Result<Lathe, CmdError> {
  let (point, axis) = match p.get("axis").filter(|v| v.as_str().is_none_or(|t| !t.trim().is_empty())) {
    Some(v) if v.get("face").is_some() || v.get("point").is_some() => {
      let f = face_ref(s, v)?;
      let b = s.model.body(f.body).ok_or(CmdError::new("no such body"))?;
      let id = f.resolve(b.faces(), b.mesh(), b.diagonal()).ok_or(CmdError::new("the axis face can't be found"))?;
      let info = b.faces().iter().find(|x| x.id == id).ok_or(CmdError::new("the axis face can't be found"))?;
      if info.kind != fox_kernel::SurfaceKind::Cylinder {
        return Err(CmdError::new("the lathe axis comes from a cylindrical face"));
      }
      // pointing out of the part's middle, towards its front
      let (mid, _) = turn::detect_axis(probe, &s.model, Some(info.normal)).map_err(CmdError::new)?;
      let n = if (mid - info.origin).dot(info.normal) > 0.0 { -info.normal } else { info.normal };
      let n = if p.get("flip").is_some() && flag(p, "flip") { -n } else { n };
      (info.origin, n)
    }
    Some(v) => turn::detect_axis(probe, &s.model, Some(direction(v)?)).map_err(CmdError::new)?,
    None => turn::detect_axis(probe, &s.model, None).map_err(CmdError::new)?,
  };
  let origin = match opt_str(p, "origin").filter(|o| !o.trim().is_empty()) {
    Some(o) => LatheOrigin::parse(o)
      .ok_or_else(|| CmdError::new(format!("unknown lathe origin '{o}' (front, stock_front, back)")))?,
    None => LatheOrigin::Front,
  };
  let mut l = Lathe {
    axis_point: point.to_array(),
    axis: axis.to_array(),
    bar_diameter: 0.0,
    stock_front: num(p, "stock_front", 1.0),
    chuck_gap: num(p, "chuck_gap", 5.0),
    grip: num(p, "grip", 15.0),
    chuck_diameter: num(p, "chuck_diameter", 0.0),
    origin,
  };
  // the bar: the part's largest diameter and the side stock, up to a whole millimetre
  let prof = turn::profile(&l, probe, &s.model).map_err(CmdError::new)?;
  let big = prof.r.iter().copied().fold(0.0, f64::max);
  l.bar_diameter = num(p, "bar_diameter", (2.0 * (big + probe.stock_side.max(0.0)) - 1e-3).ceil());
  Ok(l)
}

/// A 4/5-axis machine from `machine` (none for "3axis").
fn machine_of(p: &Value) -> Result<Option<Machine>, CmdError> {
  let Some(k) = opt_str(p, "machine").filter(|k| !k.trim().is_empty()) else { return Ok(None) };
  if matches!(k.trim().to_ascii_lowercase().as_str(), "3axis" | "3_axis" | "none" | "mill") {
    return Ok(None);
  }
  let kin = Kinematics::parse(k).ok_or_else(|| {
    let ids: Vec<&str> = Kinematics::ALL.iter().map(|k| k.id()).collect();
    CmdError::new(format!("unknown machine '{k}' (3axis, {})", ids.join(", ")))
  })?;
  let mut m = Machine::new(kin);
  m.tilt_min = num(p, "tilt_min", m.tilt_min);
  m.tilt_max = num(p, "tilt_max", m.tilt_max);
  m.pivot = num(p, "pivot", 0.0);
  if !(m.tilt_min <= 0.0 && m.tilt_max >= 0.0 && m.pivot >= 0.0) {
    return Err(CmdError::new("the tilt travel must include 0 and the pivot can't be negative"));
  }
  Ok(Some(m))
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
  let lathe = match opt_str(p, "type").map(|t| t.trim().to_ascii_lowercase()) {
    Some(t) if matches!(t.as_str(), "turning" | "lathe" | "turn") => true,
    Some(t) if matches!(t.as_str(), "milling" | "mill" | "") => false,
    Some(t) => return Err(CmdError::new(format!("unknown setup type '{t}' (milling, turning)"))),
    None => false,
  };
  let mut st = Setup {
    id,
    name: opt_str(p, "name").filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Setup{id}")),
    body,
    stock_side: num(p, "stock_side", 2.0),
    stock_top: num(p, "stock_top", 1.0),
    operations: Vec::new(),
    post: post_of(p, lathe)?.unwrap_or_default(),
    imperial: imperial_of(p)?.unwrap_or(false),
    straight: p.get("arcs").is_some() && !flag(p, "arcs"),
    arc_tolerance: num(p, "arc_tolerance", 0.0).max(0.0),
    fixtures: fixtures_of(s, p, body)?,
    lathe: None,
    machine: if lathe { None } else { machine_of(p)? },
    frame_stock: None,
  };
  if lathe {
    st.lathe = Some(lathe_of(s, p, &st)?);
  }
  let (lo, hi) = cam::stock(&st, &s.model).map_err(CmdError::new)?;
  let origin = cam::origin(&st, &s.model).map_err(CmdError::new)?;
  let mut out = json!({"setup": id, "stock": [lo.to_array(), hi.to_array()], "origin": origin.to_array()});
  if let Some(l) = &st.lathe {
    out["type"] = json!("turning");
    out["axis"] = json!(l.axis);
    out["bar_diameter"] = json!(l.bar_diameter);
  }
  if let Some(m) = &st.machine {
    out["machine"] = json!(m.kinematics.id());
  }
  s.edit(|d| {
    d.cam.push(st);
    Ok(())
  })?;
  Ok(out)
}

/// The fixture bodies a command names: `fixtures` (body ids) and the bodies of `components`
/// (component ids), never the part itself.
fn fixtures_of(s: &Session, p: &Value, part: Option<BodyId>) -> Result<Vec<BodyId>, CmdError> {
  let ids = |k: &str| -> Result<Vec<u32>, CmdError> {
    let mut out = Vec::new();
    for v in p.get(k).and_then(Value::as_array).into_iter().flatten() {
      let id = v
        .as_u64()
        .or_else(|| v.get("id").and_then(Value::as_u64))
        .ok_or_else(|| CmdError::new(format!("'{k}' takes ids")))?;
      out.push(id as u32);
    }
    Ok(out)
  };
  let mut out: Vec<BodyId> = Vec::new();
  for b in ids("fixtures")? {
    if s.model.body(BodyId(b)).is_none() {
      return Err(CmdError::new(format!("no body {b}")));
    }
    out.push(BodyId(b));
  }
  for c in ids("components")? {
    let comp =
      s.design.components.iter().find(|x| x.id.0 == c).ok_or_else(|| CmdError::new(format!("no component {c}")))?;
    out.extend(comp.bodies.iter().copied().filter(|b| s.model.body(*b).is_some()));
  }
  out.sort();
  out.dedup();
  if part.is_some_and(|b| out.contains(&b)) {
    return Err(CmdError::new("the part can't be a fixture of its own setup"));
  }
  if part.is_none() && !out.is_empty() && s.model.bodies.iter().all(|b| out.contains(&b.id)) {
    return Err(CmdError::new("every body is a fixture: there is nothing left to machine"));
  }
  Ok(out)
}

/// Sets the fixtures of a setup (the bodies holding the part, checked for collisions).
fn set_fixtures(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let fixtures = fixtures_of(s, p, s.design.cam[i].body)?;
  let mut probe = s.design.cam[i].clone();
  probe.fixtures = fixtures.clone();
  let paths = cam::toolpaths(&probe, &s.model).map_err(CmdError::new)?;
  let warnings = cam::collide::check(&probe, &s.model, &paths).map_err(CmdError::new)?;
  let id = probe.id;
  s.edit(|d| {
    d.cam[i].fixtures = fixtures.clone();
    Ok(())
  })?;
  s.toolpaths.clear();
  s.toolpaths_of = None;
  s.simulation = None;
  Ok(
    json!({"setup": id, "fixtures": fixtures.iter().map(|b| b.0).collect::<Vec<_>>(), "warnings": warnings_json(&warnings)}),
  )
}

fn warnings_json(w: &[cam::sim::Warning]) -> Vec<Value> {
  w.iter()
    .map(|w| {
      json!({
        "operation": w.operation, "kind": w.kind.id(), "message": w.message(), "at": w.at.to_array(),
        "depth": w.depth, "moves": w.moves,
      })
    })
    .collect()
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
    "holder": t.holder.iter().map(|h| json!({"diameter": h.diameter, "length": h.length})).collect::<Vec<_>>(),
    "holder_text": HolderStep::format_list(&t.holder),
    "width": t.width, "surface_speed": t.surface_speed, "feed_per_rev": t.feed_per_rev,
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
    holder: match t.get("holder") {
      Some(h) => holder_of(h)?,
      None => base.holder.clone(),
    },
    width: num(t, "width", base.width),
    surface_speed: num(t, "surface_speed", base.surface_speed),
    feed_per_rev: num(t, "feed_per_rev", base.feed_per_rev),
    ..base
  };
  tool.check().map_err(CmdError::new)?;
  Ok(tool)
}

/// Holder steps: `[{diameter, length}]`, `[[diameter, length]]` or "Ø×length, …".
fn holder_of(h: &Value) -> Result<Vec<HolderStep>, CmdError> {
  match h {
    Value::Null => Ok(Vec::new()),
    Value::String(t) => HolderStep::parse_list(t).map_err(CmdError::new),
    Value::Array(items) => items
      .iter()
      .map(|v| match v {
        Value::Array(pair) => Ok(HolderStep {
          diameter: pair.first().and_then(Value::as_f64).unwrap_or(0.0),
          length: pair.get(1).and_then(Value::as_f64).unwrap_or(0.0),
        }),
        Value::Object(_) => Ok(HolderStep { diameter: num(v, "diameter", 0.0), length: num(v, "length", 0.0) }),
        Value::Number(d) => Ok(HolderStep { diameter: d.as_f64().unwrap_or(0.0), length: 0.0 }),
        _ => Err(CmdError::new("a holder step is {diameter, length}")),
      })
      .collect(),
    _ => Err(CmdError::new("'holder' is a list of steps ({diameter, length}) or \"Ø×length, …\"")),
  }
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
///
/// A lathe operation given no tool takes the library's first tool of a kind it can use (`lathe`;
/// with `finish`, the turning insert with the smallest nose).
fn op_tool(s: &Session, p: &Value, lathe: Option<&[ToolKind]>, finish: bool) -> Result<Tool, CmdError> {
  if let Some(kinds) = lathe
    && p.get("tool").is_none()
    && p.get("tool_number").is_none()
    && p.get("tool_diameter").is_none()
  {
    let lib = library(s)?;
    let found = kinds
      .iter()
      .find_map(|k| {
        let mut of: Vec<&Tool> = lib.tools.iter().filter(|t| t.kind == *k).collect();
        if finish {
          of.sort_by(|a, b| a.corner_radius.total_cmp(&b.corner_radius));
        }
        of.first().copied()
      })
      .ok_or_else(|| {
        CmdError::new(format!(
          "the tool library has no {} tool: give 'tool' or 'tool_number'",
          kinds[0].id().replace('_', "-")
        ))
      })?;
    let mut q = p.clone();
    q["tool_number"] = json!(found.number);
    return op_tool(s, &q, None, false);
  }
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
  let lathe_kinds: Option<&[ToolKind]> = match name {
    "cam.turn_face" | "cam.turn_rough" | "cam.turn_finish" => Some(&[ToolKind::Turning]),
    "cam.groove" => Some(&[ToolKind::Groove]),
    "cam.part_off" => Some(&[ToolKind::PartOff, ToolKind::Groove]),
    "cam.thread" => Some(&[ToolKind::Thread]),
    _ => None,
  };
  let on_lathe = s.design.cam[i].lathe.is_some();
  if lathe_kinds.is_some() != on_lathe {
    return Err(CmdError::new(if on_lathe {
      "a turning setup takes lathe operations (cam.turn_face, cam.turn_rough, cam.turn_finish, cam.groove, cam.part_off, cam.thread)"
    } else {
      "lathe operations need a turning setup (cam.setup with type: turning)"
    }));
  }
  let tool = op_tool(s, p, lathe_kinds, name == "cam.turn_finish")?;
  let tool_axis = match (p.get("tool_axis"), p.get("work_plane")) {
    (Some(v), _) => Some(direction(v)?.to_array()),
    (None, Some(f)) => {
      let f = face_ref(s, f)?;
      if !f.planar {
        return Err(CmdError::new("the work plane comes from a flat face"));
      }
      Some(outward(s, &f).to_array())
    }
    _ => None,
  }
  .filter(|t| DVec3::from(*t).dot(DVec3::Z) < 1.0 - 1e-12);
  if tool_axis.is_some() && (on_lathe || s.design.cam[i].machine.is_none()) {
    return Err(CmdError::new("a tilted tool axis needs a 4/5-axis machine on the setup (cam.setup machine)"));
  }
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
    "cam.adaptive3d" => OpKind::Adaptive3d { rest: flag(p, "rest") },
    "cam.pencil" => {
      let passes = num(p, "passes", 0.0);
      if !(passes >= 0.0 && passes.fract() == 0.0) {
        return Err(CmdError::new("the pencil passes are a whole number either side"));
      }
      OpKind::Pencil { faces: faces_of(s, p, "faces")?, passes: passes as u32 }
    }
    "cam.steep_shallow" => {
      OpKind::SteepShallow { faces: faces_of(s, p, "faces")?, threshold: num(p, "threshold", 45.0) }
    }
    "cam.contour3d" => OpKind::Contour3d { faces: faces_of(s, p, "faces")?, min_slope: num(p, "min_slope", 0.0) },
    "cam.rotary" => OpKind::Rotary { around: flag(p, "around") },
    "cam.swarf" => {
      let faces = faces_of(s, p, "faces")?;
      if faces.is_empty() {
        return Err(CmdError::new("missing 'faces' (the walls to swarf)"));
      }
      OpKind::Swarf { faces }
    }
    "cam.turn_face" => OpKind::TurnFace,
    "cam.turn_rough" => OpKind::TurnRough,
    "cam.turn_finish" => OpKind::TurnFinish,
    "cam.groove" => OpKind::Groove,
    "cam.part_off" => OpKind::PartOff,
    "cam.thread" => {
      let mut faces = faces_of(s, p, "faces")?;
      if let Some(f) = p.get("face") {
        faces.push(face_ref(s, f)?);
      }
      if p.get("pitch").is_none() {
        return Err(CmdError::new("missing 'pitch'"));
      }
      OpKind::Thread {
        pitch: num(p, "pitch", 0.0),
        faces,
        z_start: num(p, "z_start", 0.0),
        z_end: num(p, "z_end", 0.0),
        depth: num(p, "depth", 0.0),
      }
    }
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
    "cam.rotary" => "Rotary",
    "cam.swarf" => "Swarf",
    "cam.turn_face" => "Turn Face",
    "cam.turn_rough" => "Profile Roughing",
    "cam.turn_finish" => "Profile Finishing",
    "cam.groove" => "Groove",
    "cam.part_off" => "Part Off",
    "cam.thread" => "Thread",
    _ => "Drill",
  };
  // adaptive clearing takes light, deep cuts and spirals in by default
  let (stepover, stepdown, entry) = match name {
    "cam.adaptive" => (0.1, 10.0, Entry::Helix),
    "cam.parallel" | "cam.scallop" | "cam.pencil" => (0.1, 2.0, Entry::Plunge),
    "cam.adaptive3d" => (0.15, 3.0, Entry::Helix),
    "cam.steep_shallow" | "cam.contour3d" => (0.1, 0.5, Entry::Plunge),
    "cam.rotary" => (0.15, 3.0, Entry::Plunge),
    "cam.swarf" => (0.4, 5.0, Entry::Plunge),
    // lathe step-downs: facing depth, radial depth of cut, peck depth, the first thread pass
    "cam.turn_face" | "cam.turn_finish" => (0.4, 1.0, Entry::Plunge),
    "cam.turn_rough" => (0.4, 1.5, Entry::Plunge),
    "cam.groove" => (0.8, 2.0, Entry::Plunge),
    "cam.part_off" => (0.4, 3.0, Entry::Plunge),
    "cam.thread" => (0.4, 0.15, Entry::Plunge),
    _ => (0.4, 2.0, Entry::Plunge),
  };
  // roughing leaves some for finishing
  let stock_to_leave = match name {
    "cam.adaptive3d" => 0.3,
    "cam.turn_rough" => 0.2,
    _ => 0.0,
  };
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
    tool_axis,
  };
  // check it computes, after the setup's other operations, before keeping it
  let mut probe = s.design.cam[i].clone();
  probe.operations.push(op.clone());
  let paths = cam::toolpaths(&probe, &s.model).map_err(CmdError::new)?;
  let moves = paths.last().map(|t| t.moves.len()).unwrap_or(0);
  // collisions of this operation's tool and holder with the part and the fixtures
  let warnings = match paths.last() {
    Some(last) => {
      let mut w = cam::collide::check(&probe, &s.model, &paths).map_err(CmdError::new)?;
      w.retain(|w| w.operation == last.name);
      w
    }
    None => Vec::new(),
  };
  let opname = op.name.clone();
  let number = op.tool.number;
  s.edit(|d| {
    d.cam[i].operations.push(op);
    Ok(())
  })?;
  Ok(json!({"operation": opname, "moves": moves, "tool": number, "warnings": warnings_json(&warnings)}))
}

/// Computes the toolpaths of a setup (shown in the viewport) and reports their length and
/// time, and collisions of the holders with the part and of tools and holders with fixtures.
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
  let warnings = cam::collide::check(st, &s.model, &paths).map_err(CmdError::new)?;
  let id = st.id;
  s.toolpaths = paths;
  s.toolpaths_of = Some((id, s.revision));
  Ok(json!({"setup": id, "operations": report, "minutes": total, "warnings": warnings_json(&warnings)}))
}

/// Writes a setup's G-code with its post-processor (or `post`/`units`); returns it when no
/// path is given.
fn post(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let mut st = s.design.cam[i].clone();
  if let Some(id) = post_of(p, st.lathe.is_some())? {
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
  let code = cam::gcode(&st, &paths, origin).map_err(CmdError::new)?;
  let used = post_id(&st).unwrap_or("fanuc");
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
/// into the stock, cuts deeper than the flutes, holders into the material, tools or holders
/// into the fixtures). The result is drawn in the viewport.
fn simulate(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let st = &s.design.cam[i];
  let paths = cam::toolpaths(st, &s.model).map_err(CmdError::new)?;
  if paths.is_empty() {
    return Err(CmdError::new("the setup has no operations"));
  }
  let sim = cam::simulate(st, &s.model, &paths).map_err(CmdError::new)?;
  let warnings = warnings_json(sim.warnings());
  let mut out = json!({
    "setup": st.id,
    "cell": sim.cell(),
    "removed": sim.removed().iter().map(|(n, v)| json!({"operation": n, "mm3": v})).collect::<Vec<_>>(),
    "remaining_mm3": sim.remaining(),
    "warnings": warnings,
    "kind": match &sim {
      cam::Simulated::Mill(_) => "heightfield",
      cam::Simulated::Voxels(_) => "voxels",
      cam::Simulated::Turned(_) => "turned",
    },
  });
  if let cam::Simulated::Voxels(v) = &sim
    && let (Some(r), Some(m)) = (v.rotary, st.machine)
  {
    let (tilt, turn) = m.kinematics.letters();
    out["rotary"] = json!({tilt.to_string(): [r.0, r.1]});
    if let Some(c) = turn {
      out["rotary"][c.to_string()] = json!([r.2, r.3]);
    }
  }
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
