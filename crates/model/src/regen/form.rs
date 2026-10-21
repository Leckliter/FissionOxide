//! Regeneration of form (T-spline) features: the cage's limit surface sewn into a solid, or into
//! a surface body if the cage is open.

use fox_kernel::Shape;

use super::{Ctx, FResult, ke, plane_matrix};
use crate::{FeatureId, FeatureKind};

impl Ctx<'_> {
  pub(super) fn apply_form(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::Form { plane, cage, operation } = kind else {
      return Err("this feature type isn't supported".into());
    };
    let pl = self.resolve_plane(plane)?;
    self.state.forms.insert(id, pl);
    let mut faces = Vec::new();
    for s in crate::form::surfaces(cage)? {
      faces.push(Shape::bspline_face(&s).map_err(ke)?);
    }
    let skin = if faces.len() == 1 {
      faces.remove(0)
    } else {
      Shape::compound(faces.iter()).map_err(ke)?.sewn(1e-4).map_err(ke)?
    };
    let skin = skin.transformed(plane_matrix(&pl)).map_err(ke)?;
    if !cage.is_closed() {
      self.new_body(id, skin);
      return Ok(None);
    }
    if skin.solid_count() != 1 {
      return Err("the form's surface doesn't close into a solid".into());
    }
    self.operate(id, skin, *operation)?;
    Ok(None)
  }
}
