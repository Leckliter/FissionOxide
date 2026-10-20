//! Opening, importing and exporting every file format FissionOxide knows (see
//! [`fox_exchange::FORMATS`]).
//!
//! Opening a non-native file makes a new design from it, as Fusion does: a STEP, IGES or mesh
//! becomes a design with one Import feature, a DXF a design with one sketch. Importing adds the
//! same features to the current design.

use std::path::{Path, PathBuf};

use fox_exchange::archive::{Archive, read_foxz, write_foxz};
use fox_exchange::dxf::{Drawing, Entity, Spline, read_dxf, write_dxf};
use fox_exchange::svg::{read_svg, write_svg};
use fox_exchange::{FormatKind, Mesh, format_for_path};
use fox_model::{Design, FeatureId, FeatureKind, MeshData};
use fox_sketch::{CurveKind, PtSpec, Sketch};
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, Session};

/// Largest mesh (triangles per import) turned into a body.
pub const MAX_MESH_TRIANGLES: usize = 250_000;
/// Snap distance (mm) for joining DXF endpoints into shared sketch points.
const DXF_SNAP: f64 = 1e-4;

fn ext_of(path: &Path) -> String {
  path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn stem_of(path: &Path) -> String {
  path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into())
}

fn file_name(path: &Path) -> String {
  path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
}

fn unknown(path: &Path) -> CmdError {
  let known: Vec<String> =
    fox_exchange::FORMATS.iter().flat_map(|f| f.extensions.iter().map(|e| format!(".{e}"))).collect();
  CmdError::new(format!("{}: unknown file type (FissionOxide reads {})", path.display(), known.join(" ")))
}

fn ex(e: fox_exchange::ExchangeError) -> CmdError {
  CmdError::new(e.to_string())
}

/// Opens any readable file as a session. Native designs keep their path (so Save writes back);
/// translated files start unsaved.
pub fn open(path: &Path) -> Result<Session, CmdError> {
  let fmt = format_for_path(path).ok_or_else(|| unknown(path))?;
  if !fmt.import {
    return Err(CmdError::new(format!("FissionOxide can't read {} files", fmt.name)));
  }
  match fmt.id {
    "fox" => {
      let mut s = Session::new(Design::load(path)?);
      s.path = Some(path.to_path_buf());
      Ok(s)
    }
    "foxz" => {
      let a = read_foxz(&std::fs::read(path)?).map_err(ex)?;
      let mut s = Session::new(Design::from_json(&a.design)?);
      s.path = Some(path.to_path_buf());
      Ok(s)
    }
    _ => {
      let mut s = Session::new(Design::new(stem_of(path)));
      import(&mut s, path, &json!({}))?;
      // the translation is the starting point, not an undoable step
      s.undo.clear();
      s.redo.clear();
      s.dirty = true;
      Ok(s)
    }
  }
}

/// Saves the design: `.foxz` writes an archive (with a thumbnail), anything else `.fox` JSON.
pub fn save_design(s: &Session, path: &Path) -> Result<(), CmdError> {
  if matches!(ext_of(path).as_str(), "foxz" | "clfz") {
    let a = Archive {
      name: s.design.name.clone(),
      design: s.design.to_json(),
      thumbnail: thumbnail_png(s, 256, 256),
      history: Vec::new(),
    };
    std::fs::write(path, write_foxz(&a).map_err(ex)?)?;
  } else {
    s.design.save(path)?;
  }
  Ok(())
}

/// Imports a file into the current design. Returns the new feature(s).
pub fn import(s: &mut Session, path: &Path, p: &Value) -> CmdResult {
  let fmt = format_for_path(path).ok_or_else(|| unknown(path))?;
  if !fmt.import {
    return Err(CmdError::new(format!("FissionOxide can't read {} files", fmt.name)));
  }
  let name = file_name(path);
  match fmt.id {
    "step" => import_step(s, path, name),
    "iges" | "brep" => {
      let shapes = match fmt.id {
        "iges" => fox_kernel::read_iges(path)?,
        _ => fox_kernel::read_brep_file(path)?,
      };
      let breps = shapes.iter().map(|sh| sh.to_brep()).collect::<Result<Vec<_>, _>>()?;
      add_import(s, FeatureKind::Import { file_name: name, breps, meshes: Vec::new() })
    }
    "stl" | "obj" | "ply" | "3mf" => {
      let meshes = fox_exchange::read_mesh(fmt.id, &std::fs::read(path)?).map_err(ex)?;
      import_meshes(s, name, meshes, p)
    }
    "dxf" | "svg" => import_dxf(s, path, p),
    "fox" | "foxz" => {
      // insert another design's bodies (as exact geometry) into this one
      let other = open(path)?;
      if other.model.bodies.is_empty() {
        return Err(CmdError::new(format!("{name} has no bodies to insert")));
      }
      let breps = other.model.bodies.iter().map(|b| b.shape.to_brep()).collect::<Result<Vec<_>, _>>()?;
      add_import(s, FeatureKind::Import { file_name: name, breps, meshes: Vec::new() })
    }
    "f3d" => import_fusion(s, path, p),
    other => Err(CmdError::new(format!("import of '{other}' is not implemented"))),
  }
}

