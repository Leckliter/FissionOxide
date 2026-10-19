//! Closed profiles (regions) of a sketch, the things Extrude and Revolve consume.
//!
//! Non-construction lines and arcs form a planar graph (endpoints that coincide are the same
//! vertex). Dangling edges are pruned, then faces are traced with the "turn most clockwise"
//! rule: bounded faces come out counter-clockwise, each component's outer boundary clockwise.
//! Full circles are their own components. A region is a bounded face minus the outer boundaries
//! of the components directly inside it — a rectangle with a circle in it yields the ring and
//! the disc, as in Fusion.
//!
//! Curves that cross without sharing an endpoint are not split (yet): draw them meeting at
//! endpoints.

use std::collections::HashMap;
use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::{CurveKind, Sketch, angle_of, ccw_sweep, dist};

/// Points closer than this are the same graph vertex.
const MERGE_TOL: f64 = 1e-6;
/// How far a computed tangent meeting point may sit from a curve end lying on the other curve and
/// still be taken as that end (a tangency known to 1e-9 is only known to ~1e-4 along the curve).
const TANGENT_SLACK: f64 = 1e-3;
/// Segments per full turn when an arc is flattened for area / containment tests.
const ARC_STEPS: f64 = 96.0;
/// Hostile-input guard on edges considered.
const MAX_EDGES: usize = 20_000;

/// One boundary piece of a loop, in traversal order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Seg {
  Line {
    a: [f64; 2],
    b: [f64; 2],
  },
  /// From `start_angle`, sweeping `sweep` radians (positive = counter-clockwise).
  Arc {
    center: [f64; 2],
    radius: f64,
    start_angle: f64,
    sweep: f64,
  },
  /// A whole circle starting at angle 0, counter-clockwise when `ccw`.
  Circle {
    center: [f64; 2],
    radius: f64,
    ccw: bool,
  },
  /// A cubic Bézier piece of a spline (control points, start to end).
  Bezier {
    p: [[f64; 2]; 4],
  },
}

impl Seg {
  pub fn start(&self) -> [f64; 2] {
    match *self {
      Seg::Line { a, .. } => a,
      Seg::Arc { center, radius, start_angle, .. } => polar(center, radius, start_angle),
      Seg::Circle { center, radius, .. } => [center[0] + radius, center[1]],
      Seg::Bezier { p } => p[0],
    }
  }

  pub fn end(&self) -> [f64; 2] {
    match *self {
      Seg::Line { b, .. } => b,
      Seg::Arc { center, radius, start_angle, sweep } => polar(center, radius, start_angle + sweep),
      Seg::Circle { center, radius, .. } => [center[0] + radius, center[1]],
      Seg::Bezier { p } => p[3],
    }
  }

  /// A point on the segment between its ends (for arcs: the middle of the sweep).
  pub fn mid(&self) -> [f64; 2] {
    match *self {
      Seg::Line { a, b } => [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
      Seg::Arc { center, radius, start_angle, sweep } => polar(center, radius, start_angle + sweep / 2.0),
      Seg::Circle { center, radius, ccw } => polar(center, radius, if ccw { TAU / 2.0 } else { -TAU / 2.0 }),
      Seg::Bezier { p } => crate::bezier_at(&p, 0.5),
    }
  }

  pub fn reversed(&self) -> Seg {
    match *self {
      Seg::Line { a, b } => Seg::Line { a: b, b: a },
      Seg::Arc { center, radius, start_angle, sweep } => {
        Seg::Arc { center, radius, start_angle: start_angle + sweep, sweep: -sweep }
      }
      Seg::Circle { center, radius, ccw } => Seg::Circle { center, radius, ccw: !ccw },
      Seg::Bezier { p } => Seg::Bezier { p: [p[3], p[2], p[1], p[0]] },
    }
  }

  /// Flattened points from start to end (start included, end excluded).
  pub fn flatten(&self, out: &mut Vec<[f64; 2]>) {
    match *self {
      Seg::Line { a, .. } => out.push(a),
      Seg::Arc { center, radius, start_angle, sweep } => {
        let n = ((sweep.abs() / TAU) * ARC_STEPS).ceil().max(2.0) as usize;
        for i in 0..n {
          out.push(polar(center, radius, start_angle + sweep * i as f64 / n as f64));
        }
      }
      Seg::Circle { center, radius, ccw } => {
        let n = ARC_STEPS as usize;
        let s = if ccw { 1.0 } else { -1.0 };
        for i in 0..n {
          out.push(polar(center, radius, s * TAU * i as f64 / n as f64));
        }
      }
      Seg::Bezier { p } => {
        for i in 0..16 {
          out.push(crate::bezier_at(&p, i as f64 / 16.0));
        }
      }
    }
  }
}

/// A closed chain of segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loop {
  pub segs: Vec<Seg>,
}

