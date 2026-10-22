//! The Render workspace's docked tool dialogs beside Scene Settings and New Light: TEXTURE MAP
//! CONTROLS (SETUP), DIELECTRIC PRIORITY CONTROL (OPTICAL OVERRIDES) and IN-CANVAS RENDER
//! SETTINGS (IN-CANVAS RENDER), laid out as Fusion's command dialogs: a header, the inputs, and
//! OK / Cancel under a rule.

use egui::{Rect, Stroke, pos2, vec2};
use fox_engine::Sel;
use fox_engine::model::BodyId;
use fox_engine::rendering::raytrace::{Axis, Projection, TextureSpec};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::render_panels::footer;
use crate::{App, dialogs, panels, theme};

/// A docked dialog: its header, `body`, then Cancel and OK (greyed, saying why, while `blocked`).
/// Returns (OK pressed, Cancel pressed).
fn docked(
  ctx: &egui::Context,
  id: &str,
  icon: Icon,
  title: &str,
  blocked: Option<&str>,
  canvas: Rect,
  body: impl FnOnce(&mut egui::Ui),
) -> (bool, bool) {
  let width = 320.0;
  let accent = icons::accent_of(icon);
  let mut out = (false, false);
  egui::Area::new(egui::Id::new(id))
    .pivot(egui::Align2::RIGHT_TOP)
    .default_pos(panels::dialog_dock(canvas, false))
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, icon, title, width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          body(ui);
        });
        out = footer(ui, width, |ui| {
          let cancel = panels::secondary_button(ui, "Cancel").clicked();
          let resp = dialogs::ok_button(ui, accent, blocked.is_none());
          match blocked {
            None => (resp.clicked(), cancel),
            Some(why) => {
              resp.on_disabled_hover_text(why);
              (false, cancel)
            }
          }
        });
      });
    });
  out
}

/// The bodies of the selection: picked bodies, and the bodies of picked faces.
pub(crate) fn selected_bodies(app: &App) -> Vec<BodyId> {
  let mut out = Vec::new();
  for s in &app.s.selection {
    let b = match s {
      Sel::Body { body } | Sel::Face { body, .. } => *body,
      _ => continue,
    };
    if !out.contains(&b) {
      out.push(b);
    }
  }
  out
}

/// Opens one of the workspace's docked dialogs in place of the others (and of a command
/// dialog), as Fusion shows one at a time.
pub(crate) fn close_docked(app: &mut App) {
  app.cancel_dialog();
  let r = &mut app.render;
  r.scene_open = false;
  r.light = None;
  r.texture_map = None;
  r.priority = None;
  r.in_canvas_settings = None;
}

// ---------------------------------------------------------------- texture map controls

/// Fusion's projections, in its drop-down's order.
pub(crate) const PROJECTIONS: [Projection; 5] =
  [Projection::Planar, Projection::Box, Projection::Spherical, Projection::Cylindrical, Projection::Automatic];

/// The TEXTURE MAP CONTROLS dialog's values: the body they were read from and its mapping.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextureMapForm {
  pub body: Option<BodyId>,
  /// The mapping being edited (`None`: the body's material has no texture).
  pub spec: Option<TextureSpec>,
}

impl TextureMapForm {
  /// Reads the mapping of the first selected body (or the body of a selected face).
  fn sync(&mut self, app: &App) {
    let body = selected_bodies(app).first().copied();
    if body != self.body {
      self.body = body;
      self.spec = body.and_then(|b| fox_engine::rendering::body_texture(&app.s, b));
    }
  }
}

