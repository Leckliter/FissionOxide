//! Regeneration of the second batch of solid features: sweeps along a guide rail or with a twist,
//! lofts along rails or a centre line, setback fillet corners, extrudes up to a curved face or a
//! body, emboss/deboss, boundary fill and replace face.

use fox_kernel::glam::DVec3;
use fox_kernel::{BoolOp, Inside, KernelError, Plane, Shape, SurfaceKind};

use super::{Ctx, FResult, Model, ke, pick_profiles, union_all};
use crate::{BodyId, FeatureId, FeatureKind, PathRef, PlaneRef};

/// What an extrude stops at besides a parallel flat face.
pub(super) enum Trim {
  /// A face (curved or tilted) of body index `body`, plus `offset` past it.
  Face { body: usize, face: u32, offset: f64 },
  /// The first contact with a body.
  Body { body: usize },
}

/// How a sweep's profile is steered along the path.
pub(super) enum Guide {
  Plain,
  Rail(Shape),
  Twist(f64),
}

pub(super) fn sweep(face: &Shape, path: &Shape, guide: &Guide) -> Result<Shape, String> {
  match guide {
    Guide::Plain => face.sweep(path),
    Guide::Rail(rail) => face.sweep_rail(path, rail),
    Guide::Twist(a) => face.sweep_twisted(path, *a),
  }
  .map_err(ke)
}

/// The cells a boundary fill with these tools would offer (after the whole timeline), for picking.
pub fn boundary_cells(model: &Model, tools: &[BodyId], planes: &[PlaneRef]) -> Result<Vec<Shape>, String> {
  let mut shapes = Vec::new();
  for b in tools {
    shapes.push(model.body(*b).map(|b| b.shape.clone()).ok_or(format!("body {} no longer exists", b.0))?);
  }
  let mut pls = Vec::new();
  for p in planes {
    pls.push(match p {
      PlaneRef::Origin { plane } => plane.plane(),
      PlaneRef::Construction { feature } => *model.planes.get(feature).ok_or("its construction plane failed")?,
      PlaneRef::Face { face } => {
        let body = model.body(face.body).ok_or("the plane's body no longer exists")?;
        let fid = face.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or("the face could not be found")?;
        let info = body.faces().iter().find(|f| f.id == fid).ok_or("the face could not be found")?;
        if info.kind != SurfaceKind::Plane {
          return Err("the face is not planar".into());
        }
        super::sketch_plane_on(info.normal, info.origin)?
      }
      // only where the plane is and which way it faces matter here
      PlaneRef::Free { origin, normal, .. } => {
        Plane::from_normal(DVec3::from(*origin), DVec3::from(*normal), DVec3::X).map_err(ke)?
      }
    });
  }
  cells_of(shapes, &pls)
}

/// The closed cells bodies and planes bound; planes become squares well past the bodies.
fn cells_of(mut shapes: Vec<Shape>, planes: &[Plane]) -> Result<Vec<Shape>, String> {
  if shapes.is_empty() {
    return Err("pick the bodies that bound the cells".into());
  }
  let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
  for s in &shapes {
    let m = s.mass().map_err(ke)?;
    lo = lo.min(m.bbox_min);
    hi = hi.max(m.bbox_max);
  }
  let (center, size) = ((lo + hi) / 2.0, 2.0 * (hi - lo).length() + 10.0);
  for pl in planes {
    let n = pl.normal();
    shapes.push(Shape::plane_face(center - n * (center - pl.origin).dot(n), n, size).map_err(ke)?);
  }
  let all = Shape::boundary_cells(&shapes).map_err(ke)?;
  if all.is_empty() {
    return Err("the tools don't enclose any cells".into());
  }
  Ok(all)
}

/// Re-blends the corners of a fillet where three or more of the filleted edges (`ids` of `orig`)
/// meet, `d` back from the vertex.
pub(super) fn setback(orig: &Shape, filleted: &Shape, ids: &[u32], d: f64) -> fox_kernel::Result<Shape> {
  let mut ends: Vec<(DVec3, usize)> = Vec::new();
  for e in orig.edges()?.iter().filter(|e| ids.contains(&e.id) && (e.start - e.end).length() > 1e-9) {
    for p in [e.start, e.end] {
      match ends.iter_mut().find(|(q, _)| (*q - p).length() < 1e-6) {
        Some(x) => x.1 += 1,
        None => ends.push((p, 1)),
      }
    }
  }
  let corners: Vec<DVec3> = ends.into_iter().filter(|(_, n)| *n >= 3).map(|(p, _)| p).collect();
  if corners.is_empty() {
    return Err(KernelError::Input("a setback needs a corner where three of the edges meet".into()));
  }
  filleted.setback_corners(&corners, d)
}

