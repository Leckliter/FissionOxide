//! Regeneration of the assembly features added after joints: saved joint origins, rigid groups,
//! motion links and components inserted from other designs.
//!
//! Motion links act on joints anywhere in the timeline (a link usually comes after the joints it
//! ties together), so they are gathered before the replay into [`Mates`]; their own timeline
//! entries only report problems. Rigid groups take effect where they are in the timeline: joints
//! after them move the whole group.

use std::collections::{BTreeMap, HashMap};

use fox_kernel::Shape;
use fox_kernel::glam::{DMat4, DVec3};

use super::{Ctx, FResult, ke};
use crate::assembly::{self, MotionAxis};
use crate::{ComponentId, Design, FeatureId, FeatureKind};

/// What motion links do to the joints, worked out before the replay.
#[derive(Debug, Default)]
pub(super) struct Mates {
  /// Joint motions set by motion links: joint → (motion, value in mm / rad).
  driven: BTreeMap<FeatureId, Vec<(MotionAxis, f64)>>,
  /// Why a motion link doesn't work, by link.
  errors: BTreeMap<FeatureId, String>,
}

impl Mates {
  /// Feeds the regeneration cache key: links change joints earlier in the timeline.
  pub(super) fn key(&self) -> String {
    format!("{self:?}")
  }

  /// Overrides the motions a link drives.
  pub(super) fn drive(&self, joint: FeatureId, m: &mut assembly::Motion) {
    for (axis, v) in self.driven.get(&joint).into_iter().flatten() {
      axis.set(m, *v);
    }
  }
}

impl Ctx<'_> {
  /// A component and everything rigidly grouped with it so far (directly or through other groups).
  fn group_of(&self, c: ComponentId) -> Vec<ComponentId> {
    let mut out = vec![c];
    loop {
      let more: Vec<ComponentId> = self
        .state
        .groups
        .iter()
        .filter(|g| g.iter().any(|x| out.contains(x)))
        .flatten()
        .filter(|x| !out.contains(x))
        .copied()
        .collect();
      if more.is_empty() {
        return out;
      }
      for m in more {
        if !out.contains(&m) {
          out.push(m);
        }
      }
    }
  }
}

struct Link {
  id: FeatureId,
  name: String,
  leader: (FeatureId, MotionAxis),
  ratio: f64,
}

