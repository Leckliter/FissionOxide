//! 3D finishing on the triangulated part: parallel rows and constant-scallop rings, with the
//! tool "dropped" onto the part (its tip as low as it goes without touching the part's top
//! surface, sampled on a fine heightfield), so any tool shape follows the surface without
//! gouging it.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use fox_kernel::glam::{DVec2, DVec3};

use super::region::{Grid, simplify};
use super::sim::Heightfield;
use super::{Operation, Path, Setup, resolve};
use crate::FaceRef;
use crate::regen::Model;

/// The part's surface, what to machine of it and the tool dropped onto it.
pub(super) struct Surface<'a> {
  pub(super) part: Heightfield,
  /// Distance from each part cell to the nearest cell of the faces to machine.
  near: Vec<f64>,
  pub(super) op: &'a Operation,
  /// The tool's tip never goes below this (the part's bottom).
  pub(super) floor: f64,
  pub(super) lo: DVec2,
  pub(super) hi: DVec2,
}

impl Surface<'_> {
  pub(super) fn new<'a>(
    setup: &Setup,
    model: &Model,
    op: &'a Operation,
    faces: &[FaceRef],
  ) -> Result<Surface<'a>, String> {
    let (plo, phi) = super::part_box(setup, model)?;
    let r = op.tool.radius();
    let extent = (phi - plo).truncate().max_element().max(1e-3);
    let h = (op.tool.diameter / 30.0).min(op.tool.diameter * op.stepover / 4.0).clamp(extent / 1500.0, 0.25);
    let lo = plo.truncate() - DVec2::splat(r + h);
    let hi = phi.truncate() + DVec2::splat(r + h);
    let mut part = Heightfield::new(lo, hi, h, 0.0);
    let meshes: Vec<&fox_kernel::TriMesh> = super::bodies(setup, model).into_iter().filter_map(|b| b.mesh()).collect();
    if meshes.is_empty() {
      return Err("the part has no surface to machine".into());
    }
    part.rasterize_over(&meshes);
    // the faces to machine, on the same grid
    let mut target = part.clone();
    if !faces.is_empty() {
      let mut picked: Vec<fox_kernel::TriMesh> = Vec::new();
      for f in faces {
        let (b, id) = resolve(model, f)?;
        let m = b.mesh().ok_or("a body has no mesh")?;
        let mut sub = fox_kernel::TriMesh { positions: m.positions.clone(), ..Default::default() };
        for (t, tri) in m.indices.chunks_exact(3).enumerate() {
          if m.tri_face.get(t) == Some(&id) {
            sub.indices.extend_from_slice(tri);
          }
        }
        picked.push(sub);
      }
      target.rasterize_over(&picked.iter().collect::<Vec<_>>());
    }
    let cells = Grid { x0: part.x0 + h / 2.0, y0: part.y0 + h / 2.0, h, nx: part.nx, ny: part.ny, v: Vec::new() };
    let set: Vec<bool> = target.z.iter().map(|z| z.is_finite()).collect();
    if !set.iter().any(|x| *x) {
      return Err("there is no surface to machine under the faces".into());
    }
    let near = cells.distance_to(&set);
    // the area the tool's centre covers: within its radius of the faces
    let (mut tlo, mut thi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for (k, s) in set.iter().enumerate() {
      if *s {
        let c = part.centre(k % part.nx, k / part.nx);
        tlo = tlo.min(c);
        thi = thi.max(c);
      }
    }
    Ok(Surface { part, near, op, floor: plo.z, lo: tlo - DVec2::splat(r), hi: thi + DVec2::splat(r) })
  }

  /// Whether the tool at `c` touches the faces to machine.
  pub(super) fn covers(&self, c: DVec2) -> bool {
    let p = &self.part;
    let i = ((c.x - p.x0) / p.h - 0.5).round();
    let j = ((c.y - p.y0) / p.h - 0.5).round();
    if i < 0.0 || j < 0.0 || i >= p.nx as f64 || j >= p.ny as f64 {
      return false;
    }
    self.near[j as usize * p.nx + i as usize] <= self.op.tool.radius()
  }

  pub(super) fn drop(&self, c: DVec2) -> DVec3 {
    DVec3::new(c.x, c.y, self.part.drop_tool_over(&self.op.tool, c).max(self.floor))
  }

  /// A straight move dropped onto the surface, sampled finely enough to follow it.
  pub(super) fn dropped(&self, a: DVec2, b: DVec2) -> Vec<DVec3> {
    let n = (((b - a).length() / self.part.h).ceil() as usize).max(1);
    (0..=n).map(|k| self.drop(a.lerp(b, k as f64 / n as f64))).collect()
  }

  /// Emits passes (tool centre polylines in XY), dropped, nearest first, staying down between
  /// passes less than `link` apart.
  pub(super) fn emit(&self, p: &mut Path, passes: Vec<(Vec<DVec2>, bool)>, link: f64) {
    let tol = (self.part.h * 0.05).clamp(0.001, 0.005);
    let mut at: Option<DVec3> = None;
    for (pts, closed) in passes {
      let mut out: Vec<DVec3> = Vec::new();
      let m = if closed { pts.len() } else { pts.len() - 1 };
      for i in 0..m {
        let seg = self.dropped(pts[i], pts[(i + 1) % pts.len()]);
        out.extend(if out.is_empty() { &seg[..] } else { &seg[1..] });
      }
      let out = simplify3(&out, tol);
      if out.len() < 2 {
        continue;
      }
      match at {
        Some(a) if (a.truncate() - out[0].truncate()).length() <= link => {
          for q in self.dropped(a.truncate(), out[0].truncate()).into_iter().skip(1) {
            p.cut(q);
          }
        }
        _ => {
          p.leave();
          p.enter(out[0]);
        }
      }
      for q in &out[1..] {
        p.cut(*q);
      }
      at = out.last().copied();
    }
    p.leave();
  }
}

