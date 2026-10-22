//! Flat patterns that follow the model: a sketch keeps which sheet metal body to flatten (and
//! where to put the flat) and owns the fixed curves of the flat's outline and bend lines; fox-model
//! works the flat pattern out at every regeneration and hands it over with
//! [`Sketch::set_flat_outline`], so the curves follow the body as it changes (as spun profiles do,
//! see [`crate::SketchSpun`]).

use serde::{Deserialize, Serialize};

use crate::{CurveId, PointId, Sketch, SketchError, SpunSeg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FlatId(pub u32);

/// A flat pattern kept in a sketch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SketchFlat {
  pub id: FlatId,
  /// The model's id of the sheet metal body.
  pub body: u32,
  /// On the sketch plane with the flat's lower-left corner at the origin (else where the flat
  /// lies: the sketch is on its stationary face).
  #[serde(default)]
  pub at_origin: bool,
  /// The stationary face of a flat placed at the origin, as a point on it and its outward normal
  /// when it was picked (`None`: the body's default face). A flat on its stationary face follows
  /// the face the sketch is on.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub face: Option<([f64; 3], [f64; 3])>,
  /// The outline (and bend lines, as centre lines) last generated, in sketch coordinates.
  #[serde(default)]
  pub outline: Vec<SpunSeg>,
  /// The curves and points the outline made (all fixed).
  #[serde(default)]
  pub curves: Vec<CurveId>,
  #[serde(default)]
  pub points: Vec<PointId>,
}

impl Sketch {
  pub fn flat(&self, id: FlatId) -> Option<&SketchFlat> {
    self.flats.iter().find(|s| s.id == id)
  }

  /// The flat pattern a curve belongs to.
  pub fn flat_of(&self, curve: CurveId) -> Option<FlatId> {
    self.flats.iter().find(|s| s.curves.contains(&curve)).map(|s| s.id)
  }

  /// Adds a flat pattern of sheet metal body `body`; it has no curves until the model gives it an
  /// outline.
  pub fn add_flat(&mut self, body: u32, at_origin: bool, face: Option<([f64; 3], [f64; 3])>) -> FlatId {
    let id = FlatId(self.alloc());
    self.flats.push(SketchFlat {
      id,
      body,
      at_origin,
      face,
      outline: Vec::new(),
      curves: Vec::new(),
      points: Vec::new(),
    });
    id
  }

  /// Replaces a flat pattern's curves with `outline`. Returns whether anything changed. An
  /// outline within 1e-9 of the current one keeps the curves; one of the same pieces (only moved
  /// or resized) keeps them too, moving their points, so constraints and dimensions on them stay.
  pub fn set_flat_outline(&mut self, id: FlatId, outline: Vec<SpunSeg>) -> Result<bool, SketchError> {
    let s = self.flat(id).cloned().ok_or(SketchError::InvalidValue(format!("no flat pattern {}", id.0)))?;
    if s.outline.len() == outline.len() && s.outline.iter().zip(&outline).all(|(a, b)| a.close(b, 1e-9)) {
      return Ok(false);
    }
    if !self.move_flat(&s, &outline) {
      self.clear_flat(&s);
      let (curves, points) = self.place_outline(&outline)?;
      if let Some(f) = self.flats.iter_mut().find(|f| f.id == id) {
        f.curves = curves;
        f.points = points;
      }
    }
    if let Some(f) = self.flats.iter_mut().find(|f| f.id == id) {
      f.outline = outline;
    }
    Ok(true)
  }