impl Loop {
  pub fn polygon(&self) -> Vec<[f64; 2]> {
    let mut pts = Vec::new();
    for s in &self.segs {
      s.flatten(&mut pts);
    }
    pts
  }

  /// Signed area (positive = counter-clockwise).
  pub fn signed_area(&self) -> f64 {
    polygon_area(&self.polygon())
  }

  pub fn reversed(&self) -> Loop {
    Loop { segs: self.segs.iter().rev().map(|s| s.reversed()).collect() }
  }

  pub fn contains(&self, p: [f64; 2]) -> bool {
    point_in_polygon(&self.polygon(), p)
  }
}

/// A region: counter-clockwise outer loop and clockwise holes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
  pub outer: Loop,
  pub holes: Vec<Loop>,
}

impl Profile {
  pub fn area(&self) -> f64 {
    self.outer.signed_area() + self.holes.iter().map(|h| h.signed_area()).sum::<f64>()
  }

  pub fn contains(&self, p: [f64; 2]) -> bool {
    self.outer.contains(p) && !self.holes.iter().any(|h| h.contains(p))
  }

  /// A point strictly inside the region (used to refer to it robustly across edits).
  pub fn interior_point(&self) -> [f64; 2] {
    let mut polys = vec![self.outer.polygon()];
    polys.extend(self.holes.iter().map(|h| h.polygon()));
    let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in polys.iter().flatten() {
      ymin = ymin.min(p[1]);
      ymax = ymax.max(p[1]);
    }
    // Scan a few horizontal lines; take the midpoint of the widest inside interval.
    let mut best: Option<([f64; 2], f64)> = None;
    for k in 1..16 {
      let y = ymin + (ymax - ymin) * (k as f64 / 16.0 + 0.0123);
      let mut xs = Vec::new();
      for poly in &polys {
        for i in 0..poly.len() {
          let a = poly[i];
          let b = poly[(i + 1) % poly.len()];
          if (a[1] > y) != (b[1] > y) {
            xs.push(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
          }
        }
      }
      xs.sort_by(f64::total_cmp);
      for pair in xs.chunks(2) {
        if let [x0, x1] = pair {
          let w = x1 - x0;
          if best.is_none_or(|(_, bw)| w > bw) {
            best = Some(([(x0 + x1) / 2.0, y], w));
          }
        }
      }
    }
    best.map(|(p, _)| p).unwrap_or_else(|| self.outer.segs.first().map(|s| s.start()).unwrap_or([0.0, 0.0]))
  }
}

/// All closed regions of the sketch, largest first.
pub fn find_profiles(sketch: &Sketch) -> Vec<Profile> {
  let (comps, circles) = build_components(sketch);
  // Each "component" = (bounded faces, outer boundary loop, sample point)
  struct Comp {
    faces: Vec<Loop>,
    outer: Loop,
    sample: [f64; 2],
  }
  let mut all: Vec<Comp> = Vec::new();
  for c in comps {
    let mut faces = Vec::new();
    let mut outer: Option<(Loop, f64)> = None;
    for l in c {
      let a = l.signed_area();
      if a > 1e-12 {
        faces.push(l);
      } else if a < -1e-12 && outer.as_ref().is_none_or(|(_, oa)| a < *oa) {
        outer = Some((l, a));
      }
    }
    if let Some((outer, _)) = outer {
      let sample = outer.segs.first().map(|s| s.mid()).unwrap_or([0.0, 0.0]);
      all.push(Comp { faces, outer, sample });
    }
  }
  for (center, radius) in circles {
    let ccw = Loop { segs: vec![Seg::Circle { center, radius, ccw: true }] };
    let sample = [center[0] + radius, center[1]];
    all.push(Comp { faces: vec![ccw.clone()], outer: ccw.reversed(), sample });
  }

  let mut profiles = Vec::new();
  for (ci, c) in all.iter().enumerate() {
    for face in &c.faces {
      // components lying inside this face
      let inside: Vec<usize> = (0..all.len()).filter(|&di| di != ci && face.contains(all[di].sample)).collect();
      // keep only the immediate ones (not inside a face of another inside component)
      let holes: Vec<Loop> = inside
        .iter()
        .filter(|&&di| !inside.iter().any(|&ei| ei != di && all[ei].faces.iter().any(|f| f.contains(all[di].sample))))
        .map(|&di| all[di].outer.clone())
        .collect();
      profiles.push(Profile { outer: face.clone(), holes });
    }
  }
  profiles.sort_by(|a, b| b.area().total_cmp(&a.area()));
  profiles
}

/// Half-edge face tracing per connected component. Returns the traced loops of each component and
/// the full circles (centre, radius).
fn build_components(sketch: &Sketch) -> (Vec<Vec<Loop>>, Vec<([f64; 2], f64)>) {
  let mut verts: Vec<[f64; 2]> = Vec::new();
  let vid = |p: [f64; 2], verts: &mut Vec<[f64; 2]>| -> usize {
    if let Some(i) = verts.iter().position(|q| dist(*q, p) <= MERGE_TOL) {
      i
    } else {
      verts.push(p);
      verts.len() - 1
    }
  };
  // Edges as segments oriented u→v
  let mut edges: Vec<(usize, usize, Seg)> = Vec::new();
  let mut circles: Vec<([f64; 2], f64)> = Vec::new();
  for c in sketch.curves.iter().filter(|c| !c.construction).take(MAX_EDGES) {
    match &c.kind {
      CurveKind::Line { a, b } => {
        let (Some(a), Some(b)) = (sketch.pos(*a), sketch.pos(*b)) else { continue };
        if dist(a, b) <= MERGE_TOL {
          continue;
        }
        let (u, v) = (vid(a, &mut verts), vid(b, &mut verts));
        edges.push((u, v, Seg::Line { a, b }));
      }
      CurveKind::Circle { center, radius } => {
        if let Some(c) = sketch.pos(*center)
          && *radius > MERGE_TOL
        {
          circles.push((c, *radius));
        }
      }
      CurveKind::Arc { center, start, end } => {
        let (Some(c), Some(s), Some(e)) = (sketch.pos(*center), sketch.pos(*start), sketch.pos(*end)) else {
          continue;
        };
        let r = dist(c, s);
        if r <= MERGE_TOL {
          continue;
        }
        let a0 = angle_of(c, s);
        let sweep = ccw_sweep(a0, angle_of(c, e));
        let seg = Seg::Arc { center: c, radius: r, start_angle: a0, sweep };
        let (u, v) = (vid(seg.start(), &mut verts), vid(seg.end(), &mut verts));
        if u == v {
          // an arc closed on itself is a circle
          circles.push((c, r));
          continue;
        }
        edges.push((u, v, seg));
      }
      CurveKind::Spline { .. }
      | CurveKind::ControlSpline { .. }
      | CurveKind::Ellipse { .. }
      | CurveKind::Bezier { .. } => {
        for b in sketch.beziers(c.id) {
          if dist(b[0], b[3]) > MERGE_TOL {
            edges.push((0, 0, Seg::Bezier { p: b }));
          }
        }
      }
    }
  }
  // Curves that cross mid-span split each other there, as in Fusion (no trimming needed).
  let (split, rest) = split_at_crossings(edges.iter().map(|e| e.2).collect(), circles);
  circles = rest;
  verts.clear();
  edges.clear();
  for seg in split {
    let (u, v) = (vid(seg.start(), &mut verts), vid(seg.end(), &mut verts));
    if u != v {
      edges.push((u, v, seg));
    }
  }
  // Prune dangling edges repeatedly.
  let mut alive = vec![true; edges.len()];
  loop {
    let mut deg = vec![0usize; verts.len()];
    for (i, (u, v, _)) in edges.iter().enumerate() {
      if alive[i] {
        deg[*u] += 1;
        deg[*v] += 1;
      }
    }
    let mut changed = false;
    for (i, (u, v, _)) in edges.iter().enumerate() {
      if alive[i] && (deg[*u] < 2 || deg[*v] < 2) {
        alive[i] = false;
        changed = true;
      }
    }
    if !changed {
      break;
    }
  }
  // Half-edges: 2i = u→v, 2i+1 = v→u
  let half = |h: usize| -> (usize, usize, Seg) {
    let (u, v, s) = edges[h / 2];
    if h.is_multiple_of(2) { (u, v, s) } else { (v, u, s.reversed()) }
  };
  let mut out_by_vert: HashMap<usize, Vec<(f64, usize)>> = HashMap::new();
  for (i, _) in edges.iter().enumerate().filter(|(i, _)| alive[*i]) {
    for h in [2 * i, 2 * i + 1] {
      let (u, _, s) = half(h);
      let ang = departure_angle(&s);
      out_by_vert.entry(u).or_default().push((ang, h));
    }
  }
  for list in out_by_vert.values_mut() {
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
  }
  // next(h): at h's end vertex, the outgoing half-edge just clockwise of twin(h).
  let next = |h: usize| -> Option<usize> {
    let (_, v, _) = half(h);
    let twin = h ^ 1;
    let list = out_by_vert.get(&v)?;
    let pos = list.iter().position(|&(_, x)| x == twin)?;
    let prev = if pos == 0 { list.len() - 1 } else { pos - 1 };
    list.get(prev).map(|&(_, x)| x)
  };
  // Components via union-find over alive edges.
  let mut parent: Vec<usize> = (0..verts.len()).collect();
  fn find(p: &mut [usize], mut x: usize) -> usize {
    while p[x] != x {
      p[x] = p[p[x]];
      x = p[x];
    }
    x
  }
  for (i, (u, v, _)) in edges.iter().enumerate() {
    if alive[i] {
      let (a, b) = (find(&mut parent, *u), find(&mut parent, *v));
      parent[a] = b;
    }
  }
  let mut comp_loops: HashMap<usize, Vec<Loop>> = HashMap::new();
  let mut used = vec![false; edges.len() * 2];
  for start in 0..edges.len() * 2 {
    if used[start] || !alive[start / 2] {
      continue;
    }
    let mut segs = Vec::new();
    let mut h = start;
    let mut ok = false;
    for _ in 0..(edges.len() * 2 + 1) {
      used[h] = true;
      segs.push(half(h).2);
      match next(h) {
        Some(n) if n == start => {
          ok = true;
          break;
        }
        Some(n) if !used[n] => h = n,
        _ => break,
      }
    }
    if ok {
      let root = find(&mut parent, half(start).0);
      comp_loops.entry(root).or_default().push(Loop { segs });
    }
  }
  let mut comps: Vec<(usize, Vec<Loop>)> = comp_loops.into_iter().collect();
  comps.sort_by_key(|(k, _)| *k);
  (comps.into_iter().map(|(_, v)| v).collect(), circles)
}

/// Splits segments (and circles) where they cross each other's interiors. Circles crossed at two
/// or more points become arcs; the rest stay circles.
fn split_at_crossings(segs: Vec<Seg>, circles: Vec<([f64; 2], f64)>) -> (Vec<Seg>, Vec<([f64; 2], f64)>) {
  use crate::edit::{Geo, meet};
  // spline pieces don't split (yet); they join the graph as they are
  let (curved, segs): (Vec<Seg>, Vec<Seg>) = segs.into_iter().partition(|s| matches!(s, Seg::Bezier { .. }));
  let n_segs = segs.len();
  let mut geos: Vec<Geo> = segs.iter().filter_map(Geo::of_seg).collect();
  geos.extend(circles.iter().map(|(c, r)| Geo::Arc { c: *c, r: *r, a0: 0.0, sweep: TAU }));
  if geos.len() > 2000 {
    return (segs.into_iter().chain(curved).collect(), circles);
  }
  let mut cuts: Vec<Vec<f64>> = vec![Vec::new(); geos.len()];
  // ends of segment `k` lying on curve `h`
  let ends_on = |k: usize, h: &Geo| -> Vec<[f64; 2]> {
    if k >= n_segs {
      return Vec::new();
    }
    let g = &geos[k];
    [g.at(0.0), g.at(g.end_param())].into_iter().filter(|&p| h.contains(p) && h.distance(p) <= MERGE_TOL).collect()
  };
  for i in 0..geos.len() {
    for j in i + 1..geos.len() {
      // An end lying on the other curve splits it there. Near a tangency the computed meeting
      // point can sit well along the curve from that end (a solved tangent point is within a hair
      // of the circle but not of the exact tangency), so the end wins over a meeting point near it.
      let ends: Vec<[f64; 2]> = ends_on(i, &geos[j]).into_iter().chain(ends_on(j, &geos[i])).collect();
      let meets = meet(&geos[i], &geos[j]).into_iter().filter(|&p| !ends.iter().any(|&e| dist(e, p) <= TANGENT_SLACK));
      for p in meets.chain(ends.iter().copied()) {
        if geos[i].contains(p) && geos[j].contains(p) {
          cuts[i].push(geos[i].param(p));
          cuts[j].push(geos[j].param(p));
        }
      }
    }
  }
  let to_seg = |g: &Geo, t0: f64, t1: f64| -> Seg {
    match *g {
      Geo::Line { .. } => Seg::Line { a: g.at(t0), b: g.at(t1) },
      Geo::Arc { c, r, a0, .. } => Seg::Arc { center: c, radius: r, start_angle: a0 + t0, sweep: t1 - t0 },
    }
  };
  let mut out = Vec::new();
  let mut left = Vec::new();
  for (i, g) in geos.iter().enumerate() {
    let end = g.end_param();
    let tol = match *g {
      Geo::Line { a, b } => MERGE_TOL / dist(a, b).max(MERGE_TOL),
      Geo::Arc { r, .. } => MERGE_TOL / r.max(MERGE_TOL),
    };
    let mut ts: Vec<f64> = cuts[i].iter().copied().filter(|t| g.full() || (*t > tol && *t < end - tol)).collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|a, b| (*a - *b).abs() <= tol);
    if g.full() {
      if ts.len() > 1 && (ts[0] + TAU - ts[ts.len() - 1]).abs() <= tol {
        ts.pop();
      }
      if ts.len() < 2 {
        left.push(circles[i - n_segs]);
        continue;
      }
      for k in 0..ts.len() {
        let (t0, t1) = (ts[k], if k + 1 < ts.len() { ts[k + 1] } else { ts[0] + TAU });
        out.push(to_seg(g, t0, t1));
      }
      continue;
    }
    let mut prev = 0.0;
    for t in ts.into_iter().chain([end]) {
      if t - prev > tol {
        out.push(to_seg(g, prev, t));
      }
      prev = t;
    }
    if i < n_segs && out.last().is_none() {
      out.push(segs[i]);
    }
  }
  out.extend(curved);
  (out, left)
}

