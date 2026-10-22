//! The INSPECT analyses' dialogs (Curvature Comb, Curvature Map, Draft, Environment Map,
//! Isocurve, Zebra, Accessibility, Minimum Radius) and Center of Mass, as Fusion lays them out:
//! a pick row first ("Select"), nothing drawn and OK greyed until something is picked, then the
//! analysis's own rows. OK keeps the analysis in the browser's Analysis folder (see
//! [`crate::analyses`]); Cancel and Escape leave nothing behind.

use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{Choice, choice, icon_row};
use super::{Dialog, Kind, chip_text, plane_of, sel_chip};
use crate::App;
use crate::icons::Icon;
use crate::select::Priority;
use crate::theme;

/// An analysis dialog's settings, with Fusion's defaults.
pub(super) struct AnalysisForm {
  /// The selection row the next picks go to: 0 the bodies (or edges, faces), 1 the direction.
  pub row: usize,
  /// The picked direction (unit) and what it was picked on.
  pub dir: Option<[f64; 3]>,
  /// The selection as last seen, to tell new picks from old ones.
  seen: Vec<Sel>,
  /// Curvature Map's Type, Zebra's Direction or Isocurve's Type.
  pub choice: &'static str,
  /// Curvature Map's Display.
  pub display: &'static str,
  pub scale: f64,
  pub opacity: f64,
  /// Draft Angle and its Tolerance (degrees), each a range.
  pub angle: [f64; 2],
  pub tolerance: bool,
  pub tol: [f64; 2],
  pub repeats: f64,
  pub density: f64,
  /// Environment Map's environment (one of [`ENVIRONMENTS`]).
  pub environment: &'static str,
  pub rotate: f64,
  pub mirror: bool,
  pub count: f64,
  /// Minimum Radius: the largest tool radius and the radius coloured red below (mm).
  pub max_tool: f64,
  pub min_radius: f64,
}

impl AnalysisForm {
  pub fn new(kind: &str) -> AnalysisForm {
    AnalysisForm {
      row: 0,
      dir: None,
      seen: Vec::new(),
      choice: match kind {
        "zebra" => "horizontal",
        "isocurve" => "uv",
        _ => "gaussian",
      },
      display: "smooth",
      scale: if kind == "comb" { 10.0 } else { 1.0 },
      opacity: 100.0,
      angle: [-2.0, 2.0],
      tolerance: true,
      tol: [-0.5, 0.5],
      repeats: 20.0,
      density: 50.0,
      environment: ENVIRONMENTS[0],
      rotate: 0.0,
      mirror: true,
      count: 5.0,
      max_tool: 25.0,
      min_radius: 0.0,
    }
  }
}

/// The analyses with a Direction row (and a single Body).
fn directed(kind: &str) -> bool {
  matches!(kind, "draft" | "accessibility")
}

/// The analyses that take one body (Fusion's "Body" row rather than "Bodies").
fn single_body(kind: &str) -> bool {
  directed(kind) || kind == "min_radius"
}

const CURVATURE_TYPES: [Choice; 3] = [
  choice("gaussian", Icon::Sphere, "Gaussian"),
  choice("mean", Icon::Cylinder, "Mean"),
  choice("max", Icon::Torus, "Maximum"),
];
const CURVATURE_DISPLAY: [Choice; 2] =
  [choice("smooth", Icon::CurvMap, "Smooth"), choice("banded", Icon::Zebra, "Banded")];
const ZEBRA_DIRECTIONS: [Choice; 2] =
  [choice("horizontal", Icon::Horizontal, "Horizontal"), choice("vertical", Icon::Vertical, "Vertical")];
const ISOCURVE_TYPES: [Choice; 3] =
  [choice("u", Icon::Vertical, "U"), choice("v", Icon::Horizontal, "V"), choice("uv", Icon::Isocurve, "U and V")];

/// Environment Map's environments, Fusion's default first.
const ENVIRONMENTS: [&str; 6] =
  ["Sharp Highlights", "Soft Highlights", "Photo Booth", "Infinity Pool", "Grey Room", "Tranquility Blue"];

