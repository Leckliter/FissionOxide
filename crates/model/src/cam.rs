//! Manufacture (CAM): milling setups with 2.5-axis operations — facing, 2D pockets, 2D contours
//! and drilling — their toolpaths, computed from the model as it is now, and G-code.
//!
//! Operations refer to faces like features do, so changing the design moves the toolpaths with
//! it. Coordinates in G-code are relative to the setup's work origin: the top of the stock at
//! its minimum X/Y corner (Z is negative into the stock).

use clf_kernel::glam::DVec3;
use clf_kernel::{Shape, SurfaceKind};
use serde::{Deserialize, Serialize};

use crate::regen::{Body, Model};
use crate::{BodyId, FaceRef};

/// Clearance above the stock for rapid moves between operations (mm).
const CLEARANCE: f64 = 10.0;
/// Retract height above the stock between passes (mm).
const RETRACT: f64 = 3.0;
/// Most passes one operation may make (guards against tiny step-overs).
const MAX_PASSES: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Setup {
  pub id: u32,
  pub name: String,
  /// The part to machine; None = every body.
  #[serde(default)]
  pub body: Option<BodyId>,
  /// Stock beyond the part on each side, and above its top (mm).
  #[serde(default = "two")]
  pub stock_side: f64,
  #[serde(default = "one")]
  pub stock_top: f64,
  #[serde(default)]
  pub operations: Vec<Operation>,
}

fn one() -> f64 {
  1.0
}

fn two() -> f64 {
  2.0
}

/// A flat end mill (drills use the same record: the diameter is the drill's).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
  /// Tool number in the machine's changer.
  pub number: u32,
  pub diameter: f64,
  /// Spindle speed (rpm).
  #[serde(default = "rpm")]
  pub rpm: f64,
  /// Cutting feed (mm/min).
  #[serde(default = "feed")]
  pub feed: f64,
  /// Plunge feed (mm/min).
  #[serde(default = "plunge")]
  pub plunge: f64,
}

fn rpm() -> f64 {
  10000.0
}

fn feed() -> f64 {
  1000.0
}

fn plunge() -> f64 {
  300.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Operation {
  pub name: String,
  pub kind: OpKind,
  pub tool: Tool,
  /// Sideways step as a fraction of the tool diameter.
  #[serde(default = "stepover")]
  pub stepover: f64,
  /// Depth per pass (mm).
  #[serde(default = "stepdown")]
  pub stepdown: f64,
  /// Material left on walls for a finishing pass (mm).
  #[serde(default)]
  pub stock_to_leave: f64,
}

fn stepover() -> f64 {
  0.4
}

fn stepdown() -> f64 {
  2.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpKind {
  /// Faces the stock down to the top of the part.
  Face,
  /// Clears pockets down to flat floor faces (their outline is the pocket wall, holes in them
  /// are islands left standing).
  Pocket { floors: Vec<FaceRef> },
  /// Follows the outline of faces (default the part's bottom face) outside it (or inside), down
  /// through the part.
  Contour {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    inside: bool,
  },
  /// Drills holes (cylindrical faces with a vertical axis); `peck` > 0 pecks.
  Drill {
    #[serde(default)]
    holes: Vec<FaceRef>,
    #[serde(default)]
    peck: f64,
  },
}

/// One move of a toolpath, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Move {
  pub rapid: bool,
  pub to: DVec3,
  /// Feed rate for cutting moves (mm/min).
  pub feed: f64,
}

/// The toolpath of one operation.
#[derive(Debug, Clone)]
pub struct Toolpath {
  pub name: String,
  pub tool: Tool,
  pub moves: Vec<Move>,
  /// Drilling: the holes (top point, depth) for canned cycles in G-code.
  pub drill: Option<(Vec<(DVec3, f64)>, f64)>,
}

impl Toolpath {
  /// Cutting and rapid travel (mm) and the time it takes (min), from the start position.
  pub fn stats(&self, from: DVec3) -> (f64, f64, f64) {
    let (mut cut, mut rapid, mut time) = (0.0, 0.0, 0.0);
    let mut at = from;
    for m in &self.moves {
      let l = (m.to - at).length();
      if m.rapid {
        rapid += l;
        time += l / 5000.0;
      } else {
        cut += l;
        time += l / m.feed.max(1.0);
      }
      at = m.to;
    }
    (cut, rapid, time)
  }
}

/// The stock box (min, max) of a setup.
pub fn stock(setup: &Setup, model: &Model) -> Result<(DVec3, DVec3), String> {
  let (lo, hi) = part_box(setup, model)?;
  let side = DVec3::new(setup.stock_side, setup.stock_side, 0.0);
  Ok((lo - side, hi + side + DVec3::new(0.0, 0.0, setup.stock_top)))
}

/// The work origin: the top of the stock at its minimum X/Y corner.
pub fn origin(setup: &Setup, model: &Model) -> Result<DVec3, String> {
  let (lo, hi) = stock(setup, model)?;
  Ok(DVec3::new(lo.x, lo.y, hi.z))
}

fn bodies<'a>(setup: &Setup, model: &'a Model) -> Vec<&'a Body> {
  model.bodies.iter().filter(|b| setup.body.is_none_or(|id| id == b.id)).collect()
}

