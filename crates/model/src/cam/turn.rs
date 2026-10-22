//! Turning: lathe setups (the spindle axis, the bar in the chuck, the work origin), their
//! operations — facing, profile roughing, finishing, grooving, part-off and threading — the
//! turned stock's simulation and lathe posts (Fanuc, Haas, LinuxCNC).
//!
//! A lathe part turns about its axis, so only its profile matters: the largest radius of the
//! part along the axis, from a fine tessellation. Outside turning follows the profile as seen
//! from the front (undercuts behind a larger diameter are left to grooving). Lathe coordinates
//! are Z along the axis (positive towards the tailstock, 0 at the work origin) and X the
//! radius (written as a diameter in G-code). Turning inserts are programmed at their
//! imaginary tip (the corner of the square around the nose, tip direction 3); grooving and
//! part-off inserts at their corner towards the chuck; threading inserts at their tip.

use std::fmt::Write;

use fox_kernel::glam::DVec3;
use serde::{Deserialize, Serialize};

use super::sim::{GOUGE_TOLERANCE, SimMesh, Warning, WarningKind};
use super::voxel::Notes;
use super::{Move, OpKind, Operation, Setup, ToolKind, Toolpath};
use crate::regen::Model;

/// Clearance above the stock for approaches and retracts (mm, radial).
const CLEAR: f64 = 2.0;

/// Where a lathe setup's Z0 is.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LatheOrigin {
  /// The part's front face (towards the tailstock).
  #[default]
  Front,
  /// The front of the bar stock.
  StockFront,
  /// The part's back face (towards the chuck).
  Back,
}

impl LatheOrigin {
  pub fn parse(s: &str) -> Option<LatheOrigin> {
    match s.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
      "front" | "part_front" => Some(LatheOrigin::Front),
      "stock_front" | "stock" => Some(LatheOrigin::StockFront),
      "back" | "part_back" => Some(LatheOrigin::Back),
      _ => None,
    }
  }

  pub fn id(self) -> &'static str {
    match self {
      LatheOrigin::Front => "front",
      LatheOrigin::StockFront => "stock_front",
      LatheOrigin::Back => "back",
    }
  }
}

/// A lathe setup: the part turns about `axis` (through `axis_point`, pointing from the chuck to
/// the tailstock), cut from a bar held in a chuck.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lathe {
  pub axis_point: [f64; 3],
  pub axis: [f64; 3],
  /// Bar diameter (mm).
  pub bar_diameter: f64,
  /// Bar in front of the part, to face off (mm).
  #[serde(default = "one")]
  pub stock_front: f64,
  /// Room between the part's back and the chuck jaws, to part off in (mm).
  #[serde(default = "five")]
  pub chuck_gap: f64,
  /// Length of bar held in the jaws (mm).
  #[serde(default = "grip")]
  pub grip: f64,
  /// Outside diameter of the chuck jaws (mm).
  #[serde(default)]
  pub chuck_diameter: f64,
  #[serde(default)]
  pub origin: LatheOrigin,
}

fn one() -> f64 {
  1.0
}

fn five() -> f64 {
  5.0
}

fn grip() -> f64 {
  15.0
}

/// A lathe move in work coordinates: `z` along the axis, `x` the radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatheMove {
  pub rapid: bool,
  pub z: f64,
  pub x: f64,
  /// Feed per revolution (mm/rev; 0 on rapids).
  pub feed: f64,
  /// Threading: the lead per revolution (a spindle-synchronised move); 0 otherwise.
  pub pitch: f64,
}

/// The part's outline along the axis: its largest radius in each cell.
#[derive(Debug, Clone)]
pub struct Profile {
  /// Axis coordinate of the back of cell 0, and the cell length.
  pub a0: f64,
  pub h: f64,
  /// Largest radius of the part over each cell.
  pub r: Vec<f64>,
}

impl Profile {
  pub fn back(&self) -> f64 {
    self.a0
  }

  pub fn front(&self) -> f64 {
    self.a0 + self.h * self.r.len() as f64
  }

  pub fn centre(&self, i: usize) -> f64 {
    self.a0 + (i as f64 + 0.5) * self.h
  }

  /// The cell holding an axis position, if on the part.
  pub fn cell(&self, a: f64) -> Option<usize> {
    let f = ((a - self.a0) / self.h).floor();
    (f >= 0.0 && (f as usize) < self.r.len()).then_some(f as usize)
  }

  /// The profile as seen from the front: at each cell, the largest radius from there forward.
  pub fn from_front(&self) -> Vec<f64> {
    let mut out = self.r.clone();
    for i in (0..out.len().saturating_sub(1)).rev() {
      out[i] = out[i].max(out[i + 1]);
    }
    out
  }
}

/// The lathe frame in the world: a point on the axis, the axis and a radial direction to show
/// toolpaths along.
struct Frame {
  p: DVec3,
  d: DVec3,
  e: DVec3,
}

impl Frame {
  fn new(l: &Lathe) -> Result<Frame, String> {
    let d = DVec3::from(l.axis).try_normalize().ok_or("the lathe axis can't be zero")?;
    // toolpaths show on the upper side (or the front, for a vertical axis)
    let e = (DVec3::Z - d * d.z).try_normalize().unwrap_or_else(|| (DVec3::NEG_Y - d * -d.y).normalize());
    Ok(Frame { p: DVec3::from(l.axis_point), d, e })
  }

  fn world(&self, a: f64, x: f64) -> DVec3 {
    self.p + self.d * a + self.e * x
  }

