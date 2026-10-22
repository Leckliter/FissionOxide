//! Sheet metal rules as Fusion shows them: the docked SHEET METAL RULES palette (MODIFY > Sheet
//! Metal Rules) with the rules "In this design" and the rule library, its modal Edit Rule (and
//! New Rule) window, the browser's "Rule:" row and the SWITCH SHEET METAL RULE dialog its Switch
//! Rule button opens, and the sheet metal icons of the browser's root and bodies.
//!
//! The library starts as Fusion's (`fox_model::sheet_rule_library`); edits to it are kept in
//! `sheet_rules.json` beside the settings and handed to the session, which copies a library rule
//! into the design when a sheet metal feature first uses it.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Color32, RichText, Stroke, pos2, vec2};
use fox_engine::model::{
  Corner3Relief, CornerPlacement, CornerRelief, LengthUnit, ReliefShape, SheetMetalRule, SheetRuleLibrary,
};
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// The palette's area id.
pub const PALETTE: &str = "sheet-rules-palette";
/// The SWITCH SHEET METAL RULE dialog's area id.
pub const SWITCH_AREA: &str = "switch-rule-dialog";
/// The status bar while Switch Rule is open.
pub const SWITCH_PROMPT: &str = "Select a sheet metal rule";
const WIDTH: f32 = 300.0;

/// Which tree a rule row is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
  /// "In this design": the design's own rules.
  Design,
  Library,
}

/// What a rule row's icons and menu do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
  Edit(Place, String),
  /// New Rule: the Edit Rule window on a copy of the row's rule.
  New(Place, String),
  /// The copy icon: a copy named "<name>(1)" beside it.
  Copy(Place, String),
  SetDefault(String),
  Delete(Place, String),
}

/// The Edit Rule (or New Rule) window.
#[derive(Debug, Clone)]
pub struct EditRule {
  pub place: Place,
  /// The rule's name when the window opened; `None` for New Rule.
  pub from: Option<String>,
  /// The values as they opened: Save is greyed until one changes.
  pub original: SheetMetalRule,
  pub rule: SheetMetalRule,
  pub error: Option<String>,
  focus: bool,
}

impl EditRule {
  fn new(place: Place, from: Option<String>, rule: SheetMetalRule) -> EditRule {
    EditRule { place, from, original: rule.clone(), rule, error: None, focus: true }
  }

  pub fn title(&self) -> &'static str {
    if self.from.is_some() { "Edit Rule" } else { "New Rule" }
  }

  /// Whether Save is enabled: a value changed (a new rule can be saved as it is).
  pub fn changed(&self) -> bool {
    self.from.is_none() || self.rule != self.original
  }
}

/// The palette, its windows and the rule library.
#[derive(Debug, Default)]
pub struct SheetRulesUi {
  /// The SHEET METAL RULES palette is showing.
  pub open: bool,
  pub library: SheetRuleLibrary,
  /// `sheet_rules.json`; `None` keeps the library in memory (tests).
  path: Option<PathBuf>,
  pub edit: Option<EditRule>,
  /// The SWITCH SHEET METAL RULE dialog's pick while it is open.
  pub switch: Option<String>,
}

impl SheetRulesUi {
  /// The library saved beside the settings, else Fusion's.
  pub fn load() -> SheetRulesUi {
    let path = fox_desktop::config_dir().join("sheet_rules.json");
    let library = std::fs::read_to_string(&path)
      .ok()
      .and_then(|t| serde_json::from_str::<SheetRuleLibrary>(&t).ok())
      .filter(|l| !l.rules.is_empty())
      .unwrap_or_default();
    SheetRulesUi { library, path: Some(path), ..SheetRulesUi::default() }
  }

  fn save(&self) {
    if let Some(path) = &self.path
      && let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&self.library).unwrap_or_default())
    {
      log::warn!("couldn't save {}: {e}", path.display());
    }
  }
}

/// Hands the library to the session (a new or opened document starts with Fusion's).
pub fn sync(app: &mut App) {
  if app.s.sheet_library != app.sheet_rules.library {
    app.s.sheet_library = app.sheet_rules.library.clone();
  }
}

/// MODIFY > Sheet Metal Rules: shows the palette. It changes nothing in the design.
pub fn open_palette(app: &mut App) {
  app.sheet_rules.open = true;
}

pub fn close_palette(app: &mut App) {
  app.sheet_rules.open = false;
  app.sheet_rules.edit = None;
}

/// A library rule's row label: the default reads "<name> (Default)".
pub fn library_label(lib: &SheetRuleLibrary, name: &str) -> String {
  if lib.default == name { format!("{name} (Default)") } else { name.to_string() }
}

/// The rules of a tree.
pub fn rules(app: &App, place: Place) -> Vec<SheetMetalRule> {
  match place {
    Place::Design => app.s.design.sheet_metal_rules.clone(),
    Place::Library => app.sheet_rules.library.rules.clone(),
  }
}

fn find(app: &App, place: Place, name: &str) -> Option<SheetMetalRule> {
  rules(app, place).into_iter().find(|r| r.name == name)
}

/// The unit a rule's values read in: inches for a rule whose thickness is in inches, else mm.
pub fn rule_unit(rule: &SheetMetalRule) -> LengthUnit {
  let inch = rule.thickness.split(|c: char| !c.is_alphanumeric()).any(|w| w == "in" || w == "inch");
  if inch { LengthUnit::In } else { LengthUnit::Mm }
}

fn length(mm: f64, unit: LengthUnit) -> String {
  match unit {
    LengthUnit::In => format!("{:.3} in", unit.from_mm(mm)),
    u => format!("{:.2} {}", u.from_mm(mm), u.symbol()),
  }
}

