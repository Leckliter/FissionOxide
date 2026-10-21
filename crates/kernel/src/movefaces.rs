//! Move Face with any rigid motion, as Fusion's direct Move Face does: the chosen faces turn
//! and/or slide and the body follows. Built from existing operations, no C++ of its own:
//!
//! - **One flat face** is tilted onto its new plane: the body with the face pushed in (so it
//!   lies wholly inside the new plane) is joined with the body with the face pulled out, cut
//!   off at the new plane. Neighbours keep their surfaces and grow or shrink to meet it, as
//!   [`Shape::offset_faces`] makes them.
//! - **A group of faces or curved faces** (a boss, a hole, a pocket) is lifted off the body by
//!   removing the faces (the neighbours close the gap) and moved: the material the faces
//!   bounded is joined back moved, the space they bounded is cut back moved.

use glam::{DMat4, DVec3, DVec4};

use crate::{BoolOp, KernelError, Result, Shape, SurfaceKind};

impl Shape {
  /// This solid with `faces` (1-based face-map indices) moved by the rigid motion `m`.
  pub fn move_faces(&self, faces: &[u32], m: DMat4) -> Result<Shape> {
    if faces.is_empty() {
      return Err(KernelError::Input("pick the faces to move".into()));
    }
    let info = self.faces()?;
    let picked: Vec<_> = info.iter().filter(|f| faces.contains(&f.id)).collect();
    if picked.len() != faces.len() {
      return Err(KernelError::Input("a face is not on this body".into()));
    }
    let flat = picked.iter().all(|f| f.kind == SurfaceKind::Plane);
    if faces.len() == 1 && flat {
      return self.tilt_face(faces[0], m).or_else(|e| self.move_feature(faces, m).map_err(|_| e));
    }
    match self.move_feature(faces, m) {
      Ok(s) => Ok(s),
      // a group of flat faces that don't bound a feature: tilt them one by one
      Err(e) if flat => self.tilt_each(faces, m).map_err(|_| e),
      Err(e) => Err(e),
    }
  }

  /// A flat face moved onto its moved plane; the neighbours extend or trim to meet it.
  fn tilt_face(&self, face: u32, m: DMat4) -> Result<Shape> {
    let f = self.faces()?.into_iter().find(|f| f.id == face).ok_or(KernelError::Input("no such face".into()))?;
    let (o, n) = (f.center, f.normal.normalize());
    let (o2, n2) = (m.transform_point3(o), m.transform_vector3(n).normalize());
    let cos = n.dot(n2);
    if cos < 0.1 {
      return Err(KernelError::Input("a flat face can't turn 85° or more onto its side".into()));
    }
    let b = self.face_shape(face)?.mass()?;
    let (lo, hi) = (b.bbox_min, b.bbox_max);
    // how far the face must go in and out (along its normal) to reach the new plane everywhere,
    // with room for neighbours that slant outward
    let (mut din, mut dout) = (0.0f64, 0.0f64);
    for i in 0..8 {
      let p = DVec3::new(
        if i & 1 == 0 { lo.x } else { hi.x },
        if i & 2 == 0 { lo.y } else { hi.y },
        if i & 4 == 0 { lo.z } else { hi.z },
      );
      let s = (p - o2).dot(n2) / cos;
      din = din.max(s);
      dout = dout.max(-s);
    }
    let size = (hi - lo).length().max(1e-3);
    let pad = |d: f64| if d > 1e-7 { d * 1.5 + size * 0.05 } else { 0.0 };
    let (din, dout) = (pad(din), pad(dout));
    if din == 0.0 && dout == 0.0 {
      return Err(KernelError::Input("the face stays on its own plane".into()));
    }
    let inner = if din > 0.0 { self.offset_faces(&[face], -din)? } else { self.clone() };
    let outer = if dout > 0.0 { self.offset_faces(&[face], dout)? } else { self.clone() };
    // everything on the material side of the new plane
    let all = self.mass()?;
    let l = (all.bbox_max - all.bbox_min).length() * 4.0 + din + dout + (o2 - o).length() + 10.0;
    let x = n2.any_orthonormal_vector();
    let frame = DMat4::from_cols(x.extend(0.0), n2.cross(x).extend(0.0), n2.extend(0.0), DVec4::from((o2, 1.0)));
    let half = Shape::make_box(DVec3::new(-l, -l, -l), DVec3::new(l, l, 0.0))?.transformed(frame)?;
    let kept = outer.boolean(&half, BoolOp::Intersect)?;
    one_solid(inner.boolean(&kept, BoolOp::Union)?)
  }

