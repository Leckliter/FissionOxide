//! Fit-point splines through the modify tools: what Trim, Break and Extend leave of one stays a
//! fit-point spline (through the fit points it keeps and its new ends) rather than becoming a
//! Bézier chain, as in Fusion.
//!
//! Sketches without [`QUINTIC_FIT_SPLINE`] keep the Catmull-Rom splines' rules (one piece per
//! gap, ends that keep their direction, an extra fit point so an extended part keeps its shape);
//! sketches with it follow the minimum-energy quintic, whose pieces don't line up with the gaps.

use crate::behaviour::QUINTIC_FIT_SPLINE;
use crate::freeform::{Bz, bz_d1, path_closed, path_d1, path_sub};
use crate::{ConstraintKind, CurveId, CurveKind, PointId, Sketch, SketchError, dist};

type P = [f64; 2];

/// Fit points closer than this (in pieces) to a new end are dropped, so no sliver piece is left
/// (Catmull-Rom splines: one piece per gap).
const SLIVER: f64 = 0.1;

/// As [`SLIVER`] for a quintic spline (in pieces, about a tenth of a gap), so no sliver gap is
/// left.
const QUINTIC_SLIVER: f64 = 0.5;

fn unit(a: P) -> Option<P> {
  let l = a[0].hypot(a[1]);
  (l > 1e-15).then(|| [a[0] / l, a[1] / l])
}

impl Sketch {
  /// Makes curve `id` (or a new curve when None) the part of curve `kind` (drawn as `path`)
  /// between chain parameters `u.0` < `u.1`, running from point `ends.0` to `ends.1`: an open
  /// fit-point spline stays one, anything else becomes a Bézier chain.
  pub(crate) fn put_piece(
    &mut self,
    id: Option<CurveId>,
    kind: &CurveKind,
    path: &[Bz],
    u: (f64, f64),
    ends: (PointId, PointId),
    construction: bool,
  ) -> Result<CurveId, SketchError> {
    let (u0, u1) = u;
    let (a, b) = ends;
    let CurveKind::Spline { points, start_handle, end_handle, start_curvature, end_curvature } = kind else {
      return self.put_bezier(id, &path_sub(path, u0, u1), a, b, construction);
    };
    let quintic = self.behaviours.has(QUINTIC_FIT_SPLINE);
    // the piece each fit point starts (a Catmull-Rom spline has one piece per gap)
    let starts: Vec<usize> = if quintic {
      crate::spline_fit(kind, &|p| self.pos(p), true).map(|f| f.1).unwrap_or_default()
    } else {
      (0..points.len()).collect()
    };
    if path_closed(path) || starts.len() != points.len() || starts.last() != Some(&path.len()) {
      return self.put_bezier(id, &path_sub(path, u0, u1), a, b, construction);
    }
    let n = path.len() as f64;
    let (cut_start, cut_end) = (u0 > 1e-9, u1 < n - 1e-9);
    let sliver = if quintic { QUINTIC_SLIVER } else { SLIVER };
    // (chain parameter, point) of each fit point kept
    let mut fit: Vec<(f64, PointId)> = vec![(u0, a)];
    for (k, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
      let t = starts[k] as f64;
      let lo = if cut_start { u0 + sliver } else { u0 };
      let hi = if cut_end { u1 - sliver } else { u1 };
      if t > lo + 1e-9 && t < hi - 1e-9 {
        fit.push((t, *p));
      }
    }
    fit.push((u1, b));
    let pos = |s: &Sketch, p: PointId| s.pos(p).ok_or(SketchError::NoPoint(p));
    let sh = if !cut_start {
      *start_handle
    } else if quintic {
      // a new end keeps the direction and speed the curve had there through a tangent handle:
      // the inner control point of a cubic following the old curve to the next fit point
      let (pa, un) = (pos(self, a)?, fit[1].0);
      let d = path_d1(path, u0);
      let l = (un - u0) / 3.0;
      Some(self.add_point(pa[0] + d[0] * l, pa[1] + d[1] * l))
    } else {
      // a new end keeps the direction the curve had there through a tangent handle
      let (pa, (un, pn)) = (pos(self, a)?, fit[1]);
      let sub = path_sub(path, u0, un);
      let h = match sub.as_slice() {
        [one] => one[1],
        _ => {
          let d = unit(path_d1(path, u0)).unwrap_or([0.0, 0.0]);
          let l = dist(pa, pos(self, pn)?) / 3.0;
          [pa[0] + d[0] * l, pa[1] + d[1] * l]
        }
      };
      Some(self.add_point(h[0], h[1]))
    };
    let eh = if !cut_end {
      *end_handle
    } else if quintic {
      let (pb, up) = (pos(self, b)?, fit[fit.len() - 2].0);
      let last = path.len().saturating_sub(1).min((u1.ceil() as usize).saturating_sub(1));
      let d = path.get(last).map(|bz| bz_d1(bz, u1 - last as f64)).unwrap_or([0.0, 0.0]);
      let l = (u1 - up) / 3.0;
      Some(self.add_point(pb[0] - d[0] * l, pb[1] - d[1] * l))
    } else {
      let (pb, (up, pp)) = (pos(self, b)?, fit[fit.len() - 2]);
      let sub = path_sub(path, up, u1);
      let h = match sub.as_slice() {
        [one] => one[2],
        _ => {
          let last = path.len().saturating_sub(1).min(u1.floor() as usize);
          let d = path.get(last).and_then(|bz| unit(bz_d1(bz, u1 - last as f64))).unwrap_or([0.0, 0.0]);
          let l = dist(pb, pos(self, pp)?) / 3.0;
          [pb[0] - d[0] * l, pb[1] - d[1] * l]
        }
      };
      Some(self.add_point(h[0], h[1]))
    };
    // an end that stays keeps its curvature handle, a cut one has none
    let kind = CurveKind::Spline {
      points: fit.into_iter().map(|f| f.1).collect(),
      start_handle: sh,
      end_handle: eh,
      start_curvature: if cut_start { None } else { *start_curvature },
      end_curvature: if cut_end { None } else { *end_curvature },
    };
    self.put_curve(id, kind, construction)
  }