fn number(v: f64) -> String {
  let s = format!("{v:.3}");
  s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn relief_label(r: ReliefShape) -> &'static str {
  match r {
    ReliefShape::Rectangular => "Rectangular",
    ReliefShape::Round => "Round",
    ReliefShape::None => "None",
  }
}

fn relief_key(r: ReliefShape) -> &'static str {
  match r {
    ReliefShape::Rectangular => "rectangular",
    ReliefShape::Round => "round",
    ReliefShape::None => "none",
  }
}

pub fn corner_label(r: CornerRelief) -> &'static str {
  match r {
    CornerRelief::None => "No Replacement",
    CornerRelief::Round => "Round",
    CornerRelief::Square => "Square",
    CornerRelief::Tear => "Tear",
  }
}

fn corner_key(r: CornerRelief) -> &'static str {
  match r {
    CornerRelief::None => "none",
    CornerRelief::Round => "round",
    CornerRelief::Square => "square",
    CornerRelief::Tear => "tear",
  }
}

pub fn placement_label(p: CornerPlacement) -> &'static str {
  match p {
    CornerPlacement::Intersection => "Intersection",
    CornerPlacement::Tangent => "Tangent",
  }
}

fn placement_key(p: CornerPlacement) -> &'static str {
  match p {
    CornerPlacement::Intersection => "intersection",
    CornerPlacement::Tangent => "tangent",
  }
}

pub fn corner3_label(r: Corner3Relief) -> &'static str {
  match r {
    Corner3Relief::NoReplacement => "No Replacement",
    Corner3Relief::Intersection => "Intersection",
    Corner3Relief::FullRound => "Full Round",
    Corner3Relief::RoundWithRadius => "Round with Radius",
  }
}

fn corner3_key(r: Corner3Relief) -> &'static str {
  match r {
    Corner3Relief::NoReplacement => "no_replacement",
    Corner3Relief::Intersection => "intersection",
    Corner3Relief::FullRound => "full_round",
    Corner3Relief::RoundWithRadius => "round_with_radius",
  }
}