/// Why a check box is greyed: FissionOxide's analyses draw on the display mesh.
const MESH_ONLY: &str = "Not in FissionOxide yet: the analysis works on the display mesh";

impl Dialog {
  /// Is there enough picked to show the analysis (and to OK it)?
  pub(super) fn analysis_ready(&self, kind: &str) -> bool {
    match kind {
      "comb" => !self.edges.is_empty(),
      "isocurve" => !self.bodies.is_empty() || !self.faces.is_empty(),
      k if directed(k) => !self.bodies.is_empty() && self.an.dir.is_some(),
      _ => !self.bodies.is_empty(),
    }
  }

  /// What the analysis still needs, as the cursor prompt says it.
  pub(super) fn analysis_missing(&self, kind: &str) -> &'static str {
    match kind {
      "comb" => "select edges to analyze",
      "isocurve" => "select bodies or faces to analyze",
      k if single_body(k) && self.bodies.is_empty() => "select a body to analyze",
      k if directed(k) => "select a direction",
      _ => "select bodies to analyze",
    }
  }

  /// The render parameters the analysis draws with (see `fox_engine::scene::view_options`).
  pub(super) fn analysis_params(&self, kind: &str) -> Value {
    let a = &self.an;
    let mut p = json!({"analysis": kind, "bodies": self.bodies, "opacity": a.opacity});
    let set = |p: &mut Value, more: Value| {
      if let (Some(p), Value::Object(m)) = (p.as_object_mut(), more) {
        p.extend(m);
      }
    };
    match kind {
      "curvature" => {
        set(&mut p, json!({"type": a.choice, "banded": a.display == "banded", "scale": a.scale}));
      }
      "zebra" => set(&mut p, json!({"repeats": a.repeats, "vertical": a.choice == "vertical"})),
      "draft" => {
        let tol = if a.tolerance { a.tol[0].abs().max(a.tol[1].abs()) } else { 0.0 };
        set(&mut p, json!({"pull": a.dir, "lo": a.angle[0], "hi": a.angle[1], "tol": tol}));
      }
      "accessibility" => set(&mut p, json!({"pull": a.dir})),
      "environment" => {
        set(&mut p, json!({"rotate": a.rotate, "mirror": a.mirror, "environment": a.environment}));
      }
      "isocurve" => set(&mut p, json!({"faces": self.faces, "type": a.choice, "count": a.count})),
      "min_radius" => set(&mut p, json!({"min": a.min_radius, "max": a.max_tool})),
      "comb" => set(&mut p, json!({"edges": self.edges, "density": a.density, "scale": a.scale})),
      _ => {}
    }
    p
  }

  /// Takes the picks made in the canvas or the browser since the last frame, routed by the
  /// active row, and points the selection filter at what the row picks.
  pub(super) fn analysis_sync(&mut self, app: &mut App, kind: &str) {
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
    }
    let wants_faces = kind == "isocurve" || (directed(kind) && self.an.row == 1);
    app.sel_priority = match kind {
      "comb" => Priority::Edge,
      _ if wants_faces => Priority::Face,
      _ => Priority::Body,
    };
    let s = &app.s;
    let fresh: Vec<Sel> = s.selection.iter().filter(|x| !self.an.seen.contains(x)).copied().collect();
    let gone = self.an.seen.iter().any(|x| !s.selection.contains(x));
    self.an.seen = s.selection.clone();
    if directed(kind) && self.an.row == 1 {
      // the direction: a planar face's normal, a cylinder's axis, a straight edge, or an origin
      // or construction plane picked in the browser
      let picked = fresh.iter().rev().find_map(|x| direction_of(s, x));
      let plane = app.picked_plane.as_ref().and_then(|p| plane_of(s, p)).map(|p| p.normal().to_array());
      if let Some(d) = picked.or(plane) {
        self.an.dir = Some(d);
      }
      return;
    }
    match kind {
      "comb" => self.edges = super::sel_edges(s),
      "isocurve" => {
        self.faces = super::sel_faces(s);
        self.bodies = super::sel_only_bodies(s);
      }
      k if single_body(k) => {
        // the newest body picked (a click on a face or edge picks its body)
        if let Some(b) = fresh.iter().rev().find_map(body_of) {
          self.bodies = vec![b];
          if directed(k) && self.an.dir.is_none() {
            // Fusion moves on to Direction once the body is picked
            self.an.row = 1;
          }
        } else if gone && !s.selection.iter().filter_map(body_of).any(|b| self.bodies.contains(&b)) {
          self.bodies.clear();
        }
      }
      _ => self.bodies = super::sel_bodies(s),
    }
  }

  /// The rows of an analysis dialog.
  pub(super) fn inspect_analysis_rows(&mut self, app: &mut App, ui: &mut egui::Ui, kind: &'static str) {
    self.analysis_sync(app, kind);
    let accent = self.accent();
    // the first row: what to analyse
    let (label, text, has) = match kind {
      "comb" => ("Edges", chip_text(self.edges.len(), "selected"), !self.edges.is_empty()),
      "isocurve" => {
        let n = self.bodies.len() + self.faces.len();
        ("Bodies/Faces", chip_text(n, "selected"), n > 0)
      }
      k if single_body(k) => ("Body", chip_text(self.bodies.len(), "selected"), !self.bodies.is_empty()),
      _ => ("Bodies", chip_text(self.bodies.len(), "selected"), !self.bodies.is_empty()),
    };
    if directed(kind) {
      self.pick_row_an(ui, label, 0, &text, has, accent);
      let d = if self.an.dir.is_some() { "1 selected".to_string() } else { "Select".to_string() };
      let has = self.an.dir.is_some();
      self.pick_row_an(ui, "Direction", 1, &d, has, accent);
    } else {
      ui.label(label);
      sel_chip(ui, accent, &text, has);
      ui.end_row();
    }
    let a = &mut self.an;
    match kind {
      "curvature" => {
        ui.label("Type");
        icon_row(ui, &mut a.choice, &CURVATURE_TYPES, "");
        ui.end_row();
        ui.label("Display");
        icon_row(ui, &mut a.display, &CURVATURE_DISPLAY, "");
        ui.end_row();
        slider_row(ui, "Scale", &mut a.scale, 0.0..=10.0, 3, "");
        opacity_row(ui, &mut a.opacity);
        greyed_check(ui, "High Quality");
      }
      "zebra" => {
        ui.label("Direction");
        icon_row(ui, &mut a.choice, &ZEBRA_DIRECTIONS, "");
        ui.end_row();
        slider_row(ui, "Repeats", &mut a.repeats, 1.0..=100.0, 0, "");
        opacity_row(ui, &mut a.opacity);
        greyed_check(ui, "Lock Stripes");
        greyed_check(ui, "High Quality");
      }
      "draft" => {
        range_row(ui, "Draft Angle", &mut a.angle, -45.0..=45.0);
        ui.label("");
        ui.checkbox(&mut a.tolerance, "Tolerance");
        ui.end_row();
        if a.tolerance {
          range_row(ui, "Tolerance", &mut a.tol, -5.0..=5.0);
        }
        opacity_row(ui, &mut a.opacity);
      }
      "accessibility" => opacity_row(ui, &mut a.opacity),
      "environment" => {
        ui.label("Environment");
        egui::ComboBox::from_id_salt("an-env").selected_text(a.environment).width(150.0).show_ui(ui, |ui| {
          for e in ENVIRONMENTS {
            ui.selectable_value(&mut a.environment, e, e);
          }
        });
        ui.end_row();
        slider_row(ui, "Rotate", &mut a.rotate, -180.0..=180.0, 1, " deg");
        ui.label("");
        ui.checkbox(&mut a.mirror, "Apply Mirror");
        ui.end_row();
      }
      "isocurve" => {
        ui.label("Type");
        icon_row(ui, &mut a.choice, &ISOCURVE_TYPES, "");
        ui.end_row();
        slider_row(ui, "Isocurves", &mut a.count, 1.0..=50.0, 0, "");
        greyed_check(ui, "Curvature Combs");
      }
      "min_radius" => {
        let u = app.s.design.units;
        ui.label("Maximum Tool Radius");
        let mut v = u.from_mm(a.max_tool);
        if ui
          .add(
            egui::DragValue::new(&mut v)
              .speed(0.1 / u.mm())
              .range(0.0..=f64::MAX)
              .max_decimals(3)
              .suffix(format!(" {}", u.symbol())),
          )
          .changed()
        {
          a.max_tool = v * u.mm();
        }
        ui.end_row();
        let mut v = u.from_mm(a.min_radius);
        let top = u.from_mm(a.max_tool);
        slider_row(ui, "Minimum Radius", &mut v, 0.0..=top, 3, &format!(" {}", u.symbol()));
        a.min_radius = (v * u.mm()).min(a.max_tool);
        opacity_row(ui, &mut a.opacity);
        greyed_check(ui, "Sharp Edges");
      }
      "comb" => {
        slider_row(ui, "Density", &mut a.density, 2.0..=200.0, 0, "");
        slider_row(ui, "Scale", &mut a.scale, 0.0..=100.0, 3, "");
      }
      _ => {}
    }
    // nothing is drawn until something is picked, as in Fusion
    app.analysis = self.analysis_ready(kind).then(|| self.analysis_params(kind));
  }

  /// A selection row of a dialog with two (Body and Direction): lit while it is the active row,
  /// a click on its chip makes it active.
  fn pick_row_an(&mut self, ui: &mut egui::Ui, label: &str, row: usize, text: &str, has: bool, accent: egui::Color32) {
    ui.label(label);
    let active = self.an.row == row;
    let chip = sel_chip(ui, accent, text, active || has);
    if !active && chip.interact(egui::Sense::click()).on_hover_text("Pick for this row").clicked() {
      self.an.row = row;
    }
    ui.end_row();
  }

  /// Center of Mass: one Selection row (bodies), and the centre worked out for the glyph.
  pub(super) fn center_of_mass_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
    }
    app.sel_priority = Priority::Body;
    let b = super::sel_bodies(&app.s);
    if b != self.bodies {
      self.bodies = b;
      self.report = None;
    }
    if !self.bodies.is_empty() && self.report.is_none() {
      self.report = center_of_mass(&mut app.s, &self.bodies).map(|c| json!({"center": c}));
    }
    ui.label("Selection");
    sel_chip(ui, self.accent(), &chip_text(self.bodies.len(), "selected"), !self.bodies.is_empty());
    ui.end_row();
  }

  /// OK on an analysis or Center of Mass: what to keep in the Analysis folder (its kind and
  /// parameters), once enough is picked.
  pub(super) fn analysis_ok(&self) -> Option<(&'static str, Value)> {
    match self.kind {
      Kind::Analysis(k) if self.analysis_ready(k) => Some((k, self.analysis_params(k))),
      Kind::CenterOfMass if !self.bodies.is_empty() => Some(("center_of_mass", json!({"bodies": self.bodies}))),
      _ => None,
    }
  }
}

