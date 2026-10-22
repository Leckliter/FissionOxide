//! Manufacture (CAM): milling setups with 2.5-axis operations — facing, 2D pockets, 2D contours
//! and drilling — their toolpaths, computed from the model as it is now, and G-code. Milling
//! setups may name a 4/5-axis machine ([`axes`]): operations then take a tilted tool axis (3+2),
//! and rotary and swarf operations move the axis as they cut. Lathe setups ([`turn`]) turn
//! the part instead.
//!
//! Operations refer to faces like features do, so changing the design moves the toolpaths with
//! it. Coordinates in G-code are relative to the setup's work origin: the top of the stock at
//! its minimum X/Y corner (Z is negative into the stock).

use fox_kernel::glam::DVec3;
use fox_kernel::{Shape, SurfaceKind};
use serde::{Deserialize, Serialize};

use crate::regen::{Body, Model};
use crate::{BodyId, FaceRef};

pub mod adaptive;
pub mod adaptive3d;
pub mod arcs;
pub mod axes;
pub mod collide;
pub mod finish3d;
pub mod post;
pub mod region;
pub mod rest;
pub mod sim;
pub mod surface;
pub mod tools;
pub mod turn;
pub mod voxel;

pub use tools::{HolderStep, Library, ToolKind};

/// Clearance above the stock for rapid moves between operations (mm).
pub(crate) const CLEARANCE: f64 = 10.0;
/// Retract height above the stock between passes (mm).
pub(crate) const RETRACT: f64 = 3.0;
/// Most passes one operation may make (guards against tiny step-overs).
pub(crate) const MAX_PASSES: usize = 2000;

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
  /// Post-processor for its G-code (see [`post`]; "" = Fanuc).
  #[serde(default)]
  pub post: String,
  /// G-code in inches.
  #[serde(default)]
  pub imperial: bool,
  /// Keep straight moves in G-code (no G2/G3 arcs fitted to the toolpaths).
  #[serde(default)]
  pub straight: bool,
  /// How far an arc may stray from the moves it replaces (mm; 0 = 0.01).
  #[serde(default)]
  pub arc_tolerance: f64,
  /// Bodies that hold the part (vise jaws, clamps, a fixture plate): never machined, and
  /// checked for collisions with the tool and its holder.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub fixtures: Vec<BodyId>,
  /// A lathe setup: the part turns about this spindle axis (milling settings above are unused).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub lathe: Option<turn::Lathe>,
  /// A 4/5-axis milling machine: its kinematics for tilted and rotary operations.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub machine: Option<axes::Machine>,
  /// The stock box in a tilted operation's frame (set while computing it).
  #[serde(skip)]
  pub frame_stock: Option<(DVec3, DVec3)>,
}

fn one() -> f64 {
  1.0
}

fn two() -> f64 {
  2.0
}

/// A cutting tool (see [`tools`] for the shapes and the library).
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
  #[serde(default)]
  pub kind: ToolKind,
  /// Name in the tool library ("" = described by its shape).
  #[serde(default)]
  pub name: String,
  /// Bull-nose corner radius (mm).
  #[serde(default)]
  pub corner_radius: f64,
  /// Drill point or chamfer mill included angle (degrees).
  #[serde(default = "tip_angle")]
  pub angle: f64,
  /// Chamfer mill: diameter of the flat at its tip (mm).
  #[serde(default)]
  pub tip_diameter: f64,
  /// Length of the cutting flutes (mm; 0 = unknown): deeper cuts rub the shank.
  #[serde(default)]
  pub flute_length: f64,
  #[serde(default)]
  pub flutes: u32,
  /// Diameter of the tool holder (mm; 0 = no holder checks): a plain cylinder, when it has no
  /// `holder` steps.
  #[serde(default)]
  pub holder_diameter: f64,
  /// How far the tool sticks out of its holder, tip to holder face (mm).
  #[serde(default)]
  pub stickout: f64,
  /// A stepped holder: cylinders from its face up (the last runs up to the spindle).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub holder: Vec<tools::HolderStep>,
  /// Lathe grooving and part-off insert width (mm; 0 = the diameter).
  #[serde(default)]
  pub width: f64,
  /// Lathe constant surface speed (m/min; 0 = a fixed `rpm`, which is otherwise the limit).
  #[serde(default)]
  pub surface_speed: f64,
  /// Lathe feed per revolution (mm/rev; 0 = `feed` over `rpm`).
  #[serde(default)]
  pub feed_per_rev: f64,
}

