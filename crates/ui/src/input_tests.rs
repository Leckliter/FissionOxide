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

fn key(key: egui::Key) -> Event {
  Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
}

/// Keys as a keyboard delivers a typed value: each digit's key press and its text.
fn typing(s: &str) -> Vec<Event> {
  let mut ev = Vec::new();
  for c in s.chars() {
    if let Some(k) = egui::Key::from_name(&c.to_string()) {
      ev.push(key(k));
    }
    ev.push(Event::Text(c.to_string()));
  }
  ev
}

/// The sketch's driving dimension values, in creation order.
fn dims(app: &App, sk: fox_engine::model::FeatureId) -> Vec<f64> {
  let s = app.s.design.sketch(sk).unwrap();
  s.constraints.iter().filter_map(|c| c.kind.dim().map(|d| d.value)).collect()
}

/// Tab moves between the heads-up boxes of every tool with more than one, as in Fusion:
/// `40 Tab 20 Enter` sets both values.
#[test]
fn tab_moves_between_the_heads_up_boxes() {
  use crate::viewport::Tool;
  for tool in [Tool::Rect, Tool::CenterRect, Tool::Line] {
    let mut h = Harness::new();
    h.app.run("sketch.create", serde_json::json!({"plane": "xy"})).unwrap();
    let sk = h.app.s.edit_sketch.unwrap();
    h.app.set_tool(tool);
    let c = h.app.canvas.center();
    h.move_then_click(c);
    h.frame(vec![Event::PointerMoved(c + vec2(60.0, -40.0))]);
    h.frame(vec![]);
    h.frame(typing("40"));
    h.frame(vec![]);
    h.frame(vec![key(egui::Key::Tab)]);
    h.frame(vec![]);
    h.frame(typing("20"));
    h.frame(vec![]);
    h.frame(vec![key(egui::Key::Enter)]);
    h.frame(vec![]);
    let d = dims(&h.app, sk);
    if tool == Tool::Line {
      // the angle box sets the line's direction
      let s = h.app.s.design.sketch(sk).unwrap();
      let c = s.curves.last().unwrap().id;
      let [a, b] = [0, 1].map(|k| s.pos(s.curve_points(c)[k]).unwrap());
      let ang = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
      assert!(d == [40.0] && (ang - 20.0).abs() < 1e-6, "{tool:?}: {d:?} at {ang}°");
    } else {
      assert_eq!(d, [40.0, 20.0], "{tool:?}");
    }
  }
}
