//! Regeneration of construction geometry (planes, axes, points by method), threads, coils,
//! pipes, ribs and webs, scale, split/remove faces, thicken, surfaces and mesh tools.

use fox_kernel::glam::{DMat3, DQuat, DVec3};
use fox_kernel::{BoolOp, EdgeInfo, FaceInfo, Plane, Shape, SurfaceKind};
use fox_sketch::{CurveKind, Loop, Profile, Seg};

use super::{Ctx, FResult, ke, refresh_face, union_all};
use crate::construct::{AxisDef, CoilSection, MeshOp, PipeSection, PlaneDef, PointDef, PointRef};
use crate::{EdgeRef, FaceRef, FeatureId, FeatureKind, Operation, PlaneRef};

/// ISO metric coarse pitches by nominal diameter (mm).
const ISO_COARSE: [(f64, f64); 18] = [
  (1.0, 0.25),
  (1.6, 0.35),
  (2.0, 0.4),
  (2.5, 0.45),
  (3.0, 0.5),
  (4.0, 0.7),
  (5.0, 0.8),
  (6.0, 1.0),
  (8.0, 1.25),
  (10.0, 1.5),
  (12.0, 1.75),
  (14.0, 2.0),
  (16.0, 2.0),
  (20.0, 2.5),
  (24.0, 3.0),
  (30.0, 3.5),
  (36.0, 4.0),
  (42.0, 4.5),
];

/// The ISO coarse pitch for a thread of this major diameter (nearest size).
pub fn iso_pitch(diameter: f64) -> f64 {
  ISO_COARSE.iter().min_by(|a, b| (a.0 - diameter).abs().total_cmp(&(b.0 - diameter).abs())).map(|s| s.1).unwrap_or(1.0)
}

