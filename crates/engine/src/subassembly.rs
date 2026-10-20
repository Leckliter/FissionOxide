//! Sub-assemblies and mechanisms: designs inserted with their own components and joints
//! (rigid or flexible), component nesting, cam followers, and the bill of materials by level.

use std::collections::HashMap;

use fox_model::assembly::JointGeometry;
use fox_model::regen::Regenerator;
use fox_model::subassembly::{SubAssembly, SubComponent, SubMate};
use fox_model::{BodyId, ComponentId, Design, FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{expr_of, opt_str, req_str, req_u32};
use crate::mates::{components_of, feature_by, feature_json, rename};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "cam_follower" => cam_follower(s, p),
    "component.flexible" => (|| {
      let (fid, _) = insert_for(s, p)?;
      let on = p.get("flexible").and_then(Value::as_bool).unwrap_or(true);
      s.edit(|d| {
        let Some(FeatureKind::InsertDesign { sub: Some(sub), name, .. }) = d.feature_mut(fid).map(|f| &mut f.kind)
        else {
          return Err(CmdError::new("that component has no sub-assembly"));
        };
        if on && !sub.has_joints() {
          return Err(CmdError::new(format!("{name} has no joints to solve")));
        }
        sub.flexible = on;
        Ok(())
      })?;
      Ok(feature_json(s, fid))
    })(),
    "component.sub_joints" => (|| {
      let (_, sub) = insert_for(s, p)?;
      Ok(json!(sub_joints(&s.design, &sub)))
    })(),
    "component.drive" => drive(s, p),
    "component.parent" => (|| {
      let c = ComponentId(req_u32(p, "component")?);
      s.design.component(c).ok_or(CmdError::new(format!("no component {}", c.0)))?;
      let parent = match p.get("parent") {
        None | Some(Value::Null) => None,
        Some(v) => {
          let id = ComponentId(v.as_u64().ok_or(CmdError::new("'parent' must be a component id"))? as u32);
          s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
          if id == c || ancestors(&s.design, id).contains(&c) {
            return Err(CmdError::new("a component can't go inside itself"));
          }
          Some(id)
        }
      };
      s.edit(|d| {
        d.component_mut(c).ok_or(CmdError::new("no such component"))?.parent = parent;
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    _ => return None,
  })
}

// ---- inserted sub-assemblies ----

/// An inserted design's assembly structure: what [`Source::attach`] turns into components and
/// a [`SubAssembly`] in the host.
pub(crate) struct Source {
  design: Design,
  /// The source body behind each inserted body (insert body k = `bodies[k]`).
  bodies: Vec<BodyId>,
  /// The bodies before joints moved them, same order; empty without joints.
  free: Vec<String>,
  params: HashMap<String, f64>,
}

impl Source {
  /// The structure of a regenerated source design.
  pub fn of(src: &Session) -> Result<Source, CmdError> {
    let bodies: Vec<BodyId> = src.model.bodies.iter().map(|b| b.id).collect();
    let jointed = src.design.features[..src.design.active_len()]
      .iter()
      .any(|f| !f.suppressed && matches!(f.kind, FeatureKind::Joint { .. }));
    let mut free = Vec::new();
    if jointed && !src.design.components.is_empty() {
      // the bodies before any joint moved them
      let mut d = src.design.clone();
      for f in &mut d.features {
        if matches!(
          f.kind,
          FeatureKind::Joint { .. }
            | FeatureKind::RigidGroup { .. }
            | FeatureKind::MotionLink { .. }
            | FeatureKind::CamFollower { .. }
        ) {
          f.suppressed = true;
        }
      }
      let m = Regenerator::default().regenerate(&mut d);
      for b in &bodies {
        match m.body(*b) {
          Some(body) => free.push(body.shape.to_brep().map_err(|e| CmdError::new(e.to_string()))?),
          None => {
            free.clear();
            break;
          }
        }
      }
    }
    Ok(Source { design: src.design.clone(), bodies, free, params: src.model.params.clone() })
  }

  /// Gives the inserted component `top` (made by insert feature `fid`) the source's components
  /// as children, and stores the source's joints in the feature. Updating an insert keeps the
  /// host components already standing in for source components (and the rigid/flexible choice).
  pub fn attach(&self, d: &mut Design, fid: FeatureId, top: ComponentId) -> Result<(), CmdError> {
    let old = match d.feature(fid).map(|f| &f.kind) {
      Some(FeatureKind::InsertDesign { sub, .. }) => sub.as_deref().cloned(),
      _ => return Err(CmdError::new("not an inserted design")),
    };
    if self.design.components.is_empty() {
      // the source isn't an assembly: the inserted component holds its bodies
      if let Some(old) = &old {
        for c in &old.components {
          let _ = d.remove_component(c.id);
        }
      }
      if let Some(FeatureKind::InsertDesign { sub, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) {
        *sub = None;
      }
      return Ok(());
    }
    let host_body = |b: BodyId| self.bodies.iter().position(|x| *x == b).map(|k| BodyId(fid.0 * 1000 + k as u32));
    // a host component per source component
    let mut map: HashMap<ComponentId, ComponentId> = HashMap::new();
    let mut subs = Vec::new();
    for sc in &self.design.components {
      let bodies: Vec<BodyId> = sc.bodies.iter().filter_map(|b| host_body(*b)).collect();
      let reuse = old.iter().flat_map(|o| &o.components).find(|x| x.source == sc.id).map(|x| x.id);
      let id = match reuse.filter(|id| d.component(*id).is_some()) {
        Some(id) => {
          for c in &mut d.components {
            c.bodies.retain(|b| !bodies.contains(b));
          }
          if let Some(c) = d.component_mut(id) {
            c.bodies = bodies;
          }
          id
        }
        None => {
          let id = d.add_component(&sc.name, bodies)?;
          if let Some(c) = d.component_mut(id) {
            c.material = sc.material.clone();
          }
          id
        }
      };
      map.insert(sc.id, id);
      subs.push(SubComponent { source: sc.id, id, name: sc.name.clone() });
    }
    for sc in &self.design.components {
      let parent = sc.parent.and_then(|p| map.get(&p).copied()).unwrap_or(top);
      if let Some(c) = map.get(&sc.id).and_then(|id| d.component_mut(*id)) {
        c.parent = Some(parent);
      }
    }
    // source components that are gone
    for c in old.iter().flat_map(|o| &o.components) {
      if !subs.iter().any(|x| x.id == c.id) {
        let _ = d.remove_component(c.id);
      }
    }
    let mut mates = self.mates(fid, &map, &host_body);
    // keep the motions driven here for joints the source still has
    for m in &mut mates {
      let was = old.iter().flat_map(|o| &o.mates).find(|o| o.name == m.name);
      if let (
        FeatureKind::Joint { rotation, slide, slide2, pitch, .. },
        Some(FeatureKind::Joint { rotation: r, slide: s1, slide2: s2, pitch: p, .. }),
      ) = (&mut m.kind, was.map(|o| &o.kind))
      {
        (*rotation, *slide, *slide2, *pitch) = (r.clone(), s1.clone(), s2.clone(), p.clone());
      }
    }
    let sub = SubAssembly {
      components: subs,
      free_breps: if mates.is_empty() { Vec::new() } else { self.free.clone() },
      mates,
      flexible: old.is_some_and(|o| o.flexible),
      top: Some(top),
    };
    if let Some(FeatureKind::InsertDesign { sub: slot, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) {
      *slot = Some(Box::new(sub));
    }
    Ok(())
  }

  /// The source's joint origins and joints, renamed to the host and with values frozen.
  fn mates(
    &self,
    fid: FeatureId,
    comps: &HashMap<ComponentId, ComponentId>,
    host_body: &dyn Fn(BodyId) -> Option<BodyId>,
  ) -> Vec<SubMate> {
    let mut out: Vec<SubMate> = Vec::new();
    let mut origins: HashMap<FeatureId, FeatureId> = HashMap::new();
    let d = &self.design;
    for f in d.features[..d.active_len()].iter().filter(|f| !f.suppressed) {
      let mut kind = f.kind.clone();
      let ok = match &mut kind {
        FeatureKind::JointOrigin { component, geometry, angle, offset, .. } => {
          self.freeze(angle, true)
            && self.freeze(offset, false)
            && remap(component, comps)
            && regeo(geometry, host_body, &origins)
        }
        FeatureKind::Joint {
          moving,
          moving_origin,
          fixed,
          fixed_origin,
          angle,
          offset,
          rotation,
          slide,
          slide2,
          pitch,
          limits,
          lead,
          ..
        } => {
          let mut ok = remap(moving, comps) && fixed.as_mut().is_none_or(|c| remap(c, comps));
          ok &= regeo(moving_origin, host_body, &origins) && regeo(fixed_origin, host_body, &origins);
          for (e, a) in
            [(angle, true), (offset, false), (rotation, true), (slide, false), (slide2, false), (pitch, true)]
          {
            ok &= self.freeze(e, a);
          }
          ok &= lead.is_empty() || self.freeze(lead, false);
          for (lim, a) in [(&mut limits.rotation, true), (&mut limits.slide, false)] {
            if let Some((lo, hi)) = lim {
              ok &= self.freeze(lo, a) && self.freeze(hi, a);
            }
          }
          ok
        }
        _ => continue,
      };
      if !ok {
        continue;
      }
      if matches!(kind, FeatureKind::JointOrigin { .. }) {
        origins.insert(f.id, SubAssembly::origin_id(fid, out.len()));
      }
      out.push(SubMate { name: f.name.clone(), kind });
    }
    if !out.iter().any(|m| matches!(m.kind, FeatureKind::Joint { .. })) {
      out.clear();
    }
    out
  }

  /// Replaces a source expression with its value (the source's parameters don't exist here).
  fn freeze(&self, e: &mut String, angle: bool) -> bool {
    let kind = if angle { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
    match fox_expr::eval(e, kind, &self.params) {
      Ok(v) => {
        *e = if angle { format!("{} deg", round(v.to_degrees())) } else { format!("{} mm", round(v)) };
        true
      }
      Err(_) => false,
    }
  }
}

fn round(v: f64) -> f64 {
  (v * 1e9).round() / 1e9
}

fn remap(c: &mut ComponentId, map: &HashMap<ComponentId, ComponentId>) -> bool {
  map.get(c).map(|n| *c = *n).is_some()
}

fn regeo(
  g: &mut JointGeometry,
  host_body: &dyn Fn(BodyId) -> Option<BodyId>,
  origins: &HashMap<FeatureId, FeatureId>,
) -> bool {
  match g {
    JointGeometry::Face { face } => host_body(face.body).map(|b| face.body = b).is_some(),
    JointGeometry::Edge { edge } => host_body(edge.body).map(|b| edge.body = b).is_some(),
    JointGeometry::Point { .. } => true,
    JointGeometry::Origin { origin } => origins.get(origin).map(|o| *origin = *o).is_some(),
  }
}

/// The insert feature whose sub-assembly hangs below component `c`.
pub(crate) fn insert_with_top(d: &Design, c: ComponentId) -> Option<FeatureId> {
  d.features.iter().find_map(|f| match &f.kind {
    FeatureKind::InsertDesign { sub: Some(sub), .. } if sub.top == Some(c) => Some(f.id),
    _ => None,
  })
}

/// The insert feature (and its sub-assembly) for `component`: the inserted component itself or
/// any component of its sub-assembly.
fn insert_for(s: &Session, p: &Value) -> Result<(FeatureId, SubAssembly), CmdError> {
  let c = ComponentId(req_u32(p, "component")?);
  let comp = s.design.component(c).ok_or(CmdError::new(format!("no component {}", c.0)))?;
  s.design
    .features
    .iter()
    .find_map(|f| match &f.kind {
      FeatureKind::InsertDesign { sub: Some(sub), .. }
        if sub.top == Some(c) || sub.components.iter().any(|x| x.id == c) =>
      {
        Some((f.id, (**sub).clone()))
      }
      _ => None,
    })
    .ok_or_else(|| CmdError::new(format!("{} isn't an inserted sub-assembly", comp.name)))
}

fn sub_joints(d: &Design, sub: &SubAssembly) -> Vec<Value> {
  let comp = |c: ComponentId| d.component(c).map(|c| c.name.clone());
  sub
    .mates
    .iter()
    .filter_map(|m| match &m.kind {
      FeatureKind::Joint { kind, moving, fixed, rotation, slide, slide2, pitch, .. } => Some(json!({
        "name": m.name,
        "kind": serde_json::to_value(kind).unwrap_or_default(),
        "moving": comp(*moving),
        "fixed": fixed.and_then(comp),
        "rotation": rotation, "slide": slide, "slide2": slide2, "pitch": pitch,
        "flexible": sub.flexible,
      })),
      _ => None,
    })
    .collect()
}

/// Sets motion values of a joint inside a flexible sub-assembly.
fn drive(s: &mut Session, p: &Value) -> CmdResult {
  let (fid, sub) = insert_for(s, p)?;
  let joint = req_str(p, "joint")?.to_string();
  if !sub.flexible {
    return Err(CmdError::new("the sub-assembly is rigid; make it flexible to drive its joints"));
  }
  let k = sub
    .mates
    .iter()
    .position(|m| m.name == joint && matches!(m.kind, FeatureKind::Joint { .. }))
    .ok_or_else(|| CmdError::new(format!("the sub-assembly has no joint '{joint}'")))?;
  let mut set = Vec::new();
  for key in ["rotation", "slide", "slide2", "pitch"] {
    if let Some(v) = p.get(key) {
      set.push((key, expr_of(v)?));
    }
  }
  s.edit(|d| {
    if let Some(FeatureKind::InsertDesign { sub: Some(sub), .. }) = d.feature_mut(fid).map(|f| &mut f.kind)
      && let Some(FeatureKind::Joint { rotation, slide, slide2, pitch, .. }) = sub.mates.get_mut(k).map(|m| &mut m.kind)
    {
      for (key, v) in set {
        *match key {
          "rotation" => &mut *rotation,
          "slide" => &mut *slide,
          "slide2" => &mut *slide2,
          _ => &mut *pitch,
        } = v;
      }
    }
    Ok(())
  })?;
  let mut out = feature_json(s, fid);
  out["warning"] = json!(s.model.status(fid).and_then(|x| x.warning.clone()));
  Ok(out)
}

fn ancestors(d: &Design, c: ComponentId) -> Vec<ComponentId> {
  let mut out = Vec::new();
  let mut at = d.component(c).and_then(|c| c.parent);
  while let Some(p) = at {
    if out.contains(&p) {
      break;
    }
    out.push(p);
    at = d.component(p).and_then(|c| c.parent);
  }
  out
}

// ---- cams ----

fn cam_follower(s: &mut Session, p: &Value) -> CmdResult {
  let follower = feature_by(s, p.get("follower"), "joint", |k| matches!(k, FeatureKind::Joint { .. }))?;
  let cam = match p.get("cam") {
    Some(v) => {
      let id = ComponentId(v.as_u64().ok_or(CmdError::new("'cam' must be a component id"))? as u32);
      s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
      id
    }
    None => *components_of(s, p)?.first().ok_or(CmdError::new("missing 'cam' (a component, or bodies of it)"))?,
  };
  let clearance = match p.get("clearance") {
    Some(v) => expr_of(v)?,
    None => "0 mm".into(),
  };
  let flat = p.get("flat").and_then(Value::as_bool).unwrap_or(false);
  let id = s.edit(|d| Ok(d.add_feature(FeatureKind::CamFollower { cam, follower, flat, clearance })?))?;
  rename(s, id, p)?;
  let mut out = feature_json(s, id);
  let joint_error = s.model.status(follower).and_then(|x| x.error.clone());
  if out["error"].is_null() && joint_error.is_some() {
    out["error"] = json!(joint_error);
  }
  Ok(out)
}

// ---- bill of materials ----

/// Components in browser order: each followed by its sub-components, with their depth.
pub(crate) fn tree(d: &Design) -> Vec<(ComponentId, usize)> {
  fn walk(d: &Design, parent: Option<ComponentId>, depth: usize, out: &mut Vec<(ComponentId, usize)>) {
    for c in &d.components {
      let p = c.parent.filter(|p| d.component(*p).is_some());
      if p == parent && !out.iter().any(|(x, _)| *x == c.id) && depth < 64 {
        out.push((c.id, depth));
        walk(d, Some(c.id), depth + 1, out);
      }
    }
  }
  let mut out = Vec::new();
  walk(d, None, 0, &mut out);
  // components caught in a parent loop still get a row
  for c in &d.components {
    if !out.iter().any(|(x, _)| *x == c.id) {
      out.push((c.id, 0));
    }
  }
  out
}

pub(crate) fn bom(s: &Session, p: &Value) -> CmdResult {
  let view = opt_str(p, "view").unwrap_or("indented");
  let body_volume = |b: &BodyId| s.model.body(*b).and_then(|b| b.shape.mass().ok()).map(|m| m.volume).unwrap_or(0.0);
  let material =
    |c: &fox_model::Component| if c.material.is_empty() { "Steel".to_string() } else { c.material.clone() };
  let tree = tree(&s.design);
  let below = |c: ComponentId| -> Vec<ComponentId> {
    let at = tree.iter().position(|(x, _)| *x == c).unwrap_or(0);
    let depth = tree[at].1;
    std::iter::once(c).chain(tree[at + 1..].iter().take_while(|(_, dd)| *dd > depth).map(|(x, _)| *x)).collect()
  };
  // mass of a component and everything below it (each part in its own material)
  let totals = |c: ComponentId| -> (usize, f64, f64) {
    below(c).iter().filter_map(|x| s.design.component(*x)).fold((0, 0.0, 0.0), |(n, v, m), k| {
      let vol: f64 = k.bodies.iter().map(body_volume).sum();
      (n + k.bodies.len(), v + vol, m + vol * fox_model::assembly::density(&material(k)) / 1000.0)
    })
  };
  let row = |c: &fox_model::Component, level: usize| {
    let volume: f64 = c.bodies.iter().map(body_volume).sum();
    let m = material(c);
    let children = s.design.components.iter().filter(|k| k.parent == Some(c.id)).count();
    let mut r = json!({
      "component": c.id.0,
      "name": c.name,
      "bodies": c.bodies.len(),
      "material": m,
      "volume_mm3": volume,
      "mass_g": volume * fox_model::assembly::density(&m) / 1000.0,
      "grounded": c.grounded,
      "level": level,
      "parent": c.parent.map(|p| p.0),
    });
    if children > 0 {
      let (n, v, mass) = totals(c.id);
      r["sub_components"] = json!(children);
      r["total_bodies"] = json!(n);
      r["total_volume_mm3"] = json!(v);
      r["total_mass_g"] = json!(mass);
    }
    r
  };
  let mut rows: Vec<Value> = match view {
    "indented" => tree.iter().filter_map(|(c, level)| s.design.component(*c).map(|c| row(c, *level))).collect(),
    "top" => tree
      .iter()
      .filter(|(_, l)| *l == 0)
      .filter_map(|(c, _)| s.design.component(*c))
      .map(|c| {
        let (n, v, mass) = totals(c.id);
        json!({
          "component": c.id.0, "name": c.name, "bodies": n, "material": material(c),
          "volume_mm3": v, "mass_g": mass, "grounded": c.grounded, "level": 0, "parent": null,
        })
      })
      .collect(),
    "parts" => {
      // components without sub-components, alike ones (name, material, volume) counted together
      let mut out: Vec<Value> = Vec::new();
      for (c, _) in &tree {
        let Some(c) = s.design.component(*c) else { continue };
        if s.design.components.iter().any(|k| k.parent == Some(c.id)) || c.bodies.is_empty() {
          continue;
        }
        let r = row(c, 0);
        let same = |o: &Value| {
          o["name"] == r["name"]
            && o["material"] == r["material"]
            && (o["volume_mm3"].as_f64().unwrap_or(0.0) - r["volume_mm3"].as_f64().unwrap_or(0.0)).abs() < 1e-3
        };
        match out.iter_mut().find(|o| same(o)) {
          Some(o) => {
            o["quantity"] = json!(o["quantity"].as_u64().unwrap_or(1) + 1);
            if let Some(a) = o["components"].as_array_mut() {
              a.push(json!(c.id.0));
            }
          }
          None => out.push(json!({
            "component": c.id.0, "components": [c.id.0], "name": r["name"], "material": r["material"],
            "bodies": r["bodies"], "volume_mm3": r["volume_mm3"], "mass_g": r["mass_g"], "quantity": 1,
          })),
        }
      }
      out
    }
    o => return Err(CmdError::new(format!("unknown view '{o}' (indented, top, parts)"))),
  };
  let loose: Vec<BodyId> =
    s.model.bodies.iter().map(|b| b.id).filter(|b| s.design.component_of(*b).is_none()).collect();
  if !loose.is_empty() {
    let volume: f64 = loose.iter().map(body_volume).sum();
    rows.push(json!({
      "component": null, "name": "(root bodies)", "bodies": loose.len(), "material": "Steel",
      "volume_mm3": volume, "mass_g": volume * fox_model::assembly::density("Steel") / 1000.0, "grounded": true,
      "level": 0, "parent": null,
    }));
  }
  Ok(json!(rows))
}
