//! Fusion's Hole and Rectangular Pattern dialogs: the single hole placed by clicking a face and
//! held at offsets from edges (or holes at a sketch's points), tap type, drill point and the hole
//! diagram with its value boxes; the pattern type, directions along an edge, flip, suppressed
//! instances and the drag arrow.

use egui::{Color32, Pos2, Rect, RichText, Stroke, pos2, vec2};
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, FeatureId};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{Choice, choice as ic, greyed, icon_row};
use super::manip::mm;
use super::{Dialog, Kind, axis_label, axis_vec, is_planar, sel_chip, value_edit_w};
use crate::handles::Handle;
use crate::icons::Icon;
use crate::{App, theme};

/// The dialogs' extra values (kept in [`super::solid::Extra`]).
#[derive(Debug, Clone, Default)]
pub(super) struct State {
  /// Hole: Fusion's single hole on a face (else holes at the sketch's points).
  pub(super) single: bool,
  /// Hole: where the face was clicked (a model point) and the face it was found for.
  at: Option<(Value, [f64; 3])>,
  /// Hole: the distances from the point to the reference edges (the dialog's `edges`).
  offsets: [String; 2],
  /// Hole: Simple, Clearance or Tapped ("" = Simple), the fastener size ("" = custom).
  tap: &'static str,
  size: &'static str,
  /// Hole: the drill point's tip angle; flat ("" = angled).
  tip: String,
  flat: bool,
  /// Hole: the Shape Settings group is folded up.
  shape_closed: bool,
  /// Pattern type: "bodies" ("") or "components".
  pub(super) kind: &'static str,
  /// Directions picked as edges: the edge and its unit direction.
  edges: [Option<(Value, [f64; 3])>; 2],
  flip: [bool; 2],
  /// Components of the picked bodies (component patterns).
  comps: Vec<u32>,
  /// Instances left out ([i, j], the original at [0, 0]).
  suppress: Vec<[u32; 2]>,
}

/// ISO metric sizes: name, medium clearance hole (ISO 273), tap drill and coarse pitch.
const SIZES: [(&str, f64, f64, f64); 11] = [
  ("M2", 2.4, 1.6, 0.4),
  ("M2.5", 2.9, 2.05, 0.45),
  ("M3", 3.4, 2.5, 0.5),
  ("M4", 4.5, 3.3, 0.7),
  ("M5", 5.5, 4.2, 0.8),
  ("M6", 6.6, 5.0, 1.0),
  ("M8", 9.0, 6.8, 1.25),
  ("M10", 11.0, 8.5, 1.5),
  ("M12", 13.5, 10.2, 1.75),
  ("M16", 17.5, 14.0, 2.0),
  ("M20", 22.0, 17.5, 2.5),
];

/// Fusion's Hole Placement, Hole Type, Hole Tap Type and Drill Point, as rows of icons.
const PLACEMENTS: [Choice; 2] =
  [ic("single", Icon::HoleSingle, "Single Hole"), ic("points", Icon::HoleAtPoints, "Multiple Holes")];
const HOLES: [Choice; 3] = [
  ic("simple", Icon::HoleSimple, "Simple"),
  ic("counterbore", Icon::HoleCounterbore, "Counterbore"),
  ic("countersink", Icon::HoleCountersink, "Countersink"),
];
const TAPS: [Choice; 4] = [
  ic("simple", Icon::TapSimple, "Simple"),
  ic("clearance", Icon::TapClearance, "Clearance"),
  ic("tapped", Icon::TapTapped, "Tapped"),
  greyed("taper", Icon::TapTaper, "Taper Tapped"),
];
const DRILL_POINTS: [Choice; 2] = [ic("angle", Icon::DrillAngle, "Angle"), ic("flat", Icon::DrillFlat, "Flat")];

/// Fusion's pattern types: key, label and whether Fox can pattern them yet.
const PATTERN_TYPES: [(&str, &str, bool); 4] = [
  ("faces", "Faces", false),
  ("bodies", "Bodies", true),
  ("features", "Features", false),
  ("components", "Components", true),
];

/// Most instances the suppress grid shows in a direction.
const GRID_MAX: u32 = 12;

fn ref_of(v: &Value, k: &str) -> Option<u32> {
  v.get(k).and_then(Value::as_u64).map(|x| x as u32)
}

/// A straight edge's ends.
pub(super) fn line_of(s: &Session, e: &Value) -> Option<(DVec3, DVec3)> {
  let b = s.model.body(BodyId(ref_of(e, "body")?))?;
  let e = b.edges().iter().find(|x| Some(x.id) == ref_of(e, "edge"))?;
  (e.kind == fox_engine::kernel::CurveKind::Line).then_some((e.start, e.end))
}

fn face_info(s: &Session, f: &Value) -> Option<fox_engine::kernel::FaceInfo> {
  s.model.body(BodyId(ref_of(f, "body")?))?.faces().iter().find(|x| Some(x.id) == ref_of(f, "face")).copied()
}

/// Is `p` on the face (within one of its display triangles, seen along its normal)?
fn on_face(s: &Session, f: &Value, p: DVec3) -> bool {
  let (Some(b), Some(id), Some(info)) =
    (s.model.body(BodyId(ref_of(f, "body").unwrap_or(0))), ref_of(f, "face"), face_info(s, f))
  else {
    return false;
  };
  let Some(m) = b.mesh() else { return false };
  let n = info.normal;
  let v = |i: u32| DVec3::from(m.positions[i as usize].map(f64::from));
  m.indices.as_chunks::<3>().0.iter().zip(&m.tri_face).filter(|(_, f)| **f == id).any(|(t, _)| {
    let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
    let side = |u: DVec3, w: DVec3| (w - u).cross(p - u).dot(n);
    let (x, y, z) = (side(a, b), side(b, c), side(c, a));
    let tol = -1e-6 * (b - a).length_squared().max(1.0);
    (x >= tol && y >= tol && z >= tol) || (x <= -tol && y <= -tol && z <= -tol)
  })
}

