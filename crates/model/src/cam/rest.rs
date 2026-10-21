//! Rest machining: a pocket that only cuts what the setup's earlier operations left, such as
//! the corners a larger tool couldn't reach. The earlier toolpaths are run through a stock
//! heightfield ([`super::sim`]) and each pass of the pocket is kept only where material still
//! stands under the tool.

use fox_kernel::glam::DVec3;

use super::sim::{self, Heightfield};
use super::{Entry, Operation, Path, Setup, Tool, Toolpath, oriented};
use crate::regen::Model;

/// Material a column must stand above the cutting level to count (mm).
pub(super) const LEFT: f64 = 0.02;

/// The stock after `previous`, on a grid fine enough for `tool`.
pub(super) fn remaining(
  setup: &Setup,
  model: &Model,
  previous: &[Toolpath],
  tool: &Tool,
) -> Result<Heightfield, String> {
  let (lo, hi) = super::stock(setup, model)?;
  let h = sim::cell_size(lo, hi, previous).min(tool.diameter / 12.0).max((hi - lo).truncate().max_element() / 1500.0);
  let mut stock = Heightfield::new(lo.truncate(), hi.truncate(), h, hi.z);
  let mut at = super::origin(setup, model)?;
  at.z += super::CLEARANCE;
  for tp in previous {
    sim::cut_only(&mut stock, tp, &mut at);
  }
  Ok(stock)
}

/// The pocket's rings at depth `z` where they still have material to cut, entering from `top`.
pub(super) fn cut_rings(p: &mut Path, left: &Heightfield, rings: &[Vec<Vec<DVec3>>], z: f64, top: f64, op: &Operation) {
  let r = op.tool.radius();
  // columns well inside the tool only, so the wall beside a tangent pass doesn't count
  let reach = (r - left.h * 0.5).max(left.h * 0.5);
  let slope = op.ramp_angle.clamp(0.5, 90.0).to_radians().tan();
  p.top = top;
  for ring in rings.iter().rev() {
    for lp in ring {
      let lp: Vec<DVec3> = oriented(lp, true).into_iter().map(|q| DVec3::new(q.x, q.y, z)).collect();
      let pts = resample(&lp, left.h);
      let n = pts.len();
      let hit: Vec<bool> = pts.iter().map(|q| left.material_above(q.truncate(), reach, z, LEFT)).collect();
      if !hit.iter().any(|x| *x) {
        continue;
      }
      // widen each run by half the tool so it starts and ends in cleared space
      let pad = ((r * 0.5) / left.h.max(1e-6)).ceil() as usize;
      let mut keep = vec![false; n];
      for (i, _) in hit.iter().enumerate().filter(|(_, x)| **x) {
        for k in 0..=2 * pad {
          keep[(i + n * 2 + k - pad) % n] = true;
        }
      }
      if keep.iter().all(|x| *x) {
        let entry = if op.entry == Entry::Plunge { Entry::Plunge } else { Entry::Ramp };
        p.cut_loop(&lp, top, entry, slope);
        p.leave();
        continue;
      }
      let start = keep.iter().position(|x| !*x).unwrap_or(0);
      let mut run: Vec<DVec3> = Vec::new();
      for k in 1..=n {
        let i = (start + k) % n;
        if keep[i] {
          run.push(pts[i]);
        } else if !run.is_empty() {
          cut_open(p, &std::mem::take(&mut run), top, op.entry, slope);
        }
      }
      if !run.is_empty() {
        cut_open(p, &run, top, op.entry, slope);
      }
    }
  }
  p.leave();
}

/// Points along a closed loop at most `step` apart.
fn resample(lp: &[DVec3], step: f64) -> Vec<DVec3> {
  let mut out = Vec::new();
  let n = lp.len();
  for i in 0..n {
    let (a, b) = (lp[i], lp[(i + 1) % n]);
    let k = ((b - a).length() / step.max(1e-6)).ceil().max(1.0) as usize;
    for j in 0..k {
      out.push(a.lerp(b, j as f64 / k as f64));
    }
  }
  out
}

/// An open pass: straight down at its start, or ramping back and forth along it from `top`.
fn cut_open(p: &mut Path, pts: &[DVec3], top: f64, entry: Entry, slope: f64) {
  if pts.len() < 2 {
    return;
  }
  let z = pts[0].z;
  let length: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
  if entry == Entry::Plunge || top <= z + 1e-9 || length < 1e-6 {
    p.enter(pts[0]);
  } else {
    p.enter(DVec3::new(pts[0].x, pts[0].y, top));
    // zigzag down along the pass, then back to its start at depth
    let mut zc = top;
    let mut forward = true;
    let mut i = 0usize;
    let mut guard = 0;
    while zc > z + 1e-9 && guard < super::MAX_PASSES {
      guard += 1;
      let j = if forward { i + 1 } else { i - 1 };
      let (a, b) = (pts[i], pts[j]);
      let seg = (b - a).length();
      if zc - seg * slope <= z {
        let t = (zc - z) / slope / seg.max(1e-12);
        let q = a.lerp(b, t.min(1.0));
        p.cut(DVec3::new(q.x, q.y, z));
        // back to the start at depth
        for k in (0..=if forward { i } else { j }).rev() {
          p.cut(pts[k]);
        }
        zc = z;
        break;
      }
      zc -= seg * slope;
      p.cut(DVec3::new(b.x, b.y, zc));
      i = j;
      if i == 0 || i == pts.len() - 1 {
        forward = i == 0;
      }
    }
    if zc > z + 1e-9 {
      for k in (0..=i).rev() {
        p.cut(pts[k]);
      }
    }
  }
  for q in &pts[1..] {
    p.cut(*q);
  }
  p.leave();
}
