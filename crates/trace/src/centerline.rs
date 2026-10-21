//! Centre-line (skeleton) tracing: strokes become single open lines along their middle, for
//! engraving, V-carving and pen plotting.
//!
//! The thresholded mask is thinned to a one-pixel skeleton (Zhang–Suen, then the staircase
//! pixels that keep it two wide on diagonals are removed), the skeleton is read as a graph whose
//! nodes are free ends and junctions, short spurs are pruned, each remaining branch is smoothed,
//! its free ends are pushed back out to the ends of the stroke, and it is fitted with lines,
//! arcs and cubics like an outline. Branches meeting at a junction end at the same point
//! exactly (the junction's centre), so the sketch gets them connected.

use std::collections::HashMap;

use crate::fit::{bezier, dist, schneider, sub};
use crate::{Fit, Options, P, Seg, Stroke};

/// Offsets of the 8 neighbours in ring order: N, NE, E, SE, S, SW, W, NW (y up).
const RING: [(i64, i64); 8] = [(0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1)];
/// Points either side of a point used to measure the bend there.
const CORNER_REACH: usize = 3;
/// Moving-average half window, and passes, used to smooth a branch before fitting.
const SMOOTH_REACH: usize = 2;
const SMOOTH_PASSES: usize = 2;

struct Grid<'a> {
  m: &'a [bool],
  w: usize,
  h: usize,
}

impl Grid<'_> {
  fn at(&self, x: i64, y: i64) -> bool {
    x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.m[y as usize * self.w + x as usize]
  }
}

/// The 8 neighbours of `i` as bits, in [`RING`] order.
fn ring(m: &[bool], w: usize, h: usize, i: usize) -> [bool; 8] {
  let g = Grid { m, w, h };
  let (x, y) = ((i % w) as i64, (i / w) as i64);
  RING.map(|(dx, dy)| g.at(x + dx, y + dy))
}

/// Zhang–Suen thinning, in place.
fn thin(m: &mut [bool], w: usize, h: usize) {
  loop {
    let mut changed = false;
    for pass in 0..2 {
      let mut del = Vec::new();
      for i in 0..m.len() {
        if !m[i] {
          continue;
        }
        let n = ring(m, w, h, i);
        let b = n.iter().filter(|v| **v).count();
        if !(2..=6).contains(&b) {
          continue;
        }
        let a = (0..8).filter(|&k| !n[k] && n[(k + 1) % 8]).count();
        if a != 1 {
          continue;
        }
        let (p2, p4, p6, p8) = (n[0], n[2], n[4], n[6]);
        let ok =
          if pass == 0 { !(p2 && p4 && p6) && !(p4 && p6 && p8) } else { !(p2 && p4 && p8) && !(p2 && p6 && p8) };
        if ok {
          del.push(i);
        }
      }
      changed |= !del.is_empty();
      for i in del {
        m[i] = false;
      }
    }
    if !changed {
      break;
    }
  }
}

/// How many 8-connected groups the set neighbours of a pixel form.
fn groups(n: &[bool; 8]) -> usize {
  let mut parent: [usize; 8] = std::array::from_fn(|k| k);
  fn find(p: &mut [usize; 8], mut a: usize) -> usize {
    while p[a] != a {
      a = p[a];
    }
    a
  }
  let join = |p: &mut [usize; 8], a: usize, b: usize| {
    let (ra, rb) = (find(p, a), find(p, b));
    p[ra] = rb;
  };
  for k in 0..8 {
    if n[k] && n[(k + 1) % 8] {
      join(&mut parent, k, (k + 1) % 8);
    }
    // two orthogonal neighbours a quarter turn apart touch diagonally
    if k % 2 == 0 && n[k] && n[(k + 2) % 8] {
      join(&mut parent, k, (k + 2) % 8);
    }
  }
  (0..8).filter(|&k| n[k] && find(&mut parent, k) == k).count()
}

