//! File ▸ Capture Image... and File ▸ 3D Print, as Fusion's dialogs: Capture Image ("Image
//! Options") writes the canvas as a PNG at a chosen size (transparent background and
//! anti-aliasing optional); 3D Print (a docked "3D PRINT" dialog) goes to the Manufacture
//! workspace, or writes the bodies as a mesh file (STL, 3MF or OBJ) at a chosen refinement.

use egui::{RichText, vec2};
use fox_engine::render::{self, Frame};
use serde_json::{Value, json};

use crate::{App, theme};

/// Capture Image: the image's size and look.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageForm {
  /// Fusion's "Image Resolution: Current Document Window Size" (else custom width and height).
  pub window_size: bool,
  pub width: u32,
  pub height: u32,
  /// The size of the canvas now, which `window_size` follows.
  pub canvas: [u32; 2],
  pub transparent: bool,
  pub antialias: bool,
  /// The canvas's size in screen points, which the Width and Height fields show for the window
  /// size (W: and H: show the pixels).
  pub points: [u32; 2],
  /// The unit of the custom Width and Height.
  pub unit: ImageUnit,
  /// Resolution, in pixels per inch (converts inches and millimetres to pixels).
  pub ppi: f32,
  /// Lock Aspect Ratio: a custom width change keeps the height in proportion.
  pub lock: bool,
}

impl ImageForm {
  /// The size the image is written at.
  pub fn size(&self) -> [u32; 2] {
    if self.window_size { self.canvas } else { [self.width.max(1), self.height.max(1)] }
  }

  /// `px` pixels in the form's unit.
  pub fn in_unit(&self, px: u32) -> f64 {
    match self.unit.per_inch() {
      None => px as f64,
      Some(n) => px as f64 / self.ppi.max(1.0) as f64 * n,
    }
  }

  /// `v` in the form's unit, as whole pixels (1 to 8192).
  pub fn px_of(&self, v: f64) -> u32 {
    let px = match self.unit.per_inch() {
      None => v,
      Some(n) => v / n * self.ppi.max(1.0) as f64,
    };
    px.round().clamp(1.0, 8192.0) as u32
  }

  /// Sets the custom width, keeping the proportions when the aspect ratio is locked.
  pub fn set_width(&mut self, px: u32) {
    if self.lock && self.width > 0 {
      self.height = ((px as f64 * self.height as f64 / self.width as f64).round() as u32).clamp(1, 8192);
    }
    self.width = px;
  }

  /// Sets the custom height, keeping the proportions when the aspect ratio is locked.
  pub fn set_height(&mut self, px: u32) {
    if self.lock && self.height > 0 {
      self.width = ((px as f64 * self.width as f64 / self.height as f64).round() as u32).clamp(1, 8192);
    }
    self.height = px;
  }
}

/// Image Options: the unit of a custom size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageUnit {
  Pixels,
  Inches,
  Millimeters,
  Centimeters,
}

impl ImageUnit {
  pub const ALL: [ImageUnit; 4] =
    [ImageUnit::Pixels, ImageUnit::Inches, ImageUnit::Millimeters, ImageUnit::Centimeters];

  pub fn label(self) -> &'static str {
    match self {
      ImageUnit::Pixels => "Pixels",
      ImageUnit::Inches => "Inches",
      ImageUnit::Millimeters => "Millimeters",
      ImageUnit::Centimeters => "Centimeters",
    }
  }

  /// How many of the unit make an inch (None for pixels).
  fn per_inch(self) -> Option<f64> {
    match self {
      ImageUnit::Pixels => None,
      ImageUnit::Inches => Some(1.0),
      ImageUnit::Millimeters => Some(25.4),
      ImageUnit::Centimeters => Some(2.54),
    }
  }
}

/// 3D Print: Fusion's mesh refinement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refinement {
  Low,
  Medium,
  High,
}

impl Refinement {
  pub const ALL: [Refinement; 3] = [Refinement::Low, Refinement::Medium, Refinement::High];

  pub fn label(self) -> &'static str {
    match self {
      Refinement::Low => "Low",
      Refinement::Medium => "Medium",
      Refinement::High => "High",
    }
  }
}

/// 3D Print: the file format, as Fusion's Format list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintFormat {
  StlBinary,
  StlAscii,
  ThreeMf,
  Obj,
}

