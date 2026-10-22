//! Fusion's SETUP dialog (MILLING and TURNING > New Setup): its title names the setup
//! ("SETUP : SETUP1"), the Setup, Stock and Post Process tabs above the rows, the Setup tab's
//! Machine, Setup, Work Coordinate System (WCS), Model and Fixture headings, the Stock Point
//! picker and the Stock tab's read-only Stock Dimensions.

use fox_engine::model::LengthUnit;
use fox_engine::model::cam::Setup;

use super::{Dialog, Kind, section_header, sel_chip_lit};
use crate::forms::{self, Form, SETUP_PAGES};
use crate::{App, theme};

impl Dialog {
  /// The open New Setup's form, if this is one.
  pub(super) fn setup_form(&self) -> Option<&'static Form> {
    match self.kind {
      Kind::Form(f) if forms::is_setup(f) => Some(f),
      _ => None,
    }
  }

  /// Is New Setup open with its setup previewed in the design?
  pub(crate) fn setup_previewed(&self) -> bool {
    self.setup_form().is_some() && self.applied
  }

  /// The title of a New Setup: "SETUP : SETUP1", as Fusion's.
  pub(super) fn setup_title(&self) -> Option<String> {
    let f = self.setup_form()?;
    let i = f.inputs.iter().position(|x| x.key() == "name")?;
    let name = self.form.values[i].as_str().unwrap_or("").trim();
    Some(if name.is_empty() { f.title.to_string() } else { format!("{} : {}", f.title, name.to_uppercase()) })
  }

  /// New Setup's Setup, Stock and Post Process tabs, above the rows.
  pub(super) fn setup_tabs(&mut self, ui: &mut egui::Ui) {
    if self.setup_form().is_none() {
      return;
    }
    let accent = self.accent();
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 14.0;
      for (page, name) in SETUP_PAGES.iter().enumerate() {
        let on = self.form.page == page;
        let text = egui::RichText::new(*name).color(if on { theme::TAB_ACTIVE } else { theme::MUTED });
        let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        if on {
          let r = resp.rect;
          ui.painter().line_segment(
            [egui::pos2(r.left(), r.bottom() + 3.0), egui::pos2(r.right(), r.bottom() + 3.0)],
            egui::Stroke::new(2.0, accent),
          );
        }
        if resp.clicked() {
          self.form.page = page;
        }
      }
    });
    ui.add_space(10.0);
  }

  /// A New Setup row's place: the heading it starts (drawn here), and whether the row is drawn
  /// (on the tab showing, under an unfolded heading). The Stock Point picker is drawn here.
  /// True when the row is done with.
  pub(super) fn setup_field(&mut self, f: &'static Form, i: usize, ui: &mut egui::Ui) -> bool {
    if !forms::is_setup(f) {
      return false;
    }
    let key = f.inputs[i].key();
    if forms::setup_page(key) != self.form.page {
      return true;
    }
    let turning = forms::setup_turning(f, &self.form);
    if let Some(title) = forms::setup_section(key, turning) {
      self.form.section_open = section_header(ui, egui::Id::new(("setup-section", title)), title);
    }
    if !self.form.section_open {
      return true;
    }
    if key == "stock_point" {
      // Fusion's box point picker: the stock box's corner the WCS sits on
      ui.label("Stock Point");
      sel_chip_lit(ui, self.accent(), "Box Point", true, false)
        .on_hover_text("The WCS sits on the stock box's top corner at its least X and Y");
      ui.end_row();
      return true;
    }
    false
  }

  /// The rows' first heading starts unfolded.
  pub(super) fn setup_rows_begin(&mut self) {
    self.form.section_open = true;
  }

  /// The Stock tab's read-only Stock Dimensions (milling): the stock box's size.
  pub(super) fn setup_stock_dimensions(&mut self, app: &App, ui: &mut egui::Ui) {
    let Some(f) = self.setup_form() else { return };
    if self.form.page != 1 || forms::setup_turning(f, &self.form) {
      return;
    }
    let title = "Stock Dimensions";
    if !section_header(ui, egui::Id::new(("setup-section", title)), title) {
      return;
    }
    let size = if self.applied { app.s.design.cam.last().and_then(|st| stock_size(st, app)) } else { None };
    let unit = app.s.design.units;
    for (k, label) in ["Width (X)", "Depth (Y)", "Height (Z)"].iter().enumerate() {
      ui.label(*label);
      let text = size.map_or_else(|| "-".to_string(), |v| length_text(v[k], unit));
      ui.add_enabled_ui(false, |ui| ui.label(text));
      ui.end_row();
    }
  }
}

/// The stock box's size along X, Y and Z (mm).
fn stock_size(st: &Setup, app: &App) -> Option<[f64; 3]> {
  let (lo, hi) = fox_engine::model::cam::stock(st, &app.s.model).ok()?;
  Some((hi - lo).to_array())
}

/// A length in the design's unit, as the read-only rows show it.
pub(crate) fn length_text(mm: f64, unit: LengthUnit) -> String {
  let digits = if matches!(unit, LengthUnit::In | LengthUnit::Ft) { 3 } else { 2 };
  format!("{:.*} {}", digits, mm / unit.mm(), unit.symbol())
}
