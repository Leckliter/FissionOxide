//! Fusion's one Flange dialog: a Type row of three icon toggles (Flange, Hem, Lofted flange) that
//! switches between the commands, and a Flange whose title and rows follow what is picked: closed
//! sketch profiles make a BASE FLANGE (Orientation, Sheet Metal Rule), sheet edges an EDGE FLANGE
//! (Extent, Height with an arrow on the edge, Height Datum, Bend Position, Flip, Bend Radius and
//! the rule's override groups) and open sketch curves a CONTOUR FLANGE. Nothing is picked for
//! the user. Edit Feature opens the same rows, titled "EDIT EDGE FLANGE" and so on.

use egui::RichText;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BendPosition, BodyId, FeatureId, FeatureKind};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{Choice, choice, icon_row};
use super::{Dialog, Kind, length_set, sel_chip_lit};
use crate::icons::{self, Icon};
use crate::{App, theme};

/// The Type row: Fusion's Flange dialog and its three commands.
const TYPES: [Choice; 3] = [
  choice("flange", Icon::Flange, "Flange"),
  choice("hem", Icon::Hem, "Hem"),
  choice("lofted", Icon::LoftedFlange, "Lofted flange"),
];

/// The same row as the Lofted Flange form's Type input.
pub(crate) const TYPE_ICONS: &[(&str, Icon, &str)] =
  &[("flange", Icon::Flange, "Flange"), ("hem", Icon::Hem, "Hem"), ("lofted", Icon::LoftedFlange, "Lofted flange")];

/// Base Flange's Orientation: the sheet on the sketch normal's side, the other side, or centred.
const ORIENTATIONS: [(&str, Icon, &str); 3] =
  [("side1", Icon::WallSide1, "Side 1"), ("side2", Icon::WallSide2, "Side 2"), ("center", Icon::WallCenter, "Center")];

/// Edge Flange's Bend Position (Fusion's default is Inside).
const POSITIONS: [(&str, &str); 3] = [("inside", "Inside"), ("outside", "Outside"), ("adjacent", "Adjacent")];

/// The cursor prompt while nothing is picked.
pub(super) const PICK_PROMPT: &str = "Select sheet metal edges or sketch geometry to use for flange";
/// The Profiles chip's tooltip and the prompt once a profile is picked.
const PROFILES_HINT: &str = "Select more closed sketch profiles, adjust settings, or hold Ctrl to modify selection";

/// The dialog kind behind a Type row key, and back.
fn type_kind(key: &str) -> Kind {
  match key {
    "hem" => Kind::Hem,
    "lofted" => Kind::Form(&crate::forms::LOFTED_FLANGE),
    _ => Kind::Flange,
  }
}

fn type_key(kind: Kind) -> Option<&'static str> {
  match kind {
    Kind::Flange => Some("flange"),
    Kind::Hem => Some("hem"),
    Kind::Form(f) if std::ptr::eq(f, &crate::forms::LOFTED_FLANGE) => Some("lofted"),
    _ => None,
  }
}

/// The Flange dialog's own state.
pub(super) struct State {
  /// Base Flange's Orientation (see [`ORIENTATIONS`]).
  pub(super) orientation: &'static str,
  /// The Bend Override, 2-Bend and 3-Bend Corner Override groups shown open.
  open: [bool; 3],
}

impl Default for State {
  fn default() -> State {
    State { orientation: "side1", open: [true; 3] }
  }
}

impl Dialog {
  /// Fusion's Type row: three icon toggles switching between Flange, Hem and Lofted Flange (not
  /// while editing a feature).
  pub(super) fn flange_type_row(&mut self, ui: &mut egui::Ui) {
    if self.editing.is_some() {
      return;
    }
    let Some(mut cur) = type_key(self.kind) else { return };
    ui.label("Type");
    if icon_row(ui, &mut cur, &TYPES, "") {
      self.switch = Some(type_kind(cur));
    }
    ui.end_row();
  }

  /// The Lofted Flange form's Type input changed: the dialog becomes the command it names.
  pub(super) fn flange_type_switch(&mut self, f: &'static crate::forms::Form) {
    if !std::ptr::eq(f, &crate::forms::LOFTED_FLANGE) {
      return;
    }
    let i = f.inputs.iter().position(|x| x.key() == "flange_type");
    let cur = i.and_then(|i| self.form.values.get(i)).and_then(Value::as_str).unwrap_or("lofted");
    if cur != "lofted" {
      self.switch = Some(type_kind(cur));
    }
  }

