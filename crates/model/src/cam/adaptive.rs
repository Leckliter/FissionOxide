//! Adaptive clearing of 2D pockets. The cleared area starts as a helix at the pocket's widest
//! point and grows by the step-over (the "optimal load") each pass, clipped to where the tool
//! fits: the tool follows the edge of what is already cut, so it only ever takes the step-over's
//! width of material instead of slotting into corners. Passes link inside the cleared area
//! where they can and retract where they can't.
//!
//! The growth is computed once on a grid of tool centres (distance transforms and iso-lines,
//! see [`super::region`]) and replayed at every depth.

use clf_kernel::glam::{DVec2, DVec3};

use super::region::{Grid, Region, simplify};
use super::{Entry, MAX_PASSES, Operation, Path};

/// One step of the clearing plan, in XY.
#[derive(Debug, Clone)]
enum Step {
  /// Enter here: a helix of this radius, or a plunge when it is 0.
  Seed { c: DVec2, rho: f64 },
  /// A cutting pass; `closed` loops end where they start.
  Cut { pts: Vec<DVec2>, closed: bool, link: Link },
}

/// How the tool gets to the start of a pass from the end of the last one.
#[derive(Debug, Clone, Copy)]
enum Link {
  /// Straight there through cleared space.
  Direct,
  /// Through a cleared point next to the start, staying down, then into the cut sideways.
  Through(DVec2),
  /// Up, over to a cleared point next to the start, down there and into the cut.
  Retract(DVec2),
}

/// Clears a pocket floor's region at each of `levels` (top first), the material starting at
/// `top`.
pub(super) fn clear(p: &mut Path, region: &Region, op: &Operation, levels: &[f64], top: f64) -> Result<(), String> {
  let plan = plan(region, op)?;
  let slope = op.ramp_angle.clamp(0.5, 90.0).to_radians().tan();
  let mut above = top;
  for &z in levels {
    p.top = above;
    let at3 = |q: DVec2| DVec3::new(q.x, q.y, z);
    for step in &plan {
      match step {
        Step::Seed { c, rho } => {
          p.leave();
          if *rho > 0.0 {
            p.helix(at3(*c), *rho, above, z, slope);
          } else {
            p.enter(at3(*c));
          }
        }
        Step::Cut { pts, closed, link } => {
          match link {
            Link::Direct => {}
            Link::Through(q) => p.cut(at3(*q)),
            Link::Retract(q) => {
              p.leave();
              p.enter(at3(*q));
            }
          }
          for q in pts {
            p.cut(at3(*q));
          }
          if *closed {
            p.cut(at3(pts[0]));
          }
        }
      }
    }
    p.leave();
    above = z;
  }
  Ok(())
}