  /// (axis coordinate, radius) of a world point.
  fn local(&self, q: DVec3) -> (f64, f64) {
    let v = q - self.p;
    let a = v.dot(self.d);
    (a, (v - self.d * a).length())
  }
}

/// The spindle axis for a part: along `hint` (or the world axis the part is round about,
/// longest first) through the middle of the part's box. Returns (point, direction).
pub fn detect_axis(setup: &Setup, model: &Model, hint: Option<DVec3>) -> Result<(DVec3, DVec3), String> {
  let (lo, hi) = super::part_box(setup, model)?;
  let mid = (lo + hi) / 2.0;
  if let Some(h) = hint {
    return Ok((mid, h.try_normalize().ok_or("the lathe axis can't be zero")?));
  }
  let ext = hi - lo;
  let mut best: Option<(f64, DVec3)> = None;
  for (k, dir) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().enumerate() {
    let (u, v) = (ext[(k + 1) % 3], ext[(k + 2) % 3]);
    let round = (u - v).abs() <= 0.01 * u.max(v).max(1e-9);
    // round ones first, then the longest
    let score = if round { 1e9 } else { 0.0 } + ext[k];
    if best.is_none_or(|b| score > b.0) {
      best = Some((score, dir));
    }
  }
  Ok((mid, best.map(|b| b.1).unwrap_or(DVec3::Z)))
}

/// The part's profile about a lathe's axis.
pub fn profile(l: &Lathe, setup: &Setup, model: &Model) -> Result<Profile, String> {
  let f = Frame::new(l)?;
  let mut meshes = Vec::new();
  for b in super::bodies(setup, model) {
    meshes.push(super::fine_mesh(b)?);
  }
  let (mut back, mut front) = (f64::INFINITY, f64::NEG_INFINITY);
  for m in &meshes {
    for v in &m.positions {
      let (a, _) = f.local(DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
      back = back.min(a);
      front = front.max(a);
    }
  }
  if !(front > back) {
    return Err("the setup has no part to turn".into());
  }
  let n = (((front - back) / 0.02).ceil() as usize).clamp(200, 4000);
  let h = (front - back) / n as f64;
  let mut r = vec![f64::NEG_INFINITY; n];
  for m in &meshes {
    for tri in m.indices.chunks_exact(3) {
      let pts: Vec<(f64, DVec3)> = tri
        .iter()
        .map(|&i| {
          let v = m.positions[i as usize];
          let q = DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64);
          (f.local(q).0, q)
        })
        .collect();
      let alo = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
      let ahi = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
      let i0 = (((alo - back) / h).floor().max(0.0) as usize).min(n - 1);
      let i1 = (((ahi - back) / h).floor().max(0.0) as usize).min(n - 1);
      for (i, ri) in r.iter_mut().enumerate().take(i1 + 1).skip(i0) {
        let (s0, s1) = (back + i as f64 * h, back + (i + 1) as f64 * h);
        // the largest radius of the triangle within the slab is at a corner of its clipped piece
        for p in clip(&pts, s0, s1) {
          *ri = ri.max(f.local(p).1);
        }
      }
    }
  }
  // cells a triangle edge only grazed get their neighbours' radius
  for i in 0..n {
    if !r[i].is_finite() {
      r[i] = r[i.saturating_sub(1)].max(r[(i + 1).min(n - 1)]).max(0.0);
    }
  }
  Ok(Profile { a0: back, h, r })
}

/// The corners of a triangle's piece between two axis positions.
fn clip(pts: &[(f64, DVec3)], s0: f64, s1: f64) -> Vec<DVec3> {
  let mut poly: Vec<(f64, DVec3)> = pts.to_vec();
  for (bound, keep_above) in [(s0, true), (s1, false)] {
    let inside = |a: f64| if keep_above { a >= bound } else { a <= bound };
    let mut out = Vec::new();
    for k in 0..poly.len() {
      let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
      if inside(a.0) {
        out.push(a);
      }
      if inside(a.0) != inside(b.0) {
        let t = (bound - a.0) / (b.0 - a.0);
        out.push((bound, a.1.lerp(b.1, t)));
      }
    }
    poly = out;
    if poly.is_empty() {
      break;
    }
  }
  poly.into_iter().map(|p| p.1).collect()
}

/// Everything a lathe setup's operations need.
struct Ctx {
  frame: Frame,
  prof: Profile,
  /// The profile seen from the front.
  front_view: Vec<f64>,
  /// Axis coordinates: the work origin, the bar's front, the chuck jaws' face, the bar's end.
  z0: f64,
  stock_front: f64,
  chuck: f64,
  bar_end: f64,
  /// Bar and chuck radius.
  bar: f64,
  chuck_r: f64,
}

impl Ctx {
  fn new(l: &Lathe, setup: &Setup, model: &Model) -> Result<Ctx, String> {
    let frame = Frame::new(l)?;
    let prof = profile(l, setup, model)?;
    let big = prof.r.iter().copied().fold(0.0, f64::max);
    if !(l.bar_diameter / 2.0 >= big - 1e-6) {
      return Err(format!("the bar (Ø{}) is smaller than the part (Ø{:.3})", l.bar_diameter, 2.0 * big));
    }
    if !(l.stock_front >= 0.0 && l.chuck_gap >= 0.0 && l.grip >= 0.0) {
      return Err("the stock lengths can't be negative".into());
    }
    let stock_front = prof.front() + l.stock_front;
    let chuck = prof.back() - l.chuck_gap;
    let z0 = match l.origin {
      LatheOrigin::Front => prof.front(),
      LatheOrigin::StockFront => stock_front,
      LatheOrigin::Back => prof.back(),
    };
    let bar = l.bar_diameter / 2.0;
    let chuck_r = if l.chuck_diameter > 0.0 { l.chuck_diameter / 2.0 } else { bar + 20.0 };
    Ok(Ctx { front_view: prof.from_front(), frame, prof, z0, stock_front, chuck, bar_end: chuck - l.grip, bar, chuck_r })
  }