/// Drops points within `tol` of the 3D line through their neighbours, and never where that
/// line would pass below a dropped point (that would cut into the surface).
pub(super) fn simplify3(points: &[DVec3], tol: f64) -> Vec<DVec3> {
  if points.len() < 3 {
    return points.to_vec();
  }
  let fits = |p: DVec3, a: DVec3, b: DVec3| {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
    // the line's height over the point, measured along the move in XY
    let txy = ((p - a).truncate().dot(ab.truncate()) / ab.truncate().length_squared().max(1e-300)).clamp(0.0, 1.0);
    (a + ab * t - p).length() <= tol && a.z + ab.z * txy >= p.z - 1e-6
  };
  let mut out = vec![points[0]];
  let mut anchor = 0;
  for i in 1..points.len() - 1 {
    if !(anchor + 1..=i).all(|k| fits(points[k], points[anchor], points[i + 1])) {
      out.push(points[i]);
      anchor = i;
    }
  }
  out.push(points[points.len() - 1]);
  out
}

/// Parallel rows `angle` degrees from the X axis, a step-over apart, zigzagging.
pub(super) fn parallel(
  p: &mut Path,
  setup: &Setup,
  model: &Model,
  op: &Operation,
  faces: &[FaceRef],
  angle: f64,
) -> Result<(), String> {
  let s = Surface::new(setup, model, op, faces)?;
  let step = op.tool.diameter * op.stepover;
  let passes = rows(&s, step, angle, &|c| s.covers(c))?;
  if passes.is_empty() {
    return Err("the tool can't reach the faces".into());
  }
  s.emit(p, passes, step * 1.5);
  Ok(())
}

/// Rows `angle` degrees from the X axis, `step` apart, zigzagging: their runs where `keep`
/// holds, as tool centre polylines in XY.
pub(super) fn rows(
  s: &Surface,
  step: f64,
  angle: f64,
  keep: &dyn Fn(DVec2) -> bool,
) -> Result<Vec<(Vec<DVec2>, bool)>, String> {
  let a = angle.to_radians();
  let (u, v) = (DVec2::new(a.cos(), a.sin()), DVec2::new(-a.sin(), a.cos()));
  let corners = [s.lo, DVec2::new(s.hi.x, s.lo.y), s.hi, DVec2::new(s.lo.x, s.hi.y)];
  let (u0, u1) =
    corners.iter().map(|c| c.dot(u)).fold((f64::INFINITY, f64::NEG_INFINITY), |m, x| (m.0.min(x), m.1.max(x)));
  let (v0, v1) =
    corners.iter().map(|c| c.dot(v)).fold((f64::INFINITY, f64::NEG_INFINITY), |m, x| (m.0.min(x), m.1.max(x)));
  let rows = ((v1 - v0) / step).ceil() as usize + 1;
  if rows > super::MAX_PASSES {
    return Err("too many passes: raise the step-over".into());
  }
  let du = s.part.h;
  let n = ((u1 - u0) / du).ceil() as usize + 1;
  let mut passes = Vec::new();
  for k in 0..rows {
    let vv = (v0 + k as f64 * step).min(v1);
    // the row's runs over the faces
    let mut runs: Vec<Vec<DVec2>> = Vec::new();
    let mut run = Vec::new();
    for i in 0..n {
      let c = u * (u0 + i as f64 * du).min(u1) + v * vv;
      if keep(c) {
        run.push(c);
      } else if !run.is_empty() {
        runs.push(std::mem::take(&mut run));
      }
    }
    if !run.is_empty() {
      runs.push(run);
    }
    if k % 2 == 1 {
      runs.reverse();
      for r in &mut runs {
        r.reverse();
      }
    }
    passes.extend(runs.into_iter().filter(|r| r.len() > 1).map(|r| (r, false)));
  }
  Ok(passes)
}