/// Gathers the active motion links of a design.
pub(super) fn mates(design: &Design, params: &HashMap<String, f64>) -> Mates {
  let mut out = Mates::default();
  let active = &design.features[..design.active_len()];
  let joint = |id: FeatureId| {
    active.iter().find(|f| f.id == id && !f.suppressed).and_then(|f| match &f.kind {
      FeatureKind::Joint { kind, rotation, slide, slide2, pitch, .. } => {
        Some((f.name.clone(), *kind, [rotation.clone(), slide.clone(), slide2.clone(), pitch.clone()]))
      }
      _ => None,
    })
  };
  let eval = |e: &str, angle: bool| {
    let kind = if angle { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
    fox_expr::eval(e, kind, params).map_err(|err| format!("'{e}': {err}"))
  };
  // follower (joint, motion) → link
  let mut links: BTreeMap<(FeatureId, MotionAxis), Link> = BTreeMap::new();
  for f in active.iter().filter(|f| !f.suppressed) {
    if let FeatureKind::MotionLink {
      leader,
      leader_motion,
      follower,
      follower_motion,
      leader_value,
      follower_value,
      reverse,
    } = &f.kind
    {
      let r = (|| {
        let (lname, lkind, _) = joint(*leader).ok_or("the leading joint no longer exists")?;
        let (fname, fkind, _) = joint(*follower).ok_or("the driven joint no longer exists")?;
        if leader == follower {
          return Err("a joint can't drive itself".to_string());
        }
        if !leader_motion.allowed(lkind) {
          return Err(format!("{lname} has no {} motion", leader_motion.name()));
        }
        if !follower_motion.allowed(fkind) {
          return Err(format!("{fname} has no {} motion", follower_motion.name()));
        }
        if let Some(l) = links.get(&(*follower, *follower_motion)) {
          return Err(format!("{fname}'s {} is already driven by {}", follower_motion.name(), l.name));
        }
        let a = eval(leader_value, leader_motion.is_angle())?;
        let b = eval(follower_value, follower_motion.is_angle())?;
        if a.abs() < 1e-12 {
          return Err("the leading joint's value can't be zero".into());
        }
        let sign = if *reverse { -1.0 } else { 1.0 };
        Ok(Link { id: f.id, name: f.name.clone(), leader: (*leader, *leader_motion), ratio: sign * b / a })
      })();
      match r {
        Ok(l) => {
          links.insert((*follower, *follower_motion), l);
        }
        Err(e) => {
          out.errors.insert(f.id, e);
        }
      }
    }
  }
  // a motion's value: what a link makes it, else the joint's own expression
  fn value(
    at: (FeatureId, MotionAxis),
    links: &BTreeMap<(FeatureId, MotionAxis), Link>,
    own: &dyn Fn(FeatureId, MotionAxis) -> Result<f64, String>,
    depth: usize,
  ) -> Result<f64, String> {
    match links.get(&at) {
      None => own(at.0, at.1),
      Some(_) if depth > links.len() => Err("the motion links form a loop".into()),
      Some(l) => Ok(value(l.leader, links, own, depth + 1)? * l.ratio),
    }
  }
  let own = |j: FeatureId, a: MotionAxis| {
    let (_, _, exprs) = joint(j).ok_or("the leading joint no longer exists")?;
    eval(&exprs[a as usize], a.is_angle())
  };
  for (at, l) in &links {
    match value(*at, &links, &own, 0) {
      Ok(v) => out.driven.entry(at.0).or_default().push((at.1, v)),
      Err(e) => {
        out.errors.insert(l.id, e);
      }
    }
  }
  out
}

impl Ctx<'_> {
  /// Saved joint origins, rigid groups, motion links and inserted designs.
  pub(super) fn apply_assembly(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::JointOrigin { component, geometry, angle, offset, flip } => {
        let comp = self.component(*component)?;
        let name = comp.name.clone();
        if comp.bodies.iter().all(|b| self.body_index(*b).is_err()) {
          return Err(format!("{name} has no bodies"));
        }
        let base = self.joint_frame(geometry)?;
        let turn = if *flip { DMat4::from_rotation_x(std::f64::consts::PI) } else { DMat4::IDENTITY };
        let f = base
          * DMat4::from_translation(DVec3::new(0.0, 0.0, self.len(offset)?))
          * DMat4::from_rotation_z(self.angle(angle)?)
          * turn;
        self.state.joint_origins.insert(id, (*component, f));
        Ok(None)
      }
      FeatureKind::RigidGroup { components, poses } => {
        if components.len() < 2 {
          return Err("a rigid group needs at least two components".into());
        }
        for c in components.iter() {
          self.component(*c)?;
        }
        self.state.groups.push(components.clone());
        if poses.len() != components.len() {
          return Ok(None);
        }
        // how far joints moved each member since the group was made; members that haven't
        // moved (their bodies came later than the joints) follow the first one that has
        let moved: Vec<DMat4> = components
          .iter()
          .zip(poses.iter())
          .map(|(c, p)| self.placed(*c) * DMat4::from_cols_array(p).inverse())
          .collect();
        let still = |m: &DMat4| m.abs_diff_eq(DMat4::IDENTITY, 1e-9);
        let Some(lead) = moved.iter().find(|m| !still(m)).copied() else { return Ok(None) };
        let lagging: Vec<ComponentId> =
          components.iter().zip(&moved).filter(|(_, m)| still(m)).map(|(c, _)| *c).collect();
        self.transform_components(id, &lagging, lead)?;
        Ok(None)
      }
      FeatureKind::MotionLink { .. } => match self.mates.errors.get(&id) {
        Some(e) => Err(e.clone()),
        None => Ok(None),
      },
      FeatureKind::InsertDesign { name, breps, translate, .. } => {
        if breps.is_empty() {
          return Err(format!("{name} has no bodies to insert"));
        }
        let v = DVec3::new(self.len(&translate[0])?, self.len(&translate[1])?, self.len(&translate[2])?);
        for b in breps.iter() {
          let s = Shape::from_brep(b).map_err(ke)?.translated(v).map_err(ke)?;
          for body in s.bodies().map_err(ke)? {
            self.new_body(id, body);
          }
        }
        Ok(None)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }

  fn component(&self, id: ComponentId) -> Result<&crate::Component, String> {
    self.components.iter().find(|c| c.id == id).ok_or(format!("component {} no longer exists", id.0))
  }

  /// A saved joint origin's frame where its component is now.
  pub(super) fn origin_frame(&self, origin: FeatureId) -> Result<DMat4, String> {
    self
      .state
      .joint_origins
      .get(&origin)
      .map(|o| o.1)
      .ok_or_else(|| "the joint origin failed or comes later in the timeline".to_string())
  }

  /// Moves a jointed component, with its rigid group and their saved joint origins, by `m`.
  pub(super) fn move_component(
    &mut self,
    id: FeatureId,
    moving: ComponentId,
    fixed: Option<ComponentId>,
    m: DMat4,
  ) -> FResult {
    let members = self.group_of(moving);
    if let Some(f) = fixed
      && members.contains(&f)
    {
      return Err("both components are in the same rigid group".into());
    }
    for c in &members {
      let comp = self.component(*c)?;
      if comp.grounded {
        return Err(format!("{} is grounded (rigid group)", comp.name));
      }
    }
    if self.transform_components(id, &members, m)? == 0 {
      return Err(format!("{} has no bodies", self.component(moving)?.name));
    }
    Ok(None)
  }

  /// How far joints have moved a component.
  fn placed(&self, c: ComponentId) -> DMat4 {
    self.state.placed.get(&c).copied().unwrap_or(DMat4::IDENTITY)
  }

  /// Moves components' bodies and saved joint origins by `m`; returns how many bodies moved.
  fn transform_components(&mut self, id: FeatureId, comps: &[ComponentId], m: DMat4) -> Result<usize, String> {
    let mut bodies = Vec::new();
    for c in comps {
      bodies.extend(self.component(*c)?.bodies.iter().filter_map(|b| self.body_index(*b).ok()));
      let p = m * self.placed(*c);
      self.state.placed.insert(*c, p);
    }
    for bi in &bodies {
      let s = self.state.bodies[*bi].shape.transformed(m).map_err(ke)?;
      self.state.bodies[*bi] = self.state.bodies[*bi].with_shape(s, id);
    }
    for (c, f) in self.state.joint_origins.values_mut() {
      if comps.contains(c) {
        *f = m * *f;
      }
    }
    Ok(bodies.len())
  }
}