  /// The tool nose centre's lowest radius at each cell centre: the front view offset by `rr`,
  /// on past the front face as far as the offset reaches (−∞ where nothing is in the way).
  fn offset(&self, rr: f64) -> Vec<f64> {
    let (h, n) = (self.prof.h, self.front_view.len());
    let reach = (rr / h).ceil() as usize + 1;
    (0..n + reach)
      .map(|i| {
        let mut best = f64::NEG_INFINITY;
        for j in i.saturating_sub(reach)..(i + reach + 1).min(n) {
          // the closest the two cells come along the axis
          let dz = ((i as f64 - j as f64).abs() - 1.0).max(0.0) * h;
          if dz <= rr {
            best = best.max(self.front_view[j] + (rr * rr - dz * dz).sqrt());
          }
        }
        best
      })
      .collect()
  }
}

/// A lathe toolpath being built (axis coordinates in, work coordinates out).
struct Lp<'a> {
  ctx: &'a Ctx,
  moves: Vec<LatheMove>,
  feed: f64,
}

impl Lp<'_> {
  fn rapid(&mut self, a: f64, x: f64) {
    self.moves.push(LatheMove { rapid: true, z: a - self.ctx.z0, x, feed: 0.0, pitch: 0.0 });
  }

  fn cut(&mut self, a: f64, x: f64) {
    self.moves.push(LatheMove { rapid: false, z: a - self.ctx.z0, x, feed: self.feed, pitch: 0.0 });
  }

  fn thread(&mut self, a: f64, x: f64, pitch: f64) {
    self.moves.push(LatheMove { rapid: false, z: a - self.ctx.z0, x, feed: pitch, pitch });
  }

  /// Back out of the cut at 45° and up clear of the bar.
  fn leave(&mut self) {
    if let Some(m) = self.moves.last().copied() {
      self.cut(m.z + self.ctx.z0 + 0.5, m.x + 0.5);
      self.rapid(m.z + self.ctx.z0 + 0.5, self.ctx.bar + CLEAR);
    }
  }
}

/// Drops points of a polyline that lie within `tol` of the line through their neighbours.
fn simplify(pts: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
  let mut out: Vec<(f64, f64)> = Vec::new();
  for &p in pts {
    while out.len() >= 2 {
      let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
      let (dx, dy) = (p.0 - a.0, p.1 - a.1);
      let len = (dx * dx + dy * dy).sqrt();
      let off = if len < 1e-12 { 0.0 } else { ((b.0 - a.0) * dy - (b.1 - a.1) * dx).abs() / len };
      // the middle point must also lie between the others
      let along = (b.0 - a.0) * dx + (b.1 - a.1) * dy;
      if off <= tol && along >= 0.0 && along <= len * len {
        out.pop();
      } else {
        break;
      }
    }
    out.push(p);
  }
  out
}

/// Toolpaths of a lathe setup's operations.
pub fn toolpaths(l: &Lathe, setup: &Setup, model: &Model) -> Result<Vec<Toolpath>, String> {
  let ctx = Ctx::new(l, setup, model)?;
  let mut out = Vec::new();
  for op in &setup.operations {
    let moves = turn(&ctx, op, model).map_err(|e| format!("{}: {e}", op.name))?;
    let world = moves
      .iter()
      .map(|m| Move {
        rapid: m.rapid,
        to: ctx.frame.world(m.z + ctx.z0, m.x),
        feed: if m.pitch > 0.0 { m.pitch * op.tool.rpm } else { m.feed * op.tool.rpm_at(m.x) },
      })
      .collect();
    out.push(Toolpath { name: op.name.clone(), tool: op.tool.clone(), moves: world, drill: None, axes: Vec::new(), turn: moves });
  }
  Ok(out)
}

