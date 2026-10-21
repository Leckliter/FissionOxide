//! Collision checks of the tool and its holder while the toolpaths are generated: the holder
//! (its stepped cylinders, see [`super::Tool::holder_sections`]) against the finished part,
//! which is always there, and the whole tool and holder against the setup's fixtures (vise
//! jaws, clamps), which are never to be touched. The stock in between is checked by the
//! simulation ([`super::sim`]), which runs the fixture check too.
//!
//! Obstacles are heightfields seen from above (the tool and holder reach up to the spindle,
//! so only the top of what is under them matters), with the highest column per block kept for
//! quick "how high is it under this holder step" answers.

use fox_kernel::glam::{DVec2, DVec3};

use super::sim::{Blocks, GOUGE_TOLERANCE, Heightfield, SIDE_TOLERANCE, Warning, WarningKind};
use super::{Setup, Tool, Toolpath};
use crate::regen::{Body, Model};

/// Something the tool must keep off: its top surface, and its extent.
pub(super) struct Obstacle {
  hf: Heightfield,
  blocks: Blocks,
  lo: DVec2,
  hi: DVec2,
  top: f64,
}

impl Obstacle {
  /// The top of some bodies on a grid of cell about `h` (None when they have no surface).
  pub(super) fn of(bodies: &[&Body], h: f64) -> Option<Obstacle> {
    let meshes: Vec<&fox_kernel::TriMesh> = bodies.iter().filter_map(|b| b.mesh()).collect();
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for m in &meshes {
      for p in &m.positions {
        let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        lo = lo.min(p);
        hi = hi.max(p);
      }
    }
    if !lo.is_finite() || !hi.is_finite() {
      return None;
    }
    let extent = (hi - lo).truncate().max_element().max(1e-3);
    let h = h.max(extent / 1000.0);
    let pad = DVec2::splat(h);
    let mut hf = Heightfield::new(lo.truncate() - pad, hi.truncate() + pad, h, 0.0);
    hf.rasterize(&meshes);
    let blocks = Blocks::new(&hf, 8.0 * h);
    Some(Obstacle { hf, blocks, lo: lo.truncate(), hi: hi.truncate(), top: hi.z })
  }

  /// How far the tool moving from `a` to `b` runs into the obstacle at worst (over the
  /// tolerance), and where it first does: its holder, and its cutting end too with `tool_too`.
  pub(super) fn hit(&mut self, tool: &Tool, a: DVec3, b: DVec3, tool_too: bool) -> Option<(DVec3, f64)> {
    let sections = tool.holder_sections();
    let r = tool.radius();
    let reach = sections.iter().map(|s| s.1).fold(if tool_too { r } else { 0.0 }, f64::max);
    if reach <= 0.0 {
      return None;
    }
    // quick outs: the lowest part checked stays above it, or the move stays clear of it
    let low = if tool_too { 0.0 } else { sections[0].0 };
    if a.z.min(b.z) + low >= self.top {
      return None;
    }
    let (mlo, mhi) = (a.truncate().min(b.truncate()) - DVec2::splat(reach), a.truncate().max(b.truncate()) + reach);
    if mhi.x < self.lo.x || mhi.y < self.lo.y || mlo.x > self.hi.x || mlo.y > self.hi.y {
      return None;
    }
    let step = self.blocks.size.min(r.max(self.hf.h)).max(self.hf.h);
    let n = ((b - a).length() / step).ceil().max(1.0) as usize;
    let mut first: Option<DVec3> = None;
    let mut worst: f64 = 0.0;
    for s in 0..=n {
      let p = a.lerp(b, s as f64 / n as f64);
      let q = p.truncate();
      if (q - q.clamp(self.lo, self.hi)).length() > reach || p.z + low >= self.top {
        continue;
      }
      let mut over = f64::NEG_INFINITY;
      if tool_too {
        let hf = &self.hf;
        hf.around(q, (r - SIDE_TOLERANCE).max(0.0), |k, d| {
          if let Some(prof) = tool.profile(d) {
            over = over.max(hf.z[k] - (p.z + prof));
          }
        });
      }
      for (z0, rad) in &sections {
        over = over.max(self.blocks.highest(&self.hf, q, (rad - SIDE_TOLERANCE).max(0.0)) - (p.z + z0));
      }
      if over > GOUGE_TOLERANCE {
        first.get_or_insert(p);
        worst = worst.max(over);
      }
    }
    first.map(|p| (p, worst))
  }
}

/// The setup's fixtures as one obstacle, on a grid fine enough for the toolpaths' tools.
pub(super) fn fixtures(setup: &Setup, model: &Model, paths: &[Toolpath]) -> Option<Obstacle> {
  let bodies = super::fixtures(setup, model);
  if bodies.is_empty() {
    return None;
  }
  Obstacle::of(&bodies, cell(paths))
}

fn cell(paths: &[Toolpath]) -> f64 {
  let dmin = paths.iter().map(|t| t.tool.diameter).fold(f64::INFINITY, f64::min);
  if dmin.is_finite() { (dmin / 8.0).clamp(0.05, 1.0) } else { 0.5 }
}

/// Warnings of one kind or another for one operation: where each first happened, how deep it
/// got at worst and how many moves had it.
pub(super) struct Tally {
  name: String,
  out: Vec<Warning>,
  counted: Vec<WarningKind>,
}

impl Tally {
  pub(super) fn new(name: &str) -> Tally {
    Tally { name: name.to_string(), out: Vec::new(), counted: Vec::new() }
  }

  pub(super) fn note(&mut self, kind: WarningKind, at: DVec3, depth: f64) {
    let fresh = !self.counted.contains(&kind);
    match self.out.iter_mut().find(|w| w.kind == kind) {
      Some(w) => {
        w.depth = w.depth.max(depth);
        if fresh {
          w.moves += 1;
        }
      }
      None => self.out.push(Warning { operation: self.name.clone(), kind, at, depth, moves: 1 }),
    }
    if fresh {
      self.counted.push(kind);
    }
  }

  /// Done with a move: the next one counts again.
  pub(super) fn next_move(&mut self) {
    self.counted.clear();
  }

  pub(super) fn done(self) -> Vec<Warning> {
    self.out
  }
}

/// Collisions found while generating a setup's toolpaths: tool holders running into the part,
/// and tools or holders running into the fixtures.
pub fn check(setup: &Setup, model: &Model, paths: &[Toolpath]) -> Result<Vec<Warning>, String> {
  let holders = paths.iter().any(|t| !t.tool.holder_sections().is_empty());
  let mut part = if holders { Obstacle::of(&super::bodies(setup, model), cell(paths)) } else { None };
  let mut fix = fixtures(setup, model, paths);
  if part.is_none() && fix.is_none() {
    return Ok(Vec::new());
  }
  let mut at = super::origin(setup, model)?;
  at.z += super::CLEARANCE;
  let mut out = Vec::new();
  for tp in paths {
    let mut tally = Tally::new(&tp.name);
    for m in &tp.moves {
      if let Some(o) = part.as_mut()
        && let Some((p, d)) = o.hit(&tp.tool, at, m.to, false)
      {
        tally.note(WarningKind::Holder, p, d);
      }
      if let Some(o) = fix.as_mut()
        && let Some((p, d)) = o.hit(&tp.tool, at, m.to, true)
      {
        tally.note(WarningKind::Fixture, p, d);
      }
      tally.next_move();
      at = m.to;
    }
    out.extend(tally.done());
  }
  Ok(out)
}
