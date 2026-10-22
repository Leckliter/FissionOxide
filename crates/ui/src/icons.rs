//! Toolbar, browser, timeline and chrome icons, drawn in code (no image assets, no font glyph
//! gaps). Icons come in Fusion's colour style (grey solids, a blue highlighted face, orange
//! sketch accents) or in one flat colour for disabled, rolled-back and failing states.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
  Sketch,
  Extrude,
  Revolve,
  Box,
  Cylinder,
  Sphere,
  Torus,
  Pattern,
  CircPattern,
  Mirror,
  Fillet,
  Chamfer,
  Shell,
  Combine,
  Move,
  Plane,
  Measure,
  Import,
  Remove,
  Params,
  Select,
  Line,
  Rect,
  CenterRect,
  Circle,
  Arc3,
  CenterArc,
  Polygon,
  Slot,
  Point,
  Dimension,
  Construction,
  Coincident,
  Horizontal,
  Vertical,
  Parallel,
  Perpendicular,
  Tangent,
  Equal,
  Concentric,
  Midpoint,
  Collinear,
  Symmetric,
  Fix,
  Finish,
  // application bar
  DataPanel,
  File,
  Save,
  Undo,
  Redo,
  Plus,
  Close,
  Bell,
  Help,
  // browser
  Folder,
  Gear,
  Component,
  Body,
  /// A surface body's browser row: an orange curved sheet, where a solid body has a grey block.
  SurfaceBody,
  SketchSmall,
  Eye,
  EyeOff,
  Origin,
  Views,
  // navigation bar
  Orbit,
  LookAt,
  Pan,
  Zoom,
  Fit,
  Display,
  Grid,
  Viewports,
  // timeline
  ToStart,
  StepBack,
  Play,
  StepForward,
  ToEnd,
  // full toolbar menus
  PressPull,
  Draft,
  Scale,
  OffsetFace,
  ReplaceFace,
  SplitFace,
  SplitBody,
  SilhouetteSplit,
  Align,
  RemoveBody,
  /// Simplify > Remove Features: a block losing its small features.
  RemoveFeatures,
  /// Simplify > Remove Faces: a block losing a face.
  RemoveFaces,
  /// Simplify > Replace with Primitives: a body turned into a box.
  ReplacePrimitives,
  PhysMaterial,
  Appearance,
  ManageMaterials,
  ComputeAll,
  Bom,
  CreateForm,
  Derive,
  Sweep,
  Loft,
  Rib,
  Web,
  Emboss,
  Hole,
  Thread,
  Coil,
  Pipe,
  Thicken,
  BoundaryFill,
  BaseFeature,
  JointOrigin,
  Configure,
  ConfigTable,
  ConfigRules,
  Ucs,
  PlaneAlt,
  Axis,
  CPoint,
  // CONSTRUCT's own icon for each command, as Fusion's
  /// Plane at Angle: a plane tilted up from an edge, with the angle.
  PlaneAngle,
  /// Tangent Plane: a plane resting on a cylinder.
  PlaneTangent,
  /// Midplane: a plane halfway between two others.
  Midplane,
  /// Perpendicular Plane: a plane standing square on another.
  PlanePerpendicular,
  /// Plane Through Two Edges: a plane through two lines.
  PlaneTwoEdges,
  /// Plane Through Three Points: a plane through three points.
  PlaneThreePoints,
  /// Plane Along Path: a plane across a curved path.
  PlaneAlongPath,
  /// Axis Through Cylinder/Cone/Torus: a cylinder's axis.
  AxisCylinder,
  /// Axis Perpendicular To Face: an axis standing on a face.
  AxisNormal,
  /// Axis Through Two Planes: the line where two planes cross.
  AxisTwoPlanes,
  /// Axis Through Two Points: an axis through two points.
  AxisTwoPoints,
  /// Axis Through Edge: an axis along a block's edge.
  AxisEdge,
  /// Point At Vertex: a point on a block's corner.
  PointVertex,
  /// Point Through Two Edges: a point where two lines cross.
  PointTwoEdges,
  /// Point Through Three Planes: a point where three planes meet.
  PointThreePlanes,
  /// Point At Center Of Circle/Sphere/Torus: a point at a circle's centre.
  PointCenter,
  /// Point At Edge And Plane: a point where a line pierces a plane.
  PointEdgePlane,
  /// Point Along Path: a point on a curved path.
  PointAlongPath,
  Interference,
  Section,
  CurvComb,
  CurvMap,
  DraftAnalysis,
  EnvMap,
  Isocurve,
  Zebra,
  Accessibility,
  CenterOfMass,
  MinRadius,
  Navigator,
  ComponentColors,
  FaceGroups,
  History,
  Sustainability,
  Decal,
  Canvas,
  InsertSvg,
  InsertDxf,
  InsertMesh,
  InsertComponent,
  Fastener,
  DuplicateJoints,
  McMaster,
  Manufacturer,
  AddToAssembly,
  WindowSel,
  FreeformSel,
  PaintSel,
  Patch,
  Ruled,
  SurfOffset,
  Trim,
  Untrim,
  Extend,
  Stitch,
  Unstitch,
  ReverseNormal,
  Tessellate,
  MeshSection,
  BaseMesh,
  Repair,
  GenFaceGroups,
  CombineFaceGroups,
  DirectEdit,
  Remesh,
  Reduce,
  PlaneCut,
  Smooth,
  Separate,
  ScaleMesh,
  ConvertMesh,
  /// A mesh body's browser row: a yellow triangulated block.
  MeshBody,
  /// A mesh feature in the timeline (Insert Mesh's "Base Mesh Feature").
  MeshFeature,
  /// INSERT MESH's Center: a block on the origin's cross.
  MeshCenter,
  /// INSERT MESH's Move To Ground: a block coming down onto the ground line.
  MoveToGround,
  Flange,
  Hem,
  LoftedFlange,
  Fold,
  ConvertSheet,
  FlatPattern,
  Unfold,
  SmRules,
  CornerClosure,
  Rip,
  Refold,
  DriveJoint,
  MotionStudy,
  NewDrawing,
  BaseView,
  ProjectedView,
  SectionView,
  DetailView,
  PartsList,
  TitleBlock,
  SheetSettings,
  ExportPdf,
  ExportDxf,
  ExportSvg,
  Print3d,
  Scripts,
  Marketplace,
  Assistant,
  Gauge,
  Clock,
  // sketch family
  MidpointLine,
  Rect3,
  Circle2P,
  Circle3P,
  Circle2T,
  Circle3T,
  TangentArc,
  PolygonCirc,
  PolygonEdge,
  Ellipse,
  OverallSlot,
  CenterPointSlot,
  ArcSlot3,
  ArcSlotCenter,
  SplineFit,
  SplineCtrl,
  Conic,
  SkText,
  TraceImage,
  SkMirror,
  SkCircPattern,
  SkRectPattern,
  Project,
  Intersect,
  Include3D,
  ProjectSurface,
  SkFillet,
  SkChamfer,
  BlendCurve,
  SkOffset,
  SkTrim,
  SkExtend,
  SkBreak,
  SkScale,
  HorizVert,
  Curvature,
  PolygonCon,
  FinishAuto,
  // drawing annotations
  BreakView,
  OrdinateDim,
  AngleDim,
  FeatureControl,
  DatumId,
  SurfaceTexture,
  Balloon,
  /// Inspect > Properties: a sheet of key/value rows.
  Properties,
  /// Sketch Scale to Size / Fit in Box.
  SkScaleTo,
  /// Insert > Sketch Library.
  SketchLibrary,
  // sketch clean-up
  SkRemoveDuplicates,
  SkCleanup,
  /// The Sketch Palette's Centerline linetype toggle (dash-dot).
  Centerline,
  // Move/Copy's Move Type and Set Pivot
  MoveFree,
  MoveTranslate,
  MoveRotate,
  MovePointToPoint,
  MovePointToPosition,
  SetPivot,
  // Shell Type: sharp or rounded corners
  ShellSharp,
  ShellRound,
  // Hole: Placement, Hole Type, Hole Tap Type and Drill Point
  HoleSingle,
  HoleAtPoints,
  HoleSimple,
  HoleCounterbore,
  HoleCountersink,
  TapSimple,
  TapClearance,
  TapTapped,
  TapTaper,
  DrillAngle,
  DrillFlat,
  // Extrude dialog values (see `dialogs::choice`): Type toggles, Start, Direction, Extent Type,
  // Wall Location and Operation
  ExtrudeSolid,
  ExtrudeThin,
  ProfilePlane,
  StartOffset,
  StartObject,
  OneSide,
  TwoSides,
  SymmetricExtent,
  ExtentDistance,
  ToObject,
  ThroughAll,
  WallSide1,
  WallSide2,
  WallCenter,
  OpJoin,
  OpCut,
  OpIntersect,
  OpNewBody,
  OpNewComponent,
  // Text's Type row and the Project / Include dialogs' Selection Filter
  TextBox,
  TextOnPath,
  FilterBodies,
  FilterFaces,
  // Sketch Offset's Flip button: the offset to the other side
  Flip,
  /// Sketch Distance and Angle Chamfer: the bevel with its angle arc.
  SkChamferAngle,
  /// Sketch Two Distance Chamfer: a lopsided bevel with a tick on each leg.
  SkChamfer2,
  // Change Parameters' toolbar
  FilterFavorites,
  Star,
  Duplicate,
  Trash,
  Export,
  // Text's Typeface, Flip and Alignment rows
  Bold,
  Italic,
  FlipH,
  FlipV,
  AlignLeft,
  AlignCenter,
  AlignRight,
  AlignTop,
  AlignMiddle,
  AlignBottom,
  // the sketch patterns' Distribution and Direction drop-downs
  DistFull,
  DistAngle,
  DistSymmetric,
  DistExtent,
  DistSpacing,
  DirOne,
  DirSymmetric,
  /// An edit pencil (Convert Design Type).
  Pencil,
  /// A named view: a framed picture.
  NamedView,
  // Grid and Snaps' two commands
  GridSettings,
  SetIncrements,
  /// The navigation bar's Zoom Window: a magnifier inside a dashed box.
  ZoomWindow,
  // Coil's Rotation toggles
  RotateCw,
  RotateCcw,
  /// Rib's and Web's To Next extent: grown until the body is met.
  ToNext,
  // Emboss's Effect toggles: raised or sunk into the face
  EmbossRaise,
  EmbossSink,
  /// Thread's Threads Library: the cloud of custom thread definitions.
  ThreadsLibrary,
  // the solid patterns' Type row (rectangular and circular use `Pattern` / `CircPattern`)
  PathPattern,
  // Joint Origin's Origin Mode row
  OriginSimple,
  OriginBetween,
  OriginTwoEdge,
  // Draft's Type row
  DraftFixedPlane,
  DraftPartingLine,
  // Hem's Type drop-down: the sheet's end folded back, seen edge-on
  HemFlat,
  HemOpen,
  HemRolled,
  HemTeardrop,
  HemRope,
  // the timeline's sheet metal flanges: a flat sheet, and a sheet with a flange bent up
  BaseFlange,
  EdgeFlange,
  // Reduce's Remesh Type, Plane Cut's Fill Type and Convert Mesh's Method (see `mesh`)
  RemeshAdaptive,
  RemeshUniform,
  FillNone,
  FillUniform,
  FillMinimal,
  ConvertFaceted,
  ConvertPrismatic,
  ConvertOrganic,
  // the Render workspace
  /// Lights: a sun.
  Sun,
  /// Render: the teapot.
  Teapot,
  /// In-canvas Render while it runs: a stop square.
  Stop,
  /// Dielectric Priority Control: a glass with 0, 1 and 2.
  DielectricPriority,
  /// Texture Map Controls: a chequered square with its handles.
  TextureMap,
  /// In-canvas Render Settings: the viewport with a gear.
  InCanvasSettings,
  /// Capture Image: a camera.
  Camera,
  // the Animation workspace's commands
  /// New Storyboard: a strip of film frames.
  Storyboard,
  /// Transform Components: a component with move arrows.
  TransformComponents,
  /// Restore Home: a house.
  RestoreHome,
  /// Auto Explode: blocks flying apart.
  Explode,
  /// Manual Explode: one block pulled out along an arrow.
  ManualExplode,
  /// Create Callout: a speech bubble.
  Callout,
  /// The Animation workspace's View: a camera.
  AnimView,
  /// Publish Video: a film clapper with a play mark.
  Video,
  /// An assembly (the Animation browser's root): two components side by side.
  Assembly,
  /// Mesh Combine's Merge: two meshes side by side, joined as they are.
  OpMerge,
}

#[derive(Clone, Copy)]
enum Tone {
  Ink,
  Fill,
  Light,
  Dark,
  Blue,
  BlueLight,
  BlueDark,
  Orange,
  Green,
  Red,
  White,
  Purple,
  PurpleLight,
  // FissionOxide's own families, each dark slate with one accent: sketch (teal vertices), sketch
  // edits (coral), constraints (violet) and construction geometry (amber)
  Slate,
  Teal,
  Coral,
  Violet,
  Amber,
}

/// An icon's accent family, one of the brand colour families (`fox-brand`). Every icon is a
/// dark slate line drawing with one accent colour: blue for solid modelling (the default),
/// orange for surfaces, gold for meshes, amber for construction geometry, green for inspecting
/// and selecting, teal for sketching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
  Blue,
  Orange,
  Gold,
  Amber,
  Green,
  Teal,
}

impl Tint {
  fn accent(self) -> Color32 {
    match self {
      Tint::Blue => theme::ICON_BLUE,
      Tint::Orange => theme::ICON_ORANGE,
      Tint::Gold => theme::color(fox_brand::GOLD.base),
      Tint::Amber => theme::color(fox_brand::AMBER.base),
      Tint::Green => theme::ICON_GREEN,
      Tint::Teal => theme::color(fox_brand::TEAL.base),
    }
  }
}

/// The accent colour of `icon`'s family, for UI that should match a command (its dialog).
pub fn accent_of(icon: Icon) -> Color32 {
  family(icon).accent()
}

/// A light tint of `c` for backgrounds (selection chips, the primary button).
pub fn tint_of(c: Color32, amount: f32) -> Color32 {
  mix(c, Color32::WHITE, amount)
}

/// The accent family an icon belongs to when no tint is given.
fn family(icon: Icon) -> Tint {
  use Icon::*;
  match icon {
    Plane | PlaneAlt | Axis | CPoint => Tint::Amber,
    PlaneAngle | PlaneTangent | Midplane | PlanePerpendicular | PlaneTwoEdges | PlaneThreePoints | PlaneAlongPath
    | AxisCylinder | AxisNormal | AxisTwoPlanes | AxisTwoPoints | AxisEdge | PointVertex | PointTwoEdges
    | PointThreePlanes | PointCenter | PointEdgePlane | PointAlongPath => Tint::Amber,
    Measure | Interference | Section | CurvComb | CurvMap | DraftAnalysis | EnvMap | Isocurve | Zebra
    | Accessibility | CenterOfMass | MinRadius | Navigator | ComponentColors | FaceGroups | History
    | Sustainability | Select | WindowSel | FreeformSel | PaintSel => Tint::Green,
    Sketch | SketchSmall | Construction => Tint::Teal,
    MeshBody | MeshFeature => Tint::Gold,
    SurfaceBody => Tint::Orange,
    _ => Tint::Blue,
  }
}

/// Icons whose colours carry meaning and stay literal (delete red, finish green, RGB axes,
/// the colour wheel, the traffic light).
fn literal(icon: Icon) -> bool {
  use Icon::*;
  matches!(icon, Finish | FinishAuto | Remove | Origin | Ucs | Appearance | ComputeAll | Untrim | Unstitch)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
  let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
  Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

const SLATE: Color32 = theme::ICON_INK;
/// Outlines of grey bodies: a mid grey, so solids read as light shapes, not ink drawings.
const GREY_EDGE: Color32 = Color32::from_rgb(122, 132, 146);

struct Pen<'a> {
  p: &'a Painter,
  r: Rect,
  mono: Option<Color32>,
  tint: Tint,
  literal: bool,
  w: f32,
}

