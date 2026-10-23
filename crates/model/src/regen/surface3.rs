//! Regeneration of the SURFACE tab's Create commands as Fusion's dialogs build them: Extrude and
//! Revolve of picked profiles as well as curves (with Start, Direction and two sides), Patch with
//! a continuity per boundary edge, Ruled grown from edges, and the surface Sweep and Loft.

use fox_kernel::Shape;
use fox_sketch::CurveId;

use super::extra::sketch_wires;
use super::{Ctx, FResult, SketchResult, ke, pick_profiles};
use crate::surface::{Continuity, RuledType};
use crate::{FeatureId, FeatureKind, PathRef};

/// The curves a surface is made from: each picked profile's boundary loops as wires, and the
/// picked curves one wire per segment (no profiles and no curves: every curve of the sketch).
fn source_wires(sk: &SketchResult, profiles: &[[f64; 2]], curves: &[CurveId]) -> Result<Vec<Shape>, String> {
  let mut out = Vec::new();
  if !profiles.is_empty() {
    let (chosen, _) = pick_profiles(sk, profiles);
    if chosen.is_empty() {
      return Err("the selected profiles no longer exist".into());
    }
    for p in &chosen {
      for l in std::iter::once(&p.outer).chain(&p.holes) {
        out.push(Shape::path(&sk.plane, &l.segs).map_err(ke)?);
      }
    }
    if curves.is_empty() {
      return Ok(out);
    }
  }
  out.extend(sketch_wires(&sk.plane, &sk.sketch, curves)?);
  Ok(out)
}

