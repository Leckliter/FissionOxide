//! Stock removal simulation on a heightfield (one Z column per grid cell, a "dexel" seen from
//! above): every cutting move lowers the columns under the tool's shape, and the result is
//! checked against the part's own heightfield for gouges, rapids into the stock and material
//! above the flutes.

use fox_kernel::glam::{DVec2, DVec3};

use super::{Setup, Tool, Toolpath};
use crate::regen::Model;

/// Depth below the part's surface that counts as a gouge (mm).
pub const GOUGE_TOLERANCE: f64 = 0.02;
/// How far inside the tool's radius a column must be to count for a gouge (mm), so a tool
/// running tangent to a wall doesn't trip on the column next to it.
const SIDE_TOLERANCE: f64 = 0.03;

/// Heights of cell columns on a regular grid; cell (i, j) is centred at
/// (x0 + (i + ½) h, y0 + (j + ½) h).
#[derive(Debug, Clone)]
pub struct Heightfield {
  pub x0: f64,
  pub y0: f64,
  pub h: f64,
  pub nx: usize,
  pub ny: usize,
  pub z: Vec<f64>,
}

impl Heightfield {
  pub fn new(lo: DVec2, hi: DVec2, h: f64, z: f64) -> Heightfield {
    let nx = (((hi.x - lo.x) / h).round() as usize).max(1);
    let ny = (((hi.y - lo.y) / h).round() as usize).max(1);
    // stretch the cells a little so the grid covers the box exactly
    let h = ((hi.x - lo.x) / nx as f64).max((hi.y - lo.y) / ny as f64).max(1e-6);
    Heightfield { x0: lo.x, y0: lo.y, h, nx, ny, z: vec![z; nx * ny] }
  }

  pub fn centre(&self, i: usize, j: usize) -> DVec2 {
    DVec2::new(self.x0 + (i as f64 + 0.5) * self.h, self.y0 + (j as f64 + 0.5) * self.h)
  }

  /// Cells whose centres are within `radius` of a point: (index, distance).
  fn around(&self, c: DVec2, radius: f64, mut f: impl FnMut(usize, f64)) {
    let i0 = (((c.x - radius - self.x0) / self.h - 0.5).ceil().max(0.0)) as usize;
    let j0 = (((c.y - radius - self.y0) / self.h - 0.5).ceil().max(0.0)) as usize;
    let i1 = ((c.x + radius - self.x0) / self.h - 0.5).floor();
    let j1 = ((c.y + radius - self.y0) / self.h - 0.5).floor();
    if i1 < 0.0 || j1 < 0.0 {
      return;
    }
    let (i1, j1) = ((i1 as usize).min(self.nx - 1), (j1 as usize).min(self.ny - 1));
    for j in j0..=j1 {
      for i in i0..=i1 {
        let d = (self.centre(i, j) - c).length();
        if d <= radius {
          f(j * self.nx + i, d);
        }
      }
    }
  }

  /// Whether material stands above `z` + `tol` within `radius` of a point.
  pub fn material_above(&self, c: DVec2, radius: f64, z: f64, tol: f64) -> bool {
    let mut any = false;
    self.around(c, radius, |k, _| any |= self.z[k] > z + tol);
    any
  }

  /// The lowest the tool's tip may go at a point without cutting into the columns within its
  /// radius ("drop cutter"), each column standing for its whole square (see
  /// [`Heightfield::rasterize_over`]).
  pub fn drop_tool_over(&self, tool: &Tool, c: DVec2) -> f64 {
    let mut top = f64::NEG_INFINITY;
    let half = self.h / 2.0;
    self.around(c, tool.radius() + half * std::f64::consts::SQRT_2, |k, _| {
      let q = self.centre(k % self.nx, k / self.nx);
      let d = DVec2::new(((c.x - q.x).abs() - half).max(0.0), ((c.y - q.y).abs() - half).max(0.0)).length();
      if let Some(p) = tool.profile(d) {
        top = top.max(self.z[k] - p);
      }
    });
    top
  }