impl Pen<'_> {
  fn u(&self, x: f32, y: f32) -> Pos2 {
    pos2(self.r.min.x + x * self.r.width(), self.r.min.y + y * self.r.height())
  }

  fn col(&self, t: Tone) -> Color32 {
    match self.mono {
      Some(c) => match t {
        Tone::Fill | Tone::Dark | Tone::Blue | Tone::BlueDark | Tone::Orange | Tone::Green => c.gamma_multiply(0.35),
        Tone::BlueLight => c.gamma_multiply(0.2),
        Tone::Light => c.gamma_multiply(0.15),
        Tone::White => Color32::from_white_alpha(200),
        Tone::Purple => c.gamma_multiply(0.35),
        Tone::PurpleLight => c.gamma_multiply(0.15),
        Tone::Slate | Tone::Teal | Tone::Coral | Tone::Violet => c,
        Tone::Amber => c.gamma_multiply(0.4),
        Tone::Ink | Tone::Red => c,
      },
      None => {
        // the unified style: slate lines, white/grey bodies, everything coloured in shades of
        // the family accent
        let accent = self.tint.accent();
        let lit = |c: Color32| if self.literal { c } else { accent };
        match t {
          Tone::Ink | Tone::Slate => SLATE,
          Tone::Fill => Color32::from_rgb(233, 236, 240),
          Tone::Light | Tone::White => Color32::WHITE,
          Tone::Dark => Color32::from_rgb(212, 217, 224),
          Tone::Blue => accent,
          Tone::BlueLight | Tone::PurpleLight => mix(accent, Color32::WHITE, 0.5),
          Tone::BlueDark => mix(accent, Color32::WHITE, 0.72),
          Tone::Orange => lit(theme::ICON_ORANGE),
          Tone::Green => lit(theme::ICON_GREEN),
          Tone::Purple => lit(theme::color(fox_brand::PURPLE.base)),
          Tone::Amber => lit(theme::color(fox_brand::AMBER.base)),
          Tone::Red => theme::ICON_RED,
          Tone::Teal => theme::color(fox_brand::TEAL.base),
          Tone::Coral => theme::color(fox_brand::CORAL.base),
          Tone::Violet => theme::color(fox_brand::PURPLE.dark),
        }
      }
    }
  }

  /// Fill colour: accent fills are light tints of the accent, so icons stay line drawings
  /// with the strong accent kept for lines and small details.
  fn fc(&self, t: Tone) -> Color32 {
    if self.mono.is_some() || self.literal {
      return self.col(t);
    }
    let w = Color32::WHITE;
    match t {
      Tone::Blue => mix(self.col(t), w, 0.3),
      Tone::BlueLight | Tone::PurpleLight => mix(self.tint.accent(), w, 0.62),
      Tone::BlueDark => mix(self.tint.accent(), w, 0.8),
      Tone::Orange | Tone::Green | Tone::Purple | Tone::Amber => mix(self.col(t), w, 0.3),
      _ => self.col(t),
    }
  }

  /// A tube-like stroke: a slate outline around a light accent core.
  fn band(&self, pts: Vec<Pos2>, k: f32) {
    self.p.add(Shape::line(pts.clone(), Stroke::new(self.w * k, self.edge(Tone::Blue))));
    self.p.add(Shape::line(pts, Stroke::new(self.w * (k - 1.6), self.fc(Tone::Blue))));
  }

  /// Outline colour: a darker shade of the face it bounds (Fusion's tonal edges), so solids
  /// read as light shapes rather than ink drawings. Slate stays for lines and details.
  fn edge(&self, t: Tone) -> Color32 {
    if let Some(c) = self.mono {
      return c;
    }
    let dark = |c: Color32| mix(c, SLATE, 0.3);
    match t {
      Tone::Fill | Tone::Light | Tone::White | Tone::Dark => GREY_EDGE,
      Tone::Blue | Tone::BlueLight | Tone::BlueDark | Tone::PurpleLight => dark(self.tint.accent()),
      Tone::Orange | Tone::Green | Tone::Purple | Tone::Amber | Tone::Red | Tone::Teal | Tone::Coral | Tone::Violet => {
        dark(self.col(t))
      }
      Tone::Ink | Tone::Slate => SLATE,
    }
  }

  fn pts(&self, pts: &[(f32, f32)]) -> Vec<Pos2> {
    pts.iter().map(|&(x, y)| self.u(x, y)).collect()
  }

  /// A filled convex shape with an outline.
  fn solid(&self, pts: &[(f32, f32)], t: Tone) {
    let v = self.pts(pts);
    self.p.add(Shape::convex_polygon(v.clone(), self.fc(t), Stroke::NONE));
    self.p.add(Shape::closed_line(v, Stroke::new(self.w * 0.8, self.edge(t))));
  }

  fn fill(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::convex_polygon(self.pts(pts), self.fc(t), Stroke::NONE));
  }

  fn path(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::line(self.pts(pts), Stroke::new(self.w, self.col(t))));
  }

  fn closed(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::closed_line(self.pts(pts), Stroke::new(self.w, self.col(t))));
  }

  fn line(&self, a: (f32, f32), b: (f32, f32), t: Tone) {
    self.p.line_segment([self.u(a.0, a.1), self.u(b.0, b.1)], Stroke::new(self.w, self.col(t)));
  }

  fn thick(&self, a: (f32, f32), b: (f32, f32), t: Tone, k: f32) {
    self.p.line_segment([self.u(a.0, a.1), self.u(b.0, b.1)], Stroke::new(self.w * k, self.col(t)));
  }

  fn dashed(&self, a: (f32, f32), b: (f32, f32), t: Tone) {
    let (a, b) = (self.u(a.0, a.1), self.u(b.0, b.1));
    let n = 7;
    for i in (0..n).step_by(2) {
      let t0 = i as f32 / n as f32;
      let t1 = (i + 1) as f32 / n as f32;
      self.p.line_segment([a + (b - a) * t0, a + (b - a) * t1], Stroke::new(self.w * 0.8, self.col(t)));
    }
  }

  fn circle(&self, c: (f32, f32), rad: f32, fill: Option<Tone>, stroke: Option<Tone>) {
    let c = self.u(c.0, c.1);
    let rad = rad * self.r.width();
    if let Some(f) = fill {
      self.p.circle_filled(c, rad, self.fc(f));
    }
    if let Some(s) = stroke {
      let col = if fill.is_some() { self.edge(s) } else { self.col(s) };
      self.p.circle_stroke(c, rad, Stroke::new(if fill.is_some() { self.w * 0.8 } else { self.w }, col));
    }
  }

  /// Sketch family: a geometry stroke.
  fn sk(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::line(self.pts(pts), Stroke::new(self.w * 1.15, self.col(t))));
  }

  /// Sketch family: a closed geometry stroke.
  fn skc(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::closed_line(self.pts(pts), Stroke::new(self.w * 1.15, self.col(t))));
  }

  /// Sketch family: an arc stroke.
  fn ska(&self, c: (f32, f32), rad: f32, a0: f32, a1: f32, t: Tone) {
    self.p.add(Shape::line(self.arc_pts(c, rad, rad, a0, a1), Stroke::new(self.w * 1.15, self.col(t))));
  }

  /// Sketch family: a vertex, a hollow teal square (Fusion uses filled dots).
  fn vtx(&self, c: (f32, f32)) {
    let h = (self.r.width() * 0.075).max(2.2);
    let rect = Rect::from_center_size(self.u(c.0, c.1), Vec2::splat(h * 2.0));
    self.p.rect(
      rect,
      0.5,
      self.col(Tone::White),
      Stroke::new(self.w * 0.9, self.col(Tone::Teal)),
      egui::StrokeKind::Inside,
    );
  }

  fn dot(&self, c: (f32, f32), t: Tone) {
    let rad = (self.r.width() * 0.07).max(1.6);
    self.p.circle_filled(self.u(c.0, c.1), rad, self.col(t));
  }

  /// Arc in unit coordinates, angles counter-clockwise from +x (screen y up).
  fn arc_pts(&self, c: (f32, f32), rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = 20;
    (0..=n)
      .map(|i| {
        let t = a0 + (a1 - a0) * i as f32 / n as f32;
        self.u(c.0 + t.cos() * rx, c.1 - t.sin() * ry)
      })
      .collect()
  }

  fn arc(&self, c: (f32, f32), rad: f32, a0: f32, a1: f32, t: Tone) {
    self.p.add(Shape::line(self.arc_pts(c, rad, rad, a0, a1), Stroke::new(self.w, self.col(t))));
  }

  fn ellipse(&self, c: (f32, f32), rx: f32, ry: f32, fill: Option<Tone>, stroke: Tone) {
    let pts = self.arc_pts(c, rx, ry, 0.0, std::f32::consts::TAU);
    if let Some(f) = fill {
      self.p.add(Shape::convex_polygon(pts.clone(), self.fc(f), Stroke::NONE));
    }
    self.p.add(Shape::closed_line(pts, Stroke::new(self.w * 0.8, self.edge(stroke))));
  }

  fn arrow_head(&self, tip: (f32, f32), dir: (f32, f32), size: f32, t: Tone) {
    let d = vec2(dir.0, dir.1).normalized();
    let s = vec2(-d.y, d.x);
    let tip = self.u(tip.0, tip.1);
    let k = size * self.r.width();
    self.p.add(Shape::convex_polygon(
      vec![tip, tip - d * k + s * k * 0.6, tip - d * k - s * k * 0.6],
      self.col(t),
      Stroke::NONE,
    ));
  }

  fn text(&self, at: (f32, f32), s: &str, size: f32, t: Tone) {
    self.p.text(
      self.u(at.0, at.1),
      egui::Align2::CENTER_CENTER,
      s,
      egui::FontId::proportional(size * self.r.width()),
      self.col(t),
    );
  }

  /// An isometric block: front, top and side faces.
  fn block(&self, x0: f32, y0: f32, x1: f32, y1: f32, d: f32, top: Tone, front: Tone, side: Tone) {
    self.solid(&[(x0, y0), (x0 + d, y0 - d * 0.7), (x1 + d, y0 - d * 0.7), (x1, y0)], top);
    self.solid(&[(x1, y0), (x1 + d, y0 - d * 0.7), (x1 + d, y1 - d * 0.7), (x1, y1)], side);
    self.solid(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], front);
  }
}

/// Draws `icon` into `r` in one flat colour.
pub fn draw(p: &Painter, r: Rect, icon: Icon, c: Color32) {
  paint(p, r, icon, Some(c), Tint::Blue);
}

/// Draws `icon` in colour with its solids in the `tint` family.
pub fn draw_tinted(p: &Painter, r: Rect, icon: Icon, tint: Tint) {
  // blue is the default family: icons from other families keep their own accent
  let tint = if tint == Tint::Blue { family(icon) } else { tint };
  paint(p, r, icon, None, tint);
}

/// Draws `icon` into `r` in Fusion's colour style.
pub fn draw_color(p: &Painter, r: Rect, icon: Icon) {
  paint(p, r, icon, None, family(icon));
}

