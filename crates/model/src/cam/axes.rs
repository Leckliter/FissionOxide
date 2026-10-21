//! 4/5-axis milling: machine kinematics, 3+2 (indexed) operations, the rotary 4th axis and
//! simultaneous swarf.
//!
//! Toolpaths stay in work coordinates with a tool axis per move ([`Toolpath::axes`]); the
//! kinematics turn each position and axis into machine positions and rotary angles for the
//! posts ([`Machine::solve`], [`Machine::to_machine`]) and check them against the machine's
//! travel in the simulation. The rotary axes are taken to cross at the setup's work origin.
//!
//! A 3+2 operation is any milling operation with a tilted `tool_axis`: the part is turned so
//! the axis points up Z, the operation computed as usual in that frame and its moves turned
//! back, the tool axis held fixed.

use fox_kernel::TriMesh;
use fox_kernel::glam::{DMat3, DMat4, DQuat, DVec2, DVec3};
use serde::{Deserialize, Serialize};

use super::sim::Heightfield;
use super::{CLEARANCE, Move, OpKind, Operation, Setup, Toolpath, ToolKind};
use crate::regen::{Body, Model};
use crate::FaceRef;

/// How a machine's rotary axes move the tool relative to the part.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kinematics {
  /// Trunnion table: A tilts the table about X, C turns it about Z on the trunnion.
  TableTable,
  /// B tilts the spindle head about Y, C turns the table about Z.
  HeadTable,
  /// A 4th axis: A turns the part about X.
  Rotary,
}

impl Kinematics {
  pub const ALL: [Kinematics; 3] = [Kinematics::TableTable, Kinematics::HeadTable, Kinematics::Rotary];

  pub fn id(self) -> &'static str {
    match self {
      Kinematics::TableTable => "table_table",
      Kinematics::HeadTable => "head_table",
      Kinematics::Rotary => "rotary",
    }
  }

  pub fn parse(s: &str) -> Option<Kinematics> {
    match s.trim().to_ascii_lowercase().replace(['-', ' ', '/'], "_").as_str() {
      "table_table" | "trunnion" | "ac" | "5_axis" | "5axis" => Some(Kinematics::TableTable),
      "head_table" | "bc" => Some(Kinematics::HeadTable),
      "rotary" | "4th" | "4_axis" | "4axis" | "a" => Some(Kinematics::Rotary),
      _ => None,
    }
  }

  /// The G-code letters of the tilting and the turning axis (none on a 4th axis).
  pub fn letters(self) -> (char, Option<char>) {
    match self {
      Kinematics::TableTable => ('A', Some('C')),
      Kinematics::HeadTable => ('B', Some('C')),
      Kinematics::Rotary => ('A', None),
    }
  }
}

/// A 4/5-axis machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Machine {
  pub kinematics: Kinematics,
  /// Travel of the tilting axis (A on a trunnion, B on a head; degrees). A 4th axis turns freely.
  #[serde(default = "tilt_min")]
  pub tilt_min: f64,
  #[serde(default = "tilt_max")]
  pub tilt_max: f64,
  /// Head/table: from the head's pivot to the tool tip (mm); 0 = the control follows the tip
  /// itself (tool centre point control).
  #[serde(default)]
  pub pivot: f64,
}

fn tilt_min() -> f64 {
  -120.0
}

fn tilt_max() -> f64 {
  120.0
}

impl Machine {
  pub fn new(kinematics: Kinematics) -> Machine {
    Machine { kinematics, tilt_min: tilt_min(), tilt_max: tilt_max(), pivot: 0.0 }
  }
}

/// Rotary axis positions (degrees): the tilting axis (A or B) and the turning one (C).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Angles {
  pub tilt: f64,
  pub turn: f64,
}

/// `a` plus whole turns, nearest `near`.
fn unwrap(a: f64, near: f64) -> f64 {
  a + ((near - a) / 360.0).round() * 360.0
}

fn rx(deg: f64) -> DMat3 {
  DMat3::from_rotation_x(deg.to_radians())
}

fn rz(deg: f64) -> DMat3 {
  DMat3::from_rotation_z(deg.to_radians())
}