/// Removes the inside pixel of every L (two orthogonal neighbours a quarter turn apart) that
/// the skeleton doesn't need to stay connected, so diagonal runs are one pixel wide and every
/// skeleton pixel's neighbours are its path neighbours.
fn unstair(m: &mut [bool], w: usize, h: usize) {
  for i in 0..m.len() {
    if !m[i] {
      continue;
    }
    let n = ring(m, w, h, i);
    let count = n.iter().filter(|v| **v).count();
    let corner = (0..4).any(|q| n[2 * q] && n[(2 * q + 2) % 8]);
    if corner && count >= 2 && groups(&n) == 1 {
      m[i] = false;
    }
  }
}

/// Chamfer distance (1, √2) from each foreground pixel to the nearest background pixel or the
/// image border; a stroke `w` px wide has about `(w + 1) / 2` on its centre line.
fn distance(m: &[bool], w: usize, h: usize) -> Vec<f64> {
  let big = (w + h) as f64;
  let mut d: Vec<f64> = m.iter().map(|&v| if v { big } else { 0.0 }).collect();
  let s2 = std::f64::consts::SQRT_2;
  let get = |d: &[f64], x: i64, y: i64| {
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h { 0.0 } else { d[y as usize * w + x as usize] }
  };
  for y in 0..h as i64 {
    for x in 0..w as i64 {
      let i = y as usize * w + x as usize;
      if d[i] == 0.0 {
        continue;
      }
      let v = (get(&d, x - 1, y) + 1.0)
        .min(get(&d, x, y - 1) + 1.0)
        .min(get(&d, x - 1, y - 1) + s2)
        .min(get(&d, x + 1, y - 1) + s2);
      d[i] = d[i].min(v);
    }
  }
  for y in (0..h as i64).rev() {
    for x in (0..w as i64).rev() {
      let i = y as usize * w + x as usize;
      if d[i] == 0.0 {
        continue;
      }
      let v = (get(&d, x + 1, y) + 1.0)
        .min(get(&d, x, y + 1) + 1.0)
        .min(get(&d, x + 1, y + 1) + s2)
        .min(get(&d, x - 1, y + 1) + s2);
      d[i] = d[i].min(v);
    }
  }
  d
}

/// A skeleton branch: pixel indices from one node to another (or round a loop).
struct Branch {
  px: Vec<usize>,
  /// End nodes (indices into the node list); `None` for a loop without nodes.
  ends: Option<(usize, usize)>,
}

struct Node {
  /// Where branches ending here end: the centre of its pixels.
  at: P,
  /// A free end (one branch) rather than a junction.
  free: bool,
}

fn centre(i: usize, w: usize) -> P {
  [(i % w) as f64 + 0.5, (i / w) as f64 + 0.5]
}

