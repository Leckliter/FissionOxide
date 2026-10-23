//! The Electronics pages whose rows aren't a label and a control: Color's swatch grid, Misc's two
//! columns, Default Grid and Units' size rows, Default Values' tables and Drill Legend's symbol
//! table. FissionOxide has no electronics workspace, so every control is greyed at Fusion's
//! default.

use egui::{Color32, RichText, Sense, Stroke, vec2};

use super::{COMBO_W, FIELD_W, NOT_YET, ROW_H, fixed_combo, fixed_field, link, paint_spin_arrows};
use crate::theme;

/// A greyed check box with its label at its right.
fn check(ui: &mut egui::Ui, label: &str, on: bool) {
  let mut on = on;
  ui.add_enabled(false, egui::Checkbox::new(&mut on, label)).on_disabled_hover_text(NOT_YET);
}

/// A greyed check box alone.
fn boxed(ui: &mut egui::Ui, on: bool) {
  let mut on = on;
  ui.add_enabled(false, egui::Checkbox::without_text(&mut on)).on_disabled_hover_text(NOT_YET);
}

/// A greyed button.
fn button(ui: &mut egui::Ui, text: &str) {
  ui.add_enabled(false, egui::Button::new(text)).on_disabled_hover_text(NOT_YET);
}

/// A greyed number box with up and down arrows showing `value`.
fn spin(ui: &mut egui::Ui, value: &str) {
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 0.0;
    fixed_field(ui, value, FIELD_W - 22.0, NOT_YET);
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, ROW_H), Sense::hover());
    paint_spin_arrows(ui.painter(), rect, theme::MUTED);
  });
}

/// A colour swatch as a small framed button; `hatched` draws Fusion's error-polygon fill.
fn swatch(ui: &mut egui::Ui, fill: Color32, hatched: bool) {
  let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 18.0), Sense::hover());
  let p = ui.painter();
  p.rect_filled(rect, 1.0, fill);
  if hatched {
    let clip = p.with_clip_rect(rect);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
      clip.line_segment(
        [egui::pos2(x, rect.bottom()), egui::pos2(x + rect.height(), rect.top())],
        Stroke::new(1.0, Color32::BLACK),
      );
      x += 5.0;
    }
  }
  p.rect_stroke(rect, 1.0, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  resp.on_hover_text(NOT_YET);
}

/// A bold group title.
fn group(ui: &mut egui::Ui, title: &str) {
  ui.add_space(4.0);
  ui.label(RichText::new(title).strong());
}

/// The Theme grid's columns.
pub(super) const COLOR_COLUMNS: [&str; 6] = ["Board", "Grid Dots", "Grid Lines", "Canvas", "Dimmed", "Indicator"];

/// The Theme grid's rows: Dark and Light, a swatch per column.
const THEMES: [(&str, [Color32; 6]); 2] = [
  (
    "Dark",
    [
      Color32::BLACK,
      Color32::from_gray(128),
      Color32::from_gray(24),
      Color32::from_gray(24),
      Color32::from_gray(100),
      Color32::from_rgb(140, 110, 20),
    ],
  ),
  (
    "Light",
    [
      Color32::WHITE,
      Color32::from_gray(70),
      Color32::from_gray(236),
      Color32::from_gray(210),
      Color32::from_gray(150),
      Color32::from_rgb(230, 200, 110),
    ],
  ),
];

/// Electronics > Color: the Theme swatch grid, Highlight and PCB Constraints and Routing.
pub(super) fn color(ui: &mut egui::Ui) {
  group(ui, "Theme");
  egui::Grid::new("prefs-elec-colors").num_columns(7).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
    ui.label("");
    for c in COLOR_COLUMNS {
      ui.label(c);
    }
    ui.end_row();
    for (name, colours) in THEMES {
      ui.label(name);
      for c in colours {
        swatch(ui, c, false);
      }
      ui.end_row();
    }
  });
  ui.horizontal(|ui| {
    button(ui, "Reset to default colors");
    link(ui, "Preview");
  });
  group(ui, "Highlight");
  ui.horizontal(|ui| {
    ui.label("Darker");
    let mut v = 50.0_f32;
    ui.add_enabled(false, egui::Slider::new(&mut v, 0.0..=100.0).show_value(false)).on_disabled_hover_text(NOT_YET);
    ui.label("Brighter");
    ui.label("50 %");
  });
  check(ui, "High Contrast", false);
  ui.horizontal(|ui| {
    ui.label("Hue:");
    let mut v = 0.0_f32;
    ui.add_enabled(false, egui::Slider::new(&mut v, 0.0..=360.0).show_value(false)).on_disabled_hover_text(NOT_YET);
  });
  button(ui, "Reset");
  group(ui, "PCB Constraints and Routing");
  check(ui, "Use different fill style for unfinished routes", true);
  ui.horizontal(|ui| {
    ui.label("Fill style of unfinished routes");
    swatch(ui, Color32::BLACK, false);
  });
  check(ui, "Live DRC", true);
  ui.horizontal(|ui| {
    ui.label("Fill style of error polygons");
    swatch(ui, Color32::WHITE, true);
  });
}