impl PrintFormat {
  pub const ALL: [PrintFormat; 4] =
    [PrintFormat::StlBinary, PrintFormat::StlAscii, PrintFormat::ThreeMf, PrintFormat::Obj];

  pub fn label(self) -> &'static str {
    match self {
      PrintFormat::StlBinary => "STL (Binary)",
      PrintFormat::StlAscii => "STL (ASCII)",
      PrintFormat::ThreeMf => "3MF",
      PrintFormat::Obj => "OBJ",
    }
  }

  /// The format's id in [`fox_engine::exchange::FORMATS`].
  pub fn id(self) -> &'static str {
    match self {
      PrintFormat::StlBinary | PrintFormat::StlAscii => "stl",
      PrintFormat::ThreeMf => "3mf",
      PrintFormat::Obj => "obj",
    }
  }
}

/// 3D Print: Fusion's Preparation Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preparation {
  /// An additive setup in the Manufacture workspace (Fusion's default).
  Manufacture,
  /// The bodies written as a mesh file.
  MeshExport,
}

impl Preparation {
  pub const ALL: [Preparation; 2] = [Preparation::Manufacture, Preparation::MeshExport];

  pub fn label(self) -> &'static str {
    match self {
      Preparation::Manufacture => "Manufacture",
      Preparation::MeshExport => "Mesh Export",
    }
  }
}

/// Fusion's text under Preparation Type: Manufacture.
pub const MANUFACTURE_NOTE: &str = "Create a new additive setup in the Manufacture workspace to prepare your models for 3D \
  printing, generate support structures, slice and export machine files such as G-Code or CLI.";

#[derive(Debug, Clone, PartialEq)]
pub struct PrintForm {
  pub preparation: Preparation,
  /// The bodies selected when the dialog opened; empty = every visible body.
  pub bodies: Vec<u32>,
  pub refinement: Refinement,
  pub format: PrintFormat,
}

#[derive(Debug, Default)]
pub struct Capture {
  pub image: Option<ImageForm>,
  pub print: Option<PrintForm>,
}

/// File ▸ Capture Image...
pub fn open_image(app: &mut App) {
  let points = [(app.canvas.width().round() as u32).max(1), (app.canvas.height().round() as u32).max(1)];
  let canvas = if app.canvas_px[0] > 0 && app.canvas_px[1] > 0 { app.canvas_px } else { points };
  app.capture.image = Some(ImageForm {
    window_size: true,
    width: canvas[0],
    height: canvas[1],
    canvas,
    transparent: false,
    antialias: true,
    points,
    unit: ImageUnit::Pixels,
    ppi: 72.0,
    lock: true,
  });
}

/// File ▸ 3D Print: its dialog takes the place of the command dialog that was open.
pub fn open_print(app: &mut App) {
  app.cancel_dialog();
  let bodies = app
    .s
    .selection
    .iter()
    .filter_map(|s| if let fox_engine::Sel::Body { body } = s { Some(body.0) } else { None })
    .collect();
  app.capture.print = Some(PrintForm {
    preparation: Preparation::Manufacture,
    bodies,
    refinement: Refinement::Medium,
    format: PrintFormat::StlBinary,
  });
}

/// The canvas rendered at `size`: the scene as the viewport shows it now. A transparent image
/// leaves the background (and grid) clear; anti-aliasing renders at twice the size and averages.
pub fn render_image(app: &mut App, form: &ImageForm) -> Frame {
  let [w, h] = form.size();
  let opts = crate::viewport::scene_options(app);
  let (mut scene, _) = app.scene.build(&app.s, &opts);
  scene.style = app.visual_style;
  app.effects.apply(&mut scene);
  let mut frame = if form.antialias {
    downsample(&render::render(&scene, &app.cam, w * 2, h * 2))
  } else {
    render::render(&scene, &app.cam, w, h)
  };
  if form.transparent {
    for (px, id) in frame.rgba.chunks_mut(4).zip(&frame.ids) {
      if *id == 0 {
        px.copy_from_slice(&[0, 0, 0, 0]);
      }
    }
  }
  frame
}

