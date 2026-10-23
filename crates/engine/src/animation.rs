//! The Animation workspace's commands (`animation.*`): storyboards and the actions they record
//! (Transform Components, Restore Home, Auto Explode, Manual Explode, Show/Hide, Appearance's
//! opacity, Create Callout, View), what a storyboard shows at a time, and Publish Video.
//!
//! Storyboards are display only (see [`fox_model::animation`]): recording an action is an undo
//! step but regenerates nothing.

use fox_kernel::glam::{DAffine3, DVec3};
use fox_model::animation::{
  self, ActionKind, AnimCamera, DEFAULT_DURATION, Pose, StoryboardKind, Targets, TransformSource,
};
use fox_model::explode::{ExplodeOrder, ExplodedView};
use fox_model::{BodyId, ComponentId};
use serde_json::{Value, json};

use crate::commands::{opt_str, req_str};
use crate::scene::{SceneBuilder, ViewOptions};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "animation.storyboards" => Ok(storyboards(s)),
    "animation.storyboard.add" => add_storyboard(s, p),
    "animation.transform" => transform(s, p),
    "animation.restore_home" => restore_home(s, p),
    "animation.explode" => auto_explode(s, p),
    "animation.manual_explode" => manual_explode(s, p),
    "animation.visibility" => visibility(s, p),
    "animation.opacity" => opacity(s, p),
    "animation.callout" => callout(s, p),
    "animation.view" => view(s, p),
    "animation.action.edit" => edit_action(s, p),
    "animation.action.delete" => delete_action(s, p),
    "animation.pose" => pose_cmd(s, p),
    "animation.publish" => publish(s, p),
    _ => return None,
  })
}

/// What storyboard `i` shows at `t`.
pub fn pose(s: &Session, i: usize, t: f64) -> Pose {
  s.design.animation.pose(&s.design, i, t, &|b| s.body_visible(b))
}

/// The display options that draw `pose`: bodies moved, shown or hidden and faded as it has them.
pub fn view_options(s: &Session, pose: &Pose) -> ViewOptions {
  ViewOptions {
    transforms: pose.transforms.clone(),
    only_bodies: Some(shown_bodies(s, pose)),
    opacity: faded(pose),
    ..ViewOptions::default()
  }
}

/// The bodies `pose` shows.
pub fn shown_bodies(s: &Session, pose: &Pose) -> Vec<BodyId> {
  s.model.bodies.iter().map(|b| b.id).filter(|b| pose.shows(*b, s.body_visible(*b))).collect()
}

/// The see-through bodies of `pose`, as the scene's opacity list (alpha 0 to 255).
pub fn faded(pose: &Pose) -> Vec<(BodyId, u8)> {
  let mut v: Vec<(BodyId, u8)> =
    pose.opacity.iter().map(|(b, o)| (*b, (o.clamp(0.0, 1.0) * 255.0).round().min(254.0) as u8)).collect();
  v.sort_by_key(|(b, _)| b.0);
  v
}

/// `storyboard?`: an index or a name; the first by default.
fn index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  let list = s.design.animation.listed();
  match p.get("storyboard") {
    None | Some(Value::Null) => Ok(0),
    Some(Value::Number(n)) => {
      let i = n.as_u64().map(|i| i as usize).filter(|i| *i < list.len());
      i.ok_or_else(|| CmdError::new(format!("no storyboard {n} (there are {})", list.len())))
    }
    Some(Value::String(name)) => list
      .iter()
      .position(|sb| sb.name.eq_ignore_ascii_case(name))
      .ok_or_else(|| CmdError::new(format!("no storyboard '{name}'"))),
    Some(_) => Err(CmdError::new("'storyboard' is an index or a name")),
  }
}

/// A non-negative number of seconds, or `default`.
fn seconds(p: &Value, k: &str, default: f64) -> Result<f64, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(default),
    Some(v) => v
      .as_f64()
      .filter(|t| t.is_finite() && *t >= 0.0)
      .ok_or_else(|| CmdError::new(format!("'{k}' must be a number of seconds of at least 0"))),
  }
}

