//! Commands for lofted and miter flanges and flat patterns as sketches.

use fox_model::{
  CornerMode, CornerRelief, FeatureKind, JoinBendPosition, JoinRelief, LoftMethod, LoftProfile, OriginPlane, PlaneRef,
};
use fox_sketch::CurveId;
use serde_json::{Value, json};

use crate::commands::{
  add_feature, edge_ref_of, edge_refs, face_ref, ids, opt_expr, opt_str, req_expr, rule_name, sheet_body,
  sketch_for_feature,
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
          rounded: p.get("rounded").and_then(Value::as_bool).unwrap_or(false),
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
    "sheet.check_dxf" => check_dxf(s, p),
    "sheet.corner_closure" => corner_closure(s, p),
    "sheet.join_by_bend" => join_by_bend(s, p),
    "sheet.fillet" => (|| {
      let edges = edge_refs(s, p)?;
      add_feature(s, FeatureKind::SheetFillet { edges, radius: req_expr(p, "radius")? })
    })(),
    "sheet.chamfer" => (|| {
      let edges = edge_refs(s, p)?;
      let distance = req_expr(p, "distance")?;
      let distance2 = match opt_str(p, "chamfer_type").unwrap_or("equal") {
        "equal" => None,
        "two_distances" => Some(req_expr(p, "distance2")?),
        o => return Err(CmdError::new(format!("unknown chamfer type '{o}' (equal, two_distances)"))),
      };
      let flipped = p.get("flipped").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::SheetChamfer { edges, distance, distance2, flipped })
    })(),
    _ => return None,
  })
}

/// Joins two sheet metal bodies with a bend between an edge picked on a flange of each.
fn join_by_bend(s: &mut Session, p: &Value) -> CmdResult {
  // forms pass the two picks as `edge1` and `edge2`; `edges: [a, b]` works too
  let pick = |k: &str, i: usize| p.get(k).or_else(|| p.get("edges").and_then(|e| e.get(i)));
  let (Some(a), Some(b)) = (pick("edge1", 0), pick("edge2", 1)) else {
    return Err(CmdError::new("pick an edge on each of the two flanges"));
  };
  let (edge1, edge2) = (edge_ref_of(s, a)?, edge_ref_of(s, b)?);
  let position = match opt_str(p, "position").unwrap_or("tangent") {
    "tangent" => JoinBendPosition::Tangent,
    "first" => JoinBendPosition::First,
    "second" => JoinBendPosition::Second,
    o => return Err(CmdError::new(format!("unknown bend position '{o}' (tangent, first, second)"))),
  };
  let relief = match opt_str(p, "relief").unwrap_or("rule") {
    "rule" => JoinRelief::Rule,
    "rectangular" => JoinRelief::Rectangular,
    "round" => JoinRelief::Round,
    "none" => JoinRelief::None,
    o => return Err(CmdError::new(format!("unknown bend relief '{o}' (rule, rectangular, round, none)"))),
  };
  let bend_radius = opt_expr(p, "bend_radius").unwrap_or_default();
  add_feature(s, FeatureKind::JoinByBend { edge1, edge2, bend_radius, position, relief })
}