impl Machine {
  /// The rotary positions that point the tool along `t` (work coordinates, tip to shank),
  /// within the machine's travel, nearest `prev`; None when it can't.
  pub fn solve(&self, t: DVec3, prev: Angles) -> Option<Angles> {
    let t = t.try_normalize()?;
    if self.kinematics == Kinematics::Rotary {
      // the tool stays square to the A axis
      if t.x.abs() > 1e-6 {
        return None;
      }
      return Some(Angles { tilt: unwrap(t.y.atan2(t.z).to_degrees(), prev.tilt), turn: 0.0 });
    }
    let rho = t.truncate().length();
    let tilt = rho.atan2(t.z).to_degrees();
    let turn = if rho < 1e-9 {
      prev.turn
    } else if self.kinematics == Kinematics::TableTable {
      t.x.atan2(t.y).to_degrees()
    } else {
      -t.y.atan2(t.x).to_degrees()
    };
    let mut best: Option<(f64, Angles)> = None;
    for (a, c) in [(tilt, turn), (-tilt, turn + 180.0)] {
      if a < self.tilt_min - 1e-9 || a > self.tilt_max + 1e-9 {
        continue;
      }
      let c = unwrap(c, prev.turn);
      let cost = (a - prev.tilt).abs() + (c - prev.turn).abs();
      if best.is_none_or(|b| cost < b.0 - 1e-9) {
        best = Some((cost, Angles { tilt: a, turn: c }));
      }
    }
    best.map(|b| b.1)
  }

  /// The machine's linear positions for the tool tip at `p` (relative to the work origin).
  pub fn to_machine(&self, p: DVec3, a: Angles) -> DVec3 {
    match self.kinematics {
      Kinematics::Rotary => rx(a.tilt) * p,
      Kinematics::TableTable => rx(a.tilt) * (rz(a.turn) * p),
      Kinematics::HeadTable => {
        let b = a.tilt.to_radians();
        rz(a.turn) * p + self.pivot * DVec3::new(b.sin(), 0.0, b.cos())
      }
    }
  }

  /// The tool tip and axis relative to the work origin for machine positions: the inverse of
  /// [`Machine::to_machine`].
  pub fn from_machine(&self, m: DVec3, a: Angles) -> (DVec3, DVec3) {
    match self.kinematics {
      Kinematics::Rotary => (rx(-a.tilt) * m, rx(-a.tilt) * DVec3::Z),
      Kinematics::TableTable => {
        let back = rz(-a.turn) * rx(-a.tilt);
        (back * m, back * DVec3::Z)
      }
      Kinematics::HeadTable => {
        let b = a.tilt.to_radians();
        let axis = DVec3::new(b.sin(), 0.0, b.cos());
        (rz(-a.turn) * (m - self.pivot * axis), rz(-a.turn) * axis)
      }
    }
  }

  /// Whether the tool stays along the machine's Z (so XY arcs and drilling cycles hold).
  pub fn tool_along_z(&self, a: Angles) -> bool {
    self.kinematics != Kinematics::HeadTable || a.tilt.abs() < 1e-9
  }
}

/// A rotary setup's stock: the turning axis (a point on it), the stock radius about it and the
/// stock's ends along X; None for other setups.
pub(crate) fn rotary_stock(setup: &Setup, model: &Model) -> Result<Option<(DVec3, f64, f64, f64)>, String> {
  if setup.machine.map(|m| m.kinematics) != Some(Kinematics::Rotary) || setup.frame_stock.is_some() {
    return Ok(None);
  }
  let (lo, hi) = super::part_box(setup, model)?;
  let c = DVec3::new(0.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0);
  let mut r: f64 = 0.0;
  for b in super::bodies(setup, model) {
    for p in b.mesh().map(|m| m.positions.as_slice()).unwrap_or_default() {
      r = r.max(DVec2::new(p[1] as f64 - c.y, p[2] as f64 - c.z).length());
    }
  }
  if r <= 0.0 {
    r = DVec2::new(hi.y - lo.y, hi.z - lo.z).length() / 2.0;
  }
  Ok(Some((c, r + setup.stock_side, lo.x, hi.x)))
}