/// `start?` (default: the storyboard's end) and `duration?` (default one second).
fn span(s: &Session, p: &Value, i: usize) -> Result<(f64, f64), CmdError> {
  let end = s.design.animation.listed().get(i).map(|sb| sb.length()).unwrap_or(0.0);
  Ok((seconds(p, "start", end)?, seconds(p, "duration", DEFAULT_DURATION)?))
}

fn vec3(v: &Value, k: &str) -> Result<[f64; 3], CmdError> {
  let a: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
  match a[..] {
    [x, y, z] if [x, y, z].iter().all(|c| c.is_finite()) => Ok([x, y, z]),
    _ => Err(CmdError::new(format!("'{k}' must be three numbers"))),
  }
}

fn opt_vec3(p: &Value, k: &str) -> Result<Option<[f64; 3]>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => vec3(v, k).map(Some),
  }
}

/// `components?: [id | name]` and `bodies?: [id]`.
fn targets(s: &Session, p: &Value) -> Result<Targets, CmdError> {
  let mut t = Targets::default();
  if let Some(list) = p.get("components").filter(|v| !v.is_null()) {
    let list = list.as_array().ok_or(CmdError::new("'components' is a list of component ids or names"))?;
    for c in list {
      let id = c.as_u64().map(|n| ComponentId(n as u32)).or_else(|| {
        let n = c.as_str()?;
        s.design.components.iter().find(|x| x.name.eq_ignore_ascii_case(n)).map(|x| x.id)
      });
      let id =
        id.filter(|id| s.design.component(*id).is_some()).ok_or_else(|| CmdError::new(format!("no component {c}")))?;
      if !t.components.contains(&id) {
        t.components.push(id);
      }
    }
  }
  if p.get("bodies").is_some_and(|v| !v.is_null()) {
    for b in crate::commands::ids(p, "bodies").into_iter().map(BodyId) {
      if s.model.body(b).is_none() {
        return Err(CmdError::new(format!("no body {}", b.0)));
      }
      if !t.bodies.contains(&b) {
        t.bodies.push(b);
      }
    }
  }
  Ok(t)
}

fn req_targets(s: &Session, p: &Value) -> Result<Targets, CmdError> {
  let t = targets(s, p)?;
  if t.is_empty() {
    return Err(CmdError::new("select the components (components or bodies)"));
  }
  Ok(t)
}

/// Records actions on storyboard `i` (one undo step); returns their ids and where they end.
fn record(s: &mut Session, i: usize, actions: Vec<(f64, f64, ActionKind)>) -> CmdResult {
  if actions.is_empty() {
    return Err(CmdError::new("nothing to record: the components are where the action would put them"));
  }
  let ids = s.edit_deferred(|d| {
    let mut ids = Vec::new();
    for (start, duration, kind) in actions {
      ids.push(d.animation.add_action(i, start, duration, kind).ok_or(CmdError::new("no such storyboard"))?);
    }
    Ok(ids)
  })?;
  let sb = &s.design.animation.storyboards[i];
  let end = ids.iter().filter_map(|id| sb.action(*id)).map(|a| a.end()).fold(0.0, f64::max);
  Ok(json!({"storyboard": i, "actions": ids, "end": end, "length": sb.length()}))
}

fn storyboards(s: &Session) -> Value {
  let list: Vec<Value> = s
    .design
    .animation
    .listed()
    .iter()
    .enumerate()
    .map(|(i, sb)| {
      let actions: Vec<Value> = sb
        .actions
        .iter()
        .map(|a| {
          let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
          v["label"] = json!(a.kind.label());
          v["end"] = json!(a.end());
          v
        })
        .collect();
      json!({"index": i, "name": sb.name, "type": sb.kind, "length": sb.length(), "actions": actions})
    })
    .collect();
  json!({"storyboards": list})
}

