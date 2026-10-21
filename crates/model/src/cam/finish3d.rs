//! 3D finishing by the shape of the surface: contour (waterline) passes at constant heights on
//! steep areas, steep-and-shallow (waterlines where steep, parallel rows where not) and pencil
//! passes along concave corners.
//!
//! All work on the "relief": the lowest the tool's tip may go at each node of a grid over the
//! area to machine (the tool dropped onto the part's heightfield, see [`super::surface`]).
//! Waterlines are its iso-lines; the slope of the part is its gradient; pencil passes follow
//! its creases, where the tool touches the part on two sides at once.

use fox_kernel::glam::{DVec2, DVec3};

use super::region::{Grid, simplify};
use super::surface::{Surface, rows, simplify3};
use super::{Operation, Path, Setup};
use crate::FaceRef;
use crate::regen::Model;

/// Pencil passes follow creases that turn the tool's path up by more than this (degrees): two
/// faces meeting at less than 160°.
const PENCIL_BEND: f64 = 20.0;

/// The tool dropped at the nodes of a grid over the area to machine.
struct Relief {
  /// Tip heights, the part's bottom where the tool doesn't touch it.
  g: Grid,
  /// Tip heights, −∞ where the tool doesn't touch the part.
  raw: Vec<f64>,
  /// Slope of the part under the tool (degrees from horizontal).
  slope: Vec<f64>,
  /// Nodes where the tool touches the faces to machine (or a node beside them does).
  covered: Vec<bool>,
}

impl Relief {
  fn new(s: &Surface) -> Relief {
    let extent = (s.hi - s.lo).max_element().max(1e-3);
    let h = (2.0 * s.part.h).max(extent / 400.0);
    let mut g = Grid::new(s.lo, s.hi, h, 2, 0.0);
    let n = g.v.len();
    let raw: Vec<f64> = (0..n).map(|i| s.part.drop_tool_over(&s.op.tool, g.node(i))).collect();
    for (v, r) in g.v.iter_mut().zip(&raw) {
      *v = r.max(s.floor);
    }
    let (nx, ny) = (g.nx, g.ny);
    let mut slope = vec![0.0; n];
    for j in 0..ny {
      for i in 0..nx {
        let at = |i: usize, j: usize| g.v[j * nx + i];
        let (i0, i1) = (i.saturating_sub(1), (i + 1).min(nx - 1));
        let (j0, j1) = (j.saturating_sub(1), (j + 1).min(ny - 1));
        let dx = (at(i1, j) - at(i0, j)) / ((i1 - i0).max(1) as f64 * h);
        let dy = (at(i, j1) - at(i, j0)) / ((j1 - j0).max(1) as f64 * h);
        slope[j * nx + i] = DVec2::new(dx, dy).length().atan().to_degrees();
      }
    }
    let off = [DVec2::ZERO, DVec2::X * h, -DVec2::X * h, DVec2::Y * h, -DVec2::Y * h];
    let covered = (0..n).map(|i| off.iter().any(|d| s.covers(g.node(i) + *d))).collect();
    Relief { g, raw, slope, covered }
  }

  /// Whether the tool at `p` (or a node's width beside it) touches the faces to machine: the
  /// tool running round the part's outside sits right on the edge of that area.
  fn reaches(&self, s: &Surface, p: DVec2) -> bool {
    let h = self.g.h;
    [DVec2::ZERO, DVec2::X * h, -DVec2::X * h, DVec2::Y * h, -DVec2::Y * h].iter().any(|d| s.covers(p + *d))
  }

  fn slope_at(&self, p: DVec2) -> f64 {
    self.slope[self.g.index(p)]
  }
}

