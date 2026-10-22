//! The sheet metal dialogs' rows as Fusion lays them out. Fold, Convert to Sheet Metal and Rip
//! open with one lit selection row (Reference, Source, Face/Edge/Vertex) and OK greyed, their
//! other rows coming with the pick; Hem, once its edges are picked, lists them ("Edge 1 | Full
//! Edge") above Edges, Type (Fusion's five hems), Length ("Thickness * 4"), Bend Position, Flip
//! and the Bend Override group.

use egui::{RichText, Stroke, pos2, vec2};

use super::{Dialog, sel_chip_lit, value_edit};
use crate::App;
use crate::icons::{self, Icon};
use crate::theme;

/// Hem's Type values as Fusion's drop-down lists them: key, name and icon.
pub(super) const HEM_TYPES: [(&str, &str, Icon); 5] = [
  ("flat", "Flat", Icon::HemFlat),
  ("open", "Open", Icon::HemOpen),
  ("rolled", "Rolled", Icon::HemRolled),
  ("teardrop", "Teardrop", Icon::HemTeardrop),
  ("rope", "Rope", Icon::HemRope),
];

/// Fusion's hem length: four sheet thicknesses.
pub(super) const HEM_LENGTH: &str = "Thickness * 4";
/// An open hem's gap until one is typed.
pub(super) const HEM_GAP: &str = "Thickness * 0.5";
/// A flat hem's gap: the least the kernel folds without fusing the hem to the sheet.
pub(super) const FLAT_HEM_GAP: &str = "0.01 mm";

/// Bend Position: the hem's bend starts at the edge (Adjacent); the others need engine support.
const BEND_POSITIONS: [(&str, &str); 3] = [("adjacent", "Adjacent"), ("inside", "Inside"), ("outside", "Outside")];

/// Does FissionOxide fold this hem type? (Rolled, teardrop and rope hems need curled sections.)
pub(super) fn hem_folds(key: &str) -> bool {
  matches!(key, "flat" | "open")
}

/// What a selection list's buttons asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListAct {
  None,
  Clear,
}

/// Fusion's selection list: a white box of rows ("Edge 1 | Full Edge"), empty until something is
/// picked, with + (another set, which FissionOxide doesn't have) and x (clear) beside it.
fn sel_list(ui: &mut egui::Ui, rows: &[(String, &str)]) -> ListAct {
  let mut act = ListAct::None;
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 4.0;
    let h = 22.0 * rows.len().max(1) as f32 + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(150.0, h), egui::Sense::hover());
    let p = ui.painter();
    p.rect(rect, 2.0, egui::Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    for (i, (name, extent)) in rows.iter().enumerate() {
      let y = rect.top() + 2.0 + 22.0 * i as f32 + 11.0;
      if i == 0 {
        // the row being worked on, as Fusion highlights it
        let r = egui::Rect::from_min_max(pos2(rect.left() + 1.0, y - 11.0), pos2(rect.right() - 1.0, y + 11.0));
        p.rect_filled(r, 0.0, theme::SELECTED);
      }
      let font = egui::FontId::proportional(12.0);
      p.text(pos2(rect.left() + 6.0, y), egui::Align2::LEFT_CENTER, name, font.clone(), theme::TEXT);
      let x = rect.left() + 62.0;
      p.line_segment([pos2(x, y - 7.0), pos2(x, y + 7.0)], Stroke::new(1.0, theme::BORDER));
      p.text(pos2(x + 6.0, y), egui::Align2::LEFT_CENTER, *extent, font, theme::TEXT);
    }
    ui.vertical(|ui| {
      ui.spacing_mut().item_spacing.y = 2.0;
      ui.add_enabled(false, egui::Button::new("+").small())
        .on_disabled_hover_text("Each command makes one set in FissionOxide");
      if ui.add_enabled(!rows.is_empty(), egui::Button::new("✕").small()).on_hover_text("Clear the selection").clicked()
      {
        act = ListAct::Clear;
      }
    });
  });
  act
}

impl Dialog {
  /// A selection row, lit as Fusion's active selection input: "Select", then the count.
  fn pick_row(&self, ui: &mut egui::Ui, label: &str, n: usize) {
    ui.label(label);
    let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
    sel_chip_lit(ui, self.accent(), &text, true, n > 0);
    ui.end_row();
  }

