//! The Render workspace's commands (`render.*`): physically based materials on bodies and faces,
//! the environment, the render camera and settings, final renders by the `fox-raytrace` path
//! tracer, and the render gallery kept in the design.
//!
//! Render settings are view data, like named views and appearances: changing them makes an undo
//! step but regenerates nothing.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use fox_model::render::{BodyMaterial, BodyPriority, FaceMaterial, RenderDoc};
use fox_model::{BodyId, Design};
use fox_raytrace::{
  Axis, Background, Environment, GalleryImage, Light, Material, Pattern, Png, Preset, Progressive, Projection,
  RenderCamera, RenderImage, RenderMesh, RenderScene, RenderSettings, TextureSource, TextureSpec, ToneMap,
};
use fox_render::{Camera, StandardView};
use serde_json::{Value, json};

use crate::commands::{face_ref, ids, opt_str, parse_color, req_str, req_u32};
use crate::{CmdError, CmdResult, Session};

pub use fox_raytrace as raytrace;

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "render.materials" => Ok(materials(s, p)),
    "render.material" => material_get(s, p),
    "render.material.define" => material_define(s, p),
    "render.material.delete" => material_delete(s, p),
    "render.material.apply" => material_apply(s, p),
    "render.assignments" => Ok(assignments(s)),
    "render.environment" => environment(s, p),
    "render.camera" => camera(s, p),
    "render.settings" => settings(s, p),
    "render.image" => image(s, p),
    "render.gallery" => Ok(json!(s.design.render.gallery.iter().map(gallery_json).collect::<Vec<_>>())),
    "render.gallery.add" => gallery_add(s, p),
    "render.gallery.export" => gallery_export(s, p),
    "render.gallery.rename" => gallery_rename(s, p),
    "render.gallery.delete" => gallery_delete(s, p),
    "render.lights" => Ok(json!(s.design.render.lights)),
    "render.light.add" => light_add(s, p),
    "render.light.edit" => light_edit(s, p),
    "render.light.delete" => light_delete(s, p),
    "render.priority" => priority(s, p),
    "render.priorities" => Ok(priorities_json(s)),
    "render.texture_map" => texture_map(s, p),
    _ => return None,
  })
}

/// Changes the design's render data as one undo step, without regenerating.
fn edit<T>(s: &mut Session, label: &str, f: impl FnOnce(&mut RenderDoc) -> Result<T, CmdError>) -> Result<T, CmdError> {
  if s.cmd_label.is_none() {
    s.cmd_label = Some(label.to_string());
  }
  s.edit_deferred(|d| f(&mut d.render))
}

// ---------------------------------------------------------------- materials

fn material_json(m: &Material, own: bool) -> Value {
  let mut v = serde_json::to_value(m).unwrap_or(Value::Null);
  if let Some(o) = v.as_object_mut() {
    o.insert("design".into(), json!(own));
  }
  v
}

/// The library's materials, then the design's own (`{category?, query?}` filters them).
fn materials(s: &Session, p: &Value) -> Value {
  let cat = opt_str(p, "category");
  let q = opt_str(p, "query").map(str::to_ascii_lowercase);
  let keep = |m: &Material| {
    cat.is_none_or(|c| m.category.eq_ignore_ascii_case(c))
      && q.as_ref().is_none_or(|q| m.name.to_ascii_lowercase().contains(q.as_str()))
  };
  let mut out: Vec<Value> = fox_raytrace::material::library()
    .iter()
    .filter(|m| keep(m) && !s.design.render.materials.iter().any(|d| d.name.eq_ignore_ascii_case(&m.name)))
    .map(|m| material_json(m, false))
    .collect();
  out.extend(s.design.render.materials.iter().filter(|m| keep(m)).map(|m| material_json(m, true)));
  json!(out)
}

fn material_get(s: &Session, p: &Value) -> CmdResult {
  let name = req_str(p, "name")?;
  let m = s.design.render.material(name).ok_or_else(|| unknown_material(name))?;
  let own = s.design.render.materials.iter().any(|d| d.name.eq_ignore_ascii_case(name));
  Ok(material_json(&m, own))
}

fn unknown_material(name: &str) -> CmdError {
  CmdError::new(format!("unknown material '{name}' (see 'render.materials')"))
}

fn num(p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => match v.as_f64() {
      Some(x) if x.is_finite() => Ok(Some(x)),
      _ => Err(CmdError::new(format!("'{k}' must be a number"))),
    },
  }
}

/// A number in `range` (inclusive).
fn unit_num(p: &Value, k: &str, lo: f64, hi: f64) -> Result<Option<f32>, CmdError> {
  match num(p, k)? {
    Some(x) if !(lo..=hi).contains(&x) => Err(CmdError::new(format!("'{k}' must be between {lo} and {hi}"))),
    x => Ok(x.map(|x| x as f32)),
  }
}

fn flag(p: &Value, k: &str) -> Result<Option<bool>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Bool(b)) => Ok(Some(*b)),
    Some(_) => Err(CmdError::new(format!("'{k}' must be true or false"))),
  }
}