  /// Extends an open Catmull-Rom fit-point spline (a sketch without [`QUINTIC_FIT_SPLINE`]) to
  /// `hit` on curve `other` by adding a fit point there (the old end stays as a fit point); when
  /// the hit is far enough out, a fit point placed so the old end keeps its tangent goes in first,
  /// so the old part keeps its shape exactly. False for any other curve, a quintic fit spline
  /// among them: refitting it through a new point would reshape all of it, and Fusion's keeps its
  /// shape, so the caller carries its end piece on.
  pub(crate) fn extend_fit(
    &mut self,
    curve: CurveId,
    kind: &CurveKind,
    from_end: bool,
    hit: (P, CurveId),
  ) -> Result<bool, SketchError> {
    let CurveKind::Spline { points, start_handle, end_handle, start_curvature, end_curvature } = kind else {
      return Ok(false);
    };
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return Ok(false) };
    if points.len() >= 1000 || self.behaviours.has(QUINTIC_FIT_SPLINE) {
      return Ok(false);
    }
    let old = if from_end { *last } else { *first };
    let old_pos = self.pos(old).ok_or(SketchError::NoPoint(old))?;
    let mut added = Vec::new();
    // the neighbour q that gives the old end, now inside, the tangent T it had: the inner
    // tangent rule makes it (q - F[n-2]) / 2 at the end, (F[1] - q) / 2 at the start
    let bz = self.beziers(curve);
    let keep = match (from_end, bz.first(), bz.last(), points.get(1), points.get(points.len().wrapping_sub(2))) {
      (true, _, Some(b), _, Some(prev)) => {
        let (t, f) = (bz_d1(b, 1.0), self.pos(*prev).unwrap_or(old_pos));
        Some([f[0] + 2.0 * t[0], f[1] + 2.0 * t[1]])
      }
      (false, Some(b), _, Some(next), _) => {
        let (t, f) = (bz_d1(b, 0.0), self.pos(*next).unwrap_or(old_pos));
        Some([f[0] - 2.0 * t[0], f[1] - 2.0 * t[1]])
      }
      _ => None,
    };
    let far = |q: P| {
      let (d, h) = ([q[0] - old_pos[0], q[1] - old_pos[1]], [hit.0[0] - old_pos[0], hit.0[1] - old_pos[1]]);
      let dd = d[0] * d[0] + d[1] * d[1];
      dd > 1e-18 && (h[0] * d[0] + h[1] * d[1]) / dd > 1.5
    };
    if let Some(q) = keep.filter(|q| far(*q)) {
      added.push(self.add_point(q[0], q[1]));
    }
    let np = self.add_point(hit.0[0], hit.0[1]);
    added.push(np);
    let mut pts = points.clone();
    // the extended end loses its handles
    let (sh, eh, sk, ek, dropped) = if from_end {
      pts.extend(added);
      (*start_handle, None, *start_curvature, None, [*end_handle, *end_curvature])
    } else {
      pts.splice(0..0, added.into_iter().rev());
      (None, *end_handle, None, *end_curvature, [*start_handle, *start_curvature])
    };
    self.drop_smooth_at(curve, old_pos);
    if let Some(c) = self.curve_mut(curve) {
      c.kind =
        CurveKind::Spline { points: pts, start_handle: sh, end_handle: eh, start_curvature: sk, end_curvature: ek };
    }
    for h in dropped.into_iter().flatten() {
      self.drop_if_unused(h);
    }
    self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: hit.1 })?;
    Ok(true)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::freeform::{path_at, path_param};
  use crate::{Behaviours, PtSpec};

  fn spline(s: &mut Sketch, pts: &[P]) -> CurveId {
    let ids = pts.iter().map(|p| s.add_point(p[0], p[1])).collect();
    s.add_curve(CurveKind::spline(ids)).unwrap()
  }

  /// A sketch as saved before the quintic fit-point spline: Catmull-Rom splines.
  fn legacy() -> Sketch {
    let mut s = Sketch::new();
    s.behaviours = Behaviours::legacy();
    s
  }

  /// Largest distance from points of `piece` to the curve `path`.
  fn deviation(path: &[Bz], piece: &[Bz]) -> f64 {
    piece
      .iter()
      .flat_map(|b| (0..=8).map(move |k| crate::bezier_at(b, k as f64 / 8.0)))
      .map(|p| dist(path_at(path, path_param(path, p)), p))
      .fold(0.0, f64::max)
  }

  fn fit_points(s: &Sketch, id: CurveId) -> Vec<P> {
    let Some(CurveKind::Spline { points, .. }) = s.curve(id).map(|c| &c.kind) else { panic!("a fit-point spline") };
    points.iter().map(|p| s.pos(*p).unwrap()).collect()
  }

  /// Trims and breaks a five-point spline in sketches made by `new`; `tol` bounds how far the
  /// kept parts move from the old curve.
  fn trim_and_break(new: fn() -> Sketch, tol: f64) {
    let pts = [[0.0, 0.0], [10.0, 10.0], [20.0, 0.0], [30.0, 10.0], [40.0, 0.0]];
    let mut s = new();
    let sp = spline(&mut s, &pts);
    let before = s.beziers(sp);
    s.add_line(PtSpec::At(15.0, -5.0), PtSpec::At(15.0, 20.0)).unwrap();
    s.add_line(PtSpec::At(25.0, -5.0), PtSpec::At(25.0, 20.0)).unwrap();
    let parts = s.trim(sp, [20.0, 0.0]).unwrap();
    assert_eq!(parts.len(), 2);
    let left = fit_points(&s, parts[0]);
    let right = fit_points(&s, parts[1]);
    assert_eq!(&left[..2], &pts[..2], "the fit points it keeps stay where they were");
    assert!((left[2][0] - 15.0).abs() < 1e-9, "a new fit point at the cut");
    assert!((right[0][0] - 25.0).abs() < 1e-9);
    assert_eq!(&right[1..], &pts[3..]);
    for p in &parts {
      let dev = deviation(&before, &s.beziers(*p));
      assert!(dev < tol, "close to the old shape: {dev}");
      // the end at the cut leaves in the direction the old curve had there
      let CurveKind::Spline { start_handle, end_handle, .. } = s.curve(*p).unwrap().kind.clone() else { panic!() };
      assert!(start_handle.is_some() != end_handle.is_some());
    }
    assert!(s.solve().ok());
    // breaking at a point gives two fit-point splines meeting there
    let mut s = new();
    let sp = spline(&mut s, &pts);
    let parts = s.break_curve(sp, [10.0, 10.0], true, 0.0).unwrap();
    assert_eq!(parts.len(), 2);
    let (a, b) = (fit_points(&s, parts[0]), fit_points(&s, parts[1]));
    assert!(dist(a[a.len() - 1], b[0]) < 1e-12);
    assert!(s.solve().ok());
    let back: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
  }

  #[test]
  fn trimmed_and_broken_splines_stay_fit_point_splines() {
    trim_and_break(legacy, 0.6);
  }

  #[test]
  fn trimmed_and_broken_quintic_splines_stay_fit_point_splines() {
    // the kept fit points refit a minimum-energy curve, which spreads the change from the cut
    // over the whole part, so it moves further than a Catmull-Rom spline's
    trim_and_break(Sketch::new, 2.0);
  }

  #[test]
  fn extended_splines_gain_a_fit_point() {
    let mut s = legacy();
    let sp = spline(&mut s, &[[0.0, 0.0], [10.0, 2.0], [20.0, 0.0]]);
    s.add_line(PtSpec::At(40.0, -50.0), PtSpec::At(40.0, 50.0)).unwrap();
    let before = s.beziers(sp);
    s.extend(sp, [20.0, 0.0]).unwrap();
    let f = fit_points(&s, sp);
    assert_eq!(f.len(), 5, "{f:?}");
    assert_eq!(f[2], [20.0, 0.0], "the old end stays a fit point");
    assert!((f[4][0] - 40.0).abs() < 1e-6);
    let after = s.beziers(sp);
    assert!(deviation(&after, &before) < 1e-9, "the old part keeps its shape");
    assert!(s.solve().ok());
    // from the start, to a line close by
    let before = s.beziers(sp);
    s.add_line(PtSpec::At(-3.0, -50.0), PtSpec::At(-3.0, 50.0)).unwrap();
    s.extend(sp, [0.0, 0.0]).unwrap();
    let f = fit_points(&s, sp);
    assert_eq!(f.len(), 6);
    assert!((f[0][0] + 3.0).abs() < 1e-6);
    let dev = deviation(&s.beziers(sp), &before);
    assert!(dev < 0.5, "{dev}");
    assert!(s.solve().ok());
  }

  #[test]
  fn extended_quintic_splines_keep_their_shape() {
    // as Fusion's: the old part stays exactly where it was and the end piece runs on to the line
    // (refitting through a fit point at the hit would move all of it)
    let mut s = Sketch::new();
    let sp = spline(&mut s, &[[0.0, 0.0], [10.0, 2.0], [20.0, 0.0]]);
    s.add_line(PtSpec::At(40.0, -50.0), PtSpec::At(40.0, 50.0)).unwrap();
    let before = s.beziers(sp);
    s.extend(sp, [20.0, 0.0]).unwrap();
    let after = s.beziers(sp);
    assert!(deviation(&after, &before) < 1e-9, "the old part keeps its shape");
    let end = after.last().unwrap()[3];
    assert!((end[0] - 40.0).abs() < 1e-6, "{end:?}");
    assert!(s.solve().ok());
    // from the start, to a line close by
    let before = s.beziers(sp);
    s.add_line(PtSpec::At(-3.0, -50.0), PtSpec::At(-3.0, 50.0)).unwrap();
    s.extend(sp, [0.0, 0.0]).unwrap();
    let after = s.beziers(sp);
    assert!(deviation(&after, &before) < 1e-9);
    assert!((after[0][0][0] + 3.0).abs() < 1e-6, "{:?}", after[0][0]);
    assert!(s.solve().ok());
  }
}