/// Misc's left column: check boxes with their defaults.
pub(super) const MISC_CHECKS: [(&str, bool); 14] = [
  ("Beep", true),
  ("Check connects", true),
  ("Optimizing", true),
  ("Ratsnest processes polygons", false),
  ("Multi-threaded copper pouring", false),
  ("Display pad names", true),
  ("Display signal names on pads", true),
  ("Display signal names on traces", true),
  ("Display via lengths", true),
  ("Display drills", true),
  ("Auto end net and bus", true),
  ("Auto set junction", true),
  ("Group command default on", true),
  ("Enable GPU rendering", true),
];

/// Misc's right column: label, value, unit.
const MISC_VALUES: [(&str, &str, &str); 7] = [
  ("Min. visible text size", "3", "pixel"),
  ("Min. visible grid size", "5", "pixel"),
  ("Catch factor", "5", "%"),
  ("Select factor", "2", "%"),
  ("Snap length", "20 mil", ""),
  ("Xref label format", "%F%N/%S.%C%R", ""),
  ("Xref part format", "/%S.%C%R", ""),
];

/// Electronics > Misc: check boxes at the left, values with their units at the right.
pub(super) fn misc(ui: &mut egui::Ui) {
  ui.horizontal_top(|ui| {
    ui.vertical(|ui| {
      ui.spacing_mut().item_spacing.y = 5.0;
      for (label, on) in MISC_CHECKS {
        check(ui, label, on);
      }
    });
    ui.add_space(24.0);
    egui::Grid::new("prefs-elec-misc").num_columns(3).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
      for (label, value, unit) in MISC_VALUES {
        ui.label(label);
        fixed_field(ui, value, FIELD_W + 20.0, NOT_YET);
        ui.label(unit);
        ui.end_row();
      }
    });
  });
}

/// Electronics > Default Grid and Units: schematic and PCB grids.
pub(super) fn grid(ui: &mut egui::Ui) {
  let groups = [
    ("Schematic & Symbols", ["Display Grid for Schematic", "Display Grid for Symbols"], ["0.1", "0.01"], "inch"),
    ("2D PCB & Footprints", ["Display Grid for 2D PCB", "Display Grid for Footprints"], ["50", "5"], "mil"),
  ];
  for (i, (title, shows, sizes, unit)) in groups.into_iter().enumerate() {
    group(ui, title);
    egui::Grid::new(("prefs-elec-grid", i)).num_columns(4).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
      for (label, on) in [(shows[0], false), (shows[1], true)] {
        ui.label(label);
        boxed(ui, on);
        ui.end_row();
      }
      for (label, size) in [("Size", Some(sizes[0])), ("Multiple", None), ("Alt", Some(sizes[1]))] {
        ui.label(label);
        match size {
          Some(size) => {
            spin(ui, size);
            fixed_combo(ui, &format!("elec-grid-{i}-{label}"), unit, NOT_YET);
            link(ui, "Finest");
          }
          None => fixed_field(ui, "1", FIELD_W - 8.0, NOT_YET),
        }
        ui.end_row();
      }
    });
  }
}