/// Applies the material fields present in `p` to `m`.
fn set_material_fields(m: &mut Material, p: &Value) -> Result<(), CmdError> {
  if let Some(c) = p.get("color").filter(|c| !c.is_null()) {
    m.color = parse_color(c)?;
  }
  if let Some(c) = opt_str(p, "category") {
    m.category = c.to_string();
  }
  for (k, slot, hi) in [
    ("metallic", &mut m.metallic, 1.0),
    ("roughness", &mut m.roughness, 1.0),
    ("clearcoat", &mut m.clearcoat, 1.0),
    ("clearcoat_roughness", &mut m.clearcoat_roughness, 1.0),
    ("transmission", &mut m.transmission, 1.0),
  ] {
    if let Some(v) = unit_num(p, k, 0.0, hi)? {
      *slot = v;
    }
  }
  if let Some(v) = unit_num(p, "ior", 1.0, 3.0)? {
    m.ior = v;
  }
  if let Some(c) = p.get("emission").filter(|c| !c.is_null()) {
    m.emission = parse_color(c)?;
    if m.emission_strength == 0.0 {
      m.emission_strength = 1.0;
    }
  }
  if let Some(v) = unit_num(p, "emission_strength", 0.0, 1000.0)? {
    m.emission_strength = v;
  }
  match p.get("texture") {
    None => {}
    Some(Value::Null) => m.texture = None,
    Some(t) => m.texture = Some(texture_spec(m.texture.clone().unwrap_or_default(), t)?),
  }
  Ok(())
}

/// `{image: path | pattern: name, color2?, projection?, axis?, scale?, rotation?, offset?,
/// roughness_variation?}` over an existing texture.
fn texture_spec(mut t: TextureSpec, p: &Value) -> Result<TextureSpec, CmdError> {
  if let Some(path) = opt_str(p, "image") {
    let abs = std::path::absolute(path).map(|a| a.display().to_string()).unwrap_or_else(|_| path.to_string());
    if !Path::new(&abs).exists() {
      return Err(CmdError::new(format!("no image at {abs}")));
    }
    t.source = TextureSource::Image(abs);
  } else if let Some(name) = opt_str(p, "pattern") {
    let pat = Pattern::parse(name).ok_or_else(|| {
      let all: Vec<&str> = Pattern::ALL.iter().map(|p| p.name()).collect();
      CmdError::new(format!("unknown pattern '{name}' ({})", all.join(", ")))
    })?;
    t.source = TextureSource::Pattern(pat);
  }
  if let Some(c) = p.get("color2").filter(|c| !c.is_null()) {
    t.color2 = parse_color(c)?;
  }
  if let Some(pr) = opt_str(p, "projection") {
    t.projection = Projection::parse(pr).ok_or_else(|| {
      CmdError::new(format!("unknown projection '{pr}' (automatic, box, planar, cylindrical, spherical)"))
    })?;
  }
  if let Some(a) = opt_str(p, "axis") {
    t.axis = Axis::parse(a).ok_or_else(|| CmdError::new(format!("unknown axis '{a}' (x, y, z)")))?;
  }
  if let Some(v) = num(p, "scale")? {
    if v <= 0.0 {
      return Err(CmdError::new("'scale' must be positive"));
    }
    t.scale = v as f32;
  }
  if let Some(v) = num(p, "rotation")? {
    t.rotation = v as f32;
  }
  if let Some(o) = p.get("offset").and_then(Value::as_array) {
    match o.as_slice() {
      [u, v] => t.offset = [u.as_f64().unwrap_or(0.0) as f32, v.as_f64().unwrap_or(0.0) as f32],
      _ => return Err(CmdError::new("'offset' must be [u, v]")),
    }
  }
  if let Some(v) = unit_num(p, "roughness_variation", -1.0, 1.0)? {
    t.roughness_variation = v;
  }
  Ok(t)
}

/// `{name, from?, color?, metallic?, roughness?, ...}`: makes (or changes) one of the design's
/// materials, starting from a library material or what it was.
fn material_define(s: &mut Session, p: &Value) -> CmdResult {
  let name = req_str(p, "name")?.trim().to_string();
  if name.is_empty() {
    return Err(CmdError::new("a material needs a name"));
  }
  let own = s.design.render.materials.iter().position(|m| m.name.eq_ignore_ascii_case(&name));
  let mut m = match (opt_str(p, "from"), own) {
    (Some(from), _) => s.design.render.material(from).ok_or_else(|| unknown_material(from))?,
    (None, Some(i)) => s.design.render.materials[i].clone(),
    (None, None) => fox_raytrace::material::find(&name).unwrap_or_default(),
  };
  if !p.get("category").is_some_and(Value::is_string) && (own.is_none() || p.get("from").is_some()) {
    m.category = "Design".into();
  }
  m.name = name.clone();
  set_material_fields(&mut m, p)?;
  let out = material_json(&m, true);
  edit(s, "Edit Material", |r| {
    match own {
      Some(i) => r.materials[i] = m,
      None => r.materials.push(m),
    }
    Ok(())
  })?;
  Ok(out)
}

fn material_delete(s: &mut Session, p: &Value) -> CmdResult {
  let name = req_str(p, "name")?;
  let i = s
    .design
    .render
    .materials
    .iter()
    .position(|m| m.name.eq_ignore_ascii_case(name))
    .ok_or_else(|| CmdError::new(format!("the design has no material '{name}' of its own")))?;
  let n = s.design.render.materials[i].name.clone();
  edit(s, "Delete Material", |r| {
    r.materials.remove(i);
    // bodies and faces using it go back to their appearance (or the library material of that name)
    if fox_raytrace::material::find(&n).is_none() {
      r.bodies.retain(|b| !b.material.eq_ignore_ascii_case(&n));
      r.faces.retain(|f| !f.material.eq_ignore_ascii_case(&n));
    }
    Ok(())
  })?;
  Ok(json!({"deleted": n}))
}

