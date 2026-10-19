//! Assembly commands: components, joints, motion, interference and the bill of materials.

use clf_kernel::BoolOp;
use clf_model::assembly::{self, JointGeometry, JointKind, JointLimits};
use clf_model::{BodyId, ComponentId, FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{edge_ref_of, expr_of, face_ref, ids, opt_expr, opt_str, req_str, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "component.create" => (|| {
      let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
      for b in &bodies {
        if s.model.body(*b).is_none() {
          return Err(CmdError::new(format!("no body {}", b.0)));
        }
      }
      let name = opt_str(p, "name").unwrap_or("").to_string();
      let id = s.edit(|d| Ok(d.add_component(&name, bodies)?))?;
      Ok(json!({"component": id.0, "name": s.design.component(id).map(|c| c.name.clone())}))
    })(),
    "component.add_bodies" => (|| {
      let id = component_id(s, p, "component")?;
      let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
      s.edit(|d| {
        for c in &mut d.components {
          c.bodies.retain(|b| !bodies.contains(b));
        }
        let c = d.component_mut(id).ok_or(CmdError::new("no such component"))?;
        c.bodies.extend(bodies);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "component.delete" => (|| {
      let id = component_id(s, p, "component")?;
      s.edit(|d| Ok(d.remove_component(id)?))?;
      Ok(json!({}))
    })(),
    "component.rename" => (|| {
      let id = component_id(s, p, "component")?;
      let n = req_str(p, "name")?.to_string();
      s.edit(|d| {
        d.component_mut(id).ok_or(CmdError::new("no such component"))?.name = n;
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "component.ground" => (|| {
      let id = component_id(s, p, "component")?;
      let on = p.get("grounded").and_then(Value::as_bool).unwrap_or(true);
      s.edit(|d| {
        d.component_mut(id).ok_or(CmdError::new("no such component"))?.grounded = on;
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "component.material" => (|| {
      let id = component_id(s, p, "component")?;
      let m = req_str(p, "material")?;
      let (name, _) = assembly::MATERIALS.iter().find(|(n, _)| n.eq_ignore_ascii_case(m)).ok_or_else(|| {
        let all: Vec<&str> = assembly::MATERIALS.iter().map(|(n, _)| *n).collect();
        CmdError::new(format!("unknown material '{m}' ({})", all.join(", ")))
      })?;
      s.edit(|d| {
        d.component_mut(id).ok_or(CmdError::new("no such component"))?.material = name.to_string();
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "joint" => joint(s, p),
    "joint.drive" => (|| {
      let id = FeatureId(req_u32(p, "joint")?);
      let mut set = serde_json::Map::new();
      for k in ["rotation", "slide", "slide2", "pitch"] {
        if let Some(v) = p.get(k) {
          set.insert(k.into(), json!(expr_of(v)?));
        }
      }
      if !matches!(s.design.feature(id).map(|f| &f.kind), Some(FeatureKind::Joint { .. })) {
        return Err(CmdError::new("that feature is not a joint"));
      }
      s.execute("feature.edit", &json!({"feature": id.0, "set": set}))
    })(),
    "motion.study" => motion_study(s, p),
    "assembly.interference" => Ok(json!(interference(&s.model.bodies, s))),
    "assembly.bom" => Ok(bom(s)),
    _ => return None,
  })
}

fn component_id(s: &Session, p: &Value, k: &str) -> Result<ComponentId, CmdError> {
  let id = ComponentId(req_u32(p, k)?);
  s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
  Ok(id)
}

/// {"face": face} | {"edge": edge} | {"at": [x,y,z], "z"?, "x"?}
fn geometry(s: &Session, v: &Value) -> Result<JointGeometry, CmdError> {
  if let Some(f) = v.get("face") {
    return Ok(JointGeometry::Face { face: face_ref(s, f)? });
  }
  if let Some(e) = v.get("edge") {
    return Ok(JointGeometry::Edge { edge: edge_ref_of(s, e)? });
  }
  if v.get("at").is_some() {
    let opt = |k: &str, d: [f64; 3]| if v.get(k).is_some() { req_vec3(v, k) } else { Ok(d) };
    return Ok(JointGeometry::Point {
      point: req_vec3(v, "at")?,
      z: opt("z", [0.0, 0.0, 1.0])?,
      x: opt("x", [1.0, 0.0, 0.0])?,
    });
  }
  Err(CmdError::new("a joint origin is {\"face\": ...}, {\"edge\": ...} or {\"at\": [x, y, z], \"z\"?, \"x\"?}"))
}

fn limit(p: &Value, k: &str) -> Result<Option<(String, String)>, CmdError> {
  match p.get(k).and_then(Value::as_array).map(|a| a.as_slice()) {
    None => Ok(None),
    Some([lo, hi]) => Ok(Some((expr_of(lo)?, expr_of(hi)?))),
    Some(_) => Err(CmdError::new(format!("'{k}' limits are [min, max]"))),
  }
}

fn joint(s: &mut Session, p: &Value) -> CmdResult {
  let moving = component_id(s, p, "moving")?;
  let fixed = match p.get("fixed") {
    Some(Value::Null) | None => None,
    Some(_) => Some(component_id(s, p, "fixed")?),
  };
  let moving_origin = geometry(s, p.get("moving_origin").ok_or(CmdError::new("missing 'moving_origin'"))?)?;
  let fixed_origin = geometry(s, p.get("fixed_origin").ok_or(CmdError::new("missing 'fixed_origin'"))?)?;
  let kind = match opt_str(p, "kind").unwrap_or("rigid") {
    "rigid" => JointKind::Rigid,
    "revolute" => JointKind::Revolute,
    "slider" => JointKind::Slider,
    "cylindrical" => JointKind::Cylindrical,
    "pin_slot" => JointKind::PinSlot,
    "planar" => JointKind::Planar,
    "ball" => JointKind::Ball,
    o => {
      return Err(CmdError::new(format!(
        "unknown joint '{o}' (rigid, revolute, slider, cylindrical, pin_slot, planar, ball)"
      )));
    }
  };
  let limits = match p.get("limits") {
    Some(l) => JointLimits { rotation: limit(l, "rotation")?, slide: limit(l, "slide")? },
    None => JointLimits::default(),
  };
  let e = |k: &str| opt_expr(p, k).unwrap_or_else(|| "0".into());
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  let kind = FeatureKind::Joint {
    moving,
    moving_origin,
    fixed,
    fixed_origin,
    kind,
    angle: e("angle"),
    offset: e("offset"),
    flip: flag("flip"),
    as_built: flag("as_built"),
    rotation: e("rotation"),
    slide: e("slide"),
    slide2: e("slide2"),
    pitch: e("pitch"),
    limits,
  };
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  let st = s.model.status(id);
  Ok(json!({
    "feature": id.0,
    "name": s.design.feature(id).map(|f| f.name.clone()),
    "error": st.and_then(|x| x.error.clone()),
  }))
}

/// Steps a joint's motion and reports interference at each step (the design is left as it was).
fn motion_study(s: &Session, p: &Value) -> CmdResult {
  let id = FeatureId(req_u32(p, "joint")?);
  let field = opt_str(p, "motion").unwrap_or("rotation");
  if !["rotation", "slide", "slide2", "pitch"].contains(&field) {
    return Err(CmdError::new("motion is rotation, slide, slide2 or pitch"));
  }
  let kind = if matches!(field, "rotation" | "pitch") { clf_expr::Kind::Angle } else { clf_expr::Kind::Length };
  let scope = s.model.params.clone();
  let from = clf_expr::eval(&expr_of(p.get("from").ok_or(CmdError::new("missing 'from'"))?)?, kind, &scope)?;
  let to = clf_expr::eval(&expr_of(p.get("to").ok_or(CmdError::new("missing 'to'"))?)?, kind, &scope)?;
  let steps = p.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 360) as usize;
  let mut out = Vec::new();
  for i in 0..=steps {
    let v = from + (to - from) * i as f64 / steps as f64;
    let mut d = s.design.clone();
    let f = d.feature_mut(id).ok_or(CmdError::new("no such joint"))?;
    let FeatureKind::Joint { rotation, slide, slide2, pitch, .. } = &mut f.kind else {
      return Err(CmdError::new("that feature is not a joint"));
    };
    let slot = match field {
      "rotation" => rotation,
      "slide" => slide,
      "slide2" => slide2,
      _ => pitch,
    };
    *slot = if kind == clf_expr::Kind::Angle { format!("{} deg", v.to_degrees()) } else { format!("{v} mm") };
    let m = clf_model::regen::regenerate(&mut d);
    let err = m.status(id).and_then(|st| st.error.clone());
    let clashes = if err.is_none() { interference(&m.bodies, s) } else { Vec::new() };
    out.push(json!({
      "value": if kind == clf_expr::Kind::Angle { v.to_degrees() } else { v },
      "error": err,
      "interference": clashes,
    }));
  }
  Ok(json!(out))
}

/// Pairs of bodies (in different components) that overlap, with the overlap volume.
fn interference(bodies: &[clf_model::regen::Body], s: &Session) -> Vec<Value> {
  let comp = |b: BodyId| s.design.component_of(b).map(|c| c.id);
  let boxes: Vec<_> = bodies.iter().map(|b| b.shape.mass().ok().map(|m| (m.bbox_min, m.bbox_max))).collect();
  let mut out = Vec::new();
  for i in 0..bodies.len() {
    for j in i + 1..bodies.len() {
      let (a, b) = (&bodies[i], &bodies[j]);
      if comp(a.id).is_some() && comp(a.id) == comp(b.id) {
        continue;
      }
      let (Some(ba), Some(bb)) = (boxes[i], boxes[j]) else { continue };
      if (0..3).any(|k| ba.1[k] < bb.0[k] - 1e-6 || bb.1[k] < ba.0[k] - 1e-6) {
        continue;
      }
      if let Ok(c) = a.shape.boolean(&b.shape, BoolOp::Intersect)
        && let Ok(m) = c.mass()
        && m.volume > 1e-6
      {
        out.push(json!({"bodies": [a.id.0, b.id.0], "volume_mm3": m.volume}));
      }
    }
  }
  out
}

fn bom(s: &Session) -> Value {
  let body_volume = |b: BodyId| s.model.body(b).and_then(|b| b.shape.mass().ok()).map(|m| m.volume).unwrap_or(0.0);
  let mut rows: Vec<Value> = s
    .design
    .components
    .iter()
    .map(|c| {
      let volume: f64 = c.bodies.iter().map(|b| body_volume(*b)).sum();
      let material = if c.material.is_empty() { "Steel" } else { c.material.as_str() };
      json!({
        "component": c.id.0,
        "name": c.name,
        "bodies": c.bodies.len(),
        "material": material,
        "volume_mm3": volume,
        "mass_g": volume * assembly::density(material) / 1000.0,
        "grounded": c.grounded,
      })
    })
    .collect();
  let loose: Vec<BodyId> =
    s.model.bodies.iter().map(|b| b.id).filter(|b| s.design.component_of(*b).is_none()).collect();
  if !loose.is_empty() {
    let volume: f64 = loose.iter().map(|b| body_volume(*b)).sum();
    rows.push(json!({
      "component": null, "name": "(root bodies)", "bodies": loose.len(), "material": "Steel",
      "volume_mm3": volume, "mass_g": volume * assembly::density("Steel") / 1000.0, "grounded": true,
    }));
  }
  json!(rows)
}
