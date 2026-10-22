//! Simulation commands (`sim.*`): studies (linear static stress, modal frequencies, steady-state
//! thermal), their materials, constraints, loads, contacts and mesh settings; meshing, solving,
//! probing and reports. Studies live in the design ([`fox_model::sim`]); meshes and results stay
//! in the session ([`SimRun`]) and are drawn as contours (see [`draw`]).

use fox_kernel::glam::DVec3;
use fox_model::BodyId;
use fox_model::sim::{self, ContactMode, Load, Study, StudyKind};
use fox_sim::results::{self, Solution};
use fox_sim::{FaceSel, Material, MeshOptions, MeshStats, Model, Part, Refinement, SurfaceMesh};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str, req_vec3};
use crate::{CmdError, CmdResult, Session};

/// A meshed (and maybe solved) study.
#[derive(Debug, Clone)]
pub struct SimRun {
  pub study: u32,
  /// The study definition and design the mesh was made for: stale once either changes.
  pub def: Study,
  pub features: Vec<fox_model::Feature>,
  pub model: Model,
  /// The body of each part.
  pub bodies: Vec<BodyId>,
  pub stats: Vec<MeshStats>,
  pub solution: Option<Solution>,
}

impl SimRun {
  fn current(&self, s: &Session, st: &Study) -> bool {
    self.def.bodies == st.bodies
      && self.def.mesh == st.mesh
      && self.def.materials == st.materials
      && self.features == s.design.features
  }
}

/// How the viewport shows a study's results.
#[derive(Debug, Clone, PartialEq)]
pub struct SimView {
  pub study: u32,
  /// A field id (see [`fox_sim::results::Solution::fields`]); empty = the study's default.
  pub field: String,
  /// The mode shown (modal, from 1).
  pub mode: usize,
  /// Deformation scale: `None` = adjusted (the largest displacement shows as 5 % of the model's
  /// size), `Some(0)` = undeformed, `Some(1)` = true scale.
  pub scale: Option<f64>,
  /// Animation phase (radians): the deformation is scaled by its sine (mode shapes); `None` = still.
  pub phase: Option<f64>,
  /// Show the mesh instead of results.
  pub mesh: bool,
}

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "sim.study" => create(s, p),
    "sim.edit" => edit(s, p),
    "sim.delete" => delete(s, p),
    "sim.list" => Ok(json!(s.design.sim.iter().map(|st| study_json(s, st)).collect::<Vec<_>>())),
    "sim.info" => index(s, p).map(|i| study_json(s, &s.design.sim[i])),
    "sim.materials" => Ok(json!(Material::library())),
    "sim.material" => material(s, p),
    "sim.constraint" => constraint(s, p),
    "sim.load" => load(s, p),
    "sim.remove" => remove(s, p),
    "sim.contact" => contact(s, p),
    "sim.mesh" => mesh(s, p),
    "sim.solve" => solve(s, p),
    "sim.results" => results_cmd(s, p),
    "sim.probe" => probe(s, p),
    "sim.report" => report(s, p),
    _ => return None,
  })
}

/// The study a command names (`study`: id or name), or the last one.
fn index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  match p.get("study") {
    Some(Value::Number(n)) => {
      let id = n.as_u64().unwrap_or(0);
      s.design.sim.iter().position(|st| u64::from(st.id) == id).ok_or(CmdError::new(format!("no study {id}")))
    }
    Some(Value::String(n)) => {
      s.design.sim.iter().position(|st| st.name.eq_ignore_ascii_case(n)).ok_or(CmdError::new(format!("no study '{n}'")))
    }
    _ if s.design.sim.is_empty() => Err(CmdError::new("there is no simulation study: sim.study first")),
    _ => Ok(s.design.sim.len() - 1),
  }
}

fn num(p: &Value, k: &str) -> Option<f64> {
  match p.get(k)? {
    Value::Number(n) => n.as_f64(),
    Value::String(e) => {
      let scope = std::collections::HashMap::new();
      fox_expr::eval(e, fox_expr::Kind::Unitless, &scope)
        .or_else(|_| fox_expr::eval(e, fox_expr::Kind::Length, &scope))
        .ok()
    }
    _ => None,
  }
  .filter(|v| v.is_finite())
}

fn flag(p: &Value, k: &str, default: bool) -> bool {
  p.get(k).and_then(Value::as_bool).unwrap_or(default)
}

fn bodies_of(s: &Session, p: &Value, k: &str) -> Result<Option<Vec<BodyId>>, CmdError> {
  let Some(v) = p.get(k) else { return Ok(None) };
  let list = match v {
    Value::Array(a) => a.clone(),
    Value::Null => return Ok(Some(Vec::new())),
    other => vec![other.clone()],
  };
  let mut out = Vec::new();
  for b in list {
    out.push(body_of(s, &b)?);
  }
  Ok(Some(out))
}

fn body_of(s: &Session, v: &Value) -> Result<BodyId, CmdError> {
  match v {
    Value::Number(n) => {
      let id = BodyId(n.as_u64().unwrap_or(0) as u32);
      s.model.body(id).map(|b| b.id).ok_or(CmdError::new(format!("no body {}", id.0)))
    }
    Value::String(name) => s
      .model
      .bodies
      .iter()
      .find(|b| body_name(s, b.id).eq_ignore_ascii_case(name))
      .map(|b| b.id)
      .ok_or(CmdError::new(format!("no body '{name}'"))),
    _ => Err(CmdError::new("a body is an id or a name")),
  }
}

fn body_name(s: &Session, id: BodyId) -> String {
  s.design
    .body_settings(id)
    .and_then(|b| b.name.clone())
    .or_else(|| s.model.body(id).map(|b| b.name.clone()))
    .unwrap_or_else(|| format!("Body {}", id.0))
}

