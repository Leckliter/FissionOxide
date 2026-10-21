//! Regeneration of construction geometry (planes, axes, points by method), threads, coils,
//! pipes, ribs and webs, scale, split/remove faces, thicken, surfaces and mesh tools.

use fox_kernel::glam::{DMat3, DQuat, DVec3};
use fox_kernel::{BoolOp, EdgeInfo, FaceInfo, Plane, Shape, SurfaceKind};
use fox_sketch::behaviour::ISO_THREADS;
use fox_sketch::{CurveId, CurveKind, Loop, Profile, Seg};

use super::{Ctx, FResult, ke, refresh_face, union_all};
use crate::construct::{AxisDef, CoilSection, LineRef, MeshOp, PipeSection, PlaneDef, PointDef, PointRef};
use crate::{AxisRef, EdgeRef, FaceRef, FeatureId, FeatureKind, Operation, OriginPlane, PathRef, PlaneRef};

/// The ISO coarse pitch for a thread of this major diameter (nearest size).
pub fn iso_pitch(diameter: f64) -> f64 {
  crate::thread::coarse_pitch(diameter)
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
      FeatureKind::Thread { faces, pitch, length, offset, modeled, left, flip, designation, class } => {
        if faces.is_empty() {
          return Err("select cylindrical faces to thread".into());
        }
        // a designation is new data no file from before ISO_THREADS has, so it always gets the
        // ISO behaviour; a plain thread in an older design rebuilds the legacy way
        let iso = self.behaviours.has(ISO_THREADS) || !designation.trim().is_empty();
        let mut labels = Vec::new();
        for fr in faces.iter_mut() {
          let (bi, info) = if iso { self.thread_face_info(fr)? } else { self.face_info(fr)? };
          if info.kind != SurfaceKind::Cylinder {
            return Err("threads go on cylindrical faces".into());
          }
          if !iso {
            labels.push(self.legacy_thread(id, bi, &info, pitch, length, offset, *modeled, *left, *flip)?);
            continue;
          }
          let (o, d, r) = (info.origin, canonical_axis(info.normal.normalize()), info.radius);
          let (t0, t1, external) = self.cylinder_extent(bi, info.id, o, d).ok_or("the face has no mesh")?;
          let (t0, t1) = self.exact_extent(bi, info.id, o, d, (t0, t1));
          let (dims, label) = if designation.trim().is_empty() {
            let p = if pitch.trim().is_empty() { iso_pitch(2.0 * r) } else { self.len(pitch)? };
            if p <= 0.0 {
              return Err("the pitch must be positive".into());
            }
            (crate::thread::basic_dims(2.0 * r, p, !external), format!("M{}x{}", trim(2.0 * r), trim(p)))
          } else {
            let (nominal, p) = crate::thread::parse_designation(designation)
              .ok_or_else(|| format!("unknown thread size '{designation}' (ISO metric, as M10x1.5)"))?;
            crate::thread::check_drawn_size(nominal, p, 2.0 * r, !external)?;
            (crate::thread::iso_dims(nominal, p, class, !external)?, format!("M{}x{}", trim(nominal), trim(p)))
          };
          let p = dims.pitch;
          let off = if offset.trim().is_empty() { 0.0 } else { self.len(offset)? };
          let full = t1 - t0 - off;
          let l = if length.trim().is_empty() { full } else { self.len(length)?.min(full) };
          if l <= p * 0.5 {
            return Err("the thread is shorter than half a pitch".into());
          }
          // the threaded span along the axis, from the face's low end (or its high end)
          let (a, b) = if *flip { (t1 - off - l, t1 - off) } else { (t0 + off, t0 + off + l) };
          let rc = dims.crest / 2.0;
          if (rc - r).abs() > 1e-7 {
            self.resize_cylinder(id, bi, (o, d), (t0, t1), (r, rc), external)?;
          }
          labels.push(label);
          if *modeled {
            let groove = thread_groove(o, d, (a, b), &dims, external, *left).map_err(ke)?;
            let body = &self.state.bodies[bi];
            // one crest face, not a strip per turn: only the threaded face's strips merge
            let shape = body
              .shape
              .boolean(&groove, BoolOp::Subtract)
              .map_err(ke)?
              .merge_cylinder_strips(o, d, rc, (t0, t1))
              .map_err(ke)?;
            let bid = body.id;
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
            self.split_multi(id, bid);
          } else {
            let (start, dir) = if *flip { (o + d * b, -d) } else { (o + d * a, d) };
            self.state.threads.push((id, helix_points(start, dir, perpendicular(dir), rc, p, l, *left)));
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
          Loop::new((0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect())
        };
        let outer = match section {
          CoilSection::Circular => Loop::new(vec![Seg::Circle { center: [0.0, 0.0], radius: half, ccw: true }]),
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
        if self.state.curves3d.contains_key(path_sketch) {
          let picked: Vec<u32> = path_curves.iter().map(|c| c.0).collect();
          return self.pipe_along_curves(id, *path_sketch, &picked, diameter, wall, *operation, *section, distance);
        }
        if let Some(wires) = self.state.intersections.get(path_sketch).cloned() {
          if *section != PipeSection::Circular || !distance.trim().is_empty() {
            return Err("a pipe along an intersection curve is round and runs the whole curve".into());
          }
          let r = self.len(diameter)? / 2.0;
          let w = if wall.trim().is_empty() { None } else { Some(self.len(wall)?) };
          if r <= 0.0 || w.is_some_and(|w| w <= 0.0 || w >= r) {
            return Err("the diameter must be positive and the wall thinner than the radius".into());
          }
          return self.pipe_along(id, &wires, r, w, *operation);
        }
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
            src: Vec::new(),
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
          src: Vec::new(),
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
          src: Vec::new(),
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
      FeatureKind::JoinByBend { .. } | FeatureKind::SheetFillet { .. } | FeatureKind::SheetChamfer { .. } => {
        self.apply_sheet_modify(id, kind)
      }
      FeatureKind::JointOrigin { .. }
      | FeatureKind::RigidGroup { .. }
      | FeatureKind::MotionLink { .. }
      | FeatureKind::InsertDesign { .. } => self.apply_assembly(id, kind),
      FeatureKind::CamFollower { .. } => self.apply_assembly(id, kind),
      FeatureKind::AlignConstraint { .. } => self.apply_assembly(id, kind),
      FeatureKind::PlaneConstraint { .. } => self.apply_assembly(id, kind),
      FeatureKind::BaseFeature { .. } => self.apply_base(id, kind),
      FeatureKind::Form { .. } => self.apply_form(id, kind),
      FeatureKind::BoundingSolid { .. } => self.apply_bounding(id, kind),
      FeatureKind::ProjectToSurface { .. } => self.apply_project(id, kind),
      FeatureKind::PathPattern { .. } => self.apply_path_pattern(id, kind),
      FeatureKind::Arrange { .. } => self.apply_arrange(id, kind),
      FeatureKind::FullRoundFillet { sets } => self.full_round(id, sets),
      FeatureKind::IntersectionCurve { .. } => self.apply_curves3d(id, kind),
      FeatureKind::Canvas { plane, .. } => {
        let pl = self.resolve_plane(plane)?;
        self.state.canvases.insert(id, pl);
        Ok(None)
      }
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

  pub(super) fn faces_by_body(&mut self, faces: &mut [FaceRef]) -> Result<Vec<(usize, Vec<u32>)>, String> {
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

  /// Resolves a thread's face reference like [`Ctx::face_info`], but among cylinders scores how
  /// far the picked point is off each one's surface too: a tube's bore and outside share a
  /// centroid, and the plain score can't tell them apart. Only threads with [`ISO_THREADS`] use
  /// it, so other features (and older threads) resolve as they did.
  fn thread_face_info(&mut self, fr: &mut FaceRef) -> Result<(usize, FaceInfo), String> {
    let bi = self.body_index(fr.body)?;
    let body = &self.state.bodies[bi];
    let (p, n0) = (DVec3::from(fr.point), DVec3::from(fr.normal));
    let score = |f: &FaceInfo| {
      let v = p - f.origin;
      let off = ((v - f.normal * v.dot(f.normal)).length() - f.radius).abs();
      (p - f.center).length() + off
    };
    let fid = body
      .faces()
      .iter()
      .filter(|f| f.kind == SurfaceKind::Cylinder && f.normal.dot(n0).abs() >= 0.999)
      .min_by(|a, b| score(a).total_cmp(&score(b)))
      .map(|f| f.id);
    let Some(fid) = fid else { return self.face_info(fr) };
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

  /// A straight edge, axis or sketch line as a point on it and its direction (for an edge, the
  /// start and start-to-end).
  fn line_of(&mut self, l: &mut LineRef) -> Result<(DVec3, DVec3), String> {
    match l {
      LineRef::Edge(e) => {
        let (_, info) = self.edge_info(e)?;
        if info.kind != fox_kernel::CurveKind::Line {
          return Err("pick straight edges, axes or sketch lines".into());
        }
        Ok((info.start, info.end - info.start))
      }
      LineRef::Sketch { sketch, curve } => self.resolve_axis(&mut AxisRef::SketchLine { curve: *curve }, Some(*sketch)),
      LineRef::Axis(a) => {
        if matches!(a, AxisRef::SketchLine { .. }) {
          return Err("give a sketch line as {sketch, curve}".into());
        }
        self.resolve_axis(a, None)
      }
    }
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

  /// Points along sketch curves in world space, start to end (one chain when there are several).
  fn curve_points(&self, path: &PathRef) -> Result<Vec<DVec3>, String> {
    let sk = self.state.sketches.get(&path.sketch).ok_or("the path's sketch failed")?;
    const STEPS: usize = 256;
    if let [c] = path.curves[..]
      && let Some(c3) = sk.sketch.curve3d(c)
    {
      return Ok(c3.kind.polyline(STEPS).into_iter().map(DVec3::from).collect());
    }
    if let Some(segs) = crate::include::path_segs(sk, &path.curves)? {
      // through included 3D geometry: sample the wire
      let wire = Shape::path3d(&segs).map_err(ke)?;
      let mut pts: Vec<DVec3> = Vec::new();
      for l in wire.edge_lines(0.001).map_err(ke)? {
        let mut q = l.points;
        if let (Some(last), Some(first), Some(end)) = (pts.last(), q.first(), q.last())
          && (*last - *first).length() > (*last - *end).length()
        {
          q.reverse();
        }
        pts.extend(q);
      }
      return Ok(pts);
    }
    let segs = match path.curves[..] {
      // a lone curve keeps its own direction (and may be closed)
      [c] => sk.sketch.curve_segs(c),
      _ => crate::sheet::open_path(&sk.sketch, &path.curves)?,
    };
    let mut pts: Vec<DVec3> = Vec::new();
    for s in &segs {
      let n = if matches!(s, Seg::Line { .. }) { 1 } else { STEPS };
      let from = usize::from(!pts.is_empty());
      pts.extend((from..=n).map(|i| sk.plane.to_world(seg_point(s, i as f64 / n as f64))));
    }
    if pts.len() < 2 {
      return Err("the path has no curves".into());
    }
    Ok(pts)
  }

  /// The point and unit tangent at fraction `t` of an edge's length (from its start), or
  /// `distance` along it. With [`EXACT_EDGE_FRAMES`] a fraction is taken on the exact curve;
  /// designs made before it (and distances) walk the edge's tessellated polyline and take the
  /// direction of the segment the point falls on.
  ///
  /// [`EXACT_EDGE_FRAMES`]: fox_sketch::behaviour::EXACT_EDGE_FRAMES
  fn along(&mut self, e: &mut EdgeRef, t: &str, distance: &str) -> Result<(DVec3, DVec3), String> {
    if distance.trim().is_empty() && self.behaviours.has(fox_sketch::behaviour::EXACT_EDGE_FRAMES) {
      let t = self.factor(t)?.clamp(0.0, 1.0);
      let (bi, info) = self.edge_info(e)?;
      let edge = self.state.bodies[bi].shape.edge_shape(info.id).map_err(ke)?;
      return edge.along_curve(t, Some(info.start)).map_err(ke);
    }
    let pts = self.edge_points(e)?;
    self.along_points(&pts, t, distance)
  }

  /// As [`Ctx::along`], along sketch curves: with
  /// [`EXACT_EDGE_FRAMES`](fox_sketch::behaviour::EXACT_EDGE_FRAMES) a fraction of one plane
  /// curve is taken on the exact curve; anything else follows the sampled path.
  fn along_curve(&self, path: &PathRef, t: &str, distance: &str) -> Result<(DVec3, DVec3), String> {
    if distance.trim().is_empty()
      && self.behaviours.has(fox_sketch::behaviour::EXACT_EDGE_FRAMES)
      && let [c] = path.curves[..]
      && let Some(r) = self.along_sketch(path.sketch, c, t)?
    {
      return Ok(r);
    }
    let pts = self.curve_points(path)?;
    self.along_points(&pts, t, distance)
  }

  /// The point and unit tangent at fraction `t` of a plane sketch curve's length, from its start
  /// point, in world coordinates, on the exact curve; `None` for a 3D or included curve.
  fn along_sketch(&self, sketch: FeatureId, curve: CurveId, t: &str) -> Result<Option<(DVec3, DVec3)>, String> {
    let t = self.factor(t)?.clamp(0.0, 1.0);
    let sk = self.state.sketches.get(&sketch).ok_or("its sketch failed")?;
    if sk.sketch.curve3d(curve).is_some() || crate::include::path_segs(sk, &[curve])?.is_some() {
      return Ok(None);
    }
    let segs = sk.sketch.curve_segs(curve);
    let first = segs.first().ok_or("the sketch curve no longer exists")?;
    let start = sk.plane.to_world(first.start());
    let wire = Shape::path(&sk.plane, &segs).map_err(ke)?;
    wire.along_curve(t, Some(start)).map(Some).map_err(ke)
  }

  /// The point and unit tangent at fraction `t` of a polyline's length, or (when set) at
  /// `distance` from its start, clamped to its length.
  fn along_points(&self, pts: &[DVec3], t: &str, distance: &str) -> Result<(DVec3, DVec3), String> {
    let total: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut left = if distance.trim().is_empty() {
      self.factor(t)?.clamp(0.0, 1.0) * total
    } else {
      self.len(distance)?.clamp(0.0, total)
    };
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

  /// A thread on one face as designs without [`ISO_THREADS`] build it: a sharp V groove from the
  /// face's end along its own axis direction, swept the thread's length; returns its label.
  #[allow(clippy::too_many_arguments)]
  fn legacy_thread(
    &mut self,
    id: FeatureId,
    bi: usize,
    info: &FaceInfo,
    pitch: &str,
    length: &str,
    offset: &str,
    modeled: bool,
    left: bool,
    flip: bool,
  ) -> Result<String, String> {
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
    let (start, dir) = if flip { (o + d * (t1 - off), -d) } else { (o + d * (t0 + off), d) };
    let x = perpendicular(dir);
    if modeled {
      let groove = legacy_thread_groove(start, dir, x, r, p, l, external, left).map_err(ke)?;
      let body = &self.state.bodies[bi];
      let shape = body.shape.boolean(&groove, BoolOp::Subtract).map_err(ke)?;
      let bid = body.id;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
      self.split_multi(id, bid);
    } else {
      self.state.threads.push((id, helix_points(start, dir, x, r, p, l, left)));
    }
    Ok(format!("M{}x{}", trim(2.0 * r), trim(p)))
  }

  /// A cylindrical face's extent along its axis from its edges (exact, where the mesh's extent
  /// `approx` is only as good as single precision); `approx` when the edges don't bound it.
  fn exact_extent(&self, bi: usize, face: u32, o: DVec3, d: DVec3, approx: (f64, f64)) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for e in self.state.bodies[bi].edges().iter().filter(|e| e.faces.contains(&face)) {
      for p in [e.start, e.mid, e.end] {
        let s = (p - o).dot(d);
        lo = lo.min(s);
        hi = hi.max(s);
      }
    }
    let tol = 1e-4 * (approx.1 - approx.0).abs().max(1.0);
    if (lo - approx.0).abs() < tol && (hi - approx.1).abs() < tol { (lo, hi) } else { approx }
  }

  /// Resizes a cylindrical face spanning `span` along the axis from radius `r` to `rc`: cuts away
  /// or adds the ring between the two (a boss shrinks or grows outside, a hole inside).
  fn resize_cylinder(
    &mut self,
    id: FeatureId,
    bi: usize,
    (o, d): (DVec3, DVec3),
    span: (f64, f64),
    (r, rc): (f64, f64),
    external: bool,
  ) -> Result<(), String> {
    let (base, h) = (o + d * span.0, span.1 - span.0);
    let cyl = |radius: f64| Shape::cylinder(base, d, radius, h).map_err(ke);
    // the ring reaches past the face into the air when removing, into the material when adding
    let delta = (rc - r).abs();
    let (inner, outer) = if rc < r { (rc, r + delta) } else { ((r - delta).max(r * 0.5), rc) };
    let ring = cyl(outer)?.boolean(&cyl(inner)?, BoolOp::Subtract).map_err(ke)?;
    let op = if external == (rc < r) { BoolOp::Subtract } else { BoolOp::Union };
    let body = &self.state.bodies[bi];
    let shape = body.shape.boolean(&ring, op).map_err(ke)?;
    self.state.bodies[bi] = body.with_shape(shape, id);
    Ok(())
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
        // the bisector it picks was matched to Fusion with the legacy normals (XZ facing −Y; case
        // 347): with Fusion's (+Y) it would take the other bisector between XY and XZ
        let legacy = crate::Frames::Legacy;
        let (pa, pb) = (self.resolve_plane_framed(a, legacy)?, self.resolve_plane_framed(b, legacy)?);
        let (na, nb) = (pa.normal(), pb.normal());
        if na.cross(nb).length() < 1e-9 {
          let mid = pa.origin + na * ((pb.origin - pa.origin).dot(na) / 2.0);
          return Ok(Plane { origin: mid, ..pa });
        }
        let (o, _) = plane_line(&pa, &pb).ok_or("the planes don't meet")?;
        let nb = if na.dot(nb) < 0.0 { -nb } else { nb };
        let mut n = na + nb;
        if let (PlaneRef::Face { face: fa, .. }, PlaneRef::Face { face: fb, .. }) = (a, b) {
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
        // Fusion's frame: the world origin dropped onto the plane, x = Z × n (world X when the
        // normal is along Z), y = n × x
        let n = n.normalize();
        let x = DVec3::Z.cross(n);
        let x = if x.length() < 1e-9 { DVec3::X } else { x };
        Plane::from_normal(n * n.dot(a), n, x).map_err(ke)
      }
      PlaneDef::TwoEdges { a, b } => {
        let (oa, da) = self.line_of(a)?;
        let (ob, db) = self.line_of(b)?;
        let mut n = da.cross(db);
        if n.length() < 1e-9 {
          n = da.cross(ob - oa);
        }
        if n.length() < 1e-9 {
          return Err("the edges are collinear".into());
        }
        if n.normalize().dot(ob - oa).abs() > 1e-4 {
          return Err("the edges are not in one plane".into());
        }
        Plane::from_normal(oa, n, da).map_err(ke)
      }
      PlaneDef::Tangent { face, near } => {
        let (p, n) = self.face_point(face, DVec3::from(*near))?;
        Plane::from_normal(p, n, perpendicular(n)).map_err(ke)
      }
      PlaneDef::AlongPath { edge, t, distance } => {
        let (p, d) = self.along(edge, t, distance)?;
        Plane::from_normal(p, d, perpendicular(d)).map_err(ke)
      }
      PlaneDef::AlongCurve { path, t, distance } => {
        let (p, d) = self.along_curve(path, t, distance)?;
        Plane::from_normal(p, d, perpendicular(d)).map_err(ke)
      }
      PlaneDef::Perpendicular { base, through, distance, v } => {
        let bp = self.resolve_plane(base)?;
        // Fusion's XZ plane runs its U direction along -X; Fox's XZ runs X along +X.
        let u = if matches!(base, PlaneRef::Origin { plane: OriginPlane::Xz }) { -bp.x } else { bp.x };
        let n = if *v { bp.y } else { u };
        let o = match through {
          Some(p) => self.point_of(p)?,
          None => bp.origin + n * self.len(distance)?,
        };
        Plane::from_normal(o, n, bp.normal()).map_err(ke)
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
      AxisDef::SketchLine { sketch, curve } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?;
        let Some(CurveKind::Line { a, b }) = sk.sketch.curve(*curve).map(|c| &c.kind) else {
          return Err("the axis line no longer exists".into());
        };
        let (Some(pa), Some(pb)) = (sk.sketch.pos(*a), sk.sketch.pos(*b)) else {
          return Err("the axis line no longer exists".into());
        };
        let (pa, pb) = (sk.plane.to_world(pa), sk.plane.to_world(pb));
        Ok((pa, pb - pa))
      }
      AxisDef::Axis { feature } => {
        self.state.axes.get(feature).copied().ok_or_else(|| "its construction axis failed".to_string())
      }
    }
  }

  /// A line as a point on it and its direction, or `None` for a curved body edge.
  fn straight(&mut self, l: &mut LineRef) -> Result<Option<(DVec3, DVec3)>, String> {
    match l {
      LineRef::Edge(e) => {
        let (_, info) = self.edge_info(e)?;
        Ok((info.kind == fox_kernel::CurveKind::Line).then_some((info.start, info.end - info.start)))
      }
      _ => self.line_of(l).map(Some),
    }
  }

  fn point_by(&mut self, def: &mut PointDef) -> Result<DVec3, String> {
    match def {
      PointDef::Ref { point } => self.point_of(point),
      PointDef::TwoEdges { a, b } if self.behaviours.has(fox_sketch::behaviour::INFINITE_LINE_CROSSING) => {
        if let (Some(la), Some(lb)) = (self.straight(a)?, self.straight(b)?) {
          return lines_meet(la, lb);
        }
        // a curved edge: where the two edges come closest
        let (LineRef::Edge(a), LineRef::Edge(b)) = (a, b) else {
          return Err("pick straight edges or lines".into());
        };
        let (ba, ea) = self.edge_info(a)?;
        let (bb, eb) = self.edge_info(b)?;
        let sa = self.state.bodies[ba].shape.edge_shape(ea.id).map_err(ke)?;
        let sb = self.state.bodies[bb].shape.edge_shape(eb.id).map_err(ke)?;
        let (_, pa, pb) = sa.distance_to(&sb).map_err(ke)?;
        Ok((pa + pb) / 2.0)
      }
      // designs made before INFINITE_LINE_CROSSING: the midpoint of the closest approach
      PointDef::TwoEdges { a, b } => {
        if let (LineRef::Edge(a), LineRef::Edge(b)) = (&mut *a, &mut *b) {
          // two body edges: their closest approach as bounded curves (curved edges too)
          let (ba, ea) = self.edge_info(a)?;
          let (bb, eb) = self.edge_info(b)?;
          let sa = self.state.bodies[ba].shape.edge_shape(ea.id).map_err(ke)?;
          let sb = self.state.bodies[bb].shape.edge_shape(eb.id).map_err(ke)?;
          let (_, pa, pb) = sa.distance_to(&sb).map_err(ke)?;
          return Ok((pa + pb) / 2.0);
        }
        // an axis or sketch line: the closest approach of the two infinite lines
        let (oa, da) = self.line_of(a)?;
        let (ob, db) = self.line_of(b)?;
        let (da, db) = (da.normalize(), db.normalize());
        let w = oa - ob;
        let (k, d, e) = (da.dot(db), da.dot(w), db.dot(w));
        let den = 1.0 - k * k;
        if den < 1e-12 {
          return Err("the lines are parallel".into());
        }
        let (s, t) = ((k * e - d) / den, (e - k * d) / den);
        Ok((oa + da * s + ob + db * t) / 2.0)
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
      PointDef::LinePlane { line, plane } => {
        let pl = self.resolve_plane(plane)?;
        let n = pl.normal();
        let (o, d) = self.resolve_axis(line, None)?;
        let den = d.dot(n);
        if den.abs() < 1e-12 {
          return Err("the line is parallel to the plane".into());
        }
        Ok(o - d * ((o - pl.origin).dot(n) / den))
      }
      PointDef::AlongPath { edge, t, distance } => Ok(self.along(edge, t, distance)?.0),
      PointDef::AlongCurve { path, t, distance } => Ok(self.along_curve(path, t, distance)?.0),
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

/// Where two infinite lines (a point and a direction each) cross.
fn lines_meet((p1, d1): (DVec3, DVec3), (p2, d2): (DVec3, DVec3)) -> Result<DVec3, String> {
  let (d1, d2) = (
    d1.try_normalize().ok_or("the first line has no length")?,
    d2.try_normalize().ok_or("the second line has no length")?,
  );
  let n = d1.cross(d2);
  let n2 = n.length_squared();
  if n2 < 1e-18 {
    return Err("the lines are parallel".into());
  }
  let w = p2 - p1;
  let q1 = p1 + d1 * (w.cross(d2).dot(n) / n2);
  let q2 = p2 + d2 * (w.cross(d1).dot(n) / n2);
  if (q1 - q2).length() > 1e-4 {
    return Err("the lines don't meet".into());
  }
  Ok((q1 + q2) / 2.0)
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
    Loop::new((0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect())
  };
  // the inradius: a wall this thick leaves no hole
  let inner = match section {
    PipeSection::Circular | PipeSection::Square => size / 2.0,
    PipeSection::Triangular => size / 3.0,
  };
  // the loop with inradius `r`, counter-clockwise or (for a hole) clockwise
  let shape = |r: f64, ccw: bool| -> Loop {
    let l = match section {
      PipeSection::Circular => Loop::new(vec![Seg::Circle { center: [0.0, 0.0], radius: r, ccw: true }]),
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
    Seg::Quintic { p } => fox_sketch::quintic_at(&p, u),
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
    // path_head cuts a quintic spline's cubic pieces instead
    Seg::Quintic { .. } => *s,
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
      match *s {
        // a piece of a quintic spline is no longer one: take its cubic pieces instead
        Seg::Quintic { p } => {
          let pieces: Vec<Seg> = fox_sketch::quintic_beziers(&p).into_iter().map(|p| Seg::Bezier { p }).collect();
          out.extend(path_head(&pieces, left));
        }
        _ => out.push(seg_head(s, left)),
      }
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
    Seg::EllipseArc { .. } | Seg::Conic { .. } | Seg::Quintic { .. } => (s.start(), s.end_tangents().0),
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

/// A thread's helical groove: the ISO 68-1 profile, 60° flanks half a pitch apart at the pitch
/// diameter and a flat root, cut in from the crest diameter (inwards on a boss, outwards in a
/// hole) between `span.0` and `span.1` along the axis, where planes across the axis end it.
fn thread_groove(
  o: DVec3,
  d: DVec3,
  span: (f64, f64),
  dims: &crate::thread::ThreadDims,
  external: bool,
  left: bool,
) -> fox_kernel::Result<Shape> {
  let p = dims.pitch;
  let (rc, rp, rb) = (dims.crest / 2.0, dims.pitch_diameter / 2.0, dims.root / 2.0);
  let tan = 30f64.to_radians().tan();
  let out = if external { 1.0 } else { -1.0 };
  // half the groove's width at radius r: a quarter pitch at the pitch radius, wider towards the crest
  let half = |r: f64| p / 4.0 + out * (r - rp) * tan;
  let land = p - 2.0 * half(rc);
  if land <= 0.0 || (rb - rc) * out >= 0.0 || half(rb) <= 0.0 {
    return Err(fox_kernel::KernelError::Input("the thread profile does not fit the pitch".into()));
  }
  // the flanks run a little past the crest so the cut is clean, staying clear of the next turn
  let past = rc + out * 0.25 * land / (2.0 * tan);
  let x = perpendicular(d);
  let start = o + d * (span.0 - p);
  let sec = Plane { origin: start + x * rp, x, y: d };
  let pts = [[rb - rp, -half(rb)], [past - rp, -half(past)], [past - rp, half(past)], [rb - rp, half(rb)]];
  let outer = Loop::new((0..4).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % 4] }).collect());
  let face = Shape::face(&sec, &Profile { outer, holes: vec![] })?;
  // a pitch longer at both ends, then cut square at the span's ends
  let sweep = face.helix_sweep(start, d, x, rp, p, span.1 - span.0 + 2.0 * p, left)?;
  let slab = Shape::cylinder(o + d * span.0, d, rc.max(rb) + p, span.1 - span.0)?;
  sweep.boolean(&slab, BoolOp::Intersect)
}

/// A thread's helical V groove as designs without [`ISO_THREADS`] build it: ISO 60° flanks, cut
/// into the material from the cylinder of radius `r` (inwards on a boss, outwards in a hole).
#[allow(clippy::too_many_arguments)]
fn legacy_thread_groove(
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
    src: Vec::new(),
  };
  let face = Shape::face(&sec, &Profile { outer, holes: vec![] })?;
  face.helix_sweep(start, dir, x, r, pitch, length, left)
}

/// The axis direction a thread's low end is measured along: its largest component positive.
fn canonical_axis(d: DVec3) -> DVec3 {
  let a = d.abs();
  let c = if a.x >= a.y && a.x >= a.z {
    d.x
  } else if a.y >= a.z {
    d.y
  } else {
    d.z
  };
  if c < 0.0 { -d } else { d }
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

  #[test]
  fn lines_meet_where_extended_and_not_when_skew_or_parallel() {
    // the L-prism's top edges: y = 4 (x 8..20) and x = 0 (y 0..10) meet extended at (0, 4, 5)
    let a = (DVec3::new(20.0, 4.0, 5.0), DVec3::new(-12.0, 0.0, 0.0));
    let b = (DVec3::new(0.0, 0.0, 5.0), DVec3::new(0.0, 10.0, 0.0));
    assert!((lines_meet(a, b).unwrap() - DVec3::new(0.0, 4.0, 5.0)).length() < 1e-9);
    let skew = (DVec3::new(0.0, 0.0, 6.0), DVec3::Y);
    assert!(lines_meet(a, skew).is_err());
    assert!(lines_meet(a, (DVec3::ZERO, DVec3::X)).is_err());
  }

  #[test]
  fn an_old_two_edges_point_reads_as_body_edges() {
    let e = EdgeRef {
      body: crate::BodyId(2000),
      point: [10.0, 0.0, 5.0],
      dir: [1.0, 0.0, 0.0],
      n1: [0.0, 0.0, 1.0],
      n2: [0.0, -1.0, 0.0],
      circular: false,
      radius: 0.0,
      start: [0.0; 3],
      end: [0.0; 3],
    };
    let old = serde_json::json!({"type": "two_edges", "a": e, "b": e});
    let def: PointDef = serde_json::from_value(old).unwrap();
    assert_eq!(def, PointDef::TwoEdges { a: LineRef::Edge(e.clone()), b: LineRef::Edge(e) });
    let line = serde_json::json!({"type": "two_edges", "a": {"sketch": 3, "curve": 3}, "b": {"type": "construction", "feature": 5}});
    let def: PointDef = serde_json::from_value(line).unwrap();
    assert_eq!(def.depends_on(), vec![FeatureId(3), FeatureId(5)]);
  }
}