/// `corners`: miter (default) | open | closed | overlap | butt.
pub(crate) fn corner_mode(p: &Value) -> Result<CornerMode, CmdError> {
  Ok(match opt_str(p, "corners").unwrap_or("miter") {
    "miter" => CornerMode::Miter,
    "open" => CornerMode::Open,
    "closed" => CornerMode::Closed,
    "overlap" => CornerMode::Overlap,
    "butt" => CornerMode::Butt,
    o => return Err(CmdError::new(format!("unknown corner mode '{o}' (miter, open, closed, overlap, butt)"))),
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
/// at the origin (`on: "xy"`, and always for lofted flanges, which have no stationary face). Its
/// curves follow the body as the model changes (fixed, as projected geometry), unless `follow`
/// is false: then they are free curves made once.
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
  // `follow: false`, or (from the form) `curves: "once"`, makes free curves once
  let follow = match (p.get("follow").and_then(Value::as_bool), opt_str(p, "curves")) {
    (Some(f), _) => f,
    (None, None | Some("follow")) => true,
    (None, Some("once")) => false,
    (None, Some(o)) => return Err(CmdError::new(format!("unknown curves '{o}' (follow, once)"))),
  };
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
  let picked = |fid: u32| body.faces().iter().find(|f| f.id == fid).copied().ok_or(CmdError::new("no such face"));
  let (plane, sketch) = if let (true, Some(fid)) = (on_face, fid) {
    let info = picked(fid)?;
    let plane = PlaneRef::Face { face: face_ref(s, &json!({"body": id.0, "face": fid}))?, x: None };
    let sketch = if follow {
      let mut sk = fox_sketch::Sketch::new();
      sk.add_flat(id.0, false, None);
      sk
    } else {
      let pl = fox_model::regen::sketch_plane_on(info.normal, info.origin, s.design.frames()).map_err(CmdError::new)?;
      fox_model::sheet2::flat_sketch(&flat, &pl, [0.0, 0.0]).map_err(CmdError::new)?
    };
    (plane, sketch)
  } else {
    let sketch = if follow {
      // the stationary face, when one was picked, as a point on it and its normal
      let pick = match (&face, fid) {
        (Some(_), Some(fid)) if !developed => {
          let info = picked(fid)?;
          Some((info.center.to_array(), info.normal.to_array()))
        }
        _ => None,
      };
      let mut sk = fox_sketch::Sketch::new();
      sk.add_flat(id.0, true, pick);
      sk
    } else {
      let sk = fox_model::sheet2::flat_sketch(&flat, &flat.plane, [0.0, 0.0]).map_err(CmdError::new)?;
      let (lo, _) = sk.bounds().ok_or(CmdError::new("the flat pattern is empty"))?;
      fox_model::sheet2::flat_sketch(&flat, &flat.plane, [-lo[0], -lo[1]]).map_err(CmdError::new)?
    };
    (PlaneRef::Origin { plane: OriginPlane::Xy }, sketch)
  };
  let bends = flat.bends.len();
  let mut out = add_feature(s, FeatureKind::Sketch { plane, sketch, links: Vec::new() })?;
  let sk = out["feature"].as_u64().map(|f| fox_model::FeatureId(f as u32));
  let curves = sk.and_then(|f| s.model.sketches.get(&f)).map_or(0, |r| r.sketch.curves.len());
  out["body"] = json!(id.0);
  out["curves"] = json!(curves);
  out["bends"] = json!(bends);
  out["follows"] = json!(follow);
  Ok(out)
}

/// A body's flat pattern DXF: layered by role, with bend notes when the body has sheet settings.
pub(crate) fn flat_dxf(
  body: &fox_model::regen::Body,
  flat: &fox_model::sheet::FlatPattern,
) -> Result<String, CmdError> {
  match &body.sheet {
    Some(sheet) => fox_model::sheet::flat_dxf_for(flat, sheet),
    None => fox_model::sheet::flat_dxf(flat),
  }
  .map_err(CmdError::new)
}

/// [`flat_dxf`] with Export Flat Pattern's options.
pub(crate) fn flat_dxf_with(
  body: &fox_model::regen::Body,
  flat: &fox_model::sheet::FlatPattern,
  opts: &fox_model::sheet::FlatDxfOptions,
) -> Result<String, CmdError> {
  fox_model::sheet::flat_dxf_with(flat, body.sheet.as_ref(), opts).map_err(CmdError::new)
}

/// Checks a flat pattern DXF file (`path`), or a sheet metal body's flat pattern, against shop
/// rules. The rules default to the body's sheet (or the file's `SHEET` note); `min_feature` and
/// `hole_to_bend` override them.
fn check_dxf(s: &Session, p: &Value) -> CmdResult {
  use fox_exchange::flatcheck::{self, Rules};
  let len = |k: &str| -> Result<Option<f64>, CmdError> {
    match p.get(k) {
      None => Ok(None),
      Some(v) => {
        let e = v.as_f64().map(|x| x.to_string()).or_else(|| v.as_str().map(str::to_string)).unwrap_or_default();
        Ok(Some(fox_expr::eval(&e, fox_expr::Kind::Length, &s.model.params)?))
      }
    }
  };
  let (min_feature, hole_to_bend) = (len("min_feature")?, len("hole_to_bend")?);
  let (bytes, body, sheet) = match opt_str(p, "path") {
    Some(path) => (std::fs::read(path)?, None, None),
    None => {
      let id = sheet_body(s, p)?;
      let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
      let face = match p.get("face") {
        Some(f) => {
          let r = face_ref(s, f)?;
          Some(r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?)
        }
        None => None,
      };
      let flat = body.flat_pattern(face).map_err(CmdError::new)?;
      (flat_dxf(body, &flat)?.into_bytes(), Some(id), body.sheet.clone())
    }
  };
  let layered = flatcheck::read_layered(&bytes).map_err(|e| CmdError::new(e.to_string()))?;
  let mut rules = match (&sheet, flatcheck::sheet_note(&layered.texts)) {
    (Some(sh), _) => Rules::for_sheet(sh.thickness, sh.bend_radius),
    (None, Some((t, r))) => Rules::for_sheet(t, r),
    (None, None) if min_feature.is_some() || hole_to_bend.is_some() => Rules::for_sheet(1.0, 1.0),
    (None, None) => {
      return Err(CmdError::new("the DXF has no SHEET note: pass min_feature and hole_to_bend"));
    }
  };
  if let Some(v) = min_feature {
    rules.min_feature = v;
  }
  if let Some(v) = hole_to_bend {
    rules.hole_to_bend = v;
  }
  let report = flatcheck::check(&layered, rules);
  let mut out = serde_json::to_value(&report).map_err(|e| CmdError::new(e.to_string()))?;
  out["ok"] = json!(report.ok());
  if let Some(id) = body {
    out["body"] = json!(id.0);
  }
  Ok(out)
}