/// Waterline passes a step-down apart where `keep` holds (each a 3D polyline; closed loops end
/// where they start), top level first, nearest first within a level.
fn waterlines(s: &Surface, rel: &Relief, step: f64, keep: &dyn Fn(DVec2) -> bool) -> Result<Vec<Vec<DVec3>>, String> {
  let g = &rel.g;
  let (mut top, mut bottom) = (f64::NEG_INFINITY, f64::INFINITY);
  for i in 0..g.v.len() {
    if rel.covered[i] {
      top = top.max(g.v[i]);
      bottom = bottom.min(g.v[i]);
    }
  }
  if !top.is_finite() || top - bottom < 1e-3 {
    return Ok(Vec::new());
  }
  let zs = super::levels(top, bottom + 0.01, step);
  if zs.len() >= super::MAX_PASSES {
    return Err("too many levels: raise the step-down".into());
  }
  let mut out: Vec<Vec<DVec3>> = Vec::new();
  let mut at = DVec2::new(s.lo.x, s.lo.y);
  for z in zs {
    let w = Grid { v: g.v.iter().map(|v| v - z).collect(), ..g.clone() };
    let mut level: Vec<(Vec<DVec3>, bool)> = Vec::new();
    for l in w.contours() {
      // each point at the level, or on the safe side of it where the iso-line cut a step
      let pts: Vec<(DVec3, bool)> = l.iter().map(|q| (lift(s, &w, *q, z), keep(*q))).collect();
      if pts.iter().all(|p| p.1) {
        level.push((pts.iter().map(|p| p.0).collect(), true));
        continue;
      }
      // the kept runs, starting after a dropped point so a run doesn't wrap
      let Some(start) = pts.iter().position(|p| !p.1) else { continue };
      let mut run: Vec<DVec3> = Vec::new();
      for k in 1..=pts.len() {
        let (q, kept) = pts[(start + k) % pts.len()];
        if kept {
          run.push(q);
        } else if !run.is_empty() {
          level.push((std::mem::take(&mut run), false));
        }
      }
      if !run.is_empty() {
        level.push((run, false));
      }
    }
    // nearest first
    while !level.is_empty() {
      let (k, _) = level
        .iter()
        .enumerate()
        .map(|(k, (pts, closed))| {
          let d = if *closed {
            pts.iter().map(|q| (q.truncate() - at).length()).fold(f64::INFINITY, f64::min)
          } else {
            (pts[0].truncate() - at).length().min((pts[pts.len() - 1].truncate() - at).length())
          };
          (k, d)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0));
      let (mut pts, closed) = level.swap_remove(k);
      if closed {
        let i = (0..pts.len())
          .min_by(|a, b| (pts[*a].truncate() - at).length().total_cmp(&(pts[*b].truncate() - at).length()))
          .unwrap_or(0);
        pts.rotate_left(i);
        pts.push(pts[0]);
      } else if (pts[pts.len() - 1].truncate() - at).length() < (pts[0].truncate() - at).length() {
        pts.reverse();
      }
      let length: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
      if pts.len() < 2 || length < g.h {
        continue;
      }
      at = pts[pts.len() - 1].truncate();
      out.push(pts);
    }
  }
  Ok(out)
}

/// A waterline point at height `z`: the iso-line's crossing of a grid edge, moved along that
/// edge to where the tool just touches the part (linear interpolation puts it anywhere across
/// a step), on the clear side.
fn lift(s: &Surface, w: &Grid, q: DVec2, z: f64) -> DVec3 {
  let tol = 0.002;
  let (fx, fy) = ((q.x - w.x0) / w.h, (q.y - w.y0) / w.h);
  let max = |n: usize| (n.saturating_sub(2)) as f64;
  // the edge's two nodes: on a row (y on a grid line) or a column
  let (a, b) = if (fy - fy.round()).abs() < 1e-6 {
    let (i, j) = (fx.floor().clamp(0.0, max(w.nx)) as usize, fy.round().clamp(0.0, max(w.ny) + 1.0) as usize);
    (j * w.nx + i, j * w.nx + i + 1)
  } else {
    let (i, j) = (fx.round().clamp(0.0, max(w.nx) + 1.0) as usize, fy.floor().clamp(0.0, max(w.ny)) as usize);
    (j * w.nx + i, (j + 1) * w.nx + i)
  };
  let (good, bad) = match (w.v[a] <= 0.0, w.v[b] <= 0.0) {
    (true, false) => (w.node(a), w.node(b)),
    (false, true) => (w.node(b), w.node(a)),
    _ => {
      let tip = s.drop(q).z;
      return DVec3::new(q.x, q.y, z.max(tip));
    }
  };
  let (mut good, mut bad) = (good, bad);
  for _ in 0..12 {
    let m = (good + bad) / 2.0;
    if s.drop(m).z <= z + tol {
      good = m;
    } else {
      bad = m;
    }
  }
  DVec3::new(good.x, good.y, z.max(s.drop(good).z))
}