fn tip_angle() -> f64 {
  118.0
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
  /// How the tool gets down to each cutting level.
  #[serde(default)]
  pub entry: Entry,
  /// Ramp and helix angle (degrees from horizontal).
  #[serde(default = "ramp_angle")]
  pub ramp_angle: f64,
  /// Helix diameter (mm; 0 = the tool's diameter), less where the pocket is narrower.
  #[serde(default)]
  pub helix_diameter: f64,
  /// 3+2 milling: the tool axis (pointing from the tip up the tool) the operation is computed
  /// along, its work plane square to it; None = straight down Z.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tool_axis: Option<[f64; 3]>,
}

fn ramp_angle() -> f64 {
  5.0
}

/// How an operation's tool enters the material at each level.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
  /// Straight down at the plunge feed.
  #[default]
  Plunge,
  /// Down along the cut at the ramp angle.
  Ramp,
  /// Down a helix at the ramp angle (pockets; contours ramp instead).
  Helix,
}

impl Entry {
  pub fn parse(s: &str) -> Option<Entry> {
    match s.trim().to_ascii_lowercase().as_str() {
      "plunge" => Some(Entry::Plunge),
      "ramp" => Some(Entry::Ramp),
      "helix" | "helical" => Some(Entry::Helix),
      _ => None,
    }
  }
}

/// Holding tabs on a 2D contour: bridges of material left at the bottom so the part stays
/// attached to the stock.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Tabs {
  /// Tabs per loop, evenly spaced along it.
  pub count: u32,
  /// Tab width along the outline (mm).
  #[serde(default = "tab_width")]
  pub width: f64,
  /// Tab height above the bottom of the cut part (mm).
  #[serde(default = "tab_height")]
  pub height: f64,
}

fn tab_width() -> f64 {
  5.0
}

fn tab_height() -> f64 {
  2.0
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
  /// are islands left standing). `rest` only cuts what the setup's earlier operations left.
  Pocket {
    floors: Vec<FaceRef>,
    #[serde(default)]
    rest: bool,
  },
  /// Follows the outline of faces (default the part's bottom face) outside it (or inside), down
  /// through the part.
  Contour {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    inside: bool,
    #[serde(default)]
    tabs: Option<Tabs>,
  },
  /// Drills holes (cylindrical faces with a vertical axis); `peck` > 0 pecks.
  Drill {
    #[serde(default)]
    holes: Vec<FaceRef>,
    #[serde(default)]
    peck: f64,
  },
  /// Adaptive clearing of pockets down to flat floors: the cleared area grows out from a helix
  /// by the step-over each pass, so the tool's engagement stays about constant.
  Adaptive { floors: Vec<FaceRef> },
  /// 3D finishing in parallel rows across the part (faces, default all of it), the tool
  /// dropped onto its surface; `angle` turns the rows from the X axis (degrees).
  Parallel {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    angle: f64,
  },
  /// 3D finishing in rings a constant distance apart over the surface (constant scallop),
  /// from the outline of the faces (default the whole part) inwards.
  Scallop {
    #[serde(default)]
    faces: Vec<FaceRef>,
  },
  /// 3D adaptive roughing: the stock around the part cleared level by level, each level grown
  /// in from the open side (or a helix in closed areas) by the step-over so the engagement
  /// stays about constant; flat areas of the part get a level of their own. `rest` only cuts
  /// what the setup's earlier operations left (the corners a larger tool couldn't reach).
  Adaptive3d {
    #[serde(default)]
    rest: bool,
  },
  /// 3D pencil finishing: one pass along each concave corner and fillet tighter than the tool,
  /// where it touches the part on two sides at once, and `passes` more either side of it a
  /// step-over apart.
  Pencil {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    passes: u32,
  },
  /// 3D steep and shallow finishing: contour (waterline) passes a step-down apart where the
  /// part is steeper than `threshold` degrees, parallel rows a step-over apart where it isn't.
  SteepShallow {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default = "threshold")]
    threshold: f64,
  },
  /// 3D contour (waterline) finishing: closed passes at constant heights a step-down apart,
  /// where the part is at least `min_slope` degrees steep.
  Contour3d {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    min_slope: f64,
  },
  /// 4th axis: the tool kept square to the rotary (X) axis, dropped onto the part as it turns;
  /// passes along the axis a step-over apart around it (or rings around it a step-over apart
  /// along it), in levels a step-down apart from the stock down to the part.
  Rotary {
    #[serde(default)]
    around: bool,
  },
  /// Simultaneous 5-axis swarf: the side of the tool along drafted or conical walls, its axis
  /// following each wall's steepest line, in passes a step-down apart from the wall tops.
  Swarf { faces: Vec<FaceRef> },
  /// Lathe: faces the front of the bar down to the part.
  TurnFace,
  /// Lathe: roughs the outside down to the part in passes along the axis a step-down deep.
  TurnRough,
  /// Lathe: one finishing pass down the front face and along the outside profile.
  TurnFinish,
  /// Lathe: plunges the grooves (undercuts the profile passes can't reach).
  Groove,
  /// Lathe: parts the finished part off the bar at its back face.
  PartOff,
  /// Lathe: an outside thread of `pitch` along a cylinder (`faces`, else the given span from
  /// `z_start` to `z_end` along the axis, in work coordinates), `depth` deep (0 = ISO metric).
  Thread {
    pitch: f64,
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    z_start: f64,
    #[serde(default)]
    z_end: f64,
    #[serde(default)]
    depth: f64,
  },
}