  /// Flat faces tilted one at a time, each found again by where it was after the one before.
  fn tilt_each(&self, faces: &[u32], m: DMat4) -> Result<Shape> {
    let info = self.faces()?;
    let mut at: Vec<(DVec3, DVec3)> =
      faces.iter().filter_map(|id| info.iter().find(|f| f.id == *id)).map(|f| (f.center, f.normal)).collect();
    let mut s = self.clone();
    while let Some((c, n)) = at.pop() {
      let now = s.faces()?;
      let f = now
        .iter()
        .filter(|f| f.kind == SurfaceKind::Plane && f.normal.dot(n) > 0.999 && (f.center - c).dot(n).abs() < 1e-6)
        .min_by(|a, b| (a.center - c).length().total_cmp(&(b.center - c).length()))
        .ok_or(KernelError::Kernel("a moved face was lost".into()))?;
      s = s.tilt_face(f.id, m)?;
    }
    Ok(s)
  }

  /// The faces as a feature: removed (the body heals), and what they bounded put back moved.
  fn move_feature(&self, faces: &[u32], m: DMat4) -> Result<Shape> {
    let base = self.remove_faces(faces)?;
    let (v0, v1) = (self.mass()?.volume, base.mass()?.volume);
    if base.faces()?.len() == self.faces()?.len() && (v0 - v1).abs() <= 1e-9 * v0.abs().max(1.0) {
      return Err(KernelError::Kernel(
        "the faces can't be lifted off the body (it can't be closed without them)".into(),
      ));
    }
    let part = |r: Result<Shape>| match r {
      Ok(s) => Ok(Some(s)),
      Err(KernelError::Empty) => Ok(None),
      Err(e) => Err(e),
    };
    // material the faces bounded (a boss), and space they bounded (a hole)
    let added = part(self.boolean(&base, BoolOp::Subtract))?;
    let removed = part(base.boolean(self, BoolOp::Subtract))?;
    let picked: Vec<_> = self.faces()?.into_iter().filter(|f| faces.contains(&f.id)).collect();
    let mut out = base;
    if let Some(a) = added {
      out = out.boolean(&grown(&a, &picked, m).transformed(m)?, BoolOp::Union)?;
    }
    if let Some(r) = removed {
      out = out.boolean(&grown(&r, &picked, m).transformed(m)?, BoolOp::Subtract)?;
    }
    one_solid(out)
  }
}

/// A lifted region with its other faces (where it met the body: a hole's openings, a boss's
/// foot) pushed out by as far as the motion carries any of it across them, so once moved it
/// still reaches through the body's skin (a tilted hole stays open) or into it (a tilted boss
/// stays seated). The region as it was when that isn't needed or can't be done.
fn grown(region: &Shape, picked: &[crate::FaceInfo], m: DMat4) -> Shape {
  let (Ok(b), Ok(fs)) = (region.mass(), region.faces()) else { return region.clone() };
  let size = (b.bbox_max - b.bbox_min).length();
  let own = |f: &crate::FaceInfo| {
    picked.iter().any(|g| {
      g.kind == f.kind
        && (g.center - f.center).length() < 1e-5 * size.max(1.0)
        && (g.area - f.area).abs() < 1e-5 * g.area.max(1.0)
    })
  };
  let ends: Vec<&crate::FaceInfo> = fs.iter().filter(|f| !own(f)).collect();
  if ends.is_empty() || ends.len() == fs.len() {
    return region.clone();
  }
  // how far the motion carries the region across its ends (along a flat end's normal)
  let mut reach = 0.0f64;
  for i in 0..8 {
    let p = DVec3::new(
      if i & 1 == 0 { b.bbox_min.x } else { b.bbox_max.x },
      if i & 2 == 0 { b.bbox_min.y } else { b.bbox_max.y },
      if i & 4 == 0 { b.bbox_min.z } else { b.bbox_max.z },
    );
    let d = m.transform_point3(p) - p;
    for e in &ends {
      reach = reach.max(if e.kind == SurfaceKind::Plane { d.dot(e.normal).abs() } else { d.length() });
    }
  }
  if reach < 1e-7 {
    return region.clone();
  }
  let ids: Vec<u32> = ends.iter().map(|f| f.id).collect();
  region
    .offset_faces(&ids, reach * 1.2 + size * 0.01)
    .ok()
    .filter(|s| s.solid_count() == 1)
    .unwrap_or_else(|| region.clone())
}

fn one_solid(s: Shape) -> Result<Shape> {
  match s.solid_count() {
    1 if s.is_valid() => Ok(s),
    1 => Err(KernelError::Kernel("moving the faces made an invalid solid".into())),
    _ => Err(KernelError::Kernel("the moved faces no longer meet the rest of the body".into())),
  }
}
