//! Regeneration of 3D curves: intersection curves (sketch curves pushed along their sketch's
//! normal, cut by body faces or by other sketches' curves pushed along theirs, as Fusion's
//! Intersection Curve) and pipes that follow them.

use fox_kernel::Shape;
use fox_sketch::{CurveId, Loop, Profile, Seg};

use super::{Ctx, FResult, ke, union_all};
use crate::{FeatureId, FeatureKind, Operation, PathRef};

/// How far apart two section edges' ends may be and still join into one wire (mm).
const JOIN_TOL: f64 = 1e-4;

impl Ctx<'_> {
  pub(super) fn apply_curves3d(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::IntersectionCurve { curves, faces, with } => {
        let walls = self.pushed_curves(curves, with)?;
        let mut targets = Vec::new();
        for fr in faces.iter_mut() {
          let (bi, info) = self.face_info(fr)?;
          targets.push(self.state.bodies[bi].shape.face_shape(info.id).map_err(ke)?);
        }
        for w in with.iter() {
          targets.extend(self.pushed_curves(w, std::slice::from_ref(curves))?);
        }
        if targets.is_empty() {
          return Err("select faces or sketch curves to intersect with".into());
        }
        let tool = Shape::compound(&targets).map_err(ke)?;
        let mut wires = Vec::new();
        for wall in &walls {
          // a curve that misses every face adds nothing; only all of them missing is an error
          if let Ok(w) = wall.section_wires(&tool, JOIN_TOL) {
            wires.extend(w);
          }
        }
        if wires.is_empty() {
          return Err("the curves don't cross the faces or curves to intersect with".into());
        }
        self.state.curves3d.insert(id, wires);
        Ok(None)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }

  /// Each curve of `r` pushed along its sketch's normal both ways, far enough to pass every body
  /// and the `others` sketches: one surface per curve.
  fn pushed_curves(&self, r: &PathRef, others: &[PathRef]) -> Result<Vec<Shape>, String> {
    let sk = self.state.sketches.get(&r.sketch).ok_or("a sketch of the intersection curve failed")?;
    let n = sk.plane.normal();
    let mut far = self.through_all(&sk.plane);
    for o in others.iter().chain(std::iter::once(r)) {
      if let Some(other) = self.state.sketches.get(&o.sketch) {
        let (lo, hi) = other.sketch.bounds().unwrap_or(([0.0; 2], [0.0; 2]));
        let span =
          (other.plane.to_world(lo) - sk.plane.origin).length() + (other.plane.to_world(hi) - sk.plane.origin).length();
        far += span;
      }
    }
    let ids: Vec<CurveId> = if r.curves.is_empty() {
      sk.sketch.curves.iter().filter(|c| !c.construction).map(|c| c.id).collect()
    } else {
      r.curves.clone()
    };
    let start = sk.plane.offset(-far);
    let mut out = Vec::new();
    for c in ids {
      let segs = sk.sketch.curve_segs(c);
      if segs.is_empty() {
        return Err(format!("sketch curve {} could not be found", c.0));
      }
      out.push(Shape::path(&start, &segs).map_err(ke)?.extrude(n * 2.0 * far).map_err(ke)?);
    }
    if out.is_empty() {
      return Err("the sketch has no curves to intersect".into());
    }
    Ok(out)
  }

  /// A pipe along the wires of an intersection curve: one round tube (hollow with a wall) per
  /// wire, its section square to the wire where it starts (on a closed wire, as Fusion, halfway
  /// along its first curve).
  pub(super) fn pipe_along(
    &mut self,
    id: FeatureId,
    wires: &[Shape],
    r: f64,
    wall: Option<f64>,
    operation: Operation,
  ) -> FResult {
    let mut tubes = Vec::new();
    for w in wires {
      let w = w.wire_from_middle().map_err(ke)?;
      let (start, tangent) = w.wire_start().map_err(ke)?;
      let sec = fox_kernel::Plane::from_normal(start, tangent, tangent.any_orthonormal_vector()).map_err(ke)?;
      let mut holes = Vec::new();
      if let Some(t) = wall {
        holes.push(Loop { segs: vec![Seg::Circle { center: [0.0, 0.0], radius: r - t, ccw: false }] });
      }
      let outer = Loop { segs: vec![Seg::Circle { center: [0.0, 0.0], radius: r, ccw: true }] };
      let face = Shape::face(&sec, &Profile { outer, holes }).map_err(ke)?;
      tubes.push(face.sweep(&w).map_err(ke)?);
    }
    let shape = if tubes.len() == 1 { tubes.remove(0) } else { union_all(tubes)? };
    self.operate(id, shape, operation)?;
    Ok(None)
  }
}