fn plan(region: &Region, op: &Operation) -> Result<Vec<Step>, String> {
  let r = op.tool.radius();
  let wall = r + op.stock_to_leave;
  let s = op.tool.diameter * op.stepover;
  let extent = (region.hi - region.lo).max_element();
  let h = (s / 3.0).min(r / 4.0).max(extent / 1500.0).max(0.01);
  let mut grid = Grid::new(region.lo, region.hi, h, 2, 0.0);
  let helix = if op.entry == Entry::Plunge {
    0.0
  } else if op.helix_diameter > 0.0 {
    op.helix_diameter / 2.0
  } else {
    r
  };
  let cap = wall + helix.max(s) + 4.0 * h;
  region.sdf_grid(&mut grid, cap);
  let sdf = grid.v.clone();
  let margin = 1e-3;
  let allowed: Vec<bool> = sdf.iter().map(|v| *v >= wall + margin).collect();
  if !allowed.iter().any(|a| *a) {
    return Err("the tool doesn't fit in the pocket".into());
  }
  // how deep each node is inside the region (approximately, past the clamp), to seed at the
  // widest point
  let outside: Vec<bool> = sdf.iter().map(|v| *v <= 0.0).collect();
  let depth = grid.distance_to(&outside);
  let n = grid.v.len();
  let (nx, ny) = (grid.nx, grid.ny);
  let mut cleared = vec![false; n];
  let mut steps = Vec::new();
  let mut at: DVec2;
  let mut passes = 0;
  // a seed at the widest point of what's left, until nothing is
  while let Some(seed) = (0..n).filter(|&i| allowed[i] && !cleared[i]).max_by(|a, b| depth[*a].total_cmp(&depth[*b])) {
    let c = grid.node(seed);
    let rho = if helix > 0.0 {
      let room = region.sdf(c, cap) - wall - 0.01;
      let rho = helix.min(room);
      if rho >= 0.1 * r { rho } else { 0.0 }
    } else {
      0.0
    };
    steps.push(Step::Seed { c, rho });
    at = c + DVec2::new(rho, 0.0);
    for (i, cl) in cleared.iter_mut().enumerate() {
      if allowed[i] && (grid.node(i) - c).length() <= rho + 1e-9 {
        *cl = true;
      }
    }
    cleared[seed] = true;
    loop {
      passes += 1;
      if passes > MAX_PASSES {
        return Err("too many passes: raise the step-over".into());
      }
      let f = grid.distance_to(&cleared);
      // the next cleared area: within a step-over of what is cleared, where the tool fits,
      // and connected to it (not across a wall)
      let mut grow = cleared.clone();
      let mut queue: Vec<usize> = (0..n).filter(|&i| cleared[i]).collect();
      let mut added = 0;
      while let Some(k) = queue.pop() {
        let (i, j) = ((k % nx) as i64, (k / nx) as i64);
        for (di, dj) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
          let (a, b) = (i + di, j + dj);
          if a < 0 || b < 0 || a >= nx as i64 || b >= ny as i64 {
            continue;
          }
          let m = b as usize * nx + a as usize;
          if !grow[m] && allowed[m] && f[m] <= s + 1e-9 {
            grow[m] = true;
            added += 1;
            queue.push(m);
          }
        }
      }
      if added == 0 {
        break;
      }
      // its outline: where the step-over distance meets the walls' limit
      for i in 0..n {
        let edge = (f[i] - s).max(wall + margin - sdf[i]);
        grid.v[i] = if grow[i] {
          edge.min(-1e-6)
        } else if edge > 0.0 {
          edge
        } else {
          // within reach but cut off by a wall: left for a later seed
          h
        };
      }
      let fgrid = Grid { v: f, ..grid.clone() };
      // passes nearest first
      let mut pieces: Vec<(Vec<DVec2>, bool)> = Vec::new();
      for l in grid.contours() {
        pieces.extend(new_parts(&l, &fgrid, s));
      }
      while !pieces.is_empty() {
        let (k, flip) = nearest_piece(&pieces, at);
        let (mut pts, closed) = pieces.swap_remove(k);
        if flip && !closed {
          pts.reverse();
        }
        if closed {
          let i = pts
            .iter()
            .enumerate()
            .min_by(|a, b| (*a.1 - at).length().total_cmp(&(*b.1 - at).length()))
            .map(|(i, _)| i)
            .unwrap_or(0);
          pts.rotate_left(i);
        }
        let pts = keep_off_walls(region, simplify(&pts, (h * 0.1).min(0.01), closed), wall, closed);
        if pts.len() < 2 {
          continue;
        }
        let link = link(&grid, &cleared, &allowed, at, pts[0]);
        at = if closed { pts[0] } else { pts[pts.len() - 1] };
        steps.push(Step::Cut { pts, closed, link });
      }
      cleared = grow;
    }
  }
  Ok(steps)
}

/// The parts of an outline that cut new material (more than a third of the step-over from
/// what was cleared before), as (points, closed).
fn new_parts(l: &[DVec2], f: &Grid, s: f64) -> Vec<(Vec<DVec2>, bool)> {
  let fresh: Vec<bool> = l.iter().map(|q| f.sample(*q) > s / 3.0).collect();
  if fresh.iter().all(|x| *x) {
    return vec![(l.to_vec(), true)];
  }
  // runs of fresh points, starting after a stale one so a run doesn't wrap
  let n = l.len();
  let Some(start) = fresh.iter().position(|x| !*x) else { return Vec::new() };
  let mut out = Vec::new();
  let mut run: Vec<DVec2> = Vec::new();
  for k in 1..=n {
    let i = (start + k) % n;
    if fresh[i] {
      if run.is_empty() {
        // start from the stale neighbour so the pass reaches the old edge
        run.push(l[(i + n - 1) % n]);
      }
      run.push(l[i]);
    } else if !run.is_empty() {
      run.push(l[i]);
      out.push((std::mem::take(&mut run), false));
    }
  }
  if run.len() > 1 {
    out.push((run, false));
  }
  out
}

/// The piece whose start (or end, `flip`, for an open one) is nearest.
fn nearest_piece(pieces: &[(Vec<DVec2>, bool)], at: DVec2) -> (usize, bool) {
  let mut best = (0, false, f64::INFINITY);
  for (k, (pts, closed)) in pieces.iter().enumerate() {
    let d = if *closed {
      pts.iter().map(|q| (*q - at).length()).fold(f64::INFINITY, f64::min)
    } else {
      (pts[0] - at).length()
    };
    if d < best.2 {
      best = (k, false, d);
    }
    if !closed && (pts[pts.len() - 1] - at).length() < best.2 {
      best = (k, true, (pts[pts.len() - 1] - at).length());
    }
  }
  (best.0, best.1)
}

