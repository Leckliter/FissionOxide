//! Opening, importing and exporting every file format CLFusion knows (see
//! [`clf_exchange::FORMATS`]).
//!
//! Opening a non-native file makes a new design from it, as Fusion does: a STEP, IGES or mesh
//! becomes a design with one Import feature, a DXF a design with one sketch. Importing adds the
//! same features to the current design.

use std::path::{Path, PathBuf};

use clf_exchange::archive::{Archive, read_clfz, write_clfz};
use clf_exchange::dxf::{Entity, read_dxf, write_dxf};
use clf_exchange::{FormatKind, Mesh, format_for_path};
use clf_model::{Design, FeatureId, FeatureKind, MeshData};
use clf_sketch::{CurveKind, PtSpec, Sketch};
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
    clf_exchange::FORMATS.iter().flat_map(|f| f.extensions.iter().map(|e| format!(".{e}"))).collect();
  CmdError::new(format!("{}: unknown file type (CLFusion reads {})", path.display(), known.join(" ")))
}

fn ex(e: clf_exchange::ExchangeError) -> CmdError {
  CmdError::new(e.to_string())
}

/// Opens any readable file as a session. Native designs keep their path (so Save writes back);
/// translated files start unsaved.
pub fn open(path: &Path) -> Result<Session, CmdError> {
  let fmt = format_for_path(path).ok_or_else(|| unknown(path))?;
  if !fmt.import {
    return Err(CmdError::new(format!("CLFusion can't read {} files", fmt.name)));
  }
  match fmt.id {
    "clf" => {
      let mut s = Session::new(Design::load(path)?);
      s.path = Some(path.to_path_buf());
      Ok(s)
    }
    "clfz" => {
      let a = read_clfz(&std::fs::read(path)?).map_err(ex)?;
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

/// Saves the design: `.clfz` writes an archive (with a thumbnail), anything else `.clf` JSON.
pub fn save_design(s: &Session, path: &Path) -> Result<(), CmdError> {
  if ext_of(path) == "clfz" {
    let a = Archive {
      name: s.design.name.clone(),
      design: s.design.to_json(),
      thumbnail: thumbnail_png(s, 256, 256),
      history: Vec::new(),
    };
    std::fs::write(path, write_clfz(&a).map_err(ex)?)?;
  } else {
    s.design.save(path)?;
  }
  Ok(())
}

/// Imports a file into the current design. Returns the new feature(s).
pub fn import(s: &mut Session, path: &Path, p: &Value) -> CmdResult {
  let fmt = format_for_path(path).ok_or_else(|| unknown(path))?;
  if !fmt.import {
    return Err(CmdError::new(format!("CLFusion can't read {} files", fmt.name)));
  }
  let name = file_name(path);
  match fmt.id {
    "step" | "iges" | "brep" => {
      let shapes = match fmt.id {
        "step" => clf_kernel::read_step(path)?,
        "iges" => clf_kernel::read_iges(path)?,
        _ => clf_kernel::read_brep_file(path)?,
      };
      let breps = shapes.iter().map(|sh| sh.to_brep()).collect::<Result<Vec<_>, _>>()?;
      add_import(s, FeatureKind::Import { file_name: name, breps, meshes: Vec::new() })
    }
    "stl" | "obj" | "ply" | "3mf" => {
      let meshes = clf_exchange::read_mesh(fmt.id, &std::fs::read(path)?).map_err(ex)?;
      import_meshes(s, name, meshes, p)
    }
    "dxf" => import_dxf(s, path, p),
    "clf" | "clfz" => {
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
      "{file_name} has {tris} triangles; CLFusion turns meshes of up to {MAX_MESH_TRIANGLES} into bodies (decimate it first)"
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
  let info = clf_exchange::archive::inspect_fusion(&std::fs::read(path)?).map_err(ex)?;
  if info.embedded.is_empty() {
    return Err(CmdError::new(format!(
      "{} is an Autodesk Fusion archive. Fusion stores its geometry in Autodesk's proprietary ShapeManager \
       format ({} B-rep blobs here), which no open-source tool can read. In Fusion use File > Export > STEP \
       (or IGES / 3MF) and import that; you can still keep the .f3d in the data panel with hub.upload.",
      file_name(path),
      info.brep_blobs
    )));
  }
  let dir = std::env::temp_dir().join(format!("clf-f3d-{}", std::process::id()));
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

/// A DXF becomes a new sketch (on `plane`, default XY) with endpoints joined.
fn import_dxf(s: &mut Session, path: &Path, p: &Value) -> CmdResult {
  let drawing = read_dxf(&std::fs::read(path)?).map_err(ex)?;
  if drawing.entities.is_empty() {
    return Err(CmdError::new(format!("{} has no lines, arcs, circles or polylines", file_name(path))));
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
      if let Some(c) = add_entity(sk, *e)? {
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
  if !drawing.skipped.is_empty() {
    let list: Vec<String> = drawing.skipped.iter().map(|(k, n)| format!("{n} {k}")).collect();
    out["warning"] = json!(format!("skipped unsupported DXF entities: {}", list.join(", ")));
  }
  Ok(out)
}

/// The curves of a DXF file as a free-standing sketch (endpoints joined), and a warning about
/// skipped entities.
pub(crate) fn dxf_sketch(path: &Path) -> Result<(Sketch, Option<String>), CmdError> {
  let drawing = read_dxf(&std::fs::read(path)?).map_err(ex)?;
  let mut sk = Sketch::new();
  for (e, construction) in &drawing.entities {
    if let Some(c) = add_entity(&mut sk, *e)?
      && *construction
    {
      sk.set_construction(c, true)?;
    }
  }
  let warning = (!drawing.skipped.is_empty()).then(|| {
    let list: Vec<String> = drawing.skipped.iter().map(|(k, n)| format!("{n} {k}")).collect();
    format!("skipped unsupported DXF entities: {}", list.join(", "))
  });
  Ok((sk, warning))
}

fn snap(sk: &Sketch, at: [f64; 2]) -> PtSpec {
  match sk.nearest_point(at, DXF_SNAP, &[]) {
    Some(id) => PtSpec::Point(id),
    None => PtSpec::At(at[0], at[1]),
  }
}

fn add_entity(sk: &mut Sketch, e: Entity) -> Result<Option<clf_sketch::CurveId>, CmdError> {
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
    Entity::Point { at } => {
      if finite(&at) && sk.nearest_point(at, DXF_SNAP, &[]).is_none() {
        sk.add_point(at[0], at[1]);
      }
      None
    }
  })
}

/// Sketch geometry as DXF entities (sketch coordinates).
fn sketch_entities(sk: &Sketch) -> Vec<(Entity, bool)> {
  let pos = |id| sk.pos(id).unwrap_or([0.0, 0.0]);
  let mut out = Vec::new();
  let mut on_curve = std::collections::HashSet::new();
  for c in &sk.curves {
    for p in clf_sketch::curve_points(&c.kind) {
      on_curve.insert(p);
    }
    if c.kind.is_freeform() {
      // DXF gets the spline as short lines
      for w in sk.spline_points(c.id).windows(2) {
        out.push((Entity::Line { a: w[0], b: w[1] }, c.construction));
      }
      continue;
    }
    let e = match c.kind {
      CurveKind::Spline { .. }
      | CurveKind::ControlSpline { .. }
      | CurveKind::Ellipse { .. }
      | CurveKind::Bezier { .. } => {
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
    if p.id != clf_sketch::ORIGIN && !on_curve.contains(&p.id) {
      out.push((Entity::Point { at: [p.x, p.y] }, false));
    }
  }
  out
}

/// Export tessellation: finer than display.
fn fine_tol(b: &clf_model::regen::Body) -> f64 {
  (b.diagonal() * 0.0005).clamp(0.002, 0.1)
}

/// The bodies an export writes: `bodies` ids, or every visible body.
fn export_bodies<'a>(s: &'a Session, p: &Value) -> Result<Vec<&'a clf_model::regen::Body>, CmdError> {
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
pub fn body_meshes(s: &Session, bodies: &[&clf_model::regen::Body], weld: bool) -> Vec<Mesh> {
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
    return Err(CmdError::new(format!("CLFusion can't write {} files ({})", fmt.name, fmt.note)));
  }
  let mut out = json!({"path": path, "format": fmt.id});
  match fmt.kind {
    FormatKind::Design | FormatKind::Archive => {
      if fmt.id == "clfz" {
        let a = Archive {
          name: s.design.name.clone(),
          design: s.design.to_json(),
          thumbnail: thumbnail_png(s, 256, 256),
          history: Vec::new(),
        };
        std::fs::write(&path, write_clfz(&a).map_err(ex)?)?;
      } else {
        s.design.save(&path)?;
      }
    }
    FormatKind::Brep => {
      let bodies = export_bodies(s, p)?;
      let shapes = bodies.iter().map(|b| &b.shape);
      match fmt.id {
        "step" => clf_kernel::write_step(shapes, &path)?,
        "iges" => clf_kernel::write_iges(shapes, &path)?,
        _ => clf_kernel::write_brep_file(shapes, &path)?,
      }
      out["bodies"] = json!(bodies.len());
    }
    FormatKind::Mesh => {
      let bodies = export_bodies(s, p)?;
      let weld = matches!(fmt.id, "stl" | "3mf" | "ply");
      let meshes = body_meshes(s, &bodies, weld);
      let bytes = match fmt.id {
        "stl" if p.get("ascii").and_then(Value::as_bool).unwrap_or(false) => {
          clf_exchange::write_stl_ascii(&meshes).into_bytes()
        }
        "3mf" => clf_exchange::write_3mf(&meshes, thumbnail_png(s, 256, 256).as_deref()).map_err(ex)?,
        id => clf_exchange::write_mesh(id, &meshes).map_err(ex)?,
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
      std::fs::write(&path, clf_model::sheet::flat_dxf(&flat).map_err(CmdError::new)?)?;
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
      let ents = sketch_entities(sk);
      if ents.is_empty() {
        return Err(CmdError::new("the sketch is empty"));
      }
      std::fs::write(&path, write_dxf(&ents))?;
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
      let info = clf_exchange::archive::inspect_fusion(&std::fs::read(path)?).map_err(ex)?;
      out["fusion"] = info.summary();
    }
    "clfz" => {
      let a = read_clfz(&std::fs::read(path)?).map_err(ex)?;
      out["name"] = json!(a.name);
      out["versions"] = json!(a.history.iter().map(|v| v.info.clone()).collect::<Vec<_>>());
    }
    "stl" | "obj" | "ply" | "3mf" => {
      let m = clf_exchange::read_mesh(fmt.id, &std::fs::read(path)?).map_err(ex)?;
      out["meshes"] = json!(
        m.iter()
          .map(|m| json!({"name": m.name, "triangles": m.triangle_count(), "bounds": m.bounds()}))
          .collect::<Vec<_>>()
      );
    }
    "dxf" => {
      let d = read_dxf(&std::fs::read(path)?).map_err(ex)?;
      out["entities"] = json!(d.entities.len());
      out["skipped"] = json!(d.skipped);
    }
    _ => {}
  }
  Ok(out)
}

/// A PNG preview of the design from the home view.
pub fn thumbnail_png(s: &Session, w: u32, h: u32) -> Option<Vec<u8>> {
  use clf_render::{Camera, StandardView};
  let mut cam = Camera::default();
  cam.set_view(StandardView::Home);
  let (lo, hi) =
    s.model.bounds().unwrap_or((clf_kernel::glam::DVec3::splat(-50.0), clf_kernel::glam::DVec3::splat(50.0)));
  cam.fit(lo, hi, w as f64 / h.max(1) as f64);
  let opts = crate::scene::ViewOptions { hide_grid: true, ..crate::scene::ViewOptions::default() };
  let (scene, _) = crate::scene::SceneBuilder::default().build(s, &opts);
  clf_render::render(&scene, &cam, w, h).to_png().ok()
}
