//! The ui-leftovers-chrome batch, driven headlessly: the marking menu's right-drag flick, the
//! Viewports fly-out's Multiple Views, Theme: Dark over the painted chrome, the status bar's
//! sketch curve readout and the arrow glyph the UI font lacks.

use egui::{Color32, Event, Pos2, RawInput, Rect, vec2};
use fox_engine::{Sel, Session};
use serde_json::json;

use crate::{App, panels, theme};

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
}

impl Harness {
  fn new(app: App) -> Harness {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut h = Harness { ctx, app, time: 0.0 };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  /// A frame as eframe runs it (the dark theme's recolouring included); every shape it drew.
  fn frame(&mut self, events: Vec<Event>) -> Vec<egui::Shape> {
    self.time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    let mut out = self.ctx.run_ui(input, |ui| {
      app.draw(ui);
      theme::darken_chrome(ui.ctx());
    });
    out.textures_delta.clear();
    fn walk(s: egui::Shape, out: &mut Vec<egui::Shape>) {
      match s {
        egui::Shape::Vec(v) => v.into_iter().for_each(|s| walk(s, out)),
        s => out.push(s),
      }
    }
    let mut shapes = Vec::new();
    for c in out.shapes {
      walk(c.shape, &mut shapes);
    }
    shapes
  }

  fn press(&mut self, button: egui::PointerButton, pos: Pos2, pressed: bool) {
    self.frame(vec![Event::PointerButton { pos, button, pressed, modifiers: Default::default() }]);
  }

  /// A right-drag from `a` to `b`, let go at `b`.
  fn right_drag(&mut self, a: Pos2, b: Pos2) {
    self.frame(vec![Event::PointerMoved(a)]);
    self.press(egui::PointerButton::Secondary, a, true);
    for i in 1..=6 {
      self.frame(vec![Event::PointerMoved(a + (b - a) * i as f32 / 6.0)]);
    }
    self.press(egui::PointerButton::Secondary, b, false);
    self.frame(vec![]);
  }

  fn click(&mut self, pos: Pos2) {
    self.frame(vec![Event::PointerMoved(pos)]);
    self.press(egui::PointerButton::Primary, pos, true);
    self.press(egui::PointerButton::Primary, pos, false);
    self.frame(vec![]);
  }
}

fn boxed() -> App {
  let mut app = App::with_session(Session::default());
  app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
  app
}

/// The rects filled with `fill`.
fn rects(shapes: &[egui::Shape], fill: Color32) -> usize {
  shapes.iter().filter(|s| matches!(s, egui::Shape::Rect(r) if r.fill == fill)).count()
}

#[test]
fn a_right_drag_runs_the_marking_menu_slot_it_points_at() {
  let mut h = Harness::new(boxed());
  // the canvas right of the box, clear of the browser and the ViewCube
  let at = h.app.canvas.center() + vec2(380.0, 120.0);
  let cam = h.app.cam;
  // east is Press Pull with nothing selected, as on Fusion's ring
  h.right_drag(at, at + vec2(120.0, 0.0));
  assert_eq!(h.app.cam, cam, "Fusion's preset doesn't orbit with the right button");
  assert!(h.app.marking.is_none(), "the gesture runs the item without the menu");
  let kind = h.app.dialog.as_ref().map(|d| d.kind);
  assert!(matches!(kind, Some(crate::dialogs::Kind::PressPull)), "{kind:?}");
  h.app.cancel_dialog();
  // north-west is Undo: the box goes
  h.right_drag(at, at + vec2(-90.0, -90.0));
  assert!(h.app.s.model.bodies.is_empty(), "Undo ran");
  // a right-click without dragging still opens the menu
  h.press(egui::PointerButton::Secondary, at, true);
  h.press(egui::PointerButton::Secondary, at, false);
  h.frame(vec![]);
  assert!(h.app.marking.is_some());
}

#[test]
fn a_flick_toward_a_list_opens_it() {
  let mut h = Harness::new(boxed());
  let at = h.app.canvas.center() + vec2(380.0, 120.0);
  // south-west is the Sketch item with its list of sketch tools
  h.right_drag(at, at + vec2(-90.0, 90.0));
  assert!(h.app.marking.is_some(), "the menu opens on the item's list");
  assert!(h.app.dialog.is_none());
}

#[test]
fn multiple_views_draw_four_views_and_a_click_activates_one() {
  let mut h = Harness::new(boxed());
  let full = h.app.canvas;
  crate::viewports::set_multiple(&mut h.app, true);
  h.frame(vec![]);
  h.frame(vec![]);
  let places = crate::viewports::rects(full);
  assert_eq!(h.app.canvas, places[3], "the active view is the bottom-right quarter");
  assert_eq!(crate::viewports::canvas(&h.app), full, "the navigation bar stays under all four");
  let front = h.app.viewports.cams[1];
  h.click(places[1].center() + vec2(-60.0, 60.0));
  assert_eq!(h.app.viewports.active, 1);
  assert_eq!(h.app.cam, front);
  h.frame(vec![]);
  assert_eq!(h.app.canvas, places[1]);
  // the fly-out's Single View gives the whole canvas back
  crate::viewports::set_multiple(&mut h.app, false);
  h.frame(vec![]);
  h.frame(vec![]);
  assert_eq!(h.app.canvas, full);
}

#[test]
fn theme_dark_recolours_the_painted_chrome() {
  let mut h = Harness::new(boxed());
  let light = h.frame(vec![]);
  assert!(rects(&light, theme::APP_BAR) > 0 && rects(&light, theme::TIMELINE) > 0);
  theme::set_theme(&h.ctx, crate::preferences::Theme::Dark);
  h.frame(vec![]);
  let dark = h.frame(vec![]);
  for (name, c) in [("application bar", theme::APP_BAR), ("toolbar", theme::TOOLBAR), ("timeline", theme::TIMELINE)] {
    assert_eq!(rects(&dark, c), 0, "the {name} is dark");
  }
  let surface = theme::color(fox_brand::dark::SURFACE);
  assert!(rects(&dark, surface) > 0);
  // painted text in the light palette's ink reads light
  let inked = dark.iter().any(|s| match s {
    egui::Shape::Text(t) => t.override_text_color == Some(theme::color(fox_brand::dark::TEXT)),
    _ => false,
  });
  assert!(inked);
  assert!(dark.iter().all(|s| !matches!(s, egui::Shape::Text(t) if t.override_text_color == Some(theme::TEXT))));
  // and back
  theme::set_theme(&h.ctx, crate::preferences::Theme::Light);
  h.frame(vec![]);
  assert!(rects(&h.frame(vec![]), theme::APP_BAR) > 0);
}

#[test]
fn the_dark_canvas_is_drawn_dark() {
  let mut h = Harness::new(boxed());
  theme::set_theme(&h.ctx, crate::preferences::Theme::Dark);
  h.frame(vec![]);
  h.frame(vec![]);
  let frame = h.app.frame.as_ref().expect("the CPU renderer drew the canvas");
  let px = &frame.rgba[..4];
  assert!(px[0] < 80 && px[1] < 80 && px[2] < 90, "the top-left pixel is dark: {px:?}");
}

#[test]
fn a_sketch_curve_reads_its_kind_and_size_in_the_document_unit() {
  let mut app = App::with_session(Session::default());
  app.run("design.units", json!({"unit": "in"}));
  let sk = app.run("sketch.create", json!({"plane": "xy"})).unwrap()["sketch"].as_u64().unwrap();
  let sketch = fox_engine::model::FeatureId(sk as u32);
  let line = app.run("sketch.line", json!({"points": [[0, 0], [25.4, 0]]})).unwrap()["curves"][0].as_u64().unwrap();
  let arc =
    app.run("sketch.arc", json!({"center": [60, 0], "start": [66.35, 0], "end": [60, 6.35]})).unwrap()["curves"][0]
      .as_u64()
      .unwrap();
  app.run("sketch.finish", json!({}));
  let curve = |c: u64| Sel::SketchCurve { sketch, curve: fox_engine::sketch::CurveId(c as u32) };
  app.s.selection = vec![curve(line)];
  assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Sketch Line | Length : 1.00 in"));
  app.s.selection = vec![curve(arc)];
  assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Sketch Arc | Diameter : 0.50 in"));
  // in millimetres
  app.run("design.units", json!({"unit": "mm"}));
  assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Sketch Arc | Diameter : 12.70 mm"));
}

#[test]
fn no_ui_string_uses_the_arrow_the_font_lacks() {
  // the UI font has no U+2192: it would draw a box (comments and tests aside)
  fn scan(dir: &std::path::Path, found: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
      let p = e.path();
      if p.is_dir() {
        scan(&p, found);
        continue;
      }
      let name = p.file_name().unwrap().to_string_lossy().to_string();
      if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
        continue;
      }
      let text = std::fs::read_to_string(&p).unwrap();
      let code = text.split("#[cfg(test)]\nmod tests").next().unwrap_or("");
      for (n, l) in code.lines().enumerate() {
        let before_comment = l.split("//").next().unwrap_or("");
        // plugins_ui::drawable is what swaps a plugin's arrows for ">"
        if before_comment.contains('\u{2192}') && !l.contains(".replace([") {
          found.push(format!("{}:{}: {}", p.display(), n + 1, l.trim()));
        }
      }
    }
  }
  let mut found = Vec::new();
  scan(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
  assert!(found.is_empty(), "{found:#?}");
}
