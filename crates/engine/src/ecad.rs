//! The Electronics workspace's commands (`ecad.*`): the schematic, the component libraries
//! (with KiCad import), the board layout and its rules, fab output, and the link to 3D: a PCB
//! feature whose board body and part models follow the layout.

use std::path::{Path, PathBuf};

use fox_ecad::board::{Hole, Layer, Pour, Side, Track, Via};
use fox_ecad::geom::{P, add, rot};
use fox_ecad::schematic::{Bus, Label, Wire};
use fox_ecad::{Library, Model3d, Project, Violation, gerber, kicad, router, svg};
use fox_model::pcb::{PcbModel, PcbOutline, PcbPart, face_frame};
use fox_model::{Design, FeatureId, FeatureKind};
use fox_sketch::find_profiles;
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str, req_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  if !name.starts_with("ecad.") {
    return None;
  }
  Some(match name {
    "ecad.new" => new(s, p),
    "ecad.info" => info(s),
    "ecad.library" => library(s, p),
    "ecad.library.import" => library_import(s, p),
    "ecad.library.export" => library_export(s, p),
    "ecad.part.add" => part_add(s, p),
    "ecad.part.edit" => part_edit(s, p),
    "ecad.part.delete" => {
      let r = req_str(p, "refdes").map(str::to_string);
      r.and_then(|r| modify(s, |pr| if pr.remove_part(&r) { Ok(json!({"deleted": r})) } else { Err(format!("no part {r}")) }))
    }
    "ecad.wire" => wire(s, p),
    "ecad.label" => label(s, p),
    "ecad.connect" => connect(s, p),
    "ecad.bus" => bus(s, p),
    "ecad.no_connect" => no_connect(s, p),
    "ecad.schematic.delete" => schematic_delete(s, p),
    "ecad.netlist" => project(s).map(|pr| json!(pr.netlist().nets)),
    "ecad.erc" => project(s).map(|pr| report(&pr.erc())),
    "ecad.board.outline" => outline(s, p),
    "ecad.board.rules" => rules(s, p),
    "ecad.place" => place(s, p),
    "ecad.route" => route(s, p),
    "ecad.via" => via(s, p),
    "ecad.pour" => pour(s, p),
    "ecad.hole" => hole(s, p),
    "ecad.board.delete" => board_delete(s, p),
    "ecad.unroute" => unroute(s, p),
    "ecad.autoroute" => autoroute(s, p),
    "ecad.ratsnest" => project(s).map(|pr| json!(pr.board.ratsnest(&pr.pads()))),
    "ecad.drc" => project(s).map(|pr| report(&pr.drc())),
    "ecad.export" => export(s, p),
    "ecad.svg" => svg_cmd(s, p),
    "ecad.push3d" => push3d(s),
    _ => return None,
  })
}

/// The design's electronics project.
pub(crate) fn project(s: &Session) -> Result<&Project, CmdError> {
  s.design.electronics.as_deref().ok_or(CmdError::new("the design has no electronics yet: add a part or run ecad.new"))
}

fn report(v: &[Violation]) -> Value {
  let errors = v.iter().filter(|x| x.severity == fox_ecad::Severity::Error).count();
  json!({"errors": errors, "warnings": v.len() - errors, "violations": v})
}

/// Changes the project in one undo step (making it if the design has none), refreshing the
/// outline from its sketch or face and the PCB feature from the layout.
fn modify<T>(s: &mut Session, f: impl FnOnce(&mut Project) -> Result<T, String>) -> Result<T, CmdError> {
  modify_src(s, None, f)
}

/// [`modify`], also setting where the outline comes from (`Some(None)`: drawn by hand).
fn modify_src<T>(s: &mut Session, new_source: Option<Option<Value>>, f: impl FnOnce(&mut Project) -> Result<T, String>) -> Result<T, CmdError> {
  let source = match &new_source {
    Some(src) => src.clone(),
    None => s.design.electronics.as_ref().and_then(|pr| pr.outline_from.clone()),
  };
  let fresh = match &source {
    Some(src) => Some(outline_source(s, src)?),
    None => None,
  };
  let out = s.edit(|d| {
    let name = d.name.clone();
    let pr = d.electronics.get_or_insert_with(|| Box::new(Project::new(&name)));
    if let Some(src) = new_source {
      pr.outline_from = src;
    }
    if let Some((poly, ..)) = &fresh {
      pr.board.outline = poly.clone();
    }
    let v = f(pr).map_err(CmdError::new)?;
    sync_feature(d, fresh.as_ref().map(|f| (f.1.clone(), f.2.clone())))?;
    Ok(v)
  })?;
  tidy_component(s);
  Ok(out)
}

fn num(p: &Value, k: &str) -> Option<f64> {
  p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite())
}

fn pt(v: &Value) -> Option<P> {
  let a = v.as_array()?;
  match a.as_slice() {
    [x, y] => Some([x.as_f64()?, y.as_f64()?]),
    _ => None,
  }
}

fn req_pt(p: &Value, k: &str) -> Result<P, CmdError> {
  p.get(k).and_then(pt).ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y]")))
}

fn pts(p: &Value, k: &str) -> Result<Vec<P>, CmdError> {
  let a = p.get(k).and_then(Value::as_array).ok_or_else(|| CmdError::new(format!("'{k}' must be a list of [x, y]")))?;
  a.iter().map(|v| pt(v).ok_or_else(|| CmdError::new(format!("'{k}' must be a list of [x, y]")))).collect()
}