fn add_storyboard(s: &mut Session, p: &Value) -> CmdResult {
  let kind = match opt_str(p, "type") {
    None | Some("clean") => StoryboardKind::Clean,
    Some("start_from_end") => StoryboardKind::StartFromEnd,
    Some(o) => return Err(CmdError::new(format!("unknown storyboard type '{o}' (clean, start_from_end)"))),
  };
  let name = opt_str(p, "name").map(str::trim).filter(|n| !n.is_empty()).map(String::from);
  let i = s.edit_deferred(|d| Ok(d.animation.add_storyboard(name, kind)))?;
  Ok(json!({"index": i, "name": s.design.animation.storyboards[i].name}))
}

/// A body's bounding box (from its display mesh) moved by `m`.
fn moved_bounds(s: &Session, b: BodyId, m: &DAffine3) -> Option<(DVec3, DVec3)> {
  let mesh = s.model.body(b)?.mesh()?;
  let mut it = mesh.positions.iter().map(|p| m.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)));
  let first = it.next()?;
  Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// The centre of the bodies' box where `pose` has them.
pub fn centre(s: &Session, pose: &Pose, bodies: &[BodyId]) -> Option<DVec3> {
  let mut acc: Option<(DVec3, DVec3)> = None;
  for b in bodies {
    if let Some((lo, hi)) = moved_bounds(s, *b, &pose.transform(*b)) {
      let (a, c) = acc.get_or_insert((lo, hi));
      *a = a.min(lo);
      *c = c.max(hi);
    }
  }
  acc.map(|(lo, hi)| (lo + hi) / 2.0)
}