impl Ctx<'_> {
  pub(super) fn apply_solid(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::Emboss { sketch, profiles, face, depth, deboss, wrap } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (chosen, missing) = pick_profiles(&sk.profiles, profiles);
        if chosen.is_empty() {
          return Err("no profiles to emboss".into());
        }
        let d = self.len(depth)?;
        let (bi, info) = self.face_info(face)?;
        let faces = chosen.iter().map(|p| Shape::face(&sk.plane, p)).collect::<Result<Vec<_>, _>>().map_err(ke)?;
        // project from the sketch towards the face
        let mut dir = sk.plane.normal();
        if (info.center - sk.plane.origin).dot(dir) < 0.0 {
          dir = -dir;
        }
        let body = &self.state.bodies[bi];
        let tool = body.shape.emboss_tool(info.id, &faces, dir, d, *deboss, *wrap).map_err(ke)?;
        let shape = body.shape.boolean(&tool, if *deboss { BoolOp::Subtract } else { BoolOp::Union }).map_err(ke)?;
        let bid = body.id;
        self.state.bodies[bi] = body.with_shape(shape, id);
        self.split_multi(id, bid);
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::BoundaryFill { tools, planes, cells, operation, remove_tools } => {
        let mut shapes = Vec::new();
        for b in tools.iter() {
          let bi = self.body_index(*b)?;
          shapes.push(self.state.bodies[bi].shape.clone());
        }
        let mut pls = Vec::new();
        for p in planes.iter_mut() {
          pls.push(self.resolve_plane(p)?);
        }
        let all = cells_of(shapes, &pls)?;
        let mut keep = Vec::new();
        let mut lost = 0;
        if cells.is_empty() {
          keep = all;
        } else {
          let mut taken = vec![false; all.len()];
          for p in cells.iter() {
            let hit = all.iter().position(|c| c.classify(DVec3::from(*p)).is_ok_and(|w| w == Inside::In));
            match hit {
              Some(i) => taken[i] = true,
              None => lost += 1,
            }
          }
          for (c, t) in all.into_iter().zip(taken) {
            if t {
              keep.push(c);
            }
          }
        }
        if keep.is_empty() {
          return Err("none of the picked cells exist any more".into());
        }
        let tool = union_all(keep)?;
        if *remove_tools {
          self.state.bodies.retain(|b| !tools.contains(&b.id));
        }
        self.operate(id, tool, *operation)?;
        Ok((lost > 0).then(|| format!("{lost} picked cell(s) no longer exist")))
      }
      FeatureKind::ReplaceFace { faces, target, plane } => {
        if faces.is_empty() {
          return Err("pick the faces to replace".into());
        }
        let surface = match (target.as_mut(), plane.as_mut()) {
          (Some(fr), _) => {
            let (tbi, info) = self.face_info(fr)?;
            self.state.bodies[tbi].shape.face_shape(info.id).map_err(ke)?
          }
          (None, Some(p)) => {
            let pl = self.resolve_plane(p)?;
            Shape::plane_face(pl.origin, pl.normal(), 1.0).map_err(ke)?
          }
          (None, None) => return Err("pick the replacement surface or plane".into()),
        };
        let size = self.through_all(&Plane::XY);
        let ext = surface.extended_face(size, 0.0, DVec3::ZERO, DVec3::Z).map_err(ke)?;
        let mut by_body: Vec<(usize, Vec<(u32, DVec3, DVec3)>)> = Vec::new();
        for fr in faces.iter_mut() {
          let (bi, info) = self.face_info(fr)?;
          if info.kind != SurfaceKind::Plane {
            return Err("only flat faces can be replaced".into());
          }
          let f = (info.id, info.normal, info.center);
          match by_body.iter_mut().find(|(b, _)| *b == bi) {
            Some((_, v)) => v.push(f),
            None => by_body.push((bi, vec![f])),
          }
        }
        for (bi, list) in by_body {
          let shape = replace(&self.state.bodies[bi].shape, &list, &ext, size)?;
          let bid = self.state.bodies[bi].id;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
          self.split_multi(id, bid);
        }
        Ok(None)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }

  fn path_shape(&self, r: &PathRef) -> Result<Shape, String> {
    let ps = self.state.sketches.get(&r.sketch).ok_or("a rail or centre line sketch failed")?;
    crate::include::sketch_path(ps, &r.curves)
  }

  pub(super) fn sweep_guide(&self, rail: Option<&PathRef>, twist: &str) -> Result<Guide, String> {
    let a = if twist.trim().is_empty() { 0.0 } else { self.angle(twist)? };
    match rail {
      Some(_) if a.abs() > 1e-12 => Err("a sweep along a guide rail can't twist".into()),
      Some(r) => Ok(Guide::Rail(self.path_shape(r)?)),
      None if a.abs() > 1e-12 => Ok(Guide::Twist(a)),
      None => Ok(Guide::Plain),
    }
  }

  pub(super) fn loft_guided(
    &self,
    faces: &[Shape],
    ruled: bool,
    closed: bool,
    rails: &[PathRef],
    centerline: Option<&PathRef>,
  ) -> Result<Shape, String> {
    match (rails.is_empty(), centerline) {
      (true, None) => Shape::loft(faces, ruled, closed).map_err(ke),
      _ if closed => Err("a closed loft can't follow rails or a centre line".into()),
      (false, Some(_)) => Err("a loft follows rails or a centre line, not both".into()),
      (true, Some(c)) => Shape::loft_centerline(faces, &self.path_shape(c)?).map_err(ke),
      (false, None) => {
        let rs = rails.iter().map(|r| self.path_shape(r)).collect::<Result<Vec<_>, _>>()?;
        Shape::loft_rails(faces, &rs).map_err(ke)
      }
    }
  }

  /// Cuts an extrusion (run long from the sketch `plane`) back to where it meets a face or body.
  pub(super) fn trim_to_object(&self, tool: Shape, trim: Trim, plane: &Plane) -> Result<Shape, String> {
    let size = self.through_all(plane);
    let com = tool.mass().map_err(ke)?.center_of_mass;
    let n = if (com - plane.origin).dot(plane.normal()) < 0.0 { -plane.normal() } else { plane.normal() };
    // where the extrusion starts
    let start = com - n * (com - plane.origin).dot(n);
    let (pieces, body) = match trim {
      Trim::Face { body, face, offset } => {
        let f = self.state.bodies[body].shape.face_shape(face).map_err(ke)?;
        let ext = f.extended_face(size, offset, start, n).map_err(ke)?;
        (tool.split_by(&ext).map_err(ke)?, None)
      }
      Trim::Body { body } => {
        let b = &self.state.bodies[body].shape;
        (tool.split_by(b).map_err(ke)?, Some(b))
      }
    };
    if pieces.len() < 2 {
      return Err("the extrusion doesn't reach the target".into());
    }
    let start = Shape::plane_face(start, plane.normal(), 2.0 * size).map_err(ke)?;
    let tol = 1e-6 * size.max(1.0);
    let mut keep = Vec::new();
    for p in pieces {
      let touches = p.distance_to(&start).map_err(ke)?.0 < tol;
      let inside = match body {
        Some(b) => b.classify(p.mass().map_err(ke)?.center_of_mass).map_err(ke)? == Inside::In,
        None => false,
      };
      if touches && !inside {
        keep.push(p);
      }
    }
    union_all(keep).map_err(|_| "the extrusion doesn't reach the target".to_string())
  }
}