/// How to get from `a` to the start `b` of the next pass: straight when the way is cleared,
/// else through the cleared point nearest `b`, staying down if the way there is cleared.
fn link(grid: &Grid, cleared: &[bool], allowed: &[bool], a: DVec2, b: DVec2) -> Link {
  let clear = |a: DVec2, b: DVec2| {
    let n = (((b - a).length() / (grid.h * 0.5)).ceil() as usize).clamp(1, 100_000);
    (0..=n).all(|k| {
      let i = grid.index(a.lerp(b, k as f64 / n as f64));
      cleared[i] && allowed[i]
    })
  };
  if clear(a, b) {
    return Link::Direct;
  }
  let Some(k) = (0..cleared.len())
    .filter(|&i| cleared[i])
    .min_by(|x, y| (grid.node(*x) - b).length().total_cmp(&(grid.node(*y) - b).length()))
  else {
    return Link::Direct;
  };
  let q = grid.node(k);
  if clear(a, q) { Link::Through(q) } else { Link::Retract(q) }
}

/// Moves points that came out closer than `wall` to the region's boundary back out to it,
/// adding midpoints where a segment would still cut too close.
fn keep_off_walls(region: &Region, pts: Vec<DVec2>, wall: f64, closed: bool) -> Vec<DVec2> {
  let cap = wall + 1.0;
  let fix = |mut q: DVec2| {
    for _ in 0..6 {
      let d = region.sdf(q, cap);
      if d >= wall {
        break;
      }
      let e = 1e-4;
      let g = DVec2::new(
        region.sdf(q + DVec2::X * e, cap) - region.sdf(q - DVec2::X * e, cap),
        region.sdf(q + DVec2::Y * e, cap) - region.sdf(q - DVec2::Y * e, cap),
      );
      if g.length() < 1e-12 {
        break;
      }
      q += g.normalize() * (wall - d + 1e-5);
    }
    q
  };
  let pts: Vec<DVec2> = pts.into_iter().map(fix).collect();
  let mut out = Vec::with_capacity(pts.len());
  let m = if closed { pts.len() } else { pts.len().saturating_sub(1) };
  for i in 0..pts.len() {
    out.push(pts[i]);
    if i < m {
      let next = pts[(i + 1) % pts.len()];
      let mid = (pts[i] + next) / 2.0;
      if region.sdf(mid, cap) < wall - 1e-6 {
        out.push(fix(mid));
      }
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::super::{OpKind, Tool};
  use super::*;

  fn op(stepover: f64) -> Operation {
    Operation {
      name: "A".into(),
      kind: OpKind::Adaptive { floors: Vec::new() },
      tool: Tool::flat(1, 6.0),
      stepover,
      stepdown: 10.0,
      stock_to_leave: 0.0,
      entry: Entry::Helix,
      ramp_angle: 3.0,
      helix_diameter: 0.0,
    }
  }

  /// An L-shaped pocket 40 × 30 with a 15 × 12 notch, and a round island.
  fn pocket() -> Region {
    let v = |x: f64, y: f64| DVec3::new(x, y, 0.0);
    let outer = vec![v(0.0, 0.0), v(40.0, 0.0), v(40.0, 18.0), v(25.0, 18.0), v(25.0, 30.0), v(0.0, 30.0)];
    let island: Vec<DVec3> = (0..48)
      .rev()
      .map(|k| k as f64 / 48.0 * std::f64::consts::TAU)
      .map(|a| v(10.0 + 3.0 * a.cos(), 12.0 + 3.0 * a.sin()))
      .collect();
    Region::new(&[outer, island]).unwrap()
  }

  /// Engagement angles (degrees) along horizontal cutting moves at `z`, on a fine material
  /// grid; moves down to `z` (the last turn of a helix) remove material without counting.
  fn engagement(p: &Path, region: &Region, r: f64, z: f64) -> Vec<f64> {
    let h = 0.05;
    let g = Grid::new(region.lo, region.hi, h, 4, 0.0);
    let mut material: Vec<bool> = (0..g.v.len()).map(|i| region.contains(g.node(i))).collect();
    let mut out = Vec::new();
    let mut at = p.moves[0].to;
    for m in &p.moves {
      let horizontal = (m.to.z - z).abs() < 1e-9 && (at.z - z).abs() < 1e-9;
      if !m.rapid && m.to.z <= z + 1.0 {
        let len = (m.to - at).truncate().length();
        let n = (len / 0.1).ceil().max(1.0) as usize;
        for k in 0..=n {
          let c = at.lerp(m.to, k as f64 / n as f64).truncate();
          if horizontal && k > 0 {
            // material on a circle just outside the tool
            let hits =
              (0..360).filter(|a| material[g.index(c + DVec2::from_angle((*a as f64).to_radians()) * (r + h))]).count();
            out.push(hits as f64);
          }
          let rad = (r / h).ceil() as i64 + 1;
          let ci = g.index(c) as i64;
          for dj in -rad..=rad {
            for di in -rad..=rad {
              let k = ci + dj * g.nx as i64 + di;
              if k >= 0 && (k as usize) < material.len() && (g.node(k as usize) - c).length() <= r {
                material[k as usize] = false;
              }
            }
          }
        }
      }
      at = m.to;
    }
    out
  }

  #[test]
  fn adaptive_stays_inside_and_keeps_the_engagement_down() {
    let region = pocket();
    let op = op(0.1);
    let mut p = Path { moves: Vec::new(), feed: 1000.0, plunge: 300.0, safe: 20.0, top: 0.0 };
    clear(&mut p, &region, &op, &[-5.0], 0.0).unwrap();
    // the tool centre keeps a radius off the walls and the island
    for m in p.moves.iter().filter(|m| !m.rapid) {
      assert!(region.sdf(m.to.truncate(), 10.0) >= 3.0 - 1e-6, "{:?}", m.to);
      assert!(m.to.z >= -5.0 - 1e-9);
    }
    // it clears the pocket: the material wherever the tool's centre fits is cut
    let cuts: Vec<DVec2> =
      p.moves.iter().filter(|m| !m.rapid && (m.to.z + 5.0).abs() < 1e-9).map(|m| m.to.truncate()).collect();
    let mut segs = Vec::new();
    for w in p.moves.windows(2) {
      if !w[1].rapid && (w[0].to.z + 5.0).abs() < 1e-9 && (w[1].to.z + 5.0).abs() < 1e-9 {
        segs.push((w[0].to.truncate(), w[1].to.truncate()));
      }
    }
    assert!(cuts.len() > 50);
    let g = Grid::new(region.lo, region.hi, 0.5, 0, 0.0);
    for i in 0..g.v.len() {
      let q = g.node(i);
      if region.sdf(q, 10.0) >= 3.0 {
        let d = segs.iter().map(|(a, b)| super::super::region::seg_distance(q, *a, *b)).fold(f64::INFINITY, f64::min);
        assert!(d <= 3.0 - 0.05, "{q:?} {d}");
      }
    }
    // a 10% step-over takes about 37° of the tool (and this measure adds about 10° behind it);
    // where a pass meets a wall it takes more, but never a full slot
    let e = engagement(&p, &region, 3.0, -5.0);
    let mut sorted = e.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let p95 = sorted[sorted.len() * 95 / 100];
    let max = sorted[sorted.len() - 1];
    assert!(median <= 55.0 && p95 <= 100.0 && max <= 165.0, "median {median} p95 {p95} max {max}");
    // a ring pocket of the same region slots in
    let mut q = Path { moves: Vec::new(), feed: 1000.0, plunge: 300.0, safe: 20.0, top: 0.0 };
    q.enter(DVec3::new(20.0, 9.0, -5.0));
    q.cut(DVec3::new(33.0, 9.0, -5.0));
    let slot = engagement(&q, &region, 3.0, -5.0);
    assert!(slot.iter().copied().fold(0.0, f64::max) >= 179.0);
  }

  #[test]
  fn narrow_parts_are_reached_from_their_own_seed() {
    // two rooms joined by a channel narrower than the tool
    let v = |x: f64, y: f64| DVec3::new(x, y, 0.0);
    let outline = vec![
      v(0.0, 0.0),
      v(15.0, 0.0),
      v(15.0, 6.0),
      v(20.0, 6.0),
      v(20.0, 0.0),
      v(35.0, 0.0),
      v(35.0, 15.0),
      v(20.0, 15.0),
      v(20.0, 9.0),
      v(15.0, 9.0),
      v(15.0, 15.0),
      v(0.0, 15.0),
    ];
    let region = Region::new(&[outline]).unwrap();
    let plan = plan(&region, &op(0.15)).unwrap();
    let seeds = plan.iter().filter(|s| matches!(s, Step::Seed { .. })).count();
    assert_eq!(seeds, 2);
    assert!(plan.iter().all(|s| match s {
      Step::Cut { pts, .. } => pts.iter().all(|q| region.sdf(*q, 10.0) >= 3.0 - 1e-6),
      Step::Seed { c, rho } => region.sdf(*c, 10.0) >= 3.0 + rho,
    }));
  }
}