fn import_meshes(s: &mut Session, file_name: String, meshes: Vec<Mesh>, p: &Value) -> CmdResult {
  let tris: usize = meshes.iter().map(Mesh::triangle_count).sum();
  if tris > MAX_MESH_TRIANGLES {
    return Err(CmdError::new(format!(
      "{file_name} has {tris} triangles; FissionOxide turns meshes of up to {MAX_MESH_TRIANGLES} into bodies (decimate it first)"
    )));
  }
  let merge = p.get("merge_coplanar").and_then(Value::as_bool).unwrap_or(true);
  let meshes = meshes
    .into_iter()
    .map(|m| MeshData {
      name: m.name,
      positions: m.positions.iter().flatten().copied().collect(),
      indices: m.indices,
      merge_coplanar: merge,
    })
    .collect();
  add_import(s, FeatureKind::Import { file_name, breps: Vec::new(), meshes })
}

/// A STEP file's bodies with their names and colours; its sub-assemblies become components.
/// Files whose structure can't be read come in as plain bodies.
fn import_step(s: &mut Session, path: &Path, file_name: String) -> CmdResult {
  let parts = match fox_kernel::read_step_assembly(path) {
    Ok(p) => p,
    Err(_) => fox_kernel::read_step(path)?
      .into_iter()
      .map(|shape| fox_kernel::StepBody { name: String::new(), component: String::new(), color: None, shape })
      .collect(),
  };
  let breps = parts.iter().map(|p| p.shape.to_brep()).collect::<Result<Vec<_>, _>>()?;
  let mut out = add_import(s, FeatureKind::Import { file_name, breps, meshes: Vec::new() })?;
  let bodies: Vec<fox_model::BodyId> = out["bodies"]
    .as_array()
    .map(|a| a.iter().filter_map(Value::as_u64).map(|v| fox_model::BodyId(v as u32)).collect())
    .unwrap_or_default();
  let named = parts.iter().any(|p| !p.name.is_empty() || p.color.is_some() || !p.component.is_empty());
  if bodies.len() != parts.len() || !named {
    return Ok(out);
  }
  let components = s.edit(|d| {
    let mut groups: Vec<(String, Vec<fox_model::BodyId>)> = Vec::new();
    for (b, part) in bodies.iter().zip(&parts) {
      let set = d.body_settings_mut(*b);
      if !part.name.trim().is_empty() {
        set.name = Some(part.name.trim().to_string());
      }
      set.color = part.color;
      if !part.component.is_empty() {
        match groups.iter_mut().find(|g| g.0 == part.component) {
          Some(g) => g.1.push(*b),
          None => groups.push((part.component.clone(), vec![*b])),
        }
      }
    }
    let mut ids = Vec::new();
    for (name, members) in groups {
      ids.push(d.add_component(&name, members)?.0);
    }
    Ok(ids)
  });
  // one undo step with the import
  if let Ok(ids) = components {
    s.undo.pop();
    out["components"] = json!(ids);
  }
  Ok(out)
}

/// Writes bodies as a STEP assembly: components as sub-assemblies, bodies as named, coloured
/// parts.
fn write_step(s: &Session, bodies: &[&fox_model::regen::Body], path: &Path) -> Result<(), CmdError> {
  let info: Vec<(String, String, Option<[u8; 3]>)> = bodies
    .iter()
    .map(|b| {
      let component = s.design.component_of(b.id).map(|c| c.name.clone()).unwrap_or_default();
      (s.body_name(b.id), component, s.design.body_settings(b.id).and_then(|x| x.color))
    })
    .collect();
  let parts: Vec<fox_kernel::StepPart> = bodies
    .iter()
    .zip(&info)
    .map(|(b, (name, component, color))| fox_kernel::StepPart { name, component, color: *color, shape: &b.shape })
    .collect();
  Ok(fox_kernel::write_step_assembly(&s.design.name, &parts, path)?)
}

