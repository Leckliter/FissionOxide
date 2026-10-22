//! Change Parameters (Modify > Change Parameters), as in Fusion: a PARAMETERS modal with one
//! table of Favorites, User Parameters and Model Parameters (the document, then every feature's
//! and sketch's dimensions d1, d2, ... grouped by feature), with a star, name, unit, expression,
//! value and comment per row. Its toolbar filters (ƒx: set by an expression; favourites; the
//! filter box), stars, adds, copies, deletes, imports and exports user parameters, and
//! Automatic Compute decides whether an edit applies at once or waits for OK. Expressions are
//! editable and change the model as they're committed; OK, ✕ or Escape closes it.

use egui::{Color32, Pos2, Rect, RichText, Stroke, pos2, vec2};
use fox_engine::Session;
use fox_engine::model::{FeatureId, LengthUnit, ParamKind};
use serde_json::json;

use crate::App;
use crate::icons::Icon;
use crate::theme;

/// A parameter row: a user parameter or one of a feature's expressions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
  pub source: Source,
  /// d1, d2, ... or the user parameter's name.
  pub name: String,
  /// What the row sets ("distance", "angle"); the name again for a user parameter.
  pub label: String,
  pub kind: ParamKind,
  /// The stored expression.
  pub expr: String,
  pub value: Option<f64>,
  pub comment: String,
  pub favorite: bool,
  pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
  User,
  Model { feature: FeatureId, index: usize },
}

/// A feature's model parameters.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Group {
  pub feature: FeatureId,
  pub name: String,
  pub rows: Vec<Row>,
}

pub(crate) fn user_rows(s: &Session) -> Vec<Row> {
  s.design
    .parameters
    .iter()
    .map(|p| Row {
      source: Source::User,
      name: p.name.clone(),
      label: p.name.clone(),
      kind: p.kind,
      expr: p.expr.clone(),
      value: s.model.params.get(&p.name).copied(),
      comment: p.comment.clone(),
      favorite: p.favorite,
      error: s.model.param_errors.get(&p.name).cloned(),
    })
    .collect()
}

pub(crate) fn model_groups(s: &Session) -> Vec<Group> {
  let mut out: Vec<Group> = Vec::new();
  for m in s.design.model_parameters() {
    let value = fox_engine::expr::eval(&m.expr, m.kind, &s.model.params).ok();
    let row = Row {
      source: Source::Model { feature: m.feature, index: m.index },
      name: m.name,
      label: m.role,
      kind: m.kind,
      expr: m.expr,
      value,
      comment: String::new(),
      favorite: m.favorite,
      error: None,
    };
    match out.last_mut() {
      Some(g) if g.feature == m.feature => g.rows.push(row),
      _ => out.push(Group { feature: m.feature, name: m.feature_name, rows: vec![row] }),
    }
  }
  out
}

/// Does a row match the filter box (any of its name, what it sets, expression, comment or its
/// feature's name, ignoring case)?
pub(crate) fn matches(row: &Row, group: &str, filter: &str) -> bool {
  let f = filter.trim().to_lowercase();
  f.is_empty()
    || [row.name.as_str(), row.label.as_str(), row.expr.as_str(), row.comment.as_str(), group]
      .iter()
      .any(|t| t.to_lowercase().contains(&f))
}

pub(crate) fn unit_label(kind: ParamKind, unit: LengthUnit) -> &'static str {
  match kind {
    ParamKind::Length => unit.symbol(),
    ParamKind::Angle => "deg",
    ParamKind::Unitless => "",
  }
}

/// A value (mm, radians or plain) in the row's unit, without the unit.
pub(crate) fn value_text(kind: ParamKind, v: f64, unit: LengthUnit) -> String {
  let t = fox_engine::expr::trim_number;
  match kind {
    ParamKind::Length => t(unit.from_mm(v)),
    ParamKind::Angle => t(v.to_degrees()),
    ParamKind::Unitless => t(v),
  }
}

/// The expression a row's box shows: a stored bare length is millimetres (see
/// [`LengthUnit::shown`]).
pub(crate) fn shown_expr(row: &Row, unit: LengthUnit) -> String {
  if row.kind == ParamKind::Length { unit.shown(&row.expr) } else { row.expr.clone() }
}

/// What a row stores for text typed in its box: a bare length means the design's unit.
pub(crate) fn typed_expr(kind: ParamKind, text: &str, unit: LengthUnit) -> String {
  if kind == ParamKind::Length { unit.typed(text.trim()) } else { text.trim().to_string() }
}

// column edges, from the table's left
const STAR: f32 = 4.0;
const PARAM: f32 = 26.0;
const NAME: f32 = 230.0;
const UNIT: f32 = 300.0;
const EXPR: f32 = 350.0;
const VALUE: f32 = 520.0;
const COMMENT: f32 = 600.0;
const END: f32 = 796.0;
const ROW_H: f32 = 24.0;
/// The modal's size.
const SIZE: egui::Vec2 = vec2(END + 24.0, 520.0);
const TOOL: f32 = 24.0;
/// The modal's area id (tests look the window up by it).
pub const MODAL_ID: &str = "change-parameters";

struct Table {
  left: f32,
  unit: LengthUnit,
}

impl Table {
  fn col(&self, row: Rect, a: f32, b: f32) -> Rect {
    Rect::from_min_max(pos2(self.left + a, row.top() + 2.0), pos2(self.left + b - 6.0, row.bottom() - 2.0))
  }
}