impl OpKind {
  /// A lathe operation.
  pub fn turns(&self) -> bool {
    matches!(
      self,
      OpKind::TurnFace
        | OpKind::TurnRough
        | OpKind::TurnFinish
        | OpKind::Groove
        | OpKind::PartOff
        | OpKind::Thread { .. }
    )
  }

  /// The faces the operation refers to.
  pub fn faces_mut(&mut self) -> Vec<&mut FaceRef> {
    match self {
      OpKind::Pocket { floors, .. } | OpKind::Adaptive { floors } => floors.iter_mut().collect(),
      OpKind::Contour { faces, .. }
      | OpKind::Parallel { faces, .. }
      | OpKind::Scallop { faces }
      | OpKind::Pencil { faces, .. }
      | OpKind::SteepShallow { faces, .. }
      | OpKind::Contour3d { faces, .. }
      | OpKind::Swarf { faces }
      | OpKind::Thread { faces, .. } => faces.iter_mut().collect(),
      OpKind::Drill { holes, .. } => holes.iter_mut().collect(),
      OpKind::Face
      | OpKind::Adaptive3d { .. }
      | OpKind::Rotary { .. }
      | OpKind::TurnFace
      | OpKind::TurnRough
      | OpKind::TurnFinish
      | OpKind::Groove
      | OpKind::PartOff => Vec::new(),
    }
  }
}

fn threshold() -> f64 {
  45.0
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
  /// The tool axis at the end of each move (unit, tip to shank); empty = straight down Z.
  pub axes: Vec<DVec3>,
  /// Lathe moves in work coordinates (the world `moves` show them in the viewport).
  pub turn: Vec<turn::LatheMove>,
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

/// The stock box (min, max) of a setup (around the bar, for a lathe; around the turning
/// cylinder, for a rotary 4th axis).
pub fn stock(setup: &Setup, model: &Model) -> Result<(DVec3, DVec3), String> {
  if let Some(b) = setup.frame_stock {
    return Ok(b);
  }
  if let Some(l) = &setup.lathe {
    return turn::stock_box(l, setup, model);
  }
  if let Some((c, radius, x0, x1)) = axes::rotary_stock(setup, model)? {
    return Ok((DVec3::new(x0, c.y - radius, c.z - radius), DVec3::new(x1, c.y + radius, c.z + radius)));
  }
  let (lo, hi) = part_box(setup, model)?;
  let side = DVec3::new(setup.stock_side, setup.stock_side, 0.0);
  Ok((lo - side, hi + side + DVec3::new(0.0, 0.0, setup.stock_top)))
}

/// The work origin: the top of the stock at its minimum X/Y corner; on a 5-axis machine the
/// centre of the stock's bottom, where its rotary axes cross; on a rotary 4th axis the turning
/// axis at the stock's minimum X; on a lathe the axis at the front face (or the back face or the
/// stock's front).
pub fn origin(setup: &Setup, model: &Model) -> Result<DVec3, String> {
  if let Some(l) = &setup.lathe {
    return turn::world_origin(l, setup, model);
  }
  let (lo, hi) = stock(setup, model)?;
  match setup.machine.map(|m| m.kinematics) {
    Some(axes::Kinematics::Rotary) => Ok(DVec3::new(lo.x, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0)),
    Some(_) => Ok(DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z)),
    None => Ok(DVec3::new(lo.x, lo.y, hi.z)),
  }
}

/// A body's triangles finer than its display mesh (in size and in the angle between facets),
/// for lathe profiles, swarf walls and the 4/5-axis simulation.
pub(crate) fn fine_mesh(b: &Body) -> Result<fox_kernel::TriMesh, String> {
  b.shape.tessellate_with((b.diagonal() * 5e-5).clamp(0.002, 0.02), 0.05).map_err(|e| e.to_string())
}