  /// The highest the given meshes reach over each cell's square (an upper bound; −∞ where they
  /// don't reach), for keeping a tool off them.
  pub fn rasterize_over(&mut self, meshes: &[&fox_kernel::TriMesh]) {
    self.z.fill(f64::NEG_INFINITY);
    for m in meshes {
      for tri in m.indices.chunks_exact(3) {
        let p = |i: u32| {
          let v = m.positions[i as usize];
          DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
        };
        let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
        let top = a.z.max(b.z).max(c.z);
        let det = (b - a).truncate().perp_dot((c - a).truncate());
        // the triangle's plane, unless it stands on edge
        let plane = |q: DVec2| {
          let u = (q - a.truncate()).perp_dot((c - a).truncate()) / det;
          let v = (b - a).truncate().perp_dot(q - a.truncate()) / det;
          a.z + u * (b.z - a.z) + v * (c.z - a.z)
        };
        let lo = a.min(b).min(c).truncate();
        let hi = a.max(b).max(c).truncate();
        let i0 = ((lo.x - self.x0) / self.h).floor().max(0.0) as usize;
        let j0 = ((lo.y - self.y0) / self.h).floor().max(0.0) as usize;
        let (fi, fj) = (((hi.x - self.x0) / self.h).floor(), ((hi.y - self.y0) / self.h).floor());
        if fi < 0.0 || fj < 0.0 {
          continue;
        }
        let (i1, j1) = ((fi as usize).min(self.nx - 1), (fj as usize).min(self.ny - 1));
        let t = [a.truncate(), b.truncate(), c.truncate()];
        for j in j0..=j1 {
          for i in i0..=i1 {
            let s0 = DVec2::new(self.x0 + i as f64 * self.h, self.y0 + j as f64 * self.h);
            let s1 = s0 + DVec2::splat(self.h);
            if !overlaps(&t, s0, s1) {
              continue;
            }
            let z = if det.abs() < 1e-9 * (hi - lo).length_squared().max(1e-12) {
              top
            } else {
              [s0, DVec2::new(s1.x, s0.y), s1, DVec2::new(s0.x, s1.y)]
                .iter()
                .map(|q| plane(*q))
                .fold(f64::NEG_INFINITY, f64::max)
                .min(top)
            };
            let k = j * self.nx + i;
            self.z[k] = self.z[k].max(z);
          }
        }
      }
    }
  }

  /// The top surface of the given meshes, sampled at the cell centres (−∞ where there is none).
  pub fn rasterize(&mut self, meshes: &[&fox_kernel::TriMesh]) {
    self.z.fill(f64::NEG_INFINITY);
    for m in meshes {
      for tri in m.indices.chunks_exact(3) {
        let p = |i: u32| {
          let v = m.positions[i as usize];
          DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
        };
        let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
        let det = (b - a).truncate().perp_dot((c - a).truncate());
        if det.abs() < 1e-12 {
          continue;
        }
        let lo = a.min(b).min(c).truncate();
        let hi = a.max(b).max(c).truncate();
        let i0 = (((lo.x - self.x0) / self.h - 0.5).ceil().max(0.0)) as usize;
        let j0 = (((lo.y - self.y0) / self.h - 0.5).ceil().max(0.0)) as usize;
        let (fi, fj) = (((hi.x - self.x0) / self.h - 0.5).floor(), ((hi.y - self.y0) / self.h - 0.5).floor());
        if fi < 0.0 || fj < 0.0 {
          continue;
        }
        let (i1, j1) = ((fi as usize).min(self.nx - 1), (fj as usize).min(self.ny - 1));
        for j in j0..=j1 {
          for i in i0..=i1 {
            let q = self.centre(i, j);
            let u = (q - a.truncate()).perp_dot((c - a).truncate()) / det;
            let v = (b - a).truncate().perp_dot(q - a.truncate()) / det;
            if u < -1e-9 || v < -1e-9 || u + v > 1.0 + 1e-9 {
              continue;
            }
            let z = a.z + u * (b.z - a.z) + v * (c.z - a.z);
            let k = j * self.nx + i;
            self.z[k] = self.z[k].max(z);
          }
        }
      }
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
  /// The tool cut into the part.
  Gouge,
  /// A rapid move ran into the stock.
  RapidCollision,
  /// Material stood above the flutes: the shank or holder rubs.
  Shank,
  /// The tool holder ran into the stock or the part.
  Holder,
}

impl WarningKind {
  pub fn id(self) -> &'static str {
    match self {
      WarningKind::Gouge => "gouge",
      WarningKind::RapidCollision => "rapid_collision",
      WarningKind::Shank => "shank",
      WarningKind::Holder => "holder",
    }
  }
}

/// One kind of problem in one operation: where it first happened, how deep it got at worst and
/// how many moves had it.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
  pub operation: String,
  pub kind: WarningKind,
  pub at: DVec3,
  pub depth: f64,
  pub moves: usize,
}

