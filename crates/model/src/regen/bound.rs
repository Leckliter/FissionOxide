//! Regeneration of Bounding Solid: a box or cylinder body around other bodies.

use fox_kernel::glam::DVec3;
use fox_kernel::{Plane, Shape};

use super::{Ctx, FResult, ke, plane_matrix};
use crate::FeatureId;
use crate::FeatureKind;
use crate::bounding::{BoundShape, enclosing_circle};

impl Ctx<'_> {
  pub(super) fn apply_bounding(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::BoundingSolid { bodies, shape, plane, offset, sides } = kind else {
      return Err("this feature type isn't supported".into());
    };
    let frame = match plane.as_mut() {
      Some(p) => self.resolve_plane(p)?,
      None => Plane::XY,
    };
    let to_world = plane_matrix(&frame);
    let to_local = to_world.inverse();
    let picked = self.select(bodies)?;
    if picked.is_empty() {
      return Err("pick the bodies to enclose".into());
    }
    let u = self.len(offset)?;
    let grow: Vec<f64> = match sides.len() {
      0 => vec![u; 6],
      6 => sides.iter().map(|e| self.len(e)).collect::<Result<_, _>>()?,
      _ => return Err("per-side offsets are six: -x, +x, -y, +y, -z, +z".into()),
    };
    // the bodies' extent in the frame
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for &bi in &picked {
      let local = self.state.bodies[bi].shape.transformed(to_local).map_err(ke)?;
      let m = local.mass().map_err(ke)?;
      lo = lo.min(m.bbox_min);
      hi = hi.max(m.bbox_max);
    }
    if !(lo.is_finite() && hi.is_finite()) {
      return Err("the bodies have no extent".into());
    }
    let lo = lo - DVec3::new(grow[0], grow[2], grow[4]);
    let hi = hi + DVec3::new(grow[1], grow[3], grow[5]);
    let local = match shape {
      BoundShape::Box => Shape::make_box(lo, hi).map_err(ke)?,
      BoundShape::Cylinder => {
        let mut pts = Vec::new();
        for &bi in &picked {
          let mesh = self.state.bodies[bi].mesh().ok_or("a body could not be tessellated")?;
          for p in &mesh.positions {
            let q = to_local.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            pts.push([q.x, q.y]);
          }
        }
        let (c, r) = enclosing_circle(&pts).ok_or("the bodies have no extent")?;
        Shape::cylinder(DVec3::new(c[0], c[1], lo.z), DVec3::Z, r + u, hi.z - lo.z).map_err(ke)?
      }
    };
    let tool = local.transformed(to_world).map_err(ke)?;
    self.new_body(id, tool);
    Ok(None)
  }
}