/// The bodies to machine: the setup's part, or every body but the fixtures.
pub(crate) fn bodies<'a>(setup: &Setup, model: &'a Model) -> Vec<&'a Body> {
  model.bodies.iter().filter(|b| setup.body.is_none_or(|id| id == b.id) && !setup.fixtures.contains(&b.id)).collect()
}

/// The setup's fixtures that still exist.
pub(crate) fn fixtures<'a>(setup: &Setup, model: &'a Model) -> Vec<&'a Body> {
  model.bodies.iter().filter(|b| setup.fixtures.contains(&b.id)).collect()
}

pub(crate) fn part_box(setup: &Setup, model: &Model) -> Result<(DVec3, DVec3), String> {
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
  let id = b.find_face(f).ok_or("a face of the operation can't be found")?;
  Ok((b, id))
}

/// Toolpaths of every operation of a setup.
pub fn toolpaths(setup: &Setup, model: &Model) -> Result<Vec<Toolpath>, String> {
  if let Some(l) = &setup.lathe {
    return turn::toolpaths(l, setup, model);
  }
  let mut out = Vec::new();
  for op in &setup.operations {
    let tp = axes::toolpath(setup, op, model, &out).map_err(|e| format!("{}: {e}", op.name))?;
    out.push(tp);
  }
  Ok(out)
}

/// A pocket floor: its face and height, checked to be flat and facing up.
fn floor_face(model: &Model, f: &FaceRef) -> Result<(Shape, f64), String> {
  let (b, id) = resolve(model, f)?;
  let info = b.faces().iter().find(|x| x.id == id).ok_or("a floor face can't be found")?;
  if info.kind != SurfaceKind::Plane || info.normal.z < 0.999 {
    return Err("pocket floors must be flat faces facing up".into());
  }
  Ok((b.shape.face_shape(id).map_err(|e| e.to_string())?, info.origin.z))
}

