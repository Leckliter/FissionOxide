//! Circles tangent to two curves with a given radius, or to three curves (Fusion's 2-Tangent and
//! 3-Tangent Circle), held by tangent constraints.

use crate::{ConstraintKind, CurveId, CurveKind, Dim, Sketch, SketchError, dist};

type P = [f64; 2];

/// A curve a tangent circle can touch.
#[derive(Clone, Copy)]
enum Target {
  /// A line through `a` with unit direction `d`.
  Line { a: P, d: P },
  /// A circle (or an arc's circle).
  Round { c: P, r: f64 },
}

impl Target {
  /// Ways to be tangent: lines on either side; circles outside, inside it, or around it.
  fn branches(&self) -> &'static [i8] {
    match self {
      Target::Line { .. } => &[1, -1],
      Target::Round { .. } => &[0, 1, 2],
    }
  }

  /// Zero when a circle at `c` with radius `r` touches this target on `branch`.
  fn residual(&self, branch: i8, c: P, r: f64) -> f64 {
    match *self {
      Target::Line { a, d } => {
        let sd = d[0] * (c[1] - a[1]) - d[1] * (c[0] - a[0]);
        sd - f64::from(branch) * r
      }
      Target::Round { c: o, r: ro } => {
        let dd = dist(c, o);
        match branch {
          0 => dd - (ro + r),
          1 => dd - (ro - r),
          _ => dd - (r - ro),
        }
      }
    }
  }
}

/// Damped Gauss–Newton on a few unknowns; Some(x) when every residual vanishes.
fn newton(f: &dyn Fn(&[f64]) -> Vec<f64>, mut x: Vec<f64>) -> Option<Vec<f64>> {
  let n = x.len();
  let norm = |v: &[f64]| v.iter().map(|a| a * a).sum::<f64>();
  let mut r = f(&x);
  let mut lambda = 1e-6;
  for _ in 0..100 {
    if r.iter().all(|v| v.abs() < 1e-11) {
      return Some(x);
    }
    let m = r.len();
    let mut j = vec![0.0; m * n];
    for k in 0..n {
      let h = 1e-7 * (1.0 + x[k].abs());
      let mut xp = x.clone();
      xp[k] += h;
      let mut xm = x.clone();
      xm[k] -= h;
      let (rp, rm) = (f(&xp), f(&xm));
      for i in 0..m {
        j[i * n + k] = (rp[i] - rm[i]) / (2.0 * h);
      }
    }
    let mut improved = false;
    while lambda < 1e10 {
      // (JᵀJ + λI) dx = -Jᵀr
      let mut a = vec![0.0; n * (n + 1)];
      for p in 0..n {
        for q in 0..n {
          a[p * (n + 1) + q] =
            (0..m).map(|i| j[i * n + p] * j[i * n + q]).sum::<f64>() + if p == q { lambda } else { 0.0 };
        }
        a[p * (n + 1) + n] = -(0..m).map(|i| j[i * n + p] * r[i]).sum::<f64>();
      }
      let Some(dx) = gauss(&mut a, n) else {
        lambda *= 10.0;
        continue;
      };
      let xn: Vec<f64> = x.iter().zip(&dx).map(|(a, b)| a + b).collect();
      let rn = f(&xn);
      if rn.iter().all(|v| v.is_finite()) && norm(&rn) < norm(&r) {
        x = xn;
        r = rn;
        lambda = (lambda / 10.0).max(1e-15);
        improved = true;
        break;
      }
      lambda *= 10.0;
    }
    if !improved {
      break;
    }
  }
  r.iter().all(|v| v.abs() < 1e-9).then_some(x)
}

/// Solves the n×(n+1) augmented system in place.
fn gauss(a: &mut [f64], n: usize) -> Option<Vec<f64>> {
  let w = n + 1;
  for col in 0..n {
    let piv = (col..n).max_by(|p, q| a[p * w + col].abs().total_cmp(&a[q * w + col].abs()))?;
    if a[piv * w + col].abs() < 1e-300 {
      return None;
    }
    for k in 0..w {
      a.swap(col * w + k, piv * w + k);
    }
    for row in 0..n {
      if row != col {
        let f = a[row * w + col] / a[col * w + col];
        for k in 0..w {
          a[row * w + k] -= f * a[col * w + k];
        }
      }
    }
  }
  Some((0..n).map(|i| a[i * w + n] / a[i * w + i]).collect())
}

