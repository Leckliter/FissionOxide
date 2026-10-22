//! The design's flat pattern, as Fusion's Create Flat Pattern makes it: a sheet metal body laid
//! flat on a stationary face, kept with the design (the browser's Flat Pattern row) rather than
//! in the timeline, and what Export Flat Pattern writes from it.

use fox_model::{FlatPatternDef, sheet::FlatPattern};
use serde_json::{Value, json};

use crate::commands::face_ref;
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "sheet.create_flat_pattern" => create(s, p),
    "sheet.delete_flat_pattern" => delete(s),
    _ => return None,
  })
}

/// `{face: {body, face}}`: the flat pattern of the face's sheet metal body, the face staying put.
/// It replaces the design's flat pattern (one per design, as Fusion keeps one per component).
fn create(s: &mut Session, p: &Value) -> CmdResult {
  let f = p.get("face").ok_or(CmdError::new("missing 'face': the stationary face"))?;
  let face = face_ref(s, f)?;
  let body = s.model.body(face.body).ok_or(CmdError::new(format!("no body {}", face.body.0)))?;
  if body.sheet.as_ref().is_none_or(|i| i.flat) {
    return Err(CmdError::new("the stationary face must be on a folded sheet metal body"));
  }
  if !face.planar {
    return Err(CmdError::new("the stationary face must be planar"));
  }
  let id = face.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?;
  // it must unfold there
  body.flat_pattern(Some(id)).map_err(CmdError::new)?;
  let b = face.body;
  s.edit(|d| {
    d.flat_pattern = Some(FlatPatternDef { body: b, face });
    Ok(())
  })?;
  Ok(json!({"body": b.0, "face": id}))
}

fn delete(s: &mut Session) -> CmdResult {
  if s.design.flat_pattern.is_none() {
    return Err(CmdError::new("there is no flat pattern"));
  }
  s.edit(|d| {
    d.flat_pattern = None;
    Ok(())
  })?;
  Ok(json!({"deleted": true}))
}

/// The design's flat pattern, worked out on the current model: its body and the flat. None
/// without one, or when its body or face is gone or no longer unfolds.
pub fn design_flat(s: &Session) -> Option<(fox_model::BodyId, FlatPattern)> {
  let def = s.design.flat_pattern.as_ref()?;
  let body = s.model.body(def.body)?;
  let face = def.face.resolve(body.faces(), body.mesh(), body.diagonal())?;
  Some((def.body, body.flat_pattern(Some(face)).ok()?))
}

/// The design's flat pattern as `sheet.flat_pattern` takes it (`body`, `face`), for calls that
/// name neither.
pub(crate) fn default_params(s: &Session, p: &Value) -> Option<Value> {
  if p.get("body").is_some() || p.get("face").is_some() {
    return None;
  }
  let def = s.design.flat_pattern.as_ref()?;
  let body = s.model.body(def.body)?;
  let face = def.face.resolve(body.faces(), body.mesh(), body.diagonal())?;
  let mut q = p.clone();
  q["body"] = json!(def.body.0);
  q["face"] = json!({"body": def.body.0, "face": face});
  Some(q)
}

/// What the flat pattern environment shows (its browser's Bend Lines folder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatShow {
  pub center_lines: bool,
  pub extent_lines: bool,
}

/// The flat pattern as the canvas draws it in its environment: laid in the Front (XZ) plane,
/// the stationary face towards the viewer, with its edges and bend lines.
#[derive(Debug, Clone)]
pub(crate) struct FlatDraw {
  pub body: fox_model::BodyId,
  pub mesh: fox_kernel::TriMesh,
  pub edges: Vec<Vec<[f32; 3]>>,
  pub center: Vec<[[f32; 3]; 2]>,
  pub extent: Vec<[[f32; 3]; 2]>,
}

/// The bend centre lines' colour (dash-dot) and the extent lines' (thin).
pub(crate) const CENTER_LINE_COLOR: [u8; 4] = [70, 110, 190, 255];
pub(crate) const EXTENT_LINE_COLOR: [u8; 4] = [120, 120, 120, 255];

impl FlatDraw {
  pub(crate) fn of(s: &Session) -> Option<FlatDraw> {
    let (body, flat) = design_flat(s)?;
    let pl = flat.plane;
    let n = pl.normal();
    // stationary plane (u, v) -> (u, 0, v); the material behind it, at +y
    let map = |p: fox_kernel::glam::DVec3| {
      let [u, v] = pl.to_local(p);
      [u as f32, -(p - pl.origin).dot(n) as f32, v as f32]
    };
    let dir = |d: [f32; 3]| {
      let d = fox_kernel::glam::DVec3::new(f64::from(d[0]), f64::from(d[1]), f64::from(d[2]));
      [d.dot(pl.x) as f32, -d.dot(n) as f32, d.dot(pl.y) as f32]
    };
    let mut mesh = flat.shape.tessellate(0.02).ok()?;
    for q in mesh.positions.iter_mut() {
      *q = map(fox_kernel::glam::Vec3::from(*q).as_dvec3());
    }
    for q in mesh.normals.iter_mut() {
      *q = dir(*q);
    }
    // (u, v, n) -> (u, -n, v) flips handedness: turn the triangles back round
    for t in mesh.indices.chunks_mut(3) {
      t.swap(1, 2);
    }
    let edges =
      flat.shape.edge_lines(0.02).ok()?.into_iter().map(|e| e.points.into_iter().map(map).collect()).collect();
    let center = flat.bends.iter().map(|b| [map(b.start), map(b.end)]).collect();
    let sheet = s.model.body(body)?.sheet.as_ref()?;
    let extent = fox_model::sheet::flat_extent_lines(&flat, sheet).into_iter().map(|(a, b)| [map(a), map(b)]).collect();
    Some(FlatDraw { body, mesh, edges, center, extent })
  }
}

/// The box the flat pattern environment draws the flat in, for framing it.
pub fn flat_bounds(s: &Session) -> Option<(fox_kernel::glam::DVec3, fox_kernel::glam::DVec3)> {
  let d = FlatDraw::of(s)?;
  let mut lo = fox_kernel::glam::DVec3::splat(f64::INFINITY);
  let mut hi = -lo;
  for q in &d.mesh.positions {
    let q = fox_kernel::glam::Vec3::from(*q).as_dvec3();
    lo = lo.min(q);
    hi = hi.max(q);
  }
  lo.is_finite().then_some((lo, hi))
}