impl Ctx<'_> {
  /// Features added after the first release of the timeline (see the module docs).
  pub(super) fn apply_extra(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::PlaneBy { def } => {
        let pl = self.plane_by(def)?;
        self.state.planes.insert(id, pl);
        Ok(None)
      }
      FeatureKind::ConstructionAxis { def } => {
        let (o, d) = self.axis_by(def)?;
        let d = d.try_normalize().ok_or("the axis has no direction")?;
        self.state.axes.insert(id, (o, d));
        Ok(None)
      }
      FeatureKind::ConstructionPoint { def } => {
        let p = self.point_by(def)?;
        self.state.points.insert(id, p);
        Ok(None)
      }
      FeatureKind::Thread { faces, pitch, length, offset, modeled, left, flip } => {
        if faces.is_empty() {
          return Err("select cylindrical faces to thread".into());
        }
        let mut labels = Vec::new();
        for fr in faces.iter_mut() {
          let (bi, info) = self.face_info(fr)?;
          if info.kind != SurfaceKind::Cylinder {
            return Err("threads go on cylindrical faces".into());
          }
          let (o, d, r) = (info.origin, info.normal.normalize(), info.radius);
          let (t0, t1, external) = self.cylinder_extent(bi, info.id, o, d).ok_or("the face has no mesh")?;
          let p = if pitch.trim().is_empty() { iso_pitch(2.0 * r) } else { self.len(pitch)? };
          if p <= 0.0 {
            return Err("the pitch must be positive".into());
          }
          let off = if offset.trim().is_empty() { 0.0 } else { self.len(offset)? };
          let full = t1 - t0 - off;
          let l = if length.trim().is_empty() { full } else { self.len(length)?.min(full) };
          if l <= p * 0.5 {
            return Err("the thread is shorter than half a pitch".into());
          }
          // the helix runs from the chosen end along the axis
          let (start, dir) = if *flip { (o + d * (t1 - off), -d) } else { (o + d * (t0 + off), d) };
          let x = perpendicular(dir);
          labels.push(format!("M{}x{}", trim(2.0 * r), trim(p)));
          if *modeled {
            let groove = thread_groove(start, dir, x, r, p, l, external, *left).map_err(ke)?;
            let body = &self.state.bodies[bi];
            let shape = body.shape.boolean(&groove, BoolOp::Subtract).map_err(ke)?;
            let bid = body.id;
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
            self.split_multi(id, bid);
          } else {
            self.state.threads.push((id, helix_points(start, dir, x, r, p, l, *left)));
          }
        }
        Ok(None)
      }
      FeatureKind::Coil { plane, at, diameter, pitch, height, section, section_size, left, operation } => {
        let pl = self.resolve_plane(plane)?;
        let (r, p, h, s) = (self.len(diameter)? / 2.0, self.len(pitch)?, self.len(height)?, self.len(section_size)?);
        if r <= 0.0 || p <= 0.0 || h <= 0.0 || s <= 0.0 {
          return Err("diameter, pitch, height and section size must be positive".into());
        }
        if s >= p && *section != CoilSection::Circular || s >= p * 0.999 {
          return Err("the section is too big for the pitch (the turns would overlap)".into());
        }
        if s / 2.0 >= r {
          return Err("the section is too big for the diameter".into());
        }
        let o = pl.to_world(*at);
        let (d, x) = (pl.normal(), pl.x);
        // the section lies in the plane through the axis, centred on the helix start
        let sec = Plane { origin: o + x * r, x, y: d };
        let half = s / 2.0;
        let lp = |pts: &[[f64; 2]]| -> Loop {
          Loop { segs: (0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect() }
        };
        let outer = match section {
          CoilSection::Circular => Loop { segs: vec![Seg::Circle { center: [0.0, 0.0], radius: half, ccw: true }] },
          CoilSection::Square => lp(&[[-half, -half], [half, -half], [half, half], [-half, half]]),
          CoilSection::TriangleOut => lp(&[[-half, -half], [half, 0.0], [-half, half]]),
          CoilSection::TriangleIn => lp(&[[half, -half], [half, half], [-half, 0.0]]),
        };
        let face = Shape::face(&sec, &Profile { outer, holes: vec![] }).map_err(ke)?;
        let coil = face.helix_sweep(o, d, x, r, p, h, *left).map_err(ke)?;
        self.operate(id, coil, *operation)?;
        Ok(None)
      }
      FeatureKind::Pipe { path_sketch, path_curves, diameter, wall, operation, section, distance } => {
        let ps = self.state.sketches.get(path_sketch).ok_or("the path sketch failed")?.clone();
        let fraction = if distance.trim().is_empty() {
          None
        } else {
          let f = self.factor(distance)?;
          if !(f > 0.0 && f <= 1.0) {
            return Err("the distance is a fraction of the path, above 0 and at most 1".into());
          }
          (f < 1.0).then_some(f)
        };
        let (path, start, tangent) = match crate::include::path_segs(&ps, path_curves)? {
          Some(_) if fraction.is_some() => {
            return Err("a pipe along included 3D geometry runs the whole path for now".into());
          }
          Some(segs) => {
            let first = segs.first().ok_or("the path is empty")?;
            (Shape::path3d(&segs).map_err(ke)?, first.start(), first.start_tangent())
          }
          None => {
            let mut segs = crate::sheet::open_path(&ps.sketch, path_curves)?;
            if let Some(f) = fraction {
              let total: f64 = segs.iter().map(seg_length).sum();
              segs = path_head(&segs, total * f);
            }
            let path = Shape::path(&ps.plane, &segs).map_err(ke)?;
            let first = segs.first().ok_or("the path is empty")?;
            let (start, tangent) = seg_start(first);
            (path, ps.plane.to_world(start), ps.plane.x * tangent[0] + ps.plane.y * tangent[1])
          }
        };
        let size = self.len(diameter)?;
        if size <= 0.0 {
          return Err("the size must be positive".into());
        }
        let n = ps.plane.normal();
        let t = tangent.try_normalize().ok_or("the path has no direction at its start")?;
        let sec = if *section == PipeSection::Circular {
          // the frame older files were built with: x (so the circle's seam) along the sketch
          // normal. Changing it would move the seam edge and side face of every saved round pipe.
          Plane::from_normal(start, t, n).map_err(ke)?
        } else {
          // the section's x runs across the path in the sketch plane, its y along the sketch normal
          Plane { origin: start, x: n.cross(t), y: n }
        };
        let w = if wall.trim().is_empty() { None } else { Some(self.len(wall)?) };
        let (outer, holes) = pipe_section(*section, size, w)?;
        let face = Shape::face(&sec, &Profile { outer, holes }).map_err(ke)?;
        let pipe = face.sweep(&path).map_err(ke)?;
        // a flat side's straight runs make one face, as in Fusion
        let pipe = if *section == PipeSection::Circular { pipe } else { pipe.unified().map_err(ke)? };
        self.operate(id, pipe, *operation)?;
        Ok(None)
      }
      FeatureKind::Rib { sketch, curves, thickness, depth, flip, web } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let t = self.len(thickness)?;
        if t <= 0.0 {
          return Err("the thickness must be positive".into());
        }
        let n = sk.plane.normal();
        let mut lines = Vec::new();
        for c in &sk.sketch.curves {
          if !(curves.is_empty() && !c.construction || curves.contains(&c.id)) {
            continue;
          }
          if let CurveKind::Line { a, b } = &c.kind
            && let (Some(a), Some(b)) = (sk.sketch.pos(*a), sk.sketch.pos(*b))
          {
            lines.push((sk.plane.to_world(a), sk.plane.to_world(b)));
          }
        }
        if lines.is_empty() {
          return Err("ribs and webs grow from sketch lines".into());
        }
        let reach = if depth.trim().is_empty() { None } else { Some(self.len(depth)?) };
        let far = self.through_all(&sk.plane);
        let mut tools = Vec::new();
        for (a, b) in lines {
          let u = (b - a).try_normalize().ok_or("a rib line has no length")?;
          let across = n.cross(u);
          // a rib is thick across the sketch plane and grows in it; a web the other way round
          let (thick, grow) = if *web { (across, n) } else { (n, across) };
          let grow = if *flip { -grow } else { grow };
          let len = (b - a).length();
          let sec = Plane { origin: a, x: u, y: thick };
          let rect = Loop {
            segs: vec![
              Seg::Line { a: [0.0, -t / 2.0], b: [len, -t / 2.0] },
              Seg::Line { a: [len, -t / 2.0], b: [len, t / 2.0] },
              Seg::Line { a: [len, t / 2.0], b: [0.0, t / 2.0] },
              Seg::Line { a: [0.0, t / 2.0], b: [0.0, -t / 2.0] },
            ],
          };
          let face = Shape::face(&sec, &Profile { outer: rect, holes: vec![] }).map_err(ke)?;
          if let Some(d) = reach {
            tools.push(face.extrude(grow * d).map_err(ke)?);
            continue;
          }
          // to the next face: grow a long slab, keep the piece outside the bodies that starts at
          // the line and ends against a body (it doesn't run out to the far end). Like Fusion, it
          // grows towards the material: the other way is tried if nothing is met.
          let solids: Vec<Shape> = self.state.bodies.iter().map(|b| b.shape.clone()).collect();
          if solids.is_empty() {
            return Err("there is no body for the rib to meet".into());
          }
          let all = union_all(solids)?;
          let mut found = None;
          for g in [grow, -grow] {
            let slab = face.extrude(g * far).map_err(ke)?;
            let rest = slab.boolean(&all, BoolOp::Subtract).map_err(ke)?;
            let probe = Shape::vertex((a + b) / 2.0 + g * (t * 0.01).min(0.01)).map_err(ke)?;
            for piece in rest.solids().map_err(ke)? {
              let touches = piece.distance_to(&probe).map(|(d, ..)| d < 1e-3).unwrap_or(false);
              let m = piece.mass().map_err(ke)?;
              let span = [m.bbox_min, m.bbox_max].iter().map(|q| (*q - a).dot(g)).fold(f64::MIN, f64::max);
              if touches && span < far * 0.99 {
                found = Some(piece);
                break;
              }
            }
            if found.is_some() {
              break;
            }
          }
          tools.push(found.ok_or("the rib doesn't meet the body (give it a depth)")?);
        }
        self.operate(id, union_all(tools)?, Operation::Join)?;
        Ok(None)
      }
      FeatureKind::Scale { bodies, center, factor, factors } => {
        let s = match factors {
          Some([x, y, z]) => DVec3::new(self.factor(x)?, self.factor(y)?, self.factor(z)?),
          None => DVec3::splat(self.factor(factor)?),
        };
        if s.min_element() <= 0.0 {
          return Err("scale factors must be positive".into());
        }
        let c = DVec3::from(*center);
        for bi in self.select(bodies)? {
          let shape = self.state.bodies[bi].shape.scaled(c, s).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::SplitFace { faces, tool } => {
        let pl = self.resolve_plane(tool)?;
        let size = self.through_all(&pl);
        let sq = Loop {
          segs: vec![
            Seg::Line { a: [-size, -size], b: [size, -size] },
            Seg::Line { a: [size, -size], b: [size, size] },
            Seg::Line { a: [size, size], b: [-size, size] },
            Seg::Line { a: [-size, size], b: [-size, -size] },
          ],
        };
        let cutter = Shape::face(&pl, &Profile { outer: sq, holes: vec![] }).map_err(ke)?;
        for (bi, fids) in self.faces_by_body(faces)? {
          let shape = self.state.bodies[bi].shape.split_faces(&fids, &cutter).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::RemoveFaces { faces } => {
        for (bi, fids) in self.faces_by_body(faces)? {
          let shape = self.state.bodies[bi].shape.remove_faces(&fids).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::Thicken { faces, bodies, thickness, symmetric, flip, operation, round } => {
        let t = self.len(thickness)? * if *flip { -1.0 } else { 1.0 };
        let mut skins = Vec::new();
        for (bi, fids) in self.faces_by_body(faces)? {
          for f in fids {
            skins.push(self.state.bodies[bi].shape.face_shape(f).map_err(ke)?);
          }
        }
        let mut consumed = Vec::new();
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          skins.push(self.state.bodies[bi].shape.clone());
          consumed.push(*b);
        }
        if skins.is_empty() {
          return Err("select faces or surface bodies to thicken".into());
        }
        let mut tools = Vec::new();
        for skin in skins {
          let n = skin.faces().map_err(ke)?.len() as u32;
          if n <= 1 {
            tools.extend(thicken_face(&skin, t, *symmetric)?);
            continue;
          }
          // a surface of several faces is offset as one, so the corners where its faces meet at
          // an angle close up (sharp or rounded) as Fusion's do
          if let Some(slab) = thicken_joined(&skin, t, *symmetric, *round) {
            tools.push(slab);
            continue;
          }
          // failing that each face is thickened on its own and the slabs are joined, which
          // leaves a notch at convex corners
          for f in 1..=n {
            tools.extend(thicken_face(&skin.face_shape(f).map_err(ke)?, t, *symmetric)?);
          }
        }
        // thickened surface bodies replace the surfaces
        self.state.bodies.retain(|b| !consumed.contains(&b.id) || b.shape.solid_count() > 0);
        self.operate(id, union_all(tools)?, *operation)?;
        Ok(None)
      }
      FeatureKind::SurfaceExtrude { sketch, curves, distance, symmetric, flip } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let d = self.len(distance)?;
        let n = sk.plane.normal() * if *flip { -1.0 } else { 1.0 };
        let mut faces = Vec::new();
        for w in sketch_wires(&sk.plane, &sk.sketch, curves)? {
          let w = if *symmetric { w.translated(-n * d).map_err(ke)? } else { w };
          faces.push(w.extrude(n * if *symmetric { 2.0 * d } else { d }).map_err(ke)?);
        }
        self.new_surface(id, faces)?;
        Ok(None)
      }
      FeatureKind::SurfaceRevolve { sketch, curves, axis, angle } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (o, d) = self.resolve_axis(axis, Some(*sketch))?;
        let a = self.angle(angle)?;
        let mut faces = Vec::new();
        for w in sketch_wires(&sk.plane, &sk.sketch, curves)? {
          faces.push(w.revolve(o, d, a).map_err(ke)?);
        }
        self.new_surface(id, faces)?;
        Ok(None)
      }
      FeatureKind::Patch { edges, sketch } => {
        let mut parts = Vec::new();
        if let Some(sk) = sketch {
          let sk = self.state.sketches.get(sk).ok_or("its sketch failed")?.clone();
          parts.extend(sketch_wires(&sk.plane, &sk.sketch, &[])?);
        }
        for e in edges.iter_mut() {
          let (bi, info) = self.edge_info(e)?;
          parts.push(self.state.bodies[bi].shape.edge_shape(info.id).map_err(ke)?);
        }
        if parts.is_empty() {
          return Err("pick the boundary edges or a sketch".into());
        }
        let boundary = Shape::compound(parts.iter()).map_err(ke)?;
        let face = boundary.patch().map_err(ke)?;
        self.new_body(id, face);
        Ok(None)
      }
      FeatureKind::Stitch { bodies, tolerance } => {
        let tol = self.len(tolerance)?;
        let idx: Vec<usize> = bodies.iter().map(|b| self.body_index(*b)).collect::<Result<_, _>>()?;
        if idx.is_empty() {
          return Err("select the surface bodies to stitch".into());
        }
        let all = Shape::compound(idx.iter().map(|&i| &self.state.bodies[i].shape)).map_err(ke)?;
        let sewn = all.sewn(tol).map_err(ke)?;
        let keep = self.state.bodies[idx[0]].id;
        self.state.bodies[idx[0]] = self.state.bodies[idx[0]].with_shape(sewn, id);
        self.state.bodies.retain(|b| b.id == keep || !bodies.contains(&b.id));
        Ok(None)
      }
      FeatureKind::Unstitch { bodies } => {
        for b in bodies.clone() {
          let bi = self.body_index(b)?;
          let shape = self.state.bodies[bi].shape.clone();
          let n = self.state.bodies[bi].faces().len() as u32;
          self.state.bodies.remove(bi);
          for f in 1..=n {
            let face = shape.face_shape(f).map_err(ke)?;
            self.new_body(id, face);
          }
        }
        Ok(None)
      }
      FeatureKind::Ruled { a, b } => {
        let (ba, ea) = self.edge_info(a)?;
        let (bb, eb) = self.edge_info(b)?;
        let sa = self.state.bodies[ba].shape.edge_shape(ea.id).map_err(ke)?;
        let sb = self.state.bodies[bb].shape.edge_shape(eb.id).map_err(ke)?;
        let face = sa.ruled_to(&sb).map_err(ke)?;
        self.new_body(id, face);
        Ok(None)
      }
      FeatureKind::SurfaceOffset { .. } => self.apply_surface(id, kind),
      FeatureKind::Trim { bodies, tool, flip } => {
        let pl = self.resolve_plane(tool)?;
        let n = pl.normal() * if *flip { -1.0 } else { 1.0 };
        let size = self.through_all(&pl);
        let sq = Loop {
          segs: vec![
            Seg::Line { a: [-size, -size], b: [size, -size] },
            Seg::Line { a: [size, -size], b: [size, size] },
            Seg::Line { a: [size, size], b: [-size, size] },
            Seg::Line { a: [-size, size], b: [-size, -size] },
          ],
        };
        let cutter = Shape::face(&pl, &Profile { outer: sq, holes: vec![] }).map_err(ke)?;
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let split = self.state.bodies[bi].shape.split_raw(&cutter).map_err(ke)?;
          // keep the faces behind the plane
          let mut keep = Vec::new();
          for f in split.faces().map_err(ke)? {
            if (f.center - pl.origin).dot(n) < 0.0 {
              keep.push(split.face_shape(f.id).map_err(ke)?);
            }
          }
          if keep.is_empty() {
            return Err("nothing is left behind the plane (try Flip)".into());
          }
          let skin = Shape::compound(keep.iter()).map_err(ke)?.sewn(1e-6).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(skin, id);
        }
        Ok(None)
      }
      FeatureKind::ReverseNormal { bodies } => {
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let shape = self.state.bodies[bi].shape.reversed().map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::Mesh { bodies, op, amount } => {
        let amount = fox_expr::eval(amount, fox_expr::Kind::Unitless, self.params).map_err(|e| e.to_string())?;
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let mesh = self.state.bodies[bi].mesh().cloned().ok_or("the body has no mesh")?;
          let (pos, tris) = crate::meshops::weld(&mesh);
          let (pos, tris) = match op {
            MeshOp::Reduce => crate::meshops::reduce(&pos, &tris, amount.clamp(0.02, 1.0)),
            MeshOp::Smooth => (crate::meshops::smooth(&pos, &tris, amount.round().clamp(1.0, 50.0) as usize), tris),
            MeshOp::Convert => (pos, tris),
          };
          let pos32: Vec<[f32; 3]> = pos.iter().map(|p| p.as_vec3().to_array()).collect();
          let shape = Shape::from_mesh(&pos32, &tris, *op == MeshOp::Convert).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::SurfaceExtend { .. } | FeatureKind::Untrim { .. } | FeatureKind::TrimWith { .. } => {
        self.apply_surface(id, kind)
      }
      FeatureKind::LoftedFlange { .. } | FeatureKind::MiterFlange { .. } => self.apply_sheet2(id, kind),
      FeatureKind::JointOrigin { .. }
      | FeatureKind::RigidGroup { .. }
      | FeatureKind::MotionLink { .. }
      | FeatureKind::InsertDesign { .. } => self.apply_assembly(id, kind),
      FeatureKind::CamFollower { .. } => self.apply_assembly(id, kind),
      FeatureKind::BaseFeature { .. } => self.apply_base(id, kind),
      _ => Err("this feature type isn't supported".into()),
    }
  }

  fn factor(&self, e: &str) -> Result<f64, String> {
    fox_expr::eval(e, fox_expr::Kind::Unitless, self.params).map_err(|err| format!("'{e}': {err}"))
  }

  /// New surface bodies: the faces sewn into one shell.
  fn new_surface(&mut self, id: FeatureId, faces: Vec<Shape>) -> Result<(), String> {
    if faces.is_empty() {
      return Err("the sketch has no curves to make a surface from".into());
    }
    let skin = if faces.len() == 1 {
      faces.into_iter().next().ok_or("no surface")?
    } else {
      Shape::compound(faces.iter()).map_err(ke)?.sewn(1e-6).map_err(ke)?
    };
    self.new_body(id, skin);
    Ok(())
  }

  fn faces_by_body(&mut self, faces: &mut [FaceRef]) -> Result<Vec<(usize, Vec<u32>)>, String> {
    let mut out: Vec<(usize, Vec<u32>)> = Vec::new();
    for fr in faces.iter_mut() {
      let (bi, info) = self.face_info(fr)?;
      match out.iter_mut().find(|(b, _)| *b == bi) {
        Some((_, v)) => v.push(info.id),
        None => out.push((bi, vec![info.id])),
      }
    }
    Ok(out)
  }

  /// Resolves a face reference to its body index and current face (refreshing the reference).
  pub(super) fn face_info(&mut self, fr: &mut FaceRef) -> Result<(usize, FaceInfo), String> {
    let bi = self.body_index(fr.body)?;
    let body = &self.state.bodies[bi];
    let fid = fr.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or("a face could not be found")?;
    refresh_face(fr, body, fid);
    let info = body.faces().iter().find(|f| f.id == fid).copied().ok_or("a face could not be found")?;
    Ok((bi, info))
  }

  /// Resolves an edge reference to its body index and current edge (refreshing the reference).
  pub(super) fn edge_info(&mut self, e: &mut EdgeRef) -> Result<(usize, EdgeInfo), String> {
    let bi = self.body_index(e.body)?;
    let body = &self.state.bodies[bi];
    let eid = e.resolve(body.edges(), body.faces(), body.diagonal()).ok_or("an edge could not be found")?;
    let info = body.edges().iter().find(|x| x.id == eid).copied().ok_or("an edge could not be found")?;
    *e = EdgeRef::from_edge(e.body, &info, body.faces());
    Ok((bi, info))
  }

  /// An edge as a polyline, start to end.
  fn edge_points(&mut self, e: &mut EdgeRef) -> Result<Vec<DVec3>, String> {
    let (bi, info) = self.edge_info(e)?;
    let body = &self.state.bodies[bi];
    let tol = (body.diagonal() * 0.0005).clamp(0.001, 0.05);
    let lines = body.shape.edge_shape(info.id).map_err(ke)?.edge_lines(tol).map_err(ke)?;
    let mut pts = lines.into_iter().next().map(|l| l.points).ok_or("the edge has no points")?;
    if pts.first().is_some_and(|p| (*p - info.start).length() > (*p - info.end).length()) {
      pts.reverse();
    }
    Ok(pts)
  }

  /// The point and unit tangent at fraction `t` of an edge's length.
  fn along(&mut self, e: &mut EdgeRef, t: &str) -> Result<(DVec3, DVec3), String> {
    let t = self.factor(t)?.clamp(0.0, 1.0);
    let pts = self.edge_points(e)?;
    let total: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut left = t * total;
    for w in pts.windows(2) {
      let l = (w[1] - w[0]).length();
      if left <= l || std::ptr::eq(w, pts.windows(2).last().unwrap_or(w)) {
        let dir = (w[1] - w[0]).try_normalize().ok_or("the edge has no length")?;
        return Ok((w[0] + dir * left.min(l), dir));
      }
      left -= l;
    }
    Err("the edge has no length".into())
  }

  fn point_of(&mut self, p: &mut PointRef) -> Result<DVec3, String> {
    match p {
      PointRef::At { at } => Ok(DVec3::from(*at)),
      PointRef::Vertex { edge, end } => {
        let (_, info) = self.edge_info(edge)?;
        Ok(if *end { info.end } else { info.start })
      }
      PointRef::Center { edge } => {
        let (_, info) = self.edge_info(edge)?;
        if info.kind != fox_kernel::CurveKind::Circle {
          return Err("the edge is not circular".into());
        }
        Ok(info.center)
      }
      PointRef::Construction { feature } => {
        self.state.points.get(feature).copied().ok_or_else(|| "its construction point failed".to_string())
      }
    }
  }

  /// The surface point of a face nearest `near`, with the outward normal there.
  fn face_point(&mut self, fr: &mut FaceRef, near: DVec3) -> Result<(DVec3, DVec3), String> {
    let (bi, info) = self.face_info(fr)?;
    match info.kind {
      SurfaceKind::Plane => Ok((near - info.normal * (near - info.origin).dot(info.normal), info.normal)),
      SurfaceKind::Cylinder => {
        let d = info.normal.normalize();
        let v = near - info.origin;
        let along = d * v.dot(d);
        let radial = (v - along).try_normalize().ok_or("the point is on the axis")?;
        let outward = self.outward(bi, info.id, radial);
        Ok((info.origin + along + radial * info.radius, outward))
      }
      SurfaceKind::Sphere => {
        let n = (near - info.origin).try_normalize().ok_or("the point is at the centre")?;
        let outward = self.outward(bi, info.id, n);
        Ok((info.origin + n * info.radius, outward))
      }
      _ => {
        // the nearest tessellation vertex of the face
        let m = self.state.bodies[bi].mesh().ok_or("the body has no mesh")?;
        let mut best: Option<(f64, DVec3, DVec3)> = None;
        for (t, tri) in m.indices.chunks_exact(3).enumerate() {
          if m.tri_face.get(t) != Some(&info.id) {
            continue;
          }
          for &i in tri {
            let (Some(p), Some(n)) = (m.positions.get(i as usize), m.normals.get(i as usize)) else { continue };
            let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            let d = (p - near).length();
            if best.is_none_or(|b| d < b.0) {
              best = Some((d, p, DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)));
            }
          }
        }
        best.map(|(_, p, n)| (p, n.normalize_or_zero())).ok_or_else(|| "the face has no mesh".to_string())
      }
    }
  }

  /// `dir` or its reverse, whichever agrees with the face's mesh normals (outward).
  fn outward(&self, bi: usize, face: u32, dir: DVec3) -> DVec3 {
    let Some(m) = self.state.bodies[bi].mesh() else { return dir };
    for (t, tri) in m.indices.chunks_exact(3).enumerate() {
      if m.tri_face.get(t) == Some(&face)
        && let Some(n) = m.normals.get(tri[0] as usize)
      {
        // compare with the radial direction at that vertex, not at the probe
        return if DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64).dot(dir) >= 0.0 { dir } else { -dir };
      }
    }
    dir
  }

  /// A cylinder face's extent along its axis (from `o` along `d`), and whether it is external
  /// (a boss: normals point away from the axis) rather than a hole.
  fn cylinder_extent(&self, bi: usize, face: u32, o: DVec3, d: DVec3) -> Option<(f64, f64, bool)> {
    let m = self.state.bodies[bi].mesh()?;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut votes = 0.0;
    for (t, tri) in m.indices.chunks_exact(3).enumerate() {
      if m.tri_face.get(t) != Some(&face) {
        continue;
      }
      for &i in tri {
        let (Some(p), Some(n)) = (m.positions.get(i as usize), m.normals.get(i as usize)) else { continue };
        let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        let s = (p - o).dot(d);
        lo = lo.min(s);
        hi = hi.max(s);
        let radial = p - o - d * s;
        votes += DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64).dot(radial);
      }
    }
    lo.is_finite().then_some((lo, hi, votes >= 0.0))
  }

  fn plane_by(&mut self, def: &mut PlaneDef) -> Result<Plane, String> {
    match def {
      PlaneDef::Angle { axis, base, angle } => {
        let (o, d) = self.resolve_axis(axis, None)?;
        let d = d.try_normalize().ok_or("the axis has no direction")?;
        let bp = self.resolve_plane(base)?;
        let n0 = bp.normal();
        let m = (n0 - d * n0.dot(d)).try_normalize().unwrap_or_else(|| perpendicular(d));
        let a = self.angle(angle)?;
        let n = DQuat::from_axis_angle(d, a) * m;
        Plane::from_normal(o, n, d).map_err(ke)
      }
      PlaneDef::Midplane { a, b } => {
        let (pa, pb) = (self.resolve_plane(a)?, self.resolve_plane(b)?);
        let (na, nb) = (pa.normal(), pb.normal());
        if na.cross(nb).length() < 1e-9 {
          let mid = pa.origin + na * ((pb.origin - pa.origin).dot(na) / 2.0);
          return Ok(Plane { origin: mid, ..pa });
        }
        let (o, _) = plane_line(&pa, &pb).ok_or("the planes don't meet")?;
        let nb = if na.dot(nb) < 0.0 { -nb } else { nb };
        let mut n = na + nb;
        if let (PlaneRef::Face { face: fa }, PlaneRef::Face { face: fb }) = (a, b) {
          // two body faces: Fusion takes the bisector that runs between them, with their centroids
          // on opposite sides (through the body at a convex edge), not the one outside it
          let (ca, cb) = (self.face_info(fa)?.1.origin, self.face_info(fb)?.1.origin);
          let side = |n: DVec3| (ca - o).dot(n) * (cb - o).dot(n);
          if side(na - nb) < side(n) {
            n = na - nb;
          }
        }
        Plane::from_normal(o, n, pa.x).map_err(ke)
      }
      PlaneDef::ThreePoints { a, b, c } => {
        let (a, b, c) = (self.point_of(a)?, self.point_of(b)?, self.point_of(c)?);
        let n = (b - a).cross(c - a);
        if n.length() < 1e-9 {
          return Err("the three points are in a line".into());
        }
        Plane::from_normal(a, n, b - a).map_err(ke)
      }
      PlaneDef::TwoEdges { a, b } => {
        let (_, ea) = self.edge_info(a)?;
        let (_, eb) = self.edge_info(b)?;
        if ea.kind != fox_kernel::CurveKind::Line || eb.kind != fox_kernel::CurveKind::Line {
          return Err("pick two straight edges".into());
        }
        let da = ea.end - ea.start;
        let mut n = da.cross(eb.end - eb.start);
        if n.length() < 1e-9 {
          n = da.cross(eb.start - ea.start);
        }
        if n.length() < 1e-9 {
          return Err("the edges are collinear".into());
        }
        if n.normalize().dot(eb.start - ea.start).abs() > 1e-4 {
          return Err("the edges are not in one plane".into());
        }
        Plane::from_normal(ea.start, n, da).map_err(ke)
      }
      PlaneDef::Tangent { face, near } => {
        let (p, n) = self.face_point(face, DVec3::from(*near))?;
        Plane::from_normal(p, n, perpendicular(n)).map_err(ke)
      }
      PlaneDef::AlongPath { edge, t } => {
        let (p, d) = self.along(edge, t)?;
        Plane::from_normal(p, d, perpendicular(d)).map_err(ke)
      }
    }
  }

  fn axis_by(&mut self, def: &mut AxisDef) -> Result<(DVec3, DVec3), String> {
    match def {
      AxisDef::Face { face } => {
        let (_, info) = self.face_info(face)?;
        match info.kind {
          SurfaceKind::Cylinder | SurfaceKind::Cone | SurfaceKind::Torus => Ok((info.origin, info.normal)),
          _ => Err("pick a cylinder, cone or torus".into()),
        }
      }
      AxisDef::Normal { face, near } => self.face_point(face, DVec3::from(*near)),
      AxisDef::TwoPlanes { a, b } => {
        let (pa, pb) = (self.resolve_plane(a)?, self.resolve_plane(b)?);
        plane_line(&pa, &pb).ok_or_else(|| "the planes are parallel".to_string())
      }
      AxisDef::TwoPoints { a, b } => {
        let (a, b) = (self.point_of(a)?, self.point_of(b)?);
        Ok((a, b - a))
      }
      AxisDef::Edge { edge } => {
        let (_, e) = self.edge_info(edge)?;
        match e.kind {
          fox_kernel::CurveKind::Line => Ok((e.start, e.end - e.start)),
          fox_kernel::CurveKind::Circle => Ok((e.center, e.axis)),
          _ => Err("pick a straight or circular edge".into()),
        }
      }
    }
  }

  fn point_by(&mut self, def: &mut PointDef) -> Result<DVec3, String> {
    match def {
      PointDef::Ref { point } => self.point_of(point),
      PointDef::TwoEdges { a, b } => {
        let (ba, ea) = self.edge_info(a)?;
        let (bb, eb) = self.edge_info(b)?;
        let sa = self.state.bodies[ba].shape.edge_shape(ea.id).map_err(ke)?;
        let sb = self.state.bodies[bb].shape.edge_shape(eb.id).map_err(ke)?;
        let (_, pa, pb) = sa.distance_to(&sb).map_err(ke)?;
        Ok((pa + pb) / 2.0)
      }
      PointDef::ThreePlanes { a, b, c } => {
        let ps = [self.resolve_plane(a)?, self.resolve_plane(b)?, self.resolve_plane(c)?];
        let m = DMat3::from_cols(ps[0].normal(), ps[1].normal(), ps[2].normal()).transpose();
        if m.determinant().abs() < 1e-9 {
          return Err("the planes don't meet in a point".into());
        }
        let rhs = DVec3::new(
          ps[0].normal().dot(ps[0].origin),
          ps[1].normal().dot(ps[1].origin),
          ps[2].normal().dot(ps[2].origin),
        );
        Ok(m.inverse() * rhs)
      }
      PointDef::Center { face } => {
        let (_, info) = self.face_info(face)?;
        match info.kind {
          SurfaceKind::Sphere | SurfaceKind::Torus | SurfaceKind::Cylinder | SurfaceKind::Cone => Ok(info.origin),
          _ => Ok(info.center),
        }
      }
      PointDef::EdgePlane { edge, plane } => {
        let pl = self.resolve_plane(plane)?;
        let n = pl.normal();
        let (_, info) = self.edge_info(edge)?;
        let dist = |p: DVec3| (p - pl.origin).dot(n);
        if info.kind == fox_kernel::CurveKind::Line {
          let d = info.end - info.start;
          let den = d.dot(n);
          if den.abs() < 1e-12 {
            return Err("the edge is parallel to the plane".into());
          }
          return Ok(info.start - d * (dist(info.start) / den));
        }
        let pts = self.edge_points(edge)?;
        for w in pts.windows(2) {
          let (a, b) = (dist(w[0]), dist(w[1]));
          if a == 0.0 || a.signum() != b.signum() {
            return Ok(w[0] + (w[1] - w[0]) * (a / (a - b)));
          }
        }
        Err("the edge doesn't cross the plane".into())
      }
      PointDef::AlongPath { edge, t } => Ok(self.along(edge, t)?.0),
    }
  }
}

