//! The Document Settings dialogs, docked at the right of the canvas as Fusion's command dialogs:
//! CHANGE ACTIVE UNITS (the Units row's menu) and CONVERT <DOCUMENT> (the Part Design row's
//! pencil and menu), and the edit pencil those rows show.
//!
//! FissionOxide's designs store a length unit only, so the mass half of a unit system follows
//! the length (`LengthUnit::mass_symbol`, as the Units row shows it) and is not picked apart. FissionOxide has one design type, Part, so Convert
//! Design Type lists Fusion's other types greyed and its Convert button stays greyed.

use egui::{Color32, RichText, Stroke, pos2, vec2};
use fox_engine::model::LengthUnit;
use serde_json::json;

use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// The open Document Settings dialog and what it has picked so far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocDialog {
  /// Change Active Units: the unit system picked.
  Units { unit: LengthUnit },
  /// Convert Design Type: the design type picked.
  Convert { kind: DesignType },
}

/// Fusion's design types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignType {
  Part,
  Assembly,
  Hybrid,
}

impl DesignType {
  pub const ALL: [DesignType; 3] = [DesignType::Part, DesignType::Assembly, DesignType::Hybrid];

  pub fn label(self) -> &'static str {
    match self {
      DesignType::Part => "Part",
      DesignType::Assembly => "Assembly",
      DesignType::Hybrid => "Hybrid",
    }
  }

  /// Whether FissionOxide can make a design of this type: part designs only.
  pub fn available(self) -> bool {
    self == DesignType::Part
  }
}

/// The dialog's area id.
pub const AREA: &str = "doc-dialog";
/// The status bar while Change Active Units is open, as Fusion's.
pub const UNITS_PROMPT: &str = "Units";
/// The status bar while Convert Design Type is open, as Fusion's.
pub const CONVERT_PROMPT: &str = "Design Experience";

/// A length unit as Fusion's Length drop-down shows it: "inch (in)".
pub fn length_label(u: LengthUnit) -> String {
  format!("{} ({})", u.label().to_lowercase(), u.symbol())
}

/// A unit system as Fusion's Unit System drop-down shows it: "inch (in), ouncemass".
pub fn system_label(u: LengthUnit) -> String {
  format!("{}, {}", length_label(u), u.mass_symbol())
}

/// Change Active Units: opens the dialog on the design's unit.
pub fn open_units(app: &mut App) {
  let unit = app.s.design.units;
  open(app, DocDialog::Units { unit }, UNITS_PROMPT);
}

/// Convert Design Type: opens the dialog on the design's type.
pub fn open_convert(app: &mut App) {
  open(app, DocDialog::Convert { kind: DesignType::Part }, CONVERT_PROMPT);
}

fn open(app: &mut App, d: DocDialog, prompt: &str) {
  // a command dialog that was open is cancelled, as starting any command does in Fusion
  app.cancel_dialog();
  close(app);
  app.doc_dialog = Some(d);
  app.hint(prompt);
}

/// Closes the dialog without applying it (Cancel, Escape).
pub fn close(app: &mut App) {
  if app.doc_dialog.take().is_some() {
    app.end_hint();
  }
}

pub fn units_open(app: &App) -> bool {
  matches!(app.doc_dialog, Some(DocDialog::Units { .. }))
}

pub fn convert_open(app: &App) -> bool {
  matches!(app.doc_dialog, Some(DocDialog::Convert { .. }))
}

/// Whether the dialog picked something other than what the design has: OK (or Convert) is
/// greyed until then.
pub fn changed(app: &App) -> bool {
  app.doc_dialog.is_some_and(|d| differs(app, d))
}

fn differs(app: &App, d: DocDialog) -> bool {
  match d {
    DocDialog::Units { unit } => unit != app.s.design.units,
    DocDialog::Convert { kind } => kind != DesignType::Part && kind.available(),
  }
}

/// OK: applies what the dialog picked and closes it.
pub fn confirm(app: &mut App) {
  if !changed(app) {
    return;
  }
  if let Some(DocDialog::Units { unit }) = app.doc_dialog {
    close(app);
    app.run("design.units", json!({"unit": unit.symbol()}));
  }
}

