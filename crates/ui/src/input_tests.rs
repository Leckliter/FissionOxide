//! Pointer input driven through a real egui context: clicks must land on what's drawn on top.

use egui::{Event, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;

use crate::App;

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
}

impl Harness {
  fn new() -> Harness {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness { ctx, app: App::with_session(Session::default()), time: 0.0 };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  fn frame(&mut self, events: Vec<Event>) {
    self.time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    self.ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
  }

  fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
  }

  /// The pointer jumps to `pos` and clicks, all between two frames (as remote desktop,
  /// tablets and automation deliver it).
  fn jump_and_click(&mut self, pos: Pos2) {
    self.frame(vec![Event::PointerMoved(pos), Self::button(pos, true), Self::button(pos, false)]);
    self.frame(vec![]);
  }

  /// The pointer moves to `pos` and clicks a frame later.
  fn move_then_click(&mut self, pos: Pos2) {
    self.frame(vec![Event::PointerMoved(pos)]);
    self.frame(vec![Self::button(pos, true)]);
    self.frame(vec![Self::button(pos, false)]);
    self.frame(vec![]);
  }

  /// A left-button drag from `a` to `b` over a few frames.
  fn drag(&mut self, a: Pos2, b: Pos2) {
    self.frame(vec![Event::PointerMoved(a)]);
    self.frame(vec![Self::button(a, true)]);
    for i in 1..=6 {
      self.frame(vec![Event::PointerMoved(a + (b - a) * i as f32 / 6.0)]);
    }
    self.frame(vec![Self::button(b, false)]);
    self.frame(vec![]);
  }

  fn dialog_rect(&self) -> Rect {
    self.ctx.memory(|m| m.area_rect(egui::Id::new("feature-dialog"))).expect("the dialog is open")
  }

  fn open_create_sketch(&mut self) {
    self.app.new_sketch();
    for _ in 0..3 {
      self.frame(vec![]);
    }
    assert!(self.app.dialog.is_some());
  }
}

/// Cancel sits at the bottom right of the dialog's button row.
fn cancel_pos(dialog: Rect) -> Pos2 {
  dialog.right_bottom() + vec2(-10.0 - 36.0, -7.0 - 12.0)
}

#[test]
fn the_create_sketch_dialog_takes_clicks_where_it_opens() {
  let mut h = Harness::new();
  h.open_create_sketch();
  let r = h.dialog_rect();
  assert!(r.right() > h.app.canvas.right() - 60.0, "the dialog opens on the right of the canvas: {r:?}");
  h.move_then_click(cancel_pos(r));
  assert!(h.app.dialog.is_none(), "Cancel closes the dialog");
}

#[test]
fn a_click_right_after_a_jump_lands() {
  let mut h = Harness::new();
  h.open_create_sketch();
  let r = h.dialog_rect();
  h.jump_and_click(cancel_pos(r));
  assert!(h.app.dialog.is_none(), "Cancel closes the dialog");
}

#[test]
fn the_viewcube_home_button_and_faces_take_clicks() {
  let mut h = Harness::new();
  let canvas = h.app.canvas;
  let center = canvas.right_top() + vec2(-92.0, 80.0);
  h.app.cam.set_view(fox_engine::render::StandardView::Front);
  h.app.cam.orbit(0.2, 0.1);
  h.frame(vec![]);
  let before = h.app.cam;
  // turned a little off Front, the middle of the cube is still its FRONT face
  h.move_then_click(center);
  assert_ne!(h.app.cam, before, "clicking the cube's face changes the view");
  let before = h.app.cam;
  h.move_then_click(center + vec2(-62.0, -50.0));
  assert_ne!(h.app.cam, before, "Home changes the view");
}

#[test]
fn dragging_the_viewcube_orbits() {
  let mut h = Harness::new();
  let center = h.app.canvas.right_top() + vec2(-92.0, 80.0);
  let before = h.app.cam;
  h.drag(center + vec2(10.0, 10.0), center + vec2(-50.0, 30.0));
  assert_ne!(h.app.cam, before, "the view turned");
  assert!(h.app.drag.is_none() && h.app.s.selection.is_empty(), "no selection box");
}

#[test]
fn a_dialog_over_the_viewcube_takes_the_click() {
  let mut h = Harness::new();
  h.open_create_sketch();
  let center = h.app.canvas.right_top() + vec2(-92.0, 80.0);
  // drag the dialog by its header until its body covers the cube
  let r = h.dialog_rect();
  let grab = r.left_top() + vec2(150.0, 12.0);
  let body = r.left_top() + vec2(40.0, 70.0);
  h.drag(grab, grab + (center - body));
  let r = h.dialog_rect();
  assert!(r.contains(center), "the dialog covers the cube: {r:?}");
  let before = h.app.cam;
  h.move_then_click(center);
  assert_eq!(h.app.cam, before, "the cube under the dialog doesn't change the view");
  h.move_then_click(cancel_pos(r));
  assert!(h.app.dialog.is_none(), "Cancel closes the dialog");
}

/// Every piece of text drawn, with where.
fn texts(out: &egui::FullOutput) -> Vec<(Rect, String)> {
  fn walk(s: &egui::Shape, out: &mut Vec<(Rect, String)>) {
    match s {
      egui::Shape::Text(t) => out.push((t.visual_bounding_rect(), t.galley.text().to_string())),
      egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
      _ => {}
    }
  }
  let mut all = Vec::new();
  for c in &out.shapes {
    walk(&c.shape, &mut all);
  }
  all
}

