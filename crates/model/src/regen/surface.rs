//! Regeneration of the surface tools added with the Surface workspace's second pass: extend,
//! untrim, trim with a body or sketch, and offsets with a distance per face.

use clf_kernel::glam::DVec3;
use clf_kernel::{Plane, Shape};
use clf_sketch::{Profile, Seg};

use super::{Ctx, FResult, ke};
use crate::surface::{FaceDistance, TrimTool};
use crate::{BodyId, FaceRef, FeatureId, FeatureKind};

/// Which side of a trimming tool a point of the trimmed surface is on: > 0 is cut away by
/// default, < 0 kept.
enum Side {
  /// A body: inside a solid, or in front of a surface.
  Body(Shape),
  /// Inside the sketch's profiles (sketch plane coordinates).
  Profiles(Plane, Vec<Profile>),
  /// Left of the sketch curves (polylines in sketch plane coordinates).
  Curves(Plane, Vec<Vec<[f64; 2]>>),
}

impl Side {
  fn of(&self, p: DVec3) -> Result<f64, String> {
    match self {
      Side::Body(tool) => tool.trim_side(p).map_err(ke),
      Side::Profiles(plane, profiles) => {
        let q = plane.to_local(p);
        Ok(if profiles.iter().any(|pr| pr.contains(q)) { 1.0 } else { -1.0 })
      }
      Side::Curves(plane, lines) => {
        let q = plane.to_local(p);
        let mut best = (f64::INFINITY, 0.0);
        for line in lines {
          for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let len2 = ab[0] * ab[0] + ab[1] * ab[1];
            if len2 <= 0.0 {
              continue;
            }
            let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
            let c = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            let d = (q[0] - c[0]).hypot(q[1] - c[1]);
            if d < best.0 {
              best = (d, ab[0] * (q[1] - a[1]) - ab[1] * (q[0] - a[0]));
            }
          }
        }
        Ok(best.1)
      }
    }
  }
}