fn part_box(setup: &Setup, model: &Model) -> Result<(DVec3, DVec3), String> {
  let mut acc: Option<(DVec3, DVec3)> = None;
  for b in bodies(setup, model) {
    let m = b.shape.mass().map_err(|e| e.to_string())?;
    acc = Some(match acc {
      Some((a, c)) => (a.min(m.bbox_min), c.max(m.bbox_max)),
      None => (m.bbox_min, m.bbox_max),
    });
  }
  // bounding boxes carry the kernel's tolerance gap: round it off to a micron
  let snap = |v: DVec3| (v * 1000.0).round() / 1000.0;
  acc.map(|(a, b)| (snap(a), snap(b))).ok_or_else(|| "the setup has no part to machine".to_string())
}

/// Resolves a face reference against the model: the body and the face's current id.
fn resolve<'a>(model: &'a Model, f: &FaceRef) -> Result<(&'a Body, u32), String> {
  let b = model.body(f.body).ok_or_else(|| format!("body {} no longer exists", f.body.0))?;
  let id = f.resolve(b.faces(), b.mesh(), b.diagonal()).ok_or("a face of the operation can't be found")?;
  Ok((b, id))
}

/// Toolpaths of every operation of a setup.
pub fn toolpaths(setup: &Setup, model: &Model) -> Result<Vec<Toolpath>, String> {
  setup.operations.iter().map(|op| toolpath(setup, op, model).map_err(|e| format!("{}: {e}", op.name))).collect()
}

