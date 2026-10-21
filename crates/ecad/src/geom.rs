//! 2D geometry in millimetres: points, transforms, distances between copper shapes.

/// A point or vector (mm).
pub type P = [f64; 2];

pub fn add(a: P, b: P) -> P {
  [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: P, b: P) -> P {
  [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: P, s: f64) -> P {
  [a[0] * s, a[1] * s]
}

pub fn dot(a: P, b: P) -> f64 {
  a[0] * b[0] + a[1] * b[1]
}

pub fn cross(a: P, b: P) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

pub fn len(a: P) -> f64 {
  a[0].hypot(a[1])
}

pub fn dist(a: P, b: P) -> f64 {
  len(sub(a, b))
}

/// Rotates `p` about the origin by `deg` degrees counter-clockwise; multiples of 90° are exact.
pub fn rot(p: P, deg: f64) -> P {
  let d = deg.rem_euclid(360.0);
  for (k, (c, s)) in [(0.0, (1.0, 0.0)), (90.0, (0.0, 1.0)), (180.0, (-1.0, 0.0)), (270.0, (0.0, -1.0))] {
    if (d - k).abs() < 1e-9 {
      return [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    }
  }
  let (s, c) = d.to_radians().sin_cos();
  [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// A placement: optional mirror in X (a part on the bottom side, seen from the top), then a
/// rotation, then a translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xf {
  pub at: P,
  pub rot: f64,
  pub mirror: bool,
}

impl Xf {
  pub const ID: Xf = Xf { at: [0.0, 0.0], rot: 0.0, mirror: false };

  pub fn apply(&self, p: P) -> P {
    let p = if self.mirror { [-p[0], p[1]] } else { p };
    add(rot(p, self.rot), self.at)
  }

  /// An angle (deg) carried through the transform.
  pub fn angle(&self, a: f64) -> f64 {
    let a = if self.mirror { 180.0 - a } else { a };
    (a + self.rot).rem_euclid(360.0)
  }
}

pub fn point_seg_dist(p: P, a: P, b: P) -> f64 {
  let ab = sub(b, a);
  let l2 = dot(ab, ab);
  if l2 < 1e-18 {
    return dist(p, a);
  }
  let t = (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0);
  dist(p, add(a, scale(ab, t)))
}

/// Whether segments ab and cd cross or touch.
pub fn segs_intersect(a: P, b: P, c: P, d: P) -> bool {
  let d1 = cross(sub(b, a), sub(c, a));
  let d2 = cross(sub(b, a), sub(d, a));
  let d3 = cross(sub(d, c), sub(a, c));
  let d4 = cross(sub(d, c), sub(b, c));
  if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
    return true;
  }
  const E: f64 = 1e-12;
  (d1.abs() < E && point_seg_dist(c, a, b) < 1e-9)
    || (d2.abs() < E && point_seg_dist(d, a, b) < 1e-9)
    || (d3.abs() < E && point_seg_dist(a, c, d) < 1e-9)
    || (d4.abs() < E && point_seg_dist(b, c, d) < 1e-9)
}

pub fn seg_seg_dist(a: P, b: P, c: P, d: P) -> f64 {
  if segs_intersect(a, b, c, d) {
    return 0.0;
  }
  point_seg_dist(a, c, d).min(point_seg_dist(b, c, d)).min(point_seg_dist(c, a, b)).min(point_seg_dist(d, a, b))
}

/// Even-odd point in polygon.
pub fn point_in_poly(p: P, poly: &[P]) -> bool {
  let mut inside = false;
  let n = poly.len();
  for i in 0..n {
    let (a, b) = (poly[i], poly[(i + 1) % n]);
    if (a[1] > p[1]) != (b[1] > p[1]) {
      let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
      if p[0] < x {
        inside = !inside;
      }
    }
  }
  inside
}

/// The polygon's edges as (a, b) pairs, closing back to the start.
pub fn edges(poly: &[P]) -> impl Iterator<Item = (P, P)> + '_ {
  (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()]))
}