impl Warning {
  pub fn message(&self) -> String {
    let what = match self.kind {
      WarningKind::Gouge => "cuts into the part",
      WarningKind::RapidCollision => "rapids into the stock",
      WarningKind::Shank => "cuts deeper than the flutes",
      WarningKind::Holder => "runs the tool holder into the material",
    };
    format!(
      "{} {what} ({} move(s), up to {:.3} mm, first at X{:.3} Y{:.3} Z{:.3})",
      self.operation, self.moves, self.depth, self.at.x, self.at.y, self.at.z
    )
  }
}

/// The stock after a setup's toolpaths, and what went wrong on the way.
#[derive(Debug, Clone)]
pub struct Simulation {
  pub stock: Heightfield,
  /// The part's top surface on the same grid (−∞ off the part).
  pub part: Heightfield,
  /// Stock bottom.
  pub base: f64,
  pub warnings: Vec<Warning>,
  /// Removed volume per operation (mm³).
  pub removed: Vec<(String, f64)>,
  /// Cells cut into the part.
  pub gouged: Vec<bool>,
}

impl Simulation {
  /// Stock left above the part (mm³), ignoring cells off the part.
  pub fn remaining(&self) -> f64 {
    let a = self.stock.h * self.stock.h;
    self
      .stock
      .z
      .iter()
      .zip(&self.part.z)
      .map(|(s, p)| if p.is_finite() { (s - p).max(0.0) * a } else { (s - self.base).max(0.0) * a })
      .sum()
  }

  /// A mesh of the stock for display: the top surface (downsampled to at most `max` cells a
  /// side) and its four sides. Gouged cells are coloured red.
  pub fn mesh(&self, max: usize) -> SimMesh {
    let hf = &self.stock;
    let step = hf.nx.max(hf.ny).div_ceil(max.max(2)).max(1);
    let (mx, my) = (hf.nx.div_ceil(step), hf.ny.div_ceil(step));
    let mut m = SimMesh::default();
    let at = |i: usize, j: usize| -> (DVec3, bool) {
      // the lowest column of the block, so cuts narrower than a block still show
      let (mut z, mut g) = (f64::INFINITY, false);
      for jj in j * step..((j + 1) * step).min(hf.ny) {
        for ii in i * step..((i + 1) * step).min(hf.nx) {
          z = z.min(hf.z[jj * hf.nx + ii]);
          g |= self.gouged[jj * hf.nx + ii];
        }
      }
      let x = hf.x0 + ((i * step) as f64 + (step.min(hf.nx - i * step)) as f64 / 2.0) * hf.h;
      let y = hf.y0 + ((j * step) as f64 + (step.min(hf.ny - j * step)) as f64 / 2.0) * hf.h;
      (DVec3::new(x, y, z.max(self.base)), g)
    };
    let pts: Vec<(DVec3, bool)> = (0..my).flat_map(|j| (0..mx).map(move |i| (i, j))).map(|(i, j)| at(i, j)).collect();
    let stock = [196, 170, 120, 255];
    let red = [220, 40, 40, 255];
    for j in 0..my {
      for i in 0..mx {
        let (p, g) = pts[j * mx + i];
        let z = |i: usize, j: usize| pts[j.min(my - 1) * mx + i.min(mx - 1)].0.z;
        let dx = (z(i + 1, j) - z(i.saturating_sub(1), j)) / (2.0 * hf.h * step as f64);
        let dy = (z(i, j + 1) - z(i, j.saturating_sub(1))) / (2.0 * hf.h * step as f64);
        let n = DVec3::new(-dx, -dy, 1.0).normalize();
        m.positions.push(p.as_vec3().to_array());
        m.normals.push(n.as_vec3().to_array());
        m.colors.push(if g { red } else { stock });
      }
    }
    for j in 0..my.saturating_sub(1) {
      for i in 0..mx.saturating_sub(1) {
        let a = (j * mx + i) as u32;
        let (b, c, d) = (a + 1, a + 1 + mx as u32, a + mx as u32);
        m.indices.extend_from_slice(&[a, b, c, a, c, d]);
      }
    }
    // the sides, from the outermost cell centres down to the stock bottom
    let side = |m: &mut SimMesh, ring: Vec<usize>, normal: DVec3| {
      let base = m.positions.len() as u32;
      for &k in &ring {
        let p = pts[k].0;
        for z in [p.z, self.base] {
          m.positions.push(DVec3::new(p.x, p.y, z).as_vec3().to_array());
          m.normals.push(normal.as_vec3().to_array());
          m.colors.push(stock);
        }
      }
      for s in 0..ring.len().saturating_sub(1) as u32 {
        let (a, b) = (base + 2 * s, base + 2 * s + 2);
        m.indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
      }
    };
    side(&mut m, (0..mx).collect(), DVec3::NEG_Y);
    side(&mut m, (0..my).map(|j| j * mx + mx - 1).collect(), DVec3::X);
    side(&mut m, (0..mx).rev().map(|i| (my - 1) * mx + i).collect(), DVec3::Y);
    side(&mut m, (0..my).rev().map(|j| j * mx).collect(), DVec3::NEG_X);
    m
  }
}

