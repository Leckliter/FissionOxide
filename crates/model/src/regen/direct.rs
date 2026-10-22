//! Regeneration of base features: frozen bodies, then direct edits in order.

use fox_kernel::glam::{DMat4, DVec3};
use fox_kernel::{Shape, SurfaceKind};

use super::{Body, Ctx, FResult, ke};
use crate::direct::DirectEdit;
use crate::{FeatureId, FeatureKind};

impl Ctx<'_> {
  pub(super) fn apply_base(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::BaseFeature { bodies, edits } = kind else {
      return Err("this feature type isn't supported".into());
    };
    for b in bodies.iter() {
      if self.state.bodies.iter().any(|x| x.id == b.id) {
        return Err(format!("body {} already exists", b.id.0));
      }
      let shape = Shape::from_brep(&b.brep).map_err(ke)?;
      self.state.body_counter += 1;
      let name = if b.name.is_empty() { format!("Body{}", self.state.body_counter) } else { b.name.clone() };
      let mut body = Body::new(b.id, name, shape, id);
      if b.mesh {
        body.kind = super::BodyKind::Mesh;
      }
      self.state.bodies.push(body);
    }
    let mut warnings = Vec::new();
    for (i, edit) in edits.iter_mut().enumerate() {
      let label = edit.label();
      match self.direct_edit(id, edit) {
        Ok(Some(w)) => warnings.push(format!("{label} {}: {w}", i + 1)),
        Ok(None) => {}
        Err(e) => return Err(format!("{label} {}: {e}", i + 1)),
      }
    }
    Ok((!warnings.is_empty()).then(|| warnings.join("; ")))
  }

  /// One direct edit, done by the matching parametric operation with fixed values (so the faces
  /// are re-found and refreshed the same way).
  fn direct_edit(&mut self, id: FeatureId, edit: &mut DirectEdit) -> FResult {
    match edit {
      DirectEdit::PressPull { faces, distance } => {
        let mut k = FeatureKind::OffsetFaces { faces: std::mem::take(faces), distance: mm(*distance) };
        let r = self.apply(id, &mut k);
        if let FeatureKind::OffsetFaces { faces: f, .. } = k {
          *faces = f;
        }
        r
      }
      DirectEdit::DeleteFaces { faces } => {
        if faces.is_empty() {
          return Err("select faces to delete".into());
        }
        let bodies: Vec<_> = faces.iter().map(|f| f.body).collect();
        let count = |c: &Self| -> usize {
          c.state.bodies.iter().filter(|b| bodies.contains(&b.id)).map(|b| b.faces().len()).sum()
        };
        let before = count(self);
        let mut k = FeatureKind::RemoveFaces { faces: std::mem::take(faces) };
        let r = self.apply(id, &mut k);
        if let FeatureKind::RemoveFaces { faces: f } = k {
          *faces = f;
        }
        // the kernel leaves a body as it was when it can't heal it without the faces
        if r.is_ok() && count(self) == before {
          return Ok(Some("the faces could not be removed (the body can't be closed without them)".into()));
        }
        r
      }
      DirectEdit::MoveFaces { faces, translate, rotate } => {
        if faces.is_empty() {
          return Err("select faces to move".into());
        }
        let v = DVec3::from(*translate);
        let turn = match rotate {
          Some(r) if r.angle.abs() > 1e-9 => Some(r.matrix().ok_or("the rotation axis has no direction")?),
          _ => None,
        };
        let mut flat = true;
        for fr in faces.iter_mut() {
          flat &= self.face_info(fr)?.1.kind == SurfaceKind::Plane;
        }
        if turn.is_some() || !flat {
          let m = DMat4::from_translation(v) * turn.unwrap_or(DMat4::IDENTITY);
          if turn.is_none() && v.length() < 1e-9 {
            return Ok(Some("the faces don't move".into()));
          }
          for (bi, fids) in self.faces_by_body(faces)? {
            let shape = self.state.bodies[bi].shape.move_faces(&fids, m).map_err(ke)?;
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
          }
          // (the references stay on the faces as they were before the move, where replaying finds them)
          return Ok(None);
        }
        // each flat face moves by the part of the vector along its normal
        let mut moves = Vec::new();
        for fr in faces.iter_mut() {
          let (_, info) = self.face_info(fr)?;
          moves.push(v.dot(info.normal));
        }
        let mut still = 0;
        for (fr, d) in faces.iter_mut().zip(moves) {
          if d.abs() < 1e-9 {
            still += 1;
            continue;
          }
          let mut k = FeatureKind::OffsetFaces { faces: vec![fr.clone()], distance: mm(d) };
          self.apply(id, &mut k)?;
          if let FeatureKind::OffsetFaces { faces: mut f, .. } = k
            && let Some(r) = f.pop()
          {
            *fr = r;
          }
        }
        Ok((still > 0).then(|| format!("{still} face(s) only slide within their own plane and don't change")))
      }
    }
  }
}

fn mm(v: f64) -> String {
  format!("{v} mm")
}