fn faces(s: &Session, p: &Value) -> Result<Vec<fox_model::FaceRef>, CmdError> {
  let list = match p.get("faces").or_else(|| p.get("face")) {
    Some(Value::Array(a)) => a.clone(),
    Some(v) => vec![v.clone()],
    None => return Err(CmdError::new("missing 'faces'")),
  };
  if list.is_empty() {
    return Err(CmdError::new("'faces' is empty"));
  }
  list.iter().map(|f| face_ref(s, f)).collect()
}

fn create(s: &mut Session, p: &Value) -> CmdResult {
  let kind = match opt_str(p, "type") {
    Some(t) => {
      StudyKind::parse(t).ok_or(CmdError::new(format!("unknown study type '{t}' (static, modal, thermal)")))?
    }
    None => StudyKind::Static,
  };
  let bodies = bodies_of(s, p, "bodies")?.unwrap_or_default();
  let id = s.design.sim.iter().map(|st| st.id).max().unwrap_or(0) + 1;
  let name = opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("{} {id}", kind.label()));
  let mut st = Study::new(id, name, kind);
  st.bodies = bodies;
  if let Some(m) = p.get("modes").and_then(Value::as_u64) {
    st.modes = (m as u32).clamp(1, 100);
  }
  if kind == StudyKind::Static && flag(p, "gravity", false) {
    st.loads.push(Load::Gravity { acceleration: [0.0, 0.0, -9806.65] });
  }
  let out = json!({"study": id, "name": st.name, "type": kind.id()});
  s.edit(|d| {
    d.sim.push(st);
    Ok(())
  })?;
  Ok(out)
}

fn edit(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let bodies = bodies_of(s, p, "bodies")?;
  let kind = match opt_str(p, "type") {
    Some(t) => Some(StudyKind::parse(t).ok_or(CmdError::new(format!("unknown study type '{t}'")))?),
    None => None,
  };
  s.edit(|d| {
    let st = &mut d.sim[i];
    if let Some(n) = opt_str(p, "name") {
      st.name = n.to_string();
    }
    if let Some(k) = kind {
      st.kind = k;
    }
    if let Some(b) = bodies {
      st.bodies = b;
    }
    if let Some(m) = p.get("modes").and_then(Value::as_u64) {
      st.modes = (m as u32).clamp(1, 100);
    }
    Ok(())
  })?;
  Ok(study_json(s, &s.design.sim[i]))
}

fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let id = s.design.sim[i].id;
  s.edit(|d| {
    d.sim.remove(i);
    Ok(())
  })?;
  s.sim_runs.retain(|r| r.study != id);
  Ok(json!({"deleted": id}))
}

fn material(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let targets: Vec<BodyId> = match bodies_of(s, p, "bodies")?.or(bodies_of(s, p, "body")?) {
    Some(b) if !b.is_empty() => b,
    _ => study_bodies(s, &s.design.sim[i]),
  };
  let name = opt_str(p, "material").unwrap_or("").to_string();
  if !name.is_empty() && !Material::library().iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
    return Err(CmdError::new(format!(
      "unknown material '{name}' ({})",
      Material::library().iter().map(|m| m.name.clone()).collect::<Vec<_>>().join(", ")
    )));
  }
  let ov = |body| sim::MaterialOverride {
    body,
    material: name.clone(),
    youngs: num(p, "youngs"),
    poisson: num(p, "poisson"),
    density: num(p, "density"),
    yield_strength: num(p, "yield").or(num(p, "yield_strength")),
    conductivity: num(p, "conductivity"),
  };
  s.edit(|d| {
    let st = &mut d.sim[i];
    for &b in &targets {
      st.materials.retain(|m| m.body != b);
      let o = ov(b);
      if o != sim::MaterialOverride::none(b) {
        st.materials.push(o);
      }
    }
    Ok(())
  })?;
  let st = &s.design.sim[i];
  Ok(json!(targets.iter().map(|b| json!({"body": b.0, "material": material_of(s, st, *b)})).collect::<Vec<_>>()))
}

/// A body's simulation material: the study's override over its component's physical material.
pub fn material_of(s: &Session, st: &Study, body: BodyId) -> Material {
  let base = s
    .design
    .components
    .iter()
    .find(|c| c.bodies.contains(&body) && !c.material.is_empty())
    .map(|c| c.material.as_str())
    .unwrap_or("Steel");
  let Some(o) = st.materials.iter().find(|m| m.body == body) else { return Material::named(base) };
  let mut m = Material::named(if o.material.is_empty() { base } else { &o.material });
  let custom = o.youngs.is_some() || o.poisson.is_some() || o.density.is_some() || o.yield_strength.is_some();
  if let Some(v) = o.youngs {
    m.youngs = v;
  }
  if let Some(v) = o.poisson {
    m.poisson = v;
  }
  if let Some(v) = o.density {
    m.density = v;
  }
  if let Some(v) = o.yield_strength {
    m.yield_strength = v;
  }
  if let Some(v) = o.conductivity {
    m.conductivity = v;
  }
  if custom || o.conductivity.is_some() {
    m.name = format!("{} (custom)", m.name);
  }
  m
}

/// The bodies a study simulates: its own list, or every visible solid.
pub fn study_bodies(s: &Session, st: &Study) -> Vec<BodyId> {
  if !st.bodies.is_empty() {
    return st.bodies.iter().copied().filter(|b| s.model.body(*b).is_some()).collect();
  }
  s.model.bodies.iter().filter(|b| s.body_visible(b.id) && b.shape.solid_count() > 0).map(|b| b.id).collect()
}

