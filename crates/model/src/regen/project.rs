//! Regeneration of Project To Surface (sketch curves draped onto faces as 3D curves) and of pipes
//! that follow those curves.

use fox_kernel::{Projection3d, Shape};

use super::{Ctx, FResult, ke, union_all};
use crate::construct::PipeSection;
use crate::{FeatureId, FeatureKind, Operation};

impl Ctx<'_> {
  pub(super) fn apply_project(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::ProjectToSurface { sketch, curves, faces, along } = kind else {
      return Err("this feature type isn't supported".into());
    };
    let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
    let mut sources = Vec::new();
    for c in &sk.sketch.curves {
      if !(curves.is_empty() && !c.reference() || curves.contains(&c.id)) {
        continue;
      }
      let segs = sk.sketch.curve_segs(c.id);
      if !segs.is_empty() {
        sources.push(Shape::path(&sk.plane, &segs).map_err(ke)?);
      }
    }
    if sources.is_empty() {
      return Err("there are no sketch curves to project".into());
    }
    if faces.is_empty() {
      return Err("select the faces to project onto".into());
    }
    let mut targets = Vec::new();
    for fr in faces.iter_mut() {
      let (bi, info) = self.face_info(fr)?;
      targets.push(self.state.bodies[bi].shape.face_shape(info.id).map_err(ke)?);
    }
    let how = match along {
      Some(a) => Projection3d::Along(self.resolve_axis(a, Some(*sketch))?.1),
      None => Projection3d::ClosestPoint,
    };
    let made = Shape::project_to_faces(&sources, &targets, how, sk.plane.normal()).map_err(ke)?;
    self.state.curves3d.insert(id, made);
    Ok(None)
  }

  /// A round pipe along each of a Project To Surface feature's curves (or the chosen ones,
  /// 1-based), open or closed.
  #[allow(clippy::too_many_arguments)]
  pub(super) fn pipe_along_curves(
    &mut self,
    id: FeatureId,
    path: FeatureId,
    picked: &[u32],
    diameter: &str,
    wall: &str,
    operation: Operation,
    section: PipeSection,
    distance: &str,
  ) -> FResult {
    let all = self.state.curves3d.get(&path).ok_or("the path curves failed")?.clone();
    if section != PipeSection::Circular {
      return Err("a pipe along 3D curves has a circular section".into());
    }
    if !distance.trim().is_empty() {
      return Err("a pipe along 3D curves follows all of each curve".into());
    }
    let size = self.len(diameter)?;
    if size <= 0.0 {
      return Err("the size must be positive".into());
    }
    let inner = if wall.trim().is_empty() {
      0.0
    } else {
      let w = self.len(wall)?;
      if !(w > 0.0 && w < size / 2.0) {
        return Err("the wall must be above zero and less than the radius".into());
      }
      size / 2.0 - w
    };
    let mut pipes = Vec::new();
    for (i, c) in all.iter().enumerate() {
      if picked.is_empty() || picked.contains(&(i as u32 + 1)) {
        pipes.push(c.round_pipe(size / 2.0, inner).map_err(ke)?);
      }
    }
    if pipes.is_empty() {
      return Err("none of the chosen curves exist".into());
    }
    self.operate(id, union_all(pipes)?, operation)?;
    // Fusion's faces never cross the seam of the tube they lie on
    for b in self.state.bodies.iter_mut().filter(|b| b.feature == id) {
      let split = b.shape.split_seams().map_err(ke)?;
      *b = b.with_shape(split, id);
    }
    Ok(None)
  }
}
