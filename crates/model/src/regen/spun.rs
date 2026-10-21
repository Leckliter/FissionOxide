//! Spun profiles (Fusion's Spun Profile): the outline of the region bodies sweep through as they
//! turn about an axis, drawn on the sketch plane on one side of the axis. The kernel measures how
//! far the bodies reach from the axis at stations along it (`Shape::radial_slices`); this module
//! fits lines, arcs and splines to those reaches and hands the outline to the sketch.

use fox_kernel::glam::DVec3;
use fox_kernel::{Plane, Shape};
use fox_sketch::{SpunAxis, SpunSeg};

use super::Ctx;
use crate::BodyId;

/// Samples taken inside each stretch of the axis between two corner stations.
const SAMPLES: usize = 16;
/// The most stretches an outline is cut into (corner stations plus those found by bisection).
const MAX_STRETCHES: usize = 256;

impl Ctx<'_> {
  /// Works out every spun profile of the sketch from the bodies as they are now. Returns a warning
  /// naming the profiles that could not be traced (they keep their last outline).
  pub(super) fn refresh_spuns(&self, plane: &Plane, sketch: &mut fox_sketch::Sketch) -> Option<String> {
    let mut errors = Vec::new();
    for sp in sketch.spuns.clone() {
      let made = self
        .spun_outline(plane, sketch, &sp)
        .and_then(|o| sketch.set_spun_outline(sp.id, o).map_err(|e| e.to_string()));
      if let Err(e) = made {
        errors.push(format!("spun profile: {e}"));
      }
    }
    (!errors.is_empty()).then(|| errors.join("; "))
  }

  fn spun_outline(
    &self,
    plane: &Plane,
    sketch: &fox_sketch::Sketch,
    sp: &fox_sketch::SketchSpun,
  ) -> Result<Vec<SpunSeg>, String> {
    let (o, d) = match &sp.axis {
      SpunAxis::World { origin, dir } => (DVec3::from(*origin), DVec3::from(*dir)),
      SpunAxis::Line { curve } => {
        let Some(fox_sketch::CurveKind::Line { a, b }) = sketch.curve(*curve).map(|c| &c.kind) else {
          return Err("the axis line no longer exists".into());
        };
        let (Some(pa), Some(pb)) = (sketch.pos(*a), sketch.pos(*b)) else {
          return Err("the axis line no longer exists".into());
        };
        let (pa, pb) = (plane.to_world(pa), plane.to_world(pb));
        (pa, pb - pa)
      }
    };
    let mut shapes = Vec::new();
    for b in &sp.bodies {
      let bi = self.body_index(BodyId(*b))?;
      shapes.push(&self.state.bodies[bi].shape);
    }
    outline(&shapes, o, d, plane, sp.flip, sp.centerline)
  }
}