/// The New Parameter row's state.
#[derive(Debug, Clone, Default)]
struct NewParam {
  name: String,
  /// 0 = the design's length unit, 1 = degrees, 2 = no unit.
  unit: usize,
  expr: String,
  comment: String,
  /// The name box has had the keyboard once.
  started: bool,
}

/// The dialog's own state while it is open: Fusion's toolbar filters, the Parameter column's
/// sort, the row picked (for the star, copy and delete buttons) and Automatic Compute.
#[derive(Debug, Clone, Default)]
pub struct Panel {
  pub filter: String,
  /// ƒx: only parameters set by an expression, not a plain number.
  pub expressions_only: bool,
  pub favorites_only: bool,
  /// The Parameter column's sort: None = the design's order, Some(false) ascending, Some(true)
  /// descending (by what a row sets).
  pub sort: Option<bool>,
  /// The row picked, as [`row_key`] names it.
  pub selected: Option<String>,
  /// Automatic Compute off: expression edits are held in `pending` until OK (or until it is
  /// switched back on).
  pub manual: bool,
  /// (source, name, expression) of each held edit.
  pub pending: Vec<(Source, String, String)>,
  new: Option<NewParam>,
}

/// What names a row across frames (and in [`Panel::selected`]).
pub fn row_key(source: &Source, name: &str) -> String {
  match source {
    Source::User => format!("u-{name}"),
    Source::Model { feature, index } => format!("m-{}-{index}", feature.0),
  }
}

/// Is a stored expression more than a plain number (with or without a unit)? The ƒx filter
/// keeps these.
pub fn is_expression(expr: &str) -> bool {
  let mut parts = expr.split_whitespace();
  let number = parts.next().is_some_and(|t| t.parse::<f64>().is_ok());
  let unit = parts.next().is_none_or(|u| u.chars().all(char::is_alphabetic));
  !(number && unit && parts.next().is_none())
}

/// Does a row pass the toolbar's filters (the box, ƒx and favourites)?
pub fn shown(panel: &Panel, row: &Row, group: &str) -> bool {
  matches(row, group, &panel.filter)
    && (!panel.favorites_only || row.favorite)
    && (!panel.expressions_only || is_expression(&row.expr))
}

/// Rows in the Parameter column's order.
pub fn sorted(panel: &Panel, mut rows: Vec<Row>) -> Vec<Row> {
  if let Some(desc) = panel.sort {
    rows.sort_by_cached_key(|r| r.label.to_lowercase());
    if desc {
      rows.reverse();
    }
  }
  rows
}

/// User parameters as Fusion exports them: a CSV of name, unit, expression, value and comment.
pub fn csv_text(rows: &[Row], unit: LengthUnit) -> String {
  let cell = |t: &str| {
    if t.contains([',', '"', '\n']) { format!("\"{}\"", t.replace('"', "\"\"")) } else { t.to_string() }
  };
  let mut out = String::from("Name,Unit,Expression,Value,Comments\n");
  for r in rows.iter().filter(|r| r.source == Source::User) {
    let value = r.value.map(|v| value_text(r.kind, v, unit)).unwrap_or_default();
    let line = [&r.name, unit_label(r.kind, unit), &shown_expr(r, unit), &value, &r.comment];
    out.push_str(&line.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
    out.push('\n');
  }
  out
}

/// One CSV line's cells (quotes and doubled quotes as spreadsheets write them).
fn csv_cells(line: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut cur = String::new();
  let mut quoted = false;
  let mut chars = line.chars().peekable();
  while let Some(c) = chars.next() {
    match c {
      '"' if quoted && chars.peek() == Some(&'"') => {
        chars.next();
        cur.push('"');
      }
      '"' => quoted = !quoted,
      ',' if !quoted => out.push(std::mem::take(&mut cur)),
      c => cur.push(c),
    }
  }
  out.push(cur);
  out.into_iter().map(|c| c.trim().to_string()).collect()
}

/// User parameters read from a CSV as [`csv_text`] writes it (the columns found by their
/// header; a bare length means the design's unit): (name, kind, expression, comment).
pub fn parse_csv(text: &str, unit: LengthUnit) -> Vec<(String, ParamKind, String, String)> {
  let mut lines = text.lines().filter(|l| !l.trim().is_empty());
  let Some(head) = lines.next() else { return Vec::new() };
  let heads: Vec<String> = csv_cells(head).iter().map(|h| h.to_lowercase()).collect();
  let col = |n: &str| heads.iter().position(|h| h == n);
  let (Some(name), Some(expr)) = (col("name"), col("expression")) else { return Vec::new() };
  let (unit_col, comment) = (col("unit"), col("comments").or_else(|| col("comment")));
  let mut out = Vec::new();
  for l in lines {
    let cells = csv_cells(l);
    let get = |i: Option<usize>| i.and_then(|i| cells.get(i)).cloned().unwrap_or_default();
    let (n, e) = (get(Some(name)), get(Some(expr)));
    if n.is_empty() || e.is_empty() {
      continue;
    }
    let kind = match get(unit_col).to_lowercase().as_str() {
      "deg" | "rad" => ParamKind::Angle,
      "" | "none" | "unitless" => ParamKind::Unitless,
      _ => ParamKind::Length,
    };
    out.push((n, kind, typed_expr(kind, &e, unit), get(comment)));
  }
  out
}

/// What the toolbar asked for this frame.
enum Act {
  Star(Row),
  Copy(Row),
  Delete(Row),
  Import,
  Export,
}

/// Change Parameters, as Fusion's: a centred modal titled PARAMETERS with an X at the top
/// right, over a window it doesn't dim.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.show_params {
    return;
  }
  let mut close = false;
  let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(8));
  egui::Modal::new(egui::Id::new(MODAL_ID)).frame(frame).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
    ui.set_width(SIZE.x);
    ui.set_height(SIZE.y);
    ui.horizontal(|ui| {
      ui.label(RichText::new("PARAMETERS").strong());
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::click());
        if resp.hovered() {
          ui.painter().rect_filled(r, 2.0, Color32::from_gray(228));
        }
        crate::icons::draw(ui.painter(), r.shrink(5.0), Icon::Close, theme::ICON_INK);
        if resp.on_hover_text("Close").clicked() {
          close = true;
        }
      });
    });
    ui.separator();
    contents(app, ui);
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
      ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 24.0), egui::Sense::hover());
        ui.painter().circle_stroke(r.center(), 7.0, Stroke::new(1.2, theme::GROUP_LABEL));
        ui.painter().text(
          r.center(),
          egui::Align2::CENTER_CENTER,
          "i",
          egui::FontId::proportional(11.0),
          theme::GROUP_LABEL,
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if crate::panels::primary_button(ui, "OK", theme::ACCENT).clicked() {
            close = true;
          }
        });
      });
    });
  });
  if !close && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
    close = true;
  }
  if close {
    finish(app);
  }
}