/// A displayable triangle mesh (counter-clockwise seen from outside).
#[derive(Debug, Clone, Default)]
pub struct SimMesh {
  pub positions: Vec<[f32; 3]>,
  pub normals: Vec<[f32; 3]>,
  pub indices: Vec<u32>,
  pub colors: Vec<[u8; 4]>,
}

/// A grid size for a stock and the tools that cut it: fine enough for the smallest tool, at
/// most about a thousand cells a side.
pub fn cell_size(lo: DVec3, hi: DVec3, paths: &[Toolpath]) -> f64 {
  let extent = (hi.x - lo.x).max(hi.y - lo.y).max(1e-3);
  let dmin = paths.iter().map(|t| t.tool.diameter).fold(f64::INFINITY, f64::min);
  let mut h = (extent / 350.0).max(0.05);
  if dmin.is_finite() && dmin / h < 8.0 {
    h = (dmin / 8.0).max(extent / 1000.0);
  }
  h
}

/// The stock of a setup before any cut, and the part's top surface, on a grid of cell `h`.
pub fn start(setup: &Setup, model: &Model, h: f64) -> Result<(Heightfield, Heightfield, f64), String> {
  let (lo, hi) = super::stock(setup, model)?;
  let stock = Heightfield::new(lo.truncate(), hi.truncate(), h, hi.z);
  let mut part = stock.clone();
  let meshes: Vec<&fox_kernel::TriMesh> = super::bodies(setup, model).into_iter().filter_map(|b| b.mesh()).collect();
  part.rasterize(&meshes);
  Ok((stock, part, lo.z))
}

/// Runs a setup's toolpaths through the stock.
pub fn simulate(setup: &Setup, model: &Model, paths: &[Toolpath]) -> Result<Simulation, String> {
  let (lo, hi) = super::stock(setup, model)?;
  let h = cell_size(lo, hi, paths);
  let (stock, part, base) = start(setup, model, h)?;
  let mut sim =
    Simulation { gouged: vec![false; stock.z.len()], stock, part, base, warnings: Vec::new(), removed: Vec::new() };
  let mut at = super::origin(setup, model)?;
  at.z += super::CLEARANCE;
  for tp in paths {
    let removed = run(&mut sim, tp, &mut at);
    sim.removed.push((tp.name.clone(), removed));
  }
  Ok(sim)
}

/// Cuts the stock along a toolpath only (no checks): for rest machining.
pub fn cut_only(stock: &mut Heightfield, tp: &Toolpath, at: &mut DVec3) {
  for m in &tp.moves {
    if !m.rapid {
      sweep(stock, &tp.tool, *at, m.to, |_, _, _, _| {});
    }
    *at = m.to;
  }
}