/// One read-only line of an expanded rule row: its depth under the rule, label and value (empty
/// for a group heading).
pub type ValueRow = (usize, &'static str, String);

/// The values an expanded rule row shows, as Fusion lists them: Thickness, K Factor, Gap, then
/// Bend Conditions and Corner Conditions (2-Bend and 3-Bend Intersection), in the rule's unit.
pub fn value_rows(rule: &SheetMetalRule) -> Vec<ValueRow> {
  let unit = rule_unit(rule);
  let Ok(v) = fox_engine::model::sheet::eval_rule(rule, &HashMap::new()) else {
    // a rule that refers to the design's parameters reads as its expressions
    return vec![(0, "Thickness", rule.thickness.clone()), (0, "K Factor", rule.k_factor.clone())];
  };
  let l = |mm: f64| length(mm, unit);
  let mut rows = vec![
    (0, "Thickness", l(v.thickness)),
    (0, "K Factor", number(v.k_factor)),
    (0, "Gap", l(v.gap)),
    (0, "Bend Conditions", String::new()),
    (1, "Bend radius", l(v.bend_radius)),
    (1, "Relief shape", relief_label(v.relief).into()),
    (1, "Relief width", l(v.relief_width)),
    (1, "Relief depth", l(v.relief_depth)),
    (1, "Relief remnant", l(v.relief_remnant)),
    (0, "Corner Conditions", String::new()),
    (1, "2-Bend Intersection", String::new()),
    (2, "Relief shape", corner_label(v.corner_relief).into()),
    (2, "Relief size", l(v.corner_relief_size())),
    (2, "Relief placement", placement_label(v.corner_placement).into()),
    (1, "3-Bend Intersection", String::new()),
    (2, "Relief shape", corner3_label(v.corner3_relief).into()),
  ];
  if v.corner3_relief == Corner3Relief::RoundWithRadius {
    rows.push((2, "Relief radius", l(v.corner3_radius)));
  }
  rows
}

/// A rule as `sheet.rule` parameters (every value, so the design's copy is the rule shown).
pub fn rule_params(rule: &SheetMetalRule) -> Value {
  json!({
    "name": rule.name, "thickness": rule.thickness, "bend_radius": rule.bend_radius, "k_factor": rule.k_factor,
    "relief": relief_key(rule.relief), "relief_width": rule.relief_width, "relief_depth": rule.relief_depth,
    "relief_remnant": rule.relief_remnant, "gap": rule.gap, "corner_relief": corner_key(rule.corner_relief),
    "corner_relief_size": rule.corner_relief_size, "corner_placement": placement_key(rule.corner_placement),
    "corner3_relief": corner3_key(rule.corner3_relief), "corner3_radius": rule.corner3_radius,
  })
}

/// "<name>(1)", "<name>(2)" ...: the first name not taken (Fusion's copies, as "Steel (in)(1)").
pub fn copy_name(taken: &[String], name: &str) -> String {
  (1..).map(|i| format!("{name}({i})")).find(|n| !taken.contains(n)).unwrap_or_default()
}

fn names(app: &App, place: Place) -> Vec<String> {
  rules(app, place).into_iter().map(|r| r.name).collect()
}

/// Whether a row's menu item applies: Set as Default to library rules, Delete to a library rule
/// other than the default and to a design rule no feature uses.
pub fn enabled(app: &App, action: &Action) -> bool {
  match action {
    Action::SetDefault(n) => app.sheet_rules.library.default != *n,
    Action::Delete(Place::Library, n) => app.sheet_rules.library.default != *n,
    Action::Delete(Place::Design, n) => {
      let default = app.s.design.sheet_metal_rules.first().is_some_and(|r| r.name == *n);
      !app.s.design.features.iter().any(|f| f.kind.sheet_rule().is_some_and(|r| r == n || (r.is_empty() && default)))
    }
    _ => true,
  }
}

/// Carries out a rule row's icon or menu item.
pub fn act(app: &mut App, action: Action) {
  if !enabled(app, &action) {
    return;
  }
  match action {
    Action::Edit(place, name) => {
      if let Some(r) = find(app, place, &name) {
        app.sheet_rules.edit = Some(EditRule::new(place, Some(name), r));
      }
    }
    Action::New(place, name) => {
      if let Some(mut r) = find(app, place, &name) {
        r.name = copy_name(&names(app, place), &name);
        app.sheet_rules.edit = Some(EditRule::new(place, None, r));
      }
    }
    Action::Copy(place, name) => {
      let Some(mut r) = find(app, place, &name) else { return };
      r.name = copy_name(&names(app, place), &name);
      match place {
        Place::Design => {
          app.run("sheet.rule", rule_params(&r));
        }
        Place::Library => {
          let lib = &mut app.sheet_rules.library;
          let at = lib.rules.iter().position(|x| x.name == name).map_or(lib.rules.len(), |i| i + 1);
          lib.rules.insert(at, r);
          app.sheet_rules.save();
        }
      }
    }
    Action::SetDefault(name) => {
      app.sheet_rules.library.default = name;
      app.sheet_rules.save();
    }
    Action::Delete(Place::Library, name) => {
      app.sheet_rules.library.rules.retain(|r| r.name != name);
      app.sheet_rules.save();
    }
    Action::Delete(Place::Design, name) => {
      app.run("sheet.rule_remove", json!({"name": name}));
    }
  }
  sync(app);
}

/// Edit Rule's Save: the design's rule through `sheet.rule` (renaming it if its name changed),
/// a library rule in the library. Keeps the window open with the reason when a value is wrong.
pub fn save_edit(app: &mut App) {
  let Some(e) = app.sheet_rules.edit.clone() else { return };
  if !e.changed() {
    return;
  }
  let name = e.rule.name.trim().to_string();
  let taken = names(app, e.place).into_iter().any(|n| n == name && Some(&n) != e.from.as_ref());
  let error = if name.is_empty() {
    Some("The rule needs a name".to_string())
  } else if taken {
    Some(format!("There is already a rule named {name}"))
  } else {
    fox_engine::model::sheet::eval_rule(&e.rule, &HashMap::new()).err()
  };
  if let Some(err) = error {
    // a design rule may refer to the design's parameters: the command has the last word
    if e.place == Place::Library || name.is_empty() || taken {
      if let Some(e) = app.sheet_rules.edit.as_mut() {
        e.error = Some(err);
      }
      return;
    }
  }
  let mut rule = e.rule.clone();
  rule.name = name.clone();
  match e.place {
    Place::Design => {
      let mut p = rule_params(&rule);
      if let Some(from) = &e.from {
        p["from"] = json!(from);
      }
      if app.run("sheet.rule", p).is_none() {
        let msg = app.status.as_ref().map(|s| s.0.clone());
        if let Some(e) = app.sheet_rules.edit.as_mut() {
          e.error = msg;
        }
        return;
      }
    }
    Place::Library => {
      let lib = &mut app.sheet_rules.library;
      match e.from.as_ref().and_then(|f| lib.rules.iter().position(|r| r.name == *f)) {
        Some(i) => lib.rules[i] = rule,
        None => lib.rules.push(rule),
      }
      if e.from.as_ref().is_some_and(|f| *f == lib.default) {
        lib.default = name;
      }
      app.sheet_rules.save();
    }
  }
  app.sheet_rules.edit = None;
  sync(app);
}

// ---------------------------------------------------------------- the browser

/// The browser's Rule row: "Rule: .063 Aluminum" once the design has sheet metal.
pub fn browser_label(app: &App) -> Option<String> {
  app.s.design.component_sheet_rule().map(|r| format!("Rule: {}", r.name))
}

/// The browser root's icon: a sheet metal component once the design has sheet metal.
pub fn root_icon(app: &App) -> Icon {
  if app.s.design.component_sheet_rule().is_some() { Icon::SheetComponent } else { Icon::Component }
}

/// Switch Rule: opens the docked dialog on the rule the design uses.
pub fn open_switch(app: &mut App) {
  let Some(r) = app.s.design.component_sheet_rule() else { return };
  app.cancel_dialog();
  crate::doc_dialogs::close(app);
  app.sheet_rules.switch = Some(r.name);
  app.hint(SWITCH_PROMPT);
}

pub fn close_switch(app: &mut App) {
  if app.sheet_rules.switch.take().is_some() {
    app.end_hint();
  }
}

/// The rules a design can take, as Fusion's Sheet Metal Rule lists show them: the design's,
/// then the library's.
pub fn rule_choices(s: &fox_engine::Session) -> Vec<String> {
  let mut out: Vec<String> = s.design.sheet_metal_rules.iter().map(|r| r.name.clone()).collect();
  for r in &s.sheet_library.rules {
    if !out.contains(&r.name) {
      out.push(r.name.clone());
    }
  }
  out
}

/// The rules Switch Rule offers.
pub fn switch_choices(app: &App) -> Vec<String> {
  rule_choices(&app.s)
}

/// Whether Switch Rule's OK is enabled: a rule other than the design's is picked.
pub fn switch_changed(app: &App) -> bool {
  let now = app.s.design.component_sheet_rule().map(|r| r.name);
  app.sheet_rules.switch.as_ref().is_some_and(|p| Some(p) != now.as_ref())
}

/// Switch Rule's OK.
pub fn confirm_switch(app: &mut App) {
  if !switch_changed(app) {
    return;
  }
  let Some(rule) = app.sheet_rules.switch.clone() else { return };
  close_switch(app);
  sync(app);
  app.run("sheet.switch_rule", json!({"rule": rule}));
}

/// The Rule row, between Document Settings and Named Views: the rules icon, "Rule: <name>", and
/// on hover a Switch Rule button at its right. It has no right-click menu, as in Fusion.
pub(crate) fn browser_row(app: &mut App, ui: &mut egui::Ui) {
  let Some(label) = browser_label(app) else { return };
  let id = egui::Id::new("tree-sheet-rule");
  let open = app.sheet_rules.switch.is_some();
  let ev = panels::tree_row(ui, id, 1, None, None, Icon::SmRules, &label, open, theme::TEXT);
  let Some(r) = ev.response else { return };
  let br = switch_button_rect(r.rect);
  let button = ui.interact(br, id.with("switch"), egui::Sense::click());
  if r.hovered() || button.hovered() || open {
    let p = ui.painter();
    if button.hovered() || open {
      p.rect_filled(br, 2.0, theme::HOVER);
    }
    paint_switch(p, br.shrink(3.0), if button.hovered() { theme::ACCENT } else { theme::GROUP_LABEL });
  }
  if button.on_hover_text("Switch Rule").clicked() {
    open_switch(app);
  }
}

/// Where the Rule row's Switch Rule button sits: at the row's right end.
pub fn switch_button_rect(row: egui::Rect) -> egui::Rect {
  egui::Rect::from_center_size(pos2(row.right() - 16.0, row.center().y), vec2(20.0, 20.0))
}

/// Two opposed arrows: Fusion's Switch Rule glyph.
fn paint_switch(p: &egui::Painter, r: egui::Rect, c: Color32) {
  let s = Stroke::new(1.4, c);
  let (y1, y2) = (r.top() + r.height() * 0.32, r.top() + r.height() * 0.68);
  p.line_segment([pos2(r.left() + 1.0, y1), pos2(r.right() - 1.0, y1)], s);
  p.line_segment([pos2(r.right() - 4.0, y1 - 3.0), pos2(r.right() - 1.0, y1)], s);
  p.line_segment([pos2(r.left() + 1.0, y2), pos2(r.right() - 1.0, y2)], s);
  p.line_segment([pos2(r.left() + 4.0, y2 + 3.0), pos2(r.left() + 1.0, y2)], s);
}

// ---------------------------------------------------------------- drawing

/// Draws the palette, the Edit Rule window and the Switch Rule dialog.
pub fn show(app: &mut App, ctx: &egui::Context) {
  sync(app);
  if app.sheet_rules.switch.is_some() && app.dialog.is_some() {
    // a command started from elsewhere takes the dialog's place
    close_switch(app);
  }
  if app.sheet_rules.switch.is_some() {
    switch_dialog(app, ctx);
  } else if app.sheet_rules.open {
    palette(app, ctx);
  }
  if app.sheet_rules.edit.is_some() {
    edit_window(app, ctx);
  }
}

fn bottom_bar(ui: &mut egui::Ui, buttons: impl FnOnce(&mut egui::Ui)) {
  let sep = ui.allocate_exact_size(vec2(WIDTH, 1.0), egui::Sense::hover()).0;
  ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
  egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
      let (r, _) = ui.allocate_exact_size(vec2(18.0, 24.0), egui::Sense::hover());
      ui.painter().circle_stroke(r.center(), 7.0, Stroke::new(1.2, theme::GROUP_LABEL));
      ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::proportional(11.0),
        theme::GROUP_LABEL,
      );
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
    });
  });
}