/// An operation's toolpath on a setup that may have a 4/5-axis machine.
pub(super) fn toolpath(setup: &Setup, op: &Operation, model: &Model, previous: &[Toolpath]) -> Result<Toolpath, String> {
  let kin = setup.machine.map(|m| m.kinematics);
  match &op.kind {
    OpKind::Rotary { around } => {
      if kin != Some(Kinematics::Rotary) {
        return Err("rotary machining needs a setup on a rotary 4th axis".into());
      }
      rotary(setup, op, model, *around)
    }
    OpKind::Swarf { faces } => {
      if !matches!(kin, Some(Kinematics::TableTable | Kinematics::HeadTable)) {
        return Err("swarf needs a setup on a 5-axis machine".into());
      }
      swarf(setup, op, model, faces)
    }
    _ => match op.tool_axis.map(DVec3::from).filter(|t| t.normalize_or_zero().dot(DVec3::Z) < 1.0 - 1e-12) {
      None => super::toolpath(setup, op, model, previous),
      Some(t) => {
        let m = setup.machine.ok_or("a tilted tool axis needs a 4/5-axis machine on the setup")?;
        let t = t.try_normalize().ok_or("the tool axis can't be zero")?;
        if m.solve(t, Angles::default()).is_none() {
          return Err(format!(
            "the machine can't point the tool along ({:.3}, {:.3}, {:.3})",
            t.x, t.y, t.z
          ));
        }
        tilted(setup, op, model, previous, t)
      }
    },
  }
}

/// A face reference turned with the part.
fn turned_ref(f: &FaceRef, q: DQuat, o: DVec3) -> FaceRef {
  let fwd = |p: [f64; 3]| (o + q * (DVec3::from(p) - o)).to_array();
  let mut out = f.clone();
  out.point = fwd(f.point);
  out.normal = (q * DVec3::from(f.normal)).to_array();
  if f.lo != f.hi {
    let (a, b) = turned_box((DVec3::from(f.lo), DVec3::from(f.hi)), q, o);
    out.lo = a.to_array();
    out.hi = b.to_array();
  }
  out
}

/// The box around a box turned about `o`.
fn turned_box((lo, hi): (DVec3, DVec3), q: DQuat, o: DVec3) -> (DVec3, DVec3) {
  let mut a = DVec3::splat(f64::INFINITY);
  let mut b = DVec3::splat(f64::NEG_INFINITY);
  for k in 0..8 {
    let c = DVec3::new(
      if k & 1 == 0 { lo.x } else { hi.x },
      if k & 2 == 0 { lo.y } else { hi.y },
      if k & 4 == 0 { lo.z } else { hi.z },
    );
    let p = o + q * (c - o);
    a = a.min(p);
    b = b.max(p);
  }
  (a, b)
}

/// 3+2: the operation computed with the part turned so `t` points up, its moves turned back.
fn tilted(setup: &Setup, op: &Operation, model: &Model, previous: &[Toolpath], t: DVec3) -> Result<Toolpath, String> {
  let q = DQuat::from_rotation_arc(t, DVec3::Z);
  let o = super::origin(setup, model)?;
  let m4 = DMat4::from_translation(o) * DMat4::from_quat(q) * DMat4::from_translation(-o);
  let mut frame = Model::default();
  for b in super::bodies(setup, model) {
    let shape = b.shape.transformed(m4).map_err(|e| e.to_string())?;
    frame.bodies.push(Body::new(b.id, b.name.clone(), shape, b.feature));
  }
  let mut fs = setup.clone();
  fs.frame_stock = Some(turned_box(super::stock(setup, model)?, q, o));
  fs.machine = None;
  let mut fop = op.clone();
  fop.tool_axis = None;
  for f in fop.kind.faces_mut() {
    *f = turned_ref(f, q, o);
  }
  let before: Vec<Toolpath> = previous
    .iter()
    .map(|tp| {
      let mut tp = tp.clone();
      for m in &mut tp.moves {
        m.to = o + q * (m.to - o);
      }
      tp
    })
    .collect();
  let mut tp = super::toolpath(&fs, &fop, &frame, &before)?;
  let back = q.inverse();
  for m in &mut tp.moves {
    m.to = o + back * (m.to - o);
  }
  if let Some((holes, _)) = &mut tp.drill {
    for (top, _) in holes {
      *top = o + back * (*top - o);
    }
  }
  tp.axes = vec![t; tp.moves.len()];
  Ok(tp)
}