/// OK, ✕ or Escape: the edits held while Automatic Compute was off are applied, and the
/// dialog closes.
pub fn finish(app: &mut App) {
  compute(app);
  app.show_params = false;
  app.params_ui.new = None;
  app.params_ui.selected = None;
}

/// Applies the held edits (together, in the foreground: OK waits for them).
pub fn compute(app: &mut App) {
  for (source, name, expr) in std::mem::take(&mut app.params_ui.pending) {
    match source {
      Source::User => app.run("param.set", json!({"name": name, "expr": expr})),
      Source::Model { feature, index } => {
        app.run("param.model_set", json!({"feature": feature.0, "index": index, "expr": expr}))
      }
    };
  }
}

fn contents(app: &mut App, ui: &mut egui::Ui) {
  let unit = app.s.design.units;
  let doc = crate::panels::root_label(app);
  let panel = app.params_ui.clone();
  let users: Vec<Row> = sorted(&panel, user_rows(&app.s).into_iter().filter(|r| shown(&panel, r, "")).collect());
  let groups: Vec<Group> = model_groups(&app.s)
    .into_iter()
    .map(|mut g| {
      let name = g.name.clone();
      g.rows.retain(|r| shown(&panel, r, &name));
      g.rows = sorted(&panel, std::mem::take(&mut g.rows));
      g
    })
    .filter(|g| !g.rows.is_empty())
    .collect();
  let favorites: Vec<Row> =
    users.iter().chain(groups.iter().flat_map(|g| g.rows.iter())).filter(|r| r.favorite).cloned().collect();
  let selected: Option<Row> = panel.selected.as_ref().and_then(|k| {
    users.iter().chain(groups.iter().flat_map(|g| g.rows.iter())).find(|r| row_key(&r.source, &r.name) == *k).cloned()
  });
  toolbar(app, ui, selected);
  ui.add_space(4.0);
  let filtering = !app.params_ui.filter.trim().is_empty();
  let body = SIZE.y - 2.0 * 30.0 - TOOL - 24.0;
  egui::ScrollArea::both().auto_shrink([false, false]).max_height(body).show(ui, |ui| {
    ui.set_min_width(END);
    ui.spacing_mut().item_spacing.y = 0.0;
    let t = Table { left: ui.max_rect().left(), unit };
    header(app, ui, &t);
    // Favorites and User Parameters are plain headings, always open; Model Parameters folds
    heading(ui, "Favorites", None);
    for r in &favorites {
      row(app, ui, &t, r, "fav");
    }
    heading(ui, "User Parameters", Some(Icon::Params));
    for r in &users {
      row(app, ui, &t, r, "user");
    }
    if let Some(mut n) = app.params_ui.new.take() {
      app.params_ui.new = if new_row(app, ui, &t, &mut n) { None } else { Some(n) };
    }
    section(ui, "params-model", "Model Parameters", None, true, filtering, |ui| {
      // Fusion lists the document ("(Unsaved)" until saved), folded, then each feature
      section(ui, "params-doc", &doc, Some(Icon::Component), false, filtering, |ui| {
        for g in &groups {
          let id = format!("params-feature-{}", g.feature.0);
          section(ui, &id, &g.name, None, false, filtering, |ui| {
            for r in &g.rows {
              row(app, ui, &t, r, "model");
            }
          });
        }
      });
    });
  });
}