fn palette(app: &mut App, ctx: &egui::Context) {
  let dock = panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  let mut close = false;
  let mut action = None;
  egui::Area::new(egui::Id::new(PALETTE))
    .pivot(egui::Align2::RIGHT_TOP)
    .fixed_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, Icon::SmRules, "SHEET METAL RULES", WIDTH);
        egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 6, top: 6, bottom: 6 }).show(ui, |ui| {
          egui::ScrollArea::vertical().max_height((app.canvas.height() - 320.0).max(160.0)).show(ui, |ui| {
            ui.set_width(WIDTH - 12.0);
            for (place, title) in [(Place::Design, "In this design"), (Place::Library, "Library")] {
              let fid = egui::Id::new("sm-rules-tree").with(title);
              if folder_row(ui, fid, title) {
                for r in rules(app, place) {
                  let label = if place == Place::Library {
                    library_label(&app.sheet_rules.library, &r.name)
                  } else {
                    r.name.clone()
                  };
                  if let Some(a) = rule_row(app, ui, fid.with(&r.name), place, &r, &label) {
                    action = Some(a);
                  }
                }
              }
            }
          });
        });
        bottom_bar(ui, |ui| {
          if panels::secondary_button(ui, "Close").clicked() {
            close = true;
          }
        });
      });
    });
  if close {
    close_palette(app);
  }
  if let Some(a) = action {
    act(app, a);
  }
}