fn toolpath(setup: &Setup, op: &Operation, model: &Model) -> Result<Toolpath, String> {
  let d = op.tool.diameter;
  if d.is_nan() || d <= 0.0 {
    return Err("the tool diameter must be positive".into());
  }
  if op.stepdown.is_nan() || op.stepdown <= 0.0 || op.stepover.is_nan() || op.stepover <= 0.0 || op.stepover > 1.0 {
    return Err("step-down must be positive and step-over between 0 and 1".into());
  }
  let (slo, shi) = stock(setup, model)?;
  let (plo, phi) = part_box(setup, model)?;
  let safe = shi.z + CLEARANCE;
  let mut p = Path { moves: Vec::new(), feed: op.tool.feed, plunge: op.tool.plunge, safe };
  let r = d / 2.0;
  let mut drill = None;
  match &op.kind {
    OpKind::Face => {
      let step = d * op.stepover;
      let (x0, x1) = (slo.x - r, shi.x + r);
      for z in levels(shi.z, phi.z, op.stepdown) {
        // rows a step-over apart from one side of the stock to the other, the last on the edge
        let mut y = slo.y + r;
        let mut forward = true;
        let mut first = true;
        while y < shi.y - r + step - 1e-9 {
          let yy = y.min(shi.y - r);
          let (a, b) = if forward { (x0, x1) } else { (x1, x0) };
          if first {
            p.enter(DVec3::new(a, yy, z));
            first = false;
          } else {
            p.cut(DVec3::new(a, yy, z));
          }
          p.cut(DVec3::new(b, yy, z));
          forward = !forward;
          y += step;
          if p.moves.len() > MAX_PASSES * 4 {
            return Err("too many passes: raise the step-over".into());
          }
        }
        p.leave();
      }
    }
    OpKind::Pocket { floors } => {
      if floors.is_empty() {
        return Err("pick the pocket floors".into());
      }
      for f in floors {
        let (b, id) = resolve(model, f)?;
        let info = b.faces().iter().find(|x| x.id == id).ok_or("a floor face can't be found")?;
        if info.kind != SurfaceKind::Plane || info.normal.z < 0.999 {
          return Err("pocket floors must be flat faces facing up".into());
        }
        let face = b.shape.face_shape(id).map_err(|e| e.to_string())?;
        // rings from the wall inwards; the centre is cut first, working out to the wall
        let rings = rings(&face, r + op.stock_to_leave, d * op.stepover)?;
        if rings.is_empty() {
          return Err("the tool doesn't fit in the pocket".into());
        }
        let floor = info.origin.z;
        for z in levels(phi.z, floor, op.stepdown) {
          for ring in rings.iter().rev() {
            for lp in ring {
              let lp = oriented(lp, true);
              p.enter(at_z(lp[0], z));
              for q in &lp[1..] {
                p.cut(at_z(*q, z));
              }
              p.cut(at_z(lp[0], z));
              p.leave();
            }
          }
        }
      }
    }
    OpKind::Contour { faces, inside } => {
      let outlines: Vec<Shape> = if faces.is_empty() {
        // the part's lowest face pointing down
        let mut best: Option<(f64, Shape)> = None;
        for b in bodies(setup, model) {
          for f in b.faces() {
            if f.kind == SurfaceKind::Plane && f.normal.z < -0.999 && best.as_ref().is_none_or(|(z, _)| f.origin.z < *z)
            {
              best = Some((f.origin.z, b.shape.face_shape(f.id).map_err(|e| e.to_string())?));
            }
          }
        }
        vec![best.ok_or("the part has no flat bottom face: pick the faces to contour")?.1]
      } else {
        faces
          .iter()
          .map(|f| resolve(model, f).and_then(|(b, id)| b.shape.face_shape(id).map_err(|e| e.to_string())))
          .collect::<Result<_, _>>()?
      };
      let off = (r + op.stock_to_leave) * if *inside { -1.0 } else { 1.0 };
      let mut loops = Vec::new();
      for f in &outlines {
        let all = f.offset_loops(off, 0.01).map_err(|e| e.to_string())?;
        // outside: the outer loop only; inside: every loop
        let mut ls: Vec<Vec<DVec3>> = all;
        ls.sort_by(|a, b| area(b).abs().total_cmp(&area(a).abs()));
        if !*inside {
          ls.truncate(1);
        }
        loops.extend(ls);
      }
      if loops.is_empty() {
        return Err("the contour can't be offset by the tool radius".into());
      }
      // cut through the part, a little below its bottom
      for z in levels(shi.z, plo.z - 0.5, op.stepdown) {
        for lp in &loops {
          // climb milling: clockwise outside a part, counter-clockwise inside
          let lp = oriented(lp, *inside);
          p.enter(at_z(lp[0], z));
          for q in &lp[1..] {
            p.cut(at_z(*q, z));
          }
          p.cut(at_z(lp[0], z));
          p.leave();
        }
      }
    }
    OpKind::Drill { holes, peck } => {
      let mut found = Vec::new();
      let faces: Vec<(&Body, u32)> = if holes.is_empty() {
        // every vertical hole in the part
        bodies(setup, model)
          .into_iter()
          .flat_map(|b| b.faces().iter().filter(|f| f.kind == SurfaceKind::Cylinder).map(move |f| (b, f.id)))
          .collect()
      } else {
        holes.iter().map(|h| resolve(model, h)).collect::<Result<_, _>>()?
      };
      for (b, id) in faces {
        let Some(f) = b.faces().iter().find(|x| x.id == id) else { continue };
        if f.kind != SurfaceKind::Cylinder || f.normal.z.abs() < 0.999 {
          if !holes.is_empty() {
            return Err("drill holes must be vertical cylinders".into());
          }
          continue;
        }
        let Some((zlo, zhi, hole)) = cylinder_span(b, id) else { continue };
        if !hole {
          continue;
        }
        let top = DVec3::new(f.origin.x, f.origin.y, zhi);
        if !found.iter().any(|(q, _): &(DVec3, f64)| (*q - top).length() < 1e-6) {
          found.push((top, zhi - zlo));
        }
      }
      if found.is_empty() {
        return Err("there are no vertical holes to drill".into());
      }
      // nearest neighbour order from the work origin
      let mut at = origin(setup, model)?;
      let mut ordered = Vec::new();
      while !found.is_empty() {
        let (i, _) = found
          .iter()
          .enumerate()
          .min_by(|a, b| (a.1.0 - at).length().total_cmp(&(b.1.0 - at).length()))
          .ok_or("no holes")?;
        let h = found.remove(i);
        at = h.0;
        ordered.push(h);
      }
      for (top, depth) in &ordered {
        p.rapid(DVec3::new(top.x, top.y, top.z + RETRACT));
        let bottom = top.z - depth;
        if *peck > 0.0 {
          let mut z = top.z;
          while z > bottom + 1e-9 {
            z = (z - peck).max(bottom);
            p.moves.push(Move { rapid: false, to: DVec3::new(top.x, top.y, z), feed: op.tool.plunge });
            p.rapid(DVec3::new(top.x, top.y, top.z + RETRACT));
          }
        } else {
          p.moves.push(Move { rapid: false, to: DVec3::new(top.x, top.y, bottom), feed: op.tool.plunge });
          p.rapid(DVec3::new(top.x, top.y, top.z + RETRACT));
        }
      }
      p.rapid_up();
      drill = Some((ordered, *peck));
    }
  }
  Ok(Toolpath { name: op.name.clone(), tool: op.tool.clone(), moves: p.moves, drill })
}