/// Distance from a point to a polygon's boundary.
pub fn point_poly_edge_dist(p: P, poly: &[P]) -> f64 {
  edges(poly).map(|(a, b)| point_seg_dist(p, a, b)).fold(f64::INFINITY, f64::min)
}

/// Signed area (counter-clockwise positive).
pub fn area(poly: &[P]) -> f64 {
  edges(poly).map(|(a, b)| cross(a, b)).sum::<f64>() / 2.0
}

/// Axis-aligned bounds of points.
pub fn bounds(pts: impl IntoIterator<Item = P>) -> Option<(P, P)> {
  let mut it = pts.into_iter();
  let first = it.next()?;
  Some(it.fold((first, first), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])))
}

/// A piece of copper (or any filled shape): a capsule (a segment swept by a disc, which covers
/// round pads, vias and tracks) or a convex or concave polygon.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape2 {
  Capsule { a: P, b: P, r: f64 },
  Poly(Vec<P>),
}

impl Shape2 {
  pub fn circle(c: P, d: f64) -> Shape2 {
    Shape2::Capsule { a: c, b: c, r: d / 2.0 }
  }

  /// A w×h rectangle centred at `c`, rotated by `deg`.
  pub fn rect(c: P, size: P, deg: f64) -> Shape2 {
    let (hx, hy) = (size[0] / 2.0, size[1] / 2.0);
    Shape2::Poly([[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]].iter().map(|&p| add(rot(p, deg), c)).collect())
  }

  /// An obround (stadium) w×h centred at `c`, rotated by `deg`.
  pub fn oval(c: P, size: P, deg: f64) -> Shape2 {
    let (w, h) = (size[0], size[1]);
    if (w - h).abs() < 1e-9 {
      return Shape2::circle(c, w);
    }
    let (half, r) = if w > h { (([(w - h) / 2.0, 0.0]), h / 2.0) } else { ([0.0, (h - w) / 2.0], w / 2.0) };
    Shape2::Capsule { a: add(c, rot(scale(half, -1.0), deg)), b: add(c, rot(half, deg)), r }
  }

  /// Grown by `d` on every side (polygons keep their corners: a slight over-estimate of the
  /// true offset, safe for clearance checks).
  pub fn grown(&self, d: f64) -> Shape2 {
    match self {
      Shape2::Capsule { a, b, r } => Shape2::Capsule { a: *a, b: *b, r: r + d },
      Shape2::Poly(pts) => {
        let c = scale(pts.iter().fold([0.0, 0.0], |s, p| add(s, *p)), 1.0 / pts.len().max(1) as f64);
        Shape2::Poly(
          pts
            .iter()
            .map(|p| {
              let v = sub(*p, c);
              let l = len(v).max(1e-12);
              // corners move out along the diagonal by d·√2 at most
              add(*p, scale(v, d * std::f64::consts::SQRT_2 / l))
            })
            .collect(),
        )
      }
    }
  }

  pub fn bbox(&self) -> (P, P) {
    match self {
      Shape2::Capsule { a, b, r } => {
        ([a[0].min(b[0]) - r, a[1].min(b[1]) - r], [a[0].max(b[0]) + r, a[1].max(b[1]) + r])
      }
      Shape2::Poly(pts) => bounds(pts.iter().copied()).unwrap_or(([0.0; 2], [0.0; 2])),
    }
  }

  pub fn contains(&self, p: P) -> bool {
    match self {
      Shape2::Capsule { a, b, r } => point_seg_dist(p, *a, *b) <= *r,
      Shape2::Poly(pts) => point_in_poly(p, pts),
    }
  }

  /// A point surely inside the shape.
  pub fn anchor(&self) -> P {
    match self {
      Shape2::Capsule { a, b, .. } => scale(add(*a, *b), 0.5),
      Shape2::Poly(pts) => scale(pts.iter().fold([0.0, 0.0], |s, p| add(s, *p)), 1.0 / pts.len().max(1) as f64),
    }
  }
}

