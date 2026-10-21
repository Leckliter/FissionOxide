//! A maze autorouter: routes ratsnest connections one at a time, shortest first, on a grid over
//! every copper layer with vias, keeping the design rules' clearances to other nets' copper,
//! holes and the board edge. Manual routing stays the main tool; this fills in the rest.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::board::{BoardPad, Copper, Item, Layer, Track, Via};
use crate::geom::{P, Shape2, point_in_poly, point_poly_edge_dist, point_seg_dist};
use crate::project::Project;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct RouteReport {
  pub routed: usize,
  /// Connections that found no path: (net, from, to).
  pub failed: Vec<(String, P, P)>,
  pub tracks: usize,
  pub vias: usize,
}

/// Distance from a point to a shape's edge (0 inside).
fn point_dist(p: P, s: &Shape2) -> f64 {
  match s {
    Shape2::Capsule { a, b, r } => (point_seg_dist(p, *a, *b) - r).max(0.0),
    Shape2::Poly(pts) => {
      if point_in_poly(p, pts) {
        0.0
      } else {
        point_poly_edge_dist(p, pts)
      }
    }
  }
}

struct Grid {
  lo: P,
  step: f64,
  nx: usize,
  ny: usize,
}

impl Grid {
  fn center(&self, i: usize, j: usize) -> P {
    [self.lo[0] + i as f64 * self.step, self.lo[1] + j as f64 * self.step]
  }

  fn idx(&self, i: usize, j: usize) -> usize {
    j * self.nx + i
  }

  /// Marks cells within `reach` of the shape.
  fn rasterize(&self, s: &Shape2, reach: f64, mut f: impl FnMut(usize)) {
    let (lo, hi) = s.bbox();
    let i0 = (((lo[0] - reach - self.lo[0]) / self.step).floor().max(0.0)) as usize;
    let j0 = (((lo[1] - reach - self.lo[1]) / self.step).floor().max(0.0)) as usize;
    let i1 = ((((hi[0] + reach - self.lo[0]) / self.step).ceil()) as usize).min(self.nx.saturating_sub(1));
    let j1 = ((((hi[1] + reach - self.lo[1]) / self.step).ceil()) as usize).min(self.ny.saturating_sub(1));
    for j in j0..=j1 {
      for i in i0..=i1 {
        if point_dist(self.center(i, j), s) < reach - 1e-9 {
          f(self.idx(i, j));
        }
      }
    }
  }
}

/// Routes the unrouted connections of `nets` (all when empty) on a `step` grid.
pub fn autoroute(pr: &mut Project, nets: &[String], step: f64) -> RouteReport {
  let mut report = RouteReport::default();
  let Some((lo, hi)) = crate::geom::bounds(pr.board.outline.iter().copied()) else { return report };
  if pr.board.outline.len() < 3 || step <= 0.0 {
    return report;
  }
  let lo = [(lo[0] / step).floor() * step, (lo[1] / step).floor() * step];
  let grid = Grid { lo, step, nx: ((hi[0] - lo[0]) / step).ceil() as usize + 1, ny: ((hi[1] - lo[1]) / step).ceil() as usize + 1 };
  if grid.nx * grid.ny > 4_000_000 {
    return report;
  }
  let layers = pr.board.copper_layers();
  let r = pr.board.rules.clone();
  let hw = r.track_width / 2.0;
  let via_r = r.via_diameter / 2.0;
  let n = grid.nx * grid.ny;
  let mut inside_track = vec![false; n];
  let mut inside_via = vec![false; n];
  for j in 0..grid.ny {
    for i in 0..grid.nx {
      let p = grid.center(i, j);
      if point_in_poly(p, &pr.board.outline) {
        let d = point_poly_edge_dist(p, &pr.board.outline);
        inside_track[grid.idx(i, j)] = d >= r.edge_clearance + hw;
        inside_via[grid.idx(i, j)] = d >= r.edge_clearance + via_r;
      }
    }
  }
  let mut failed: Vec<(String, P, P)> = Vec::new();
  loop {
    let pads: Vec<BoardPad> = pr.pads();
    let mut air: Vec<_> = pr.board.ratsnest(&pads);
    air.retain(|a| (nets.is_empty() || nets.contains(&a.net)) && !failed.iter().any(|f| f.0 == a.net && crate::geom::near(f.1, a.a) && crate::geom::near(f.2, a.b)));
    air.sort_by(|a, b| crate::geom::dist(a.a, a.b).total_cmp(&crate::geom::dist(b.a, b.b)));
    let Some(job) = air.first().cloned() else { break };
    let cu = pr.board.copper(&pads);
    match route_one(pr, &grid, &layers, &cu, &pads, &job.net, job.a, job.b, &inside_track, &inside_via) {
      Some((tracks, vias)) => {
        report.routed += 1;
        report.tracks += tracks.len();
        report.vias += vias.len();
        pr.board.tracks.extend(tracks);
        pr.board.vias.extend(vias);
      }
      None => failed.push((job.net.clone(), job.a, job.b)),
    }
  }
  report.failed = failed;
  report
}