/// The centre of mass of `bodies` (mm), if they can be weighed.
pub(crate) fn center_of_mass(s: &mut Session, bodies: &[u32]) -> Option<[f64; 3]> {
  let r = s.execute("inspect.center_of_mass", &json!({"bodies": bodies})).ok()?;
  let c: Vec<f64> = r.get("center")?.as_array()?.iter().filter_map(Value::as_f64).collect();
  match c.as_slice() {
    [x, y, z] => Some([*x, *y, *z]),
    _ => None,
  }
}

fn body_of(x: &Sel) -> Option<u32> {
  match x {
    Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } => Some(body.0),
    _ => None,
  }
}

/// The direction a pick gives: a planar face's normal, a cylinder's or cone's axis, a straight
/// edge's direction.
fn direction_of(s: &Session, x: &Sel) -> Option<[f64; 3]> {
  use fox_engine::kernel::{CurveKind, SurfaceKind};
  match x {
    Sel::Face { body, face } => {
      let f = s.model.body(*body)?.faces().iter().find(|f| f.id == *face)?;
      matches!(f.kind, SurfaceKind::Plane | SurfaceKind::Cylinder | SurfaceKind::Cone)
        .then(|| f.normal.normalize_or_zero().to_array())
    }
    Sel::Edge { body, edge } => {
      let e = s.model.body(*body)?.edges().iter().find(|e| e.id == *edge)?;
      let d = e.end - e.start;
      (e.kind == CurveKind::Line && d.length() > 1e-9).then(|| d.normalize().to_array())
    }
    _ => None,
  }
}

