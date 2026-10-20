//! Commands for the second batch of solid features: emboss/deboss, boundary fill and replace
//! face, and the path references (guide rails, loft rails and centre lines) sweep and loft take.

use fox_kernel::Shape;
use fox_kernel::glam::DVec3;
use fox_model::bounding::BoundShape;
use fox_model::{BodyId, BodySel, FaceRef, FeatureId, FeatureKind, PathRef, PlaneRef};
use fox_sketch::CurveId;
use serde_json::{Value, json};

use crate::commands::{
  add_feature, body_sel, expr_of, face_ref, ids, operation, plane_ref, profile_points, req_expr, sketch_for_feature,
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
    "bounding_solid" => (|| {
      let bodies = body_sel(p);
      if let BodySel::Bodies { ids } = &bodies {
        for b in ids {
          s.model.body(*b).ok_or(CmdError::new(format!("no body {}", b.0)))?;
        }
      }
      let shape = match p.get("shape").and_then(Value::as_str).unwrap_or("box") {
        "box" => BoundShape::Box,
        "cylinder" => BoundShape::Cylinder,
        o => return Err(CmdError::new(format!("unknown shape '{o}' (box, cylinder)"))),
      };
      let plane = p.get("plane").map(|v| plane_ref(s, v)).transpose()?;
      let offset = p.get("offset").map(expr_of).transpose()?.unwrap_or_else(|| "0".into());
      let sides = match p.get("sides") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) if a.len() == 6 => a.iter().map(expr_of).collect::<Result<_, _>>()?,
        Some(_) => return Err(CmdError::new("'sides' is six offsets: [-x, +x, -y, +y, -z, +z]")),
      };
      add_feature(s, FeatureKind::BoundingSolid { bodies, shape, plane, offset, sides })
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