  /// The selection list's x: the picks (and the preview of them) go.
  fn clear_picks(&mut self, app: &mut App) {
    self.withdraw(&mut app.s);
    app.s.selection.clear();
    self.faces.clear();
    self.bodies.clear();
    self.edges.clear();
  }

  /// FOLD: Reference (the stationary side of a face on the sheet) above the list of folds; the
  /// bend line, angle and sides once the face is picked.
  pub(super) fn fold_rows(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    let n = self.faces.len().min(1);
    self.pick_row(ui, "Reference", n);
    ui.label("");
    let rows: Vec<(String, &str)> = (0..n).map(|_| ("Fold 1".to_string(), "Bend line")).collect();
    if sel_list(ui, &rows) == ListAct::Clear {
      self.clear_picks(app);
      *dirty = true;
    }
    ui.end_row();
    if n == 0 {
      return;
    }
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
    super::icon_row::flip_toggle(ui, "Flip", "Flip direction", &mut self.flip, dirty);
    Self::check(ui, "Fold the other side", &mut self.flag, dirty);
  }

  /// CONVERT TO SHEET METAL: Source (a face of the body, which stays put), then the rule.
  pub(super) fn convert_rows(&mut self, ui: &mut egui::Ui) {
    let n = if self.faces.is_empty() { self.bodies.len().min(1) } else { 1 };
    self.pick_row(ui, "Source", n);
    if n > 0 {
      self.rule_combo(ui);
    }
  }

  /// RIP: Face/Edge/Vertex (FissionOxide rips along edges), then the gap.
  pub(super) fn rip_rows(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.label("Face/Edge/Vertex");
    let n = self.edges.len();
    let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
    sel_chip_lit(ui, self.accent(), &text, true, n > 0).on_hover_text("FissionOxide rips along edges");
    ui.end_row();
    if n > 0 {
      Self::field(ui, "Gap", &mut self.distance, dirty);
    }
  }

  /// HEM: Edges until one is picked; then the edge list, Edges, Type, Length (and an open hem's
  /// Gap), Bend Position, Flip and the Bend Override group.
  pub(super) fn hem_rows(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    let n = self.edges.len();
    if n == 0 {
      self.pick_row(ui, "Edges", 0);
      return;
    }
    ui.label("");
    let rows: Vec<(String, &str)> = (1..=n).map(|i| (format!("Edge {i}"), "Full Edge")).collect();
    if sel_list(ui, &rows) == ListAct::Clear {
      self.clear_picks(app);
      *dirty = true;
      return;
    }
    ui.end_row();
    self.pick_row(ui, "Edges", n);
    ui.label("Type");
    if hem_type_combo(ui, &mut self.choice) {
      *dirty = true;
    }
    ui.end_row();
    expr_field(ui, "Length", &mut self.distance, dirty);
    if self.choice == "open" {
      expr_field(ui, "Gap", &mut self.distance2, dirty);
    }
    ui.label("Bend Position");
    let shown = BEND_POSITIONS.iter().find(|(k, _)| *k == self.position).map_or("Adjacent", |(_, l)| *l);
    egui::ComboBox::from_id_salt("hem-pos").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      for (k, l) in BEND_POSITIONS {
        if k == "adjacent" {
          if ui.selectable_label(self.position == k, l).clicked() && self.position != k {
            self.position = k;
            *dirty = true;
          }
        } else {
          ui.add_enabled(false, egui::Button::selectable(false, l))
            .on_disabled_hover_text("A hem's bend starts at the edge in FissionOxide");
        }
      }
    });
    ui.end_row();
    ui.label("Flip");
    if icons::icon_button(ui, Icon::Flip, 24.0, "Flip", self.flip, None).clicked() {
      self.flip = !self.flip;
      *dirty = true;
    }
    ui.end_row();
    bend_override(ui);
  }
}