/// A schematic point: [x, y], or {part, pin} for a pin's connection point.
fn sch_point(pr: &Project, v: &Value) -> Result<P, String> {
  if let Some(p) = pt(v) {
    return Ok(p);
  }
  let part = v.get("part").and_then(Value::as_str).ok_or("a point is [x, y] or {part, pin}")?;
  let pin = match v.get("pin") {
    Some(Value::String(s)) => s.clone(),
    Some(Value::Number(n)) => n.to_string(),
    _ => return Err("a point is [x, y] or {part, pin}".into()),
  };
  pin_point(pr, part, &pin).map(|(p, _)| p)
}

/// A pin's connection point and the direction pointing away from its symbol (degrees).
fn pin_point(pr: &Project, part: &str, pin: &str) -> Result<(P, f64), String> {
  let libs = pr.libs();
  let sp = pr.schematic.parts.iter().find(|p| p.refdes == part).ok_or(format!("no part {part}"))?;
  let sym = libs.device(&sp.device).and_then(|d| libs.symbol(&d.symbol)).ok_or(format!("{part}: unknown device"))?;
  let pn = sym
    .pins
    .iter()
    .find(|p| p.number == pin)
    .or_else(|| sym.pins.iter().find(|p| p.name.eq_ignore_ascii_case(pin)))
    .ok_or(format!("{part} has no pin {pin}"))?;
  let xf = sp.xf();
  Ok((xf.apply(pn.at), xf.angle(pn.dir) + 180.0))
}

fn new(s: &mut Session, p: &Value) -> CmdResult {
  let example = p.get("example").and_then(Value::as_bool).unwrap_or(false);
  let mut pr = if example { fox_ecad::example::usb_mcu_board().map_err(CmdError::new)? } else { Project::new(&s.design.name) };
  if let Some(n) = opt_str(p, "name") {
    pr.name = n.to_string();
  }
  s.edit(|d| {
    d.electronics = Some(Box::new(pr));
    sync_feature(d, None)
  })?;
  tidy_component(s);
  info(s)
}

fn info(s: &Session) -> CmdResult {
  let pr = project(s)?;
  let pads = pr.pads();
  let bounds = fox_ecad::geom::bounds(pr.board.outline.iter().copied());
  let erc = pr.erc();
  let drc = pr.drc();
  let count = |v: &[Violation], sev| v.iter().filter(|x| x.severity == sev).count();
  Ok(json!({
    "name": pr.name,
    "parts": pr.schematic.parts.iter().filter(|p| !p.refdes.starts_with('#')).count(),
    "nets": pr.netlist().nets.len(),
    "wires": pr.schematic.wires.len(),
    "labels": pr.schematic.labels.len(),
    "board": {
      "size": bounds.map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1]]),
      "outline_from": pr.outline_from,
      "layers": pr.board.layers,
      "thickness": pr.board.thickness,
      "placed": pr.board.placements.len(),
      "pads": pads.len(),
      "tracks": pr.board.tracks.len(),
      "vias": pr.board.vias.len(),
      "pours": pr.board.pours.len(),
      "unrouted": pr.board.ratsnest(&pads).len(),
    },
    "erc": {"errors": count(&erc, fox_ecad::Severity::Error), "warnings": count(&erc, fox_ecad::Severity::Warning)},
    "drc": {"errors": count(&drc, fox_ecad::Severity::Error), "warnings": count(&drc, fox_ecad::Severity::Warning)},
    "pcb_feature": pcb_feature(&s.design).map(|f| f.0),
    "libraries": pr.libraries.iter().map(|l| &l.name).collect::<Vec<_>>(),
  }))
}

fn library(s: &Session, p: &Value) -> CmdResult {
  let q = opt_str(p, "query").unwrap_or("").to_ascii_lowercase();
  let only = opt_str(p, "library");
  let empty = Project::new("");
  let pr = s.design.electronics.as_deref().unwrap_or(&empty);
  let libs = pr.libs();
  let mut out = Vec::new();
  for lib in libs.0.iter().filter(|l| only.is_none_or(|o| l.name.eq_ignore_ascii_case(o))) {
    for d in &lib.devices {
      let hay = format!("{} {} {} {}", d.name, d.description, d.footprint, d.value).to_ascii_lowercase();
      if q.split_whitespace().all(|w| hay.contains(w)) {
        out.push(json!({
          "library": lib.name, "device": d.name, "description": d.description, "prefix": d.prefix, "value": d.value,
          "symbol": d.symbol, "footprint": d.footprint,
          "pins": libs.symbol(&d.symbol).map(|s| s.pins.len()),
          "pads": libs.footprint(&d.footprint).map(|f| f.pads.len()),
        }));
      }
    }
    if p.get("footprints").and_then(Value::as_bool).unwrap_or(false) {
      for f in &lib.footprints {
        if q.is_empty() || f.name.to_ascii_lowercase().contains(&q) {
          out.push(json!({"library": lib.name, "footprint": f.name, "pads": f.pads.len()}));
        }
      }
    }
  }
  Ok(json!(out))
}