/// Gap between two shapes' edges; 0 when they touch or overlap.
pub fn gap(s: &Shape2, t: &Shape2) -> f64 {
  match (s, t) {
    (Shape2::Capsule { a, b, r }, Shape2::Capsule { a: c, b: d, r: r2 }) => {
      (seg_seg_dist(*a, *b, *c, *d) - r - r2).max(0.0)
    }
    (Shape2::Capsule { a, b, r }, Shape2::Poly(p)) | (Shape2::Poly(p), Shape2::Capsule { a, b, r }) => {
      if point_in_poly(*a, p) || point_in_poly(*b, p) {
        return 0.0;
      }
      (edges(p).map(|(c, d)| seg_seg_dist(*a, *b, c, d)).fold(f64::INFINITY, f64::min) - r).max(0.0)
    }
    (Shape2::Poly(p), Shape2::Poly(q)) => {
      if p.first().is_some_and(|v| point_in_poly(*v, q)) || q.first().is_some_and(|v| point_in_poly(*v, p)) {
        return 0.0;
      }
      let mut m = f64::INFINITY;
      for (a, b) in edges(p) {
        for (c, d) in edges(q) {
          m = m.min(seg_seg_dist(a, b, c, d));
        }
      }
      m
    }
  }
}

/// Gap between a shape and a polygon's boundary line (for copper-to-edge checks), and whether
/// the shape lies inside the polygon.
pub fn gap_to_outline(s: &Shape2, outline: &[P]) -> (f64, bool) {
  let inside = point_in_poly(s.anchor(), outline);
  let d = match s {
    Shape2::Capsule { a, b, r } => {
      edges(outline).map(|(c, d)| seg_seg_dist(*a, *b, c, d)).fold(f64::INFINITY, f64::min) - r
    }
    Shape2::Poly(p) => {
      let mut m = f64::INFINITY;
      for (a, b) in edges(p) {
        for (c, d) in edges(outline) {
          m = m.min(seg_seg_dist(a, b, c, d));
        }
      }
      m
    }
  };
  (d.max(0.0), inside)
}

/// Points along an arc from `start` to `end` degrees (counter-clockwise when end > start).
pub fn arc_points(c: P, r: f64, start: f64, end: f64, max_step_deg: f64) -> Vec<P> {
  let n = (((end - start).abs() / max_step_deg).ceil() as usize).max(1);
  (0..=n)
    .map(|i| {
      let a = (start + (end - start) * i as f64 / n as f64).to_radians();
      [c[0] + r * a.cos(), c[1] + r * a.sin()]
    })
    .collect()
}

/// The circle through three points: (centre, radius), or None when they're collinear.
pub fn circle3(a: P, b: P, c: P) -> Option<(P, f64)> {
  let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  if d.abs() < 1e-12 {
    return None;
  }
  let (a2, b2, c2) = (dot(a, a), dot(b, b), dot(c, c));
  let ux = (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d;
  let uy = (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d;
  Some(([ux, uy], dist([ux, uy], a)))
}

/// Start and end angles (deg) of the arc from `a` through `m` to `b`, with end > start when
/// the arc runs counter-clockwise.
pub fn arc3_angles(c: P, a: P, m: P, b: P) -> (f64, f64) {
  let ang = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]).to_degrees();
  let (sa, sm, sb) = (ang(a), ang(m), ang(b));
  let ccw = |x: f64, y: f64| (y - x).rem_euclid(360.0);
  if ccw(sa, sm) <= ccw(sa, sb) { (sa, sa + ccw(sa, sb)) } else { (sa, sa - ccw(sb, sa)) }
}

/// Rounds to a grid step.
pub fn snap(p: P, grid: f64) -> P {
  if grid <= 0.0 {
    return p;
  }
  [(p[0] / grid).round() * grid, (p[1] / grid).round() * grid]
}

pub fn near(a: P, b: P) -> bool {
  dist(a, b) < 1e-4
}