/// `previous`: the toolpaths of the setup's earlier operations (for rest machining).
fn toolpath(setup: &Setup, op: &Operation, model: &Model, previous: &[Toolpath]) -> Result<Toolpath, String> {
  if op.kind.turns() {
    return Err("a lathe operation needs a turning setup".into());
  }
  if op.tool.kind.turns() {
    return Err("a lathe tool can't mill".into());
  }
  let d = op.tool.diameter;
  if d.is_nan() || d <= 0.0 {
    return Err("the tool diameter must be positive".into());
  }
  if op.stepdown.is_nan() || op.stepdown <= 0.0 || op.stepover.is_nan() || op.stepover <= 0.0 || op.stepover > 1.0 {
    return Err("step-down must be positive and step-over between 0 and 1".into());
  }
  op.tool.check()?;
  let drilling = matches!(op.kind, OpKind::Drill { .. });
  let chamfering = matches!(op.kind, OpKind::Contour { .. }) && op.tool.kind == ToolKind::Chamfer;
  if !drilling && !chamfering && !op.tool.kind.mills() {
    return Err(format!("a {} can't mill sideways: use an end mill", op.tool.kind.id().replace('_', "-")));
  }
  if op.entry != Entry::Plunge && !(op.ramp_angle > 0.0 && op.ramp_angle <= 90.0) {
    return Err("the ramp angle must be between 0 and 90 degrees".into());
  }
  let (slo, shi) = stock(setup, model)?;
  let (plo, phi) = part_box(setup, model)?;
  let safe = shi.z + CLEARANCE;
  let mut p = Path { moves: Vec::new(), feed: op.tool.feed, plunge: op.tool.plunge, safe, top: shi.z };
  let r = d / 2.0;
  let slope = op.ramp_angle.to_radians().tan();
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
        p.top = z;
      }
    }
    OpKind::Pocket { floors, rest } => {
      if floors.is_empty() {
        return Err("pick the pocket floors".into());
      }
      // what the earlier operations left, for rest machining
      let left = if *rest { Some(rest::remaining(setup, model, previous, &op.tool)?) } else { None };
      for f in floors {
        let (face, floor) = floor_face(model, f)?;
        // rings from the wall inwards; the centre is cut first, working out to the wall
        let rings = rings(&face, r + op.stock_to_leave, d * op.stepover)?;
        if rings.is_empty() {
          return Err("the tool doesn't fit in the pocket".into());
        }
        let region = region::Region::new(&face.face_loops(0.01).map_err(|e| e.to_string())?);
        let wall = r + op.stock_to_leave;
        // the stock above the part may not be faced yet
        let mut top = shi.z;
        for z in levels(phi.z, floor, op.stepdown) {
          p.top = top;
          if let Some(left) = &left {
            rest::cut_rings(&mut p, left, &rings, z, top, op);
            top = z;
            continue;
          }
          let mut first = true;
          for ring in rings.iter().rev() {
            for lp in ring {
              let lp: Vec<DVec3> = oriented(lp, true).into_iter().map(|q| at_z(q, z)).collect();
              if op.entry == Entry::Plunge {
                p.cut_loop(&lp, top, Entry::Plunge, slope);
                p.leave();
                continue;
              }
              // stay down between rings when the way there keeps clear of the walls
              let at = p.moves.last().map(|m| m.to);
              let link = at.filter(|a| (a.z - z).abs() < 1e-9).and_then(|a| {
                let k = nearest(&lp, a);
                region.as_ref().filter(|rg| clear_of_walls(rg, a, lp[k], wall)).map(|_| k)
              });
              if let Some(k) = link {
                let lp = rotated(&lp, k);
                p.cut(lp[0]);
                p.follow(&lp);
                continue;
              }
              p.leave();
              // a helix down at the pocket's widest point for its first ring, else a ramp
              let helix = (first && op.entry == Entry::Helix)
                .then(|| region.as_ref().and_then(|rg| helix_at(rg, op, wall, &lp)))
                .flatten();
              match helix {
                Some((c, rho, k)) => {
                  p.helix(c, rho, top, z, slope);
                  let lp = rotated(&lp, k);
                  p.cut(lp[0]);
                  p.follow(&lp);
                }
                None => p.cut_loop(&lp, top, Entry::Ramp, slope),
              }
              first = false;
            }
          }
          p.leave();
          top = z;
        }
      }
    }
    OpKind::Contour { faces, inside, tabs } => {
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
      let tabs = tabs.filter(|t| t.count > 0);
      if let Some(t) = tabs
        && !(t.width > 0.0 && t.height > 0.0)
      {
        return Err("tabs need a positive width and height".into());
      }
      // cut through the part, a little below its bottom
      let mut top = shi.z;
      for z in levels(shi.z, plo.z - 0.5, op.stepdown) {
        p.top = top;
        for lp in &loops {
          // climb milling: clockwise outside a part, counter-clockwise inside
          let lp = oriented(lp, *inside);
          let lp: Vec<DVec3> = match tabs {
            // bridges up to the tab height where this level cuts below it
            Some(t) if z < plo.z + t.height - 1e-9 => tabbed(&lp, z, plo.z + t.height, t, r)?,
            _ => lp.iter().map(|q| at_z(*q, z)).collect(),
          };
          let entry = if op.entry == Entry::Plunge { Entry::Plunge } else { Entry::Ramp };
          p.cut_loop(&lp, top, entry, slope);
          p.leave();
        }
        top = z;
      }
    }
    OpKind::Adaptive { floors } => {
      if floors.is_empty() {
        return Err("pick the pocket floors".into());
      }
      for f in floors {
        let (face, floor) = floor_face(model, f)?;
        let region = region::Region::new(&face.face_loops(0.01).map_err(|e| e.to_string())?)
          .ok_or("the pocket floor has no outline")?;
        adaptive::clear(&mut p, &region, op, &levels(phi.z, floor, op.stepdown), shi.z)?;
      }
    }
    OpKind::Parallel { faces, angle } => surface::parallel(&mut p, setup, model, op, faces, *angle)?,
    OpKind::Scallop { faces } => surface::scallop(&mut p, setup, model, op, faces)?,
    OpKind::Adaptive3d { rest } => {
      let left = if *rest { Some(rest::remaining(setup, model, previous, &op.tool)?) } else { None };
      adaptive3d::rough(&mut p, setup, model, op, left.as_ref())?
    }
    OpKind::Pencil { faces, passes } => finish3d::pencil(&mut p, setup, model, op, faces, *passes)?,
    OpKind::SteepShallow { faces, threshold } => finish3d::steep_shallow(&mut p, setup, model, op, faces, *threshold)?,
    OpKind::Contour3d { faces, min_slope } => finish3d::contour(&mut p, setup, model, op, faces, *min_slope)?,
    OpKind::Rotary { .. } | OpKind::Swarf { .. } => return Err("this operation needs a 4/5-axis machine".into()),
    OpKind::TurnFace
    | OpKind::TurnRough
    | OpKind::TurnFinish
    | OpKind::Groove
    | OpKind::PartOff
    | OpKind::Thread { .. } => {
      return Err("a lathe operation needs a turning setup".into());
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
      // as the canned cycles run them (G98): over the hole at the clearance height, down to
      // the retract plane, pecks from there, and back up
      for (top, depth) in &ordered {
        let r = top.z + RETRACT;
        p.rapid(DVec3::new(top.x, top.y, safe));
        p.rapid(DVec3::new(top.x, top.y, r));
        let bottom = top.z - depth;
        if *peck > 0.0 {
          let mut z = r;
          while z > bottom + 1e-9 {
            z = (z - peck).max(bottom);
            p.moves.push(Move { rapid: false, to: DVec3::new(top.x, top.y, z), feed: op.tool.plunge });
            p.rapid(DVec3::new(top.x, top.y, r));
          }
        } else {
          p.moves.push(Move { rapid: false, to: DVec3::new(top.x, top.y, bottom), feed: op.tool.plunge });
          p.rapid(DVec3::new(top.x, top.y, r));
        }
        p.rapid(DVec3::new(top.x, top.y, safe));
      }
      drill = Some((ordered, *peck));
    }
  }
  Ok(Toolpath {
    name: op.name.clone(),
    tool: op.tool.clone(),
    moves: p.moves,
    drill,
    axes: Vec::new(),
    turn: Vec::new(),
  })
}

