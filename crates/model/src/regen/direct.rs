//! Regeneration of base features: frozen bodies, then direct edits in order.

use clf_kernel::glam::DVec3;
use clf_kernel::{Shape, SurfaceKind};

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
      self.state.bodies.push(Body::new(b.id, name, shape, id));
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
      DirectEdit::MoveFaces { faces, translate } => {
        if faces.is_empty() {
          return Err("select faces to move".into());
        }
        let v = DVec3::from(*translate);
        // each flat face moves by the part of the vector along its normal
        let mut moves = Vec::new();
        for fr in faces.iter_mut() {
          let (_, info) = self.face_info(fr)?;
          if info.kind != SurfaceKind::Plane {
            return Err("move face works on flat faces (offset curved faces with press-pull)".into());
          }
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