/// A value box with Fusion's greyed "fx" after it while it holds an expression of parameters.
fn expr_field(ui: &mut egui::Ui, label: &str, v: &mut String, dirty: &mut bool) {
  ui.label(label);
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 4.0;
    if value_edit(ui, v).changed() {
      *dirty = true;
    }
    if !v.trim().is_empty() && fox_engine::expr::eval_raw(v, &fox_engine::expr::Empty).is_err() {
      ui.label(RichText::new("fx").italics().size(11.0).color(egui::Color32::from_gray(150)));
    }
  });
  ui.end_row();
}

/// Hem's Type drop-down, an icon before each value; the hems FissionOxide can't fold are greyed.
fn hem_type_combo(ui: &mut egui::Ui, cur: &mut &'static str) -> bool {
  let pad = "      ";
  let paint = |ui: &egui::Ui, row: egui::Rect, icon: Icon| {
    let at = egui::Rect::from_center_size(row.left_center() + vec2(14.0, 0.0), egui::Vec2::splat(16.0));
    icons::draw_color(ui.painter(), at, icon);
  };
  let (_, name, icon) = HEM_TYPES.iter().find(|(k, _, _)| k == cur).copied().unwrap_or(HEM_TYPES[0]);
  let mut changed = false;
  let r =
    egui::ComboBox::from_id_salt("hem-type").selected_text(format!("{pad}{name}")).width(140.0).show_ui(ui, |ui| {
      for (k, l, ic) in HEM_TYPES {
        let r = if hem_folds(k) {
          let r = ui.selectable_label(*cur == k, format!("{pad}{l}"));
          if r.clicked() && *cur != k {
            *cur = k;
            changed = true;
          }
          r
        } else {
          ui.add_enabled(false, egui::Button::selectable(false, format!("{pad}{l}")))
            .on_disabled_hover_text("FissionOxide folds flat and open hems only so far")
        };
        paint(ui, r.rect, ic);
      }
    });
  paint(ui, r.response.rect, icon);
  changed
}

