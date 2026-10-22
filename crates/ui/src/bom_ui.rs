//! MANAGE ▸ Bill of Materials: Fusion's BILL OF MATERIALS palette, docked over the lower right
//! of the canvas rather than a command dialog (the ribbon button doesn't stay pressed). A "-" at
//! the left of its header folds it to the header, ">>" at the right tucks it into a tab at the
//! canvas's right edge. Under the header a toolbar (the Structured / Parts Only view, the Working
//! version, link, download, Column Settings and a kebab menu), then the table of components, and
//! Close. As in Fusion, a design never saved shows no BOM data, only an empty state.

use egui::{Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2, pos2, vec2};
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// The columns Column Settings starts with unticked.
pub(crate) const DEFAULT_HIDDEN: &[&str] = &["Material (Legacy)"];

/// Column Settings' GENERAL group, in Fusion's order.
pub(crate) const GENERAL: &[&str] =
  &["Thumbnail", "Description", "Revision/Version", "Part Number", "Quantity", "Material", "Material (Legacy)"];

/// Column Settings' MANAGE group.
pub(crate) const MANAGE: &[&str] = &["Item Number", "Lifecycle", "Revision", "State", "Change Order"];

/// The table's columns after the check box and Part Name (which always lead), as Fusion first
/// lays them out: the blank-headed thumbnail, then the version column with an icon for a header.
pub(crate) const ORDER: &[&str] = &[
  "Thumbnail",
  "Revision/Version",
  "Description",
  "Part Number",
  "Quantity",
  "Material",
  "Material (Legacy)",
  "Item Number",
  "Lifecycle",
  "Revision",
  "State",
  "Change Order",
];

const WIDTH: f32 = 450.0;
const HEIGHT: f32 = 370.0;
const HEADER: f32 = 30.0;
const TOOLBAR: f32 = 36.0;
const FOOTER: f32 = 40.0;
const ROW: f32 = 26.0;
/// The Part Name column, and the check box column before it.
const NAME_W: f32 = 150.0;
const CHECK_W: f32 = 28.0;

/// The first dropdown: the components as an indented structure, or each part once with its
/// quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum View {
  #[default]
  Structured,
  PartsOnly,
}

impl View {
  pub(crate) const ALL: [View; 2] = [View::Structured, View::PartsOnly];

  pub(crate) fn label(self) -> &'static str {
    match self {
      View::Structured => "Structured",
      View::PartsOnly => "Parts Only",
    }
  }

  /// `assembly.bom`'s view.
  fn key(self) -> &'static str {
    match self {
      View::Structured => "indented",
      View::PartsOnly => "parts",
    }
  }
}

/// The second dropdown's versions: only Working can be picked (Released and Pending come from
/// a Manage extension FissionOxide doesn't have, and are greyed as in Fusion's).
pub(crate) const VERSIONS: &[(&str, bool)] = &[("Working", true), ("Released", false), ("Pending", false)];

/// The open palette.
#[derive(Debug, Clone, Default)]
pub(crate) struct Bom {
  pub view: View,
  /// Folded to its header by the "-".
  pub folded: bool,
  /// Tucked into its tab at the canvas's right edge by ">>".
  pub tucked: bool,
  /// `assembly.bom`'s rows: fetched on open, on a view change and by Refresh BOM.
  pub rows: Vec<Value>,
  /// Each row's check box.
  pub checked: Vec<bool>,
  /// Column Settings' groups folded shut.
  pub general_folded: bool,
  pub manage_folded: bool,
  pub error: Option<String>,
  /// The header being dragged to another place (Column Settings' Reset Order puts it back).
  dragging: Option<String>,
}

/// Opens the palette (or brings back a folded or tucked one) with the design's rows.
pub(crate) fn open(app: &mut App) {
  let mut b = app.bom.take().unwrap_or_default();
  b.folded = false;
  b.tucked = false;
  refresh(app, &mut b);
  app.bom = Some(b);
}