/// `{material?, body? | bodies? | component? | faces?}`: puts a material on bodies or faces;
/// without one, takes theirs off (they render in their appearance again).
fn material_apply(s: &mut Session, p: &Value) -> CmdResult {
  let mat = match opt_str(p, "material") {
    Some(n) => Some(s.design.render.material(n).ok_or_else(|| unknown_material(n))?.name),
    None => None,
  };
  if let Some(list) = p.get("faces").and_then(Value::as_array) {
    if list.is_empty() {
      return Err(CmdError::new("no faces given"));
    }
    let mut refs = Vec::new();
    let mut picked = Vec::new();
    for f in list {
      let r = face_ref(s, f)?;
      let b = s.model.body(r.body).ok_or(CmdError::new("no such body"))?;
      let id = r.resolve(b.faces(), b.mesh(), b.diagonal()).ok_or(CmdError::new("the face can't be found"))?;
      picked.push((r.body, id));
      refs.push(r);
    }
    // what those faces had goes
    let old: Vec<usize> =
      face_materials(s).into_iter().filter(|(b, f, _, _)| picked.contains(&(*b, *f))).map(|x| x.3).collect();
    edit(s, "Render Material", |r| {
      let mut i = 0;
      r.faces.retain(|_| {
        i += 1;
        !old.contains(&(i - 1))
      });
      if let Some(m) = &mat {
        r.faces.extend(refs.into_iter().map(|face| FaceMaterial { face, material: m.clone() }));
      }
      Ok(())
    })?;
    return Ok(json!({
      "faces": picked.iter().map(|(b, f)| json!({"body": b.0, "face": f})).collect::<Vec<_>>(),
      "material": mat,
    }));
  }
  let mut bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if let Some(b) = p.get("body").and_then(Value::as_u64) {
    bodies.push(BodyId(b as u32));
  }
  if let Some(c) = p.get("component").and_then(Value::as_u64) {
    let comp =
      s.design.components.iter().find(|x| x.id.0 as u64 == c).ok_or(CmdError::new(format!("no component {c}")))?;
    bodies.extend(comp.bodies.iter().copied());
  }
  if bodies.is_empty() {
    return Err(CmdError::new("pass 'body', 'bodies', 'component' or 'faces'"));
  }
  for b in &bodies {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  edit(s, "Render Material", |r| {
    r.bodies.retain(|x| !bodies.contains(&x.body));
    if let Some(m) = &mat {
      r.bodies.extend(bodies.iter().map(|b| BodyMaterial { body: *b, material: m.clone() }));
    }
    Ok(())
  })?;
  Ok(json!({"bodies": bodies.iter().map(|b| b.0).collect::<Vec<_>>(), "material": mat}))
}

/// Each face material as it resolves now: (body, face id, material name, index in the list).
fn face_materials(s: &Session) -> Vec<(BodyId, u32, String, usize)> {
  let mut out = Vec::new();
  for (i, fm) in s.design.render.faces.iter().enumerate() {
    let Some(b) = s.model.body(fm.face.body) else { continue };
    for f in fm.face.resolve_all(b.faces(), b.mesh(), b.diagonal()) {
      out.push((fm.face.body, f, fm.material.clone(), i));
    }
  }
  out
}

/// What every body (and face with its own material) renders as.
fn assignments(s: &Session) -> Value {
  let faces = face_materials(s);
  let bodies: Vec<Value> = s
    .model
    .bodies
    .iter()
    .map(|b| {
      let m = body_material(s, b.id);
      json!({
        "body": b.id.0,
        "name": s.body_name(b.id),
        "material": m.name,
        "assigned": s.design.render.body_material(b.id).is_some(),
        "faces": faces.iter().filter(|f| f.0 == b.id).map(|f| json!({"face": f.1, "material": f.2})).collect::<Vec<_>>(),
      })
    })
    .collect();
  json!({"bodies": bodies})
}

/// A body's material: its render material, else what its appearance colour renders as.
pub fn body_material(s: &Session, body: BodyId) -> Material {
  let d = &s.design;
  if let Some(m) = d.render.body_material(body).and_then(|n| d.render.material(n)) {
    return m;
  }
  match d.body_color(body) {
    Some(c) => fox_raytrace::material::for_color(c),
    None => fox_raytrace::material::default_material(),
  }
}

// ---------------------------------------------------------------- environment, camera, settings

fn env_json(e: &Environment) -> Value {
  let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
  if let Some(o) = v.as_object_mut() {
    o.insert(
      "presets".into(),
      json!(Preset::ALL.iter().map(|p| json!({"name": p.name(), "label": p.label()})).collect::<Vec<_>>()),
    );
  }
  v
}

/// `{preset?, hdri?, intensity?, rotation?, background?, background_color?, background_color2?,
/// ground?, shadows?, ground_reflection?, ground_offset?}`; returns the environment.
fn environment(s: &mut Session, p: &Value) -> CmdResult {
  let mut e = s.design.render.environment.clone();
  if let Some(name) = opt_str(p, "preset") {
    e.preset = Preset::parse(name).ok_or_else(|| {
      let all: Vec<&str> = Preset::ALL.iter().map(|p| p.name()).collect();
      CmdError::new(format!("unknown environment '{name}' ({})", all.join(", ")))
    })?;
    if p.get("hdri").is_none() {
      e.hdri = None;
    }
  }
  match p.get("hdri") {
    None => {}
    Some(Value::Null) => e.hdri = None,
    Some(v) => {
      let path = v.as_str().ok_or(CmdError::new("'hdri' must be a path"))?;
      let abs = std::path::absolute(path).map(|a| a.display().to_string()).unwrap_or_else(|_| path.to_string());
      // read it now, so a bad file is reported here and not at render time
      fox_raytrace::EnvMap::build(&Environment { hdri: Some(abs.clone()), ..Environment::default() })
        .map_err(CmdError::new)?;
      e.hdri = Some(abs);
    }
  }
  if let Some(v) = unit_num(p, "intensity", 0.0, 100.0)? {
    e.intensity = v;
  }
  if let Some(v) = num(p, "rotation")? {
    e.rotation = v.rem_euclid(360.0) as f32;
  }
  if let Some(b) = opt_str(p, "background") {
    e.background = Background::parse(b)
      .ok_or_else(|| CmdError::new(format!("unknown background '{b}' (environment, color, gradient, transparent)")))?;
  }
  if let Some(c) = p.get("background_color").filter(|c| !c.is_null()) {
    e.background_color = parse_color(c)?;
    if p.get("background").is_none() && e.background == Background::Environment {
      e.background = Background::Color;
    }
  }
  if let Some(c) = p.get("background_color2").filter(|c| !c.is_null()) {
    e.background_color2 = parse_color(c)?;
  }
  if let Some(v) = flag(p, "ground")? {
    e.ground = v;
  }
  if let Some(v) = flag(p, "shadows")? {
    e.shadows = v;
  }
  if let Some(v) = unit_num(p, "ground_reflection", 0.0, 1.0)? {
    e.ground_reflection = v;
  }
  if let Some(v) = unit_num(p, "ground_offset", 0.0, 1e6)? {
    e.ground_offset = v;
  }
  if let Some(v) = flag(p, "flatten_ground")? {
    e.flatten_ground = v;
  }
  if e != s.design.render.environment {
    let e2 = e.clone();
    edit(s, "Environment", |r| {
      r.environment = e2;
      Ok(())
    })?;
  }
  Ok(env_json(&e))
}

fn camera_json(c: &RenderCamera, stored: bool) -> Value {
  json!({
    "stored": stored,
    "target": c.view.target, "yaw": c.view.yaw.to_degrees(), "pitch": c.view.pitch.to_degrees(),
    "distance": c.view.distance, "fov": c.view.fov.to_degrees(), "ortho": c.view.ortho,
    "roll": c.view.roll.to_degrees(),
    "fstop": c.fstop, "focus_distance": c.focus_distance, "exposure": c.exposure, "tone_map": c.tone_map,
  })
}

/// The default framing: the home view fitted to the model.
pub fn fitted_camera(s: &Session, aspect: f64) -> Camera {
  let mut cam = Camera::default();
  cam.set_view(StandardView::Home);
  let (lo, hi) =
    s.model.bounds().unwrap_or((fox_kernel::glam::DVec3::splat(-50.0), fox_kernel::glam::DVec3::splat(50.0)));
  cam.fit(lo, hi, aspect);
  cam
}

/// The camera renders use: the stored one, else the home view fitted to the model.
pub fn render_camera(s: &Session) -> RenderCamera {
  let st = &s.design.render.settings;
  s.design.render.camera.unwrap_or_else(|| RenderCamera {
    view: fitted_camera(s, st.width as f64 / st.height.max(1) as f64),
    ..RenderCamera::default()
  })
}

/// Changes `cam` by what `p` gives (angles in degrees).
fn set_camera_fields(s: &Session, cam: &mut RenderCamera, p: &Value) -> Result<(), CmdError> {
  if let Some(v) = opt_str(p, "view") {
    if let Some(sv) = StandardView::parse(v) {
      let st = &s.design.render.settings;
      let mut c = fitted_camera(s, st.width as f64 / st.height.max(1) as f64);
      c.set_view(sv);
      let (lo, hi) =
        s.model.bounds().unwrap_or((fox_kernel::glam::DVec3::splat(-50.0), fox_kernel::glam::DVec3::splat(50.0)));
      c.fit(lo, hi, st.width as f64 / st.height.max(1) as f64);
      c.ortho = cam.view.ortho;
      cam.view = c;
    } else {
      let nv = s
        .design
        .named_views
        .iter()
        .find(|n| n.name.eq_ignore_ascii_case(v))
        .ok_or_else(|| CmdError::new(format!("no view '{v}' (a named view, or home, top, front, ...)")))?;
      cam.view = Camera {
        target: nv.target,
        yaw: nv.yaw,
        pitch: nv.pitch,
        distance: nv.distance,
        fov: nv.fov,
        ortho: nv.ortho,
        roll: 0.0,
      };
    }
  }
  if p.get("target").is_some() {
    cam.view.target = crate::commands::req_vec3(p, "target")?;
  }
  if let Some(v) = num(p, "yaw")? {
    cam.view.yaw = v.to_radians().rem_euclid(std::f64::consts::TAU);
  }
  if let Some(v) = num(p, "pitch")? {
    cam.view.pitch = v.clamp(-89.9, 89.9).to_radians();
  }
  if let Some(v) = num(p, "roll")? {
    cam.view.roll = v.to_radians();
  }
  if let Some(v) = num(p, "distance")? {
    if v <= 0.0 {
      return Err(CmdError::new("'distance' must be positive"));
    }
    cam.view.distance = v;
  }
  if let Some(v) = num(p, "fov")? {
    if !(1.0..=150.0).contains(&v) {
      return Err(CmdError::new("'fov' is in degrees, between 1 and 150"));
    }
    cam.view.fov = v.to_radians();
  }
  if let Some(v) = num(p, "focal_length")? {
    if v <= 0.0 {
      return Err(CmdError::new("'focal_length' must be positive"));
    }
    cam.view.fov = 2.0 * (12.0 / v).atan();
  }
  if let Some(v) = flag(p, "ortho")? {
    cam.view.ortho = v;
  }
  if let Some(v) = num(p, "fstop")? {
    if v < 0.0 {
      return Err(CmdError::new("'fstop' must be 0 (no depth of field) or more"));
    }
    cam.fstop = v;
  }
  match p.get("focus_distance") {
    None => {}
    Some(Value::Null) => cam.focus_distance = None,
    Some(_) => {
      let v = num(p, "focus_distance")?.unwrap_or(0.0);
      if v <= 0.0 {
        return Err(CmdError::new("'focus_distance' must be positive"));
      }
      cam.focus_distance = Some(v);
    }
  }
  if let Some(v) = unit_num(p, "exposure", -10.0, 10.0)? {
    cam.exposure = v;
  }
  if let Some(t) = opt_str(p, "tone_map") {
    cam.tone_map =
      ToneMap::parse(t).ok_or_else(|| CmdError::new(format!("unknown tone map '{t}' (aces, reinhard, linear)")))?;
  }
  Ok(())
}

/// `{view?, target?, yaw?, pitch?, roll?, distance?, fov? | focal_length?, ortho?, fstop?,
/// focus_distance?, exposure?, tone_map?, reset?}`; returns the camera.
fn camera(s: &mut Session, p: &Value) -> CmdResult {
  if p.get("reset").and_then(Value::as_bool) == Some(true) {
    if s.design.render.camera.is_some() {
      edit(s, "Render Camera", |r| {
        r.camera = None;
        Ok(())
      })?;
    }
    return Ok(camera_json(&render_camera(s), false));
  }
  let changes = p.as_object().is_some_and(|o| !o.is_empty());
  let mut cam = render_camera(s);
  set_camera_fields(s, &mut cam, p)?;
  if changes && s.design.render.camera != Some(cam) {
    edit(s, "Render Camera", |r| {
      r.camera = Some(cam);
      Ok(())
    })?;
  }
  Ok(camera_json(&cam, s.design.render.camera.is_some()))
}

fn size_of(p: &Value, k: &str) -> Result<Option<u32>, CmdError> {
  match num(p, k)? {
    Some(v) if !(1.0..=fox_raytrace::tracer::MAX_SIZE as f64).contains(&v) => {
      Err(CmdError::new(format!("'{k}' must be between 1 and {}", fox_raytrace::tracer::MAX_SIZE)))
    }
    v => Ok(v.map(|v| v as u32)),
  }
}

/// Reads `width`, `height`, `samples`, `max_bounces`, `denoise`, `seed`, `threads` (and a
/// `resolution` name) over `st`.
fn set_settings_fields(st: &mut RenderSettings, p: &Value) -> Result<(), CmdError> {
  if let Some(r) = opt_str(p, "resolution") {
    (st.width, st.height) = match r.to_ascii_lowercase().as_str() {
      "720p" | "hd" => (1280, 720),
      "1080p" | "full_hd" | "fhd" => (1920, 1080),
      "4k" | "uhd" => (3840, 2160),
      "square" => (1024, 1024),
      "thumbnail" => (400, 300),
      other => match other.split_once('x') {
        Some((w, h)) => (
          w.trim().parse().map_err(|_| CmdError::new(format!("bad resolution '{r}'")))?,
          h.trim().parse().map_err(|_| CmdError::new(format!("bad resolution '{r}'")))?,
        ),
        None => return Err(CmdError::new(format!("unknown resolution '{r}' (720p, 1080p, 4k, square, WxH)"))),
      },
    };
    if st.width == 0 || st.height == 0 || st.width.max(st.height) > fox_raytrace::tracer::MAX_SIZE {
      return Err(CmdError::new(format!("bad resolution '{r}'")));
    }
  }
  if let Some(v) = size_of(p, "width")? {
    st.width = v;
  }
  if let Some(v) = size_of(p, "height")? {
    st.height = v;
  }
  match num(p, "samples")? {
    Some(v) if !(1.0..=1_000_000.0).contains(&v) => return Err(CmdError::new("'samples' must be 1 or more")),
    Some(v) => st.samples = v as u32,
    None => {}
  }
  match num(p, "max_bounces")? {
    Some(v) if !(0.0..=64.0).contains(&v) => return Err(CmdError::new("'max_bounces' must be between 0 and 64")),
    Some(v) => st.max_bounces = v as u32,
    None => {}
  }
  if let Some(v) = flag(p, "denoise")? {
    st.denoise = v;
  }
  if let Some(v) = num(p, "seed")? {
    st.seed = v as u32;
  }
  if let Some(v) = num(p, "threads")? {
    st.threads = v.max(0.0) as usize;
  }
  Ok(())
}

/// `{resolution? | width?, height?, samples?, max_bounces?, denoise?, seed?}`; returns them.
fn settings(s: &mut Session, p: &Value) -> CmdResult {
  let mut st = s.design.render.settings;
  set_settings_fields(&mut st, p)?;
  st.threads = 0;
  if st != s.design.render.settings {
    edit(s, "Render Settings", |r| {
      r.settings = st;
      Ok(())
    })?;
  }
  Ok(json!(st))
}

// ---------------------------------------------------------------- the scene

/// What the bodies render as: one mesh per body and material, faces with their own material
/// split out. `only` limits it to some bodies.
pub fn render_scene(s: &Session, camera: RenderCamera, only: Option<&[BodyId]>) -> RenderScene {
  let mut scene = RenderScene {
    environment: s.design.render.environment.clone(),
    camera,
    lights: s.design.render.lights.clone(),
    ..RenderScene::default()
  };
  let face_mats = face_materials(s);
  for body in &s.model.bodies {
    let shown = match only {
      Some(o) => o.contains(&body.id),
      None => s.body_visible(body.id),
    };
    if !shown {
      continue;
    }
    let Some(m) = body.mesh() else { continue };
    let base = body_material(s, body.id);
    let face_cols = crate::views::body_face_colors(s, body.id);
    let mut per_face: HashMap<u32, Material> = HashMap::new();
    for (f, c) in &face_cols {
      per_face.insert(*f, fox_raytrace::material::for_color(*c));
    }
    for (b, f, name, _) in &face_mats {
      if *b == body.id
        && let Some(mat) = s.design.render.material(name)
      {
        per_face.insert(*f, mat);
      }
    }
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &m.positions {
      for k in 0..3 {
        lo[k] = lo[k].min(p[k]);
        hi[k] = hi[k].max(p[k]);
      }
    }
    let origin = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
    // triangles grouped by material
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for t in 0..m.indices.len() / 3 {
      let mat = m.tri_face.get(t).and_then(|f| per_face.get(f)).unwrap_or(&base);
      let mi = match scene.materials.iter().position(|x| x == mat) {
        Some(i) => i,
        None => {
          scene.materials.push(mat.clone());
          scene.materials.len() - 1
        }
      };
      match groups.iter_mut().find(|g| g.0 == mi) {
        Some(g) => g.1.push(t),
        None => groups.push((mi, vec![t])),
      }
    }
    for (mi, tris) in groups {
      let mut remap: HashMap<u32, u32> = HashMap::new();
      let priority = s.design.render.priority(body.id);
      let mut rm = RenderMesh { material: mi, origin, priority, ..RenderMesh::default() };
      for t in tris {
        for k in 0..3 {
          let v = m.indices[t * 3 + k];
          let n = *remap.entry(v).or_insert_with(|| {
            rm.positions.push(m.positions[v as usize]);
            rm.normals.push(m.normals.get(v as usize).copied().unwrap_or([0.0, 0.0, 1.0]));
            rm.positions.len() as u32 - 1
          });
          rm.indices.push(n);
        }
      }
      scene.meshes.push(rm);
    }
  }
  scene
}

/// The material of every face of the shown bodies, for the GPU viewport: (body, face) → material,
/// and each body's own.
pub fn viewport_materials(s: &Session) -> (HashMap<BodyId, Material>, HashMap<(BodyId, u32), Material>) {
  let mut bodies = HashMap::new();
  let mut faces = HashMap::new();
  for (b, f, name, _) in face_materials(s) {
    if let Some(m) = s.design.render.material(&name) {
      faces.insert((b, f), m);
    }
  }
  for body in &s.model.bodies {
    bodies.insert(body.id, body_material(s, body.id));
    for (f, c) in crate::views::body_face_colors(s, body.id) {
      faces.entry((body.id, f)).or_insert_with(|| fox_raytrace::material::for_color(c));
    }
  }
  (bodies, faces)
}

// ---------------------------------------------------------------- rendering

fn now() -> u64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Renders with the design's camera and settings (`p` overrides them for this render).
pub fn render_with(s: &Session, p: &Value) -> Result<(RenderImage, RenderSettings, f64, Vec<String>), CmdError> {
  let mut st = s.design.render.settings;
  set_settings_fields(&mut st, p)?;
  let mut cam = render_camera(s);
  if s.design.render.camera.is_none() {
    cam.view = fitted_camera(s, st.width as f64 / st.height.max(1) as f64);
  }
  set_camera_fields(s, &mut cam, p)?;
  let only: Option<Vec<BodyId>> = p.get("bodies").map(|_| ids(p, "bodies").into_iter().map(BodyId).collect());
  let scene = render_scene(s, cam, only.as_deref());
  let t0 = std::time::Instant::now();
  let mut prog = Progressive::new(&scene, st).map_err(CmdError::new)?;
  let warnings = prog.prepared.warnings.clone();
  prog.run(&AtomicBool::new(false), |_, _| {});
  let img = prog.image(st.denoise);
  Ok((img, st, t0.elapsed().as_secs_f64(), warnings))
}

/// Adds a rendered image to the gallery; returns its id.
pub fn add_to_gallery(
  s: &mut Session,
  name: Option<&str>,
  png: Vec<u8>,
  size: (u32, u32),
  samples: u32,
  seconds: f64,
) -> Result<u32, CmdError> {
  let id = s.design.render.next_gallery_id();
  let name = name.map(str::to_string).unwrap_or_else(|| format!("Render {id}"));
  let img =
    GalleryImage { id, name, width: size.0, height: size.1, samples, seconds, created: now(), png: Png(Arc::new(png)) };
  edit(s, "Render", |r| {
    r.gallery.push(img);
    Ok(())
  })?;
  Ok(id)
}

/// `{path?, resolution? | width?, height?, samples?, denoise?, seed?, view?, camera fields...,
/// bodies?, name?, gallery?: bool}`: a final render. Writes a PNG when given a path and keeps it
/// in the gallery unless `gallery` is false.
fn image(s: &mut Session, p: &Value) -> CmdResult {
  let (img, st, seconds, warnings) = render_with(s, p)?;
  let png = img.to_png().map_err(CmdError::new)?;
  let path = opt_str(p, "path");
  if let Some(path) = path {
    std::fs::write(path, &png)?;
  }
  let keep = flag(p, "gallery")?.unwrap_or(true);
  let id = if keep {
    Some(add_to_gallery(s, opt_str(p, "name"), png, (img.width, img.height), img.samples, seconds)?)
  } else {
    None
  };
  Ok(json!({
    "path": path, "gallery": id, "width": img.width, "height": img.height, "samples": img.samples,
    "denoised": st.denoise, "seconds": (seconds * 1000.0).round() / 1000.0, "warnings": warnings,
  }))
}

fn gallery_json(g: &GalleryImage) -> Value {
  json!({
    "id": g.id, "name": g.name, "width": g.width, "height": g.height, "samples": g.samples,
    "seconds": g.seconds, "created": g.created, "bytes": g.png.0.len(),
  })
}

fn gallery_index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  let id = req_u32(p, "id")?;
  s.design
    .render
    .gallery
    .iter()
    .position(|g| g.id == id)
    .ok_or_else(|| CmdError::new(format!("no render {id} in the gallery")))
}