/// The dialog's title: "CHANGE ACTIVE UNITS", or "CONVERT UNTITLED" with the document's name.
pub fn title(app: &App, d: DocDialog) -> String {
  match d {
    DocDialog::Units { .. } => "CHANGE ACTIVE UNITS".into(),
    DocDialog::Convert { .. } => format!("CONVERT {}", panels::doc_name(app).to_uppercase()),
  }
}

/// Draws the open dialog, docked where command dialogs dock.
pub fn show(app: &mut App, ctx: &egui::Context) {
  let Some(start) = app.doc_dialog else { return };
  if app.dialog.is_some() {
    // a command started from elsewhere takes the dialog's place
    close(app);
    return;
  }
  let mut d = start;
  let (icon, ok_label) = match d {
    DocDialog::Units { .. } => (Icon::Measure, "OK"),
    DocDialog::Convert { .. } => (Icon::Pencil, "Convert"),
  };
  let title = title(app, d);
  let width = 300.0;
  let dock = panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  let (mut ok, mut cancel) = (false, false);
  egui::Area::new(egui::Id::new(AREA))
    .pivot(egui::Align2::RIGHT_TOP)
    .fixed_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, icon, &title, width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          egui::Grid::new("doc-dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(
            ui,
            |ui| match &mut d {
              DocDialog::Units { unit } => units_fields(ui, unit),
              DocDialog::Convert { kind } => convert_fields(ui, kind),
            },
          );
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if panels::secondary_button(ui, "Cancel").clicked() {
                cancel = true;
              }
              if ok_button(ui, ok_label, icons::accent_of(icon), differs(app, d)).clicked() {
                ok = true;
              }
            });
          });
        });
      });
    });
  if app.doc_dialog.is_none() {
    return;
  }
  app.doc_dialog = Some(d);
  if cancel {
    close(app);
  } else if ok {
    confirm(app);
  }
}

fn units_fields(ui: &mut egui::Ui, unit: &mut LengthUnit) {
  ui.label("Unit System");
  egui::ComboBox::from_id_salt("unit-system").selected_text(system_label(*unit)).width(170.0).show_ui(ui, |ui| {
    for u in LengthUnit::ALL {
      ui.selectable_value(unit, u, system_label(u));
    }
  });
  ui.end_row();
  // the length and mass follow the unit system
  for (label, value, id) in
    [("Length", length_label(*unit), "unit-length"), ("Mass", unit.mass_symbol().to_string(), "unit-mass")]
  {
    ui.add_enabled_ui(false, |ui| ui.label(label));
    ui.add_enabled_ui(false, |ui| {
      egui::ComboBox::from_id_salt(id).selected_text(value).width(170.0).show_ui(ui, |_| {});
    });
    ui.end_row();
  }
  ui.add_enabled_ui(false, |ui| ui.label("Set as Default"));
  let mut default = false;
  ui.add_enabled(false, egui::Checkbox::without_text(&mut default));
  ui.end_row();
}

fn convert_fields(ui: &mut egui::Ui, kind: &mut DesignType) {
  ui.label("Design Type");
  ui.horizontal(|ui| {
    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
    icons::draw_color(ui.painter(), r, Icon::Component);
    egui::ComboBox::from_id_salt("design-type").selected_text(kind.label()).width(148.0).show_ui(ui, |ui| {
      for t in DesignType::ALL {
        ui.add_enabled_ui(t.available(), |ui| ui.selectable_value(kind, t, t.label()))
          .response
          .on_disabled_hover_text("FissionOxide makes part designs only");
      }
    });
  });
  ui.end_row();
}

/// The dialog's OK (or Convert) button: blue when enabled, greyed until something changed.
pub(crate) fn ok_button(ui: &mut egui::Ui, label: &str, accent: Color32, enabled: bool) -> egui::Response {
  if enabled {
    return panels::primary_button(ui, label, accent);
  }
  ui.add_enabled(
    false,
    egui::Button::new(RichText::new(label).color(theme::MUTED))
      .fill(Color32::from_gray(240))
      .stroke(Stroke::new(1.0, theme::BORDER))
      .corner_radius(3)
      .min_size(vec2(72.0, 24.0)),
  )
}