/// Adds an import feature; if it fails to build, nothing is added.
fn add_import(s: &mut Session, kind: FeatureKind) -> CmdResult {
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  if let Some(e) = s.model.status(id).and_then(|st| st.error.clone()) {
    s.undo();
    s.drop_redo();
    return Err(CmdError::new(format!("import failed: {e}")));
  }
  let added: Vec<u32> = s.model.bodies.iter().filter(|b| b.feature == id).map(|b| b.id.0).collect();
  let solids = s.model.bodies.iter().filter(|b| b.feature == id && b.shape.solid_count() > 0).count();
  let mut out = json!({"feature": id.0, "bodies": added, "solids": solids});
  if solids < added.len() {
    out["warning"] = json!(format!(
      "{} of {} imported bodies are open surfaces, not solids (the geometry isn't watertight)",
      added.len() - solids,
      added.len()
    ));
  }
  Ok(out)
}

/// A Fusion archive: its geometry is proprietary, but standard-format files packed inside it can
/// be imported.
fn import_fusion(s: &mut Session, path: &Path, p: &Value) -> CmdResult {
  let info = fox_exchange::archive::inspect_fusion(&std::fs::read(path)?).map_err(ex)?;
  if info.embedded.is_empty() {
    return Err(CmdError::new(format!(
      "{} is an Autodesk Fusion archive. Fusion stores its geometry in Autodesk's proprietary ShapeManager \
       format ({} B-rep blobs here), which no open-source tool can read. In Fusion use File > Export > STEP \
       (or IGES / 3MF) and import that; you can still keep the .f3d in the data panel with hub.upload.",
      file_name(path),
      info.brep_blobs
    )));
  }
  let dir = std::env::temp_dir().join(format!("fox-f3d-{}", std::process::id()));
  std::fs::create_dir_all(&dir)?;
  let mut results = Vec::new();
  for (name, data) in &info.embedded {
    let tmp = dir.join(Path::new(name).file_name().unwrap_or_default());
    std::fs::write(&tmp, data)?;
    let r = import(s, &tmp, p);
    let _ = std::fs::remove_file(&tmp);
    results.push(match r {
      Ok(v) => json!({"file": name, "ok": v}),
      Err(e) => json!({"file": name, "error": e.0}),
    });
  }
  let _ = std::fs::remove_dir(&dir);
  Ok(
    json!({"embedded": results, "warning": "only the standard-format files packed inside the Fusion archive were imported"}),
  )
}

/// A DXF or SVG file's 2D geometry.
fn read_drawing(path: &Path) -> Result<Drawing, CmdError> {
  let bytes = std::fs::read(path)?;
  let svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
  if svg { read_svg(&bytes) } else { read_dxf(&bytes) }.map_err(ex)
}

fn skipped_warning(path: &Path, d: &Drawing) -> Option<String> {
  let kind =
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) { "SVG content" } else { "DXF entities" };
  (!d.skipped.is_empty()).then(|| {
    let list: Vec<String> = d.skipped.iter().map(|(k, n)| format!("{n} {k}")).collect();
    format!("skipped unsupported {kind}: {}", list.join(", "))
  })
}

/// A DXF or SVG becomes a new sketch (on `plane`, default XY) with endpoints joined.
fn import_dxf(s: &mut Session, path: &Path, p: &Value) -> CmdResult {
  let drawing = read_drawing(path)?;
  if drawing.entities.is_empty() {
    return Err(CmdError::new(format!("{} has no lines, arcs, circles or curves", file_name(path))));
  }
  let was_editing = s.edit_sketch;
  let plane = p.get("plane").cloned().unwrap_or(json!("xy"));
  let r = crate::commands::run(s, "sketch.create", &json!({"plane": plane}))?;
  let id = FeatureId(r["sketch"].as_u64().unwrap_or_default() as u32);
  s.edit_sketch = was_editing;
  let name = stem_of(path);
  let ents = drawing.entities.clone();
  let added = s.edit(move |d| {
    if let Some(f) = d.feature_mut(id) {
      f.name = name;
    }
    let sk = d.sketch_mut(id)?;
    let mut n = 0usize;
    for (e, construction) in &ents {
      if let Some(c) = add_entity(sk, e.clone())? {
        if *construction {
          sk.set_construction(c, true)?;
        }
        n += 1;
      }
    }
    Ok(n)
  });
  // merge the sketch creation and the geometry into one undo step
  let added = match added {
    Ok(n) => {
      s.undo.pop();
      n
    }
    Err(e) => {
      s.undo();
      s.drop_redo();
      return Err(e);
    }
  };
  let mut out =
    json!({"sketch": id.0, "curves": added, "profiles": s.model.sketches.get(&id).map(|r| r.profiles.len())});
  if let Some(w) = skipped_warning(path, &drawing) {
    out["warning"] = json!(w);
  }
  Ok(out)
}