/// Reads the skeleton as a graph of nodes (free ends, junction clusters) and branches.
fn graph(m: &[bool], w: usize, h: usize) -> (Vec<Node>, Vec<Branch>) {
  let g = Grid { m, w, h };
  let nbrs = |i: usize| -> Vec<usize> {
    let (x, y) = ((i % w) as i64, (i / w) as i64);
    RING
      .iter()
      .filter(|(dx, dy)| g.at(x + dx, y + dy))
      .map(|(dx, dy)| (y + dy) as usize * w + (x + dx) as usize)
      .collect()
  };
  let deg = |i: usize| nbrs(i).len();
  // nodes: each free end alone, junction pixels clustered by adjacency
  let mut node_of: HashMap<usize, usize> = HashMap::new();
  let mut nodes: Vec<Node> = Vec::new();
  for (i, &on) in m.iter().enumerate() {
    if !on || node_of.contains_key(&i) {
      continue;
    }
    match deg(i) {
      1 => {
        node_of.insert(i, nodes.len());
        nodes.push(Node { at: centre(i, w), free: true });
      }
      d if d >= 3 => {
        let id = nodes.len();
        let (mut stack, mut sum, mut n) = (vec![i], [0.0, 0.0], 0.0);
        node_of.insert(i, id);
        while let Some(j) = stack.pop() {
          let c = centre(j, w);
          sum = [sum[0] + c[0], sum[1] + c[1]];
          n += 1.0;
          for k in nbrs(j) {
            if deg(k) >= 3 && !node_of.contains_key(&k) {
              node_of.insert(k, id);
              stack.push(k);
            }
          }
        }
        nodes.push(Node { at: [sum[0] / n, sum[1] / n], free: false });
      }
      _ => {}
    }
  }
  let mut used = vec![false; m.len()];
  let mut branches = Vec::new();
  let mut seen_pairs: HashMap<(usize, usize), ()> = HashMap::new();
  let mut starts: Vec<usize> = node_of.keys().copied().collect();
  starts.sort_unstable();
  for &s in &starts {
    let a = node_of[&s];
    for first in nbrs(s) {
      if let Some(&b) = node_of.get(&first) {
        // two node pixels side by side: a branch between different nodes, once
        if b != a && seen_pairs.insert((s.min(first), s.max(first)), ()).is_none() {
          branches.push(Branch { px: vec![s, first], ends: Some((a, b)) });
        }
        continue;
      }
      if used[first] {
        continue;
      }
      let (mut px, mut prev, mut cur) = (vec![s, first], s, first);
      used[first] = true;
      loop {
        let next = nbrs(cur).into_iter().find(|&k| k != prev && (!used[k] || node_of.contains_key(&k)));
        let Some(next) = next else { break };
        px.push(next);
        if let Some(&b) = node_of.get(&next) {
          branches.push(Branch { px, ends: Some((a, b)) });
          px = Vec::new();
          break;
        }
        used[next] = true;
        (prev, cur) = (cur, next);
      }
      if !px.is_empty() {
        // ran into an already walked pixel (shouldn't happen on a clean skeleton): keep it open
        let b = nodes.len();
        nodes.push(Node { at: centre(cur, w), free: true });
        branches.push(Branch { px, ends: Some((a, b)) });
      }
    }
  }
  // what's left are loops without junctions or ends (an O)
  for i in 0..m.len() {
    if !m[i] || used[i] || node_of.contains_key(&i) {
      continue;
    }
    let (mut px, mut prev, mut cur) = (vec![i], usize::MAX, i);
    used[i] = true;
    while let Some(next) = nbrs(cur).into_iter().find(|&k| k != prev && !used[k]) {
      used[next] = true;
      px.push(next);
      (prev, cur) = (cur, next);
    }
    if px.len() >= 3 {
      px.push(i);
      branches.push(Branch { px, ends: None });
    }
  }
  (nodes, branches)
}

/// A branch as points, its ends at its nodes' centres.
struct Path {
  pts: Vec<P>,
  ends: Option<(usize, usize)>,
  /// Skeleton pixels, for the stroke width.
  px: Vec<usize>,
}