/// Cuts 3D passes (already at their heights) in order, staying down between them when the
/// way is short and clear of the part, else going up and over.
fn emit3(p: &mut Path, s: &Surface, passes: Vec<Vec<DVec3>>, link: f64) {
  let tol = (s.part.h * 0.05).clamp(0.001, 0.005);
  for pts in passes {
    let pts = simplify3(&pts, tol);
    if pts.len() < 2 {
      continue;
    }
    let at = p.moves.last().filter(|m| !m.rapid && m.to.z < p.safe - 1e-9).map(|m| m.to);
    let clear = at.is_some_and(|a| {
      let b = pts[0];
      let len = (b - a).truncate().length();
      let n = ((len / s.part.h).ceil() as usize).max(1);
      len <= link
        && (0..=n).all(|k| {
          let q = a.lerp(b, k as f64 / n as f64);
          s.drop(q.truncate()).z <= q.z + 1e-3
        })
    });
    if clear {
      p.cut(pts[0]);
    } else {
      p.leave();
      p.enter(pts[0]);
    }
    for q in &pts[1..] {
      p.cut(*q);
    }
  }
  p.leave();
}

/// Contour finishing: waterlines a step-down apart where the part is at least `min_slope`
/// degrees steep (flat areas never get one).
pub(super) fn contour(
  p: &mut Path,
  setup: &Setup,
  model: &Model,
  op: &Operation,
  faces: &[FaceRef],
  min_slope: f64,
) -> Result<(), String> {
  if !(0.0..90.0).contains(&min_slope) {
    return Err("the slope must be from 0 to 90 degrees".into());
  }
  let s = Surface::new(setup, model, op, faces)?;
  let rel = Relief::new(&s);
  let min = min_slope.max(1.0);
  let passes = waterlines(&s, &rel, op.stepdown, &|q| rel.reaches(&s, q) && rel.slope_at(q) >= min)?;
  if passes.is_empty() {
    return Err("there are no steep areas to contour".into());
  }
  emit3(p, &s, passes, op.stepdown * 2.0 + op.tool.diameter * op.stepover);
  Ok(())
}

/// Steep and shallow finishing: waterlines where the part is steeper than `threshold` degrees,
/// parallel rows a step-over apart where it isn't (overlapping a little).
pub(super) fn steep_shallow(
  p: &mut Path,
  setup: &Setup,
  model: &Model,
  op: &Operation,
  faces: &[FaceRef],
  threshold: f64,
) -> Result<(), String> {
  if !(threshold > 0.0 && threshold < 90.0) {
    return Err("the threshold must be between 0 and 90 degrees".into());
  }
  let s = Surface::new(setup, model, op, faces)?;
  let rel = Relief::new(&s);
  let overlap = 2.0;
  let steep = waterlines(&s, &rel, op.stepdown, &|q| rel.reaches(&s, q) && rel.slope_at(q) >= threshold - overlap)?;
  let step = op.tool.diameter * op.stepover;
  let shallow = rows(&s, step, 0.0, &|q| s.covers(q) && rel.slope_at(q) <= threshold + overlap)?;
  if steep.is_empty() && shallow.is_empty() {
    return Err("the tool can't reach the faces".into());
  }
  if !steep.is_empty() {
    emit3(p, &s, steep, op.stepdown * 2.0 + step);
  }
  if !shallow.is_empty() {
    s.emit(p, shallow, step * 1.5);
  }
  Ok(())
}

/// Most pencil passes either side of a corner.
pub const MAX_PENCIL_PASSES: u32 = 50;

/// Steeper than this (degrees) between two samples of the relief, the tool would jump a cliff
/// to get from a corner to an offset pass there: that piece of the pass is left out.
const PENCIL_CLIFF: f64 = 80.0;

/// Offset passes follow only the stretches of a crease less steep than this (degrees).
const PENCIL_CLIMB: f64 = 60.0;