/// Fetches the rows for the palette's view.
pub(crate) fn refresh(app: &mut App, b: &mut Bom) {
  match app.s.execute("assembly.bom", &json!({"view": b.view.key()})) {
    Ok(v) => {
      b.rows = v.as_array().cloned().unwrap_or_default();
      b.error = None;
    }
    Err(e) => {
      b.rows.clear();
      b.error = Some(e.to_string());
    }
  }
  b.checked = vec![false; b.rows.len()];
}

/// Picks the Structured / Parts Only view.
pub(crate) fn set_view(app: &mut App, b: &mut Bom, view: View) {
  if b.view != view {
    b.view = view;
    refresh(app, b);
  }
}

/// Whether the table has data to show: Fusion shows a BOM only for a design saved at least once.
pub(crate) fn has_data(app: &App, b: &Bom) -> bool {
  !panels::is_unsaved(app) && !b.rows.is_empty()
}

/// The columns shown, in order (after the check box and Part Name).
pub(crate) fn columns(prefs: &crate::preferences::Prefs) -> Vec<&'static str> {
  ordered(prefs).into_iter().filter(|c| !prefs.bom_hidden.iter().any(|h| h == c)).collect()
}

/// Every column in the user's order: the saved order's known names, then the rest in Fusion's.
fn ordered(prefs: &crate::preferences::Prefs) -> Vec<&'static str> {
  let mut out: Vec<&'static str> =
    prefs.bom_order.iter().filter_map(|n| ORDER.iter().copied().find(|c| c == n)).collect();
  out.dedup();
  for c in ORDER {
    if !out.contains(c) {
      out.push(c);
    }
  }
  out
}

/// Ticks or unticks a column in Column Settings; saved with the preferences.
pub(crate) fn set_column(app: &mut App, column: &str, shown: bool) {
  let hidden = &mut app.preferences.values.bom_hidden;
  hidden.retain(|h| h != column);
  if !shown {
    hidden.push(column.to_string());
  }
  save_columns(app);
}

/// Moves a column (a header dragged onto another's place).
pub(crate) fn move_column(app: &mut App, column: &str, to: &str) {
  let mut order = ordered(&app.preferences.values);
  let (Some(from), Some(at)) = (order.iter().position(|c| *c == column), order.iter().position(|c| *c == to)) else {
    return;
  };
  let c = order.remove(from);
  order.insert(at, c);
  app.preferences.values.bom_order = order.iter().map(|c| c.to_string()).collect();
  save_columns(app);
}

/// Column Settings' Reset Order: the columns back in Fusion's order (the ticks stay).
pub(crate) fn reset_order(app: &mut App) {
  app.preferences.values.bom_order.clear();
  save_columns(app);
}

/// The column settings live in the preferences; the Preferences dialog's draft follows them so
/// its OK doesn't put them back.
fn save_columns(app: &mut App) {
  app.preferences.draft.bom_hidden = app.preferences.values.bom_hidden.clone();
  app.preferences.draft.bom_order = app.preferences.values.bom_order.clone();
  app.preferences.save();
}

/// A row's text in a column ("" for the columns FissionOxide has no data for: Fusion's Manage
/// extension fills them).
pub(crate) fn cell(app: &App, row: &Value, column: &str) -> String {
  let s = |k: &str| row.get(k).and_then(Value::as_str).unwrap_or("").to_string();
  match column {
    "Description" => s("description"),
    "Part Number" => {
      let n = s("part_number");
      if n.is_empty() { s("name") } else { n }
    }
    "Quantity" => row.get("quantity").and_then(Value::as_u64).unwrap_or(1).to_string(),
    "Material" | "Material (Legacy)" => s("material"),
    "Revision/Version" => format!("v{}", app.s.hub.as_ref().map(|h| h.version).unwrap_or(1)),
    _ => String::new(),
  }
}