/// Where the edit pencil of a browser row named `label` at `depth` sits: just right of its name
/// pill (laid out as `panels::tree_row` lays it out).
pub fn pencil_rect(ui: &egui::Ui, row: egui::Rect, depth: usize, label: &str) -> egui::Rect {
  let x = row.min.x + 8.0 + depth as f32 * 16.0 + 44.0;
  let label_w = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), theme::TEXT).size().x;
  egui::Rect::from_center_size(pos2(x + label_w + 36.0, row.center().y), vec2(20.0, 20.0))
}

/// A Document Settings row's edit pencil. It shows while the row's dialog is `active`, and with
/// `on_hover` also while the row is hovered (Part Design's, as in Fusion; the Units row's shows
/// only while Change Active Units is open). Returns whether it was clicked.
pub fn row_pencil(
  ui: &mut egui::Ui,
  row: &egui::Response,
  depth: usize,
  label: &str,
  id: egui::Id,
  on_hover: bool,
  active: bool,
  tip: &str,
) -> bool {
  if !on_hover && !active {
    return false;
  }
  let pr = pencil_rect(ui, row.rect, depth, label);
  let pencil = ui.interact(pr, id, egui::Sense::click());
  if row.hovered() || pencil.hovered() || active {
    let p = ui.painter();
    if pencil.hovered() || active {
      p.rect_filled(pr, 2.0, theme::HOVER);
    }
    paint_pencil(p, pr.shrink(4.0), if pencil.hovered() { theme::ACCENT } else { theme::GROUP_LABEL });
  }
  pencil.on_hover_text(tip).clicked()
}

/// A small pencil: a slanted body with a tip, as Fusion's edit glyph.
fn paint_pencil(p: &egui::Painter, r: egui::Rect, c: Color32) {
  let (tip, end) = (pos2(r.left() + 1.0, r.bottom() - 1.0), pos2(r.right() - 1.0, r.top() + 1.0));
  p.line_segment([tip + vec2(3.0, -3.0), end], Stroke::new(3.2, c));
  p.add(egui::Shape::convex_polygon(vec![tip, tip + vec2(1.2, -4.4), tip + vec2(4.4, -1.2)], c, Stroke::NONE));
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  #[test]
  fn unit_systems_read_as_fusions() {
    assert_eq!(system_label(LengthUnit::In), "inch (in), ouncemass");
    assert_eq!(length_label(LengthUnit::In), "inch (in)");
    assert_eq!(system_label(LengthUnit::Mm), "millimeter (mm), g");
  }

  #[test]
  fn change_active_units_applies_on_ok_only_after_a_change() {
    let mut app = App::with_session(Session::default());
    open_units(&mut app);
    assert!(units_open(&app));
    assert_eq!(app.status.as_ref().map(|s| s.0.as_str()), Some(UNITS_PROMPT), "the status bar reads Units");
    assert!(!changed(&app), "OK is greyed until the unit system changes");
    confirm(&mut app);
    assert!(units_open(&app), "a greyed OK does nothing");
    app.doc_dialog = Some(DocDialog::Units { unit: LengthUnit::In });
    assert!(changed(&app));
    confirm(&mut app);
    assert_eq!(app.s.design.units, LengthUnit::In);
    assert!(app.doc_dialog.is_none());
    assert!(app.status.as_ref().is_none_or(|s| s.0 != UNITS_PROMPT), "the prompt goes with it");
  }

  #[test]
  fn convert_design_type_offers_only_part() {
    let mut app = App::with_session(Session::default());
    open_convert(&mut app);
    let d = app.doc_dialog.expect("open");
    assert_eq!(title(&app, d), "CONVERT UNTITLED");
    assert_eq!(app.status.as_ref().map(|s| s.0.as_str()), Some(CONVERT_PROMPT));
    assert!(!changed(&app), "Convert is greyed");
    assert_eq!(DesignType::ALL.iter().filter(|t| t.available()).collect::<Vec<_>>(), [&DesignType::Part]);
    // the other dialog takes its place, with its own prompt
    open_units(&mut app);
    assert!(units_open(&app) && !convert_open(&app));
    assert_eq!(app.status.as_ref().map(|s| s.0.as_str()), Some(UNITS_PROMPT));
    close(&mut app);
    assert!(app.doc_dialog.is_none() && app.status.is_none());
  }
}