/// Direction a segment leaves its start, nudged along the curve so tangent line/arc pairs sort.
fn departure_angle(s: &Seg) -> f64 {
  match *s {
    Seg::Line { a, b } => angle_of(a, b),
    Seg::Arc { center, radius, start_angle, sweep } => {
      let p0 = polar(center, radius, start_angle);
      let p1 = polar(center, radius, start_angle + sweep * 1e-3);
      angle_of(p0, p1)
    }
    Seg::Circle { .. } => 0.0,
    Seg::Bezier { p } => {
      let q = if dist(p[0], p[1]) > 1e-12 {
        p[1]
      } else if dist(p[0], p[2]) > 1e-12 {
        p[2]
      } else {
        p[3]
      };
      angle_of(p[0], q)
    }
  }
}

fn polar(c: [f64; 2], r: f64, a: f64) -> [f64; 2] {
  [c[0] + r * a.cos(), c[1] + r * a.sin()]
}

pub(crate) fn polygon_area(p: &[[f64; 2]]) -> f64 {
  let n = p.len();
  let mut s = 0.0;
  for i in 0..n {
    let a = p[i];
    let b = p[(i + 1) % n];
    s += a[0] * b[1] - b[0] * a[1];
  }
  s / 2.0
}

pub(crate) fn point_in_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
  let mut inside = false;
  let n = poly.len();
  for i in 0..n {
    let a = poly[i];
    let b = poly[(i + 1) % n];
    if (a[1] > p[1]) != (b[1] > p[1]) {
      let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
      if p[0] < x {
        inside = !inside;
      }
    }
  }
  inside
}