/// One lathe operation's moves.
fn turn(ctx: &Ctx, op: &Operation, model: &Model) -> Result<Vec<LatheMove>, String> {
  let tool = &op.tool;
  tool.check()?;
  if !op.kind.turns() {
    return Err("a lathe setup only takes lathe operations".into());
  }
  let want: &[ToolKind] = match op.kind {
    OpKind::TurnFace | OpKind::TurnRough | OpKind::TurnFinish => &[ToolKind::Turning],
    OpKind::Groove => &[ToolKind::Groove],
    OpKind::PartOff => &[ToolKind::PartOff, ToolKind::Groove],
    _ => &[ToolKind::Thread],
  };
  if !want.contains(&tool.kind) {
    return Err(format!(
      "this operation needs a {} tool",
      want.iter().map(|k| k.id().replace('_', "-")).collect::<Vec<_>>().join(" or ")
    ));
  }
  if !(op.stepdown > 0.0) || !(op.stepover > 0.0 && op.stepover <= 1.0) || op.stock_to_leave < 0.0 {
    return Err("step-down must be positive, step-over between 0 and 1 and the stock to leave not negative".into());
  }
  let mut p = Lp { ctx, moves: Vec::new(), feed: tool.per_rev() };
  let (front, back, bar) = (ctx.prof.front(), ctx.prof.back(), ctx.bar);
  let rn = tool.corner_radius;
  let leave = op.stock_to_leave;
  // clear of the bar's front
  p.rapid(ctx.stock_front + CLEAR, bar + CLEAR);
  match &op.kind {
    OpKind::TurnFace => {
      let mut z = ctx.stock_front;
      let bottom = front + leave;
      if z <= bottom + 1e-9 {
        return Err("there is no stock in front of the part to face".into());
      }
      while z > bottom + 1e-9 {
        z = (z - op.stepdown).max(bottom);
        p.rapid(z, bar + CLEAR);
        p.cut(z, -rn);
        p.cut(z + 0.5, -rn + 0.5);
        p.rapid(z + 0.5, bar + CLEAR);
      }
    }
    OpKind::TurnRough | OpKind::TurnFinish => {
      let rr = rn + leave;
      let c = ctx.offset(rr);
      let n = c.len();
      // the nose centre's path: the offset front view at each cell centre, as far back as the
      // tip may go
      let last = (0..n).find(|&i| ctx.prof.centre(i) - rn >= back - 1e-9).unwrap_or(0);
      if matches!(op.kind, OpKind::TurnRough) {
        let start = ctx.stock_front + CLEAR;
        let lowest = c[last..].iter().copied().fold(f64::INFINITY, f64::min) - rn;
        let mut above = bar;
        for level in super::levels(bar, lowest.max(0.0), op.stepdown) {
          // from the front back to where the profile rises above this level
          let mut stop = n;
          for i in (last..n).rev() {
            if c[i] - rn > level + 1e-9 {
              break;
            }
            stop = i;
          }
          let z_stop = if stop == n { ctx.prof.centre(n) - rn } else { ctx.prof.centre(stop) - rn };
          p.rapid(start, above + 0.5);
          p.rapid(start, level);
          p.cut(z_stop, level);
          // up the shoulder to the level above, so no step is left
          let mut climb = vec![(z_stop, level)];
          let mut i = stop;
          while i > last {
            i -= 1;
            let x = (c[i] - rn).min(above);
            climb.push((ctx.prof.centre(i) - rn, x));
            if x >= above - 1e-9 {
              break;
            }
          }
          for &(a, x) in &simplify(&climb, 0.001)[1..] {
            p.cut(a, x);
          }
          p.leave();
          above = level;
        }
      } else {
        let mut pts = vec![(front + rr - rn, -rn)];
        for i in (last..n).rev().filter(|&i| c[i].is_finite() && ctx.prof.centre(i) < front + rr) {
          pts.push((ctx.prof.centre(i) - rn, c[i].max(0.0) - rn));
        }
        let pts = simplify(&pts, 0.001);
        p.rapid(pts[0].0 + CLEAR, pts[0].1);
        p.cut(pts[0].0, pts[0].1);
        for &(a, x) in &pts[1..] {
          p.cut(a, x);
        }
        let end = pts[pts.len() - 1];
        p.cut(end.0, end.1 + 0.5);
        p.rapid(end.0, bar + CLEAR);
      }
    }
    OpKind::Groove => {
      let w = tool.cut_width();
      let fv = &ctx.front_view;
      let r = &ctx.prof.r;
      let mut grooves = Vec::new();
      let mut i = 0;
      while i < r.len() {
        if r[i] < fv[i] - 0.01 {
          let j0 = i;
          while i < r.len() && r[i] < fv[i] - 0.01 {
            i += 1;
          }
          grooves.push((ctx.prof.a0 + j0 as f64 * ctx.prof.h, ctx.prof.a0 + i as f64 * ctx.prof.h));
        }
        i += 1;
      }
      if grooves.is_empty() {
        return Err("the part has no grooves (undercuts the profile passes can't reach)".into());
      }
      // highest of the part under the insert placed at `a` (its chuck-side corner)
      let under = |a: f64, of: &[f64]| -> f64 {
        let (i0, i1) = (ctx.prof.cell(a + 1e-9), ctx.prof.cell(a + w - 1e-9));
        let (i0, i1) = (i0.unwrap_or(0), i1.unwrap_or(of.len() - 1));
        of[i0..=i1.max(i0)].iter().copied().fold(0.0, f64::max)
      };
      for (g0, g1) in grooves {
        if g1 - g0 < w - 1e-6 {
          return Err(format!("a groove at Z{:.3} is narrower than the {w} mm insert", g1 - ctx.z0));
        }
        let steps = ((g1 - g0 - w) / (w * op.stepover)).ceil().max(0.0) as usize;
        for k in 0..=steps {
          let a = if steps == 0 { g0 } else { g0 + (g1 - g0 - w) * k as f64 / steps as f64 };
          // in from above the bar: earlier passes may not have cleared the way
          let bottom = under(a, r) + leave;
          p.rapid(a, bar + CLEAR);
          plunge(&mut p, a, bar + CLEAR, bottom, op.stepdown);
          p.rapid(a, bar + CLEAR);
        }
      }
    }
    OpKind::PartOff => {
      let w = tool.cut_width();
      if w > ctx.prof.back() - ctx.chuck + 1e-9 {
        return Err(format!("the {w} mm blade doesn't fit in the {:.3} mm gap before the chuck", back - ctx.chuck));
      }
      let a = back - w;
      p.rapid(a, bar + CLEAR);
      plunge(&mut p, a, bar + CLEAR, -0.5, op.stepdown);
      p.rapid(a, bar + CLEAR);
    }
    OpKind::Thread { pitch, faces, z_start, z_end, depth } => {
      if !(*pitch > 0.0) {
        return Err("the thread pitch must be positive".into());
      }
      let (a0, a1) = if let Some(f) = faces.first() {
        let (b, id) = super::resolve(model, f)?;
        let m = b.mesh().ok_or("the thread face has no mesh")?;
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for (t, tri) in m.indices.chunks_exact(3).enumerate() {
          if m.tri_face.get(t) == Some(&id) {
            for &i in tri {
              let v = m.positions[i as usize];
              let (a, _) = ctx.frame.local(DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
              lo = lo.min(a);
              hi = hi.max(a);
            }
          }
        }
        if !(hi > lo) {
          return Err("the thread face isn't along the axis".into());
        }
        (lo, hi)
      } else {
        let (a, b) = (z_start + ctx.z0, z_end + ctx.z0);
        if (a - b).abs() < 1e-6 {
          return Err("pick the thread's face or give its span (z_start, z_end)".into());
        }
        (a.min(b), a.max(b))
      };
      // the thread's diameter, away from the cells its ends share with a shoulder
      let h = ctx.prof.h;
      let cells: Vec<f64> = (0..ctx.prof.r.len())
        .filter(|&i| ctx.prof.centre(i) > a0 + h && ctx.prof.centre(i) < a1 - h)
        .map(|i| ctx.prof.r[i])
        .collect();
      let major = cells.iter().copied().fold(0.0, f64::max);
      if major <= 0.0 {
        return Err("there is nothing to thread there".into());
      }
      let full = if *depth > 0.0 { *depth } else { 0.61343 * pitch };
      if full >= major {
        return Err("the thread is deeper than the part".into());
      }
      // equal chip areas: each pass a little shallower, then a spring pass at full depth
      // a thread up to a shoulder stops two pitches short of it
      let shoulder = ctx.prof.cell(a0 - ctx.prof.h).is_some_and(|i| ctx.front_view[i] > major + 1e-3);
      let a0 = if shoulder { a0 + 2.0 * pitch } else { a0 };
      if a0 >= a1 {
        return Err("the thread is too short".into());
      }
      let first = op.stepdown.min(full);
      let n = ((full / first).powi(2)).ceil().clamp(1.0, 200.0) as usize;
      let mut depths: Vec<f64> = (1..=n).map(|k| (first * (k as f64).sqrt()).min(full)).collect();
      depths.push(full);
      let (start, end) = (a1 + 2.0 * pitch, a0);
      let ahead = (0..ctx.front_view.len()).filter(|&i| ctx.prof.centre(i) > a1 + h && ctx.prof.centre(i) < start);
      if ahead.into_iter().any(|i| ctx.front_view[i] > major + 1e-3) {
        return Err("the thread starts behind a larger diameter".into());
      }
      for d in depths {
        p.rapid(start, major + CLEAR / 2.0);
        p.rapid(start, major - d);
        p.thread(end, major - d, *pitch);
        p.rapid(end, major + CLEAR / 2.0);
      }
      p.rapid(start, major + CLEAR / 2.0);
    }
    _ => return Err("a lathe setup only takes lathe operations".into()),
  }
  p.rapid(ctx.stock_front + CLEAR, bar + CLEAR);
  Ok(p.moves)
}

/// Plunges straight in from `top` to `bottom` at `a`, pecking back 0.5 mm every `peck`.
fn plunge(p: &mut Lp, a: f64, top: f64, bottom: f64, peck: f64) {
  let mut x = top;
  while x > bottom + 1e-9 {
    x = (x - peck).max(bottom);
    p.cut(a, x);
    if x > bottom + 1e-9 {
      p.rapid(a, x + 0.5);
      p.rapid(a, x + 0.1);
    }
  }
}

/// The work origin in the world.
pub fn world_origin(l: &Lathe, setup: &Setup, model: &Model) -> Result<DVec3, String> {
  let ctx = Ctx::new(l, setup, model)?;
  Ok(ctx.frame.world(ctx.z0, 0.0))
}

/// The box around the bar.
pub fn stock_box(l: &Lathe, setup: &Setup, model: &Model) -> Result<(DVec3, DVec3), String> {
  let ctx = Ctx::new(l, setup, model)?;
  let d = ctx.frame.d;
  let reach = DVec3::new((1.0 - d.x * d.x).max(0.0).sqrt(), (1.0 - d.y * d.y).max(0.0).sqrt(), (1.0 - d.z * d.z).max(0.0).sqrt()) * ctx.bar;
  let (a, b) = (ctx.frame.world(ctx.bar_end, 0.0), ctx.frame.world(ctx.stock_front, 0.0));
  Ok((a.min(b) - reach, a.max(b) + reach))
}

/// The bar after a lathe setup's toolpaths: its radius along the axis.
#[derive(Debug, Clone)]
pub struct TurnSim {
  /// Axis coordinate of cell 0's back, the cell length.
  pub a0: f64,
  pub h: f64,
  /// The bar's radius at each cell centre, and the part's (−∞ off the part).
  pub stock: Vec<f64>,
  pub part: Vec<f64>,
  pub gouged: Vec<bool>,
  pub warnings: Vec<Warning>,
  pub removed: Vec<(String, f64)>,
  axis_point: DVec3,
  axis: DVec3,
  radial: DVec3,
}

impl TurnSim {
  /// Bar left on the part (mm³), off the part only up to the part-off cut.
  pub fn remaining(&self) -> f64 {
    let pi = std::f64::consts::PI;
    self
      .stock
      .iter()
      .zip(&self.part)
      .filter(|(_, p)| p.is_finite())
      .map(|(s, p)| pi * (s.max(0.0).powi(2) - p.max(0.0).powi(2)).max(0.0) * self.h)
      .sum()
  }

  /// The bar as a solid of revolution for display; gouges in red.
  pub fn mesh(&self) -> SimMesh {
    let mut m = SimMesh::default();
    let seg = 48;
    let side = self.axis.cross(self.radial);
    let step = self.stock.len().div_ceil(500).max(1);
    let rings: Vec<(f64, f64, bool)> = (0..self.stock.len())
      .step_by(step)
      .map(|i| {
        let j = (i + step).min(self.stock.len());
        let r = self.stock[i..j].iter().copied().fold(f64::INFINITY, f64::min).max(0.0);
        (self.a0 + (i as f64 + 0.5) * self.h, r, self.gouged[i..j].iter().any(|g| *g))
      })
      .collect();
    let stock = [196, 170, 120, 255];
    let red = [220, 40, 40, 255];
    let at = |a: f64, r: f64, k: usize| {
      let t = std::f64::consts::TAU * k as f64 / seg as f64;
      let n = self.radial * t.cos() + side * t.sin();
      (self.axis_point + self.axis * a + n * r, n)
    };
    for (a, r, g) in &rings {
      for k in 0..=seg {
        let (p, n) = at(*a, *r, k);
        m.positions.push(p.as_vec3().to_array());
        m.normals.push(n.as_vec3().to_array());
        m.colors.push(if *g { red } else { stock });
      }
    }
    let w = (seg + 1) as u32;
    for i in 0..rings.len().saturating_sub(1) as u32 {
      for k in 0..seg as u32 {
        let (a, b) = (i * w + k, (i + 1) * w + k);
        m.indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
      }
    }
    // the ends
    for (end, normal) in [(0, -self.axis), (rings.len().saturating_sub(1), self.axis)] {
      let Some((a, r, _)) = rings.get(end) else { continue };
      let centre = m.positions.len() as u32;
      m.positions.push((self.axis_point + self.axis * *a).as_vec3().to_array());
      m.normals.push(normal.as_vec3().to_array());
      m.colors.push(stock);
      for k in 0..=seg {
        m.positions.push(at(*a, *r, k).0.as_vec3().to_array());
        m.normals.push(normal.as_vec3().to_array());
        m.colors.push(stock);
      }
      for k in 0..seg as u32 {
        let (p, q) = (centre + 1 + k, centre + 2 + k);
        if end == 0 {
          m.indices.extend_from_slice(&[centre, q, p]);
        } else {
          m.indices.extend_from_slice(&[centre, p, q]);
        }
      }
    }
    m
  }
}

/// Runs a lathe setup's toolpaths through the bar: removed volume, gouges and the tools running
/// into the chuck.
pub fn simulate(l: &Lathe, setup: &Setup, model: &Model, paths: &[Toolpath]) -> Result<TurnSim, String> {
  let ctx = Ctx::new(l, setup, model)?;
  let h = ctx.prof.h.max((ctx.stock_front - ctx.bar_end) / 8000.0);
  let n = ((ctx.stock_front - ctx.bar_end) / h).ceil().max(1.0) as usize;
  let a0 = ctx.bar_end;
  let part: Vec<f64> = (0..n)
    .map(|i| {
      let a = a0 + (i as f64 + 0.5) * h;
      ctx.prof.cell(a).map_or(f64::NEG_INFINITY, |k| ctx.prof.r[k])
    })
    .collect();
  let mut sim = TurnSim {
    a0,
    h,
    stock: vec![ctx.bar; n],
    part,
    gouged: vec![false; n],
    warnings: Vec::new(),
    removed: Vec::new(),
    axis_point: ctx.frame.p,
    axis: ctx.frame.d,
    radial: ctx.frame.e,
  };
  let pi = std::f64::consts::PI;
  let mut at = (ctx.stock_front + CLEAR - ctx.z0, ctx.bar + CLEAR);
  for tp in paths {
    let tool = &tp.tool;
    let threading = tool.kind == ToolKind::Thread;
    // a threading insert's V reaches a pitch either side of its tip
    let flank = tp.turn.iter().map(|m| m.pitch).fold(0.5, f64::max);
    let mut notes = Notes::new(&tp.name);
    let mut removed = 0.0;
    for m in &tp.turn {
      notes.next();
      let len = ((m.z - at.0).powi(2) + (m.x - at.1).powi(2)).sqrt();
      let steps = ((len / (h / 2.0)).ceil() as usize).clamp(1, 1_000_000);
      for s in 0..=steps {
        let f = s as f64 / steps as f64;
        let (a, x) = (at.0 + (m.z - at.0) * f + ctx.z0, at.1 + (m.x - at.1) * f);
        if a < ctx.chuck - 1e-6 && x < ctx.chuck_r {
          notes.note(WarningKind::Chuck, ctx.frame.world(a, x), ctx.chuck - a);
        }
        // the cells under the insert and how low it reaches in each
        let (lo, hi) = match tool.kind {
          ToolKind::Turning => (a, a + 2.0 * tool.corner_radius),
          ToolKind::Groove | ToolKind::PartOff => (a, a + tool.cut_width()),
          _ => (a - flank, a + flank),
        };
        let i0 = ((lo - a0) / h - 0.5).ceil().max(0.0) as usize;
        let i1 = ((hi - a0) / h - 0.5).floor();
        if i1 < 0.0 {
          continue;
        }
        for i in i0..=(i1 as usize).min(n - 1) {
          let c = a0 + (i as f64 + 0.5) * h;
          let low = match tool.kind {
            ToolKind::Turning => {
              let rn = tool.corner_radius;
              let dz = c - (a + rn);
              x + rn - (rn * rn - dz * dz).max(0.0).sqrt()
            }
            ToolKind::Groove | ToolKind::PartOff => x,
            _ => x + (c - a).abs() / (tool.angle.clamp(1.0, 179.0) / 2.0).to_radians().tan(),
          };
          let old = sim.stock[i];
          if low >= old {
            continue;
          }
          if m.rapid {
            if old - low > GOUGE_TOLERANCE {
              notes.note(WarningKind::RapidCollision, ctx.frame.world(a, x), old - low);
            }
            continue;
          }
          sim.stock[i] = low;
          removed += pi * (old.max(0.0).powi(2) - low.max(0.0).powi(2)) * h;
          if !threading && sim.part[i].is_finite() && low < sim.part[i] - GOUGE_TOLERANCE {
            sim.gouged[i] = true;
            notes.note(WarningKind::Gouge, ctx.frame.world(a, x), sim.part[i] - low);
          }
        }
      }
      at = (m.z, m.x);
    }
    sim.warnings.extend(notes.list);
    sim.removed.push((tp.name.clone(), removed));
  }
  Ok(sim)
}

// ---------------------------------------------------------------- lathe posts

/// A lathe control's dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
  Fanuc,
  Haas,
  LinuxCnc,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LathePost {
  pub id: &'static str,
  pub name: &'static str,
  pub extension: &'static str,
  pub dialect: Dialect,
}

pub static LATHE_POSTS: [LathePost; 3] = [
  LathePost { id: "fanuc_lathe", name: "Fanuc lathe", extension: "nc", dialect: Dialect::Fanuc },
  LathePost { id: "haas_lathe", name: "Haas lathe", extension: "nc", dialect: Dialect::Haas },
  LathePost { id: "linuxcnc_lathe", name: "LinuxCNC lathe", extension: "ngc", dialect: Dialect::LinuxCnc },
];

/// A lathe post by id ("" = Fanuc); the milling posts' ids name their lathe counterparts.
pub fn find_post(id: &str) -> Option<&'static LathePost> {
  let id = id.trim().to_ascii_lowercase();
  let id = match id.trim_end_matches("_lathe").trim_end_matches("-lathe") {
    "" | "fanuc" => "fanuc_lathe",
    "haas" => "haas_lathe",
    "linuxcnc" | "emc" | "emc2" => "linuxcnc_lathe",
    _ => id.as_str(),
  };
  LATHE_POSTS.iter().find(|p| p.id == id)
}

