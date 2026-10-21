//! Stock removal simulation for 4/5-axis setups on a voxel grid: the tool, tilted along each
//! move's axis, clears the voxels it sweeps; voxels inside the part count as gouges. Each move's
//! axis also goes through the machine's kinematics, so tool directions beyond its rotary travel
//! are reported.

use fox_kernel::glam::{DVec2, DVec3};

use super::axes::Angles;
use super::sim::{GOUGE_TOLERANCE, SimMesh, Warning, WarningKind};
use super::{Setup, Tool, Toolpath};
use crate::regen::Model;

const STOCK: u8 = 1;
const PART: u8 = 2;
const GOUGED: u8 = 4;
/// Most voxels a simulation uses.
const MAX_VOXELS: f64 = 4.0e6;
/// Voxels a block side, for skipping empty space.
const BLOCK: usize = 8;

/// The stock as voxels after a setup's toolpaths.
#[derive(Debug, Clone)]
pub struct VoxelSim {
  /// Corner of voxel (0, 0, 0) and the voxel size.
  pub lo: DVec3,
  pub h: f64,
  pub n: [usize; 3],
  /// Flags per voxel (stock, part, gouged), X fastest.
  pub cells: Vec<u8>,
  pub warnings: Vec<Warning>,
  /// Removed volume per operation (mm³).
  pub removed: Vec<(String, f64)>,
  /// The range each rotary axis went through (degrees): (tilt min, tilt max, turn min, turn max).
  pub rotary: Option<(f64, f64, f64, f64)>,
  /// Stock voxels in each block of BLOCK³.
  blocks: Vec<u32>,
}

/// Collects one warning per kind for an operation, counting the moves that had it.
pub(crate) struct Notes {
  op: String,
  pub(crate) list: Vec<Warning>,
  counted: Vec<WarningKind>,
}

impl Notes {
  pub(crate) fn new(op: &str) -> Notes {
    Notes { op: op.to_string(), list: Vec::new(), counted: Vec::new() }
  }

  /// Starts a new move.
  pub(crate) fn next(&mut self) {
    self.counted.clear();
  }

  pub(crate) fn note(&mut self, kind: WarningKind, at: DVec3, depth: f64) {
    let fresh = !self.counted.contains(&kind);
    match self.list.iter_mut().find(|w| w.kind == kind) {
      Some(w) => {
        w.depth = w.depth.max(depth);
        if fresh {
          w.moves += 1;
        }
      }
      None => self.list.push(Warning { operation: self.op.clone(), kind, at, depth, moves: 1 }),
    }
    if fresh {
      self.counted.push(kind);
    }
  }
}

impl VoxelSim {
  fn index(&self, i: usize, j: usize, k: usize) -> usize {
    (k * self.n[1] + j) * self.n[0] + i
  }

  fn nblocks(&self) -> [usize; 3] {
    [self.n[0].div_ceil(BLOCK), self.n[1].div_ceil(BLOCK), self.n[2].div_ceil(BLOCK)]
  }

  fn block(&self, i: usize, j: usize, k: usize) -> usize {
    let b = self.nblocks();
    ((k / BLOCK) * b[1] + j / BLOCK) * b[0] + i / BLOCK
  }

  pub fn centre(&self, i: usize, j: usize, k: usize) -> DVec3 {
    self.lo + DVec3::new(i as f64 + 0.5, j as f64 + 0.5, k as f64 + 0.5) * self.h
  }

  /// Excess stock left on the part (mm³).
  pub fn remaining(&self) -> f64 {
    self.cells.iter().filter(|c| **c & (STOCK | PART) == STOCK).count() as f64 * self.h.powi(3)
  }

  /// Whether the stock is left at a point.
  pub fn stock_at(&self, p: DVec3) -> bool {
    let q = (p - self.lo) / self.h;
    if q.min_element() < 0.0 {
      return false;
    }
    let (i, j, k) = (q.x as usize, q.y as usize, q.z as usize);
    i < self.n[0] && j < self.n[1] && k < self.n[2] && self.cells[self.index(i, j, k)] & STOCK != 0
  }