#[cfg(test)]
mod tests {
  use crate::*;
  use std::f64::consts::PI;

  #[test]
  fn rectangle_is_one_profile() {
    let mut s = Sketch::new();
    s.add_rectangle([0.0, 0.0], [20.0, 10.0]).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 1);
    assert!((p[0].area() - 200.0).abs() < 1e-9);
    assert!(p[0].holes.is_empty());
  }

  #[test]
  fn rectangle_with_circle_gives_ring_and_disc() {
    let mut s = Sketch::new();
    s.add_rectangle([0.0, 0.0], [20.0, 10.0]).unwrap();
    s.add_circle(PtSpec::At(10.0, 5.0), 3.0).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 2);
    let disc = 9.0 * PI;
    assert!((p[0].area() - (200.0 - disc)).abs() < 0.1, "{}", p[0].area());
    assert_eq!(p[0].holes.len(), 1);
    assert!((p[1].area() - disc).abs() < 0.1);
    assert!(!p[0].contains([10.0, 5.0]));
    assert!(p[1].contains([10.0, 5.0]));
    assert!(p[0].contains(p[0].interior_point()));
  }

  #[test]
  fn split_rectangle_gives_two_faces() {
    let mut s = Sketch::new();
    let lines = s.add_rectangle([0.0, 0.0], [20.0, 10.0]).unwrap();
    // a line from the middle of... bottom and top are single edges, so split using shared points:
    // build an L-shape instead: rectangle + extra rectangle sharing an edge
    let tr = s.curve_points(lines[1])[1];
    let br = s.curve_points(lines[1])[0];
    let p = s.add_point(30.0, 0.0);
    let q = s.add_point(30.0, 10.0);
    s.add_curve(CurveKind::Line { a: br, b: p }).unwrap();
    s.add_curve(CurveKind::Line { a: p, b: q }).unwrap();
    s.add_curve(CurveKind::Line { a: q, b: tr }).unwrap();
    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2);
    assert!((profiles[0].area() - 200.0).abs() < 1e-9);
    assert!((profiles[1].area() - 100.0).abs() < 1e-9);
  }

  #[test]
  fn slot_profile() {
    let mut s = Sketch::new();
    s.add_slot([0.0, 0.0], [30.0, 0.0], 10.0).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 1);
    let expected = 30.0 * 10.0 + PI * 25.0;
    assert!((p[0].area() - expected).abs() < 0.2, "{}", p[0].area());
  }

  #[test]
  fn open_and_construction_geometry_is_ignored() {
    let mut s = Sketch::new();
    s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0)).unwrap();
    let c = s.add_circle(PtSpec::At(0.0, 0.0), 5.0).unwrap();
    s.set_construction(c, true).unwrap();
    assert!(find_profiles(&s).is_empty());
  }

  #[test]
  fn nested_islands() {
    let mut s = Sketch::new();
    s.add_rectangle([0.0, 0.0], [100.0, 100.0]).unwrap();
    s.add_rectangle([10.0, 10.0], [90.0, 90.0]).unwrap();
    s.add_circle(PtSpec::At(50.0, 50.0), 10.0).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 3);
    // outer frame has exactly one hole (the inner square), not the circle
    let frame = p.iter().find(|r| r.contains([5.0, 5.0])).unwrap();
    assert_eq!(frame.holes.len(), 1);
    let island = p.iter().find(|r| r.contains([20.0, 20.0])).unwrap();
    assert_eq!(island.holes.len(), 1);
    assert!((frame.area() - (10000.0 - 6400.0)).abs() < 1e-6);
  }
}