/// The part's triangles turned by `-angle` about the X axis through `c`, those that can reach
/// the strip |y| <= `half` on the upper side.
fn strip_mesh(meshes: &[&TriMesh], c: DVec3, angle: f64, half: f64) -> TriMesh {
  let (s, k) = angle.sin_cos();
  let mut out = TriMesh { positions: Vec::new(), normals: Vec::new(), indices: Vec::new(), tri_face: Vec::new() };
  for m in meshes {
    let p = |i: u32| {
      let v = m.positions[i as usize];
      let (y, z) = (v[1] as f64 - c.y, v[2] as f64 - c.z);
      // q = Rx(-angle) (p - c)
      [v[0], (y * k + z * s) as f32, (-y * s + z * k) as f32]
    };
    for tri in m.indices.chunks_exact(3) {
      let t = [p(tri[0]), p(tri[1]), p(tri[2])];
      let (ylo, yhi) = t.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |a, v| (a.0.min(v[1]), a.1.max(v[1])));
      if ylo as f64 > half || (yhi as f64) < -half || t.iter().all(|v| v[2] < 0.0) {
        continue;
      }
      let base = out.positions.len() as u32;
      out.positions.extend_from_slice(&t);
      out.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
  }
  out
}

/// 4th axis: passes along X a step-over apart around the axis (or rings around it a step-over
/// apart along X), the tool square to the axis and dropped onto the part, in levels a step-down
/// apart from the stock down.
fn rotary(setup: &Setup, op: &Operation, model: &Model, around: bool) -> Result<Toolpath, String> {
  let tool = &op.tool;
  tool.check()?;
  if !tool.kind.mills() {
    return Err("rotary machining needs an end mill".into());
  }
  if !(op.stepover > 0.0 && op.stepover <= 1.0 && op.stepdown > 0.0) {
    return Err("step-down must be positive and step-over between 0 and 1".into());
  }
  let (c, rs, x0, x1) = rotary_stock(setup, model)?.ok_or("rotary machining needs a setup on a rotary 4th axis")?;
  let r = tool.radius();
  let rpart = (rs - setup.stock_side).max(r);
  let clear = rs + CLEARANCE;
  let h = (tool.diameter / 12.0).clamp(0.05, 1.0);
  let step = tool.diameter * op.stepover;
  let span = |a: f64, b: f64, d: f64| -> Vec<f64> {
    let n = ((b - a) / d).ceil().max(1.0) as usize;
    (0..=n).map(|k| a + (b - a) * k as f64 / n as f64).collect()
  };
  let (thetas, xs): (Vec<f64>, Vec<f64>) = if around {
    let n = ((std::f64::consts::TAU * rpart / h).ceil() as usize).clamp(90, 720);
    ((0..=n).map(|k| std::f64::consts::TAU * k as f64 / n as f64).collect(), span(x0, x1, step))
  } else {
    let n = ((std::f64::consts::TAU * rpart / step).ceil() as usize).max(8);
    if n > super::MAX_PASSES {
      return Err("too many passes: raise the step-over".into());
    }
    ((0..n).map(|k| std::f64::consts::TAU * k as f64 / n as f64).collect(), span(x0, x1, h))
  };
  let fine: Vec<TriMesh> = super::bodies(setup, model).into_iter().map(super::fine_mesh).collect::<Result<_, _>>()?;
  let meshes: Vec<&TriMesh> = fine.iter().collect();
  // the tool's tip radius dropped onto the part at each angle and X
  let mut drop = Vec::with_capacity(thetas.len());
  let cell = (h / 2.0).max(0.02);
  for &th in &thetas {
    let strip = strip_mesh(&meshes, c, th, r + cell);
    let mut hf = Heightfield::new(DVec2::new(x0 - r - cell, -r - cell), DVec2::new(x1 + r + cell, r + cell), cell, 0.0);
    hf.rasterize_over(&[&strip]);
    drop.push(xs.iter().map(|&x| hf.drop_tool_over(tool, DVec2::new(x, 0.0)).max(0.0) + op.stock_to_leave).collect::<Vec<_>>());
  }
  let lowest = drop.iter().flatten().copied().fold(f64::INFINITY, f64::min);
  let levels = super::levels(rs, lowest.max(0.0), op.stepdown);
  let world = |th: f64, x: f64, z: f64| DVec3::new(x, c.y - z * th.sin(), c.z + z * th.cos());
  let axis = |th: f64| DVec3::new(0.0, -th.sin(), th.cos());
  let mut moves: Vec<Move> = Vec::new();
  let mut axes: Vec<DVec3> = Vec::new();
  let mut push = |moves: &mut Vec<Move>, rapid: bool, th: f64, x: f64, z: f64, feed: f64| {
    moves.push(Move { rapid, to: world(th, x, z), feed });
    axes.push(axis(th));
  };
  let mut above = rs;
  for level in levels {
    // passes: (angle index, X index) along each
    let passes: Vec<Vec<(usize, usize)>> = if around {
      (0..xs.len()).map(|j| (0..thetas.len()).map(|i| (i, j)).collect()).collect()
    } else {
      (0..thetas.len())
        .map(|i| {
          let row: Vec<(usize, usize)> = (0..xs.len()).map(|j| (i, j)).collect();
          if i % 2 == 1 { row.into_iter().rev().collect() } else { row }
        })
        .collect()
    };
    for pass in passes {
      let pts: Vec<(f64, f64, f64)> = pass.iter().map(|&(i, j)| (thetas[i], xs[j], drop[i][j].max(level))).collect();
      // nothing left above this level here
      if pts.iter().all(|p| p.2 >= above - 1e-9) {
        continue;
      }
      let (th, x, z) = pts[0];
      push(&mut moves, true, th, x, clear, 0.0);
      push(&mut moves, true, th, x, (above + super::RETRACT).min(clear), 0.0);
      push(&mut moves, false, th, x, z, tool.plunge);
      for &(th, x, z) in &pts[1..] {
        push(&mut moves, false, th, x, z, tool.feed);
      }
      let (th, x, _) = *pts.last().unwrap_or(&pts[0]);
      push(&mut moves, true, th, x, clear, 0.0);
      if moves.len() > super::MAX_PASSES * 2000 {
        return Err("too many moves: raise the step-over or step-down".into());
      }
    }
    above = level;
  }
  if moves.is_empty() {
    return Err("there is nothing to cut".into());
  }
  Ok(Toolpath { name: op.name.clone(), tool: tool.clone(), moves, drill: None, axes, turn: Vec::new() })
}