/// Where two planes meet: a point on the line and its direction.
fn plane_line(a: &Plane, b: &Plane) -> Option<(DVec3, DVec3)> {
  let (na, nb) = (a.normal(), b.normal());
  let l = na.cross(nb);
  let l2 = l.length_squared();
  if l2 < 1e-18 {
    return None;
  }
  let (ca, cb) = (na.dot(a.origin), nb.dot(b.origin));
  Some(((nb.cross(l) * ca + l.cross(na) * cb) / l2, l.normalize()))
}

/// Any unit vector perpendicular to `d`.
fn perpendicular(d: DVec3) -> DVec3 {
  let a = if d.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
  (a - d * a.dot(d)).normalize()
}

fn trim(v: f64) -> String {
  fox_expr::trim_number((v * 1000.0).round() / 1000.0)
}

/// The outer loop and hole of a pipe section of this size, centred on the origin (see
/// [`PipeSection`]); `wall` makes it hollow by an inward offset.
fn pipe_section(section: PipeSection, size: f64, wall: Option<f64>) -> Result<(Loop, Vec<Loop>), String> {
  let poly = |pts: &[[f64; 2]]| -> Loop {
    Loop { segs: (0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect() }
  };
  // the inradius: a wall this thick leaves no hole
  let inner = match section {
    PipeSection::Circular | PipeSection::Square => size / 2.0,
    PipeSection::Triangular => size / 3.0,
  };
  // the loop with inradius `r`, counter-clockwise or (for a hole) clockwise
  let shape = |r: f64, ccw: bool| -> Loop {
    let l = match section {
      PipeSection::Circular => Loop { segs: vec![Seg::Circle { center: [0.0, 0.0], radius: r, ccw: true }] },
      PipeSection::Square => poly(&[[-r, -r], [r, -r], [r, r], [-r, r]]),
      PipeSection::Triangular => {
        // base along x at y = -size/2, apex at y = size/2: the centroid is size/6 below the path
        let c = -size / 6.0;
        let half = r * 3f64.sqrt();
        poly(&[[-half, c - r], [half, c - r], [0.0, c + 2.0 * r]])
      }
    };
    if ccw { l } else { l.reversed() }
  };
  let mut holes = Vec::new();
  if let Some(w) = wall {
    if w <= 0.0 || w >= inner {
      return Err("the wall must be thinner than half the section".into());
    }
    holes.push(shape(inner - w, false));
  }
  Ok((shape(inner, true), holes))
}

/// A point along a path segment at `u` (0 = start, 1 = end).
fn seg_point(s: &Seg, u: f64) -> [f64; 2] {
  match *s {
    Seg::Line { a, b } => [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u],
    Seg::Arc { center, radius, start_angle, sweep } => {
      let a = start_angle + sweep * u;
      [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
    }
    Seg::Circle { center, radius, ccw } => {
      let a = if ccw { u } else { -u } * std::f64::consts::TAU;
      [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
    }
    Seg::Bezier { p } => fox_sketch::bezier_at(&p, u),
    Seg::EllipseArc { center, major, minor, start, sweep } => {
      let a = major[0].hypot(major[1]).max(1e-300);
      let v = [-major[1] / a * minor, major[0] / a * minor];
      let (sn, c) = (start + sweep * u).sin_cos();
      [center[0] + major[0] * c + v[0] * sn, center[1] + major[1] * c + v[1] * sn]
    }
    Seg::Conic { p, w } => fox_sketch::conic_at(&p, w, u),
  }
}

/// Parameter steps for measuring segments that have no closed-form length.
const LENGTH_STEPS: usize = 512;

/// Cumulative chord lengths at `LENGTH_STEPS` even parameter steps.
fn seg_lengths(s: &Seg) -> Vec<f64> {
  let mut out = vec![0.0];
  let mut prev = seg_point(s, 0.0);
  for i in 1..=LENGTH_STEPS {
    let q = seg_point(s, i as f64 / LENGTH_STEPS as f64);
    out.push(out[i - 1] + (q[0] - prev[0]).hypot(q[1] - prev[1]));
    prev = q;
  }
  out
}

fn seg_length(s: &Seg) -> f64 {
  match *s {
    Seg::Line { a, b } => (b[0] - a[0]).hypot(b[1] - a[1]),
    Seg::Arc { radius, sweep, .. } => radius * sweep.abs(),
    Seg::Circle { radius, .. } => radius * std::f64::consts::TAU,
    _ => seg_lengths(s).last().copied().unwrap_or(0.0),
  }
}

/// The first `len` of a segment.
fn seg_head(s: &Seg, len: f64) -> Seg {
  let u = match *s {
    Seg::Line { .. } | Seg::Arc { .. } | Seg::Circle { .. } => len / seg_length(s).max(1e-300),
    _ => {
      let l = seg_lengths(s);
      let i = l.partition_point(|x| *x < len).clamp(1, LENGTH_STEPS);
      let f = (len - l[i - 1]) / (l[i] - l[i - 1]).max(1e-300);
      (i as f64 - 1.0 + f) / LENGTH_STEPS as f64
    }
  }
  .clamp(0.0, 1.0);
  match *s {
    Seg::Line { a, .. } => Seg::Line { a, b: seg_point(s, u) },
    Seg::Arc { center, radius, start_angle, sweep } => Seg::Arc { center, radius, start_angle, sweep: sweep * u },
    Seg::Circle { center, radius, ccw } => {
      let sweep = if ccw { u } else { -u } * std::f64::consts::TAU;
      Seg::Arc { center, radius, start_angle: 0.0, sweep }
    }
    Seg::Bezier { p } => {
      // de Casteljau: the control points of the piece up to `u`
      let lerp = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u];
      let (p01, p12, p23) = (lerp(p[0], p[1]), lerp(p[1], p[2]), lerp(p[2], p[3]));
      let (p012, p123) = (lerp(p01, p12), lerp(p12, p23));
      Seg::Bezier { p: [p[0], p01, p012, lerp(p012, p123)] }
    }
    Seg::EllipseArc { center, major, minor, start, sweep } => {
      Seg::EllipseArc { center, major, minor, start, sweep: sweep * u }
    }
    Seg::Conic { p, w } => {
      // de Casteljau on the weighted points: the piece up to `u` is again a conic, its control
      // point weighted w01 and its end w012; rescaled so both ends weigh 1
      let w01 = 1.0 - u + u * w;
      let w012 = (1.0 - u) * (1.0 - u) + 2.0 * u * (1.0 - u) * w + u * u;
      let mid = [((1.0 - u) * p[0][0] + u * w * p[1][0]) / w01, ((1.0 - u) * p[0][1] + u * w * p[1][1]) / w01];
      Seg::Conic { p: [p[0], mid, fox_sketch::conic_at(&p, w, u)], w: w01 / w012.sqrt() }
    }
  }
}

/// The first `len` of a chained path.
fn path_head(segs: &[Seg], len: f64) -> Vec<Seg> {
  let mut out = Vec::new();
  let mut left = len;
  for s in segs {
    let l = seg_length(s);
    if left >= l - 1e-9 {
      out.push(*s);
      left -= l;
      continue;
    }
    if left > 1e-9 {
      out.push(seg_head(s, left));
    }
    break;
  }
  out
}

/// Start point and direction (sketch coordinates) of a path segment.
fn seg_start(s: &Seg) -> ([f64; 2], [f64; 2]) {
  match s {
    Seg::Line { a, b } => (*a, [b[0] - a[0], b[1] - a[1]]),
    Seg::Arc { center, radius, start_angle, sweep } => {
      let (c, sn) = (start_angle.cos(), start_angle.sin());
      let dir = sweep.signum();
      ([center[0] + radius * c, center[1] + radius * sn], [-sn * dir, c * dir])
    }
    Seg::Circle { center, radius, .. } => ([center[0] + radius, center[1]], [0.0, 1.0]),
    Seg::Bezier { p } => (p[0], [p[1][0] - p[0][0], p[1][1] - p[0][1]]),
    Seg::EllipseArc { .. } | Seg::Conic { .. } => (s.start(), s.end_tangents().0),
  }
}

/// One wire per chosen sketch curve (default every non-construction curve).
pub(super) fn sketch_wires(
  plane: &Plane,
  sketch: &fox_sketch::Sketch,
  curves: &[fox_sketch::CurveId],
) -> Result<Vec<Shape>, String> {
  let mut out = Vec::new();
  for c in &sketch.curves {
    if !(curves.is_empty() && !c.construction || curves.contains(&c.id)) {
      continue;
    }
    for seg in sketch.curve_segs(c.id) {
      out.push(Shape::path(plane, &[seg]).map_err(ke)?);
    }
  }
  Ok(out)
}

/// A thread's helical V groove: ISO 60° flanks, cut into the material from the cylinder of
/// radius `r` (inwards on a boss, outwards in a hole).
fn thread_groove(
  start: DVec3,
  dir: DVec3,
  x: DVec3,
  r: f64,
  pitch: f64,
  length: f64,
  external: bool,
  left: bool,
) -> fox_kernel::Result<Shape> {
  let h = if external { 0.6134 * pitch } else { 0.5413 * pitch };
  // a little past the surface on the open side so the cut is clean
  let base = 0.25 * h;
  let half = (h + base) * (30f64).to_radians().tan();
  let (apex, back) = if external { (-h, base) } else { (h, -base) };
  let sec = Plane { origin: start + x * r, x, y: dir };
  let outer = Loop {
    segs: vec![
      Seg::Line { a: [back, -half], b: [apex, 0.0] },
      Seg::Line { a: [apex, 0.0], b: [back, half] },
      Seg::Line { a: [back, half], b: [back, -half] },
    ],
  };
  let face = Shape::face(&sec, &Profile { outer, holes: vec![] })?;
  face.helix_sweep(start, dir, x, r, pitch, length, left)
}

/// A cosmetic thread's helix as points (for display).
fn helix_points(start: DVec3, dir: DVec3, x: DVec3, r: f64, pitch: f64, length: f64, left: bool) -> Vec<DVec3> {
  let y = dir.cross(x);
  let turns = length / pitch;
  let n = ((turns * 48.0).ceil() as usize).clamp(8, 20_000);
  (0..=n)
    .map(|i| {
      let s = i as f64 / n as f64;
      let a = std::f64::consts::TAU * turns * s * if left { -1.0 } else { 1.0 };
      start + dir * (length * s) + (x * a.cos() + y * a.sin()) * r
    })
    .collect()
}

/// One face thickened by `t` (`symmetric`: `t` on each side).
fn thicken_face(face: &Shape, t: f64, symmetric: bool) -> Result<Vec<Shape>, String> {
  if !symmetric {
    return Ok(vec![face.thicken(t).map_err(ke)?]);
  }
  // one slab from the surface offset back: two slabs would leave every side wall in two pieces
  match face.offset_surface(-t).and_then(|back| back.thicken(2.0 * t)) {
    Ok(slab) => Ok(vec![slab]),
    Err(_) => Ok(vec![face.thicken(t).map_err(ke)?, face.thicken(-t).map_err(ke)?]),
  }
}

/// A shell of several faces thickened by `t` (`symmetric`: on each side) as one solid, the
/// corners where its faces meet sharp or `round`; `None` when OCCT can't do it.
fn thicken_joined(skin: &Shape, t: f64, symmetric: bool, round: bool) -> Option<Shape> {
  if !symmetric {
    return skin.thicken_joined(t, round).ok();
  }
  if !round {
    // offset back by t, then thickened by 2t through the surface: one slab whose side walls are
    // single faces
    if let Ok(slab) = skin.offset_joined(-t, false).and_then(|back| back.thicken_joined(2.0 * t, false)) {
      return Some(slab);
    }
  }
  // rounded corners are arcs about the surface's own edges, so each side is thickened apart
  let a = skin.thicken_joined(t, round).ok()?;
  let b = skin.thicken_joined(-t, round).ok()?;
  a.boolean(&b, BoolOp::Union).ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_conic_path_head_stays_on_the_conic() {
    let (p, w) = ([[0.0, 0.0], [10.0, 25.0], [40.0, 0.0]], fox_sketch::conic_weight(0.8));
    let whole = Seg::Conic { p, w };
    let head = seg_head(&whole, seg_length(&whole) * 0.3);
    let Seg::Conic { p: h, w: hw } = head else { panic!("{head:?}") };
    assert_eq!(h[0], p[0]);
    // every point of the head is a point of the whole conic, and it is 0.3 of the length
    for k in 0..=8 {
      let q = fox_sketch::conic_at(&h, hw, k as f64 / 8.0);
      let d = (0..=20000)
        .map(|i| seg_point(&whole, i as f64 / 20000.0))
        .map(|s| (s[0] - q[0]).hypot(s[1] - q[1]))
        .fold(f64::INFINITY, f64::min);
      assert!(d < 1e-2, "{q:?} is {d} off");
    }
    assert!((seg_length(&head) - seg_length(&whole) * 0.3).abs() < 1e-3 * seg_length(&whole));
  }
}