  /// The stock's surface for display: blocks of voxels (at most `max` a side), solid where most
  /// of a block is stock, red where the tool cut into the part.
  pub fn mesh(&self, max: usize) -> SimMesh {
    let step = self.n.iter().max().copied().unwrap_or(1).div_ceil(max.max(2)).max(1);
    let b = [self.n[0].div_ceil(step), self.n[1].div_ceil(step), self.n[2].div_ceil(step)];
    let mut solid = vec![false; b[0] * b[1] * b[2]];
    let mut red = vec![false; solid.len()];
    for k in 0..self.n[2] {
      for j in 0..self.n[1] {
        for i in 0..self.n[0] {
          let c = self.cells[self.index(i, j, k)];
          let bi = ((k / step) * b[1] + j / step) * b[0] + i / step;
          red[bi] |= c & GOUGED != 0;
        }
      }
    }
    for bk in 0..b[2] {
      for bj in 0..b[1] {
        for bi in 0..b[0] {
          let (mut on, mut all) = (0, 0);
          for k in bk * step..((bk + 1) * step).min(self.n[2]) {
            for j in bj * step..((bj + 1) * step).min(self.n[1]) {
              for i in bi * step..((bi + 1) * step).min(self.n[0]) {
                all += 1;
                on += (self.cells[self.index(i, j, k)] & STOCK != 0) as usize;
              }
            }
          }
          solid[(bk * b[1] + bj) * b[0] + bi] = 2 * on > all;
        }
      }
    }
    let at = |i: isize, j: isize, k: isize| -> bool {
      i >= 0
        && j >= 0
        && k >= 0
        && (i as usize) < b[0]
        && (j as usize) < b[1]
        && (k as usize) < b[2]
        && solid[(k as usize * b[1] + j as usize) * b[0] + i as usize]
    };
    let size = self.h * step as f64;
    let mut m = SimMesh::default();
    let stock = [196, 170, 120, 255];
    let gouge = [220, 40, 40, 255];
    for k in 0..b[2] as isize {
      for j in 0..b[1] as isize {
        for i in 0..b[0] as isize {
          if !at(i, j, k) {
            continue;
          }
          let colour = if red[(k as usize * b[1] + j as usize) * b[0] + i as usize] { gouge } else { stock };
          let o = self.lo + DVec3::new(i as f64, j as f64, k as f64) * size;
          for (axis, sign) in [(0, -1), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)] {
            let mut nb = [i, j, k];
            nb[axis] += sign;
            if at(nb[0], nb[1], nb[2]) {
              continue;
            }
            let mut n = DVec3::ZERO;
            n[axis] = sign as f64;
            // the face's corners counter-clockwise seen from outside
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut base = o;
            if sign > 0 {
              base[axis] += size;
            }
            let mut du = DVec3::ZERO;
            let mut dv = DVec3::ZERO;
            du[u] = size;
            dv[v] = size;
            let (du, dv) = if sign > 0 { (du, dv) } else { (dv, du) };
            let first = m.positions.len() as u32;
            for p in [base, base + du, base + du + dv, base + dv] {
              m.positions.push(p.as_vec3().to_array());
              m.normals.push(n.as_vec3().to_array());
              m.colors.push(colour);
            }
            m.indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
          }
        }
      }
    }
    m
  }
}

/// Whether a point is inside the tool with its tip at `tip` and axis `a` (unit), `shrink`
/// inside its surface; the depth inside it.
fn inside(tool: &Tool, length: f64, tip: DVec3, a: DVec3, q: DVec3, shrink: f64) -> Option<f64> {
  let d = q - tip;
  let s = d.dot(a);
  if s < -1e-9 || s > length {
    return None;
  }
  let rho = (d - a * s).length();
  let r = tool.radius();
  if rho > r - shrink {
    return None;
  }
  let prof = tool.profile(rho)?;
  (s >= prof + shrink).then_some((r - rho).min(s - prof))
}