/// A body with flat faces (id, outward normal, centre) moved to an (extended) surface: each face
/// is pushed out past the surface, then everything beyond the surface above the faces cut away.
fn replace(body: &Shape, faces: &[(u32, DVec3, DVec3)], surface: &Shape, size: f64) -> Result<Shape, String> {
  let mut grown = body.clone();
  let mut columns = Vec::new();
  let mut beyond = Vec::new();
  for &(fid, n, c) in faces {
    let f = body.face_shape(fid).map_err(ke)?;
    grown = grown.boolean(&f.extrude(n * size).map_err(ke)?, BoolOp::Union).map_err(ke)?;
    columns.push(f.translated(-n * size).map_err(ke)?.extrude(n * (2.0 * size)).map_err(ke)?);
    beyond.push(surface.side_of(c + n * size).map_err(ke)?.signum());
  }
  let column = union_all(columns)?;
  let pieces = grown.split_by(surface).map_err(ke)?;
  if pieces.len() < 2 {
    return Err("the surface doesn't cross the faces' extension".into());
  }
  let mut keep = Vec::new();
  for p in pieces {
    let m = p.mass().map_err(ke)?;
    let side = surface.side_of(m.center_of_mass).map_err(ke)?.signum();
    let above = beyond.contains(&side)
      && p
        .boolean(&column, BoolOp::Intersect)
        .ok()
        .and_then(|x| x.mass().ok())
        .is_some_and(|x| x.volume > 1e-6 * m.volume);
    if !above {
      keep.push(p);
    }
  }
  union_all(keep).map_err(|_| "nothing of the body is left".to_string())
}