pub(crate) struct Path {
  moves: Vec<Move>,
  feed: f64,
  plunge: f64,
  safe: f64,
  /// Top of the material at the level being cut: entries rapid down to a retract above it.
  top: f64,
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
    self.rapid(DVec3::new(at.x, at.y, (self.top.max(at.z) + RETRACT).min(self.safe)));
    self.moves.push(Move { rapid: false, to: at, feed: self.plunge });
  }

  fn cut(&mut self, to: DVec3) {
    self.moves.push(Move { rapid: false, to, feed: self.feed });
  }

  fn leave(&mut self) {
    if self.moves.last().is_some_and(|m| m.to.z < self.safe - 1e-9) {
      self.rapid_up();
    }
  }

  /// Around a closed loop from its first point (where the tool is) back to it.
  fn follow(&mut self, lp: &[DVec3]) {
    for q in &lp[1..] {
      self.cut(*q);
    }
    self.cut(lp[0]);
  }

  /// A closed loop (each point at its own depth), entering from `top`, the top of the material
  /// there: straight down, or ramping down along the loop at `slope` (dz per unit length) and
  /// then going once more round at full depth.
  fn cut_loop(&mut self, lp: &[DVec3], top: f64, entry: Entry, slope: f64) {
    let n = lp.len();
    let length: f64 = (0..n).map(|i| (lp[(i + 1) % n] - lp[i]).truncate().length()).sum();
    let low = lp.iter().map(|q| q.z).fold(f64::INFINITY, f64::min);
    let ramp = (top - low) / slope;
    if entry == Entry::Plunge
      || n < 2
      || length < 1e-6
      || slope <= 0.0
      || top <= low + 1e-9
      || ramp / length > MAX_PASSES as f64
    {
      self.enter(lp[0]);
      self.follow(lp);
      return;
    }
    // the ramp line z = top - s·slope, never below the loop's own depth
    let line = |s: f64| top - s * slope;
    self.enter(DVec3::new(lp[0].x, lp[0].y, top.max(lp[0].z)));
    let total = ramp + length;
    let mut s = 0.0;
    let mut i = 0;
    while s < total - 1e-9 {
      let (a, b) = (lp[i % n], lp[(i + 1) % n]);
      let seg = (b - a).truncate().length();
      let end = (s + seg).min(total);
      let tend = if seg > 1e-12 { ((end - s) / seg).min(1.0) } else { 1.0 };
      let za = |t: f64| a.z + (b.z - a.z) * t;
      // where the ramp line meets the loop's depth inside this segment
      if seg > 1e-12 {
        let f = |t: f64| line(s + t * seg) - za(t);
        let (f0, f1) = (f(0.0), f(tend));
        if f0 > 1e-9 && f1 < -1e-9 {
          let t = f0 / (f0 - f1) * tend;
          let q = a.lerp(b, t);
          self.cut(DVec3::new(q.x, q.y, za(t)));
        }
      }
      let q = a.lerp(b, tend);
      self.cut(DVec3::new(q.x, q.y, za(tend).max(line(end))));
      s = end;
      i += 1;
    }
  }

  /// A helix down from `top` to `z` around `c` (counter-clockwise), then once round at `z`;
  /// ends at `c` + (`rho`, 0).
  fn helix(&mut self, c: DVec3, rho: f64, top: f64, z: f64, slope: f64) {
    let start = DVec3::new(c.x + rho, c.y, top.max(z));
    self.enter(start);
    // the slope along the chords actually cut
    let per_turn = 72.0 * rho * (std::f64::consts::PI / 36.0).sin() * slope;
    let turns = ((top - z).max(0.0) / per_turn.max(1e-9)).min(MAX_PASSES as f64 / 36.0);
    let steps = ((turns * 36.0).ceil() as usize).max(1);
    let total = turns * std::f64::consts::TAU;
    for k in 1..=steps {
      let t = k as f64 / steps as f64;
      let a = total * t;
      self.cut(DVec3::new(c.x + rho * a.cos(), c.y + rho * a.sin(), top + (z - top) * t));
    }
    for k in 1..=36 {
      let a = total + std::f64::consts::TAU * k as f64 / 36.0;
      self.cut(DVec3::new(c.x + rho * a.cos(), c.y + rho * a.sin(), z));
    }
    self.cut(DVec3::new(c.x + rho, c.y, z));
  }
}