struct LOut {
  dialect: Dialect,
  scale: f64,
  imperial: bool,
  g: String,
}

impl LOut {
  fn line(&mut self, s: &str) {
    let _ = writeln!(self.g, "{s}");
  }

  fn fmt(&self, v: f64, places: usize) -> String {
    let s = format!("{:.*}", places, v);
    let mut s = s.trim_end_matches('0').to_string();
    if s == "-." || s == "-0." {
      s = "0.".into();
    }
    if self.dialect == Dialect::LinuxCnc {
      s = s.trim_end_matches('.').to_string();
      if s.is_empty() || s == "-" {
        s = "0".into();
      }
    }
    s
  }

  fn num(&self, v: f64) -> String {
    self.fmt(v * self.scale, if self.imperial { 4 } else { 3 })
  }

  /// Feed per revolution in the program's units.
  fn per_rev(&self, v: f64) -> String {
    self.fmt(v * self.scale, if self.imperial { 5 } else { 4 })
  }

  fn code(&self, letter: char, n: u32) -> String {
    if self.dialect == Dialect::Haas { format!("{letter}{n:02}") } else { format!("{letter}{n}") }
  }

  fn home(&mut self) {
    match self.dialect {
      Dialect::LinuxCnc => self.line("G28"),
      Dialect::Fanuc => {
        self.line("G28 U0.");
        self.line("G28 W0.");
      }
      Dialect::Haas => {
        self.line("G28 U0.");
        self.line("G28 W0.");
      }
    }
  }
}