/// A point on a wall's bottom rail: where it is and the wall's outward normal there.
#[derive(Clone, Copy)]
struct Rail {
  p: DVec3,
  n: DVec3,
}

/// Simultaneous 5-axis swarf: the flat end mill's side along walls, its axis on each wall's
/// steepest line (exact on drafted and conical walls), in passes a step-down apart from the top.
fn swarf(setup: &Setup, op: &Operation, model: &Model, faces: &[FaceRef]) -> Result<Toolpath, String> {
  let tool = &op.tool;
  tool.check()?;
  if tool.kind != ToolKind::Flat {
    return Err("swarf cuts with the side of a flat end mill".into());
  }
  if faces.is_empty() {
    return Err("pick the walls to swarf".into());
  }
  if !(op.stepdown > 0.0) {
    return Err("step-down must be positive".into());
  }
  // the walls' triangles with their outward facet normals
  let mut tris: Vec<([DVec3; 3], DVec3)> = Vec::new();
  for f in faces {
    let (b, id) = super::resolve(model, f)?;
    // finer than the display mesh, so the tool follows the wall itself
    let m = super::fine_mesh(b)?;
    for (t, tri) in m.indices.chunks_exact(3).enumerate() {
      if m.tri_face.get(t) != Some(&id) {
        continue;
      }
      let p = |i: u32| {
        let v = m.positions[i as usize];
        DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
      };
      let out: DVec3 = tri.iter().map(|&i| m.normals[i as usize]).map(|n| DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)).sum();
      let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
      let Some(mut n) = (b - a).cross(c - a).try_normalize() else { continue };
      if n.dot(out) < 0.0 {
        n = -n;
      }
      tris.push(([a, b, c], n));
    }
  }
  let zb = tris.iter().flat_map(|t| t.0).map(|v| v.z).fold(f64::INFINITY, f64::min);
  let zt = tris.iter().flat_map(|t| t.0).map(|v| v.z).fold(f64::NEG_INFINITY, f64::max);
  if !(zt - zb > 1e-3) {
    return Err("swarf walls can't be flat".into());
  }
  // the walls cut at a height: the rail the tool's tip follows there
  let rails = |zc: f64| -> Vec<(Vec<(Rail, Rail)>, bool)> {
    let mut segs: Vec<(Rail, Rail)> = Vec::new();
    for (t, n) in &tris {
      let mut cut = Vec::new();
      for k in 0..3 {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        if (a.z - zc) * (b.z - zc) < 0.0 {
          cut.push(Rail { p: a.lerp(b, (zc - a.z) / (b.z - a.z)), n: *n });
        }
      }
      if cut.len() == 2 {
        segs.push((cut[0], cut[1]));
      }
    }
    chain(segs, 1e-6 * (zt - zb).max(1.0))
  };
  // the walls' top edge: each tool axis runs along the wall to it (the ruling), so the tool's
  // side follows the wall itself rather than one facet extended up
  let top_z = zt - 1e-3 * (zt - zb);
  let top: Vec<(DVec3, DVec3)> = rails(top_z).into_iter().flat_map(|c| c.0).map(|s| (s.0.p, s.1.p)).collect();
  let ruling = |w: &Rail, up: DVec3| -> DVec3 {
    let guess = w.p + up * ((top_z - w.p.z) / up.z);
    let near = top
      .iter()
      .map(|(a, b)| {
        let d = *b - *a;
        let t = if d.length_squared() > 0.0 { ((guess - *a).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        *a + d * t
      })
      .min_by(|a, b| a.distance(guess).total_cmp(&b.distance(guess)));
    match near {
      Some(q) if q.distance(guess) < 0.1 * (zt - zb) && q.z - w.p.z > 0.05 * (zt - zb) => {
        (q - w.p).try_normalize().unwrap_or(up)
      }
      _ => up,
    }
  };
  let safe = super::stock(setup, model)?.1.z + CLEARANCE;
  let (r, leave) = (tool.radius(), op.stock_to_leave);
  let mut moves = Vec::new();
  let mut axes = Vec::new();
  // from the top down, the last pass just above the walls' bottom
  for z in super::levels(zt, zb + 1e-3 * (zt - zb), op.stepdown) {
    let chains = rails(z);
    if chains.is_empty() {
      return Err("the walls have no edge to follow".into());
    }
    for (segs, closed) in chains {
      // each segment with its facet's normal: the axis turns in place where facets meet
      let mut pts: Vec<Rail> = segs.iter().flat_map(|s| [s.0, s.1]).collect();
      if closed && let Some(first) = pts.first().copied() {
        pts.push(first);
      }
      // climb milling: the wall on the tool's right
      let turn: f64 = segs.iter().map(|s| s.0.n.dot((s.1.p - s.0.p).cross(DVec3::Z))).sum();
      if turn > 0.0 {
        pts.reverse();
      }
      let mut first = true;
      for w in densify(&pts, 2f64.to_radians()) {
        let up = ruling(&w, steepest(w.n).ok_or("swarf walls can't be flat")?);
        let out = w.n.reject_from(up).try_normalize().unwrap_or(w.n);
        let tip = w.p + out * (r + leave);
        if first {
          let lift = up * ((safe - tip.z).max(0.0) / up.z);
          moves.push(Move { rapid: true, to: tip + lift, feed: 0.0 });
          axes.push(up);
          moves.push(Move { rapid: false, to: tip, feed: tool.plunge });
          axes.push(up);
          first = false;
        } else {
          moves.push(Move { rapid: false, to: tip, feed: tool.feed });
          axes.push(up);
        }
      }
      if let (Some(m), Some(up)) = (moves.last().copied(), axes.last().copied()) {
        moves.push(Move { rapid: true, to: m.to + up * ((safe - m.to.z).max(0.0) / up.z), feed: 0.0 });
        axes.push(up);
      }
    }
  }
  Ok(Toolpath { name: op.name.clone(), tool: tool.clone(), moves, drill: None, axes, turn: Vec::new() })
}

/// The steepest direction up a wall with outward normal `n`.
fn steepest(n: DVec3) -> Option<DVec3> {
  let up = DVec3::Z - n * n.z;
  (up.length() > 1e-6).then(|| up.normalize())
}

/// Points between rail points so the tool axis turns at most `step` (radians) a move.
fn densify(pts: &[Rail], step: f64) -> Vec<Rail> {
  let mut out: Vec<Rail> = Vec::new();
  for w in pts.windows(2) {
    let (a, b) = (w[0], w[1]);
    let turn = a.n.angle_between(b.n);
    if turn < 1e-12 && a.p.distance(b.p) < 1e-12 {
      continue;
    }
    let n = ((turn / step).ceil() as usize).max(1);
    if out.is_empty() {
      out.push(a);
    }
    for k in 1..=n {
      let s = k as f64 / n as f64;
      out.push(Rail { p: a.p.lerp(b.p, s), n: a.n.lerp(b.n, s).normalize_or_zero() });
    }
  }
  if out.is_empty() {
    out.extend_from_slice(pts);
  }
  out
}

/// Joins segments end to end into chains, each segment running on from the last: (segments,
/// closed).
fn chain(mut segs: Vec<(Rail, Rail)>, tol: f64) -> Vec<(Vec<(Rail, Rail)>, bool)> {
  let mut out = Vec::new();
  while let Some(s) = segs.pop() {
    let mut run = std::collections::VecDeque::from([s]);
    loop {
      let end = run[run.len() - 1].1.p;
      if let Some(i) = segs.iter().position(|s| s.0.p.distance(end) < tol || s.1.p.distance(end) < tol) {
        let s = segs.swap_remove(i);
        run.push_back(if s.0.p.distance(end) < tol { s } else { (s.1, s.0) });
        continue;
      }
      let start = run[0].0.p;
      if let Some(i) = segs.iter().position(|s| s.0.p.distance(start) < tol || s.1.p.distance(start) < tol) {
        let s = segs.swap_remove(i);
        run.push_front(if s.1.p.distance(start) < tol { s } else { (s.1, s.0) });
        continue;
      }
      break;
    }
    let closed = run.len() > 2 && run[0].0.p.distance(run[run.len() - 1].1.p) < tol;
    out.push((run.into_iter().collect(), closed));
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn kinematics_round_trip() {
    for kin in Kinematics::ALL {
      let m = Machine { pivot: if kin == Kinematics::HeadTable { 150.0 } else { 0.0 }, ..Machine::new(kin) };
      let axes: Vec<DVec3> = if kin == Kinematics::Rotary {
        (0..12).map(|k| (k as f64 * 0.6).sin_cos()).map(|(s, c)| DVec3::new(0.0, -s, c)).collect()
      } else {
        vec![DVec3::Z, DVec3::new(0.3, 0.2, 0.9), DVec3::new(-0.5, 0.1, 0.4), DVec3::X, DVec3::new(0.0, -1.0, 0.2)]
      };
      let mut prev = Angles::default();
      for t in axes {
        let t = t.normalize();
        let a = m.solve(t, prev).unwrap_or_else(|| panic!("{kin:?} {t}"));
        let p = DVec3::new(12.0, -7.0, 3.5);
        let (back, axis) = m.from_machine(m.to_machine(p, a), a);
        assert!(back.distance(p) < 1e-9 && axis.distance(t) < 1e-9, "{kin:?} {t} {a:?}");
        // a 4th axis turns freely
        assert!(kin == Kinematics::Rotary || (a.tilt >= m.tilt_min - 1e-9 && a.tilt <= m.tilt_max + 1e-9));
        prev = a;
      }
    }
    // beyond the tilt range, and off a 4th axis's plane
    let tight = Machine { tilt_min: -30.0, tilt_max: 30.0, ..Machine::new(Kinematics::TableTable) };
    assert!(tight.solve(DVec3::X, Angles::default()).is_none());
    assert!(Machine::new(Kinematics::Rotary).solve(DVec3::new(0.2, 0.0, 1.0), Angles::default()).is_none());
    // the turning axis winds on instead of jumping back a turn
    let m = Machine::new(Kinematics::TableTable);
    let a = m.solve(DVec3::new(-0.01, -0.5, 0.8), Angles { tilt: 30.0, turn: 179.0 }).unwrap();
    assert!((a.turn - 180.0).abs() < 2.0, "{a:?}");
  }
}