impl Sketch {
  fn tangent_target(&self, id: CurveId) -> Result<Target, SketchError> {
    let what = SketchError::WrongGeometry("a tangent circle", "lines, circles or arcs");
    match self.curve(id).map(|c| &c.kind).ok_or(SketchError::NoCurve(id))? {
      CurveKind::Line { a, b } => {
        let (a, b) = (self.pos(*a).ok_or(what.clone())?, self.pos(*b).ok_or(what.clone())?);
        let l = dist(a, b);
        if l < 1e-12 {
          return Err(what);
        }
        Ok(Target::Line { a, d: [(b[0] - a[0]) / l, (b[1] - a[1]) / l] })
      }
      CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. } => {
        Ok(Target::Round { c: self.pos(*center).ok_or(what)?, r: self.radius(id).unwrap_or(0.0) })
      }
      _ => Err(what),
    }
  }

  /// A circle tangent to two curves with radius `radius`, or to three curves (`radius` None).
  /// Curves are lines, circles or arcs (lines count as infinite, arcs as whole circles). `near`
  /// picks the side of each line the circle sits on (its quadrant, as in Fusion); of the circles
  /// left, the one whose centre is nearest `near` is drawn. Two curves also get a radius
  /// dimension.
  pub fn add_tangent_circle(
    &mut self,
    curves: &[CurveId],
    radius: Option<Dim>,
    near: P,
  ) -> Result<CurveId, SketchError> {
    let targets: Vec<Target> = curves.iter().map(|c| self.tangent_target(*c)).collect::<Result<_, _>>()?;
    let n = targets.len();
    match (n, &radius) {
      (2, Some(d)) if d.value.is_finite() && d.value > 0.0 => {}
      (2, _) => return Err(SketchError::InvalidValue("a 2-tangent circle needs a positive radius".into())),
      (3, _) => {}
      _ => return Err(SketchError::WrongGeometry("a tangent circle", "two or three curves")),
    }
    if (1..n).any(|i| curves[..i].contains(&curves[i])) {
      return Err(SketchError::WrongGeometry("a tangent circle", "different curves"));
    }
    let fixed_r = radius.as_ref().map(|d| d.value);
    // starting size: the mean distance from the pick to the curves
    let r0 = fixed_r.unwrap_or_else(|| {
      let ds: Vec<f64> = curves.iter().filter_map(|c| self.distance_to_curve(*c, near)).collect();
      (ds.iter().sum::<f64>() / ds.len().max(1) as f64).max(1e-3)
    });
    let combos = |sided: bool| -> Vec<Vec<i8>> {
      let mut combos: Vec<Vec<i8>> = vec![vec![]];
      for t in &targets {
        let bs = match (*t, sided) {
          (Target::Line { a, d }, true) => match d[0] * (near[1] - a[1]) - d[1] * (near[0] - a[0]) {
            sd if sd > 1e-9 => vec![1],
            sd if sd < -1e-9 => vec![-1],
            _ => t.branches().to_vec(),
          },
          _ => t.branches().to_vec(),
        };
        combos = combos.into_iter().flat_map(|c| bs.iter().map(move |b| [c.clone(), vec![*b]].concat())).collect();
      }
      combos
    };
    let nearest = |combos: &[Vec<i8>]| -> Option<(f64, P, f64)> {
      let mut best: Option<(f64, P, f64)> = None;
      for combo in combos {
        let eval = |x: &[f64]| -> Vec<f64> {
          let (c, r) = ([x[0], x[1]], fixed_r.unwrap_or_else(|| x.get(2).copied().unwrap_or(0.0)));
          targets.iter().zip(combo).map(|(t, b)| t.residual(*b, c, r)).collect()
        };
        let x0 = if fixed_r.is_some() { vec![near[0], near[1]] } else { vec![near[0], near[1], r0] };
        let Some(x) = newton(&eval, x0) else { continue };
        let (c, r) = ([x[0], x[1]], fixed_r.unwrap_or_else(|| x.get(2).copied().unwrap_or(0.0)));
        if !(r > 1e-9 && c[0].is_finite() && c[1].is_finite()) {
          continue;
        }
        let d = dist(c, near);
        if best.is_none_or(|b| d < b.0 - 1e-9) {
          best = Some((d, c, r));
        }
      }
      best
    };
    // Like Fusion, the pick names the side of each line the circle sits on (the quadrant it is
    // in); the nearest centre decides among what's left, and among everything if nothing fits.
    let best = nearest(&combos(true)).or_else(|| nearest(&combos(false)));
    let (_, c, r) = best.ok_or(SketchError::InvalidValue("no circle is tangent to those curves".into()))?;
    let center = self.add_point(c[0], c[1]);
    let circle = self.add_curve(CurveKind::Circle { center, radius: r })?;
    for other in curves {
      self.add_constraint(ConstraintKind::Tangent { a: *other, b: circle })?;
    }
    if let Some(d) = radius {
      self.add_constraint(ConstraintKind::Radius { curve: circle, value: d })?;
    }
    Ok(circle)
  }
}