/// A lathe program for a setup's toolpaths: X as a diameter, feeds per revolution, constant
/// surface speed where the tool has one, threads as spindle-synchronised moves.
pub fn write(post: &LathePost, imperial: bool, title: &str, paths: &[Toolpath]) -> String {
  let mut o = LOut { dialect: post.dialect, scale: if imperial { 1.0 / 25.4 } else { 1.0 }, imperial, g: String::new() };
  let comment = super::post::comment;
  let units = if imperial { "G20" } else { "G21" };
  if post.dialect != Dialect::LinuxCnc {
    o.line("%");
    o.line(&format!("O2000 ({})", comment(title)));
  } else {
    o.line(&format!("({})", comment(title)));
  }
  o.line(&format!("(FISSIONOXIDE - {} POST, {})", comment(post.name), if imperial { "INCH" } else { "MM" }));
  o.line("(WORK ORIGIN: Z0 ON THE SETUP'S ORIGIN FACE, X0 ON THE SPINDLE AXIS - X IS A DIAMETER)");
  match post.dialect {
    Dialect::Fanuc => o.line(&format!("{units} G18 G40 G99")),
    Dialect::Haas => o.line(&format!("{units} G18 G40 G99")),
    Dialect::LinuxCnc => o.line(&format!("{units} G18 G7 G40 G90 G95")),
  }
  o.line("G54");
  for tp in paths {
    let t = &tp.tool;
    o.line(&format!("({} - {})", comment(&tp.name), comment(&t.describe())));
    o.line(&o.code('M', 9));
    o.line(&o.code('M', 5));
    o.home();
    match post.dialect {
      Dialect::Fanuc => o.line(&format!("T{0:02}{0:02}", t.number)),
      Dialect::Haas => o.line(&format!("T{}{:02}", t.number, t.number)),
      Dialect::LinuxCnc => o.line(&format!("T{} M6 G43", t.number)),
    }
    let threading = tp.turn.iter().any(|m| m.pitch > 0.0);
    let max = t.rpm.round() as i64;
    if threading || t.surface_speed <= 0.0 {
      o.line(&format!("{} S{max} {}", o.code('G', 97), o.code('M', 3)));
    } else {
      // m/min, or feet a minute in inches
      let css = if imperial { t.surface_speed * 3.28084 } else { t.surface_speed }.round() as i64;
      match post.dialect {
        Dialect::LinuxCnc => o.line(&format!("G96 D{max} S{css} M3")),
        _ => {
          o.line(&format!("G50 S{max}"));
          o.line(&format!("G96 S{css} {}", o.code('M', 3)));
        }
      }
    }
    o.line(&o.code('M', 8));
    let mut last_feed = -1.0;
    for m in &tp.turn {
      let (x, z) = (o.num(2.0 * m.x), o.num(m.z));
      if m.rapid {
        o.line(&format!("{} X{x} Z{z}", o.code('G', 0)));
        continue;
      }
      if m.pitch > 0.0 {
        let lead = o.per_rev(m.pitch);
        match post.dialect {
          Dialect::LinuxCnc => o.line(&format!("G33 X{x} Z{z} K{lead}")),
          _ => o.line(&format!("G32 X{x} Z{z} F{lead}")),
        }
        last_feed = -1.0;
        continue;
      }
      if (m.feed - last_feed).abs() > 1e-12 {
        o.line(&format!("{} X{x} Z{z} F{}", o.code('G', 1), o.per_rev(m.feed)));
        last_feed = m.feed;
      } else {
        o.line(&format!("{} X{x} Z{z}", o.code('G', 1)));
      }
    }
  }
  o.line(&o.code('M', 9));
  o.line(&o.code('M', 5));
  o.home();
  if post.dialect == Dialect::LinuxCnc {
    o.line("M2");
  } else {
    o.line("M30");
    o.line("%");
  }
  o.g
}

