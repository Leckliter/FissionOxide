//! Regeneration of PCB features: the board body and its parts' 3D models.

use fox_kernel::glam::{DMat4, DVec3};
use fox_kernel::{BoolOp, Plane, Shape};
use fox_sketch::{Loop, Profile, Seg};

use super::{Ctx, FResult, ke, pick_profiles, plane_matrix, refresh_face};
use crate::pcb::{PcbModel, PcbOutline, face_frame};
use crate::{FeatureId, FeatureKind};

impl Ctx<'_> {
  pub(super) fn apply_pcb(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::Pcb { name, outline, plane, thickness, holes, parts } = kind else {
      return Err("this feature type isn't supported".into());
    };
    let t = *thickness;
    if t <= 0.0 {
      return Err("the board needs a thickness".into());
    }
    let (frame, face) = match outline {
      PcbOutline::Polygon { points } => {
        if points.len() < 3 {
          return Err("the board outline needs at least three points".into());
        }
        let frame = match plane.as_mut() {
          Some(p) => self.resolve_plane(p)?,
          None => Plane::XY,
        };
        let n = points.len();
        let segs = (0..n).map(|i| Seg::Line { a: points[i], b: points[(i + 1) % n] }).collect();
        let face = Shape::face(&frame, &Profile { outer: Loop::new(segs), holes: Vec::new() }).map_err(ke)?;
        (frame, face)
      }
      PcbOutline::Profile { sketch, point } => {
        let sk = self.state.sketches.get(sketch).ok_or("the outline's sketch failed")?.clone();
        let (chosen, _) = pick_profiles(&sk, &[*point]);
        let prof = chosen.first().ok_or("the outline's sketch region no longer exists")?;
        (sk.plane, Shape::face(&sk.plane, prof).map_err(ke)?)
      }
      PcbOutline::Face { face } => {
        let bi = self.body_index(face.body)?;
        let body = &self.state.bodies[bi];
        let fid = face.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or("the outline's face could not be found")?;
        refresh_face(face, body, fid);
        let info = body.faces().iter().find(|f| f.id == fid).ok_or("the outline's face could not be found")?;
        if info.kind != fox_kernel::SurfaceKind::Plane {
          return Err("the outline's face must be planar".into());
        }
        let frame = face_frame(info.origin, info.normal).ok_or("the outline's face has no normal")?;
        (frame, body.shape.face_shape(fid).map_err(ke)?)
      }
    };
    let to_world = plane_matrix(&frame);
    let mut board = face.extrude(frame.normal() * t).map_err(ke)?;
    if !holes.is_empty() {
      let cyls: Vec<Shape> = holes
        .iter()
        .filter(|h| h[2] > 0.0)
        .map(|h| Shape::cylinder(DVec3::new(h[0], h[1], -1.0), DVec3::Z, h[2] / 2.0, t + 2.0).and_then(|c| c.transformed(to_world)))
        .collect::<Result<_, _>>()
        .map_err(ke)?;
      if !cyls.is_empty() {
        let tool = Shape::compound(&cyls).map_err(ke)?;
        board = board.boolean(&tool, BoolOp::Subtract).map_err(ke)?;
      }
    }
    let bid = self.new_body(id, board);
    if let Some(b) = self.state.bodies.iter_mut().find(|b| b.id == bid) {
      b.name = name.clone();
    }
    for part in parts.iter() {
      // the part's frame: on top it stands on the board's upper face; underneath it is turned
      // over (180° about Y) and hangs from the lower face
      let place = DMat4::from_translation(DVec3::new(part.at[0], part.at[1], if part.bottom { 0.0 } else { t }))
        * if part.bottom { DMat4::from_rotation_y(std::f64::consts::PI) } else { DMat4::IDENTITY }
        * DMat4::from_rotation_z((if part.bottom { -part.rot } else { part.rot }).to_radians());
      let local = match &part.model {
        PcbModel::Box { size, offset } => {
          if size.iter().any(|s| *s <= 0.0) {
            continue;
          }
          Shape::make_box(
            DVec3::new(offset[0] - size[0] / 2.0, offset[1] - size[1] / 2.0, 0.0),
            DVec3::new(offset[0] + size[0] / 2.0, offset[1] + size[1] / 2.0, size[2]),
          )
          .map_err(ke)?
        }
        PcbModel::Cylinder { diameter, height, offset } => {
          if *diameter <= 0.0 || *height <= 0.0 {
            continue;
          }
          Shape::cylinder(DVec3::new(offset[0], offset[1], 0.0), DVec3::Z, diameter / 2.0, *height).map_err(ke)?
        }
        PcbModel::File { path, offset, rotate, scale } => {
          let Ok(shapes) = fox_kernel::read_step(std::path::Path::new(path)) else {
            log::warn!("{}: could not read the 3D model {path}", part.refdes);
            continue;
          };
          if shapes.is_empty() {
            continue;
          }
          let m = DMat4::from_translation(DVec3::from(*offset))
            * DMat4::from_rotation_z(rotate[2].to_radians())
            * DMat4::from_rotation_y(rotate[1].to_radians())
            * DMat4::from_rotation_x(rotate[0].to_radians());
          let s = Shape::compound(&shapes).map_err(ke)?;
          let s = if *scale != [1.0; 3] { s.scaled(DVec3::ZERO, DVec3::from(*scale)).map_err(ke)? } else { s };
          s.transformed(m).map_err(ke)?
        }
      };
      let shape = local.transformed(to_world * place).map_err(ke)?;
      let pid = self.new_body(id, shape);
      if let Some(b) = self.state.bodies.iter_mut().find(|b| b.id == pid) {
        b.name = part.refdes.clone();
      }
    }
    Ok(None)
  }
}