fn paint(p: &Painter, r: Rect, icon: Icon, mono: Option<Color32>, tint: Tint) {
  use Tone::*;
  use std::f32::consts::{FRAC_PI_2, PI, TAU};
  let w = (r.width() / 24.0).clamp(0.9, 1.25);
  let k = Pen { p, r, mono, tint, literal: literal(icon), w };
  match icon {
    Icon::Sketch => {
      k.solid(&[(0.08, 0.14), (0.78, 0.14), (0.78, 0.78), (0.08, 0.78)], White);
      k.skc(&[(0.08, 0.14), (0.78, 0.14), (0.78, 0.78), (0.08, 0.78)], Slate);
      k.vtx((0.08, 0.14));
      k.vtx((0.78, 0.14));
      k.vtx((0.08, 0.78));
      k.thick((0.78, 0.58), (0.78, 0.98), Green, 2.2);
      k.thick((0.58, 0.78), (0.98, 0.78), Green, 2.2);
    }
    Icon::Extrude => {
      k.block(0.12, 0.34, 0.62, 0.94, 0.26, BlueLight, Blue, BlueDark);
      k.thick((0.37, 0.3), (0.37, 0.04), Ink, 1.0);
      k.arrow_head((0.37, 0.0), (0.0, -1.0), 0.13, Ink);
    }
    Icon::Revolve => {
      // a wide revolved sheet, not a thin ring
      k.band(k.arc_pts((0.5, 0.48), 0.34, 0.28, -2.9, 0.1), 8.0);
      k.arrow_head((0.92, 0.42), (0.25, -1.0), 0.18, Blue);
      k.dashed((0.5, 0.02), (0.5, 0.98), Ink);
    }
    Icon::Box => k.block(0.1, 0.38, 0.66, 0.9, 0.26, BlueLight, Blue, BlueDark),
    Icon::Cylinder => {
      k.ellipse((0.5, 0.78), 0.32, 0.12, Some(Blue), Blue);
      k.fill(&[(0.18, 0.25), (0.82, 0.25), (0.82, 0.78), (0.18, 0.78)], Blue);
      k.line((0.18, 0.25), (0.18, 0.78), BlueDark);
      k.line((0.82, 0.25), (0.82, 0.78), BlueDark);
      k.ellipse((0.5, 0.25), 0.32, 0.12, Some(BlueLight), Blue);
    }
    Icon::Sphere => {
      k.circle((0.5, 0.5), 0.4, Some(Blue), Some(Blue));
      k.circle((0.38, 0.38), 0.13, Some(BlueLight), None);
    }
    Icon::Torus => {
      k.ellipse((0.5, 0.5), 0.44, 0.28, Some(Blue), Blue);
      k.ellipse((0.5, 0.47), 0.18, 0.08, Some(White), Blue);
    }
    Icon::Pattern => {
      for (i, (x, y)) in [(0.06, 0.08), (0.56, 0.08), (0.06, 0.58), (0.56, 0.58)].into_iter().enumerate() {
        k.solid(&[(x, y), (x + 0.36, y), (x + 0.36, y + 0.34), (x, y + 0.34)], if i == 0 { BlueDark } else { Blue });
      }
    }
    Icon::CircPattern => {
      k.circle((0.5, 0.5), 0.3, None, Some(Light));
      for i in 0..6 {
        let t = TAU * i as f32 / 6.0 - FRAC_PI_2;
        let c = (0.5 + t.cos() * 0.32, 0.5 + t.sin() * 0.32);
        k.circle(c, 0.11, Some(if i == 0 { BlueDark } else { Blue }), Some(Blue));
      }
    }
    Icon::Mirror => {
      k.solid(&[(0.06, 0.85), (0.42, 0.85), (0.42, 0.2)], BlueLight);
      k.solid(&[(0.94, 0.85), (0.58, 0.85), (0.58, 0.2)], Blue);
      k.dashed((0.5, 0.04), (0.5, 0.96), Ink);
    }
    Icon::Fillet => {
      let mut v = k.pts(&[(0.92, 0.92), (0.08, 0.92), (0.08, 0.08), (0.5, 0.08)]);
      v.extend(k.arc_pts((0.5, 0.5), 0.42, 0.42, FRAC_PI_2, 0.0));
      k.p.add(Shape::convex_polygon(v.clone(), k.col(Fill), Stroke::NONE));
      k.p.add(Shape::closed_line(v, Stroke::new(w * 0.8, k.col(Ink))));
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.36, 0.36, FRAC_PI_2, 0.0), Stroke::new(w * 2.6, k.col(Blue))));
    }
    Icon::Chamfer => {
      k.solid(&[(0.08, 0.92), (0.08, 0.45), (0.45, 0.08), (0.92, 0.08), (0.92, 0.92)], Fill);
      k.solid(&[(0.08, 0.45), (0.45, 0.08), (0.58, 0.2), (0.2, 0.58)], Blue);
    }
    Icon::Shell => {
      k.block(0.08, 0.38, 0.7, 0.92, 0.22, Light, Fill, Dark);
      k.solid(&[(0.18, 0.36), (0.28, 0.27), (0.76, 0.27), (0.66, 0.36)], Blue);
    }
    Icon::Combine => {
      k.block(0.06, 0.4, 0.5, 0.88, 0.2, BlueLight, Blue, BlueDark);
      k.block(0.36, 0.22, 0.78, 0.66, 0.2, BlueLight, Blue, BlueDark);
    }
    Icon::Move => {
      for (tip, dir) in
        [((0.5, 0.02), (0.0, -1.0)), ((0.5, 0.98), (0.0, 1.0)), ((0.02, 0.5), (-1.0, 0.0)), ((0.98, 0.5), (1.0, 0.0))]
      {
        k.thick((0.5, 0.5), (tip.0 - dir.0 * 0.12, tip.1 - dir.1 * 0.12), Ink, 1.6);
        k.arrow_head(tip, dir, 0.2, Ink);
      }
    }
    Icon::Plane => {
      k.skc(&[(0.04, 0.9), (0.28, 0.6), (0.96, 0.6), (0.72, 0.9)], Slate);
      k.solid(&[(0.04, 0.42), (0.28, 0.1), (0.96, 0.1), (0.72, 0.42)], Amber);
      k.thick((0.5, 0.78), (0.5, 0.36), Slate, 1.0);
      k.arrow_head((0.5, 0.3), (0.0, -1.0), 0.12, Slate);
    }
    Icon::Measure => {
      k.solid(&[(0.04, 0.44), (0.96, 0.44), (0.96, 0.72), (0.04, 0.72)], Orange);
      for i in 1..8 {
        let x = 0.04 + i as f32 * 0.115;
        k.line((x, 0.44), (x, if i % 2 == 0 { 0.6 } else { 0.54 }), Ink);
      }
      k.line((0.04, 0.24), (0.96, 0.24), Blue);
      k.line((0.04, 0.14), (0.04, 0.34), Blue);
      k.line((0.96, 0.14), (0.96, 0.34), Blue);
    }
    Icon::Import => {
      k.solid(&[(0.08, 0.6), (0.92, 0.6), (0.92, 0.92), (0.08, 0.92)], Fill);
      k.thick((0.5, 0.04), (0.5, 0.52), Blue, 1.6);
      k.arrow_head((0.5, 0.68), (0.0, 1.0), 0.2, Blue);
    }
    Icon::Remove => {
      k.thick((0.2, 0.2), (0.8, 0.8), Red, 1.4);
      k.thick((0.8, 0.2), (0.2, 0.8), Red, 1.4);
    }
    Icon::Params => {
      k.solid(&[(0.06, 0.14), (0.94, 0.14), (0.94, 0.86), (0.06, 0.86)], Light);
      k.text((0.5, 0.5), "ƒx", 0.55, Blue);
    }
    Icon::Select => {
      for (a, b) in [
        ((0.06, 0.06), (0.78, 0.06)),
        ((0.06, 0.06), (0.06, 0.7)),
        ((0.78, 0.06), (0.78, 0.4)),
        ((0.06, 0.7), (0.4, 0.7)),
      ] {
        k.dashed(a, b, Green);
      }
      k.solid(&[(0.46, 0.34), (0.46, 0.98), (0.62, 0.84), (0.82, 0.84)], White);
    }
    Icon::Line => {
      k.sk(&[(0.14, 0.86), (0.86, 0.14)], Slate);
      k.vtx((0.14, 0.86));
      k.vtx((0.86, 0.14));
    }
    Icon::Rect => {
      k.skc(&[(0.1, 0.22), (0.9, 0.22), (0.9, 0.78), (0.1, 0.78)], Slate);
      k.vtx((0.1, 0.78));
      k.vtx((0.9, 0.22));
    }
    Icon::CenterRect => {
      k.skc(&[(0.1, 0.22), (0.9, 0.22), (0.9, 0.78), (0.1, 0.78)], Slate);
      k.dashed((0.5, 0.5), (0.9, 0.22), Slate);
      k.vtx((0.5, 0.5));
      k.vtx((0.9, 0.22));
    }
    Icon::Circle => {
      k.ska((0.5, 0.5), 0.38, 0.0, TAU, Slate);
      k.dashed((0.5, 0.5), (0.77, 0.23), Slate);
      k.vtx((0.5, 0.5));
      k.vtx((0.77, 0.23));
    }
    Icon::Arc3 => {
      k.ska((0.5, 0.78), 0.4, 0.0, PI, Slate);
      k.vtx((0.1, 0.78));
      k.vtx((0.9, 0.78));
      k.vtx((0.5, 0.38));
    }
    Icon::CenterArc => {
      k.ska((0.22, 0.78), 0.6, 0.0, FRAC_PI_2, Slate);
      k.dashed((0.22, 0.78), (0.82, 0.78), Slate);
      k.dashed((0.22, 0.78), (0.22, 0.18), Slate);
      k.vtx((0.22, 0.78));
      k.vtx((0.82, 0.78));
      k.vtx((0.22, 0.18));
    }
    Icon::Polygon => {
      let pts: Vec<(f32, f32)> = (0..6)
        .map(|i| {
          let t = TAU * i as f32 / 6.0;
          (0.5 + t.cos() * 0.4, 0.5 + t.sin() * 0.4)
        })
        .collect();
      k.skc(&pts, Slate);
      k.dashed((0.5, 0.5), (0.9, 0.5), Slate);
      k.vtx((0.5, 0.5));
      k.vtx((0.9, 0.5));
    }
    Icon::Slot => {
      k.ska((0.3, 0.5), 0.22, FRAC_PI_2, 1.5 * PI, Slate);
      k.ska((0.7, 0.5), 0.22, -FRAC_PI_2, FRAC_PI_2, Slate);
      k.sk(&[(0.3, 0.28), (0.7, 0.28)], Slate);
      k.sk(&[(0.3, 0.72), (0.7, 0.72)], Slate);
      k.dashed((0.3, 0.5), (0.7, 0.5), Slate);
      k.vtx((0.3, 0.5));
      k.vtx((0.7, 0.5));
    }
    Icon::Point => {
      k.sk(&[(0.5, 0.1), (0.5, 0.3)], Slate);
      k.sk(&[(0.5, 0.7), (0.5, 0.9)], Slate);
      k.sk(&[(0.1, 0.5), (0.3, 0.5)], Slate);
      k.sk(&[(0.7, 0.5), (0.9, 0.5)], Slate);
      k.vtx((0.5, 0.5));
    }
    Icon::Dimension => {
      k.sk(&[(0.1, 0.5), (0.1, 0.94)], Slate);
      k.sk(&[(0.9, 0.5), (0.9, 0.94)], Slate);
      k.sk(&[(0.16, 0.72), (0.84, 0.72)], Teal);
      k.arrow_head((0.11, 0.72), (-1.0, 0.0), 0.14, Teal);
      k.arrow_head((0.89, 0.72), (1.0, 0.0), 0.14, Teal);
      k.text((0.5, 0.32), "10", 0.42, Slate);
    }
    Icon::Construction => {
      k.dashed((0.12, 0.88), (0.88, 0.12), Orange);
      k.vtx((0.12, 0.88));
      k.vtx((0.88, 0.12));
    }
    // Fusion's constraint icons: red/orange line art
    Icon::Coincident => {
      // an "L" corner with a point at it
      k.sk(&[(0.22, 0.18), (0.22, 0.8), (0.86, 0.8)], Coral);
      k.circle((0.22, 0.8), 0.11, Some(Coral), None);
    }
    Icon::Horizontal => {
      k.thick((0.1, 0.62), (0.9, 0.62), Slate, 1.2);
      k.text((0.5, 0.3), "H", 0.4, Violet);
    }
    Icon::Vertical => {
      k.thick((0.64, 0.1), (0.64, 0.9), Slate, 1.2);
      k.text((0.3, 0.5), "V", 0.4, Violet);
    }
    Icon::Parallel => {
      // two slanted bars
      k.thick((0.16, 0.86), (0.48, 0.14), Coral, 1.3);
      k.thick((0.52, 0.86), (0.84, 0.14), Coral, 1.3);
    }
    Icon::Perpendicular => {
      // an "L"
      k.sk(&[(0.24, 0.14), (0.24, 0.84), (0.88, 0.84)], Coral);
    }
    Icon::Tangent => {
      // a circle with a tangent line
      k.ska((0.5, 0.6), 0.27, 0.0, TAU, Coral);
      k.thick((0.06, 0.33), (0.94, 0.33), Coral, 1.2);
    }
    Icon::Equal => {
      // two bars
      k.thick((0.14, 0.36), (0.86, 0.36), Coral, 1.4);
      k.thick((0.14, 0.64), (0.86, 0.64), Coral, 1.4);
    }
    Icon::Concentric => {
      // two rings
      k.ska((0.5, 0.5), 0.42, 0.0, TAU, Coral);
      k.ska((0.5, 0.5), 0.2, 0.0, TAU, Coral);
    }
    Icon::Midpoint => {
      // a triangle
      k.skc(&[(0.5, 0.16), (0.88, 0.82), (0.12, 0.82)], Coral);
    }
    Icon::Collinear => {
      // two bars offset along one line
      k.thick((0.06, 0.74), (0.46, 0.46), Coral, 1.4);
      k.thick((0.54, 0.54), (0.94, 0.26), Coral, 1.4);
    }
    Icon::Symmetric => {
      // "[ ]" brackets
      k.sk(&[(0.36, 0.16), (0.14, 0.16), (0.14, 0.84), (0.36, 0.84)], Coral);
      k.sk(&[(0.64, 0.16), (0.86, 0.16), (0.86, 0.84), (0.64, 0.84)], Coral);
    }
    Icon::Fix => {
      // a padlock
      k.ska((0.5, 0.42), 0.2, 0.0, PI, Coral);
      k.fill(&[(0.22, 0.44), (0.78, 0.44), (0.78, 0.9), (0.22, 0.9)], Coral);
      k.circle((0.5, 0.66), 0.06, Some(White), None);
    }
    Icon::Finish => {
      k.circle((0.5, 0.5), 0.46, Some(Green), None);
      k.p.add(Shape::line(k.pts(&[(0.27, 0.52), (0.44, 0.68), (0.74, 0.34)]), Stroke::new(w * 2.2, k.col(White))));
    }
    Icon::DataPanel => {
      for i in 0..3 {
        for j in 0..3 {
          let (x, y) = (0.12 + i as f32 * 0.29, 0.12 + j as f32 * 0.29);
          k.fill(&[(x, y), (x + 0.18, y), (x + 0.18, y + 0.18), (x, y + 0.18)], Ink);
        }
      }
    }
    Icon::File => {
      k.solid(&[(0.18, 0.06), (0.62, 0.06), (0.82, 0.26), (0.82, 0.94), (0.18, 0.94)], White);
      k.path(&[(0.62, 0.06), (0.62, 0.26), (0.82, 0.26)], Ink);
      k.line((0.3, 0.5), (0.7, 0.5), Ink);
      k.line((0.3, 0.66), (0.7, 0.66), Ink);
      k.line((0.3, 0.82), (0.58, 0.82), Ink);
    }
    Icon::Save => {
      k.solid(&[(0.1, 0.1), (0.76, 0.1), (0.9, 0.24), (0.9, 0.9), (0.1, 0.9)], Ink);
      k.fill(&[(0.26, 0.1), (0.66, 0.1), (0.66, 0.36), (0.26, 0.36)], White);
      k.fill(&[(0.24, 0.56), (0.76, 0.56), (0.76, 0.9), (0.24, 0.9)], White);
    }
    Icon::Undo => {
      k.p
        .add(Shape::line(k.arc_pts((0.52, 0.58), 0.32, 0.32, PI * 0.95, -PI * 0.25), Stroke::new(w * 1.2, k.col(Ink))));
      k.arrow_head((0.18, 0.62), (-0.1, 1.0), 0.2, Ink);
    }
    Icon::Redo => {
      k.p.add(Shape::line(k.arc_pts((0.48, 0.58), 0.32, 0.32, PI * 0.05, PI * 1.25), Stroke::new(w * 1.2, k.col(Ink))));
      k.arrow_head((0.82, 0.62), (0.1, 1.0), 0.2, Ink);
    }
    Icon::Plus => {
      k.thick((0.5, 0.15), (0.5, 0.85), Ink, 1.2);
      k.thick((0.15, 0.5), (0.85, 0.5), Ink, 1.2);
    }
    Icon::Close => {
      k.line((0.25, 0.25), (0.75, 0.75), Ink);
      k.line((0.75, 0.25), (0.25, 0.75), Ink);
    }
    Icon::Bell => {
      k.p.add(Shape::line(k.arc_pts((0.5, 0.45), 0.26, 0.26, 0.0, PI), Stroke::new(w, k.col(Ink))));
      k.path(&[(0.24, 0.45), (0.24, 0.66), (0.12, 0.78), (0.88, 0.78), (0.76, 0.66), (0.76, 0.45)], Ink);
      k.arc((0.5, 0.82), 0.1, PI, TAU, Ink);
    }
    Icon::Help => {
      k.circle((0.5, 0.5), 0.42, None, Some(Ink));
      k.text((0.5, 0.52), "?", 0.6, Ink);
    }
    Icon::Folder => {
      let (back, front) = if mono.is_some() { (Fill, Light) } else { (Dark, Fill) };
      k.solid(&[(0.06, 0.2), (0.4, 0.2), (0.48, 0.3), (0.94, 0.3), (0.94, 0.86), (0.06, 0.86)], back);
      k.solid(&[(0.06, 0.4), (0.94, 0.4), (0.94, 0.86), (0.06, 0.86)], front);
    }
    Icon::Gear => {
      for i in 0..8 {
        let t = TAU * i as f32 / 8.0;
        k.thick((0.5 + t.cos() * 0.26, 0.5 + t.sin() * 0.26), (0.5 + t.cos() * 0.44, 0.5 + t.sin() * 0.44), Ink, 2.2);
      }
      k.circle((0.5, 0.5), 0.3, Some(Fill), Some(Ink));
      k.circle((0.5, 0.5), 0.12, Some(White), Some(Ink));
    }
    Icon::Component => {
      k.block(0.1, 0.38, 0.66, 0.9, 0.26, White, White, Light);
      k.circle((0.38, 0.64), 0.1, Some(Blue), None);
    }
    Icon::Body => k.block(0.1, 0.38, 0.66, 0.9, 0.26, Light, Fill, Dark),
    Icon::MeshCenter => {
      k.block(0.22, 0.4, 0.66, 0.84, 0.18, BlueLight, Blue, BlueDark);
      k.line((0.5, 0.04), (0.5, 0.96), Ink);
      k.line((0.04, 0.6), (0.96, 0.6), Ink);
    }
    Icon::MoveToGround => {
      k.block(0.24, 0.2, 0.64, 0.56, 0.16, BlueLight, Blue, BlueDark);
      k.thick((0.44, 0.6), (0.44, 0.78), Ink, 1.3);
      k.arrow_head((0.44, 0.84), (0.0, 1.0), 0.12, Ink);
      k.thick((0.04, 0.94), (0.96, 0.94), Ink, 1.4);
    }
    Icon::MeshBody | Icon::MeshFeature => {
      // a yellow block cut into triangles on every face
      let (x0, y0, x1, y1, d) =
        if icon == Icon::MeshBody { (0.1, 0.38, 0.66, 0.9, 0.26) } else { (0.06, 0.4, 0.6, 0.94, 0.3) };
      k.block(x0, y0, x1, y1, d, BlueLight, Blue, BlueDark);
      k.line((x0, y1), (x1, y0), BlueDark);
      k.line((x0, y0), (x1 + d, y0 - d * 0.7), BlueDark);
      k.line((x1, y1), (x1 + d, y0 - d * 0.7), BlueDark);
    }
    Icon::SurfaceBody => {
      // a thin sheet curving up at its back, drawn in strips so each fill is convex
      let near = |t: f32| (0.06 + 0.6 * t, 0.86 - 0.22 * (t * PI).sin());
      let far = |t: f32| (0.34 + 0.6 * t, 0.5 - 0.22 * (t * PI).sin());
      let n = 8;
      for i in 0..n {
        let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        k.fill(&[near(a), near(b), far(b), far(a)], Blue);
      }
      let mut outline: Vec<(f32, f32)> = (0..=n).map(|i| near(i as f32 / n as f32)).collect();
      outline.extend((0..=n).rev().map(|i| far(i as f32 / n as f32)));
      k.p.add(Shape::closed_line(k.pts(&outline), Stroke::new(w * 0.8, k.edge(Blue))));
    }
    Icon::SketchSmall => {
      k.solid(&[(0.08, 0.2), (0.92, 0.2), (0.92, 0.84), (0.08, 0.84)], White);
      k.path(&[(0.22, 0.7), (0.22, 0.36), (0.54, 0.36)], Teal);
      k.circle((0.64, 0.6), 0.12, None, Some(Teal));
    }
    Icon::Eye => {
      k.ellipse((0.5, 0.5), 0.46, 0.28, Some(White), Ink);
      k.circle((0.5, 0.5), 0.16, Some(Ink), None);
    }
    Icon::EyeOff => {
      k.ellipse((0.5, 0.5), 0.46, 0.28, None, Fill);
      k.line((0.14, 0.86), (0.86, 0.14), Ink);
    }
    Icon::Origin => {
      k.line((0.3, 0.75), (0.95, 0.75), Red);
      k.line((0.3, 0.75), (0.3, 0.08), Blue);
      k.line((0.3, 0.75), (0.05, 0.95), Green);
      k.dot((0.3, 0.75), Ink);
    }
    Icon::Views => {
      k.solid(&[(0.08, 0.24), (0.7, 0.24), (0.7, 0.8), (0.08, 0.8)], White);
      k.fill(&[(0.7, 0.42), (0.94, 0.26), (0.94, 0.78), (0.7, 0.62)], Ink);
    }
    Icon::Orbit => {
      k.ellipse((0.5, 0.5), 0.42, 0.18, None, Ink);
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.18, 0.42, 0.2, PI - 0.2), Stroke::new(w, k.col(Ink))));
      k.circle((0.5, 0.5), 0.12, Some(Blue), None);
      k.arrow_head((0.9, 0.42), (0.2, -1.0), 0.14, Ink);
    }
    Icon::LookAt => {
      k.solid(&[(0.1, 0.62), (0.32, 0.36), (0.92, 0.36), (0.7, 0.62)], Light);
      k.thick((0.5, 0.06), (0.5, 0.4), Blue, 1.2);
      k.arrow_head((0.5, 0.5), (0.0, 1.0), 0.16, Blue);
    }
    Icon::Pan => {
      for (tip, dir) in
        [((0.5, 0.04), (0.0, -1.0)), ((0.5, 0.96), (0.0, 1.0)), ((0.04, 0.5), (-1.0, 0.0)), ((0.96, 0.5), (1.0, 0.0))]
      {
        k.line((0.5, 0.5), (tip.0 - dir.0 * 0.1, tip.1 - dir.1 * 0.1), Ink);
        k.arrow_head(tip, dir, 0.16, Ink);
      }
    }
    Icon::Zoom => {
      k.circle((0.42, 0.42), 0.3, Some(White), Some(Ink));
      k.thick((0.64, 0.64), (0.92, 0.92), Ink, 2.0);
      k.line((0.28, 0.42), (0.56, 0.42), Ink);
      k.line((0.42, 0.28), (0.42, 0.56), Ink);
    }
    Icon::Fit => {
      k.circle((0.42, 0.42), 0.3, Some(White), Some(Ink));
      k.thick((0.64, 0.64), (0.92, 0.92), Ink, 2.0);
      k.solid(&[(0.3, 0.3), (0.54, 0.3), (0.54, 0.54), (0.3, 0.54)], Blue);
    }
    Icon::Display => {
      k.solid(&[(0.06, 0.12), (0.94, 0.12), (0.94, 0.72), (0.06, 0.72)], White);
      k.line((0.5, 0.72), (0.5, 0.88), Ink);
      k.line((0.3, 0.9), (0.7, 0.9), Ink);
      k.block(0.32, 0.4, 0.58, 0.64, 0.12, Light, Blue, Fill);
    }
    Icon::Grid => {
      for i in 0..4 {
        let t = 0.1 + i as f32 * 0.267;
        k.line((t, 0.1), (t, 0.9), Ink);
        k.line((0.1, t), (0.9, t), Ink);
      }
    }
    Icon::Viewports => {
      k.solid(&[(0.08, 0.12), (0.92, 0.12), (0.92, 0.88), (0.08, 0.88)], White);
      k.line((0.5, 0.12), (0.5, 0.88), Ink);
      k.line((0.08, 0.5), (0.92, 0.5), Ink);
    }
    Icon::ToStart => {
      k.thick((0.22, 0.2), (0.22, 0.8), Ink, 1.4);
      k.fill(&[(0.3, 0.5), (0.78, 0.2), (0.78, 0.8)], Ink);
    }
    Icon::StepBack => {
      k.fill(&[(0.22, 0.5), (0.72, 0.2), (0.72, 0.8)], Ink);
    }
    Icon::Play => {
      k.fill(&[(0.28, 0.2), (0.28, 0.8), (0.8, 0.5)], Ink);
    }
    Icon::StepForward => {
      k.fill(&[(0.28, 0.2), (0.28, 0.8), (0.78, 0.5)], Ink);
    }
    Icon::ToEnd => {
      k.fill(&[(0.22, 0.2), (0.22, 0.8), (0.7, 0.5)], Ink);
      k.thick((0.78, 0.2), (0.78, 0.8), Ink, 1.4);
    }
    Icon::PressPull => {
      k.block(0.08, 0.36, 0.5, 0.92, 0.22, BlueLight, Blue, BlueDark);
      k.thick((0.62, 0.62), (0.88, 0.62), Ink, 1.2);
      k.arrow_head((0.98, 0.62), (1.0, 0.0), 0.16, Ink);
    }
    Icon::Draft => {
      k.solid(&[(0.24, 0.1), (0.76, 0.1), (0.92, 0.9), (0.08, 0.9)], Fill);
      k.solid(&[(0.24, 0.1), (0.4, 0.1), (0.3, 0.9), (0.08, 0.9)], Blue);
    }
    Icon::Scale => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.solid(&[(0.06, 0.06), (0.52, 0.06), (0.52, 0.52), (0.06, 0.52)], Blue);
    }
    Icon::OffsetFace => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.solid(&[(0.24, 0.24), (0.76, 0.24), (0.76, 0.76), (0.24, 0.76)], BlueLight);
    }
    Icon::ReplaceFace => {
      k.block(0.1, 0.42, 0.66, 0.92, 0.24, Blue, White, Light);
      k.thick((0.42, 0.62), (0.42, 0.22), Ink, 1.2);
      k.arrow_head((0.42, 0.12), (0.0, -1.0), 0.16, Ink);
    }
    Icon::SplitFace => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.44), (0.08, 0.44)], Blue);
      k.solid(&[(0.08, 0.54), (0.92, 0.54), (0.92, 0.9), (0.08, 0.9)], White);
    }
    Icon::SplitBody => {
      k.block(0.08, 0.62, 0.74, 0.92, 0.2, Light, Fill, Dark);
      k.block(0.08, 0.26, 0.74, 0.5, 0.2, BlueLight, Blue, BlueDark);
    }
    Icon::SilhouetteSplit => {
      k.circle((0.5, 0.5), 0.42, Some(Blue), None);
      k.p.add(Shape::convex_polygon(k.arc_pts((0.5, 0.5), 0.42, 0.42, PI, TAU), k.col(Light), Stroke::NONE));
      k.ellipse((0.5, 0.5), 0.42, 0.12, None, Ink);
    }
    Icon::Align => {
      k.thick((0.1, 0.06), (0.1, 0.94), Ink, 1.2);
      k.solid(&[(0.14, 0.18), (0.6, 0.18), (0.6, 0.42), (0.14, 0.42)], Blue);
      k.solid(&[(0.14, 0.56), (0.9, 0.56), (0.9, 0.8), (0.14, 0.8)], BlueLight);
    }
    Icon::RemoveBody => {
      k.circle((0.76, 0.5), 0.17, Some(Blue), None);
      k.thick((0.6, 0.5), (0.2, 0.5), Blue, 2.4);
      k.arrow_head((0.04, 0.5), (-1.0, 0.0), 0.2, Blue);
    }
    Icon::RemoveFeatures => {
      k.block(0.06, 0.42, 0.62, 0.94, 0.24, Light, Fill, Dark);
      k.circle((0.34, 0.28), 0.1, Some(Blue), Some(Ink));
      k.thick((0.62, 0.06), (0.94, 0.38), Red, 1.6);
      k.thick((0.94, 0.06), (0.62, 0.38), Red, 1.6);
    }
    Icon::RemoveFaces => {
      k.block(0.06, 0.42, 0.62, 0.94, 0.24, BlueLight, Light, Dark);
      k.thick((0.62, 0.06), (0.94, 0.38), Red, 1.6);
      k.thick((0.94, 0.06), (0.62, 0.38), Red, 1.6);
    }
    Icon::ReplacePrimitives => {
      k.circle((0.26, 0.5), 0.2, Some(Fill), Some(Ink));
      k.thick((0.48, 0.5), (0.58, 0.5), Ink, 1.2);
      k.arrow_head((0.64, 0.5), (1.0, 0.0), 0.12, Ink);
      k.block(0.66, 0.42, 0.9, 0.74, 0.1, BlueLight, Blue, BlueDark);
    }
    Icon::PhysMaterial => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      for (x, y) in [(0.32, 0.32), (0.68, 0.32), (0.5, 0.5), (0.32, 0.68), (0.68, 0.68)] {
        k.fill(&[(x - 0.09, y - 0.09), (x + 0.09, y - 0.09), (x + 0.09, y + 0.09), (x - 0.09, y + 0.09)], Ink);
      }
    }
    Icon::Appearance => {
      let cols = [(230, 60, 50), (245, 160, 40), (240, 220, 60), (80, 180, 70), (50, 130, 220), (150, 90, 200)];
      for (i, c) in cols.iter().enumerate() {
        let a0 = TAU * i as f32 / 6.0;
        let mut v = vec![k.u(0.5, 0.5)];
        v.extend(k.arc_pts((0.5, 0.5), 0.44, 0.44, a0, a0 + TAU / 6.0));
        let col = match mono {
          Some(m) => m.gamma_multiply(0.3 + 0.1 * i as f32),
          None => Color32::from_rgb(c.0, c.1, c.2),
        };
        k.p.add(Shape::convex_polygon(v, col, Stroke::NONE));
      }
    }
    Icon::ManageMaterials | Icon::Bom | Icon::ConfigTable => {
      k.solid(&[(0.08, 0.12), (0.92, 0.12), (0.92, 0.9), (0.08, 0.9)], White);
      for y in [0.32, 0.51, 0.7] {
        k.line((0.08, y), (0.92, y), Ink);
      }
      k.line((0.36, 0.12), (0.36, 0.9), Ink);
      if icon == Icon::Bom {
        k.fill(&[(0.0, 0.0), (0.22, 0.0), (0.22, 0.2), (0.0, 0.2)], Blue);
      }
      if icon == Icon::ConfigTable {
        k.circle((0.76, 0.24), 0.2, Some(White), Some(Ink));
        k.path(&[(0.8, 0.1), (0.72, 0.26), (0.8, 0.26), (0.72, 0.4)], Blue);
      }
    }
    Icon::ComputeAll => {
      k.solid(&[(0.3, 0.04), (0.7, 0.04), (0.7, 0.96), (0.3, 0.96)], Ink);
      k.circle((0.5, 0.22), 0.1, Some(Red), None);
      k.circle((0.5, 0.5), 0.1, Some(Orange), None);
      k.circle((0.5, 0.78), 0.1, Some(Green), None);
    }
    Icon::CreateForm => {
      k.block(0.06, 0.36, 0.6, 0.9, 0.24, PurpleLight, Purple, Purple);
      k.thick((0.78, 0.56), (0.78, 0.98), Green, 2.2);
      k.thick((0.57, 0.77), (0.99, 0.77), Green, 2.2);
    }
    Icon::Derive => {
      k.block(0.22, 0.36, 0.78, 0.92, 0.18, White, Light, Fill);
      k.thick((0.02, 0.64), (0.4, 0.64), Ink, 1.4);
      k.arrow_head((0.52, 0.64), (1.0, 0.0), 0.16, Ink);
    }
    Icon::Sweep => {
      k.band(k.arc_pts((0.12, 0.9), 0.7, 0.7, 0.0, FRAC_PI_2 * 0.9), 5.0);
      k.solid(&[(0.04, 0.08), (0.3, 0.08), (0.3, 0.34), (0.04, 0.34)], White);
    }
    Icon::Loft => {
      k.solid(&[(0.3, 0.2), (0.7, 0.2), (0.92, 0.86), (0.08, 0.86)], Blue);
      k.ellipse((0.5, 0.2), 0.2, 0.08, Some(BlueLight), Blue);
      k.circle((0.24, 0.8), 0.12, Some(White), Some(Ink));
    }
    Icon::Rib => {
      k.path(&[(0.1, 0.06), (0.1, 0.9), (0.94, 0.9)], Ink);
      k.solid(&[(0.14, 0.2), (0.14, 0.86), (0.8, 0.86)], Blue);
    }
    Icon::Web => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.thick((0.5, 0.1), (0.5, 0.9), Blue, 1.8);
      k.thick((0.1, 0.5), (0.9, 0.5), Blue, 1.8);
    }
    Icon::Emboss => {
      k.solid(&[(0.04, 0.3), (0.96, 0.3), (0.96, 0.92), (0.04, 0.92)], Light);
      k.text((0.5, 0.56), "A", 0.62, Blue);
    }
    Icon::Hole => {
      k.block(0.1, 0.4, 0.66, 0.92, 0.24, Light, Fill, Dark);
      k.ellipse((0.47, 0.31), 0.13, 0.06, Some(White), Ink);
      k.circle((0.38, 0.66), 0.13, Some(BlueLight), Some(Blue));
    }
    Icon::Thread => {
      k.solid(&[(0.24, 0.08), (0.76, 0.08), (0.76, 0.92), (0.24, 0.92)], Light);
      for i in 0..5 {
        let y = 0.16 + i as f32 * 0.16;
        k.thick((0.2, y + 0.08), (0.8, y), Blue, 1.2);
      }
    }
    Icon::Coil => {
      let mut pts = Vec::new();
      for i in 0..=8 {
        let y = 0.08 + i as f32 * 0.105;
        pts.push((if i % 2 == 0 { 0.2 } else { 0.8 }, y));
      }
      k.p.add(Shape::line(k.pts(&pts), Stroke::new(w * 2.0, k.col(Blue))));
    }
    Icon::Pipe => {
      k.band(k.pts(&[(0.12, 0.88), (0.88, 0.12)]), 5.4);
      k.line((0.12, 0.88), (0.88, 0.12), Blue);
    }
    Icon::Thicken => {
      k.solid(&[(0.04, 0.6), (0.4, 0.2), (0.96, 0.2), (0.6, 0.6)], BlueLight);
      k.solid(&[(0.04, 0.6), (0.6, 0.6), (0.6, 0.8), (0.04, 0.8)], Blue);
      k.solid(&[(0.6, 0.6), (0.96, 0.2), (0.96, 0.4), (0.6, 0.8)], BlueDark);
    }
    Icon::BoundaryFill => {
      k.solid(&[(0.06, 0.3), (0.6, 0.3), (0.6, 0.94), (0.06, 0.94)], Orange);
      k.solid(&[(0.4, 0.06), (0.94, 0.06), (0.94, 0.6), (0.4, 0.6)], Blue);
    }
    Icon::BaseFeature => {
      k.block(0.06, 0.36, 0.6, 0.9, 0.24, White, Light, Fill);
      k.thick((0.78, 0.56), (0.78, 0.98), Green, 2.2);
      k.thick((0.57, 0.77), (0.99, 0.77), Green, 2.2);
    }
    Icon::JointOrigin => {
      k.circle((0.5, 0.5), 0.36, Some(Light), Some(Ink));
      k.circle((0.5, 0.5), 0.12, Some(Blue), None);
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.46, 0.46, 0.3, 1.5), Stroke::new(w * 1.2, k.col(Ink))));
    }
    Icon::Configure | Icon::ConfigRules => {
      if icon == Icon::ConfigRules {
        k.solid(&[(0.04, 0.3), (0.84, 0.3), (0.84, 0.92), (0.04, 0.92)], BlueLight);
      }
      let (c, rad) = if icon == Icon::Configure { ((0.5, 0.5), 0.42) } else { ((0.74, 0.26), 0.24) };
      k.circle(c, rad, Some(White), Some(Ink));
      let s = rad / 0.44;
      k.p.add(Shape::line(
        k.pts(&[
          (c.0 + 0.08 * s, c.1 - 0.3 * s),
          (c.0 - 0.1 * s, c.1 + 0.04 * s),
          (c.0 + 0.1 * s, c.1 + 0.0),
          (c.0 - 0.06 * s, c.1 + 0.3 * s),
        ]),
        Stroke::new(w * 1.6, k.col(Blue)),
      ));
    }
    Icon::Ucs => {
      k.thick((0.2, 0.8), (0.95, 0.8), Red, 1.2);
      k.thick((0.2, 0.8), (0.2, 0.05), Blue, 1.2);
      k.thick((0.2, 0.8), (0.55, 0.45), Green, 1.2);
    }
    Icon::PlaneAlt => {
      k.solid(&[(0.04, 0.78), (0.3, 0.26), (0.96, 0.26), (0.7, 0.78)], Amber);
      k.ska((0.18, 0.86), 0.3, 0.2, 1.2, Slate);
    }
    Icon::Axis => {
      k.ellipse((0.4, 0.62), 0.2, 0.08, Some(Light), Slate);
      k.thick((0.08, 0.96), (0.92, 0.04), Amber, 2.0);
    }
    Icon::CPoint => {
      k.sk(&[(0.5, 0.06), (0.5, 0.94)], Slate);
      k.sk(&[(0.06, 0.5), (0.94, 0.5)], Slate);
      k.circle((0.5, 0.5), 0.2, Some(Amber), Some(Amber));
    }
    Icon::PlaneAngle => {
      k.skc(&[(0.04, 0.94), (0.24, 0.72), (0.96, 0.72), (0.76, 0.94)], Slate);
      k.solid(&[(0.14, 0.83), (0.3, 0.12), (0.98, 0.12), (0.86, 0.83)], Amber);
      k.thick((0.14, 0.83), (0.86, 0.83), Slate, 1.4);
      k.ska((0.14, 0.83), 0.28, 0.0, 1.35, Slate);
    }
    Icon::PlaneTangent => {
      k.circle((0.5, 0.66), 0.27, Some(Light), Some(Slate));
      k.solid(&[(0.04, 0.39), (0.24, 0.17), (0.96, 0.17), (0.76, 0.39)], Amber);
      k.dot((0.5, 0.39), Slate);
    }
    Icon::Midplane => {
      k.skc(&[(0.04, 0.3), (0.24, 0.06), (0.96, 0.06), (0.76, 0.3)], Slate);
      k.skc(&[(0.04, 0.94), (0.24, 0.7), (0.96, 0.7), (0.76, 0.94)], Slate);
      k.solid(&[(0.04, 0.62), (0.24, 0.38), (0.96, 0.38), (0.76, 0.62)], Amber);
    }
    Icon::PlanePerpendicular => {
      k.skc(&[(0.04, 0.94), (0.28, 0.68), (0.96, 0.68), (0.72, 0.94)], Slate);
      k.solid(&[(0.22, 0.81), (0.22, 0.08), (0.78, 0.08), (0.78, 0.81)], Amber);
      k.sk(&[(0.78, 0.69), (0.88, 0.69), (0.88, 0.8)], Slate);
    }
    Icon::PlaneTwoEdges => {
      k.solid(&[(0.04, 0.78), (0.3, 0.26), (0.96, 0.26), (0.7, 0.78)], Amber);
      k.thick((0.0, 0.9), (0.36, 0.16), Slate, 1.4);
      k.thick((0.62, 0.9), (0.98, 0.16), Slate, 1.4);
    }
    Icon::PlaneThreePoints => {
      k.solid(&[(0.04, 0.78), (0.3, 0.26), (0.96, 0.26), (0.7, 0.78)], Amber);
      for c in [(0.26, 0.64), (0.5, 0.38), (0.74, 0.62)] {
        k.dot(c, Slate);
      }
    }
    Icon::PlaneAlongPath => {
      k.sk(&[(0.04, 0.92), (0.26, 0.74), (0.48, 0.6), (0.7, 0.42), (0.86, 0.22), (0.96, 0.04)], Slate);
      k.solid(&[(0.36, 0.22), (0.58, 0.06), (0.82, 0.78), (0.6, 0.94)], Amber);
      k.dot((0.59, 0.5), Slate);
    }
    Icon::AxisCylinder => {
      k.solid(&[(0.24, 0.26), (0.76, 0.26), (0.76, 0.78), (0.24, 0.78)], Light);
      k.ellipse((0.5, 0.78), 0.26, 0.09, Some(Light), Slate);
      k.ellipse((0.5, 0.26), 0.26, 0.09, Some(Light), Slate);
      k.thick((0.5, 0.02), (0.5, 0.98), Amber, 2.0);
    }
    Icon::AxisNormal => {
      k.solid(&[(0.04, 0.92), (0.28, 0.62), (0.96, 0.62), (0.72, 0.92)], Light);
      k.thick((0.5, 0.77), (0.5, 0.04), Amber, 2.0);
      k.sk(&[(0.5, 0.62), (0.62, 0.62), (0.62, 0.72)], Slate);
    }
    Icon::AxisTwoPlanes => {
      k.solid(&[(0.17, 0.6), (0.17, 0.1), (0.83, 0.1), (0.83, 0.6)], Light);
      k.solid(&[(0.04, 0.76), (0.3, 0.44), (0.96, 0.44), (0.7, 0.76)], Light);
      k.thick((0.02, 0.6), (0.98, 0.6), Amber, 2.0);
    }
    Icon::AxisTwoPoints => {
      k.thick((0.06, 0.94), (0.94, 0.06), Amber, 2.0);
      k.circle((0.27, 0.73), 0.09, Some(White), Some(Slate));
      k.circle((0.73, 0.27), 0.09, Some(White), Some(Slate));
    }
    Icon::AxisEdge => {
      k.block(0.16, 0.42, 0.7, 0.92, 0.22, Light, Light, Dark);
      k.thick((0.02, 0.42), (0.98, 0.42), Amber, 2.0);
    }
    Icon::PointVertex => {
      k.block(0.1, 0.42, 0.66, 0.92, 0.24, Light, Light, Dark);
      k.circle((0.66, 0.42), 0.11, Some(Amber), Some(Amber));
    }
    Icon::PointTwoEdges => {
      k.thick((0.08, 0.88), (0.92, 0.18), Slate, 1.3);
      k.thick((0.08, 0.22), (0.92, 0.8), Slate, 1.3);
      k.circle((0.5, 0.53), 0.11, Some(Amber), Some(Amber));
    }
    Icon::PointThreePlanes => {
      k.solid(&[(0.4, 0.08), (0.94, 0.08), (0.94, 0.6), (0.4, 0.6)], Light);
      k.solid(&[(0.4, 0.08), (0.4, 0.6), (0.1, 0.86), (0.1, 0.34)], Dark);
      k.solid(&[(0.4, 0.6), (0.94, 0.6), (0.64, 0.86), (0.1, 0.86)], Light);
      k.circle((0.4, 0.6), 0.11, Some(Amber), Some(Amber));
    }
    Icon::PointCenter => {
      k.circle((0.5, 0.5), 0.4, None, Some(Slate));
      k.dashed((0.5, 0.16), (0.5, 0.84), Slate);
      k.dashed((0.16, 0.5), (0.84, 0.5), Slate);
      k.circle((0.5, 0.5), 0.11, Some(Amber), Some(Amber));
    }
    Icon::PointEdgePlane => {
      k.thick((0.5, 0.6), (0.5, 0.98), Slate, 1.3);
      k.solid(&[(0.04, 0.76), (0.3, 0.44), (0.96, 0.44), (0.7, 0.76)], Light);
      k.thick((0.5, 0.02), (0.5, 0.6), Slate, 1.3);
      k.circle((0.5, 0.6), 0.11, Some(Amber), Some(Amber));
    }
    Icon::PointAlongPath => {
      k.sk(&[(0.04, 0.84), (0.2, 0.56), (0.38, 0.42), (0.56, 0.5), (0.74, 0.46), (0.96, 0.16)], Slate);
      k.circle((0.56, 0.5), 0.11, Some(Amber), Some(Amber));
    }
    Icon::Interference => {
      k.solid(&[(0.06, 0.06), (0.62, 0.06), (0.62, 0.62), (0.06, 0.62)], White);
      k.solid(&[(0.38, 0.38), (0.94, 0.38), (0.94, 0.94), (0.38, 0.94)], White);
      k.fill(&[(0.38, 0.38), (0.62, 0.38), (0.62, 0.62), (0.38, 0.62)], Red);
    }
    Icon::Section => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.fill(&[(0.06, 0.06), (0.5, 0.06), (0.5, 0.94), (0.06, 0.94)], BlueLight);
      k.thick((0.5, 0.02), (0.5, 0.98), Blue, 1.4);
    }
    Icon::CurvComb => {
      k.arc((0.5, 0.95), 0.42, 0.3, PI - 0.3, Ink);
      for i in 0..7 {
        let a = 0.4 + i as f32 * 0.39;
        let (c, sn) = (a.cos(), a.sin());
        k.line((0.5 + c * 0.42, 0.95 - sn * 0.42), (0.5 + c * 0.62, 0.95 - sn * 0.62), Blue);
      }
    }
    Icon::CurvMap | Icon::DraftAnalysis | Icon::FaceGroups | Icon::GenFaceGroups | Icon::CombineFaceGroups => {
      let bands = [Blue, BlueLight, BlueDark, Fill];
      if icon == Icon::CurvMap {
        for (i, t) in bands.iter().enumerate() {
          let y = 0.08 + i as f32 * 0.21;
          k.fill(&[(0.08, y), (0.92, y), (0.92, y + 0.21), (0.08, y + 0.21)], *t);
        }
      } else if icon == Icon::DraftAnalysis {
        for (i, t) in bands.iter().enumerate() {
          let y = 0.1 + i as f32 * 0.2;
          let half = (0.42f32.powi(2) - (y + 0.1 - 0.5).powi(2)).max(0.02).sqrt();
          k.fill(&[(0.5 - half, y), (0.5 + half, y), (0.5 + half, y + 0.2), (0.5 - half, y + 0.2)], *t);
        }
      } else {
        k.block(0.08, 0.38, 0.62, 0.92, 0.26, BlueLight, Blue, BlueDark);
        if icon == Icon::CombineFaceGroups {
          k.fill(&[(0.08, 0.38), (0.62, 0.38), (0.62, 0.92), (0.08, 0.92)], Blue);
        }
      }
    }
    Icon::EnvMap => {
      k.circle((0.5, 0.5), 0.42, Some(Blue), None);
      k.circle((0.38, 0.36), 0.14, Some(BlueLight), None);
    }
    Icon::Isocurve => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      for i in 1..4 {
        let t = i as f32 * 0.22 + 0.06;
        k.line((t, 0.06), (t + 0.06, 0.94), Ink);
        k.line((0.06, t), (0.94, t + 0.04), Ink);
      }
    }
    Icon::Zebra => {
      for i in 0..3 {
        k.p.add(Shape::line(
          k.arc_pts((0.0, 0.5), 0.3 + i as f32 * 0.24, 0.42, -1.2, 1.2),
          Stroke::new(w * 2.2, k.col(Ink)),
        ));
      }
    }
    Icon::Accessibility => {
      k.circle((0.4, 0.62), 0.32, Some(Green), None);
      k.circle((0.74, 0.28), 0.16, None, Some(Ink));
    }
    Icon::CenterOfMass => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      let mut v = vec![k.u(0.5, 0.5)];
      v.extend(k.arc_pts((0.5, 0.5), 0.42, 0.42, 0.0, FRAC_PI_2));
      k.p.add(Shape::convex_polygon(v, k.col(Ink), Stroke::NONE));
      let mut v = vec![k.u(0.5, 0.5)];
      v.extend(k.arc_pts((0.5, 0.5), 0.42, 0.42, PI, PI + FRAC_PI_2));
      k.p.add(Shape::convex_polygon(v, k.col(Ink), Stroke::NONE));
    }
    Icon::MinRadius => {
      k.solid(&[(0.08, 0.08), (0.92, 0.08), (0.92, 0.92), (0.08, 0.92)], Light);
      k.arc((0.92, 0.08), 0.44, PI, PI + FRAC_PI_2, Red);
    }
    Icon::Navigator => {
      for (x, y) in [(0.08, 0.08), (0.6, 0.08), (0.08, 0.6), (0.6, 0.6)] {
        k.closed(&[(x, y), (x + 0.3, y), (x + 0.3, y + 0.3), (x, y + 0.3)], Ink);
      }
      k.line((0.38, 0.23), (0.6, 0.23), Ink);
      k.line((0.23, 0.38), (0.23, 0.6), Ink);
    }
    Icon::ComponentColors => {
      k.circle((0.34, 0.34), 0.22, Some(Purple), None);
      k.circle((0.68, 0.4), 0.2, Some(Blue), None);
      k.circle((0.44, 0.7), 0.22, Some(Orange), None);
    }
    Icon::History => {
      k.solid(&[(0.14, 0.06), (0.74, 0.06), (0.74, 0.94), (0.14, 0.94)], White);
      k.circle((0.66, 0.7), 0.24, Some(White), Some(Blue));
      k.path(&[(0.66, 0.56), (0.66, 0.7), (0.76, 0.76)], Blue);
    }
    Icon::Sustainability => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      k.text((0.5, 0.5), "CO2", 0.3, Ink);
    }
    Icon::Decal | Icon::Canvas => {
      if icon == Icon::Decal {
        k.solid(&[(0.04, 0.04), (0.64, 0.04), (0.64, 0.64), (0.04, 0.64)], Blue);
        k.solid(&[(0.36, 0.5), (0.96, 0.5), (0.96, 0.96), (0.36, 0.96)], White);
      } else {
        k.solid(&[(0.04, 0.12), (0.96, 0.12), (0.96, 0.88), (0.04, 0.88)], BlueLight);
        k.fill(&[(0.04, 0.88), (0.38, 0.42), (0.7, 0.88)], White);
        k.fill(&[(0.42, 0.88), (0.66, 0.56), (0.96, 0.88)], White);
        k.circle((0.74, 0.32), 0.08, Some(White), None);
      }
    }
    Icon::InsertSvg | Icon::InsertDxf => {
      k.solid(&[(0.16, 0.04), (0.84, 0.04), (0.84, 0.96), (0.16, 0.96)], White);
      k.fill(&[(0.16, 0.62), (0.84, 0.62), (0.84, 0.96), (0.16, 0.96)], Ink);
      k.text((0.5, 0.79), if icon == Icon::InsertSvg { "SVG" } else { "DXF" }, 0.26, White);
      k.thick((0.0, 0.3), (0.42, 0.3), Blue, 1.4);
      k.arrow_head((0.54, 0.3), (1.0, 0.0), 0.16, Blue);
    }
    Icon::InsertMesh
    | Icon::Tessellate
    | Icon::Remesh
    | Icon::Reduce
    | Icon::RemeshAdaptive
    | Icon::RemeshUniform
    | Icon::FillNone
    | Icon::FillUniform
    | Icon::FillMinimal
    | Icon::ConvertFaceted
    | Icon::ConvertPrismatic
    | Icon::ConvertOrganic => mesh::paint(&k, icon),
    Icon::BaseMesh
    | Icon::DirectEdit
    | Icon::Repair
    | Icon::PlaneCut
    | Icon::Smooth
    | Icon::ScaleMesh
    | Icon::MeshSection => {
      k.block(0.08, 0.38, 0.62, 0.92, 0.28, BlueLight, Blue, BlueDark);
      k.line((0.08, 0.38), (0.62, 0.92), BlueDark);
      k.line((0.62, 0.38), (0.08, 0.92), BlueDark);
      match icon {
        Icon::ScaleMesh => k.solid(&[(0.66, 0.04), (0.96, 0.04), (0.96, 0.34), (0.66, 0.34)], Light),
        Icon::BaseMesh | Icon::DirectEdit => {
          for i in 0..4 {
            let a = i as f32 * FRAC_PI_2 * 0.5;
            k.thick(
              (0.8 - a.cos() * 0.16, 0.2 - a.sin() * 0.16),
              (0.8 + a.cos() * 0.16, 0.2 + a.sin() * 0.16),
              Orange,
              1.4,
            );
          }
        }
        Icon::Repair => k.solid(&[(0.04, 0.72), (0.66, 0.72), (0.66, 0.86), (0.04, 0.86)], White),
        Icon::PlaneCut => k.solid(&[(0.0, 0.7), (0.3, 0.2), (0.98, 0.2), (0.68, 0.7)], Orange),
        Icon::Smooth => k.circle((0.74, 0.24), 0.18, Some(BlueLight), Some(Blue)),
        _ => k.ellipse((0.36, 0.65), 0.3, 0.1, None, Blue),
      }
    }
    Icon::InsertComponent | Icon::Manufacturer | Icon::McMaster => {
      k.block(0.18, 0.36, 0.72, 0.92, 0.24, Light, Fill, Dark);
      if icon == Icon::McMaster {
        k.text((0.45, 0.65), "M", 0.4, Ink);
      }
    }
    Icon::Fastener => {
      k.solid(&[(0.1, 0.1), (0.9, 0.1), (0.9, 0.32), (0.1, 0.32)], Fill);
      k.solid(&[(0.38, 0.32), (0.62, 0.32), (0.62, 0.94), (0.38, 0.94)], Fill);
    }
    Icon::DuplicateJoints => {
      for x in [0.08, 0.38, 0.68] {
        k.solid(&[(x, 0.2), (x + 0.22, 0.2), (x + 0.22, 0.86), (x, 0.86)], Fill);
      }
    }
    Icon::AddToAssembly => {
      k.block(0.06, 0.2, 0.36, 0.46, 0.12, White, Light, Fill);
      k.block(0.44, 0.56, 0.84, 0.94, 0.12, Light, Fill, Dark);
      k.thick((0.5, 0.14), (0.62, 0.36), Ink, 1.0);
      k.arrow_head((0.66, 0.44), (0.4, 1.0), 0.14, Ink);
    }
    Icon::WindowSel | Icon::FreeformSel | Icon::PaintSel => {
      match icon {
        Icon::WindowSel => {
          for (a, b) in [
            ((0.06, 0.06), (0.78, 0.06)),
            ((0.06, 0.06), (0.06, 0.7)),
            ((0.78, 0.06), (0.78, 0.4)),
            ((0.06, 0.7), (0.4, 0.7)),
          ] {
            k.dashed(a, b, Ink);
          }
        }
        Icon::FreeformSel => {
          k.p.add(Shape::line(k.arc_pts((0.42, 0.4), 0.36, 0.32, 0.4, TAU - 0.6), Stroke::new(w, k.col(Green))));
        }
        _ => {
          k.block(0.06, 0.4, 0.5, 0.8, 0.16, Green, White, Light);
          k.thick((0.6, 0.62), (0.94, 0.2), Orange, 2.0);
        }
      }
      if icon != Icon::PaintSel {
        k.solid(&[(0.46, 0.34), (0.46, 0.98), (0.62, 0.84), (0.82, 0.84)], White);
      }
    }
    Icon::Patch => {
      // a folded sheet, both halves in the family colour
      k.solid(&[(0.06, 0.16), (0.52, 0.16), (0.52, 0.66), (0.06, 0.66)], Blue);
      k.solid(&[(0.52, 0.16), (0.94, 0.42), (0.94, 0.92), (0.52, 0.66)], BlueLight);
    }
    Icon::Ruled => {
      k.solid(&[(0.06, 0.66), (0.46, 0.2), (0.94, 0.2), (0.54, 0.66)], BlueLight);
      k.solid(&[(0.06, 0.66), (0.54, 0.66), (0.54, 0.86), (0.06, 0.86)], Blue);
    }
    Icon::SurfOffset => {
      k.band(k.arc_pts((0.5, 1.1), 0.5, 0.5, 0.5, PI - 0.5), 3.6);
      k.p.add(Shape::line(k.arc_pts((0.5, 1.1), 0.8, 0.8, 0.6, PI - 0.6), Stroke::new(w * 1.3, k.col(Blue))));
    }
    Icon::Trim => {
      // a sheet with a piece cut away along the dashed line, scissors over it
      k.solid(&[(0.04, 0.3), (0.62, 0.3), (0.62, 0.96), (0.04, 0.96)], Blue);
      k.solid(&[(0.7, 0.3), (0.96, 0.3), (0.96, 0.96), (0.7, 0.96)], BlueLight);
      k.dashed((0.66, 0.24), (0.66, 1.0), Ink);
      k.circle((0.3, 0.2), 0.1, Some(White), Some(Ink));
      k.circle((0.3, 0.52), 0.1, Some(White), Some(Ink));
      k.thick((0.38, 0.24), (0.9, 0.5), Ink, 1.3);
      k.thick((0.38, 0.48), (0.9, 0.2), Ink, 1.3);
    }
    Icon::Untrim | Icon::Unstitch => {
      if icon == Icon::Untrim {
        k.solid(&[(0.08, 0.2), (0.7, 0.2), (0.7, 0.9), (0.08, 0.9)], Blue);
      } else {
        k.solid(&[(0.14, 0.1), (0.62, 0.1), (0.62, 0.9), (0.14, 0.9)], Blue);
        for i in 0..5 {
          let y = 0.2 + i as f32 * 0.15;
          k.line((0.3, y), (0.46, y), White);
        }
      }
      k.thick((0.62, 0.62), (0.96, 0.96), Red, 1.6);
      k.thick((0.96, 0.62), (0.62, 0.96), Red, 1.6);
    }
    Icon::Extend => {
      k.solid(&[(0.08, 0.5), (0.92, 0.5), (0.92, 0.94), (0.08, 0.94)], Blue);
      k.thick((0.5, 0.46), (0.5, 0.16), Ink, 1.4);
      k.arrow_head((0.5, 0.04), (0.0, -1.0), 0.16, Ink);
    }
    Icon::Stitch => {
      k.solid(&[(0.14, 0.06), (0.86, 0.06), (0.86, 0.94), (0.14, 0.94)], Blue);
      k.line((0.5, 0.06), (0.5, 0.94), Ink);
      for i in 0..5 {
        let y = 0.16 + i as f32 * 0.17;
        k.line((0.36, y), (0.64, y), Ink);
      }
    }
    Icon::ReverseNormal => {
      k.solid(&[(0.06, 0.4), (0.94, 0.4), (0.94, 0.94), (0.06, 0.94)], Blue);
      k.thick((0.5, 0.36), (0.5, 0.12), Ink, 1.2);
      k.arrow_head((0.5, 0.02), (0.0, -1.0), 0.14, Ink);
      k.text((0.5, 0.7), "↔", 0.4, White);
    }
    Icon::Separate => {
      k.block(0.02, 0.48, 0.38, 0.92, 0.14, BlueLight, Blue, BlueDark);
      k.block(0.5, 0.3, 0.84, 0.72, 0.14, BlueLight, Blue, BlueDark);
    }
    Icon::MidpointLine => {
      k.sk(&[(0.1, 0.9), (0.9, 0.1)], Slate);
      k.vtx((0.5, 0.5));
      k.vtx((0.9, 0.1));
    }
    Icon::Rect3 => {
      k.skc(&[(0.06, 0.56), (0.44, 0.14), (0.94, 0.48), (0.56, 0.9)], Slate);
      k.vtx((0.06, 0.56));
      k.vtx((0.44, 0.14));
      k.vtx((0.94, 0.48));
    }
    Icon::Circle2P | Icon::Circle3P => {
      k.ska((0.5, 0.5), 0.38, 0.0, TAU, Slate);
      if icon == Icon::Circle2P {
        k.dashed((0.12, 0.5), (0.88, 0.5), Slate);
        k.vtx((0.12, 0.5));
        k.vtx((0.88, 0.5));
      } else {
        k.vtx((0.12, 0.5));
        k.vtx((0.69, 0.17));
        k.vtx((0.69, 0.83));
      }
    }
    Icon::Circle2T | Icon::Circle3T => {
      if icon == Icon::Circle2T {
        k.sk(&[(0.04, 0.24), (0.96, 0.24)], Slate);
        k.sk(&[(0.2, 0.04), (0.2, 0.96)], Slate);
        k.ska((0.5, 0.54), 0.3, 0.0, TAU, Teal);
        k.vtx((0.5, 0.24));
        k.vtx((0.2, 0.54));
      } else {
        k.skc(&[(0.5, 0.04), (0.96, 0.86), (0.04, 0.86)], Slate);
        k.ska((0.5, 0.58), 0.26, 0.0, TAU, Teal);
      }
    }
    Icon::TangentArc => {
      k.sk(&[(0.04, 0.82), (0.44, 0.82)], Slate);
      k.ska((0.44, 0.5), 0.32, -FRAC_PI_2, FRAC_PI_2, Teal);
      k.vtx((0.44, 0.82));
      k.vtx((0.44, 0.18));
    }
    Icon::PolygonCirc | Icon::PolygonEdge => {
      let pts: Vec<(f32, f32)> = (0..6)
        .map(|i| {
          let t = TAU * i as f32 / 6.0;
          (0.5 + t.cos() * 0.42, 0.5 + t.sin() * 0.42)
        })
        .collect();
      k.skc(&pts, Slate);
      if icon == Icon::PolygonCirc {
        k.ska((0.5, 0.5), 0.36, 0.0, TAU, Teal);
        k.vtx((0.5, 0.5));
      } else {
        k.sk(&[pts[1], pts[2]], Teal);
        k.vtx(pts[1]);
        k.vtx(pts[2]);
      }
    }
    Icon::Ellipse => {
      k.p.add(Shape::closed_line(k.arc_pts((0.5, 0.5), 0.42, 0.26, 0.0, TAU), Stroke::new(w * 1.15, k.col(Slate))));
      k.dashed((0.08, 0.5), (0.92, 0.5), Slate);
      k.vtx((0.5, 0.5));
      k.vtx((0.92, 0.5));
    }
    Icon::OverallSlot | Icon::CenterPointSlot => {
      k.ska((0.3, 0.5), 0.22, FRAC_PI_2, 1.5 * PI, Slate);
      k.ska((0.7, 0.5), 0.22, -FRAC_PI_2, FRAC_PI_2, Slate);
      k.sk(&[(0.3, 0.28), (0.7, 0.28)], Slate);
      k.sk(&[(0.3, 0.72), (0.7, 0.72)], Slate);
      if icon == Icon::OverallSlot {
        k.vtx((0.08, 0.5));
        k.vtx((0.92, 0.5));
      } else {
        k.vtx((0.5, 0.5));
        k.vtx((0.7, 0.5));
      }
    }
    Icon::ArcSlot3 | Icon::ArcSlotCenter => {
      k.ska((0.5, 1.0), 0.72, 0.6, PI - 0.6, Slate);
      k.ska((0.5, 1.0), 0.38, 0.6, PI - 0.6, Slate);
      k.ska((0.5, 1.0), 0.55, 0.6, PI - 0.6, Teal);
      k.vtx((0.5 + 0.55 * 0.825, 1.0 - 0.55 * 0.565));
      k.vtx((0.5 - 0.55 * 0.825, 1.0 - 0.55 * 0.565));
      if icon == Icon::ArcSlot3 {
        k.vtx((0.5, 0.45));
      } else {
        k.vtx((0.5, 0.94));
      }
    }
    Icon::SplineFit | Icon::SplineCtrl => {
      let curve: Vec<(f32, f32)> = (0..=20)
        .map(|i| {
          let t = i as f32 / 20.0;
          (0.08 + 0.84 * t, 0.5 - 0.32 * (t * TAU * 0.9).sin())
        })
        .collect();
      k.sk(&curve, Slate);
      if icon == Icon::SplineFit {
        k.vtx(curve[0]);
        k.vtx(curve[7]);
        k.vtx(curve[20]);
      } else {
        let ctrl = [(0.08, 0.5), (0.3, 0.06), (0.62, 0.94), (0.92, 0.5)];
        for i in 0..3 {
          k.dashed(ctrl[i], ctrl[i + 1], Teal);
        }
        for c in ctrl {
          k.vtx(c);
        }
      }
    }
    Icon::Conic => {
      k.dashed((0.08, 0.9), (0.5, 0.12), Slate);
      k.dashed((0.5, 0.12), (0.92, 0.9), Slate);
      let curve: Vec<(f32, f32)> = (0..=16)
        .map(|i| {
          let t = i as f32 / 16.0;
          let x = 0.08 + 0.84 * t;
          (x, 0.9 - 2.6 * (t * (1.0 - t)) * 0.5)
        })
        .collect();
      k.sk(&curve, Teal);
      k.vtx((0.08, 0.9));
      k.vtx((0.92, 0.9));
      k.vtx((0.5, 0.12));
    }
    Icon::TraceImage => {
      // a photo, and the closed outline traced from it
      k.solid(&[(0.04, 0.08), (0.7, 0.08), (0.7, 0.62), (0.04, 0.62)], BlueLight);
      k.fill(&[(0.04, 0.62), (0.24, 0.32), (0.44, 0.62)], White);
      k.circle((0.52, 0.26), 0.07, Some(White), None);
      k.skc(&[(0.3, 0.5), (0.6, 0.4), (0.92, 0.54), (0.96, 0.84), (0.66, 0.96), (0.36, 0.86)], Slate);
      k.circle((0.6, 0.4), 0.06, Some(Teal), None);
      k.circle((0.96, 0.84), 0.06, Some(Teal), None);
    }
    Icon::SkText => {
      k.text((0.42, 0.46), "A", 0.78, Slate);
      k.sk(&[(0.06, 0.92), (0.94, 0.92)], Teal);
    }
    Icon::SkMirror => {
      k.dashed((0.5, 0.04), (0.5, 0.96), Slate);
      k.skc(&[(0.08, 0.84), (0.4, 0.84), (0.4, 0.2)], Slate);
      k.skc(&[(0.92, 0.84), (0.6, 0.84), (0.6, 0.2)], Teal);
    }
    Icon::SkCircPattern => {
      for i in 0..6 {
        let t = TAU * i as f32 / 6.0 - FRAC_PI_2;
        k.ska((0.5 + t.cos() * 0.34, 0.5 + t.sin() * 0.34), 0.1, 0.0, TAU, if i == 0 { Slate } else { Teal });
      }
      k.vtx((0.5, 0.5));
    }
    Icon::SkRectPattern => {
      for (i, (x, y)) in [(0.08, 0.08), (0.56, 0.08), (0.08, 0.56), (0.56, 0.56)].into_iter().enumerate() {
        k.skc(&[(x, y), (x + 0.36, y), (x + 0.36, y + 0.36), (x, y + 0.36)], if i == 0 { Slate } else { Teal });
      }
    }
    Icon::Project | Icon::Intersect | Icon::Include3D | Icon::ProjectSurface => match icon {
      Icon::Project => {
        k.block(0.2, 0.18, 0.62, 0.48, 0.16, Light, Fill, Dark);
        k.skc(&[(0.08, 0.7), (0.62, 0.7), (0.92, 0.94), (0.38, 0.94)], Teal);
        k.dashed((0.42, 0.5), (0.5, 0.8), Slate);
      }
      Icon::Intersect => {
        k.block(0.14, 0.36, 0.62, 0.92, 0.24, Light, Fill, Dark);
        k.sk(&[(0.02, 0.62), (0.98, 0.42)], Teal);
      }
      Icon::Include3D => {
        k.skc(&[(0.1, 0.36), (0.6, 0.36), (0.6, 0.9), (0.1, 0.9)], Slate);
        k.sk(&[(0.1, 0.36), (0.36, 0.1), (0.88, 0.1), (0.6, 0.36)], Slate);
        k.sk(&[(0.88, 0.1), (0.88, 0.64), (0.6, 0.9)], Teal);
      }
      _ => {
        k.p.add(Shape::convex_polygon(k.arc_pts((0.5, 1.2), 0.7, 0.6, 0.6, PI - 0.6), k.col(Light), Stroke::NONE));
        k.ska((0.5, 1.2), 0.62, 0.8, PI - 0.8, Teal);
        k.dashed((0.5, 0.04), (0.5, 0.5), Slate);
      }
    },
    Icon::SkFillet | Icon::SkChamfer => {
      k.sk(&[(0.12, 0.92), (0.12, 0.5)], Slate);
      k.sk(&[(0.5, 0.12), (0.92, 0.12)], Slate);
      k.dashed((0.12, 0.5), (0.12, 0.12), Slate);
      k.dashed((0.12, 0.12), (0.5, 0.12), Slate);
      if icon == Icon::SkFillet {
        k.ska((0.5, 0.5), 0.38, FRAC_PI_2, PI, Coral);
      } else {
        k.sk(&[(0.12, 0.5), (0.5, 0.12)], Coral);
      }
    }
    Icon::BlendCurve => {
      k.ska((0.0, 0.8), 0.28, 0.0, FRAC_PI_2, Slate);
      k.ska((1.0, 0.2), 0.28, PI, 1.5 * PI, Slate);
      let s: Vec<(f32, f32)> = (0..=12)
        .map(|i| {
          let t = i as f32 / 12.0;
          (0.28 + 0.44 * t, 0.8 - 0.6 * (t * t * (3.0 - 2.0 * t)))
        })
        .collect();
      k.sk(&s, Coral);
    }
    Icon::SkOffset => {
      k.ska((0.5, 0.95), 0.36, 0.3, PI - 0.3, Slate);
      k.ska((0.5, 0.95), 0.62, 0.3, PI - 0.3, Coral);
    }
    Icon::SkTrim => {
      k.sk(&[(0.06, 0.6), (0.5, 0.6)], Slate);
      k.dashed((0.5, 0.6), (0.94, 0.6), Coral);
      k.sk(&[(0.5, 0.08), (0.5, 0.92)], Slate);
      k.thick((0.66, 0.4), (0.86, 0.8), Coral, 1.2);
      k.thick((0.86, 0.4), (0.66, 0.8), Coral, 1.2);
    }
    Icon::SkExtend => {
      k.sk(&[(0.06, 0.6), (0.5, 0.6)], Slate);
      k.dashed((0.5, 0.6), (0.86, 0.6), Coral);
      k.sk(&[(0.88, 0.08), (0.88, 0.92)], Slate);
      k.arrow_head((0.84, 0.6), (1.0, 0.0), 0.14, Coral);
    }
    Icon::SkBreak => {
      k.sk(&[(0.06, 0.5), (0.42, 0.5)], Slate);
      k.sk(&[(0.58, 0.5), (0.94, 0.5)], Slate);
      k.sk(&[(0.42, 0.3), (0.42, 0.7)], Coral);
      k.sk(&[(0.58, 0.3), (0.58, 0.7)], Coral);
    }
    Icon::SkScale => {
      k.skc(&[(0.06, 0.5), (0.5, 0.5), (0.5, 0.94), (0.06, 0.94)], Slate);
      k.dashed((0.06, 0.06), (0.94, 0.06), Coral);
      k.dashed((0.94, 0.06), (0.94, 0.94), Coral);
      k.dashed((0.94, 0.94), (0.06, 0.94), Coral);
      k.dashed((0.06, 0.94), (0.06, 0.06), Coral);
      k.vtx((0.06, 0.94));
    }
    Icon::HorizVert => {
      // a bar with ticks (Fusion's red/orange constraint art)
      k.thick((0.1, 0.5), (0.9, 0.5), Coral, 1.4);
      k.sk(&[(0.1, 0.3), (0.1, 0.7)], Coral);
      k.sk(&[(0.9, 0.3), (0.9, 0.7)], Coral);
    }
    Icon::Curvature => {
      // a curve with a tangent tick at its crest
      k.ska((0.5, 0.86), 0.46, 0.35, PI - 0.35, Coral);
      k.sk(&[(0.28, 0.4), (0.72, 0.4)], Coral);
    }
    Icon::PolygonCon => {
      // a pentagon
      let pts: Vec<(f32, f32)> = (0..5)
        .map(|i| {
          let t = TAU * i as f32 / 5.0 - FRAC_PI_2;
          (0.5 + t.cos() * 0.42, 0.54 + t.sin() * 0.42)
        })
        .collect();
      k.skc(&pts, Coral);
    }

    Icon::FinishAuto => {
      k.circle((0.5, 0.5), 0.46, Some(Teal), None);
      k.p.add(Shape::line(k.pts(&[(0.27, 0.52), (0.44, 0.68), (0.74, 0.34)]), Stroke::new(w * 2.2, k.col(White))));
    }
    Icon::Flange | Icon::LoftedFlange => {
      let pts = [(0.22, 0.94), (0.22, 0.42), (0.3, 0.24), (0.46, 0.16), (0.92, 0.16)];
      k.band(k.pts(&pts), 5.0);
      if icon == Icon::LoftedFlange {
        k.ellipse((0.7, 0.72), 0.22, 0.12, Some(BlueLight), Blue);
      }
    }
    Icon::Hem => {
      k.band(k.pts(&[(0.06, 0.82), (0.82, 0.34), (0.5, 0.18)]), 4.4);
    }
    Icon::Fold | Icon::Unfold => {
      k.solid(&[(0.06, 0.56), (0.5, 0.56), (0.5, 0.9), (0.06, 0.9)], Blue);
      if icon == Icon::Fold {
        k.solid(&[(0.5, 0.56), (0.9, 0.16), (0.9, 0.5), (0.5, 0.9)], BlueLight);
        k.arc((0.5, 0.56), 0.3, 0.2, 1.4, Ink);
      } else {
        k.solid(&[(0.5, 0.56), (0.94, 0.56), (0.94, 0.9), (0.5, 0.9)], BlueLight);
        k.dashed((0.5, 0.56), (0.9, 0.16), Ink);
      }
    }
    Icon::ConvertSheet => {
      k.block(0.06, 0.4, 0.5, 0.92, 0.2, Light, Fill, Dark);
      k.thick((0.62, 0.9), (0.62, 0.3), Blue, 3.0);
      k.thick((0.62, 0.3), (0.94, 0.3), Blue, 3.0);
    }
    Icon::FlatPattern => {
      k.solid(&[(0.06, 0.06), (0.8, 0.06), (0.8, 0.8), (0.06, 0.8)], White);
      k.line((0.06, 0.32), (0.8, 0.32), Ink);
      k.line((0.32, 0.06), (0.32, 0.8), Ink);
      k.circle((0.82, 0.84), 0.14, Some(Blue), None);
    }
    Icon::SmRules => {
      k.solid(&[(0.04, 0.06), (0.74, 0.06), (0.74, 0.84), (0.04, 0.84)], White);
      for y in [0.3, 0.52] {
        k.line((0.04, y), (0.74, y), Ink);
      }
      k.block(0.5, 0.62, 0.86, 0.96, 0.12, Light, Fill, Dark);
    }
    Icon::CornerClosure => {
      k.solid(&[(0.06, 0.2), (0.46, 0.5), (0.46, 0.94), (0.06, 0.64)], Blue);
      k.solid(&[(0.94, 0.2), (0.54, 0.5), (0.54, 0.94), (0.94, 0.64)], BlueLight);
    }
    Icon::Refold => {
      // a flat sheet with a dashed bend line, the near half folded up
      k.solid(&[(0.04, 0.62), (0.5, 0.62), (0.5, 0.94), (0.04, 0.94)], White);
      k.solid(&[(0.5, 0.62), (0.94, 0.1), (0.94, 0.42), (0.5, 0.94)], Blue);
      k.dashed((0.5, 0.56), (0.5, 0.98), Ink);
    }
    Icon::DriveJoint => {
      k.solid(&[(0.06, 0.62), (0.94, 0.62), (0.94, 0.94), (0.06, 0.94)], White);
      k.circle((0.5, 0.62), 0.12, Some(Blue), Some(Ink));
      k.arc((0.5, 0.62), 0.36, 0.3, PI - 0.3, Blue);
      k.arrow_head((0.84, 0.48), (0.4, 1.0), 0.16, Blue);
    }
    Icon::MotionStudy => {
      k.solid(&[(0.04, 0.12), (0.96, 0.12), (0.96, 0.88), (0.04, 0.88)], White);
      k.line((0.04, 0.34), (0.96, 0.34), Ink);
      for x in [0.26, 0.48, 0.7] {
        k.line((x, 0.12), (x, 0.34), Ink);
      }
      k.path(&[(0.1, 0.78), (0.3, 0.52), (0.52, 0.7), (0.72, 0.46), (0.9, 0.58)], Blue);
    }
    Icon::NewDrawing | Icon::SheetSettings => {
      // a sheet with a border and title block
      k.solid(&[(0.04, 0.12), (0.96, 0.12), (0.96, 0.88), (0.04, 0.88)], White);
      k.closed(&[(0.12, 0.2), (0.88, 0.2), (0.88, 0.8), (0.12, 0.8)], Ink);
      k.fill(&[(0.56, 0.66), (0.88, 0.66), (0.88, 0.8), (0.56, 0.8)], Blue);
      if icon == Icon::SheetSettings {
        k.circle((0.34, 0.44), 0.13, None, Some(Blue));
        k.circle((0.34, 0.44), 0.04, Some(Blue), None);
      } else {
        k.block(0.2, 0.36, 0.44, 0.6, 0.08, Light, Fill, Dark);
      }
    }
    Icon::BaseView | Icon::ProjectedView | Icon::SectionView | Icon::DetailView => {
      k.solid(&[(0.04, 0.12), (0.96, 0.12), (0.96, 0.88), (0.04, 0.88)], White);
      match icon {
        Icon::BaseView => k.block(0.24, 0.36, 0.7, 0.76, 0.12, BlueLight, Blue, BlueDark),
        Icon::ProjectedView => {
          k.closed(&[(0.12, 0.5), (0.4, 0.5), (0.4, 0.78), (0.12, 0.78)], Ink);
          k.fill(&[(0.6, 0.22), (0.88, 0.22), (0.88, 0.5), (0.6, 0.5)], BlueLight);
          k.closed(&[(0.6, 0.22), (0.88, 0.22), (0.88, 0.5), (0.6, 0.5)], Blue);
          k.dashed((0.4, 0.5), (0.6, 0.5), Ink);
        }
        Icon::SectionView => {
          k.closed(&[(0.18, 0.26), (0.82, 0.26), (0.82, 0.74), (0.18, 0.74)], Ink);
          // hatching on the cut half
          for (a, b) in [
            ((0.18, 0.5), (0.3, 0.26)),
            ((0.18, 0.74), (0.42, 0.26)),
            ((0.3, 0.74), (0.5, 0.34)),
            ((0.42, 0.74), (0.5, 0.58)),
          ] {
            k.line(a, b, Blue);
          }
          k.thick((0.5, 0.14), (0.5, 0.86), Blue, 1.2);
        }
        _ => {
          k.closed(&[(0.1, 0.4), (0.46, 0.4), (0.46, 0.8), (0.1, 0.8)], Ink);
          k.circle((0.38, 0.46), 0.1, None, Some(Blue));
          k.circle((0.72, 0.42), 0.2, Some(BlueLight), Some(Blue));
          k.line((0.46, 0.44), (0.52, 0.42), Blue);
        }
      }
    }
    Icon::PartsList | Icon::TitleBlock => {
      k.solid(&[(0.06, 0.16), (0.94, 0.16), (0.94, 0.84), (0.06, 0.84)], White);
      k.fill(&[(0.06, 0.16), (0.94, 0.16), (0.94, 0.34), (0.06, 0.34)], Blue);
      for y in [0.5, 0.67] {
        k.line((0.06, y), (0.94, y), Ink);
      }
      if icon == Icon::PartsList {
        k.line((0.3, 0.34), (0.3, 0.84), Ink);
      } else {
        k.line((0.5, 0.34), (0.5, 0.84), Ink);
      }
    }
    Icon::ExportPdf | Icon::ExportDxf | Icon::ExportSvg => {
      k.solid(&[(0.16, 0.04), (0.84, 0.04), (0.84, 0.96), (0.16, 0.96)], White);
      k.fill(&[(0.16, 0.62), (0.84, 0.62), (0.84, 0.96), (0.16, 0.96)], Ink);
      let t = match icon {
        Icon::ExportPdf => "PDF",
        Icon::ExportDxf => "DXF",
        _ => "SVG",
      };
      k.text((0.5, 0.79), t, 0.26, White);
      k.thick((0.5, 0.42), (0.5, 0.1), Blue, 1.4);
      k.arrow_head((0.5, 0.04), (0.0, -1.0), 0.16, Blue);
    }
    Icon::Rip => {
      k.solid(&[(0.14, 0.06), (0.86, 0.06), (0.86, 0.94), (0.14, 0.94)], Blue);
      k.path(&[(0.5, 0.06), (0.4, 0.24), (0.6, 0.42), (0.4, 0.6), (0.6, 0.78), (0.5, 0.94)], White);
    }
    Icon::Scripts => {
      k.solid(&[(0.04, 0.08), (0.8, 0.08), (0.8, 0.7), (0.04, 0.7)], White);
      k.fill(&[(0.04, 0.08), (0.8, 0.08), (0.8, 0.2), (0.04, 0.2)], Ink);
      k.path(&[(0.14, 0.32), (0.26, 0.42), (0.14, 0.52)], Ink);
      k.circle((0.76, 0.76), 0.16, Some(Fill), Some(Ink));
      k.circle((0.76, 0.76), 0.06, Some(White), None);
    }
    Icon::Marketplace => {
      k.solid(&[(0.24, 0.42), (0.7, 0.42), (0.7, 0.94), (0.24, 0.94)], Ink);
      k.solid(&[(0.36, 0.16), (0.58, 0.16), (0.58, 0.42), (0.36, 0.42)], Ink);
      k.thick((0.6, 0.18), (0.86, 0.04), Orange, 2.4);
    }
    Icon::Assistant => {
      for i in 0..8 {
        let a = i as f32 * TAU / 8.0;
        let (c, s) = (a.cos(), a.sin());
        k.thick((0.5 + c * 0.14, 0.5 + s * 0.14), (0.5 + c * 0.42, 0.5 + s * 0.42), Ink, 1.4);
      }
    }
    Icon::Gauge => {
      k.circle((0.5, 0.5), 0.42, None, Some(Ink));
      k.thick((0.5, 0.5), (0.74, 0.3), Ink, 1.4);
      k.dot((0.5, 0.5), Ink);
    }
    Icon::Clock => {
      k.circle((0.5, 0.5), 0.42, None, Some(Ink));
      k.path(&[(0.5, 0.22), (0.5, 0.5), (0.7, 0.62)], Ink);
    }
    Icon::Print3d => {
      k.solid(&[(0.06, 0.12), (0.94, 0.12), (0.94, 0.9), (0.06, 0.9)], Dark);
      k.solid(&[(0.2, 0.34), (0.8, 0.34), (0.8, 0.78), (0.2, 0.78)], White);
      k.fill(&[(0.3, 0.78), (0.5, 0.48), (0.7, 0.78)], Orange);
    }
    Icon::ConvertMesh => {
      k.ellipse((0.34, 0.78), 0.22, 0.08, Some(Fill), Ink);
      k.fill(&[(0.12, 0.3), (0.56, 0.3), (0.56, 0.78), (0.12, 0.78)], Fill);
      k.ellipse((0.34, 0.3), 0.22, 0.08, Some(Light), Ink);
      k.block(0.6, 0.62, 0.86, 0.94, 0.1, BlueLight, Blue, BlueDark);
    }
    Icon::BreakView => {
      // a long bar with its middle broken out
      k.solid(&[(0.06, 0.36), (0.38, 0.36), (0.38, 0.64), (0.06, 0.64)], BlueLight);
      k.solid(&[(0.62, 0.36), (0.94, 0.36), (0.94, 0.64), (0.62, 0.64)], BlueLight);
      for x in [0.44, 0.56] {
        k.path(&[(x, 0.2), (x, 0.42), (x + 0.04, 0.47), (x - 0.04, 0.53), (x, 0.58), (x, 0.8)], Ink);
      }
    }
    Icon::OrdinateDim => {
      k.line((0.1, 0.92), (0.9, 0.92), Ink);
      for (x, t) in [(0.16, "0"), (0.5, "2"), (0.84, "5")] {
        k.line((x, 0.86), (x, 0.42), Blue);
        k.text((x, 0.26), t, 0.3, Ink);
      }
    }
    Icon::AngleDim => {
      k.thick((0.12, 0.88), (0.92, 0.88), Ink, 1.2);
      k.thick((0.12, 0.88), (0.7, 0.2), Ink, 1.2);
      k.arc((0.12, 0.88), 0.56, 0.0, 0.86, Blue);
      k.arrow_head((0.68, 0.88), (0.0, 1.0), 0.12, Blue);
      k.text((0.66, 0.6), "°", 0.36, Blue);
    }
    Icon::FeatureControl => {
      k.solid(&[(0.04, 0.3), (0.96, 0.3), (0.96, 0.7), (0.04, 0.7)], White);
      for x in [0.34, 0.74] {
        k.line((x, 0.3), (x, 0.7), Ink);
      }
      k.circle((0.19, 0.5), 0.08, None, Some(Blue));
      k.line((0.07, 0.5), (0.31, 0.5), Blue);
      k.line((0.19, 0.36), (0.19, 0.64), Blue);
      k.text((0.54, 0.5), ".1", 0.3, Ink);
      k.text((0.85, 0.5), "A", 0.3, Ink);
    }
    Icon::DatumId => {
      k.solid(&[(0.26, 0.06), (0.74, 0.06), (0.74, 0.5), (0.26, 0.5)], White);
      k.text((0.5, 0.28), "A", 0.36, Ink);
      k.line((0.5, 0.5), (0.5, 0.72), Ink);
      k.fill(&[(0.32, 0.9), (0.68, 0.9), (0.5, 0.72)], Blue);
      k.thick((0.1, 0.92), (0.9, 0.92), Ink, 1.2);
    }
    Icon::SurfaceTexture => {
      k.thick((0.06, 0.9), (0.94, 0.9), Ink, 1.2);
      k.path(&[(0.26, 0.5), (0.44, 0.88), (0.72, 0.18), (0.94, 0.18)], Blue);
      k.line((0.26, 0.5), (0.6, 0.5), Blue);
      k.text((0.18, 0.24), "Ra", 0.24, Ink);
    }
    Icon::Balloon => {
      k.circle((0.62, 0.36), 0.26, Some(White), Some(Ink));
      k.text((0.62, 0.36), "1", 0.3, Ink);
      k.line((0.44, 0.55), (0.16, 0.84), Blue);
      k.dot((0.16, 0.84), Blue);
    }
    Icon::Properties => {
      k.solid(&[(0.14, 0.06), (0.86, 0.06), (0.86, 0.94), (0.14, 0.94)], White);
      k.closed(&[(0.14, 0.06), (0.86, 0.06), (0.86, 0.94), (0.14, 0.94)], Ink);
      for y in [0.28, 0.5, 0.72] {
        k.dot((0.28, y), Blue);
        k.line((0.42, y), (0.74, y), Ink);
      }
    }
    Icon::SkScaleTo => {
      // a shape grown to touch its box, with arrows out to the corners
      k.dashed((0.04, 0.04), (0.96, 0.04), Coral);
      k.dashed((0.96, 0.04), (0.96, 0.96), Coral);
      k.dashed((0.96, 0.96), (0.04, 0.96), Coral);
      k.dashed((0.04, 0.96), (0.04, 0.04), Coral);
      k.skc(&[(0.3, 0.3), (0.7, 0.3), (0.7, 0.7), (0.3, 0.7)], Slate);
      k.line((0.7, 0.3), (0.86, 0.14), Coral);
      k.arrow_head((0.9, 0.1), (0.7, -0.7), 0.12, Coral);
      k.line((0.3, 0.7), (0.14, 0.86), Coral);
      k.arrow_head((0.1, 0.9), (-0.7, 0.7), 0.12, Coral);
    }
    Icon::SketchLibrary => {
      // a drawer of parts: a keyhole and a stud on a sheet
      k.solid(&[(0.06, 0.1), (0.94, 0.1), (0.94, 0.9), (0.06, 0.9)], White);
      k.circle((0.32, 0.36), 0.1, None, Some(Slate));
      k.sk(&[(0.32, 0.46), (0.32, 0.72)], Slate);
      k.circle((0.68, 0.36), 0.12, None, Some(Teal));
      k.circle((0.68, 0.36), 0.05, Some(Teal), None);
      k.sk(&[(0.56, 0.7), (0.8, 0.7)], Teal);
      k.vtx((0.32, 0.72));
    }
    Icon::SkRemoveDuplicates => {
      // a line with a copy lying on it, the copy crossed out
      k.sk(&[(0.06, 0.36), (0.94, 0.36)], Slate);
      k.dashed((0.06, 0.5), (0.94, 0.5), Coral);
      k.vtx((0.06, 0.36));
      k.vtx((0.94, 0.36));
      k.thick((0.56, 0.64), (0.84, 0.92), Coral, 1.2);
      k.thick((0.84, 0.64), (0.56, 0.92), Coral, 1.2);
    }
    Icon::SkCleanup => {
      // an outline with its gap closed, and a sparkle
      k.sk(&[(0.5, 0.94), (0.06, 0.94), (0.06, 0.4), (0.5, 0.4)], Slate);
      k.sk(&[(0.5, 0.4), (0.62, 0.4), (0.62, 0.94), (0.5, 0.94)], Coral);
      k.dot((0.5, 0.4), Coral);
      k.dot((0.5, 0.94), Coral);
      k.thick((0.8, 0.06), (0.8, 0.38), Coral, 1.0);
      k.thick((0.64, 0.22), (0.96, 0.22), Coral, 1.0);
    }
    Icon::Centerline => {
      // dash-dot-dash along the diagonal, as a centerline draws
      k.thick((0.1, 0.9), (0.36, 0.64), Orange, 1.0);
      k.dot((0.5, 0.5), Orange);
      k.thick((0.64, 0.36), (0.9, 0.1), Orange, 1.0);
    }
    // ---- Move/Copy ----
    Icon::MoveFree => {
      // the triad: three arrows from one point
      k.thick((0.3, 0.7), (0.3, 0.12), Red, 1.2);
      k.arrow_head((0.3, 0.06), (0.0, -1.0), 0.14, Red);
      k.thick((0.3, 0.7), (0.9, 0.7), Green, 1.2);
      k.arrow_head((0.96, 0.7), (1.0, 0.0), 0.14, Green);
      k.thick((0.3, 0.7), (0.08, 0.94), Blue, 1.2);
      k.arrow_head((0.04, 0.98), (-0.7, 0.7), 0.14, Blue);
      k.circle((0.3, 0.7), 0.06, Some(Ink), None);
    }
    Icon::MoveTranslate => {
      k.solid(&[(0.08, 0.3), (0.42, 0.3), (0.42, 0.64), (0.08, 0.64)], Light);
      k.closed(&[(0.52, 0.3), (0.86, 0.3), (0.86, 0.64), (0.52, 0.64)], Blue);
      k.thick((0.2, 0.82), (0.72, 0.82), Blue, 1.2);
      k.arrow_head((0.8, 0.82), (1.0, 0.0), 0.14, Blue);
    }
    Icon::MoveRotate => {
      k.solid(&[(0.3, 0.46), (0.7, 0.46), (0.7, 0.86), (0.3, 0.86)], Light);
      k.arc((0.5, 0.5), 0.4, 0.3, 2.9, Blue);
      k.arrow_head((0.9, 0.42), (0.1, 1.0), 0.14, Blue);
      k.dashed((0.5, 0.04), (0.5, 0.98), Ink);
    }
    Icon::MovePointToPoint => {
      k.circle((0.18, 0.74), 0.08, Some(Blue), Some(BlueDark));
      k.circle((0.82, 0.26), 0.08, Some(White), Some(Blue));
      k.thick((0.28, 0.66), (0.66, 0.38), Blue, 1.2);
      k.arrow_head((0.72, 0.34), (0.8, -0.6), 0.14, Blue);
    }
    Icon::MovePointToPosition => {
      k.circle((0.24, 0.7), 0.08, Some(Blue), Some(BlueDark));
      k.thick((0.34, 0.62), (0.62, 0.42), Blue, 1.2);
      k.arrow_head((0.68, 0.38), (0.8, -0.6), 0.14, Blue);
      k.line((0.56, 0.18), (0.94, 0.18), Ink);
      k.line((0.94, 0.18), (0.94, 0.56), Ink);
      k.text((0.8, 0.36), "xyz", 0.3, Ink);
    }
    Icon::SetPivot => {
      // a cross hair with a dot: where the move turns about
      k.circle((0.5, 0.5), 0.3, None, Some(Ink));
      k.line((0.5, 0.06), (0.5, 0.32), Ink);
      k.line((0.5, 0.68), (0.5, 0.94), Ink);
      k.line((0.06, 0.5), (0.32, 0.5), Ink);
      k.line((0.68, 0.5), (0.94, 0.5), Ink);
      k.circle((0.5, 0.5), 0.09, Some(Blue), None);
    }
    // ---- Shell Type ----
    Icon::ShellSharp => {
      // a hollow box with square corners
      k.solid(&[(0.1, 0.18), (0.9, 0.18), (0.9, 0.9), (0.1, 0.9)], Light);
      k.solid(&[(0.3, 0.38), (0.7, 0.38), (0.7, 0.9), (0.3, 0.9)], White);
      k.closed(&[(0.3, 0.38), (0.7, 0.38), (0.7, 0.9), (0.3, 0.9)], Blue);
    }
    Icon::ShellRound => {
      // the same with rounded corners inside
      k.solid(&[(0.1, 0.18), (0.9, 0.18), (0.9, 0.9), (0.1, 0.9)], Light);
      let mut pts = vec![(0.3, 0.9), (0.3, 0.52)];
      for i in 0..=6 {
        let t = PI - FRAC_PI_2 * i as f32 / 6.0;
        pts.push((0.44 + t.cos() * 0.14, 0.52 - t.sin() * 0.14));
      }
      for i in 0..=6 {
        let t = FRAC_PI_2 - FRAC_PI_2 * i as f32 / 6.0;
        pts.push((0.56 + t.cos() * 0.14, 0.52 - t.sin() * 0.14));
      }
      pts.push((0.7, 0.9));
      k.solid(&pts, White);
      k.path(&pts, Blue);
    }
    // ---- Hole ----
    Icon::HoleSingle => {
      k.solid(&[(0.08, 0.3), (0.92, 0.3), (0.92, 0.9), (0.08, 0.9)], Light);
      k.ellipse((0.5, 0.3), 0.14, 0.06, Some(White), Blue);
    }
    Icon::HoleAtPoints => {
      k.solid(&[(0.08, 0.3), (0.92, 0.3), (0.92, 0.9), (0.08, 0.9)], Light);
      for x in [0.26, 0.5, 0.74] {
        k.ellipse((x, 0.3), 0.08, 0.035, Some(White), Blue);
        k.vtx((x, 0.14));
      }
    }
    Icon::HoleSimple => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.38, 0.1), (0.62, 0.1), (0.62, 0.72), (0.5, 0.84), (0.38, 0.72)], White);
      k.path(&[(0.38, 0.1), (0.38, 0.72), (0.5, 0.84), (0.62, 0.72), (0.62, 0.1)], Blue);
    }
    Icon::HoleCounterbore => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.fill(&[(0.24, 0.1), (0.76, 0.1), (0.76, 0.34), (0.24, 0.34)], White);
      k.fill(&[(0.38, 0.34), (0.62, 0.34), (0.62, 0.72), (0.5, 0.84), (0.38, 0.72)], White);
      k.path(
        &[
          (0.24, 0.1),
          (0.24, 0.34),
          (0.38, 0.34),
          (0.38, 0.72),
          (0.5, 0.84),
          (0.62, 0.72),
          (0.62, 0.34),
          (0.76, 0.34),
          (0.76, 0.1),
        ],
        Blue,
      );
    }
    Icon::HoleCountersink => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.fill(&[(0.2, 0.1), (0.8, 0.1), (0.62, 0.34), (0.38, 0.34)], White);
      k.fill(&[(0.38, 0.34), (0.62, 0.34), (0.62, 0.72), (0.5, 0.84), (0.38, 0.72)], White);
      k.path(&[(0.2, 0.1), (0.38, 0.34), (0.38, 0.72), (0.5, 0.84), (0.62, 0.72), (0.62, 0.34), (0.8, 0.1)], Blue);
    }
    Icon::TapSimple => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.36, 0.1), (0.64, 0.1), (0.64, 0.9), (0.36, 0.9)], White);
      k.line((0.36, 0.1), (0.36, 0.9), Blue);
      k.line((0.64, 0.1), (0.64, 0.9), Blue);
    }
    Icon::TapClearance => {
      // a plain hole with the bolt's shank through it, clear of the walls
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.3, 0.1), (0.7, 0.1), (0.7, 0.9), (0.3, 0.9)], White);
      k.line((0.3, 0.1), (0.3, 0.9), Blue);
      k.line((0.7, 0.1), (0.7, 0.9), Blue);
      k.solid(&[(0.42, 0.04), (0.58, 0.04), (0.58, 0.96), (0.42, 0.96)], Dark);
    }
    Icon::TapTapped => {
      // threads on the walls
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.36, 0.1), (0.64, 0.1), (0.64, 0.9), (0.36, 0.9)], White);
      for i in 0..5 {
        let y = 0.18 + i as f32 * 0.15;
        k.line((0.3, y), (0.42, y + 0.06), Blue);
        k.line((0.58, y + 0.06), (0.7, y), Blue);
      }
    }
    Icon::TapTaper => {
      // threads on walls that narrow towards the bottom
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.32, 0.1), (0.68, 0.1), (0.6, 0.9), (0.4, 0.9)], White);
      for i in 0..5 {
        let y = 0.18 + i as f32 * 0.15;
        let dx = 0.04 * i as f32 / 4.0;
        k.line((0.26 + dx, y), (0.38 + dx, y + 0.06), Blue);
        k.line((0.62 - dx, y + 0.06), (0.74 - dx, y), Blue);
      }
    }
    Icon::DrillAngle => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.36, 0.1), (0.64, 0.1), (0.64, 0.66), (0.5, 0.82), (0.36, 0.66)], White);
      k.path(&[(0.36, 0.1), (0.36, 0.66), (0.5, 0.82), (0.64, 0.66), (0.64, 0.1)], Blue);
    }
    Icon::DrillFlat => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.9), (0.08, 0.9)], Light);
      k.solid(&[(0.36, 0.1), (0.64, 0.1), (0.64, 0.78), (0.36, 0.78)], White);
      k.path(&[(0.36, 0.1), (0.36, 0.78), (0.64, 0.78), (0.64, 0.1)], Blue);
    }
    // Extrude dialog values: small line drawings that sit before a dropdown value's text
    Icon::ExtrudeSolid => {
      k.block(0.14, 0.42, 0.62, 0.92, 0.24, BlueLight, Blue, BlueDark);
      k.thick((0.38, 0.36), (0.38, 0.1), Ink, 1.0);
      k.arrow_head((0.38, 0.04), (0.0, -1.0), 0.13, Ink);
    }
    Icon::ExtrudeThin => {
      k.block(0.14, 0.42, 0.62, 0.92, 0.24, BlueDark, Blue, BlueDark);
      k.solid(&[(0.24, 0.52), (0.52, 0.52), (0.52, 0.82), (0.24, 0.82)], White);
      k.thick((0.38, 0.36), (0.38, 0.1), Ink, 1.0);
      k.arrow_head((0.38, 0.04), (0.0, -1.0), 0.13, Ink);
    }
    Icon::ProfilePlane => {
      k.solid(&[(0.06, 0.78), (0.34, 0.42), (0.94, 0.42), (0.66, 0.78)], Light);
      k.circle((0.5, 0.6), 0.12, Some(BlueLight), Some(Blue));
      k.thick((0.5, 0.42), (0.5, 0.14), Ink, 1.0);
      k.arrow_head((0.5, 0.08), (0.0, -1.0), 0.13, Ink);
    }
    Icon::StartOffset => {
      k.solid(&[(0.06, 0.9), (0.34, 0.6), (0.94, 0.6), (0.66, 0.9)], Light);
      k.closed(&[(0.06, 0.48), (0.34, 0.18), (0.94, 0.18), (0.66, 0.48)], Blue);
      k.dashed((0.2, 0.75), (0.2, 0.33), Ink);
      k.arrow_head((0.2, 0.3), (0.0, -1.0), 0.11, Ink);
    }
    Icon::StartObject => {
      k.block(0.1, 0.5, 0.6, 0.92, 0.3, Dark, Fill, Dark);
      k.closed(&[(0.1, 0.5), (0.4, 0.29), (0.9, 0.29), (0.6, 0.5)], Blue);
      k.thick((0.5, 0.36), (0.5, 0.1), Ink, 1.0);
      k.arrow_head((0.5, 0.04), (0.0, -1.0), 0.13, Ink);
    }
    Icon::OneSide => {
      k.thick((0.3, 0.1), (0.3, 0.9), Slate, 1.2);
      k.solid(&[(0.3, 0.3), (0.72, 0.3), (0.72, 0.7), (0.3, 0.7)], BlueLight);
      k.thick((0.72, 0.5), (0.86, 0.5), Ink, 1.0);
      k.arrow_head((0.94, 0.5), (1.0, 0.0), 0.13, Ink);
    }
    Icon::TwoSides => {
      k.thick((0.42, 0.1), (0.42, 0.9), Slate, 1.2);
      k.solid(&[(0.16, 0.3), (0.42, 0.3), (0.42, 0.7), (0.16, 0.7)], BlueDark);
      k.solid(&[(0.42, 0.3), (0.8, 0.3), (0.8, 0.7), (0.42, 0.7)], BlueLight);
      k.arrow_head((0.06, 0.5), (-1.0, 0.0), 0.11, Ink);
      k.arrow_head((0.94, 0.5), (1.0, 0.0), 0.11, Ink);
    }
    Icon::SymmetricExtent => {
      k.dashed((0.5, 0.06), (0.5, 0.94), Slate);
      k.solid(&[(0.22, 0.3), (0.5, 0.3), (0.5, 0.7), (0.22, 0.7)], BlueLight);
      k.solid(&[(0.5, 0.3), (0.78, 0.3), (0.78, 0.7), (0.5, 0.7)], BlueLight);
      k.arrow_head((0.08, 0.5), (-1.0, 0.0), 0.11, Ink);
      k.arrow_head((0.92, 0.5), (1.0, 0.0), 0.11, Ink);
    }
    Icon::ExtentDistance => {
      k.solid(&[(0.1, 0.22), (0.56, 0.22), (0.56, 0.78), (0.1, 0.78)], BlueLight);
      k.thick((0.78, 0.26), (0.78, 0.74), Ink, 1.0);
      k.arrow_head((0.78, 0.18), (0.0, -1.0), 0.11, Ink);
      k.arrow_head((0.78, 0.82), (0.0, 1.0), 0.11, Ink);
      k.line((0.6, 0.22), (0.92, 0.22), Slate);
      k.line((0.6, 0.78), (0.92, 0.78), Slate);
    }
    Icon::ToObject => {
      k.solid(&[(0.08, 0.08), (0.92, 0.08), (0.92, 0.34), (0.08, 0.34)], Fill);
      k.solid(&[(0.3, 0.7), (0.7, 0.7), (0.7, 0.92), (0.3, 0.92)], BlueLight);
      k.thick((0.5, 0.7), (0.5, 0.46), Ink, 1.0);
      k.arrow_head((0.5, 0.38), (0.0, -1.0), 0.13, Ink);
    }
    Icon::ThroughAll => {
      k.solid(&[(0.08, 0.28), (0.92, 0.28), (0.92, 0.72), (0.08, 0.72)], Fill);
      k.solid(&[(0.38, 0.06), (0.62, 0.06), (0.62, 0.94), (0.38, 0.94)], BlueLight);
      k.arrow_head((0.5, 0.02), (0.0, -1.0), 0.13, Ink);
    }
    Icon::WallSide1 => {
      k.thick((0.5, 0.08), (0.5, 0.92), Slate, 1.2);
      k.solid(&[(0.26, 0.2), (0.5, 0.2), (0.5, 0.8), (0.26, 0.8)], BlueLight);
    }
    Icon::WallSide2 => {
      k.thick((0.5, 0.08), (0.5, 0.92), Slate, 1.2);
      k.solid(&[(0.5, 0.2), (0.74, 0.2), (0.74, 0.8), (0.5, 0.8)], BlueLight);
    }
    Icon::WallCenter => {
      k.solid(&[(0.38, 0.2), (0.62, 0.2), (0.62, 0.8), (0.38, 0.8)], BlueLight);
      k.dashed((0.5, 0.08), (0.5, 0.92), Slate);
    }
    Icon::OpJoin => {
      k.fill(&[(0.08, 0.08), (0.6, 0.08), (0.6, 0.6), (0.08, 0.6)], BlueLight);
      k.fill(&[(0.4, 0.4), (0.92, 0.4), (0.92, 0.92), (0.4, 0.92)], BlueLight);
      k.path(&[(0.6, 0.4), (0.6, 0.08), (0.08, 0.08), (0.08, 0.6), (0.4, 0.6)], Blue);
      k.path(&[(0.4, 0.6), (0.4, 0.92), (0.92, 0.92), (0.92, 0.4), (0.6, 0.4)], Blue);
    }
    Icon::OpCut => {
      k.solid(&[(0.08, 0.08), (0.6, 0.08), (0.6, 0.6), (0.08, 0.6)], BlueLight);
      k.solid(&[(0.4, 0.4), (0.92, 0.4), (0.92, 0.92), (0.4, 0.92)], White);
      k.closed(&[(0.4, 0.4), (0.92, 0.4), (0.92, 0.92), (0.4, 0.92)], Red);
    }
    Icon::OpIntersect => {
      k.closed(&[(0.08, 0.08), (0.6, 0.08), (0.6, 0.6), (0.08, 0.6)], Slate);
      k.closed(&[(0.4, 0.4), (0.92, 0.4), (0.92, 0.92), (0.4, 0.92)], Slate);
      k.solid(&[(0.4, 0.4), (0.6, 0.4), (0.6, 0.6), (0.4, 0.6)], Blue);
    }
    Icon::OpNewBody => {
      k.block(0.1, 0.4, 0.56, 0.9, 0.26, BlueLight, Blue, BlueDark);
      k.thick((0.8, 0.08), (0.8, 0.36), Ink, 1.2);
      k.thick((0.66, 0.22), (0.94, 0.22), Ink, 1.2);
    }
    Icon::OpNewComponent => {
      k.block(0.08, 0.5, 0.48, 0.9, 0.2, Dark, Fill, Dark);
      k.block(0.5, 0.56, 0.74, 0.9, 0.16, BlueLight, Blue, BlueDark);
      k.thick((0.8, 0.08), (0.8, 0.36), Ink, 1.2);
      k.thick((0.66, 0.22), (0.94, 0.22), Ink, 1.2);
    }
    Icon::TextBox => {
      k.dashed((0.08, 0.14), (0.92, 0.14), Slate);
      k.dashed((0.08, 0.86), (0.92, 0.86), Slate);
      k.dashed((0.08, 0.14), (0.08, 0.86), Slate);
      k.dashed((0.92, 0.14), (0.92, 0.86), Slate);
      k.text((0.5, 0.5), "A", 0.62, Teal);
    }
    Icon::TextOnPath => {
      k.ska((0.5, 1.1), 0.72, 0.5, PI - 0.5, Slate);
      k.text((0.5, 0.36), "A", 0.56, Teal);
    }
    Icon::FilterBodies => {
      k.block(0.14, 0.34, 0.66, 0.9, 0.2, Light, Blue, Slate);
    }
    Icon::FilterFaces => {
      k.solid(&[(0.1, 0.9), (0.9, 0.9), (0.9, 0.34), (0.1, 0.34)], Light);
      k.sk(&[(0.1, 0.34), (0.34, 0.1), (0.9, 0.1), (0.9, 0.34)], Slate);
      k.thick((0.1, 0.34), (0.9, 0.34), Teal, 2.0);
    }
    Icon::Flip => {
      // the curve, with an arrow out of each side
      k.thick((0.5, 0.1), (0.5, 0.9), Slate, 1.2);
      k.thick((0.5, 0.36), (0.78, 0.36), Blue, 1.0);
      k.arrow_head((0.9, 0.36), (1.0, 0.0), 0.13, Blue);
      k.thick((0.5, 0.64), (0.22, 0.64), Ink, 1.0);
      k.arrow_head((0.1, 0.64), (-1.0, 0.0), 0.13, Ink);
    }
    Icon::SkChamferAngle => {
      k.sk(&[(0.12, 0.92), (0.12, 0.5)], Slate);
      k.sk(&[(0.5, 0.12), (0.92, 0.12)], Slate);
      k.dashed((0.12, 0.5), (0.12, 0.12), Slate);
      k.dashed((0.12, 0.12), (0.5, 0.12), Slate);
      k.sk(&[(0.12, 0.5), (0.5, 0.12)], Coral);
      // the angle between the bevel and the first line
      k.ska((0.12, 0.5), 0.2, -FRAC_PI_2, -FRAC_PI_2 + 0.78, Teal);
    }
    Icon::SkChamfer2 => {
      k.sk(&[(0.12, 0.92), (0.12, 0.62)], Slate);
      k.sk(&[(0.36, 0.12), (0.92, 0.12)], Slate);
      k.dashed((0.12, 0.62), (0.12, 0.12), Slate);
      k.dashed((0.12, 0.12), (0.36, 0.12), Slate);
      k.sk(&[(0.12, 0.62), (0.36, 0.12)], Coral);
      // a tick on each leg: the two distances differ
      k.sk(&[(0.04, 0.62), (0.2, 0.62)], Teal);
      k.sk(&[(0.36, 0.04), (0.36, 0.2)], Teal);
    }
    // ---- Change Parameters ----
    Icon::FilterFavorites => {
      // a funnel with a small star at its mouth
      k.solid(&[(0.06, 0.16), (0.74, 0.16), (0.48, 0.52), (0.48, 0.9), (0.32, 0.8), (0.32, 0.52)], Light);
      k.closed(&star_pts((0.78, 0.26), 0.2, 0.08), Blue);
    }
    Icon::Star => {
      k.closed(&star_pts((0.5, 0.5), 0.44, 0.19), Blue);
    }
    Icon::Duplicate => {
      // two sheets, the copy in front
      k.solid(&[(0.3, 0.06), (0.9, 0.06), (0.9, 0.7), (0.3, 0.7)], Light);
      k.solid(&[(0.1, 0.3), (0.7, 0.3), (0.7, 0.94), (0.1, 0.94)], White);
      k.closed(&[(0.1, 0.3), (0.7, 0.3), (0.7, 0.94), (0.1, 0.94)], Blue);
    }
    Icon::Trash => {
      // a bin with its lid off
      k.solid(&[(0.18, 0.3), (0.82, 0.3), (0.74, 0.94), (0.26, 0.94)], Light);
      k.thick((0.1, 0.3), (0.9, 0.3), Ink, 1.2);
      k.line((0.38, 0.3), (0.38, 0.14), Ink);
      k.line((0.62, 0.3), (0.62, 0.14), Ink);
      k.line((0.38, 0.14), (0.62, 0.14), Ink);
      k.line((0.4, 0.42), (0.42, 0.82), Blue);
      k.line((0.6, 0.42), (0.58, 0.82), Blue);
    }
    Icon::Export => {
      // Import's tray, the arrow up out of it
      k.solid(&[(0.08, 0.6), (0.92, 0.6), (0.92, 0.92), (0.08, 0.92)], Fill);
      k.thick((0.5, 0.52), (0.5, 0.06), Blue, 1.6);
      k.arrow_head((0.5, 0.04), (0.0, -1.0), 0.2, Blue);
    }
    Icon::Bold => {
      k.text((0.5, 0.5), "B", 0.78, Ink);
      k.text((0.52, 0.5), "B", 0.78, Ink);
    }
    Icon::Italic => {
      k.text((0.5, 0.5), "I", 0.78, Ink);
      k.line((0.3, 0.86), (0.7, 0.14), Ink);
    }
    Icon::FlipH => {
      // two mirrored triangles about a vertical dashed axis
      k.dashed((0.5, 0.08), (0.5, 0.92), Slate);
      k.solid(&[(0.42, 0.26), (0.42, 0.74), (0.1, 0.74)], Teal);
      k.sk(&[(0.58, 0.26), (0.58, 0.74), (0.9, 0.74), (0.58, 0.26)], Slate);
    }
    Icon::FlipV => {
      k.dashed((0.08, 0.5), (0.92, 0.5), Slate);
      k.solid(&[(0.26, 0.42), (0.74, 0.42), (0.74, 0.1)], Teal);
      k.sk(&[(0.26, 0.58), (0.74, 0.58), (0.74, 0.9), (0.26, 0.58)], Slate);
    }
    Icon::AlignLeft => {
      for (y, w) in [(0.2, 0.8), (0.4, 0.5), (0.6, 0.8), (0.8, 0.5)] {
        k.thick((0.1, y), (0.1 + w, y), Ink, 1.3);
      }
    }
    Icon::AlignCenter => {
      for (y, w) in [(0.2, 0.8), (0.4, 0.5), (0.6, 0.8), (0.8, 0.5)] {
        k.thick((0.5 - w / 2.0, y), (0.5 + w / 2.0, y), Ink, 1.3);
      }
    }
    Icon::AlignRight => {
      for (y, w) in [(0.2, 0.8), (0.4, 0.5), (0.6, 0.8), (0.8, 0.5)] {
        k.thick((0.9 - w, y), (0.9, y), Ink, 1.3);
      }
    }
    Icon::AlignTop => {
      k.thick((0.1, 0.14), (0.9, 0.14), Ink, 1.3);
      k.text((0.5, 0.58), "A", 0.6, Teal);
    }
    Icon::AlignMiddle => {
      k.dashed((0.1, 0.5), (0.9, 0.5), Slate);
      k.text((0.5, 0.5), "A", 0.6, Teal);
    }
    Icon::AlignBottom => {
      k.thick((0.1, 0.86), (0.9, 0.86), Ink, 1.3);
      k.text((0.5, 0.44), "A", 0.6, Teal);
    }
    Icon::DistFull => {
      // instances all the way round
      k.circle((0.5, 0.5), 0.36, None, Some(Slate));
      for i in 0..6 {
        let a = i as f32 * PI / 3.0;
        k.circle((0.5 + 0.36 * a.cos(), 0.5 + 0.36 * a.sin()), 0.07, Some(Teal), None);
      }
    }
    Icon::DistAngle => {
      // instances over a swept angle from the first
      k.ska((0.5, 0.5), 0.36, -PI / 2.0, PI / 2.0, Slate);
      k.line((0.5, 0.5), (0.5, 0.14), Slate);
      k.line((0.5, 0.5), (0.5, 0.86), Slate);
      for a in [-PI / 2.0, -PI / 6.0, PI / 6.0, PI / 2.0] {
        k.circle((0.5 + 0.36 * a.cos(), 0.5 + 0.36 * a.sin()), 0.07, Some(Teal), None);
      }
    }
    Icon::DistSymmetric => {
      // instances either side of the first
      k.ska((0.5, 0.5), 0.36, -PI / 2.0, PI / 2.0, Slate);
      k.dashed((0.5, 0.5), (0.86, 0.5), Slate);
      for a in [-PI / 2.0, 0.0, PI / 2.0] {
        k.circle((0.5 + 0.36 * a.cos(), 0.5 + 0.36 * a.sin()), 0.07, Some(Teal), None);
      }
    }
    Icon::DistExtent => {
      // the whole pattern's size: a bar across every instance
      k.line((0.1, 0.3), (0.9, 0.3), Slate);
      k.line((0.1, 0.22), (0.1, 0.38), Slate);
      k.line((0.9, 0.22), (0.9, 0.38), Slate);
      for x in [0.16, 0.5, 0.84] {
        k.solid(&[(x - 0.08, 0.56), (x + 0.08, 0.56), (x + 0.08, 0.84), (x - 0.08, 0.84)], Teal);
      }
    }
    Icon::DistSpacing => {
      // the step between neighbours
      k.line((0.16, 0.3), (0.5, 0.3), Slate);
      k.line((0.16, 0.22), (0.16, 0.38), Slate);
      k.line((0.5, 0.22), (0.5, 0.38), Slate);
      for x in [0.16, 0.5, 0.84] {
        k.solid(&[(x - 0.08, 0.56), (x + 0.08, 0.56), (x + 0.08, 0.84), (x - 0.08, 0.84)], Teal);
      }
    }
    Icon::DirOne => {
      k.solid(&[(0.1, 0.36), (0.3, 0.36), (0.3, 0.64), (0.1, 0.64)], Teal);
      k.line((0.38, 0.5), (0.84, 0.5), Slate);
      k.arrow_head((0.9, 0.5), (1.0, 0.0), 0.16, Slate);
    }
    Icon::DirSymmetric => {
      k.solid(&[(0.4, 0.36), (0.6, 0.36), (0.6, 0.64), (0.4, 0.64)], Teal);
      k.line((0.34, 0.5), (0.14, 0.5), Slate);
      k.arrow_head((0.08, 0.5), (-1.0, 0.0), 0.14, Slate);
      k.line((0.66, 0.5), (0.86, 0.5), Slate);
      k.arrow_head((0.92, 0.5), (1.0, 0.0), 0.14, Slate);
    }
    Icon::Pencil => {
      k.solid(&[(0.62, 0.14), (0.86, 0.38), (0.38, 0.86), (0.14, 0.62)], Blue);
      k.fill(&[(0.14, 0.62), (0.38, 0.86), (0.08, 0.92)], Ink);
      k.line((0.54, 0.22), (0.78, 0.46), Ink);
    }
    Icon::NamedView => {
      // a picture in its frame: a hill and the sun
      k.solid(&[(0.08, 0.16), (0.92, 0.16), (0.92, 0.84), (0.08, 0.84)], White);
      k.fill(&[(0.18, 0.74), (0.42, 0.4), (0.6, 0.62), (0.7, 0.5), (0.82, 0.74)], Blue);
      k.circle((0.68, 0.33), 0.07, Some(Blue), None);
    }
    Icon::GridSettings => {
      // a grid with a gear at its lower right
      for i in 0..3 {
        let t = 0.1 + i as f32 * 0.3;
        k.line((t, 0.1), (t, 0.7), Ink);
        k.line((0.1, t), (0.7, t), Ink);
      }
      for i in 0..8 {
        let a = i as f32 * TAU / 8.0;
        let (c, s) = (a.cos(), a.sin());
        k.thick((0.72 + c * 0.13, 0.72 + s * 0.13), (0.72 + c * 0.2, 0.72 + s * 0.2), Blue, 1.6);
      }
      k.circle((0.72, 0.72), 0.14, Some(Blue), Some(Ink));
      k.circle((0.72, 0.72), 0.05, Some(White), None);
    }
    Icon::SetIncrements => {
      // a ruler with an arrow stepping along it
      k.solid(&[(0.08, 0.56), (0.92, 0.56), (0.92, 0.86), (0.08, 0.86)], White);
      for i in 0..7 {
        let x = 0.16 + i as f32 * 0.113;
        let h = if i % 2 == 0 { 0.7 } else { 0.64 };
        k.line((x, 0.56), (x, h), Ink);
      }
      k.line((0.14, 0.3), (0.76, 0.3), Blue);
      k.arrow_head((0.86, 0.3), (1.0, 0.0), 0.16, Blue);
      k.line((0.14, 0.18), (0.14, 0.42), Ink);
    }
    Icon::ZoomWindow => {
      for (a, b) in [
        ((0.06, 0.06), (0.94, 0.06)),
        ((0.94, 0.06), (0.94, 0.94)),
        ((0.94, 0.94), (0.06, 0.94)),
        ((0.06, 0.94), (0.06, 0.06)),
      ] {
        k.dashed(a, b, Ink);
      }
      k.circle((0.44, 0.44), 0.2, Some(White), Some(Ink));
      k.thick((0.59, 0.59), (0.78, 0.78), Ink, 2.0);
      k.line((0.34, 0.44), (0.54, 0.44), Ink);
      k.line((0.44, 0.34), (0.44, 0.54), Ink);
    }
    // a coil's turn seen down its axis: three quarters of a circle and the arrow at its end
    Icon::RotateCw => {
      k.arc((0.5, 0.52), 0.32, 2.4, -2.4, Blue);
      k.arrow_head((0.26, 0.74), (-0.68, -0.74), 0.18, Blue);
    }
    Icon::RotateCcw => {
      k.arc((0.5, 0.52), 0.32, 0.74, 5.54, Blue);
      k.arrow_head((0.74, 0.74), (0.68, -0.74), 0.18, Blue);
    }
    Icon::ToNext => {
      // a wall grown down until it meets the floor below
      k.solid(&[(0.08, 0.7), (0.92, 0.7), (0.92, 0.92), (0.08, 0.92)], Fill);
      k.solid(&[(0.38, 0.08), (0.62, 0.08), (0.62, 0.42), (0.38, 0.42)], BlueLight);
      k.thick((0.5, 0.44), (0.5, 0.6), Ink, 1.0);
      k.arrow_head((0.5, 0.68), (0.0, 1.0), 0.13, Ink);
    }
    Icon::EmbossRaise => {
      // a letter-shaped block standing on the face
      k.solid(&[(0.06, 0.66), (0.94, 0.66), (0.94, 0.9), (0.06, 0.9)], Light);
      k.block(0.3, 0.3, 0.7, 0.66, 0.14, BlueLight, Blue, BlueDark);
    }
    Icon::EmbossSink => {
      // the same block sunk into the face
      k.solid(&[(0.06, 0.4), (0.94, 0.4), (0.94, 0.9), (0.06, 0.9)], Light);
      k.solid(&[(0.3, 0.4), (0.7, 0.4), (0.7, 0.66), (0.3, 0.66)], White);
      k.closed(&[(0.3, 0.4), (0.7, 0.4), (0.7, 0.66), (0.3, 0.66)], Blue);
    }
    Icon::ThreadsLibrary => {
      // a cloud
      k.circle((0.36, 0.56), 0.18, Some(BlueLight), Some(Blue));
      k.circle((0.6, 0.46), 0.22, Some(BlueLight), Some(Blue));
      k.solid(&[(0.2, 0.6), (0.82, 0.6), (0.82, 0.74), (0.2, 0.74)], BlueLight);
      k.line((0.2, 0.74), (0.82, 0.74), Blue);
    }
    Icon::PathPattern => {
      // copies along a curve
      k.sk(&[(0.08, 0.82), (0.3, 0.5), (0.5, 0.42), (0.7, 0.5), (0.92, 0.2)], Slate);
      for (i, (x, y)) in [(0.12, 0.72), (0.46, 0.36), (0.8, 0.28)].into_iter().enumerate() {
        k.solid(
          &[(x - 0.1, y - 0.1), (x + 0.1, y - 0.1), (x + 0.1, y + 0.1), (x - 0.1, y + 0.1)],
          if i == 0 { BlueDark } else { Blue },
        );
      }
    }
    Icon::OriginSimple => {
      // an origin snapped on a face
      k.solid(&[(0.08, 0.62), (0.56, 0.4), (0.92, 0.6), (0.44, 0.84)], BlueLight);
      k.thick((0.5, 0.62), (0.5, 0.12), Blue, 1.3);
      k.circle((0.5, 0.62), 0.07, Some(Blue), None);
    }
    Icon::OriginBetween => {
      // midway between two faces
      k.solid(&[(0.08, 0.3), (0.56, 0.12), (0.92, 0.28), (0.44, 0.46)], BlueLight);
      k.solid(&[(0.08, 0.72), (0.56, 0.54), (0.92, 0.7), (0.44, 0.88)], BlueLight);
      k.dashed((0.5, 0.3), (0.5, 0.7), Ink);
      k.circle((0.5, 0.5), 0.07, Some(Blue), None);
    }
    Icon::OriginTwoEdge => {
      // where two edges cross
      k.thick((0.12, 0.2), (0.88, 0.8), Ink, 1.2);
      k.thick((0.12, 0.8), (0.88, 0.2), Ink, 1.2);
      k.circle((0.5, 0.5), 0.08, Some(Blue), None);
    }
    Icon::DraftFixedPlane => {
      // a block tapering down from the fixed plane across its top
      k.solid(&[(0.26, 0.32), (0.74, 0.32), (0.9, 0.9), (0.1, 0.9)], Fill);
      k.thick((0.04, 0.32), (0.96, 0.32), Blue, 1.4);
    }
    Icon::DraftPartingLine => {
      // two tapers meeting at the parting line
      k.solid(&[(0.22, 0.1), (0.78, 0.1), (0.92, 0.5), (0.08, 0.5)], Fill);
      k.solid(&[(0.08, 0.5), (0.92, 0.5), (0.78, 0.9), (0.22, 0.9)], Fill);
      k.thick((0.04, 0.5), (0.96, 0.5), Blue, 1.4);
    }
    Icon::HemFlat | Icon::HemOpen => {
      // folded flat onto the sheet, or back with a gap
      let g = if icon == Icon::HemFlat { 0.08 } else { 0.18 };
      let mut pts = vec![(0.06, 0.5 + g)];
      pts.extend(unit_arc((0.74, 0.5), g, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2));
      pts.push((0.3, 0.5 - g));
      k.band(k.pts(&pts), 3.4);
    }
    Icon::HemRolled | Icon::HemTeardrop | Icon::HemRope => {
      use std::f32::consts::{FRAC_PI_2, PI};
      // a roll round the sheet's end: three quarters, a teardrop closing on the sheet, a full rope
      let (c, r) = ((0.62, 0.48), 0.24);
      let mut pts = vec![(0.06, 0.48 + r)];
      let sweep = match icon {
        Icon::HemRolled => 1.5 * PI,
        Icon::HemTeardrop => 1.75 * PI,
        _ => 2.1 * PI,
      };
      pts.extend(unit_arc(c, r, -FRAC_PI_2, -FRAC_PI_2 + sweep));
      if icon == Icon::HemTeardrop {
        pts.push((0.44, 0.48 + r - 0.04));
      }
      k.band(k.pts(&pts), 3.4);
    }
    Icon::BaseFlange => {
      // a flat sheet seen from above, its thickness along the front
      k.solid(&[(0.06, 0.52), (0.44, 0.3), (0.94, 0.48), (0.56, 0.72)], BlueLight);
      k.solid(&[(0.06, 0.52), (0.56, 0.72), (0.56, 0.82), (0.06, 0.62)], Blue);
      k.solid(&[(0.56, 0.72), (0.94, 0.48), (0.94, 0.58), (0.56, 0.82)], BlueDark);
    }
    Icon::EdgeFlange => {
      // the sheet with a wall bent up along its back edge
      k.solid(&[(0.06, 0.62), (0.44, 0.44), (0.94, 0.6), (0.56, 0.8)], BlueLight);
      k.solid(&[(0.06, 0.62), (0.56, 0.8), (0.56, 0.88), (0.06, 0.7)], Blue);
      k.solid(&[(0.44, 0.44), (0.44, 0.08), (0.94, 0.24), (0.94, 0.6)], Blue);
    }
    Icon::Sun => {
      for i in 0..8 {
        let a = i as f32 * TAU / 8.0;
        let (c, s) = (a.cos(), a.sin());
        k.thick((0.5 + c * 0.3, 0.5 + s * 0.3), (0.5 + c * 0.44, 0.5 + s * 0.44), Amber, 1.4);
      }
      k.circle((0.5, 0.5), 0.2, Some(Amber), Some(Ink));
    }
    Icon::Teapot => {
      // body, lid knob, spout and handle
      k.ellipse((0.48, 0.6), 0.28, 0.24, Some(Blue), Ink);
      k.solid(&[(0.34, 0.38), (0.62, 0.38), (0.56, 0.3), (0.4, 0.3)], BlueLight);
      k.circle((0.48, 0.25), 0.05, Some(Ink), None);
      k.thick((0.72, 0.62), (0.94, 0.38), Ink, 1.6);
      k.arc((0.2, 0.6), 0.12, PI * 0.5, PI * 1.5, Ink);
    }
    Icon::Stop => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      k.fill(&[(0.34, 0.34), (0.66, 0.34), (0.66, 0.66), (0.34, 0.66)], Red);
    }
    Icon::DielectricPriority => {
      // a glass, with the priorities 0, 1 and 2 beside it
      k.solid(&[(0.08, 0.12), (0.5, 0.12), (0.44, 0.9), (0.14, 0.9)], BlueLight);
      k.line((0.12, 0.4), (0.47, 0.4), Blue);
      k.text((0.74, 0.18), "0", 0.26, Ink);
      k.text((0.74, 0.5), "1", 0.26, Ink);
      k.text((0.74, 0.82), "2", 0.26, Ink);
    }
    Icon::TextureMap => {
      k.solid(&[(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9)], White);
      for (x, y) in [(0.1, 0.1), (0.5, 0.1), (0.3, 0.3), (0.7, 0.3), (0.1, 0.5), (0.5, 0.5), (0.3, 0.7), (0.7, 0.7)] {
        k.fill(&[(x, y), (x + 0.2, y), (x + 0.2, y + 0.2), (x, y + 0.2)], Blue);
      }
      for c in [(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9)] {
        k.circle(c, 0.07, Some(White), Some(Ink));
      }
    }
    Icon::InCanvasSettings => {
      k.solid(&[(0.06, 0.12), (0.94, 0.12), (0.94, 0.72), (0.06, 0.72)], White);
      k.line((0.5, 0.72), (0.5, 0.88), Ink);
      k.line((0.3, 0.9), (0.7, 0.9), Ink);
      for i in 0..6 {
        let a = i as f32 * TAU / 6.0;
        k.thick((0.62, 0.42), (0.62 + a.cos() * 0.2, 0.42 + a.sin() * 0.2), Ink, 1.6);
      }
      k.circle((0.62, 0.42), 0.13, Some(Blue), Some(Ink));
      k.circle((0.62, 0.42), 0.05, Some(White), None);
    }
    Icon::Camera => {
      k.solid(&[(0.06, 0.3), (0.94, 0.3), (0.94, 0.86), (0.06, 0.86)], Fill);
      k.solid(&[(0.34, 0.18), (0.62, 0.18), (0.68, 0.3), (0.28, 0.3)], Fill);
      k.circle((0.5, 0.58), 0.2, Some(Blue), Some(Ink));
      k.circle((0.5, 0.58), 0.08, Some(BlueLight), None);
    }
    Icon::Storyboard => {
      // a film strip: three frames between sprocket rows
      k.solid(&[(0.04, 0.22), (0.96, 0.22), (0.96, 0.78), (0.04, 0.78)], Dark);
      for x in [0.1, 0.4, 0.7] {
        k.solid(&[(x, 0.34), (x + 0.22, 0.34), (x + 0.22, 0.66), (x, 0.66)], BlueLight);
      }
      k.thick((0.82, 0.02), (0.82, 0.22), Blue, 1.2);
      k.thick((0.72, 0.12), (0.92, 0.12), Blue, 1.2);
    }
    Icon::TransformComponents => {
      k.block(0.22, 0.42, 0.62, 0.86, 0.18, BlueLight, Blue, BlueDark);
      k.thick((0.42, 0.3), (0.42, 0.08), Ink, 1.0);
      k.arrow_head((0.42, 0.04), (0.0, -1.0), 0.12, Ink);
      k.thick((0.66, 0.64), (0.9, 0.64), Ink, 1.0);
      k.arrow_head((0.96, 0.64), (1.0, 0.0), 0.12, Ink);
    }
    Icon::RestoreHome => {
      k.solid(&[(0.2, 0.5), (0.8, 0.5), (0.8, 0.9), (0.2, 0.9)], White);
      k.fill(&[(0.06, 0.52), (0.5, 0.12), (0.94, 0.52)], Blue);
      k.solid(&[(0.42, 0.64), (0.58, 0.64), (0.58, 0.9), (0.42, 0.9)], Fill);
    }
    Icon::Explode => {
      k.block(0.34, 0.4, 0.62, 0.66, 0.12, BlueLight, Blue, BlueDark);
      k.block(0.04, 0.08, 0.26, 0.28, 0.08, BlueLight, Blue, BlueDark);
      k.block(0.72, 0.76, 0.92, 0.96, 0.06, BlueLight, Blue, BlueDark);
      k.dashed((0.28, 0.3), (0.38, 0.38), Ink);
      k.dashed((0.66, 0.68), (0.74, 0.76), Ink);
    }
    Icon::ManualExplode => {
      k.block(0.08, 0.5, 0.42, 0.9, 0.12, BlueLight, Blue, BlueDark);
      k.block(0.6, 0.18, 0.86, 0.46, 0.1, BlueLight, Blue, BlueDark);
      k.thick((0.36, 0.42), (0.56, 0.3), Ink, 1.0);
      k.arrow_head((0.6, 0.27), (0.85, -0.5), 0.12, Ink);
    }
    Icon::Callout => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.62), (0.08, 0.62)], White);
      k.fill(&[(0.22, 0.62), (0.42, 0.62), (0.18, 0.9)], Ink);
      for y in [0.26, 0.38, 0.5] {
        k.line((0.2, y), (0.8, y), Blue);
      }
    }
    Icon::AnimView => {
      // a camera body with its lens
      k.solid(&[(0.06, 0.3), (0.94, 0.3), (0.94, 0.84), (0.06, 0.84)], Dark);
      k.solid(&[(0.3, 0.18), (0.56, 0.18), (0.56, 0.3), (0.3, 0.3)], Dark);
      k.circle((0.5, 0.57), 0.18, Some(BlueLight), Some(White));
    }
    Icon::Assembly => {
      k.block(0.04, 0.5, 0.42, 0.9, 0.16, White, White, Light);
      k.block(0.5, 0.34, 0.8, 0.66, 0.14, White, White, Light);
      k.circle((0.23, 0.7), 0.07, Some(Blue), None);
      k.circle((0.65, 0.5), 0.07, Some(Blue), None);
    }
    Icon::Video => {
      k.solid(&[(0.06, 0.36), (0.94, 0.36), (0.94, 0.92), (0.06, 0.92)], Dark);
      k.solid(&[(0.06, 0.14), (0.9, 0.04), (0.92, 0.2), (0.08, 0.3)], Light);
      k.fill(&[(0.4, 0.48), (0.66, 0.64), (0.4, 0.8)], White);
    }
    Icon::OpMerge => {
      k.solid(&[(0.06, 0.2), (0.46, 0.2), (0.46, 0.8), (0.06, 0.8)], BlueLight);
      k.solid(&[(0.54, 0.2), (0.94, 0.2), (0.94, 0.8), (0.54, 0.8)], BlueLight);
      k.closed(&[(0.06, 0.2), (0.94, 0.2), (0.94, 0.8), (0.06, 0.8)], Blue);
      k.line((0.5, 0.2), (0.5, 0.8), Blue);
    }
  }
}