/// The curves of a DXF or SVG file as a free-standing sketch (endpoints joined), and a warning
/// about skipped entities.
pub(crate) fn dxf_sketch(path: &Path) -> Result<(Sketch, Option<String>), CmdError> {
  let drawing = read_drawing(path)?;
  let mut sk = Sketch::new();
  for (e, construction) in &drawing.entities {
    if let Some(c) = add_entity(&mut sk, e.clone())?
      && *construction
    {
      sk.set_construction(c, true)?;
    }
  }
  Ok((sk, skipped_warning(path, &drawing)))
}

fn snap(sk: &Sketch, at: [f64; 2]) -> PtSpec {
  match sk.nearest_point(at, DXF_SNAP, &[]) {
    Some(id) => PtSpec::Point(id),
    None => PtSpec::At(at[0], at[1]),
  }
}

fn add_entity(sk: &mut Sketch, e: Entity) -> Result<Option<fox_sketch::CurveId>, CmdError> {
  let finite = |v: &[f64]| v.iter().all(|x| x.is_finite());
  Ok(match e {
    Entity::Line { a, b } => {
      if !finite(&[a[0], a[1], b[0], b[1]]) || (a[0] - b[0]).hypot(a[1] - b[1]) < DXF_SNAP {
        return Ok(None);
      }
      let (pa, pb) = (snap(sk, a), snap(sk, b));
      Some(sk.add_line(pa, pb)?.0)
    }
    Entity::Circle { center, radius } => {
      if !finite(&[center[0], center[1], radius]) || radius < DXF_SNAP {
        return Ok(None);
      }
      let c = snap(sk, center);
      Some(sk.add_circle(c, radius)?)
    }
    Entity::Arc { center, radius, start, end } => {
      if !finite(&[center[0], center[1], radius, start, end]) || radius < DXF_SNAP {
        return Ok(None);
      }
      let (ps, pe) = (Entity::arc_point(center, radius, start), Entity::arc_point(center, radius, end));
      if (ps[0] - pe[0]).hypot(ps[1] - pe[1]) < DXF_SNAP {
        // a full-turn arc is a circle
        let c = snap(sk, center);
        return Ok(Some(sk.add_circle(c, radius)?));
      }
      let c = snap(sk, center);
      let c = sk.resolve(c)?;
      let s0 = snap(sk, ps);
      let s0 = sk.resolve(s0)?;
      let e0 = snap(sk, pe);
      let e0 = sk.resolve(e0)?;
      Some(sk.add_curve(CurveKind::Arc { center: c, start: s0, end: e0 })?)
    }
    Entity::Bezier { p } => {
      if !finite(&p.concat()) || p.iter().all(|q| (q[0] - p[0][0]).hypot(q[1] - p[0][1]) < DXF_SNAP) {
        return Ok(None);
      }
      // end points join neighbours; control points never snap
      let a = snap(sk, p[0]);
      let a = sk.resolve(a)?;
      let (c1, c2) = (sk.add_point(p[1][0], p[1][1]), sk.add_point(p[2][0], p[2][1]));
      let b = snap(sk, p[3]);
      let b = sk.resolve(b)?;
      Some(sk.add_curve(CurveKind::ControlSpline { points: vec![a, c1, c2, b] })?)
    }
    Entity::Point { at } => {
      if finite(&at) && sk.nearest_point(at, DXF_SNAP, &[]).is_none() {
        sk.add_point(at[0], at[1]);
      }
      None
    }
    Entity::Ellipse { center, major, ratio, .. } if e.is_full_ellipse() => {
      let minor = major[0].hypot(major[1]) * ratio;
      if !finite(&[center[0], center[1], major[0], major[1], minor]) || minor < DXF_SNAP {
        return Ok(None);
      }
      let c = snap(sk, center);
      let c = sk.resolve(c)?;
      let m = sk.add_point(center[0] + major[0], center[1] + major[1]);
      Some(sk.add_curve(CurveKind::Ellipse { center: c, major: m, minor })?)
    }
    Entity::Spline(sp) => add_spline(sk, &sp)?,
    Entity::Ellipse { center, major, ratio, start, end } => add_elliptical_arc(sk, center, major, ratio, start, end)?,
  })
}