/// Pencil finishing: a pass along each crease of the relief that bends up by more than
/// [`PENCIL_BEND`], where the tool sits in a corner touching the part on two sides, and
/// `passes` more either side of it a step-over apart (inside out), where the tool rides the
/// faces meeting at the corner.
pub(super) fn pencil(
  p: &mut Path,
  setup: &Setup,
  model: &Model,
  op: &Operation,
  faces: &[FaceRef],
  passes: u32,
) -> Result<(), String> {
  if passes > MAX_PENCIL_PASSES {
    return Err(format!("at most {MAX_PENCIL_PASSES} pencil passes either side"));
  }
  let s = Surface::new(setup, model, op, faces)?;
  let rel = Relief::new(&s);
  let marks = creases(&rel);
  let mut all: Vec<(Vec<DVec2>, bool)> = chains(&rel.g, &marks)
    .into_iter()
    .map(|(nodes, closed)| {
      let pts: Vec<DVec2> = nodes.iter().map(|k| rel.g.node(*k)).collect();
      (simplify(&pts, rel.g.h * 0.3, closed), closed)
    })
    .filter(|(pts, _)| pts.len() > 1)
    .collect();
  if all.is_empty() {
    return Err("there are no corners tighter than the tool to pencil".into());
  }
  let step = op.tool.diameter * op.stepover;
  if passes > 0 {
    let used: Vec<bool> = {
      // the creases the passes follow, but not where they climb steeply (up a vertical corner,
      // where there are no faces either side for the tool to ride)
      let mut on = vec![false; marks.len()];
      let climb = PENCIL_CLIMB.to_radians().tan();
      for (pts, _) in &all {
        for w in pts.windows(2) {
          let rise = (rel.g.sample(w[1]) - rel.g.sample(w[0])).abs();
          if rise > climb * (w[1] - w[0]).length() {
            continue;
          }
          let n = (((w[1] - w[0]).length() / (rel.g.h * 0.5)).ceil() as usize).max(1);
          for k in 0..=n {
            on[rel.g.index(w[0].lerp(w[1], k as f64 / n as f64))] = true;
          }
        }
      }
      on
    };
    let (dist, from) = nearest(&rel.g, &used);
    let mut at = all.last().and_then(|(pts, _)| pts.last().copied()).unwrap_or(s.lo);
    for k in 1..=passes {
      let d = k as f64 * step;
      let w = Grid { v: dist.iter().map(|v| v - d).collect(), ..rel.g.clone() };
      let mut level: Vec<(Vec<DVec2>, bool)> = Vec::new();
      for l in w.contours() {
        let keep: Vec<bool> = l.iter().map(|q| rel.reaches(&s, *q) && rides(&rel, &from, *q)).collect();
        level.extend(runs(&l, &keep, rel.g.h));
      }
      // nearest first
      while !level.is_empty() {
        let (i, flip) = nearest_run(&level, at);
        let (mut pts, closed) = level.swap_remove(i);
        if flip {
          pts.reverse();
        }
        if closed {
          let i =
            (0..pts.len()).min_by(|a, b| (pts[*a] - at).length().total_cmp(&(pts[*b] - at).length())).unwrap_or(0);
          pts.rotate_left(i);
        }
        let pts = simplify(&pts, rel.g.h * 0.3, closed);
        if pts.len() < 2 {
          continue;
        }
        at = if closed { pts[0] } else { pts[pts.len() - 1] };
        all.push((pts, closed));
      }
    }
  }
  s.emit(p, all, (rel.g.h * 3.0).max(step * 1.5));
  Ok(())
}