/// One of Default Values' tables: its title and values, the first being the default.
type ValueTable = (&'static str, &'static [&'static str]);

/// Default Values' groups and their tables, as far as Fusion's were recorded (each list runs
/// on in Fusion; Hole (NPTH)'s values weren't recorded).
pub(super) const DEFAULT_VALUES: &[(&str, &[ValueTable])] = &[
  ("Traces/Lines", &[("Width", &["6", "7.87402", "8"])]),
  ("VIAs", &[("Diameter", &["auto", "19.68504", "21.65354"]), ("Drill Sizes", &["13.77953", "15.74803", "17.71654"])]),
  ("Hole (NPTH)", &[("Drill Sizes", &[])]),
  ("PTH Pad", &[("Diameter", &["auto", "10", "12"]), ("Drill Sizes", &["23.62205", "25.59055", "27.55906"])]),
  ("SMD Pad", &[("Size", &["50 x 25", "50 x 26", "56 x 28"])]),
  ("Polygons", &[("Isolate", &["0", "10", "12"]), ("Spacing", &["50", "56", "66"])]),
  ("Miter", &[("Radius", &["0", "10", "12.5"])]),
  ("Text", &[("Size", &["70", "76", "86"])]),
  (
    "Dimension",
    &[
      ("Line Width", &["5.11811", "10", "10.23622"]),
      ("Extension Width", &["auto", "3.93701", "5"]),
      ("Extension Length", &["auto", "39.37008", "50"]),
    ],
  ),
];

/// A table's + and x under it and the reset arrow at its right, greyed.
fn table_buttons(ui: &mut egui::Ui) {
  ui.horizontal(|ui| {
    button(ui, "+");
    button(ui, "x");
    button(ui, "⟲");
  });
}

/// Electronics > Default Values: Design Layers, then each group's Default / Value / Unit tables.
pub(super) fn default_values(ui: &mut egui::Ui) {
  group(ui, "Design Layers");
  check(ui, "Display layer IDs", true);
  check(ui, "Display vias and pads in their respective routable layers", false);
  for (g, (title, tables)) in DEFAULT_VALUES.iter().enumerate() {
    group(ui, title);
    for (t, (name, values)) in tables.iter().enumerate() {
      ui.label(*name);
      egui::Frame::NONE.stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        egui::Grid::new(("prefs-elec-values", g, t)).num_columns(3).min_col_width(80.0).striped(true).show(ui, |ui| {
          for h in ["Default", "Value", "Unit"] {
            ui.label(RichText::new(h).strong());
          }
          ui.end_row();
          for (i, v) in values.iter().enumerate() {
            ui.add_enabled(false, egui::RadioButton::new(i == 0, "")).on_disabled_hover_text(NOT_YET);
            ui.label(*v);
            ui.label(if *v == "auto" { "" } else { "mil" });
            ui.end_row();
          }
        });
      });
      table_buttons(ui);
    }
    if *title == "SMD Pad" {
      ui.horizontal(|ui| {
        ui.label("Roundness");
        fixed_combo(ui, "elec-smd-roundness", "0 %", NOT_YET);
      });
    }
  }
}

/// Drill Legend's symbols: number, diameter in mm and in mil. Symbols 1 to 6 are drawn (a plus,
/// a cross, a square, a circle, an hourglass, a bowtie); the later ones weren't recorded.
pub(super) const DRILLS: [(u32, &str, &str); 12] = [
  (1, "0.2", "7.9"),
  (2, "0.25", "9.8"),
  (3, "0.3", "11.8"),
  (4, "0.35", "13.8"),
  (5, "0.4", "15.7"),
  (6, "0.45", "17.7"),
  (7, "0.5", "19.7"),
  (8, "0.55", "21.7"),
  (9, "0.6", "23.6"),
  (10, "0.65", "25.6"),
  (11, "0.7", "27.6"),
  (12, "0.75", "29.5"),
];

/// A drill symbol in Fusion's blue.
fn drill_glyph(ui: &mut egui::Ui, n: u32) {
  let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
  let p = ui.painter();
  let c = rect.center();
  let ink = Stroke::new(1.3, theme::ICON_BLUE);
  let r = 5.0;
  let (tl, tr, bl, br) = (c + vec2(-r, -r), c + vec2(r, -r), c + vec2(-r, r), c + vec2(r, r));
  match n {
    1 => {
      p.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], ink);
      p.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], ink);
    }
    2 => {
      p.line_segment([tl, br], ink);
      p.line_segment([tr, bl], ink);
    }
    3 => {
      p.rect_stroke(egui::Rect::from_center_size(c, vec2(2.0 * r, 2.0 * r)), 0.0, ink, egui::StrokeKind::Middle);
    }
    4 => {
      p.circle_stroke(c, r, ink);
    }
    5 => {
      p.add(egui::Shape::closed_line(vec![tl, tr, bl, br], ink));
    }
    6 => {
      p.add(egui::Shape::closed_line(vec![tl, br, tr, bl], ink));
    }
    _ => {}
  }
}

/// Electronics > Drill Legend: the symbol table at the left, Symbol Appearance at the right.
pub(super) fn drill_legend(ui: &mut egui::Ui) {
  ui.horizontal_top(|ui| {
    ui.vertical(|ui| {
      group(ui, "Symbol Assignments");
      egui::Frame::NONE.stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        egui::Grid::new("prefs-elec-drills").num_columns(4).min_col_width(50.0).striped(true).show(ui, |ui| {
          for h in ["#", "Symbol", "mm", "mil"] {
            ui.label(RichText::new(h).strong());
          }
          ui.end_row();
          for (n, mm, mil) in DRILLS {
            ui.label(n.to_string());
            drill_glyph(ui, n);
            ui.label(mm);
            ui.label(mil);
            ui.end_row();
          }
        });
      });
      table_buttons(ui);
    });
    ui.add_space(24.0);
    ui.vertical(|ui| {
      ui.set_width(COMBO_W);
      group(ui, "Symbol Appearance");
      egui::Grid::new("prefs-elec-drill-look").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
        for (label, value) in [("Diameter", "80 mil"), ("Width", "1 mil")] {
          ui.label(label);
          fixed_field(ui, value, FIELD_W - 20.0, NOT_YET);
          ui.end_row();
        }
      });
    });
  });
}
