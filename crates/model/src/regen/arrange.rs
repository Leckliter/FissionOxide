//! Regeneration of Arrange: copies of components laid flat and packed on an envelope (the
//! packing itself is in [`crate::arrange`]).

use fox_kernel::SurfaceKind;
use fox_kernel::glam::{DMat4, DQuat, DVec3};

use super::{Ctx, FResult, ke};
use crate::arrange::{self, ArrangeSolver, Envelope, Part};
use crate::{FeatureId, FeatureKind};

/// A component laid flat in the envelope's frame: the transform from world space, and the
/// outline seen from above.
struct Flat {
  m: DMat4,
  outline: Vec<[f64; 2]>,
}

impl Ctx<'_> {
  pub(super) fn apply_arrange(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::Arrange { items, plane, length, width, solver, spacing, frame } = kind else {
      return Err("not an arrange feature".into());
    };
    let (l, w, gap, fr) = (self.len(length)?, self.len(width)?, self.len(spacing)?, self.len(frame)?);
    if !(l > 0.0 && w > 0.0) {
      return Err("the envelope's length and width must be positive".into());
    }
    if gap < 0.0 || fr < 0.0 {
      return Err("the spacing and frame width can't be negative".into());
    }
    if 2.0 * fr >= l.min(w) {
      return Err("the frame leaves no room in the envelope".into());
    }
    if items.is_empty() {
      return Err("select components to arrange".into());
    }
    let pl = self.resolve_plane(plane)?;
    let env_frame =
      DMat4::from_cols(pl.x.extend(0.0), pl.y.extend(0.0), pl.normal().extend(0.0), pl.origin.extend(1.0));
    let to_local = env_frame.inverse();
    let mut parts = Vec::new();
    let mut copies = Vec::new();
    for item in items.iter() {
      let comp = self.components.iter().find(|c| c.id == item.component);
      let name = comp.map(|c| c.name.clone()).ok_or(format!("component {} no longer exists", item.component.0))?;
      if item.quantity == 0 {
        return Err(format!("{name}: the quantity must be at least 1"));
      }
      let comps = self.with_children(vec![item.component]);
      let src: Vec<usize> = comps
        .iter()
        .filter_map(|c| self.components.iter().find(|k| k.id == *c))
        .flat_map(|c| c.bodies.iter())
        .filter_map(|b| self.body_index(*b).ok())
        .collect();
      if src.is_empty() {
        return Err(format!("{name} has no bodies"));
      }
      let flat = self.lay_flat(&src, to_local, *solver)?;
      for _ in 0..item.quantity {
        parts.push(Part { outline: flat.outline.clone(), angles: item.rotation.angles(*solver) });
        copies.push((src.clone(), flat.m));
      }
    }
    let placed = arrange::pack(&parts, Envelope { size: [l, w], spacing: gap, frame: fr, solver: *solver });
    let missing = placed.iter().filter(|p| p.is_none()).count();
    if missing > 0 {
      return Err(format!("{missing} of {} parts don't fit in the envelope", placed.len()));
    }
    for ((src, m), at) in copies.into_iter().zip(placed.into_iter().flatten()) {
      let place =
        DMat4::from_translation(DVec3::new(at.offset[0], at.offset[1], 0.0)) * DMat4::from_rotation_z(at.angle);
      let m = env_frame * place * m;
      for bi in src {
        let shape = self.state.bodies[bi].shape.transformed(m).map_err(ke)?;
        self.new_body(id, shape);
      }
    }
    Ok(None)
  }

  /// Turns a component's bodies (in the envelope's frame) so their largest planar face lies on
  /// the plane, facing down, with everything above it.
  fn lay_flat(&self, src: &[usize], to_local: DMat4, solver: ArrangeSolver) -> Result<Flat, String> {
    // (area, how much it already faces down, body, face)
    let mut down: Option<(f64, f64, usize, u32)> = None;
    for &bi in src {
      for f in self.state.bodies[bi].faces().iter().filter(|f| f.kind == SurfaceKind::Plane) {
        let facing = -to_local.transform_vector3(f.normal).normalize_or_zero().z;
        let better = match down {
          None => true,
          Some((a, d, ..)) => {
            let tol = 1e-6 * a.max(f.area);
            f.area > a + tol || ((f.area - a).abs() <= tol && facing > d + 1e-9)
          }
        };
        if better {
          down = Some((f.area, facing, bi, f.id));
        }
      }
    }
    let turn = match down {
      Some((_, facing, bi, fid)) => {
        let body = &self.state.bodies[bi];
        let n =
          to_local.transform_vector3(body.faces().iter().find(|f| f.id == fid).map(|f| f.normal).unwrap_or(DVec3::Z));
        if facing > 1.0 - 1e-12 {
          DQuat::IDENTITY
        } else if facing < -1.0 + 1e-12 {
          DQuat::from_rotation_x(std::f64::consts::PI)
        } else {
          DQuat::from_rotation_arc(n.normalize(), DVec3::NEG_Z)
        }
      }
      None => DQuat::IDENTITY,
    };
    let m = DMat4::from_quat(turn) * to_local;
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for &bi in src {
      let mp = self.state.bodies[bi].shape.transformed(m).and_then(|s| s.mass()).map_err(ke)?;
      lo = lo.min(mp.bbox_min);
      hi = hi.max(mp.bbox_max);
    }
    let lift = DMat4::from_translation(DVec3::new(0.0, 0.0, -lo.z));
    let rect = vec![[lo.x, lo.y], [hi.x, lo.y], [hi.x, hi.y], [lo.x, hi.y]];
    let outline = match (solver, down) {
      (ArrangeSolver::TrueShape, Some((_, _, bi, fid))) => {
        let body = &self.state.bodies[bi];
        let tol = (body.diagonal() * 1e-3).clamp(1e-3, 0.5);
        let loops = body.shape.face_shape(fid).and_then(|f| f.face_loops(tol)).map_err(ke)?;
        let outer: Vec<[f64; 2]> = loops
          .first()
          .map(|l| l.iter().map(|p| m.transform_point3(*p)).map(|p| [p.x, p.y]).collect())
          .unwrap_or_default();
        // parts that overhang their down face (or have no usable outline) pack by their box
        let (olo, ohi) = outer.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(a, b), p| {
          ([a[0].min(p[0]), a[1].min(p[1])], [b[0].max(p[0]), b[1].max(p[1])])
        });
        let slack = 1e-6 * (hi - lo).length() + 1e-6;
        let covers =
          olo[0] <= lo.x + slack && olo[1] <= lo.y + slack && ohi[0] >= hi.x - slack && ohi[1] >= hi.y - slack;
        if outer.len() >= 3 && covers { outer } else { rect }
      }
      _ => rect,
    };
    Ok(Flat { m: lift * m, outline })
  }
}