fn constraint(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let ty = opt_str(p, "type").unwrap_or("fixed");
  let faces = faces(s, p)?;
  let c = match ty {
    "fixed" => sim::Constraint::Fixed { faces, x: flag(p, "x", true), y: flag(p, "y", true), z: flag(p, "z", true) },
    "pin" => sim::Constraint::Pin {
      faces,
      radial: flag(p, "radial", true),
      axial: flag(p, "axial", true),
      tangential: flag(p, "tangential", false),
    },
    "frictionless" => sim::Constraint::Frictionless { faces },
    other => return Err(CmdError::new(format!("unknown constraint '{other}' (fixed, pin, frictionless)"))),
  };
  s.edit(|d| {
    d.sim[i].constraints.push(c);
    Ok(())
  })?;
  Ok(json!({"constraint": s.design.sim[i].constraints.len() - 1}))
}

/// A force-like vector: `[x, y, z]` in `key`, or a `magnitude` along `direction` ([x, y, z]),
/// `normal` (into the first face; negative: out of it) or an axis name.
fn vector(s: &Session, p: &Value, key: &str, faces: &[fox_model::FaceRef]) -> Result<[f64; 3], CmdError> {
  if p.get(key).is_some_and(Value::is_array) {
    return req_vec3(p, key);
  }
  let mag =
    num(p, "magnitude").or(num(p, key)).ok_or(CmdError::new(format!("missing '{key}' ([x, y, z]) or 'magnitude'")))?;
  let dir = match p.get("direction") {
    Some(Value::Array(_)) => DVec3::from(req_vec3(p, "direction")?),
    Some(Value::String(a)) => match a.trim_start_matches('+') {
      "x" => DVec3::X,
      "y" => DVec3::Y,
      "z" => DVec3::Z,
      "-x" => -DVec3::X,
      "-y" => -DVec3::Y,
      "-z" => -DVec3::Z,
      "normal" => -face_normal(s, faces)?,
      other => return Err(CmdError::new(format!("unknown direction '{other}'"))),
    },
    None => -face_normal(s, faces)?,
    _ => return Err(CmdError::new("'direction' is [x, y, z], an axis or 'normal'")),
  };
  let d = dir.try_normalize().ok_or(CmdError::new("the direction has no length"))?;
  Ok((d * mag).to_array())
}

/// The first face's outward normal.
fn face_normal(s: &Session, faces: &[fox_model::FaceRef]) -> Result<DVec3, CmdError> {
  let f = faces.first().ok_or(CmdError::new("no faces"))?;
  let b = s.model.body(f.body).ok_or(CmdError::new("the face's body is gone"))?;
  let id = f.resolve(b.faces(), b.mesh(), b.diagonal()).ok_or(CmdError::new("the face can't be found"))?;
  let info = b.faces().iter().find(|x| x.id == id).ok_or(CmdError::new("the face can't be found"))?;
  if info.kind == fox_kernel::SurfaceKind::Plane {
    return Ok(info.normal);
  }
  // curved faces: the average of their triangles' normals
  let m = b.mesh().ok_or(CmdError::new("the body has no mesh"))?;
  let mut n = DVec3::ZERO;
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if m.tri_face.get(t) == Some(&id) {
      let [a, b, c] = [0, 1, 2].map(|k| DVec3::from(m.positions[tri[k] as usize].map(f64::from)));
      n += (b - a).cross(c - a);
    }
  }
  n.try_normalize().ok_or(CmdError::new("the face has no single direction: give one"))
}

fn load(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let ty = opt_str(p, "type").unwrap_or("force");
  let need = |k: &str| num(p, k).ok_or(CmdError::new(format!("missing '{k}'")));
  let l = match ty {
    "force" => {
      let faces = faces(s, p)?;
      Load::Force { force: vector(s, p, "force", &faces)?, faces }
    }
    "pressure" => Load::Pressure { faces: faces(s, p)?, pressure: need("pressure").or(need("value"))? },
    "bearing" => {
      let faces = faces(s, p)?;
      Load::Bearing { force: vector(s, p, "force", &faces)?, faces }
    }
    "moment" => {
      let faces = faces(s, p)?;
      Load::Moment { moment: vector(s, p, "moment", &faces)?, faces }
    }
    "gravity" => Load::Gravity {
      acceleration: if p.get("acceleration").is_some() { req_vec3(p, "acceleration")? } else { [0.0, 0.0, -9806.65] },
    },
    "temperature" => Load::Temperature { faces: faces(s, p)?, temperature: need("temperature").or(need("value"))? },
    "heat_flux" => Load::HeatFlux { faces: faces(s, p)?, flux: need("flux").or(need("value"))? },
    "heat" | "heat_power" | "heat_source" => {
      Load::HeatPower { faces: faces(s, p)?, power: need("power").or(need("value"))? }
    }
    "convection" => Load::Convection {
      faces: faces(s, p)?,
      coefficient: need("coefficient").or(need("value"))?,
      ambient: num(p, "ambient").unwrap_or(20.0),
    },
    "internal_heat" => Load::InternalHeat {
      body: body_of(s, p.get("body").ok_or(CmdError::new("missing 'body'"))?)?,
      power: need("power").or(need("value"))?,
    },
    other => {
      return Err(CmdError::new(format!(
        "unknown load '{other}' (force, pressure, bearing, moment, gravity, temperature, heat_flux, heat, convection, internal_heat)"
      )));
    }
  };
  let kind = s.design.sim[i].kind;
  if l.thermal() != (kind == StudyKind::Thermal) {
    return Err(CmdError::new(format!("a {} isn't a load of a {} study", l.label(), kind.label())));
  }
  s.edit(|d| {
    d.sim[i].loads.push(l);
    Ok(())
  })?;
  Ok(json!({"load": s.design.sim[i].loads.len() - 1}))
}