fn route_one(
  pr: &Project,
  grid: &Grid,
  layers: &[Layer],
  cu: &[Copper],
  pads: &[BoardPad],
  net: &str,
  a: P,
  b: P,
  inside_track: &[bool],
  inside_via: &[bool],
) -> Option<(Vec<Track>, Vec<Via>)> {
  let r = &pr.board.rules;
  let hw = r.track_width / 2.0;
  let via_r = r.via_diameter / 2.0;
  let n = grid.nx * grid.ny;
  let nl = layers.len();
  let li = |l: Layer| layers.iter().position(|x| *x == l);
  let mut blocked = vec![false; n * nl];
  let mut via_blocked = vec![false; n];
  for c in cu.iter().filter(|c| c.net != net || c.net.is_empty()) {
    if matches!(c.item, Item::Pour(_)) {
      continue;
    }
    for l in &c.layers {
      if let Some(k) = li(*l) {
        grid.rasterize(&c.shape, r.clearance + hw, |i| blocked[k * n + i] = true);
      }
    }
    grid.rasterize(&c.shape, r.clearance + via_r, |i| via_blocked[i] = true);
  }
  // holes: vias keep the hole-to-hole clearance from every drill, tracks clear unplated ones
  for (at, d, plated) in pr.board.drills(pads) {
    let hole = Shape2::circle(at, d);
    grid.rasterize(&hole, r.hole_clearance + r.via_drill / 2.0, |i| via_blocked[i] = true);
    if !plated {
      for k in 0..nl {
        grid.rasterize(&hole, r.clearance + hw, |i| blocked[k * n + i] = true);
      }
    }
  }
  // the copper each end sits in: its cells are where the route may start and finish
  let ends = |p: P| -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for c in cu.iter().filter(|c| c.net == net && !matches!(c.item, Item::Pour(_)) && c.shape.contains(p)) {
      let shrink = match &c.shape {
        Shape2::Capsule { a, b, r } => Shape2::Capsule { a: *a, b: *b, r: r * 0.6 },
        s => s.clone(),
      };
      for l in &c.layers {
        if let Some(k) = li(*l) {
          grid.rasterize(&shrink, 1e-6, |i| out.push((k, i)));
          let ci = (((p[0] - grid.lo[0]) / grid.step).round() as usize).min(grid.nx - 1);
          let cj = (((p[1] - grid.lo[1]) / grid.step).round() as usize).min(grid.ny - 1);
          out.push((k, grid.idx(ci, cj)));
        }
      }
    }
    out.sort();
    out.dedup();
    out
  };
  let starts = ends(a);
  let goals = ends(b);
  if starts.is_empty() || goals.is_empty() {
    return None;
  }
  let mut is_goal = vec![false; n * nl];
  for &(k, i) in &goals {
    is_goal[k * n + i] = true;
  }
  let mut forced = vec![false; n * nl];
  for &(k, i) in starts.iter().chain(goals.iter()) {
    forced[k * n + i] = true;
  }
  let free = |k: usize, i: usize| forced[k * n + i] || (!blocked[k * n + i] && inside_track[i]);
  let h = |i: usize| {
    let (x, y) = ((i % grid.nx) as f64, (i / grid.nx) as f64);
    let bx = (b[0] - grid.lo[0]) / grid.step;
    let by = (b[1] - grid.lo[1]) / grid.step;
    let (dx, dy) = ((x - bx).abs(), (y - by).abs());
    ((dx.max(dy) - dx.min(dy)) + dx.min(dy) * std::f64::consts::SQRT_2) * 0.999
  };
  const VIA_COST: f64 = 12.0;
  const TURN: f64 = 0.3;
  let scale = 1000.0;
  // state: (layer, cell, direction 0..8 / 8 = none)
  let ns = n * nl * 9;
  let mut cost = vec![f64::INFINITY; ns];
  let mut prev = vec![usize::MAX; ns];
  let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
  let sid = |k: usize, i: usize, d: usize| (k * n + i) * 9 + d;
  for &(k, i) in &starts {
    let s = sid(k, i, 8);
    cost[s] = 0.0;
    heap.push(Reverse(((h(i) * scale) as u64, s)));
  }
  let dirs: [(i64, i64); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
  let mut found = None;
  let mut pops = 0usize;
  while let Some(Reverse((_, s))) = heap.pop() {
    pops += 1;
    if pops > 3_000_000 {
      break;
    }
    let (cell, d) = (s / 9, s % 9);
    let (k, i) = (cell / n, cell % n);
    let c0 = cost[s];
    if is_goal[cell] {
      found = Some(s);
      break;
    }
    let (x, y) = ((i % grid.nx) as i64, (i / grid.nx) as i64);
    for (nd, (dx, dy)) in dirs.iter().enumerate() {
      let (x2, y2) = (x + dx, y + dy);
      if x2 < 0 || y2 < 0 || x2 >= grid.nx as i64 || y2 >= grid.ny as i64 {
        continue;
      }
      let i2 = grid.idx(x2 as usize, y2 as usize);
      if !free(k, i2) {
        continue;
      }
      if *dx != 0 && *dy != 0 {
        let (ia, ib) = (grid.idx(x2 as usize, y as usize), grid.idx(x as usize, y2 as usize));
        if !free(k, ia) || !free(k, ib) {
          continue;
        }
      }
      let step = if *dx != 0 && *dy != 0 { std::f64::consts::SQRT_2 } else { 1.0 };
      let turn = if d == 8 || d == nd { 0.0 } else { TURN };
      let c = c0 + step + turn;
      let t = sid(k, i2, nd);
      if c < cost[t] {
        cost[t] = c;
        prev[t] = s;
        heap.push(Reverse((((c + h(i2)) * scale) as u64, t)));
      }
    }
    if !via_blocked[i] && inside_via[i] {
      for k2 in 0..nl {
        if k2 == k || !free(k2, i) {
          continue;
        }
        let c = c0 + VIA_COST;
        let t = sid(k2, i, 8);
        if c < cost[t] {
          cost[t] = c;
          prev[t] = s;
          heap.push(Reverse((((c + h(i)) * scale) as u64, t)));
        }
      }
    }
  }
  let mut s = found?;
  let mut path: Vec<(usize, usize)> = Vec::new();
  loop {
    let cell = s / 9;
    let st = (cell / n, cell % n);
    if path.last() != Some(&st) {
      path.push(st);
    }
    if prev[s] == usize::MAX {
      break;
    }
    s = prev[s];
  }
  path.reverse();
  // cells → tracks and vias
  let mut tracks = Vec::new();
  let mut vias = Vec::new();
  let pt = |i: usize| grid.center(i % grid.nx, i / grid.nx);
  let mut seg_start = 0;
  for w in 1..=path.len() {
    let end_run = w == path.len() || path[w].0 != path[seg_start].0;
    if end_run {
      let run = &path[seg_start..w];
      let k = run[0].0;
      // merge collinear steps
      let mut pts: Vec<P> = vec![pt(run[0].1)];
      let mut last_dir: Option<(i64, i64)> = None;
      for q in run.windows(2) {
        let (a, b) = (q[0].1, q[1].1);
        let d = ((b % grid.nx) as i64 - (a % grid.nx) as i64, (b / grid.nx) as i64 - (a / grid.nx) as i64);
        if last_dir == Some(d) {
          if let Some(l) = pts.last_mut() {
            *l = pt(b);
          }
        } else {
          pts.push(pt(b));
        }
        last_dir = Some(d);
      }
      for s in pts.windows(2) {
        tracks.push(Track { layer: layers[k], a: s[0], b: s[1], width: r.track_width, net: net.to_string() });
      }
      if w < path.len() {
        vias.push(Via { at: pt(path[w].1), drill: r.via_drill, diameter: r.via_diameter, net: net.to_string() });
      }
      seg_start = w;
    }
  }
  Some((tracks, vias))
}