/// Halves a frame, averaging each 2×2 block (a pixel is "something" if any of the four is).
fn downsample(big: &Frame) -> Frame {
  let (w, h) = (big.width / 2, big.height / 2);
  let mut out = Frame::new(w, h);
  for y in 0..h {
    for x in 0..w {
      let mut sum = [0u32; 4];
      let mut id = 0;
      for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let i = ((y * 2 + dy) * big.width + x * 2 + dx) as usize;
        for (acc, v) in sum.iter_mut().zip(&big.rgba[i * 4..i * 4 + 4]) {
          *acc += *v as u32;
        }
        if id == 0 {
          id = big.ids[i];
        }
      }
      let o = (y * w + x) as usize;
      for (px, acc) in out.rgba[o * 4..o * 4 + 4].iter_mut().zip(sum) {
        *px = (acc / 4) as u8;
      }
      out.ids[o] = id;
    }
  }
  out
}

/// The export command's parameters for a 3D Print form (its file goes in `path`).
pub fn print_params(form: &PrintForm, path: &std::path::Path) -> Value {
  let mut p = json!({"path": path});
  if !form.bodies.is_empty() {
    p["bodies"] = json!(form.bodies);
  }
  if form.format == PrintFormat::StlAscii {
    p["ascii"] = json!(true);
  }
  p
}

/// Both dialogs, while open.
pub fn windows(app: &mut App, ctx: &egui::Context) {
  image_window(app, ctx);
  print_window(app, ctx);
}

/// Fusion's "Image Options": the size (the window's, or custom in pixels, inches, millimetres or
/// centimetres at a resolution), the look, and OK. Its X and Escape close it.
fn image_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.capture.image.clone() else { return };
  let (ok, closed) = crate::file_dialogs::floating(ctx, "image-options", "Image Options", 380.0, |ui| {
    ui.vertical_centered(|ui| {
      ui.label(RichText::new("Image Resolution").strong());
      egui::ComboBox::from_id_salt("capture-res")
        .width(240.0)
        .selected_text(if form.window_size { "Current Document Window Size" } else { "Custom" })
        .show_ui(ui, |ui| {
          ui.selectable_value(&mut form.window_size, true, "Current Document Window Size");
          ui.selectable_value(&mut form.window_size, false, "Custom");
        });
    });
    ui.add_space(6.0);
    let custom = !form.window_size;
    let [w, h] = form.size();
    egui::Grid::new("capture-image").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
      ui.label("Width");
      if custom {
        let mut v = form.in_unit(form.width);
        if ui.add(egui::DragValue::new(&mut v).speed(1.0).max_decimals(2).range(0.01..=8192.0)).changed() {
          let px = form.px_of(v);
          form.set_width(px);
        }
      } else {
        ui.add_enabled(false, egui::DragValue::new(&mut form.points[0].clone()));
      }
      ui.add_enabled_ui(custom, |ui| {
        egui::ComboBox::from_id_salt("capture-unit").width(100.0).selected_text(form.unit.label()).show_ui(ui, |ui| {
          for u in ImageUnit::ALL {
            ui.selectable_value(&mut form.unit, u, u.label());
          }
        });
      });
      ui.label(RichText::new(format!("W: {w}")).color(theme::MUTED));
      ui.end_row();
      ui.label("Height");
      if custom {
        let mut v = form.in_unit(form.height);
        if ui.add(egui::DragValue::new(&mut v).speed(1.0).max_decimals(2).range(0.01..=8192.0)).changed() {
          let px = form.px_of(v);
          form.set_height(px);
        }
      } else {
        ui.add_enabled(false, egui::DragValue::new(&mut form.points[1].clone()));
      }
      ui.add_enabled_ui(custom, |ui| ui.label(form.unit.label()));
      ui.label(RichText::new(format!("H: {h}")).color(theme::MUTED));
      ui.end_row();
      ui.label("Resolution");
      ui.add_enabled(custom, egui::DragValue::new(&mut form.ppi).speed(1.0).range(1.0..=2400.0).max_decimals(0));
      ui.add_enabled_ui(custom, |ui| ui.label("Pixels/Inch"));
      ui.end_row();
    });
    ui.add_space(2.0);
    // the window's size has its own proportions: the lock shows checked and greyed
    let mut lock = form.lock || !custom;
    if ui.add_enabled(custom, egui::Checkbox::new(&mut lock, "Lock Aspect Ratio")).changed() {
      form.lock = lock;
    }
    ui.add_space(6.0);
    ui.checkbox(&mut form.transparent, "Transparent Background");
    ui.checkbox(&mut form.antialias, "Enable Anti-aliasing");
    ui.add_space(10.0);
    ui.allocate_ui_with_layout(
      vec2(ui.available_width(), crate::file_dialogs::BUTTON.y),
      egui::Layout::right_to_left(egui::Align::Center),
      |ui| ui.add(crate::file_dialogs::blue_button("OK")).clicked(),
    )
    .inner
  });
  if ok {
    app.capture.image = None;
    if let Some(p) = rfd::FileDialog::new()
      .add_filter("PNG image", &["png"])
      .set_file_name(format!("{}.png", app.s.design.name))
      .save_file()
    {
      let frame = render_image(app, &form);
      app.status = Some(match frame.save_png(&p) {
        Ok(()) => (format!("Saved {}", p.display()), false),
        Err(e) => (format!("Couldn't save the image: {e}"), true),
      });
    }
  } else if closed {
    app.capture.image = None;
  } else if app.capture.image.is_some() {
    app.capture.image = Some(form);
  }
}