/// The spun outline of `shapes` about the axis through `o` along `d`, in `plane`'s coordinates:
/// on the side of the axis the bodies are on (the other side when `flip` is set), with the axis as
/// a centre line when `centerline` is set.
pub fn outline(
  shapes: &[&Shape],
  o: DVec3,
  d: DVec3,
  plane: &Plane,
  flip: bool,
  centerline: bool,
) -> Result<Vec<SpunSeg>, String> {
  if shapes.is_empty() {
    return Err("there are no bodies to spin".into());
  }
  let d = d.try_normalize().ok_or("the axis has no direction")?;
  let n = plane.normal();
  // the axis as drawn: projected onto the sketch plane
  let dp = (d - n * d.dot(n)).try_normalize().ok_or("the axis is perpendicular to the sketch plane")?;
  let op = o - n * (o - plane.origin).dot(n);
  let mut w = n.cross(dp);

  // which side: where the bodies are (the sketch's side to the right of the axis when they straddle it)
  let mut lo = DVec3::splat(f64::MAX);
  let mut hi = DVec3::splat(f64::MIN);
  let mut events = Vec::new();
  for s in shapes {
    let m = s.mass().map_err(|e| e.to_string())?;
    lo = lo.min(m.bbox_min);
    hi = hi.max(m.bbox_max);
    events.push(s.axial_events(o, d).map_err(|e| e.to_string())?);
  }
  let centre = (lo + hi) / 2.0;
  if (centre - op).dot(w) < -1e-9 * (hi - lo).length().max(1.0) {
    w = -w;
  }
  if flip {
    w = -w;
  }
  let tmin = events.iter().map(|e| e.tmin).fold(f64::MAX, f64::min);
  let tmax = events.iter().map(|e| e.tmax).fold(f64::MIN, f64::max);
  if !(tmax > tmin) {
    return Err("the bodies have no extent along the axis".into());
  }
  let rmax = events.iter().flat_map(|e| e.vertices.iter().map(|v| v.1)).fold(0.0, f64::max);
  let size = (tmax - tmin).max(rmax).max(1.0);
  let tol = 1e-5 * size;
  let eps = 1e-7 * size;

  let slices = |ts: &[f64]| -> Result<Vec<Vec<(f64, f64)>>, String> {
    let mut all: Vec<Vec<(f64, f64)>> = vec![Vec::new(); ts.len()];
    for s in shapes {
      for (i, spans) in s.radial_slices(o, d, ts).map_err(|e| e.to_string())?.into_iter().enumerate() {
        all[i].extend(spans);
      }
    }
    Ok(all.into_iter().map(|v| merge_spans(v, eps)).collect())
  };

  let mut stations: Vec<f64> =
    events.iter().flat_map(|e| e.stations.iter().copied()).filter(|t| *t > tmin + eps && *t < tmax - eps).collect();
  stations.push(tmin);
  stations.push(tmax);
  stations.sort_by(f64::total_cmp);
  stations.dedup_by(|a, b| (*a - *b).abs() <= eps);

  // each stretch between stations: its ends and, per span, the inner and outer reach fitted
  struct Stretch {
    a: f64,
    b: f64,
    /// Per span: (inner, outer).
    spans: Vec<(Fit, Fit)>,
  }
  let mut todo: Vec<(f64, f64)> = stations.windows(2).map(|w| (w[0], w[1])).rev().collect();
  let mut stretches: Vec<Stretch> = Vec::new();
  while let Some((a, b)) = todo.pop() {
    if b - a <= eps {
      continue;
    }
    let delta = (1e-6 * (b - a)).max(1e-9);
    let mut ts = vec![a + delta];
    ts.extend((1..SAMPLES).map(|k| a + (b - a) * k as f64 / SAMPLES as f64));
    ts.push(b - delta);
    let sl = slices(&ts)?;
    if let Some(k) = (0..sl.len() - 1).find(|&k| sl[k].len() != sl[k + 1].len()) {
      if stretches.len() + todo.len() >= MAX_STRETCHES {
        return Err("the bodies' outline is too complicated".into());
      }
      // the number of spans changes in here: find where, and treat the two sides apart
      let (mut l, mut r) = (ts[k], ts[k + 1]);
      let nl = sl[k].len();
      for _ in 0..40 {
        if r - l <= eps * 0.1 {
          break;
        }
        let m = (l + r) / 2.0;
        if slices(&[m])?[0].len() == nl {
          l = m;
        } else {
          r = m;
        }
      }
      let m = (l + r) / 2.0;
      todo.push((m, b));
      todo.push((a, m));
      continue;
    }
    let count = sl[0].len();
    let mut spans = Vec::with_capacity(count);
    for j in 0..count {
      let inner: Vec<[f64; 2]> = ts.iter().zip(&sl).map(|(t, s)| [*t, s[j].0]).collect();
      let outer: Vec<[f64; 2]> = ts.iter().zip(&sl).map(|(t, s)| [*t, s[j].1]).collect();
      spans.push((Fit::new(&inner, a, b, tol), Fit::new(&outer, a, b, tol)));
    }
    stretches.push(Stretch { a, b, spans });
  }
  stretches.sort_by(|x, y| x.a.total_cmp(&y.a));

  // (t, r) on the sketch
  let at = |t: f64, r: f64| -> [f64; 2] { plane.to_local(op + dp * t + w * r) };
  let mut segs: Vec<SpunSeg> = Vec::new();
  for s in &stretches {
    for (inner, outer) in &s.spans {
      for f in [inner, outer] {
        segs.push(f.seg(&at));
      }
    }
  }
  // across the axis at each station: where the material on one side is not matched on the other
  let ends = |s: Option<&Stretch>, t: f64| -> Vec<(f64, f64)> {
    s.filter(|s| (s.a - t).abs() <= eps || (s.b - t).abs() <= eps)
      .map(|s| s.spans.iter().map(|(i, o)| (i.at(t), o.at(t))).collect())
      .unwrap_or_default()
  };
  let mut cuts: Vec<f64> = stretches.iter().flat_map(|s| [s.a, s.b]).collect();
  cuts.sort_by(f64::total_cmp);
  cuts.dedup_by(|a, b| (*a - *b).abs() <= eps);
  for t in cuts {
    let left = ends(stretches.iter().find(|s| (s.b - t).abs() <= eps), t);
    let right = ends(stretches.iter().find(|s| (s.a - t).abs() <= eps), t);
    for (p, q) in symmetric_difference(&left, &right, eps) {
      segs.push(SpunSeg::Line { a: at(t, p), b: at(t, q) });
    }
  }
  let mut segs = join_segments(segs, eps.max(1e-7));
  if centerline {
    segs.push(SpunSeg::Centerline { a: at(tmin, 0.0), b: at(tmax, 0.0) });
  }
  Ok(segs)
}