/// The table as CSV (the download button): Part Name and the shown text columns.
pub(crate) fn csv(app: &App, b: &Bom) -> String {
  let cols: Vec<&str> = columns(&app.preferences.values).into_iter().filter(|c| *c != "Thumbnail").collect();
  let field = |t: &str| {
    if t.contains([',', '"', '\n']) { format!("\"{}\"", t.replace('"', "\"\"")) } else { t.to_string() }
  };
  let mut out = std::iter::once("Part Name").chain(cols.iter().copied()).map(field).collect::<Vec<_>>().join(",");
  out.push('\n');
  for r in &b.rows {
    let name = r.get("name").and_then(Value::as_str).unwrap_or("");
    let line: Vec<String> =
      std::iter::once(field(name)).chain(cols.iter().map(|c| field(&cell(app, r, c)))).collect();
    out.push_str(&line.join(","));
    out.push('\n');
  }
  out
}

/// The palette's place: docked at the canvas's lower right.
pub(crate) fn dock(canvas: Rect) -> Pos2 {
  canvas.right_bottom() + vec2(-10.0, -10.0)
}

/// The tab the palette tucks into: at the canvas's right edge, below the Sketch Palette's.
pub(crate) fn tab_rect(canvas: Rect) -> Rect {
  let top = crate::palette::tab_rect(canvas).bottom() + 8.0;
  Rect::from_min_max(pos2(canvas.right() - 26.0, top), pos2(canvas.right() - 2.0, top + 196.0))
}

/// Draws the palette while it is open.
pub(crate) fn show(app: &mut App, ctx: &egui::Context) {
  let Some(mut b) = app.bom.take() else { return };
  let keep = if b.tucked {
    tab(&mut b, app.canvas, ctx);
    true
  } else {
    palette(app, &mut b, ctx)
  };
  if keep && app.bom.is_none() {
    app.bom = Some(b);
  }
}

/// The vertical "BILL OF MATERIALS" tab; a click opens the palette again.
fn tab(b: &mut Bom, canvas: Rect, ctx: &egui::Context) {
  let rect = tab_rect(canvas);
  egui::Area::new(egui::Id::new("bom-palette-tab")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
    let (rect, resp) = ui.allocate_exact_size(rect.size(), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, if resp.hovered() { theme::HOVER } else { theme::FLOAT_HEADER });
    let ink = theme::GROUP_LABEL;
    for dy in [-3.0, 3.0] {
      let t = rect.center_top() + vec2(dy, 14.0);
      p.add(egui::Shape::convex_polygon(
        vec![t + vec2(-3.0, 0.0), t + vec2(2.0, -4.0), t + vec2(2.0, 4.0)],
        ink,
        Stroke::NONE,
      ));
    }
    let galley = p.layout_no_wrap("BILL OF MATERIALS".into(), FontId::proportional(12.0), ink);
    let pos = pos2(rect.center().x - galley.size().y / 2.0, rect.top() + 36.0 + galley.size().x);
    p.add(egui::epaint::TextShape::new(pos, galley, ink).with_angle(-std::f32::consts::FRAC_PI_2));
    if resp.clicked() {
      b.tucked = false;
    }
  });
}

/// The palette; false once Close is pressed.
fn palette(app: &mut App, b: &mut Bom, ctx: &egui::Context) -> bool {
  let mut keep = true;
  egui::Area::new(egui::Id::new("bom-palette"))
    .fixed_pos(dock(app.canvas))
    .pivot(egui::Align2::RIGHT_BOTTOM)
    .constrain(true)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        header(ui, b);
        if b.folded {
          return;
        }
        toolbar(app, ui, b);
        let body = HEIGHT - HEADER - TOOLBAR - FOOTER;
        let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, body), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        if has_data(app, b) {
          table(app, &mut inner, b);
        } else {
          empty_state(&mut inner, rect, app, b);
        }
        let sep = ui.allocate_exact_size(vec2(WIDTH, 1.0), Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.set_width(WIDTH - 20.0);
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if close_button(ui).clicked() {
              keep = false;
            }
          });
        });
      });
    });
  keep
}

