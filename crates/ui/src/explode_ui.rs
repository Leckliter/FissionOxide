//! The Render Components window over the engine's `view.render_components`. Fusion's File ▸ View
//! has neither it nor an Explode toggle, so nothing in the menus opens it: the engine's
//! `view.explode` and `view.render_components` run from TEXT COMMANDS.

use egui::RichText;
use serde_json::{Value, json};

use crate::{App, theme};

/// Where a batch render looks from.
#[derive(Debug, Clone, PartialEq)]
pub enum CameraChoice {
  /// Degrees around and up.
  Angles { yaw: f64, pitch: f64 },
  /// A named view's (or standard view's) direction.
  View(String),
}

/// The Render Components window.
pub struct RenderComponents {
  pub open: bool,
  pub folder: String,
  pub size: u32,
  pub camera: CameraChoice,
  pub edges: bool,
  pub label: String,
  pub pattern: String,
  /// The last run's summary: (text, is_error).
  pub result: Option<(String, bool)>,
}

impl Default for RenderComponents {
  fn default() -> Self {
    RenderComponents {
      open: false,
      folder: String::new(),
      size: 1080,
      camera: CameraChoice::Angles { yaw: 15.0, pitch: 20.0 },
      edges: false,
      label: String::new(),
      pattern: "{label} - {component}".into(),
      result: None,
    }
  }
}

impl RenderComponents {
  /// `view.render_components` parameters (`None` until a folder is picked).
  pub fn params(&self) -> Option<Value> {
    let folder = self.folder.trim();
    if folder.is_empty() {
      return None;
    }
    let camera = match &self.camera {
      CameraChoice::Angles { yaw, pitch } => json!({"yaw": yaw, "pitch": pitch}),
      CameraChoice::View(v) => json!(v),
    };
    let mut p = json!({
      "folder": folder,
      "size": self.size,
      "camera": camera,
      "style": if self.edges { "shaded_edges" } else { "shaded" },
      "name_pattern": self.pattern,
    });
    if !self.label.trim().is_empty() {
      p["label"] = json!(self.label.trim());
    }
    Some(p)
  }
}

/// The Render Components window, while open.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.render_components.open {
    return;
  }
  let mut open = true;
  let mut go = false;
  let views: Vec<String> = app.s.design.named_views.iter().map(|v| v.name.clone()).collect();
  egui::Window::new("Render Components").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
    let rc = &mut app.render_components;
    egui::Grid::new("render_components").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
      ui.label("Folder");
      ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut rc.folder).desired_width(220.0));
        if ui.button("Browse...").clicked()
          && let Some(p) = rfd::FileDialog::new().set_title("Folder for the images").pick_folder()
        {
          rc.folder = p.display().to_string();
        }
      });
      ui.end_row();
      ui.label("Size");
      ui.add(egui::DragValue::new(&mut rc.size).range(16..=8192).suffix(" px"));
      ui.end_row();
      ui.label("Camera");
      let text = match &rc.camera {
        CameraChoice::Angles { .. } => "Angles".to_string(),
        CameraChoice::View(v) => v.clone(),
      };
      egui::ComboBox::from_id_salt("rc_camera").selected_text(text).show_ui(ui, |ui| {
        if ui.selectable_label(matches!(rc.camera, CameraChoice::Angles { .. }), "Angles").clicked() {
          rc.camera = CameraChoice::Angles { yaw: 15.0, pitch: 20.0 };
        }
        for v in views.iter().map(String::as_str).chain(["top", "front", "right", "home"]) {
          if ui.selectable_label(rc.camera == CameraChoice::View(v.to_string()), v).clicked() {
            rc.camera = CameraChoice::View(v.to_string());
          }
        }
      });
      ui.end_row();
      if let CameraChoice::Angles { yaw, pitch } = &mut rc.camera {
        ui.label("Around / up");
        ui.horizontal(|ui| {
          ui.add(egui::DragValue::new(yaw).range(-360.0..=360.0).suffix("°"));
          ui.add(egui::DragValue::new(pitch).range(-89.0..=89.0).suffix("°"));
        });
        ui.end_row();
      }
      ui.label("Edges");
      ui.checkbox(&mut rc.edges, "Shaded with edges");
      ui.end_row();
      ui.label("Label");
      ui.add(egui::TextEdit::singleline(&mut rc.label).hint_text("the design's name").desired_width(220.0));
      ui.end_row();
      ui.label("File names");
      ui.add(egui::TextEdit::singleline(&mut rc.pattern).desired_width(220.0))
        .on_hover_text("{label}, {component}, {number} (the name's leading number), {index}");
      ui.end_row();
    });
    ui.label(
      RichText::new("Each component is drawn on its own; visibility is left as it is.").small().color(theme::MUTED),
    );
    if let Some((t, err)) = &rc.result {
      ui.label(RichText::new(t).color(if *err { theme::ERROR } else { theme::MUTED }));
    }
    ui.horizontal(|ui| {
      go = ui.add_enabled(rc.params().is_some(), egui::Button::new("Render")).clicked();
    });
  });
  if go && let Some(p) = app.render_components.params() {
    render(app, &p);
  }
  if !open {
    app.render_components.open = false;
  }
}

/// Runs the batch render and notes the outcome in the window and the status bar.
pub fn render(app: &mut App, p: &Value) {
  app.render_components.result = Some(match app.run("view.render_components", p.clone()) {
    Some(r) => {
      let msg = format!("Rendered {} components to {}", r["images"], r["folder"].as_str().unwrap_or(""));
      app.status = Some((msg.clone(), false));
      (msg, false)
    }
    None => (app.status.as_ref().map(|s| s.0.clone()).unwrap_or_else(|| "Render failed".into()), true),
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn app_with_layers() -> App {
    let mut app = App::with_session(Session::default());
    for (i, name) in ["01 - Back", "02 - Face"].iter().enumerate() {
      app.run("box", json!({"width": 20, "depth": 20, "height": 2})).unwrap();
      let body = app.s.model.bodies.last().unwrap().id.0;
      app.run("move", json!({"bodies": [body], "translate": [0, 0, 2 * i]})).unwrap();
      app.run("component.create", json!({"name": name, "bodies": [body]})).unwrap();
    }
    app
  }

  #[test]
  fn explode_toggles_and_renders_components_to_a_folder() {
    let mut app = app_with_layers();
    app.run("view.explode", json!({"factor": 4.0}));
    assert_eq!(app.s.design.exploded_view.as_ref().map(|v| v.factor), Some(4.0));
    app.run("view.explode", json!({"off": true}));
    assert!(app.s.design.exploded_view.is_none());

    let dir = std::env::temp_dir().join(format!("fox-ui-render-components-{}", std::process::id()));
    let rc =
      RenderComponents { folder: dir.display().to_string(), size: 24, label: "Sign".into(), ..Default::default() };
    assert!(RenderComponents::default().params().is_none());
    let p = rc.params().unwrap();
    assert_eq!(p["camera"], json!({"yaw": 15.0, "pitch": 20.0}));
    render(&mut app, &p);
    assert_eq!(app.render_components.result.as_ref().map(|r| r.1), Some(false), "{:?}", app.render_components.result);
    assert!(dir.join("Sign - 01 - Back.png").exists() && dir.join("Sign - Part Data.csv").exists());
    let _ = std::fs::remove_dir_all(&dir);
  }
}
