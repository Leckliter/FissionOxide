//! The ui-leftovers-chrome batch, driven headlessly: the marking menu's right-drag flick, the
//! Viewports fly-out's Multiple Views, Theme: Dark over the painted chrome, the status bar's
//! sketch curve readout and the glyphs the UI font lacks.

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

  /// A quick click: the move, the press and the release in one frame, as a window that was idle
  /// gets them.
  fn quick_click(&mut self, pos: Pos2) {
    let b = |pressed| Event::PointerButton {
      pos,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    self.frame(vec![Event::PointerMoved(pos), b(true), b(false)]);
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
  // north-east is Press Pull with nothing selected, as on Fusion's ring
  h.right_drag(at, at + vec2(90.0, -90.0));
  assert_eq!(h.app.cam, cam, "Fusion's preset doesn't orbit with the right button");
  assert!(h.app.marking.is_none(), "the gesture runs the item without the menu");
  let kind = h.app.dialog.as_ref().map(|d| d.kind);
  assert!(matches!(kind, Some(crate::dialogs::Kind::PressPull)), "{kind:?}");
  h.app.cancel_dialog();
  // west is Undo: the box goes
  h.right_drag(at, at + vec2(-120.0, 0.0));
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
  // south is the Sketch item with its list of sketch tools
  h.right_drag(at, at + vec2(0.0, 120.0));
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
fn every_ui_string_uses_glyphs_the_ui_font_has() {
  // a character none of the UI font's faces has draws as a box (comments and tests aside); the
  // proportional faces are also the monospace family's fallbacks. (egui's own has_glyph says no
  // for whatever its replacement glyph's face holds, so the faces' character maps are read.)
  let fonts = egui::FontDefinitions::default();
  let faces: Vec<ttf_parser::Face> = fonts.families[&egui::FontFamily::Proportional]
    .iter()
    .map(|name| {
      let data = &fonts.font_data[name];
      ttf_parser::Face::parse(&data.font, data.index).unwrap()
    })
    .collect();
  let has = |c: char| faces.iter().any(|f| f.glyph_index(c).is_some());
  fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
      let p = e.path();
      if p.is_dir() {
        files(&p, out);
      } else if p.extension().is_some_and(|x| x == "rs") && !p.to_string_lossy().ends_with("_tests.rs") {
        out.push(p);
      }
    }
  }
  let mut paths = Vec::new();
  files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut paths);
  let mut found = Vec::new();
  for p in paths {
    let text = std::fs::read_to_string(&p).unwrap();
    let code = text.split("#[cfg(test)]\nmod tests").next().unwrap_or("");
    let viewport = p.ends_with("viewport.rs");
    for (n, l) in code.lines().enumerate() {
      // plugins_ui::drawable swaps a plugin's arrows for ">"
      if l.contains(".replace([") {
        continue;
      }
      let before_comment = l.split("//").next().unwrap_or("");
      for c in before_comment.chars().filter(|c| !c.is_ascii()) {
        // a dimension's diameter sign is painted (viewport::DimText), not taken from the font
        if (viewport && c == '\u{2300}') || has(c) {
          continue;
        }
        found.push(format!("{}:{}: U+{:04X} {c} in {}", p.display(), n + 1, c as u32, l.trim()));
      }
    }
  }
  assert!(found.is_empty(), "{found:#?}");
}

/// The text shapes among `shapes`: the string and where it was drawn.
fn texts(shapes: &[egui::Shape]) -> Vec<(String, Rect)> {
  shapes
    .iter()
    .filter_map(|s| match s {
      egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.visual_bounding_rect())),
      _ => None,
    })
    .collect()
}