  /// Is this one of the Type row's dialogs (the switch keeps the picked edges)?
  pub(super) fn flange_family(&self) -> bool {
    matches!(self.kind, Kind::Flange | Kind::Hem)
  }

  /// A new Flange dialog: what is picked already decides its kind; nothing is picked for the
  /// user, as in Fusion. Height starts at 0.00 (OK waits for it), Bend Position at Inside.
  pub(super) fn flange_defaults(&mut self, s: &Session) {
    let (sketch, curves) = picked_curves(s);
    if self.profiles.is_empty() && !curves.is_empty() {
      self.sketch = sketch;
      self.curves = curves;
    }
    self.choice = self.flange_choice();
    self.distance = "0 mm".into();
    self.distance2 = "10 mm".into();
    self.angle = "90 deg".into();
    self.position = "inside";
    self.op = "new_body";
  }

  /// What the picks make: sheet edges an edge flange, profiles a base flange, open sketch curves
  /// a contour flange; nothing picked, the bare FLANGE.
  fn flange_choice(&self) -> &'static str {
    if !self.edges.is_empty() {
      "edge"
    } else if !self.profiles.is_empty() {
      "base"
    } else if !self.curves.is_empty() {
      "contour"
    } else {
      ""
    }
  }

  /// Picks up the viewport's picks (see [`Dialog::flange_choice`]).
  pub(super) fn flange_sync(&mut self, s: &Session) {
    // an edge flange's preview changes the very edges picked
    if !self.applied {
      let e = super::sel_edges(s);
      if e != self.edges {
        self.edges = e;
      }
    }
    let mut pts = Vec::new();
    let mut sketch = None;
    for sel in &s.selection {
      if let Sel::Profile { sketch: sk, index } = sel
        && let Some(p) = s.model.sketches.get(sk).and_then(|r| r.profiles.get(*index))
      {
        sketch = Some(*sk);
        pts.push(p.interior_point());
      }
    }
    if !pts.is_empty() && pts != self.profiles {
      self.sketch = sketch;
      self.profiles = pts;
      self.dirty = true;
    }
    let (sk, curves) = picked_curves(s);
    if !curves.is_empty() && curves != self.curves {
      self.curves = curves;
      if self.profiles.is_empty() {
        self.sketch = sk;
      }
      self.dirty = true;
    }
    let choice = self.flange_choice();
    if choice != self.choice {
      self.choice = choice;
      self.dirty = !self.picks();
    }
  }

  /// The chip's or the list's ✕: every pick goes, and the dialog is the bare FLANGE again.
  fn clear_flange_picks(&mut self, s: &mut Session) {
    self.withdraw(s);
    self.edges.clear();
    self.profiles.clear();
    self.curves.clear();
    self.choice = "";
    s.selection.retain(|x| !matches!(x, Sel::Edge { .. } | Sel::Profile { .. } | Sel::SketchCurve { .. }));
    self.dirty = true;
  }

  /// Enough input to try (a new dialog; an edit's stored picks stand).
  pub(super) fn flange_ready(&self) -> bool {
    match self.choice {
      "edge" => !self.edges.is_empty(),
      "base" => self.sketch.is_some() && !self.profiles.is_empty(),
      "contour" => self.sketch.is_some() && !self.curves.is_empty(),
      _ => false,
    }
  }

  /// A new edge flange's Height still at 0.00: OK waits, as in Fusion.
  pub(super) fn flange_waits(&self) -> bool {
    self.kind == Kind::Flange && self.choice == "edge" && self.editing.is_none() && !length_set(&self.distance)
  }

  /// The command for the picks.
  pub(super) fn flange_command(&self) -> Option<(&'static str, Value)> {
    Some(match self.choice {
      "edge" => (
        "sheet.edge_flange",
        json!({
          "edges": self.edges, "height": self.distance, "angle": self.angle, "flip": self.flip,
          "position": self.position, "corners": self.corners,
        }),
      ),
      "contour" => {
        let mut p = json!({"sketch": self.sketch?.0, "curves": self.curves, "distance": self.distance2,
          "symmetric": self.flag, "flip": self.flip});
        self.put_rule(&mut p);
        ("sheet.contour_flange", p)
      }
      "base" => {
        let o = self.flange.orientation;
        let mut p = json!({"sketch": self.sketch?.0, "profiles": self.profiles, "flip": o == "side2",
          "center": o == "center", "operation": "new_body"});
        self.put_rule(&mut p);
        ("sheet.flange", p)
      }
      _ => return None,
    })
  }

  /// Fields for `feature.edit`.
  pub(super) fn flange_edit_set(&self) -> Option<Value> {
    Some(match (self.kind, self.choice) {
      (Kind::Flange, "base") => {
        let o = self.flange.orientation;
        json!({"rule": self.rule, "flip": o == "side2", "center": o == "center"})
      }
      (Kind::Flange, "contour") => {
        json!({"distance": self.distance2, "symmetric": self.flag, "flip": self.flip, "rule": self.rule})
      }
      (Kind::Flange, "edge") => {
        json!({"height": self.distance, "angle": self.angle, "flip": self.flip, "position": self.position})
      }
      (Kind::Hem, _) => {
        let gap = if self.choice == "open" { self.distance2.as_str() } else { super::sheet::FLAT_HEM_GAP };
        json!({"length": self.distance, "gap": gap, "flip": self.flip})
      }
      _ => return None,
    })
  }

  /// The title for the picks: BASE, EDGE or CONTOUR FLANGE, the bare FLANGE before a pick.
  pub(super) fn flange_title(&self) -> Option<&'static str> {
    Some(match (self.kind, self.choice) {
      (Kind::Flange, "base") => "BASE FLANGE",
      (Kind::Flange, "edge") => "EDGE FLANGE",
      (Kind::Flange, "contour") => "CONTOUR FLANGE",
      (Kind::Flange, _) => "FLANGE",
      (Kind::Hem, _) => "HEM",
      _ => return None,
    })
  }

  /// Fusion's cursor prompt once the flange has its picks (whatever the pointer is over).
  pub(super) fn flange_prompt(&self) -> Option<&'static str> {
    if self.kind != Kind::Flange || self.editing.is_some() {
      return None;
    }
    match self.choice {
      "edge" => Some("Select more sheet metal edges or specify height to extrude"),
      "base" => Some(PROFILES_HINT),
      _ => None,
    }
  }

  /// The previewed base flange's new sheet, drawn in Fusion's preview blue.
  pub fn sheet_preview_bodies(&self, s: &Session) -> Vec<BodyId> {
    if self.kind != Kind::Flange || self.choice != "base" {
      return Vec::new();
    }
    self.preview_bodies(s)
  }

  // ---- rows ----

  /// The Flange dialog's rows for its picks.
  pub(super) fn flange_rows(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    self.flange_type_row(ui);
    let creating = !self.picks_fixed();
    if creating && self.pick_list(ui) {
      self.clear_flange_picks(&mut app.s);
    }
    let n = match self.choice {
      "edge" => self.edges.len(),
      "base" => self.profiles.len().max(usize::from(!creating)),
      "contour" => self.curves.len().max(usize::from(!creating)),
      _ => 0,
    };
    ui.label(if self.choice == "base" { "Profiles" } else { "Edges/Profiles" });
    let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
    let (chip, x) = chip_x(ui, self.accent(), &text, creating && n > 0);
    if self.choice == "base" {
      chip.on_hover_text(PROFILES_HINT);
    }
    ui.end_row();
    if x {
      self.clear_flange_picks(&mut app.s);
      return;
    }
    match self.choice {
      "base" => {
        Self::icon_choose(ui, "Orientation", "fl-orient", &mut self.flange.orientation, &ORIENTATIONS, dirty);
        self.sheet_rule_row(app, ui);
      }
      "edge" => self.edge_flange_rows(app, ui, dirty),
      "contour" => {
        ui.label("Sketch");
        ui.label(self.sketch.and_then(|s| app.s.design.feature(s)).map(|f| f.name.clone()).unwrap_or_default());
        ui.end_row();
        Self::field(ui, "Distance", &mut self.distance2, dirty);
        Self::check(ui, "Symmetric", &mut self.flag, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
        self.sheet_rule_row(app, ui);
      }
      _ => {}
    }
  }

  /// The list of picks under the Type row ("Profile 1", "Edge 1 | Full Edge"), with + and ✕
  /// below it. True when ✕ was clicked.
  fn pick_list(&mut self, ui: &mut egui::Ui) -> bool {
    let rows: Vec<String> = match self.choice {
      "edge" => (1..=self.edges.len()).map(|i| format!("Edge {i}")).collect(),
      "base" => (1..=self.profiles.len()).map(|i| format!("Profile {i}")).collect(),
      "contour" => (1..=self.curves.len()).map(|i| format!("Curve {i}")).collect(),
      _ => Vec::new(),
    };
    let mut clear = false;
    ui.label("");
    ui.vertical(|ui| {
      egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
          ui.set_min_size(egui::vec2(172.0, 48.0));
          for (i, r) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
              ui.label(r);
              if self.choice == "edge" {
                ui.label(RichText::new("|").color(theme::MUTED));
                egui::ComboBox::from_id_salt(("fl-extent", i)).selected_text("Full Edge").width(90.0).show_ui(
                  ui,
                  |ui| {
                    let _ = ui.selectable_label(true, "Full Edge");
                  },
                );
              }
            });
          }
        });
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let _ = ui.small_button("+").on_hover_text("Add");
        if ui.small_button("✕").on_hover_text("Remove").clicked() {
          clear = true;
        }
      });
    });
    ui.end_row();
    clear && !rows.is_empty()
  }

  /// Edge Flange: Extent, Height (and Angle while editing), Height Datum, Bend Position, Flip,
  /// the rule's Bend Radius and the override groups (greyed: the rule's values are used).
  fn edge_flange_rows(&mut self, app: &App, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.label("Extent");
    let mut extent = "distance";
    super::choice::combo(ui, "fl-extent-type", &[("distance", "Distance")], &mut extent, super::choice::extrude_icon);
    ui.end_row();
    Self::field(ui, "Height", &mut self.distance, dirty);
    if self.editing.is_some() {
      Self::field(ui, "Angle", &mut self.angle, dirty);
    }
    let mut datum = "outer";
    Self::choose(ui, "Height Datum", "fl-datum", &mut datum, &[("outer", "Outer Faces")], dirty);
    Self::choose(ui, "Bend Position", "fl-pos", &mut self.position, &POSITIONS, dirty);
    ui.label("Flip");
    if icons::icon_button(ui, Icon::Flip, 22.0, "Flip", self.flip, None).clicked() {
      self.flip = !self.flip;
      *dirty = true;
    }
    ui.end_row();
    let rule = app.s.design.sheet_rule(&self.rule).unwrap_or_default();
    greyed_row(ui, "Bend Radius", &rule.bend_radius);
    let relief = match rule.relief {
      fox_engine::model::ReliefShape::Rectangular => "Default (Rectangular)",
      fox_engine::model::ReliefShape::Round => "Default (Round)",
      fox_engine::model::ReliefShape::None => "Default (None)",
    };
    let corner = match rule.corner_relief {
      fox_engine::model::CornerRelief::None => "Default (None)",
      fox_engine::model::CornerRelief::Round => "Default (Round)",
      fox_engine::model::CornerRelief::Square => "Default (Square)",
      fox_engine::model::CornerRelief::Tear => "Default (Tear)",
    };
    let groups: [(&str, Vec<(&str, &str)>); 3] = [
      (
        "Bend Override",
        vec![
          ("Relief shape", relief),
          ("Relief width", "Thickness"),
          ("Relief depth", "Thickness * 0.5"),
          ("Relief remnant", "Thickness * 2.0"),
        ],
      ),
      (
        "2-Bend Corner Override",
        vec![("Relief shape", corner), ("Relief size", "Thickness * 4.0"), ("Relief placement", "Intersection")],
      ),
      ("3-Bend Corner Override", vec![("Relief shape", "Default (Round with Radius)"), ("Relief radius", "Thickness")]),
    ];
    for (i, (name, rows)) in groups.iter().enumerate() {
      let arrow = if self.flange.open[i] { "▾" } else { "▸" };
      if ui
        .add(egui::Label::new(RichText::new(format!("{arrow} {name}")).strong()).sense(egui::Sense::click()))
        .clicked()
      {
        self.flange.open[i] = !self.flange.open[i];
      }
      ui.label("");
      ui.end_row();
      if self.flange.open[i] {
        for (label, value) in rows {
          greyed_row(ui, label, value);
        }
      }
    }
  }

  /// The Sheet Metal Rule drop-down: the design's rules, the one used shown (the default when
  /// none is named).
  fn sheet_rule_row(&mut self, app: &App, ui: &mut egui::Ui) {
    ui.label("Sheet Metal Rule");
    let default = app.s.design.sheet_rule("").map(|r| r.name).unwrap_or_default();
    let shown = if self.rule.is_empty() { default.clone() } else { self.rule.clone() };
    egui::ComboBox::from_id_salt("fl-rule").selected_text(shown.clone()).width(140.0).show_ui(ui, |ui| {
      let mut names = self.rules.clone();
      if names.is_empty() {
        names.push(default.clone());
      }
      for r in names {
        if ui.selectable_label(shown == r, &r).clicked() && shown != r {
          self.rule = r;
          self.dirty = true;
        }
      }
    });
    ui.end_row();
  }
}