/// Where the last viewport click meets the planar face `f`, when it lands on the face.
fn click_on(app: &App, f: &Value) -> Option<[f64; 3]> {
  let click = app.last_click?;
  let r = app.canvas;
  if !r.contains(click) {
    return None;
  }
  // the ray only depends on where the click is across the view
  let k = 4.0;
  let (w, h) = ((r.width() * k).round() as u32, (r.height() * k).round() as u32);
  let (o, d) = app.cam.ray(((click.x - r.min.x) * k) as f64, ((click.y - r.min.y) * k) as f64, w, h);
  let info = face_info(&app.s, f)?;
  let p = fox_engine::render::ray_plane(o, d, info.origin, info.normal)?;
  on_face(&app.s, f, p).then_some([p.x, p.y, p.z])
}

/// The distance from `p` to the line through `a` and `b`.
fn line_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
  let d = (b - a).normalize_or_zero();
  let v = p - a;
  (v - d * v.dot(d)).length()
}

/// Sketch points a hole would drill: not the origin, not on a curve.
pub(super) fn free_points(s: &Session, sketch: FeatureId) -> usize {
  let Some(r) = s.model.sketches.get(&sketch) else { return 0 };
  let used: Vec<_> = r.sketch.curves.iter().flat_map(|c| r.sketch.curve_points(c.id)).collect();
  r.sketch.points.iter().filter(|p| p.id != fox_engine::sketch::ORIGIN && !used.contains(&p.id)).count()
}

/// Sketches made to place a single hole on a face (Fusion shows no sketch for those): the
/// browser leaves them out.
pub(crate) fn placement_sketches(s: &Session) -> Vec<FeatureId> {
  s.design
    .features
    .iter()
    .filter_map(|f| match &f.kind {
      fox_engine::model::FeatureKind::Hole { sketch, placed: true, .. } => Some(*sketch),
      _ => None,
    })
    .collect()
}

impl Dialog {
  // ---- hole ----

  /// Fusion's Hole defaults: one hole on a face, unless the sketch in play has points to drill.
  pub(super) fn hole_defaults(&mut self, s: &Session) {
    let face = s.selection.iter().any(|x| matches!(x, Sel::Face { body, face } if is_planar(s, *body, *face)));
    self.solid.hp.single = face || self.sketch.is_none_or(|sk| free_points(s, sk) == 0);
    self.solid.hp.tap = "simple";
    self.solid.hp.tip = "118 deg".into();
  }

  pub(super) fn hole_single(&self) -> bool {
    self.kind == Kind::Hole && self.solid.hp.single
  }

  pub(super) fn hole_ready(&self) -> bool {
    if self.solid.hp.single { !self.faces.is_empty() } else { self.sketch.is_some() }
  }

  /// Takes a single hole's picks from the selection: the last planar face picked (the hole goes
  /// where it was clicked) and up to two straight edges to hold it from.
  pub(super) fn hole_sync(&mut self, app: &App) {
    if !self.hole_single() || self.editing.is_some() || self.applied {
      return;
    }
    let s = &app.s;
    let face: Vec<Value> = s
      .selection
      .iter()
      .rev()
      .find_map(|x| match x {
        Sel::Face { body, face } if is_planar(s, *body, *face) => Some(json!({"body": body.0, "face": face})),
        _ => None,
      })
      .into_iter()
      .collect();
    if face != self.faces {
      self.faces = face;
      self.solid.hp.at = self.faces.first().and_then(|f| click_on(app, f).map(|p| (f.clone(), p)));
    }
    let edges: Vec<Value> = s
      .selection
      .iter()
      .filter_map(|x| match x {
        Sel::Edge { body, edge } => Some(json!({"body": body.0, "edge": edge})),
        _ => None,
      })
      .filter(|e| line_of(s, e).is_some())
      .take(2)
      .collect();
    if edges != self.edges {
      // a new reference starts at the distance the hole is from it now
      let centre = self.hole_centre(s);
      let u = self.unit;
      for (i, e) in edges.iter().enumerate() {
        if self.edges.get(i) != Some(e) {
          let now = centre.zip(line_of(s, e)).map(|(c, (a, b))| line_distance(c, a, b));
          self.solid.hp.offsets[i] = now.map(|v| mm(u, v)).unwrap_or_else(|| u.localize("10 mm"));
        }
      }
      self.edges = edges;
    }
  }

  /// Where a single hole sits now: the click, or the face's centre.
  fn hole_centre(&self, s: &Session) -> Option<DVec3> {
    let f = self.faces.first()?;
    match &self.solid.hp.at {
      Some((g, p)) if g == f => Some(DVec3::from(*p)),
      _ => face_info(s, f).map(|i| i.origin),
    }
  }