/// `{path, name?}`: keeps an image file (a PNG made elsewhere, or by the Render window) in the
/// gallery.
fn gallery_add(s: &mut Session, p: &Value) -> CmdResult {
  let path = req_str(p, "path")?;
  let bytes = std::fs::read(path).map_err(|e| CmdError::new(format!("{path}: {e}")))?;
  let img = image::load_from_memory(&bytes).map_err(|e| CmdError::new(format!("{path}: {e}")))?;
  // always kept as PNG
  let png = if bytes.starts_with(b"\x89PNG") {
    bytes
  } else {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| CmdError::new(e.to_string()))?;
    out.into_inner()
  };
  let samples = p.get("samples").and_then(Value::as_u64).unwrap_or(0) as u32;
  let seconds = num(p, "seconds")?.unwrap_or(0.0);
  let id = add_to_gallery(s, opt_str(p, "name"), png, (img.width(), img.height()), samples, seconds)?;
  Ok(json!({"id": id}))
}

fn gallery_export(s: &mut Session, p: &Value) -> CmdResult {
  let i = gallery_index(s, p)?;
  let path = req_str(p, "path")?;
  let g = &s.design.render.gallery[i];
  std::fs::write(path, g.png.0.as_slice())?;
  Ok(json!({"path": path, "width": g.width, "height": g.height}))
}