/// A greyed row: a value the dialog shows but can't change yet (the rule's).
fn greyed_row(ui: &mut egui::Ui, label: &str, value: &str) {
  ui.label(RichText::new(label).color(theme::MUTED));
  ui.add_enabled_ui(false, |ui| {
    ui.horizontal(|ui| {
      let mut v = value.to_string();
      ui.add(egui::TextEdit::singleline(&mut v).desired_width(118.0));
      ui.label(RichText::new("fx").italics().color(theme::MUTED));
    })
  })
  .response
  .on_disabled_hover_text("From the sheet metal rule");
  ui.end_row();
}

/// A selection chip whose ✕ clears it: the chip's response and whether the ✕ was clicked.
fn chip_x(ui: &mut egui::Ui, accent: egui::Color32, text: &str, has: bool) -> (egui::Response, bool) {
  let r = sel_chip_lit(ui, accent, text, true, has);
  if !has {
    return (r, false);
  }
  // the ✕ is drawn 10 pt from the chip area's right end
  let at = egui::Rect::from_center_size(egui::pos2(r.rect.right() - 10.0, r.rect.center().y), egui::Vec2::splat(16.0));
  let x = ui.interact(at, r.id.with("clear"), egui::Sense::click());
  let clicked = x.on_hover_text("Clear the selection").clicked();
  (r, clicked)
}