fn run(sim: &mut Simulation, tp: &Toolpath, at: &mut DVec3) -> f64 {
  let tool = &tp.tool;
  let mut warn: Vec<Warning> = Vec::new();
  let mut note = |kind: WarningKind, p: DVec3, depth: f64, counted: &mut [bool; 4]| {
    let slot = kind as usize;
    match warn.iter_mut().find(|w| w.kind == kind) {
      Some(w) => {
        w.depth = w.depth.max(depth);
        if !counted[slot] {
          w.moves += 1;
        }
      }
      None => warn.push(Warning { operation: tp.name.clone(), kind, at: p, depth, moves: 1 }),
    }
    counted[slot] = true;
  };
  let mut removed = 0.0;
  let area = sim.stock.h * sim.stock.h;
  let r = tool.radius();
  let mut holder = (tool.holder_diameter > 0.0 && tool.stickout > 0.0).then(|| Blocks::new(&sim.stock, tool));
  for m in &tp.moves {
    let mut counted = [false; 4];
    if let Some(b) = holder.as_mut() {
      // the holder's face, `stickout` above the tip, over the material along the move
      let len = (m.to - *at).length();
      let n = (len / b.size).ceil().max(1.0) as usize;
      for s in 0..=n {
        let p = at.lerp(m.to, s as f64 / n as f64);
        let over = b.highest(&sim.stock, p.truncate(), tool.holder_diameter / 2.0) - (p.z + tool.stickout);
        if over > GOUGE_TOLERANCE {
          note(WarningKind::Holder, p, over, &mut counted);
        }
      }
    }
    if m.rapid {
      // a rapid must clear the stock everywhere under the tool
      let len = (m.to - *at).length();
      let n = (len / sim.stock.h).ceil().max(1.0) as usize;
      for s in 0..=n {
        let p = at.lerp(m.to, s as f64 / n as f64);
        let mut worst: f64 = 0.0;
        sim.stock.around(p.truncate(), (r - SIDE_TOLERANCE).max(0.0), |k, d| {
          if let Some(prof) = tool.profile(d) {
            worst = worst.max(sim.stock.z[k] - (p.z + prof));
          }
        });
        if worst > GOUGE_TOLERANCE {
          note(WarningKind::RapidCollision, p, worst, &mut counted);
        }
      }
    } else {
      let (stock, part, gouged, base) = (&mut sim.stock, &sim.part, &mut sim.gouged, sim.base);
      let (blocks, nx) = (&mut holder, stock.nx);
      sweep(stock, tool, *at, m.to, |k, old, new, (p, d)| {
        if let Some(b) = blocks.as_mut() {
          b.lowered(k, nx);
        }
        removed += (old - new.max(base)).max(0.0) * area;
        if part.z[k].is_finite() && new < part.z[k] - GOUGE_TOLERANCE && d < r - SIDE_TOLERANCE {
          gouged[k] = true;
          note(WarningKind::Gouge, p, part.z[k] - new, &mut counted);
        }
        if tool.flute_length > 0.0 && old > p.z + tool.flute_length + GOUGE_TOLERANCE {
          note(WarningKind::Shank, p, old - p.z - tool.flute_length, &mut counted);
        }
      });
    }
    *at = m.to;
  }
  sim.warnings.extend(warn);
  removed
}

/// The highest column of each square block of cells, kept up to date lazily as cuts lower
/// them: quick answers to "how high is the material under the tool holder".
struct Blocks {
  /// Cells a block side.
  b: usize,
  bx: usize,
  by: usize,
  top: Vec<f64>,
  stale: Vec<bool>,
  /// Sampling step along moves (mm).
  size: f64,
}

impl Blocks {
  fn new(hf: &Heightfield, tool: &Tool) -> Blocks {
    let b = ((tool.holder_diameter / 12.0 / hf.h).floor() as usize).clamp(1, 64);
    let (bx, by) = (hf.nx.div_ceil(b), hf.ny.div_ceil(b));
    let mut out = Blocks { b, bx, by, top: vec![f64::NEG_INFINITY; bx * by], stale: vec![true; bx * by], size: 0.0 };
    out.size = (b as f64 * hf.h).max(hf.h);
    out
  }

  fn lowered(&mut self, cell: usize, nx: usize) {
    let (i, j) = (cell % nx, cell / nx);
    self.stale[(j / self.b) * self.bx + i / self.b] = true;
  }