/// The header: the "-" that folds the palette (a "+" while folded), the title, and ">>".
fn header(ui: &mut egui::Ui, b: &mut Bom) {
  let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, HEADER), Sense::hover());
  let corners =
    if b.folded { egui::CornerRadius::same(2) } else { egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 } };
  ui.painter().rect_filled(rect, corners, theme::DIALOG_HEADER);
  if !b.folded {
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
  }
  let ink = theme::GROUP_LABEL;
  let fold = Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), Vec2::splat(18.0));
  let r = ui.interact(fold, ui.id().with("bom-fold"), Sense::click());
  let p = ui.painter();
  if r.hovered() {
    p.rect_filled(fold, 2.0, theme::HOVER);
  }
  p.line_segment([fold.center() - vec2(5.0, 0.0), fold.center() + vec2(5.0, 0.0)], Stroke::new(1.5, ink));
  if b.folded {
    p.line_segment([fold.center() - vec2(0.0, 5.0), fold.center() + vec2(0.0, 5.0)], Stroke::new(1.5, ink));
  }
  if r.clicked() {
    b.folded = !b.folded;
  }
  p.text(rect.left_center() + vec2(32.0, 0.0), egui::Align2::LEFT_CENTER, "BILL OF MATERIALS", FontId::proportional(12.5), ink);
  let tuck = Rect::from_min_max(pos2(rect.right() - 24.0, rect.top() + 4.0), rect.right_bottom() - vec2(2.0, 4.0));
  let r = ui.interact(tuck, ui.id().with("bom-tuck"), Sense::click());
  if r.hovered() {
    ui.painter().rect_filled(tuck, 2.0, theme::HOVER);
  }
  for dx in [-14.0, -8.0] {
    let t = rect.right_center() + vec2(dx, 0.0);
    ui.painter().add(egui::Shape::convex_polygon(
      vec![t + vec2(-2.0, -4.0), t + vec2(3.0, 0.0), t + vec2(-2.0, 4.0)],
      ink,
      Stroke::NONE,
    ));
  }
  if r.clicked() {
    b.tucked = true;
  }
}

/// The toolbar row: the two dropdowns at the left; link, download, Column Settings and the
/// kebab at the right.
fn toolbar(app: &mut App, ui: &mut egui::Ui, b: &mut Bom) {
  let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, TOOLBAR), Sense::hover());
  let mut row = ui.new_child(
    egui::UiBuilder::new()
      .max_rect(rect.shrink2(vec2(10.0, 5.0)))
      .layout(egui::Layout::left_to_right(egui::Align::Center)),
  );
  row.spacing_mut().item_spacing = vec2(8.0, 0.0);
  let data = has_data(app, b);
  let view = dropdown_field(&mut row, b.view.label(), 112.0);
  egui::Popup::menu(&view).id(egui::Id::new("bom-view-menu")).show(|ui| {
    for v in View::ALL {
      if tick_item(ui, v.label(), b.view == v, true).clicked() {
        set_view(app, b, v);
      }
    }
  });
  let version = dropdown_field(&mut row, "Working", 100.0);
  egui::Popup::menu(&version).id(egui::Id::new("bom-version-menu")).show(|ui| {
    for (v, enabled) in VERSIONS {
      tick_item(ui, v, *v == "Working", *enabled);
    }
  });
  row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
    let kebab = glyph_button(ui, Glyph::Kebab, true, "More");
    egui::Popup::menu(&kebab).id(egui::Id::new("bom-more-menu")).show(|ui| {
      if menu_item(ui, Some(Glyph::Refresh), "Refresh BOM", data).clicked() {
        refresh(app, b);
      }
      // comparing needs the saved BOM's own record, which FissionOxide doesn't keep
      menu_item(ui, None, "Compare to Model", false);
    });
    let gear = glyph_button(ui, Glyph::Gear, true, "Column Settings");
    egui::Popup::from_toggle_button_response(&gear)
      .id(egui::Id::new("bom-columns"))
      .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
      .show(|ui| column_settings(app, ui, b));
    if glyph_button(ui, Glyph::Download, data, "Export").clicked() {
      export(app, b);
    }
    // a web link to the BOM: FissionOxide's designs have no web page
    glyph_button(ui, Glyph::Link, false, "Copy Link");
  });
}