/// Sorted, overlapping spans merged into one.
fn merge_spans(mut v: Vec<(f64, f64)>, eps: f64) -> Vec<(f64, f64)> {
  v.sort_by(|a, b| a.0.total_cmp(&b.0));
  let mut out: Vec<(f64, f64)> = Vec::new();
  for (a, b) in v {
    match out.last_mut() {
      Some(l) if a <= l.1 + eps => l.1 = l.1.max(b),
      _ => out.push((a, b)),
    }
  }
  out
}

/// The stretches of the line covered by exactly one of two sets of spans.
fn symmetric_difference(a: &[(f64, f64)], b: &[(f64, f64)], eps: f64) -> Vec<(f64, f64)> {
  let a = merge_spans(a.to_vec(), eps);
  let b = merge_spans(b.to_vec(), eps);
  let mut xs: Vec<f64> = a.iter().chain(&b).flat_map(|s| [s.0, s.1]).collect();
  xs.sort_by(f64::total_cmp);
  xs.dedup_by(|x, y| (*x - *y).abs() <= eps);
  let inside = |v: &[(f64, f64)], x: f64| v.iter().any(|s| x > s.0 && x < s.1);
  let mut out: Vec<(f64, f64)> = Vec::new();
  for w in xs.windows(2) {
    if w[1] - w[0] <= eps {
      continue;
    }
    let m = (w[0] + w[1]) / 2.0;
    if inside(&a, m) != inside(&b, m) {
      match out.last_mut() {
        Some(l) if (l.1 - w[0]).abs() <= eps => l.1 = w[1],
        _ => out.push((w[0], w[1])),
      }
    }
  }
  out
}

/// How far one edge of a span reaches from the axis along a stretch, as a curve in (t, r).
enum Fit {
  /// r = c + k t.
  Line { c: f64, k: f64, a: f64, b: f64 },
  /// (t - ct)² + (r - cr)² = radius², on the side of `cr` given by `sign`.
  Arc { ct: f64, cr: f64, radius: f64, sign: f64, a: f64, b: f64 },
  /// Through these (t, r) points (the ends at exactly the stretch's ends).
  Spline { points: Vec<[f64; 2]> },
}

impl Fit {
  fn new(p: &[[f64; 2]], a: f64, b: f64, tol: f64) -> Fit {
    // straight?
    let n = p.len() as f64;
    let (st, sr) = p.iter().fold((0.0, 0.0), |s, q| (s.0 + q[0], s.1 + q[1]));
    let (mt, mr) = (st / n, sr / n);
    let (stt, str_) = p.iter().fold((0.0, 0.0), |s, q| (s.0 + (q[0] - mt).powi(2), s.1 + (q[0] - mt) * (q[1] - mr)));
    if stt > 0.0 {
      let k = str_ / stt;
      let c = mr - k * mt;
      if p.iter().all(|q| (c + k * q[0] - q[1]).abs() <= tol) {
        return Fit::Line { c, k, a, b };
      }
    }
    // circular?
    let (f, m, l) = (p[0], p[p.len() / 2], p[p.len() - 1]);
    if let Some(cc) = fox_sketch::circumcenter(f, m, l) {
      let radius = (f[0] - cc[0]).hypot(f[1] - cc[1]);
      let on = p.iter().all(|q| ((q[0] - cc[0]).hypot(q[1] - cc[1]) - radius).abs() <= tol);
      let sign = if m[1] >= cc[1] { 1.0 } else { -1.0 };
      let same_side = p.iter().all(|q| (q[1] - cc[1]) * sign >= -tol);
      if on && same_side && radius < 1e9 * tol {
        return Fit::Arc { ct: cc[0], cr: cc[1], radius, sign, a, b };
      }
    }
    // a spline through the samples, its ends moved onto the stretch's ends
    let mut points = p.to_vec();
    let k = points.len();
    let end = |q0: [f64; 2], q1: [f64; 2], t: f64| -> [f64; 2] {
      let s = if (q1[0] - q0[0]).abs() > 0.0 { (t - q0[0]) / (q1[0] - q0[0]) } else { 0.0 };
      [t, q0[1] + (q1[1] - q0[1]) * s]
    };
    points[0] = end(p[0], p[1], a);
    points[k - 1] = end(p[k - 1], p[k - 2], b);
    Fit::Spline { points }
  }