/// Open sketch curves picked in the viewport outside sketch editing: the first sketch's.
fn picked_curves(s: &Session) -> (Option<FeatureId>, Vec<u32>) {
  let mut sketch = None;
  let mut out = Vec::new();
  for sel in &s.selection {
    if let Sel::SketchCurve { sketch: sk, curve } = sel
      && s.edit_sketch != Some(*sk)
      && sketch.is_none_or(|x| x == *sk)
    {
      sketch = Some(*sk);
      out.push(curve.0);
    }
  }
  (sketch, out)
}

/// Where an edge flange's arrow sits and points: the picked edge's midpoint, along the normal of
/// the sheet face the edge borders (the way the flange bends with Flip off).
pub(super) fn flange_anchor(s: &Session, key: &Value) -> Option<(DVec3, DVec3)> {
  let get = |k: &str| key.get(k).and_then(Value::as_u64).map(|v| v as u32);
  let body = s.model.body(BodyId(get("body")?))?;
  let t = body.sheet.as_ref()?.thickness;
  let e = body.edges().iter().find(|e| Some(e.id) == get("edge"))?;
  let (panel, _) = fox_engine::model::sheet::edge_faces(body.faces(), e.faces, e.length, t)?;
  Some((e.mid, panel.normal))
}

/// An edited edge flange's arrow: the stored edge's midpoint, along whichever of the faces that
/// met there is still a face of the sheet through that point (the largest; the side wall is
/// gone under the bend).
pub(super) fn flange_edit_anchor(s: &Session, kind: &FeatureKind) -> Option<(DVec3, DVec3)> {
  let FeatureKind::EdgeFlange { edges, .. } = kind else { return None };
  let e = edges.first()?;
  let at = DVec3::from(e.point);
  let area = |n: DVec3| -> f64 {
    let Some(b) = s.model.body(e.body) else { return 0.0 };
    b.faces()
      .iter()
      .filter(|f| f.kind == fox_engine::kernel::SurfaceKind::Plane)
      .filter(|f| f.normal.dot(n) > 1.0 - 1e-6 && (f.origin - at).dot(n).abs() < 1e-6)
      .map(|f| f.area)
      .fold(0.0, f64::max)
  };
  let (n1, n2) = (DVec3::from(e.n1), DVec3::from(e.n2));
  let n = if area(n2) > area(n1) { n2 } else { n1 };
  Some((at, n.try_normalize()?))
}

