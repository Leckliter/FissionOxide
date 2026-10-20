//! Regeneration of form (T-spline) features: the cage's limit surface sewn into a solid.

use fox_kernel::Shape;

use super::{Ctx, FResult, ke, plane_matrix};
use crate::{FeatureId, FeatureKind};

impl Ctx<'_> {
  pub(super) fn apply_form(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::Form { plane, cage, operation } = kind else {
      return Err("this feature type isn't supported".into());
    };
    let pl = self.resolve_plane(plane)?;
    let mut faces = Vec::new();
    for s in cage.surfaces()? {
      faces.push(Shape::bspline_face(&s).map_err(ke)?);
    }
    let skin = Shape::compound(faces.iter()).map_err(ke)?.sewn(1e-4).map_err(ke)?;
    if skin.solid_count() != 1 {
      return Err("the form's surface doesn't close into a solid".into());
    }
    let solid = skin.transformed(plane_matrix(&pl)).map_err(ke)?;
    self.operate(id, solid, *operation)?;
    Ok(None)
  }
}