/// Rings a constant distance apart measured over the surface (so the scallops left between
/// them stay about the same on steep and flat areas), from the outline of the faces inwards.
pub(super) fn scallop(
  p: &mut Path,
  setup: &Setup,
  model: &Model,
  op: &Operation,
  faces: &[FaceRef],
) -> Result<(), String> {
  let s = Surface::new(setup, model, op, faces)?;
  let step = op.tool.diameter * op.stepover;
  let h = (step / 3.0).max(s.part.h);
  let mut g = Grid::new(s.lo, s.hi, h, 2, 0.0);
  let n = g.v.len();
  let inside: Vec<bool> = (0..n).map(|i| s.covers(g.node(i))).collect();
  let tip: Vec<DVec3> = (0..n).map(|i| if inside[i] { s.drop(g.node(i)) } else { DVec3::ZERO }).collect();
  // distance over the surface from the outline (Dijkstra on 16 neighbours)
  let (nx, ny) = (g.nx as i64, g.ny as i64);
  let mut dist = vec![f64::INFINITY; n];
  let mut heap = BinaryHeap::new();
  let steps: [(i64, i64); 16] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (2, 1),
    (2, -1),
    (-2, 1),
    (-2, -1),
    (1, 2),
    (1, -2),
    (-1, 2),
    (-1, -2),
  ];
  for k in 0..n {
    let (i, j) = ((k as i64) % nx, (k as i64) / nx);
    let edge = steps[..4].iter().any(|(di, dj)| {
      let (a, b) = (i + di, j + dj);
      a < 0 || b < 0 || a >= nx || b >= ny || !inside[(b * nx + a) as usize]
    });
    if inside[k] && edge {
      dist[k] = 0.0;
      heap.push(Item(0.0, k));
    }
  }
  while let Some(Item(d, k)) = heap.pop() {
    if d > dist[k] {
      continue;
    }
    let (i, j) = ((k as i64) % nx, (k as i64) / nx);
    for (di, dj) in steps {
      let (a, b) = (i + di, j + dj);
      if a < 0 || b < 0 || a >= nx || b >= ny {
        continue;
      }
      let m = (b * nx + a) as usize;
      if !inside[m] {
        continue;
      }
      let nd = d + (tip[m] - tip[k]).length();
      if nd < dist[m] {
        dist[m] = nd;
        heap.push(Item(nd, m));
      }
    }
  }
  let deepest = dist.iter().copied().filter(|d| d.is_finite()).fold(0.0, f64::max);
  let mut passes = Vec::new();
  let mut level = h * 0.5;
  while level <= deepest + 1e-9 {
    if passes.len() > super::MAX_PASSES {
      return Err("too many passes: raise the step-over".into());
    }
    for (v, d) in g.v.iter_mut().zip(&dist) {
      *v = if d.is_finite() { level - d } else { h };
    }
    for l in g.contours() {
      let l = simplify(&l, (h * 0.05).min(0.005), true);
      if l.len() > 2 {
        passes.push((l, true));
      }
    }
    level += step;
  }
  if passes.is_empty() {
    return Err("the tool can't reach the faces".into());
  }
  s.emit(p, passes, step * 1.5);
  Ok(())
}

/// A heap entry ordered by smallest distance first.
#[derive(PartialEq)]
struct Item(f64, usize);

impl Eq for Item {}

impl PartialOrd for Item {
  fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
    Some(self.cmp(o))
  }
}

impl Ord for Item {
  fn cmp(&self, o: &Self) -> Ordering {
    o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
  }
}