#[test]
fn the_open_windows_tree_lists_new_project_and_the_samples_down_its_left_pane() {
  let root = std::env::temp_dir().join(format!("fox-ui-open-tree-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  let mut app = boxed();
  app.s.store_root = Some(root.clone());
  app.run("hub.save", json!({"name": "Bracket", "new": true, "explicit": true}));
  let mut h = Harness::new(app);
  crate::file_dialogs::open(&mut h.app);
  h.frame(vec![]);
  let drawn = texts(&h.frame(vec![]));
  let window = h.ctx.memory(|m| m.area_rect(crate::file_dialogs::modal_id("open"))).expect("the Open window");
  let at = |label: &str| drawn.iter().find(|(t, _)| t == label).map(|(_, r)| *r).unwrap_or_else(|| panic!("{label}"));
  let mut tree = vec![at("Default Project"), at("+  New Project"), at("Samples")];
  tree.extend(crate::file_dialogs::SAMPLES.iter().map(|s| at(s)));
  for pair in tree.windows(2) {
    assert!(pair[1].top() >= pair[0].bottom() - 1.0, "one under the other: {pair:?}");
  }
  for r in &tree {
    assert!(r.left() > window.left() && r.right() < window.left() + 260.0, "in the tree's pane: {r:?} in {window:?}");
  }
  let _ = std::fs::remove_dir_all(root);
}

#[test]
fn text_commands_is_a_bar_under_the_timeline_that_a_click_expands() {
  let mut h = Harness::new(boxed());
  let drawn = texts(&h.frame(vec![]));
  let (_, bar) = drawn.iter().find(|(t, _)| t == "TEXT COMMANDS").expect("the bar shows at start, as in Fusion");
  assert!(bar.bottom() > 900.0 - crate::text_commands::BAR_H, "along the bottom of the window: {bar:?}");
  assert!(h.app.canvas.bottom() < bar.top(), "under the canvas and the timeline");
  assert!(!drawn.iter().any(|(t, _)| t == "Txt"), "collapsed: no input line yet");
  h.click(bar.center());
  assert!(h.app.text_commands.expanded);
  for _ in 0..10 {
    h.frame(vec![]);
  }
  let drawn = texts(&h.frame(vec![]));
  let (_, txt) = drawn.iter().find(|(t, _)| t == "Txt").expect("expanded: the input line and Txt / Py / TS");
  let (_, bar) = drawn.iter().find(|(t, _)| t == "TEXT COMMANDS").unwrap();
  assert!(txt.bottom() <= 900.0 && txt.top() > bar.bottom(), "the input line under the bar: {bar:?} {txt:?}");
  let height = 900.0 - bar.top();
  assert!(height > 130.0 && height < 170.0, "Fusion's height, kept frame after frame: {bar:?}");
  assert!(h.app.canvas.height() > 400.0, "the canvas keeps its room: {:?}", h.app.canvas);
  // File > View > Hide Text Commands takes it away
  crate::file_menu::view_action(&mut h.app, 3);
  let drawn = texts(&h.frame(vec![]));
  assert!(!drawn.iter().any(|(t, _)| t == "TEXT COMMANDS"));
}

#[test]
fn theme_dark_from_preferences_darkens_the_chrome_and_light_brings_it_back() {
  let mut h = Harness::new(boxed());
  let find = |h: &mut Harness, label: &str| {
    let drawn = texts(&h.frame(vec![]));
    drawn.iter().rev().find(|(t, _)| t == label).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("{label}"))
  };
  // Preferences > General > Theme: Dark, then Apply
  crate::preferences::open(&mut h.app);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let combo = || crate::preferences::combo_rect_id("prefs-theme");
  let theme_box = h.ctx.data(|d| d.get_temp::<Rect>(combo())).expect("the Theme drop-down");
  h.click(theme_box.center());
  let dark = find(&mut h, "Dark");
  h.click(dark);
  let apply = find(&mut h, "Apply");
  h.click(apply);
  assert!(theme::is_dark(&h.ctx));
  let shapes = h.frame(vec![]);
  let painted = [
    ("application bar", theme::APP_BAR),
    ("toolbar", theme::TOOLBAR),
    ("timeline", theme::TIMELINE),
    ("workspace switcher", theme::SWITCHER),
  ];
  for (name, c) in painted {
    assert_eq!(rects(&shapes, c), 0, "the {name} is dark, not only Preferences");
  }
  // the page's heading box keeps a dark ground and light text
  let heading = texts(&shapes).into_iter().find(|(t, _)| t == "Preferences controlling general UI behavior");
  assert!(heading.is_some(), "the heading reads");
  assert!(
    !shapes.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == Color32::WHITE && r.rect.height() > 20.0
    && r.rect.width() > 300.0)),
    "no white strip"
  );
  // the drop-down still opens in the dark dialog, over it (the click on Apply put the dialog on
  // top of the list's old place), and Light + OK undoes it
  let theme_box = h.ctx.data(|d| d.get_temp::<Rect>(combo())).expect("the Theme drop-down");
  h.quick_click(theme_box.center());
  assert!(egui::Popup::is_any_open(&h.ctx), "the Theme list opens");
  let light = find(&mut h, "Light");
  let under = h.ctx.memory(|m| m.layer_id_at(light));
  let modal = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(crate::preferences::MODAL_ID));
  assert!(under.is_some_and(|l| l != modal), "the list is above the dialog, not hidden under it: {under:?}");
  h.click(light);
  assert_eq!(h.app.preferences.draft.theme, crate::preferences::Theme::Light);
  let ok = find(&mut h, "OK");
  h.click(ok);
  assert!(!h.app.preferences.open);
  assert!(!theme::is_dark(&h.ctx));
  assert!(rects(&h.frame(vec![]), theme::APP_BAR) > 0, "the light chrome is back");
}