fn remove(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let c = p.get("constraint").and_then(Value::as_u64).map(|v| v as usize);
  let l = p.get("load").and_then(Value::as_u64).map(|v| v as usize);
  let r = p.get("refinement").and_then(Value::as_u64).map(|v| v as usize);
  if c.is_none() && l.is_none() && r.is_none() {
    return Err(CmdError::new("give 'constraint', 'load' or 'refinement' (an index)"));
  }
  s.edit(|d| {
    let st = &mut d.sim[i];
    if let Some(c) = c {
      (c < st.constraints.len())
        .then(|| st.constraints.remove(c))
        .ok_or(CmdError::new(format!("no constraint {c}")))?;
    }
    if let Some(l) = l {
      (l < st.loads.len()).then(|| st.loads.remove(l)).ok_or(CmdError::new(format!("no load {l}")))?;
    }
    if let Some(r) = r {
      (r < st.mesh.refine.len())
        .then(|| st.mesh.refine.remove(r))
        .ok_or(CmdError::new(format!("no refinement {r}")))?;
    }
    Ok(())
  })?;
  Ok(study_json(s, &s.design.sim[i]))
}

fn contact(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let mode = match opt_str(p, "mode") {
    Some("automatic") => Some(ContactMode::Automatic),
    Some("manual") => Some(ContactMode::Manual),
    Some(other) => return Err(CmdError::new(format!("unknown contact mode '{other}' (automatic, manual)"))),
    None => None,
  };
  let pair = |k: &str| -> Result<Option<(BodyId, BodyId)>, CmdError> {
    match bodies_of(s, p, k)? {
      Some(b) if b.len() == 2 && b[0] != b[1] => Ok(Some((b[0], b[1]))),
      Some(_) => Err(CmdError::new(format!("'{k}' takes two different bodies"))),
      None => Ok(None),
    }
  };
  let (bond, unbond) = (pair("bond")?, pair("unbond")?);
  s.edit(|d| {
    let st = &mut d.sim[i];
    if let Some(m) = mode {
      st.contacts = m;
    }
    if let Some((a, b)) = bond {
      st.contacts = ContactMode::Manual;
      if !st.bonded.iter().any(|&(x, y)| (x, y) == (a, b) || (y, x) == (a, b)) {
        st.bonded.push((a, b));
      }
    }
    if let Some((a, b)) = unbond {
      st.bonded.retain(|&(x, y)| !((x, y) == (a, b) || (y, x) == (a, b)));
    }
    Ok(())
  })?;
  let st = &s.design.sim[i];
  Ok(json!({"mode": if st.contacts == ContactMode::Automatic { "automatic" } else { "manual" },
    "bonded": st.bonded.iter().map(|(a, b)| [a.0, b.0]).collect::<Vec<_>>()}))
}

/// The settings part of `sim.mesh`, then meshing.
fn mesh(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let size = match p.get("size") {
    Some(Value::Null) => Some(None),
    Some(Value::String(t)) if t == "auto" => Some(None),
    Some(_) => {
      Some(Some(num(p, "size").filter(|v| *v > 0.0).ok_or(CmdError::new("'size' must be a positive length"))?))
    }
    None => None,
  };
  let mut refine = Vec::new();
  for r in p.get("refine").and_then(Value::as_array).into_iter().flatten() {
    let f = faces(s, r)?;
    let size = num(r, "size").filter(|v| *v > 0.0).ok_or(CmdError::new("a refinement needs a positive 'size'"))?;
    refine.push(sim::MeshRefinement { faces: f, size });
  }
  let clear = flag(p, "clear_refine", false);
  if size.is_some() || !refine.is_empty() || clear {
    s.edit(|d| {
      let m = &mut d.sim[i].mesh;
      if let Some(v) = size {
        m.size = v;
      }
      if clear {
        m.refine.clear();
      }
      m.refine.extend(refine);
      Ok(())
    })?;
  }
  let run = mesh_study(s, i)?;
  Ok(mesh_json(s, &run))
}

fn mesh_json(s: &Session, run: &SimRun) -> Value {
  json!({
    "study": run.study,
    "nodes": run.model.node_count(),
    "elements": run.model.element_count(),
    "bodies": run.bodies.iter().zip(&run.stats).zip(&run.model.parts).map(|((b, st), part)| json!({
      "body": b.0, "name": body_name(s, *b), "material": part.material.name, "size": part.size,
      "nodes": st.nodes, "elements": st.elements, "volume": st.volume,
      "min_quality": st.min_quality, "mean_quality": st.mean_quality,
    })).collect::<Vec<_>>(),
  })
}