/// A DXF elliptical arc as an elliptical arc curve: its ends join neighbours, its centre snaps.
fn add_elliptical_arc(
  sk: &mut Sketch,
  center: [f64; 2],
  major: [f64; 2],
  ratio: f64,
  start: f64,
  end: f64,
) -> Result<Option<fox_sketch::CurveId>, CmdError> {
  let a = major[0].hypot(major[1]);
  let minor = a * ratio;
  if ![center[0], center[1], major[0], major[1], minor, start, end].iter().all(|x| x.is_finite()) || minor < DXF_SNAP {
    return Ok(None);
  }
  let m = [center[0] + major[0], center[1] + major[1]];
  let Some(e) = fox_sketch::Ellipse2::new(center, m, minor) else { return Ok(None) };
  let (ps, pe) = (e.at(start), e.at(end));
  if dist(ps, pe) < DXF_SNAP {
    return Ok(None);
  }
  let c = snap(sk, center);
  let c = sk.resolve(c)?;
  let mp = sk.add_point(m[0], m[1]);
  let s0 = snap(sk, ps);
  let s0 = sk.resolve(s0)?;
  let e0 = snap(sk, pe);
  let e0 = sk.resolve(e0)?;
  Ok(Some(sk.add_curve(CurveKind::EllipticalArc { center: c, major: mp, minor, start: s0, end: e0 })?))
}

/// An elliptical arc as a DXF ELLIPSE with its start and end parameters (the axes swapped when
/// the minor radius is the larger, as DXF wants).
fn elliptical_arc_entity(sk: &Sketch, kind: &CurveKind) -> Option<Entity> {
  let CurveKind::EllipticalArc { center, major, minor, start, end } = *kind else { return None };
  let (c, m) = (sk.pos(center)?, sk.pos(major)?);
  let e = fox_sketch::Ellipse2::new(c, m, minor)?;
  let t0 = e.param(sk.pos(start)?);
  let sweep = (e.param(sk.pos(end)?) - t0).rem_euclid(std::f64::consts::TAU);
  let sweep = if sweep < 1e-12 { std::f64::consts::TAU } else { sweep };
  let Entity::Ellipse { major, ratio, .. } = Entity::ellipse(c, [m[0] - c[0], m[1] - c[1]], minor) else { return None };
  // with the axes swapped the DXF parameter runs a quarter turn behind
  let shift = if minor > e.a { -std::f64::consts::FRAC_PI_2 } else { 0.0 };
  Some(Entity::Ellipse { center: c, major, ratio, start: t0 + shift, end: t0 + shift + sweep })
}

/// A DXF spline as the sketch curve that draws it exactly: a fit-point spline when the file's
/// fit points give the same curve (or are all it has), a control-point spline for a clamped
/// uniform B-spline, otherwise a Bézier chain.
fn add_spline(sk: &mut Sketch, sp: &Spline) -> Result<Option<fox_sketch::CurveId>, CmdError> {
  let pieces = sp.beziers();
  let finite = |v: &[[f64; 2]]| v.iter().flatten().all(|x| x.is_finite());
  if !finite(&sp.control) || !finite(&sp.fit) {
    return Ok(None);
  }
  let size = pieces.iter().flatten().chain(&sp.fit).fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs())).max(1.0);
  let same = |a: &[[[f64; 2]; 4]], b: &[[[f64; 2]; 4]]| {
    a.len() == b.len() && a.iter().flatten().zip(b.iter().flatten()).all(|(p, q)| dist(*p, *q) < 1e-7 * size)
  };
  let ends = |sk: &mut Sketch, pts: &[[f64; 2]]| -> Result<Vec<fox_sketch::PointId>, CmdError> {
    let n = pts.len();
    let mut ids = Vec::with_capacity(n);
    for (i, p) in pts.iter().enumerate() {
      // end points join neighbours; inner points never snap
      if i == 0 || i + 1 == n {
        let s = snap(sk, *p);
        ids.push(sk.resolve(s)?);
      } else {
        ids.push(sk.add_point(p[0], p[1]));
      }
    }
    Ok(ids)
  };
  let closed = sp.fit.len() >= 3 && sp.fit.first().zip(sp.fit.last()).is_some_and(|(a, b)| dist(*a, *b) < DXF_SNAP);
  if sp.fit.len() >= 2 && (sp.control.is_empty() || same(&fox_sketch::fit_point_beziers(&sp.fit, closed), &pieces)) {
    if sp.fit.iter().all(|p| dist(*p, sp.fit[0]) < DXF_SNAP) {
      return Ok(None);
    }
    if sp.fit.len() > 1000 {
      return add_chain(sk, &pieces);
    }
    let pts = ends(sk, &sp.fit)?;
    return Ok(Some(sk.add_curve(CurveKind::spline(pts))?));
  }
  let control_spline = !sp.is_rational()
    && sp.is_uniform()
    && sp.control.len() <= 1000
    && match sp.degree {
      3 => sp.control.len() >= 4,
      // the sketch's two- and three-point control splines are a line and a quadratic
      d => sp.control.len() == d + 1 && d <= 2,
    };
  if control_spline {
    let pts = ends(sk, &sp.control)?;
    return Ok(Some(sk.add_curve(CurveKind::ControlSpline { points: pts })?));
  }
  add_chain(sk, &pieces)
}