impl Ctx<'_> {
  /// Surface Extrude, Surface Revolve and Patch (also in [`Ctx::apply_extra`] for designs
  /// before these rows) and the Create commands added with them.
  pub(super) fn apply_surface3(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::SurfaceExtrude { sketch, curves, distance, symmetric, flip, profiles, offset, distance2 } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let d = self.len(distance)?;
        let lift = if offset.trim().is_empty() { 0.0 } else { self.len(offset)? };
        let n = sk.plane.normal() * if *flip { -1.0 } else { 1.0 };
        let (start, length) = match distance2 {
          Some(d2) => {
            let d2 = self.len(d2)?;
            (-d2, d + d2)
          }
          None if *symmetric => (-d, 2.0 * d),
          None => (0.0, d),
        };
        if length.abs() < 1e-9 {
          return Err("the distance must not be zero".into());
        }
        let shift = sk.plane.normal() * lift + n * start;
        let mut faces = Vec::new();
        for w in source_wires(&sk, profiles, curves)? {
          let w = if shift.length() > 0.0 { w.translated(shift).map_err(ke)? } else { w };
          faces.push(w.extrude(n * length).map_err(ke)?);
        }
        self.new_surface(id, faces)?;
        Ok(None)
      }
      FeatureKind::SurfaceRevolve { sketch, curves, axis, angle, profiles, symmetric, angle2 } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (o, d) = self.resolve_axis(axis, Some(*sketch))?;
        let a = self.angle(angle)?;
        let (back, total) = match angle2 {
          _ if *symmetric => (a, 2.0 * a),
          Some(a2) => {
            let a2 = self.angle(a2)?;
            (a2, a + a2)
          }
          None => (0.0, a),
        };
        let mut faces = Vec::new();
        for w in source_wires(&sk, profiles, curves)? {
          let w = if back.abs() > 1e-12 { w.rotated(o, d, -back).map_err(ke)? } else { w };
          faces.push(w.revolve(o, d, total).map_err(ke)?);
        }
        self.new_surface(id, faces)?;
        Ok(None)
      }
      FeatureKind::Patch { edges, sketch, continuity, flip } => {
        let mut parts = Vec::new();
        if let Some(sk) = sketch {
          let sk = self.state.sketches.get(sk).ok_or("its sketch failed")?.clone();
          parts.extend(sketch_wires(&sk.plane, &sk.sketch, &[])?.into_iter().map(|w| (w, Continuity::Connected, None)));
        }
        for (i, e) in edges.iter_mut().enumerate() {
          let (bi, info) = self.edge_info(e)?;
          let shape = &self.state.bodies[bi].shape;
          let c = continuity.get(i).copied().unwrap_or_default();
          let face = match info.faces.iter().copied().find(|f| *f != 0) {
            Some(f) if c != Continuity::Connected => Some(shape.face_shape(f).map_err(ke)?),
            _ => None,
          };
          parts.push((shape.edge_shape(info.id).map_err(ke)?, c, face));
        }
        if parts.is_empty() {
          return Err("pick the boundary edges or a sketch".into());
        }
        let face = if parts.iter().all(|p| p.1 == Continuity::Connected) {
          Shape::compound(parts.iter().map(|p| &p.0)).map_err(ke)?.patch().map_err(ke)?
        } else {
          let list: Vec<_> = parts.into_iter().map(|(e, c, f)| (e, c.kernel(), f)).collect();
          Shape::patch_with(&list, &[]).map_err(ke)?
        };
        let face = if *flip { face.reversed().map_err(ke)? } else { face };
        self.new_body(id, face);
        Ok(None)
      }
      FeatureKind::RuledSurface { edges, distance, angle, along } => {
        let d = self.len(distance)?;
        let a = self.angle(angle)?;
        if edges.is_empty() {
          return Err("pick the edges to rule from".into());
        }
        let mut faces = Vec::new();
        for e in edges.iter_mut() {
          let (bi, info) = self.edge_info(e)?;
          let shape = &self.state.bodies[bi].shape;
          let f = info.faces.iter().copied().find(|f| *f != 0).ok_or("an edge has no face beside it")?;
          let (edge, face) = (shape.edge_shape(info.id).map_err(ke)?, shape.face_shape(f).map_err(ke)?);
          faces.push(edge.ruled_from_edge(&face, d, a, *along == RuledType::Tangent).map_err(ke)?);
        }
        self.new_surface(id, faces)?;
        Ok(None)
      }
      FeatureKind::SurfaceSweep { sketch, profiles, curves, path_sketch, path_curves, parallel } => {
        let sk = self.state.sketches.get(sketch).ok_or("the profile sketch failed")?.clone();
        let ps = self.state.sketches.get(path_sketch).ok_or("the path sketch failed")?;
        let path = crate::include::sketch_path(ps, path_curves)?;
        let wires = Shape::chain_wires(&source_wires(&sk, profiles, curves)?, 1e-6).map_err(ke)?;
        let mut faces = Vec::new();
        for w in &wires {
          faces.push(w.sweep_open(&path, *parallel).map_err(ke)?);
        }
        self.new_surface(id, faces)?;
        Ok(None)
      }
      FeatureKind::SurfaceLoft { sections, closed, rails, centerline } => {
        let mut wires = Vec::new();
        for (i, sec) in sections.iter().enumerate() {
          let sk = self.state.sketches.get(&sec.sketch).ok_or(format!("the sketch of section {} failed", i + 1))?;
          let chain = match sec.profile {
            Some(p) => source_wires(sk, &[p], &[])?.into_iter().take(1).collect(),
            None => Shape::chain_wires(&sketch_wires(&sk.plane, &sk.sketch, &sec.curves)?, 1e-6).map_err(ke)?,
          };
          match <[Shape; 1]>::try_from(chain) {
            Ok([w]) => wires.push(w),
            Err(_) => return Err(format!("section {} must be one chain of curves", i + 1)),
          }
        }
        let path = |r: &PathRef| {
          let ps = self.state.sketches.get(&r.sketch).ok_or("a rail or centre line sketch failed")?;
          crate::include::sketch_path(ps, &r.curves)
        };
        let skin = match (rails.is_empty(), centerline) {
          (true, None) => Shape::loft_open(&wires, *closed).map_err(ke)?,
          _ if *closed => return Err("a closed loft can't follow rails or a centre line".into()),
          (false, Some(_)) => return Err("a loft follows rails or a centre line, not both".into()),
          (true, Some(c)) => Shape::loft_open_centerline(&wires, &path(c)?).map_err(ke)?,
          (false, None) => {
            let rs = rails.iter().map(path).collect::<Result<Vec<_>, _>>()?;
            Shape::loft_open_rails(&wires, &rs).map_err(ke)?
          }
        };
        self.new_body(id, skin);
        Ok(None)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }
}
