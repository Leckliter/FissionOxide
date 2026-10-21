//! Flat pattern sketches that follow the model: every regeneration flattens the sheet metal body
//! as it is at the sketch and hands the outline and bend lines to the sketch
//! ([`fox_sketch::Sketch::set_flat_outline`]).

use fox_kernel::Plane;

use super::Ctx;
use crate::{BodyId, FaceRef, PlaneRef};

impl Ctx<'_> {
  /// Works out every flat pattern of the sketch from the bodies as they are now. Returns a warning
  /// naming those that could not be flattened (they keep their last outline).
  pub(super) fn refresh_flats(&self, plane: &PlaneRef, pl: &Plane, sketch: &mut fox_sketch::Sketch) -> Option<String> {
    let mut errors = Vec::new();
    for f in sketch.flats.clone() {
      let made =
        self.flat_outline(plane, pl, &f).and_then(|o| sketch.set_flat_outline(f.id, o).map_err(|e| e.to_string()));
      if let Err(e) = made {
        errors.push(format!("flat pattern: {e}"));
      }
    }
    (!errors.is_empty()).then(|| errors.join("; "))
  }

  fn flat_outline(
    &self,
    plane: &PlaneRef,
    pl: &Plane,
    f: &fox_sketch::SketchFlat,
  ) -> Result<Vec<fox_sketch::SpunSeg>, String> {
    let id = BodyId(f.body);
    let body = &self.state.bodies[self.body_index(id)?];
    if body.sheet.is_none() {
      return Err(format!("body {} is no longer sheet metal", f.body));
    }
    let resolve = |r: &FaceRef| {
      r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or("the stationary face could not be found".to_string())
    };
    let face = match (f.at_origin, plane, &f.face) {
      // on the stationary face: the face the sketch is on
      (false, PlaneRef::Face { face, .. }, _) if face.body == id => Some(resolve(face)?),
      (true, _, Some((point, normal))) => {
        let r = FaceRef { body: id, point: *point, normal: *normal, planar: true, lo: [0.0; 3], hi: [0.0; 3], name: 0 };
        Some(resolve(&r)?)
      }
      _ => None,
    };
    let flat = body.flat_pattern(face)?;
    if !f.at_origin {
      return crate::sheet2::flat_outline(&flat, pl, [0.0, 0.0]);
    }
    // where the flat lies, its lower-left corner moved to the sketch's origin
    let (lo, _) =
      crate::sheet2::flat_sketch(&flat, &flat.plane, [0.0, 0.0])?.bounds().ok_or("the flat pattern is empty")?;
    crate::sheet2::flat_outline(&flat, &flat.plane, [-lo[0], -lo[1]])
  }
}