/// Cubic Bézier pieces as one Bézier chain curve.
fn add_chain(sk: &mut Sketch, pieces: &[[[f64; 2]; 4]]) -> Result<Option<fox_sketch::CurveId>, CmdError> {
  if pieces.is_empty() || pieces.len() > 1000 || !pieces.iter().flatten().flatten().all(|x| x.is_finite()) {
    return Ok(None);
  }
  let first = pieces[0][0];
  if pieces.iter().flatten().all(|q| dist(*q, first) < DXF_SNAP) {
    return Ok(None);
  }
  let mut ids = Vec::with_capacity(pieces.len() * 3 + 1);
  let a = snap(sk, first);
  ids.push(sk.resolve(a)?);
  for (i, p) in pieces.iter().enumerate() {
    ids.push(sk.add_point(p[1][0], p[1][1]));
    ids.push(sk.add_point(p[2][0], p[2][1]));
    if i + 1 == pieces.len() {
      let b = snap(sk, p[3]);
      ids.push(sk.resolve(b)?);
    } else {
      ids.push(sk.add_point(p[3][0], p[3][1]));
    }
  }
  Ok(Some(sk.add_curve(CurveKind::Bezier { points: ids })?))
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Sketch geometry as DXF entities (sketch coordinates); `exact` gives splines and ellipses as
/// Bézier pieces (SVG) instead of SPLINE and ELLIPSE entities (DXF).
fn sketch_entities(sk: &Sketch, exact: bool) -> Vec<(Entity, bool)> {
  let pos = |id| sk.pos(id).unwrap_or([0.0, 0.0]);
  let mut out = Vec::new();
  let mut on_curve = std::collections::HashSet::new();
  for c in &sk.curves {
    for p in fox_sketch::curve_points(&c.kind) {
      on_curve.insert(p);
    }
    if c.kind.is_freeform() {
      if exact {
        for p in sk.beziers(c.id) {
          out.push((Entity::Bezier { p }, c.construction));
        }
        continue;
      }
      // DXF gets real SPLINE and ELLIPSE entities that read back as the same kind of curve
      let pts = |ids: &[fox_sketch::PointId]| ids.iter().map(|p| pos(*p)).collect::<Vec<_>>();
      let e = match &c.kind {
        CurveKind::Ellipse { center, major, minor } => {
          let (c0, m) = (pos(*center), pos(*major));
          Entity::ellipse(c0, [m[0] - c0[0], m[1] - c0[1]], *minor)
        }
        CurveKind::ControlSpline { points } => {
          let degree = points.len().clamp(2, 4) - 1;
          Entity::Spline(Spline::clamped_uniform(degree, pts(points)))
        }
        CurveKind::Spline { points, .. } => {
          Entity::Spline(Spline { fit: pts(points), ..Spline::from_beziers(&sk.beziers(c.id)) })
        }
        CurveKind::EllipticalArc { .. } => match elliptical_arc_entity(sk, &c.kind) {
          Some(e) => e,
          None => continue,
        },
        _ => Entity::Spline(Spline::from_beziers(&sk.beziers(c.id))),
      };
      out.push((e, c.construction));
      continue;
    }
    let e = match c.kind {
      CurveKind::Spline { .. }
      | CurveKind::ControlSpline { .. }
      | CurveKind::Ellipse { .. }
      | CurveKind::Bezier { .. }
      | CurveKind::EllipticalArc { .. } => {
        continue;
      }
      CurveKind::Line { a, b } => Entity::Line { a: pos(a), b: pos(b) },
      CurveKind::Circle { center, radius } => Entity::Circle { center: pos(center), radius },
      CurveKind::Arc { center, start, end } => {
        let (c0, s0, e0) = (pos(center), pos(start), pos(end));
        Entity::Arc {
          center: c0,
          radius: (s0[0] - c0[0]).hypot(s0[1] - c0[1]),
          start: (s0[1] - c0[1]).atan2(s0[0] - c0[0]),
          end: (e0[1] - c0[1]).atan2(e0[0] - c0[0]),
        }
      }
    };
    out.push((e, c.construction));
  }
  for p in &sk.points {
    if p.id != fox_sketch::ORIGIN && !on_curve.contains(&p.id) {
      out.push((Entity::Point { at: [p.x, p.y] }, false));
    }
  }
  out
}

/// Export tessellation: finer than display.
fn fine_tol(b: &fox_model::regen::Body) -> f64 {
  (b.diagonal() * 0.0005).clamp(0.002, 0.1)
}

/// The bodies an export writes: `bodies` ids, or every visible body.
fn export_bodies<'a>(s: &'a Session, p: &Value) -> Result<Vec<&'a fox_model::regen::Body>, CmdError> {
  let wanted: Vec<u32> = p
    .get("bodies")
    .and_then(Value::as_array)
    .map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as u32).collect())
    .unwrap_or_default();
  let bodies: Vec<_> = s
    .model
    .bodies
    .iter()
    .filter(|b| if wanted.is_empty() { s.body_visible(b.id) } else { wanted.contains(&b.id.0) })
    .collect();
  if bodies.is_empty() {
    return Err(CmdError::new("there are no bodies to export"));
  }
  Ok(bodies)
}