/// The dialog for an existing flange or hem (timeline double-click), its rows as created.
pub(super) fn edit_dialog(app: &App, kind: &FeatureKind) -> Option<Dialog> {
  let mut d = match kind {
    FeatureKind::Flange { sketch, profiles, rule, flip, center, .. } => {
      let mut d = Dialog::blank(Kind::Flange);
      d.choice = "base";
      d.sketch = Some(*sketch);
      d.profiles = profiles.clone();
      d.rule = rule.clone();
      d.flange.orientation = if *center {
        "center"
      } else if *flip {
        "side2"
      } else {
        "side1"
      };
      d
    }
    FeatureKind::ContourFlange { sketch, curves, distance, symmetric, flip, rule, .. } => {
      let mut d = Dialog::blank(Kind::Flange);
      d.choice = "contour";
      d.sketch = Some(*sketch);
      d.curves = curves.iter().map(|c| c.0).collect();
      d.distance2 = distance.clone();
      d.flag = *symmetric;
      d.flip = *flip;
      d.rule = rule.clone();
      d
    }
    FeatureKind::EdgeFlange { edges, height, angle, flip, position, corners, .. } => {
      let mut d = Dialog::blank(Kind::Flange);
      d.choice = "edge";
      d.edges = vec![Value::Null; edges.len()];
      d.distance = height.clone();
      d.angle = tenths(angle);
      d.flip = *flip;
      d.position = match position {
        BendPosition::Adjacent => "adjacent",
        BendPosition::Inside => "inside",
        BendPosition::Outside => "outside",
      };
      d.corners = serde_json::to_value(corners)
        .ok()
        .and_then(|v| super::CORNERS.iter().find(|(k, _)| v == *k).map(|(k, _)| *k))
        .unwrap_or("miter");
      d
    }
    FeatureKind::Hem { edges, length, gap, flip } => {
      let mut d = Dialog::blank(Kind::Hem);
      d.edges = vec![Value::Null; edges.len()];
      d.distance = length.clone();
      // a flat hem keeps the kernel's least gap; any other gap is an open hem's
      d.choice = if gap.trim() == super::sheet::FLAT_HEM_GAP { "flat" } else { "open" };
      d.distance2 = if d.choice == "open" { gap.clone() } else { super::sheet::HEM_GAP.into() };
      d.position = "adjacent";
      d.flip = *flip;
      d
    }
    _ => return None,
  };
  d.rules = app.s.design.sheet_metal_rules.iter().map(|r| r.name.clone()).collect();
  Some(d)
}

