//! Feature dialogs (the floating panels on the right), with live preview.
//!
//! A preview is a real command applied to the session; changing a value withdraws it (undo) and
//! applies the new one, so OK just keeps the last preview and Cancel withdraws it — exactly one
//! undo step per feature, as in Fusion.

use clf_engine::model::{AxisRef, BodyId, ExtentKind, FeatureId, FeatureKind, Operation, PlaneRef, Primitive};
use clf_engine::{Sel, Session};
use egui::RichText;
use serde_json::{Value, json};

use crate::icons::Icon;
use crate::theme;
use crate::{App, panels};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  PickPlane,
  Extrude,
  Revolve,
  Fillet,
  Chamfer,
  Shell,
  Box,
  Cylinder,
  Sphere,
  Torus,
  RectPattern,
  CircPattern,
  Mirror,
  Combine,
  Move,
  OffsetPlane,
  Inspect,
  Loft,
  Sweep,
  Hole,
  PressPull,
  Draft,
  SplitBody,
  // sheet metal
  Flange,
  Hem,
  Fold,
  ConvertSheet,
  Rip,
  Unfold,
  SheetRules,
  FlatPattern,
  // assembly
  NewComponent,
  Joint,
  Material,
  Interference,
  Bom,
  // sketch modify (run on the sketch being edited)
  SkFillet,
  SkChamfer,
  SkChamfer2,
  SkOffset,
  SkMirror,
  SkCircPattern,
  SkRectPattern,
  Project,
}

impl Kind {
  fn title(self) -> &'static str {
    match self {
      Kind::PickPlane => "CREATE SKETCH",
      Kind::Extrude => "EXTRUDE",
      Kind::Revolve => "REVOLVE",
      Kind::Fillet => "FILLET",
      Kind::Chamfer => "CHAMFER",
      Kind::Shell => "SHELL",
      Kind::Box => "BOX",
      Kind::Cylinder => "CYLINDER",
      Kind::Sphere => "SPHERE",
      Kind::Torus => "TORUS",
      Kind::RectPattern => "RECTANGULAR PATTERN",
      Kind::CircPattern => "CIRCULAR PATTERN",
      Kind::Mirror => "MIRROR",
      Kind::Combine => "COMBINE",
      Kind::Move => "MOVE / ROTATE",
      Kind::OffsetPlane => "OFFSET PLANE",
      Kind::Inspect => "PROPERTIES",
      Kind::Loft => "LOFT",
      Kind::Sweep => "SWEEP",
      Kind::Hole => "HOLE",
      Kind::PressPull => "PRESS PULL",
      Kind::Draft => "DRAFT",
      Kind::SplitBody => "SPLIT BODY",
      Kind::Flange => "FLANGE",
      Kind::Hem => "HEM",
      Kind::Fold => "FOLD",
      Kind::ConvertSheet => "CONVERT TO SHEET METAL",
      Kind::Rip => "RIP",
      Kind::Unfold => "UNFOLD",
      Kind::SheetRules => "SHEET METAL RULES",
      Kind::FlatPattern => "FLAT PATTERN",
      Kind::NewComponent => "NEW COMPONENT",
      Kind::Joint => "JOINT",
      Kind::Material => "PHYSICAL MATERIAL",
      Kind::Interference => "INTERFERENCE",
      Kind::Bom => "BILL OF MATERIALS",
      Kind::SkFillet => "SKETCH FILLET",
      Kind::SkChamfer | Kind::SkChamfer2 => "SKETCH CHAMFER",
      Kind::SkOffset => "OFFSET",
      Kind::SkMirror => "SKETCH MIRROR",
      Kind::SkCircPattern => "CIRCULAR PATTERN",
      Kind::SkRectPattern => "RECTANGULAR PATTERN",
      Kind::Project => "PROJECT",
    }
  }

  fn icon(self) -> Icon {
    match self {
      Kind::PickPlane => Icon::Sketch,
      Kind::Extrude => Icon::Extrude,
      Kind::Revolve => Icon::Revolve,
      Kind::Fillet => Icon::Fillet,
      Kind::Chamfer => Icon::Chamfer,
      Kind::Shell => Icon::Shell,
      Kind::Box => Icon::Box,
      Kind::Cylinder => Icon::Cylinder,
      Kind::Sphere => Icon::Sphere,
      Kind::Torus => Icon::Torus,
      Kind::RectPattern => Icon::Pattern,
      Kind::CircPattern => Icon::CircPattern,
      Kind::Mirror => Icon::Mirror,
      Kind::Combine => Icon::Combine,
      Kind::Move => Icon::Move,
      Kind::OffsetPlane => Icon::Plane,
      Kind::Inspect => Icon::Measure,
      Kind::Loft => Icon::Loft,
      Kind::Sweep => Icon::Sweep,
      Kind::Hole => Icon::Hole,
      Kind::PressPull => Icon::PressPull,
      Kind::Draft => Icon::Draft,
      Kind::SplitBody => Icon::SplitBody,
      Kind::Flange => Icon::Flange,
      Kind::Hem => Icon::Hem,
      Kind::Fold => Icon::Fold,
      Kind::ConvertSheet => Icon::ConvertSheet,
      Kind::Rip => Icon::Rip,
      Kind::Unfold => Icon::Unfold,
      Kind::SheetRules => Icon::SmRules,
      Kind::FlatPattern => Icon::FlatPattern,
      Kind::NewComponent => Icon::Component,
      Kind::Joint => Icon::JointOrigin,
      Kind::Material => Icon::PhysMaterial,
      Kind::Interference => Icon::Interference,
      Kind::Bom => Icon::Bom,
      Kind::SkFillet => Icon::SkFillet,
      Kind::SkChamfer | Kind::SkChamfer2 => Icon::SkChamfer,
      Kind::SkOffset => Icon::SkOffset,
      Kind::SkMirror => Icon::SkMirror,
      Kind::SkCircPattern => Icon::SkCircPattern,
      Kind::SkRectPattern => Icon::SkRectPattern,
      Kind::Project => Icon::Project,
    }
  }

  /// Icon, name and tooltip of the dialogs added with the loft … sketch-modify commands.
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    let (name, hint) = match self {
      Kind::Loft => ("Loft", "Blend through profiles on two or more sketches"),
      Kind::Sweep => ("Sweep", "Sweep a profile along a path sketch"),
      Kind::Hole => ("Hole", "Drill holes at a sketch's points (H)"),
      Kind::PressPull => ("Press Pull", "Push or pull faces along their normals (Q)"),
      Kind::Draft => ("Draft", "Taper faces about a pull direction"),
      Kind::SplitBody => ("Split Body", "Split bodies along a plane or planar face"),
      Kind::Flange => ("Flange", "Sheet metal from a profile, an open path or sheet edges"),
      Kind::Hem => ("Hem", "Fold sheet edges back on themselves"),
      Kind::Fold => ("Fold", "Bend a flat sheet along a sketch line"),
      Kind::ConvertSheet => ("Convert to Sheet Metal", "Turn a thin solid into sheet metal"),
      Kind::Rip => ("Rip", "Slit a solid along edges so it can unfold"),
      Kind::Unfold => ("Unfold", "Flatten a sheet metal body onto a face"),
      Kind::SheetRules => ("Sheet Metal Rules", "Thickness, bend radius and reliefs of sheet metal"),
      Kind::FlatPattern => ("Create Flat Pattern", "Flat pattern size and bends, and DXF export"),
      Kind::NewComponent => ("New Component", "Make a component from the selected bodies"),
      Kind::Joint => ("Joint", "Join two components at a face or edge of each"),
      Kind::Material => ("Physical Material", "Set a component's material for mass and the BOM"),
      Kind::Interference => ("Interference", "Find overlapping bodies of different components"),
      Kind::Bom => ("Bill of Materials", "Components, materials and masses"),
      Kind::SkFillet => ("Fillet", "Round the corner where two lines meet"),
      Kind::SkChamfer => ("Equal Distance Chamfer", "Bevel the corner where two lines meet"),
      Kind::SkChamfer2 => ("Distance-Distance Chamfer", "Bevel a corner by two distances"),
      Kind::SkOffset => ("Offset", "Offset a chain of curves (O)"),
      Kind::SkMirror => ("Mirror", "Mirror sketch curves across a line"),
      Kind::SkCircPattern => ("Circular Pattern", "Copy sketch curves around a point"),
      Kind::SkRectPattern => ("Rectangular Pattern", "Copy sketch curves in rows and columns"),
      Kind::Project => ("Project", "Project a body's or face's edges into the sketch"),
      _ => (self.title(), ""),
    };
    (self.icon(), name, hint)
  }

  /// Dialogs whose input is picked edges/faces/bodies of the model: they preview on demand,
  /// because a preview changes the very geometry being picked.
  fn picks_model(self) -> bool {
    matches!(
      self,
      Kind::Fillet
        | Kind::Chamfer
        | Kind::Shell
        | Kind::Combine
        | Kind::PressPull
        | Kind::Draft
        | Kind::Hem
        | Kind::Rip
        | Kind::ConvertSheet
        | Kind::Unfold
        | Kind::Joint
    )
  }

  /// Dialogs that only show information (no OK, nothing applied).
  fn reports(self) -> bool {
    matches!(self, Kind::Inspect | Kind::FlatPattern | Kind::Interference | Kind::Bom)
  }

  /// Dialogs that change the sketch being edited.
  pub fn sketch_edit(self) -> bool {
    matches!(
      self,
      Kind::SkFillet
        | Kind::SkChamfer
        | Kind::SkChamfer2
        | Kind::SkOffset
        | Kind::SkMirror
        | Kind::SkCircPattern
        | Kind::SkRectPattern
        | Kind::Project
    )
  }
}

const OPS: [(&str, &str); 4] = [("new_body", "New body"), ("join", "Join"), ("cut", "Cut"), ("intersect", "Intersect")];
const EXTENTS: [(&str, &str); 4] =
  [("one_side", "One side"), ("symmetric", "Symmetric"), ("two_sides", "Two sides"), ("through_all", "All")];

pub struct Dialog {
  pub kind: Kind,
  /// Feature being edited (timeline double-click); None while creating.
  pub editing: Option<FeatureId>,
  /// A preview step is applied to the session.
  applied: bool,
  error: Option<String>,
  dirty: bool,
  /// Set by a field that finished the dialog itself (e.g. picking a sketch plane).
  closed: bool,
  // captured inputs
  pub sketch: Option<FeatureId>,
  profiles: Vec<[f64; 2]>,
  edges: Vec<Value>,
  faces: Vec<Value>,
  bodies: Vec<u32>,
  plane: Value,
  plane_label: String,
  // fields
  distance: String,
  distance2: String,
  extent: &'static str,
  flip: bool,
  op: &'static str,
  angle: String,
  axis: String,
  radius: String,
  count: String,
  spacing: String,
  dir: String,
  use_dir2: bool,
  count2: String,
  spacing2: String,
  dir2: String,
  join: bool,
  a: String,
  b: String,
  c: String,
  at: [String; 2],
  translate: [String; 3],
  keep_tools: bool,
  /// Loft sections: sketch and a point inside the profile.
  sections: Vec<(u32, [f64; 2])>,
  /// Sweep path sketch.
  path: Option<FeatureId>,
  /// Picked sketch curves.
  curves: Vec<u32>,
  /// Mirror / fold line.
  line: Option<u32>,
  /// Joint origins in pick order: body and `{"face"|"edge": ...}`.
  origins: Vec<(u32, Value)>,
  /// Components of the picked bodies / joint origins (None = a root body).
  comps: Vec<Option<u32>>,
  /// Hole type, flange type or joint type.
  choice: &'static str,
  /// Edge flange position / sheet metal relief shape.
  position: &'static str,
  rule: String,
  rules: Vec<String>,
  flag: bool,
  flag2: bool,
  d: String,
  e: String,
  name: String,
  /// Output of report dialogs (flat pattern, interference, BOM).
  report: Option<Value>,
}