/// Points along an arc in unit-square coordinates, angles counter-clockwise from +x (screen y up).
fn unit_arc(c: (f32, f32), r: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
  (0..=16).map(|i| a0 + (a1 - a0) * i as f32 / 16.0).map(|t| (c.0 + t.cos() * r, c.1 - t.sin() * r)).collect()
}

/// A five-pointed star's outline (unit-square coordinates), its first point straight up.
fn star_pts(c: (f32, f32), outer: f32, inner: f32) -> Vec<(f32, f32)> {
  (0..10)
    .map(|i| {
      let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
      let r = if i % 2 == 0 { outer } else { inner };
      (c.0 + a.cos() * r, c.1 + a.sin() * r)
    })
    .collect()
}

/// A large toolbar icon (Fusion shows toolbar commands as icons only; the name is the tooltip),
/// its solids in the `tint` family. The caller adds the tooltip.
pub fn tool_button_bare(ui: &mut egui::Ui, icon: Icon, tint: Tint, active: bool) -> egui::Response {
  tool_button_wide(ui, icon, tint, active, TOOL_BUTTON_W)
}

/// A toolbar icon cell: Fusion's 32 px icon in a 40 px cell.
pub const TOOL_BUTTON_W: f32 = 40.0;

/// [`tool_button_bare`] stretched to `width`, the icon centred: the big FINISH SKETCH button,
/// which Fusion makes clickable across the whole panel.
pub fn tool_button_wide(ui: &mut egui::Ui, icon: Icon, tint: Tint, active: bool, width: f32) -> egui::Response {
  let size = vec2(width.max(TOOL_BUTTON_W), 40.0);
  let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
  let ic = Rect::from_center_size(rect.center(), Vec2::splat(32.0));
  if active {
    // Fusion shows the running tool as a white icon on solid blue
    ui.painter().rect_filled(rect, 2.0, theme::ACCENT);
    draw(ui.painter(), ic, icon, Color32::WHITE);
  } else {
    if resp.hovered() {
      ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
    }
    draw_tinted(ui.painter(), ic, icon, tint);
  }
  resp
}

/// A small square icon-only button (constraints, timeline, chrome). `color` None draws the
/// icon in colour.
pub fn icon_button(
  ui: &mut egui::Ui,
  icon: Icon,
  size: f32,
  hint: &str,
  active: bool,
  color: Option<Color32>,
) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
  if active || resp.hovered() {
    let (fill, stroke) = if active { (theme::SELECTED, theme::ACCENT) } else { (theme::HOVER, theme::SELECTED) };
    ui.painter().rect(rect, 2.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  }
  let ir = rect.shrink(size * 0.18);
  match color {
    Some(c) => draw(ui.painter(), ir, icon, c),
    None => draw_color(ui.painter(), ir, icon),
  }
  if hint.is_empty() { resp } else { resp.on_hover_text(hint) }
}

/// A small downward chevron.
pub fn chevron(p: &Painter, c: Pos2, color: Color32) {
  p.add(Shape::convex_polygon(
    vec![c + vec2(-3.5, -1.5), c + vec2(3.5, -1.5), c + vec2(0.0, 2.5)],
    color,
    Stroke::NONE,
  ));
}

mod mesh;
#[cfg(test)]
mod sheet;