  /// The reach at station t (one of the stretch's ends).
  fn at(&self, t: f64) -> f64 {
    match self {
      Fit::Line { c, k, .. } => c + k * t,
      Fit::Arc { ct, cr, radius, sign, .. } => cr + sign * (radius * radius - (t - ct) * (t - ct)).max(0.0).sqrt(),
      Fit::Spline { points } => {
        let (f, l) = (points[0], points[points.len() - 1]);
        if (t - f[0]).abs() <= (t - l[0]).abs() { f[1] } else { l[1] }
      }
    }
  }

  fn seg(&self, at: &impl Fn(f64, f64) -> [f64; 2]) -> SpunSeg {
    match self {
      Fit::Line { a, b, .. } => SpunSeg::Line { a: at(*a, self.at(*a)), b: at(*b, self.at(*b)) },
      Fit::Arc { a, b, .. } => {
        let m = (a + b) / 2.0;
        SpunSeg::Arc { a: at(*a, self.at(*a)), mid: at(m, self.at(m)), b: at(*b, self.at(*b)) }
      }
      Fit::Spline { points } => SpunSeg::Spline { points: points.iter().map(|q| at(q[0], q[1])).collect() },
    }
  }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The ends of a line, arc, spline or Bézier chain.
fn ends(s: &SpunSeg) -> Option<([f64; 2], [f64; 2])> {
  match s {
    SpunSeg::Line { a, b } | SpunSeg::Arc { a, b, .. } => Some((*a, *b)),
    SpunSeg::Spline { points } | SpunSeg::Bezier { points } => Some((points[0], points[points.len() - 1])),
    SpunSeg::Circle { .. } | SpunSeg::Centerline { .. } => None,
  }
}

/// Drops pieces with no length and joins pieces that carry on one another where nothing else
/// meets them: lines along the same line, arcs on the same circle (an arc that closes becomes a
/// circle).
fn join_segments(segs: Vec<SpunSeg>, tol: f64) -> Vec<SpunSeg> {
  let mut segs: Vec<SpunSeg> = segs
    .into_iter()
    .filter(|s| match s {
      SpunSeg::Line { a, b } => dist(*a, *b) > tol,
      SpunSeg::Arc { a, mid, b } => dist(*a, *mid) > tol && dist(*mid, *b) > tol,
      _ => true,
    })
    .collect();
  'again: loop {
    for i in 0..segs.len() {
      let Some((ia, ib)) = ends(&segs[i]) else { continue };
      for p in [ia, ib] {
        let users: Vec<usize> = (0..segs.len())
          .filter(|&k| ends(&segs[k]).is_some_and(|(a, b)| dist(a, p) <= tol || dist(b, p) <= tol))
          .collect();
        if users.len() != 2 {
          continue;
        }
        let j = if users[0] == i { users[1] } else { users[0] };
        let Some((ja, jb)) = ends(&segs[j]) else { continue };
        let far_i = if dist(ia, p) <= tol { ib } else { ia };
        let far_j = if dist(ja, p) <= tol { jb } else { ja };
        let joined = match (&segs[i], &segs[j]) {
          (SpunSeg::Line { .. }, SpunSeg::Line { .. }) => {
            let (u, v) = ([p[0] - far_i[0], p[1] - far_i[1]], [far_j[0] - p[0], far_j[1] - p[1]]);
            let cross = u[0] * v[1] - u[1] * v[0];
            let dot = u[0] * v[0] + u[1] * v[1];
            (cross.abs() <= tol * dist(far_i, far_j) && dot > 0.0).then_some(SpunSeg::Line { a: far_i, b: far_j })
          }
          (SpunSeg::Arc { a, mid, b }, SpunSeg::Arc { a: c, mid: m, b: e }) => {
            match (fox_sketch::circumcenter(*a, *mid, *b), fox_sketch::circumcenter(*c, *m, *e)) {
              (Some(c1), Some(c2)) if dist(c1, c2) <= tol && (dist(c1, *a) - dist(c2, *c)).abs() <= tol => {
                if dist(far_i, far_j) <= tol {
                  Some(SpunSeg::Circle { center: c1, radius: dist(c1, *a) })
                } else {
                  Some(SpunSeg::Arc { a: far_i, mid: p, b: far_j })
                }
              }
              _ => None,
            }
          }
          _ => None,
        };
        if let Some(s) = joined {
          let (lo, hi) = (i.min(j), i.max(j));
          segs.remove(hi);
          segs[lo] = s;
          continue 'again;
        }
      }
    }
    return segs;
  }
}