/// A folder of the palette's tree, open to begin with. Returns whether it is open.
fn folder_row(ui: &mut egui::Ui, id: egui::Id, title: &str) -> bool {
  let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::click());
  let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
  if resp.clicked() {
    open = !open;
    ui.data_mut(|d| d.insert_temp(id, open));
  }
  let p = ui.painter();
  triangle(p, pos2(rect.left() + 10.0, rect.center().y), open);
  icons::draw_color(
    p,
    egui::Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(16.0, 16.0)),
    Icon::Folder,
  );
  p.text(
    pos2(rect.left() + 42.0, rect.center().y),
    egui::Align2::LEFT_CENTER,
    title,
    egui::FontId::proportional(13.0),
    theme::TEXT,
  );
  open
}

fn triangle(p: &egui::Painter, c: egui::Pos2, open: bool) {
  let pts = if open {
    vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  } else {
    vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
  };
  p.add(egui::Shape::convex_polygon(pts, theme::GROUP_LABEL, Stroke::NONE));
}

/// Where a rule row's pencil (edit) and copy icons sit: at its right end.
pub fn row_icon_rects(row: egui::Rect) -> (egui::Rect, egui::Rect) {
  let c = row.right_center();
  (
    egui::Rect::from_center_size(c - vec2(40.0, 0.0), vec2(20.0, 20.0)),
    egui::Rect::from_center_size(c - vec2(16.0, 0.0), vec2(20.0, 20.0)),
  )
}

/// A rule row: collapsed, with its values below when expanded; the pencil and copy icons and a
/// tooltip with the name on hover, Fusion's menu on right-click.
fn rule_row(
  app: &App,
  ui: &mut egui::Ui,
  id: egui::Id,
  place: Place,
  rule: &SheetMetalRule,
  label: &str,
) -> Option<Action> {
  let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::click());
  let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
  let (pencil_r, copy_r) = row_icon_rects(rect);
  let pencil = ui.interact(pencil_r, id.with("edit"), egui::Sense::click());
  let copy = ui.interact(copy_r, id.with("copy"), egui::Sense::click());
  let hovered = resp.hovered() || pencil.hovered() || copy.hovered();
  if resp.clicked() {
    open = !open;
    ui.data_mut(|d| d.insert_temp(id, open));
  }
  let p = ui.painter();
  if hovered {
    p.rect_filled(rect, 2.0, theme::HOVER);
  }
  triangle(p, pos2(rect.left() + 26.0, rect.center().y), open);
  icons::draw_color(
    p,
    egui::Rect::from_center_size(pos2(rect.left() + 44.0, rect.center().y), vec2(16.0, 16.0)),
    Icon::SmRules,
  );
  p.text(
    pos2(rect.left() + 58.0, rect.center().y),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(13.0),
    theme::TEXT,
  );
  let mut action = None;
  if hovered {
    let ink = |h: bool| if h { theme::ACCENT } else { theme::GROUP_LABEL };
    icons::draw(p, pencil_r.shrink(3.0), Icon::Pencil, ink(pencil.hovered()));
    paint_copy(p, copy_r.shrink(4.0), ink(copy.hovered()));
  }
  if pencil.on_hover_text("Edit Rule").clicked() {
    action = Some(Action::Edit(place, rule.name.clone()));
  }
  if copy.on_hover_text("Copy").clicked() {
    action = Some(Action::Copy(place, rule.name.clone()));
  }
  let resp = resp.on_hover_text(&rule.name);
  resp.context_menu(|ui| {
    for (text, a) in [
      ("Edit Rule", Action::Edit(place, rule.name.clone())),
      ("New Rule", Action::New(place, rule.name.clone())),
      ("Set as Default", Action::SetDefault(rule.name.clone())),
      ("Delete", Action::Delete(place, rule.name.clone())),
    ] {
      let on = enabled(app, &a) && !(place == Place::Design && matches!(a, Action::SetDefault(_)));
      if ui.add_enabled(on, egui::Button::new(text)).clicked() {
        action = Some(a);
        ui.close();
      }
    }
  });
  if open {
    for (depth, name, value) in value_rows(rule) {
      let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), egui::Sense::hover());
      let x = r.left() + 62.0 + depth as f32 * 14.0;
      let heading = value.is_empty();
      let color = if heading { theme::GROUP_LABEL } else { theme::TEXT };
      ui.painter().text(
        pos2(x, r.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(12.0),
        color,
      );
      if !heading {
        ui.painter().text(
          pos2(r.right() - 8.0, r.center().y),
          egui::Align2::RIGHT_CENTER,
          value,
          egui::FontId::proportional(12.0),
          theme::MUTED,
        );
      }
    }
  }
  action
}

/// Two overlapping sheets: the copy glyph.
fn paint_copy(p: &egui::Painter, r: egui::Rect, c: Color32) {
  let s = Stroke::new(1.2, c);
  let back = egui::Rect::from_min_size(r.min, r.size() * 0.7);
  let front = egui::Rect::from_min_size(r.min + r.size() * 0.3, r.size() * 0.7);
  p.rect_stroke(back, 1.0, s, egui::StrokeKind::Inside);
  p.rect_filled(front, 1.0, theme::DIALOG_BODY);
  p.rect_stroke(front, 1.0, s, egui::StrokeKind::Inside);
}