fn gallery_rename(s: &mut Session, p: &Value) -> CmdResult {
  let i = gallery_index(s, p)?;
  let name = req_str(p, "name")?.trim().to_string();
  if name.is_empty() {
    return Err(CmdError::new("a render needs a name"));
  }
  edit(s, "Rename Render", |r| {
    r.gallery[i].name = name.clone();
    Ok(())
  })?;
  Ok(json!({"id": s.design.render.gallery[i].id, "name": name}))
}

fn gallery_delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = gallery_index(s, p)?;
  let id = s.design.render.gallery[i].id;
  edit(s, "Delete Render", |r| {
    r.gallery.remove(i);
    Ok(())
  })?;
  Ok(json!({"deleted": id}))
}

/// The design's render data with the gallery left out (what the viewport's look depends on).
pub fn look_of(d: &Design) -> RenderDoc {
  RenderDoc { gallery: Vec::new(), ..d.render.clone() }
}

// ---------------------------------------------------------------- lights

fn light_index(s: &Session, name: &str) -> Result<usize, CmdError> {
  s.design
    .render
    .lights
    .iter()
    .position(|l| l.name.eq_ignore_ascii_case(name))
    .ok_or_else(|| CmdError::new(format!("no light '{name}' (see 'render.lights')")))
}