/// The index of a loop's point nearest to `p` (in XY).
fn nearest(lp: &[DVec3], p: DVec3) -> usize {
  lp.iter()
    .enumerate()
    .min_by(|a, b| (a.1.truncate() - p.truncate()).length().total_cmp(&(b.1.truncate() - p.truncate()).length()))
    .map(|(i, _)| i)
    .unwrap_or(0)
}

/// A closed loop starting at its `k`th point.
fn rotated(lp: &[DVec3], k: usize) -> Vec<DVec3> {
  lp[k..].iter().chain(&lp[..k]).copied().collect()
}

/// Whether a straight move keeps the tool centre `wall` or more inside a region.
fn clear_of_walls(region: &region::Region, a: DVec3, b: DVec3, wall: f64) -> bool {
  let len = (b - a).truncate().length();
  let n = ((len / 0.05).ceil() as usize).clamp(1, 2000);
  (0..=n).all(|k| region.sdf(a.lerp(b, k as f64 / n as f64).truncate(), wall + 1.0) >= wall - 1e-6)
}

/// A helix entry for a pocket's first ring: the centre (the pocket's widest point), radius and
/// the ring point to go to from its end, if the helix fits and the way there is clear.
fn helix_at(region: &region::Region, op: &Operation, wall: f64, lp: &[DVec3]) -> Option<(DVec3, f64, usize)> {
  let want = if op.helix_diameter > 0.0 { op.helix_diameter / 2.0 } else { op.tool.radius() };
  let h = ((region.hi - region.lo).max_element() / 200.0).max(0.02);
  let (c, depth) = region.deepest(h, wall + want + 0.1)?;
  let rho = want.min(depth - wall - 0.01);
  if rho < 0.1 * op.tool.radius() {
    return None;
  }
  let c = DVec3::new(c.x, c.y, lp[0].z);
  let end = c + DVec3::new(rho, 0.0, 0.0);
  let k = nearest(lp, end);
  clear_of_walls(region, end, lp[k], wall).then_some((c, rho, k))
}