/// Meshes a study's bodies (reusing the last mesh while nothing it depends on changed).
fn mesh_study(s: &mut Session, i: usize) -> Result<SimRun, CmdError> {
  let st = s.design.sim[i].clone();
  if let Some(r) = s.sim_runs.iter().find(|r| r.study == st.id)
    && r.current(s, &st)
  {
    return Ok(r.clone());
  }
  let bodies = study_bodies(s, &st);
  if bodies.is_empty() {
    return Err(CmdError::new("the study has no solid bodies to simulate"));
  }
  let mut parts = Vec::new();
  let mut stats = Vec::new();
  for &b in &bodies {
    let body = s.model.body(b).ok_or(CmdError::new(format!("no body {}", b.0)))?;
    let (lo, hi) = body.shape.mass().map(|m| (m.bbox_min, m.bbox_max)).map_err(|e| CmdError::new(e.to_string()))?;
    let mut opts = MeshOptions::auto(lo.to_array(), hi.to_array());
    if let Some(size) = st.mesh.size {
      opts.size = size;
    }
    for r in &st.mesh.refine {
      let ids: Vec<u32> = r
        .faces
        .iter()
        .filter(|f| f.body == b)
        .filter_map(|f| f.resolve(body.faces(), body.mesh(), body.diagonal()))
        .collect();
      if !ids.is_empty() {
        opts.refine.push(Refinement { faces: ids, size: r.size });
      }
    }
    let min_size = opts.refine.iter().map(|r| r.size).fold(opts.size, f64::min);
    let tm = body
      .shape
      .tessellate_fine((min_size * 0.05).max(body.diagonal() * 1e-5), 0.25)
      .map_err(|e| CmdError::new(format!("{}: {e}", body_name(s, b))))?;
    let positions: Vec<[f64; 3]> = tm.positions.iter().map(|p| p.map(f64::from)).collect();
    let surf = SurfaceMesh::weld(&positions, &tm.indices, &tm.tri_face, body.diagonal() * 1e-6);
    let (mesh, st_) =
      fox_sim::mesh_solid(&surf, &opts).map_err(|e| CmdError::new(format!("{}: {e}", body_name(s, b))))?;
    parts.push(Part { name: body_name(s, b), mesh, material: material_of(s, &st, b), size: opts.size });
    stats.push(st_);
  }
  let run = SimRun {
    study: st.id,
    def: st.clone(),
    features: s.design.features.clone(),
    model: Model { parts, ..Default::default() },
    bodies,
    stats,
    solution: None,
  };
  s.sim_runs.retain(|r| r.study != st.id);
  s.sim_runs.push(run.clone());
  Ok(run)
}

/// The face selections of refs in a meshed run.
fn sels(s: &Session, run: &SimRun, refs: &[fox_model::FaceRef]) -> Result<Vec<FaceSel>, CmdError> {
  let mut out = Vec::new();
  for f in refs {
    let Some(part) = run.bodies.iter().position(|b| *b == f.body) else {
      return Err(CmdError::new(format!("a face is on body {}, which isn't in the study", f.body.0)));
    };
    let body = s.model.body(f.body).ok_or(CmdError::new("a face's body is gone"))?;
    let ids = f.resolve_all(body.faces(), body.mesh(), body.diagonal());
    if ids.is_empty() {
      return Err(CmdError::new(format!("a face on {} can't be found any more", body_name(s, f.body))));
    }
    out.extend(ids.into_iter().map(|face| FaceSel { part, face }));
  }
  Ok(out)
}

/// The solver model: the mesh with the study's constraints, loads and contacts.
fn build_model(s: &Session, st: &Study, run: &SimRun) -> Result<Model, CmdError> {
  let mut m = Model { parts: run.model.parts.clone(), ..Default::default() };
  for (k, p) in m.parts.iter_mut().enumerate() {
    p.material = material_of(s, st, run.bodies[k]);
  }
  for c in &st.constraints {
    let faces = sels(s, run, c.faces())?;
    m.constraints.push(match c {
      sim::Constraint::Fixed { x, y, z, .. } => fox_sim::Constraint::Fixed { faces, x: *x, y: *y, z: *z },
      sim::Constraint::Pin { radial, axial, tangential, .. } => {
        fox_sim::Constraint::Pin { faces, radial: *radial, axial: *axial, tangential: *tangential, axis: None }
      }
      sim::Constraint::Frictionless { .. } => fox_sim::Constraint::Frictionless { faces },
    });
  }
  for l in &st.loads {
    let f = || sels(s, run, l.faces());
    match l {
      Load::Force { force, .. } => m.loads.push(fox_sim::Load::Force { faces: f()?, force: *force }),
      Load::Pressure { pressure, .. } => m.loads.push(fox_sim::Load::Pressure { faces: f()?, pressure: *pressure }),
      Load::Bearing { force, .. } => m.loads.push(fox_sim::Load::Bearing { faces: f()?, force: *force }),
      Load::Moment { moment, .. } => m.loads.push(fox_sim::Load::Moment { faces: f()?, moment: *moment }),
      Load::Gravity { acceleration } => m.loads.push(fox_sim::Load::Gravity { acceleration: *acceleration }),
      Load::Temperature { temperature, .. } => {
        m.thermal.push(fox_sim::ThermalLoad::Temperature { faces: f()?, temperature: *temperature })
      }
      // W/m² → W/mm²
      Load::HeatFlux { flux, .. } => m.thermal.push(fox_sim::ThermalLoad::HeatFlux { faces: f()?, flux: flux * 1e-6 }),
      Load::HeatPower { power, .. } => m.thermal.push(fox_sim::ThermalLoad::HeatPower { faces: f()?, power: *power }),
      Load::Convection { coefficient, ambient, .. } => m.thermal.push(fox_sim::ThermalLoad::Convection {
        faces: f()?,
        coefficient: coefficient * 1e-6,
        ambient: *ambient,
      }),
      Load::InternalHeat { body, power } => {
        let part =
          run.bodies.iter().position(|b| b == body).ok_or(CmdError::new("the heated body isn't in the study"))?;
        m.thermal.push(fox_sim::ThermalLoad::InternalHeat { part, power: *power })
      }
    }
  }
  m.contacts = match st.contacts {
    ContactMode::Automatic => fox_sim::Contacts::Automatic,
    ContactMode::Manual => fox_sim::Contacts::Bonded {
      pairs: st
        .bonded
        .iter()
        .filter_map(|(a, b)| Some((run.bodies.iter().position(|x| x == a)?, run.bodies.iter().position(|x| x == b)?)))
        .collect(),
      tolerance: None,
    },
  };
  Ok(m)
}