fn vec3_f32(p: &Value, k: &str) -> Result<[f32; 3], CmdError> {
  Ok(crate::commands::req_vec3(p, k)?.map(|v| v as f32))
}

/// Applies the light fields present in `p` to `l`.
fn set_light_fields(l: &mut Light, p: &Value) -> Result<(), CmdError> {
  if p.get("position").is_some_and(|v| !v.is_null()) {
    l.position = vec3_f32(p, "position")?;
  }
  if let Some(v) = unit_num(p, "intensity", 0.0, 1e7)? {
    l.intensity = v;
  }
  if let Some(c) = p.get("color").filter(|c| !c.is_null()) {
    l.color = parse_color(c)?;
  }
  if let Some(v) = flag(p, "on")? {
    l.on = v;
  }
  Ok(())
}

/// `{position, intensity?, color?, name?, on?}`: places a light; returns it.
fn light_add(s: &mut Session, p: &Value) -> CmdResult {
  if s.design.render.lights.len() >= fox_raytrace::light::MAX_LIGHTS {
    return Err(CmdError::new(format!("a design takes up to {} lights", fox_raytrace::light::MAX_LIGHTS)));
  }
  if p.get("position").is_none_or(Value::is_null) {
    return Err(CmdError::new("a light needs a 'position' [x, y, z]"));
  }
  let lights = &s.design.render.lights;
  let name = match opt_str(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
    Some(n) if lights.iter().any(|l| l.name.eq_ignore_ascii_case(n)) => {
      return Err(CmdError::new(format!("there is already a light '{n}'")));
    }
    Some(n) => n.to_string(),
    None => (1..)
      .map(|i| format!("Light{i}"))
      .find(|n| !lights.iter().any(|l| l.name.eq_ignore_ascii_case(n)))
      .unwrap_or_default(),
  };
  let mut l = Light { name, ..Light::default() };
  set_light_fields(&mut l, p)?;
  let out = json!(l);
  edit(s, "New Light", |r| {
    r.lights.push(l);
    Ok(())
  })?;
  Ok(out)
}

