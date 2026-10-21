//! File ▸ Capture Image... and File ▸ 3D Print, as Fusion's dialogs: Capture Image writes the
//! canvas as a PNG at a chosen size (transparent background and anti-aliasing optional); 3D
//! Print writes the bodies as a mesh file (STL, 3MF or OBJ) at a chosen refinement.

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
}

impl ImageForm {
  /// The size the image is written at.
  pub fn size(&self) -> [u32; 2] {
    if self.window_size { self.canvas } else { [self.width.max(1), self.height.max(1)] }
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

#[derive(Debug, Clone, PartialEq)]
pub struct PrintForm {
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
  let canvas = if app.canvas_px[0] > 0 && app.canvas_px[1] > 0 {
    app.canvas_px
  } else {
    [(app.canvas.width().round() as u32).max(1), (app.canvas.height().round() as u32).max(1)]
  };
  app.capture.image = Some(ImageForm {
    window_size: true,
    width: canvas[0],
    height: canvas[1],
    canvas,
    transparent: false,
    antialias: true,
  });
}

/// File ▸ 3D Print.
pub fn open_print(app: &mut App) {
  let bodies = app
    .s
    .selection
    .iter()
    .filter_map(|s| if let fox_engine::Sel::Body { body } = s { Some(body.0) } else { None })
    .collect();
  app.capture.print = Some(PrintForm { bodies, refinement: Refinement::Medium, format: PrintFormat::StlBinary });
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

fn image_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.capture.image.clone() else { return };
  let mut open = true;
  let mut ok = false;
  let mut cancel = false;
  egui::Window::new("Capture Image")
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.set_width(320.0);
      egui::Grid::new("capture-image").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Image Resolution");
        egui::ComboBox::from_id_salt("capture-res")
          .width(190.0)
          .selected_text(if form.window_size { "Current Document Window Size" } else { "Custom" })
          .show_ui(ui, |ui| {
            ui.selectable_value(&mut form.window_size, true, "Current Document Window Size");
            ui.selectable_value(&mut form.window_size, false, "Custom");
          });
        ui.end_row();
        let [w, h] = form.size();
        ui.label("Width");
        ui.add_enabled_ui(!form.window_size, |ui| {
          if form.window_size {
            ui.label(format!("{w} px"));
          } else {
            ui.add(egui::DragValue::new(&mut form.width).range(1..=8192).suffix(" px"));
          }
        });
        ui.end_row();
        ui.label("Height");
        ui.add_enabled_ui(!form.window_size, |ui| {
          if form.window_size {
            ui.label(format!("{h} px"));
          } else {
            ui.add(egui::DragValue::new(&mut form.height).range(1..=8192).suffix(" px"));
          }
        });
        ui.end_row();
      });
      ui.add_space(4.0);
      ui.checkbox(&mut form.transparent, "Transparent Background");
      ui.checkbox(&mut form.antialias, "Enable Anti-aliasing");
      ui.add_space(6.0);
      ui.separator();
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Cancel").clicked() {
          cancel = true;
        }
        if ui.add(egui::Button::new(RichText::new("OK").color(egui::Color32::WHITE)).fill(theme::ACCENT)).clicked() {
          ok = true;
        }
      });
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
  } else if !open || cancel {
    app.capture.image = None;
  } else {
    app.capture.image = Some(form);
  }
}

fn print_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.capture.print.clone() else { return };
  let mut open = true;
  let mut ok = false;
  let mut cancel = false;
  let visible = app.s.model.bodies.iter().filter(|b| app.s.body_visible(b.id)).count();
  egui::Window::new("3D Print")
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.set_width(320.0);
      egui::Grid::new("print-3d").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Selection");
        let n = form.bodies.len();
        ui.label(if n == 0 {
          format!("{visible} visible {}", if visible == 1 { "body" } else { "bodies" })
        } else {
          format!("{n} {} selected", if n == 1 { "body" } else { "bodies" })
        });
        ui.end_row();
        ui.label("Refinement");
        egui::ComboBox::from_id_salt("print-refinement").width(150.0).selected_text(form.refinement.label()).show_ui(
          ui,
          |ui| {
            for r in Refinement::ALL {
              ui.selectable_value(&mut form.refinement, r, r.label());
            }
          },
        );
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
      });
      ui.add_space(4.0);
      let mut utility = false;
      ui.add_enabled(false, egui::Checkbox::new(&mut utility, "Send to 3D Print Utility"))
        .on_disabled_hover_text("No print utility is installed; the mesh is saved to a file");
      ui.label(
        RichText::new("The mesh is tessellated at the design's display refinement.").small().color(theme::MUTED),
      );
      ui.add_space(6.0);
      ui.separator();
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Cancel").clicked() {
          cancel = true;
        }
        if ui
          .add_enabled(
            visible > 0 || !form.bodies.is_empty(),
            egui::Button::new(RichText::new("OK").color(egui::Color32::WHITE)).fill(theme::ACCENT),
          )
          .clicked()
        {
          ok = true;
        }
      });
    });
  if ok {
    app.capture.print = None;
    let Some(f) = fox_engine::exchange::FORMATS.iter().find(|f| f.id == form.format.id()) else { return };
    let ext = f.extensions.first().copied().unwrap_or("stl");
    if let Some(p) = rfd::FileDialog::new()
      .add_filter(f.name, f.extensions)
      .set_file_name(format!("{}.{ext}", app.s.design.name))
      .save_file()
      && app.run("export", print_params(&form, &p)).is_some()
    {
      app.status = Some((format!("Exported {}", p.display()), false));
    }
  } else if !open || cancel {
    app.capture.print = None;
  } else {
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