/// The 3D PRINT dialog's area (docked like the command dialogs).
pub(crate) const PRINT_AREA: &str = "print-3d-dialog";

/// 3D Print's OK: the Manufacture workspace, or the mesh file.
pub(crate) fn print_ok(app: &mut App, form: &PrintForm) {
  app.capture.print = None;
  if form.preparation == Preparation::Manufacture {
    crate::panels::switch_workspace(app, "Manufacture");
    app.status = Some((
      "Additive setups aren't in FissionOxide yet: 3D Print ▸ Mesh Export writes an STL, 3MF or OBJ to slice".into(),
      false,
    ));
    return;
  }
  let Some(f) = fox_engine::exchange::FORMATS.iter().find(|f| f.id == form.format.id()) else { return };
  let ext = f.extensions.first().copied().unwrap_or("stl");
  if let Some(p) = rfd::FileDialog::new()
    .add_filter(f.name, f.extensions)
    .set_file_name(format!("{}.{ext}", app.s.design.name))
    .save_file()
    && app.run("export", print_params(form, &p)).is_some()
  {
    app.status = Some((format!("Exported {}", p.display()), false));
  }
}

/// Fusion's docked "3D PRINT" dialog: Preparation Type (Manufacture, or Mesh Export with the
/// selection, refinement and format), the info icon, OK and Cancel. Escape closes it.
fn print_window(app: &mut App, ctx: &egui::Context) {
  if app.dialog.is_some() {
    // a command started: its dialog takes the dock
    app.capture.print = None;
  }
  let Some(mut form) = app.capture.print.clone() else { return };
  let mut ok = false;
  let mut cancel = false;
  let visible = app.s.model.bodies.iter().filter(|b| app.s.body_visible(b.id)).count();
  let width = 300.0;
  let icon = crate::icons::Icon::Print3d;
  let accent = crate::icons::accent_of(icon);
  let dock = crate::panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  egui::Area::new(egui::Id::new(PRINT_AREA))
    .pivot(egui::Align2::RIGHT_TOP)
    .default_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      crate::panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        crate::panels::dialog_header(ui, icon, "3D PRINT", width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          egui::Grid::new("print-3d").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
            ui.label("Preparation Type");
            egui::ComboBox::from_id_salt("print-prep").width(150.0).selected_text(form.preparation.label()).show_ui(
              ui,
              |ui| {
                for p in Preparation::ALL {
                  ui.selectable_value(&mut form.preparation, p, p.label());
                }
              },
            );
            ui.end_row();
            if form.preparation == Preparation::MeshExport {
              ui.label("Selection");
              let n = form.bodies.len();
              ui.label(if n == 0 {
                format!("{visible} visible {}", if visible == 1 { "body" } else { "bodies" })
              } else {
                format!("{n} {} selected", if n == 1 { "body" } else { "bodies" })
              });
              ui.end_row();
              ui.label("Refinement");
              egui::ComboBox::from_id_salt("print-refinement")
                .width(150.0)
                .selected_text(form.refinement.label())
                .show_ui(ui, |ui| {
                  for r in Refinement::ALL {
                    ui.selectable_value(&mut form.refinement, r, r.label());
                  }
                });
              ui.end_row();
              ui.label("Format");
              egui::ComboBox::from_id_salt("print-format").width(150.0).selected_text(form.format.label()).show_ui(
                ui,
                |ui| {
                  for f in PrintFormat::ALL {
                    ui.selectable_value(&mut form.format, f, f.label());
                  }
                },
              );
              ui.end_row();
            }
          });
          ui.add_space(6.0);
          if form.preparation == Preparation::Manufacture {
            ui.add(egui::Label::new(RichText::new(MANUFACTURE_NOTE).color(theme::TEXT)).wrap());
          } else {
            let mut utility = false;
            ui.add_enabled(false, egui::Checkbox::new(&mut utility, "Send to 3D Print Utility"))
              .on_disabled_hover_text("No print utility is installed; the mesh is saved to a file");
          }
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 24.0), egui::Sense::hover());
            ui.painter().circle_stroke(r.center(), 7.0, egui::Stroke::new(1.2, theme::GROUP_LABEL));
            ui.painter().text(
              r.center(),
              egui::Align2::CENTER_CENTER,
              "i",
              egui::FontId::proportional(11.0),
              theme::GROUP_LABEL,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if crate::panels::secondary_button(ui, "Cancel").clicked() {
                cancel = true;
              }
              let can = form.preparation == Preparation::Manufacture || visible > 0 || !form.bodies.is_empty();
              if ui.add_enabled_ui(can, |ui| crate::panels::primary_button(ui, "OK", accent)).inner.clicked() {
                ok = true;
              }
            });
          });
        });
      });
    });
  if ok {
    print_ok(app, &form);
  } else if cancel {
    app.capture.print = None;
  } else if app.capture.print.is_some() {
    app.capture.print = Some(form);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  #[test]
  fn capture_image_renders_the_canvas_at_the_chosen_size() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10}));
    app.canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 300.0));
    open_image(&mut app);
    let form = app.capture.image.clone().expect("open");
    assert!(form.window_size);
    assert_eq!(form.size(), [400, 300], "the canvas's size");
    let frame = render_image(&mut app, &form);
    assert_eq!((frame.width, frame.height), (400, 300));
    assert!(frame.rgba.chunks(4).all(|p| p[3] == 255), "opaque");
    // a custom size, transparent, without anti-aliasing
    let form = ImageForm { window_size: false, width: 64, height: 48, transparent: true, antialias: false, ..form };
    assert_eq!(form.size(), [64, 48]);
    let frame = render_image(&mut app, &form);
    assert_eq!((frame.width, frame.height), (64, 48));
    assert_eq!(frame.rgba[3], 0, "the top-left corner is background");
    assert!(frame.rgba.chunks(4).any(|p| p[3] == 255), "the box is drawn");
    // the same in anti-aliased form
    let frame = render_image(&mut app, &ImageForm { antialias: true, ..form });
    assert_eq!((frame.width, frame.height), (64, 48));
    assert!(frame.rgba.chunks(4).any(|p| p[3] == 255));
  }

  #[test]
  fn print_3d_exports_the_selection_in_the_chosen_format() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10}));
    let body = app.s.model.bodies[0].id;
    open_print(&mut app);
    let form = app.capture.print.clone().expect("open");
    assert!(form.bodies.is_empty(), "nothing selected: every visible body");
    assert_eq!(form.format, PrintFormat::StlBinary);
    let p = print_params(&form, std::path::Path::new("a.stl"));
    assert_eq!(p, json!({"path": "a.stl"}));
    app.s.selection = vec![fox_engine::Sel::Body { body }];
    open_print(&mut app);
    let mut form = app.capture.print.clone().unwrap();
    assert_eq!(form.bodies, vec![body.0]);
    form.format = PrintFormat::StlAscii;
    let p = print_params(&form, std::path::Path::new("a.stl"));
    assert_eq!(p, json!({"path": "a.stl", "bodies": [body.0], "ascii": true}));
    assert_eq!(PrintFormat::ThreeMf.id(), "3mf");
    assert!(fox_engine::exchange::FORMATS.iter().any(|f| f.id == "3mf" && f.export));
  }
}