/// The download button: the table as a CSV file.
fn export(app: &mut App, b: &Bom) {
  let name = format!("{} BOM.csv", crate::panels::doc_name(app));
  if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(name).save_file() {
    match std::fs::write(&path, csv(app, b)) {
      Ok(()) => app.status = Some((format!("Exported {}", path.display()), false)),
      Err(e) => app.status = Some((format!("Couldn't write {}: {e}", path.display()), true)),
    }
  }
}

/// The Column Settings popover: Reset Order, then the GENERAL and MANAGE groups of check boxes.
fn column_settings(app: &mut App, ui: &mut egui::Ui, b: &mut Bom) {
  ui.set_width(200.0);
  ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
  ui.label(RichText::new("Column Settings").strong());
  if panels::secondary_button(ui, "Reset Order").clicked() {
    reset_order(app);
  }
  ui.add_space(2.0);
  for (title, list, folded) in [("GENERAL", GENERAL, &mut b.general_folded), ("MANAGE", MANAGE, &mut b.manage_folded)] {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
      p.rect_filled(r, 2.0, theme::HOVER);
    }
    let t = r.left_center() + vec2(7.0, 0.0);
    let tri = if *folded {
      vec![t + vec2(-2.0, -4.0), t + vec2(3.0, 0.0), t + vec2(-2.0, 4.0)]
    } else {
      vec![t + vec2(-4.0, -2.0), t + vec2(4.0, -2.0), t + vec2(0.0, 3.0)]
    };
    p.add(egui::Shape::convex_polygon(tri, theme::GROUP_LABEL, Stroke::NONE));
    p.text(r.left_center() + vec2(18.0, 0.0), egui::Align2::LEFT_CENTER, title, FontId::proportional(11.5), theme::GROUP_LABEL);
    if resp.clicked() {
      *folded = !*folded;
    }
    if *folded {
      continue;
    }
    for c in list {
      let mut on = !app.preferences.values.bom_hidden.iter().any(|h| h == c);
      ui.horizontal(|ui| {
        ui.add_space(14.0);
        if ui.checkbox(&mut on, *c).changed() {
          set_column(app, c, on);
        }
      });
    }
  }
}

/// No data: the filing cabinet and Fusion's two lines.
fn empty_state(ui: &mut egui::Ui, rect: Rect, app: &App, b: &Bom) {
  let c = rect.center() - vec2(0.0, 34.0);
  filing_cabinet(ui.painter(), c);
  let (title, line) = if panels::is_unsaved(app) {
    (
      "Can't show bill of materials data for this design.",
      "Design must be saved at least once for BOM data to be available.".to_string(),
    )
  } else {
    ("No components to list.", b.error.clone().unwrap_or_else(|| "Create components to build the bill of materials.".into()))
  };
  let p = ui.painter();
  p.text(c + vec2(0.0, 56.0), egui::Align2::CENTER_CENTER, title, FontId::proportional(13.0), theme::TEXT);
  // bold, as Fusion's first line: drawn twice a hair apart
  p.text(c + vec2(0.4, 56.0), egui::Align2::CENTER_CENTER, title, FontId::proportional(13.0), theme::TEXT);
  p.text(c + vec2(0.0, 78.0), egui::Align2::CENTER_CENTER, line, FontId::proportional(12.0), theme::MUTED);
}

/// The empty state's illustration: a two-drawer filing cabinet.
fn filing_cabinet(p: &egui::Painter, c: Pos2) {
  let body = Rect::from_center_size(c, vec2(54.0, 66.0));
  p.rect(body, 3.0, theme::ILLUSTRATION, Stroke::new(1.5, theme::ILLUSTRATION_FRAME), egui::StrokeKind::Inside);
  for k in 0..2 {
    let d = Rect::from_min_size(body.min + vec2(6.0, 6.0 + k as f32 * 29.0), vec2(42.0, 25.0));
    p.rect(d, 2.0, Color32::WHITE, Stroke::new(1.2, theme::ILLUSTRATION_FRAME), egui::StrokeKind::Inside);
    let h = Rect::from_center_size(d.center() - vec2(0.0, 4.0), vec2(14.0, 4.0));
    p.rect_filled(h, 1.5, theme::ILLUSTRATION_FRAME);
  }
}