fn light_edit(s: &mut Session, p: &Value) -> CmdResult {
  let i = light_index(s, req_str(p, "name")?)?;
  let mut l = s.design.render.lights[i].clone();
  set_light_fields(&mut l, p)?;
  if let Some(n) = opt_str(p, "rename").map(str::trim) {
    if n.is_empty() {
      return Err(CmdError::new("a light needs a name"));
    }
    if s.design.render.lights.iter().enumerate().any(|(j, x)| j != i && x.name.eq_ignore_ascii_case(n)) {
      return Err(CmdError::new(format!("there is already a light '{n}'")));
    }
    l.name = n.to_string();
  }
  let out = json!(l);
  if l != s.design.render.lights[i] {
    edit(s, "Edit Light", |r| {
      r.lights[i] = l;
      Ok(())
    })?;
  }
  Ok(out)
}

fn light_delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = light_index(s, req_str(p, "name")?)?;
  let name = s.design.render.lights[i].name.clone();
  edit(s, "Delete Light", |r| {
    r.lights.remove(i);
    Ok(())
  })?;
  Ok(json!({"deleted": name}))
}

// ---------------------------------------------------------------- dielectric priority

/// Fusion's priorities.
const PRIORITIES: [u8; 3] = [0, 1, 2];

fn priorities_json(s: &Session) -> Value {
  json!(
    s.design.render.priorities.iter().map(|p| json!({"body": p.body.0, "priority": p.priority})).collect::<Vec<_>>()
  )
}