fn length(pts: &[P]) -> f64 {
  pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Drops spurs (branches from a free end to a junction shorter than `spur`) and joins the two
/// branches left at a former junction, until nothing changes.
fn prune(nodes: &mut [Node], mut paths: Vec<Path>, spur: f64) -> Vec<Path> {
  loop {
    let degree = |paths: &[Path], n: usize| {
      paths.iter().map(|p| p.ends.map_or(0, |(a, b)| usize::from(a == n) + usize::from(b == n))).sum::<usize>()
    };
    let before = paths.len();
    // a spur: from a free end to a junction, short. A junction with any other branch loses all
    // its spurs; one with nothing but spurs (a small +) keeps its two longest
    let spur_at = |p: &Path| match p.ends {
      Some((a, b)) if nodes[a].free != nodes[b].free && length(&p.pts) < spur => {
        Some(if nodes[a].free { b } else { a })
      }
      _ => None,
    };
    let mut left: Vec<usize> = (0..nodes.len()).map(|n| degree(&paths, n)).collect();
    let mut solid = vec![0usize; nodes.len()];
    for p in &paths {
      if spur_at(p).is_none()
        && let Some((a, b)) = p.ends
      {
        solid[a] += 1;
        solid[b] += 1;
      }
    }
    let mut order: Vec<usize> = (0..paths.len()).collect();
    order.sort_by(|&i, &j| length(&paths[i].pts).total_cmp(&length(&paths[j].pts)));
    let mut drop = vec![false; paths.len()];
    for i in order {
      let len = length(&paths[i].pts);
      match (paths[i].ends, spur_at(&paths[i])) {
        (Some((a, b)), _) if a == b => {
          // a tiny loop off a junction (a leftover of thinning)
          if len < spur.max(4.0) && left[a] >= 4 {
            drop[i] = true;
            left[a] -= 2;
          }
        }
        (_, Some(j)) if solid[j] > 0 || left[j] >= 3 => {
          drop[i] = true;
          left[j] -= 1;
        }
        _ => {}
      }
    }
    let mut k = 0;
    paths.retain(|_| {
      k += 1;
      !drop[k - 1]
    });
    let mut changed = paths.len() != before;
    for (n, node) in nodes.iter_mut().enumerate() {
      if !node.free && degree(&paths, n) == 1 {
        // all but one branch pruned: a free end now
        node.free = true;
        changed = true;
      }
    }
    // join branches at nodes left with exactly two of them
    let junctions: Vec<usize> = (0..nodes.len()).filter(|&n| !nodes[n].free).collect();
    for n in junctions {
      if nodes[n].free || degree(&paths, n) != 2 {
        continue;
      }
      let ix: Vec<usize> = paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.ends.is_some_and(|(a, b)| a == n || b == n))
        .map(|(i, _)| i)
        .collect();
      if ix.len() == 1 {
        // a loop from the node back to itself: a closed stroke
        let p = &mut paths[ix[0]];
        p.ends = None;
        changed = true;
        continue;
      }
      let (i, j) = (ix[0], ix[1]);
      let mut q = paths.remove(j.max(i));
      let mut p = paths.remove(j.min(i));
      // orient p to end at n and q to start at n
      if let Some((a, _)) = p.ends
        && a == n
      {
        p.pts.reverse();
        p.ends = p.ends.map(|(a, b)| (b, a));
      }
      if let Some((_, b)) = q.ends
        && b == n
      {
        q.pts.reverse();
        q.ends = q.ends.map(|(a, b)| (b, a));
      }
      let (pa, qb) = (p.ends.map_or(n, |e| e.0), q.ends.map_or(n, |e| e.1));
      p.pts.extend(q.pts.into_iter().skip(1));
      p.px.extend(q.px);
      p.ends = Some((pa, qb));
      nodes[n].free = true; // no longer a junction (and no branch ends here now)
      paths.push(p);
      changed = true;
    }
    if !changed {
      return paths;
    }
  }
}

/// Moving average over the inner points; the ends stay (a loop is smoothed all round).
fn smooth(pts: &mut [P], closed: bool) {
  let n = pts.len();
  if n < 3 {
    return;
  }
  for _ in 0..SMOOTH_PASSES {
    let src = pts.to_vec();
    let m = if closed { n - 1 } else { n };
    for (i, out) in pts.iter_mut().enumerate().take(m) {
      if !closed && (i == 0 || i + 1 == n) {
        continue;
      }
      let (mut s, mut c) = ([0.0, 0.0], 0.0);
      for k in -(SMOOTH_REACH as i64)..=SMOOTH_REACH as i64 {
        let j = i as i64 + k;
        let j = if closed {
          j.rem_euclid(m as i64) as usize
        } else if j < 0 || j >= n as i64 {
          continue;
        } else {
          j as usize
        };
        s = [s[0] + src[j][0], s[1] + src[j][1]];
        c += 1.0;
      }
      *out = [s[0] / c, s[1] / c];
    }
    if closed {
      pts[n - 1] = pts[0];
    }
  }
}