impl Harness {
  /// A frame `dt` seconds after the last, returning the text drawn.
  fn frame_texts(&mut self, dt: f64, events: Vec<Event>) -> Vec<(Rect, String)> {
    self.time += dt;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    let out = self.ctx.run_ui(input, |ui| app.draw(ui));
    let t = texts(&out);
    out.drop_without_applying_deltas();
    t
  }

  /// Where `text` is drawn (the first match), after a frame with `events`.
  fn find(&mut self, events: Vec<Event>, text: &str) -> Option<Rect> {
    self.frame_texts(1.0 / 60.0, events).into_iter().find(|(_, t)| t == text).map(|(r, _)| r)
  }

  /// Rests the pointer at `pos` for `secs`, returning the text drawn at the end.
  fn rest(&mut self, pos: Pos2, secs: f64) -> Vec<String> {
    self.frame_texts(1.0 / 60.0, vec![Event::PointerMoved(pos)]);
    let mut last = Vec::new();
    for _ in 0..(secs / 0.1) as usize {
      last = self.frame_texts(0.1, vec![]);
    }
    last.into_iter().map(|(_, t)| t).collect()
  }

  /// The SOLID tab's CREATE ▾ group label.
  fn create_label(&mut self) -> Rect {
    let all = self.frame_texts(1.0 / 60.0, vec![]);
    all.into_iter().find(|(_, t)| t.starts_with("CREATE")).map(|(r, _)| r).expect("CREATE is on the toolbar")
  }
}

#[test]
fn the_extrude_tooltip_shows_its_shortcut_then_grows() {
  let mut h = Harness::new();
  let label = h.create_label();
  let tip = crate::tips::tip(crate::tips::Scope::Design, "Extrude").unwrap();
  // the toolbar icons sit above the group label; find Extrude's by hovering along them
  let y = label.top() - 22.0;
  let mut found = None;
  let mut x = label.left() - 80.0;
  while x < label.right() + 200.0 && found.is_none() {
    let shown = h.rest(Pos2::new(x, y), 0.8);
    if shown.iter().any(|t| t == "Extrude") {
      found = Some((x, shown));
    }
    x += 12.0;
  }
  let (x, shown) = found.expect("an icon shows the Extrude tooltip");
  assert!(shown.iter().any(|t| t == tip.line), "the one-line description: {shown:?}");
  assert!(shown.iter().any(|t| t == "Shortcut: E"), "{shown:?}");
  assert!(!shown.iter().any(|t| t == tip.more), "not expanded yet");
  let shown = h.rest(Pos2::new(x, y), 2.5);
  assert!(shown.iter().any(|t| t == tip.more), "expanded after resting: {shown:?}");
  // the user's key shows instead
  h.app.prefs.keys.set("design.Extrude", "Shift+X", "E");
  let shown = h.rest(Pos2::new(x + 1.0, y), 0.8);
  assert!(shown.iter().any(|t| t == "Shortcut: Shift+X"), "{shown:?}");
}

/// Opens CREATE ▾ and opens the ⋮ options of `row`, returning where "Pin to Toolbar" is.
fn row_options(h: &mut Harness, row: &str, option: &str) -> Pos2 {
  let label = h.create_label();
  h.move_then_click(label.center());
  let r = h.find(vec![], row).unwrap_or_else(|| panic!("{row} is in the CREATE menu"));
  h.frame(vec![Event::PointerMoved(r.center())]);
  let dots = h.find(vec![], "⋮").expect("the hovered row shows ⋮");
  assert!((dots.center().y - r.center().y).abs() < 6.0, "on the {row} row");
  h.frame(vec![Event::PointerMoved(dots.center())]);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  h.find(vec![], option).unwrap_or_else(|| panic!("the options list {option}")).center()
}

#[test]
fn pin_to_toolbar_and_add_to_shortcuts_from_a_menu_row() {
  let mut h = Harness::new();
  let at = row_options(&mut h, "Torus", "Pin to Toolbar");
  h.move_then_click(at);
  assert_eq!(h.app.prefs.pinned, ["SOLID/CREATE/Torus"]);
  let at = row_options(&mut h, "Torus", "Add to Shortcuts");
  h.move_then_click(at);
  assert_eq!(h.app.prefs.shortcuts, ["design.Torus"]);
  // and now it offers to undo both
  let at = row_options(&mut h, "Torus", "Unpin from Toolbar");
  h.move_then_click(at);
  assert!(h.app.prefs.pinned.is_empty());
  row_options(&mut h, "Torus", "Remove from Shortcuts");
}

#[test]
fn change_keyboard_shortcut_captures_a_key_and_warns_on_a_clash() {
  let mut h = Harness::new();
  let at = row_options(&mut h, "Torus", "Change Keyboard Shortcut...");
  h.move_then_click(at);
  assert!(h.app.key_capture.is_some());
  let key = |key, modifiers| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
  // E is Extrude's
  let shown: Vec<String> =
    h.frame_texts(1.0 / 60.0, vec![key(egui::Key::E, egui::Modifiers::NONE)]).into_iter().map(|t| t.1).collect();
  assert!(shown.iter().any(|t| t.starts_with("Already used by Extrude")), "{shown:?}");
  h.frame_texts(1.0 / 60.0, vec![key(egui::Key::T, egui::Modifiers::SHIFT)]);
  let ok = h.find(vec![], "OK").expect("the OK button");
  h.move_then_click(ok.center());
  assert!(h.app.key_capture.is_none());
  assert_eq!(crate::menus::key_of(&h.app.prefs, crate::tips::Scope::Design, "Torus"), "Shift+T");
  assert_eq!(crate::menus::key_of(&h.app.prefs, crate::tips::Scope::Design, "Extrude"), "E");
}