/// The width of a column.
fn col_width(column: &str) -> f32 {
  match column {
    "Thumbnail" => 34.0,
    "Revision/Version" => 34.0,
    "Quantity" => 70.0,
    "Description" | "Material" | "Material (Legacy)" => 130.0,
    _ => 110.0,
  }
}

/// The table: a header row, then a row per component, scrolling both ways.
fn table(app: &mut App, ui: &mut egui::Ui, b: &mut Bom) {
  let cols = columns(&app.preferences.values);
  let total = CHECK_W + NAME_W + cols.iter().map(|c| col_width(c)).sum::<f32>();
  egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    // the header row
    let (hr, _) = ui.allocate_exact_size(vec2(total.max(WIDTH), ROW), Sense::hover());
    ui.painter().rect_filled(hr, 0.0, theme::DIALOG_BODY);
    ui.painter().line_segment([hr.left_bottom(), hr.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
    let all = !b.checked.is_empty() && b.checked.iter().all(|c| *c);
    if check_box(ui, hr.left_center() + vec2(CHECK_W / 2.0, 0.0), all, "bom-all").clicked() {
      b.checked.iter_mut().for_each(|c| *c = !all);
    }
    let mut x = hr.left() + CHECK_W;
    head_text(ui, pos2(x + 6.0, hr.center().y), "Part Name");
    x += NAME_W;
    let mut drop_on: Option<&'static str> = None;
    for c in &cols {
      let r = Rect::from_min_size(pos2(x, hr.top()), vec2(col_width(c), ROW));
      let resp = ui.interact(r, ui.id().with(("bom-col", *c)), Sense::drag());
      if resp.drag_started() {
        b.dragging = Some(c.to_string());
      }
      if b.dragging.as_deref().is_some_and(|d| d != *c)
        && ui.ctx().pointer_hover_pos().is_some_and(|p| r.contains(p))
      {
        ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(2.0, theme::ACCENT));
        drop_on = Some(*c);
      }
      ui.painter().line_segment([r.left_top() + vec2(0.0, 5.0), r.left_bottom() - vec2(0.0, 5.0)], Stroke::new(1.0, theme::SEPARATOR));
      match *c {
        "Thumbnail" => {}
        "Revision/Version" => version_glyph(ui.painter(), r.center()),
        _ => head_text(ui, pos2(x + 6.0, hr.center().y), c),
      }
      x += col_width(c);
    }
    if ui.ctx().input(|i| i.pointer.any_released())
      && let Some(d) = b.dragging.take()
      && let Some(to) = drop_on
    {
      move_column(app, &d, to);
    }
    // the rows
    let structured = b.view == View::Structured;
    for (i, row) in b.rows.iter().enumerate() {
      let (rr, resp) = ui.allocate_exact_size(vec2(total.max(WIDTH), ROW), Sense::hover());
      if resp.hovered() {
        ui.painter().rect_filled(rr, 0.0, theme::HOVER);
      }
      ui.painter().line_segment([rr.left_bottom(), rr.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
      if let Some(on) = b.checked.get_mut(i)
        && check_box(ui, rr.left_center() + vec2(CHECK_W / 2.0, 0.0), *on, ("bom-row", i)).clicked()
      {
        *on = !*on;
      }
      let level = if structured { row.get("level").and_then(Value::as_u64).unwrap_or(0) as f32 } else { 0.0 };
      let name = row.get("name").and_then(Value::as_str).unwrap_or("");
      let x0 = rr.left() + CHECK_W;
      let name_rect = Rect::from_min_size(pos2(x0, rr.top()), vec2(NAME_W, ROW));
      ui.painter().with_clip_rect(name_rect).text(
        pos2(x0 + 6.0 + level * 14.0, rr.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        FontId::proportional(12.5),
        theme::TEXT,
      );
      let mut x = x0 + NAME_W;
      for c in &cols {
        let r = Rect::from_min_size(pos2(x, rr.top()), vec2(col_width(c), ROW));
        if *c == "Thumbnail" {
          let m = row.get("material").and_then(Value::as_str).unwrap_or("Steel");
          crate::material_ui::ball(ui.painter(), r.center(), 8.0, crate::material_ui::material_color(m));
        } else {
          ui.painter().with_clip_rect(r.shrink2(vec2(4.0, 0.0))).text(
            pos2(x + 6.0, rr.center().y),
            egui::Align2::LEFT_CENTER,
            cell(app, row, c),
            FontId::proportional(12.5),
            theme::TEXT,
          );
        }
        x += col_width(c);
      }
    }
  });
  if !ui.ctx().input(|i| i.pointer.any_down()) {
    b.dragging = None;
  }
}

fn head_text(ui: &egui::Ui, at: Pos2, text: &str) {
  ui.painter().text(at, egui::Align2::LEFT_CENTER, text, FontId::proportional(12.0), theme::GROUP_LABEL);
}

/// The Revision/Version column's header icon: a small sheet with a corner turned down.
fn version_glyph(p: &egui::Painter, c: Pos2) {
  let ink = theme::GROUP_LABEL;
  let r = Rect::from_center_size(c, vec2(10.0, 13.0));
  let pts = vec![r.left_top(), r.right_top() - vec2(3.0, 0.0), r.right_top() + vec2(0.0, 3.0), r.right_bottom(), r.left_bottom()];
  p.add(egui::Shape::closed_line(pts, Stroke::new(1.2, ink)));
  for k in 0..2 {
    let y = r.top() + 6.0 + k as f32 * 3.0;
    p.line_segment([pos2(r.left() + 2.5, y), pos2(r.right() - 2.5, y)], Stroke::new(1.0, ink));
  }
}

/// A table check box drawn at `c`; returns its response.
fn check_box(ui: &mut egui::Ui, c: Pos2, on: bool, id: impl std::hash::Hash) -> egui::Response {
  let r = Rect::from_center_size(c, Vec2::splat(14.0));
  let resp = ui.interact(r, ui.id().with(id), Sense::click());
  let p = ui.painter();
  let stroke = if resp.hovered() { theme::ACCENT } else { theme::BORDER };
  if on {
    p.rect(r, 2.0, theme::ACCENT, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
    check_mark(p, c, Color32::WHITE);
  } else {
    p.rect(r, 2.0, Color32::WHITE, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  }
  resp
}

fn check_mark(p: &egui::Painter, c: Pos2, ink: Color32) {
  p.add(egui::Shape::line(vec![c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0), c + vec2(4.0, -3.0)], Stroke::new(1.6, ink)));
}

/// A dropdown field: the value in a white box with a chevron at its right; a click opens its
/// menu under it, another closes it.
fn dropdown_field(ui: &mut egui::Ui, value: &str, width: f32) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
  let p = ui.painter();
  let border = if resp.hovered() { theme::ACCENT } else { theme::BORDER };
  p.rect(rect, 2.0, Color32::WHITE, Stroke::new(1.0, border), egui::StrokeKind::Inside);
  p.text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, value, FontId::proportional(12.5), theme::TEXT);
  icons::chevron(p, rect.right_center() - vec2(11.0, 0.0), theme::GROUP_LABEL);
  resp
}

/// A dropdown menu's entry, with a tick when it is the current one; greyed when it can't be
/// picked.
fn tick_item(ui: &mut egui::Ui, label: &str, ticked: bool, enabled: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(130.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
  let p = ui.painter();
  if enabled && resp.hovered() {
    p.rect_filled(rect, 2.0, theme::HOVER);
  }
  let ink = if enabled { theme::TEXT } else { theme::MUTED };
  if ticked {
    check_mark(p, rect.left_center() + vec2(12.0, 0.0), ink);
  }
  p.text(rect.left_center() + vec2(26.0, 0.0), egui::Align2::LEFT_CENTER, label, FontId::proportional(12.5), ink);
  resp
}

/// The kebab menu's entry, with its icon (if any) at the left; greyed without data.
fn menu_item(ui: &mut egui::Ui, glyph: Option<Glyph>, label: &str, enabled: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(160.0, 26.0), if enabled { Sense::click() } else { Sense::hover() });
  let p = ui.painter();
  if enabled && resp.hovered() {
    p.rect_filled(rect, 2.0, theme::HOVER);
  }
  let ink = if enabled { theme::TEXT } else { theme::MUTED };
  if let Some(g) = glyph {
    draw_glyph(p, Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), Vec2::splat(14.0)), g, ink);
  }
  p.text(rect.left_center() + vec2(30.0, 0.0), egui::Align2::LEFT_CENTER, label, FontId::proportional(12.5), ink);
  resp
}