impl Ctx<'_> {
  /// Surface features of the second pass (see the module docs).
  pub(super) fn apply_surface(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::SurfaceExtend { edges, distance, mode } => {
        let d = self.len(distance)?;
        if !(d > 0.0) {
          return Err("the distance must be positive".into());
        }
        if edges.is_empty() {
          return Err("select the surface edges to extend".into());
        }
        let mut by_body: Vec<(usize, Vec<u32>)> = Vec::new();
        for e in edges.iter_mut() {
          let (bi, info) = self.edge_info(e)?;
          if info.faces[1] != 0 {
            return Err("only open (boundary) edges of a surface can be extended".into());
          }
          match by_body.iter_mut().find(|(b, _)| *b == bi) {
            Some((_, v)) => v.push(info.id),
            None => by_body.push((bi, vec![info.id])),
          }
        }
        for (bi, ids) in by_body {
          let shape = self.state.bodies[bi].shape.extend_edges(&ids, d, mode.kernel()).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(None)
      }
      FeatureKind::Untrim { faces, bodies, loops } => {
        let mut by_body = self.face_ids(faces)?;
        for b in bodies.iter() {
          let bi = self.body_index(*b)?;
          let all: Vec<u32> = self.state.bodies[bi].faces().iter().map(|f| f.id).collect();
          match by_body.iter_mut().find(|(x, _)| *x == bi) {
            Some((_, v)) => *v = all,
            None => by_body.push((bi, all)),
          }
        }
        if by_body.is_empty() {
          return Err("select the faces or surface bodies to untrim".into());
        }
        let mut changed = false;
        for (bi, ids) in by_body {
          let body = &self.state.bodies[bi];
          let shape = body.shape.untrim(&ids, loops.kernel()).map_err(ke)?;
          let shape = if ids.len() < body.faces().len() { shape.sewn(1e-6).map_err(ke)? } else { shape };
          let area = |s: &Shape| s.mass().map(|m| m.area).unwrap_or(0.0);
          changed |= (area(&shape) - area(&body.shape)).abs() > 1e-9;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok((!changed).then(|| "the faces have no trims to remove".to_string()))
      }
      FeatureKind::TrimWith { bodies, tool, flip, keep } => self.trim_with(id, bodies, tool, *flip, keep),
      FeatureKind::SurfaceOffset { faces, bodies, distance, face_distances } => {
        self.surface_offset(id, faces, bodies, distance, face_distances)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }

  /// Face references grouped by body index, as face ids.
  fn face_ids(&mut self, faces: &mut [FaceRef]) -> Result<Vec<(usize, Vec<u32>)>, String> {
    let mut out: Vec<(usize, Vec<u32>)> = Vec::new();
    for fr in faces.iter_mut() {
      let (bi, info) = self.face_info(fr)?;
      match out.iter_mut().find(|(b, _)| *b == bi) {
        Some((_, v)) if !v.contains(&info.id) => v.push(info.id),
        Some(_) => {}
        None => out.push((bi, vec![info.id])),
      }
    }
    Ok(out)
  }

  fn trim_with(
    &mut self,
    id: FeatureId,
    bodies: &[BodyId],
    tool: &mut TrimTool,
    flip: bool,
    keep: &[[f64; 3]],
  ) -> FResult {
    if bodies.is_empty() {
      return Err("select the surface bodies to trim".into());
    }
    let (cutter, side) = match tool {
      TrimTool::Body { body } => {
        if bodies.contains(body) {
          return Err("a body can't trim itself".into());
        }
        let shape = self.state.bodies[self.body_index(*body)?].shape.clone();
        (shape.clone(), Side::Body(shape))
      }
      TrimTool::Sketch { sketch, curves } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (wires, side) = if curves.is_empty() && !sk.profiles.is_empty() {
          let mut wires = Vec::new();
          for pr in &sk.profiles {
            for l in std::iter::once(&pr.outer).chain(&pr.holes) {
              wires.push(Shape::path(&sk.plane, &l.segs).map_err(ke)?);
            }
          }
          (wires, Side::Profiles(sk.plane, sk.profiles.clone()))
        } else {
          let mut lines = Vec::new();
          for c in &sk.sketch.curves {
            if !(curves.is_empty() && !c.construction || curves.contains(&c.id)) {
              continue;
            }
            for seg in sk.sketch.curve_segs(c.id) {
              lines.push(seg_points(&seg));
            }
          }
          (super::extra::sketch_wires(&sk.plane, &sk.sketch, curves)?, Side::Curves(sk.plane, lines))
        };
        if wires.is_empty() {
          return Err("the sketch has no curves to trim with".into());
        }
        // the curves projected along the sketch normal: a curtain through everything
        let n = sk.plane.normal();
        let size = self.through_all(&sk.plane);
        let mut curtain = Vec::new();
        for w in wires {
          curtain.push(w.translated(-n * size).map_err(ke)?.extrude(n * 2.0 * size).map_err(ke)?);
        }
        (Shape::compound(curtain.iter()).map_err(ke)?, side)
      }
    };
    // split every target, then decide face by face
    let mut pieces: Vec<(usize, Shape, Vec<(u32, Option<DVec3>)>)> = Vec::new();
    for b in bodies {
      let bi = self.body_index(*b)?;
      let split = self.state.bodies[bi].shape.split_raw(&cutter).map_err(ke)?;
      let pts = face_points(&split)?;
      pieces.push((bi, split, pts));
    }
    let mut chosen: Vec<Vec<bool>> = pieces.iter().map(|p| vec![false; p.2.len()]).collect();
    if keep.is_empty() {
      for (k, (_, _, pts)) in pieces.iter().enumerate() {
        for (j, (_, p)) in pts.iter().enumerate() {
          if let Some(p) = p {
            let s = side.of(*p)?;
            chosen[k][j] = if flip { s > 0.0 } else { s <= 0.0 };
          }
        }
      }
    } else {
      // the piece nearest each picked point stays
      for at in keep {
        let v = Shape::vertex(DVec3::from(*at)).map_err(ke)?;
        let mut best: Option<(f64, usize, usize)> = None;
        for (k, (_, split, pts)) in pieces.iter().enumerate() {
          for (j, (fid, _)) in pts.iter().enumerate() {
            let d = split.face_shape(*fid).map_err(ke)?.distance_to(&v).map_err(ke)?.0;
            if best.is_none_or(|b| d < b.0) {
              best = Some((d, k, j));
            }
          }
        }
        if let Some((_, k, j)) = best {
          chosen[k][j] = true;
        }
      }
    }
    let mut removed = false;
    for (k, (bi, split, pts)) in pieces.into_iter().enumerate() {
      let mut kept = Vec::new();
      for (j, (fid, _)) in pts.iter().enumerate() {
        if chosen[k][j] {
          kept.push(split.face_shape(*fid).map_err(ke)?);
        } else {
          removed = true;
        }
      }
      if kept.is_empty() {
        return Err("nothing of the surface is left (try Flip)".into());
      }
      let skin = Shape::compound(kept.iter()).map_err(ke)?.sewn(1e-6).map_err(ke)?;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(skin, id);
    }
    Ok((!removed).then(|| "the tool doesn't cut the surface".to_string()))
  }

  fn surface_offset(
    &mut self,
    id: FeatureId,
    faces: &mut [FaceRef],
    bodies: &[BodyId],
    distance: &str,
    face_distances: &mut [FaceDistance],
  ) -> FResult {
    let d = self.len(distance)?;
    // per-face distances, by body index and face id
    let mut own: Vec<(usize, u32, f64)> = Vec::new();
    for fd in face_distances.iter_mut() {
      let dist = self.len(&fd.distance)?;
      let (bi, info) = self.face_info(&mut fd.face)?;
      own.push((bi, info.id, dist));
    }
    let dist_of = |bi: usize, f: u32| own.iter().find(|o| o.0 == bi && o.1 == f).map_or(d, |o| o.2);
    let mut skins = Vec::new();
    let mut picked: Vec<(usize, u32)> = Vec::new();
    for (bi, fids) in self.face_ids(faces)? {
      for f in fids {
        skins.push((self.state.bodies[bi].shape.face_shape(f).map_err(ke)?, dist_of(bi, f), Vec::new()));
        picked.push((bi, f));
      }
    }
    let mut whole = Vec::new();
    for b in bodies.iter() {
      let bi = self.body_index(*b)?;
      whole.push(bi);
      let overrides: Vec<(u32, f64)> = own.iter().filter(|o| o.0 == bi).map(|o| (o.1, o.2)).collect();
      skins.push((self.state.bodies[bi].shape.clone(), d, overrides));
    }
    // faces given only a distance of their own are offset like picked faces
    for &(bi, f, dist) in &own {
      if !whole.contains(&bi) && !picked.contains(&(bi, f)) {
        skins.push((self.state.bodies[bi].shape.face_shape(f).map_err(ke)?, dist, Vec::new()));
      }
    }
    if skins.is_empty() {
      return Err("select faces or bodies to offset".into());
    }
    for (skin, dist, overrides) in skins {
      let off = if overrides.is_empty() {
        skin.offset_surface(dist).map_err(ke)?
      } else {
        skin.offset_variable(dist, &overrides).map_err(ke)?
      };
      self.new_body(id, off);
    }
    Ok(None)
  }
}

/// A segment as a polyline, end point included.
fn seg_points(seg: &Seg) -> Vec<[f64; 2]> {
  let mut pts = Vec::new();
  seg.flatten(&mut pts);
  pts.push(match seg {
    Seg::Circle { .. } => seg.start(),
    _ => seg.end(),
  });
  pts
}

/// Every face of a shape with a point well inside it (the centre of its largest display
/// triangle), or `None` for faces too small to tessellate.
fn face_points(shape: &Shape) -> Result<Vec<(u32, Option<DVec3>)>, String> {
  let diag = shape.mass().map(|m| (m.bbox_max - m.bbox_min).length()).unwrap_or(1.0);
  let mesh = shape.tessellate((diag * 0.0015).clamp(0.005, 0.5)).map_err(ke)?;
  let mut best: std::collections::HashMap<u32, (f64, DVec3)> = std::collections::HashMap::new();
  for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
    let Some(&face) = mesh.tri_face.get(t) else { continue };
    let p: Vec<DVec3> = tri
      .iter()
      .filter_map(|&i| mesh.positions.get(i as usize))
      .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
      .collect();
    if p.len() != 3 {
      continue;
    }
    let area = (p[1] - p[0]).cross(p[2] - p[0]).length();
    if best.get(&face).is_none_or(|b| area > b.0) {
      best.insert(face, (area, (p[0] + p[1] + p[2]) / 3.0));
    }
  }
  Ok(shape.faces().map_err(ke)?.iter().map(|f| (f.id, best.get(&f.id).map(|b| b.1))).collect())
}