fn solve(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let st = s.design.sim[i].clone();
  let run = mesh_study(s, i)?;
  let model = build_model(s, &st, &run)?;
  let opts = fox_sim::SolveOptions { modes: st.modes as usize, ..Default::default() };
  let kind = match st.kind {
    StudyKind::Static => fox_sim::StudyKind::Static,
    StudyKind::Modal => fox_sim::StudyKind::Modal,
    StudyKind::Thermal => fox_sim::StudyKind::Thermal,
  };
  let sol = fox_sim::solve(kind, &model, opts).map_err(CmdError::new)?;
  let summary: Vec<Value> =
    results::summary(&model, &sol).into_iter().map(|(k, v)| json!({"quantity": k, "value": v})).collect();
  let out = json!({
    "study": st.id, "type": st.kind.id(), "nodes": model.node_count(), "elements": model.element_count(),
    "ties": model.ties().len(), "summary": summary, "results": result_values(&model, &sol),
  });
  if let Some(r) = s.sim_runs.iter_mut().find(|r| r.study == st.id) {
    r.model = model;
    r.def = st;
    r.solution = Some(sol);
  }
  Ok(out)
}

/// Key numbers of a solution.
fn result_values(model: &Model, sol: &Solution) -> Value {
  let nodes = model.nodes();
  let r = |id: &str| results::range(&sol.field(id, 1).unwrap_or_default(), &nodes);
  match sol {
    Solution::Static(st) => json!({
      "max_von_mises": r("von_mises").max, "max_von_mises_at": r("von_mises").max_at,
      "max_displacement": r("displacement").max, "max_displacement_at": r("displacement").max_at,
      "min_safety_factor": r("safety_factor").min, "min_safety_factor_at": r("safety_factor").min_at,
      "reactions": st.reactions, "applied": st.applied,
    }),
    Solution::Modal(m) => json!({
      "frequencies": m.modes.iter().map(|x| x.frequency).collect::<Vec<_>>(),
      "participation": m.modes.iter().map(|x| x.participation).collect::<Vec<_>>(),
      "mass_kg": m.mass,
    }),
    Solution::Thermal(t) => json!({
      "max_temperature": r("temperature").max, "max_temperature_at": r("temperature").max_at,
      "min_temperature": r("temperature").min, "min_temperature_at": r("temperature").min_at,
      "max_heat_flux": r("heat_flux").max, "heat_flow": t.heat_flow,
    }),
  }
}

/// The solved run of a study, current with the design.
pub fn solved(s: &Session, study: u32) -> Option<(&SimRun, &Solution)> {
  let st = s.design.sim.iter().find(|x| x.id == study)?;
  let r = s.sim_runs.iter().find(|r| r.study == study)?;
  if !r.current(s, st) || r.def != *st {
    return None;
  }
  Some((r, r.solution.as_ref()?))
}

fn need_solved<'a>(s: &'a Session, p: &Value) -> Result<(&'a SimRun, &'a Solution), CmdError> {
  let i = index(s, p)?;
  let id = s.design.sim[i].id;
  solved(s, id).ok_or(CmdError::new("the study has no current results: sim.solve first"))
}

fn field_of<'a>(sol: &Solution, p: &'a Value) -> Result<&'a str, CmdError> {
  let f = opt_str(p, "field").unwrap_or(sol.fields()[0].id);
  sol.field_info(f).map(|_| f).ok_or_else(|| {
    CmdError::new(format!("unknown field '{f}' ({})", sol.fields().iter().map(|f| f.id).collect::<Vec<_>>().join(", ")))
  })
}

fn results_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let (run, sol) = need_solved(s, p)?;
  let field = field_of(sol, p)?;
  let mode = p.get("mode").and_then(Value::as_u64).unwrap_or(1) as usize;
  let values = sol.field(field, mode).ok_or(CmdError::new(format!("no mode {mode}")))?;
  let r = results::range(&values, &run.model.nodes());
  let info = sol.field_info(field).unwrap_or(sol.fields()[0]);
  Ok(json!({
    "study": run.study, "field": field, "label": info.label, "unit": info.unit,
    "min": r.min, "max": r.max, "min_at": r.min_at, "max_at": r.max_at,
    "fields": sol.fields().iter().map(|f| f.id).collect::<Vec<_>>(),
    "results": result_values(&run.model, sol),
  }))
}

fn probe(s: &mut Session, p: &Value) -> CmdResult {
  let (run, sol) = need_solved(s, p)?;
  let field = field_of(sol, p)?;
  let mode = p.get("mode").and_then(Value::as_u64).unwrap_or(1) as usize;
  let values = sol.field(field, mode).ok_or(CmdError::new(format!("no mode {mode}")))?;
  let pts: Vec<[f64; 3]> = match p.get("points") {
    Some(Value::Array(a)) => a.iter().map(|v| req_vec3(&json!({"p": v}), "p")).collect::<Result<_, _>>()?,
    _ => vec![req_vec3(p, "point")?],
  };
  let info = sol.field_info(field).unwrap_or(sol.fields()[0]);
  let vals: Vec<Value> = pts
    .iter()
    .map(|q| match fox_sim::probe(&run.model, &values, *q) {
      Some(v) => json!({"point": q, "value": v}),
      None => json!({"point": q, "value": null}),
    })
    .collect();
  Ok(json!({"field": field, "unit": info.unit, "probes": vals}))
}