/// Distance from every node to the nearest node of `set`, and that node (usize::MAX where
/// there is none): a front grown out from the set, each node taking its neighbours' nearest.
fn nearest(g: &Grid, set: &[bool]) -> (Vec<f64>, Vec<usize>) {
  use std::cmp::Reverse;
  use std::collections::BinaryHeap;
  let (nx, ny) = (g.nx as i64, g.ny as i64);
  let n = set.len();
  let mut dist = vec![f64::INFINITY; n];
  let mut from = vec![usize::MAX; n];
  let mut heap = BinaryHeap::new();
  for k in (0..n).filter(|k| set[*k]) {
    dist[k] = 0.0;
    from[k] = k;
    heap.push(Reverse((0u64, k)));
  }
  while let Some(Reverse((dk, k))) = heap.pop() {
    if f64::from_bits(dk) > dist[k] {
      continue;
    }
    let (i, j) = (k as i64 % nx, k as i64 / nx);
    let src = g.node(from[k]);
    for (di, dj) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
      let (a, b) = (i + di, j + dj);
      if a < 0 || b < 0 || a >= nx || b >= ny {
        continue;
      }
      let m = (b * nx + a) as usize;
      let dm = (g.node(m) - src).length();
      if dm < dist[m] - 1e-12 {
        dist[m] = dm;
        from[m] = from[k];
        heap.push(Reverse((dm.to_bits(), m)));
      }
    }
  }
  (dist, from)
}

/// Whether the tool gets from the corner nearest `q` out to `q` riding the surface: the relief
/// never steeper than [`PENCIL_CLIFF`] on the way, and the tool on the part all along.
fn rides(rel: &Relief, from: &[usize], q: DVec2) -> bool {
  let g = &rel.g;
  let k = g.index(q);
  if from[k] == usize::MAX || !rel.raw[k].is_finite() {
    return false;
  }
  let c = g.node(from[k]);
  let len = (q - c).length();
  let n = ((len / (g.h * 0.5)).ceil() as usize).max(1);
  let limit = PENCIL_CLIFF.to_radians().tan() * len / n as f64;
  let z = |t: f64| g.sample(c.lerp(q, t));
  (1..=n).all(|i| {
    let (a, b) = ((i - 1) as f64 / n as f64, i as f64 / n as f64);
    rel.raw[g.index(c.lerp(q, b))].is_finite() && (z(b) - z(a)).abs() <= limit + 1e-9
  })
}

/// The runs of a closed line where `keep` holds: the whole line closed when it all does, else
/// open pieces at least `min` long.
fn runs(l: &[DVec2], keep: &[bool], min: f64) -> Vec<(Vec<DVec2>, bool)> {
  if keep.iter().all(|x| *x) {
    return vec![(l.to_vec(), true)];
  }
  let Some(start) = keep.iter().position(|x| !*x) else { return Vec::new() };
  let n = l.len();
  let mut out = Vec::new();
  let mut run: Vec<DVec2> = Vec::new();
  let flush = |run: &mut Vec<DVec2>, out: &mut Vec<(Vec<DVec2>, bool)>| {
    let length: f64 = run.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    if run.len() > 1 && length >= min {
      out.push((std::mem::take(run), false));
    }
    run.clear();
  };
  for k in 1..=n {
    let i = (start + k) % n;
    if keep[i] {
      run.push(l[i]);
    } else {
      flush(&mut run, &mut out);
    }
  }
  flush(&mut run, &mut out);
  out
}

/// The run nearest `at`: its index, and whether to cut it backwards (open runs only).
fn nearest_run(runs: &[(Vec<DVec2>, bool)], at: DVec2) -> (usize, bool) {
  let mut best = (0, false, f64::INFINITY);
  for (k, (pts, closed)) in runs.iter().enumerate() {
    let (a, b) = ((pts[0] - at).length(), (pts[pts.len() - 1] - at).length());
    if *closed {
      let d = pts.iter().map(|q| (*q - at).length()).fold(f64::INFINITY, f64::min);
      if d < best.2 {
        best = (k, false, d);
      }
    } else if a.min(b) < best.2 {
      best = (k, b < a, a.min(b));
    }
  }
  (best.0, best.1)
}