/// Fusion's toolbar row: ƒx and favourites filters, the star, the filter box, add, copy, delete,
/// import and export, and Automatic Compute.
fn toolbar(app: &mut App, ui: &mut egui::Ui, selected: Option<Row>) {
  use crate::icons::icon_button;
  let mut act: Option<Act> = None;
  let user_picked = selected.as_ref().is_some_and(|r| r.source == Source::User);
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 4.0;
    let p = &mut app.params_ui;
    if icon_button(ui, Icon::Params, TOOL, "Show only parameters set by an expression", p.expressions_only, None)
      .clicked()
    {
      p.expressions_only = !p.expressions_only;
    }
    if icon_button(ui, Icon::FilterFavorites, TOOL, "Show only Favorites", p.favorites_only, None).clicked() {
      p.favorites_only = !p.favorites_only;
    }
    let star_tip = match &selected {
      Some(r) if r.favorite => "Remove the selected parameter from Favorites",
      _ => "Add the selected parameter to Favorites",
    };
    if ui
      .add_enabled_ui(selected.is_some(), |ui| icon_button(ui, Icon::Star, TOOL, star_tip, false, None))
      .inner
      .on_disabled_hover_text("Pick a parameter row first")
      .clicked()
      && let Some(r) = &selected
    {
      act = Some(Act::Star(r.clone()));
    }
    ui.add(egui::TextEdit::singleline(&mut p.filter).hint_text("Filter all parameters").desired_width(200.0));
    if icon_button(ui, Icon::Plus, TOOL, "Add a user parameter", p.new.is_some(), None).clicked() && p.new.is_none() {
      p.new = Some(NewParam::default());
    }
    let user_only = "Pick a user parameter row first";
    if ui
      .add_enabled_ui(user_picked, |ui| {
        icon_button(ui, Icon::Duplicate, TOOL, "Copy the selected user parameter", false, None)
      })
      .inner
      .on_disabled_hover_text(user_only)
      .clicked()
      && let Some(r) = &selected
    {
      act = Some(Act::Copy(r.clone()));
    }
    if ui
      .add_enabled_ui(user_picked, |ui| {
        icon_button(ui, Icon::Trash, TOOL, "Delete the selected user parameter (only if unused)", false, None)
      })
      .inner
      .on_disabled_hover_text(user_only)
      .clicked()
      && let Some(r) = &selected
    {
      act = Some(Act::Delete(r.clone()));
    }
    if icon_button(ui, Icon::Import, TOOL, "Import user parameters from a CSV file", false, None).clicked() {
      act = Some(Act::Import);
    }
    if icon_button(ui, Icon::Export, TOOL, "Export the user parameters to a CSV file", false, None).clicked() {
      act = Some(Act::Export);
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      let mut auto = !p.manual;
      if ui
        .checkbox(&mut auto, "Automatic Compute")
        .on_hover_text("Apply each change as it is made; off, changes wait for OK")
        .changed()
      {
        p.manual = !auto;
      }
    });
  });
  if !app.params_ui.manual && !app.params_ui.pending.is_empty() {
    compute(app);
  }
  match act {
    Some(Act::Star(r)) => {
      let p = match r.source {
        Source::User => json!({"name": r.name, "on": !r.favorite}),
        Source::Model { feature, index } => json!({"feature": feature.0, "index": index, "on": !r.favorite}),
      };
      app.run("param.favorite", p);
    }
    Some(Act::Copy(r)) => {
      let taken: Vec<String> = app.s.design.parameters.iter().map(|p| p.name.clone()).collect();
      let name = std::iter::once(format!("{}_copy", r.name))
        .chain((2..).map(|k| format!("{}_copy{k}", r.name)))
        .find(|n| !taken.contains(n))
        .unwrap_or_default();
      let kind = ["length", "angle", "unitless"][match r.kind {
        ParamKind::Length => 0,
        ParamKind::Angle => 1,
        ParamKind::Unitless => 2,
      }];
      app.run("param.add", json!({"name": name, "expr": r.expr, "kind": kind, "comment": r.comment}));
    }
    Some(Act::Delete(r)) => {
      app.run("param.remove", json!({"name": r.name}));
      app.params_ui.selected = None;
    }
    Some(Act::Import) => {
      if let Some(path) = rfd::FileDialog::new().set_title("Import parameters").add_filter("CSV", &["csv"]).pick_file()
      {
        match std::fs::read_to_string(&path) {
          Ok(text) => import(app, &text),
          Err(e) => app.status = Some((format!("Couldn't read {}: {e}", path.display()), true)),
        }
      }
    }
    Some(Act::Export) => {
      let name = format!("{} parameters.csv", crate::panels::doc_name(app));
      if let Some(path) = rfd::FileDialog::new()
        .set_title("Export parameters")
        .add_filter("CSV", &["csv"])
        .set_file_name(name)
        .save_file()
      {
        let text = csv_text(&user_rows(&app.s), app.s.design.units);
        app.status = Some(match std::fs::write(&path, text) {
          Ok(()) => (format!("Exported {}", path.display()), false),
          Err(e) => (format!("Couldn't write {}: {e}", path.display()), true),
        });
      }
    }
    None => {}
  }
}

/// Adds the user parameters of a CSV (an existing name takes the new expression and comment).
pub fn import(app: &mut App, text: &str) {
  let rows = parse_csv(text, app.s.design.units);
  let n = rows.len();
  for (name, kind, expr, comment) in rows {
    if app.s.design.parameters.iter().any(|p| p.name == name) {
      app.run("param.set", json!({"name": name, "expr": expr}));
      app.run("param.comment", json!({"name": name, "comment": comment}));
    } else {
      let kname = match kind {
        ParamKind::Length => "length",
        ParamKind::Angle => "angle",
        ParamKind::Unitless => "unitless",
      };
      app.run("param.add", json!({"name": name, "expr": expr, "kind": kname, "comment": comment}));
    }
  }
  app.status = Some((format!("Imported {n} parameters"), false));
}

/// A section's name with its icon, if any.
fn title_row(ui: &mut egui::Ui, title: &str, icon: Option<Icon>) {
  if let Some(icon) = icon {
    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
    crate::icons::draw_color(ui.painter(), r, icon);
  }
  ui.label(RichText::new(title).strong());
}