#[cfg(test)]
mod crossing_tests {
  use super::*;
  use crate::PtSpec;

  #[test]
  fn overlapping_rectangles_and_a_circle_on_a_line_make_regions() {
    let mut s = Sketch::new();
    s.add_rectangle([0.0, 0.0], [20.0, 10.0]).unwrap();
    s.add_rectangle([10.0, 5.0], [30.0, 15.0]).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 3, "two L-shapes and their overlap");
    let total: f64 = p.iter().map(|r| r.area()).sum();
    assert!((total - (200.0 + 200.0 - 50.0)).abs() < 1e-6);
    let mut s = Sketch::new();
    s.add_circle(PtSpec::At(0.0, 0.0), 10.0).unwrap();
    s.add_line(PtSpec::At(-20.0, 0.0), PtSpec::At(20.0, 0.0)).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 2, "two half discs");
    assert!(p.iter().all(|r| (r.area() - std::f64::consts::PI * 50.0).abs() < 0.5), "areas are of flattened arcs");
  }

  #[test]
  fn lines_ending_tangent_on_circles_make_a_belt() {
    // circles r 20 and r 10, 50 apart, with both outer tangent lines; the solver leaves tangent
    // points slid along the line by ~4e-5 (only ~4e-11 off the circle), so place them like that
    let (r1, r2, c2) = (20.0, 10.0, [50.0, 0.0]);
    let a = ((r1 - r2) / 50.0f64).acos();
    let mut s = Sketch::new();
    s.add_circle(PtSpec::At(0.0, 0.0), r1).unwrap();
    s.add_circle(PtSpec::At(c2[0], c2[1]), r2).unwrap();
    for sign in [1.0, -1.0] {
      let n = [a.cos(), sign * a.sin()];
      let p1 = [r1 * n[0], r1 * n[1]];
      let p2 = [c2[0] + r2 * n[0], c2[1] + r2 * n[1]];
      let l = dist(p1, p2);
      let t = [(p2[0] - p1[0]) / l, (p2[1] - p1[1]) / l];
      let slide = 4e-5;
      s.add_line(
        PtSpec::At(p1[0] - t[0] * slide, p1[1] - t[1] * slide),
        PtSpec::At(p2[0] + t[0] * slide, p2[1] + t[1] * slide),
      )
      .unwrap();
    }
    let p = find_profiles(&s);
    assert_eq!(p.len(), 3, "two discs and the band between them");
    assert!(p.iter().any(|r| r.contains([25.0, 0.0])), "the band is a region");
  }
}