fn switch_dialog(app: &mut App, ctx: &egui::Context) {
  let dock = panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  let Some(mut pick) = app.sheet_rules.switch.clone() else { return };
  let choices = switch_choices(app);
  let component = if panels::is_unsaved(app) { "(Unsaved)".to_string() } else { panels::doc_name(app) };
  let (mut ok, mut cancel) = (false, false);
  let changed = switch_changed(app);
  egui::Area::new(egui::Id::new(SWITCH_AREA))
    .pivot(egui::Align2::RIGHT_TOP)
    .fixed_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, Icon::SmRules, "SWITCH SHEET METAL RULE", WIDTH);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          egui::Grid::new("switch-rule").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
            ui.label("Component");
            ui.horizontal(|ui| {
              let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
              icons::draw_color(ui.painter(), r, Icon::SheetComponent);
              ui.label(&component);
            });
            ui.end_row();
            ui.label("Sheet Metal Rule");
            egui::ComboBox::from_id_salt("switch-rule-pick").selected_text(&pick).width(160.0).show_ui(ui, |ui| {
              for n in &choices {
                ui.selectable_value(&mut pick, n.clone(), n);
              }
            });
            ui.end_row();
          });
        });
        bottom_bar(ui, |ui| {
          if panels::secondary_button(ui, "Cancel").clicked() {
            cancel = true;
          }
          let accent = icons::accent_of(Icon::SmRules);
          let okb = if changed {
            panels::primary_button(ui, "OK", accent)
          } else {
            ui.add_enabled(
              false,
              egui::Button::new(RichText::new("OK").color(theme::MUTED))
                .fill(Color32::from_gray(240))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(3)
                .min_size(vec2(72.0, 24.0)),
            )
          };
          if okb.clicked() {
            ok = true;
          }
        });
      });
    });
  if app.sheet_rules.switch.is_none() {
    return;
  }
  app.sheet_rules.switch = Some(pick);
  if cancel {
    close_switch(app);
  } else if ok {
    confirm_switch(app);
  }
}

/// One text field of the Edit Rule window.
fn text_row(ui: &mut egui::Ui, label: &str, v: &mut String, focus: bool) {
  ui.label(label);
  let r = ui.add(egui::TextEdit::singleline(v).desired_width(170.0));
  if focus {
    r.request_focus();
  }
  ui.end_row();
}

fn combo<T: Copy + PartialEq>(
  ui: &mut egui::Ui,
  label: &str,
  id: &str,
  v: &mut T,
  all: &[T],
  name: fn(T) -> &'static str,
) {
  ui.label(label);
  egui::ComboBox::from_id_salt(id).selected_text(name(*v)).width(170.0).show_ui(ui, |ui| {
    for &x in all {
      ui.selectable_value(v, x, name(x));
    }
  });
  ui.end_row();
}

