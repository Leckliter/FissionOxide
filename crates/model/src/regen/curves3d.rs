//! Regeneration of 3D curves: intersection curves (sketch curves pushed along their sketch's
//! normal, cut by body faces or by other sketches' curves pushed along theirs, as Fusion's
//! Intersection Curve) and pipes that follow them.

use fox_kernel::{BoolOp, Shape};
use fox_sketch::{CurveId, Loop, Profile, Seg};

use super::{Ctx, FResult, ke, union_all};
use crate::{FeatureId, FeatureKind, Operation, PathRef};

/// How far apart two section edges' ends may be and still join into one wire (mm).
const JOIN_TOL: f64 = 1e-4;

/// How sharply a path may turn where two of its curves meet (radians) and still be swept through
/// as one smooth run; past this the join is a corner.
const CORNER_ANGLE: f64 = 1e-2;

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
        // each curve is sectioned on its own: chain the pieces where neighbouring curves meet
        // (a rectangle's sides), so a pipe sweeps one continuous path as in Fusion, not separate
        // flat-ended tubes notched at the corners
        let wires = if wires.len() > 1 { Shape::chain_wires(&wires, JOIN_TOL).map_err(ke)? } else { wires };
        self.state.intersections.insert(id, wires);
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
  /// along its first curve). A wire with sharp corners (a rectangle's sides meeting) is swept a
  /// smooth run at a time, and each corner is closed with a ball of the pipe's radius: exactly
  /// what a round section swept around the corner fills, as Fusion's pipes do.
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
      let corners = w.wire_corners(CORNER_ANGLE).map_err(ke)?;
      if corners.is_empty() {
        let w = w.wire_from_middle().map_err(ke)?;
        let inner = wall.map(|t| r - t);
        tubes.push(round_sweep(&w, r, inner)?);
        continue;
      }
      // a hollow pipe is the solid one with the solid one of the bore cut out, corners and all
      let runs = w.split_at_corners(CORNER_ANGLE).map_err(ke)?;
      let solid = |radius: f64| -> Result<Shape, String> {
        let mut parts = Vec::new();
        for run in &runs {
          parts.push(round_sweep(run, radius, None)?);
        }
        for c in &corners {
          parts.push(Shape::sphere(*c, radius).map_err(ke)?);
        }
        union_all(parts)
      };
      let mut tube = solid(r)?;
      if let Some(t) = wall {
        tube = tube.boolean(&solid(r - t)?, BoolOp::Subtract).map_err(ke)?;
      }
      tubes.push(tube);
    }
    let shape = if tubes.len() == 1 { tubes.remove(0) } else { union_all(tubes)? };
    // never hand on a broken solid: a pipe that can't be built cleanly is an error
    if !shape.is_valid() {
      return Err("the pipe could not be built cleanly along this path".into());
    }
    self.operate(id, shape, operation)?;
    Ok(None)
  }
}

/// A round section of `radius` (with a bore of `inner`) square to `path` where it starts, swept
/// along it.
fn round_sweep(path: &Shape, radius: f64, inner: Option<f64>) -> Result<Shape, String> {
  let (start, tangent) = path.wire_start().map_err(ke)?;
  let sec = fox_kernel::Plane::from_normal(start, tangent, tangent.any_orthonormal_vector()).map_err(ke)?;
  let holes =
    inner.map(|ri| Loop::new(vec![Seg::Circle { center: [0.0, 0.0], radius: ri, ccw: false }])).into_iter().collect();
  let outer = Loop::new(vec![Seg::Circle { center: [0.0, 0.0], radius, ccw: true }]);
  let face = Shape::face(&sec, &Profile { outer, holes }).map_err(ke)?;
  face.sweep(path).map_err(ke)
}
