//! Commands for the Surface workspace's extend, untrim, trim with a body or sketch, and
//! offsets with a distance per face.

use fox_model::surface::{ExtendMode, FaceDistance, TrimTool, UntrimLoops};
use fox_model::{BodyId, FeatureId, FeatureKind};
use fox_sketch::CurveId;
use serde_json::Value;

use crate::commands::{add_feature, edge_ref_of, expr_of, face_ref, ids, opt_expr, opt_str, req_expr, req_u32};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "surface.extend" => (|| {
      let mut edges = Vec::new();
      for e in p.get("edges").and_then(Value::as_array).ok_or(CmdError::new("missing 'edges'"))? {
        edges.push(edge_ref_of(s, e)?);
      }
      if edges.is_empty() {
        return Err(CmdError::new("no edges given"));
      }
      let mode = match opt_str(p, "mode").unwrap_or("natural") {
        "natural" => ExtendMode::Natural,
        "tangent" => ExtendMode::Tangent,
        "perpendicular" | "normal" => ExtendMode::Perpendicular,
        o => return Err(CmdError::new(format!("unknown mode '{o}' (natural, tangent, perpendicular)"))),
      };
      let close_corners = p.get("close_corners").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::SurfaceExtend { edges, distance: req_expr(p, "distance")?, mode, close_corners })
    })(),
    "surface.untrim" => (|| {
      let mut faces = Vec::new();
      for f in p.get("faces").and_then(Value::as_array).into_iter().flatten() {
        faces.push(face_ref(s, f)?);
      }
      let bodies = bodies(s, p, "bodies")?;
      if faces.is_empty() && bodies.is_empty() {
        return Err(CmdError::new("pass the 'faces' or 'bodies' to untrim"));
      }
      let loops = match opt_str(p, "loops").unwrap_or("all") {
        "all" => UntrimLoops::All,
        "inner" | "holes" => UntrimLoops::Inner,
        "outer" => UntrimLoops::Outer,
        o => return Err(CmdError::new(format!("unknown loops '{o}' (all, inner, outer)"))),
      };
      let extension = opt_expr(p, "extension").unwrap_or_default();
      add_feature(s, FeatureKind::Untrim { faces, bodies, loops, extension })
    })(),
    "surface.stitch.gaps" => (|| {
      let bodies = bodies(s, p, "bodies")?;
      let max = p.get("max_gap").and_then(Value::as_f64).unwrap_or(1.0);
      let open = fox_model::surface::open_edges(&s.model, &bodies, max);
      Ok(serde_json::json!(
        open.iter().map(|o| serde_json::json!({"body": o.body.0, "edge": o.edge, "gap": o.gap})).collect::<Vec<_>>()
      ))
    })(),
    // a plane tool stays with the plane trim (features.rs)
    "surface.trim" if trim_tool_kind(p).is_some() => trim_with(s, p),
    _ => return None,
  })
}

/// Whether `tool` names a body ({"body": id}) or a sketch ({"sketch": id, "curves"?}).
fn trim_tool_kind(p: &Value) -> Option<&'static str> {
  let t = p.get("tool")?.as_object()?;
  if t.contains_key("sketch") {
    Some("sketch")
  } else if t.contains_key("body") && !t.contains_key("face") {
    Some("body")
  } else {
    None
  }
}

/// Fusion's Trim (`cut_only`): only the surfaces the tool cuts change. With no surfaces to cut
/// at all, the tool cuts none of them, which is Fusion's error.
pub(crate) fn cut_only(p: &Value) -> Result<bool, CmdError> {
  let on = p.get("cut_only").and_then(Value::as_bool).unwrap_or(false);
  if on && ids(p, "bodies").is_empty() {
    return Err(CmdError::new("Cutting Tool does not intersect any surface!"));
  }
  Ok(on)
}

fn trim_with(s: &mut Session, p: &Value) -> CmdResult {
  let cut_only = cut_only(p)?;
  let targets = bodies(s, p, "bodies")?;
  if targets.is_empty() {
    return Err(CmdError::new("missing 'bodies'"));
  }
  let t = p.get("tool").ok_or(CmdError::new("missing 'tool'"))?;
  let tool = if trim_tool_kind(p) == Some("sketch") {
    let sketch = FeatureId(req_u32(t, "sketch")?);
    s.design.sketch(sketch)?;
    s.edit_sketch = None;
    TrimTool::Sketch { sketch, curves: ids(t, "curves").into_iter().map(CurveId).collect() }
  } else {
    let body = BodyId(req_u32(t, "body")?);
    if s.model.body(body).is_none() {
      return Err(CmdError::new(format!("no body {}", body.0)));
    }
    if targets.contains(&body) {
      return Err(CmdError::new("the trimming tool can't be one of the bodies it trims"));
    }
    TrimTool::Body { body }
  };
  let (keep, remove) = (points(p, "keep")?, points(p, "remove")?);
  let flip = p.get("flip").and_then(Value::as_bool).unwrap_or(false);
  add_feature(s, FeatureKind::TrimWith { bodies: targets, tool, flip, keep, remove, cut_only })
}

/// `face_distances`: [{face, distance}] for a variable offset.
pub(crate) fn face_distances(s: &Session, p: &Value) -> Result<Vec<FaceDistance>, CmdError> {
  let mut out = Vec::new();
  for fd in p.get("face_distances").and_then(Value::as_array).into_iter().flatten() {
    let face = face_ref(s, fd.get("face").ok_or(CmdError::new("each face distance needs a 'face'"))?)?;
    let distance = expr_of(fd.get("distance").ok_or(CmdError::new("each face distance needs a 'distance'"))?)?;
    out.push(FaceDistance { face, distance });
  }
  Ok(out)
}

fn bodies(s: &Session, p: &Value, k: &str) -> Result<Vec<BodyId>, CmdError> {
  let out: Vec<BodyId> = ids(p, k).into_iter().map(BodyId).collect();
  for b in &out {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  Ok(out)
}

/// `[[x, y, z], ...]` under `k` (none: empty).
pub(crate) fn points(p: &Value, k: &str) -> Result<Vec<[f64; 3]>, CmdError> {
  let mut out = Vec::new();
  for v in p.get(k).and_then(Value::as_array).into_iter().flatten() {
    let nums: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    match nums.as_slice() {
      [x, y, z] if nums.iter().all(|c| c.is_finite()) => out.push([*x, *y, *z]),
      _ => return Err(CmdError::new(format!("'{k}' points must be [x, y, z]"))),
    }
  }
  Ok(out)
}
