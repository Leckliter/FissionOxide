//! Final checks on a layer's fitted contours: orientation and intersections.
//!
//! Every contour is flattened finely and its edges are tested against all others of the layer,
//! itself included, through a uniform grid. A piece that crosses or touches anything (or a
//! contour whose orientation flipped) is fitted again more tightly: first as a polyline within
//! [`FINE`] px, then as the exact pixel outline, which never touches anything by construction.

use std::f64::consts::TAU;

use crate::fit::{Ctx, Prepared, add, arc_sweep, bezier, cross, dist, dot, mul, sub};
use crate::{Contour, Options, P, Seg};

/// Tolerance of the first fallback.
const FINE: f64 = 0.25;
/// Largest distance between a curve and its flattening, px.
const FLAT_EPS: f64 = 0.02;
/// Grid cell size, px.
const CELL: f64 = 4.0;
/// Fallback levels: 0 fitted, 1 fine polyline, 2 pixel outline.
const LAST_LEVEL: u8 = 2;

pub(crate) fn finish(prepared: &[Prepared], options: &Options) -> Vec<Contour> {
  let mut levels: Vec<Vec<u8>> = prepared.iter().map(|p| vec![0; p.pieces.len()]).collect();
  let mut built = build(prepared, &levels, options);
  for _ in 0..=LAST_LEVEL {
    let bad = conflicts(prepared, &built);
    let mut bumped = false;
    for (lv, flags) in levels.iter_mut().zip(&bad) {
      for (l, &f) in lv.iter_mut().zip(flags) {
        if f && *l < LAST_LEVEL {
          eprintln!("DEBUGBUMP to {}", *l + 1);
          *l += 1;
          bumped = true;
        }
      }
    }
    if !bumped {
      break;
    }
    built = build(prepared, &levels, options);
  }
  built
    .into_iter()
    .zip(prepared)
    .map(|(segs, p)| Contour { segs: segs.into_iter().map(|(s, _)| s).collect(), hole: p.hole })
    .collect()
}

/// Each contour's segments at its pieces' current levels, tagged with the piece index.
fn build(prepared: &[Prepared], levels: &[Vec<u8>], options: &Options) -> Vec<Vec<(Seg, usize)>> {
  prepared
    .iter()
    .zip(levels)
    .map(|(p, lv)| {
      let ctx = Ctx { s: &p.s, corner: &p.corner, tol: options.tolerance, fit: options.fit };
      let mut segs = Vec::new();
      for (k, (piece, &l)) in p.pieces.iter().zip(lv).enumerate() {
        match l {
          0 => segs.push((piece.seg, k)),
          1 => segs.extend(
            ctx
              .dp_ranges(piece.i, piece.j, options.tolerance.min(FINE))
              .into_iter()
              .map(|(i, j)| (Seg::Line { a: ctx.at(i), b: ctx.at(j) }, k)),
          ),
          _ => segs.extend(ctx.raw(piece.i, piece.j).into_iter().map(|s| (s, k))),
        }
      }
      segs
    })
    .collect()
}

/// Points along `seg` after its start, ending exactly at its end. `hole` sets a circle's
/// direction.
pub(crate) fn flatten(seg: &Seg, hole: bool, out: &mut Vec<P>) {
  let steps = |r: f64, sweep: f64| {
    let step = if r > FLAT_EPS { 2.0 * (1.0 - FLAT_EPS / r).acos() } else { TAU };
    ((sweep / step).ceil() as usize).clamp(2, 4096)
  };
  match *seg {
    Seg::Line { b, .. } => out.push(b),
    Seg::Arc { a, b, center, ccw } => {
      let r = dist(a, center);
      let sweep = arc_sweep(a, b, center, ccw);
      let n = steps(r, sweep);
      let a0 = (a[1] - center[1]).atan2(a[0] - center[0]);
      let sign = if ccw { 1.0 } else { -1.0 };
      for k in 1..n {
        let t = a0 + sign * sweep * k as f64 / n as f64;
        out.push([center[0] + r * t.cos(), center[1] + r * t.sin()]);
      }
      out.push(b);
    }
    Seg::Cubic { a, c1, c2, b } => {
      let c = [a, c1, c2, b];
      let d1 = sub(add(a, c2), mul(c1, 2.0));
      let d2 = sub(add(c1, b), mul(c2, 2.0));
      let dd = d1[0].hypot(d1[1]).max(d2[0].hypot(d2[1]));
      let n = ((0.75 * dd / FLAT_EPS).sqrt().ceil() as usize).clamp(2, 4096);
      for k in 1..n {
        out.push(bezier(&c, k as f64 / n as f64));
      }
      out.push(b);
    }
    Seg::Circle { center, radius } => {
      let n = steps(radius, TAU).max(8);
      let sign = if hole { -1.0 } else { 1.0 };
      for k in 1..n {
        let t = sign * TAU * k as f64 / n as f64;
        out.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
      }
      out.push(seg.end());
    }
  }
}

struct Edge {
  a: P,
  b: P,
  contour: usize,
  /// Position in the contour's cyclic edge sequence.
  k: usize,
  piece: usize,
}