/// Pushes a free end out along its direction while it stays inside the stroke, at most `reach`.
fn extend(pts: &mut Vec<P>, at_end: bool, grid: &Grid, reach: f64) {
  if pts.len() < 2 || reach <= 0.0 {
    return;
  }
  let n = pts.len();
  let k = (n - 1).min(4);
  let (tip, back) = if at_end { (pts[n - 1], pts[n - 1 - k]) } else { (pts[0], pts[k]) };
  let d = sub(tip, back);
  let l = d[0].hypot(d[1]);
  if l < 1e-9 {
    return;
  }
  let u = [d[0] / l, d[1] / l];
  let mut t = 0.0;
  while t + 0.25 <= reach {
    let q = [tip[0] + u[0] * (t + 0.25), tip[1] + u[1] * (t + 0.25)];
    if !grid.at(q[0].floor() as i64, q[1].floor() as i64) {
      break;
    }
    t += 0.25;
  }
  if t > 0.0 {
    let q = [tip[0] + u[0] * t, tip[1] + u[1] * t];
    if at_end {
      pts.push(q);
    } else {
      pts.insert(0, q);
    }
  }
}

/// Bend angle (degrees, 180 = straight) at `i`, measured over `reach` points either side.
fn bend(q: &[P], i: usize, reach: usize) -> f64 {
  let r = reach.min(i).min(q.len() - 1 - i);
  if r == 0 {
    return 180.0;
  }
  let (a, b) = (sub(q[i - r], q[i]), sub(q[i + r], q[i]));
  let (la, lb) = (a[0].hypot(a[1]), b[0].hypot(b[1]));
  if la < 1e-9 || lb < 1e-9 {
    return 180.0;
  }
  ((a[0] * b[0] + a[1] * b[1]) / (la * lb)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Indices of sharp corners: bends under `limit` that are the sharpest nearby. Thinning rounds a
/// stroke's corners over about half its width, so `reach` grows with it.
fn corners(q: &[P], limit: f64, reach: usize) -> Vec<usize> {
  if limit <= 0.0 || q.len() < 2 * reach + 1 {
    return Vec::new();
  }
  let b: Vec<f64> = (0..q.len()).map(|i| bend(q, i, reach)).collect();
  let mut out: Vec<usize> = Vec::new();
  for i in reach..q.len() - reach {
    let lo = i.saturating_sub(reach);
    let hi = (i + reach).min(q.len() - 1);
    if b[i] < limit && (lo..=hi).all(|j| b[j] > b[i] || (b[j] == b[i] && j >= i)) {
      if out.last().is_some_and(|&l| i - l <= reach) {
        continue;
      }
      out.push(i);
    }
  }
  out
}

/// Where the lines `a`–`b` and `c`–`d` cross, if they aren't parallel.
fn intersect(a: P, b: P, c: P, d: P) -> Option<P> {
  let (u, v) = (sub(b, a), sub(d, c));
  let den = u[0] * v[1] - u[1] * v[0];
  if den.abs() < 1e-9 * u[0].hypot(u[1]) * v[0].hypot(v[1]) {
    return None;
  }
  let w = sub(c, a);
  let t = (w[0] * v[1] - w[1] * v[0]) / den;
  Some([a[0] + u[0] * t, a[1] + u[1] * t])
}

/// Fits `q` run by run between the corners `cs`; where two lines meet at a corner, their shared
/// point moves to where the lines cross (thinning rounded it). `wrap`: `q` is a loop starting
/// and ending at a corner.
fn fit_runs(q: &[P], cs: &[usize], reach: usize, wrap: bool, options: &Options) -> Vec<Seg> {
  let mut out = Vec::new();
  let mut joins: Vec<(usize, usize)> = Vec::new();
  let mut from = 0;
  for &k in cs.iter().chain(std::iter::once(&(q.len() - 1))) {
    if k > from {
      fit_run(&q[from..=k], options.tolerance, options.fit, reach, &mut out);
      if k + 1 < q.len() && !out.is_empty() {
        joins.push((out.len() - 1, k));
      }
      from = k;
    }
  }
  if wrap && out.len() >= 2 {
    joins.push((out.len() - 1, 0));
  }
  for (si, k) in joins {
    let next = (si + 1) % out.len();
    let (Seg::Line { a: a0, b: b0 }, Seg::Line { a: a1, b: b1 }) = (out[si], out[next]) else { continue };
    let n = q.len() - 1;
    let r = (2 * reach).min(n / 2).max(1);
    let (before, after) = if k == 0 { (q[n - r], q[r]) } else { (q[k.saturating_sub(r)], q[(k + r).min(n)]) };
    let (far0, far1) = (a0, b1);
    if dist(far0, before) < 1e-9 || dist(after, far1) < 1e-9 {
      continue;
    }
    if let Some(x) = intersect(far0, before, after, far1)
      && dist(x, b0) <= 2.0 * r as f64 + 1.0
    {
      out[si] = Seg::Line { a: a0, b: x };
      out[next] = Seg::Line { a: x, b: b1 };
      let _ = a1;
    }
  }
  out
}

fn chord_worst(q: &[P]) -> (f64, usize) {
  let (a, b) = (q[0], q[q.len() - 1]);
  let mut worst = (0.0, q.len() / 2);
  for (i, p) in q.iter().enumerate().take(q.len() - 1).skip(1) {
    let e = seg_dist(*p, a, b);
    if e > worst.0 {
      worst = (e, i);
    }
  }
  worst
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
  let d = sub(b, a);
  let l2 = d[0] * d[0] + d[1] * d[1];
  if l2 == 0.0 {
    return dist(p, a);
  }
  let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
  dist(p, [a[0] + d[0] * t, a[1] + d[1] * t])
}

fn circumcentre(a: P, b: P, c: P) -> Option<P> {
  let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  if d.abs() < 1e-9 {
    return None;
  }
  let (a2, b2, c2) = (a[0] * a[0] + a[1] * a[1], b[0] * b[0] + b[1] * b[1], c[0] * c[0] + c[1] * c[1]);
  Some([
    (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d,
    (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d,
  ])
}

/// An arc from the first to the last point through the middle one, if every point is within
/// `tol` of it and they run round it one way, less than a full turn.
fn arc(q: &[P], tol: f64) -> Option<Seg> {
  let (a, b) = (q[0], q[q.len() - 1]);
  let c = circumcentre(a, q[q.len() / 2], b)?;
  let r = dist(a, c);
  if r > 1e5 || q.iter().any(|p| (dist(*p, c) - r).abs() > tol) {
    return None;
  }
  let ang = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]);
  let mut total = 0.0;
  let mut sign = 0.0;
  for w in q.windows(2) {
    let d = (ang(w[1]) - ang(w[0]) + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
    if d.abs() < 1e-12 {
      continue;
    }
    if sign == 0.0 {
      sign = d.signum();
    } else if d.signum() != sign && d.abs() > 0.2 {
      return None;
    }
    total += d;
  }
  // arcs bulging less than the tolerance are lines; nearly whole circles are left to splits
  let sweep = total.abs();
  if !(1e-3..=1.9 * std::f64::consts::PI).contains(&sweep) || r * (1.0 - (sweep / 2.0).cos()) < tol {
    return None;
  }
  Some(Seg::Arc { a, b, center: c, ccw: total > 0.0 })
}

fn tangent(q: &[P], i: usize, forward: bool) -> P {
  let r = 3.min(q.len() - 1);
  let (p0, p1) = if forward { (q[i], q[(i + r).min(q.len() - 1)]) } else { (q[i], q[i.saturating_sub(r)]) };
  let d = sub(p1, p0);
  let l = d[0].hypot(d[1]).max(1e-12);
  [d[0] / l, d[1] / l]
}

/// Fits one run between corners (or a junction and a corner) within `tol`. The `trim` points at
/// each end of a long run may stray further from a line: thinning pulls a skeleton about near
/// its corners and junctions.
fn fit_run(q: &[P], tol: f64, fit: Fit, trim: usize, out: &mut Vec<Seg>) {
  if q.len() < 2 {
    return;
  }
  let (a, b) = (q[0], q[q.len() - 1]);
  let (worst, at) = chord_worst(q);
  // the line through the run's inner part, against which its ends may stray a little more
  let trimmed = q.len() > 6 * trim.max(1) && {
    let (ia, ib) = (q[trim], q[q.len() - 1 - trim]);
    let d = sub(ib, ia);
    let l = d[0].hypot(d[1]);
    let off = |p: &P| ((p[0] - ia[0]) * d[1] - (p[1] - ia[1]) * d[0]).abs() / l;
    l > 1e-9 && q[trim..q.len() - trim].iter().all(|p| off(p) <= tol) && q.iter().all(|p| off(p) <= 3.0 * tol)
  };
  if (worst <= tol || trimmed) && dist(a, b) > 1e-9 {
    out.push(Seg::Line { a, b });
    return;
  }
  if fit != Fit::Lines && q.len() >= 3 {
    if let Some(s) = arc(q, tol) {
      out.push(s);
      return;
    }
    if fit == Fit::Curves && q.len() >= 4 {
      let (c, err, _) = schneider(q, tangent(q, 0, true), tangent(q, q.len() - 1, false), tol);
      // the control polygon must not be degenerate or loop far outside the points
      let mid = bezier(&c, 0.5);
      if err <= tol && mid.iter().all(|v| v.is_finite()) {
        out.push(Seg::Cubic { a: c[0], c1: c[1], c2: c[2], b: c[3] });
        return;
      }
    }
  }
  if q.len() <= 2 {
    out.push(Seg::Line { a, b });
    return;
  }
  let k = at.clamp(1, q.len() - 2);
  fit_run(&q[..=k], tol, fit, trim, out);
  fit_run(&q[k..], tol, fit, trim, out);
}

/// Least-squares circle (Kåsa) through the points: centre and radius.
fn circle(q: &[P]) -> Option<(P, f64)> {
  let n = q.len() as f64;
  let (mx, my) = (q.iter().map(|p| p[0]).sum::<f64>() / n, q.iter().map(|p| p[1]).sum::<f64>() / n);
  let (mut suu, mut svv, mut suv, mut suuu, mut svvv, mut suvv, mut svuu) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
  for p in q {
    let (u, v) = (p[0] - mx, p[1] - my);
    suu += u * u;
    svv += v * v;
    suv += u * v;
    suuu += u * u * u;
    svvv += v * v * v;
    suvv += u * v * v;
    svuu += v * u * u;
  }
  let det = suu * svv - suv * suv;
  if det.abs() < 1e-12 {
    return None;
  }
  let (r1, r2) = (0.5 * (suuu + suvv), 0.5 * (svvv + svuu));
  let uc = (r1 * svv - r2 * suv) / det;
  let vc = (suu * r2 - suv * r1) / det;
  let r = (uc * uc + vc * vc + (suu + svv) / n).sqrt();
  Some(([mx + uc, my + vc], r))
}

/// Fits a branch of a stroke `width` px wide: split at corners, each run fitted.
fn fit_path(pts: &[P], closed: bool, width: f64, options: &Options) -> Vec<Seg> {
  let tol = options.tolerance;
  let reach = ((width / 2.0).round() as usize + 1).clamp(CORNER_REACH, 3 * CORNER_REACH);
  let cs = corners(pts, options.corner_angle, reach);
  if closed {
    let ring = &pts[..pts.len() - 1];
    if cs.is_empty()
      && options.fit != Fit::Lines
      && let Some((c, r)) = circle(ring)
      && r > tol
      && ring.iter().all(|p| (dist(*p, c) - r).abs() <= tol)
    {
      return vec![Seg::Circle { center: c, radius: r }];
    }
    // start at a corner (or anywhere), close back on the start point exactly
    let s = cs.first().copied().unwrap_or(0);
    let mut q: Vec<P> = ring[s..].iter().chain(&ring[..s]).copied().collect();
    q.push(q[0]);
    let mut cs = corners(&q, options.corner_angle, reach);
    let wrap = !cs.is_empty() || s != 0;
    if cs.is_empty() {
      cs.push(q.len() / 2);
    }
    return fit_runs(&q, &cs, reach, wrap && corners(pts, options.corner_angle, reach).first() == Some(&s), options);
  }
  fit_runs(pts, &cs, reach, false, options)
}

/// The strokes of a mask's centre lines.
pub(crate) fn strokes(mask: &[bool], w: usize, h: usize, options: &Options) -> Vec<Stroke> {
  let dt = distance(mask, w, h);
  let mut skel = mask.to_vec();
  thin(&mut skel, w, h);
  unstair(&mut skel, w, h);
  let (mut nodes, branches) = graph(&skel, w, h);
  // stroke width: twice the distance to the background along the skeleton, less the centre pixel
  let on: Vec<f64> = (0..skel.len()).filter(|&i| skel[i]).map(|i| dt[i]).collect();
  let mut sorted = on.clone();
  sorted.sort_by(f64::total_cmp);
  let typical = sorted.get(sorted.len() / 2).map_or(1.0, |d| (2.0 * d - 1.0).max(1.0));
  let spur = options.spurs.unwrap_or(1.5 * typical).max(0.0);
  let paths: Vec<Path> = branches
    .into_iter()
    .map(|b| {
      let mut pts: Vec<P> = b.px.iter().map(|&i| centre(i, w)).collect();
      if let Some((a, z)) = b.ends {
        if let Some(p) = pts.first_mut() {
          *p = nodes[a].at;
        }
        if let Some(p) = pts.last_mut() {
          *p = nodes[z].at;
        }
      }
      Path { pts, ends: b.ends, px: b.px }
    })
    .collect();
  let paths = prune(&mut nodes, paths, spur);
  let grid = Grid { m: mask, w, h };
  let mut out = Vec::new();
  for mut p in paths {
    // a loop through a junction (the bowl of an e) stays open, starting and ending there
    let closed = p.ends.is_none();
    if closed && p.pts.first() != p.pts.last() {
      let first = p.pts[0];
      p.pts.push(first);
    }
    smooth(&mut p.pts, closed);
    let widths: Vec<f64> = p.px.iter().map(|&i| (2.0 * dt[i] - 1.0).max(1.0)).collect();
    let width = if widths.is_empty() { typical } else { widths.iter().sum::<f64>() / widths.len() as f64 };
    if let Some((a, b)) = p.ends
      && a != b
    {
      // ends that stop in the open reach back out to where the stroke ends
      let reach = (width / 2.0).max(0.0);
      if nodes[b].free {
        extend(&mut p.pts, true, &grid, reach);
      }
      if nodes[a].free {
        extend(&mut p.pts, false, &grid, reach);
      }
    }
    if length(&p.pts) < 1.0 && !closed {
      continue;
    }
    let segs = fit_path(&p.pts, closed, width, options);
    if !segs.is_empty() {
      out.push(Stroke { segs, closed, width });
    }
  }
  out
}