/// An angle as Edit Feature shows it, to a tenth of a degree ("90 deg" → "90.0 deg"); an
/// expression stays as typed.
fn tenths(angle: &str) -> String {
  match angle.trim().strip_suffix("deg").map(str::trim).and_then(|v| v.parse::<f64>().ok()) {
    Some(v) => format!("{v:.1} deg"),
    None => angle.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, Rect, vec2};

  use super::*;

  /// Draws the dialog for two frames (it previews when ready; a new window draws nothing on its
  /// first); the text drawn on the second.
  fn frame(ctx: &egui::Context, app: &mut App, d: &mut Dialog) -> Vec<String> {
    frame1(ctx, app, d);
    frame1(ctx, app, d)
  }

  fn frame1(ctx: &egui::Context, app: &mut App, d: &mut Dialog) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let raw =
      egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))), ..Default::default() };
    let out = ctx.run_ui(raw, |ui| {
      d.show(app, ui.ctx());
    });
    let mut texts = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut texts);
    }
    out.drop_without_applying_deltas();
    texts
  }

  fn has(texts: &[String], label: &str) -> bool {
    texts.iter().any(|t| t.trim() == label)
  }

  /// The batch's setup: a 100 x 60 mm rectangle sketched on XY and finished.
  fn plate(app: &mut App) -> FeatureId {
    app.run("sketch.create", json!({"plane": "xy"}));
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 60]}));
    let sk = app.s.edit_sketch.unwrap();
    app.run("sketch.finish", json!({}));
    app.last_sketch = Some(sk);
    sk
  }

  /// OK the showing dialog the way its button does.
  fn ok(app: &mut App, d: &mut Dialog, ctx: &egui::Context) {
    d.ok_requested = true;
    let keep = d.show(app, ctx);
    assert!(!keep, "OK closes the dialog: {:?}", d.error);
  }

  /// `ui:sheet:flange-dialog-open`: FLANGE opens with the Type icons and an empty pick, OK
  /// greyed and Fusion's prompt, even with one profile to pick; HEM is the same dialog.
  #[test]
  fn flange_opens_with_type_icons_and_nothing_picked() {
    let mut app = App::with_session(Session::default());
    plate(&mut app);
    let ctx = egui::Context::default();
    let mut d = Dialog::open(&mut app, Kind::Flange);
    let t = frame(&ctx, &mut app, &mut d);
    assert!(d.profiles.is_empty() && d.edges.is_empty(), "nothing picked for the user");
    assert_eq!(d.heading(), "FLANGE");
    assert!(has(&t, "Type") && has(&t, "Edges/Profiles") && has(&t, "Select"), "{t:?}");
    assert!(!has(&t, "Operation") && !has(&t, "Rule") && !has(&t, "Flip"), "{t:?}");
    assert!(!d.ok_enabled() && !d.applied);
    assert_eq!(d.prompt(false).as_deref(), Some(PICK_PROMPT));
    assert_eq!(d.profile_sketches().len(), 1, "the sketch's profiles show to be picked");
    // the Type row's Hem: the HEM dialog, with its prompt
    d.switch = Some(type_kind("hem"));
    frame1(&ctx, &mut app, &mut d);
    assert_eq!((d.kind, d.heading().as_str()), (Kind::Hem, "HEM"));
    assert_eq!(d.prompt(false).as_deref(), Some("Select an edge to hem"));
    let t = frame(&ctx, &mut app, &mut d);
    assert!(has(&t, "Type"), "{t:?}");
    // and Lofted flange: the Lofted Flange form, whose Type row leads back
    d.switch = Some(type_kind("lofted"));
    frame1(&ctx, &mut app, &mut d);
    let Kind::Form(f) = d.kind else { panic!("{:?}", d.kind) };
    assert!(std::ptr::eq(f, &crate::forms::LOFTED_FLANGE));
    let i = f.inputs.iter().position(|x| x.key() == "flange_type").unwrap();
    assert_eq!(d.form.values[i], json!("lofted"));
    d.form.values[i] = json!("flange");
    d.flange_type_switch(f);
    assert_eq!(d.switch, Some(Kind::Flange));
    d.cancel(&mut app.s);
  }

  /// `ui:sheet:base-flange-rows`: a picked profile makes BASE FLANGE with Profile 1, Profiles,
  /// Orientation and Sheet Metal Rule (no Operation or Flip), a blue preview, and BaseFlange1.
  #[test]
  fn a_picked_profile_makes_a_base_flange() {
    let mut app = App::with_session(Session::default());
    let sk = plate(&mut app);
    let ctx = egui::Context::default();
    let mut d = Dialog::open(&mut app, Kind::Flange);
    frame(&ctx, &mut app, &mut d);
    app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
    frame(&ctx, &mut app, &mut d);
    let t = frame(&ctx, &mut app, &mut d);
    assert_eq!(d.heading(), "BASE FLANGE");
    for label in ["Profile 1", "Profiles", "1 selected", "Orientation", "Side 1", "Sheet Metal Rule"] {
      assert!(has(&t, label), "{label} in {t:?}");
    }
    assert!(!has(&t, "Operation") && !has(&t, "Flip") && !has(&t, "Edges/Profiles"), "{t:?}");
    assert!(d.applied && d.ok_enabled(), "{:?}", d.error);
    assert_eq!(d.prompt(false).as_deref(), Some(PROFILES_HINT));
    assert_eq!(d.sheet_preview_bodies(&app.s).len(), 1, "the previewed sheet draws blue");
    // Orientation Center: the sheet straddles the sketch
    d.flange.orientation = "center";
    d.dirty = true;
    frame(&ctx, &mut app, &mut d);
    let (lo, hi) = app.s.model.bounds().unwrap();
    assert!((lo.z + hi.z).abs() < 1e-6 && hi.z > 0.0, "{lo:?} {hi:?}");
    d.flange.orientation = "side1";
    d.dirty = true;
    frame(&ctx, &mut app, &mut d);
    ok(&mut app, &mut d, &ctx);
    let f = app.s.design.features.last().unwrap();
    assert_eq!(f.name, "BaseFlange1");
    assert!(matches!(f.kind, FeatureKind::Flange { center: false, flip: false, .. }));
    // the ✕ takes the pick back: the bare FLANGE again
    let mut d = Dialog::open(&mut app, Kind::Flange);
    app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
    frame(&ctx, &mut app, &mut d);
    assert_eq!(d.choice, "base");
    d.clear_flange_picks(&mut app.s);
    frame(&ctx, &mut app, &mut d);
    assert!(d.choice.is_empty() && d.profiles.is_empty() && app.s.selection.is_empty() && !d.applied);
    assert_eq!(d.heading(), "FLANGE");
    d.cancel(&mut app.s);
  }

  /// `ui:sheet:edge-flange-rows` and `ui:sheet:edit-feature`: a picked sheet edge makes EDGE
  /// FLANGE with Fusion's rows, Height at 0.00 (OK greyed) on an arrow up from the edge; 20 folds
  /// the flange up; the timeline counts it with the base flange (EdgeFlange2) and Edit opens
  /// EDIT EDGE FLANGE with Angle 90.0 deg and the arrow.
  #[test]
  fn a_picked_sheet_edge_makes_an_edge_flange() {
    let mut app = App::with_session(Session::default());
    plate(&mut app);
    app.run("sheet.flange", json!({}));
    let ctx = egui::Context::default();
    let b = &app.s.model.bodies[0];
    let t = b.sheet.as_ref().unwrap().thickness;
    // the front edge on top of the sheet
    let e = b.edges().iter().find(|e| e.length > 99.0 && e.mid.y.abs() < 1e-6 && e.mid.z > t / 2.0).unwrap();
    let mid = e.mid;
    app.s.selection = vec![Sel::Edge { body: b.id, edge: e.id }];
    let mut d = Dialog::open(&mut app, Kind::Flange);
    let texts = frame(&ctx, &mut app, &mut d);
    assert_eq!(d.heading(), "EDGE FLANGE");
    for label in [
      "Edge 1",
      "Full Edge",
      "1 selected",
      "Extent",
      "Distance",
      "Height",
      "Height Datum",
      "Outer Faces",
      "Bend Position",
    ] {
      assert!(has(&texts, label), "{label} in {texts:?}");
    }
    for label in ["Inside", "Bend Radius", "Bend Override", "2-Bend Corner Override", "3-Bend Corner Override"] {
      assert!(texts.iter().any(|x| x.contains(label)), "{label} in {texts:?}");
    }
    assert!(!has(&texts, "Angle") && !has(&texts, "Corners") && !has(&texts, "Bend angle"), "{texts:?}");
    assert!(!d.ok_enabled() && !d.applied, "Height 0.00: OK waits");
    assert_eq!(d.prompt(false).as_deref(), Some("Select more sheet metal edges or specify height to extrude"));
    d.refresh_manip(&app.s);
    let m = d.manipulator(&app.s).expect("an arrow on the edge");
    assert!((m.origin - mid).length() < 1e-6 && m.dir.z > 0.99 && m.value.abs() < 1e-9, "{m:?}");
    assert!(d.value_box_field().is_some(), "the arrow's value box edits Height");
    // dragged to 20: the flange folds up, away from the sketch
    d.set_manipulator(20.0, 0.1);
    assert_eq!(d.distance, "20 mm");
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.ok_enabled(), "{:?}", d.error);
    let (lo, hi) = app.s.model.bounds().unwrap();
    assert!((hi.z - 20.0).abs() < 1e-6 && lo.z.abs() < 1e-6, "{lo:?} {hi:?}");
    ok(&mut app, &mut d, &ctx);
    let f = app.s.design.features.last().unwrap();
    assert_eq!(f.name, "EdgeFlange2", "one counter for every flange");
    let id = f.id;
    // Edit: EDIT EDGE FLANGE, no Type row, Angle 90.0 deg, the arrow on the edge
    let mut d = Dialog::edit(&app, id).expect("an edge flange is editable");
    d.begin_edit(&mut app.s);
    let texts = frame(&ctx, &mut app, &mut d);
    assert_eq!(d.heading(), "EDIT EDGE FLANGE");
    assert!(has(&texts, "Angle") && has(&texts, "Height") && !has(&texts, "Type"), "{texts:?}");
    assert_eq!(d.angle, "90.0 deg");
    let m = d.manipulator(&app.s).expect("the edited flange's arrow");
    assert!((m.origin - mid).length() < 1e-6 && m.dir.z > 0.99 && (m.value - 20.0).abs() < 1e-9, "{m:?}");
    d.distance = "25 mm".into();
    d.dirty = true;
    frame(&ctx, &mut app, &mut d);
    ok(&mut app, &mut d, &ctx);
    let FeatureKind::EdgeFlange { height, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
    assert_eq!(height, "25 mm");
    // the base flange and a hem edit too
    let base = app.s.design.features.iter().find(|f| f.name == "BaseFlange1").unwrap().id;
    let mut d = Dialog::edit(&app, base).expect("a base flange is editable");
    assert_eq!(d.heading(), "EDIT BASE FLANGE");
    d.cancel(&mut app.s);
  }

  #[test]
  fn an_edited_angle_reads_to_a_tenth() {
    assert_eq!(tenths("90 deg"), "90.0 deg");
    assert_eq!(tenths("45.26 deg"), "45.3 deg");
    assert_eq!(tenths("a * 2"), "a * 2");
  }
}