fn edit_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut e) = app.sheet_rules.edit.take() else { return };
  let (mut save, mut cancel) = (false, false);
  let modal = egui::Modal::new(egui::Id::new("sheet-edit-rule")).show(ctx, |ui| {
    ui.set_width(360.0);
    ui.label(RichText::new(e.title()).strong().size(14.0));
    ui.add_space(8.0);
    let focus = std::mem::take(&mut e.focus);
    let r = &mut e.rule;
    egui::Grid::new("edit-rule-top").num_columns(2).min_col_width(120.0).spacing([10.0, 6.0]).show(ui, |ui| {
      text_row(ui, "Rule Name", &mut r.name, focus);
      text_row(ui, "Thickness", &mut r.thickness, false);
      text_row(ui, "K Factor", &mut r.k_factor, false);
      text_row(ui, "Gap", &mut r.gap, false);
    });
    egui::CollapsingHeader::new("Bend Conditions").id_salt("edit-rule-bend").default_open(false).show(ui, |ui| {
      egui::Grid::new("edit-rule-bend-grid").num_columns(2).min_col_width(108.0).spacing([10.0, 6.0]).show(ui, |ui| {
        text_row(ui, "Bend radius", &mut r.bend_radius, false);
        combo(
          ui,
          "Relief shape",
          "edit-rule-relief",
          &mut r.relief,
          &[ReliefShape::Round, ReliefShape::Rectangular, ReliefShape::None],
          relief_label,
        );
        text_row(ui, "Relief width", &mut r.relief_width, false);
        text_row(ui, "Relief depth", &mut r.relief_depth, false);
        text_row(ui, "Relief remnant", &mut r.relief_remnant, false);
      });
    });
    egui::CollapsingHeader::new("Corner Conditions").id_salt("edit-rule-corner").default_open(false).show(ui, |ui| {
      ui.label(RichText::new("2-Bend Intersection").color(theme::GROUP_LABEL));
      egui::Grid::new("edit-rule-2bend").num_columns(2).min_col_width(108.0).spacing([10.0, 6.0]).show(ui, |ui| {
        combo(
          ui,
          "Relief shape",
          "edit-rule-corner2",
          &mut r.corner_relief,
          &[CornerRelief::Round, CornerRelief::Square, CornerRelief::Tear, CornerRelief::None],
          corner_label,
        );
        text_row(ui, "Relief size", &mut r.corner_relief_size, false);
        combo(
          ui,
          "Relief placement",
          "edit-rule-placement",
          &mut r.corner_placement,
          &[CornerPlacement::Intersection, CornerPlacement::Tangent],
          placement_label,
        );
      });
      ui.label(RichText::new("3-Bend Intersection").color(theme::GROUP_LABEL));
      egui::Grid::new("edit-rule-3bend").num_columns(2).min_col_width(108.0).spacing([10.0, 6.0]).show(ui, |ui| {
        combo(
          ui,
          "Relief shape",
          "edit-rule-corner3",
          &mut r.corner3_relief,
          &[
            Corner3Relief::RoundWithRadius,
            Corner3Relief::FullRound,
            Corner3Relief::Intersection,
            Corner3Relief::NoReplacement,
          ],
          corner3_label,
        );
        if r.corner3_relief == Corner3Relief::RoundWithRadius {
          text_row(ui, "Relief radius", &mut r.corner3_radius, false);
        }
      });
    });
    if let Some(err) = &e.error {
      ui.add_space(4.0);
      ui.label(RichText::new(err).color(theme::ERROR));
    }
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      if panels::secondary_button(ui, "Cancel").clicked() {
        cancel = true;
      }
      let changed = e.changed();
      if ui.add_enabled(changed, egui::Button::new("Save").min_size(vec2(72.0, 24.0))).clicked() {
        save = true;
      }
    });
  });
  if modal.should_close() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
    cancel = true;
  }
  if cancel {
    return;
  }
  app.sheet_rules.edit = Some(e);
  if save {
    save_edit(app);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn app() -> App {
    App::with_session(Session::default())
  }

  /// A frame of the whole window, so what `show` draws is laid out.
  fn frame(app: &mut App, ctx: &egui::Context) {
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))),
      ..Default::default()
    };
    ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
  }

  /// A plate flanged from an 80 × 40 rectangle, with `rule` (or the default).
  fn flange(app: &mut App, rule: Option<&str>) {
    app.run("sketch.create", json!({"plane": "xy"}));
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    app.run("sketch.finish", json!({}));
    let p = rule.map_or(json!({}), |r| json!({"rule": r}));
    assert!(app.run("sheet.flange", p).is_some(), "{:?}", app.status);
  }

  fn value<'a>(rows: &'a [ValueRow], depth: usize, label: &str) -> &'a str {
    &rows.iter().find(|r| r.0 == depth && r.1 == label).unwrap().2
  }

  #[test]
  fn library_rows_read_as_fusions() {
    let lib = SheetRuleLibrary::default();
    assert_eq!(lib.rules.len(), 19);
    assert_eq!(lib.rules[0].name, "Steel (mm)");
    assert_eq!(lib.rules[18].name, "Steel (in)(1)");
    assert_eq!(library_label(&lib, ".063 Aluminum"), ".063 Aluminum (Default)");
    assert_eq!(library_label(&lib, "Steel (mm)"), "Steel (mm)");
    let steel = value_rows(lib.get("Steel (mm)").unwrap());
    assert_eq!(value(&steel, 0, "Thickness"), "2.50 mm");
    assert_eq!(value(&steel, 0, "K Factor"), "0.44");
    assert_eq!(value(&steel, 0, "Gap"), "2.50 mm");
    let alu = value_rows(lib.get(".063 Aluminum").unwrap());
    let labels: Vec<&str> = alu.iter().map(|r| r.1).collect();
    assert_eq!(
      labels,
      [
        "Thickness",
        "K Factor",
        "Gap",
        "Bend Conditions",
        "Bend radius",
        "Relief shape",
        "Relief width",
        "Relief depth",
        "Relief remnant",
        "Corner Conditions",
        "2-Bend Intersection",
        "Relief shape",
        "Relief size",
        "Relief placement",
        "3-Bend Intersection",
        "Relief shape",
        "Relief radius",
      ]
    );
    for (depth, label, v) in [
      (0, "Thickness", "0.063 in"),
      (0, "K Factor", "0.4"),
      (0, "Gap", "0.063 in"),
      (1, "Bend radius", "0.035 in"),
      (1, "Relief shape", "Round"),
      (1, "Relief width", "0.063 in"),
      (1, "Relief depth", "0.032 in"),
      (1, "Relief remnant", "0.126 in"),
      (2, "Relief size", "0.252 in"),
      (2, "Relief placement", "Intersection"),
      (2, "Relief radius", "0.063 in"),
    ] {
      assert_eq!(value(&alu, depth, label), v, "{label}");
    }
    assert_eq!(alu[11].2, "Round");
    assert_eq!(alu[15].2, "Round with Radius");
  }

  #[test]
  fn the_palette_opens_docked_without_dirtying_the_document() {
    let ctx = egui::Context::default();
    let mut app = app();
    frame(&mut app, &ctx);
    assert!(!app.s.dirty);
    panels::Cmd::SheetRules.run(&mut app);
    assert!(app.dialog.is_none(), "a palette, not an OK/Cancel command dialog");
    frame(&mut app, &ctx);
    frame(&mut app, &ctx);
    let r = ctx.memory(|m| m.area_rect(egui::Id::new(PALETTE))).expect("the palette shows");
    assert!(r.width() > 250.0);
    assert!(!app.s.dirty, "opening the palette changes nothing");
    assert!(app.s.design.sheet_metal_rules.is_empty());
    close_palette(&mut app);
    assert!(!app.sheet_rules.open);
  }

  #[test]
  fn edit_rule_opens_on_the_rule_and_saves_only_changes() {
    let mut app = app();
    act(&mut app, Action::Edit(Place::Library, ".063 Aluminum".into()));
    let e = app.sheet_rules.edit.as_ref().unwrap();
    assert_eq!(e.title(), "Edit Rule");
    assert!(!e.changed(), "Save is greyed until a value changes");
    let r = &e.rule;
    assert_eq!((r.name.as_str(), r.thickness.as_str(), r.k_factor.as_str()), (".063 Aluminum", "0.063 in", "0.4"));
    assert_eq!((r.gap.as_str(), r.relief_width.as_str()), ("Thickness", "Thickness"));
    assert_eq!((r.relief_depth.as_str(), r.relief_remnant.as_str()), ("Thickness * 0.5", "Thickness * 2.0"));
    assert_eq!((r.corner_relief_size.as_str(), r.corner3_radius.as_str()), ("Thickness * 4.0", "Thickness"));
    assert_eq!(r.relief, ReliefShape::Round);
    assert_eq!(r.corner_relief, CornerRelief::Round);
    assert_eq!(r.corner_placement, CornerPlacement::Intersection);
    assert_eq!(r.corner3_relief, Corner3Relief::RoundWithRadius);
    save_edit(&mut app);
    assert!(app.sheet_rules.edit.is_some(), "nothing changed: Save does nothing");
    // rename the library default: it stays the default
    let e = app.sheet_rules.edit.as_mut().unwrap();
    e.rule.name = "Alu 1.6".into();
    e.rule.thickness = "1.6 mm".into();
    save_edit(&mut app);
    assert!(app.sheet_rules.edit.is_none());
    assert_eq!(app.sheet_rules.library.default, "Alu 1.6");
    assert_eq!(app.s.sheet_library.default_rule().thickness, "1.6 mm");
    // a bad value keeps the window open with the reason
    act(&mut app, Action::Edit(Place::Library, "Alu 1.6".into()));
    app.sheet_rules.edit.as_mut().unwrap().rule.k_factor = "2".into();
    save_edit(&mut app);
    assert!(app.sheet_rules.edit.as_ref().unwrap().error.is_some());
    app.sheet_rules.edit = None;
    // New Rule opens on a copy; the copy icon adds one beside it
    act(&mut app, Action::New(Place::Library, "Steel (mm)".into()));
    assert_eq!(app.sheet_rules.edit.as_ref().unwrap().title(), "New Rule");
    assert_eq!(app.sheet_rules.edit.as_ref().unwrap().rule.name, "Steel (mm)(1)");
    save_edit(&mut app);
    assert!(app.sheet_rules.library.get("Steel (mm)(1)").is_some());
    act(&mut app, Action::Copy(Place::Library, "Steel (mm)".into()));
    assert_eq!(app.sheet_rules.library.rules[1].name, "Steel (mm)(2)");
    // the default can't be deleted; another rule can, and Set as Default moves the default
    assert!(!enabled(&app, &Action::Delete(Place::Library, "Alu 1.6".into())));
    act(&mut app, Action::Delete(Place::Library, "Steel (mm)(2)".into()));
    assert!(app.sheet_rules.library.get("Steel (mm)(2)").is_none());
    act(&mut app, Action::SetDefault("Steel (mm)".into()));
    assert_eq!(app.s.sheet_library.default, "Steel (mm)");
    // a new design's first flange takes the library's default
    flange(&mut app, None);
    assert_eq!(browser_label(&app).as_deref(), Some("Rule: Steel (mm)"));
  }

  #[test]
  fn first_flange_shows_the_rule_row_and_sheet_metal_icons() {
    let mut app = app();
    assert!(browser_label(&app).is_none());
    assert_eq!(root_icon(&app), Icon::Component);
    flange(&mut app, None);
    assert_eq!(browser_label(&app).as_deref(), Some("Rule: .063 Aluminum"));
    assert_eq!(root_icon(&app), Icon::SheetComponent);
    let body = app.s.model.bodies[0].id;
    assert_eq!(panels::body_icon(&app, body), Icon::SheetBody);
    // the design's rule is in "In this design"
    let names: Vec<String> = rules(&app, Place::Design).into_iter().map(|r| r.name).collect();
    assert_eq!(names, [".063 Aluminum"]);
    assert!(!enabled(&app, &Action::Delete(Place::Design, ".063 Aluminum".into())), "in use");
  }

  #[test]
  fn switch_rule_offers_every_rule_and_switches() {
    let ctx = egui::Context::default();
    let mut app = app();
    flange(&mut app, None);
    open_switch(&mut app);
    assert_eq!(app.sheet_rules.switch.as_deref(), Some(".063 Aluminum"));
    assert_eq!(app.status.as_ref().map(|s| s.0.as_str()), Some(SWITCH_PROMPT));
    frame(&mut app, &ctx);
    frame(&mut app, &ctx);
    assert!(ctx.memory(|m| m.area_rect(egui::Id::new(SWITCH_AREA))).is_some());
    let choices = switch_choices(&app);
    assert_eq!(choices[0], ".063 Aluminum");
    assert_eq!(choices.len(), 19);
    assert!(!switch_changed(&app), "OK is greyed until another rule is picked");
    confirm_switch(&mut app);
    assert!(app.sheet_rules.switch.is_some());
    app.sheet_rules.switch = Some("Steel (mm)".into());
    assert!(switch_changed(&app));
    confirm_switch(&mut app);
    assert!(app.sheet_rules.switch.is_none());
    assert_eq!(browser_label(&app).as_deref(), Some("Rule: Steel (mm)"));
    let t = app.s.model.bodies[0].sheet.as_ref().unwrap().thickness;
    assert!((t - 2.5).abs() < 1e-9);
    // Escape closes it, changing nothing
    open_switch(&mut app);
    app.escape();
    assert!(app.sheet_rules.switch.is_none());
    assert_eq!(browser_label(&app).as_deref(), Some("Rule: Steel (mm)"));
  }

  #[test]
  fn editing_a_design_rule_renames_it_for_its_features() {
    let mut app = app();
    flange(&mut app, None);
    act(&mut app, Action::Edit(Place::Design, ".063 Aluminum".into()));
    let e = app.sheet_rules.edit.as_mut().unwrap();
    e.rule.name = "Plate".into();
    e.rule.thickness = "2 mm".into();
    save_edit(&mut app);
    assert!(app.sheet_rules.edit.is_none(), "{:?}", app.status);
    assert_eq!(browser_label(&app).as_deref(), Some("Rule: Plate"));
    let t = app.s.model.bodies[0].sheet.as_ref().unwrap().thickness;
    assert!((t - 2.0).abs() < 1e-9);
    // the library keeps its rule
    assert!(app.sheet_rules.library.get(".063 Aluminum").is_some());
  }
}
