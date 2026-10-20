//! Change Parameters (Modify > Change Parameters), as in Fusion: one table of Favorites, User
//! Parameters and Model Parameters (every feature's and sketch's dimensions d1, d2, ... grouped
//! by feature), with a star, name, unit, expression, value and comment per row and a filter box.
//! Expressions are editable and change the model as they're committed.

use egui::{Color32, Pos2, Rect, RichText, Stroke, pos2, vec2};
use fox_engine::Session;
use fox_engine::model::{FeatureId, LengthUnit, ParamKind};
use serde_json::json;

use crate::App;
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
pub(crate) enum Source {
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
const DELETE: f32 = 770.0;
const END: f32 = 796.0;
const ROW_H: f32 = 24.0;

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
#[derive(Clone, Default)]
struct NewParam {
  name: String,
  /// 0 = the design's length unit, 1 = degrees, 2 = no unit.
  unit: usize,
  expr: String,
  comment: String,
  /// The name box has had the keyboard once.
  started: bool,
}

pub fn window(app: &mut App, ctx: &egui::Context) {
  let mut open = true;
  let center = app.canvas.center();
  egui::Window::new("Parameters")
    .id(egui::Id::new("change-parameters"))
    .open(&mut open)
    .collapsible(false)
    .resizable(true)
    .default_size(vec2(END + 24.0, 480.0))
    .default_pos(center)
    .pivot(egui::Align2::CENTER_CENTER)
    .show(ctx, |ui| contents(app, ui));
  if !open {
    app.show_params = false;
  }
}

fn contents(app: &mut App, ui: &mut egui::Ui) {
  let unit = app.s.design.units;
  let fid = egui::Id::new("params-filter");
  let mut filter = ui.data_mut(|d| d.get_temp::<String>(fid)).unwrap_or_default();
  let nid = egui::Id::new("params-new");
  let mut new = ui.data_mut(|d| d.get_temp::<NewParam>(nid));
  ui.horizontal(|ui| {
    if ui.button("＋ User Parameter").on_hover_text("Add a user parameter").clicked() && new.is_none() {
      new = Some(NewParam::default());
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      ui.add(egui::TextEdit::singleline(&mut filter).hint_text("🔍 Filter").desired_width(200.0));
    });
  });
  ui.add_space(4.0);
  let users: Vec<Row> = user_rows(&app.s).into_iter().filter(|r| matches(r, "", &filter)).collect();
  let groups: Vec<Group> = model_groups(&app.s)
    .into_iter()
    .map(|mut g| {
      let name = g.name.clone();
      g.rows.retain(|r| matches(r, &name, &filter));
      g
    })
    .filter(|g| !g.rows.is_empty())
    .collect();
  let favorites: Vec<Row> =
    users.iter().chain(groups.iter().flat_map(|g| g.rows.iter())).filter(|r| r.favorite).cloned().collect();
  let filtering = !filter.trim().is_empty();
  egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
    ui.set_min_width(END);
    ui.spacing_mut().item_spacing.y = 0.0;
    let t = Table { left: ui.max_rect().left(), unit };
    header(ui, &t);
    section(ui, "params-favorites", &format!("Favorites ({})", favorites.len()), true, filtering, |ui| {
      if favorites.is_empty() {
        ui.label(RichText::new("Star a parameter to keep it here.").small().color(theme::MUTED));
      }
      for r in &favorites {
        row(app, ui, &t, r, "fav");
      }
    });
    section(ui, "params-user", &format!("User Parameters ({})", users.len()), true, filtering, |ui| {
      for r in &users {
        row(app, ui, &t, r, "user");
      }
      if let Some(n) = new.as_mut()
        && new_row(app, ui, &t, n)
      {
        new = None;
      }
    });
    let n: usize = groups.iter().map(|g| g.rows.len()).sum();
    section(ui, "params-model", &format!("Model Parameters ({n})"), true, filtering, |ui| {
      for g in &groups {
        let id = format!("params-feature-{}", g.feature.0);
        section(ui, &id, &format!("{} ({})", g.name, g.rows.len()), false, filtering, |ui| {
          for r in &g.rows {
            row(app, ui, &t, r, "model");
          }
        });
      }
    });
  });
  ui.data_mut(|d| {
    d.insert_temp(fid, filter);
    match new {
      Some(n) => {
        d.insert_temp(nid, n);
      }
      None => d.remove::<NewParam>(nid),
    }
  });
}