/// Meshes of the bodies. `weld` merges the per-face vertices (watertight meshes for 3D printing)
/// and drops normals.
pub fn body_meshes(s: &Session, bodies: &[&fox_model::regen::Body], weld: bool) -> Vec<Mesh> {
  bodies
    .iter()
    .filter_map(|b| {
      let t = b.shape.tessellate(fine_tol(b)).ok()?;
      let m = Mesh { name: s.body_name(b.id), positions: t.positions, normals: t.normals, indices: t.indices };
      Some(if weld { Mesh { name: m.name.clone(), ..m.welded() } } else { m })
    })
    .collect()
}

/// A DXF export of bodies that are all sheet metal (no sketch named) writes the flat pattern.
fn sheet_export(s: &Session, p: &Value) -> bool {
  p.get("sketch").is_none()
    && p.get("bodies").is_some()
    && export_bodies(s, p).is_ok_and(|b| !b.is_empty() && b.iter().all(|b| b.sheet.is_some()))
}

pub fn export(s: &Session, p: &Value) -> CmdResult {
  let path = PathBuf::from(p.get("path").and_then(Value::as_str).ok_or(CmdError::new("missing 'path'"))?);
  let fmt = format_for_path(&path).ok_or_else(|| unknown(&path))?;
  if !fmt.export {
    return Err(CmdError::new(format!("FissionOxide can't write {} files ({})", fmt.name, fmt.note)));
  }
  let mut out = json!({"path": path, "format": fmt.id});
  match fmt.kind {
    FormatKind::Design | FormatKind::Archive => {
      if fmt.id == "foxz" {
        let a = Archive {
          name: s.design.name.clone(),
          design: s.design.to_json(),
          thumbnail: thumbnail_png(s, 256, 256),
          history: Vec::new(),
        };
        std::fs::write(&path, write_foxz(&a).map_err(ex)?)?;
      } else {
        s.design.save(&path)?;
      }
    }
    FormatKind::Brep => {
      let bodies = export_bodies(s, p)?;
      let shapes = bodies.iter().map(|b| &b.shape);
      match fmt.id {
        "step" => write_step(s, &bodies, &path)?,
        "iges" => fox_kernel::write_iges(shapes, &path)?,
        _ => fox_kernel::write_brep_file(shapes, &path)?,
      }
      out["bodies"] = json!(bodies.len());
    }
    FormatKind::Mesh => {
      let bodies = export_bodies(s, p)?;
      let weld = matches!(fmt.id, "stl" | "3mf" | "ply");
      let meshes = body_meshes(s, &bodies, weld);
      let bytes = match fmt.id {
        "stl" if p.get("ascii").and_then(Value::as_bool).unwrap_or(false) => {
          fox_exchange::write_stl_ascii(&meshes).into_bytes()
        }
        "3mf" => fox_exchange::write_3mf(&meshes, thumbnail_png(s, 256, 256).as_deref()).map_err(ex)?,
        id => fox_exchange::write_mesh(id, &meshes).map_err(ex)?,
      };
      std::fs::write(&path, bytes)?;
      out["bodies"] = json!(bodies.len());
      out["triangles"] = json!(meshes.iter().map(Mesh::triangle_count).sum::<usize>());
    }
    FormatKind::Sketch if p.get("flat_pattern").and_then(Value::as_bool).unwrap_or(false) || sheet_export(s, p) => {
      let bodies = export_bodies(s, p)?;
      let [b] = bodies.as_slice() else {
        return Err(CmdError::new("a flat pattern DXF holds one sheet metal body; pass 'bodies': [id]"));
      };
      let flat = b.flat_pattern(None).map_err(CmdError::new)?;
      std::fs::write(&path, crate::sheet2::flat_dxf(b, &flat)?)?;
      out["flat_pattern"] = json!(b.id.0);
    }
    FormatKind::Sketch => {
      let id = match p.get("sketch").and_then(Value::as_u64) {
        Some(v) => FeatureId(v as u32),
        None => s
          .edit_sketch
          .or_else(|| {
            s.design
              .features
              .iter()
              .take(s.design.active_len())
              .rev()
              .find(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
              .map(|f| f.id)
          })
          .ok_or(CmdError::new("there is no sketch to export (DXF holds 2D sketch geometry)"))?,
      };
      let sk = s
        .model
        .sketches
        .get(&id)
        .map(|r| &r.sketch)
        .ok_or(CmdError::new("that sketch isn't computed (rolled back?)"))?;
      let svg = fmt.id == "svg";
      let ents = sketch_entities(sk, svg);
      if ents.is_empty() {
        return Err(CmdError::new("the sketch is empty"));
      }
      std::fs::write(&path, if svg { write_svg(&ents) } else { write_dxf(&ents) })?;
      out["sketch"] = json!(id.0);
      out["entities"] = json!(ents.len());
    }
  }
  Ok(out)
}

/// What a file contains, without opening it.
pub fn inspect(path: &Path) -> CmdResult {
  let fmt = format_for_path(path).ok_or_else(|| unknown(path))?;
  let size = std::fs::metadata(path)?.len();
  let mut out = json!({"path": path, "format": fmt, "size": size});
  match fmt.id {
    "f3d" => {
      let info = fox_exchange::archive::inspect_fusion(&std::fs::read(path)?).map_err(ex)?;
      out["fusion"] = info.summary();
    }
    "foxz" => {
      let a = read_foxz(&std::fs::read(path)?).map_err(ex)?;
      out["name"] = json!(a.name);
      out["versions"] = json!(a.history.iter().map(|v| v.info.clone()).collect::<Vec<_>>());
    }
    "stl" | "obj" | "ply" | "3mf" => {
      let m = fox_exchange::read_mesh(fmt.id, &std::fs::read(path)?).map_err(ex)?;
      out["meshes"] = json!(
        m.iter()
          .map(|m| json!({"name": m.name, "triangles": m.triangle_count(), "bounds": m.bounds()}))
          .collect::<Vec<_>>()
      );
    }
    "dxf" | "svg" => {
      let d = read_drawing(path)?;
      out["entities"] = json!(d.entities.len());
      out["skipped"] = json!(d.skipped);
    }
    _ => {}
  }
  Ok(out)
}

/// A PNG preview of the design from the home view.
pub fn thumbnail_png(s: &Session, w: u32, h: u32) -> Option<Vec<u8>> {
  let mut cam = fox_render::Camera::default();
  cam.set_view(fox_render::StandardView::Home);
  thumbnail_from(s, cam, w, h)
}

/// A PNG preview of the design from `yaw` and `pitch` (degrees, as `view.render` takes them).
pub fn thumbnail_png_turned(s: &Session, w: u32, h: u32, yaw: f64, pitch: f64) -> Option<Vec<u8>> {
  let cam = fox_render::Camera { yaw: yaw.to_radians(), pitch: pitch.to_radians(), ..Default::default() };
  thumbnail_from(s, cam, w, h)
}

fn thumbnail_from(s: &Session, mut cam: fox_render::Camera, w: u32, h: u32) -> Option<Vec<u8>> {
  let (lo, hi) =
    s.model.bounds().unwrap_or((fox_kernel::glam::DVec3::splat(-50.0), fox_kernel::glam::DVec3::splat(50.0)));
  cam.fit(lo, hi, w as f64 / h.max(1) as f64);
  let opts = crate::scene::ViewOptions { hide_grid: true, ..crate::scene::ViewOptions::default() };
  let (scene, _) = crate::scene::SceneBuilder::default().build(s, &opts);
  fox_render::render(&scene, &cam, w, h).to_png().ok()
}