/// Nodes on a crease of the relief: where its slope turns up by more than the pencil bend
/// across the node (or across it and its neighbour, when the crease falls between them), the
/// most of its neighbours along that direction.
fn creases(rel: &Relief) -> Vec<bool> {
  let g = &rel.g;
  let (nx, ny) = (g.nx as i64, g.ny as i64);
  let n = g.v.len();
  let limit = PENCIL_BEND.to_radians();
  let ok = |i: i64, j: i64| i >= 0 && j >= 0 && i < nx && j < ny && rel.raw[(j * nx + i) as usize].is_finite();
  // the bend at a node along a direction (radians, positive when it turns up)
  let bend = |i: i64, j: i64, di: i64, dj: i64| -> Option<f64> {
    if !(ok(i, j) && ok(i - di, j - dj) && ok(i + di, j + dj)) {
      return None;
    }
    let len = g.h * ((di * di + dj * dj) as f64).sqrt();
    let at = |i: i64, j: i64| rel.raw[(j * nx + i) as usize];
    let m = at(i, j);
    let (a, b) = (at(i - di, j - dj), at(i + di, j + dj));
    Some(((b - m) / len).atan() - ((m - a) / len).atan())
  };
  let mut out = vec![false; n];
  for (k, o) in out.iter_mut().enumerate() {
    if !rel.covered[k] {
      continue;
    }
    let (i, j) = ((k as i64) % nx, (k as i64) / nx);
    for (di, dj) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
      let Some(b) = bend(i, j, di, dj) else { continue };
      if b <= 0.0 {
        continue;
      }
      let before = bend(i - di, j - dj, di, dj).unwrap_or(0.0);
      let after = bend(i + di, j + dj, di, dj).unwrap_or(0.0);
      // the most along the direction (the first of two equal ones), and enough with the
      // larger neighbour when the crease falls between nodes
      if b > before && b >= after && b + before.max(after).max(0.0) >= limit {
        *o = true;
        break;
      }
    }
  }
  out
}

/// Chains of marked nodes, thinned to one node across: (nodes, closed).
fn chains(g: &Grid, marks: &[bool]) -> Vec<(Vec<usize>, bool)> {
  let (nx, ny) = (g.nx as i64, g.ny as i64);
  let mut used = vec![false; marks.len()];
  let xy = |k: usize| DVec2::new((k as i64 % nx) as f64, (k as i64 / nx) as f64);
  let mut cover = |used: &mut [bool], k: usize| {
    let (i, j) = (k as i64 % nx, k as i64 / nx);
    for dj in -1..=1 {
      for di in -1..=1 {
        let (a, b) = (i + di, j + dj);
        if a >= 0 && b >= 0 && a < nx && b < ny {
          used[(b * nx + a) as usize] = true;
        }
      }
    }
  };
  // from a node, onwards to marked nodes two away, keeping on in about the same direction
  let walk = |used: &mut [bool], cover: &mut dyn FnMut(&mut [bool], usize), from: usize, dir: Option<DVec2>| {
    let mut path = Vec::new();
    let (mut cur, mut dir) = (from, dir);
    loop {
      cover(used, cur);
      let (i, j) = (cur as i64 % nx, cur as i64 / nx);
      let mut best: Option<(f64, usize)> = None;
      for dj in -2..=2 {
        for di in -2..=2 {
          let (a, b) = (i + di, j + dj);
          if a < 0 || b < 0 || a >= nx || b >= ny {
            continue;
          }
          let k = (b * nx + a) as usize;
          if !marks[k] || used[k] {
            continue;
          }
          let d = DVec2::new(di as f64, dj as f64);
          let along = dir.map_or(0.0, |v| d.normalize().dot(v));
          if along < -0.2 {
            continue;
          }
          // nearest, then straightest
          let score = d.length() - 0.5 * along;
          if best.is_none_or(|b| score < b.0) {
            best = Some((score, k));
          }
        }
      }
      let Some((_, next)) = best else { break };
      dir = Some((xy(next) - xy(cur)).normalize());
      path.push(next);
      cur = next;
    }
    path
  };
  let mut out = Vec::new();
  for start in 0..marks.len() {
    if !marks[start] || used[start] {
      continue;
    }
    let ahead = walk(&mut used, &mut cover, start, None);
    let back_dir = ahead.first().map(|k| (xy(start) - xy(*k)).normalize());
    let behind = if back_dir.is_some() { walk(&mut used, &mut cover, start, back_dir) } else { Vec::new() };
    let mut chain: Vec<usize> = behind.into_iter().rev().collect();
    chain.push(start);
    chain.extend(ahead);
    if chain.len() < 4 {
      continue;
    }
    let closed = chain.len() > 6 && (xy(chain[0]) - xy(chain[chain.len() - 1])).length() <= 3.5;
    out.push((chain, closed));
  }
  out
}