/// A collapsible group; open while the filter is in use.
fn section(ui: &mut egui::Ui, id: &str, title: &str, open: bool, filtering: bool, body: impl FnOnce(&mut egui::Ui)) {
  let mut h = egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(id).default_open(open);
  if filtering {
    h = h.open(Some(true));
  }
  h.show(ui, body);
}

fn header(ui: &mut egui::Ui, t: &Table) {
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
}

/// One parameter row. `ctx` keeps a row's boxes apart where it shows twice (Favorites).
fn row(app: &mut App, ui: &mut egui::Ui, t: &Table, r: &Row, ctx: &str) {
  let (rect, _) =
    ui.allocate_exact_size(vec2((t.left + END - ui.cursor().left()).max(10.0), ROW_H), egui::Sense::hover());
  let rect = Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(t.left + END, rect.bottom()));
  let key = match r.source {
    Source::User => format!("u-{}", r.name),
    Source::Model { feature, index } => format!("m-{}-{index}", feature.0),
  };
  let id = egui::Id::new(("param-row", ctx, &key));
  if ui.rect_contains_pointer(rect) {
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
  // expression
  let shown = shown_expr(r, t.unit);
  let eid = id.with("expr");
  let mut buf = ui.data_mut(|d| d.get_temp::<String>(eid)).unwrap_or_else(|| shown.clone());
  let resp = ui.put(t.col(rect, EXPR, VALUE), egui::TextEdit::singleline(&mut buf));
  if resp.changed() {
    ui.data_mut(|d| d.insert_temp(eid, buf.clone()));
  }
  if resp.lost_focus() {
    if buf.trim() != shown.trim() && !buf.trim().is_empty() {
      let expr = typed_expr(r.kind, &buf, t.unit);
      match r.source {
        Source::User => app.run_background("param.set", json!({"name": r.name, "expr": expr})),
        Source::Model { feature, index } => {
          app.run("param.model_set", json!({"feature": feature.0, "index": index, "expr": expr}));
        }
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
    let resp = ui.put(t.col(rect, COMMENT, DELETE), egui::TextEdit::singleline(&mut c).hint_text("comment"));
    if resp.changed() {
      ui.data_mut(|d| d.insert_temp(cid, c.clone()));
    }
    if resp.lost_focus() {
      if c != r.comment {
        app.run("param.comment", json!({"name": r.name, "comment": c}));
      }
      ui.data_mut(|d| d.remove::<String>(cid));
    }
    let dr = Rect::from_center_size(pos2(t.left + DELETE + 10.0, rect.center().y), vec2(20.0, 20.0));
    if ui.put(dr, egui::Button::new("🗑").small().frame(false)).on_hover_text("Delete (only if unused)").clicked() {
      app.run("param.remove", json!({"name": r.name}));
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
  boxes.push(ui.put(t.col(rect, COMMENT, DELETE), egui::TextEdit::singleline(&mut n.comment).hint_text("comment")));
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

  #[test]
  fn the_dialog_draws_every_section() {
    let mut app = App::with_session(session());
    app.show_params = true;
    let ctx = egui::Context::default();
    let frame = |app: &mut App, f: &mut dyn FnMut(&egui::Context)| {
      let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
        ..Default::default()
      };
      let mut out = ctx.run_ui(raw, |ui| {
        f(ui.ctx());
        window(app, ui.ctx());
      });
      out.textures_delta.clear();
    };
    frame(&mut app, &mut |_| {});
    // a new parameter row and a filter
    frame(&mut app, &mut |c| {
      c.data_mut(|d| {
        d.insert_temp(egui::Id::new("params-new"), NewParam::default());
        d.insert_temp(egui::Id::new("params-filter"), "extrude".to_string());
      })
    });
    frame(&mut app, &mut |_| {});
    assert!(app.show_params);
    // the window sits over the canvas, not at the top left over the toolbar
    let r = ctx.memory(|m| m.area_rect(egui::Id::new("change-parameters"))).unwrap();
    assert!(r.left() > 50.0 && r.top() > 50.0, "{r:?}");
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