/// A value with a slider, as Fusion's analysis rows: the slider and its box.
fn slider_row(
  ui: &mut egui::Ui,
  label: &str,
  v: &mut f64,
  range: std::ops::RangeInclusive<f64>,
  decimals: usize,
  suffix: &str,
) {
  ui.label(label);
  ui.add(egui::Slider::new(v, range).fixed_decimals(decimals).suffix(suffix));
  ui.end_row();
}

fn opacity_row(ui: &mut egui::Ui, v: &mut f64) {
  slider_row(ui, "Opacity", v, 0.0..=100.0, 0, "");
}

/// A check box FissionOxide can't act on yet, greyed with the reason as its tooltip.
fn greyed_check(ui: &mut egui::Ui, label: &str) {
  ui.label("");
  let mut off = false;
  ui.add_enabled_ui(false, |ui| ui.checkbox(&mut off, label)).inner.on_disabled_hover_text(MESH_ONLY);
  ui.end_row();
}

/// A range (degrees): its two boxes and Fusion's two-thumb slider between them.
fn range_row(ui: &mut egui::Ui, label: &str, v: &mut [f64; 2], range: std::ops::RangeInclusive<f64>) {
  ui.label(label);
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 4.0;
    let (lo, hi) = (*range.start(), *range.end());
    let top = v[1];
    ui.add(egui::DragValue::new(&mut v[0]).speed(0.1).range(lo..=top).fixed_decimals(1).suffix(" deg"));
    two_thumb(ui, v, lo, hi);
    let bottom = v[0];
    ui.add(egui::DragValue::new(&mut v[1]).speed(0.1).range(bottom..=hi).fixed_decimals(1).suffix(" deg"));
  });
  ui.end_row();
}

