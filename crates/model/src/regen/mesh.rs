//! Regeneration of the mesh tools with the settings of Fusion's dialogs (Reduce, Smooth,
//! Convert Mesh) and of Plane Cut.

use fox_kernel::Shape;
use fox_kernel::glam::DVec3;

use super::{Body, Ctx, FResult, ke};
use crate::construct::{ConvertMethod, FillType, MeshOp, PlaneCutType, ReduceType};
use crate::meshtools::{self, Target};
use crate::{FeatureId, FeatureKind};

/// A body's triangles for the mesh tools, welded. Mesh bodies are faceted solids today, so these
/// are its display tessellation's; a mesh body kind hands over its own triangles here.
pub(crate) fn triangles(b: &Body) -> Result<(Vec<DVec3>, Vec<u32>), String> {
  Ok(crate::meshops::weld(b.mesh().ok_or("the body has no mesh")?))
}

/// The shape of a mesh body with these triangles (the way back from [`triangles`]); `merge`
/// merges coplanar triangles into single faces.
pub(crate) fn shape(pos: &[DVec3], tris: &[u32], merge: bool) -> Result<Shape, String> {
  let pos32: Vec<[f32; 3]> = pos.iter().map(|p| p.as_vec3().to_array()).collect();
  Shape::from_mesh(&pos32, tris, merge).map_err(ke)
}

impl Ctx<'_> {
  pub(super) fn apply_mesh(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::Mesh { bodies, op, amount, settings: Some(set) } => {
        let value = fox_expr::eval(amount, fox_expr::Kind::Unitless, self.params).map_err(|e| e.to_string());
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let (pos, tris) = triangles(&self.state.bodies[bi])?;
          match op {
            MeshOp::Reduce => {
              let n = tris.len() / 3;
              let target = match set.reduce {
                ReduceType::Tolerance => Target::Tolerance(self.len(amount)?),
                ReduceType::Proportion => {
                  let pc = value.clone()?.clamp(0.0, 100.0);
                  Target::Count((n as f64 * pc / 100.0).ceil() as usize)
                }
                ReduceType::FaceCount => Target::Count(value.clone()?.round().max(0.0) as usize),
              };
              // a proportion or count never takes the surface further than 1% of the body's size
              let guard = match target {
                Target::Tolerance(_) => f64::INFINITY,
                Target::Count(_) => 0.01 * self.state.bodies[bi].diagonal(),
              };
              let (p, t) = meshtools::decimate(&pos, &tris, target, set.uniform, guard);
              // a closed mesh is never flattened or opened: it stays as it was
              let collapsed = meshtools::is_closed(&tris)
                && (!meshtools::is_closed(&t)
                  || meshtools::volume(&p, &t).abs() < 0.5 * meshtools::volume(&pos, &tris).abs());
              if t.len() == tris.len() || collapsed {
                self.state.bodies[bi].feature = id;
                continue;
              }
              let s = shape(&p, &t, false)?;
              self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
            }
            MeshOp::Smooth => {
              let k = value.clone()?;
              let (p, t) = meshtools::smooth_volume(&pos, &tris, k);
              let s = shape(&p, &t, false)?;
              self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
            }
            // a new solid body; the mesh body stays (the command hides it)
            MeshOp::Convert => {
              let s = match set.method {
                ConvertMethod::Faceted => shape(&pos, &tris, false)?,
                ConvertMethod::Prismatic => shape(&pos, &tris, true)?,
                ConvertMethod::Organic => {
                  return Err(
                    "Organic conversion isn't in FissionOxide yet: the kernel can't fit smooth surfaces".into(),
                  );
                }
              };
              self.new_body(id, s);
            }
          }
        }
        Ok(None)
      }
      FeatureKind::MeshPlaneCut { body, plane, distance, y_angle, z_angle, flip, cut, fill } => {
        let pl = self.resolve_plane(plane)?;
        let (o, n) = meshtools::cut_plane(
          pl.origin,
          pl.x,
          pl.y,
          self.len(distance)?,
          self.angle(y_angle)?,
          self.angle(z_angle)?,
          *flip,
        );
        let bi = self.body_index(*body)?;
        let (pos, tris) = triangles(&self.state.bodies[bi])?;
        let (mut p, behind, front) = meshtools::plane_cut(&pos, &tris, o, n);
        let closed = |p: &mut Vec<DVec3>, side: Vec<u32>, behind: bool| {
          let cap = match fill {
            FillType::None => Vec::new(),
            FillType::Uniform => meshtools::fill_cut(p, &side, o, n, behind, true),
            FillType::Minimal => meshtools::fill_cut(p, &side, o, n, behind, false),
          };
          side.into_iter().chain(cap).collect::<Vec<u32>>()
        };
        match cut {
          PlaneCutType::Trim => {
            if behind.is_empty() {
              return Err("nothing is left behind the plane (try Flip)".into());
            }
            let kept = closed(&mut p, behind, true);
            let (p, t) = meshtools::compact(&p, &kept);
            let s = shape(&p, &t, false)?;
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
          }
          PlaneCutType::SplitBody => {
            if behind.is_empty() || front.is_empty() {
              return Err("the plane doesn't cross the body".into());
            }
            let a = closed(&mut p, behind, true);
            let b = closed(&mut p, front, false);
            let (pa, ta) = meshtools::compact(&p, &a);
            let (pb, tb) = meshtools::compact(&p, &b);
            let (sa, sb) = (shape(&pa, &ta, false)?, shape(&pb, &tb, false)?);
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(sa, id);
            self.new_body(id, sb);
          }
          PlaneCutType::SplitFaces => {
            let all: Vec<u32> = behind.into_iter().chain(front).collect();
            let s = shape(&p, &all, false)?;
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
          }
        }
        Ok(None)
      }
      _ => Err("not a mesh feature".into()),
    }
  }
}