/// A section that doesn't fold (Fusion's Favorites and User Parameters): its name with no arrow.
fn heading(ui: &mut egui::Ui, title: &str, icon: Option<Icon>) {
  ui.horizontal(|ui| {
    ui.set_min_height(ROW_H);
    // lined up with the folding sections' names
    ui.add_space(ui.spacing().indent);
    title_row(ui, title, icon);
  });
}

/// A collapsible group; open while the filter is in use.
fn section(
  ui: &mut egui::Ui,
  id: &str,
  title: &str,
  icon: Option<Icon>,
  open: bool,
  filtering: bool,
  body: impl FnOnce(&mut egui::Ui),
) {
  let mut state =
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(id), open);
  if filtering {
    state.set_open(true);
  }
  state.show_header(ui, |ui| title_row(ui, title, icon)).body(body);
}

/// The column headings; the Parameter column's arrow sorts the rows.
fn header(app: &mut App, ui: &mut egui::Ui, t: &Table) {
  let (rect, _) = ui.allocate_exact_size(vec2(END, ROW_H), egui::Sense::hover());
  let p = ui.painter();
  p.rect_filled(rect, 0.0, theme::DIALOG_HEADER);
  let font = egui::FontId::proportional(12.0);
  for (x, label) in [
    (PARAM, "Parameter"),
    (NAME, "Name"),
    (UNIT, "Unit"),
    (EXPR, "Expression"),
    (VALUE, "Value"),
    (COMMENT, "Comments"),
  ] {
    p.text(pos2(t.left + x, rect.center().y), egui::Align2::LEFT_CENTER, label, font.clone(), theme::GROUP_LABEL);
  }
  let sort = app.params_ui.sort;
  let ar = Rect::from_center_size(pos2(t.left + PARAM + 72.0, rect.center().y), vec2(16.0, 16.0));
  let resp = ui.interact(ar, egui::Id::new("params-sort"), egui::Sense::click());
  let (glyph, col) = match sort {
    None => ("▲", theme::MUTED),
    Some(false) => ("▲", theme::TEXT),
    Some(true) => ("▼", theme::TEXT),
  };
  ui.painter().text(ar.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(10.0), col);
  if resp.on_hover_text("Sort by parameter").clicked() {
    app.params_ui.sort = Some(sort != Some(false));
  }
}

/// One parameter row. `ctx` keeps a row's boxes apart where it shows twice (Favorites).
fn row(app: &mut App, ui: &mut egui::Ui, t: &Table, r: &Row, ctx: &str) {
  let (rect, resp) =
    ui.allocate_exact_size(vec2((t.left + END - ui.cursor().left()).max(10.0), ROW_H), egui::Sense::click());
  let rect = Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(t.left + END, rect.bottom()));
  let key = row_key(&r.source, &r.name);
  let id = egui::Id::new(("param-row", ctx, &key));
  // a click on the row picks it (for the toolbar's star, copy and delete)
  if resp.clicked() {
    app.params_ui.selected = Some(key.clone());
  }
  if app.params_ui.selected.as_deref() == Some(key.as_str()) {
    ui.painter().rect_filled(rect, 0.0, theme::SELECTED);
  } else if ui.rect_contains_pointer(rect) {
    ui.painter().rect_filled(rect, 0.0, theme::HOVER);
  }
  // ★
  let sr = Rect::from_center_size(pos2(t.left + STAR + 8.0, rect.center().y), vec2(18.0, 18.0));
  let star = ui.interact(sr, id.with("star"), egui::Sense::click());
  paint_star(ui.painter(), sr.center(), 6.5, r.favorite, if star.hovered() { theme::ACCENT } else { STAR_INK });
  if star.on_hover_text(if r.favorite { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
    let p = match r.source {
      Source::User => json!({"name": r.name, "on": !r.favorite}),
      Source::Model { feature, index } => json!({"feature": feature.0, "index": index, "on": !r.favorite}),
    };
    app.run("param.favorite", p);
  }
  let font = egui::FontId::proportional(12.5);
  let text = |ui: &egui::Ui, a: f32, s: &str, c: Color32| {
    let at = pos2(t.left + a, rect.center().y);
    ui.painter().text(at, egui::Align2::LEFT_CENTER, s, font.clone(), c);
  };
  // the label column starts at the row (indented under its group) and ends at Name
  let label_x = (rect.left() + 4.0).max(t.left + PARAM) - t.left;
  text(ui, label_x, &r.label, theme::TEXT);
  if matches!(r.source, Source::Model { .. }) {
    text(ui, NAME, &r.name, theme::TEXT);
  }
  text(ui, UNIT, unit_label(r.kind, t.unit), theme::MUTED);
  // expression (an edit held by Automatic Compute off shows in the box)
  let held = app.params_ui.pending.iter().find(|(s, ..)| *s == r.source).map(|(.., e)| e.clone());
  let shown = held.clone().unwrap_or_else(|| shown_expr(r, t.unit));
  let eid = id.with("expr");
  let mut buf = ui.data_mut(|d| d.get_temp::<String>(eid)).unwrap_or_else(|| shown.clone());
  let resp = ui.put(t.col(rect, EXPR, VALUE), egui::TextEdit::singleline(&mut buf));
  if resp.changed() {
    ui.data_mut(|d| d.insert_temp(eid, buf.clone()));
  }
  if resp.lost_focus() {
    if buf.trim() != shown.trim() && !buf.trim().is_empty() {
      let expr = typed_expr(r.kind, &buf, t.unit);
      if app.params_ui.manual {
        app.params_ui.pending.retain(|(s, ..)| *s != r.source);
        app.params_ui.pending.push((r.source, r.name.clone(), expr));
      } else {
        apply_expr(app, r.source, &r.name, expr);
      }
    }
    ui.data_mut(|d| d.remove::<String>(eid));
  }
  // value
  let (val, col) = match (r.value, &r.error) {
    (Some(v), None) => (value_text(r.kind, v, t.unit), theme::MUTED),
    _ => ("error".to_string(), theme::ERROR),
  };
  let vr = ui.interact(t.col(rect, VALUE, COMMENT), id.with("value"), egui::Sense::hover());
  text(ui, VALUE, &val, col);
  if let Some(e) = &r.error {
    vr.on_hover_text(e);
  }
  // comment: editable for user parameters
  if r.source == Source::User {
    let cid = id.with("comment");
    let mut c = ui.data_mut(|d| d.get_temp::<String>(cid)).unwrap_or_else(|| r.comment.clone());
    let resp = ui.put(t.col(rect, COMMENT, END), egui::TextEdit::singleline(&mut c).hint_text("comment"));
    if resp.changed() {
      ui.data_mut(|d| d.insert_temp(cid, c.clone()));
    }
    if resp.lost_focus() {
      if c != r.comment {
        app.run("param.comment", json!({"name": r.name, "comment": c}));
      }
      ui.data_mut(|d| d.remove::<String>(cid));
    }
  }
}