/// The docked TEXTURE MAP CONTROLS dialog: Object, Projection, Axis, Scale, Rotation, Offset and
/// Reset; OK changes the body's texture mapping.
pub(crate) fn texture_map_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.render.texture_map.clone() else { return };
  form.sync(app);
  let accent = icons::accent_of(Icon::TextureMap);
  let has = form.body.is_some();
  let blocked = match (has, form.spec.is_some()) {
    (false, _) => Some("Select a body or face"),
    (true, false) => Some("The selected body's appearance has no texture to map"),
    (true, true) => None,
  };
  let mut reset = false;
  let (ok, cancel) = docked(ctx, "texture-map", Icon::TextureMap, "TEXTURE MAP CONTROLS", blocked, app.canvas, |ui| {
    egui::Grid::new("texture-map-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
      ui.label("Object");
      dialogs::sel_chip(ui, accent, &dialogs::chip_text(has as usize, "selected"), has);
      ui.end_row();
      let Some(t) = &mut form.spec else {
        if has {
          ui.label("");
          ui.label(egui::RichText::new("No texture").color(theme::MUTED));
          ui.end_row();
        }
        return;
      };
      ui.label("Projection");
      egui::ComboBox::from_id_salt("texture-map-proj").selected_text(t.projection.label()).width(150.0).show_ui(
        ui,
        |ui| {
          for p in PROJECTIONS {
            ui.selectable_value(&mut t.projection, p, p.label());
          }
        },
      );
      ui.end_row();
      if !matches!(t.projection, Projection::Box | Projection::Automatic) {
        ui.label("Axis");
        ui.horizontal(|ui| {
          for (a, n) in [(Axis::X, "X"), (Axis::Y, "Y"), (Axis::Z, "Z")] {
            ui.selectable_value(&mut t.axis, a, n);
          }
        });
        ui.end_row();
      }
      ui.label("Scale");
      ui.add(egui::DragValue::new(&mut t.scale).range(0.1..=10000.0).speed(0.5).suffix(" mm"));
      ui.end_row();
      ui.label("Rotation");
      ui.add(egui::DragValue::new(&mut t.rotation).range(-360.0..=360.0).speed(1.0).suffix(" deg"));
      ui.end_row();
      ui.label("Offset");
      ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut t.offset[0]).speed(0.01).prefix("U "));
        ui.add(egui::DragValue::new(&mut t.offset[1]).speed(0.01).prefix("V "));
      });
      ui.end_row();
      ui.label("");
      reset = ui.button("Reset").on_hover_text("Puts the texture back where the appearance had it").clicked();
      ui.end_row();
    });
  });
  if reset && let Some(t) = &mut form.spec {
    let name = form.body.map(|b| fox_engine::rendering::body_material(&app.s, b).name).unwrap_or_default();
    let lib = fox_engine::rendering::raytrace::material::find(&name).and_then(|m| m.texture).unwrap_or_default();
    (t.projection, t.axis, t.scale, t.rotation, t.offset) =
      (lib.projection, lib.axis, lib.scale, lib.rotation, lib.offset);
  }
  if ok && let (Some(b), Some(t)) = (form.body, &form.spec) {
    let p = json!({
      "body": b.0, "projection": t.projection.label().to_ascii_lowercase(),
      "axis": format!("{:?}", t.axis).to_ascii_lowercase(), "scale": t.scale, "rotation": t.rotation,
      "offset": t.offset,
    });
    if app.run("render.texture_map", p).is_some() {
      app.render.texture_map = None;
      return;
    }
  }
  app.render.texture_map = (!cancel).then_some(form);
}

// ---------------------------------------------------------------- dielectric priority

/// The DIELECTRIC PRIORITY CONTROL dialog's priority (`None` until picked: the bodies' own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PriorityForm {
  pub priority: Option<u8>,
}

/// The docked DIELECTRIC PRIORITY CONTROL dialog: Bodies and Priority (0, 1, 2); OK gives the
/// selected bodies the priority.
pub(crate) fn priority_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.render.priority else { return };
  let bodies = selected_bodies(app);
  let accent = icons::accent_of(Icon::DielectricPriority);
  let current = bodies.first().map_or(0, |b| app.s.design.render.priority(*b));
  let mut prio = form.priority.unwrap_or(current);
  let (ok, cancel) = docked(
    ctx,
    "dielectric-priority",
    Icon::DielectricPriority,
    "DIELECTRIC PRIORITY CONTROL",
    bodies.is_empty().then_some("Select the bodies to give a priority"),
    app.canvas,
    |ui| {
      egui::Grid::new("priority-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
        ui.label("Bodies");
        dialogs::sel_chip(ui, accent, &dialogs::chip_text(bodies.len(), "selected"), !bodies.is_empty());
        ui.end_row();
        ui.label("Priority");
        egui::ComboBox::from_id_salt("priority-value").selected_text(prio.to_string()).width(150.0).show_ui(ui, |ui| {
          for p in [0u8, 1, 2] {
            ui.selectable_value(&mut prio, p, p.to_string());
          }
        });
        ui.end_row();
      });
    },
  );
  if prio != current || form.priority.is_some() {
    form.priority = Some(prio);
  }
  if ok {
    let ids: Vec<u32> = bodies.iter().map(|b| b.0).collect();
    if app.run("render.priority", json!({"bodies": ids, "priority": prio})).is_some() {
      app.render.priority = None;
      return;
    }
  }
  app.render.priority = (!cancel).then_some(form);
}