#[cfg(test)]
mod tests {
  use crate::*;

  #[test]
  fn two_tangent_circle_in_a_corner() {
    let mut s = Sketch::new();
    let (h, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(50.0, 0.0)).unwrap();
    let (v, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(0.0, 50.0)).unwrap();
    let c = s.add_tangent_circle(&[h, v], Some(Dim::new(5.0)), [8.0, 7.0]).unwrap();
    assert!(s.solve().ok());
    let o = s.pos(s.curve_points(c)[0]).unwrap();
    assert!(dist(o, [5.0, 5.0]) < 1e-6, "{o:?}");
    // the other quadrant when picked there
    let c2 = s.add_tangent_circle(&[h, v], Some(Dim::new(3.0)), [-4.0, 2.0]).unwrap();
    assert!(s.solve().ok());
    assert!(dist(s.pos(s.curve_points(c2)[0]).unwrap(), [-3.0, 3.0]) < 1e-6);
    assert!(s.add_tangent_circle(&[h, v], None, [1.0, 1.0]).is_err(), "two curves need a radius");
  }

  #[test]
  fn two_tangent_circle_stays_in_the_picked_quadrant() {
    // a 20° wedge picked near its vertex: the circle sits in the wedge, not the nearer one below
    let mut s = Sketch::new();
    let (h, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(40.0, 0.0)).unwrap();
    let (w, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(28.191, 10.261)).unwrap();
    let c = s.add_tangent_circle(&[h, w], Some(Dim::new(4.0)), [3.0, 0.4]).unwrap();
    assert!(s.solve().ok());
    let o = s.pos(s.curve_points(c)[0]).unwrap();
    assert!((o[1] - 4.0).abs() < 1e-6 && (o[0] - 22.6845).abs() < 1e-3, "{o:?}");
  }

  #[test]
  fn three_tangent_circle_is_the_incircle_or_touches_circles() {
    let mut s = Sketch::new();
    // 3-4-5 right triangle: incircle radius 1 at (1, 1)
    let (a, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(4.0, 0.0)).unwrap();
    let (b, _) = s.add_line(PtSpec::At(4.0, 0.0), PtSpec::At(0.0, 3.0)).unwrap();
    let (c, _) = s.add_line(PtSpec::At(0.0, 3.0), PtSpec::At(0.0, 0.0)).unwrap();
    let circle = s.add_tangent_circle(&[a, b, c], None, [1.3, 1.0]).unwrap();
    assert!(s.solve().ok());
    assert!((s.radius(circle).unwrap() - 1.0).abs() < 1e-6);
    // tangent to two circles and a line
    let mut s = Sketch::new();
    let (l, _) = s.add_line(PtSpec::At(-50.0, 0.0), PtSpec::At(50.0, 0.0)).unwrap();
    let c1 = s.add_circle(PtSpec::At(-10.0, 5.0), 5.0).unwrap();
    let c2 = s.add_circle(PtSpec::At(10.0, 5.0), 5.0).unwrap();
    let t = s.add_tangent_circle(&[l, c1, c2], None, [0.0, 1.0]).unwrap();
    assert!(s.solve().ok());
    let (o, r) = (s.pos(s.curve_points(t)[0]).unwrap(), s.radius(t).unwrap());
    assert!((o[1] - r).abs() < 1e-6 && (dist(o, [10.0, 5.0]) - 5.0 - r).abs() < 1e-6, "{o:?} {r}");
  }
}