/// Runs a 4/5-axis setup's toolpaths through its stock.
pub fn simulate(setup: &Setup, model: &Model, paths: &[Toolpath]) -> Result<VoxelSim, String> {
  let (lo, hi) = super::stock(setup, model)?;
  let ext = hi - lo;
  let dmin = paths.iter().map(|t| t.tool.diameter).fold(f64::INFINITY, f64::min);
  let mut h = (ext.max_element() / 160.0).max(0.05);
  if dmin.is_finite() {
    h = h.min((dmin / 5.0).max(0.05));
  }
  h = h.max((ext.x * ext.y * ext.z / MAX_VOXELS).cbrt());
  let n = [(ext.x / h).ceil().max(1.0) as usize, (ext.y / h).ceil().max(1.0) as usize, (ext.z / h).ceil().max(1.0) as usize];
  let mut sim = VoxelSim {
    lo,
    h,
    n,
    cells: vec![0; n[0] * n[1] * n[2]],
    warnings: Vec::new(),
    removed: Vec::new(),
    rotary: None,
    blocks: Vec::new(),
  };
  let nb = sim.nblocks();
  sim.blocks = vec![0; nb[0] * nb[1] * nb[2]];
  // the stock: the box, or the cylinder of a 4th axis
  let rotary = super::axes::rotary_stock(setup, model)?;
  for k in 0..n[2] {
    for j in 0..n[1] {
      for i in 0..n[0] {
        let c = sim.centre(i, j, k);
        let on = match rotary {
          Some((axis, r, _, _)) => DVec2::new(c.y - axis.y, c.z - axis.z).length() <= r,
          None => true,
        };
        if on {
          let at = sim.index(i, j, k);
          sim.cells[at] = STOCK;
          let b = sim.block(i, j, k);
          sim.blocks[b] += 1;
        }
      }
    }
  }
  mark_part(&mut sim, setup, model);
  let origin = super::origin(setup, model)?;
  let mut at = origin + DVec3::Z * (hi.z - origin.z + super::CLEARANCE);
  let mut axis = DVec3::Z;
  let mut angles = Angles::default();
  let mut range: Option<(f64, f64, f64, f64)> = None;
  for tp in paths {
    let mut notes = Notes::new(&tp.name);
    let mut removed = 0.0;
    let tool = &tp.tool;
    let length = if tool.flute_length > 0.0 { tool.flute_length } else { 2.0 * tool.diameter };
    for (i, m) in tp.moves.iter().enumerate() {
      notes.next();
      let to_axis = tp.axes.get(i).copied().unwrap_or(DVec3::Z).normalize_or_zero();
      if let Some(mach) = &setup.machine {
        match mach.solve(to_axis, angles) {
          Some(a) => {
            angles = a;
            range = Some(match range {
              Some(r) => (r.0.min(a.tilt), r.1.max(a.tilt), r.2.min(a.turn), r.3.max(a.turn)),
              None => (a.tilt, a.tilt, a.turn, a.turn),
            });
          }
          None => notes.note(WarningKind::AxisLimit, m.to, to_axis.angle_between(DVec3::Z).to_degrees()),
        }
      }
      let steps = (((m.to - at).length() / h).max(axis.angle_between(to_axis) / 2f64.to_radians()).ceil() as usize).max(1);
      for s in 0..=steps {
        let f = s as f64 / steps as f64;
        let c = Cutter {
          tool,
          length,
          tip: at.lerp(m.to, f),
          axis: axis.lerp(to_axis, f).normalize_or_zero(),
          rapid: m.rapid,
        };
        sweep(&mut sim, &c, &mut notes, &mut removed);
      }
      at = m.to;
      axis = to_axis;
    }
    sim.warnings.extend(notes.list);
    sim.removed.push((tp.name.clone(), removed));
  }
  sim.rotary = range;
  Ok(sim)
}

/// The tool at one position along a move.
struct Cutter<'a> {
  tool: &'a Tool,
  /// How far up its axis the tool cuts.
  length: f64,
  tip: DVec3,
  axis: DVec3,
  rapid: bool,
}

