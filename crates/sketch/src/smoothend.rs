//! Smooth (curvature continuous, G2) ends of fit-point splines, as Fusion makes them.
//!
//! Fusion's Curvature constraint turns on a spline end's tangent and curvature handles: the end
//! keeps its fit point and takes the other curve's direction and curvature, and the rest of the
//! curve is refitted with the least energy (see `quintic`). The curvature fixes the second
//! derivative across the curve, `C'' = κ |C'|² n` (none along it), but the speed `|C'|` at the end
//! is Fusion's own choice. Measured against `NurbsCurve3D.getData()` on two- to four-point
//! splines over a range of directions and curvatures (both turning ways), it is, to 1e-8:
//!
//! - with `L` the end gap's chord, `Δt` its share of the curve's parameter, `v = L / Δt`, `θ` the
//!   angle between the end direction and that chord and `F = (3 - cos θ) / 2`,
//! - the speed with no curvature is `s₀ = 1.44 F v`,
//! - with curvature `|κ|` large enough, `s³ |κ| = C`, where `C = F v² max(F² / 2, sin θ / Δt)`,
//! - and below that, the tangent from `(0, s₀)` to that curve: it touches at `κ* = C / (¾ s₀)³`,
//!   and `s = s₀ (1 - |κ| / (4 κ*))` for `|κ| < κ*`.

type P = [f64; 2];

