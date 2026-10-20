//! Commands for the second batch of solid features: emboss/deboss, boundary fill and replace
//! face, and the path references (guide rails, loft rails and centre lines) sweep and loft take.

use fox_kernel::Shape;
use fox_kernel::glam::DVec3;
use fox_model::{BodyId, FaceRef, FeatureId, FeatureKind, LoftEnd, PathRef, PlaneRef, ShellDirection};
use fox_sketch::CurveId;
use serde_json::{Value, json};

use crate::commands::{
  add_feature, face_ref, ids, operation, opt_expr, plane_ref, profile_points, req_expr, sketch_for_feature,
};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "emboss" | "deboss" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sketch, p)?;
      let face = face_ref(s, p.get("face").ok_or(CmdError::new("missing 'face' (the face to emboss)"))?)?;
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      s.edit_sketch = None;
      add_feature(
        s,
        FeatureKind::Emboss {
          sketch,
          profiles,
          face,
          depth: req_expr(p, "depth")?,
          deboss: name == "deboss" || flag("deboss"),
          wrap: flag("wrap"),
          horizontal: opt_expr(p, "horizontal").unwrap_or_else(|| "0".into()),
          vertical: opt_expr(p, "vertical").unwrap_or_else(|| "0".into()),
          rotation: opt_expr(p, "rotation").unwrap_or_else(|| "0".into()),
        },
      )
    })(),
    "boundary_fill" => (|| {
      let (tools, planes) = fill_tools(s, p)?;
      add_feature(
        s,
        FeatureKind::BoundaryFill {
          tools,
          planes,
          cells: points(p, "cells")?,
          operation: operation(p)?,
          remove_tools: p.get("remove_tools").and_then(Value::as_bool).unwrap_or(false),
        },
      )
    })(),
    "boundary_fill.cells" => (|| {
      let (tools, planes) = fill_tools(s, p)?;
      cells(s, &tools, &planes)
    })(),
    "replace_face" => (|| {
      let mut faces = Vec::new();
      for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
        faces.push(face_ref(s, f)?);
      }
      if faces.is_empty() {
        return Err(CmdError::new("no faces given"));
      }
      let target = p.get("target").map(|v| face_ref(s, v)).transpose()?;
      let plane = p.get("plane").map(|v| plane_ref(s, v)).transpose()?;
      if target.is_none() && plane.is_none() {
        return Err(CmdError::new("missing 'target' (a face) or 'plane'"));
      }
      add_feature(s, FeatureKind::ReplaceFace { faces, target, plane })
    })(),
    _ => return None,
  })
}

/// A path: a sketch id (every curve) or {sketch, curves?: [id]}.
pub(crate) fn path_ref(s: &Session, v: &Value) -> Result<PathRef, CmdError> {
  let (sketch, curves) = match v.as_u64() {
    Some(id) => (id, Vec::new()),
    None => (
      v.get("sketch").and_then(Value::as_u64).ok_or(CmdError::new("a path is a sketch id or {sketch, curves?}"))?,
      ids(v, "curves").into_iter().map(CurveId).collect(),
    ),
  };
  let sketch = FeatureId(u32::try_from(sketch).map_err(|_| CmdError::new("bad sketch id"))?);
  s.design.sketch(sketch)?;
  Ok(PathRef { sketch, curves })
}

/// A loft section's end condition: `end_condition` (free, point_sharp or point_tangent, with
/// `weight`, default 1) on a `{sketch, ...}` section; a bare sketch id is free.
pub(crate) fn loft_end(v: &Value) -> Result<LoftEnd, CmdError> {
  let Some(kind) = v.get("end_condition") else { return Ok(LoftEnd::Free) };
  let weight = v.get("weight").map(|w| w.as_f64().ok_or(CmdError::new("'weight' must be a number"))).transpose()?;
  match kind.as_str() {
    Some("free") => Ok(LoftEnd::Free),
    Some("point_sharp") => Ok(LoftEnd::PointSharp),
    Some("point_tangent") => {
      let weight = weight.unwrap_or(1.0);
      if !(weight > 0.0 && weight.is_finite()) {
        return Err(CmdError::new("a point tangent weight must be above zero"));
      }
      Ok(LoftEnd::PointTangent { weight })
    }
    _ => Err(CmdError::new("end_condition is free, point_sharp or point_tangent")),
  }
}

/// A face reference standing for a whole body (`extrude` to a body): {body} alone uses its first face.
pub(crate) fn body_face_ref(s: &Session, v: &Value) -> Result<FaceRef, CmdError> {
  if v.get("face").is_some() || v.get("point").is_some() {
    return face_ref(s, v);
  }
  let id = v.get("body").and_then(Value::as_u64).or(v.as_u64()).ok_or(CmdError::new("'to' needs a body"))?;
  let body = BodyId(u32::try_from(id).map_err(|_| CmdError::new("bad body id"))?);
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let f = b.faces().first().ok_or(CmdError::new("the body has no faces"))?;
  Ok(FaceRef::from_face(body, f, f.center))
}

fn points(p: &Value, k: &str) -> Result<Vec<[f64; 3]>, CmdError> {
  let Some(list) = p.get(k).and_then(Value::as_array) else { return Ok(Vec::new()) };
  list
    .iter()
    .map(|v| match v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()) {
      Some(c) if c.len() == 3 => Ok([c[0], c[1], c[2]]),
      _ => Err(CmdError::new(format!("'{k}' are [x, y, z] points"))),
    })
    .collect()
}