impl Dialog {
  fn blank(kind: Kind) -> Dialog {
    Dialog {
      kind,
      editing: None,
      applied: false,
      error: None,
      dirty: false,
      closed: false,
      sketch: None,
      profiles: Vec::new(),
      edges: Vec::new(),
      faces: Vec::new(),
      bodies: Vec::new(),
      plane: json!("xy"),
      plane_label: "XY plane".into(),
      distance: "10 mm".into(),
      distance2: "10 mm".into(),
      extent: "one_side",
      flip: false,
      op: "new_body",
      angle: "360 deg".into(),
      axis: "z".into(),
      radius: "2 mm".into(),
      count: "3".into(),
      spacing: "20 mm".into(),
      dir: "x".into(),
      use_dir2: false,
      count2: "2".into(),
      spacing2: "20 mm".into(),
      dir2: "y".into(),
      join: true,
      a: "20 mm".into(),
      b: "20 mm".into(),
      c: "20 mm".into(),
      at: ["0".into(), "0".into()],
      translate: ["0".into(), "0".into(), "0".into()],
      keep_tools: false,
      sections: Vec::new(),
      path: None,
      curves: Vec::new(),
      line: None,
      origins: Vec::new(),
      comps: Vec::new(),
      choice: "",
      position: "",
      rule: String::new(),
      rules: Vec::new(),
      flag: false,
      flag2: false,
      d: String::new(),
      e: String::new(),
      name: String::new(),
      report: None,
    }
  }

  /// Does this dialog preview on demand (see [`Kind::picks_model`])?
  fn picks(&self) -> bool {
    self.kind.picks_model() || (self.kind == Kind::Flange && self.choice == "edge")
  }

