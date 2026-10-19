//! 2D drawings of the design: projected views laid out on a sheet with dimensions, notes and a
//! title block, as in Fusion's Drawing workspace.
//!
//! A [`Drawing`] is stored in the design (so it saves, undoes and follows parameter changes);
//! [`render`] turns it into a [`Plot`] — pen strokes and text in sheet millimetres — which
//! [`crate::plot`] writes as SVG, PDF or DXF. Views are exact hidden-line projections from the
//! kernel, so they always match the model.

use clf_kernel::glam::DVec3;
use clf_kernel::{BoolOp, LineKind, Projection, Shape, SurfaceKind};
use serde::{Deserialize, Serialize};

use crate::plot::{Anchor, Item, Pen, Plot};
use crate::regen::Model;
use crate::{BodyId, ComponentId, Design};

mod annotate;
mod breaks;

pub use annotate::{Characteristic, Datums, FinishProcess, MaterialCondition, Standard};
pub use breaks::{BreakDirection, DEFAULT_GAP, ViewBreak};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
  pub id: u32,
  pub name: String,
  #[serde(default)]
  pub sheet: SheetSize,
  /// Sheet mm per model mm; `None` = the largest standard scale that fits.
  #[serde(default)]
  pub scale: Option<f64>,
  #[serde(default)]
  pub projection: ProjectionAngle,
  /// Bodies drawn; empty = every visible body.
  #[serde(default)]
  pub bodies: Vec<BodyId>,
  pub views: Vec<DrawingView>,
  #[serde(default)]
  pub dimensions: Vec<DrawingDim>,
  /// Overall sizes and hole diameters added automatically.
  #[serde(default = "yes")]
  pub auto_dimensions: bool,
  #[serde(default)]
  pub title: TitleBlock,
  /// A parts list table (when the design has components).
  #[serde(default = "yes")]
  pub parts_list: bool,
  /// Drafting standard of the annotations (GD&T, surface finish, decimal sign).
  #[serde(default)]
  pub standard: Standard,
  /// A balloon for every parts list row that has none, kept in step with the parts list.
  #[serde(default)]
  pub auto_balloons: bool,
}

fn yes() -> bool {
  true
}

/// Landscape sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SheetSize {
  A4,
  #[default]
  A3,
  A2,
  A1,
  A0,
  Letter,
  Tabloid,
}

impl SheetSize {
  /// Width and height in mm.
  pub fn size(self) -> (f64, f64) {
    match self {
      SheetSize::A4 => (297.0, 210.0),
      SheetSize::A3 => (420.0, 297.0),
      SheetSize::A2 => (594.0, 420.0),
      SheetSize::A1 => (841.0, 594.0),
      SheetSize::A0 => (1189.0, 841.0),
      SheetSize::Letter => (279.4, 215.9),
      SheetSize::Tabloid => (431.8, 279.4),
    }
  }

  pub fn parse(s: &str) -> Option<SheetSize> {
    Some(match s.to_ascii_lowercase().as_str() {
      "a4" => SheetSize::A4,
      "a3" => SheetSize::A3,
      "a2" => SheetSize::A2,
      "a1" => SheetSize::A1,
      "a0" => SheetSize::A0,
      "letter" => SheetSize::Letter,
      "tabloid" | "b" => SheetSize::Tabloid,
      _ => return None,
    })
  }
}