/// Flags, per contour and piece, those that intersect or touch anything.
fn conflicts(prepared: &[Prepared], built: &[Vec<(Seg, usize)>]) -> Vec<Vec<bool>> {
  let mut bad: Vec<Vec<bool>> = prepared.iter().map(|p| vec![false; p.pieces.len()]).collect();
  let mut edges: Vec<Edge> = Vec::new();
  let mut counts = Vec::with_capacity(built.len());
  let mut pts = Vec::new();
  for (c, segs) in built.iter().enumerate() {
    let hole = prepared[c].hole;
    let first = edges.len();
    let mut area = 0.0;
    if let Some((s, _)) = segs.first() {
      let mut prev = s.start();
      for &(seg, piece) in segs {
        pts.clear();
        flatten(&seg, hole, &mut pts);
        for &p in &pts {
          if p == prev {
            continue;
          }
          area += cross(prev, p);
          edges.push(Edge { a: prev, b: p, contour: c, k: edges.len() - first, piece });
          prev = p;
        }
      }
    }
    let n = edges.len() - first;
    counts.push(n);
    if n < 3 || !(area != 0.0) || (area < 0.0) != hole {
      bad[c].iter_mut().for_each(|f| *f = true);
    }
  }
  if edges.is_empty() {
    return bad;
  }
  let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
  for e in &edges {
    for p in [e.a, e.b] {
      for d in 0..2 {
        lo[d] = lo[d].min(p[d]);
        hi[d] = hi[d].max(p[d]);
      }
    }
  }
  let nx = ((hi[0] - lo[0]) / CELL) as u64 + 1;
  let cell = |v: f64, d: usize| ((v - lo[d]) / CELL) as u64;
  let mut keys: Vec<(u64, u32)> = Vec::with_capacity(edges.len() * 2);
  for (id, e) in edges.iter().enumerate() {
    let (x0, x1) = (cell(e.a[0].min(e.b[0]), 0), cell(e.a[0].max(e.b[0]), 0));
    let (y0, y1) = (cell(e.a[1].min(e.b[1]), 1), cell(e.a[1].max(e.b[1]), 1));
    for y in y0..=y1 {
      for x in x0..=x1 {
        keys.push((y * nx + x, id as u32));
      }
    }
  }
  keys.sort_unstable();
  for group in keys.chunk_by(|a, b| a.0 == b.0) {
    for (gi, &(_, ea)) in group.iter().enumerate() {
      for &(_, eb) in &group[gi + 1..] {
        let (e1, e2) = (&edges[ea as usize], &edges[eb as usize]);
        let clash = if e1.contour == e2.contour && adjacent(e1.k, e2.k, counts[e1.contour]) {
          folds(e1, e2)
        } else {
          touches(e1.a, e1.b, e2.a, e2.b)
        };
        if clash {
          bad[e1.contour][e1.piece] = true;
          bad[e2.contour][e2.piece] = true;
        }
      }
    }
  }
  bad
}

fn adjacent(a: usize, b: usize, n: usize) -> bool {
  let d = (a + n - b) % n;
  d == 1 || d == n - 1
}

/// Consecutive edges that double back over each other.
fn folds(e1: &Edge, e2: &Edge) -> bool {
  let (a, b, c) = if e1.b == e2.a { (e1.a, e1.b, e2.b) } else { (e2.a, e2.b, e1.b) };
  let (u, v) = (sub(b, a), sub(c, b));
  cross(u, v).abs() <= 1e-9 * dist(a, b) * dist(b, c) && dot(u, v) < 0.0
}

/// Whether segments `p1p2` and `q1q2` share any point.
pub(crate) fn touches(p1: P, p2: P, q1: P, q2: P) -> bool {
  const E: f64 = 1e-9;
  if p1[0].max(p2[0]) < q1[0].min(q2[0]) - E
    || q1[0].max(q2[0]) < p1[0].min(p2[0]) - E
    || p1[1].max(p2[1]) < q1[1].min(q2[1]) - E
    || q1[1].max(q2[1]) < p1[1].min(p2[1]) - E
  {
    return false;
  }
  let (lp, lq) = (dist(p1, p2), dist(q1, q2));
  let sign = |v: f64, eps: f64| {
    if v > eps {
      1
    } else if v < -eps {
      -1
    } else {
      0
    }
  };
  let s1 = sign(cross(sub(q2, q1), sub(p1, q1)), E * lq);
  let s2 = sign(cross(sub(q2, q1), sub(p2, q1)), E * lq);
  let s3 = sign(cross(sub(p2, p1), sub(q1, p1)), E * lp);
  let s4 = sign(cross(sub(p2, p1), sub(q2, p1)), E * lp);
  if s1 * s2 < 0 && s3 * s4 < 0 {
    return true;
  }
  let within = |a: P, b: P, p: P| {
    p[0] >= a[0].min(b[0]) - E && p[0] <= a[0].max(b[0]) + E && p[1] >= a[1].min(b[1]) - E && p[1] <= a[1].max(b[1]) + E
  };
  (s1 == 0 && within(q1, q2, p1))
    || (s2 == 0 && within(q1, q2, p2))
    || (s3 == 0 && within(p1, p2, q1))
    || (s4 == 0 && within(p1, p2, q2))
}