struct Path {
  moves: Vec<Move>,
  feed: f64,
  plunge: f64,
  safe: f64,
}

impl Path {
  fn rapid(&mut self, to: DVec3) {
    self.moves.push(Move { rapid: true, to, feed: 0.0 });
  }

  fn rapid_up(&mut self) {
    if let Some(last) = self.moves.last().map(|m| m.to) {
      self.rapid(DVec3::new(last.x, last.y, self.safe));
    }
  }

  /// Over the start at the clearance height, then plunge.
  fn enter(&mut self, at: DVec3) {
    self.rapid(DVec3::new(at.x, at.y, self.safe));
    self.rapid(DVec3::new(at.x, at.y, at.z + RETRACT.min(self.safe - at.z)));
    self.moves.push(Move { rapid: false, to: at, feed: self.plunge });
  }

  fn cut(&mut self, to: DVec3) {
    self.moves.push(Move { rapid: false, to, feed: self.feed });
  }

  fn leave(&mut self) {
    self.rapid_up();
  }
}

/// Depths from `top` down to `bottom`, `step` apart, ending exactly at `bottom`.
fn levels(top: f64, bottom: f64, step: f64) -> Vec<f64> {
  let mut out = Vec::new();
  let mut z = top;
  while z > bottom + 1e-9 && out.len() < MAX_PASSES {
    z = (z - step).max(bottom);
    out.push(z);
  }
  if out.is_empty() {
    out.push(bottom);
  }
  out
}

/// Offset rings of a pocket floor: the first `first` in from the wall, then `step` apart, until
/// the tool no longer fits. Each ring may have several loops.
fn rings(face: &Shape, first: f64, step: f64) -> Result<Vec<Vec<Vec<DVec3>>>, String> {
  let mut out = Vec::new();
  let mut off = first;
  while out.len() < MAX_PASSES {
    let loops = face.offset_loops(-off, 0.01).map_err(|e| e.to_string())?;
    let loops: Vec<Vec<DVec3>> = loops.into_iter().filter(|l| area(l).abs() > 1e-6 && span(l) > 1e-3).collect();
    if loops.is_empty() {
      break;
    }
    out.push(loops);
    off += step;
  }
  Ok(out)
}

fn span(l: &[DVec3]) -> f64 {
  let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
  for p in l {
    lo = lo.min(*p);
    hi = hi.max(*p);
  }
  (hi - lo).length()
}

/// Signed area in XY (positive = counter-clockwise seen from above).
fn area(l: &[DVec3]) -> f64 {
  let n = l.len();
  (0..n).map(|i| l[i].x * l[(i + 1) % n].y - l[(i + 1) % n].x * l[i].y).sum::<f64>() / 2.0
}

/// A loop running counter-clockwise (seen from above) when `ccw`, else clockwise.
fn oriented(l: &[DVec3], ccw: bool) -> Vec<DVec3> {
  let mut v = l.to_vec();
  if (area(&v) > 0.0) != ccw {
    v.reverse();
  }
  v
}