/// Where the top and side views go relative to the front view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionAngle {
  /// ASME: the top view above the front view, the right view to its right.
  #[default]
  ThirdAngle,
  /// ISO: the top view below the front view, the right view to its left.
  FirstAngle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingView {
  pub id: u32,
  pub kind: ViewKind,
  /// Centre on the sheet (mm); `None` = placed automatically.
  #[serde(default)]
  pub at: Option<[f64; 2]>,
  #[serde(default = "yes")]
  pub hidden_lines: bool,
  /// Stretches left out of the view (a broken view).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub breaks: Vec<ViewBreak>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
  X,
  Y,
  Z,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewKind {
  Front,
  Back,
  Top,
  Bottom,
  Left,
  Right,
  Iso,
  /// The part cut by a plane across `axis` at `offset`, seen looking at the cut.
  Section {
    axis: Axis,
    offset: f64,
    #[serde(default)]
    flip: bool,
  },
  /// A sheet metal body's flat pattern with its bend lines.
  Flat {
    body: BodyId,
  },
  /// An enlarged circle of another view around a model point.
  Detail {
    parent: u32,
    center: [f64; 3],
    /// Model mm.
    radius: f64,
    /// Enlargement over the drawing's scale.
    #[serde(default = "two")]
    factor: f64,
  },
}

fn two() -> f64 {
  2.0
}

impl ViewKind {
  pub fn parse(s: &str) -> Option<ViewKind> {
    Some(match s.to_ascii_lowercase().as_str() {
      "front" => ViewKind::Front,
      "back" => ViewKind::Back,
      "top" => ViewKind::Top,
      "bottom" => ViewKind::Bottom,
      "left" => ViewKind::Left,
      "right" => ViewKind::Right,
      "iso" | "isometric" => ViewKind::Iso,
      _ => return None,
    })
  }

  /// Direction towards the viewer and the sheet's right in world space.
  fn basis(&self) -> (DVec3, DVec3) {
    let iso = (DVec3::new(1.0, -1.0, 1.0).normalize(), DVec3::new(1.0, 1.0, 0.0).normalize());
    match self {
      ViewKind::Front => (-DVec3::Y, DVec3::X),
      ViewKind::Back => (DVec3::Y, -DVec3::X),
      ViewKind::Top => (DVec3::Z, DVec3::X),
      ViewKind::Bottom => (-DVec3::Z, DVec3::X),
      ViewKind::Right => (DVec3::X, DVec3::Y),
      ViewKind::Left => (-DVec3::X, -DVec3::Y),
      ViewKind::Iso | ViewKind::Flat { .. } | ViewKind::Detail { .. } => iso,
      ViewKind::Section { axis, flip, .. } => {
        let (z, x) = match axis {
          Axis::X => (DVec3::X, DVec3::Y),
          Axis::Y => (-DVec3::Y, DVec3::X),
          Axis::Z => (DVec3::Z, DVec3::X),
        };
        if *flip { (-z, -x) } else { (z, x) }
      }
    }
  }

  fn label(&self, n: usize) -> String {
    match self {
      ViewKind::Section { .. } => {
        let c = char::from(b'A' + (n % 26) as u8);
        format!("SECTION {c}-{c}")
      }
      ViewKind::Flat { .. } => "FLAT PATTERN".into(),
      ViewKind::Detail { factor, .. } => {
        let c = char::from(b'A' + (n % 26) as u8);
        format!("DETAIL {c} ({})", scale_text(*factor))
      }
      ViewKind::Iso => "ISOMETRIC".into(),
      k => format!("{k:?}").to_uppercase(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DimOrientation {
  #[default]
  Aligned,
  Horizontal,
  Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DrawingDim {
  /// Distance between two model points, measured in a view.
  Distance {
    view: u32,
    a: [f64; 3],
    b: [f64; 3],
    #[serde(default)]
    orientation: DimOrientation,
    /// Sheet mm between the measured points and the dimension line.
    #[serde(default = "dim_offset")]
    offset: f64,
  },
  /// Free text on the sheet.
  Note { at: [f64; 2], text: String },
  /// Ordinate dimensions: distances of points from `origin` along the view's horizontal or
  /// vertical.
  Ordinate {
    view: u32,
    origin: [f64; 3],
    points: Vec<[f64; 3]>,
    #[serde(default = "horizontal")]
    orientation: DimOrientation,
    /// Sheet mm past the farthest point to the values.
    #[serde(default = "dim_offset")]
    offset: f64,
  },
  /// The angle at `vertex` between the edges to `a` and `b`.
  Angle {
    view: u32,
    vertex: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    /// Arc radius, sheet mm.
    #[serde(default = "arc_radius")]
    radius: f64,
  },
  /// A GD&T feature control frame on a leader to a model point; `offset` (sheet mm) places
  /// the frame relative to the point.
  FeatureControl {
    view: u32,
    point: [f64; 3],
    #[serde(default = "leader_offset")]
    offset: [f64; 2],
    characteristic: Characteristic,
    tolerance: f64,
    /// A cylindrical (Ø) tolerance zone.
    #[serde(default)]
    diameter: bool,
    #[serde(default)]
    modifier: Option<MaterialCondition>,
    /// Primary, secondary and tertiary datum references.
    #[serde(default)]
    datums: Vec<String>,
  },
  /// A datum feature identifier on a model point.
  Datum {
    view: u32,
    point: [f64; 3],
    #[serde(default = "leader_offset")]
    offset: [f64; 2],
    label: String,
  },
  /// A surface texture symbol (Ra in µm) on a model point.
  SurfaceFinish {
    view: u32,
    point: [f64; 3],
    #[serde(default)]
    offset: [f64; 2],
    roughness: f64,
    #[serde(default)]
    process: FinishProcess,
  },
  /// A balloon with the parts list item number of a component.
  Balloon {
    view: u32,
    point: [f64; 3],
    #[serde(default = "leader_offset")]
    offset: [f64; 2],
    component: ComponentId,
  },
}

impl DrawingDim {
  /// The view a dimension or annotation hangs off (notes hang off the sheet).
  pub fn view(&self) -> Option<u32> {
    match self {
      DrawingDim::Note { .. } => None,
      DrawingDim::Distance { view, .. }
      | DrawingDim::Ordinate { view, .. }
      | DrawingDim::Angle { view, .. }
      | DrawingDim::FeatureControl { view, .. }
      | DrawingDim::Datum { view, .. }
      | DrawingDim::SurfaceFinish { view, .. }
      | DrawingDim::Balloon { view, .. } => Some(*view),
    }
  }
}

fn dim_offset() -> f64 {
  10.0
}

fn horizontal() -> DimOrientation {
  DimOrientation::Horizontal
}

fn arc_radius() -> f64 {
  15.0
}

fn leader_offset() -> [f64; 2] {
  [12.0, 12.0]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TitleBlock {
  #[serde(default)]
  pub title: String,
  #[serde(default)]
  pub author: String,
  #[serde(default)]
  pub part_number: String,
  #[serde(default)]
  pub material: String,
  #[serde(default)]
  pub revision: String,
  #[serde(default)]
  pub company: String,
}

impl Drawing {
  pub fn new(id: u32, name: impl Into<String>, kinds: Vec<ViewKind>) -> Drawing {
    Drawing {
      id,
      name: name.into(),
      sheet: SheetSize::default(),
      scale: None,
      projection: ProjectionAngle::default(),
      bodies: Vec::new(),
      views: kinds.into_iter().enumerate().map(|(i, kind)| DrawingView::new(i as u32 + 1, kind)).collect(),
      dimensions: Vec::new(),
      auto_dimensions: true,
      title: TitleBlock::default(),
      parts_list: true,
      standard: Standard::default(),
      auto_balloons: false,
    }
  }

  pub fn add_view(&mut self, kind: ViewKind, at: Option<[f64; 2]>) -> u32 {
    let id = self.views.iter().map(|v| v.id).max().unwrap_or(0) + 1;
    self.views.push(DrawingView { at, ..DrawingView::new(id, kind) });
    id
  }
}

impl DrawingView {
  pub fn new(id: u32, kind: ViewKind) -> DrawingView {
    let hidden_lines = !matches!(kind, ViewKind::Iso);
    DrawingView { id, kind, at: None, hidden_lines, breaks: Vec::new() }
  }
}

// ---------------------------------------------------------------- layout

const MARGIN: f64 = 10.0;
const TITLE_H: f64 = 36.0;
const TITLE_W: f64 = 180.0;
const GAP: f64 = 25.0;
const TEXT: f64 = 3.5;
const SCALES: &[f64] = &[20.0, 10.0, 5.0, 4.0, 2.0, 1.0, 0.5, 0.4, 0.25, 0.2, 0.1, 0.05, 0.04, 0.02, 0.01, 0.005];

/// A view projected, in model mm (view coordinates).
struct Projected {
  view: DrawingView,
  proj: Projection,
  lo: [f64; 2],
  hi: [f64; 2],
  right: DVec3,
  up: DVec3,
  /// Cut faces to hatch (segments in view coordinates).
  hatch: Vec<Vec<[[f64; 2]; 2]>>,
  /// Bend lines of a flat pattern, with their notes.
  bends: Vec<([[f64; 2]; 2], String)>,
  /// Enlargement over the drawing scale (detail views).
  mul: f64,
  /// Broken-out stretches along the view's horizontal and vertical.
  cuts: [Vec<breaks::Cut>; 2],
}

impl Projected {
  /// Size at the drawing scale's units (model mm, times any enlargement), broken-out stretches
  /// left out.
  fn size(&self) -> [f64; 2] {
    [0, 1].map(|i| (self.hi[i] - self.lo[i] - breaks::removed(&self.cuts[i])) * self.mul)
  }

  /// The part's true size in the view (model mm), what dimensions read.
  fn true_size(&self) -> [f64; 2] {
    [self.hi[0] - self.lo[0], self.hi[1] - self.lo[1]]
  }

  /// Sheet mm the view takes at `scale`, break gaps included.
  fn extent(&self, scale: f64) -> [f64; 2] {
    [0, 1].map(|i| breaks::fold(&self.cuts[i], self.lo[i], self.hi[i], scale * self.mul))
  }

  /// Is a circle clear of the broken-out stretches?
  fn shows_circle(&self, c: &clf_kernel::ProjCircle) -> bool {
    (0..2).all(|i| {
      self.cuts[i].iter().all(|k| c.center[i] + c.radius <= k.from + 1e-9 || c.center[i] - c.radius >= k.to - 1e-9)
    })
  }

  fn to_view(&self, p: DVec3) -> [f64; 2] {
    [p.dot(self.right), p.dot(self.up)]
  }
}

/// Lays the drawing out on its sheet.
pub fn render(d: &Drawing, design: &Design, model: &Model) -> Result<Plot, String> {
  let bodies: Vec<&crate::regen::Body> = model
    .bodies
    .iter()
    .filter(|b| {
      if d.bodies.is_empty() { design.body_settings(b.id).is_none_or(|s| s.visible) } else { d.bodies.contains(&b.id) }
    })
    .collect();
  if bodies.is_empty() {
    return Err("there are no bodies to draw".into());
  }
  let part = Shape::compound(bodies.iter().map(|b| &b.shape)).map_err(|e| e.to_string())?;
  let diag = part.mass().map(|m| (m.bbox_max - m.bbox_min).length()).unwrap_or(100.0).max(1.0);
  let defl = (diag * 5e-4).clamp(1e-3, 0.5);

  let mut views = Vec::new();
  for v in &d.views {
    if let ViewKind::Detail { parent, center, radius, factor } = &v.kind {
      let pv = d.views.iter().find(|x| x.id == *parent && !matches!(x.kind, ViewKind::Detail { .. }));
      let pv = pv.ok_or(format!("detail view {}: its parent view is gone", v.id))?;
      let base = project_view(pv, &part, model, defl)?;
      views.push(detail(v, base, DVec3::from(*center), *radius, *factor)?);
    } else {
      views.push(project_view(v, &part, model, defl)?);
    }
  }
  for pv in &mut views {
    pv.cuts = breaks::cuts(&pv.view.breaks, pv.lo, pv.hi);
  }
  let (w, h) = d.sheet.size();
  let scale = match d.scale {
    Some(s) if s > 0.0 && s.is_finite() => s,
    _ => auto_scale(&views, d.projection, w, h),
  };
  place(&mut views, d.projection, scale, w, h);

  let mut plot = Plot { width: w, height: h, items: Vec::new() };
  for (n, pv) in views.iter().enumerate() {
    draw_view(&mut plot, pv, scale, n);
  }
  for (n, v) in views.iter().enumerate() {
    if let ViewKind::Detail { parent, center, radius, .. } = &v.view.kind
      && let Some(pv) = views.iter().find(|x| x.view.id == *parent)
    {
      let c = sheet_pt(pv, scale, pv.to_view(DVec3::from(*center)));
      let r = radius * scale * pv.mul;
      let ring: Vec<[f64; 2]> = (0..=72)
        .map(|i| {
          let t = std::f64::consts::TAU * i as f64 / 72.0;
          [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect();
      plot.items.push(Item::Line { pen: Pen::Thin, points: ring });
      let letter = char::from(b'A' + (n % 26) as u8).to_string();
      plot.items.push(Item::Text {
        at: [c[0] + r * 0.8, c[1] + r * 0.8],
        size: TEXT,
        text: letter,
        anchor: Anchor::Start,
      });
    }
  }
  if d.auto_dimensions {
    auto_dims(&mut plot, &views, scale, d.standard);
  }
  let rows = bom_rows(design);
  if d.parts_list && !design.components.is_empty() {
    parts_list(&mut plot, &rows);
  }
  let std = d.standard;
  for dim in &d.dimensions {
    let pv = dim.view().and_then(|id| views.iter().find(|v| v.view.id == id));
    let anchor = |p: &[f64; 3]| pv.map(|pv| sheet_pt(pv, scale, pv.to_view(DVec3::from(*p)))).unwrap_or_default();
    match dim {
      DrawingDim::Distance { a, b, orientation, offset, .. } => {
        let Some(pv) = pv else { continue };
        let (va, vb) = (pv.to_view(DVec3::from(*a)), pv.to_view(DVec3::from(*b)));
        // true lengths, whatever breaks the view has
        let value = match orientation {
          DimOrientation::Aligned => (DVec3::from(*a) - DVec3::from(*b)).length(),
          DimOrientation::Horizontal => (va[0] - vb[0]).abs(),
          DimOrientation::Vertical => (va[1] - vb[1]).abs(),
        };
        linear_dim(&mut plot, anchor(a), anchor(b), *orientation, *offset, &std.number(value));
      }
      DrawingDim::Note { at, text } => {
        plot.items.push(Item::Text { at: *at, size: TEXT, text: text.clone(), anchor: Anchor::Start })
      }
      DrawingDim::Ordinate { origin, points, orientation, offset, .. } => {
        let Some(pv) = pv else { continue };
        annotate::ordinate(&mut plot, std, pv, scale, *origin, points, *orientation, *offset);
      }
      DrawingDim::Angle { vertex, a, b, radius, .. } => {
        let Some(pv) = pv else { continue };
        annotate::angle(&mut plot, std, pv, scale, *vertex, *a, *b, *radius);
      }
      DrawingDim::FeatureControl { point, offset, characteristic, tolerance, diameter, modifier, datums, .. } => {
        if pv.is_some() {
          let (ch, tol) = (*characteristic, *tolerance);
          annotate::feature_control(&mut plot, std, anchor(point), *offset, ch, tol, *diameter, *modifier, datums);
        }
      }
      DrawingDim::Datum { point, offset, label, .. } => {
        if pv.is_some() {
          annotate::datum(&mut plot, anchor(point), *offset, label);
        }
      }
      DrawingDim::SurfaceFinish { point, offset, roughness, process, .. } => {
        if pv.is_some() {
          annotate::surface_finish(&mut plot, std, anchor(point), *offset, *roughness, *process);
        }
      }
      DrawingDim::Balloon { point, offset, component, .. } => {
        // the item number comes from the parts list as it is now
        let item = rows.iter().position(|r| r.components.contains(component));
        if let (Some(_), Some(n)) = (pv, item) {
          annotate::balloon(&mut plot, anchor(point), *offset, &(n + 1).to_string());
        }
      }
    }
  }
  if d.auto_balloons {
    auto_balloons(&mut plot, d, &views, scale, &rows, design, &bodies);
  }
  frame(&mut plot, d, design, scale);
  Ok(plot)
}

fn project_view(v: &DrawingView, part: &Shape, model: &Model, defl: f64) -> Result<Projected, String> {
  let (z, right) = v.kind.basis();
  let up = z.cross(right);
  let mut hatch = Vec::new();
  let mut bends = Vec::new();
  let mut shape = part.clone();
  let (mut z, mut right, mut up) = (z, right, up);
  match &v.kind {
    ViewKind::Section { axis, offset, .. } => {
      let n = match axis {
        Axis::X => DVec3::X,
        Axis::Y => DVec3::Y,
        Axis::Z => DVec3::Z,
      };
      let toward = n * n.dot(z).signum();
      let origin = n * *offset;
      // keep the material behind the cut (away from the viewer)
      let size = 1e5;
      let keep = crate::sheet::half_space(origin, -toward, size)?;
      shape = part.boolean(&keep, BoolOp::Intersect).map_err(|e| format!("the section plane misses the part: {e}"))?;
      // the cut faces, for hatching
      let edges = shape.edges().map_err(|e| e.to_string())?;
      let lines = shape.edge_lines(defl).map_err(|e| e.to_string())?;
      for f in shape.faces().map_err(|e| e.to_string())? {
        if f.kind != SurfaceKind::Plane || f.normal.dot(toward) < 0.9999 || (f.origin - origin).dot(n).abs() > 1e-6 {
          continue;
        }
        let mut segs = Vec::new();
        for e in edges.iter().filter(|e| e.faces.contains(&f.id)) {
          if let Some(l) = lines.iter().find(|l| l.id == e.id) {
            for w in l.points.windows(2) {
              segs.push([[w[0].dot(right), w[0].dot(up)], [w[1].dot(right), w[1].dot(up)]]);
            }
          }
        }
        hatch.push(segs);
      }
    }
    ViewKind::Flat { body } => {
      let b = model.body(*body).ok_or(format!("body {} no longer exists", body.0))?;
      let flat = if b.sheet.as_ref().is_some_and(|s| s.flat) {
        let face = crate::sheet::default_face(b.faces()).ok_or("the flat body has no planar face")?;
        let info = b.faces().iter().find(|f| f.id == face).ok_or("no face")?;
        crate::sheet::FlatPattern {
          shape: b.shape.clone(),
          bends: Vec::new(),
          plane: info.plane().ok_or("the flat body has no planar face")?,
        }
      } else {
        b.flat_pattern(None)?
      };
      z = flat.plane.normal();
      right = flat.plane.x;
      up = flat.plane.y;
      shape = flat.shape.clone();
      for bd in &flat.bends {
        let seg = [[bd.start.dot(right), bd.start.dot(up)], [bd.end.dot(right), bd.end.dot(up)]];
        let dir = if bd.inner { "UP" } else { "DOWN" };
        bends.push((seg, format!("{dir} {:.0}°", bd.angle.to_degrees())));
      }
    }
    _ => {}
  }
  let proj = shape.project(z, right, defl).map_err(|e| e.to_string())?;
  let (lo, hi) = proj.bounds().ok_or("the view is empty")?;
  Ok(Projected { view: v.clone(), proj, lo, hi, right, up, hatch, bends, mul: 1.0, cuts: [Vec::new(), Vec::new()] })
}

/// A detail view: the parent's projection clipped to a circle around `center` and enlarged.
fn detail(v: &DrawingView, mut base: Projected, center: DVec3, radius: f64, factor: f64) -> Result<Projected, String> {
  if !(radius > 0.0 && factor > 0.0) {
    return Err("a detail view needs a positive radius and enlargement".into());
  }
  let c = base.to_view(center);
  let inside = |p: [f64; 2]| (p[0] - c[0]).hypot(p[1] - c[1]) <= radius;
  let mut lines = Vec::new();
  for l in &base.proj.lines {
    for piece in clip_circle(&l.points, c, radius) {
      lines.push(clf_kernel::ProjLine { kind: l.kind, points: piece });
    }
  }
  base.proj.circles.retain(|k| inside(k.center) && k.radius < radius);
  base.proj.lines = lines;
  // the boundary circle
  let ring: Vec<[f64; 2]> = (0..=96)
    .map(|i| {
      let t = std::f64::consts::TAU * i as f64 / 96.0;
      [c[0] + radius * t.cos(), c[1] + radius * t.sin()]
    })
    .collect();
  base.proj.lines.push(clf_kernel::ProjLine { kind: LineKind::Tangent, points: ring });
  base.lo = [c[0] - radius, c[1] - radius];
  base.hi = [c[0] + radius, c[1] + radius];
  base.hatch.clear();
  base.bends.clear();
  base.view = v.clone();
  base.mul = factor;
  Ok(base)
}

/// The parts of a polyline inside a circle.
fn clip_circle(pts: &[[f64; 2]], c: [f64; 2], r: f64) -> Vec<Vec<[f64; 2]>> {
  let inside = |p: [f64; 2]| (p[0] - c[0]).hypot(p[1] - c[1]) <= r;
  let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
  let mut cur: Vec<[f64; 2]> = Vec::new();
  for w in pts.windows(2) {
    let (a, b) = (w[0], w[1]);
    // where the segment crosses the circle (t in 0..1)
    let d = [b[0] - a[0], b[1] - a[1]];
    let f = [a[0] - c[0], a[1] - c[1]];
    let (qa, qb, qc) =
      (d[0] * d[0] + d[1] * d[1], 2.0 * (f[0] * d[0] + f[1] * d[1]), f[0] * f[0] + f[1] * f[1] - r * r);
    let mut ts = vec![0.0];
    let disc = qb * qb - 4.0 * qa * qc;
    if qa > 1e-18 && disc > 0.0 {
      for t in [(-qb - disc.sqrt()) / (2.0 * qa), (-qb + disc.sqrt()) / (2.0 * qa)] {
        if t > 0.0 && t < 1.0 {
          ts.push(t);
        }
      }
    }
    ts.push(1.0);
    for k in 0..ts.len() - 1 {
      let p = |t: f64| [a[0] + d[0] * t, a[1] + d[1] * t];
      let (p0, p1) = (p(ts[k]), p(ts[k + 1]));
      if inside(p((ts[k] + ts[k + 1]) / 2.0)) {
        if cur.last().is_none_or(|q| (q[0] - p0[0]).hypot(q[1] - p0[1]) > 1e-9) {
          if cur.len() > 1 {
            out.push(std::mem::take(&mut cur));
          }
          cur = vec![p0];
        }
        cur.push(p1);
      } else if cur.len() > 1 {
        out.push(std::mem::take(&mut cur));
      } else {
        cur.clear();
      }
    }
  }
  if cur.len() > 1 {
    out.push(cur);
  }
  out
}

/// Grid cell of a standard view: (column, row) around the front view at (0, 0).
fn cell(kind: &ViewKind, angle: ProjectionAngle) -> Option<(i32, i32)> {
  let s = if angle == ProjectionAngle::ThirdAngle { 1 } else { -1 };
  Some(match kind {
    ViewKind::Front => (0, 0),
    ViewKind::Top => (0, s),
    ViewKind::Bottom => (0, -s),
    ViewKind::Right => (s, 0),
    ViewKind::Left => (-s, 0),
    ViewKind::Back => (2 * s, 0),
    ViewKind::Iso => (1, 1),
    _ => return None,
  })
}

/// Column widths / row heights (model mm) of the standard-view grid, by index -2..=2; with a
/// scale, break gaps count too.
fn grid(views: &[Projected], angle: ProjectionAngle, scale: Option<f64>) -> ([f64; 5], [f64; 5]) {
  let (mut cols, mut rows) = ([0.0; 5], [0.0; 5]);
  for v in views {
    if let Some((c, r)) = cell(&v.view.kind, angle) {
      let s = match scale {
        Some(k) => v.extent(k).map(|e| e / k),
        None => v.size(),
      };
      cols[(c + 2) as usize] = f64::max(cols[(c + 2) as usize], s[0]);
      rows[(r + 2) as usize] = f64::max(rows[(r + 2) as usize], s[1]);
    }
  }
  (cols, rows)
}

fn auto_scale(views: &[Projected], angle: ProjectionAngle, w: f64, h: f64) -> f64 {
  let (cols, rows) = grid(views, angle, None);
  let extra: f64 =
    views.iter().filter(|v| cell(&v.view.kind, angle).is_none() && v.view.at.is_none()).map(|v| v.size()[0]).sum();
  let used = |a: &[f64; 5]| a.iter().filter(|x| **x > 0.0).count() as f64;
  let n_extra = views.iter().filter(|v| cell(&v.view.kind, angle).is_none() && v.view.at.is_none()).count() as f64;
  let avail_w = w - 2.0 * MARGIN - GAP * (used(&cols) + n_extra + 1.0);
  let avail_h = h - 2.0 * MARGIN - TITLE_H - GAP * (used(&rows) + 1.0);
  let need_w: f64 = cols.iter().sum::<f64>() + extra;
  let need_h = rows
    .iter()
    .sum::<f64>()
    .max(views.iter().filter(|v| cell(&v.view.kind, angle).is_none()).map(|v| v.size()[1]).fold(0.0, f64::max));
  let fit = (avail_w / need_w.max(1e-9)).min(avail_h / need_h.max(1e-9));
  SCALES.iter().copied().find(|s| *s <= fit).unwrap_or(SCALES[SCALES.len() - 1])
}

/// Puts each view's centre on the sheet.
fn place(views: &mut [Projected], angle: ProjectionAngle, scale: f64, w: f64, h: f64) {
  let (cols, rows) = grid(views, angle, Some(scale));
  let used_cols: Vec<usize> = (0..5).filter(|i| cols[*i] > 0.0).collect();
  let used_rows: Vec<usize> = (0..5).filter(|i| rows[*i] > 0.0).collect();
  let mut col_x = [0.0; 5];
  let mut x = MARGIN + GAP;
  for &c in &used_cols {
    col_x[c] = x + cols[c] * scale / 2.0;
    x += cols[c] * scale + GAP;
  }
  let mut row_y = [0.0; 5];
  let mut y = MARGIN + TITLE_H + GAP;
  for &r in &used_rows {
    row_y[r] = y + rows[r] * scale / 2.0;
    y += rows[r] * scale + GAP;
  }
  let mut next_x = x;
  for v in views.iter_mut() {
    if v.view.at.is_some() {
      continue;
    }
    if let Some((c, r)) = cell(&v.view.kind, angle) {
      v.view.at = Some([col_x[(c + 2) as usize], row_y[(r + 2) as usize]]);
    } else {
      let e = v.extent(scale);
      v.view.at = Some([next_x + e[0] / 2.0, (h - MARGIN - GAP - e[1] / 2.0).max(MARGIN + TITLE_H)]);
      next_x += e[0] + GAP;
    }
  }
  let _ = w;
}

fn sheet_pt(v: &Projected, scale: f64, p: [f64; 2]) -> [f64; 2] {
  let at = v.view.at.unwrap_or([0.0, 0.0]);
  let (k, e) = (scale * v.mul, v.extent(scale));
  [0, 1].map(|i| at[i] - e[i] / 2.0 + breaks::fold(&v.cuts[i], v.lo[i], p[i], k))
}

/// The pieces of a view-coordinate polyline left after the view's breaks.
fn unbroken(v: &Projected, pts: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
  breaks::clip(pts, 0, &v.cuts[0]).iter().flat_map(|p| breaks::clip(p, 1, &v.cuts[1])).collect()
}

fn draw_view(plot: &mut Plot, v: &Projected, scale: f64, n: usize) {
  let broken = v.cuts.iter().any(|c| !c.is_empty());
  for l in &v.proj.lines {
    let pen = match l.kind {
      LineKind::Visible | LineKind::Outline => Pen::Visible,
      LineKind::Tangent => Pen::Thin,
      LineKind::Hidden | LineKind::HiddenOutline => {
        if !v.view.hidden_lines {
          continue;
        }
        Pen::Hidden
      }
    };
    if broken {
      for piece in unbroken(v, &l.points) {
        plot.items.push(Item::Line { pen, points: piece.iter().map(|p| sheet_pt(v, scale, *p)).collect() });
      }
      continue;
    }
    plot.items.push(Item::Line { pen, points: l.points.iter().map(|p| sheet_pt(v, scale, *p)).collect() });
  }
  // centre marks on holes seen end-on
  for c in v.proj.circles.iter().filter(|c| !c.kind.hidden() && v.shows_circle(c)) {
    let o = sheet_pt(v, scale, c.center);
    let r = c.radius * scale + 2.0;
    plot.items.push(Item::Line { pen: Pen::Center, points: vec![[o[0] - r, o[1]], [o[0] + r, o[1]]] });
    plot.items.push(Item::Line { pen: Pen::Center, points: vec![[o[0], o[1] - r], [o[0], o[1] + r]] });
  }
  for faces in &v.hatch {
    // hatched in view coordinates (2.5 mm apart on the sheet), then broken like the edges
    for h in hatch_lines(faces, 2.5 / (scale * v.mul)) {
      for piece in unbroken(v, &h) {
        plot.items.push(Item::Line { pen: Pen::Thin, points: piece.iter().map(|p| sheet_pt(v, scale, *p)).collect() });
      }
    }
  }
  for (seg, note) in &v.bends {
    let Some(piece) = unbroken(v, seg).into_iter().next() else { continue };
    let (a, b) = (sheet_pt(v, scale, piece[0]), sheet_pt(v, scale, piece[piece.len() - 1]));
    plot.items.push(Item::Line { pen: Pen::Center, points: vec![a, b] });
    // the note runs along the bend line, just off it
    let d = [b[0] - a[0], b[1] - a[1]];
    let l = d[0].hypot(d[1]).max(1e-9);
    let angle = upright(d[1].atan2(d[0]).to_degrees());
    let n = [-d[1] / l, d[0] / l];
    let m = [(a[0] + b[0]) / 2.0 + n[0] * 1.5, (a[1] + b[1]) / 2.0 + n[1] * 1.5];
    plot.items.push(Item::RotatedText { at: m, size: 2.5, text: note.clone(), angle });
  }
  let at = v.view.at.unwrap_or([0.0, 0.0]);
  let e = v.extent(scale);
  // break lines at both ends of each gap, across the whole view
  for (i, cuts) in v.cuts.iter().enumerate() {
    let j = 1 - i;
    let (lo, hi) = (at[j] - e[j] / 2.0 - 2.0, at[j] + e[j] / 2.0 + 2.0);
    for c in cuts {
      let start = at[i] - e[i] / 2.0 + breaks::fold(cuts, v.lo[i], c.from, scale * v.mul);
      for pos in [start, start + c.gap] {
        plot.items.push(Item::Line { pen: Pen::Thin, points: breaks::break_line(i, pos, lo, hi) });
      }
    }
  }
  // under the view, clear of the dimensions placed below it
  let below = at[1] - e[1] / 2.0 - 19.0;
  plot.items.push(Item::Text { at: [at[0], below], size: TEXT, text: v.view.kind.label(n), anchor: Anchor::Middle });
}

/// 45° hatch lines inside the region bounded by `segs` (even-odd), `step` mm apart.
pub fn hatch_lines(segs: &[[[f64; 2]; 2]], step: f64) -> Vec<[[f64; 2]; 2]> {
  // rotate by −45° so hatch lines are horizontal
  let (c, s) = (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2);
  let rot = |p: [f64; 2]| [c * p[0] + s * p[1], -s * p[0] + c * p[1]];
  let back = |p: [f64; 2]| [c * p[0] - s * p[1], s * p[0] + c * p[1]];
  let r: Vec<[[f64; 2]; 2]> = segs.iter().map(|g| [rot(g[0]), rot(g[1])]).collect();
  let (lo, hi) =
    r.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
  let mut out = Vec::new();
  if !lo.is_finite() {
    return out;
  }
  let mut y = (lo / step).floor() * step + step / 2.0;
  while y < hi && out.len() < 20_000 {
    let mut xs: Vec<f64> = r
      .iter()
      .filter(|g| (g[0][1] > y) != (g[1][1] > y))
      .map(|g| g[0][0] + (y - g[0][1]) * (g[1][0] - g[0][0]) / (g[1][1] - g[0][1]))
      .collect();
    xs.sort_by(f64::total_cmp);
    for pair in xs.chunks_exact(2) {
      out.push([back([pair[0], y]), back([pair[1], y])]);
    }
    y += step;
  }
  out
}

/// A length for a dimension: up to two decimals, no trailing zeros.
pub fn fmt_len(v: f64) -> String {
  let s = format!("{v:.2}");
  let s = s.trim_end_matches('0').trim_end_matches('.');
  if s == "-0" { "0".into() } else { s.to_string() }
}

/// A linear dimension between sheet points `a` and `b`, its line `offset` mm away.
fn linear_dim(plot: &mut Plot, a: [f64; 2], b: [f64; 2], o: DimOrientation, offset: f64, text: &str) {
  // the measuring direction and the side the dimension line sits on
  let (dir, normal) = match o {
    DimOrientation::Horizontal => ([1.0, 0.0], [0.0, 1.0]),
    DimOrientation::Vertical => ([0.0, 1.0], [-1.0, 0.0]),
    DimOrientation::Aligned => {
      let d = [b[0] - a[0], b[1] - a[1]];
      let l = d[0].hypot(d[1]);
      if l < 1e-9 {
        return;
      }
      ([d[0] / l, d[1] / l], [-d[1] / l, d[0] / l])
    }
  };
  let dot = |p: [f64; 2], q: [f64; 2]| p[0] * q[0] + p[1] * q[1];
  let level = dot(a, normal).max(dot(b, normal)) + offset;
  let level = if offset < 0.0 { dot(a, normal).min(dot(b, normal)) + offset } else { level };
  let foot = |p: [f64; 2]| {
    let k = level - dot(p, normal);
    [p[0] + normal[0] * k, p[1] + normal[1] * k]
  };
  let (fa, fb) = (foot(a), foot(b));
  let ext = |p: [f64; 2], f: [f64; 2]| {
    let s = offset.signum();
    let start = [p[0] + normal[0] * s * 1.5, p[1] + normal[1] * s * 1.5];
    let end = [f[0] + normal[0] * s * 2.0, f[1] + normal[1] * s * 2.0];
    Item::Line { pen: Pen::Thin, points: vec![start, end] }
  };
  plot.items.push(ext(a, fa));
  plot.items.push(ext(b, fb));
  plot.items.push(Item::Line { pen: Pen::Thin, points: vec![fa, fb] });
  let u = {
    let d = [fb[0] - fa[0], fb[1] - fa[1]];
    let l = d[0].hypot(d[1]).max(1e-9);
    [d[0] / l, d[1] / l]
  };
  plot.items.push(arrow(fa, [-u[0], -u[1]]));
  plot.items.push(arrow(fb, u));
  let mid = [(fa[0] + fb[0]) / 2.0 + normal[0] * 1.2, (fa[1] + fb[1]) / 2.0 + normal[1] * 1.2];
  let _ = dir;
  let angle = if o == DimOrientation::Vertical { 90.0 } else { u[1].atan2(u[0]).to_degrees() };
  let angle = upright(angle);
  plot.items.push(Item::RotatedText { at: mid, size: TEXT, text: text.into(), angle });
}

/// A text angle (degrees) turned so the text reads left to right or bottom to top.
fn upright(angle: f64) -> f64 {
  if (-90.0 + 1e-9..=90.0 + 1e-9).contains(&angle) { angle } else { angle - 180.0 * angle.signum() }
}

/// A filled arrowhead with its tip at `tip`, pointing along `dir` (outward).
fn arrow(tip: [f64; 2], dir: [f64; 2]) -> Item {
  let (len, half) = (3.0, 0.8);
  let base = [tip[0] - dir[0] * len, tip[1] - dir[1] * len];
  let n = [-dir[1] * half, dir[0] * half];
  Item::Fill { points: vec![tip, [base[0] + n[0], base[1] + n[1]], [base[0] - n[0], base[1] - n[1]]] }
}

/// Overall sizes of the front view (and depth from a top or side view), and a diameter callout
/// for every distinct hole seen end-on.
fn auto_dims(plot: &mut Plot, views: &[Projected], scale: f64, std: Standard) {
  let front = views.iter().find(|v| v.view.kind == ViewKind::Front);
  let depth_view = views
    .iter()
    .find(|v| matches!(v.view.kind, ViewKind::Top | ViewKind::Bottom))
    .map(|v| (v, true))
    .or_else(|| views.iter().find(|v| matches!(v.view.kind, ViewKind::Right | ViewKind::Left)).map(|v| (v, false)));
  let corners = |v: &Projected| (sheet_pt(v, scale, v.lo), sheet_pt(v, scale, v.hi));
  if let Some(v) = front {
    let (lo, hi) = corners(v);
    let s = v.true_size();
    linear_dim(plot, [lo[0], lo[1]], [hi[0], lo[1]], DimOrientation::Horizontal, -10.0, &std.number(s[0]));
    linear_dim(plot, [lo[0], lo[1]], [lo[0], hi[1]], DimOrientation::Vertical, 10.0, &std.number(s[1]));
  }
  if let Some((v, vertical)) = depth_view {
    let (lo, hi) = corners(v);
    let s = v.true_size();
    if vertical {
      linear_dim(plot, [lo[0], lo[1]], [lo[0], hi[1]], DimOrientation::Vertical, 10.0, &std.number(s[1]));
    } else {
      linear_dim(plot, [lo[0], lo[1]], [hi[0], lo[1]], DimOrientation::Horizontal, -10.0, &std.number(s[0]));
    }
  }
  if front.is_none() && depth_view.is_none() {
    // single non-standard views (flat patterns, sections): their own extents
    for v in views {
      let (lo, hi) = corners(v);
      let s = v.true_size();
      linear_dim(plot, [lo[0], lo[1]], [hi[0], lo[1]], DimOrientation::Horizontal, -10.0, &std.number(s[0]));
      linear_dim(plot, [lo[0], lo[1]], [lo[0], hi[1]], DimOrientation::Vertical, 10.0, &std.number(s[1]));
    }
  }
  let mut seen: Vec<f64> = Vec::new();
  for v in views.iter().filter(|v| !matches!(v.view.kind, ViewKind::Iso)) {
    for c in v.proj.circles.iter().filter(|c| !c.kind.hidden() && v.shows_circle(c)) {
      let d = 2.0 * c.radius;
      if seen.iter().any(|x| (x - d).abs() < 1e-6) {
        continue;
      }
      seen.push(d);
      let o = sheet_pt(v, scale, c.center);
      let r = c.radius * scale;
      let k = std::f64::consts::FRAC_1_SQRT_2;
      let on = [o[0] + r * k, o[1] + r * k];
      let knee = [on[0] + 6.0, on[1] + 6.0];
      let end = [knee[0] + 4.0, knee[1]];
      plot.items.push(Item::Line { pen: Pen::Thin, points: vec![on, knee, end] });
      plot.items.push(arrow(on, [-k, -k]));
      let n = v
        .proj
        .circles
        .iter()
        .filter(|x| (2.0 * x.radius - d).abs() < 1e-6 && !x.kind.hidden() && v.shows_circle(x))
        .count();
      let text = if n > 1 { format!("{n}× Ø{}", std.number(d)) } else { format!("Ø{}", std.number(d)) };
      plot.items.push(Item::Text { at: [end[0] + 1.0, end[1] - 1.2], size: TEXT, text, anchor: Anchor::Start });
    }
  }
}

/// A row of the parts list.
#[derive(Debug, Clone, PartialEq)]
pub struct BomRow {
  pub name: String,
  pub qty: usize,
  pub material: String,
  /// The components counted in the row (a component and its copies).
  pub components: Vec<ComponentId>,
}

/// The parts list: one row per component, copies counted together; item n is row n - 1.
pub fn bom_rows(design: &Design) -> Vec<BomRow> {
  let mut rows: Vec<BomRow> = Vec::new();
  for c in &design.components {
    let base = c.name.split(':').next().unwrap_or(&c.name).to_string();
    let material = if c.material.is_empty() { "Steel".to_string() } else { c.material.clone() };
    match rows.iter_mut().find(|r| r.name == base) {
      Some(r) => {
        r.qty += 1;
        r.components.push(c.id);
      }
      None => rows.push(BomRow { name: base, qty: 1, material, components: vec![c.id] }),
    }
  }
  rows
}

/// A parts list above the title block.
fn parts_list(plot: &mut Plot, rows: &[BomRow]) {
  let (x1, row_h) = (plot.width - MARGIN, 6.0);
  let x0 = x1 - TITLE_W;
  let y0 = MARGIN + TITLE_H;
  let cols = [x0, x0 + 15.0, x0 + 30.0, x0 + 120.0, x1];
  let n = rows.len() + 1;
  for i in 0..=n {
    let y = y0 + i as f64 * row_h;
    plot
      .items
      .push(Item::Line { pen: if i == 0 || i == n { Pen::Border } else { Pen::Thin }, points: vec![[x0, y], [x1, y]] });
  }
  for x in cols {
    plot.items.push(Item::Line { pen: Pen::Thin, points: vec![[x, y0], [x, y0 + n as f64 * row_h]] });
  }
  let cell = |plot: &mut Plot, col: usize, row: usize, text: String| {
    plot.items.push(Item::Text {
      at: [cols[col] + 1.5, y0 + row as f64 * row_h + 1.8],
      size: 2.5,
      text,
      anchor: Anchor::Start,
    });
  };
  // the header sits on top; item 1 just under it
  let top = n - 1;
  for (k, h) in ["ITEM", "QTY", "PART", "MATERIAL"].iter().enumerate() {
    cell(plot, k, top, h.to_string());
  }
  for (i, r) in rows.iter().enumerate() {
    let row = top - 1 - i;
    cell(plot, 0, row, (i + 1).to_string());
    cell(plot, 1, row, r.qty.to_string());
    cell(plot, 2, row, r.name.clone());
    cell(plot, 3, row, r.material.clone());
  }
}

/// Balloons for the parts list rows that have none yet, in the isometric view (else the first),
/// on the centre of mass of the row's first drawn component.
fn auto_balloons(
  plot: &mut Plot,
  d: &Drawing,
  views: &[Projected],
  scale: f64,
  rows: &[BomRow],
  design: &Design,
  bodies: &[&crate::regen::Body],
) {
  let Some(pv) = views.iter().find(|v| v.view.kind == ViewKind::Iso).or(views.first()) else { return };
  let manual: Vec<ComponentId> = d
    .dimensions
    .iter()
    .filter_map(|x| if let DrawingDim::Balloon { component, .. } = x { Some(*component) } else { None })
    .collect();
  let mut placed = Vec::new();
  for (n, row) in rows.iter().enumerate() {
    if row.components.iter().any(|c| manual.contains(c)) {
      continue;
    }
    let drawn = |c: &ComponentId| {
      let comp = design.component(*c)?;
      let b = comp.bodies.iter().find_map(|id| bodies.iter().find(|b| b.id == *id))?;
      b.shape.mass().ok().map(|m| m.center_of_mass)
    };
    let Some(p) = row.components.iter().find_map(drawn) else { continue };
    let anchor = sheet_pt(pv, scale, pv.to_view(p));
    let offset = annotate::auto_offset(pv, scale, anchor, &placed);
    placed.push([anchor[0] + offset[0], anchor[1] + offset[1]]);
    annotate::balloon(plot, anchor, offset, &(n + 1).to_string());
  }
}

/// Border and title block.
fn frame(plot: &mut Plot, d: &Drawing, design: &Design, scale: f64) {
  let (w, h) = (plot.width, plot.height);
  let rect = |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]];
  plot.items.push(Item::Line { pen: Pen::Border, points: rect(MARGIN, MARGIN, w - MARGIN, h - MARGIN) });
  let (x0, y0, x1, y1) = (w - MARGIN - TITLE_W, MARGIN, w - MARGIN, MARGIN + TITLE_H);
  plot.items.push(Item::Line { pen: Pen::Border, points: rect(x0, y0, x1, y1) });
  let rows = [y0 + 12.0, y0 + 24.0];
  for y in rows {
    plot.items.push(Item::Line { pen: Pen::Thin, points: vec![[x0, y], [x1, y]] });
  }
  let cols = [x0 + 60.0, x0 + 120.0];
  for x in cols {
    plot.items.push(Item::Line { pen: Pen::Thin, points: vec![[x, y0], [x, rows[0]]] });
    plot.items.push(Item::Line { pen: Pen::Thin, points: vec![[x, rows[0]], [x, rows[1]]] });
  }
  let label = |plot: &mut Plot, x: f64, y: f64, k: &str, v: &str| {
    plot.items.push(Item::Text { at: [x + 1.5, y + 8.5], size: 2.0, text: k.into(), anchor: Anchor::Start });
    plot.items.push(Item::Text { at: [x + 1.5, y + 2.5], size: 3.5, text: v.into(), anchor: Anchor::Start });
  };
  let t = &d.title;
  let title = if t.title.is_empty() { design.name.clone() } else { t.title.clone() };
  label(plot, x0, rows[1], "TITLE", &title);
  label(plot, x0, rows[0], "DRAWING", &d.name);
  label(plot, cols[0], rows[0], "PART NUMBER", &t.part_number);
  label(plot, cols[1], rows[0], "MATERIAL", &t.material);
  label(plot, x0, y0, "SCALE", &scale_text(scale));
  let size = format!(
    "{:?} · mm · {} · {}",
    d.sheet,
    if d.projection == ProjectionAngle::ThirdAngle { "3rd angle" } else { "1st angle" },
    d.standard.name()
  );
  label(plot, cols[0], y0, "SHEET", &size);
  let rev = if t.revision.is_empty() { "-".to_string() } else { t.revision.clone() };
  let who =
    [t.author.as_str(), t.company.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ");
  label(plot, cols[1], y0, "REV / BY", &format!("{rev} {who}"));
}

/// "1:2", "2:1", "1:1".
pub fn scale_text(s: f64) -> String {
  if s >= 1.0 { format!("{}:1", fmt_len(s)) } else { format!("1:{}", fmt_len(1.0 / s)) }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hatch_square() {
    let sq =
      [[[0.0, 0.0], [10.0, 0.0]], [[10.0, 0.0], [10.0, 10.0]], [[10.0, 10.0], [0.0, 10.0]], [[0.0, 10.0], [0.0, 0.0]]];
    let h = hatch_lines(&sq, 1.0);
    assert!(h.len() > 10);
    for l in &h {
      for p in l {
        assert!(p[0] > -1e-9 && p[0] < 10.0 + 1e-9 && p[1] > -1e-9 && p[1] < 10.0 + 1e-9);
      }
    }
  }

  #[test]
  fn formats() {
    assert_eq!(fmt_len(40.0), "40");
    assert_eq!(fmt_len(12.345), "12.35");
    assert_eq!(scale_text(0.5), "1:2");
    assert_eq!(scale_text(2.0), "2:1");
  }
}