/// A minimal lathe G-code reader for checking programs: the moves in millimetres, X as a
/// radius. Returns (rapid, z, x, feed per revolution, thread lead).
pub fn read(g: &str) -> Result<Vec<LatheMove>, String> {
  let mut out = Vec::new();
  let (mut z, mut x, mut feed) = (0.0, 0.0, 0.0);
  let mut scale = 1.0;
  let mut motion = 0;
  let mut diameter = true;
  for (n, raw) in g.lines().enumerate() {
    let mut line = String::new();
    let mut depth = 0;
    for c in raw.chars() {
      match c {
        '(' => depth += 1,
        ')' => depth -= 1,
        _ if depth == 0 => line.push(c),
        _ => {}
      }
    }
    let line = line.trim().to_ascii_uppercase();
    if line.is_empty() || line == "%" {
      continue;
    }
    let mut words: Vec<(char, f64)> = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
      if c.is_whitespace() {
        continue;
      }
      if !c.is_ascii_alphabetic() {
        return Err(format!("line {}: unexpected '{c}'", n + 1));
      }
      let mut v = String::new();
      while let Some(&d) = chars.peek() {
        if d.is_ascii_digit() || d == '.' || d == '-' || d == '+' {
          v.push(d);
          chars.next();
        } else {
          break;
        }
      }
      words.push((c, v.parse().map_err(|_| format!("line {}: bad number after '{c}'", n + 1))?));
    }
    let get = |l: char| words.iter().find(|w| w.0 == l).map(|w| w.1);
    let mut skip = false;
    for gc in words.iter().filter(|w| w.0 == 'G').map(|w| (w.1 * 10.0).round() as u32) {
      match gc {
        0 | 10 | 320 | 330 => motion = gc,
        70 => diameter = true,
        80 => diameter = false,
        200 => scale = 25.4,
        210 => scale = 1.0,
        280 | 500 | 530 => skip = true,
        _ => {}
      }
    }
    if skip {
      continue;
    }
    if motion != 320 && let Some(f) = get('F') {
      feed = f * scale;
    }
    let (gx, gz) = (get('X'), get('Z'));
    if gx.is_none() && gz.is_none() {
      continue;
    }
    if let Some(v) = gx {
      x = v * scale / if diameter { 2.0 } else { 1.0 };
    }
    if let Some(v) = gz {
      z = v * scale;
    }
    let pitch = match motion {
      320 => get('F').ok_or(format!("line {}: G32 without a lead", n + 1))? * scale,
      330 => get('K').ok_or(format!("line {}: G33 without a lead", n + 1))? * scale,
      _ => 0.0,
    };
    out.push(LatheMove { rapid: motion == 0, z, x, feed: if pitch > 0.0 { pitch } else if motion == 0 { 0.0 } else { feed }, pitch });
  }
  Ok(out)
}