fn report(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let (run, sol) = need_solved(s, p)?;
  let st = &s.design.sim[i];
  let input = report_input(s, st, run);
  let path = opt_str(p, "path");
  let markdown = opt_str(p, "format").map(|f| f == "markdown" || f == "md").unwrap_or(false)
    || path.is_some_and(|x| x.ends_with(".md"));
  let text = results::report(&input, &run.model, sol, markdown);
  if let Some(path) = path {
    std::fs::write(path, &text).map_err(|e| CmdError::new(format!("can't write {path}: {e}")))?;
    return Ok(json!({"path": path, "bytes": text.len()}));
  }
  Ok(json!({"report": text}))
}

fn faces_text(f: &[fox_model::FaceRef]) -> String {
  format!("{} face{}", f.len(), if f.len() == 1 { "" } else { "s" })
}

fn v3(v: [f64; 3]) -> String {
  format!("({}, {}, {})", fox_expr::trim_number(v[0]), fox_expr::trim_number(v[1]), fox_expr::trim_number(v[2]))
}

pub fn describe_constraint(c: &sim::Constraint) -> String {
  match c {
    sim::Constraint::Fixed { faces, x, y, z } => {
      let axes: String = [(*x, 'X'), (*y, 'Y'), (*z, 'Z')].iter().filter(|a| a.0).map(|a| a.1).collect();
      format!("Fixed ({axes}) on {}", faces_text(faces))
    }
    sim::Constraint::Pin { faces, radial, axial, tangential } => {
      let held: Vec<&str> = [(*radial, "radial"), (*axial, "axial"), (*tangential, "tangential")]
        .iter()
        .filter(|a| a.0)
        .map(|a| a.1)
        .collect();
      format!("Pin ({}) on {}", held.join(", "), faces_text(faces))
    }
    sim::Constraint::Frictionless { faces } => format!("Frictionless on {}", faces_text(faces)),
  }
}

pub fn describe_load(l: &Load) -> String {
  let f = faces_text(l.faces());
  match l {
    Load::Force { force, .. } => format!("Force {} N on {f}", v3(*force)),
    Load::Pressure { pressure, .. } => format!("Pressure {pressure} MPa on {f}"),
    Load::Bearing { force, .. } => format!("Bearing load {} N on {f}", v3(*force)),
    Load::Moment { moment, .. } => format!("Moment {} N·mm on {f}", v3(*moment)),
    Load::Gravity { acceleration } => format!("Gravity {} mm/s²", v3(*acceleration)),
    Load::Temperature { temperature, .. } => format!("Temperature {temperature} °C on {f}"),
    Load::HeatFlux { flux, .. } => format!("Heat flux {flux} W/m² on {f}"),
    Load::HeatPower { power, .. } => format!("Heat source {power} W on {f}"),
    Load::Convection { coefficient, ambient, .. } => {
      format!("Convection {coefficient} W/(m²·K) to {ambient} °C on {f}")
    }
    Load::InternalHeat { body, power } => format!("Internal heat {power} W in body {}", body.0),
  }
}

fn report_input(s: &Session, st: &Study, run: &SimRun) -> results::ReportInput {
  results::ReportInput {
    title: format!("{} — {}", s.design.name, st.name),
    study: st.kind.label().to_string(),
    bodies: run
      .bodies
      .iter()
      .zip(&run.model.parts)
      .map(|(b, p)| (body_name(s, *b), p.material.name.clone(), p.mesh.nodes.len(), p.mesh.tets.len()))
      .collect(),
    mesh: format!(
      "{} nodes, {} quadratic tetrahedra; element size {}",
      run.model.node_count(),
      run.model.element_count(),
      run.model.parts.iter().map(|p| format!("{} mm", fox_expr::trim_number(p.size))).collect::<Vec<_>>().join(", ")
    ),
    constraints: st.constraints.iter().map(describe_constraint).collect(),
    loads: st.loads.iter().map(describe_load).collect(),
    contacts: if run.bodies.len() > 1 {
      vec![match st.contacts {
        ContactMode::Automatic => "Bonded: every pair of touching bodies (automatic)".to_string(),
        ContactMode::Manual => format!(
          "Bonded: {}",
          st.bonded
            .iter()
            .map(|(a, b)| format!("{} – {}", body_name(s, *a), body_name(s, *b)))
            .collect::<Vec<_>>()
            .join(", ")
        ),
      }]
    } else {
      Vec::new()
    },
  }
}

fn study_json(s: &Session, st: &Study) -> Value {
  let bodies = study_bodies(s, st);
  json!({
    "study": st.id, "name": st.name, "type": st.kind.id(), "label": st.kind.label(),
    "bodies": bodies.iter().map(|b| json!({"body": b.0, "name": body_name(s, *b), "material": material_of(s, st, *b).name})).collect::<Vec<_>>(),
    "constraints": st.constraints.iter().map(describe_constraint).collect::<Vec<_>>(),
    "loads": st.loads.iter().map(describe_load).collect::<Vec<_>>(),
    "contacts": if st.contacts == ContactMode::Automatic { "automatic" } else { "manual" },
    "bonded": st.bonded.iter().map(|(a, b)| [a.0, b.0]).collect::<Vec<_>>(),
    "mesh": {"size": st.mesh.size, "refinements": st.mesh.refine.iter().map(|r| json!({"faces": r.faces.len(), "size": r.size})).collect::<Vec<_>>()},
    "modes": st.modes,
    "solved": solved(s, st.id).is_some(),
  })
}

