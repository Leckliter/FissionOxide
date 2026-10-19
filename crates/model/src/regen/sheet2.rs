//! Regeneration of lofted and miter flanges (see [`crate::sheet2`]).

use fox_kernel::glam::DVec3;

use super::{Ctx, FResult};
use crate::sheet::{self, SheetInfo};
use crate::sheet2::{self, Chain};
use crate::{FeatureId, FeatureKind, LoftProfile, Operation};

impl Ctx<'_> {
  pub(super) fn apply_sheet2(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::LoftedFlange { from, to, method, bends, flip, rule, rounded } => {
        let info = self.sheet_info(rule)?;
        let (a, b) = (self.loft_chain(from)?, self.loft_chain(to)?);
        let n = self.count(bends)?;
        let (shape, mut dev, warning) = sheet2::lofted_flange(&a, &b, *method, n, *flip, *rounded, &info)?;
        self.operate(id, shape, Operation::NewBody)?;
        for b in self.state.bodies.iter_mut().filter(|b| b.feature == id && b.sheet.is_none()) {
          // the flat pattern holds while the body keeps this volume; later cuts are what's
          // missing from the body as built
          dev.volume = b.shape.mass().map(|m| m.volume).unwrap_or(dev.volume);
          dev.built = Some(std::sync::Arc::new(std::sync::Mutex::new(b.shape.clone())));
          b.sheet = Some(SheetInfo { developed: Some(std::sync::Arc::new(dev.clone())), ..info.clone() });
        }
        Ok(warning)
      }
      FeatureKind::MiterFlange { sketch, curves, edges, corners } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let path = sheet::open_path(&sk.sketch, curves)?;
        // the first edge's flange plane: x out of the sheet edge, y the sheet face's normal
        let first = edges.first().ok_or("select sheet edges")?.clone();
        let bi = self.body_index(first.body)?;
        let body = &self.state.bodies[bi];
        let info = body.sheet.clone().ok_or("flanges go on sheet metal bodies")?;
        let (t, r) = (info.thickness, info.bend_radius);
        let eid =
          first.resolve(body.edges(), body.faces(), body.diagonal()).ok_or("the first edge could not be found")?;
        let e = body.edges().iter().find(|e| e.id == eid).copied().ok_or("the first edge could not be found")?;
        let (panel, wall) = sheet::edge_faces(body.faces(), e.faces, e.length, t)
          .ok_or("pick an edge where a face of the sheet meets a square side")?;
        let dir = (e.end - e.start).normalize_or_zero();
        if sk.plane.normal().dot(dir).abs() < 0.999 {
          return Err("draw the miter flange profile on a plane across the first edge".into());
        }
        let (w, up) = (wall.normal, panel.normal);
        let to_flange = |p: [f64; 2]| {
          let v: DVec3 = sk.plane.to_world(p) - e.start;
          [v.dot(w), v.dot(up)]
        };
        let mirror = sk.plane.normal().dot(w.cross(up)) < 0.0;
        let local: Vec<_> = path.iter().map(|s| sheet2::map_seg(s, &to_flange, mirror)).collect();
        let (profile, turn, cut) = sheet2::miter_section(&local, t, r)?;
        let top = sheet2::bend_top(r, t, turn);
        self.edge_flanges(id, edges, false, (0.0, 0.0), *corners, |_| Ok((profile.clone(), -cut, top)))
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }

  /// A lofted flange profile: the sketch's largest closed profile, or a chain of its curves.
  fn loft_chain(&self, p: &LoftProfile) -> Result<Chain, String> {
    let sk = self.state.sketches.get(&p.sketch).ok_or("a profile's sketch failed")?;
    if p.curves.is_empty()
      && let Some(prof) = sk.profiles.iter().max_by(|a, b| a.area().total_cmp(&b.area()))
    {
      return Ok(Chain { segs: prof.outer.segs.clone(), closed: true, plane: sk.plane });
    }
    Ok(Chain { segs: sheet::open_path(&sk.sketch, &p.curves)?, closed: false, plane: sk.plane })
  }
}