/// A closed loop at depth `z` with bridges up to `ztab` for holding tabs, evenly spaced along
/// it; each bridge spans the tab's width plus the tool's diameter (the tool's centre path).
fn tabbed(lp: &[DVec3], z: f64, ztab: f64, tabs: Tabs, r: f64) -> Result<Vec<DVec3>, String> {
  let n = lp.len();
  let mut at = vec![0.0];
  for i in 0..n {
    at.push(at[i] + (lp[(i + 1) % n] - lp[i]).truncate().length());
  }
  let length = at[n];
  let half = tabs.width / 2.0 + r;
  let count = tabs.count as f64;
  if 2.0 * half * count >= length {
    return Err("the tabs don't fit along the contour".into());
  }
  let spans: Vec<(f64, f64)> =
    (0..tabs.count).map(|k| (k as f64 + 0.5) * length / count).map(|c| (c - half, c + half)).collect();
  let point = |s: f64| -> DVec3 {
    let i = at.partition_point(|v| *v <= s).saturating_sub(1).min(n - 1);
    let seg = at[i + 1] - at[i];
    let t = if seg > 1e-12 { (s - at[i]) / seg } else { 0.0 };
    lp[i].lerp(lp[(i + 1) % n], t)
  };
  let inside = |s: f64| spans.iter().any(|(a, b)| s > *a && s < *b);
  // vertices and tab ends in order along the loop
  let mut events: Vec<(f64, u8)> = at[..n].iter().map(|s| (*s, 0)).collect();
  for (a, b) in &spans {
    events.push((*a, 1));
    events.push((*b, 2));
  }
  events.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
  let mut out = Vec::new();
  for (s, kind) in events {
    let q = point(s);
    match kind {
      1 => {
        out.push(at_z(q, z));
        out.push(at_z(q, ztab));
      }
      2 => {
        out.push(at_z(q, ztab));
        out.push(at_z(q, z));
      }
      _ => out.push(at_z(q, if inside(s) { ztab } else { z })),
    }
  }
  Ok(out)
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

/// G-code for a setup's toolpaths with its post-processor (Fanuc unless set) and units,
/// relative to its work origin: rotary axes for a 4/5-axis machine, a lathe program for a lathe.
pub fn gcode(setup: &Setup, paths: &[Toolpath], origin: DVec3) -> Result<String, String> {
  let imperial = setup.imperial;
  if setup.lathe.is_some() {
    let post = turn::find_post(&setup.post).ok_or_else(|| format!("'{}' isn't a lathe post", setup.post))?;
    return Ok(turn::write(post, imperial, &setup.name, paths));
  }
  let post = post::find(&setup.post).unwrap_or(&post::POSTS[0]);
  let units = if imperial { post::Units::Imperial } else { post::Units::Metric };
  let arcs = (!setup.straight).then_some(setup.arc_tolerance);
  if let Some(m) = &setup.machine {
    if !post.rotary {
      return Err(format!("the {} post has no rotary axes", post.name));
    }
    return post::write_machine(post, units, &setup.name, paths, origin, arcs, Some(m));
  }
  if paths.iter().any(|t| !t.axes.is_empty()) {
    return Err("tilted toolpaths need a 4/5-axis machine".into());
  }
  Ok(post::write_with(post, units, &setup.name, paths, origin, arcs))
}

/// The stock after a setup's toolpaths: a heightfield for 3-axis milling, voxels for 4/5 axes,
/// a turned profile on a lathe.
#[derive(Debug, Clone)]
pub enum Simulated {
  Mill(sim::Simulation),
  Voxels(voxel::VoxelSim),
  Turned(turn::TurnSim),
}

impl Simulated {
  /// The heightfield of a 3-axis milling setup's simulation.
  pub fn mill(&self) -> Option<&sim::Simulation> {
    match self {
      Simulated::Mill(s) => Some(s),
      _ => None,
    }
  }

  pub fn warnings(&self) -> &[sim::Warning] {
    match self {
      Simulated::Mill(s) => &s.warnings,
      Simulated::Voxels(s) => &s.warnings,
      Simulated::Turned(s) => &s.warnings,
    }
  }

  /// Removed volume per operation (mm³).
  pub fn removed(&self) -> &[(String, f64)] {
    match self {
      Simulated::Mill(s) => &s.removed,
      Simulated::Voxels(s) => &s.removed,
      Simulated::Turned(s) => &s.removed,
    }
  }

  /// Stock left on the part (mm³).
  pub fn remaining(&self) -> f64 {
    match self {
      Simulated::Mill(s) => s.remaining(),
      Simulated::Voxels(s) => s.remaining(),
      Simulated::Turned(s) => s.remaining(),
    }
  }

  /// The simulation's resolution (mm).
  pub fn cell(&self) -> f64 {
    match self {
      Simulated::Mill(s) => s.stock.h,
      Simulated::Voxels(s) => s.h,
      Simulated::Turned(s) => s.h,
    }
  }

  /// A mesh of the stock for display, at most about `max` cells a side.
  pub fn mesh(&self, max: usize) -> sim::SimMesh {
    match self {
      Simulated::Mill(s) => s.mesh(max),
      Simulated::Voxels(s) => s.mesh(max),
      Simulated::Turned(s) => s.mesh(),
    }
  }
}

/// Runs a setup's toolpaths through its stock (see [`Simulated`]).
pub fn simulate(setup: &Setup, model: &Model, paths: &[Toolpath]) -> Result<Simulated, String> {
  if let Some(l) = &setup.lathe {
    return turn::simulate(l, setup, model, paths).map(Simulated::Turned);
  }
  if setup.machine.is_some() || paths.iter().any(|t| !t.axes.is_empty()) {
    return voxel::simulate(setup, model, paths).map(Simulated::Voxels);
  }
  sim::simulate(setup, model, paths).map(Simulated::Mill)
}
