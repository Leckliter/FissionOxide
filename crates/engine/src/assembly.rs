//! Assembly commands: components, joints, motion, interference and the bill of materials (joint
//! origins, rigid groups, motion links, contact sets and inserted designs are in [`crate::mates`]).

use fox_kernel::{BoolOp, Shape};
use fox_model::assembly::{self, JointGeometry, JointKind, JointLimits};
use fox_model::direct::BaseBody;
use fox_model::{BodyId, ComponentId, FeatureId, FeatureKind, PlaneRef};
use serde_json::{Value, json};

use crate::commands::{edge_ref_of, expr_of, face_ref, ids, opt_expr, opt_str, plane_ref, req_str, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Session, mates};

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
    "component.copy" => (|| {
      let source = component_id(s, p, "component")?;
      let t = match p.get("translate").and_then(Value::as_array) {
        Some(a) if a.len() == 3 => [expr_of(&a[0])?, expr_of(&a[1])?, expr_of(&a[2])?],
        Some(_) => return Err(CmdError::new("translate needs [x, y, z]")),
        None => ["0".into(), "0".into(), "0".into()],
      };
      let name = opt_str(p, "name").map(str::to_string);
      let (fid, cid) = s.edit(|d| {
        let fid = d.add_feature(FeatureKind::CopyComponent { source, translate: t })?;
        // the copies' body ids are fixed by the feature (see `new_body`)
        let n = d.component(source).map(|c| c.bodies.len()).unwrap_or(0) as u32;
        let bodies = (0..n).map(|k| BodyId(fid.0 * 1000 + k)).collect();
        let base = d.component(source).map(|c| c.name.clone()).unwrap_or_default();
        let count = d
          .features
          .iter()
          .filter(|f| matches!(&f.kind, FeatureKind::CopyComponent { source: s, .. } if *s == source))
          .count();
        let cid = d.add_component(&name.unwrap_or_else(|| format!("{base}:{}", count + 1)), bodies)?;
        if let (Some(src), Some(c)) = (d.component(source).map(|c| c.material.clone()), d.component_mut(cid)) {
          c.material = src;
        }
        Ok((fid, cid))
      })?;
      let st = s.model.status(fid);
      Ok(json!({"feature": fid.0, "component": cid.0, "error": st.and_then(|x| x.error.clone())}))
    })(),
    "joint" => joint(s, p),
    "constraint.center" => center_constraint(s, p),
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
      if mates::has_contacts(&s.design) && !set.is_empty() {
        return drive_to_contact(s, id, set);
      }
      s.execute("feature.edit", &json!({"feature": id.0, "set": set}))
    })(),
    "motion.study" => motion_study(s, p),
    "assembly.interference" => interference(s, p),
    "assembly.bom" => crate::subassembly::bom(s, p),
    _ => return mates::run(s, name, p),
  })
}

fn component_id(s: &Session, p: &Value, k: &str) -> Result<ComponentId, CmdError> {
  let id = ComponentId(req_u32(p, k)?);
  s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
  Ok(id)
}

/// {"face": face} | {"edge": edge} | {"at": [x,y,z], "z"?, "x"?} | {"origin": joint origin id or name}
pub(crate) fn geometry(s: &Session, v: &Value) -> Result<JointGeometry, CmdError> {
  if let Some(o) = v.get("origin") {
    return Ok(JointGeometry::Origin { origin: mates::origin_id(s, o)? });
  }
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
  Err(CmdError::new(
    "a joint origin is {\"face\": ...}, {\"edge\": ...}, {\"at\": [x, y, z], \"z\"?, \"x\"?} or {\"origin\": saved origin}",
  ))
}

fn limit(p: &Value, k: &str) -> Result<Option<(String, String)>, CmdError> {
  match p.get(k).and_then(Value::as_array).map(|a| a.as_slice()) {
    None => Ok(None),
    Some([lo, hi]) => Ok(Some((expr_of(lo)?, expr_of(hi)?))),
    Some(_) => Err(CmdError::new(format!("'{k}' limits are [min, max]"))),
  }
}