fn at_z(p: DVec3, z: f64) -> DVec3 {
  DVec3::new(p.x, p.y, z)
}

/// A cylinder face's height range and whether it is a hole (normals point at the axis).
fn cylinder_span(b: &Body, face: u32) -> Option<(f64, f64, bool)> {
  let info = b.faces().iter().find(|f| f.id == face)?;
  let m = b.mesh()?;
  let (mut lo, mut hi, mut votes) = (f64::INFINITY, f64::NEG_INFINITY, 0.0);
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if m.tri_face.get(t) != Some(&face) {
      continue;
    }
    for &i in tri {
      let (Some(p), Some(n)) = (m.positions.get(i as usize), m.normals.get(i as usize)) else { continue };
      let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
      lo = lo.min(p.z);
      hi = hi.max(p.z);
      let radial = DVec3::new(p.x - info.origin.x, p.y - info.origin.y, 0.0);
      votes += DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64).dot(radial);
    }
  }
  lo.is_finite().then_some((lo, hi, votes < 0.0))
}

/// G-code (Fanuc/Haas style, metric) for a setup's toolpaths, relative to its work origin.
pub fn gcode(setup: &Setup, paths: &[Toolpath], origin: DVec3) -> String {
  use std::fmt::Write;
  let f = |v: f64| {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" { "0".to_string() } else { s }
  };
  let mut g = String::new();
  let _ = writeln!(g, "%");
  let _ = writeln!(g, "(CLFUSION - {})", setup.name.to_uppercase());
  let _ = writeln!(g, "(WORK ORIGIN: STOCK TOP, MIN X/Y CORNER - G54)");
  let _ = writeln!(g, "G90 G94 G17 G21");
  let _ = writeln!(g, "G54");
  let mut tool = None;
  for tp in paths {
    let _ = writeln!(g, "({} - D{} MM)", tp.name.to_uppercase(), f(tp.tool.diameter));
    if tool != Some(tp.tool.number) {
      let _ = writeln!(g, "M5");
      let _ = writeln!(g, "G28 G91 Z0");
      let _ = writeln!(g, "G90");
      let _ = writeln!(g, "T{} M6", tp.tool.number);
      let _ = writeln!(g, "S{} M3", tp.tool.rpm.round() as i64);
      let _ = writeln!(g, "G43 H{}", tp.tool.number);
      tool = Some(tp.tool.number);
    }
    let local = |p: DVec3| p - origin;
    if let Some((holes, peck)) = &tp.drill {
      let r = RETRACT;
      let mut first = true;
      for (top, depth) in holes {
        let t = local(*top);
        if first {
          let _ = writeln!(g, "G0 X{} Y{} Z{}", f(t.x), f(t.y), f(t.z + CLEARANCE));
          let cycle = if *peck > 0.0 { format!("G83 Q{}", f(*peck)) } else { "G81".to_string() };
          let _ = writeln!(
            g,
            "G98 {cycle} X{} Y{} Z{} R{} F{}",
            f(t.x),
            f(t.y),
            f(t.z - depth),
            f(t.z + r),
            f(tp.tool.plunge)
          );
          first = false;
        } else {
          let _ = writeln!(g, "X{} Y{} Z{} R{}", f(t.x), f(t.y), f(t.z - depth), f(t.z + r));
        }
      }
      let _ = writeln!(g, "G80");
      continue;
    }
    let mut last_feed = -1.0;
    for m in &tp.moves {
      let p = local(m.to);
      if m.rapid {
        let _ = writeln!(g, "G0 X{} Y{} Z{}", f(p.x), f(p.y), f(p.z));
      } else if (m.feed - last_feed).abs() > 1e-9 {
        let _ = writeln!(g, "G1 X{} Y{} Z{} F{}", f(p.x), f(p.y), f(p.z), f(m.feed));
        last_feed = m.feed;
      } else {
        let _ = writeln!(g, "G1 X{} Y{} Z{}", f(p.x), f(p.y), f(p.z));
      }
    }
  }
  let _ = writeln!(g, "M5");
  let _ = writeln!(g, "G28 G91 Z0");
  let _ = writeln!(g, "G90");
  let _ = writeln!(g, "M30");
  let _ = writeln!(g, "%");
  g
}
