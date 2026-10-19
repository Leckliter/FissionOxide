//! Commands for lofted and miter flanges and flat patterns as sketches.

use fox_model::{CornerMode, CornerRelief, FeatureKind, LoftMethod, LoftProfile, OriginPlane, PlaneRef};
use fox_sketch::CurveId;
use serde_json::{Value, json};

use crate::commands::{
  add_feature, edge_refs, face_ref, ids, opt_expr, opt_str, rule_name, sheet_body, sketch_for_feature,
};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "sheet.lofted_flange" => (|| {
      let (from, to) = (loft_profile(s, p, "from")?, loft_profile(s, p, "to")?);
      let method = match opt_str(p, "method").unwrap_or("die_formed") {
        "die_formed" => LoftMethod::DieFormed,
        "press_brake" => LoftMethod::PressBrake,
        o => return Err(CmdError::new(format!("unknown method '{o}' (die_formed, press_brake)"))),
      };
      let rule = rule_name(s, p)?;
      add_feature(
        s,
        FeatureKind::LoftedFlange {
          from,
          to,
          method,
          bends: opt_expr(p, "bends").unwrap_or_else(|| "8".into()),
          flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
          rule,
        },
      )
    })(),
    "sheet.miter_flange" => (|| {
      let edges = edge_refs(s, p)?;
      let sketch = sketch_for_feature(s, p)?;
      let curves = ids(p, "curves").into_iter().map(CurveId).collect();
      add_feature(s, FeatureKind::MiterFlange { sketch, curves, edges, corners: corner_mode(p)? })
    })(),
    "sheet.flat_sketch" => flat_sketch(s, p),
    "sheet.corner_closure" => corner_closure(s, p),
    _ => return None,
  })
}

/// `corners`: miter (default) | open | closed | overlap.
pub(crate) fn corner_mode(p: &Value) -> Result<CornerMode, CmdError> {
  Ok(match opt_str(p, "corners").unwrap_or("miter") {
    "miter" => CornerMode::Miter,
    "open" => CornerMode::Open,
    "closed" => CornerMode::Closed,
    "overlap" => CornerMode::Overlap,
    o => return Err(CmdError::new(format!("unknown corner mode '{o}' (miter, open, closed, overlap)"))),
  })
}

/// Changes how an edge or miter flange (default the last one) closes its corners, and the
/// corner relief of a sheet metal rule (default the default rule).
fn corner_closure(s: &mut Session, p: &Value) -> CmdResult {
  let id = match p.get("feature").and_then(Value::as_u64) {
    Some(f) => fox_model::FeatureId(u32::try_from(f).map_err(|_| CmdError::new("bad feature id"))?),
    None => s
      .design
      .features
      .iter()
      .take(s.design.active_len())
      .rev()
      .find(|f| matches!(f.kind, FeatureKind::EdgeFlange { .. } | FeatureKind::MiterFlange { .. }))
      .map(|f| f.id)
      .ok_or(CmdError::new("there is no edge or miter flange"))?,
  };
  let mode = p.get("corners").map(|_| corner_mode(p)).transpose()?;
  let relief = match opt_str(p, "relief") {
    None => None,
    Some("none") => Some(CornerRelief::None),
    Some("round") => Some(CornerRelief::Round),
    Some("square") => Some(CornerRelief::Square),
    Some("tear") => Some(CornerRelief::Tear),
    Some(o) => return Err(CmdError::new(format!("unknown corner relief '{o}' (none, round, square, tear)"))),
  };
  let size = opt_expr(p, "relief_size");
  let rule = rule_name(s, p)?;
  s.edit(|d| {
    let f = d.feature_mut(id).ok_or(CmdError::new(format!("no feature {}", id.0)))?;
    match (&mut f.kind, mode) {
      (FeatureKind::EdgeFlange { corners, .. } | FeatureKind::MiterFlange { corners, .. }, Some(m)) => *corners = m,
      (FeatureKind::EdgeFlange { .. } | FeatureKind::MiterFlange { .. }, None) => {}
      _ => return Err(CmdError::new("the feature is not an edge or miter flange")),
    }
    if relief.is_some() || p.get("relief_size").is_some() {
      let mut r = d.sheet_rule(&rule).unwrap_or_default();
      if let Some(x) = relief {
        r.corner_relief = x;
      }
      if p.get("relief_size").is_some() {
        r.corner_relief_size = size.clone().unwrap_or_default();
      }
      d.set_sheet_rule(r)?;
    }
    Ok(())
  })?;
  let st = s.model.status(id);
  Ok(json!({"feature": id.0, "error": st.and_then(|x| x.error.clone()), "warning": st.and_then(|x| x.warning.clone())}))
}