/// Imports a library: KiCad `.kicad_sym`, `.kicad_mod`, a `.pretty` folder, or this app's own
/// JSON library format. Footprints are matched to imported symbols by name.
fn library_import(s: &mut Session, p: &Value) -> CmdResult {
  let path = PathBuf::from(req_str(p, "path")?);
  let stem = path.file_stem().and_then(|x| x.to_str()).unwrap_or("library").trim_end_matches(".pretty").to_string();
  let name = opt_str(p, "name").map(str::to_string).unwrap_or(stem);
  let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
  let mut lib = if path.is_dir() {
    kicad::import_footprint_dir(&path, &name).map_err(CmdError::new)?
  } else {
    let text = std::fs::read_to_string(&path).map_err(|e| CmdError::new(format!("{}: {e}", path.display())))?;
    match ext.as_str() {
      "kicad_sym" => kicad::import_symbols(&text, &name).map_err(CmdError::new)?,
      "kicad_mod" => Library { name: name.clone(), footprints: vec![kicad::import_footprint(&text).map_err(CmdError::new)?], ..Default::default() },
      "json" => serde_json::from_str::<Library>(&text).map_err(|e| CmdError::new(format!("{}: {e}", path.display())))?,
      _ => return Err(CmdError::new("import .kicad_sym, .kicad_mod, a .pretty folder or a .json library")),
    }
  };
  lib.name = name.clone();
  let (ns, nf, nd) = (lib.symbols.len(), lib.footprints.len(), lib.devices.len());
  modify(s, |pr| {
    match pr.libraries.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&name)) {
      Some(l) => l.merge(lib),
      None => pr.libraries.push(lib),
    }
    Ok(json!({"library": name, "symbols": ns, "footprints": nf, "devices": nd}))
  })
}

fn library_export(s: &Session, p: &Value) -> CmdResult {
  let path = PathBuf::from(req_str(p, "path")?);
  let name = opt_str(p, "library").unwrap_or(fox_ecad::builtin::NAME);
  let empty = Project::new("");
  let pr = s.design.electronics.as_deref().unwrap_or(&empty);
  let libs = pr.libs();
  let lib = libs.0.iter().find(|l| l.name.eq_ignore_ascii_case(name)).ok_or(CmdError::new(format!("no library '{name}'")))?;
  std::fs::write(&path, serde_json::to_string_pretty(lib)?)?;
  Ok(json!({"path": path, "devices": lib.devices.len(), "symbols": lib.symbols.len(), "footprints": lib.footprints.len()}))
}

fn part_add(s: &mut Session, p: &Value) -> CmdResult {
  let device = req_str(p, "device")?.to_string();
  let at = req_pt(p, "at")?;
  let refdes = opt_str(p, "refdes").map(str::to_string);
  let value = opt_str(p, "value").map(str::to_string);
  let r = num(p, "rot").unwrap_or(0.0);
  let mirror = p.get("mirror").and_then(Value::as_bool).unwrap_or(false);
  modify(s, |pr| {
    let refdes = pr.add_part(&device, at, refdes.as_deref(), value.as_deref())?;
    if let Some(part) = pr.schematic.parts.iter_mut().find(|x| x.refdes == refdes) {
      part.rot = r;
      part.mirror = mirror;
    }
    Ok(json!({"refdes": refdes}))
  })
}

fn part_edit(s: &mut Session, p: &Value) -> CmdResult {
  let refdes = req_str(p, "refdes")?.to_string();
  let p = p.clone();
  modify(s, move |pr| {
    if let Some(d) = p.get("device").and_then(Value::as_str)
      && pr.device(d).is_none()
    {
      return Err(format!("no device '{d}' in the libraries"));
    }
    let new_ref = p.get("new_refdes").and_then(Value::as_str);
    if let Some(n) = new_ref
      && n != refdes
      && pr.schematic.parts.iter().any(|x| x.refdes == n)
    {
      return Err(format!("{n} is already used"));
    }
    let part = pr.schematic.parts.iter_mut().find(|x| x.refdes == refdes).ok_or(format!("no part {refdes}"))?;
    if let Some(at) = p.get("at").and_then(pt) {
      part.at = at;
    }
    if let Some(r) = num(&p, "rot") {
      part.rot = r.rem_euclid(360.0);
    }
    if let Some(m) = p.get("mirror").and_then(Value::as_bool) {
      part.mirror = m;
    }
    if let Some(v) = p.get("value").and_then(Value::as_str) {
      part.value = v.to_string();
    }
    if let Some(d) = p.get("device").and_then(Value::as_str) {
      part.device = d.to_string();
    }
    if let Some(n) = new_ref {
      part.refdes = n.to_string();
      if let Some(pl) = pr.board.placements.iter_mut().find(|x| x.refdes == refdes) {
        pl.refdes = n.to_string();
      }
    }
    pr.sync_board();
    Ok(json!({"refdes": new_ref.unwrap_or(&refdes)}))
  })
}

fn wire(s: &mut Session, p: &Value) -> CmdResult {
  let list = p.get("points").and_then(Value::as_array).cloned().ok_or(CmdError::new("'points' must list [x, y] or {part, pin}"))?;
  if list.len() < 2 {
    return Err(CmdError::new("a wire needs two points"));
  }
  modify(s, |pr| {
    let mut ps: Vec<P> = list.iter().map(|v| sch_point(pr, v)).collect::<Result<_, _>>()?;
    // two pins straight to each other: an L through the corner
    if ps.len() == 2 && (ps[0][0] - ps[1][0]).abs() > 1e-6 && (ps[0][1] - ps[1][1]).abs() > 1e-6 && p.get("direct") != Some(&Value::Bool(true)) {
      ps.insert(1, [ps[1][0], ps[0][1]]);
    }
    let n = ps.len() - 1;
    for w in ps.windows(2) {
      pr.schematic.wires.push(Wire { a: w[0], b: w[1] });
    }
    Ok(json!({"wires": n}))
  })
}

fn label(s: &mut Session, p: &Value) -> CmdResult {
  let name = req_str(p, "name")?.to_string();
  let at = p.get("at").cloned().ok_or(CmdError::new("'at' must be [x, y] or {part, pin}"))?;
  modify(s, |pr| {
    let at = sch_point(pr, &at)?;
    pr.schematic.labels.push(Label { name: name.clone(), at });
    Ok(json!({"label": name, "at": at}))
  })
}