fn joint(s: &mut Session, p: &Value) -> CmdResult {
  let moving_origin = geometry(s, p.get("moving_origin").ok_or(CmdError::new("missing 'moving_origin'"))?)?;
  let fixed_origin = geometry(s, p.get("fixed_origin").ok_or(CmdError::new("missing 'fixed_origin'"))?)?;
  // saved joint origins know their component
  let on = |g: &JointGeometry| g.origin().and_then(|o| mates::origin_component(s, o));
  let moving = match (p.get("moving"), on(&moving_origin)) {
    (None, Some(c)) => c,
    _ => component_id(s, p, "moving")?,
  };
  let fixed = match (p.get("fixed"), on(&fixed_origin)) {
    (None, Some(c)) => Some(c),
    (Some(Value::Null) | None, _) => None,
    (Some(_), _) => Some(component_id(s, p, "fixed")?),
  };
  let kind = match opt_str(p, "kind").unwrap_or("rigid") {
    "rigid" => JointKind::Rigid,
    "revolute" => JointKind::Revolute,
    "slider" => JointKind::Slider,
    "cylindrical" => JointKind::Cylindrical,
    "pin_slot" => JointKind::PinSlot,
    "planar" => JointKind::Planar,
    "ball" => JointKind::Ball,
    "screw" => JointKind::Screw,
    o => {
      return Err(CmdError::new(format!(
        "unknown joint '{o}' (rigid, revolute, slider, cylindrical, pin_slot, planar, ball, screw)"
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
    lead: e("lead"),
  };
  if kind_is_screw(&kind) && opt_expr(p, "lead").is_none() {
    return Err(CmdError::new("a screw joint needs a 'lead' (its advance per turn)"));
  }
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  let st = s.model.status(id);
  Ok(json!({
    "feature": id.0,
    "name": s.design.feature(id).map(|f| f.name.clone()),
    "error": st.and_then(|x| x.error.clone()),
  }))
}

/// Center constraint: `moving_faces` (one plane, or two whose midplane is used) placed on the
/// midplane between `fixed_faces`, moving the component only along that plane's normal.
fn center_constraint(s: &mut Session, p: &Value) -> CmdResult {
  let planes = |s: &Session, k: &str| -> Result<Vec<PlaneRef>, CmdError> {
    let a = p.get(k).and_then(Value::as_array).ok_or(CmdError::new(format!("missing '{k}' (one or two faces)")))?;
    if a.is_empty() || a.len() > 2 {
      return Err(CmdError::new(format!("'{k}' is one face or plane, or two")));
    }
    a.iter().map(|v| plane_ref(s, v)).collect()
  };
  let moving_planes = planes(s, "moving_faces")?;
  let fixed_planes = planes(s, "fixed_faces")?;
  // a component given, else the one the first picked face is on
  let owner = |planes: &[PlaneRef]| match planes.first() {
    Some(PlaneRef::Face { face }) => s.design.component_of(face.body).map(|c| c.id),
    _ => None,
  };
  let moving = match (p.get("moving"), owner(&moving_planes)) {
    (None, Some(c)) => c,
    _ => component_id(s, p, "moving")?,
  };
  let fixed = match (p.get("fixed"), owner(&fixed_planes)) {
    (None, c) => c,
    (Some(Value::Null), _) => None,
    (Some(_), _) => Some(component_id(s, p, "fixed")?),
  };
  let offset = opt_expr(p, "offset").unwrap_or_else(|| "0".into());
  let id = s.edit(|d| {
    Ok(d.add_feature(FeatureKind::PlaneConstraint { moving, moving_planes, fixed, fixed_planes, offset })?)
  })?;
  mates::rename(s, id, p)?;
  Ok(mates::feature_json(s, id))
}

fn kind_is_screw(k: &FeatureKind) -> bool {
  matches!(k, FeatureKind::Joint { kind: JointKind::Screw, .. })
}

/// Steps a joint's motion and reports interference at each step (the design is left as it was).
/// With contact sets, the study stops where their bodies first touch.
fn motion_study(s: &Session, p: &Value) -> CmdResult {
  let id = FeatureId(req_u32(p, "joint")?);
  let field = opt_str(p, "motion").unwrap_or("rotation");
  if !["rotation", "slide", "slide2", "pitch"].contains(&field) {
    return Err(CmdError::new("motion is rotation, slide, slide2 or pitch"));
  }
  if !matches!(s.design.feature(id).map(|f| &f.kind), Some(FeatureKind::Joint { .. })) {
    return Err(CmdError::new("that feature is not a joint"));
  }
  let kind = if matches!(field, "rotation" | "pitch") { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
  let scope = s.model.params.clone();
  let from = fox_expr::eval(&expr_of(p.get("from").ok_or(CmdError::new("missing 'from'"))?)?, kind, &scope)?;
  let to = fox_expr::eval(&expr_of(p.get("to").ok_or(CmdError::new("missing 'to'"))?)?, kind, &scope)?;
  let steps = p.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 360) as usize;
  let shown = |v: f64| if kind == fox_expr::Kind::Angle { v.to_degrees() } else { v };
  let contact = mates::has_contacts(&s.design);
  let mut trial = mates::Trial::new(s, id);
  let mut out = Vec::new();
  let mut prev = from;
  for i in 0..=steps {
    let v = from + (to - from) * i as f64 / steps as f64;
    let at = mates::between(&[(field.to_string(), v)], &[(field.to_string(), v)], 0.0);
    if contact && !trial.clash(&at).is_empty() {
      // back up to where the contact set's bodies touch, and stop there
      let (a, b) = ([(field.to_string(), prev)], [(field.to_string(), v)]);
      let t = mates::last_clear(&mut trial, &a, &b);
      let stop = prev + (v - prev) * t;
      out.push(json!({"value": shown(stop), "error": null, "interference": [], "contact": true}));
      break;
    }
    let (d, m) = trial.model(&at);
    let err = m.status(id).and_then(|st| st.error.clone());
    let clashes = if err.is_none() { interference_in(&d, &m.bodies) } else { Vec::new() };
    out.push(json!({"value": shown(v), "error": err, "interference": clashes}));
    prev = v;
  }
  Ok(json!(out))
}

/// Drives a joint toward new motion values, stopping where contact set bodies first touch.
fn drive_to_contact(s: &mut Session, id: FeatureId, set: serde_json::Map<String, Value>) -> CmdResult {
  let scope = s.model.params.clone();
  let Some(FeatureKind::Joint { rotation, slide, slide2, pitch, .. }) = s.design.feature(id).map(|f| &f.kind) else {
    return Err(CmdError::new("that feature is not a joint"));
  };
  let now = |k: &str| match k {
    "rotation" => rotation.clone(),
    "slide" => slide.clone(),
    "slide2" => slide2.clone(),
    _ => pitch.clone(),
  };
  let kind = |k: &str| if matches!(k, "rotation" | "pitch") { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
  let mut from = Vec::new();
  let mut to = Vec::new();
  for (k, v) in &set {
    from.push((k.clone(), fox_expr::eval(&now(k), kind(k), &scope)?));
    to.push((k.clone(), fox_expr::eval(v.as_str().unwrap_or_default(), kind(k), &scope)?));
  }
  let mut trial = mates::Trial::new(s, id);
  let clash = trial.clash(&set);
  if clash.is_empty() {
    return s.execute("feature.edit", &json!({"feature": id.0, "set": set}));
  }
  let t = mates::last_clear(&mut trial, &from, &to);
  let stop = mates::between(&from, &to, t);
  let mut out = s.execute("feature.edit", &json!({"feature": id.0, "set": stop}))?;
  out["contact"] = json!(true);
  out["bodies"] = json!(clash.first().map(|(a, b)| [a.0, b.0]));
  out["stopped_at"] = json!(stop);
  Ok(out)
}

/// Pairs of overlapping bodies with the overlap volume: those of different components, or every
/// pair of the selected `bodies`. With `create_bodies` each overlap is added as a body of a base
/// feature (static geometry, as Fusion only allows in direct modelling), each in a new component
/// unless `components` is false.
fn interference(s: &mut Session, p: &Value) -> CmdResult {
  let picked: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  for b in &picked {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  let flag = |k: &str, d: bool| p.get(k).and_then(Value::as_bool).unwrap_or(d);
  let found = if picked.is_empty() {
    let d = &s.design;
    let comp = |b: BodyId| d.component_of(b).map(|c| c.id);
    overlap_shapes(&s.model.bodies, &|a, b| comp(a).is_none() || comp(a) != comp(b))
  } else {
    overlap_shapes(&s.model.bodies, &|a, b| picked.contains(&a) && picked.contains(&b))
  };
  let mut rows: Vec<Value> = found.iter().map(|(a, b, _, v)| json!({"bodies": [a.0, b.0], "volume_mm3": v})).collect();
  if !flag("create_bodies", false) || found.is_empty() {
    return Ok(json!(rows));
  }
  let mut breps = Vec::new();
  for (_, _, shape, _) in &found {
    breps.push(shape.to_brep()?);
  }
  let components = flag("components", true);
  let (fid, made) = s.edit(|d| {
    let fid = d.add_feature(FeatureKind::BaseFeature { bodies: Vec::new(), edits: Vec::new() })?;
    let n = d.components.iter().filter(|c| c.name.starts_with("Interference")).count();
    let bodies: Vec<BaseBody> = breps
      .into_iter()
      .enumerate()
      .map(|(k, brep)| BaseBody {
        id: BodyId(fid.0 * 1000 + k as u32),
        name: format!("Interference{}", n + k + 1),
        brep,
      })
      .collect();
    let mut made = Vec::new();
    for b in &bodies {
      let c = if components { Some(d.add_component(&b.name, vec![b.id])?.0) } else { None };
      made.push((b.id, c));
    }
    if let Some(FeatureKind::BaseFeature { bodies: slot, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) {
      *slot = bodies;
    }
    Ok((fid, made))
  })?;
  for (row, (body, comp)) in rows.iter_mut().zip(made) {
    row["body"] = json!(body.0);
    row["component"] = json!(comp);
    row["feature"] = json!(fid.0);
  }
  Ok(json!(rows))
}

fn interference_in(d: &fox_model::Design, bodies: &[fox_model::regen::Body]) -> Vec<Value> {
  let comp = |b: BodyId| d.component_of(b).map(|c| c.id);
  let apart = |a: BodyId, b: BodyId| comp(a).is_none() || comp(a) != comp(b);
  overlaps(bodies, &apart).into_iter().map(|(a, b, v)| json!({"bodies": [a.0, b.0], "volume_mm3": v})).collect()
}

/// Pairs of bodies that `check` accepts and that overlap, with the overlap volume.
pub(crate) fn overlaps(
  bodies: &[fox_model::regen::Body],
  check: &dyn Fn(BodyId, BodyId) -> bool,
) -> Vec<(BodyId, BodyId, f64)> {
  overlap_shapes(bodies, check).into_iter().map(|(a, b, _, v)| (a, b, v)).collect()
}

/// Pairs of bodies that `check` accepts and that overlap, with the overlap and its volume.
fn overlap_shapes(
  bodies: &[fox_model::regen::Body],
  check: &dyn Fn(BodyId, BodyId) -> bool,
) -> Vec<(BodyId, BodyId, Shape, f64)> {
  let boxes: Vec<_> = bodies.iter().map(|b| b.shape.mass().ok().map(|m| (m.bbox_min, m.bbox_max))).collect();
  let mut out = Vec::new();
  for i in 0..bodies.len() {
    for j in i + 1..bodies.len() {
      let (a, b) = (&bodies[i], &bodies[j]);
      if !check(a.id, b.id) {
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
        out.push((a.id, b.id, c, m.volume));
      }
    }
  }
  out
}

/// Density (g/mm³) of a body: its component's physical material, steel for root bodies.
pub(crate) fn density_of(s: &Session, body: BodyId) -> f64 {
  let material = s
    .design
    .components
    .iter()
    .find(|c| c.bodies.contains(&body) && !c.material.is_empty())
    .map(|c| c.material.as_str())
    .unwrap_or("Steel");
  assembly::density(material) / 1000.0
}