fn transform(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let targets = req_targets(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let translate = opt_vec3(p, "translate")?.unwrap_or([0.0; 3]);
  let rotate = opt_vec3(p, "rotate")?.unwrap_or([0.0; 3]);
  let pivot = match opt_vec3(p, "pivot")? {
    Some(v) => v,
    None => {
      let pose = pose(s, i, start);
      centre(s, &pose, &targets.bodies(&s.design)).unwrap_or(DVec3::ZERO).to_array()
    }
  };
  if translate.iter().chain(&rotate).all(|v| v.abs() < 1e-12) {
    return Err(CmdError::new("the components haven't been moved"));
  }
  let kind = ActionKind::Transform { targets, translate, rotate, pivot, source: TransformSource::Transform };
  record(s, i, vec![(start, duration, kind)])
}

/// The moving units below `targets`: each component on its own (its own bodies) and each body
/// of the root.
fn units(s: &Session, targets: &Targets) -> Vec<(Targets, Vec<BodyId>)> {
  let mut out: Vec<(Targets, Vec<BodyId>)> = Vec::new();
  for root in &targets.components {
    for c in fox_model::explode::subtree(&s.design, *root) {
      let Some(comp) = s.design.component(c) else { continue };
      if out.iter().any(|(t, _)| t.components == [c]) || comp.bodies.is_empty() {
        continue;
      }
      out.push((Targets { components: vec![c], bodies: vec![] }, comp.bodies.clone()));
    }
  }
  for b in &targets.bodies {
    if s.design.component_of(*b).is_none() && !out.iter().any(|(t, _)| t.bodies == [*b]) {
      out.push((Targets { components: vec![], bodies: vec![*b] }, vec![*b]));
    }
  }
  out
}

/// Restore Home: actions that take the components (by default every moved one) back to where
/// the design has them.
fn restore_home(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let now = pose(s, i, start);
  let mut t = targets(s, p)?;
  if t.is_empty() {
    t.components = s.design.components.iter().filter(|c| c.parent.is_none()).map(|c| c.id).collect();
    t.bodies = s.model.bodies.iter().map(|b| b.id).filter(|b| s.design.component_of(*b).is_none()).collect();
  }
  let mut actions = Vec::new();
  for (unit, bodies) in units(s, &t) {
    let Some(m) = bodies.iter().map(|b| now.transform(*b)).find(|m| !m.abs_diff_eq(DAffine3::IDENTITY, 1e-9)) else {
      continue;
    };
    let Some(c) = centre(s, &now, &bodies) else { continue };
    let back = m.inverse();
    let (_, q, _) = back.to_scale_rotation_translation();
    let translate = (back.transform_point3(c) - c).to_array();
    let kind = ActionKind::Transform {
      targets: unit,
      translate,
      rotate: animation::angles(q),
      pivot: c.to_array(),
      source: TransformSource::RestoreHome,
    };
    actions.push((start, duration, kind));
  }
  if actions.is_empty() {
    return Err(CmdError::new("the components are already home"));
  }
  record(s, i, actions)
}

/// The components Auto Explode spreads: the children of one picked component that has some,
/// else the picked ones, else the top-level ones.
fn explode_set(s: &Session, picked: &[ComponentId]) -> Vec<ComponentId> {
  let children = |c: ComponentId| -> Vec<ComponentId> {
    s.design.components.iter().filter(|x| x.parent == Some(c)).map(|x| x.id).collect()
  };
  match picked {
    [] => s.design.components.iter().filter(|c| c.parent.is_none()).map(|c| c.id).collect(),
    [one] if !children(*one).is_empty() => children(*one),
    many => many.to_vec(),
  }
}

/// Auto Explode (`levels: one | all`): each level's components spread apart along the direction
/// they lie in, one action each, the levels one after another.
fn auto_explode(s: &mut Session, p: &Value) -> CmdResult {
  if s.design.components.is_empty() {
    return Err(CmdError::new("there are no components to explode"));
  }
  let i = index(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let all = match opt_str(p, "levels") {
    None | Some("one") => false,
    Some("all") => true,
    Some(o) => return Err(CmdError::new(format!("unknown levels '{o}' (one, all)"))),
  };
  let scale = match p.get("scale") {
    None | Some(Value::Null) => 1.0,
    Some(v) => v.as_f64().filter(|f| f.is_finite() && *f > 0.0).ok_or(CmdError::new("'scale' must be above 0"))?,
  };
  let picked = targets(s, p)?.components;
  let now = pose(s, i, start);
  let mut level = explode_set(s, &picked);
  let mut actions = Vec::new();
  let mut k = 0usize;
  while !level.is_empty() && k < 64 {
    let mut groups: Vec<Vec<ComponentId>> = Vec::new();
    for c in &level {
      let parent = s.design.component(*c).and_then(|x| x.parent);
      match groups.iter_mut().find(|g| s.design.component(g[0]).and_then(|x| x.parent) == parent) {
        Some(g) => g.push(*c),
        None => groups.push(vec![*c]),
      }
    }
    let at = start + k as f64 * duration;
    let mut any = false;
    for g in groups.iter().filter(|g| g.len() > 1) {
      let view = ExplodedView {
        name: String::new(),
        components: g.clone(),
        direction: None,
        factor: scale,
        order: ExplodeOrder::Position,
      };
      let off = view.offsets(&s.design, |b| moved_bounds(s, b, &now.transform(b)));
      for (c, bodies) in view.units(&s.design) {
        let o = bodies.iter().find_map(|b| off.get(b)).copied().unwrap_or(DVec3::ZERO);
        if o.length() < 1e-9 {
          continue;
        }
        let kind = ActionKind::Transform {
          targets: Targets { components: vec![c], bodies: vec![] },
          translate: o.to_array(),
          rotate: [0.0; 3],
          pivot: [0.0; 3],
          source: TransformSource::AutoExplode,
        };
        actions.push((at, duration, kind));
        any = true;
      }
    }
    if !all {
      break;
    }
    if any {
      k += 1;
    }
    level = level
      .iter()
      .flat_map(|c| s.design.components.iter().filter(move |x| x.parent == Some(*c)).map(|x| x.id))
      .collect();
  }
  if actions.is_empty() {
    return Err(CmdError::new("there is nothing to explode: it takes two components side by side"));
  }
  record(s, i, actions)
}

/// Manual Explode: the components spread along `direction`, sorted by where they lie that way;
/// the i-th of N moves i/N of `distance`.
fn manual_explode(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let t = req_targets(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let dir = match p.get("direction") {
    Some(Value::String(a)) => match a.to_ascii_lowercase().as_str() {
      "x" => DVec3::X,
      "y" => DVec3::Y,
      "z" => DVec3::Z,
      "-x" => -DVec3::X,
      "-y" => -DVec3::Y,
      "-z" => -DVec3::Z,
      o => return Err(CmdError::new(format!("unknown direction '{o}' (x, y, z, -x, -y, -z or [x, y, z])"))),
    },
    Some(v @ Value::Array(_)) => DVec3::from(vec3(v, "direction")?),
    _ => return Err(CmdError::new("'direction' is x, y, z, -x, -y, -z or [x, y, z]")),
  };
  let dir = dir.try_normalize().ok_or(CmdError::new("'direction' can't be zero"))?;
  let distance =
    p.get("distance").and_then(Value::as_f64).filter(|d| d.is_finite()).ok_or(CmdError::new("give a distance"))?;
  if distance.abs() < 1e-9 {
    return Err(CmdError::new("the distance is 0"));
  }
  let now = pose(s, i, start);
  let actions = manual_parts(s, &now, &t, dir, distance)
    .into_iter()
    .map(|(targets, by)| {
      let kind = ActionKind::Transform {
        targets,
        translate: by.to_array(),
        rotate: [0.0; 3],
        pivot: [0.0; 3],
        source: TransformSource::ManualExplode,
      };
      (start, duration, kind)
    })
    .collect();
  record(s, i, actions)
}

/// Manual Explode's moves from `now`: the picked components (each carrying those below it) and
/// root bodies sorted by where they lie along `dir`, the i-th of N moving i/N of `distance`.
pub fn manual_parts(s: &Session, now: &Pose, t: &Targets, dir: DVec3, distance: f64) -> Vec<(Targets, DVec3)> {
  let nested =
    |c: &ComponentId| t.components.iter().any(|o| o != c && fox_model::explode::subtree(&s.design, *o).contains(c));
  let mut parts: Vec<(Targets, f64)> = Vec::new();
  for c in t.components.iter().filter(|c| !nested(c)) {
    let bodies = fox_model::explode::subtree_bodies(&s.design, *c);
    let at = centre(s, now, &bodies).map(|c| c.dot(dir)).unwrap_or(0.0);
    parts.push((Targets { components: vec![*c], bodies: vec![] }, at));
  }
  for b in &t.bodies {
    let at = centre(s, now, &[*b]).map(|c| c.dot(dir)).unwrap_or(0.0);
    parts.push((Targets { components: vec![], bodies: vec![*b] }, at));
  }
  parts.sort_by(|a, b| a.1.total_cmp(&b.1));
  let n = parts.len() as f64;
  parts.into_iter().enumerate().map(|(k, (t, _))| (t, dir * distance * (k as f64 + 1.0) / n)).collect()
}

fn visibility(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let t = req_targets(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let show = match p.get("show") {
    Some(Value::Bool(b)) => *b,
    None | Some(Value::Null) => {
      // toggles: hides what shows at the start, shows what is hidden
      let now = pose(s, i, start);
      let first = t.bodies(&s.design).first().copied();
      !first.is_some_and(|b| now.shows(b, s.body_visible(b)))
    }
    Some(_) => return Err(CmdError::new("'show' is true or false")),
  };
  record(s, i, vec![(start, duration, ActionKind::Visibility { targets: t, show })])
}

fn opacity(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let t = req_targets(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let o = p
    .get("opacity")
    .and_then(Value::as_f64)
    .filter(|o| (0.0..=1.0).contains(o))
    .ok_or(CmdError::new("'opacity' is a number from 0 to 1"))?;
  record(s, i, vec![(start, duration, ActionKind::Opacity { targets: t, opacity: o })])
}

fn callout(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let text = req_str(p, "text")?.trim().to_string();
  if text.is_empty() {
    return Err(CmdError::new("a callout needs text"));
  }
  let start = seconds(p, "start", 0.0)?;
  let point = DVec3::from(vec3(p.get("point").unwrap_or(&Value::Null), "point")?);
  let body = match p.get("body").and_then(Value::as_u64).map(|b| BodyId(b as u32)) {
    Some(b) if s.model.body(b).is_none() => return Err(CmdError::new(format!("no body {}", b.0))),
    b => b,
  };
  // the point where the body is in the design, so the callout moves with it
  let anchor = match body {
    Some(b) => pose(s, i, start).transform(b).inverse().transform_point3(point),
    None => point,
  };
  record(s, i, vec![(start, 0.0, ActionKind::Callout { anchor: anchor.to_array(), body, text })])
}

fn camera(v: &Value, k: &str) -> Result<AnimCamera, CmdError> {
  let num = |f: &str| v.get(f).and_then(Value::as_f64).filter(|x| x.is_finite());
  let target = vec3(v.get("target").unwrap_or(&Value::Null), &format!("{k}.target"))?;
  let (Some(yaw), Some(pitch), Some(distance)) = (num("yaw"), num("pitch"), num("distance")) else {
    return Err(CmdError::new(format!("'{k}' is {{target, yaw, pitch, distance, fov?, ortho?}} (radians, mm)")));
  };
  Ok(AnimCamera {
    target,
    yaw,
    pitch,
    distance: distance.max(1e-3),
    fov: num("fov").unwrap_or(30f64.to_radians()),
    ortho: v.get("ortho").and_then(Value::as_bool).unwrap_or(false),
  })
}

/// View: the camera glides to `to` (from `from`, else from where the storyboard has it).
fn view(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let (start, duration) = span(s, p, i)?;
  let to = camera(p.get("to").unwrap_or(&Value::Null), "to")?;
  let from = match p.get("from").filter(|v| !v.is_null()) {
    Some(v) => camera(v, "from")?,
    None => pose(s, i, start).camera.unwrap_or(to),
  };
  record(s, i, vec![(start, duration, ActionKind::View { from, to })])
}

fn action_index(s: &Session, p: &Value) -> Result<(usize, u32), CmdError> {
  let i = index(s, p)?;
  let id = crate::commands::req_u32(p, "action")?;
  if s.design.animation.listed().get(i).and_then(|sb| sb.action(id)).is_none() {
    return Err(CmdError::new(format!("no action {id} in that storyboard")));
  }
  Ok((i, id))
}

/// Moves an action (`start`), changes its length (`duration`) or a callout's `text`.
fn edit_action(s: &mut Session, p: &Value) -> CmdResult {
  let (i, id) = action_index(s, p)?;
  let start = p.get("start").map(|_| seconds(p, "start", 0.0)).transpose()?;
  let duration = p.get("duration").map(|_| seconds(p, "duration", 0.0)).transpose()?;
  let text = opt_str(p, "text").map(|t| t.trim().to_string());
  s.edit_deferred(|d| {
    let a = d.animation.storyboards[i].actions.iter_mut().find(|a| a.id == id).ok_or(CmdError::new("no action"))?;
    if let Some(t) = start {
      a.start = t;
    }
    if let Some(t) = duration {
      a.duration = t;
    }
    if let Some(t) = text {
      match &mut a.kind {
        ActionKind::Callout { text, .. } if !t.is_empty() => *text = t,
        ActionKind::Callout { .. } => return Err(CmdError::new("a callout needs text")),
        _ => return Err(CmdError::new("only a callout has text")),
      }
    }
    Ok(())
  })?;
  let a = s.design.animation.storyboards[i].action(id).cloned();
  Ok(json!({"action": a, "length": s.design.animation.storyboards[i].length()}))
}

fn delete_action(s: &mut Session, p: &Value) -> CmdResult {
  let (i, id) = action_index(s, p)?;
  s.edit_deferred(|d| {
    d.animation.storyboards[i].actions.retain(|a| a.id != id);
    Ok(())
  })?;
  Ok(json!({"deleted": id, "length": s.design.animation.storyboards[i].length()}))
}

fn pose_json(p: &Pose) -> Value {
  let mut moved: Vec<(&BodyId, &DAffine3)> = p.transforms.iter().collect();
  moved.sort_by_key(|(b, _)| b.0);
  let transforms: Vec<Value> = moved
    .iter()
    .map(|(b, m)| {
      let (_, q, t) = m.to_scale_rotation_translation();
      json!({"body": b.0, "translation": t.to_array(), "rotation": animation::angles(q)})
    })
    .collect();
  let mut visible: Vec<(&BodyId, &bool)> = p.visible.iter().collect();
  visible.sort_by_key(|(b, _)| b.0);
  let callouts: Vec<Value> =
    p.callouts.iter().map(|c| json!({"action": c.action, "anchor": c.anchor.to_array(), "text": c.text})).collect();
  json!({
    "transforms": transforms,
    "visible": visible.iter().map(|(b, v)| json!({"body": b.0, "visible": v})).collect::<Vec<_>>(),
    "opacity": faded(p).iter().map(|(b, a)| json!({"body": b.0, "opacity": *a as f64 / 255.0})).collect::<Vec<_>>(),
    "callouts": callouts,
    "camera": p.camera,
  })
}

fn pose_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let t = seconds(p, "time", 0.0)?;
  let mut out = pose_json(&pose(s, i, t));
  out["time"] = json!(t);
  Ok(out)
}

/// Publish Video: `{path, scope?: current | all, storyboard?, width?, height?, fps?, camera?}`
/// renders the storyboards (the current one, or all in turn) frame by frame and writes them as a
/// Motion JPEG `.avi`. Frames show the camera of the storyboards' views, else `camera` (default:
/// the whole design fitted).
fn publish(s: &mut Session, p: &Value) -> CmdResult {
  let path = std::path::PathBuf::from(req_str(p, "path")?);
  let size = |k: &str, d: u64| p.get(k).and_then(Value::as_u64).unwrap_or(d).clamp(16, 4096) as u32;
  // Motion JPEG wants even sizes
  let (w, h) = (size("width", 1280) & !1, size("height", 720) & !1);
  let fps = p.get("fps").and_then(Value::as_u64).unwrap_or(30).clamp(1, 120) as u32;
  let boards: Vec<usize> = match opt_str(p, "scope") {
    Some("all") => (0..s.design.animation.listed().len()).collect(),
    None | Some("current") => vec![index(s, p)?],
    Some(o) => return Err(CmdError::new(format!("unknown scope '{o}' (current, all)"))),
  };
  let mut base = fox_render::Camera::default();
  match p.get("camera").filter(|v| !v.is_null()) {
    Some(v) => {
      let c = camera(v, "camera")?;
      (base.target, base.yaw, base.pitch, base.distance, base.fov, base.ortho) =
        (c.target, c.yaw, c.pitch, c.distance, c.fov, c.ortho);
    }
    None => {
      let (lo, hi) = s.model.bounds().unwrap_or((DVec3::splat(-50.0), DVec3::splat(50.0)));
      base.fit(lo, hi, w as f64 / h as f64);
    }
  }
  let mut builder = SceneBuilder::default();
  let mut frames: Vec<Vec<u8>> = Vec::new();
  for i in boards {
    let length = s.design.animation.listed()[i].length();
    let n = ((length * fps as f64).ceil() as usize).max(1);
    for k in 0..=n {
      let t = (k as f64 / fps as f64).min(length);
      let pose = pose(s, i, t);
      let opts = ViewOptions { hide_sketches: true, hide_grid: true, ..view_options(s, &pose) };
      let (scene, _) = builder.build(s, &opts);
      let mut cam = base;
      if let Some(c) = pose.camera {
        (cam.target, cam.yaw, cam.pitch, cam.distance, cam.fov, cam.ortho) =
          (c.target, c.yaw, c.pitch, c.distance, c.fov, c.ortho);
      }
      let frame = fox_render::render(&scene, &cam, w, h);
      frames.push(jpeg(&frame.rgba, w, h)?);
    }
  }
  let avi = mjpeg_avi(&frames, w, h, fps);
  std::fs::write(&path, &avi).map_err(|e| CmdError::new(format!("{}: {e}", path.display())))?;
  Ok(
    json!({"path": path, "frames": frames.len(), "fps": fps, "width": w, "height": h, "seconds": frames.len() as f64 / fps as f64}),
  )
}

fn jpeg(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, CmdError> {
  let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
  let mut out = Vec::new();
  image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
    .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
    .map_err(|e| CmdError::new(e.to_string()))?;
  Ok(out)
}

/// A Motion JPEG AVI of `frames` (JPEG images), `fps` frames a second, with its index.
pub fn mjpeg_avi(frames: &[Vec<u8>], w: u32, h: u32, fps: u32) -> Vec<u8> {
  fn chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
      out.push(0);
    }
  }
  fn list(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(id);
    out.extend_from_slice(body);
    out
  }
  let le = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
  let n = frames.len() as u32;
  let biggest = frames.iter().map(Vec::len).max().unwrap_or(0) as u32;
  let mut avih = Vec::new();
  for x in [1_000_000 / fps, biggest * fps, 0, 0x10, n, 0, 1, biggest, w, h, 0, 0, 0, 0] {
    le(&mut avih, x);
  }
  let mut strh = Vec::new();
  strh.extend_from_slice(b"vids");
  strh.extend_from_slice(b"MJPG");
  for x in [0, 0, 0, 1, fps, 0, n, biggest, u32::MAX, 0] {
    le(&mut strh, x);
  }
  for x in [0u16, 0, w as u16, h as u16] {
    strh.extend_from_slice(&x.to_le_bytes());
  }
  let mut strf = Vec::new();
  le(&mut strf, 40);
  le(&mut strf, w);
  le(&mut strf, h);
  strf.extend_from_slice(&1u16.to_le_bytes());
  strf.extend_from_slice(&24u16.to_le_bytes());
  strf.extend_from_slice(b"MJPG");
  for x in [w * h * 3, 0, 0, 0, 0] {
    le(&mut strf, x);
  }
  let mut strl = Vec::new();
  chunk(&mut strl, b"strh", &strh);
  chunk(&mut strl, b"strf", &strf);
  let mut hdrl = Vec::new();
  chunk(&mut hdrl, b"avih", &avih);
  hdrl.extend(list(b"strl", &strl));
  let mut movi = Vec::new();
  let mut idx = Vec::new();
  for f in frames {
    // offsets count from the "movi" id
    let at = movi.len() as u32 + 4;
    chunk(&mut movi, b"00dc", f);
    idx.extend_from_slice(b"00dc");
    le(&mut idx, 0x10);
    le(&mut idx, at);
    le(&mut idx, f.len() as u32);
  }
  let mut body = Vec::new();
  body.extend_from_slice(b"AVI ");
  body.extend(list(b"hdrl", &hdrl));
  body.extend(list(b"movi", &movi));
  chunk(&mut body, b"idx1", &idx);
  let mut out = Vec::with_capacity(body.len() + 8);
  out.extend_from_slice(b"RIFF");
  out.extend_from_slice(&(body.len() as u32).to_le_bytes());
  out.extend(body);
  out
}

/// Trails: each moved unit's centre at home and where storyboard `i` has it at `t`.
pub fn trails(s: &Session, i: usize, t: f64) -> Vec<(DVec3, DVec3)> {
  let now = pose(s, i, t);
  let home = Pose::default();
  let all = Targets {
    components: s.design.components.iter().filter(|c| c.parent.is_none()).map(|c| c.id).collect(),
    bodies: s.model.bodies.iter().map(|b| b.id).filter(|b| s.design.component_of(*b).is_none()).collect(),
  };
  units(s, &all)
    .into_iter()
    .filter(|(_, bodies)| bodies.iter().any(|b| now.transforms.contains_key(b)))
    .filter_map(|(_, bodies)| Some((centre(s, &home, &bodies)?, centre(s, &now, &bodies)?)))
    .collect()
}