  /// Moves the points of a flat's curves onto a new outline of the same pieces. Returns false
  /// (changing nothing) when the pieces differ.
  fn move_flat(&mut self, s: &SketchFlat, outline: &[SpunSeg]) -> bool {
    // a fresh copy of the new outline, to read its curves' points from
    let mut fresh = Sketch::new();
    let Ok((curves, _)) = fresh.place_outline(outline) else { return false };
    if curves.len() != s.curves.len() {
      return false;
    }
    let mut moves: Vec<(PointId, [f64; 2])> = Vec::new();
    let mut radii: Vec<(CurveId, f64)> = Vec::new();
    for (old, new) in s.curves.iter().zip(&curves) {
      let (Some(a), Some(b)) = (self.curve(*old), fresh.curve(*new)) else { return false };
      if std::mem::discriminant(&a.kind) != std::mem::discriminant(&b.kind)
        || a.construction != b.construction
        || a.centerline != b.centerline
      {
        return false;
      }
      let (pa, pb) = (crate::curve_points(&a.kind), crate::curve_points(&b.kind));
      if pa.len() != pb.len() {
        return false;
      }
      for (p, q) in pa.iter().zip(&pb) {
        let Some(to) = fresh.pos(*q) else { return false };
        match moves.iter().find(|(m, _)| m == p) {
          // a point two curves share must land in one place
          Some((_, at)) if (at[0] - to[0]).hypot(at[1] - to[1]) > 1e-7 => return false,
          Some(_) => {}
          None => moves.push((*p, to)),
        }
      }
      if let Some(r) = fresh.radius(*new) {
        radii.push((*old, r));
      }
    }
    for (p, to) in moves {
      if let Some(q) = self.point_mut(p) {
        q.x = to[0];
        q.y = to[1];
      }
    }
    for (c, r) in radii {
      if let Some(crate::Curve { kind: crate::CurveKind::Circle { radius, .. }, .. }) = self.curve_mut(c) {
        *radius = r;
      }
    }
    true
  }

  /// Deletes a flat pattern and its curves.
  pub fn remove_flat(&mut self, id: FlatId) -> Result<(), SketchError> {
    let i = self
      .flats
      .iter()
      .position(|s| s.id == id)
      .ok_or(SketchError::InvalidValue(format!("no flat pattern {}", id.0)))?;
    let s = self.flats.remove(i);
    self.clear_flat(&s);
    Ok(())
  }

  fn clear_flat(&mut self, s: &SketchFlat) {
    self.curves.retain(|c| !s.curves.contains(&c.id));
    self.points.retain(|p| !s.points.contains(&p.id));
    self.constraints.retain(|c| {
      !c.kind.curves().iter().any(|x| s.curves.contains(x)) && !c.kind.points().iter().any(|x| s.points.contains(x))
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rect(w: f64) -> Vec<SpunSeg> {
    let p = [[0.0, 0.0], [w, 0.0], [w, 10.0], [0.0, 10.0]];
    let mut out: Vec<SpunSeg> = (0..4).map(|i| SpunSeg::Line { a: p[i], b: p[(i + 1) % 4] }).collect();
    out.push(SpunSeg::Circle { center: [w / 2.0, 5.0], radius: 2.0 });
    out.push(SpunSeg::Centerline { a: [w / 3.0, 0.0], b: [w / 3.0, 10.0] });
    out
  }

  #[test]
  fn a_flat_outline_moves_in_place() {
    let mut sk = Sketch::new();
    let id = sk.add_flat(1, true, None);
    assert!(sk.set_flat_outline(id, rect(20.0)).unwrap());
    let f = sk.flat(id).unwrap().clone();
    assert_eq!((f.curves.len(), f.points.len()), (6, 7));
    assert!(sk.points.iter().filter(|p| f.points.contains(&p.id)).all(|p| p.fixed));
    assert!(sk.curve(f.curves[5]).unwrap().construction);
    assert_eq!(sk.find_profiles_count(), 2);
    // the same outline changes nothing; a wider one moves the same curves
    assert!(!sk.set_flat_outline(id, rect(20.0)).unwrap());
    assert!(sk.set_flat_outline(id, rect(30.0)).unwrap());
    assert_eq!(sk.flat(id).unwrap().curves, f.curves);
    let (lo, hi) = sk.bounds().unwrap();
    assert!((hi[0] - lo[0] - 30.0).abs() < 1e-9, "{lo:?} {hi:?}");
    // different pieces replace the curves
    let mut more = rect(30.0);
    more.push(SpunSeg::Circle { center: [5.0, 5.0], radius: 1.0 });
    assert!(sk.set_flat_outline(id, more).unwrap());
    assert_eq!(sk.flat(id).unwrap().curves.len(), 7);
    // deleting one of its curves deletes the flat pattern
    let c = sk.flat(id).unwrap().curves[0];
    sk.remove_curve(c).unwrap();
    assert!(sk.flats.is_empty() && sk.curves.is_empty());
  }

  impl Sketch {
    fn find_profiles_count(&self) -> usize {
      crate::find_profiles(self).len()
    }
  }
}
