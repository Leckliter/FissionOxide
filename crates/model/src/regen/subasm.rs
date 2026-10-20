//! Regeneration of flexible sub-assemblies: a design inserted from another one whose joints are
//! solved again in the host (see [`crate::subassembly`]).

use fox_kernel::Shape;
use fox_kernel::glam::{DMat4, DVec3};

use super::{Ctx, FResult, ke};
use crate::subassembly::SubAssembly;
use crate::{FeatureId, FeatureKind};

impl Ctx<'_> {
  /// Places the source's unjointed bodies, solves its joint origins and joints in order, then
  /// moves the whole sub-assembly by `translate`. A sub-assembly joint that fails becomes a
  /// warning; its components stay where the earlier joints left them.
  pub(super) fn insert_flexible(
    &mut self,
    id: FeatureId,
    name: &str,
    breps: &[String],
    translate: &[String; 3],
    sub: &mut SubAssembly,
  ) -> FResult {
    if sub.free_breps.len() != breps.len() {
      return Err(format!("{name}'s sub-assembly doesn't match its bodies; update it from its source"));
    }
    let v = DVec3::new(self.len(&translate[0])?, self.len(&translate[1])?, self.len(&translate[2])?);
    for b in &sub.free_breps {
      let s = Shape::from_brep(b).map_err(ke)?;
      for body in s.bodies().map_err(ke)? {
        self.new_body(id, body);
      }
    }
    let mut problems = Vec::new();
    for (k, m) in sub.mates.iter_mut().enumerate() {
      let origin = matches!(m.kind, FeatureKind::JointOrigin { .. });
      let joint = matches!(m.kind, FeatureKind::Joint { .. });
      let r = if origin {
        self.apply_assembly(SubAssembly::origin_id(id, k), &mut m.kind)
      } else if joint {
        self.apply(id, &mut m.kind)
      } else {
        Ok(None)
      };
      if let Err(e) = r {
        problems.push(format!("{}: {e}", m.name));
      }
    }
    if v != DVec3::ZERO {
      let t = DMat4::from_translation(v);
      let mine = |b: crate::BodyId| b.0 / super::MAX_BODIES_PER_FEATURE == id.0;
      for i in 0..self.state.bodies.len() {
        if mine(self.state.bodies[i].id) {
          let s = self.state.bodies[i].shape.transformed(t).map_err(ke)?;
          self.state.bodies[i] = self.state.bodies[i].with_shape(s, id);
        }
      }
      for k in 0..sub.mates.len() {
        if let Some((_, f)) = self.state.joint_origins.get_mut(&SubAssembly::origin_id(id, k)) {
          *f = t * *f;
        }
      }
    }
    Ok((!problems.is_empty()).then(|| problems.join("; ")))
  }
}
