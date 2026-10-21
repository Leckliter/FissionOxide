//! Assembly commands added after joints: saved joint origins, rigid groups, motion links,
//! contact sets, and components inserted from other designs in the data hub as linked references.

use fox_model::assembly::{ContactSet, HubSource, MotionAxis};
use fox_model::regen::{Body, Model, Regenerator};
use fox_model::{BodyId, ComponentId, Design, FeatureId, FeatureKind, JointKind};
use fox_store::{ItemKind, Store};
use serde_json::{Map, Value, json};

use crate::commands::{expr_of, ids, opt_expr, opt_str, req_str, req_u32};
use crate::subassembly::Source;
use crate::{CmdError, CmdResult, Session, hub};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "joint_origin" => joint_origin(s, p),
    "rigid_group" => (|| {
      let comps = components_of(s, p)?;
      if comps.len() < 2 {
        return Err(CmdError::new("a rigid group needs at least two components"));
      }
      // where joints have put each member now (see `RigidGroup::poses`)
      let poses = comps.iter().map(|c| s.model.placed.get(c).copied().unwrap_or_default().to_cols_array()).collect();
      let id = s.edit(|d| Ok(d.add_feature(FeatureKind::RigidGroup { components: comps, poses })?))?;
      rename(s, id, p)?;
      Ok(feature_json(s, id))
    })(),
    "motion_link" => motion_link(s, p),
    "contact_set.create" => (|| {
      let comps = components_of(s, p)?;
      if comps.len() < 2 {
        return Err(CmdError::new("a contact set needs at least two components"));
      }
      let id = s.design.contact_sets.iter().map(|c| c.id).max().unwrap_or(0) + 1;
      let name = opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("Contact Set{id}"));
      s.edit(|d| {
        d.contact_sets.push(ContactSet { id, name: name.clone(), components: comps, enabled: true });
        Ok(())
      })?;
      Ok(json!({"contact_set": id, "name": name}))
    })(),
    "contact_set.enable" => (|| {
      let id = contact_set_id(s, p)?;
      let on = p.get("enabled").and_then(Value::as_bool).unwrap_or(true);
      s.edit(|d| {
        for c in d.contact_sets.iter_mut().filter(|c| c.id == id) {
          c.enabled = on;
        }
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "contact_set.delete" => (|| {
      let id = contact_set_id(s, p)?;
      s.edit(|d| {
        d.contact_sets.retain(|c| c.id != id);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "contact_set.list" => Ok(json!(
      s.design
        .contact_sets
        .iter()
        .map(|c| json!({"contact_set": c.id, "name": c.name, "components": c.components.iter().map(|x| x.0).collect::<Vec<_>>(), "enabled": c.enabled}))
        .collect::<Vec<_>>()
    )),
    "component.insert" => insert(s, p),
    "component.links" => (|| {
      if !has_inserts(&s.design) {
        return Ok(json!([]));
      }
      let st = hub::open_store(s)?;
      Ok(json!(links(s, &st)))
    })(),
    "component.update_link" => update_links(s, p),
    "component.configuration" => component_configuration(s, p),
    "component.break_link" => (|| {
      let fid = inserted_by(s, p)?;
      s.edit(|d| {
        if let Some(FeatureKind::InsertDesign { source, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) {
          *source = None;
        }
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    _ => return crate::subassembly::run(s, name, p),
  })
}

pub(crate) fn feature_json(s: &Session, id: FeatureId) -> Value {
  json!({
    "feature": id.0,
    "name": s.design.feature(id).map(|f| f.name.clone()),
    "error": s.model.status(id).and_then(|x| x.error.clone()),
  })
}

/// Optional `name` for a new feature.
pub(crate) fn rename(s: &mut Session, id: FeatureId, p: &Value) -> Result<(), CmdError> {
  if let Some(n) = opt_str(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
    let n = n.to_string();
    s.edit(|d| {
      if let Some(f) = d.feature_mut(id) {
        f.name = n;
      }
      Ok(())
    })?;
  }
  Ok(())
}

/// `components: [id]`, or the components of `bodies: [id]` (in pick order, no repeats).
pub(crate) fn components_of(s: &Session, p: &Value) -> Result<Vec<ComponentId>, CmdError> {
  let mut out = Vec::new();
  for c in ids(p, "components") {
    let id = ComponentId(c);
    s.design.component(id).ok_or(CmdError::new(format!("no component {c}")))?;
    if !out.contains(&id) {
      out.push(id);
    }
  }
  for b in ids(p, "bodies") {
    let c = s.design.component_of(BodyId(b)).ok_or(CmdError::new(format!("body {b} isn't in a component")))?;
    if !out.contains(&c.id) {
      out.push(c.id);
    }
  }
  Ok(out)
}

fn contact_set_id(s: &Session, p: &Value) -> Result<u32, CmdError> {
  let id = req_u32(p, "contact_set")?;
  s.design.contact_sets.iter().find(|c| c.id == id).ok_or(CmdError::new(format!("no contact set {id}")))?;
  Ok(id)
}

/// A feature of the given kind by id or name.
pub(crate) fn feature_by(
  s: &Session,
  v: Option<&Value>,
  what: &str,
  ok: fn(&FeatureKind) -> bool,
) -> Result<FeatureId, CmdError> {
  let v = v.ok_or(CmdError::new(format!("missing '{what}'")))?;
  let f = match v {
    Value::String(n) => s.design.features.iter().find(|f| f.name == *n),
    _ => v.as_u64().and_then(|i| s.design.feature(FeatureId(i as u32))),
  };
  match f {
    Some(f) if ok(&f.kind) => Ok(f.id),
    Some(f) => Err(CmdError::new(format!("{} is not a {what}", f.name))),
    None => Err(CmdError::new(format!("no {what} {v}"))),
  }
}

/// A saved joint origin by id or name.
pub(crate) fn origin_id(s: &Session, v: &Value) -> Result<FeatureId, CmdError> {
  feature_by(s, Some(v), "joint origin", |k| matches!(k, FeatureKind::JointOrigin { .. }))
}

/// The component a saved joint origin is on.
pub(crate) fn origin_component(s: &Session, origin: FeatureId) -> Option<ComponentId> {
  match s.design.feature(origin).map(|f| &f.kind) {
    Some(FeatureKind::JointOrigin { component, .. }) => Some(*component),
    _ => None,
  }
}

fn joint_origin(s: &mut Session, p: &Value) -> CmdResult {
  let geometry = crate::assembly::geometry(s, p)?;
  let component = match p.get("component") {
    Some(c) => {
      let id = ComponentId(c.as_u64().ok_or(CmdError::new("'component' must be a component id"))? as u32);
      s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
      id
    }
    None => {
      let b = geometry.body().ok_or(CmdError::new("missing 'component' (the origin isn't on a body)"))?;
      s.design.component_of(b).map(|c| c.id).ok_or(CmdError::new(format!("{} isn't in a component", s.body_name(b))))?
    }
  };
  let e = |k: &str| opt_expr(p, k).unwrap_or_else(|| "0".into());
  let flip = p.get("flip").and_then(Value::as_bool).unwrap_or(false);
  let kind = FeatureKind::JointOrigin {
    component,
    geometry,
    angle: e("angle"),
    offset: e("offset"),
    offset_x: e("offset_x"),
    offset_y: e("offset_y"),
    flip,
  };
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  rename(s, id, p)?;
  let mut out = feature_json(s, id);
  if let Some((_, m)) = s.model.joint_origins.get(&id) {
    out["origin"] = json!(m.w_axis.truncate().to_array());
    out["z"] = json!(m.z_axis.truncate().to_array());
  }
  Ok(out)
}

fn motion_link(s: &mut Session, p: &Value) -> CmdResult {
  let is_joint = |k: &FeatureKind| matches!(k, FeatureKind::Joint { .. });
  let leader = feature_by(s, p.get("leader"), "joint", is_joint)?;
  let follower = feature_by(s, p.get("follower"), "joint", is_joint)?;
  let kind_of = |id: FeatureId| match s.design.feature(id).map(|f| &f.kind) {
    Some(FeatureKind::Joint { kind, .. }) => *kind,
    _ => JointKind::Rigid,
  };
  let axis = |k: &str, j: FeatureId| -> Result<MotionAxis, CmdError> {
    match opt_str(p, k).filter(|a| !a.is_empty()) {
      Some(a) => MotionAxis::parse(a).ok_or(CmdError::new(format!("'{k}' is rotation, slide, slide2 or pitch"))),
      None => MotionAxis::first(kind_of(j)).ok_or(CmdError::new("a rigid joint has no motion to link")),
    }
  };
  let (lm, fm) = (axis("leader_motion", leader)?, axis("follower_motion", follower)?);
  let default = |m: MotionAxis| if m.is_angle() { "360 deg" } else { "10 mm" }.to_string();
  let value = |k: &str, m: MotionAxis| opt_expr(p, k).unwrap_or_else(|| default(m));
  let kind = FeatureKind::MotionLink {
    leader,
    leader_motion: lm,
    follower,
    follower_motion: fm,
    leader_value: value("leader_value", lm),
    follower_value: value("follower_value", fm),
    reverse: p.get("reverse").and_then(Value::as_bool).unwrap_or(false),
  };
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  rename(s, id, p)?;
  Ok(feature_json(s, id))
}

// ---- contact sets ----

/// Do any enabled contact sets apply?
pub(crate) fn has_contacts(d: &Design) -> bool {
  d.contact_sets.iter().any(|c| c.enabled && c.components.len() >= 2)
}

/// Bodies of different components in the same enabled contact set that overlap.
pub(crate) fn contacts(d: &Design, bodies: &[Body]) -> Vec<(BodyId, BodyId)> {
  let set_of = |b: BodyId| -> Vec<(u32, ComponentId)> {
    let Some(c) = d.component_of(b) else { return Vec::new() };
    d.contact_sets.iter().filter(|s| s.enabled && s.components.contains(&c.id)).map(|s| (s.id, c.id)).collect()
  };
  let pair_ok = |a: BodyId, b: BodyId| {
    let (sa, sb) = (set_of(a), set_of(b));
    sa.iter().any(|(set, ca)| sb.iter().any(|(sb, cb)| sb == set && cb != ca))
  };
  crate::assembly::overlaps(bodies, &pair_ok).into_iter().map(|(a, b, _)| (a, b)).collect()
}

/// Regenerates designs that differ in a joint's motion values, for contact searches.
pub(crate) struct Trial<'a> {
  pub design: &'a Design,
  pub joint: FeatureId,
  regen: Regenerator,
  /// Contacts at the start, which don't stop the motion.
  before: Vec<(BodyId, BodyId)>,
}

impl<'a> Trial<'a> {
  pub fn new(s: &'a Session, joint: FeatureId) -> Trial<'a> {
    Trial::over(&s.design, joint, contacts(&s.design, &s.model.bodies))
  }

  /// Trials of `design`'s joint or Move feature `joint`, where the contacts `before` were there
  /// already.
  pub fn over(design: &'a Design, joint: FeatureId, before: Vec<(BodyId, BodyId)>) -> Trial<'a> {
    Trial { design, joint, regen: Regenerator::default(), before }
  }

  /// The design with these motion values (field → expression; a Move's "x", "y" and "z"),
  /// regenerated.
  pub fn model(&mut self, set: &Map<String, Value>) -> (Design, Model) {
    let mut d = self.design.clone();
    match d.feature_mut(self.joint).map(|f| &mut f.kind) {
      Some(FeatureKind::Joint { rotation, slide, slide2, pitch, .. }) => {
        for (k, v) in set {
          let slot = match k.as_str() {
            "rotation" => &mut *rotation,
            "slide" => &mut *slide,
            "slide2" => &mut *slide2,
            _ => &mut *pitch,
          };
          if let Some(v) = v.as_str() {
            *slot = v.to_string();
          }
        }
      }
      Some(FeatureKind::Move { translate, .. }) => {
        for (k, v) in set {
          let i = match k.as_str() {
            "x" => 0,
            "y" => 1,
            _ => 2,
          };
          if let Some(v) = v.as_str() {
            translate[i] = v.to_string();
          }
        }
      }
      _ => {}
    }
    let m = self.regen.regenerate(&mut d);
    (d, m)
  }

  /// New contacts with these motion values (none if the joint fails there).
  pub fn clash(&mut self, set: &Map<String, Value>) -> Vec<(BodyId, BodyId)> {
    let (d, m) = self.model(set);
    if m.status(self.joint).is_some_and(|st| st.error.is_some()) {
      return Vec::new();
    }
    contacts(&d, &m.bodies).into_iter().filter(|c| !self.before.contains(c)).collect()
  }
}

/// Motion values as expressions: `from + t (to - from)` per field (deg / mm).
pub(crate) fn between(from: &[(String, f64)], to: &[(String, f64)], t: f64) -> Map<String, Value> {
  from
    .iter()
    .zip(to)
    .map(|((k, a), (_, b))| {
      let v = a + (b - a) * t;
      let e =
        if matches!(k.as_str(), "rotation" | "pitch") { format!("{} deg", v.to_degrees()) } else { format!("{v} mm") };
      (k.clone(), json!(e))
    })
    .collect()
}

/// The first contact on the way from `from` to `to` (looked for in steps, so a motion can't
/// jump through a thin part): how far is clear (0 to 1) and the bodies that touch there.
pub(crate) fn first_contact(
  trial: &mut Trial,
  from: &[(String, f64)],
  to: &[(String, f64)],
) -> Option<(f64, Vec<(BodyId, BodyId)>)> {
  const STEPS: usize = 12;
  for i in 1..=STEPS {
    let t = i as f64 / STEPS as f64;
    let clash = trial.clash(&between(from, to, t));
    if !clash.is_empty() {
      let lo = (i - 1) as f64 / STEPS as f64;
      return Some((clear_between(trial, from, to, lo, t), clash));
    }
  }
  None
}

/// Bisects for the furthest point short of a contact between `from` (clear) and `to` (clashing).
pub(crate) fn last_clear(trial: &mut Trial, from: &[(String, f64)], to: &[(String, f64)]) -> f64 {
  clear_between(trial, from, to, 0.0, 1.0)
}

/// [`last_clear`] between fractions `lo` (clear) and `hi` (clashing) of the way.
fn clear_between(trial: &mut Trial, from: &[(String, f64)], to: &[(String, f64)], lo: f64, hi: f64) -> f64 {
  let (mut lo, mut hi) = (lo, hi);
  for _ in 0..14 {
    let mid = (lo + hi) / 2.0;
    if trial.clash(&between(from, to, mid)).is_empty() {
      lo = mid;
    } else {
      hi = mid;
    }
  }
  lo
}

// ---- inserted designs ----

/// A hub design version as stored.
fn source_design(st: &Store, item: &str, version: u32) -> Result<Design, CmdError> {
  let it = st.item(item)?;
  if it.kind != ItemKind::Design {
    return Err(CmdError::new(format!("{} is a file, not a design; open it and save it as a design first", it.name)));
  }
  let text = String::from_utf8(st.read_version(item, Some(version))?)
    .map_err(|_| CmdError::new("the stored design isn't UTF-8"))?;
  Ok(Design::from_json(&text)?)
}

/// The bodies of a hub design version in one of its configurations (`None`: as modelled), as
/// BRep text, with its first physical material and its assembly structure.
fn source_bodies(
  st: &Store,
  item: &str,
  version: u32,
  configuration: Option<&str>,
) -> Result<(Vec<String>, String, Source), CmdError> {
  let it = st.item(item)?;
  let d = source_design(st, item, version)?;
  let d = d.with_configuration(configuration).map_err(|e| CmdError::new(format!("{} v{version}: {e}", it.name)))?;
  let src = Session::new(d);
  let mut breps = Vec::new();
  for b in &src.model.bodies {
    breps.push(b.shape.to_brep().map_err(|e| CmdError::new(e.to_string()))?);
  }
  if breps.is_empty() {
    return Err(CmdError::new(format!("{} v{version} has no bodies", it.name)));
  }
  let material = src.design.components.iter().map(|c| c.material.clone()).find(|m| !m.is_empty()).unwrap_or_default();
  let structure = Source::of(&src)?;
  Ok((breps, material, structure))
}

/// Body ids an insert feature gives its bodies (see `new_body` in the regeneration).
fn insert_bodies(fid: FeatureId, n: usize) -> Vec<BodyId> {
  (0..n as u32).map(|k| BodyId(fid.0 * 1000 + k)).collect()
}

fn insert(s: &mut Session, p: &Value) -> CmdResult {
  let item = req_str(p, "item")?;
  let st = hub::open_store(s)?;
  let item = st.item(item)?.clone();
  let version = match p.get("version").and_then(Value::as_u64) {
    Some(v) => v as u32,
    None => item.latest().map(|v| v.number).ok_or(CmdError::new("the item has no versions"))?,
  };
  item.version(version).ok_or(CmdError::new(format!("{} has no version {version}", item.name)))?;
  let configuration = match p.get("configuration") {
    None | Some(Value::Null) => None,
    Some(v) => Some(v.as_str().ok_or(CmdError::new("'configuration' is a name"))?.to_string()),
  };
  let (breps, material, structure) = source_bodies(&st, &item.id, version, configuration.as_deref())?;
  let t = match p.get("translate").and_then(Value::as_array) {
    Some(a) if a.len() == 3 => [expr_of(&a[0])?, expr_of(&a[1])?, expr_of(&a[2])?],
    Some(_) => return Err(CmdError::new("translate needs [x, y, z]")),
    None => ["0".into(), "0".into(), "0".into()],
  };
  let linked = p.get("link").and_then(Value::as_bool).unwrap_or(true);
  let name = opt_str(p, "name").map(str::to_string).unwrap_or_else(|| item.name.clone());
  let n = breps.len();
  let source = linked.then(|| HubSource { item: item.id.clone(), version, configuration: configuration.clone() });
  let (fid, cid) = s.edit(|d| {
    let fid =
      d.add_feature(FeatureKind::InsertDesign { name: name.clone(), source, breps, translate: t, sub: None })?;
    let cid = d.add_component(&name, insert_bodies(fid, n))?;
    if let Some(c) = d.component_mut(cid) {
      c.material = material;
    }
    structure.attach(d, fid, cid)?;
    Ok((fid, cid))
  })?;
  let mut out = feature_json(s, fid);
  out["component"] = json!(cid.0);
  out["version"] = json!(version);
  out["linked"] = json!(linked);
  out["configuration"] = json!(configuration);
  Ok(out)
}

fn has_inserts(d: &Design) -> bool {
  d.features.iter().any(|f| matches!(f.kind, FeatureKind::InsertDesign { .. }))
}

/// The insert feature that made a component (not one of its sub-assembly's components).
pub(crate) fn insert_of(d: &Design, c: ComponentId) -> Option<FeatureId> {
  if let Some(f) = crate::subassembly::insert_with_top(d, c) {
    return Some(f);
  }
  let b = d.component(c)?.bodies.first()?;
  let fid = FeatureId(b.0 / 1000);
  match &d.feature(fid)?.kind {
    FeatureKind::InsertDesign { sub: Some(sub), .. } if sub.components.iter().any(|x| x.id == c) => None,
    FeatureKind::InsertDesign { .. } => Some(fid),
    _ => None,
  }
}

/// The inserted component's feature from `component`.
fn inserted_by(s: &Session, p: &Value) -> Result<FeatureId, CmdError> {
  let c = ComponentId(req_u32(p, "component")?);
  s.design.component(c).ok_or(CmdError::new(format!("no component {}", c.0)))?;
  insert_of(&s.design, c).ok_or(CmdError::new("that component wasn't inserted from another design"))
}

/// Every inserted component with its source and whether a newer version is in the hub.
fn links(s: &Session, st: &Store) -> Vec<Value> {
  s.design
    .components
    .iter()
    .filter_map(|c| {
      let fid = insert_of(&s.design, c.id)?;
      let Some(FeatureKind::InsertDesign { source, .. }) = s.design.feature(fid).map(|f| &f.kind) else { return None };
      let (item, version) = match source {
        Some(h) => (Some(h.item.clone()), Some(h.version)),
        None => (None, None),
      };
      let it = source.as_ref().and_then(|h| st.item(&h.item).ok()).filter(|i| !i.trashed);
      let latest = it.and_then(|i| i.latest()).map(|v| v.number);
      Some(json!({
        "component": c.id.0,
        "name": c.name,
        "feature": fid.0,
        "linked": source.is_some(),
        "item": item,
        "source_name": it.map(|i| i.name.clone()),
        "version": version,
        "configuration": source.as_ref().and_then(|h| h.configuration.clone()),
        "latest": latest,
        "out_of_date": matches!((version, latest), (Some(v), Some(l)) if l > v),
        "missing": source.is_some() && it.is_none(),
      }))
    })
    .collect()
}

/// Updates linked components (one, or all) to their source's newest version.
fn update_links(s: &mut Session, p: &Value) -> CmdResult {
  let only = match p.get("component") {
    Some(_) => Some(inserted_by(s, p)?),
    None => None,
  };
  if !has_inserts(&s.design) {
    return Ok(json!({"updated": []}));
  }
  let st = hub::open_store(s)?;
  let mut updated = Vec::new();
  for l in links(s, &st) {
    let fid = FeatureId(l["feature"].as_u64().unwrap_or(0) as u32);
    if only.is_some_and(|o| o != fid) || !l["linked"].as_bool().unwrap_or(false) {
      continue;
    }
    if l["missing"].as_bool().unwrap_or(false) {
      return Err(CmdError::new(format!("the source of {} is no longer in the hub", l["name"])));
    }
    if !l["out_of_date"].as_bool().unwrap_or(false) {
      continue;
    }
    let item = l["item"].as_str().unwrap_or_default().to_string();
    let latest = l["latest"].as_u64().unwrap_or(0) as u32;
    let configuration = l["configuration"].as_str().map(str::to_string);
    let cid = ComponentId(l["component"].as_u64().unwrap_or(0) as u32);
    reload(s, &st, fid, cid, &item, latest, configuration)?;
    updated.push(json!({"component": cid.0, "version": latest}));
  }
  Ok(json!({"updated": updated}))
}

/// Replaces a linked component's bodies with its source's (`version`, `configuration`).
fn reload(
  s: &mut Session,
  st: &Store,
  fid: FeatureId,
  cid: ComponentId,
  item: &str,
  version: u32,
  configuration: Option<String>,
) -> Result<(), CmdError> {
  let (breps, _, structure) = source_bodies(st, item, version, configuration.as_deref())?;
  let n = breps.len();
  s.edit(|d| {
    if let Some(FeatureKind::InsertDesign { source: Some(h), breps: b, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) {
      h.version = version;
      h.configuration = configuration;
      *b = breps;
    }
    // the source may have gained or lost bodies
    let bodies = insert_bodies(fid, n);
    for c in &mut d.components {
      c.bodies.retain(|b| !bodies.contains(b));
    }
    if let Some(c) = d.component_mut(cid) {
      c.bodies = bodies;
    }
    structure.attach(d, fid, cid)?;
    Ok(())
  })
}

/// Lists the configurations of a linked component's source, or (with `configuration`) switches
/// the component to one (`null`: the source as modelled), at the version it is linked to.
fn component_configuration(s: &mut Session, p: &Value) -> CmdResult {
  let fid = inserted_by(s, p)?;
  let cid = ComponentId(req_u32(p, "component")?);
  let Some(FeatureKind::InsertDesign { source, .. }) = s.design.feature(fid).map(|f| &f.kind) else {
    return Err(CmdError::new("that component wasn't inserted from another design"));
  };
  let h = source.clone().ok_or(CmdError::new(
    "the component's link to its source is broken, so its configuration can't change; insert it again",
  ))?;
  let st = hub::open_store(s)?;
  let src = source_design(&st, &h.item, h.version)?;
  let names: Vec<String> = src.configurations.list.iter().map(|c| c.name.clone()).collect();
  let Some(v) = p.get("configuration") else {
    return Ok(json!({"component": cid.0, "configuration": h.configuration, "configurations": names}));
  };
  let want = match v {
    Value::Null => None,
    v => Some(v.as_str().ok_or(CmdError::new("'configuration' is a name"))?.to_string()),
  };
  if let Some(w) = &want
    && !names.contains(w)
  {
    return Err(CmdError::new(format!("the source has no configuration '{w}' (it has: {})", names.join(", "))));
  }
  if want != h.configuration {
    reload(s, &st, fid, cid, &h.item, h.version, want.clone())?;
  }
  let mut out = feature_json(s, fid);
  out["component"] = json!(cid.0);
  out["configuration"] = json!(want);
  out["configurations"] = json!(names);
  out["bodies"] = json!(s.design.component(cid).map(|c| c.bodies.len()));
  Ok(out)
}