/// The colour range a view shows: (min, max, label, unit).
pub fn view_range(s: &Session, v: &SimView) -> Option<(f64, f64, &'static str, &'static str)> {
  let (run, sol) = solved(s, v.study)?;
  let field = if v.field.is_empty() { sol.fields()[0].id } else { v.field.as_str() };
  let info = sol.field_info(field)?;
  let values = sol.field(field, v.mode)?;
  let r = results::range(&values, &run.model.nodes());
  Some((r.min, r.max, info.label, info.unit))
}

/// The bodies a view draws itself (the scene leaves them out).
pub fn view_bodies(s: &Session, v: &SimView) -> Vec<BodyId> {
  s.sim_runs.iter().find(|r| r.study == v.study).map(|r| r.bodies.clone()).unwrap_or_default()
}

/// Draws a study's results (or its mesh) into the scene; triangles pick as their body's faces.
pub fn draw(scene: &mut fox_render::Scene, picks: &mut crate::scene::PickTable, s: &Session, v: &SimView) {
  let Some(run) = s.sim_runs.iter().find(|r| r.study == v.study) else { return };
  let sol = if v.mesh { None } else { solved(s, v.study).map(|x| x.1) };
  let nodes = run.model.nodes();
  let (verts, tris, faces) = results::display_surface(&run.model);
  let field = sol.map(|sol| if v.field.is_empty() { sol.fields()[0].id } else { v.field.as_str() });
  let values = sol.zip(field).and_then(|(sol, f)| sol.field(f, v.mode));
  let deform = sol.and_then(|sol| sol.deformation(v.mode));
  let (lo, hi) = fox_sim::geom::bbox(&nodes);
  let size = fox_sim::geom::dist(lo, hi);
  let mut factor = match (deform, v.scale) {
    (Some(d), None) => {
      let big = d.iter().map(|x| fox_sim::geom::norm(*x)).fold(0.0, f64::max);
      if big > 0.0 { 0.05 * size / big } else { 0.0 }
    }
    (Some(_), Some(k)) => k,
    _ => 0.0,
  };
  if let Some(ph) = v.phase {
    factor *= ph.sin();
  }
  let positions: Vec<[f32; 3]> = verts
    .iter()
    .map(|&g| {
      let p = nodes[g as usize];
      let d = deform.map(|d| d[g as usize]).unwrap_or([0.0; 3]);
      [(p[0] + factor * d[0]) as f32, (p[1] + factor * d[1]) as f32, (p[2] + factor * d[2]) as f32]
    })
    .collect();
  let mut normals = vec![[0.0f32; 3]; positions.len()];
  for t in &tris {
    let [a, b, c] = t.map(|k| fox_kernel::glam::Vec3::from(positions[k as usize]));
    let n = (b - a).cross(c - a);
    for k in t {
      let m = &mut normals[*k as usize];
      *m = (fox_kernel::glam::Vec3::from(*m) + n).to_array();
    }
  }
  for n in &mut normals {
    *n = fox_kernel::glam::Vec3::from(*n).normalize_or_zero().to_array();
  }
  let colors = match &values {
    Some(vals) => {
      let r = results::range(vals, &nodes);
      let span = (r.max - r.min).max(1e-300);
      verts.iter().map(|&g| results::colormap((vals[g as usize] - r.min) / span)).collect()
    }
    None => Vec::new(),
  };
  let tri_ids =
    faces.iter().map(|(part, f)| picks.id(crate::scene::Pick::Face { body: run.bodies[*part], face: *f })).collect();
  scene.meshes.push(fox_render::Mesh {
    positions,
    normals,
    indices: tris.iter().flatten().copied().collect(),
    tri_ids,
    id: 0,
    color: [205, 210, 218, 255],
    highlight: None,
    highlight_color: [0, 0, 0, 0],
    colors,
    analyze: false,
    unlit: false,
  });
  if v.mesh || values.is_none() {
    // element edges of the boundary
    for part in &run.model.parts {
      for b in &part.mesh.boundary {
        let p = |i: usize| part.mesh.nodes[b[i] as usize].map(|x| x as f32);
        scene.lines.push(fox_render::Polyline {
          points: vec![p(0), p(3), p(1), p(4), p(2), p(5), p(0)],
          color: [40, 60, 90, 255],
          width: 1.0,
          id: 0,
          on_top: false,
          dashed: false,
          dash_dot: false,
          behind: false,
        });
      }
    }
  }
}

/// A view from JSON (`view.render`'s `sim`): `{study?, field?, mode?, scale?: number | "adjusted" |
/// "true", mesh?: bool}` or just a study id.
pub fn view_of(s: &Session, v: &Value) -> Result<SimView, String> {
  let p = if v.is_object() { v.clone() } else { json!({"study": v}) };
  let i = index(s, &p).map_err(|e| e.0)?;
  let study = s.design.sim[i].id;
  let scale = match p.get("scale") {
    None | Some(Value::Null) => None,
    Some(Value::String(t)) if t == "adjusted" => None,
    Some(Value::String(t)) if t == "true" || t == "actual" => Some(1.0),
    Some(Value::String(t)) if t == "none" || t == "undeformed" => Some(0.0),
    Some(x) => Some(x.as_f64().ok_or("'scale' is a number, adjusted, true or none")?),
  };
  let field = opt_str(&p, "field").unwrap_or("").to_string();
  if !field.is_empty()
    && let Some((_, sol)) = solved(s, study)
    && sol.field_info(&field).is_none()
  {
    return Err(format!("unknown field '{field}'"));
  }
  Ok(SimView {
    study,
    field,
    mode: p.get("mode").and_then(Value::as_u64).unwrap_or(1).max(1) as usize,
    scale,
    phase: p.get("phase").and_then(Value::as_f64),
    mesh: flag(&p, "mesh", false),
  })
}