/// The toolbar's small icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Glyph {
  Link,
  Download,
  Gear,
  Kebab,
  Refresh,
}

/// A toolbar icon button; greyed (and not clickable) when disabled.
fn glyph_button(ui: &mut egui::Ui, g: Glyph, enabled: bool, hint: &str) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), if enabled { Sense::click() } else { Sense::hover() });
  if enabled && resp.hovered() {
    ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let ink = if enabled { theme::GROUP_LABEL } else { theme::BORDER };
  draw_glyph(ui.painter(), rect.shrink(5.0), g, ink);
  resp.on_hover_text(hint)
}

fn draw_glyph(p: &egui::Painter, r: Rect, g: Glyph, ink: Color32) {
  let s = Stroke::new(1.4, ink);
  let c = r.center();
  match g {
    Glyph::Link => {
      // two links of a chain, on the diagonal
      for o in [vec2(-2.5, 2.5), vec2(2.5, -2.5)] {
        let k = c + o;
        let d = vec2(2.5, -2.5);
        p.add(egui::Shape::closed_line(
          vec![k - d + vec2(-1.8, -1.8), k + d + vec2(-1.8, -1.8), k + d + vec2(1.8, 1.8), k - d + vec2(1.8, 1.8)],
          s,
        ));
      }
    }
    Glyph::Download => {
      p.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 2.0)], s);
      p.add(egui::Shape::line(vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], s));
      p.add(egui::Shape::line(vec![c + vec2(-6.0, 2.0), c + vec2(-6.0, 6.0), c + vec2(6.0, 6.0), c + vec2(6.0, 2.0)], s));
    }
    Glyph::Gear => icons::draw(p, r, Icon::Gear, ink),
    Glyph::Kebab => {
      for dy in [-4.5, 0.0, 4.5] {
        p.circle_filled(c + vec2(0.0, dy), 1.4, ink);
      }
    }
    Glyph::Refresh => {
      let pts: Vec<Pos2> = (0..=12)
        .map(|i| 0.5 + i as f32 / 12.0 * 4.6)
        .map(|a| c + vec2(a.cos(), a.sin()) * 5.0)
        .collect();
      let end = *pts.last().unwrap_or(&c);
      p.add(egui::Shape::line(pts, s));
      p.add(egui::Shape::convex_polygon(vec![end + vec2(-3.0, -1.0), end + vec2(2.5, -3.0), end + vec2(1.5, 2.5)], ink, Stroke::NONE));
    }
  }
}

/// Fusion's palette Close: white with a blue outline.
fn close_button(ui: &mut egui::Ui) -> egui::Response {
  ui.add(
    egui::Button::new(RichText::new("Close").color(theme::TEXT))
      .fill(Color32::WHITE)
      .stroke(Stroke::new(1.0, theme::ACCENT))
      .corner_radius(3)
      .min_size(vec2(72.0, 24.0)),
  )
}