/// A pin onto a net the EAGLE way: a short wire with a label, or a power symbol for the power
/// nets, or a no-connect marker for "nc".
fn connect(s: &mut Session, p: &Value) -> CmdResult {
  let part = req_str(p, "part")?.to_string();
  let pin = match p.get("pin") {
    Some(Value::String(x)) => x.clone(),
    Some(Value::Number(n)) => n.to_string(),
    _ => return Err(CmdError::new("missing 'pin'")),
  };
  let net = req_str(p, "net")?.to_string();
  let len = num(p, "length").unwrap_or(2.54);
  modify(s, |pr| {
    let (at, out) = pin_point(pr, &part, &pin)?;
    if net.eq_ignore_ascii_case("nc") {
      pr.schematic.no_connects.push(at);
      return Ok(json!({"no_connect": at}));
    }
    let end = add(at, rot([len, 0.0], out));
    pr.schematic.wires.push(Wire { a: at, b: end });
    let power = pr.libs().device(&net).is_some_and(|d| pr.libs().symbol(&d.symbol).is_some_and(|s| s.power));
    if power {
      let r = pr.next_refdes("#PWR");
      pr.add_part(&net, end, Some(&r), Some(&net))?;
      let ground = net.to_ascii_uppercase().contains("GND");
      if let Some(sym) = pr.schematic.parts.last_mut() {
        sym.rot = (out + if ground { 90.0 } else { -90.0 }).rem_euclid(360.0);
      }
    } else {
      pr.schematic.labels.push(Label { name: net.clone(), at: end });
    }
    Ok(json!({"net": net, "at": end}))
  })
}

fn bus(s: &mut Session, p: &Value) -> CmdResult {
  let name = req_str(p, "name")?.to_string();
  let ps = pts(p, "points")?;
  if ps.len() < 2 {
    return Err(CmdError::new("a bus needs two points"));
  }
  modify(s, |pr| {
    let members = fox_ecad::schematic::bus_members(&name);
    pr.schematic.buses.push(Bus { name, pts: ps });
    Ok(json!({"members": members}))
  })
}

fn no_connect(s: &mut Session, p: &Value) -> CmdResult {
  let v = p.get("at").cloned().unwrap_or_else(|| p.clone());
  modify(s, |pr| {
    let at = sch_point(pr, &v)?;
    pr.schematic.no_connects.push(at);
    Ok(json!({"at": at}))
  })
}

/// Deletes schematic wires, labels, buses and no-connect markers at a point.
fn schematic_delete(s: &mut Session, p: &Value) -> CmdResult {
  let at = req_pt(p, "at")?;
  let tol = num(p, "tolerance").unwrap_or(0.5);
  modify(s, |pr| {
    let sch = &mut pr.schematic;
    let n0 = sch.wires.len() + sch.labels.len() + sch.buses.len() + sch.no_connects.len();
    sch.wires.retain(|w| fox_ecad::geom::point_seg_dist(at, w.a, w.b) > tol);
    sch.labels.retain(|l| fox_ecad::geom::dist(l.at, at) > tol);
    sch.buses.retain(|b| !b.pts.windows(2).any(|w| fox_ecad::geom::point_seg_dist(at, w[0], w[1]) <= tol));
    sch.no_connects.retain(|q| fox_ecad::geom::dist(*q, at) > tol);
    let n1 = sch.wires.len() + sch.labels.len() + sch.buses.len() + sch.no_connects.len();
    Ok(json!({"deleted": n0 - n1}))
  })
}

/// The polygon of a board outline source, with the 3D feature's outline and plane.
fn outline_source(s: &Session, src: &Value) -> Result<(Vec<P>, PcbOutline, Option<fox_model::PlaneRef>), CmdError> {
  if let Some(id) = src.get("sketch").and_then(Value::as_u64) {
    let id = FeatureId(u32::try_from(id).map_err(|_| CmdError::new("bad sketch id"))?);
    let sk = s.design.sketch(id)?;
    let profiles = find_profiles(sk);
    let prof = match src.get("point").and_then(pt) {
      Some(q) => profiles.iter().filter(|pr| pr.contains(q)).min_by(|a, b| a.area().abs().total_cmp(&b.area().abs())),
      None => profiles.iter().max_by(|a, b| a.area().abs().total_cmp(&b.area().abs())),
    }
    .ok_or(CmdError::new("the sketch has no closed region there for the board"))?;
    let poly = prof.outer.polygon();
    let point = src.get("point").and_then(pt).unwrap_or_else(|| interior_point(&poly));
    return Ok((poly, PcbOutline::Profile { sketch: id, point }, None));
  }
  if let Some(f) = src.get("face") {
    let fr = face_ref(s, f)?;
    let body = s.model.body(fr.body).ok_or(CmdError::new(format!("no body {}", fr.body.0)))?;
    let fid = fr.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?;
    let info = body.faces().iter().find(|x| x.id == fid).ok_or(CmdError::new("no such face"))?;
    if info.plane().is_none() {
      return Err(CmdError::new("the board outline face must be flat"));
    }
    let frame = face_frame(info.origin, info.normal).ok_or(CmdError::new("the face has no normal"))?;
    let loops = body.shape.face_shape(fid)?.face_loops(0.05)?;
    let poly = loops
      .iter()
      .map(|l| l.iter().map(|q| frame.to_local(*q)).collect::<Vec<P>>())
      .max_by(|a, b| fox_ecad::geom::area(a).abs().total_cmp(&fox_ecad::geom::area(b).abs()))
      .ok_or(CmdError::new("the face has no boundary"))?;
    return Ok((poly, PcbOutline::Face { face: fr }, None));
  }
  Err(CmdError::new("an outline source is {sketch, point?} or {face}"))
}