/// Sets a row's expression in the design.
fn apply_expr(app: &mut App, source: Source, name: &str, expr: String) {
  match source {
    Source::User => app.run_background("param.set", json!({"name": name, "expr": expr})),
    Source::Model { feature, index } => {
      app.run("param.model_set", json!({"feature": feature.0, "index": index, "expr": expr}));
    }
  }
}

/// The row adding a user parameter. Returns true when it's done (added or cancelled).
fn new_row(app: &mut App, ui: &mut egui::Ui, t: &Table, n: &mut NewParam) -> bool {
  let (rect, _) =
    ui.allocate_exact_size(vec2((t.left + END - ui.cursor().left()).max(10.0), ROW_H + 4.0), egui::Sense::hover());
  let rect = Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(t.left + END, rect.bottom()));
  ui.painter().rect_filled(rect, 0.0, theme::SELECTED);
  let label_x = (rect.left() + 4.0).max(t.left + PARAM);
  let name_r = Rect::from_min_max(pos2(label_x, rect.top() + 3.0), pos2(t.left + UNIT - 6.0, rect.bottom() - 3.0));
  let r = ui.put(name_r, egui::TextEdit::singleline(&mut n.name).hint_text("name"));
  if !n.started {
    n.started = true;
    r.request_focus();
  }
  let mut boxes = vec![r];
  let units = [t.unit.symbol(), "deg", "none"];
  let ur = t.col(rect, UNIT, EXPR);
  ui.scope_builder(egui::UiBuilder::new().max_rect(ur), |ui| {
    egui::ComboBox::from_id_salt("params-new-unit").width(ur.width()).selected_text(units[n.unit.min(2)]).show_ui(
      ui,
      |ui| {
        for (i, u) in units.iter().enumerate() {
          ui.selectable_value(&mut n.unit, i, *u);
        }
      },
    );
  });
  boxes.push(ui.put(
    t.col(rect, EXPR, VALUE),
    egui::TextEdit::singleline(&mut n.expr).hint_text(format!("10 {}", units[n.unit.min(2)])),
  ));
  boxes.push(ui.put(t.col(rect, COMMENT, END), egui::TextEdit::singleline(&mut n.comment).hint_text("comment")));
  // Enter in one of the row's boxes adds it
  let enter = ui.input(|i| i.key_pressed(egui::Key::Enter)) && boxes.iter().any(|b| b.lost_focus());
  let ok_r =
    Rect::from_min_max(pos2(t.left + VALUE, rect.top() + 3.0), pos2(t.left + COMMENT - 6.0, rect.bottom() - 3.0));
  let mut done = false;
  ui.scope_builder(egui::UiBuilder::new().max_rect(ok_r), |ui| {
    ui.horizontal(|ui| {
      if (ui.small_button("OK").clicked() || enter) && !n.name.trim().is_empty() && !n.expr.trim().is_empty() {
        let kind = [ParamKind::Length, ParamKind::Angle, ParamKind::Unitless][n.unit.min(2)];
        let kname = ["length", "angle", "unitless"][n.unit.min(2)];
        let expr = typed_expr(kind, &n.expr, t.unit);
        let p = json!({"name": n.name.trim(), "expr": expr, "kind": kname, "comment": n.comment});
        done = app.run("param.add", p).is_some();
      }
      if ui.small_button("✕").on_hover_text("Cancel").clicked() {
        done = true;
      }
    });
  });
  done
}

const STAR_INK: Color32 = Color32::from_rgb(150, 150, 150);
const STAR_FILL: Color32 = Color32::from_rgb(240, 180, 30);