  /// Opens a dialog for a new feature, capturing the current selection.
  pub fn open(app: &mut App, kind: Kind) -> Dialog {
    let mut d = Dialog::blank(kind);
    let s = &app.s;
    // profiles → sketch
    for sel in &s.selection {
      if let Sel::Profile { sketch, index } = sel
        && let Some(r) = s.model.sketches.get(sketch)
        && let Some(p) = r.profiles.get(*index)
      {
        d.sketch = Some(*sketch);
        d.profiles.push(p.interior_point());
      }
    }
    if matches!(kind, Kind::Extrude | Kind::Revolve | Kind::Sweep | Kind::Hole | Kind::Flange | Kind::Fold)
      && d.sketch.is_none()
    {
      d.sketch = s.edit_sketch.or(app.last_sketch).or_else(|| last_unconsumed_sketch(s));
      if let Some(sk) = d.sketch
        && let Some(r) = s.model.sketches.get(&sk)
        && r.profiles.len() == 1
      {
        d.profiles = vec![r.profiles[0].interior_point()];
      }
    }
    for sel in &s.selection {
      match sel {
        Sel::Edge { body, edge } => d.edges.push(json!({"body": body.0, "edge": edge})),
        Sel::Face { body, face } => {
          d.faces.push(json!({"body": body.0, "face": face}));
          if !d.bodies.contains(&body.0) {
            d.bodies.push(body.0);
          }
        }
        Sel::Body { body } if !d.bodies.contains(&body.0) => {
          d.bodies.push(body.0);
        }
        _ => {}
      }
    }
    // a selected planar face becomes the plane for primitives / offset planes / mirror
    if let Some(Sel::Face { body, face }) = s.selection.iter().find(|x| matches!(x, Sel::Face { .. }))
      && is_planar(s, *body, *face)
    {
      d.plane = json!({"body": body.0, "face": face});
      d.plane_label = format!("Face of {}", s.body_name(*body));
    }
    match kind {
      Kind::Fillet => d.radius = "2 mm".into(),
      Kind::Chamfer => d.radius = "1 mm".into(),
      Kind::Shell => d.radius = "2 mm".into(),
      Kind::Cylinder => {
        d.a = "10 mm".into();
        d.b = "20 mm".into();
      }
      Kind::Sphere => d.a = "10 mm".into(),
      Kind::Torus => {
        d.a = "20 mm".into();
        d.b = "5 mm".into();
      }
      Kind::Revolve => d.axis = "z".into(),
      Kind::Mirror => d.join = false,
      Kind::RectPattern | Kind::CircPattern => d.join = false,
      Kind::Combine => d.op = "join",
      Kind::Loft => {
        d.sections = sel_sections(s);
      }
      Kind::Sweep => {
        d.path = s
          .design
          .features
          .iter()
          .take(s.design.active_len())
          .rev()
          .find(|f| matches!(f.kind, FeatureKind::Sketch { .. }) && Some(f.id) != d.sketch)
          .map(|f| f.id);
      }
      Kind::Hole => {
        d.choice = "simple";
        d.a = "5 mm".into();
        d.b = "10 mm".into();
        d.c = "9 mm".into();
        d.d = "3 mm".into();
        d.e = "90 deg".into();
        d.op = "cut";
      }
      Kind::PressPull => d.distance = "2 mm".into(),
      Kind::Draft => {
        d.angle = "3 deg".into();
        d.plane = json!("xy");
        d.plane_label = "XY plane".into();
      }
      Kind::SplitBody => d.bodies = sel_only_bodies(s),
      Kind::Flange => {
        // edges picked → edge flange; a sketch of open curves only → contour flange
        let open_only = d.sketch.and_then(|sk| s.model.sketches.get(&sk)).is_some_and(|r| r.profiles.is_empty());
        d.choice = if !d.edges.is_empty() {
          "edge"
        } else if open_only {
          "contour"
        } else {
          "base"
        };
        d.distance = "10 mm".into();
        d.angle = "90 deg".into();
        d.position = "adjacent";
        d.op = "new_body";
      }
      Kind::Hem => {
        d.distance = "5 mm".into();
        d.distance2 = "0.5 mm".into();
      }
      Kind::Fold => {
        d.angle = "90 deg".into();
        d.line = d.sketch.and_then(|sk| s.design.sketch(sk).ok()).and_then(|sk| {
          sk.curves.iter().rev().find(|c| matches!(c.kind, clf_engine::sketch::CurveKind::Line { .. })).map(|c| c.id.0)
        });
      }
      Kind::Rip => d.distance = "0.5 mm".into(),
      Kind::SheetRules => {
        d.load_rule(s, "");
      }
      Kind::NewComponent => {
        d.name = format!("Component{}", s.design.components.len() + 1);
      }
      Kind::Joint => {
        d.choice = "rigid";
        d.angle = "0 deg".into();
        d.distance = "0 mm".into();
        d.origins = sel_origins(s);
      }
      Kind::Material => d.name = "Steel".into(),
      Kind::SkFillet => d.radius = "2 mm".into(),
      Kind::SkChamfer | Kind::SkChamfer2 => {
        d.distance = "2 mm".into();
        d.distance2 = "3 mm".into();
      }
      Kind::SkOffset => d.distance = "2 mm".into(),
      Kind::SkCircPattern => {
        d.count = "6".into();
        d.angle = "360 deg".into();
      }
      Kind::SkRectPattern => {
        d.count = "3".into();
        d.count2 = "2".into();
      }
      Kind::Project => d.flag = true,
      Kind::Extrude => {
        // extruding from a face sketch defaults to Join, like Fusion
        if let Some(sk) = d.sketch
          && let Some(FeatureKind::Sketch { plane: PlaneRef::Face { .. }, .. }) = s.design.feature(sk).map(|f| &f.kind)
        {
          d.op = "join";
        }
      }
      _ => {}
    }
    if matches!(
      kind,
      Kind::SkFillet
        | Kind::SkChamfer
        | Kind::SkChamfer2
        | Kind::SkOffset
        | Kind::SkMirror
        | Kind::SkCircPattern
        | Kind::SkRectPattern
    ) {
      d.curves = sel_curves(s);
      if kind == Kind::SkMirror {
        d.line = mirror_line(s, &d.curves);
        d.curves.retain(|c| Some(*c) != d.line);
      }
    }
    d.comps = d.bodies.iter().map(|b| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
    if kind == Kind::Joint {
      d.comps = d.origins.iter().map(|(b, _)| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
    }
    if matches!(kind, Kind::Flange | Kind::ConvertSheet) {
      d.rules = s.design.sheet_metal_rules.iter().map(|r| r.name.clone()).collect();
    }
    // Fusion finishes the sketch when a feature starts from it
    if matches!(kind, Kind::Extrude | Kind::Revolve | Kind::Loft | Kind::Sweep | Kind::Hole | Kind::Flange | Kind::Fold)
      && app.s.edit_sketch.is_some()
    {
      app.last_sketch = app.s.edit_sketch;
      let _ = app.s.execute("sketch.finish", &json!({}));
    }
    if kind.reports() {
      d.refresh_report(&mut app.s);
    }
    d.dirty = !d.picks();
    d
  }

  /// Opens the dialog for an existing feature (double-click in the timeline).
  pub fn edit(app: &App, id: FeatureId) -> Option<Dialog> {
    let f = app.s.design.feature(id)?;
    let mut d = match &f.kind {
      FeatureKind::Extrude { sketch, extent, operation, .. } => {
        let mut d = Dialog::blank(Kind::Extrude);
        d.sketch = Some(*sketch);
        d.distance = extent.distance.clone();
        d.distance2 = extent.distance2.clone();
        d.flip = extent.flip;
        d.extent = match extent.kind {
          ExtentKind::OneSide => "one_side",
          ExtentKind::Symmetric => "symmetric",
          ExtentKind::TwoSides => "two_sides",
          ExtentKind::ThroughAll => "through_all",
          ExtentKind::ToFace => "to_face",
        };
        d.op = op_str(*operation);
        d
      }
      FeatureKind::Revolve { sketch, angle, operation, axis, .. } => {
        let mut d = Dialog::blank(Kind::Revolve);
        d.sketch = Some(*sketch);
        d.angle = angle.clone();
        d.op = op_str(*operation);
        d.axis = match axis {
          AxisRef::X => "x".into(),
          AxisRef::Y => "y".into(),
          AxisRef::Z => "z".into(),
          AxisRef::SketchLine { curve } => format!("line:{}", curve.0),
          AxisRef::Edge { .. } => "edge".into(),
        };
        d
      }
      FeatureKind::Fillet { radius, edges, .. } => {
        let mut d = Dialog::blank(Kind::Fillet);
        d.radius = radius.clone();
        d.edges = vec![Value::Null; edges.len()];
        d
      }
      FeatureKind::Chamfer { distance, edges, .. } => {
        let mut d = Dialog::blank(Kind::Chamfer);
        d.radius = distance.clone();
        d.edges = vec![Value::Null; edges.len()];
        d
      }
      FeatureKind::Shell { thickness, faces } => {
        let mut d = Dialog::blank(Kind::Shell);
        d.radius = thickness.clone();
        d.faces = vec![Value::Null; faces.len()];
        d
      }
      FeatureKind::Primitive { shape, at, operation, .. } => {
        let (kind, a, b, c) = match shape {
          Primitive::Box { width, depth, height } => (Kind::Box, width.clone(), depth.clone(), height.clone()),
          Primitive::Cylinder { radius, height } => (Kind::Cylinder, radius.clone(), height.clone(), String::new()),
          Primitive::Sphere { radius } => (Kind::Sphere, radius.clone(), String::new(), String::new()),
          Primitive::Torus { major, minor } => (Kind::Torus, major.clone(), minor.clone(), String::new()),
        };
        let mut d = Dialog::blank(kind);
        d.a = a;
        d.b = b;
        d.c = c;
        d.at = [clf_engine::sketch::Dim::new(at[0]).expr, clf_engine::sketch::Dim::new(at[1]).expr];
        d.op = op_str(*operation);
        d
      }
      FeatureKind::ConstructionPlane { offset, .. } => {
        let mut d = Dialog::blank(Kind::OffsetPlane);
        d.distance = offset.clone();
        d
      }
      FeatureKind::RectangularPattern { count, spacing, count2, spacing2, direction2, join, .. } => {
        let mut d = Dialog::blank(Kind::RectPattern);
        d.count = count.clone();
        d.spacing = spacing.clone();
        d.count2 = count2.clone();
        d.spacing2 = spacing2.clone();
        d.use_dir2 = direction2.is_some();
        d.join = *join;
        d
      }
      FeatureKind::CircularPattern { count, angle, join, .. } => {
        let mut d = Dialog::blank(Kind::CircPattern);
        d.count = count.clone();
        d.angle = angle.clone();
        d.join = *join;
        d
      }
      FeatureKind::Move { translate, angle, .. } => {
        let mut d = Dialog::blank(Kind::Move);
        d.translate = translate.clone();
        d.angle = angle.clone();
        d
      }
      _ => return None,
    };
    d.editing = Some(id);
    Some(d)
  }

  /// The command (name, params) this dialog applies.
  fn command(&self) -> Option<(&'static str, Value)> {
    if let Some(id) = self.editing {
      return Some(("feature.edit", json!({"feature": id.0, "set": self.edit_set()?})));
    }
    let op = self.op;
    Some(match self.kind {
      Kind::PickPlane | Kind::Inspect | Kind::FlatPattern | Kind::Interference | Kind::Bom => return None,
      Kind::Loft => (
        "loft",
        json!({
          "sections": self.sections.iter().map(|(sk, p)| json!({"sketch": sk, "profile": p})).collect::<Vec<_>>(),
          "ruled": self.flag, "closed": self.flag2, "operation": op,
        }),
      ),
      Kind::Sweep => {
        ("sweep", json!({"sketch": self.sketch?.0, "profiles": self.profiles, "path": self.path?.0, "operation": op}))
      }
      Kind::Hole => {
        let mut p = json!({
          "sketch": self.sketch?.0, "kind": self.choice, "diameter": self.a, "flip": self.flip,
        });
        if self.flag {
          p["through_all"] = json!(true);
        } else {
          p["depth"] = json!(self.b);
        }
        match self.choice {
          "counterbore" => {
            p["head_diameter"] = json!(self.c);
            p["head_depth"] = json!(self.d);
          }
          "countersink" => {
            p["head_diameter"] = json!(self.c);
            p["head_angle"] = json!(self.e);
          }
          _ => {}
        }
        ("hole", p)
      }
      Kind::PressPull => ("press_pull", json!({"faces": self.faces, "distance": self.distance})),
      Kind::Draft => {
        ("draft", json!({"faces": self.faces, "plane": self.plane, "angle": self.angle, "flip": self.flip}))
      }
      Kind::SplitBody => {
        let mut p = json!({"plane": self.plane});
        if !self.bodies.is_empty() {
          p["bodies"] = json!(self.bodies);
        }
        ("split", p)
      }
      Kind::Flange => match self.choice {
        "edge" => (
          "sheet.edge_flange",
          json!({
            "edges": self.edges, "height": self.distance, "angle": self.angle, "flip": self.flip,
            "position": self.position,
          }),
        ),
        "contour" => {
          let mut p =
            json!({"sketch": self.sketch?.0, "distance": self.distance, "symmetric": self.flag, "flip": self.flip});
          self.put_rule(&mut p);
          ("sheet.contour_flange", p)
        }
        _ => {
          let mut p = json!({"sketch": self.sketch?.0, "profiles": self.profiles, "flip": self.flip, "operation": op});
          self.put_rule(&mut p);
          ("sheet.flange", p)
        }
      },
      Kind::Hem => {
        ("sheet.hem", json!({"edges": self.edges, "length": self.distance, "gap": self.distance2, "flip": self.flip}))
      }
      Kind::Fold => (
        "sheet.bend",
        json!({"sketch": self.sketch?.0, "line": self.line?, "angle": self.angle, "flip": self.flip, "flip_side": self.flag}),
      ),
      Kind::ConvertSheet => {
        let mut p = json!({"body": self.bodies.first()?});
        if let Some(f) = self.faces.first() {
          p["face"] = f.clone();
        }
        self.put_rule(&mut p);
        ("sheet.convert", p)
      }
      Kind::Rip => ("sheet.rip", json!({"edges": self.edges, "gap": self.distance})),
      Kind::Unfold => {
        let mut p = json!({});
        if let Some(b) = self.bodies.first() {
          p["body"] = json!(b);
        }
        if let Some(f) = self.faces.first() {
          p["face"] = f.clone();
        }
        ("sheet.unfold", p)
      }
      Kind::SheetRules => (
        "sheet.rule",
        json!({
          "name": self.name, "thickness": self.a, "bend_radius": self.b, "k_factor": self.c,
          "relief": self.position, "relief_width": self.d, "relief_depth": self.e, "gap": self.distance2,
        }),
      ),
      Kind::NewComponent => ("component.create", json!({"name": self.name, "bodies": self.bodies})),
      Kind::Joint => {
        let (_, mo) = self.origins.first()?;
        let (_, fo) = self.origins.get(1)?;
        (
          "joint",
          json!({
            "moving": (*self.comps.first()?)?, "moving_origin": mo, "fixed": self.comps.get(1).copied().flatten(),
            "fixed_origin": fo, "kind": self.choice, "angle": self.angle, "offset": self.distance, "flip": self.flip,
          }),
        )
      }
      Kind::Material => {
        ("component.material", json!({"component": self.comps.iter().flatten().next()?, "material": self.name}))
      }
      Kind::SkFillet => ("sketch.fillet", json!({"curves": self.curves, "radius": self.radius})),
      Kind::SkChamfer => ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance})),
      Kind::SkChamfer2 => {
        ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance, "distance2": self.distance2}))
      }
      Kind::SkOffset => ("sketch.offset", json!({"curves": self.curves, "distance": self.distance})),
      Kind::SkMirror => ("sketch.mirror", json!({"curves": self.curves, "line": self.line?})),
      Kind::SkCircPattern => (
        "sketch.pattern_circ",
        json!({"curves": self.curves, "center": self.at_pt()?, "count": self.count.trim().parse::<u64>().ok()?, "angle": self.angle}),
      ),
      Kind::SkRectPattern => {
        let v = |a: &str| if a == "y" { [0.0, 1.0] } else { [1.0, 0.0] };
        let mut p = json!({
          "curves": self.curves, "direction": v(&self.dir), "count": self.count.trim().parse::<u64>().ok()?,
          "spacing": self.spacing,
        });
        if self.use_dir2 {
          p["direction2"] = json!(v(&self.dir2));
          p["count2"] = json!(self.count2.trim().parse::<u64>().ok()?);
          p["spacing2"] = json!(self.spacing2);
        }
        ("sketch.pattern_rect", p)
      }
      Kind::Project => {
        let mut p = json!({"body": self.bodies.first()?, "associative": self.flag});
        if let Some(f) = self.faces.first().and_then(|f| f.get("face")) {
          p["face"] = f.clone();
        }
        ("sketch.project", p)
      }
      Kind::Extrude => (
        "extrude",
        json!({
          "sketch": self.sketch?.0, "profiles": self.profiles, "distance": self.distance, "distance2": self.distance2,
          "extent": self.extent, "flip": self.flip, "operation": op,
        }),
      ),
      Kind::Revolve => {
        let axis = match self.axis.strip_prefix("line:") {
          Some(n) => json!({"line": n.parse::<u32>().ok()?}),
          None => json!(self.axis),
        };
        (
          "revolve",
          json!({"sketch": self.sketch?.0, "profiles": self.profiles, "axis": axis, "angle": self.angle, "operation": op}),
        )
      }
      Kind::Fillet => ("fillet", json!({"edges": self.edges, "radius": self.radius})),
      Kind::Chamfer => ("chamfer", json!({"edges": self.edges, "distance": self.radius})),
      Kind::Shell => ("shell", json!({"faces": self.faces, "thickness": self.radius})),
      Kind::Box => (
        "box",
        json!({"plane": self.plane, "at": self.at_pt()?, "width": self.a, "depth": self.b, "height": self.c, "operation": op}),
      ),
      Kind::Cylinder => (
        "cylinder",
        json!({"plane": self.plane, "at": self.at_pt()?, "radius": self.a, "height": self.b, "operation": op}),
      ),
      Kind::Sphere => ("sphere", json!({"plane": self.plane, "at": self.at_pt()?, "radius": self.a, "operation": op})),
      Kind::Torus => {
        ("torus", json!({"plane": self.plane, "at": self.at_pt()?, "major": self.a, "minor": self.b, "operation": op}))
      }
      Kind::OffsetPlane => ("plane.offset", json!({"plane": self.plane, "offset": self.distance})),
      Kind::Mirror => ("mirror", json!({"plane": self.plane, "bodies": self.bodies, "join": self.join})),
      Kind::RectPattern => {
        let mut p = json!({
          "bodies": self.bodies, "direction": axis_vec(&self.dir), "count": self.count, "spacing": self.spacing, "join": self.join,
        });
        if self.use_dir2 {
          p["direction2"] = json!(axis_vec(&self.dir2));
          p["count2"] = json!(self.count2);
          p["spacing2"] = json!(self.spacing2);
        }
        ("pattern.rectangular", p)
      }
      Kind::CircPattern => (
        "pattern.circular",
        json!({"bodies": self.bodies, "axis": self.axis, "count": self.count, "angle": self.angle, "join": self.join}),
      ),
      Kind::Combine => {
        let (target, tools) = self.bodies.split_first()?;
        ("combine", json!({"target": target, "tools": tools, "operation": op, "keep_tools": self.keep_tools}))
      }
      Kind::Move => {
        let mut p = json!({"bodies": self.bodies, "translate": self.translate});
        if !matches!(self.angle.trim(), "" | "0" | "0 deg") {
          p["axis"] = json!(self.axis);
          p["angle"] = json!(self.angle);
        }
        ("move", p)
      }
    })
  }

  /// Fields for `feature.edit` when editing an existing feature.
  fn edit_set(&self) -> Option<Value> {
    Some(match self.kind {
      Kind::Extrude => json!({
        "extent": {"kind": self.extent, "distance": self.distance, "distance2": self.distance2, "flip": self.flip},
        "operation": self.op,
      }),
      Kind::Revolve => json!({"angle": self.angle, "operation": self.op}),
      Kind::Fillet => json!({"radius": self.radius}),
      Kind::Chamfer => json!({"distance": self.radius}),
      Kind::Shell => json!({"thickness": self.radius}),
      Kind::Box => {
        json!({"shape": {"type": "box", "width": self.a, "depth": self.b, "height": self.c}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::Cylinder => {
        json!({"shape": {"type": "cylinder", "radius": self.a, "height": self.b}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::Sphere => json!({"shape": {"type": "sphere", "radius": self.a}, "at": self.at_pt()?, "operation": self.op}),
      Kind::Torus => {
        json!({"shape": {"type": "torus", "major": self.a, "minor": self.b}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::OffsetPlane => json!({"offset": self.distance}),
      Kind::RectPattern => {
        json!({"count": self.count, "spacing": self.spacing, "count2": self.count2, "spacing2": self.spacing2, "join": self.join})
      }
      Kind::CircPattern => json!({"count": self.count, "angle": self.angle, "join": self.join}),
      Kind::Move => json!({"translate": self.translate, "angle": self.angle}),
      _ => return None,
    })
  }

  fn at_pt(&self) -> Option<[f64; 2]> {
    let v = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
    Some([v(&self.at[0]), v(&self.at[1])])
  }

  /// Withdraws the current preview (if any) and applies the dialog's command.
  pub fn preview(&mut self, s: &mut Session) {
    self.dirty = false;
    let selection = s.selection.clone();
    self.withdraw(s);
    if self.kind.reports() {
      self.refresh_report(s);
      return;
    }
    let Some((cmd, params)) = self.command() else {
      return;
    };
    if !self.ready() {
      return;
    }
    match s.execute(cmd, &params) {
      Ok(v) => {
        self.applied = true;
        self.error = v.get("error").and_then(Value::as_str).map(str::to_string);
      }
      Err(e) => self.error = Some(e.0),
    }
    if !self.picks() {
      s.selection = selection;
    }
  }

  /// Enough input to try?
  fn ready(&self) -> bool {
    if self.editing.is_some() {
      return true;
    }
    match self.kind {
      Kind::Extrude | Kind::Revolve => self.sketch.is_some(),
      Kind::Fillet | Kind::Chamfer => !self.edges.is_empty(),
      Kind::Shell => !self.faces.is_empty(),
      Kind::Combine => self.bodies.len() >= 2,
      Kind::Loft => self.sections.len() >= 2,
      Kind::Sweep => self.sketch.is_some() && self.path.is_some(),
      Kind::Hole | Kind::Fold => self.sketch.is_some(),
      Kind::PressPull | Kind::Draft => !self.faces.is_empty(),
      Kind::Flange => {
        if self.choice == "edge" {
          !self.edges.is_empty()
        } else {
          self.sketch.is_some()
        }
      }
      Kind::Hem | Kind::Rip => !self.edges.is_empty(),
      Kind::ConvertSheet | Kind::NewComponent | Kind::Project => !self.bodies.is_empty(),
      Kind::Joint => self.origins.len() >= 2 && self.comps.first().is_some_and(Option::is_some),
      Kind::Material => self.comps.iter().any(Option::is_some),
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 => self.curves.len() == 2,
      Kind::SkMirror => !self.curves.is_empty() && self.line.is_some(),
      Kind::SkOffset | Kind::SkCircPattern | Kind::SkRectPattern => !self.curves.is_empty(),
      _ => true,
    }
  }

  fn withdraw(&mut self, s: &mut Session) {
    if self.applied {
      s.undo();
      s.drop_redo();
      self.applied = false;
      if self.picks() && self.editing.is_none() {
        // the picks are valid again on the un-previewed model
        s.selection = self.picked_selection();
      }
    }
  }

  fn picked_selection(&self) -> Vec<Sel> {
    let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
    let mut out = Vec::new();
    for e in &self.edges {
      if let (Some(b), Some(i)) = (get(e, "body"), get(e, "edge")) {
        out.push(Sel::Edge { body: BodyId(b), edge: i });
      }
    }
    for f in &self.faces {
      if let (Some(b), Some(i)) = (get(f, "body"), get(f, "face")) {
        out.push(Sel::Face { body: BodyId(b), face: i });
      }
    }
    if self.kind == Kind::Combine {
      out.extend(self.bodies.iter().map(|b| Sel::Body { body: BodyId(*b) }));
    }
    if self.kind == Kind::Joint {
      out.clear();
      for (b, o) in &self.origins {
        if let Some(i) = o.get("face").and_then(|f| get(f, "face")) {
          out.push(Sel::Face { body: BodyId(*b), face: i });
        } else if let Some(i) = o.get("edge").and_then(|e| get(e, "edge")) {
          out.push(Sel::Edge { body: BodyId(*b), edge: i });
        }
      }
    }
    out
  }

  pub fn cancel(&mut self, s: &mut Session) {
    self.withdraw(s);
  }

  /// Is a model-picking dialog's preview showing? (Viewport clicks withdraw it first.)
  pub fn previewing_model(&self) -> bool {
    self.applied && self.picks() && self.editing.is_none()
  }

  pub fn unpreview(&mut self, s: &mut Session) {
    self.withdraw(s);
  }

  /// Sketch whose profiles should stay visible/pickable while this dialog is open.
  pub fn profile_sketches(&self) -> Vec<FeatureId> {
    match self.kind {
      Kind::Extrude | Kind::Revolve | Kind::Flange | Kind::Hole | Kind::Fold => self.sketch.into_iter().collect(),
      Kind::Sweep => self.sketch.into_iter().chain(self.path).collect(),
      Kind::Loft => self.sections.iter().map(|(sk, _)| FeatureId(*sk)).collect(),
      _ => Vec::new(),
    }
  }

  /// Picks up selection changes made in the viewport while the dialog is open.
  fn sync_selection(&mut self, s: &Session) {
    if self.editing.is_some() {
      return;
    }
    match self.kind {
      Kind::Extrude | Kind::Revolve | Kind::Sweep | Kind::Flange
        if !(self.kind == Kind::Flange && self.choice == "edge") =>
      {
        let mut pts = Vec::new();
        for sel in &s.selection {
          if let Sel::Profile { sketch, index } = sel
            && let Some(p) = s.model.sketches.get(sketch).and_then(|r| r.profiles.get(*index))
          {
            if self.sketch != Some(*sketch) {
              self.sketch = Some(*sketch);
            }
            pts.push(p.interior_point());
          }
        }
        if !pts.is_empty() && pts != self.profiles {
          self.profiles = pts;
          self.dirty = true;
        }
      }
      Kind::Fillet | Kind::Chamfer if !self.applied => {
        let e: Vec<Value> = s
          .selection
          .iter()
          .filter_map(
            |x| if let Sel::Edge { body, edge } = x { Some(json!({"body": body.0, "edge": edge})) } else { None },
          )
          .collect();
        if e != self.edges {
          self.edges = e;
        }
      }
      Kind::Loft => {
        let secs = sel_sections(s);
        if !secs.is_empty() && secs != self.sections {
          self.sections = secs;
          self.dirty = true;
        }
      }
      Kind::Hem | Kind::Rip | Kind::Flange if !self.applied => {
        let e = sel_edges(s);
        if e != self.edges {
          self.edges = e;
        }
      }
      Kind::PressPull | Kind::Draft if !self.applied => {
        let f = sel_faces(s);
        if f != self.faces {
          self.faces = f;
        }
      }
      Kind::ConvertSheet | Kind::Unfold | Kind::Project if !self.applied || !self.picks() => {
        let f: Vec<Value> = sel_faces(s).into_iter().take(1).collect();
        let b = sel_bodies(s);
        if (f != self.faces || b != self.bodies) && (!b.is_empty() || self.kind != Kind::Project) {
          self.faces = f;
          self.bodies = b;
          self.dirty = !self.picks();
        }
      }
      Kind::SplitBody => {
        let b = sel_only_bodies(s);
        if b != self.bodies {
          self.bodies = b;
          self.dirty = true;
        }
      }
      Kind::NewComponent | Kind::Material => {
        let b = sel_bodies(s);
        if b != self.bodies {
          self.comps = b.iter().map(|x| s.design.component_of(BodyId(*x)).map(|c| c.id.0)).collect();
          self.bodies = b;
          self.dirty = true;
        }
      }
      Kind::Joint if !self.applied => {
        let o = sel_origins(s);
        if o != self.origins {
          self.comps = o.iter().map(|(b, _)| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
          self.origins = o;
        }
      }
      Kind::SkFillet
      | Kind::SkChamfer
      | Kind::SkChamfer2
      | Kind::SkOffset
      | Kind::SkMirror
      | Kind::SkCircPattern
      | Kind::SkRectPattern => {
        let mut c = sel_curves(s);
        if self.kind == Kind::SkMirror {
          c.retain(|x| Some(*x) != self.line);
        }
        if !c.is_empty() && c != self.curves {
          self.curves = c;
          self.dirty = true;
        }
      }
      Kind::FlatPattern => {
        let b = sel_bodies(s);
        if b != self.bodies {
          self.bodies = b;
          self.faces = sel_faces(s).into_iter().take(1).collect();
          self.dirty = true;
        }
      }
      Kind::Shell if !self.applied => {
        let f: Vec<Value> = s
          .selection
          .iter()
          .filter_map(
            |x| if let Sel::Face { body, face } = x { Some(json!({"body": body.0, "face": face})) } else { None },
          )
          .collect();
        if f != self.faces {
          self.faces = f;
        }
      }
      Kind::Combine | Kind::Mirror | Kind::RectPattern | Kind::CircPattern | Kind::Move
        if !self.applied || !self.picks() =>
      {
        let mut b: Vec<u32> = Vec::new();
        for x in &s.selection {
          let id = match x {
            Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } => body.0,
            _ => continue,
          };
          if !b.contains(&id) {
            b.push(id);
          }
        }
        if !b.is_empty() && b != self.bodies {
          self.bodies = b;
          self.dirty = !self.picks();
        }
      }
      _ => {}
    }
  }

  /// Draws the dialog. Returns false when it closed.
  pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> bool {
    self.sync_selection(&app.s);
    if self.kind == Kind::PickPlane
      && let Some(Sel::Face { body, face }) = app.s.selection.iter().find(|x| matches!(x, Sel::Face { .. })).copied()
    {
      if is_planar(&app.s, body, face) {
        app.create_sketch(json!({"body": body.0, "face": face}));
        return false;
      }
      app.status = Some(("Pick a planar face".into(), true));
      app.s.selection.clear();
    }
    let mut keep = true;
    let mut ok = false;
    let mut cancel = false;
    let title =
      if self.editing.is_some() { format!("EDIT {}", self.kind.title()) } else { self.kind.title().to_string() };
    let width = 300.0;
    egui::Area::new(egui::Id::new("feature-dialog"))
      .default_pos(app.canvas.right_top() + egui::vec2(-34.0, 190.0))
      .pivot(egui::Align2::RIGHT_TOP)
      .constrain(true)
      .order(egui::Order::Foreground)
      .show(ctx, |ui| {
        panels::dialog_frame().show(ui, |ui| {
          ui.set_width(width);
          ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
          panels::dialog_header(ui, self.kind.icon(), &title, width);
          egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            egui::Grid::new("dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
              self.fields(app, ui);
            });
            if let Some(e) = &self.error {
              ui.add_space(6.0);
              ui.label(RichText::new(format!("⚠ {e}")).color(theme::ERROR));
            }
          });
          let sep = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover()).0;
          ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
          egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
            ui.horizontal(|ui| {
              ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
              let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 24.0), egui::Sense::hover());
              ui.painter().circle_stroke(r.center(), 7.0, egui::Stroke::new(1.2, theme::GROUP_LABEL));
              ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::proportional(11.0),
                theme::GROUP_LABEL,
              );
              if self.kind != Kind::PickPlane && !self.kind.reports() {
                if self.picks() && self.editing.is_none() {
                  let mut on = self.applied;
                  if ui.checkbox(&mut on, "Preview").changed() {
                    if self.applied {
                      self.withdraw(&mut app.s);
                    } else {
                      self.preview(&mut app.s);
                    }
                  }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                  if panels::secondary_button(ui, "Cancel").clicked() {
                    cancel = true;
                  }
                  if panels::primary_button(ui, "OK", self.accent()).clicked() {
                    ok = true;
                  }
                });
              } else {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                  if panels::secondary_button(ui, if self.kind == Kind::PickPlane { "Cancel" } else { "Close" })
                    .clicked()
                  {
                    cancel = true;
                  }
                });
              }
            });
          });
        });
      });
    if self.dirty {
      self.preview(&mut app.s);
    }
    if ok {
      if !self.applied {
        self.preview(&mut app.s);
      }
      if self.applied && self.error.is_none() {
        keep = false;
        app.s.selection.clear();
        app.status = Some((format!("{} done", self.kind.title().to_lowercase()), false));
      } else if !self.applied && self.error.is_none() {
        self.error = Some(self.missing_input().into());
      } else if self.applied {
        // keep the failing feature in the timeline (red), like Fusion
        keep = false;
      }
    }
    if cancel {
      self.cancel(&mut app.s);
      keep = false;
    }
    keep && !self.closed
  }

  /// The command family's accent colour, used by this dialog's chips and OK button.
  fn accent(&self) -> egui::Color32 {
    crate::icons::accent_of(self.kind.icon())
  }

  fn missing_input(&self) -> &'static str {
    match self.kind {
      Kind::Extrude | Kind::Revolve => "select a profile",
      Kind::Fillet | Kind::Chamfer => "select edges",
      Kind::Shell => "select faces to remove",
      Kind::Combine => "select a target body and tool bodies",
      Kind::Loft => "select profiles on two or more sketches",
      Kind::Sweep => "select a profile and a path sketch",
      Kind::Hole => "sketch points to drill (Point tool) are needed",
      Kind::Fold => "pick a sketch with a bend line",
      Kind::PressPull | Kind::Draft => "select faces",
      Kind::Flange if self.choice == "edge" => "select sheet edges",
      Kind::Flange => "select a profile",
      Kind::Hem | Kind::Rip => "select edges",
      Kind::ConvertSheet | Kind::NewComponent | Kind::Project => "select a body",
      Kind::Joint => "select a face or edge on the moving component, then one on the fixed one",
      Kind::Material => "select a body of a component",
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 => "select two lines that meet",
      Kind::SkMirror => "select curves and a mirror line",
      Kind::SkOffset | Kind::SkCircPattern | Kind::SkRectPattern => "select sketch curves",
      _ => "missing input",
    }
  }

  fn fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let creating = self.editing.is_none();
    match self.kind {
      Kind::PickPlane => {
        ui.label("Plane");
        ui.vertical(|ui| {
          for (label, p) in [("XY  (Top)", "xy"), ("XZ  (Front)", "xz"), ("YZ  (Right)", "yz")] {
            if ui.button(label).clicked() {
              app.create_sketch(json!(p));
              self.closed = true;
            }
          }
          ui.label(RichText::new("…or click a planar face").small().color(theme::MUTED));
        });
        ui.end_row();
      }
      Kind::Extrude | Kind::Revolve => {
        ui.label("Profiles");
        let n = self.profiles.len();
        if self.editing.is_some() {
          ui.label("(unchanged)");
        } else if n == 0 {
          sel_chip(ui, self.accent(), "Select (all regions)", false);
        } else {
          sel_chip(ui, self.accent(), &format!("{n} selected"), true);
        }
        ui.end_row();
        if self.kind == Kind::Extrude {
          ui.label("Extent");
          self.combo(ui, "extent", &EXTENTS, true);
          ui.end_row();
          if self.extent != "through_all" {
            ui.label(if self.extent == "two_sides" { "Distance 1" } else { "Distance" });
            self.text(ui, true);
            ui.end_row();
          }
          if self.extent == "two_sides" {
            ui.label("Distance 2");
            if ui.add(egui::TextEdit::singleline(&mut self.distance2).desired_width(140.0)).changed() {
              self.dirty = true;
            }
            ui.end_row();
          }
          ui.label("Direction");
          if ui.checkbox(&mut self.flip, "Flip").changed() {
            self.dirty = true;
          }
          ui.end_row();
        } else {
          ui.label("Axis");
          let lines = self.sketch_lines(app);
          let current = self.axis.clone();
          egui::ComboBox::from_id_salt("axis").selected_text(axis_label(&current)).show_ui(ui, |ui| {
            for a in ["x", "y", "z"] {
              if ui.selectable_label(current == a, axis_label(a)).clicked() {
                self.axis = a.into();
                self.dirty = true;
              }
            }
            for (id, label) in lines {
              let key = format!("line:{id}");
              if ui.selectable_label(current == key, label).clicked() {
                self.axis = key;
                self.dirty = true;
              }
            }
          });
          ui.end_row();
          ui.label("Angle");
          if ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Operation");
        self.combo(ui, "op", &OPS, false);
        ui.end_row();
      }
      Kind::Fillet | Kind::Chamfer | Kind::Shell => {
        let (what, n) =
          if self.kind == Kind::Shell { ("Faces", self.faces.len()) } else { ("Edges", self.edges.len()) };
        ui.label(what);
        if creating {
          sel_chip(ui, self.accent(), &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
        } else {
          ui.label(format!("{n} (unchanged)"));
        }
        ui.end_row();
        ui.label(match self.kind {
          Kind::Fillet => "Radius",
          Kind::Chamfer => "Distance",
          _ => "Thickness",
        });
        if ui.add(egui::TextEdit::singleline(&mut self.radius).desired_width(140.0)).changed()
          && (!creating || self.applied)
        {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => {
        ui.label("Plane");
        ui.label(&self.plane_label);
        ui.end_row();
        let labels: &[&str] = match self.kind {
          Kind::Box => &["Width (X)", "Depth (Y)", "Height"],
          Kind::Cylinder => &["Radius", "Height"],
          Kind::Sphere => &["Radius"],
          _ => &["Major radius", "Minor radius"],
        };
        for (i, l) in labels.iter().enumerate() {
          ui.label(*l);
          let f = match i {
            0 => &mut self.a,
            1 => &mut self.b,
            _ => &mut self.c,
          };
          if ui.add(egui::TextEdit::singleline(f).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Position X, Y");
        ui.horizontal(|ui| {
          for v in self.at.iter_mut() {
            if ui.add(egui::TextEdit::singleline(v).desired_width(64.0)).changed() {
              self.dirty = true;
            }
          }
        });
        ui.end_row();
        ui.label("Operation");
        self.combo(ui, "op", &OPS, false);
        ui.end_row();
      }
      Kind::OffsetPlane | Kind::Mirror => {
        ui.label("Plane");
        if creating {
          self.plane_choice(ui);
        } else {
          ui.label("(unchanged)");
        }
        ui.end_row();
        if self.kind == Kind::OffsetPlane {
          ui.label("Offset");
          self.text(ui, true);
          ui.end_row();
        } else {
          ui.label("Bodies");
          sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.join, "Join to original").changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
      }
      Kind::RectPattern | Kind::CircPattern => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        if self.kind == Kind::RectPattern {
          ui.label("Direction");
          self.axis_combo(ui, "dir1", true);
          ui.end_row();
          ui.label("Count");
          if ui.add(egui::TextEdit::singleline(&mut self.count).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("Spacing");
          if ui.add(egui::TextEdit::singleline(&mut self.spacing).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.use_dir2, "Second direction").changed() {
            self.dirty = true;
          }
          ui.end_row();
          if self.use_dir2 {
            ui.label("Direction 2");
            self.axis_combo(ui, "dir2", false);
            ui.end_row();
            ui.label("Count 2");
            if ui.add(egui::TextEdit::singleline(&mut self.count2).desired_width(140.0)).changed() {
              self.dirty = true;
            }
            ui.end_row();
            ui.label("Spacing 2");
            if ui.add(egui::TextEdit::singleline(&mut self.spacing2).desired_width(140.0)).changed() {
              self.dirty = true;
            }
            ui.end_row();
          }
        } else {
          ui.label("Axis");
          self.axis_combo(ui, "axis", false);
          ui.end_row();
          ui.label("Count");
          if ui.add(egui::TextEdit::singleline(&mut self.count).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("Total angle");
          if ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("");
        if ui.checkbox(&mut self.join, "Join instances").changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Combine => {
        ui.label("Target");
        match self.bodies.first() {
          Some(b) => sel_chip(ui, self.accent(), &app.s.body_name(BodyId(*b)), true),
          None => sel_chip(ui, self.accent(), "Select", false),
        };
        ui.end_row();
        ui.label("Tools");
        ui.label(format!("{}", self.bodies.len().saturating_sub(1)));
        ui.end_row();
        ui.label("Operation");
        self.combo(ui, "op", &OPS[1..], false);
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut self.keep_tools, "Keep tools");
        ui.end_row();
      }
      Kind::Move => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        for (i, l) in ["X distance", "Y distance", "Z distance"].iter().enumerate() {
          ui.label(*l);
          if ui.add(egui::TextEdit::singleline(&mut self.translate[i]).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Rotate about");
        self.axis_combo(ui, "axis", false);
        ui.end_row();
        ui.label("Angle");
        if ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(140.0)).changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Inspect => inspect(app, ui),
      _ => {
        let mut dirty = false;
        self.more_fields(app, ui, &mut dirty);
        if dirty && (!self.picks() || self.applied) {
          self.dirty = true;
        }
      }
    }
  }

  /// Fields of the loft … sketch-modify dialogs.
  fn more_fields(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    let accent = self.accent();
    let chip = |ui: &mut egui::Ui, n: usize, what: &str| {
      let text = if n == 0 { "Select".to_string() } else { format!("{n} {what}") };
      sel_chip(ui, accent, &text, n > 0);
    };
    const HOLES: [(&str, &str); 3] =
      [("simple", "Simple"), ("counterbore", "Counterbore"), ("countersink", "Countersink")];
    const FLANGES: [(&str, &str); 3] =
      [("base", "Base (profile)"), ("edge", "Edge"), ("contour", "Contour (open path)")];
    const POSITIONS: [(&str, &str); 3] = [("adjacent", "Adjacent"), ("inside", "Inside"), ("outside", "Outside")];
    const RELIEFS: [(&str, &str); 3] = [("rectangular", "Rectangular"), ("round", "Round"), ("none", "None")];
    const JOINTS: [(&str, &str); 7] = [
      ("rigid", "Rigid"),
      ("revolute", "Revolute"),
      ("slider", "Slider"),
      ("cylindrical", "Cylindrical"),
      ("pin_slot", "Pin-slot"),
      ("planar", "Planar"),
      ("ball", "Ball"),
    ];
    match self.kind {
      Kind::Loft => {
        ui.label("Profiles");
        chip(ui, self.sections.len(), "profiles");
        ui.end_row();
        for (i, (sk, _)) in self.sections.iter().enumerate() {
          ui.label(RichText::new(format!("  Section {}", i + 1)).color(theme::MUTED));
          ui.label(app.s.design.feature(FeatureId(*sk)).map(|f| f.name.clone()).unwrap_or_default());
          ui.end_row();
        }
        Self::check(ui, "Ruled (straight between sections)", &mut self.flag, dirty);
        Self::check(ui, "Closed (back to the first)", &mut self.flag2, dirty);
        Self::choose(ui, "Operation", "op", &mut self.op, &OPS, dirty);
      }
      Kind::Sweep => {
        ui.label("Profile");
        chip(ui, self.profiles.len(), "selected");
        ui.end_row();
        ui.label("Path");
        let list = Self::sketches(app, self.sketch);
        let cur = self.path.and_then(|p| list.iter().find(|(id, _)| *id == p)).map(|(_, n)| n.clone());
        egui::ComboBox::from_id_salt("path").selected_text(cur.unwrap_or("Pick a sketch".into())).width(140.0).show_ui(
          ui,
          |ui| {
            for (id, n) in &list {
              if ui.selectable_label(self.path == Some(*id), n).clicked() {
                self.path = Some(*id);
                *dirty = true;
              }
            }
          },
        );
        ui.end_row();
        Self::choose(ui, "Operation", "op", &mut self.op, &OPS, dirty);
      }
      Kind::Hole => {
        ui.label("Sketch");
        ui.label(self.sketch.and_then(|s| app.s.design.feature(s)).map(|f| f.name.clone()).unwrap_or("none".into()));
        ui.end_row();
        ui.label("Placement");
        ui.label(RichText::new("at the sketch's points").color(theme::MUTED));
        ui.end_row();
        Self::choose(ui, "Hole type", "hole", &mut self.choice, &HOLES, dirty);
        Self::field(ui, "Diameter", &mut self.a, dirty);
        if self.choice != "simple" {
          Self::field(ui, "Head diameter", &mut self.c, dirty);
          if self.choice == "counterbore" {
            Self::field(ui, "Head depth", &mut self.d, dirty);
          } else {
            Self::field(ui, "Head angle", &mut self.e, dirty);
          }
        }
        Self::check(ui, "Through all", &mut self.flag, dirty);
        if !self.flag {
          Self::field(ui, "Depth", &mut self.b, dirty);
        }
        Self::check(ui, "Flip direction", &mut self.flip, dirty);
      }
      Kind::PressPull => {
        ui.label("Faces");
        chip(ui, self.faces.len(), "selected");
        ui.end_row();
        Self::field(ui, "Distance", &mut self.distance, dirty);
      }
      Kind::Draft => {
        ui.label("Faces");
        chip(ui, self.faces.len(), "selected");
        ui.end_row();
        ui.label("Pull direction");
        let before = self.plane_label.clone();
        self.plane_choice(ui);
        *dirty |= before != self.plane_label;
        ui.end_row();
        Self::field(ui, "Angle", &mut self.angle, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::SplitBody => {
        ui.label("Bodies to split");
        sel_chip(ui, accent, &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        ui.label("Splitting tool");
        self.plane_choice(ui);
        ui.end_row();
      }
      Kind::Flange => {
        let before = self.choice;
        Self::choose(ui, "Type", "flange", &mut self.choice, &FLANGES, dirty);
        if before != self.choice {
          self.withdraw(&mut app.s);
        }
        match self.choice {
          "edge" => {
            ui.label("Edges");
            chip(ui, self.edges.len(), "selected");
            ui.end_row();
            Self::field(ui, "Height", &mut self.distance, dirty);
            Self::field(ui, "Bend angle", &mut self.angle, dirty);
            Self::choose(ui, "Position", "pos", &mut self.position, &POSITIONS, dirty);
          }
          "contour" => {
            ui.label("Sketch");
            ui.label(self.sketch.and_then(|s| app.s.design.feature(s)).map(|f| f.name.clone()).unwrap_or_default());
            ui.end_row();
            Self::field(ui, "Distance", &mut self.distance, dirty);
            Self::check(ui, "Symmetric", &mut self.flag, dirty);
            self.rule_combo(ui);
          }
          _ => {
            ui.label("Profiles");
            chip(ui, self.profiles.len(), "selected");
            ui.end_row();
            self.rule_combo(ui);
            Self::choose(ui, "Operation", "op", &mut self.op, &OPS[..2], dirty);
          }
        }
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::Hem => {
        ui.label("Edges");
        chip(ui, self.edges.len(), "selected");
        ui.end_row();
        Self::field(ui, "Length", &mut self.distance, dirty);
        Self::field(ui, "Gap", &mut self.distance2, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::Fold => {
        ui.label("Sketch");
        let list = Self::sketches(app, None);
        let cur = self.sketch.and_then(|p| list.iter().find(|(id, _)| *id == p)).map(|(_, n)| n.clone());
        egui::ComboBox::from_id_salt("fsk").selected_text(cur.unwrap_or_default()).width(140.0).show_ui(ui, |ui| {
          for (id, n) in &list {
            if ui.selectable_label(self.sketch == Some(*id), n).clicked() {
              self.sketch = Some(*id);
              self.line = None;
              *dirty = true;
            }
          }
        });
        ui.end_row();
        ui.label("Bend line");
        let lines = self.sketch_lines(app);
        let cur = self.line.map(|l| format!("Sketch line {l}")).unwrap_or("Pick a line".into());
        egui::ComboBox::from_id_salt("fline").selected_text(cur).width(140.0).show_ui(ui, |ui| {
          for (id, label) in lines {
            if ui.selectable_label(self.line == Some(id), label).clicked() {
              self.line = Some(id);
              *dirty = true;
            }
          }
        });
        ui.end_row();
        Self::field(ui, "Angle", &mut self.angle, dirty);
        Self::check(ui, "Flip direction", &mut self.flip, dirty);
        Self::check(ui, "Fold the other side", &mut self.flag, dirty);
      }
      Kind::ConvertSheet => {
        ui.label("Body");
        chip(ui, self.bodies.len().min(1), "body");
        ui.end_row();
        ui.label("Stationary face");
        chip(ui, self.faces.len(), "face");
        ui.end_row();
        self.rule_combo(ui);
      }
      Kind::Rip => {
        ui.label("Edges");
        chip(ui, self.edges.len(), "selected");
        ui.end_row();
        Self::field(ui, "Gap", &mut self.distance, dirty);
      }
      Kind::Unfold => {
        ui.label("Body");
        if self.bodies.is_empty() {
          ui.label(RichText::new("the sheet metal body").color(theme::MUTED));
        } else {
          chip(ui, 1, "body");
        }
        ui.end_row();
        ui.label("Stationary face");
        chip(ui, self.faces.len(), "face");
        ui.end_row();
      }
      Kind::SheetRules => {
        ui.label("Rule");
        let cur = self.name.clone();
        egui::ComboBox::from_id_salt("rules").selected_text(&cur).width(140.0).show_ui(ui, |ui| {
          for r in self.rules.clone() {
            if ui.selectable_label(cur == r, &r).clicked() {
              self.withdraw(&mut app.s);
              self.load_rule(&app.s, &r);
            }
          }
        });
        ui.end_row();
        Self::field(ui, "Name", &mut self.name, dirty);
        Self::field(ui, "Thickness", &mut self.a, dirty);
        Self::field(ui, "Bend radius", &mut self.b, dirty);
        Self::field(ui, "K factor", &mut self.c, dirty);
        Self::choose(ui, "Bend relief", "relief", &mut self.position, &RELIEFS, dirty);
        Self::field(ui, "Relief width", &mut self.d, dirty);
        Self::field(ui, "Relief depth", &mut self.e, dirty);
        Self::field(ui, "Gap", &mut self.distance2, dirty);
        ui.label("");
        ui.label(RichText::new("empty = from the thickness").small().color(theme::MUTED));
        ui.end_row();
      }
      Kind::FlatPattern => {
        ui.label("Body");
        if self.bodies.is_empty() {
          ui.label(RichText::new("the sheet metal body").color(theme::MUTED));
        } else {
          chip(ui, 1, "body");
        }
        ui.end_row();
        if let Some(r) = self.report.clone() {
          if let Some([w, h]) = r.get("size").and_then(|v| serde_json::from_value::<[f64; 2]>(v.clone()).ok()) {
            ui.label("Flat size");
            ui.label(format!("{w:.2} × {h:.2} mm"));
            ui.end_row();
          }
          let bends = r.get("bends").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
          ui.label("Bends");
          ui.label(bends.to_string());
          ui.end_row();
          ui.label("Export");
          if panels::secondary_button(ui, "DXF…").clicked()
            && let Some(path) =
              rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name("flat pattern.dxf").save_file()
          {
            let mut p = json!({"path": path});
            if let Some(b) = self.bodies.first() {
              p["body"] = json!(b);
            }
            if app.run("sheet.flat_pattern", p).is_some() {
              app.status = Some((format!("Exported {}", path.display()), false));
            }
          }
          ui.end_row();
        }
      }
      Kind::NewComponent => {
        Self::field(ui, "Name", &mut self.name, dirty);
        ui.label("Bodies");
        chip(ui, self.bodies.len(), "selected");
        ui.end_row();
      }
      Kind::Joint => {
        ui.label("Component 1");
        chip(ui, self.origins.len().min(1), "origin");
        ui.end_row();
        ui.label("Component 2");
        chip(ui, self.origins.len().saturating_sub(1).min(1), "origin");
        ui.end_row();
        if !self.origins.is_empty() && self.comps.first().is_some_and(Option::is_none) {
          ui.label("");
          ui.label(RichText::new("the first body isn't in a component").color(theme::WARNING));
          ui.end_row();
        }
        Self::choose(ui, "Motion", "joint", &mut self.choice, &JOINTS, dirty);
        Self::field(ui, "Angle", &mut self.angle, dirty);
        Self::field(ui, "Offset", &mut self.distance, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::Material => {
        ui.label("Component");
        let n = self.comps.iter().flatten().count();
        chip(ui, n.min(1), "component");
        ui.end_row();
        ui.label("Material");
        let cur = self.name.clone();
        egui::ComboBox::from_id_salt("mat").selected_text(&cur).width(140.0).show_ui(ui, |ui| {
          for (m, _) in clf_engine::model::assembly::MATERIALS {
            if ui.selectable_label(cur == *m, *m).clicked() {
              self.name = m.to_string();
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      Kind::Interference => {
        let rows = self.report.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
        ui.label("Result");
        ui.label(if rows.is_empty() { "No interference".to_string() } else { format!("{} overlaps", rows.len()) });
        ui.end_row();
        for r in rows {
          let ids: Vec<u32> = r
            .get("bodies")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|v| v as u32)).collect())
            .unwrap_or_default();
          let names: Vec<String> = ids.iter().map(|b| app.s.body_name(BodyId(*b))).collect();
          if ui.link(names.join(" ↔ ")).clicked() {
            app.s.selection = ids.iter().map(|b| Sel::Body { body: BodyId(*b) }).collect();
          }
          ui.label(format!("{:.3} mm³", r.get("volume_mm3").and_then(Value::as_f64).unwrap_or(0.0)));
          ui.end_row();
        }
        ui.label("");
        if panels::secondary_button(ui, "Compute").clicked() {
          self.refresh_report(&mut app.s);
        }
        ui.end_row();
      }
      Kind::Bom => {
        let rows = self.report.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
        if rows.is_empty() {
          ui.label("No components");
          ui.end_row();
        }
        for r in rows {
          let t = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
          let f = |k: &str| r.get(k).and_then(Value::as_f64).unwrap_or(0.0);
          ui.label(RichText::new(t("name")).strong());
          ui.label(format!("{} · {:.1} g", t("material"), f("mass_g")));
          ui.end_row();
        }
      }
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 => {
        ui.label("Lines");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        if self.kind == Kind::SkFillet {
          Self::field(ui, "Radius", &mut self.radius, dirty);
        } else {
          Self::field(
            ui,
            if self.kind == Kind::SkChamfer { "Distance" } else { "Distance 1" },
            &mut self.distance,
            dirty,
          );
          if self.kind == Kind::SkChamfer2 {
            Self::field(ui, "Distance 2", &mut self.distance2, dirty);
          }
        }
      }
      Kind::SkOffset => {
        ui.label("Curves");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        Self::field(ui, "Offset position", &mut self.distance, dirty);
      }
      Kind::SkMirror => {
        ui.label("Objects");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        ui.label("Mirror line");
        let lines = self.sketch_lines_of(app.s.edit_sketch, app);
        let cur = self.line.map(|l| format!("Sketch line {l}")).unwrap_or("Pick a line".into());
        egui::ComboBox::from_id_salt("mline").selected_text(cur).width(140.0).show_ui(ui, |ui| {
          for (id, label) in lines {
            if ui.selectable_label(self.line == Some(id), label).clicked() {
              self.line = Some(id);
              self.curves.retain(|c| *c != id);
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      Kind::SkCircPattern => {
        ui.label("Objects");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        ui.label("Center X, Y");
        ui.horizontal(|ui| {
          for v in self.at.iter_mut() {
            if ui.add(egui::TextEdit::singleline(v).desired_width(64.0)).changed() {
              *dirty = true;
            }
          }
        });
        ui.end_row();
        Self::field(ui, "Quantity", &mut self.count, dirty);
        Self::field(ui, "Total angle", &mut self.angle, dirty);
      }
      Kind::SkRectPattern => {
        ui.label("Objects");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        const DIRS: [(&str, &str); 2] = [("x", "Horizontal"), ("y", "Vertical")];
        let mut d1: &'static str = if self.dir == "y" { "y" } else { "x" };
        Self::choose(ui, "Direction", "sdir1", &mut d1, &DIRS, dirty);
        self.dir = d1.into();
        Self::field(ui, "Quantity", &mut self.count, dirty);
        Self::field(ui, "Distance", &mut self.spacing, dirty);
        Self::check(ui, "Second direction", &mut self.use_dir2, dirty);
        if self.use_dir2 {
          let mut d2: &'static str = if self.dir2 == "x" { "x" } else { "y" };
          Self::choose(ui, "Direction 2", "sdir2", &mut d2, &DIRS, dirty);
          self.dir2 = d2.into();
          Self::field(ui, "Quantity 2", &mut self.count2, dirty);
          Self::field(ui, "Distance 2", &mut self.spacing2, dirty);
        }
      }
      Kind::Project => {
        ui.label("Geometry");
        let what = if self.faces.is_empty() { "body" } else { "face" };
        chip(ui, self.bodies.len().min(1), what);
        ui.end_row();
        Self::check(ui, "Projected link (follows the body)", &mut self.flag, dirty);
      }
      _ => {}
    }
  }

  /// Re-runs a report dialog's query.
  fn refresh_report(&mut self, s: &mut Session) {
    let r = match self.kind {
      Kind::FlatPattern => {
        let mut p = json!({});
        if let Some(b) = self.bodies.first() {
          p["body"] = json!(b);
        }
        if let Some(f) = self.faces.first() {
          p["face"] = f.clone();
        }
        s.execute("sheet.flat_pattern", &p)
      }
      Kind::Interference => s.execute("assembly.interference", &json!({})),
      Kind::Bom => s.execute("assembly.bom", &json!({})),
      _ => return,
    };
    match r {
      Ok(v) => {
        self.report = Some(v);
        self.error = None;
      }
      Err(e) => {
        self.report = None;
        self.error = Some(e.0);
      }
    }
  }

  /// Fills the rule fields from the named rule ("" = the default rule).
  fn load_rule(&mut self, s: &Session, name: &str) {
    let r = s.design.sheet_rule(name).unwrap_or_default();
    self.name = r.name.clone();
    self.a = r.thickness;
    self.b = r.bend_radius;
    self.c = r.k_factor;
    self.d = r.relief_width;
    self.e = r.relief_depth;
    self.distance2 = r.gap;
    self.position = match r.relief {
      clf_engine::model::ReliefShape::Rectangular => "rectangular",
      clf_engine::model::ReliefShape::Round => "round",
      clf_engine::model::ReliefShape::None => "none",
    };
    self.rules = s.design.sheet_metal_rules.iter().map(|r| r.name.clone()).collect();
    if self.rules.is_empty() {
      self.rules.push(self.name.clone());
    }
  }

  fn put_rule(&self, p: &mut Value) {
    if !self.rule.is_empty() {
      p["rule"] = json!(self.rule);
    }
  }

  /// A text field that re-previews when edited.
  fn field(ui: &mut egui::Ui, label: &str, v: &mut String, dirty: &mut bool) {
    ui.label(label);
    if ui.add(egui::TextEdit::singleline(v).desired_width(140.0)).changed() {
      *dirty = true;
    }
    ui.end_row();
  }

  /// A drop-down of (key, label) choices that re-previews when changed.
  fn choose(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    cur: &mut &'static str,
    items: &[(&'static str, &'static str)],
    dirty: &mut bool,
  ) {
    ui.label(label);
    let shown = items.iter().find(|(k, _)| k == cur).map(|(_, l)| *l).unwrap_or(cur);
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(140.0).show_ui(ui, |ui| {
      for (k, l) in items {
        if ui.selectable_label(*cur == *k, *l).clicked() {
          *cur = k;
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  fn check(ui: &mut egui::Ui, label: &str, v: &mut bool, dirty: &mut bool) {
    ui.label("");
    if ui.checkbox(v, label).changed() {
      *dirty = true;
    }
    ui.end_row();
  }

  fn rule_combo(&mut self, ui: &mut egui::Ui) {
    ui.label("Rule");
    let shown = if self.rule.is_empty() { "Default".to_string() } else { self.rule.clone() };
    egui::ComboBox::from_id_salt("rule").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      if ui.selectable_label(self.rule.is_empty(), "Default").clicked() {
        self.rule.clear();
        self.dirty = true;
      }
      for r in self.rules.clone() {
        if ui.selectable_label(self.rule == r, &r).clicked() {
          self.rule = r;
          self.dirty = true;
        }
      }
    });
    ui.end_row();
  }

  /// Sketches other than `but`, newest first, for picking a path.
  fn sketches(app: &App, but: Option<FeatureId>) -> Vec<(FeatureId, String)> {
    let d = &app.s.design;
    d.features
      .iter()
      .take(d.active_len())
      .rev()
      .filter(|f| matches!(f.kind, FeatureKind::Sketch { .. }) && Some(f.id) != but)
      .map(|f| (f.id, f.name.clone()))
      .collect()
  }

  fn text(&mut self, ui: &mut egui::Ui, distance: bool) {
    let f = if distance { &mut self.distance } else { &mut self.radius };
    if ui.add(egui::TextEdit::singleline(f).desired_width(140.0)).changed() {
      self.dirty = true;
    }
  }

  fn combo(&mut self, ui: &mut egui::Ui, id: &str, items: &[(&'static str, &'static str)], extent: bool) {
    let cur = if extent { self.extent } else { self.op };
    let label = items.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or(cur);
    egui::ComboBox::from_id_salt(id).selected_text(label).width(140.0).show_ui(ui, |ui| {
      for (k, l) in items {
        if ui.selectable_label(cur == *k, *l).clicked() {
          if extent {
            self.extent = k;
          } else {
            self.op = k;
          }
          self.dirty = true;
        }
      }
    });
  }

  fn axis_combo(&mut self, ui: &mut egui::Ui, id: &str, first: bool) {
    let cur = if id == "dir1" {
      self.dir.clone()
    } else if id == "dir2" {
      self.dir2.clone()
    } else {
      self.axis.clone()
    };
    let _ = first;
    egui::ComboBox::from_id_salt(id).selected_text(axis_label(&cur)).width(140.0).show_ui(ui, |ui| {
      for a in ["x", "y", "z"] {
        if ui.selectable_label(cur == a, axis_label(a)).clicked() {
          match id {
            "dir1" => self.dir = a.into(),
            "dir2" => self.dir2 = a.into(),
            _ => self.axis = a.into(),
          }
          self.dirty = true;
        }
      }
    });
  }

  fn plane_choice(&mut self, ui: &mut egui::Ui) {
    let cur = self.plane_label.clone();
    egui::ComboBox::from_id_salt("plane").selected_text(cur.clone()).width(140.0).show_ui(ui, |ui| {
      for (l, p) in [("XY plane", "xy"), ("XZ plane", "xz"), ("YZ plane", "yz")] {
        if ui.selectable_label(cur == l, l).clicked() {
          self.plane = json!(p);
          self.plane_label = l.into();
          self.dirty = true;
        }
      }
    });
  }

  fn bodies_label(&self) -> String {
    if self.bodies.is_empty() { "all bodies".into() } else { format!("{} selected", self.bodies.len()) }
  }

  fn sketch_lines(&self, app: &App) -> Vec<(u32, String)> {
    self.sketch_lines_of(self.sketch, app)
  }

  fn sketch_lines_of(&self, sketch: Option<FeatureId>, app: &App) -> Vec<(u32, String)> {
    let Some(sk) = sketch else { return vec![] };
    let Ok(s) = app.s.design.sketch(sk) else { return vec![] };
    s.curves
      .iter()
      .filter(|c| matches!(c.kind, clf_engine::sketch::CurveKind::Line { .. }))
      .map(|c| (c.id.0, format!("Sketch line {}{}", c.id.0, if c.construction { " (construction)" } else { "" })))
      .collect()
  }
}

fn inspect(app: &mut App, ui: &mut egui::Ui) {
  let s = &app.s;
  if s.selection.is_empty() {
    ui.label("Select a body, face or edge");
    ui.end_row();
    return;
  }
  for sel in s.selection.clone().iter().take(6) {
    match sel {
      Sel::Body { body } => {
        if let Some(b) = s.model.body(*body)
          && let Ok(m) = b.shape.mass()
        {
          ui.label(RichText::new(s.body_name(*body)).strong());
          ui.end_row();
          ui.label("Volume");
          ui.label(format!("{:.3} mm³", m.volume));
          ui.end_row();
          ui.label("Area");
          ui.label(format!("{:.3} mm²", m.area));
          ui.end_row();
          let d = m.bbox_max - m.bbox_min;
          ui.label("Size");
          ui.label(format!("{:.3} × {:.3} × {:.3}", d.x, d.y, d.z));
          ui.end_row();
          ui.label("Centre of mass");
          let c = m.center_of_mass;
          ui.label(format!("{:.3}, {:.3}, {:.3}", c.x, c.y, c.z));
          ui.end_row();
        }
      }
      Sel::Face { body, face } => {
        if let Some(f) = s.model.body(*body).and_then(|b| b.faces().iter().find(|f| f.id == *face).copied()) {
          ui.label(RichText::new(format!("Face ({:?})", f.kind)).strong());
          ui.end_row();
          ui.label("Area");
          ui.label(format!("{:.3} mm²", f.area));
          ui.end_row();
          if f.radius > 0.0 {
            ui.label("Radius");
            ui.label(format!("{:.3} mm", f.radius));
            ui.end_row();
          }
        }
      }
      Sel::Edge { body, edge } => {
        if let Some(e) = s.model.body(*body).and_then(|b| b.edges().iter().find(|e| e.id == *edge).copied()) {
          ui.label(RichText::new(format!("Edge ({:?})", e.kind)).strong());
          ui.end_row();
          ui.label("Length");
          ui.label(format!("{:.3} mm", e.length));
          ui.end_row();
          if e.radius > 0.0 {
            ui.label("Radius");
            ui.label(format!("{:.3} mm", e.radius));
            ui.end_row();
          }
        }
      }
      _ => {}
    }
  }
}

fn axis_label(a: &str) -> String {
  match a {
    "x" => "X axis".into(),
    "y" => "Y axis".into(),
    "z" => "Z axis".into(),
    other => other.replace("line:", "Sketch line "),
  }
}

fn axis_vec(a: &str) -> [f64; 3] {
  match a {
    "y" => [0.0, 1.0, 0.0],
    "z" => [0.0, 0.0, 1.0],
    _ => [1.0, 0.0, 0.0],
  }
}

fn op_str(o: Operation) -> &'static str {
  match o {
    Operation::NewBody => "new_body",
    Operation::Join => "join",
    Operation::Cut => "cut",
    Operation::Intersect => "intersect",
  }
}

fn is_planar(s: &Session, body: BodyId, face: u32) -> bool {
  s.model
    .body(body)
    .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == clf_engine::kernel::SurfaceKind::Plane))
    .unwrap_or(false)
}

fn last_unconsumed_sketch(s: &Session) -> Option<FeatureId> {
  let active = s.design.active_len();
  let feats = &s.design.features[..active];
  feats
    .iter()
    .rev()
    .find(|f| {
      matches!(f.kind, FeatureKind::Sketch { .. })
        && !feats.iter().any(|g| matches!(&g.kind, FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } if *sketch == f.id))
    })
    .map(|f| f.id)
}

fn sel_edges(s: &Session) -> Vec<Value> {
  s.selection
    .iter()
    .filter_map(|x| if let Sel::Edge { body, edge } = x { Some(json!({"body": body.0, "edge": edge})) } else { None })
    .collect()
}

fn sel_faces(s: &Session) -> Vec<Value> {
  s.selection
    .iter()
    .filter_map(|x| if let Sel::Face { body, face } = x { Some(json!({"body": body.0, "face": face})) } else { None })
    .collect()
}

/// Bodies of the selected bodies, faces and edges.
fn sel_bodies(s: &Session) -> Vec<u32> {
  let mut b: Vec<u32> = Vec::new();
  for x in &s.selection {
    if let Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } = x
      && !b.contains(&body.0)
    {
      b.push(body.0);
    }
  }
  b
}

fn sel_only_bodies(s: &Session) -> Vec<u32> {
  s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(body.0) } else { None }).collect()
}

fn sel_curves(s: &Session) -> Vec<u32> {
  s.selection.iter().filter_map(|x| if let Sel::SketchCurve { curve, .. } = x { Some(curve.0) } else { None }).collect()
}

/// Loft sections from the selected profiles, one per sketch, in pick order.
fn sel_sections(s: &Session) -> Vec<(u32, [f64; 2])> {
  let mut out: Vec<(u32, [f64; 2])> = Vec::new();
  for x in &s.selection {
    if let Sel::Profile { sketch, index } = x
      && let Some(p) = s.model.sketches.get(sketch).and_then(|r| r.profiles.get(*index))
      && !out.iter().any(|(sk, _)| *sk == sketch.0)
    {
      out.push((sketch.0, p.interior_point()));
    }
  }
  out
}

/// Joint origins: the selected faces and edges, in pick order.
fn sel_origins(s: &Session) -> Vec<(u32, Value)> {
  s.selection
    .iter()
    .filter_map(|x| match x {
      Sel::Face { body, face } => Some((body.0, json!({"face": {"body": body.0, "face": face}}))),
      Sel::Edge { body, edge } => Some((body.0, json!({"edge": {"body": body.0, "edge": edge}}))),
      _ => None,
    })
    .collect()
}

/// The mirror line among the picked curves: the last picked line.
fn mirror_line(s: &Session, curves: &[u32]) -> Option<u32> {
  let sk = s.design.sketch(s.edit_sketch?).ok()?;
  curves.iter().rev().copied().find(|c| {
    sk.curve(clf_engine::sketch::CurveId(*c))
      .is_some_and(|c| matches!(c.kind, clf_engine::sketch::CurveKind::Line { .. }))
  })
}

/// A selection input: a white "Select" chip until something is picked, then a chip tinted with
/// the command's accent showing the count, with a ✕ beside it.
fn sel_chip(ui: &mut egui::Ui, accent: egui::Color32, text: &str, has: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.0);
  let tw = ui.painter().layout_no_wrap(text.to_string(), font.clone(), theme::TEXT).size().x;
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(tw + 52.0, 22.0), egui::Sense::hover());
  let chip = egui::Rect::from_min_size(rect.min, egui::vec2(tw + 32.0, 22.0));
  let p = ui.painter();
  let ir = egui::Rect::from_center_size(chip.left_center() + egui::vec2(12.0, 0.0), egui::Vec2::splat(15.0));
  let fill = if has { crate::icons::tint_of(accent, 0.84) } else { egui::Color32::WHITE };
  let stroke = if has { accent } else { theme::BORDER };
  p.rect(chip, 3.0, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  crate::icons::draw(p, ir, Icon::Select, accent);
  p.text(chip.left_center() + egui::vec2(24.0, 0.0), egui::Align2::LEFT_CENTER, text, font, theme::TEXT);
  if has {
    let x = egui::pos2(chip.right() + 10.0, chip.center().y);
    let s = egui::Stroke::new(1.4, theme::GROUP_LABEL);
    p.line_segment([x + egui::vec2(-4.0, -4.0), x + egui::vec2(4.0, 4.0)], s);
    p.line_segment([x + egui::vec2(4.0, -4.0), x + egui::vec2(-4.0, 4.0)], s);
  }
  resp
}

#[cfg(test)]
mod tests {
  use super::*;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  /// Previews the dialog's command and checks it applied without an error.
  fn apply(s: &mut Session, d: &mut Dialog) {
    assert!(d.ready(), "{:?} not ready", d.kind);
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
  }

  fn face(s: &Session, body: usize, pick: impl Fn(&clf_engine::kernel::FaceInfo) -> bool) -> Value {
    let b = &s.model.bodies[body];
    json!({"body": b.id.0, "face": b.faces().iter().find(|f| pick(f)).unwrap().id})
  }

  fn edge(s: &Session, body: usize, pick: impl Fn(&clf_engine::kernel::EdgeInfo) -> bool) -> Value {
    let b = &s.model.bodies[body];
    json!({"body": b.id.0, "edge": b.edges().iter().find(|e| pick(e)).unwrap().id})
  }

  fn interior(s: &Session, sk: u32) -> [f64; 2] {
    s.model.sketches[&FeatureId(sk)].profiles[0].interior_point()
  }

  #[test]
  fn loft_sweep_and_hole_dialogs() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"center": [0, 0], "corner": [20, 20]}));
    let a = s.edit_sketch.unwrap().0;
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 30}));
    let plane = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": plane}}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
    let b = s.edit_sketch.unwrap().0;
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Loft);
    d.sections = vec![(a, interior(&s, a)), (b, interior(&s, b))];
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);

    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 3}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = s.edit_sketch;
    d.path = Some(path);
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);

    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 10}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "sketch.create", json!({"plane": top}));
    run(&mut s, "sketch.point", json!({"at": [10, 5]}));
    let before = s.model.bodies[0].shape.mass().unwrap().volume;
    let mut d = Dialog::blank(Kind::Hole);
    d.sketch = s.edit_sketch;
    run(&mut s, "sketch.finish", json!({}));
    d.choice = "counterbore";
    d.a = "5 mm".into();
    d.b = "6 mm".into();
    d.c = "9 mm".into();
    d.d = "2 mm".into();
    apply(&mut s, &mut d);
    assert!(s.model.bodies[0].shape.mass().unwrap().volume < before);
  }

  #[test]
  fn modify_dialogs() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 20}));
    let mut d = Dialog::blank(Kind::PressPull);
    d.faces = vec![face(&s, 0, |f| f.normal.z > 0.9)];
    d.distance = "5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Draft);
    let b = &s.model.bodies[0];
    d.faces =
      b.faces().iter().filter(|f| f.normal.z.abs() < 0.1).map(|f| json!({"body": b.id.0, "face": f.id})).collect();
    d.angle = "3 deg".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SplitBody);
    d.plane = json!("xz");
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 2);
  }

  #[test]
  fn sheet_metal_dialogs() {
    let mut s = Session::default();
    let mut d = Dialog::blank(Kind::SheetRules);
    d.load_rule(&s, "");
    d.name = "Alu".into();
    d.a = "1.5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    let mut d = Dialog::blank(Kind::Flange);
    d.choice = "base";
    d.sketch = s.edit_sketch;
    d.rule = "Alu".into();
    run(&mut s, "sketch.finish", json!({}));
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Flange);
    d.choice = "edge";
    d.edges = vec![edge(&s, 0, |e| e.length > 79.0 && e.mid.y > 39.0 && e.mid.z > 1.0)];
    d.distance = "15 mm".into();
    d.angle = "90 deg".into();
    d.position = "adjacent";
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Hem);
    d.edges = vec![edge(&s, 0, |e| e.length > 79.0 && e.mid.y < 1.0 && e.mid.z > 1.0)];
    d.distance = "5 mm".into();
    d.distance2 = "0.5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::FlatPattern);
    d.refresh_report(&mut s);
    assert!(d.report.as_ref().is_some_and(|r| r["bends"].as_array().unwrap().len() == 2), "{:?}", d.error);
    let mut d = Dialog::blank(Kind::Unfold);
    apply(&mut s, &mut d);

    // fold a flat sheet along a sketch line
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    run(&mut s, "sheet.flange", json!({}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "sketch.create", json!({"plane": top}));
    let line = run(&mut s, "sketch.line", json!({"points": [[40, -5], [40, 45]]}));
    let mut d = Dialog::blank(Kind::Fold);
    d.sketch = s.edit_sketch;
    d.line = line["curves"].as_array().and_then(|a| a.first()).and_then(Value::as_u64).map(|v| v as u32);
    d.angle = "90 deg".into();
    run(&mut s, "sketch.finish", json!({}));
    apply(&mut s, &mut d);

    // convert a thin box, then rip a shelled box
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 30, "height": 2}));
    let mut d = Dialog::blank(Kind::ConvertSheet);
    d.bodies = vec![s.model.bodies[0].id.0];
    d.faces = vec![face(&s, 0, |f| f.normal.z < -0.9)];
    apply(&mut s, &mut d);
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 30, "height": 20}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "shell", json!({"faces": [top], "thickness": 1}));
    let mut d = Dialog::blank(Kind::Rip);
    d.edges = vec![edge(&s, 0, |e| e.length > 18.0 && e.length < 20.5 && e.mid.z > 5.0)];
    d.distance = "0.5 mm".into();
    apply(&mut s, &mut d);
  }

  #[test]
  fn assembly_dialogs() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
    run(&mut s, "box", json!({"width": 80, "depth": 20, "height": 5, "at": [40, 200]}));
    let (base, arm) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    for (b, n) in [(base, "Base"), (arm, "Arm")] {
      let mut d = Dialog::blank(Kind::NewComponent);
      d.bodies = vec![b];
      d.name = n.into();
      apply(&mut s, &mut d);
    }
    let comp = |s: &Session, b: u32| s.design.component_of(BodyId(b)).map(|c| c.id.0);
    let mut d = Dialog::blank(Kind::Material);
    d.comps = vec![comp(&s, arm)];
    d.name = "Aluminum".into();
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::Joint);
    d.choice = "rigid";
    d.origins = vec![
      (arm, json!({"face": face(&s, 1, |f| f.normal.z < -0.9)})),
      (base, json!({"face": face(&s, 0, |f| f.normal.z > 0.9)})),
    ];
    d.comps = vec![comp(&s, arm), comp(&s, base)];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::Bom);
    d.refresh_report(&mut s);
    assert_eq!(d.report.as_ref().and_then(Value::as_array).map(Vec::len), Some(2));
    let mut d = Dialog::blank(Kind::Interference);
    d.refresh_report(&mut s);
    assert!(d.report.is_some(), "{:?}", d.error);
  }

  #[test]
  fn sketch_modify_dialogs() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [100, 0]}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
    let lines: Vec<u32> = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    let mut d = Dialog::blank(Kind::SkFillet);
    d.curves = lines[..2].to_vec();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkChamfer2);
    d.curves = lines[2..4].to_vec();
    d.distance = "2 mm".into();
    d.distance2 = "3 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let c = run(&mut s, "sketch.circle", json!({"center": [20, 40], "radius": 3}));
    let circle = c["curves"][0].as_u64().unwrap() as u32;
    let mut d = Dialog::blank(Kind::SkOffset);
    d.curves = vec![circle];
    d.distance = "1 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkMirror);
    d.curves = vec![circle];
    d.line = Some(lines[0]);
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkCircPattern);
    d.curves = vec![circle];
    d.count = "4".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkRectPattern);
    d.curves = vec![circle];
    d.count = "2".into();
    d.use_dir2 = true;
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Project);
    d.bodies = vec![s.model.bodies[0].id.0];
    d.flag = true;
    apply(&mut s, &mut d);
    run(&mut s, "sketch.spline", json!({"points": [[0, -10], [10, -5], [20, -10]]}));
    run(&mut s, "sketch.trim", json!({"at": [30, 0]}));
  }
}