/// A slider with two thumbs for a range's ends; dragging moves the thumb nearer the pointer.
fn two_thumb(ui: &mut egui::Ui, v: &mut [f64; 2], lo: f64, hi: f64) {
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(64.0, 18.0), egui::Sense::click_and_drag());
  let x_of = |t: f64| rect.left() + 6.0 + ((t - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 * (rect.width() - 12.0);
  if let Some(p) = resp.interact_pointer_pos() {
    let t = lo + ((p.x - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0) as f64 * (hi - lo);
    let t = (t * 10.0).round() / 10.0;
    let near = if (p.x - x_of(v[0])).abs() <= (p.x - x_of(v[1])).abs() { 0 } else { 1 };
    v[near] = if near == 0 { t.min(v[1]) } else { t.max(v[0]) };
  }
  let p = ui.painter();
  let y = rect.center().y;
  p.line_segment([egui::pos2(rect.left() + 6.0, y), egui::pos2(rect.right() - 6.0, y)], (3.0, theme::BORDER));
  p.line_segment([egui::pos2(x_of(v[0]), y), egui::pos2(x_of(v[1]), y)], (3.0, theme::ACCENT));
  for t in *v {
    p.circle(egui::pos2(x_of(t), y), 5.0, egui::Color32::WHITE, (1.0, theme::GROUP_LABEL));
  }
}