fn fill_tools(s: &Session, p: &Value) -> Result<(Vec<BodyId>, Vec<PlaneRef>), CmdError> {
  let tools: Vec<BodyId> = ids(p, "tools").into_iter().map(BodyId).collect();
  for b in &tools {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  let mut planes = Vec::new();
  let list = match p.get("planes") {
    Some(Value::Array(a)) => a.iter().collect(),
    Some(v) if !v.is_null() => vec![v],
    _ => Vec::new(),
  };
  for v in list {
    planes.push(plane_ref(s, v)?);
  }
  if tools.is_empty() {
    return Err(CmdError::new("missing 'tools' (the bodies that bound the cells)"));
  }
  Ok((tools, planes))
}

/// The cells the tools bound, each with a point inside it (to pass as `cells`) and its volume.
fn cells(s: &Session, tools: &[BodyId], planes: &[PlaneRef]) -> CmdResult {
  let all = fox_model::regen::boundary_cells(&s.model, tools, planes).map_err(CmdError::new)?;
  let mut out = Vec::new();
  for (i, c) in all.iter().enumerate() {
    let mp = c.mass().map_err(|e| CmdError::new(e.to_string()))?;
    let at = inner_point(c, mp.center_of_mass, mp.bbox_min, mp.bbox_max);
    out.push(json!({"cell": i, "point": at.to_array(), "volume": mp.volume}));
  }
  Ok(json!({"cells": out}))
}

/// A point inside a solid: its centre of mass, or a probe of its bounding box when that lies outside.
fn inner_point(s: &Shape, com: DVec3, lo: DVec3, hi: DVec3) -> DVec3 {
  let inside = |p: DVec3| s.classify(p).is_ok_and(|w| w == fox_kernel::Inside::In);
  if inside(com) {
    return com;
  }
  let n = 8;
  for i in 1..n {
    for j in 1..n {
      for k in 1..n {
        let t = DVec3::new(i as f64, j as f64, k as f64) / n as f64;
        let p = lo + (hi - lo) * t;
        if inside(p) {
          return p;
        }
      }
    }
  }
  com
}

/// `shell`: faces to open (with `tangent_chain`, the faces that continue them smoothly too) and
/// whole bodies hollowed closed; walls inside, outside or on both sides of the surface.
pub(crate) fn shell(s: &mut Session, p: &Value) -> CmdResult {
  let mut picked: Vec<Value> = p.get("faces").and_then(Value::as_array).cloned().unwrap_or_default();
  let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if picked.is_empty() && bodies.is_empty() {
    return Err(CmdError::new("missing 'faces' (or 'bodies' to hollow whole)"));
  }
  for b in &bodies {
    s.model.body(*b).ok_or(CmdError::new(format!("no body {}", b.0)))?;
  }
  if p.get("tangent_chain").and_then(Value::as_bool).unwrap_or(false) {
    picked = tangent_faces(s, picked);
  }
  let mut faces = Vec::new();
  for f in &picked {
    faces.push(face_ref(s, f)?);
  }
  let round = match p.get("corners").and_then(Value::as_str) {
    None | Some("sharp") => false,
    Some("round") => true,
    Some(o) => return Err(CmdError::new(format!("unknown corners '{o}' (sharp or round)"))),
  };
  let direction = match p.get("direction").and_then(Value::as_str) {
    None | Some("inside") => ShellDirection::Inside,
    Some("outside") => ShellDirection::Outside,
    Some("both") => ShellDirection::Both,
    Some(o) => return Err(CmdError::new(format!("unknown direction '{o}' (inside, outside or both)"))),
  };
  let thickness_out = match direction {
    ShellDirection::Both if p.get("outside_thickness").is_some() => req_expr(p, "outside_thickness")?,
    _ => String::new(),
  };
  let thickness = req_expr(p, "thickness")?;
  add_feature(s, FeatureKind::Shell { faces, thickness, round, direction, thickness_out, bodies })
}

/// The picked `{body, face}` faces and every face joined to them across a smooth edge (Fusion's
/// tangent chain), e.g. a box's side faces and the rounds between them. Other picks pass through.
fn tangent_faces(s: &Session, picked: Vec<Value>) -> Vec<Value> {
  let mut out = picked.clone();
  for v in &picked {
    let (Some(b), Some(f)) = (v.get("body").and_then(Value::as_u64), v.get("face").and_then(Value::as_u64)) else {
      continue;
    };
    let Some(body) = s.model.body(BodyId(b as u32)) else { continue };
    let (faces, edges) = (body.faces(), body.edges());
    let mut todo = vec![f as u32];
    while let Some(cur) = todo.pop() {
      for e in edges.iter().filter(|e| e.faces.contains(&cur)) {
        let other = if e.faces[0] == cur { e.faces[1] } else { e.faces[0] };
        let next = json!({"body": b, "face": other});
        if other == 0 || out.contains(&next) {
          continue;
        }
        let normal = |id: u32| faces.iter().find(|x| x.id == id).and_then(|x| normal_at(x, e.mid));
        if let (Some(n1), Some(n2)) = (normal(cur), normal(other))
          && n1.dot(n2).abs() > 1.0f64.to_radians().cos()
        {
          out.push(next);
          todo.push(other);
        }
      }
    }
  }
  out
}

/// A face's normal direction (either sense) at a point on it: planes, cylinders and spheres.
fn normal_at(f: &fox_kernel::FaceInfo, p: DVec3) -> Option<DVec3> {
  use fox_kernel::SurfaceKind as K;
  let n = match f.kind {
    K::Plane => f.normal,
    K::Cylinder => {
      let r = p - f.origin;
      r - f.normal * r.dot(f.normal)
    }
    K::Sphere => p - f.center,
    _ => return None,
  };
  n.try_normalize()
}