  /// The highest column whose centre is within `radius` of a point.
  fn highest(&mut self, hf: &Heightfield, c: DVec2, radius: f64) -> f64 {
    let bs = self.b as f64 * hf.h;
    let (fx, fy) = ((c.x - hf.x0) / bs, (c.y - hf.y0) / bs);
    let reach = radius / bs + 1.0;
    let (i0, i1) = ((fx - reach).floor().max(0.0) as usize, ((fx + reach).floor().max(0.0) as usize).min(self.bx - 1));
    let (j0, j1) = ((fy - reach).floor().max(0.0) as usize, ((fy + reach).floor().max(0.0) as usize).min(self.by - 1));
    if fx + reach < 0.0 || fy + reach < 0.0 {
      return f64::NEG_INFINITY;
    }
    let mut best = f64::NEG_INFINITY;
    for bj in j0..=j1 {
      for bi in i0..=i1 {
        // the block's nearest point to the centre
        let lo = DVec2::new(hf.x0 + bi as f64 * bs, hf.y0 + bj as f64 * bs);
        let q = c.clamp(lo, lo + DVec2::splat(bs));
        if (q - c).length() > radius + hf.h {
          continue;
        }
        let k = bj * self.bx + bi;
        if self.stale[k] {
          let mut t = f64::NEG_INFINITY;
          for j in bj * self.b..((bj + 1) * self.b).min(hf.ny) {
            for i in bi * self.b..((bi + 1) * self.b).min(hf.nx) {
              t = t.max(hf.z[j * hf.nx + i]);
            }
          }
          self.top[k] = t;
          self.stale[k] = false;
        }
        if self.top[k] <= best {
          continue;
        }
        // look at the block's columns only when it could raise the answer
        for j in bj * self.b..((bj + 1) * self.b).min(hf.ny) {
          for i in bi * self.b..((bi + 1) * self.b).min(hf.nx) {
            if (hf.centre(i, j) - c).length() <= radius {
              best = best.max(hf.z[j * hf.nx + i]);
            }
          }
        }
      }
    }
    best
  }
}

/// Lowers the columns under the tool moving from `a` to `b`; `cut(cell, old, new, (tip, d))`
/// is called for every column it lowers.
fn sweep(hf: &mut Heightfield, tool: &Tool, a: DVec3, b: DVec3, mut cut: impl FnMut(usize, f64, f64, (DVec3, f64))) {
  let len = (b - a).length();
  let n = (len / hf.h).ceil().max(1.0) as usize;
  let r = tool.radius();
  for s in 0..=n {
    let p = a.lerp(b, s as f64 / n as f64);
    let (x0, y0, h, nx) = (hf.x0, hf.y0, hf.h, hf.nx);
    let i0 = (((p.x - r - x0) / h - 0.5).ceil().max(0.0)) as usize;
    let j0 = (((p.y - r - y0) / h - 0.5).ceil().max(0.0)) as usize;
    let (fi, fj) = (((p.x + r - x0) / h - 0.5).floor(), ((p.y + r - y0) / h - 0.5).floor());
    if fi < 0.0 || fj < 0.0 {
      continue;
    }
    let (i1, j1) = ((fi as usize).min(nx - 1), (fj as usize).min(hf.ny - 1));
    for j in j0..=j1 {
      for i in i0..=i1 {
        let c = DVec2::new(x0 + (i as f64 + 0.5) * h, y0 + (j as f64 + 0.5) * h);
        let d = (c - p.truncate()).length();
        let Some(prof) = tool.profile(d) else { continue };
        let k = j * nx + i;
        let z = p.z + prof;
        if z < hf.z[k] {
          let old = hf.z[k];
          hf.z[k] = z;
          cut(k, old, z, (p, d));
        }
      }
    }
  }
}

/// Whether a triangle and an axis-aligned square overlap in XY (separating axes).
fn overlaps(t: &[DVec2; 3], lo: DVec2, hi: DVec2) -> bool {
  let eps = 1e-9;
  let (tlo, thi) = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
  if tlo.x > hi.x + eps || thi.x < lo.x - eps || tlo.y > hi.y + eps || thi.y < lo.y - eps {
    return false;
  }
  let corners = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
  for k in 0..3 {
    let e = t[(k + 1) % 3] - t[k];
    let n = e.perp();
    if n.length_squared() < 1e-24 {
      continue;
    }
    let tri = t.iter().map(|p| p.dot(n));
    let (a0, a1) = tri.fold((f64::INFINITY, f64::NEG_INFINITY), |m, x| (m.0.min(x), m.1.max(x)));
    let (b0, b1) =
      corners.iter().map(|p| p.dot(n)).fold((f64::INFINITY, f64::NEG_INFINITY), |m, x| (m.0.min(x), m.1.max(x)));
    let tol = eps * n.length();
    if a0 > b1 + tol || b0 > a1 + tol {
      return false;
    }
  }
  true
}