/// `{bodies, priority}`: the bodies' dielectric priority (0 takes it off).
fn priority(s: &mut Session, p: &Value) -> CmdResult {
  let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if bodies.is_empty() {
    return Err(CmdError::new("pass the 'bodies' to give a priority"));
  }
  for b in &bodies {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  let prio = match num(p, "priority")? {
    Some(v) if PRIORITIES.contains(&(v as u8)) && v.fract() == 0.0 => v as u8,
    _ => return Err(CmdError::new("'priority' is 0, 1 or 2")),
  };
  edit(s, "Dielectric Priority", |r| {
    r.priorities.retain(|x| !bodies.contains(&x.body));
    if prio > 0 {
      r.priorities.extend(bodies.iter().map(|b| BodyPriority { body: *b, priority: prio }));
    }
    Ok(())
  })?;
  Ok(priorities_json(s))
}

// ---------------------------------------------------------------- texture map controls

/// The texture a body's material projects, if it has one.
pub fn body_texture(s: &Session, body: BodyId) -> Option<TextureSpec> {
  body_material(s, body).texture
}

/// `{body, projection?, axis?, scale?, rotation?, offset?, reset?}`: changes how the body's
/// texture is mapped. The body's material becomes a design material (a library one is copied in
/// under its name) and is put on the body, so the change stays with the design.
fn texture_map(s: &mut Session, p: &Value) -> CmdResult {
  let body = BodyId(req_u32(p, "body")?);
  if s.model.body(body).is_none() {
    return Err(CmdError::new(format!("no body {}", body.0)));
  }
  let mut m = body_material(s, body);
  let Some(old) = m.texture.clone() else {
    return Err(CmdError::new(format!("{}'s appearance '{}' has no texture to map", s.body_name(body), m.name)));
  };
  let mut t = if p.get("reset").and_then(Value::as_bool) == Some(true) {
    // where the library material had it, else Fusion's default: automatic, at the body's box
    let lib = fox_raytrace::material::find(&m.name).and_then(|l| l.texture).unwrap_or_default();
    TextureSpec {
      projection: lib.projection,
      axis: lib.axis,
      scale: lib.scale,
      rotation: lib.rotation,
      offset: lib.offset,
      ..old.clone()
    }
  } else {
    old.clone()
  };
  // only the mapping changes here, never the image or pattern
  let mut q = p.clone();
  if let Some(o) = q.as_object_mut() {
    for k in ["image", "pattern", "color2", "roughness_variation"] {
      o.remove(k);
    }
  }
  t = texture_spec(t, &q)?;
  m.texture = Some(t.clone());
  let own = s.design.render.materials.iter().position(|x| x.name.eq_ignore_ascii_case(&m.name));
  if own.is_none() {
    m.category = "Design".into();
  }
  let assigned = s.design.render.body_material(body).is_some_and(|n| n.eq_ignore_ascii_case(&m.name));
  let out = json!({
    "body": body.0, "material": m.name, "projection": t.projection, "axis": t.axis, "scale": t.scale,
    "rotation": t.rotation, "offset": t.offset,
  });
  if t != old || own.is_none() || !assigned {
    edit(s, "Texture Map Controls", |r| {
      let name = m.name.clone();
      match own {
        Some(i) => r.materials[i] = m,
        None => r.materials.push(m),
      }
      if !assigned {
        r.bodies.retain(|x| x.body != body);
        r.bodies.push(BodyMaterial { body, material: name });
      }
      Ok(())
    })?;
  }
  Ok(out)
}

/// Where NEW LIGHT puts a light aimed at the selection: off the picked faces along their
/// normals, or above picked edges, vertices and bodies, a quarter of the model's size away.
pub fn light_target(s: &Session, sel: &[crate::Sel]) -> Option<[f32; 3]> {
  use crate::Sel;
  use fox_kernel::glam::DVec3;
  let (lo, hi) = s.model.bounds()?;
  let off = (hi - lo).length().max(1.0) * 0.25;
  let mut points = Vec::new();
  for x in sel {
    let p = match *x {
      Sel::Face { body, face } => {
        let m = s.model.body(body)?.mesh()?;
        let (mut c, mut n, mut area) = (DVec3::ZERO, DVec3::ZERO, 0.0);
        for (t, f) in m.tri_face.iter().enumerate() {
          if *f != face {
            continue;
          }
          let v = |k: usize| DVec3::from(m.positions[m.indices[t * 3 + k] as usize].map(f64::from));
          let (a, b, cc) = (v(0), v(1), v(2));
          let cr = (b - a).cross(cc - a);
          let w = cr.length() / 2.0;
          c += (a + b + cc) / 3.0 * w;
          n += cr;
          area += w;
        }
        if area <= 0.0 {
          continue;
        }
        c / area + n.normalize_or_zero() * off
      }
      Sel::Edge { body, edge } => {
        let e = s.model.body(body)?.edges().iter().find(|e| e.id == edge)?;
        e.mid + DVec3::Z * off
      }
      Sel::Vertex { body, edge, end } => {
        let e = s.model.body(body)?.edges().iter().find(|e| e.id == edge)?;
        (if end { e.end } else { e.start }) + DVec3::Z * off
      }
      Sel::Body { body } => {
        let m = s.model.body(body)?.mesh()?;
        let (mut blo, mut bhi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for p in &m.positions {
          let p = DVec3::from(p.map(f64::from));
          blo = blo.min(p);
          bhi = bhi.max(p);
        }
        DVec3::new((blo.x + bhi.x) / 2.0, (blo.y + bhi.y) / 2.0, bhi.z + off)
      }
      _ => continue,
    };
    points.push(p);
  }
  if points.is_empty() {
    return None;
  }
  let c = points.iter().sum::<DVec3>() / points.len() as f64;
  Some(c.as_vec3().to_array())
}