/// The tool at one position: clears the stock it holds (or, on a rapid, reports it).
fn sweep(sim: &mut VoxelSim, c: &Cutter, notes: &mut Notes, removed: &mut f64) {
  let r = c.tool.radius();
  let top = c.tip + c.axis * c.length;
  let lo = c.tip.min(top) - DVec3::splat(r);
  let hi = c.tip.max(top) + DVec3::splat(r);
  let range = |l: f64, h: f64, axis: usize| -> Option<(usize, usize)> {
    let a = ((l - sim.lo[axis]) / sim.h - 0.5).ceil().max(0.0);
    let b = ((h - sim.lo[axis]) / sim.h - 0.5).floor();
    (b >= a && b >= 0.0).then(|| (a as usize, (b as usize).min(sim.n[axis] - 1)))
  };
  let (Some((i0, i1)), Some((j0, j1)), Some((k0, k1))) = (range(lo.x, hi.x, 0), range(lo.y, hi.y, 1), range(lo.z, hi.z, 2))
  else {
    return;
  };
  if i0 > i1 || j0 > j1 || k0 > k1 {
    return;
  }
  let vol = sim.h.powi(3);
  let nb = sim.nblocks();
  for bk in k0 / BLOCK..=k1 / BLOCK {
    for bj in j0 / BLOCK..=j1 / BLOCK {
      for bi in i0 / BLOCK..=i1 / BLOCK {
        let b = (bk * nb[1] + bj) * nb[0] + bi;
        if sim.blocks[b] == 0 {
          continue;
        }
        for k in (bk * BLOCK).max(k0)..=((bk + 1) * BLOCK - 1).min(k1) {
          for j in (bj * BLOCK).max(j0)..=((bj + 1) * BLOCK - 1).min(j1) {
            for i in (bi * BLOCK).max(i0)..=((bi + 1) * BLOCK - 1).min(i1) {
              let at = sim.index(i, j, k);
              let cell = sim.cells[at];
              if cell & STOCK == 0 {
                continue;
              }
              let q = sim.centre(i, j, k);
              if c.rapid {
                if let Some(depth) = inside(c.tool, c.length, c.tip, c.axis, q, GOUGE_TOLERANCE) {
                  notes.note(WarningKind::RapidCollision, c.tip, depth);
                }
                continue;
              }
              if inside(c.tool, c.length, c.tip, c.axis, q, 0.0).is_none() {
                continue;
              }
              sim.cells[at] &= !STOCK;
              sim.blocks[b] -= 1;
              *removed += vol;
              if cell & PART != 0
                && let Some(depth) = inside(c.tool, c.length, c.tip, c.axis, q, GOUGE_TOLERANCE)
              {
                sim.cells[at] |= GOUGED;
                notes.note(WarningKind::Gouge, c.tip, depth);
              }
            }
          }
        }
      }
    }
  }
}

/// Marks the voxels inside the part: each column's crossings of the part's surface, inside
/// between odd and even ones.
fn mark_part(sim: &mut VoxelSim, setup: &Setup, model: &Model) {
  let (nx, ny) = (sim.n[0], sim.n[1]);
  let mut hits: Vec<Vec<f64>> = vec![Vec::new(); nx * ny];
  // off the cell centres a little, so rays miss the mesh's edges
  let jitter = DVec2::new(0.00131, 0.00173) * sim.h;
  for b in super::bodies(setup, model) {
    let Ok(m) = super::fine_mesh(b) else { continue };
    for tri in m.indices.chunks_exact(3) {
      let p = |i: u32| {
        let v = m.positions[i as usize];
        DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
      };
      let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
      let det = (b - a).truncate().perp_dot((c - a).truncate());
      if det.abs() < 1e-15 {
        continue;
      }
      let lo = a.min(b).min(c);
      let hi = a.max(b).max(c);
      let i0 = (((lo.x - sim.lo.x) / sim.h - 0.5).ceil().max(0.0)) as usize;
      let j0 = (((lo.y - sim.lo.y) / sim.h - 0.5).ceil().max(0.0)) as usize;
      let (fi, fj) = (((hi.x - sim.lo.x) / sim.h - 0.5).floor(), ((hi.y - sim.lo.y) / sim.h - 0.5).floor());
      if fi < 0.0 || fj < 0.0 {
        continue;
      }
      for j in j0..=(fj as usize).min(ny - 1) {
        for i in i0..=(fi as usize).min(nx - 1) {
          let q = sim.centre(i, j, 0).truncate() + jitter;
          let u = (q - a.truncate()).perp_dot((c - a).truncate()) / det;
          let v = (b - a).truncate().perp_dot(q - a.truncate()) / det;
          if u < 0.0 || v < 0.0 || u + v > 1.0 {
            continue;
          }
          hits[j * nx + i].push(a.z + u * (b.z - a.z) + v * (c.z - a.z));
        }
      }
    }
  }
  for j in 0..ny {
    for i in 0..nx {
      let zs = &mut hits[j * nx + i];
      zs.sort_by(f64::total_cmp);
      for pair in zs.chunks_exact(2) {
        for k in 0..sim.n[2] {
          let z = sim.lo.z + (k as f64 + 0.5) * sim.h;
          if z > pair[0] && z < pair[1] {
            let at = sim.index(i, j, k);
            sim.cells[at] |= PART;
          }
        }
      }
    }
  }
}