  /// The `hole` command's parameters.
  pub(super) fn hole_params(&self) -> Option<Value> {
    let hp = &self.solid.hp;
    let mut p = json!({"kind": self.choice, "diameter": self.a, "flip": self.flip});
    if hp.single {
      let f = self.faces.first()?;
      p["face"] = f.clone();
      if let Some((g, at)) = &hp.at
        && g == f
      {
        p["at"] = json!(at);
      }
      let offsets: Vec<Value> =
        self.edges.iter().zip(&hp.offsets).map(|(e, d)| json!({"edge": e, "distance": d})).collect();
      if !offsets.is_empty() {
        p["offsets"] = json!(offsets);
      }
    } else {
      p["sketch"] = json!(self.sketch?.0);
    }
    if self.flag {
      p["through_all"] = json!(true);
    } else {
      p["depth"] = json!(self.b);
      if hp.flat {
        p["drill_point"] = json!("flat");
      } else if !hp.tip.trim().is_empty() {
        p["tip_angle"] = json!(hp.tip);
      }
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
    if matches!(hp.tap, "clearance" | "tapped") {
      p["tap"] = json!(hp.tap);
    }
    if hp.tap == "tapped"
      && let Some((_, _, _, pitch)) = SIZES.iter().find(|x| x.0 == hp.size)
    {
      p["pitch"] = json!(format!("{pitch} mm"));
    }
    Some(p)
  }

  /// Fusion's Hole dialog: Placement (icons), Face and the two Reference rows (or the sketch and
  /// its points), then the Shape Settings group: Extents, Hole Type, Hole Tap Type, Drill Point
  /// and the hole diagram with its value boxes.
  pub(super) fn hole_fields(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    let accent = self.accent();
    ui.label("Placement");
    let mut placement: &'static str = if self.solid.hp.single { "single" } else { "points" };
    let switched = icon_row(ui, &mut placement, &PLACEMENTS, "");
    ui.end_row();
    if switched {
      self.withdraw(&mut app.s);
      self.solid.hp.single = placement == "single";
      *dirty = true;
    }
    if self.solid.hp.single {
      ui.label("Face");
      let text = match self.faces.first().and_then(|f| ref_of(f, "body")) {
        Some(b) => format!("Face of {}", app.s.body_name(BodyId(b))),
        None => "Select".into(),
      };
      sel_chip(ui, accent, &text, !self.faces.is_empty()).on_hover_text("Click a planar face where the hole goes");
      ui.end_row();
      // two references, each a straight edge the hole is held at a distance from
      for i in 0..2 {
        ui.label(format!("Reference {}", i + 1));
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 4.0;
          let has = i < self.edges.len();
          sel_chip(ui, accent, if has { "1 Edge" } else { "Select" }, has)
            .on_hover_text("A straight edge to dimension the hole from");
          if has && ui.add(egui::TextEdit::singleline(&mut self.solid.hp.offsets[i]).desired_width(60.0)).changed() {
            *dirty = true;
          }
        });
        ui.end_row();
      }
    } else {
      ui.label("Sketch");
      ui.label(self.sketch.and_then(|s| app.s.design.feature(s)).map(|f| f.name.clone()).unwrap_or("none".into()));
      ui.end_row();
      ui.label("Points");
      let n = self.sketch.map(|sk| free_points(&app.s, sk)).unwrap_or(0);
      ui.label(RichText::new(format!("{n} sketch point{}", if n == 1 { "" } else { "s" })).color(theme::MUTED));
      ui.end_row();
    }
    // the Shape Settings group, folded up by its header
    let open = !self.solid.hp.shape_closed;
    ui.label("");
    let header = ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 4.0;
      let (r, resp) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::click());
      let c = r.center();
      let col = theme::GROUP_LABEL;
      let tri = if open {
        vec![pos2(c.x - 4.0, c.y - 2.0), pos2(c.x + 4.0, c.y - 2.0), pos2(c.x, c.y + 3.0)]
      } else {
        vec![pos2(c.x - 2.0, c.y - 4.0), pos2(c.x + 3.0, c.y), pos2(c.x - 2.0, c.y + 4.0)]
      };
      ui.painter().add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
      let label =
        ui.add(egui::Label::new(RichText::new("Shape Settings").color(col).strong()).sense(egui::Sense::click()));
      resp.clicked() || label.clicked()
    });
    ui.end_row();
    if header.inner {
      self.solid.hp.shape_closed = open;
    }
    if !open {
      return;
    }
    let mut extent = if self.flag { "all" } else { "distance" };
    Self::choose(ui, "Extents", "holeextent", &mut extent, &[("distance", "Distance"), ("all", "All")], dirty);
    self.flag = extent == "all";
    ui.label("Hole Type");
    *dirty |= icon_row(ui, &mut self.choice, &HOLES, "");
    ui.end_row();
    ui.label("Hole Tap Type");
    let tap_before = self.solid.hp.tap;
    let mut tap = if tap_before.is_empty() { "simple" } else { tap_before };
    *dirty |= icon_row(ui, &mut tap, &TAPS, "needs engine support");
    ui.end_row();
    self.solid.hp.tap = tap;
    if tap != "simple" {
      ui.label("Size");
      let shown = if self.solid.hp.size.is_empty() { "Custom" } else { self.solid.hp.size };
      let mut picked = None;
      egui::ComboBox::from_id_salt("holesize").selected_text(shown).width(140.0).show_ui(ui, |ui| {
        for (name, clear, drill, _) in SIZES {
          let d = if tap == "tapped" { drill } else { clear };
          if ui.selectable_label(self.solid.hp.size == name, format!("{name}  (⌀ {d} mm)")).clicked() {
            picked = Some((name, d));
          }
        }
      });
      ui.end_row();
      if let Some((name, d)) = picked {
        self.solid.hp.size = name;
        self.a = mm(self.unit, d);
        *dirty = true;
      }
    }
    if tap != tap_before && !self.solid.hp.size.is_empty() {
      // the size stays; the diameter follows the tap type
      if let Some((_, clear, drill, _)) = SIZES.iter().find(|x| x.0 == self.solid.hp.size) {
        self.a = mm(self.unit, if tap == "tapped" { *drill } else { *clear });
      }
    }
    if !self.flag {
      ui.label("Drill Point");
      let mut point: &'static str = if self.solid.hp.flat { "flat" } else { "angle" };
      *dirty |= icon_row(ui, &mut point, &DRILL_POINTS, "");
      self.solid.hp.flat = point == "flat";
      ui.end_row();
    }
    ui.label("");
    self.hole_diagram(ui, dirty);
    ui.end_row();
  }

  /// Fusion's hole diagram: a section through the hole with its value boxes on the dimensions
  /// (head diameter and depth or angle, diameter, depth, tip angle).
  fn hole_diagram(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    let w = ui.available_width().clamp(150.0, 190.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 170.0), egui::Sense::hover());
    let (head, through, flat) = (self.choice, self.flag, self.solid.hp.flat);
    let box_w = 58.0;
    // the block: left part of the rect; value boxes to its right and below
    let block =
      Rect::from_min_max(pos2(rect.left(), rect.top() + 26.0), pos2(rect.right() - box_w - 8.0, rect.bottom() - 26.0));
    let cx = block.center().x;
    let (hw, head_w) = (13.0, 23.0);
    let head_bottom = block.top() + if head == "simple" { 0.0 } else { 16.0 };
    let bottom = if through { block.bottom() } else { block.bottom() - 26.0 };
    let tip = if through || flat { bottom } else { bottom + 12.0 };
    let p = ui.painter_at(rect);
    let ink = Stroke::new(1.0, theme::TEXT);
    let dim = Stroke::new(1.0, theme::ACCENT);
    p.rect_filled(block, 0.0, Color32::from_gray(214));
    // the hole's left side top to bottom; the outline runs on round the tip and up the right
    let mut left = match head {
      "counterbore" => vec![pos2(cx - head_w, block.top()), pos2(cx - head_w, head_bottom), pos2(cx - hw, head_bottom)],
      "countersink" => vec![pos2(cx - head_w, block.top()), pos2(cx - hw, head_bottom)],
      _ => vec![pos2(cx - hw, block.top())],
    };
    left.push(pos2(cx - hw, bottom));
    let mut outline = left.clone();
    if tip > bottom {
      outline.push(pos2(cx, tip));
    }
    outline.extend(left.iter().rev().map(|q| pos2(2.0 * cx - q.x, q.y)));
    // fill the hole in convex pieces: the head, the bore and the tip
    let fill = ui.visuals().window_fill;
    let piece = |pts: Vec<Pos2>| {
      p.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
    };
    if head_bottom > block.top() {
      let low = if head == "countersink" { hw } else { head_w };
      piece(vec![
        pos2(cx - head_w, block.top()),
        pos2(cx + head_w, block.top()),
        pos2(cx + low, head_bottom),
        pos2(cx - low, head_bottom),
      ]);
    }
    piece(vec![pos2(cx - hw, head_bottom), pos2(cx + hw, head_bottom), pos2(cx + hw, bottom), pos2(cx - hw, bottom)]);
    if tip > bottom {
      piece(vec![pos2(cx - hw, bottom), pos2(cx + hw, bottom), pos2(cx, tip)]);
    }
    p.add(egui::Shape::line(outline, ink));
    p.line_segment([pos2(cx, block.top() - 4.0), pos2(cx, tip + 4.0)], Stroke::new(0.8, theme::MUTED));
    // dimension lines
    let arrow = |a: Pos2, b: Pos2| {
      p.line_segment([a, b], dim);
      for (from, to) in [(a, b), (b, a)] {
        let d = (to - from).normalized() * 5.0;
        let n = vec2(-d.y, d.x) * 0.5;
        p.line_segment([from, from + d + n], dim);
        p.line_segment([from, from + d - n], dim);
      }
    };
    let dia_y = (head_bottom + bottom) / 2.0;
    arrow(pos2(cx - hw, dia_y), pos2(cx + hw, dia_y));
    let x_depth = block.right() + 4.0;
    arrow(pos2(x_depth, head_bottom.max(block.top())), pos2(x_depth, bottom));
    if head != "simple" {
      arrow(pos2(cx - head_w, block.top() - 6.0), pos2(cx + head_w, block.top() - 6.0));
    }
    // the value boxes
    let at = |c: Pos2| Rect::from_center_size(c, vec2(box_w, 20.0));
    let mut changed = false;
    let r = at(pos2(cx, rect.bottom() - 10.0));
    changed |= ui
      .scope_builder(egui::UiBuilder::new().max_rect(r), |ui| value_edit_w(ui, &mut self.a, box_w - 4.0))
      .inner
      .on_hover_text("Diameter")
      .changed();
    if !through {
      let r = at(pos2(rect.right() - box_w / 2.0, (head_bottom + bottom) / 2.0));
      changed |=
        ui.put(r, egui::TextEdit::singleline(&mut self.b).desired_width(box_w - 4.0)).on_hover_text("Depth").changed();
      if !flat {
        let r = at(pos2(rect.right() - box_w / 2.0, (bottom + rect.bottom()) / 2.0 + 4.0));
        changed |= ui
          .put(r, egui::TextEdit::singleline(&mut self.solid.hp.tip).desired_width(box_w - 4.0))
          .on_hover_text("Drill point angle")
          .changed();
      }
    }
    if head != "simple" {
      let r = at(pos2(cx, rect.top() + 9.0));
      changed |= ui
        .put(r, egui::TextEdit::singleline(&mut self.c).desired_width(box_w - 4.0))
        .on_hover_text("Head diameter")
        .changed();
      let (v, tip_text) = if head == "counterbore" { (&mut self.d, "Head depth") } else { (&mut self.e, "Head angle") };
      let r = at(pos2(rect.right() - box_w / 2.0, (block.top() + head_bottom) / 2.0 + 2.0));
      changed |= ui.put(r, egui::TextEdit::singleline(v).desired_width(box_w - 4.0)).on_hover_text(tip_text).changed();
    }
    if changed {
      *dirty = true;
      self.solid.hp.size = "";
    }
  }

  // ---- rectangular pattern ----

  fn pattern_kind(&self) -> &'static str {
    if self.solid.hp.kind.is_empty() { "bodies" } else { self.solid.hp.kind }
  }

  fn edge_mode(&self, k: usize) -> bool {
    if k == 0 { self.dir == "edge" } else { self.use_dir2 && self.dir2 == "edge" }
  }

  /// Takes the pattern's picks: bodies (and their components) and, for a direction set to
  /// Edge, the straight edges picked for it (those don't add their body).
  pub(super) fn pattern_sync(&mut self, s: &Session) {
    let edge_modes = [self.edge_mode(0), self.edge_mode(1)];
    let picks_edges = edge_modes.iter().any(|m| *m);
    let mut lines = s.selection.iter().filter_map(|x| match x {
      Sel::Edge { body, edge } => {
        let e = json!({"body": body.0, "edge": edge});
        let (a, b) = line_of(s, &e)?;
        let d = (b - a).normalize_or_zero();
        Some((e, [d.x, d.y, d.z]))
      }
      _ => None,
    });
    for (k, on) in edge_modes.into_iter().enumerate() {
      if !on {
        continue;
      }
      let next = lines.next();
      if next.as_ref().map(|x| &x.0) != self.solid.hp.edges[k].as_ref().map(|x| &x.0) {
        self.solid.hp.edges[k] = next;
        self.dirty = true;
      }
    }
    let mut b: Vec<u32> = Vec::new();
    for x in &s.selection {
      let id = match x {
        Sel::Body { body } | Sel::Face { body, .. } => body.0,
        Sel::Edge { body, .. } if !picks_edges => body.0,
        _ => continue,
      };
      if !b.contains(&id) {
        b.push(id);
      }
    }
    if !b.is_empty() && b != self.bodies {
      self.bodies = b;
      self.dirty = true;
    }
    let mut comps = Vec::new();
    for b in &self.bodies {
      if let Some(c) = s.design.component_of(BodyId(*b)).map(|c| c.id.0)
        && !comps.contains(&c)
      {
        comps.push(c);
      }
    }
    if comps != self.solid.hp.comps {
      self.solid.hp.comps = comps;
      self.dirty |= self.pattern_kind() == "components";
    }
  }

  pub(super) fn pattern_ready(&self) -> bool {
    let dirs_ok = (0..2).all(|k| !self.edge_mode(k) || self.solid.hp.edges[k].is_some());
    dirs_ok && (self.pattern_kind() != "components" || !self.solid.hp.comps.is_empty())
  }

  pub(super) fn pattern_missing(&self) -> &'static str {
    if (0..2).any(|k| self.edge_mode(k) && self.solid.hp.edges[k].is_none()) {
      "select a straight edge for the direction"
    } else {
      "select bodies of the components to pattern"
    }
  }

  /// A direction as the command takes it: an axis vector, or the picked edge.
  fn pattern_dir(&self, k: usize) -> Option<Value> {
    let flip = self.solid.hp.flip[k];
    if self.edge_mode(k) {
      let (e, _) = self.solid.hp.edges[k].as_ref()?;
      let mut e = e.clone();
      e["flip"] = json!(flip);
      return Some(e);
    }
    let v = axis_vec(if k == 0 { &self.dir } else { &self.dir2 });
    let sign = if flip { -1.0 } else { 1.0 };
    Some(json!([v[0] * sign, v[1] * sign, v[2] * sign]))
  }

  /// The unit vector of direction `k` (for the drag arrow).
  pub(super) fn pattern_vec(&self, k: usize) -> Option<DVec3> {
    let v = if self.edge_mode(k) {
      DVec3::from(self.solid.hp.edges[k].as_ref()?.1)
    } else {
      DVec3::from(axis_vec(if k == 0 { &self.dir } else { &self.dir2 }))
    };
    Some(if self.solid.hp.flip[k] { -v } else { v })
  }

  /// The `pattern.rectangular` command's parameters.
  pub(super) fn pattern_params(&self) -> Option<Value> {
    let mut p = json!({"direction": self.pattern_dir(0)?, "count": self.count, "spacing": self.spacing,
      "distance_type": if self.pattern_extent { "extent" } else { "spacing" }});
    let bodies = self.pattern_kind() != "components";
    if bodies {
      p["symmetric"] = json!(self.symmetric[0]);
    }
    if self.use_dir2 {
      p["direction2"] = self.pattern_dir(1)?;
      p["count2"] = json!(self.count2);
      p["spacing2"] = json!(self.spacing2);
      if bodies {
        p["symmetric2"] = json!(self.symmetric[1]);
      }
    }
    if self.pattern_kind() == "components" {
      p["components"] = json!(self.solid.hp.comps);
      let (n1, n2) = self.pattern_counts();
      let off: Vec<[u32; 2]> =
        self.solid.hp.suppress.iter().copied().filter(|[i, j]| *i < n1 && *j < n2 && (*i, *j) != (0, 0)).collect();
      if !off.is_empty() {
        p["suppress"] = json!(off);
      }
    } else {
      p["bodies"] = json!(self.bodies);
      p["join"] = json!(self.join);
    }
    Some(p)
  }

  /// The instance counts shown in the suppress grid.
  fn pattern_counts(&self) -> (u32, u32) {
    let n = |v: &str| v.trim().parse::<f64>().ok().filter(|n| *n >= 1.0).map(|n| n as u32).unwrap_or(1);
    (n(&self.count), if self.use_dir2 { n(&self.count2) } else { 1 })
  }

  /// Pattern Type, Objects, both directions (axis or edge, flip), Quantity, Distance, the
  /// instances to suppress and Join.
  pub(super) fn pattern_fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let creating = self.editing.is_none();
    let accent = self.accent();
    let mut dirty = false;
    if creating {
      ui.label("Pattern type");
      let shown = PATTERN_TYPES.iter().find(|t| t.0 == self.pattern_kind()).map(|t| t.1).unwrap_or("Bodies");
      egui::ComboBox::from_id_salt("ptype").selected_text(shown).width(140.0).show_ui(ui, |ui| {
        for (k, l, ok) in PATTERN_TYPES {
          if !ok {
            ui.add_enabled(false, egui::Button::new(l).frame(false))
              .on_disabled_hover_text("Not in Fox yet: patterns copy bodies or components");
          } else if ui.selectable_label(self.pattern_kind() == k, l).clicked() {
            self.solid.hp.kind = k;
            dirty = true;
          }
        }
      });
      ui.end_row();
    }
    ui.label("Objects");
    let text = if self.pattern_kind() == "components" {
      match self.solid.hp.comps.len() {
        0 => "Select".to_string(),
        n => format!("{n} component{}", if n == 1 { "" } else { "s" }),
      }
    } else {
      self.bodies_label()
    };
    let has =
      if self.pattern_kind() == "components" { !self.solid.hp.comps.is_empty() } else { !self.bodies.is_empty() };
    sel_chip(ui, accent, &text, has);
    ui.end_row();
    if creating {
      self.pattern_direction(ui, 0, &mut dirty);
    }
    Self::field(ui, "Quantity", &mut self.count, &mut dirty);
    Self::field(ui, "Distance", &mut self.spacing, &mut dirty);
    Self::check(ui, "Spacing is the extent", &mut self.pattern_extent, &mut dirty);
    // component patterns count suppressed instances from the original, so they aren't symmetric
    let bodies = self.pattern_kind() == "bodies";
    if bodies {
      Self::check(ui, "Symmetric", &mut self.symmetric[0], &mut dirty);
    }
    Self::check(ui, "Second direction", &mut self.use_dir2, &mut dirty);
    if self.use_dir2 {
      if creating {
        self.pattern_direction(ui, 1, &mut dirty);
      }
      Self::field(ui, "Quantity 2", &mut self.count2, &mut dirty);
      Self::field(ui, "Distance 2", &mut self.spacing2, &mut dirty);
      if bodies {
        Self::check(ui, "Symmetric 2", &mut self.symmetric[1], &mut dirty);
      }
    }
    if creating && self.pattern_kind() == "components" {
      self.suppress_grid(ui, &mut dirty);
    }
    if self.pattern_kind() == "bodies" {
      Self::check(ui, "Join instances", &mut self.join, &mut dirty);
    }
    let _ = app;
    if dirty {
      self.dirty = true;
    }
  }

  /// One direction row: an axis or Edge (picked in the viewport), and Flip.
  fn pattern_direction(&mut self, ui: &mut egui::Ui, k: usize, dirty: &mut bool) {
    let accent = self.accent();
    ui.label(if k == 0 { "Direction" } else { "Direction 2" });
    let cur = if k == 0 { self.dir.clone() } else { self.dir2.clone() };
    let mut pick = None;
    ui.horizontal(|ui| {
      let shown = if cur == "edge" { "Edge".to_string() } else { axis_label(&cur) };
      egui::ComboBox::from_id_salt(("pdir", k)).selected_text(shown).width(70.0).show_ui(ui, |ui| {
        for a in ["x", "y", "z"] {
          if ui.selectable_label(cur == a, axis_label(a)).clicked() {
            pick = Some(a);
          }
        }
        if ui.selectable_label(cur == "edge", "Edge").on_hover_text("Pick a straight edge in the viewport").clicked() {
          pick = Some("edge");
        }
      });
      if cur == "edge" {
        let has = self.solid.hp.edges[k].is_some();
        sel_chip(ui, accent, if has { "1 Edge" } else { "Select" }, has);
      }
      *dirty |= ui.checkbox(&mut self.solid.hp.flip[k], "Flip").changed();
    });
    ui.end_row();
    if let Some(a) = pick {
      if k == 0 {
        self.dir = a.into();
      } else {
        self.dir2 = a.into();
      }
      self.solid.hp.edges[k] = None;
      *dirty = true;
    }
  }

  /// Fusion's instance checkboxes: clear one to suppress that instance.
  fn suppress_grid(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    let (n1, n2) = self.pattern_counts();
    ui.label("Instances");
    ui.vertical(|ui| {
      if n1 > GRID_MAX || n2 > GRID_MAX {
        ui.label(RichText::new(format!("first {GRID_MAX} per direction")).color(theme::MUTED));
      }
      egui::Grid::new("suppress").spacing([2.0, 2.0]).show(ui, |ui| {
        for j in (0..n2.min(GRID_MAX)).rev() {
          for i in 0..n1.min(GRID_MAX) {
            let off = self.solid.hp.suppress.contains(&[i, j]);
            let mut on = !off;
            let original = (i, j) == (0, 0);
            let r = ui.add_enabled(!original, egui::Checkbox::without_text(&mut on)).on_hover_text(if original {
              "The original".to_string()
            } else {
              format!("Instance {}, {}", i + 1, j + 1)
            });
            if r.changed() {
              if on {
                self.solid.hp.suppress.retain(|x| *x != [i, j]);
              } else {
                self.solid.hp.suppress.push([i, j]);
              }
              *dirty = true;
            }
          }
          ui.end_row();
        }
      });
    });
    ui.end_row();
  }

  /// The pattern's handles along each direction from the objects' centre `origin`: an arrow for
  /// the Distance field and a dot for the Quantity. With Distance as the spacing the arrow sits
  /// at the second instance and the dot at the last; with Distance as the extent, the other way
  /// round. Handles `2k` and `2k + 1` are direction `k`'s.
  pub(super) fn pattern_handles(&self, s: &Session, origin: DVec3) -> Option<Vec<Handle>> {
    let (n1, n2) = self.pattern_counts();
    let mut out = Vec::new();
    for (k, n) in [n1, n2].into_iter().enumerate().take(if self.use_dir2 { 2 } else { 1 }) {
      let field = if k == 0 { &self.spacing } else { &self.spacing2 };
      let d = fox_engine::expr::eval(field, fox_engine::expr::Kind::Length, &s.model.params).ok()?;
      let steps = f64::from(n.max(2) - 1);
      let (step, total) = if self.pattern_extent { (d / steps, d) } else { (d, d * steps) };
      let dir = self.pattern_vec(k)?;
      let (at_dist, at_count) = if self.pattern_extent { (total, step) } else { (step, total) };
      out.push(Handle::arrow(origin, dir, at_dist, mm(self.unit, d)));
      out.push(Handle::arrow(origin, dir, at_count, format!("{n} instances")).dot());
    }
    Some(out)
  }

  /// Takes a dragged pattern handle (see [`Dialog::pattern_handles`]) into the Distance or
  /// Quantity field of its direction; false when the value can't apply.
  pub(super) fn set_pattern_handle(&mut self, s: &Session, i: usize, v: f64, step: f64) -> bool {
    let k = i / 2;
    let field = if k == 0 { &mut self.spacing } else { &mut self.spacing2 };
    if i.is_multiple_of(2) {
      // a distance can't reach zero: the smallest step is as far as it goes
      *field = mm(self.unit, crate::manipulators::snap(v, step).max(step.max(1e-3)));
      return true;
    }
    let Ok(d) = fox_engine::expr::eval(field, fox_engine::expr::Kind::Length, &s.model.params) else { return false };
    if v <= 0.0 || d <= 0.0 {
      return false;
    }
    // the count that puts the dot's instance nearest the pointer: the extent shared out in steps
    // of the dot's distance, or the dot's distance in steps of the spacing
    let steps = if self.pattern_extent { d / v } else { v / d };
    let count = ((steps.round() + 1.0).min(super::manip::MAX_COUNT) as u32).to_string();
    if k == 0 {
      self.count = count;
    } else {
      self.count2 = count;
    }
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  /// An 80 × 50 × 10 block from the origin and its top face.
  fn block(s: &mut Session) -> Value {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 50]}));
    run(s, "extrude", json!({"distance": 10}));
    let b = &s.model.bodies[0];
    json!({"body": b.id.0, "face": b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id})
  }

  fn top_edge(s: &Session, axis: usize) -> Value {
    let b = &s.model.bodies[0];
    let e = b
      .edges()
      .iter()
      .find(|e| {
        e.start.z > 9.9 && e.end.z > 9.9 && (e.end - e.start)[axis].abs() > 1.0 && e.start[1 - axis].abs() < 1e-6
      })
      .unwrap();
    json!({"body": b.id.0, "edge": e.id})
  }

  fn preview(s: &mut Session, d: &mut Dialog) {
    assert!(d.ready(), "{:?} not ready", d.kind);
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
  }

  #[test]
  fn single_hole_dialog_places_a_hole_from_two_edges() {
    let mut s = Session::default();
    let top = block(&mut s);
    let before = volume(&s);
    let mut d = Dialog::blank(Kind::Hole);
    s.selection = vec![Sel::Face {
      body: BodyId(top["body"].as_u64().unwrap() as u32),
      face: top["face"].as_u64().unwrap() as u32,
    }];
    d.hole_defaults(&s);
    d.choice = "simple";
    d.a = "6 mm".into();
    d.b = "4 mm".into();
    assert!(d.hole_single() && d.picks(), "a single hole by default, picked on the model");
    assert!(!d.ready());
    d.faces = vec![top.clone()];
    d.edges = vec![top_edge(&s, 0), top_edge(&s, 1)];
    d.solid.hp.offsets = ["12 mm".into(), "20 mm".into()];
    d.solid.hp.flat = true;
    let p = d.hole_params().unwrap();
    assert_eq!(p["face"], top);
    assert_eq!(p["offsets"][1]["distance"], "20 mm");
    assert_eq!(p["drill_point"], "flat");
    preview(&mut s, &mut d);
    let removed = before - volume(&s);
    assert!((removed - std::f64::consts::PI * 9.0 * 4.0).abs() < 1e-2, "{removed}");
    let wall = s.model.bodies[0]
      .faces()
      .iter()
      .find(|f| f.kind == fox_engine::kernel::SurfaceKind::Cylinder)
      .map(|f| (f.origin.x, f.origin.y))
      .unwrap();
    assert!((wall.0 - 20.0).abs() < 1e-6 && (wall.1 - 12.0).abs() < 1e-6, "{wall:?}");
    // Cancel takes the sketch and the hole back out in one step
    d.cancel(&mut s);
    assert!((volume(&s) - before).abs() < 1e-6);
  }

  #[test]
  fn hole_dialog_tap_type_and_points_mode() {
    let mut s = Session::default();
    let top = block(&mut s);
    run(&mut s, "sketch.create", json!({"plane": top}));
    run(&mut s, "sketch.point", json!({"at": [10, 10]}));
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Hole);
    d.sketch = s.design.features.last().map(|f| f.id);
    d.hole_defaults(&s);
    assert!(!d.solid.hp.single, "a sketch with points drills them");
    d.choice = "simple";
    d.a = "5 mm".into();
    d.flag = true;
    d.solid.hp.tap = "tapped";
    let p = d.hole_params().unwrap();
    assert_eq!((p["tap"].as_str(), p.get("tip_angle")), (Some("tapped"), None));
    preview(&mut s, &mut d);
    assert!(s.design.features.iter().any(|f| matches!(f.kind, fox_engine::model::FeatureKind::Thread { .. })));
  }

  #[test]
  fn pattern_dialog_edge_direction_components_and_suppress() {
    let mut s = Session::default();
    let _ = block(&mut s);
    let body = s.model.bodies[0].id;
    // along an edge: picking it doesn't add a body, and the arrow follows it
    let mut d = Dialog::blank(Kind::RectPattern);
    d.dir = "edge".into();
    d.count = "3".into();
    d.spacing = "60 mm".into();
    d.join = false;
    s.selection = vec![Sel::Body { body }];
    d.pattern_sync(&s);
    assert!(!d.pattern_ready());
    assert_eq!(d.pattern_missing(), "select a straight edge for the direction");
    let e = top_edge(&s, 1);
    s.selection.push(Sel::Edge { body, edge: e["edge"].as_u64().unwrap() as u32 });
    d.pattern_sync(&s);
    assert_eq!(d.bodies, vec![body.0]);
    assert!(d.pattern_ready());
    let v = d.pattern_vec(0).unwrap();
    assert!(v.y.abs() > 0.999, "{v}");
    // the distance arrow at the second instance, the quantity dot at the last
    let h = d.pattern_handles(&s, DVec3::ZERO).unwrap();
    assert_eq!(h.len(), 2);
    assert!((h[0].value - 60.0).abs() < 1e-9 && (h[1].value - 120.0).abs() < 1e-9, "{h:?}");
    assert_eq!(h[1].label, "3 instances");
    assert!(d.set_pattern_handle(&s, 0, 44.6, 1.0));
    assert_eq!(d.spacing, "45 mm");
    // the dot dragged to 4 spacings out: 5 instances; it never goes below the original
    assert!(d.set_pattern_handle(&s, 1, 170.0, 1.0));
    assert_eq!(d.count, "5");
    assert!(d.set_pattern_handle(&s, 1, 3.0, 1.0));
    assert_eq!(d.count, "1");
    assert!(!d.set_pattern_handle(&s, 1, -20.0, 1.0));
    // Distance as the extent: the arrow at the last instance sets it, the dot at the second
    // shares it out
    d.count = "3".into();
    d.spacing = "100 mm".into();
    d.pattern_extent = true;
    let h = d.pattern_handles(&s, DVec3::ZERO).unwrap();
    assert!((h[0].value - 100.0).abs() < 1e-9 && (h[1].value - 50.0).abs() < 1e-9, "{h:?}");
    assert!(d.set_pattern_handle(&s, 1, 26.0, 1.0));
    assert_eq!(d.count, "5");
    // a second direction adds its own pair
    d.pattern_extent = false;
    d.use_dir2 = true;
    d.dir2 = "z".into();
    d.count2 = "2".into();
    d.spacing2 = "15 mm".into();
    let h = d.pattern_handles(&s, DVec3::ZERO).unwrap();
    assert_eq!(h.len(), 4);
    assert!(matches!(h[2].shape, crate::handles::Shape::Arrow { dir, .. } if dir == DVec3::Z));
    assert!(d.set_pattern_handle(&s, 3, 46.0, 1.0));
    assert_eq!(d.count2, "4");
    d.use_dir2 = false;
    d.count = "3".into();
    d.spacing = "60 mm".into();
    preview(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 3);
    d.cancel(&mut s);

    // components, with an instance suppressed
    run(&mut s, "component.create", json!({"name": "Block", "bodies": [body.0]}));
    let mut d = Dialog::blank(Kind::RectPattern);
    d.solid.hp.kind = "components";
    d.count = "3".into();
    d.spacing = "100 mm".into();
    s.selection = vec![Sel::Body { body }];
    d.pattern_sync(&s);
    assert_eq!(d.solid.hp.comps.len(), 1);
    d.solid.hp.suppress = vec![[1, 0], [0, 0], [7, 0]];
    let p = d.pattern_params().unwrap();
    assert_eq!(p["suppress"], json!([[1, 0]]));
    assert!(p.get("bodies").is_none());
    preview(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 2, "the original and the third instance");
  }

  /// Draws a dialog for a frame (it previews when ready, as on screen).
  fn frame(ctx: &egui::Context, app: &mut App, d: &mut Dialog) {
    let raw =
      egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| {
      d.show(app, ui.ctx());
    });
    out.textures_delta.clear();
  }

  #[test]
  fn hole_and_pattern_dialogs_draw_and_preview() {
    let mut app = App::with_session(Session::default());
    let top = block(&mut app.s);
    let body = BodyId(top["body"].as_u64().unwrap() as u32);
    let face = top["face"].as_u64().unwrap() as u32;
    let before = volume(&app.s);
    app.s.selection = vec![Sel::Face { body, face }];
    let ctx = egui::Context::default();
    let mut d = Dialog::open(&mut app, Kind::Hole);
    assert!(d.solid.hp.single);
    for (choice, tap) in [("simple", "simple"), ("counterbore", "tapped"), ("countersink", "clearance")] {
      d.choice = choice;
      d.solid.hp.tap = tap;
      d.dirty = true;
      frame(&ctx, &mut app, &mut d);
      frame(&ctx, &mut app, &mut d);
    }
    assert_eq!(d.faces, vec![top]);
    assert!(d.applied && d.error.is_none(), "{:?} {:?}", d.error, d.command());
    assert!(volume(&app.s) < before);
    d.cancel(&mut app.s);
    assert!((volume(&app.s) - before).abs() < 1e-6);

    app.s.selection = vec![Sel::Body { body }];
    let mut d = Dialog::open(&mut app, Kind::RectPattern);
    d.use_dir2 = true;
    for kind in ["bodies", "components"] {
      d.solid.hp.kind = kind;
      frame(&ctx, &mut app, &mut d);
    }
    d.solid.hp.kind = "bodies";
    d.dirty = true;
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?} {:?}", d.error, d.command());
    assert_eq!(d.handles(&app.s).len(), 4, "a distance arrow and a quantity dot per direction");
    assert_eq!(app.s.model.bodies.len(), 6);
  }

  #[test]
  fn a_single_holes_sketch_stays_out_of_the_browser() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 10}));
    let body = s.model.bodies[0].id.0;
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let r =
      run(&mut s, "hole", json!({"face": {"body": body, "face": top}, "at": [20, 20, 10], "diameter": 4, "depth": 5}));
    let placed = FeatureId(r["sketch"].as_u64().unwrap() as u32);
    assert_eq!(placement_sketches(&s), vec![placed]);
    // holes at a sketch's own points keep their sketch in the browser
    run(&mut s, "sketch.create", json!({"plane": {"body": body, "face": top}}));
    run(&mut s, "sketch.point", json!({"at": [40, 20]}));
    let own = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "hole", json!({"sketch": own.0, "diameter": 4, "depth": 5}));
    assert_eq!(placement_sketches(&s), vec![placed]);
    assert!(!State::default().shape_closed, "Shape Settings starts open");
  }
}