/// A five-pointed star, filled for a favourite.
fn paint_star(p: &egui::Painter, c: Pos2, r: f32, filled: bool, ink: Color32) {
  let pt = |i: usize, rad: f32| {
    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
    c + vec2(a.cos(), a.sin()) * rad
  };
  let outline: Vec<Pos2> = (0..10).map(|i| pt(i, if i % 2 == 0 { r } else { r * 0.45 })).collect();
  if filled {
    // a convex centre and five convex points
    let inner: Vec<Pos2> = (0..5).map(|i| outline[2 * i + 1]).collect();
    p.add(egui::Shape::convex_polygon(inner, STAR_FILL, Stroke::NONE));
    for i in 0..5 {
      let tri = vec![outline[2 * i], outline[(2 * i + 1) % 10], outline[(2 * i + 9) % 10]];
      p.add(egui::Shape::convex_polygon(tri, STAR_FILL, Stroke::NONE));
    }
  }
  let ink = if filled { Color32::from_rgb(200, 140, 10) } else { ink };
  p.add(egui::Shape::closed_line(outline, Stroke::new(1.1, ink)));
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn session() -> Session {
    let mut s = Session::default();
    let run = |s: &mut Session, c: &str, p: serde_json::Value| s.execute(c, &p).unwrap();
    run(&mut s, "param.add", json!({"name": "w", "expr": "10", "comment": "width"}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let c = r["curves"][0].as_u64().unwrap();
    run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [c], "value": "w"}));
    run(&mut s, "extrude", json!({"distance": "5 mm"}));
    s
  }

  #[test]
  fn rows_list_user_and_model_parameters_grouped_by_feature() {
    let s = session();
    let users = user_rows(&s);
    assert_eq!(users.len(), 1);
    assert_eq!((users[0].name.as_str(), users[0].value, users[0].comment.as_str()), ("w", Some(10.0), "width"));
    let groups = model_groups(&s);
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert!(groups[0].name.starts_with("Sketch") && groups[1].name.starts_with("Extrude"));
    assert_eq!(groups[0].rows[0].name, "d1");
    assert_eq!(groups[0].rows[0].expr, "w");
    assert_eq!(groups[1].rows[0].label, "distance");
    assert_eq!(groups[1].rows[0].value, Some(5.0));
    // numbering runs on across features
    let names: Vec<String> = groups.iter().flat_map(|g| g.rows.iter().map(|r| r.name.clone())).collect();
    assert!(names.iter().enumerate().all(|(i, n)| *n == format!("d{}", i + 1)), "{names:?}");
    // the filter matches names, what a row sets, expressions, comments and feature names
    let ext = &groups[1];
    assert!(matches(&ext.rows[0], &ext.name, "DIST"));
    assert!(matches(&ext.rows[0], &ext.name, "extrude"));
    assert!(!matches(&ext.rows[0], &ext.name, "width"));
    assert!(matches(&users[0], "", "width"));
    assert!(matches(&users[0], "", ""));
  }

  #[test]
  fn values_and_expressions_show_in_the_design_unit() {
    let s = session();
    let ext = model_groups(&s).remove(1).rows.remove(0);
    assert_eq!(unit_label(ext.kind, LengthUnit::In), "in");
    assert_eq!(unit_label(ParamKind::Angle, LengthUnit::In), "deg");
    assert_eq!(value_text(ext.kind, 25.4, LengthUnit::In), "1");
    assert_eq!(value_text(ParamKind::Angle, std::f64::consts::PI, LengthUnit::In), "180");
    let user = user_rows(&s).remove(0);
    assert_eq!(shown_expr(&user, LengthUnit::In), "10 mm", "a stored bare number stays mm");
    assert_eq!(shown_expr(&user, LengthUnit::Mm), "10");
    assert_eq!(typed_expr(ParamKind::Length, " 2 ", LengthUnit::In), "2 in");
    assert_eq!(typed_expr(ParamKind::Unitless, "2", LengthUnit::In), "2");
  }

  /// One frame of the dialog in a 1200 × 800 window, with `events`.
  fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
    let raw = egui::RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
      events,
      ..Default::default()
    };
    ctx.run_ui(raw, |ui| window(app, ui.ctx())).drop_without_applying_deltas();
  }

  /// `ui:dialogs:parameters-dialog-details`: Fusion's PARAMETERS has a painted X (no glyph),
  /// leaves the window behind undimmed, shows Favorites and User Parameters as plain headings and
  /// lists the document as "(Unsaved)", folded, under Model Parameters.
  #[test]
  fn the_dialog_is_undimmed_with_fusions_sections() {
    let mut app = App::with_session(session());
    app.show_params = true;
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
    let mut texts = Vec::new();
    let mut backdrop = false;
    for _ in 0..2 {
      let raw = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
      let out = ctx.run_ui(raw, |ui| window(&mut app, ui.ctx()));
      texts.clear();
      fn walk(s: &egui::Shape, texts: &mut Vec<String>, backdrop: &mut bool, screen: Rect) {
        match s {
          egui::Shape::Text(t) => texts.push(t.galley.text().to_string()),
          egui::Shape::Rect(r) => *backdrop |= r.rect.contains_rect(screen) && r.fill.a() > 0,
          egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts, backdrop, screen)),
          _ => {}
        }
      }
      backdrop = false;
      for c in &out.shapes {
        walk(&c.shape, &mut texts, &mut backdrop, screen);
      }
      out.drop_without_applying_deltas();
    }
    assert!(!backdrop, "the window behind isn't dimmed");
    assert!(!texts.iter().any(|t| t == "✕"), "the close is a painted X: {texts:?}");
    for t in ["Favorites", "User Parameters", "Model Parameters", "(Unsaved)"] {
      assert!(texts.iter().any(|x| x == t), "{t}: {texts:?}");
    }
    assert!(!texts.iter().any(|t| t == "Untitled"), "{texts:?}");
    // the document row starts folded: no feature under it shows
    assert!(!texts.iter().any(|t| t.starts_with("Extrude") || t.starts_with("Sketch")), "{texts:?}");
    // the user parameter shows without unfolding anything
    assert!(texts.iter().any(|t| t == "w"), "{texts:?}");
  }

  #[test]
  fn the_dialog_draws_every_section_and_escape_closes_it() {
    let mut app = App::with_session(session());
    app.show_params = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![]);
    // a new parameter row, the filters and a sort
    app.params_ui.new = Some(NewParam::default());
    app.params_ui.filter = "extrude".into();
    app.params_ui.sort = Some(true);
    frame(&ctx, &mut app, vec![]);
    app.params_ui.filter.clear();
    app.params_ui.favorites_only = true;
    app.params_ui.expressions_only = true;
    frame(&ctx, &mut app, vec![]);
    assert!(app.show_params);
    // the modal sits over the canvas, not at the top left over the toolbar
    let r = ctx.memory(|m| m.area_rect(egui::Id::new(MODAL_ID))).unwrap();
    assert!(r.left() > 50.0 && r.top() > 50.0, "{r:?}");
    // Escape closes it, as OK does
    let esc = egui::Event::Key {
      key: egui::Key::Escape,
      physical_key: None,
      pressed: true,
      repeat: false,
      modifiers: egui::Modifiers::NONE,
    };
    frame(&ctx, &mut app, vec![esc]);
    assert!(!app.show_params);
  }

  #[test]
  fn expression_filter_sort_and_csv() {
    // ƒx keeps the rows that are more than a number
    assert!(!is_expression("10") && !is_expression("10 mm") && !is_expression("2.5 deg"));
    assert!(is_expression("w / 2") && is_expression("2 * 3") && is_expression("w"));
    let s = session();
    let mut panel = Panel { expressions_only: true, ..Panel::default() };
    let groups = model_groups(&s);
    assert!(shown(&panel, &groups[0].rows[0], &groups[0].name), "d1 = w is an expression");
    assert!(!shown(&panel, &groups[1].rows[0], &groups[1].name), "the extrude's 5 mm isn't");
    panel.expressions_only = false;
    panel.favorites_only = true;
    assert!(!shown(&panel, &groups[0].rows[0], &groups[0].name));
    // the Parameter column's arrow sorts by what a row sets
    let rows: Vec<Row> = groups.into_iter().flat_map(|g| g.rows).collect();
    let labels = |rows: &[Row]| rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>();
    let natural = labels(&rows);
    panel.sort = Some(false);
    let up = labels(&sorted(&panel, rows.clone()));
    let mut want = natural.clone();
    want.sort_by_key(|l| l.to_lowercase());
    assert_eq!(up, want);
    panel.sort = Some(true);
    want.reverse();
    assert_eq!(labels(&sorted(&panel, rows.clone())), want);
    panel.sort = None;
    assert_eq!(labels(&sorted(&panel, rows)), natural);
    // export and import round-trip the user parameters (a comma in a comment included)
    let mut s = s;
    s.execute("param.comment", &json!({"name": "w", "comment": "width, outer"})).unwrap();
    s.execute("param.add", &json!({"name": "a", "expr": "30 deg", "kind": "angle"})).unwrap();
    let text = csv_text(&user_rows(&s), LengthUnit::Mm);
    assert!(text.starts_with("Name,Unit,Expression,Value,Comments\n"), "{text}");
    assert!(text.contains("w,mm,10,10,\"width, outer\"\n"), "{text}");
    let back = parse_csv(&text, LengthUnit::Mm);
    assert_eq!(
      back,
      vec![
        ("w".to_string(), ParamKind::Length, "10".to_string(), "width, outer".to_string()),
        ("a".to_string(), ParamKind::Angle, "30 deg".to_string(), String::new()),
      ]
    );
  }

  #[test]
  fn held_edits_apply_on_ok_and_imports_add_or_update() {
    let mut app = App::with_session(session());
    app.show_params = true;
    // Automatic Compute off: the edit waits
    app.params_ui.manual = true;
    app.params_ui.pending.push((Source::User, "w".into(), "20 mm".into()));
    assert_eq!(app.s.model.params.get("w"), Some(&10.0));
    finish(&mut app);
    assert!(!app.show_params && app.params_ui.pending.is_empty());
    assert_eq!(app.s.model.params.get("w"), Some(&20.0));
    // an import adds new parameters and updates existing ones
    import(&mut app, "Name,Unit,Expression,Value,Comments\nh,mm,7,7,height\nw,mm,12,12,wider\n");
    assert_eq!(app.s.model.params.get("h"), Some(&7.0));
    assert_eq!(app.s.model.params.get("w"), Some(&12.0));
    let w = app.s.design.parameters.iter().find(|p| p.name == "w").unwrap();
    assert_eq!(w.comment, "wider");
  }

  #[test]
  fn favourites_show_on_rows() {
    let mut s = session();
    let e = model_groups(&s)[1].feature;
    s.execute("param.favorite", &json!({"feature": e.0, "index": 0})).unwrap();
    s.execute("param.favorite", &json!({"name": "w"})).unwrap();
    assert!(user_rows(&s)[0].favorite);
    assert!(model_groups(&s)[1].rows[0].favorite);
    assert!(!model_groups(&s)[0].rows[0].favorite);
  }
}