#[cfg(test)]
mod tests {
  use super::super::regenerate;
  use crate::surface::{ExtendMode, TrimTool, UntrimLoops};
  use crate::*;
  use clf_kernel::glam::DVec3;
  use clf_sketch::Sketch;

  const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };

  fn area(m: &regen::Model) -> f64 {
    m.bodies[0].shape.mass().unwrap().area
  }

  #[test]
  fn extend_untrim_and_trim_regenerate() {
    let mut d = Design::new("sheet");
    d.add_parameter("ext", "5 mm", ParamKind::Length, "").unwrap();
    let mut sk = Sketch::new();
    sk.add_rectangle([0.0, 0.0], [10.0, 10.0]).unwrap();
    let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: sk, links: Vec::new() }).unwrap();
    d.add_feature(FeatureKind::Patch { edges: vec![], sketch: Some(s) }).unwrap();
    let m = regenerate(&mut d);
    let b = &m.bodies[0];
    let e = b.edges().iter().find(|e| (e.mid - DVec3::new(10.0, 5.0, 0.0)).length() < 1e-6).unwrap();
    let edge = EdgeRef::from_edge(b.id, e, b.faces());
    let ext = d
      .add_feature(FeatureKind::SurfaceExtend { edges: vec![edge], distance: "ext".into(), mode: ExtendMode::Natural })
      .unwrap();
    let m = regenerate(&mut d);
    assert!(m.errors().is_empty(), "{:?}", m.errors());
    assert!((area(&m) - 150.0).abs() < 1e-6, "{}", area(&m));
    // the distance follows its parameter
    d.set_parameter("ext", "8 mm").unwrap();
    let m = regenerate(&mut d);
    assert!((area(&m) - 180.0).abs() < 1e-6, "{}", area(&m));
    assert_eq!(d.feature(ext).unwrap().name, "Extend1");

    // a circle trimmed out of it, then untrimmed
    let mut c = Sketch::new();
    c.add_circle(clf_sketch::PtSpec::At(5.0, 5.0), 2.0).unwrap();
    let cs = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: c, links: Vec::new() }).unwrap();
    let body = m.bodies[0].id;
    let tw = d
      .add_feature(FeatureKind::TrimWith {
        bodies: vec![body],
        tool: TrimTool::Sketch { sketch: cs, curves: vec![] },
        flip: false,
        keep: vec![],
      })
      .unwrap();
    assert_eq!(d.feature(tw).map(|f| f.kind.depends_on()), Some(vec![cs]));
    let m = regenerate(&mut d);
    assert!(m.errors().is_empty(), "{:?}", m.errors());
    assert!((area(&m) - (180.0 - 4.0 * std::f64::consts::PI)).abs() < 1e-6, "{}", area(&m));
    d.add_feature(FeatureKind::Untrim { faces: vec![], bodies: vec![body], loops: UntrimLoops::Inner }).unwrap();
    let m = regenerate(&mut d);
    assert!(m.errors().is_empty(), "{:?}", m.errors());
    assert!((area(&m) - 180.0).abs() < 1e-6, "{}", area(&m));

    // the new features survive a save and load
    let json = serde_json::to_string(&d).unwrap();
    let back: Design = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
  }

  #[test]
  fn older_files_still_open() {
    // an offset saved before per-face distances, and extend/untrim with their defaults
    let old = r#"{"type": "surface_offset", "faces": [], "bodies": [3], "distance": "2 mm"}"#;
    let k: FeatureKind = serde_json::from_str(old).unwrap();
    assert!(matches!(k, FeatureKind::SurfaceOffset { ref face_distances, .. } if face_distances.is_empty()));
    assert!(!serde_json::to_string(&k).unwrap().contains("face_distances"));
    let u: FeatureKind = serde_json::from_str(r#"{"type": "untrim", "bodies": [3]}"#).unwrap();
    assert!(matches!(u, FeatureKind::Untrim { loops: UntrimLoops::All, .. }));
    let t: FeatureKind = serde_json::from_str(r#"{"type": "trim_with", "bodies": [3], "tool": {"body": 4}}"#).unwrap();
    assert!(matches!(t, FeatureKind::TrimWith { tool: TrimTool::Body { body: BodyId(4) }, flip: false, .. }));
  }
}