/// A point inside a polygon.
fn interior_point(poly: &[P]) -> P {
  let c = fox_ecad::geom::Shape2::Poly(poly.to_vec()).anchor();
  if fox_ecad::geom::point_in_poly(c, poly) {
    return c;
  }
  // the middle of the first span a horizontal line through the centre crosses
  let mut xs: Vec<f64> = fox_ecad::geom::edges(poly)
    .filter(|(a, b)| (a[1] > c[1]) != (b[1] > c[1]))
    .map(|(a, b)| a[0] + (c[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
    .collect();
  xs.sort_by(f64::total_cmp);
  if xs.len() >= 2 { [(xs[0] + xs[1]) / 2.0, c[1]] } else { c }
}

fn outline(s: &mut Session, p: &Value) -> CmdResult {
  let layers = p.get("layers").and_then(Value::as_u64);
  if let Some(l) = layers
    && l != 2
    && l != 4
  {
    return Err(CmdError::new("a board has 2 or 4 copper layers"));
  }
  let thickness = num(p, "thickness");
  if thickness.is_some_and(|t| t <= 0.0) {
    return Err(CmdError::new("the thickness must be positive"));
  }
  // where the outline comes from: points, a rectangle, a sketch region or a face
  let src = if p.get("sketch").is_some() || p.get("face").is_some() {
    let mut src = json!({});
    for k in ["sketch", "point", "face"] {
      if let Some(v) = p.get(k) {
        src[k] = v.clone();
      }
    }
    outline_source(s, &src)?;
    Some(Some(src))
  } else {
    None
  };
  let poly = if let Some(r) = p.get("rect").and_then(pt) {
    let o = p.get("origin").and_then(pt).unwrap_or([0.0, 0.0]);
    Some(vec![o, [o[0] + r[0], o[1]], [o[0] + r[0], o[1] + r[1]], [o[0], o[1] + r[1]]])
  } else if p.get("points").is_some() {
    let ps = pts(p, "points")?;
    if ps.len() < 3 {
      return Err(CmdError::new("an outline needs at least three points"));
    }
    Some(ps)
  } else {
    None
  };
  modify_outline(s, src.or(if poly.is_some() { Some(None) } else { None }), poly, layers, thickness)
}

fn modify_outline(s: &mut Session, src: Option<Option<Value>>, poly: Option<Vec<P>>, layers: Option<u64>, thickness: Option<f64>) -> CmdResult {
  modify_src(s, src, |pr| {
    if let Some(poly) = poly {
      pr.board.outline = poly;
    }
    if let Some(l) = layers {
      pr.board.layers = l as u8;
    }
    if let Some(t) = thickness {
      pr.board.thickness = t;
    }
    Ok(())
  })?;
  let pr = project(s)?;
  Ok(json!({"outline": pr.board.outline, "outline_from": pr.outline_from, "layers": pr.board.layers, "thickness": pr.board.thickness}))
}

fn rules(s: &mut Session, p: &Value) -> CmdResult {
  let given: Vec<(String, f64)> = p.as_object().map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect()).unwrap_or_default();
  if !given.is_empty() {
    modify(s, |pr| {
      let mut v = serde_json::to_value(&pr.board.rules).map_err(|e| e.to_string())?;
      for (k, x) in &given {
        if v.get(k).is_none() {
          return Err(format!("unknown rule '{k}'"));
        }
        if *x < 0.0 {
          return Err(format!("'{k}' can't be negative"));
        }
        v[k] = json!(x);
      }
      pr.board.rules = serde_json::from_value(v).map_err(|e| e.to_string())?;
      Ok(())
    })?;
  }
  let empty = Project::new("");
  Ok(json!(s.design.electronics.as_deref().unwrap_or(&empty).board.rules))
}

fn place(s: &mut Session, p: &Value) -> CmdResult {
  let refdes = req_str(p, "refdes")?.to_string();
  let at = p.get("at").and_then(pt);
  let r = num(p, "rot");
  let side = match opt_str(p, "side") {
    Some("top") => Some(Side::Top),
    Some("bottom") => Some(Side::Bottom),
    Some(o) => return Err(CmdError::new(format!("unknown side '{o}' (top, bottom)"))),
    None => None,
  };
  let footprint = opt_str(p, "footprint").map(str::to_string);
  modify(s, |pr| {
    if let Some(f) = &footprint
      && pr.libs().footprint(f).is_none()
    {
      return Err(format!("no footprint '{f}' in the libraries"));
    }
    let pl = pr.board.placements.iter_mut().find(|x| x.refdes == refdes).ok_or(format!("no footprint for {refdes} on the board"))?;
    if let Some(at) = at {
      pl.at = at;
    }
    if let Some(r) = r {
      pl.rot = r.rem_euclid(360.0);
    }
    if let Some(sd) = side {
      pl.side = sd;
    }
    if let Some(f) = footprint {
      pl.footprint = f;
    }
    Ok(json!({"refdes": refdes, "at": pl.at, "rot": pl.rot, "side": pl.side}))
  })
}

fn layer_of(p: &Value, def: Layer) -> Result<Layer, CmdError> {
  match opt_str(p, "layer") {
    Some(l) => Layer::parse(l).ok_or(CmdError::new(format!("unknown layer '{l}' (top, inner1, inner2, bottom)"))),
    None => Ok(def),
  }
}

/// Manual routing: a track along points on a layer; its net is the copper it starts (or ends)
/// on unless given.
fn route(s: &mut Session, p: &Value) -> CmdResult {
  let ps = pts(p, "points")?;
  if ps.len() < 2 {
    return Err(CmdError::new("a track needs two points"));
  }
  let layer = layer_of(p, Layer::Top)?;
  let width = num(p, "width");
  let net = opt_str(p, "net").map(str::to_string);
  modify(s, |pr| {
    if !pr.board.copper_layers().contains(&layer) {
      return Err(format!("the board has no {} layer", layer.name()));
    }
    let pads = pr.pads();
    let net = match net {
      Some(n) => n,
      None => pr
        .board
        .net_at(&pads, ps[0], Some(layer))
        .or_else(|| ps.last().and_then(|q| pr.board.net_at(&pads, *q, Some(layer))))
        .ok_or("the track starts and ends on no copper: pass 'net'")?,
    };
    let w = width.unwrap_or(pr.board.rules.track_width);
    for seg in ps.windows(2) {
      pr.board.tracks.push(Track { layer, a: seg[0], b: seg[1], width: w, net: net.clone() });
    }
    Ok(json!({"net": net, "segments": ps.len() - 1, "layer": layer.name()}))
  })
}

fn via(s: &mut Session, p: &Value) -> CmdResult {
  let at = req_pt(p, "at")?;
  let net = opt_str(p, "net").map(str::to_string);
  let (drill, dia) = (num(p, "drill"), num(p, "diameter"));
  modify(s, |pr| {
    let pads = pr.pads();
    let net = match net {
      Some(n) => n,
      None => pr.board.net_at(&pads, at, None).ok_or("the via is on no copper: pass 'net'")?,
    };
    let r = &pr.board.rules;
    let v = Via { at, drill: drill.unwrap_or(r.via_drill), diameter: dia.unwrap_or(r.via_diameter), net: net.clone() };
    pr.board.vias.push(v);
    Ok(json!({"net": net}))
  })
}

fn pour(s: &mut Session, p: &Value) -> CmdResult {
  let net = req_str(p, "net")?.to_string();
  let layer = layer_of(p, Layer::Bottom)?;
  let outline = if p.get("points").is_some() { Some(pts(p, "points")?) } else { None };
  let clearance = num(p, "clearance");
  modify(s, |pr| {
    if !pr.netlist().nets.iter().any(|n| n.name == net) {
      return Err(format!("no net '{net}'"));
    }
    let outline = match outline {
      Some(o) => o,
      // the board outline, pulled in by the edge clearance
      None => {
        let o = &pr.board.outline;
        if o.len() < 3 {
          return Err("draw the board outline first, or pass 'points'".into());
        }
        let c = fox_ecad::geom::Shape2::Poly(o.clone()).anchor();
        let e = pr.board.rules.edge_clearance;
        o.iter()
          .map(|q| {
            let v = fox_ecad::geom::sub(*q, c);
            let l = fox_ecad::geom::len(v).max(1e-9);
            fox_ecad::geom::sub(*q, fox_ecad::geom::scale(v, e / l))
          })
          .collect()
      }
    };
    pr.board.pours.push(Pour { layer, net: net.clone(), outline, clearance });
    Ok(json!({"pour": pr.board.pours.len() - 1, "net": net, "layer": layer.name()}))
  })
}

fn hole(s: &mut Session, p: &Value) -> CmdResult {
  let at = req_pt(p, "at")?;
  let drill = num(p, "drill").or_else(|| num(p, "diameter")).ok_or(CmdError::new("missing 'drill'"))?;
  modify(s, |pr| {
    pr.board.holes.push(Hole { at, drill });
    Ok(json!({"holes": pr.board.holes.len()}))
  })
}

/// Deletes the tracks, vias, pours and holes under a point.
fn board_delete(s: &mut Session, p: &Value) -> CmdResult {
  let at = req_pt(p, "at")?;
  let layer = match opt_str(p, "layer") {
    Some(l) => Some(Layer::parse(l).ok_or(CmdError::new(format!("unknown layer '{l}'")))?),
    None => None,
  };
  let what = opt_str(p, "what").unwrap_or("any").to_string();
  modify(s, |pr| {
    let b = &mut pr.board;
    let n0 = b.tracks.len() + b.vias.len() + b.pours.len() + b.holes.len();
    let on = |l: Layer| layer.is_none_or(|x| x == l);
    if matches!(what.as_str(), "any" | "track") {
      b.tracks.retain(|t| !(on(t.layer) && fox_ecad::geom::point_seg_dist(at, t.a, t.b) <= t.width / 2.0 + 0.05));
    }
    if matches!(what.as_str(), "any" | "via") {
      b.vias.retain(|v| fox_ecad::geom::dist(v.at, at) > v.diameter / 2.0);
    }
    if what == "pour" {
      b.pours.retain(|q| !(on(q.layer) && fox_ecad::geom::point_in_poly(at, &q.outline)));
    }
    if matches!(what.as_str(), "any" | "hole") {
      b.holes.retain(|h| fox_ecad::geom::dist(h.at, at) > h.drill / 2.0);
    }
    let n1 = b.tracks.len() + b.vias.len() + b.pours.len() + b.holes.len();
    Ok(json!({"deleted": n0 - n1}))
  })
}

fn unroute(s: &mut Session, p: &Value) -> CmdResult {
  let net = opt_str(p, "net").map(str::to_string);
  modify(s, |pr| {
    let n0 = pr.board.tracks.len() + pr.board.vias.len();
    pr.board.tracks.retain(|t| net.as_ref().is_some_and(|n| *n != t.net));
    pr.board.vias.retain(|v| net.as_ref().is_some_and(|n| *n != v.net));
    Ok(json!({"deleted": n0 - pr.board.tracks.len() - pr.board.vias.len()}))
  })
}

fn autoroute(s: &mut Session, p: &Value) -> CmdResult {
  let nets: Vec<String> = p.get("nets").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
  let grid = num(p, "grid").unwrap_or(0.25);
  if !(0.05..=2.0).contains(&grid) {
    return Err(CmdError::new("the routing grid is 0.05 to 2 mm"));
  }
  modify(s, |pr| {
    if pr.board.outline.len() < 3 {
      return Err("draw the board outline first".into());
    }
    let rep = router::autoroute(pr, &nets, grid);
    let unrouted = pr.board.ratsnest(&pr.pads()).len();
    Ok(json!({"routed": rep.routed, "tracks": rep.tracks, "vias": rep.vias, "failed": rep.failed.iter().map(|f| json!({"net": f.0, "from": f.1, "to": f.2})).collect::<Vec<_>>(), "unrouted": unrouted}))
  })
}

/// Writes the fab files (Gerbers, drill files, BOM, pick-and-place) into a folder.
fn export(s: &Session, p: &Value) -> CmdResult {
  let pr = project(s)?;
  let dir = PathBuf::from(req_str(p, "dir").or_else(|_| req_str(p, "path"))?);
  let only = opt_str(p, "what").unwrap_or("all");
  std::fs::create_dir_all(&dir)?;
  let mut written = Vec::new();
  for (name, text) in gerber::fab_files(pr) {
    let keep = match only {
      "gerber" => name.ends_with(".gbr"),
      "drill" => name.ends_with(".drl"),
      "bom" => name.ends_with("-BOM.csv"),
      "pnp" | "cpl" => name.ends_with("-PnP.csv"),
      "all" => true,
      o => return Err(CmdError::new(format!("unknown output '{o}' (all, gerber, drill, bom, pnp)"))),
    };
    if keep {
      let path = dir.join(&name);
      std::fs::write(&path, text)?;
      written.push(path);
    }
  }
  let drc = pr.drc();
  let errors = drc.iter().filter(|v| v.severity == fox_ecad::Severity::Error).count();
  Ok(json!({"files": written, "drc_errors": errors}))
}

fn svg_cmd(s: &Session, p: &Value) -> CmdResult {
  let pr = project(s)?;
  let text = match opt_str(p, "view").unwrap_or("board") {
    "schematic" => svg::schematic(pr),
    "board" => svg::board(pr),
    o => return Err(CmdError::new(format!("unknown view '{o}' (schematic, board)"))),
  };
  match opt_str(p, "path") {
    Some(path) => {
      std::fs::write(path, &text)?;
      Ok(json!({"path": path, "bytes": text.len()}))
    }
    None => Ok(json!({"svg": text})),
  }
}

/// The design's PCB feature, if the board was pushed to 3D.
pub(crate) fn pcb_feature(d: &Design) -> Option<FeatureId> {
  d.features.iter().find(|f| matches!(f.kind, FeatureKind::Pcb { .. })).map(|f| f.id)
}

/// Pushes the board to 3D: a PCB feature in the timeline and a component holding the board and
/// its parts. After this, layout changes update it.
fn push3d(s: &mut Session) -> CmdResult {
  let pr = project(s)?;
  if pr.board.outline.len() < 3 {
    return Err(CmdError::new("the board has no outline"));
  }
  if pcb_feature(&s.design).is_none() {
    let kind = pcb_kind(pr, None);
    let source = pr.outline_from.clone();
    let fresh = match &source {
      Some(src) => Some(outline_source(s, src)?),
      None => None,
    };
    s.edit(|d| {
      d.add_feature(kind)?;
      sync_feature(d, fresh.map(|f| (f.1, f.2)))
    })?;
  } else {
    modify(s, |_| Ok(()))?;
  }
  tidy_component(s);
  let id = pcb_feature(&s.design).ok_or(CmdError::new("the PCB feature is missing"))?;
  let status = s.model.status(id);
  if let Some(e) = status.and_then(|x| x.error.clone()) {
    return Err(CmdError::new(format!("the 3D board failed: {e}")));
  }
  let bodies: Vec<u32> = s.model.bodies.iter().filter(|b| b.feature == id).map(|b| b.id.0).collect();
  let comp = s.design.components.iter().find(|c| c.bodies.iter().any(|b| bodies.contains(&b.0))).map(|c| c.id.0);
  Ok(json!({"feature": id.0, "bodies": bodies, "component": comp}))
}

/// Expands `${VAR}` from the environment; KiCad model paths name their library folder so.
fn expand_vars(path: &str) -> String {
  let mut out = String::new();
  let mut rest = path;
  while let Some(i) = rest.find("${") {
    out.push_str(&rest[..i]);
    match rest[i + 2..].find('}') {
      Some(j) => {
        let var = &rest[i + 2..i + 2 + j];
        out.push_str(&std::env::var(var).unwrap_or_default());
        rest = &rest[i + 3 + j..];
      }
      None => {
        out.push_str(&rest[i..]);
        rest = "";
      }
    }
  }
  out.push_str(rest);
  out
}

/// A footprint's 3D model for the board: its own, a STEP file found beside a referenced model,
/// or a box the size of its courtyard.
fn part_model(pr: &Project, refdes: &str) -> Option<(PcbModel, Option<[f32; 3]>)> {
  let libs = pr.libs();
  let pl = pr.board.placement(refdes)?;
  let part = pr.schematic.parts.iter().find(|p| p.refdes == refdes)?;
  let dev = libs.device(&part.device)?;
  let fp = libs.footprint(if pl.footprint.is_empty() { &dev.footprint } else { &pl.footprint })?;
  let model = dev.model.as_ref().or(fp.model.as_ref());
  let fallback = || {
    let (lo, hi) = fp.courtyard();
    let inset = 0.25;
    (
      PcbModel::Box {
        size: [(hi[0] - lo[0] - 2.0 * inset).max(0.2), (hi[1] - lo[1] - 2.0 * inset).max(0.2), 1.0],
        offset: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0],
      },
      Some([0.15, 0.15, 0.15]),
    )
  };
  Some(match model {
    Some(Model3d::Box { size, offset, color }) => (PcbModel::Box { size: *size, offset: *offset }, *color),
    Some(Model3d::Cylinder { diameter, height, offset, color }) => (PcbModel::Cylinder { diameter: *diameter, height: *height, offset: *offset }, *color),
    Some(Model3d::File { path, offset, rotate, scale }) => {
      let full = expand_vars(path);
      let base = Path::new(&full);
      let found = [base.to_path_buf(), base.with_extension("step"), base.with_extension("stp")]
        .into_iter()
        .find(|c| c.is_file() && c.extension().is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp")));
      match found {
        Some(f) => (PcbModel::File { path: f.to_string_lossy().into_owned(), offset: *offset, rotate: *rotate, scale: *scale }, Some([0.6, 0.6, 0.6])),
        None => fallback(),
      }
    }
    None => fallback(),
  })
}

fn pcb_kind(pr: &Project, source: Option<(PcbOutline, Option<fox_model::PlaneRef>)>) -> FeatureKind {
  let (outline, plane) = source.unwrap_or_else(|| (PcbOutline::Polygon { points: pr.board.outline.clone() }, None));
  let holes = pr.board.drills(&pr.pads()).into_iter().map(|(at, d, _)| [at[0], at[1], d]).collect();
  let mut parts = Vec::new();
  for pl in &pr.board.placements {
    if let Some((model, _)) = part_model(pr, &pl.refdes) {
      parts.push(PcbPart { refdes: pl.refdes.clone(), at: pl.at, rot: pl.rot, bottom: pl.side == Side::Bottom, model });
    }
  }
  FeatureKind::Pcb { name: format!("PCB {}", pr.name), outline, plane, thickness: pr.board.thickness, holes, parts }
}

/// Rewrites the PCB feature (if the board was pushed to 3D) from the layout. A polygon outline
/// keeps the plane it was given.
fn sync_feature(d: &mut Design, source: Option<(PcbOutline, Option<fox_model::PlaneRef>)>) -> Result<(), CmdError> {
  let Some(pr) = d.electronics.as_deref() else { return Ok(()) };
  let Some(id) = pcb_feature(d) else { return Ok(()) };
  let mut kind = pcb_kind(pr, source.clone());
  let f = d.feature_mut(id).ok_or(CmdError::new("the PCB feature is missing"))?;
  if source.is_none()
    && let (FeatureKind::Pcb { plane: old, .. }, FeatureKind::Pcb { plane, .. }) = (&f.kind, &mut kind)
  {
    *plane = old.clone();
  }
  if f.kind != kind {
    f.kind = kind;
  }
  Ok(())
}

/// Keeps the PCB's bodies together in a component named after the board, coloured: the board
/// green, the parts after their models. Part of the step that changed them.
fn tidy_component(s: &mut Session) {
  let Some(id) = pcb_feature(&s.design) else { return };
  let Some(pr) = s.design.electronics.as_deref() else { return };
  let name = pr.name.clone();
  let colors: Vec<(String, Option<[f32; 3]>)> = pr.board.placements.iter().map(|pl| (pl.refdes.clone(), part_model(pr, &pl.refdes).and_then(|m| m.1))).collect();
  let bodies: Vec<(fox_model::BodyId, String)> = s.model.bodies.iter().filter(|b| b.feature == id).map(|b| (b.id, b.name.clone())).collect();
  if bodies.is_empty() {
    return;
  }
  let ids: Vec<fox_model::BodyId> = bodies.iter().map(|b| b.0).collect();
  let d = &mut s.design;
  let existing = d.components.iter().position(|c| c.name == name || c.bodies.iter().any(|b| ids.contains(b)));
  match existing {
    Some(i) => {
      if d.components[i].bodies != ids {
        // a body belongs to one component
        for (k, c) in d.components.iter_mut().enumerate() {
          if k != i {
            c.bodies.retain(|b| !ids.contains(b));
          }
        }
        d.components[i].bodies = ids.clone();
      }
    }
    None => {
      for c in d.components.iter_mut() {
        c.bodies.retain(|b| !ids.contains(b));
      }
      if let Ok(cid) = d.add_component(&name, ids.clone())
        && let Some(c) = d.component_mut(cid)
      {
        c.description = "Printed circuit board".into();
      }
    }
  }
  let to8 = |c: [f32; 3]| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8];
  for (i, (bid, bname)) in bodies.iter().enumerate() {
    let color = if i == 0 { Some([20, 110, 50]) } else { colors.iter().find(|c| c.0 == *bname).and_then(|c| c.1).map(to8) };
    if let Some(c) = color {
      let st = d.body_settings_mut(*bid);
      if st.color.is_none() {
        st.color = Some(c);
      }
    }
  }
}