/// Fusion's Bend Override group, folded up under its header: the bend relief's shape and sizes
/// (FissionOxide takes them from the sheet metal rule, so they're greyed).
fn bend_override(ui: &mut egui::Ui) {
  let id = ui.id().with("bend-override");
  let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
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
      ui.add(egui::Label::new(RichText::new("Bend Override").color(col).strong()).sense(egui::Sense::click()));
    resp.clicked() || label.clicked()
  });
  ui.end_row();
  if header.inner {
    ui.data_mut(|d| d.insert_temp(id, !open));
  }
  if !open {
    return;
  }
  let why = "Bend overrides aren't in FissionOxide yet: the sheet metal rule's relief applies";
  ui.label("Relief shape");
  ui.add_enabled_ui(false, |ui| {
    egui::ComboBox::from_id_salt("hem-relief").selected_text("Default").width(140.0).show_ui(ui, |_| {});
  })
  .response
  .on_disabled_hover_text(why);
  ui.end_row();
  for (label, value) in
    [("Relief width", "Thickness"), ("Relief depth", "Thickness * 0.5"), ("Relief remnant", "Thickness * 2.0")]
  {
    ui.label(label);
    let mut v = value.to_string();
    ui.add_enabled(false, egui::TextEdit::singleline(&mut v).desired_width(140.0)).on_disabled_hover_text(why);
    ui.end_row();
  }
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, RawInput, Rect};
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::*;
  use crate::dialogs::Kind;
  use crate::forms;

  /// The text the app draws in one frame.
  fn drawn(ctx: &egui::Context, app: &mut App, time: &mut f64) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    *time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(*time),
      ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| app.draw(ui));
    let mut all = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut all);
    }
    out.drop_without_applying_deltas();
    all
  }

  /// The dialog's text after a couple of frames (the first lays it out).
  fn dialog_text(app: &mut App) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut time = 0.0;
    drawn(&ctx, app, &mut time);
    drawn(&ctx, app, &mut time)
  }

  fn has(text: &[String], s: &str) -> bool {
    text.iter().any(|t| t == s)
  }

  /// A 100 x 60 base flange of the default rule.
  fn sheet() -> Session {
    let mut s = Session::default();
    s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [100, 60]})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    s.execute("sheet.flange", &json!({})).unwrap();
    s.selection.clear();
    s
  }

  fn top_face(s: &Session) -> (Sel, Value) {
    let b = &s.model.bodies[0];
    let f = b.faces().iter().filter(|f| f.normal.z > 0.9).max_by(|a, c| a.center.z.total_cmp(&c.center.z)).unwrap();
    (Sel::Face { body: b.id, face: f.id }, json!({"body": b.id.0, "face": f.id}))
  }

  /// The long top edge at the front of the sheet, as a pick and as the commands take it.
  fn front_edge(s: &Session) -> (Sel, Value) {
    let b = &s.model.bodies[0];
    let top = b.edges().iter().map(|e| e.mid.z).fold(f64::MIN, f64::max);
    let e = b.edges().iter().find(|e| e.length > 99.0 && e.mid.y < 1.0 && (e.mid.z - top).abs() < 1e-6).unwrap();
    (Sel::Edge { body: b.id, edge: e.id }, json!({"body": b.id.0, "edge": e.id}))
  }

  /// FOLD opens with Reference (lit Select) above an empty list, OK greyed and Fusion's prompt;
  /// picking the face brings the bend line and angle.
  #[test]
  fn fold_opens_with_its_reference_row() {
    let mut s = sheet();
    let (sel, face) = top_face(&s);
    s.execute("sketch.create", &json!({"plane": face})).unwrap();
    s.execute("sketch.line", &json!({"points": [[40, -5], [40, 65]]})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    s.selection.clear();
    let mut app = App::with_session(s);
    let d = Dialog::open(&mut app, Kind::Fold);
    assert!(!d.ok_enabled(), "OK waits for the reference");
    assert_eq!(d.prompt(false).as_deref(), Some("Click on the stationary side of a face on a sheet metal body."));
    app.dialog = Some(d);
    let text = dialog_text(&mut app);
    assert!(has(&text, "FOLD") && has(&text, "Reference") && has(&text, "Select"), "{text:?}");
    for gone in ["Sketch", "Bend line", "Angle", "Flip direction", "Fold the other side", "Preview"] {
      assert!(!has(&text, gone), "{gone} waits for the pick: {text:?}");
    }
    app.s.selection = vec![sel];
    let text = dialog_text(&mut app);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!(d.faces.len(), 1);
    assert!(d.ready(), "the newest sketch's line is the bend line");
    assert!(has(&text, "Bend line") && has(&text, "Angle") && has(&text, "1 selected"), "{text:?}");
  }

  /// CONVERT TO SHEET METAL opens with Source alone, lit, OK greyed; the rule comes with the pick.
  #[test]
  fn convert_opens_with_its_source_row() {
    let mut s = Session::default();
    s.execute("box", &json!({"width": 40, "depth": 30, "height": 2})).unwrap();
    s.selection.clear();
    let mut app = App::with_session(s);
    let d = Dialog::open(&mut app, Kind::ConvertSheet);
    assert!(!d.ok_enabled());
    app.dialog = Some(d);
    let text = dialog_text(&mut app);
    assert!(has(&text, "CONVERT TO SHEET METAL") && has(&text, "Source") && has(&text, "Select"), "{text:?}");
    for gone in ["Body", "Stationary face", "Rule", "Preview"] {
      assert!(!has(&text, gone), "{gone}: {text:?}");
    }
    let b = &app.s.model.bodies[0];
    let f = b.faces().iter().find(|f| f.normal.z < -0.9).unwrap();
    app.s.selection = vec![Sel::Face { body: b.id, face: f.id }];
    let text = dialog_text(&mut app);
    assert!(has(&text, "1 selected") && has(&text, "Rule"), "{text:?}");
    assert!(app.dialog.as_ref().unwrap().ready());
  }

  /// RIP opens with Face/Edge/Vertex alone, lit, OK greyed; Gap comes with the pick.
  #[test]
  fn rip_opens_with_one_row() {
    let mut s = Session::default();
    s.execute("box", &json!({"width": 40, "depth": 30, "height": 20})).unwrap();
    s.selection.clear();
    let mut app = App::with_session(s);
    let d = Dialog::open(&mut app, Kind::Rip);
    assert!(!d.ok_enabled());
    app.dialog = Some(d);
    let text = dialog_text(&mut app);
    assert!(has(&text, "RIP") && has(&text, "Face/Edge/Vertex") && has(&text, "Select"), "{text:?}");
    assert!(!has(&text, "Gap") && !has(&text, "Edges") && !has(&text, "Preview"), "{text:?}");
    let b = &app.s.model.bodies[0];
    let e = b.edges()[0].id;
    app.s.selection = vec![Sel::Edge { body: b.id, edge: e }];
    let text = dialog_text(&mut app);
    assert!(has(&text, "Gap") && has(&text, "1 selected"), "{text:?}");
  }

  /// CORNER CLOSURE opens with Edges alone, OK greyed; its corner rows come with the pick, and
  /// the command still goes to the last edge flange.
  #[test]
  fn corner_closure_opens_with_its_edges_row() {
    let mut s = sheet();
    let (_, edge) = front_edge(&s);
    s.execute("sheet.edge_flange", &json!({"edges": [edge], "height": "15 mm"})).unwrap();
    s.selection.clear();
    let mut app = App::with_session(s);
    let d = Dialog::open(&mut app, Kind::Form(&forms::CORNER_CLOSURE));
    assert!(!d.ok_enabled(), "OK waits for the edges");
    assert_eq!(d.prompt(false).as_deref(), Some("Select edges"));
    app.dialog = Some(d);
    let text = dialog_text(&mut app);
    assert!(has(&text, "CORNER CLOSURE") && has(&text, "Edges") && has(&text, "Select"), "{text:?}");
    assert!(!has(&text, "Corners") && !has(&text, "Corner relief") && !has(&text, "Preview"), "{text:?}");
    let b = &app.s.model.bodies[0];
    app.s.selection = vec![Sel::Edge { body: b.id, edge: b.edges()[0].id }];
    let text = dialog_text(&mut app);
    assert!(has(&text, "Corners") && has(&text, "Corner relief"), "{text:?}");
    let d = app.dialog.as_ref().unwrap();
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "sheet.corner_closure");
    assert!(p.get("edges").is_none(), "{p}");
  }

  /// HEM after the edge pick: the edge list, Edges, Type with Fusion's five hems (flat and open
  /// fold), Length "Thickness * 4" with fx, Bend Position, Flip, Bend Override, no Preview; the
  /// flat hem is four thicknesses long.
  #[test]
  fn hem_rows_after_the_edge_pick() {
    let s = sheet();
    let mut app = App::with_session(s);
    let d = Dialog::open(&mut app, Kind::Hem);
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select an edge to hem"));
    app.dialog = Some(d);
    app.s.selection = vec![front_edge(&app.s).0];
    let text = dialog_text(&mut app);
    for row in ["HEM", "Edge 1", "Full Edge", "Edges", "1 selected", "Type", "Length", "fx", "Bend Position", "Flip"] {
      assert!(has(&text, row), "{row}: {text:?}");
    }
    assert!(text.iter().any(|t| t.trim() == "Flat"), "{text:?}");
    assert!(has(&text, "Bend Override") && has(&text, "Adjacent"), "{text:?}");
    assert!(!has(&text, "Gap") && !has(&text, "Preview"), "{text:?}");
    let d = app.dialog.as_ref().unwrap();
    assert_eq!(d.distance, "Thickness * 4");
    assert_eq!(d.prompt(false).as_deref(), Some("Adjust settings or hold Ctrl to modify selection"));
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "sheet.hem");
    assert_eq!(p["length"], "Thickness * 4");
    assert_eq!(p["gap"], FLAT_HEM_GAP);
    assert_eq!(HEM_TYPES.map(|(_, n, _)| n), ["Flat", "Open", "Rolled", "Teardrop", "Rope"]);
    assert!(HEM_TYPES.iter().filter(|(k, _, _)| hem_folds(k)).count() == 2);
    // the hem builds: four thicknesses long, at the kernel's least gap
    let mut s = sheet();
    let (_, edge) = front_edge(&s);
    let r = s.execute("sheet.hem", &json!({"edges": [edge], "length": "Thickness * 4", "gap": FLAT_HEM_GAP}));
    assert!(
      r.is_ok() && s.model.status(s.design.features.last().unwrap().id).is_some_and(|st| st.error.is_none()),
      "{r:?}"
    );
  }
}