/// A lofted flange profile: a sketch id, or {sketch, curves?}.
fn loft_profile(s: &Session, p: &Value, k: &str) -> Result<LoftProfile, CmdError> {
  let v = p.get(k).ok_or(CmdError::new(format!("missing '{k}'")))?;
  let id = v
    .as_u64()
    .or_else(|| v.get("sketch").and_then(Value::as_u64))
    .ok_or(CmdError::new(format!("'{k}' needs a sketch")))?;
  let sketch = fox_model::FeatureId(u32::try_from(id).map_err(|_| CmdError::new("bad sketch id"))?);
  s.design.sketch(sketch)?;
  Ok(LoftProfile { sketch, curves: ids(v, "curves").into_iter().map(CurveId).collect() })
}

/// The flat pattern as a new sketch: on the stationary face, or on XY with its lower-left corner
/// at the origin (`on: "xy"`, and always for lofted flanges, which have no stationary face).
fn flat_sketch(s: &mut Session, p: &Value) -> CmdResult {
  // forms pass the picked bodies as a list
  let mut p = p.clone();
  if let Some(first) = p.get("body").and_then(Value::as_array).map(|a| a.first().cloned()) {
    match first {
      Some(b) => p["body"] = b,
      None => {
        if let Some(o) = p.as_object_mut() {
          o.remove("body");
        }
      }
    }
  }
  let p = &p;
  let id = sheet_body(s, p)?;
  let face = p.get("face").map(|f| face_ref(s, f)).transpose()?;
  if face.as_ref().is_some_and(|f| f.body != id) {
    return Err(CmdError::new("the stationary face must be on the body"));
  }
  let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
  let fid = match &face {
    Some(r) => Some(r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?),
    None => fox_model::sheet::default_face(body.faces()),
  };
  let flat = body.flat_pattern(fid).map_err(CmdError::new)?;
  let developed = body.sheet.as_ref().is_some_and(|i| i.developed.is_some());
  let on_face = match opt_str(p, "on").unwrap_or("face") {
    "face" => !developed && fid.is_some(),
    "xy" => false,
    o => return Err(CmdError::new(format!("unknown placement '{o}' (face, xy)"))),
  };
  let (plane, sketch) = if let (true, Some(fid)) = (on_face, fid) {
    let info = *body.faces().iter().find(|f| f.id == fid).ok_or(CmdError::new("no such face"))?;
    let pl = fox_model::regen::sketch_plane_on(info.normal, info.origin).map_err(CmdError::new)?;
    let sk = fox_model::sheet2::flat_sketch(&flat, &pl, [0.0, 0.0]).map_err(CmdError::new)?;
    (PlaneRef::Face { face: face_ref(s, &json!({"body": id.0, "face": fid}))? }, sk)
  } else {
    let sk = fox_model::sheet2::flat_sketch(&flat, &flat.plane, [0.0, 0.0]).map_err(CmdError::new)?;
    let (lo, _) = sk.bounds().ok_or(CmdError::new("the flat pattern is empty"))?;
    let sk = fox_model::sheet2::flat_sketch(&flat, &flat.plane, [-lo[0], -lo[1]]).map_err(CmdError::new)?;
    (PlaneRef::Origin { plane: OriginPlane::Xy }, sk)
  };
  let (curves, bends) = (sketch.curves.len(), flat.bends.len());
  let mut out = add_feature(s, FeatureKind::Sketch { plane, sketch, links: Vec::new() })?;
  out["body"] = json!(id.0);
  out["curves"] = json!(curves);
  out["bends"] = json!(bends);
  Ok(out)
}