// ---------------------------------------------------------------- in-canvas render settings

/// In-canvas Render's mode: Fusion's Fast and Advanced tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InCanvasMode {
  Fast,
  #[default]
  Advanced,
}

/// How far In-canvas Render refines before it stops: the marks on its progress bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Quality {
  Excellent,
  Final,
  #[default]
  Infinite,
}

impl Quality {
  pub(crate) const ALL: [Quality; 3] = [Quality::Excellent, Quality::Final, Quality::Infinite];

  pub(crate) fn label(self) -> &'static str {
    match self {
      Quality::Excellent => "Excellent",
      Quality::Final => "Final",
      Quality::Infinite => "Infinite",
    }
  }

  /// The iteration count it stops at (`None`: it doesn't).
  pub(crate) fn iterations(self) -> Option<u32> {
    match self {
      Quality::Excellent => Some(crate::render_panels::EXCELLENT.0),
      Quality::Final => Some(crate::render_panels::FINAL.0),
      Quality::Infinite => None,
    }
  }
}

/// In-canvas Render's settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InCanvasSettings {
  pub mode: InCanvasMode,
  pub quality: Quality,
}

/// The docked IN-CANVAS RENDER SETTINGS dialog: Fast and Advanced tabs (the open one is the
/// mode), Advanced's Quality; OK keeps them.
pub(crate) fn in_canvas_settings_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.render.in_canvas_settings else { return };
  let (ok, cancel) =
    docked(ctx, "in-canvas-settings", Icon::InCanvasSettings, "IN-CANVAS RENDER SETTINGS", None, app.canvas, |ui| {
      mode_tabs(ui, &mut form.mode);
      ui.add_space(6.0);
      egui::Grid::new("in-canvas-settings-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(
        ui,
        |ui| match form.mode {
          InCanvasMode::Fast => {
            ui.label("Limit Resolution");
            ui.add_enabled(false, egui::Checkbox::without_text(&mut false))
              .on_disabled_hover_text("FissionOxide's in-canvas render always draws at the canvas's resolution");
            ui.end_row();
          }
          InCanvasMode::Advanced => {
            ui.label("Quality");
            egui::ComboBox::from_id_salt("in-canvas-quality").selected_text(form.quality.label()).width(150.0).show_ui(
              ui,
              |ui| {
                for q in Quality::ALL {
                  ui.selectable_value(&mut form.quality, q, q.label());
                }
              },
            );
            ui.end_row();
          }
        },
      );
    });
  if ok {
    app.render.canvas_settings = form;
  }
  app.render.in_canvas_settings = (!ok && !cancel).then_some(form);
}

/// Fast | Advanced, the open one underlined in the accent.
fn mode_tabs(ui: &mut egui::Ui, mode: &mut InCanvasMode) {
  let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::hover());
  ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
  let mut x = rect.left() + 6.0;
  for (m, label) in [(InCanvasMode::Fast, "Fast"), (InCanvasMode::Advanced, "Advanced")] {
    let on = *mode == m;
    let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), theme::TEXT);
    let r = Rect::from_min_size(pos2(x - 6.0, rect.top()), vec2(g.size().x + 12.0, rect.height()));
    let resp = ui.interact(r, ui.id().with(("in-canvas-tab", label)), egui::Sense::click());
    if resp.hovered() && !on {
      ui.painter().rect_filled(r, 2.0, theme::HOVER);
    }
    ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, if on { theme::TEXT } else { theme::MUTED });
    if on {
      ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(2.0, theme::ACCENT));
    }
    if resp.clicked() {
      *mode = m;
    }
    x = r.right() + 14.0;
  }
}