fn sub(a: P, b: P) -> P {
  [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: P, b: P) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

fn unit(a: P) -> Option<P> {
  let l = a[0].hypot(a[1]);
  (l > 1e-12 && l.is_finite()).then(|| [a[0] / l, a[1] / l])
}

/// Fusion's speed (`|dC/dt|`, `t` over the whole curve in [0, 1]) at a smooth end heading `dir`
/// (unit, the way the curve runs) with signed curvature `curvature`, where the end gap has chord
/// `chord` (run the same way) and parameter share `gap`.
pub(crate) fn speed(chord: P, gap: f64, dir: P, curvature: f64) -> f64 {
  let l = chord[0].hypot(chord[1]);
  if !(l > 1e-12 && gap > 1e-12) {
    return 0.0;
  }
  let v = l / gap;
  let cos = (dir[0] * chord[0] + dir[1] * chord[1]) / l;
  let sin = cross(dir, chord).abs() / l;
  let f = (3.0 - cos) / 2.0;
  let s0 = 1.44 * f * v;
  let k = curvature.abs();
  if k < 1e-300 {
    return s0;
  }
  let c = f * v * v * (f * f / 2.0).max(sin / gap);
  let touch = c / (0.75 * s0).powi(3);
  if k >= touch { (c / k).cbrt() } else { s0 * (1.0 - k / (4.0 * touch)) }
}

/// The signed curvature (left turns positive, heading `dir`) a curvature handle at `handle` asks
/// for at `at`: that of the parabola tangent to `dir` at `at` through the handle (so the handle's
/// offset across the tangent sets it, `κ = 2 offset / along²`, any size). Zero on the tangent line
/// or level with `at`.
pub(crate) fn handle_curvature(at: P, dir: P, handle: P) -> f64 {
  let d = sub(handle, at);
  let along = d[0] * dir[0] + d[1] * dir[1];
  if along.abs() < 1e-12 { 0.0 } else { 2.0 * cross(dir, d) / (along * along) }
}

/// Where to put a curvature handle for curvature `curvature` at `at`, heading `dir`: `reach`
/// back along the tangent (beside the tangent handle) and across it as [`handle_curvature`] reads.
pub(crate) fn place_handle(at: P, dir: P, curvature: f64, reach: f64) -> P {
  let off = curvature * reach * reach / 2.0;
  let n = [-dir[1], dir[0]];
  [at[0] - reach * dir[0] + off * n[0], at[1] - reach * dir[1] + off * n[1]]
}

/// The first and second derivatives (by the curve's parameter) a smooth end at fit point `at`
/// takes: `next` is the neighbouring fit point and `gap` the parameter share of the gap between
/// them, `handle` the tangent handle and `curvature` the curvature handle. None when the tangent
/// handle sits on the end.
pub(crate) fn end_derivatives(at: P, next: P, gap: f64, handle: P, curvature: P, at_end: bool) -> Option<(P, P)> {
  // the way the curve runs at this end, and the end gap's chord run the same way
  let (dir, chord) =
    if at_end { (unit(sub(at, handle))?, sub(at, next)) } else { (unit(sub(handle, at))?, sub(next, at)) };
  let k = handle_curvature(at, dir, curvature);
  let s = speed(chord, gap, dir, k);
  let n = [-dir[1], dir[0]];
  let a = k * s * s;
  Some(([dir[0] * s, dir[1] * s], [n[0] * a, n[1] * a]))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{ConstraintKind, CurveKind, PtSpec, Sketch};

  /// Speeds Fusion 2705 gave smooth ends of fitted splines (curvature constraints to lines and
  /// arcs), each with its end gap: chord, parameter share, direction, curvature.
  #[test]
  fn speeds_match_fusion() {
    let deg = |a: f64| [a.to_radians().cos(), a.to_radians().sin()];
    // (0,0)-(40,0): the whole curve is the end gap
    let one = |dir: f64, k: f64| speed([40.0, 0.0], 1.0, deg(dir), k);
    for (dir, k, want) in [
      (0.0, 0.0, 57.6),
      (0.0, 0.005, 50.344_1),
      (0.0, 0.1, 20.0),
      (0.0, -0.5, 11.696_1),
      (30.0, 0.002, 58.361_7),
      (60.0, 0.1, 25.873_4),
      (60.0, 0.002, 68.727_2),
      (90.0, 0.0, 86.4),
      (90.0, 0.02, 51.299_3),
    ] {
      let s = one(dir, k);
      assert!((s - want).abs() < 2e-4, "{dir} {k}: {s} vs {want}");
    }
    // (0,0)-(12,8)-(25,4)-(40,10), leaving (40,10) upwards: the end gap is a third of the curve
    let gap = 1.0 - 0.651_396_19;
    for (k, want) in [(0.0, 87.709_5), (0.01, 79.407_2), (0.05, 53.175_8), (-0.1, 42.205_6)] {
      let s = speed([15.0, 6.0], gap, [0.0, 1.0], k);
      assert!((s - want).abs() < 2e-3, "{k}: {s} vs {want}");
    }
  }

  #[test]
  fn curvature_handles_round_trip() {
    let (at, dir) = ([3.0, 4.0], [0.6, 0.8]);
    for k in [-3.0, -0.01, 0.0, 0.02, 0.25, 40.0] {
      let h = place_handle(at, dir, k, 2.0);
      assert!((handle_curvature(at, dir, h) - k).abs() < 1e-12, "{k}");
      assert!(((h[0] - at[0]) * dir[0] + (h[1] - at[1]) * dir[1] + 2.0).abs() < 1e-12, "behind the end");
    }
    // at the end the handles sit behind the end point
    let (d1, d2) =
      end_derivatives([10.0, 0.0], [0.0, 0.0], 1.0, [7.0, 0.0], place_handle([10.0, 0.0], [1.0, 0.0], 0.1, 3.0), true)
        .unwrap();
    assert!(d1[0] > 0.0 && d1[1].abs() < 1e-12);
    assert!((d2[1] / (d1[0] * d1[0]) - 0.1).abs() < 1e-12 && d2[0].abs() < 1e-12);
  }

  /// Fusion's Curvature constraint between a spline whose fit points are all fixed and a fixed
  /// arc (the 104-spline-arc-curvature reference part): the end turns to leave upwards with the
  /// arc's curvature, at Fusion's speed, and nothing is left free.
  #[test]
  fn curvature_constraint_turns_a_fixed_spline_end() {
    let mut s = Sketch::new();
    let fit: Vec<_> =
      [[0.0, 0.0], [12.0, 8.0], [25.0, 4.0], [40.0, 10.0]].iter().map(|p| s.add_point(p[0], p[1])).collect();
    let sp = s.add_curve(CurveKind::spline(fit.clone())).unwrap();
    let arc = s.add_arc_3pt(PtSpec::Point(fit[3]), [50.0, 20.0], PtSpec::At(60.0, 10.0)).unwrap();
    for p in fit.iter().chain(s.curve_points(arc).iter()).copied().collect::<Vec<_>>() {
      s.point_mut(p).unwrap().fixed = true;
    }
    s.add_constraint(ConstraintKind::Curvature { a: sp, b: arc }).unwrap();
    let r = s.solve();
    assert!(r.ok() && r.dof == 0, "{r:?}");
    let kind = s.curve(sp).unwrap().kind.clone();
    let (at, dir, k) = crate::freeform::end_frame(&kind, true, &|p| s.pos(p), &s.behaviours).unwrap();
    assert!(crate::dist(at, [40.0, 10.0]) < 1e-9);
    assert!(dir[0].abs() < 1e-7 && dir[1] > 0.0, "{dir:?}");
    assert!((k + 0.1).abs() < 1e-7, "turning right into the arc: {k}");
    let CurveKind::Spline { end_handle: Some(h), end_curvature: Some(c), .. } = kind else { panic!("both handles") };
    let (h, c) = (s.pos(h).unwrap(), s.pos(c).unwrap());
    let (d1, _) = end_derivatives([40.0, 10.0], [25.0, 4.0], 1.0 - 0.651_396_19, h, c, true).unwrap();
    assert!((d1[1] - 42.206).abs() < 0.01, "Fusion's speed: {d1:?}");
  }
}
