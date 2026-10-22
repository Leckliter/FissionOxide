//! Pointer input driven through a real egui context: clicks must land on what's drawn on top.

use egui::{Event, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;
use serde_json::json;

use crate::App;

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
  /// The window width (1440 unless a test narrows it).
  width: f32,
  /// The modifier keys held during each frame (Ctrl for a Ctrl+click), and the ones last told
  /// to egui (a change is sent as the frame's first event).
  mods: egui::Modifiers,
  sent_mods: egui::Modifiers,
}

impl Harness {
  fn new() -> Harness {
    Harness::with_width(1440.0)
  }

  fn with_width(width: f32) -> Harness {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness {
      ctx,
      app: App::with_session(Session::default()),
      time: 0.0,
      width,
      mods: egui::Modifiers::NONE,
      sent_mods: egui::Modifiers::NONE,
    };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  fn frame(&mut self, events: Vec<Event>) {
    self.frame_in(vec2(self.width, 900.0), events);
  }

  /// A frame in a window of `size` (the window was resized or maximised).
  fn frame_in(&mut self, size: egui::Vec2, mut events: Vec<Event>) {
    self.time += 1.0 / 60.0;
    if self.mods != self.sent_mods {
      events.insert(0, Event::ModifiersChanged(self.mods));
      self.sent_mods = self.mods;
    }
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
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

  /// Opens a command dialog (Box, its base drawn on the XY plane: it needs no picks) and lets it
  /// settle.
  fn open_dialog(&mut self) {
    self.app.start(crate::dialogs::Kind::Box);
    if let Some(d) = self.app.dialog.as_mut() {
      d.place_on(&self.app.s, json!("xy"));
      d.place_click([0.0, 0.0]);
      d.place_click([20.0, 10.0]);
    }
    for _ in 0..3 {
      self.frame(vec![]);
    }
    assert!(self.app.dialog.is_some());
  }

  /// Where the dialog docks: the canvas's top-right corner (`Dialog::show`).
  fn dock(&self) -> Pos2 {
    self.app.canvas.right_top() + vec2(-34.0, 190.0)
  }
}

/// Cancel sits at the bottom right of the dialog's button row.
fn cancel_pos(dialog: Rect) -> Pos2 {
  dialog.right_bottom() + vec2(-10.0 - 36.0, -7.0 - 12.0)
}

/// Fusion's cursor prompt: the command's step follows the pointer over the canvas and hides
/// over the dialog; a sketch tool's step too, with nothing written in the canvas corner.
#[test]
fn the_cursor_prompt_follows_the_pointer_and_hides_over_the_dialog() {
  let mut h = Harness::new();
  let c = h.app.canvas.center();
  // Create Sketch has no dialog: the prompt just below-right of the pointer is all that asks
  h.app.new_sketch();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  assert!(h.prompt_at(c, crate::dialogs::PICK_PLANE_HINT));
  h.app.new_sketch(); // the ribbon button cancels the pick
  // a selecting dialog's prompt hides over the dialog
  h.app.run("box", serde_json::json!({"width": 10, "depth": 10, "height": 10}));
  h.app.start(crate::dialogs::Kind::Fillet);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let r = h.dialog_rect();
  let select = "Select edges, faces, or features to fillet";
  assert!(h.prompt_at(c, select));
  assert!(!h.prompt_at(r.center(), select), "hidden over the dialog");
  h.move_then_click(cancel_pos(r));
  assert!(h.app.dialog.is_none());

  h.app.run("sketch.create", serde_json::json!({"plane": "xy"})).unwrap();
  h.app.set_tool(crate::viewport::Tool::Rect);
  assert!(h.prompt_at(c, "Place first corner"));
  let all = h.rest(c, 0.1);
  assert!(!all.iter().any(|t| t.starts_with("Sketching")), "{all:?}");
  h.move_then_click(c);
  assert!(h.prompt_at(c + vec2(60.0, -40.0), "Specify size of rectangle"));
}

impl Harness {
  /// Is `text` drawn just below-right of the pointer resting at `pos` (as the cursor prompt is)?
  fn prompt_at(&mut self, pos: Pos2, text: &str) -> bool {
    self.frame_texts(1.0 / 60.0, vec![Event::PointerMoved(pos)]);
    let near = Rect::from_min_size(pos, vec2(80.0, 60.0));
    self.frame_texts(0.1, vec![]).iter().any(|(r, t)| t == text && near.contains(r.min))
  }
}

#[test]
fn a_dialog_takes_clicks_where_it_opens() {
  let mut h = Harness::new();
  h.open_dialog();
  let r = h.dialog_rect();
  assert!(r.right() > h.app.canvas.right() - 60.0, "the dialog opens on the right of the canvas: {r:?}");
  h.move_then_click(cancel_pos(r));
  assert!(h.app.dialog.is_none(), "Cancel closes the dialog");
}

/// Fusion's Text frame is placed by two clicks on the sketch (a drag's release places nothing):
/// the first corner moves the prompt on to the second, the second places the frame with
/// "Sample Text" in it, and the Text box then has the keyboard, so typing replaces the sample.
#[test]
fn text_frame_is_placed_by_two_clicks_and_typing_replaces_the_sample() {
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  h.app.start(crate::dialogs::Kind::Form(&crate::forms::SK_TEXT));
  h.frame(vec![]);
  // (the second corner goes down and left: the dialog docks left of the Sketch Palette, over
  // the canvas's upper right)
  let c = h.app.canvas.center();
  assert!(h.prompt_at(c, "Place first corner of text frame"));
  // a drag places nothing
  h.drag(c, c + vec2(-120.0, 60.0));
  let d = h.app.dialog.as_ref().expect("the dialog stays open");
  assert!(d.text_frame_start().is_none() && d.text_frame().is_none());
  h.move_then_click(c);
  assert!(h.app.dialog.as_ref().unwrap().text_frame_start().is_some(), "the first corner is placed");
  assert!(h.prompt_at(c + vec2(-60.0, 30.0), "Place second corner of text frame"));
  h.move_then_click(c + vec2(-120.0, 60.0));
  let d = h.app.dialog.as_ref().unwrap();
  assert!(d.text_frame().is_some(), "the second click places the frame");
  assert_eq!(d.form_value("text"), Some(&json!("Sample Text")));
  let sk = h.app.s.edit_sketch.unwrap();
  let texts = &h.app.s.design.sketch(sk).unwrap().texts;
  assert_eq!(texts.first().map(|t| t.text.as_str()), Some("Sample Text"), "the sample text is in the frame");
  // the Text box has the keyboard: typing replaces the sample text live
  h.frame(typing("Hi"));
  h.frame(vec![]);
  assert_eq!(h.app.dialog.as_ref().unwrap().form_value("text"), Some(&json!("Hi")));
  h.frame(vec![]);
  let texts = &h.app.s.design.sketch(sk).unwrap().texts;
  assert_eq!(texts.first().map(|t| t.text.as_str()), Some("Hi"));
}

/// Tab with nothing focused used to give egui's keyboard focus to the first widget drawn, the Data
/// Panel button, which Enter then pressed: the panel opened on its own mid-sketch. Fusion's Tab
/// never reaches the toolbar.
#[test]
fn tab_then_enter_leaves_the_data_panel_alone() {
  let mut h = Harness::new();
  assert!(!h.app.data.open);
  h.frame(vec![key(egui::Key::Tab)]);
  h.frame(vec![]);
  assert!(!h.ctx.memory(|m| m.focused().is_some()), "Tab focused a widget");
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert!(!h.app.data.open, "Enter pressed the Data Panel button");
  // in a sketch with a one-box shape (a circle's diameter) too
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  h.app.set_tool(crate::Tool::Circle);
  h.frame(vec![]);
  let c = h.app.canvas.center();
  h.move_then_click(c);
  assert!(!h.app.tstate.is_empty(), "the centre is down");
  h.frame(vec![key(egui::Key::Tab)]);
  h.frame(vec![]);
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert!(!h.app.data.open);
}

#[test]
fn a_click_right_after_a_jump_lands() {
  let mut h = Harness::new();
  h.open_dialog();
  let r = h.dialog_rect();
  h.jump_and_click(cancel_pos(r));
  assert!(h.app.dialog.is_none(), "Cancel closes the dialog");
}

#[test]
fn create_sketch_has_no_dialog() {
  let mut h = Harness::new();
  h.app.new_sketch();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  assert!(h.app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::PickPlane));
  assert!(h.ctx.memory(|m| m.area_rect(egui::Id::new("feature-dialog"))).is_none(), "only the cursor hint asks");
  assert!(crate::panels::Cmd::NewSketch.active(&h.app), "the ribbon button stays pressed");
  // the button cancels
  h.app.new_sketch();
  assert!(h.app.dialog.is_none());
  assert!(!crate::panels::Cmd::NewSketch.active(&h.app));
}

#[test]
fn a_dialog_docks_top_right_when_it_opens_and_after_a_resize() {
  let mut h = Harness::new();
  h.open_dialog();
  let r = h.dialog_rect();
  assert!((r.right_top() - h.dock()).length() < 1.0, "docked at {:?}, canvas {:?}", r, h.app.canvas);
  // dragged by its title bar, it stays where it was dropped for this command
  let grab = r.left_top() + vec2(90.0, 15.0);
  h.drag(grab, grab + vec2(-300.0, 120.0));
  let moved = h.dialog_rect();
  assert!((moved.left_top() - (r.left_top() + vec2(-300.0, 120.0))).length() < 2.0, "{moved:?} after {r:?}");
  for _ in 0..3 {
    h.frame(vec![]);
  }
  assert!((h.dialog_rect().left_top() - moved.left_top()).length() < 1.0, "it keeps its dragged position");
  // the next command opens docked again, not where the last one was dragged
  h.move_then_click(cancel_pos(moved));
  assert!(h.app.dialog.is_none());
  h.open_dialog();
  let r = h.dialog_rect();
  assert!((r.right_top() - h.dock()).length() < 1.0, "re-docked at {r:?}, not {moved:?}");
  // maximising the window moves the canvas's corner: the open dialog follows it
  for _ in 0..4 {
    h.frame_in(vec2(1920.0, 1080.0), vec![]);
  }
  let r = h.dialog_rect();
  assert!(h.app.canvas.right() > 1600.0, "the canvas grew: {:?}", h.app.canvas);
  assert!((r.right_top() - h.dock()).length() < 1.0, "docked to the new corner: {r:?}, canvas {:?}", h.app.canvas);
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
  h.open_dialog();
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
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 900.0))),
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
    self.rest_rects(pos, secs).into_iter().map(|(_, t)| t).collect()
  }

  /// [`Self::rest`] with where each text is drawn.
  fn rest_rects(&mut self, pos: Pos2, secs: f64) -> Vec<(Rect, String)> {
    self.frame_texts(1.0 / 60.0, vec![Event::PointerMoved(pos)]);
    let mut last = Vec::new();
    for _ in 0..(secs / 0.1) as usize {
      last = self.frame_texts(0.1, vec![]);
    }
    last
  }

  /// Where the SOLID tab's open CREATE ▾ menu is (its popup, frame included).
  fn create_menu_rect(&self) -> Rect {
    let id = egui::Id::new(("toolbar-menu", crate::panels::Tab::Solid, "CREATE"));
    egui::AreaState::load(&self.ctx, id).expect("the CREATE menu is open").rect()
  }

  /// The SOLID tab's CREATE ▾ group label.
  fn create_label(&mut self) -> Rect {
    let all = self.frame_texts(1.0 / 60.0, vec![]);
    all.into_iter().find(|(_, t)| t.starts_with("CREATE")).map(|(r, _)| r).expect("CREATE is on the toolbar")
  }
}

impl Harness {
  /// Every shape drawn `dt` seconds after the last frame, groups flattened.
  fn frame_shapes(&mut self, dt: f64) -> Vec<egui::Shape> {
    fn walk(s: egui::Shape, out: &mut Vec<egui::Shape>) {
      match s {
        egui::Shape::Vec(v) => v.into_iter().for_each(|s| walk(s, out)),
        s => out.push(s),
      }
    }
    self.time += dt;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 900.0))),
      time: Some(self.time),
      ..Default::default()
    };
    let app = &mut self.app;
    let out = self.ctx.run_ui(input, |ui| app.draw(ui));
    let mut all = Vec::new();
    for c in out.shapes {
      walk(c.shape, &mut all);
    }
    all
  }

  /// Rests along the ribbon above the panel whose name starts with `panel` until a button shows
  /// the tooltip titled `title`; returns the first such pointer x, the y and the texts drawn.
  fn ribbon_tip(&mut self, panel: &str, title: &str) -> (f32, f32, Vec<(Rect, String)>) {
    let all = self.frame_texts(1.0 / 60.0, vec![]);
    let label = all
      .into_iter()
      .find(|(r, t)| t.starts_with(panel) && r.top() < 140.0)
      .map(|(r, _)| r)
      .unwrap_or_else(|| panic!("{panel} is on the toolbar"));
    // the toolbar icons sit above the group label; find the button by hovering along them
    let y = label.top() - 22.0;
    let mut x = label.left() - 80.0;
    while x < label.right() + 200.0 {
      let shown = self.rest_rects(Pos2::new(x, y), 1.2);
      if shown.iter().any(|(_, t)| t == title) {
        return (x, y, shown);
      }
      x += 12.0;
    }
    panic!("no button shows the {title} tooltip");
  }
}

/// The colour a text shape is drawn in.
fn text_color(t: &egui::epaint::TextShape) -> egui::Color32 {
  t.override_text_color
    .or_else(|| t.galley.job.sections.first().map(|s| s.format.color).filter(|c| *c != egui::Color32::PLACEHOLDER))
    .unwrap_or(t.fallback_color)
}

/// The Extrude button's tooltip is Fusion's: a dark panel under the button with its left edge
/// at the button's, "Extrude (E)" in white, the description and usage in light grey at once, and
/// after resting longer a large illustration with "Press Ctrl+/ for more help." under it.
#[test]
fn the_extrude_tooltip_is_fusions_dark_panel_then_grows() {
  use crate::{ribbon_tip, theme, tips};
  let mut h = Harness::new();
  let tip = tips::tip(tips::Scope::Design, "Extrude").unwrap();
  let (x, y, shown) = h.ribbon_tip("CREATE", "Extrude (E)");
  let at = |shown: &[(Rect, String)], text: &str| {
    shown.iter().find(|(_, t)| t == text).map(|(r, _)| *r).unwrap_or_else(|| panic!("{text:?} in {shown:?}"))
  };
  let title = at(&shown, "Extrude (E)");
  assert!(title.top() > y, "under the button: {title:?}");
  // the first x that shows it is within a step of the button's left edge; the panel's 10 px
  // margin sits inside that edge
  assert!((title.left() - 10.0 - x).abs() < 14.0, "left edges aligned: {title:?} vs x {x}");
  let mut bottom = title.bottom();
  for p in tip.paragraphs() {
    let r = at(&shown, p);
    assert!(r.top() > bottom, "{p:?} below the one before");
    bottom = r.bottom();
  }
  assert!(!shown.iter().any(|(_, t)| t.starts_with("Shortcut:")), "the key is in the title: {shown:?}");
  assert!(!shown.iter().any(|(_, t)| t == tips::MORE_HELP), "not expanded yet");
  // the panel is the dark tooltip fill, the title white and the paragraphs light grey
  h.frame_texts(1.0 / 60.0, vec![egui::Event::PointerMoved(Pos2::new(x, y))]);
  let shapes = h.frame_shapes(0.1);
  assert!(
    shapes.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == theme::TOOLTIP && r.rect.contains_rect(title))),
    "a dark panel behind the title"
  );
  let color = |text: &str| {
    shapes.iter().find_map(|s| match s {
      egui::Shape::Text(t) if t.galley.text() == text => Some(text_color(t)),
      _ => None,
    })
  };
  assert_eq!(color("Extrude (E)"), Some(theme::TOOLTIP_TITLE));
  assert_eq!(color(tip.line), Some(theme::TOOLTIP_TEXT));
  assert_eq!(color(tip.more), Some(theme::TOOLTIP_TEXT));
  // resting longer adds the illustration and the help line under it
  let shown = h.rest_rects(Pos2::new(x, y), 2.5);
  let help = at(&shown, tips::MORE_HELP);
  assert!(help.top() > at(&shown, tip.more).bottom() + ribbon_tip::ILLUSTRATION_H, "under the picture: {help:?}");
  // the user's key shows instead
  h.app.prefs.keys.set("design.Extrude", "Shift+X", "E");
  let shown = h.rest(Pos2::new(x + 1.0, y), 1.2);
  assert!(shown.iter().any(|t| t == "Extrude (Shift+X)"), "{shown:?}");
}

/// Commands without Fusion's illustration, such as Offset Plane, show the help line at once and
/// never grow; commands without a shortcut, such as Revolve, show the bare name.
#[test]
fn the_offset_plane_tooltip_never_grows() {
  use crate::tips;
  let mut h = Harness::new();
  let (x, y, shown) = h.ribbon_tip("CONSTRUCT", "Offset Plane");
  let tip = tips::tip(tips::Scope::Design, "Offset Plane").unwrap();
  let texts: Vec<String> = shown.into_iter().map(|(_, t)| t).collect();
  for p in tip.paragraphs().chain([tips::MORE_HELP]) {
    assert!(texts.iter().any(|t| t == p), "{p:?} in {texts:?}");
  }
  let later = h.rest(Pos2::new(x, y), 3.0);
  assert_eq!(later, texts, "the panel doesn't grow");
  let (_, _, shown) = h.ribbon_tip("CREATE", "Revolve");
  assert!(!shown.iter().any(|(_, t)| t.starts_with("Revolve (")), "no shortcut: {shown:?}");
}

/// A menu row's tooltip is Fusion's: the description and instructions as paragraphs in a panel
/// to the right of the menu, level with the row, with no title and no shortcut line.
#[test]
fn a_menu_rows_tooltip_sits_right_of_the_menu_without_a_title() {
  let mut h = Harness::new();
  let label = h.create_label();
  h.move_then_click(label.center());
  let row = h.find(vec![], "Extrude").expect("Extrude is in the CREATE menu");
  let menu = h.create_menu_rect();
  let tip = crate::tips::tip(crate::tips::Scope::Design, "Extrude").unwrap();
  // egui shows no tooltip for a pointer that moved within 0.1 s of a click
  h.frame_texts(0.3, vec![]);
  let shown = h.rest_rects(row.center(), 0.8);
  let at = |text: &str| shown.iter().find(|(_, t)| t == text).map(|(r, _)| *r);
  for paragraph in std::iter::once(tip.line).chain(tip.more.split("\n\n")) {
    let r = at(paragraph).unwrap_or_else(|| panic!("{paragraph:?} shown: {shown:?}"));
    assert!(r.left() > menu.right(), "right of the menu: {r:?} vs {menu:?}");
    assert!(r.top() >= row.top() - 12.0 && r.top() < menu.bottom(), "level with the row: {r:?} vs {row:?}");
  }
  assert!(!shown.iter().any(|(_, t)| t.starts_with("Shortcut:")), "the row shows the key: {shown:?}");
  assert_eq!(shown.iter().filter(|(_, t)| t == "Extrude").count(), 1, "no title line: {shown:?}");
}

/// Opens CREATE ▾ and opens the ⋮ options of `row`, returning where "Pin to Toolbar" is.
fn row_options(h: &mut Harness, row: &str, option: &str) -> Pos2 {
  let label = h.create_label();
  h.move_then_click(label.center());
  let r = h.find(vec![], row).unwrap_or_else(|| panic!("{row} is in the CREATE menu"));
  h.frame(vec![Event::PointerMoved(r.center())]);
  // the hovered row's ⋮ is painted as dots at the menu's right edge (the font has no glyph, so
  // no text must stand in for it)
  assert!(h.find(vec![], "⋮").is_none(), "no missing-glyph box for the options");
  let dots = Pos2::new(h.create_menu_rect().right() - 16.0, r.center().y);
  h.frame(vec![Event::PointerMoved(dots)]);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  h.find(vec![], option).unwrap_or_else(|| panic!("the options list {option}")).center()
}

#[test]
fn pin_to_toolbar_and_pin_to_shortcuts_are_checkboxes_on_a_menu_row() {
  let mut h = Harness::new();
  // Fusion's options: two checkboxes and Change Keyboard Shortcut, nothing else
  row_options(&mut h, "Torus", "Pin to Toolbar");
  let shown: Vec<String> = h.frame_texts(1.0 / 60.0, vec![]).into_iter().map(|t| t.1).collect();
  for gone in ["Pin to Panel", "Add to Shortcuts", "Remove from Shortcuts", "Unpin from Toolbar"] {
    assert!(!shown.iter().any(|t| t == gone), "no {gone}: {shown:?}");
  }
  assert!(shown.iter().any(|t| t == "Pin to Shortcuts") && shown.iter().any(|t| t == "Change Keyboard Shortcut..."));
  h.frame(vec![Event::Key {
    key: egui::Key::Escape,
    physical_key: None,
    pressed: true,
    repeat: false,
    modifiers: egui::Modifiers::NONE,
  }]);
  h.frame(vec![]);
  let at = row_options(&mut h, "Torus", "Pin to Toolbar");
  h.move_then_click(at);
  assert_eq!(h.app.prefs.pinned, ["SOLID/CREATE/Torus"]);
  let at = row_options(&mut h, "Torus", "Pin to Shortcuts");
  h.move_then_click(at);
  assert_eq!(h.app.prefs.shortcuts, ["design.Torus"]);
  // the same checkboxes clear both
  let at = row_options(&mut h, "Torus", "Pin to Toolbar");
  h.move_then_click(at);
  assert!(h.app.prefs.pinned.is_empty());
  let at = row_options(&mut h, "Torus", "Pin to Shortcuts");
  h.move_then_click(at);
  assert!(h.app.prefs.shortcuts.is_empty());
  // Extrude starts with both checked: unchecking them unpins it from the toolbar and the box
  assert!(h.app.prefs.in_shortcuts("design.Extrude"));
  let at = row_options(&mut h, "Extrude", "Pin to Shortcuts");
  h.move_then_click(at);
  assert!(!h.app.prefs.in_shortcuts("design.Extrude"));
  let at = row_options(&mut h, "Extrude", "Pin to Toolbar");
  h.move_then_click(at);
  assert!(!h.app.prefs.shows(crate::panels::Tab::Solid, "CREATE", "Extrude", true));
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

#[test]
fn change_keyboard_shortcut_is_fusions_modal() {
  let mut h = Harness::new();
  let at = row_options(&mut h, "Extrude", "Change Keyboard Shortcut...");
  h.move_then_click(at);
  let shown = h.frame_texts(1.0 / 60.0, vec![]);
  let at = |text: &str| shown.iter().find(|(_, t)| t == text).map(|(r, _)| *r);
  for text in [
    "Change Keyboard Shortcut",
    "Assign a new shortcut key for Extrude:",
    "Hints:",
    "You can combine modifiers like Shift, Control, Alt/Option, and Command with an alphanumeric key.",
    "Leave blank to remove the shortcut.",
  ] {
    assert!(at(text).is_some(), "{text:?} shown: {shown:?}");
  }
  // the field holds the current key in lower case
  let field = at("e").unwrap_or_else(|| panic!("the key in the field: {shown:?}"));
  let prompt = at("Assign a new shortcut key for Extrude:").unwrap();
  assert!(field.top() > prompt.bottom() - 1.0, "under the prompt");
  for old in ["Press the new key, with Ctrl, Shift or Alt if you like.", "No Shortcut", "Default", "E"] {
    assert!(at(old).is_none(), "no {old:?}: {shown:?}");
  }
  // Reset to default at the left, then Cancel and OK at the right
  let (reset, cancel, ok) = (at("Reset to default").unwrap(), at("Cancel").unwrap(), at("OK").unwrap());
  assert!(reset.right() < cancel.left() && cancel.right() < ok.left(), "{reset:?} {cancel:?} {ok:?}");
  assert!((reset.center().y - ok.center().y).abs() < 2.0, "one row");
  // OK is grey until the key changes
  h.move_then_click(ok.center());
  assert!(h.app.key_capture.is_some(), "OK does nothing before the key changes");
  // Backspace leaves it blank, and OK then removes the shortcut
  let back = Event::Key {
    key: egui::Key::Backspace,
    physical_key: None,
    pressed: true,
    repeat: false,
    modifiers: egui::Modifiers::NONE,
  };
  h.frame(vec![back]);
  let ok = h.find(vec![], "OK").unwrap();
  h.move_then_click(ok.center());
  assert!(h.app.key_capture.is_none());
  assert_eq!(crate::menus::key_of(&h.app.prefs, crate::tips::Scope::Design, "Extrude"), "");
  // Reset to default brings E back
  let at = row_options(&mut h, "Extrude", "Change Keyboard Shortcut...");
  h.move_then_click(at);
  let reset = h.find(vec![], "Reset to default").unwrap();
  h.move_then_click(reset.center());
  let ok = h.find(vec![], "OK").unwrap();
  h.move_then_click(ok.center());
  assert_eq!(crate::menus::key_of(&h.app.prefs, crate::tips::Scope::Design, "Extrude"), "E");
}

#[test]
fn the_s_toolbox_is_titled_design_shortcuts_centred_with_a_search_field() {
  let mut h = Harness::new();
  let mid = h.app.canvas.center();
  h.frame(vec![Event::PointerMoved(mid)]);
  let s = Event::Key {
    key: egui::Key::S,
    physical_key: None,
    pressed: true,
    repeat: false,
    modifiers: egui::Modifiers::NONE,
  };
  h.frame(vec![s, Event::Text("s".into())]);
  assert!(h.app.toolbox.is_some(), "S opens the box");
  let shown = h.frame_texts(1.0 / 60.0, vec![]);
  let at = |text: &str| shown.iter().find(|(_, t)| t == text).map(|(r, _)| *r);
  let title = at("Design Shortcuts").unwrap_or_else(|| panic!("the title: {shown:?}"));
  assert!(at("DESIGN SHORTCUTS").is_none() && at("Search commands").is_none(), "{shown:?}");
  assert!(at("Search").is_some(), "the field's hint: {shown:?}");
  let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("s-toolbox"))).expect("the box is shown");
  assert!((title.center().x - area.center().x).abs() < 3.0, "centred: {title:?} in {area:?}");
}

#[test]
fn ctrl_s_in_a_test_saves_to_a_scratch_hub() {
  let mut h = Harness::new();
  let root = h.app.hub_root();
  assert!(root.starts_with(std::env::temp_dir()), "a test app's hub is a scratch one: {}", root.display());
  let ctrl_s = Event::Key {
    key: egui::Key::S,
    physical_key: None,
    pressed: true,
    repeat: false,
    modifiers: egui::Modifiers::COMMAND,
  };
  h.frame(vec![ctrl_s]);
  h.frame(vec![]);
  assert!(h.app.data.save.is_some(), "Ctrl+S opens the Save window");
  let w = h.ctx.memory(|m| m.area_rect(crate::file_dialogs::modal_id("save"))).expect("the Save window is shown");
  // Save is the blue button at the window's lower right, as in Fusion
  use crate::file_dialogs::{BUTTON, MARGIN};
  h.move_then_click(w.right_bottom() - vec2(MARGIN + BUTTON.x / 2.0, MARGIN + BUTTON.y / 2.0));
  assert!(h.app.data.save.is_none() && h.app.s.hub.is_some(), "saved: {:?}", h.app.status);
  let index = std::fs::read_to_string(root.join("hub.json")).expect("the scratch hub has the design");
  assert!(index.contains("Untitled"), "{index}");
}

impl Harness {
  /// A drag with `button` and `modifiers` held, across the middle of the canvas.
  fn drag_with(&mut self, button: egui::PointerButton, modifiers: egui::Modifiers) {
    let a = self.app.canvas.center() + vec2(-80.0, 40.0);
    let b = a + vec2(90.0, -40.0);
    let press = |pos, pressed| Event::PointerButton { pos, button, pressed, modifiers };
    self.frame(vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(a)]);
    self.frame(vec![press(a, true)]);
    for i in 1..=6 {
      self.frame(vec![Event::PointerMoved(a + (b - a) * i as f32 / 6.0)]);
    }
    self.frame(vec![press(b, false)]);
    self.frame(vec![Event::ModifiersChanged(egui::Modifiers::NONE)]);
  }
}

/// What a drag did to the camera: (turned, panned, zoomed).
fn camera_change(before: fox_engine::render::Camera, after: fox_engine::render::Camera) -> (bool, bool, bool) {
  let turned = (before.yaw - after.yaw).abs() > 1e-6 || (before.pitch - after.pitch).abs() > 1e-6;
  let zoomed = (before.distance - after.distance).abs() > 1e-6;
  (turned, before.target != after.target && !turned && !zoomed, zoomed)
}

#[test]
fn the_navigation_preset_drives_the_mouse_buttons() {
  use crate::preferences::NavPreset;
  use egui::{Modifiers, PointerButton::*};
  let mut h = Harness::new();
  let orbit = (true, false, false);
  let pan = (false, true, false);
  let zoom = (false, false, true);
  let cases = [
    (NavPreset::Fusion, Middle, Modifiers::NONE, pan),
    (NavPreset::Fusion, Middle, Modifiers::SHIFT, orbit),
    (NavPreset::Fusion, Secondary, Modifiers::NONE, orbit),
    (NavPreset::SolidWorks, Middle, Modifiers::NONE, orbit),
    (NavPreset::SolidWorks, Middle, Modifiers::CTRL, pan),
    (NavPreset::SolidWorks, Middle, Modifiers::SHIFT, zoom),
    (NavPreset::Inventor, Middle, Modifiers::NONE, pan),
    (NavPreset::Inventor, Middle, Modifiers::SHIFT, orbit),
    (NavPreset::Alias, Primary, Modifiers::SHIFT | Modifiers::ALT, orbit),
    (NavPreset::Alias, Middle, Modifiers::SHIFT | Modifiers::ALT, pan),
  ];
  for (preset, button, mods, want) in cases {
    h.app.preferences.values.nav = preset;
    h.app.cam = fox_engine::render::Camera::default();
    let before = h.app.cam;
    h.drag_with(button, mods);
    assert_eq!(camera_change(before, h.app.cam), want, "{preset:?} {button:?} {mods:?}");
    assert!(h.app.drag.is_none() && h.app.s.selection.is_empty(), "no selection box");
  }
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
      let c = s.curves.iter().rev().find(|c| !c.construction).unwrap().id;
      let [a, b] = [0, 1].map(|k| s.pos(s.curve_points(c)[k]).unwrap());
      let ang = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
      // and is locked with an angle dimension (radians) against a horizontal construction
      // reference, as in Fusion
      let locked = d.len() == 2 && d[0] == 40.0 && (d[1].to_degrees() - 20.0).abs() < 1e-9;
      assert!(locked && (ang - 20.0).abs() < 1e-6, "{tool:?}: {d:?} at {ang}°");
      assert!(s.curves.iter().any(|c| c.construction), "the reference line stays in the sketch");
    } else {
      assert_eq!(d, [40.0, 20.0], "{tool:?}");
    }
  }
}

/// Enter on typed heads-up values draws the rectangle and ends the Rectangle command (Select
/// becomes active), while a click-drawn rectangle keeps the tool running, as in Fusion.
#[test]
fn enter_on_typed_values_ends_the_rectangle_tool() {
  use crate::viewport::Tool;
  let mut h = Harness::new();
  h.app.run("sketch.create", serde_json::json!({"plane": "xy"})).unwrap();
  let sk = h.app.s.edit_sketch.unwrap();
  h.app.set_tool(Tool::Rect);
  let c = h.app.canvas.center();
  // a click-drawn rectangle: the tool stays (seen straight on, so the corners don't snap together)
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  h.move_then_click(c);
  h.move_then_click(c + vec2(160.0, -120.0));
  assert_eq!(h.app.s.design.sketch(sk).unwrap().curves.len(), 4);
  assert_eq!(h.app.tool, Tool::Rect, "a click-drawn rectangle keeps the tool");
  // a typed one: Enter commits it and ends the command
  h.move_then_click(c + vec2(-200.0, 100.0));
  h.frame(vec![Event::PointerMoved(c + vec2(-140.0, 60.0))]);
  h.frame(vec![]);
  h.frame(typing("40"));
  h.frame(vec![]);
  h.frame(vec![key(egui::Key::Tab)]);
  h.frame(vec![]);
  h.frame(typing("20"));
  h.frame(vec![]);
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert_eq!(h.app.s.design.sketch(sk).unwrap().curves.len(), 8, "the typed rectangle was drawn");
  assert_eq!(dims(&h.app, sk), [40.0, 20.0]);
  assert_eq!(h.app.tool, Tool::Select, "Enter on typed values ends the Rectangle command");
  assert!(h.app.tstate.is_empty());
}

/// Escape discards an unfinished spline of either kind and Enter commits it, as in Fusion (no
/// kind commits on Escape).
#[test]
fn escape_discards_a_spline_and_enter_commits_it() {
  use crate::viewport::Tool;
  for tool in [Tool::Spline, Tool::ControlSpline] {
    let mut h = Harness::new();
    h.app.run("sketch.create", serde_json::json!({"plane": "xy"})).unwrap();
    let sk = h.app.s.edit_sketch.unwrap();
    h.app.cam.set_view(fox_engine::render::StandardView::Top);
    h.app.set_tool(tool);
    let c = h.app.canvas.center();
    let draw = |h: &mut Harness| {
      h.move_then_click(c);
      h.move_then_click(c + vec2(80.0, -60.0));
      h.move_then_click(c + vec2(160.0, -10.0));
      assert!(!h.app.tstate.is_empty(), "{tool:?}: three points are down");
      assert!(h.app.s.design.sketch(sk).unwrap().curves.is_empty(), "{tool:?}: nothing drawn yet");
    };
    draw(&mut h);
    h.frame(vec![key(egui::Key::Escape)]);
    h.frame(vec![]);
    assert!(h.app.s.design.sketch(sk).unwrap().curves.is_empty(), "{tool:?}: Escape discards the spline");
    assert!(h.app.tstate.is_empty() && h.app.tool == tool, "{tool:?}: the tool stays");
    draw(&mut h);
    h.frame(vec![key(egui::Key::Enter)]);
    h.frame(vec![]);
    assert_eq!(h.app.s.design.sketch(sk).unwrap().curves.len(), 1, "{tool:?}: Enter commits the spline");
    assert!(h.app.tstate.is_empty());
  }
}

/// The browser's Origin folder is hidden by default; its bulb shows the three origin planes in
/// the canvas, each reaching about 60 px from the origin.
#[test]
fn the_origin_folder_shows_camera_sized_planes() {
  use fox_engine::kernel::glam::DVec3;
  let mut h = Harness::new();
  assert!(!h.app.origin_shown, "Origin is hidden by default");
  assert!(crate::origin::plane_quads(&h.app).is_empty());
  h.app.origin_shown = true;
  h.frame(vec![]);
  assert_eq!(crate::origin::plane_quads(&h.app).len(), 15, "three squares, each a fill and four edges");
  let size = crate::viewport::origin_plane_size(&h.app);
  let a = on_screen(&h.app, DVec3::ZERO);
  let b = on_screen(&h.app, DVec3::new(size, 0.0, 0.0));
  // the XY square's reach along X, foreshortened by the home view's turn
  let w = (b - a).length();
  assert!((30.0..=62.0).contains(&w), "the origin plane reaches {w} px");
  // looked at straight on it is the full 60 px
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  let a = on_screen(&h.app, DVec3::ZERO);
  let b = on_screen(&h.app, DVec3::new(size, 0.0, 0.0));
  assert!(((b - a).length() - 60.0).abs() < 1.0, "{}", (b - a).length());
}

/// While Create Sketch waits for a plane, every pointer position away from the bodies hovers an
/// origin plane, taken as infinite (the one the ray meets first), and its sketch grid is
/// previewed with its labels; a click there sketches on it.
#[test]
fn create_sketch_picks_infinite_planes_and_previews_their_grid() {
  use fox_engine::kernel::glam::DVec3;
  use fox_engine::model::OriginPlane;
  let mut h = Harness::new();
  h.settle();
  h.app.new_sketch();
  h.frame(vec![]);
  // far from every square (the canvas's lower left): the origin plane the ray meets first
  let far = h.app.canvas.left_bottom() + vec2(120.0, -80.0);
  let [w, ht] = h.app.canvas_px;
  let ppp = w as f32 / h.app.canvas.width();
  let (o, d) =
    h.app.cam.ray(((far.x - h.app.canvas.min.x) * ppp) as f64, ((far.y - h.app.canvas.min.y) * ppp) as f64, w, ht);
  let frames = h.app.s.design.frames();
  let first = [OriginPlane::Xy, OriginPlane::Xz, OriginPlane::Yz]
    .map(|p| p.plane(frames))
    .into_iter()
    .filter_map(|pl| fox_engine::render::ray_plane(o, d, pl.origin, pl.normal()).map(|hit| ((hit - o).dot(d), pl)))
    .filter(|(t, _)| *t > 0.0)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, pl)| pl);
  assert!(first.is_some());
  h.frame(vec![Event::PointerMoved(far)]);
  h.frame(vec![]);
  assert_eq!(h.app.grid_preview, first, "the first plane met is previewed");
  // a point well up the Z axis: the ray meets an upright plane first
  let side = h.screen(DVec3::new(0.0, 60.0, 80.0));
  h.frame(vec![Event::PointerMoved(side)]);
  h.frame(vec![]);
  let (yz, xz) = (OriginPlane::Yz.plane(frames), OriginPlane::Xz.plane(frames));
  assert!(
    h.app.grid_preview.is_some_and(|p| p == yz || p == xz),
    "an upright plane under the pointer: {:?}",
    h.app.grid_preview
  );
  let previewed = h.app.grid_preview;
  h.move_then_click(side);
  let sk = h.app.s.edit_sketch.expect("the click sketches on the hovered plane");
  assert_eq!(h.app.s.model.sketches.get(&sk).map(|r| r.plane), previewed);
  assert_eq!(h.app.grid_preview, None, "no preview once the sketch is open");
}

/// The File button (after the logo and the data panel button) and the Help (?) button (left of
/// the avatar at the right end) of the application bar.
const FILE: Pos2 = Pos2::new(88.0, 19.0);
const HELP: Pos2 = Pos2::new(1386.0, 19.0);

#[test]
fn app_bar_menus_close_on_escape() {
  for at in [FILE, HELP] {
    let mut h = Harness::new();
    h.move_then_click(at);
    assert!(egui::Popup::is_any_open(&h.ctx), "the menu at {at:?} opens");
    h.frame(vec![key(egui::Key::Escape)]);
    h.frame(vec![]);
    assert!(!egui::Popup::is_any_open(&h.ctx), "Esc closes the menu at {at:?}");
  }
}

#[test]
fn a_command_starting_closes_the_help_menu() {
  let mut h = Harness::new();
  h.move_then_click(HELP);
  assert!(egui::Popup::is_any_open(&h.ctx));
  // a shortcut starts a command over the open menu
  h.frame(vec![key(egui::Key::E), Event::Text("e".into())]);
  h.frame(vec![]);
  assert!(h.app.dialog.is_some(), "E started Extrude");
  assert!(!egui::Popup::is_any_open(&h.ctx), "the Help menu closed");
}

/// Where a world point is drawn in the viewport.
fn on_screen(app: &App, p: fox_engine::kernel::glam::DVec3) -> Pos2 {
  let r = app.canvas;
  let (x, y, _) = app.cam.project(p, r.width().round() as u32, r.height().round() as u32).unwrap();
  r.min + vec2(x as f32, y as f32)
}

#[test]
fn dragging_a_jointed_component_stops_at_its_contact_set() {
  use fox_engine::kernel::glam::DVec3;
  use serde_json::json;
  let mut h = Harness::new();
  let s = &mut h.app.s;
  let part = |s: &mut Session, w: f64, at: [f64; 2]| {
    s.execute("box", &json!({"width": w, "depth": 10, "height": 10, "at": at})).unwrap();
    let b = s.model.bodies.last().unwrap().id.0;
    let c = s.execute("component.create", &json!({"bodies": [b]})).unwrap()["component"].as_u64().unwrap();
    (c, b)
  };
  let (base, _) = part(s, 40.0, [40.0, 0.0]);
  s.execute("component.ground", &json!({"component": base})).unwrap();
  let (block, body) = part(s, 10.0, [-30.0, 0.0]);
  let o = json!({"at": [0, 0, 0], "z": [1, 0, 0]});
  s.execute(
    "joint",
    &json!({"moving": block, "moving_origin": o, "fixed_origin": o, "kind": "slider", "as_built": true}),
  )
  .unwrap();
  s.execute("contact_set.create", &json!({"components": [base, block]})).unwrap();
  let undo_depth = s.undo_history().len();
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  h.app.fit_view_now();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  // press on the block and drag it right, past the base's near side (x = 20)
  let a = on_screen(&h.app, DVec3::new(-30.0, 0.0, 10.0));
  let b = on_screen(&h.app, DVec3::new(30.0, 0.0, 10.0));
  h.drag(a, b);
  let hi = h.app.s.model.body(fox_engine::model::BodyId(body)).unwrap().shape.mass().unwrap().bbox_max.x;
  assert!(hi <= 20.0 + 1e-6 && hi > 19.9, "the block's right side is at {hi}");
  // the whole drag is one undo, and the status says why it stopped
  assert_eq!(h.app.s.undo_history().len(), undo_depth + 1);
  assert!(h.app.status.as_ref().is_some_and(|(m, _)| m.contains("contact")), "{:?}", h.app.status);
}

/// Where the toolbar panel named `label` has its name drawn.
fn panel_label(texts: &[(Rect, String)], label: &str) -> Option<Rect> {
  texts.iter().find(|(r, t)| t == label && r.top() < 140.0).map(|(r, _)| *r)
}

/// Fusion's ribbon at a narrow window: panels give up icons from the right until every panel
/// through SELECT is on screen.
#[test]
fn a_narrow_window_keeps_every_panel_through_select_on_screen() {
  let mut h = Harness::with_width(920.0);
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  for name in ["CREATE", "MODIFY", "CONFIGURE", "CONSTRUCT", "INSPECT", "INSERT", "ASSEMBLE", "SELECT"] {
    let r = panel_label(&all, name).unwrap_or_else(|| panic!("{name} is on the toolbar: {all:?}"));
    assert!(r.right() <= 920.0, "{name} is on screen at 920 px: {r:?}");
  }
  // and CREATE's menu still lists everything that lost its icon
  let label = h.create_label();
  h.move_then_click(label.center());
  assert!(h.find(vec![], "Create Form").is_some(), "Create Form stays in the CREATE menu");
}

/// `ui:toolbar:responsive-collapse`: once the row fits, SOLID shows Fusion's counts, CREATE 5
/// icons and MODIFY 4 (Press Pull, Fillet, Shell, Combine), and every panel through SELECT.
#[test]
fn a_window_that_fits_the_row_shows_fusions_icon_counts() {
  let mut h = Harness::with_width(1100.0);
  h.frame(vec![]);
  let counts: Vec<(&str, usize)> =
    h.ctx.data(|d| d.get_temp(crate::menus::fit_id(crate::panels::Tab::Solid))).expect("the SOLID toolbar drew");
  let of = |name: &str| counts.iter().find(|(l, _)| *l == name).map(|(_, n)| *n);
  assert_eq!(of("CREATE"), Some(5), "{counts:?}");
  assert_eq!(of("MODIFY"), Some(4), "{counts:?}");
  assert!(counts.iter().all(|(_, n)| *n >= 1), "{counts:?}");
}

/// The big FINISH SKETCH button is clickable across the whole panel, not only on its icon,
/// and the contextual SKETCH tab is drawn like the others (bold black, no blue box).
#[test]
fn finish_sketch_takes_a_click_beside_its_icon() {
  let mut h = Harness::new();
  h.app.run("sketch.create", serde_json::json!({"plane": "xy"})).unwrap();
  assert!(h.app.s.edit_sketch.is_some());
  h.app.tab = crate::panels::Tab::Sketch;
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  assert_eq!(all.iter().filter(|(r, t)| t == "SKETCH" && r.top() < 60.0).count(), 2, "the active tab is bold");
  let label = panel_label(&all, "FINISH SKETCH").expect("the FINISH SKETCH panel");
  // just above the panel name and well left of the icon's centre
  let at = Pos2::new(label.left() + 4.0, label.top() - 14.0);
  h.move_then_click(at);
  assert!(h.app.s.edit_sketch.is_none(), "the click beside the icon finishes the sketch");
}

/// Create Base Feature is a mode, as in Fusion: no dialog; the ribbon shows BASE FEATURE SOLID /
/// BASE FEATURE SURFACE with a FINISH BASE FEATURE panel, and the base feature joins the
/// timeline. Finish brings the SOLID tab back.
#[test]
fn create_base_feature_switches_the_ribbon() {
  let mut h = Harness::new();
  crate::panels::Cmd::BaseFeature(true).run(&mut h.app);
  assert!(h.app.dialog.is_none(), "no dialog");
  assert!(h.app.base_mode);
  let has_base = |h: &Harness| {
    h.app.s.design.features.iter().any(|f| matches!(f.kind, fox_engine::model::FeatureKind::BaseFeature { .. }))
  };
  assert!(has_base(&h), "a base feature in the timeline");
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  let tabs: Vec<&str> = all.iter().filter(|(r, _)| r.top() < 60.0).map(|(_, t)| t.as_str()).collect();
  assert!(tabs.contains(&"BASE FEATURE SOLID") && tabs.contains(&"BASE FEATURE SURFACE"), "{tabs:?}");
  assert!(!tabs.contains(&"MESH") && !tabs.contains(&"SOLID"), "{tabs:?}");
  let label = panel_label(&all, "FINISH BASE FEATURE").expect("the FINISH BASE FEATURE panel");
  h.move_then_click(Pos2::new(label.center().x, label.top() - 20.0));
  assert!(!h.app.base_mode, "Finish leaves the mode");
  assert_eq!(h.app.tab, crate::panels::Tab::Solid);
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  assert!(all.iter().any(|(r, t)| t == "SOLID" && r.top() < 60.0), "the SOLID tab is back");
  assert!(panel_label(&all, "FINISH BASE FEATURE").is_none());
  assert!(has_base(&h), "the base feature stays in the timeline");
}

/// The DESIGN switcher lists Fusion's workspaces in upper case, with submenus for the
/// documents DRAWING and ELECTRONICS create, and says "Change Workspace" on hover.
#[test]
fn the_workspace_menu_lists_fusions_workspaces() {
  let mut h = Harness::new();
  let switcher = h.find(vec![], "DESIGN").expect("the switcher");
  let shown = h.rest(switcher.center(), 1.0);
  assert!(shown.iter().any(|t| t == "Change Workspace"), "{shown:?}");
  h.move_then_click(switcher.center());
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  let names: Vec<&str> = all.iter().map(|(_, t)| t.as_str()).collect();
  for w in ["GENERATIVE DESIGN", "RENDER", "ANIMATION", "SIMULATION", "MANUFACTURE", "DRAWING", "ELECTRONICS"] {
    assert!(names.contains(&w), "{w} is in the menu: {names:?}");
  }
  assert!(!names.contains(&"Generative Design"), "no title-case entries: {names:?}");
  let drawing = all.iter().find(|(_, t)| t == "DRAWING").map(|(r, _)| *r).unwrap();
  h.frame(vec![Event::PointerMoved(drawing.center())]);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let from_design = h.find(vec![], "From Design").expect("DRAWING ▸ opens its submenu");
  assert!(h.find(vec![], "From Animation").is_some());
  h.move_then_click(from_design.center());
  assert!(h.app.drawing.is_some() || h.app.dialog.is_some(), "From Design opens the Drawing workspace");
}

/// Fusion's workspace menu rows: semi-bold upper case on taller rows, the list as wide as its
/// longest name (not 200 px), and no fill on the current workspace.
#[test]
fn the_workspace_menu_rows_are_fusions() {
  let mut h = Harness::new();
  let switcher = h.find(vec![], "DESIGN").expect("the switcher");
  h.move_then_click(switcher.center());
  // the pointer rests on the button, away from every row
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  let row = |t: &str| all.iter().find(|(r, s)| s == t && r.top() > switcher.bottom()).map(|(r, _)| *r);
  let design = row("DESIGN").expect("the DESIGN row");
  let generative = row("GENERATIVE DESIGN").expect("the GENERATIVE DESIGN row");
  let render = row("RENDER").expect("the RENDER row");
  let step = (render.center().y - generative.center().y).abs();
  assert!((step - crate::panels::WORKSPACE_ROW_H).abs() < 0.5, "rows {step} apart");
  // semi-bold: each name drawn twice, a fraction of a pixel apart
  assert_eq!(all.iter().filter(|(r, s)| s == "RENDER" && r.top() > switcher.bottom()).count(), 2);
  // the rows' fills: none is drawn behind the current DESIGN row
  let shapes = h.frame_shapes(1.0 / 60.0);
  let behind = shapes.iter().any(|s| match s {
    egui::Shape::Rect(r) => {
      r.fill == crate::theme::SELECTED && r.rect.contains(design.center()) && r.rect.height() < 60.0
    }
    _ => false,
  });
  assert!(!behind, "no fill on the current workspace");
  // the list hugs GENERATIVE DESIGN and its padlock
  let menu = shapes
    .iter()
    .filter_map(|s| match s {
      egui::Shape::Rect(r) if r.rect.contains(design.center()) && r.rect.contains(generative.center()) => Some(r.rect),
      _ => None,
    })
    .filter(|r| r.height() < 400.0)
    .min_by(|a, b| a.area().total_cmp(&b.area()))
    .expect("the menu's frame");
  assert!(menu.width() < 200.0, "the menu is {} wide", menu.width());
  assert!(menu.right() > generative.right() + 20.0, "room for the padlock: {menu:?} vs {generative:?}");
}

/// Resting on a workspace row for 1.5 s opens Fusion's dark tooltip right of the menu: the bold
/// "<Name> Workspace" title and what the workspace does.
#[test]
fn a_workspace_row_has_fusions_tooltip() {
  let mut h = Harness::new();
  let switcher = h.find(vec![], "DESIGN").expect("the switcher");
  h.move_then_click(switcher.center());
  let render = h.find(vec![], "RENDER").expect("the RENDER row");
  // egui shows no tooltip for a pointer that moved within 0.1 s of a click
  h.frame_texts(0.3, vec![]);
  let early = h.rest(render.center(), 1.0);
  assert!(!early.iter().any(|t| t == "Render Workspace"), "not yet after 1 s: {early:?}");
  let shown = h.rest_rects(render.center(), 1.8);
  let title = shown.iter().find(|(_, t)| t == "Render Workspace").map(|(r, _)| *r);
  let title = title.unwrap_or_else(|| panic!("the title after 1.5 s: {shown:?}"));
  assert!(shown.iter().any(|(_, t)| t == "Generates realistic renderings of the design."), "{shown:?}");
  assert!(title.left() > render.right(), "right of the menu: {title:?} vs {render:?}");
  assert!((title.top() - render.top()).abs() < 30.0, "level with the row: {title:?} vs {render:?}");
  // Generative Design's words are Fusion's too
  assert_eq!(
    crate::tips::workspace("Generative Design"),
    Some("Creates multiple designs that meet your geometric, performance, and manufacturing requirements.")
  );
}

/// The workspace button names every workspace at DESIGN's size, the ▾ a few px after the name,
/// growing for MANUFACTURE rather than shrinking the name.
#[test]
fn the_workspace_button_keeps_designs_label_size() {
  let mut h = Harness::new();
  let design = h.find(vec![], "DESIGN").expect("the switcher");
  crate::panels::switch_workspace(&mut h.app, "Manufacture");
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let manufacture = h.find(vec![], "MANUFACTURE").expect("the switcher names Manufacture");
  assert!((manufacture.height() - design.height()).abs() < 0.5, "{manufacture:?} vs {design:?}");
  let (width, left, caret) = crate::panels::switcher_layout(manufacture.width());
  assert!(width > 92.0, "the button grows: {width}");
  assert!(left >= 10.0, "the name isn't pressed against the edge: {left}");
  let gap = caret - 3.5 - (left + manufacture.width());
  assert!((gap - crate::panels::SWITCHER_CARET_GAP).abs() < 0.01 && gap >= 4.0, "a spaced caret: {gap}");
  // DESIGN keeps Fusion's 92 px button
  assert_eq!(crate::panels::switcher_layout(design.width()).0, 92.0);
}

/// The browser's root row and Origin folder have eye toggles, Document Settings starts expanded
/// (its Units and Part Design rows push Origin down), and hovering a row names it in the status
/// readout.
#[test]
fn browser_root_and_origin_eyes() {
  let mut h = Harness::new();
  h.app.run("box", serde_json::json!({"width": 10, "depth": 10, "height": 10}));
  let body = h.app.s.model.bodies[0].id;
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("browser"))).expect("the browser is shown");
  // the header is 26 tall, then 4 of space and 25 per row; the eye sits 24 right of the row's
  // indent (8 + 16 per level)
  let row = |i: f32| area.min.y + 30.0 + 25.0 * i + 12.5;
  let root_eye = egui::pos2(area.min.x + 8.0 + 24.0, row(0.0));
  h.frame(vec![Event::PointerMoved(root_eye)]);
  h.frame(vec![]);
  assert_eq!(h.app.ctx_menu.peek_label.as_deref(), Some("(Unsaved)"), "hovering the root names it");
  assert!(h.app.s.body_visible(body));
  h.move_then_click(root_eye);
  assert!(!h.app.s.body_visible(body), "the root eye hides the component's bodies");
  h.move_then_click(root_eye);
  assert!(h.app.s.body_visible(body));
  // root, Document Settings, Units, Part Design, Named Views, then Origin
  let origin_eye = egui::pos2(area.min.x + 8.0 + 16.0 + 24.0, row(5.0));
  assert!(!h.app.origin_shown);
  h.move_then_click(origin_eye);
  assert!(h.app.origin_shown, "the Origin folder's eye shows the origin planes");
  h.move_then_click(origin_eye);
  assert!(!h.app.origin_shown);
}

// ---------------------------------------------------------------- preferences

fn prefs_rect(h: &Harness) -> Rect {
  h.ctx.memory(|m| m.area_rect(egui::Id::new(crate::preferences::MODAL_ID))).expect("the Preferences modal is open")
}

#[test]
fn the_preferences_modal_is_centred_and_sized_like_fusions() {
  let mut h = Harness::new();
  crate::preferences::open(&mut h.app);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let r = prefs_rect(&h);
  let screen = Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0));
  assert!((r.center() - screen.center()).length() < 3.0, "centred: {r:?}");
  let size = crate::preferences::SIZE;
  assert!(r.width() >= size.x && r.width() <= size.x + 40.0, "about 750 wide: {r:?}");
  assert!(r.height() >= size.y && r.height() <= size.y + 40.0, "about 450 high: {r:?}");
}

#[test]
fn escape_cancels_the_preferences_and_drops_the_edits() {
  let mut h = Harness::new();
  crate::preferences::open(&mut h.app);
  h.frame(vec![]);
  h.app.preferences.draft.show_tooltips = false;
  h.app.preferences.draft.space_repeats = false;
  h.frame(vec![Event::Key {
    key: egui::Key::Escape,
    physical_key: None,
    pressed: true,
    repeat: false,
    modifiers: Default::default(),
  }]);
  h.frame(vec![]);
  assert!(!h.app.preferences.open, "Esc closes the modal");
  assert!(h.app.preferences.values.show_tooltips && h.app.preferences.values.space_repeats, "nothing applied");
  assert_eq!(h.app.preferences.draft, h.app.preferences.values, "the edits are gone");
  assert!(h.app.dialog.is_none());
}

fn escape_key() -> Event {
  Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
}

#[test]
fn escape_on_an_open_preferences_drop_down_closes_only_the_drop_down() {
  let mut h = Harness::new();
  crate::preferences::open(&mut h.app);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let id = crate::preferences::combo_rect_id("prefs-nav-preset");
  let combo = h.ctx.data(|d| d.get_temp::<Rect>(id)).expect("the Mouse setup drop-down is drawn");
  h.move_then_click(combo.center());
  assert!(egui::Popup::is_any_open(&h.ctx), "the Mouse setup list opens");
  h.app.preferences.draft.space_repeats = false;
  h.frame(vec![escape_key()]);
  h.frame(vec![]);
  assert!(!egui::Popup::is_any_open(&h.ctx), "Esc closes the list");
  assert!(h.app.preferences.open, "the Preferences window stays");
  assert!(!h.app.preferences.draft.space_repeats, "with its edits");
  h.frame(vec![escape_key()]);
  h.frame(vec![]);
  assert!(!h.app.preferences.open, "a second Esc cancels the window");
}

#[test]
fn cancel_and_ok_sit_at_the_bottom_right_of_the_preferences() {
  let mut h = Harness::new();
  crate::preferences::open(&mut h.app);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let r = prefs_rect(&h);
  // the button row is the last 30 px; Cancel is the rightmost button, OK the next one left
  let row_y = r.bottom() - 8.0 - 12.0;
  h.app.preferences.draft.show_prompt = false;
  h.move_then_click(Pos2::new(r.right() - 8.0 - 24.0, row_y));
  assert!(!h.app.preferences.open, "Cancel closes");
  assert!(h.app.preferences.values.show_prompt, "Cancel applies nothing");
  crate::preferences::open(&mut h.app);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  h.app.preferences.draft.show_prompt = false;
  h.move_then_click(Pos2::new(r.right() - 8.0 - 48.0 - 6.0 - 16.0, row_y));
  assert!(!h.app.preferences.open, "OK closes");
  assert!(!h.app.preferences.values.show_prompt, "OK applies the edit");
}

/// Extrude with a profile shows the floating value box at its arrow, focused: typing goes into
/// it and the dialog's Distance mirrors it; Enter is OK.
#[test]
fn the_floating_value_box_takes_typing_and_enter_is_ok() {
  use serde_json::json;
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  h.app.run("sketch.rect", json!({"corner1": [-10, -10], "corner2": [10, 10]})).unwrap();
  let sk = h.app.s.edit_sketch.unwrap();
  h.app.run("sketch.finish", json!({})).unwrap();
  h.app.s.selection = vec![fox_engine::Sel::Profile { sketch: sk, index: 0 }];
  h.app.start(crate::dialogs::Kind::Extrude);
  h.app.fit_view_now();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let focused = h.ctx.memory(|m| m.focused()).expect("the value box has the keyboard");
  assert_eq!(h.ctx.read_response(focused).map(|r| r.layer_id), Some(crate::value_box::layer()));
  // its value is selected: typing replaces it, and the dialog's field says the same
  h.frame(typing("25"));
  h.frame(vec![]);
  // the row gets the unit written out, as Fusion's does (`ui:manip:typed-value-units`)
  let distance = h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone();
  assert_eq!(distance, "25 mm");
  let features = h.app.s.design.features.len();
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert!(h.app.dialog.is_none(), "Enter in the box is OK");
  assert_eq!(h.app.s.design.features.len(), features, "the previewed extrude stayed");
  let top = h.app.s.model.bodies[0].shape.mass().unwrap().bbox_max.z;
  assert!((top - 25.0).abs() < 1e-6, "extruded by the typed 25 mm: {top}");
}

/// `ui:manip:value-box-covers-arrow`, `ui:manip:arrow-length-offscreen`,
/// `ui:manip:zero-default-value`: Fillet on a picked edge starts at 0 mm with the box focused and
/// nothing previewed; with a typed radius the box sits clear of the arrow's head, and a radius
/// far larger than the part keeps the arrow short and the box on the canvas.
#[test]
fn the_fillet_box_keeps_off_its_arrow_and_on_the_canvas() {
  use serde_json::json;
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  let body = h.app.s.model.bodies[0].id;
  h.app.s.selection = vec![fox_engine::Sel::Edge { body, edge: 1 }];
  h.app.start(crate::dialogs::Kind::Fillet);
  h.app.fit_view_now();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let focused = h.ctx.memory(|m| m.focused()).expect("the value box has the keyboard");
  assert_eq!(h.ctx.read_response(focused).map(|r| r.layer_id), Some(crate::value_box::layer()));
  assert_eq!(h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone(), "0 mm");
  assert!(!h.app.dialog.as_ref().unwrap().preview_applied(), "nothing previewed at 0 mm");
  h.frame(typing("2"));
  for _ in 0..2 {
    h.frame(vec![]);
  }
  let arrow = |h: &Harness| {
    let m = h.app.dialog.as_ref().unwrap().manipulator(&h.app.s).unwrap();
    let a = crate::viewport::screen_of(&h.app, m.origin).unwrap();
    (a, crate::viewport::screen_of(&h.app, m.origin + m.dir * m.value).unwrap())
  };
  let (a, b) = arrow(&h);
  let value_box = h.ctx.memory(|m| m.area_rect(crate::value_box::id())).expect("the value box");
  for t in [1.0, 0.75, 0.5] {
    let q = a + (b - a) * t;
    assert!(!value_box.expand(3.0).contains(q), "the box {value_box:?} covers the arrow at {q:?}");
  }
  // a 1000 mm radius on a 10 mm box: the arrow stops short and the box stays on the canvas
  *h.app.dialog.as_mut().unwrap().value_box_field().unwrap() = "1000 mm".into();
  h.app.dialog.as_mut().unwrap().value_box_edited();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let (a, b) = arrow(&h);
  assert!((b - a).length() > 200.0, "the full arrow would run {} points", (b - a).length());
  let value_box = h.ctx.memory(|m| m.area_rect(crate::value_box::id())).expect("the value box");
  assert!(h.app.canvas.contains_rect(value_box), "{value_box:?} in {:?}", h.app.canvas);
  assert!((value_box.center() - a).length() < 200.0, "the box stays near the edge: {value_box:?} {a:?}");
}

/// `ui:manip:draft-angle-handle`: a press on Draft's arc (not only its disc) turns the angle in
/// 5° steps and opens the floating box with it, instead of starting a window selection; the
/// box isn't there before.
#[test]
fn dragging_drafts_arc_turns_the_angle_in_five_degree_steps() {
  use serde_json::json;
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  let body = h.app.s.model.bodies[0].id;
  let front = h.app.s.model.bodies[0].faces().iter().find(|f| f.normal.y < -0.9).unwrap().id;
  h.app.s.selection = vec![fox_engine::Sel::Face { body, face: front }];
  h.app.start(crate::dialogs::Kind::Draft);
  // the face is the face to draft; the XY plane, picked in the viewport, the pull direction
  if let Some(d) = h.app.dialog.as_mut() {
    d.pick_pull_plane(&mut h.app.s, json!("xy"));
  }
  h.app.fit_view_now();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  assert!(h.ctx.memory(|m| m.area_rect(crate::value_box::id())).is_none(), "no box before the drag");
  let project = |p| crate::viewport::screen_of(&h.app, p);
  let placed = crate::handles::placed_points(&h.app, &project);
  let (_, arc) = placed[0].clone().expect("the disc on its arc");
  // the arc's 33 points run a quarter turn either side of the pull direction: from 45° before
  // it to about 17° before it is a 28° turn from 0.0°, snapped to 30°
  let (from, to) = (arc[8], arc[13]);
  h.drag(from, to);
  assert!(!matches!(h.app.drag, Some(crate::viewport::Drag::Box { .. })), "no window selection");
  let value_box = h.ctx.memory(|m| m.area_rect(crate::value_box::id()));
  assert!(value_box.is_some(), "the drag opened the box");
  assert_eq!(h.app.dialog.as_mut().unwrap().handle_field(0).unwrap().clone(), "30.0 deg");
}

/// Select Other, as in Fusion: a press held still on the model opens the list (Depth tab, one
/// row per entity by type) while the button is down, selecting nothing; the release selects
/// nothing either and the list stays until Escape. Alt+click simply selects.
#[test]
fn holding_the_button_on_the_model_opens_select_other() {
  use fox_engine::scene::Pick;
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20}));
  let body = h.app.s.model.bodies[0].id;
  let face = h.app.s.model.bodies[0].faces()[0].id;
  let at = h.app.canvas.center();
  // headless there is no frame to pick from: the face under the pointer is given
  h.app.hover = Some(Pick::Face { body, face });
  h.frame(vec![Event::PointerMoved(at)]);
  h.app.hover = Some(Pick::Face { body, face });
  h.frame(vec![Harness::button(at, true)]);
  for _ in 0..3 {
    h.app.hover = Some(Pick::Face { body, face });
    h.frame_texts(0.1, vec![]);
  }
  assert!(h.app.other.is_none(), "not yet: {} s", 3.0 * 0.1);
  assert!(h.app.s.selection.is_empty(), "nothing is selected on the press");
  for _ in 0..3 {
    h.app.hover = Some(Pick::Face { body, face });
    h.frame_texts(0.1, vec![]);
  }
  let menu = h.app.other.as_ref().expect("the list opens while the button is held");
  assert_eq!(menu.tab, crate::select::OtherTab::Depth);
  let types: Vec<&str> = menu.depth.iter().map(|r| r.2.as_str()).collect();
  assert!(!types.is_empty() && types.iter().all(|t| *t == "Face" || *t == "Edge"), "rows by type: {types:?}");
  assert!(h.app.s.selection.is_empty());
  h.frame_texts(0.1, vec![Harness::button(at, false)]);
  assert!(h.app.other.is_some() && h.app.s.selection.is_empty(), "the release picks nothing, the list stays");
  // (a new egui area sizes itself invisibly on its first frame)
  let texts = h.frame_texts(0.05, vec![]);
  assert!(texts.iter().any(|(_, t)| t == "Depth") && texts.iter().any(|(_, t)| t == "Parents"), "the two tabs");
  h.frame(vec![key(egui::Key::Escape)]);
  assert!(h.app.other.is_none(), "Escape closes it");
  // Alt+click (a while later, so it isn't a double click) selects, with no list
  h.frame_texts(1.0, vec![]);
  h.app.hover = Some(Pick::Face { body, face });
  let alt = egui::Modifiers { alt: true, ..Default::default() };
  let press = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: alt };
  h.frame(vec![press(true)]);
  h.app.hover = Some(Pick::Face { body, face });
  h.frame(vec![press(false)]);
  assert_eq!(h.app.s.selection.len(), 1, "Alt+click simply selects what is under the pointer");
  assert!(h.app.other.is_none());
}

/// The window box is Fusion's peach with a purple-grey border, the crossing box pale yellow
/// with a dashed dark border.
#[test]
fn box_selection_colours_match_fusion() {
  use crate::viewport::{CROSSING_BOX, WINDOW_BOX};
  assert_eq!(WINDOW_BOX, (egui::Color32::from_rgb(253, 223, 185), egui::Color32::from_rgb(170, 152, 195)));
  assert_eq!(CROSSING_BOX, (egui::Color32::from_rgb(255, 253, 202), egui::Color32::from_rgb(55, 61, 47)));
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20}));
  let a = h.app.canvas.center() + vec2(-60.0, -40.0);
  let b = a + vec2(120.0, 80.0);
  h.frame(vec![Event::PointerMoved(a)]);
  h.frame(vec![Harness::button(a, true)]);
  h.frame(vec![Event::PointerMoved(a + vec2(30.0, 20.0))]);
  h.frame(vec![Event::PointerMoved(b)]);
  assert!(matches!(h.app.drag, Some(crate::viewport::Drag::Box { .. })), "{:?}", h.app.drag.is_some());
  let out = h.shapes(vec![Event::PointerMoved(b)]);
  assert!(out.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == WINDOW_BOX.0)), "the window box is drawn");
  h.frame(vec![Harness::button(b, false)]);
  // right to left: the crossing box, with a dashed border (many short segments)
  h.frame(vec![Event::PointerMoved(b)]);
  h.frame(vec![Harness::button(b, true)]);
  h.frame(vec![Event::PointerMoved(b - vec2(30.0, 20.0))]);
  let out = h.shapes(vec![Event::PointerMoved(a)]);
  assert!(out.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == CROSSING_BOX.0)), "the crossing fill");
  let dashes = out
    .iter()
    .filter(|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == CROSSING_BOX.1))
    .count();
  assert!(dashes > 8, "a dashed border: {dashes} dashes");
  h.frame(vec![Harness::button(a, false)]);
}

impl Harness {
  /// The shapes a frame with `events` draws, flattened.
  fn shapes(&mut self, events: Vec<Event>) -> Vec<egui::Shape> {
    fn walk(s: egui::Shape, out: &mut Vec<egui::Shape>) {
      match s {
        egui::Shape::Vec(v) => v.into_iter().for_each(|s| walk(s, out)),
        s => out.push(s),
      }
    }
    self.time += 1.0 / 60.0;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    let out = self.ctx.run_ui(input, |ui| app.draw(ui));
    let mut all = Vec::new();
    for c in out.shapes.iter() {
      walk(c.shape.clone(), &mut all);
    }
    out.drop_without_applying_deltas();
    all
  }
}

/// `ui:sketch:offset-default-follow`, `ui:sketch:offset-dialog-rows`,
/// `ui:sketch:status-readout-under-palette`: O on a pre-selected line opens OFFSET beside the
/// Sketch Palette on its left, the offset at 0 mm on the line (the pointer doesn't move it), the
/// value box by the pointer with the keyboard, and the readout left of the palette; typing ".1"
/// and Enter makes the offset line and ends the tool.
#[test]
fn offset_on_a_preselected_line_starts_at_zero_and_takes_a_typed_distance() {
  use serde_json::json;
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  h.app.run("sketch.line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
  let sk = h.app.s.edit_sketch.unwrap();
  let line = h.app.s.design.sketch(sk).unwrap().curves[0].id;
  h.app.s.selection = vec![fox_engine::Sel::SketchCurve { sketch: sk, curve: line }];
  h.app.start(crate::dialogs::Kind::SkOffset);
  h.app.fit_view_now();
  let c = h.app.canvas.center() + vec2(0.0, 120.0);
  h.frame(vec![Event::PointerMoved(c)]);
  let texts = h.frame_texts(1.0 / 60.0, vec![]);
  h.frame(vec![]);
  // the dialog docks beside the palette, on its left
  let dialog = h.dialog_rect();
  let palette = h.ctx.memory(|m| m.area_rect(egui::Id::new("sketch-palette"))).expect("the palette");
  assert!(dialog.right() <= palette.left(), "side by side: {dialog:?} {palette:?}");
  assert!((dialog.top() - palette.top()).abs() < 1.0, "{dialog:?} {palette:?}");
  // the readout sits left of the palette
  let readout = texts.iter().find(|(_, t)| t.starts_with("1 Sketch")).expect("the readout");
  assert!(readout.0.right() <= palette.left(), "{readout:?} {palette:?}");
  // the value box has the keyboard, says 0 mm and sits by the pointer
  let focused = h.ctx.memory(|m| m.focused()).expect("the value box has the keyboard");
  assert_eq!(h.ctx.read_response(focused).map(|r| r.layer_id), Some(crate::value_box::layer()));
  assert_eq!(h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone(), "0 mm");
  let value_box = h.ctx.memory(|m| m.area_rect(crate::value_box::id())).expect("the value box");
  assert!((value_box.left() - (c.x + 18.0)).abs() < 2.0 && (value_box.center().y - c.y).abs() < 12.0, "{value_box:?}");
  // the pointer moving doesn't move the offset
  h.frame(vec![Event::PointerMoved(c + vec2(80.0, -60.0))]);
  h.frame(vec![]);
  assert_eq!(h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone(), "0 mm");
  assert_eq!(h.app.s.design.sketch(sk).unwrap().curves.len(), 1, "nothing previewed at 0 mm");
  // typing replaces the value; Enter applies it and the tool ends
  h.frame(typing(".1"));
  h.frame(vec![]);
  assert_eq!(h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone(), ".1 mm");
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert!(h.app.dialog.is_none(), "Enter is OK");
  let sketch = h.app.s.design.sketch(sk).unwrap();
  assert_eq!(sketch.curves.len(), 2, "the line and its offset");
  let ys: Vec<f64> =
    sketch.curves.iter().flat_map(|c| sketch.curve_points(c.id)).filter_map(|p| sketch.pos(p)).map(|p| p[1]).collect();
  assert!(ys.iter().any(|y| (y - 0.1).abs() < 1e-6), "offset by 0.1 mm: {ys:?}");
}

/// A right-to-left box crossing only one curve of a rectangle selects just that curve, after
/// Escape cleared an earlier click's selection, so Delete removes the boxed curve alone, as in
/// Fusion.
#[test]
fn a_crossing_box_over_one_curve_selects_exactly_that_curve() {
  use crate::viewport::Tool;
  use fox_engine::Sel;
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  let sk = h.app.s.edit_sketch.unwrap();
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  h.app.set_tool(Tool::Rect);
  let c = h.app.canvas.center();
  h.move_then_click(c);
  h.move_then_click(c + vec2(160.0, -120.0));
  let s = h.app.s.design.sketch(sk).unwrap().clone();
  assert_eq!(s.curves.len(), 4);
  h.app.set_tool(Tool::Select);
  // where a curve's middle lands on screen
  let plane = h.app.s.model.sketches[&sk].plane;
  let mid_of = |h: &Harness, curve: fox_engine::sketch::CurveId| {
    let p = s.curve_points(curve);
    let (a, b) = (s.pos(p[0]).unwrap(), s.pos(p[1]).unwrap());
    let w = plane.to_world([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
    let [pw, ph] = h.app.canvas_px;
    let (x, y, _) = h.app.cam.project(w, pw, ph).unwrap();
    h.app.canvas.min + vec2(x as f32, y as f32) * (h.app.canvas.width() / pw as f32)
  };
  // the line whose ends both sit at the rectangle's largest x (right) or y (top)
  let line_with = |axis: usize| {
    let coord = |p: &fox_engine::sketch::PointId| s.pos(*p).unwrap()[axis];
    let at =
      s.points.iter().filter(|p| p.id != fox_engine::sketch::ORIGIN).map(|p| [p.x, p.y][axis]).fold(f64::MIN, f64::max);
    s.curves.iter().find(|c| s.curve_points(c.id).iter().all(|p| (coord(p) - at).abs() < 1e-6)).expect("a side").id
  };
  let (top, right) = (line_with(1), line_with(0));
  // click the top line, then Escape clears it
  h.move_then_click(mid_of(&h, top));
  assert_eq!(h.app.s.selection, vec![Sel::SketchCurve { sketch: sk, curve: top }], "the click picks the top line");
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![]);
  assert!(h.app.s.selection.is_empty(), "Escape clears the selection");
  // a right-to-left box across the middle of the right line, clear of its corners
  let m = mid_of(&h, right);
  h.drag(m + vec2(20.0, 10.0), m + vec2(-20.0, -10.0));
  assert_eq!(
    h.app.s.selection,
    vec![Sel::SketchCurve { sketch: sk, curve: right }],
    "the box picks the right line only"
  );
  h.frame(vec![key(egui::Key::Delete)]);
  h.frame(vec![]);
  let after = h.app.s.design.sketch(sk).unwrap();
  assert_eq!(after.curves.len(), 3, "Delete removed one line");
  assert!(after.curve(right).is_none() && after.curve(top).is_some(), "the right line went, the top one stayed");
}

impl Harness {
  /// Where a world point lands on the window, from the viewport's camera and canvas.
  fn screen(&self, p: fox_engine::kernel::glam::DVec3) -> Pos2 {
    let [w, h] = self.app.canvas_px;
    let ppp = w as f32 / self.app.canvas.width();
    let (x, y, _) = self.app.cam.project(p, w, h).expect("in view");
    self.app.canvas.min + vec2(x as f32 / ppp, y as f32 / ppp)
  }

  /// Lets the first fit and any eased camera move finish, with the frame rendered.
  fn settle(&mut self) {
    self.app.fit_view_now();
    for _ in 0..40 {
      self.frame(vec![]);
      if self.app.cam_move.is_none() && self.app.frame.is_some() {
        break;
      }
    }
    self.frame(vec![]);
  }
}

/// With the default filters (Vertices on, as in Fusion), hovering a box corner finds the vertex
/// rather than an edge, and a click selects it with Fusion's "Vertex | X: … Y: … Z: … mm" readout.
#[test]
fn hovering_a_corner_picks_the_vertex_by_default() {
  use fox_engine::kernel::glam::DVec3;
  use fox_engine::scene::Pick;
  let mut h = Harness::new();
  assert!(h.app.sel_filter.vertices, "the Vertices filter starts on");
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  h.settle();
  let corner = DVec3::new(10.0, 10.0, 20.0);
  let at = h.screen(corner);
  h.frame(vec![Event::PointerMoved(at)]);
  h.frame(vec![]);
  let Some(Pick::Vertex { body, edge, end }) = h.app.hover else { panic!("hover {:?}", h.app.hover) };
  let pos = fox_engine::scene::vertex_pos(&h.app.s, body, edge, end).unwrap();
  assert!((pos - corner).length() < 1e-6, "{pos}");
  assert_eq!(crate::panels::selection_text(&h.app), None, "a hovered vertex reads nothing");
  h.frame(vec![Harness::button(at, true)]);
  h.frame(vec![Harness::button(at, false)]);
  h.frame(vec![]);
  assert_eq!(h.app.s.selection, vec![fox_engine::Sel::Vertex { body, edge, end }]);
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("Vertex | X: 10.00 Y: 10.00 Z: 20.00 mm"));
}

/// With the Origin folder shown, the origin point, axes and planes are hovered and picked in the
/// canvas: a click selects one (status "Origin Point | X: 0.00 …"), a second click or Escape
/// clears it, and it stays visible; while the folder is hidden nothing is picked.
#[test]
fn origin_items_are_hovered_and_picked_in_the_canvas() {
  use crate::origin::{self, OriginItem};
  use fox_engine::kernel::glam::DVec3;
  let mut h = Harness::new();
  h.settle();
  let at = h.screen(DVec3::ZERO);
  h.frame(vec![Event::PointerMoved(at)]);
  h.frame(vec![]);
  assert_eq!(h.app.hover_origin, None, "the folder is hidden");
  h.app.origin_shown = true;
  h.frame(vec![]);
  h.frame(vec![]);
  assert_eq!(h.app.hover_origin, Some(OriginItem::Point));
  assert_eq!(crate::panels::selection_text(&h.app), None, "hovered, it reads nothing");
  h.frame(vec![Harness::button(at, true)]);
  h.frame(vec![Harness::button(at, false)]);
  h.frame(vec![]);
  assert_eq!(origin::picked(&h.app), Some(OriginItem::Point));
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("Origin Point | X: 0.00 Y: 0.00 Z: 0.00 mm"));
  // clicking it again clears it
  h.frame(vec![Harness::button(at, true)]);
  h.frame(vec![Harness::button(at, false)]);
  h.frame(vec![]);
  assert_eq!(origin::picked(&h.app), None);
  // an axis: along X, well clear of the origin point
  let reach = crate::viewport::origin_plane_size(&h.app) * crate::viewport::ORIGIN_AXIS_REACH;
  let on_x = h.screen(DVec3::X * reach * 0.6);
  h.frame(vec![Event::PointerMoved(on_x)]);
  h.frame(vec![]);
  assert_eq!(h.app.hover_origin, Some(OriginItem::X));
  h.frame(vec![Harness::button(on_x, true)]);
  h.frame(vec![Harness::button(on_x, false)]);
  h.frame(vec![]);
  assert_eq!(origin::picked(&h.app), Some(OriginItem::X));
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("X Axis"));
  assert!(origin::visible(&h.app, OriginItem::X), "the picked axis stays visible");
  // Escape clears the pick; the axis still shows
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![]);
  assert_eq!(origin::picked(&h.app), None);
  assert!(origin::visible(&h.app, OriginItem::X));
  // a plane picked in the browser way, then a click on empty canvas clears it
  origin::pick(&mut h.app, OriginItem::Xz);
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("XZ Plane"));
  let far = h.app.canvas.right_top() + vec2(-40.0, 300.0);
  h.frame(vec![Event::PointerMoved(far)]);
  h.frame(vec![]);
  assert_eq!(h.app.hover_origin, None);
  h.frame(vec![Harness::button(far, true)]);
  h.frame(vec![Harness::button(far, false)]);
  h.frame(vec![]);
  assert_eq!(origin::picked(&h.app), None);
}

// ---------------------------------------------------------------- constraint glyphs

/// Each constraint's glyphs sit where Fusion puts them: Horizontal below the line's middle,
/// Vertical right of it, Parallel one under each line, Equal one on each line, Perpendicular
/// inside the shared corner, MidPoint above the point, Symmetry above each point and on the
/// axis, and Coincident nothing until its point is hovered.
#[test]
fn constraint_glyphs_sit_where_fusion_puts_them() {
  use crate::glyphs::{GAP, places};
  use fox_engine::sketch::{ConstraintKind, CurveKind, PointId, Sketch};
  let mut s = Sketch::new();
  let p = |s: &mut Sketch, x: f64, y: f64| s.add_point(x, y);
  let (a, b, c, d) = (p(&mut s, 0.0, 0.0), p(&mut s, 20.0, 0.0), p(&mut s, 0.0, 10.0), p(&mut s, 20.0, 10.0));
  let line = |s: &mut Sketch, a: PointId, b: PointId| s.add_curve(CurveKind::Line { a, b }).unwrap();
  let (bottom, top, left) = (line(&mut s, a, b), line(&mut s, c, d), line(&mut s, a, c));
  // the sketch as the screen at 1 px per mm, y down
  let screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
  let at = |k: ConstraintKind| -> Vec<Pos2> { places(&s, &k, None, &screen).into_iter().map(|p| p.at).collect() };
  let near = |p: Pos2, x: f32, y: f32| (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3;
  let h = at(ConstraintKind::Horizontal { line: bottom });
  assert!(h.len() == 1 && near(h[0], 10.0, GAP), "below the middle: {h:?}");
  let v = at(ConstraintKind::Vertical { line: left });
  assert!(v.len() == 1 && near(v[0], GAP, -5.0), "right of the middle: {v:?}");
  let par = at(ConstraintKind::Parallel { a: bottom, b: top });
  assert!(par.len() == 2 && near(par[0], 10.0, GAP) && near(par[1], 10.0, -10.0 + GAP), "one under each: {par:?}");
  let eq = at(ConstraintKind::Equal { a: bottom, b: top });
  assert!(eq.len() == 2 && near(eq[0], 10.0, 0.0) && near(eq[1], 10.0, -10.0), "on each line: {eq:?}");
  let perp = at(ConstraintKind::Perpendicular { a: bottom, b: left });
  let r = GAP / 2f32.sqrt();
  assert!(perp.len() == 1 && near(perp[0], r, -r), "inside the corner at the origin: {perp:?}");
  let mid = at(ConstraintKind::Midpoint { p: c, line: bottom });
  assert!(mid.len() == 1 && near(mid[0], 0.0, -10.0 - GAP), "above the point: {mid:?}");
  let sym = at(ConstraintKind::Symmetric { a, b, line: left });
  assert!(sym.len() == 3 && near(sym[0], 0.0, -GAP) && near(sym[1], 20.0, -GAP) && near(sym[2], 0.0, -5.0), "{sym:?}");
  assert!(at(ConstraintKind::Coincident { a: b, b: d }).is_empty(), "no coincident glyph at rest");
  let co = places(&s, &ConstraintKind::Coincident { a: b, b: d }, Some(d), &screen);
  assert!(co.len() == 1 && near(co[0].at, 20.0 - GAP, -10.0 + GAP), "below-left of the hovered point: {co:?}");
}

impl Harness {
  /// A sketch on XY with two parallel slanted lines, fitted to the view; returns the sketch,
  /// the Parallel constraint's id and its two glyphs' screen positions.
  fn parallel_sketch(&mut self) -> (fox_engine::model::FeatureId, fox_engine::sketch::ConstraintId, Vec<Pos2>) {
    self.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = self.app.s.edit_sketch.unwrap();
    // slanted, so the lines pick up no Horizontal of their own and the Parallel's glyphs stand alone
    self.app.run("sketch.line", json!({"points": [[0, 0], [40, 10]]})).unwrap();
    self.app.run("sketch.line", json!({"points": [[0, 20], [40, 30]]})).unwrap();
    let curves: Vec<u32> = self.app.s.design.sketch(sk).unwrap().curves.iter().map(|c| c.id.0).collect();
    self.app.run("sketch.constrain", json!({"type": "parallel", "curves": curves})).unwrap();
    let kinds: Vec<&str> = self.app.s.design.sketch(sk).unwrap().constraints.iter().map(|c| c.kind.name()).collect();
    assert_eq!(kinds, ["Parallel"], "only the Parallel, so its glyphs sit unstacked");
    self.app.tab = crate::panels::Tab::Sketch;
    self.app.fit_view_now();
    for _ in 0..3 {
      self.frame(vec![]);
    }
    let s = self.app.s.design.sketch(sk).unwrap();
    let c =
      s.constraints.iter().find(|c| matches!(c.kind, fox_engine::sketch::ConstraintKind::Parallel { .. })).unwrap();
    let app = &self.app;
    let screen = |p: [f64; 2]| Some(on_screen(app, fox_engine::kernel::glam::DVec3::new(p[0], p[1], 0.0)));
    let at: Vec<Pos2> = crate::glyphs::places(s, &c.kind, None, &screen).into_iter().map(|p| p.at).collect();
    (sk, c.id, at)
  }
}

/// Hovering a glyph fills every glyph of its constraint pale blue with dark ink, highlights no
/// geometry and shows no tooltip; clicking it selects the constraint and the status reads its
/// name; right-clicking it opens the marking menu for it.
#[test]
fn a_hovered_glyph_lights_its_constraint_and_a_click_names_it() {
  use crate::viewport::{GLYPH_HOVER_FILL, GLYPH_HOVER_INK};
  let mut h = Harness::new();
  let (sk, id, at) = h.parallel_sketch();
  assert_eq!(at.len(), 2, "a glyph under each line");
  let shapes = h.shapes(vec![Event::PointerMoved(at[0])]);
  let boxes = shapes.iter().filter(|s| matches!(s, egui::Shape::Rect(r) if r.fill == GLYPH_HOVER_FILL)).count();
  assert_eq!(boxes, 2, "both glyphs of the Parallel fill pale blue");
  let dark =
    shapes.iter().any(|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == GLYPH_HOVER_INK));
  assert!(dark, "the glyph's ink turns dark grey");
  assert!(h.app.glyph_hover.is_empty(), "the lines stay unhighlighted");
  let shown = h.rest(at[0], 4.0);
  assert!(!shown.iter().any(|t| t.contains("click to select") || t == "Parallel"), "no tooltip: {shown:?}");
  // a click selects it and the status names it
  h.move_then_click(at[0]);
  assert_eq!(h.app.s.selection, vec![fox_engine::Sel::SketchConstraint { sketch: sk, constraint: id }]);
  let texts = h.frame_texts(1.0 / 60.0, vec![]);
  assert!(
    texts.iter().any(|(_, t)| t == "Parallel Constraint"),
    "{:?}",
    texts.iter().map(|(_, t)| t).collect::<Vec<_>>()
  );
  // a right-click on the other glyph opens the marking menu for the constraint
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![Event::PointerMoved(at[1])]);
  let right = |pressed| Event::PointerButton {
    pos: at[1],
    button: egui::PointerButton::Secondary,
    pressed,
    modifiers: Default::default(),
  };
  h.frame(vec![right(true)]);
  h.frame(vec![right(false)]);
  h.frame(vec![]);
  assert!(h.app.marking.is_some(), "the marking menu opened");
  assert_eq!(h.app.s.selection, vec![fox_engine::Sel::SketchConstraint { sketch: sk, constraint: id }]);
}

/// A CONSTRAINTS row's tooltip is Fusion's dark panel beside the menu: the description and
/// "Press Ctrl+/ for more help.", no title, and nothing more after resting longer.
#[test]
fn a_constraint_row_tooltip_is_fusions_dark_panel() {
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  h.app.tab = crate::panels::Tab::Sketch;
  let all = h.frame_texts(1.0 / 60.0, vec![]);
  let label = panel_label(&all, "CONSTRAINTS").expect("the CONSTRAINTS panel");
  h.move_then_click(label.center());
  let row = h.find(vec![], "Coincident").expect("the Coincident row");
  // egui shows no tooltip for a pointer that moved within 0.1 s of a click
  h.frame_texts(0.3, vec![]);
  let shown = h.rest(row.center(), 1.2);
  let paragraphs = crate::tips::menu_paragraphs(crate::tips::Scope::Sketch, "Coincident");
  assert_eq!(paragraphs.last().copied(), Some(crate::tips::MORE_HELP));
  for p in &paragraphs {
    assert!(shown.iter().any(|t| t == p), "{p:?} in {shown:?}");
  }
  assert_eq!(shown.iter().filter(|t| *t == "Coincident").count(), 1, "the row, not a title: {shown:?}");
  let tip = crate::tips::tip(crate::tips::Scope::Sketch, "Coincident").unwrap();
  assert!(!shown.iter().any(|t| t == tip.line || t == tip.more), "{shown:?}");
  // it sits right of the menu and doesn't change after 3 s
  let texts = h.frame_texts(0.1, vec![]);
  let first = texts.iter().find(|(_, t)| t == paragraphs[0]).map(|(r, _)| *r).expect("the description is drawn");
  assert!(first.left() > row.right(), "beside the menu: {first:?} vs row {row:?}");
  let later = h.rest(row.center(), 3.0);
  assert_eq!(later, shown, "the panel doesn't grow");
}

impl Harness {
  /// Opens a sketch on XY and returns the palette's right edge and the y of the row `label`.
  fn palette_row(&mut self, label: &str) -> (f32, f32) {
    if self.app.s.edit_sketch.is_none() {
      self.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
      for _ in 0..3 {
        self.frame(vec![]);
      }
    }
    let r = self.find(vec![], label).unwrap_or_else(|| panic!("the palette lists {label}"));
    (self.app.canvas.right() - 34.0, r.center().y)
  }
}

/// Fusion's palette tooltips: "Construction" and "Centerline" on the Linetype icons, none on the
/// checkbox rows.
#[test]
fn the_sketch_palette_tooltips_name_the_linetypes_and_nothing_else() {
  let mut h = Harness::new();
  let (right, y) = h.palette_row("Linetype");
  // the icons sit at the row's right end: Centerline, then Construction to its left
  let shown = h.rest(Pos2::new(right - 14.0 - 11.0, y), 1.0);
  assert!(shown.iter().any(|t| t == "Centerline"), "{shown:?}");
  let shown = h.rest(Pos2::new(right - 14.0 - 22.0 - 6.0 - 11.0, y), 1.0);
  assert!(shown.iter().any(|t| t == "Construction"), "{shown:?}");
  assert!(!shown.iter().any(|t| t.contains("Normal/Construction")), "{shown:?}");
  // a checkbox row shows no tooltip: resting on its box draws the same text as resting on its label
  for label in ["Slice", "Profile", "Points", "Projected Geometries", "3D Sketch"] {
    let (right, y) = h.palette_row(label);
    let r = h.find(vec![], label).unwrap();
    let on_label = h.rest(r.center(), 1.0);
    let on_box = h.rest(Pos2::new(right - 14.0 - 9.0, y), 1.5);
    assert_eq!(on_label.len(), on_box.len(), "{label}: {on_box:?}");
    assert!(!on_box.iter().any(|t| t.contains("FissionOxide")), "{on_box:?}");
  }
}

/// Every Options checkbox is live: clicking Profile, Points, Projected Geometries, Construction
/// Geometries, Slice and 3D Sketch toggles the option.
#[test]
fn the_sketch_palette_checkboxes_all_toggle() {
  let mut h = Harness::new();
  let before = h.app.palette;
  for label in ["Slice", "Profile", "Points", "Projected Geometries", "Construction Geometries", "3D Sketch"] {
    let (right, y) = h.palette_row(label);
    h.move_then_click(Pos2::new(right - 14.0 - 9.0, y));
  }
  let p = h.app.palette;
  assert_eq!(p.slice, !before.slice);
  assert_eq!(p.profile, !before.profile);
  assert_eq!(p.points, !before.points);
  assert_eq!(p.projected, !before.projected);
  assert_eq!(p.construction, !before.construction);
  assert_eq!(p.sketch_3d, !before.sketch_3d);
  // the scene follows
  let opts = crate::viewport::scene_options(&mut h.app);
  assert_eq!(opts.sketch_show, p.show());
  assert!(opts.section.is_some(), "Slice sections at the sketch plane");
}

/// The header chevron docks the palette into a vertical SKETCH PALETTE tab at the canvas's right
/// edge; the tab floats the palette out again, where a click elsewhere puts it back and the pin
/// docks it open.
#[test]
fn the_sketch_palette_chevron_collapses_it_into_a_tab() {
  use crate::palette::Dock;
  let mut h = Harness::new();
  let (right, _) = h.palette_row("Linetype");
  let header = h.find(vec![], "SKETCH PALETTE").expect("the header");
  let chevron = Pos2::new(right - 12.0, header.center().y);
  h.move_then_click(chevron);
  assert_eq!(h.app.palette.dock, Dock::Collapsed);
  assert!(h.find(vec![], "Finish Sketch").is_none(), "the palette is gone");
  let tab = h.find(vec![], "SKETCH PALETTE").expect("the tab names it");
  let tab_rect = crate::palette::tab_rect(h.app.canvas);
  assert!(tab_rect.intersects(tab), "{tab:?} in {tab_rect:?}");
  assert!(tab_rect.right() >= h.app.canvas.right() - 2.0, "at the canvas's right edge");
  h.move_then_click(tab_rect.center());
  assert_eq!(h.app.palette.dock, Dock::Floating);
  assert!(h.find(vec![], "Finish Sketch").is_some(), "the palette floats out");
  // a click on the canvas slides it back
  h.move_then_click(h.app.canvas.center() + vec2(0.0, 150.0));
  assert_eq!(h.app.palette.dock, Dock::Collapsed);
  // pinned, it stays
  h.move_then_click(tab_rect.center());
  assert_eq!(h.app.palette.dock, Dock::Floating);
  let header = h.find(vec![], "SKETCH PALETTE").expect("the header");
  h.move_then_click(Pos2::new(right - 24.0 - 12.0, header.center().y));
  assert_eq!(h.app.palette.dock, Dock::Open);
  h.move_then_click(h.app.canvas.center() + vec2(0.0, 150.0));
  assert_eq!(h.app.palette.dock, Dock::Open);
  assert!(h.find(vec![], "Finish Sketch").is_some());
}

/// Fusion's palette ends with the 3D Sketch row and Finish Sketch; the degrees-of-freedom line is
/// a preference, off to begin with.
#[test]
fn the_sketch_palette_has_no_footer_unless_preferred() {
  let mut h = Harness::new();
  h.palette_row("3D Sketch");
  h.app.run("sketch.line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
  let dof =
    |texts: &[(Rect, String)]| texts.iter().any(|(_, t)| t.contains("degrees of freedom") || t == "Fully constrained");
  let texts = h.frame_texts(1.0 / 60.0, vec![]);
  assert!(!dof(&texts), "{texts:?}");
  h.app.preferences.values.show_dof = true;
  let texts = h.frame_texts(1.0 / 60.0, vec![]);
  assert!(dof(&texts), "{texts:?}");
}

/// The Sketches folder and each sketch row carry an eye: the row's eye hides the sketch and
/// leaves it selected, the folder's hides every sketch; hovering a sketch row lights it in the
/// canvas with no tooltip and no status text.
#[test]
fn sketch_rows_have_eyes_and_hover_quietly() {
  let mut h = Harness::new();
  h.app.create_sketch(json!("xy"));
  let sk = h.app.s.edit_sketch.expect("sketching");
  h.app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
  h.app.finish_sketch();
  h.app.browser_sketches = Some(true);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("browser"))).expect("the browser is shown");
  let row = |i: f32| area.min.y + 30.0 + 25.0 * i + 12.5;
  // root, Document Settings, Units, Part Design, Named Views, Origin, then Sketches and Sketch1
  let name = egui::pos2(area.min.x + 8.0 + 32.0 + 44.0 + 30.0, row(7.0));
  h.move_then_click(name);
  assert_eq!(h.app.s.selection, vec![fox_engine::Sel::Feature { feature: sk }], "the row selects the sketch");
  h.frame(vec![Event::PointerMoved(name)]);
  h.frame(vec![]);
  assert_eq!(h.app.ctx_menu.peek_sketches, vec![sk], "hovering lights the sketch");
  assert_eq!(h.app.ctx_menu.peek_label, None, "and names it nowhere");
  let eye = egui::pos2(area.min.x + 8.0 + 32.0 + 24.0, row(7.0));
  h.move_then_click(eye);
  assert!(!fox_engine::scene::sketch_visible(&h.app.s, sk), "the row's eye hides the sketch");
  assert_eq!(h.app.s.selection, vec![fox_engine::Sel::Feature { feature: sk }], "and keeps it selected");
  // with a sketch hidden the folder's eye is drawn only while the pointer is on its row
  h.frame(vec![Event::PointerMoved(h.app.canvas.center())]);
  h.frame(vec![]);
  assert!(h.ctx.read_response(egui::Id::new("tree-sketches").with("eye")).is_none());
  h.move_then_click(eye);
  assert!(fox_engine::scene::sketch_visible(&h.app.s, sk));
  let folder_eye = egui::pos2(area.min.x + 8.0 + 16.0 + 24.0, row(6.0));
  h.move_then_click(folder_eye);
  assert!(h.app.sketch_rows.folder_hidden, "the folder's eye hides the sketches");
  assert!(!crate::panels::sketch_drawn(&h.app, sk));
  h.move_then_click(folder_eye);
  assert!(crate::panels::sketch_drawn(&h.app, sk));
}

/// Hiding the only body greys its eye and leaves the Bodies folder's eye undrawn until the
/// pointer is on the folder's row, where it shows open (the folder still shows), as in Fusion.
/// The folder's eye hides the folder: no body is drawn, each keeping its own eye.
#[test]
fn bodies_folder_eye_waits_for_the_pointer_while_a_body_is_hidden() {
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
  let body = h.app.s.model.bodies[0].id;
  let away = h.app.canvas.center();
  h.frame(vec![Event::PointerMoved(away)]);
  h.frame(vec![]);
  let folder = egui::Id::new("tree-bodies");
  let folder_eye = |h: &Harness| h.ctx.read_response(folder.with("eye")).is_some();
  assert!(folder_eye(&h), "with every body shown the folder's eye is drawn");
  // root, Document Settings, Units, Part Design, Named Views, Origin, then Bodies and Body1
  let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("browser"))).expect("the browser is shown");
  let row = |i: f32| area.min.y + 30.0 + 25.0 * i + 12.5;
  h.move_then_click(egui::pos2(area.min.x + 8.0 + 32.0 + 24.0, row(7.0)));
  assert!(!h.app.s.body_visible(body), "the row's eye hides the body");
  h.frame(vec![Event::PointerMoved(away)]);
  h.frame(vec![]);
  assert!(!folder_eye(&h), "the folder's eye isn't drawn while the pointer is off its row");
  assert!(!h.app.ctx_menu.bodies_hidden);
  h.frame(vec![Event::PointerMoved(egui::pos2(area.min.x + 120.0, row(6.0)))]);
  h.frame(vec![]);
  assert!(folder_eye(&h), "on the folder's row it is drawn");
  h.move_then_click(egui::pos2(area.min.x + 8.0 + 16.0 + 24.0, row(6.0)));
  assert!(h.app.ctx_menu.bodies_hidden, "and hides the folder");
  assert_eq!(crate::viewport::scene_options(&mut h.app).only_bodies, Some(Vec::new()), "no body is drawn");
  h.frame(vec![Event::PointerMoved(away)]);
  h.frame(vec![]);
  assert!(folder_eye(&h), "the hidden folder's struck eye stays drawn");
  h.move_then_click(egui::pos2(area.min.x + 8.0 + 16.0 + 24.0, row(6.0)));
  assert!(!h.app.ctx_menu.bodies_hidden);
  assert!(!h.app.s.body_visible(body), "the body keeps its own eye");
}

/// The first offset plane adds a collapsed Construction folder with an eye, as in Fusion; the
/// eye hides the construction planes.
#[test]
fn construction_folder_starts_collapsed_with_an_eye() {
  let mut h = Harness::new();
  h.app.run("plane.offset", json!({"plane": "xy", "offset": 5})).unwrap();
  let plane = h.app.s.design.features.last().expect("a plane").id;
  h.frame(vec![Event::PointerMoved(h.app.canvas.center())]);
  h.frame(vec![]);
  let folder = egui::Id::new("tree-construction");
  assert!(h.ctx.read_response(folder.with("tri")).is_some(), "the folder is listed");
  assert!(h.ctx.read_response(folder.with("eye")).is_some(), "the folder has an eye");
  // root, Document Settings, Units, Part Design, Named Views, Origin, then Construction
  let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("browser"))).expect("the browser is shown");
  let y = area.min.y + 30.0 + 25.0 * 6.0 + 12.5;
  assert!(area.max.y < y + 12.5 + 10.0, "collapsed: Plane1 isn't showing ({area:?})");
  h.move_then_click(egui::pos2(area.min.x + 8.0 + 16.0 + 24.0, y));
  assert!(!h.app.construction_shown, "the folder's eye hides the planes");
  h.move_then_click(egui::pos2(area.min.x + 8.0 + 16.0 + 4.0, y));
  let open = h.ctx.memory(|m| m.area_rect(egui::Id::new("browser"))).expect("the browser is shown");
  assert!(open.max.y >= y + 12.5 + 25.0, "the expander opens it on Plane1 ({open:?})");
  assert_eq!(h.app.s.design.feature(plane).map(|f| f.name.as_str()), Some("Plane1"));
}

// ---------------------------------------------------------------- Document Settings and Named Views

/// Where the browser draws row `i` (0 is the root): the header is 26 tall, then 4 of space
/// and 25 per row.
fn browser_row(h: &Harness, i: f32) -> Rect {
  let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("browser"))).expect("the browser is shown");
  Rect::from_min_size(egui::pos2(area.min.x, area.min.y + 30.0 + 25.0 * i), vec2(270.0, 25.0))
}

/// Where a depth-2 row's edit pencil sits.
fn row_pencil_at(row: Rect, label: &str) -> egui::Pos2 {
  let ctx = egui::Context::default();
  crate::theme::apply(&ctx);
  let mut at = egui::Pos2::ZERO;
  ctx
    .run_ui(RawInput::default(), |ui| at = crate::doc_dialogs::pencil_rect(ui, row, 2, label).center())
    .drop_without_applying_deltas();
  at
}

#[test]
fn the_part_design_pencil_opens_convert_and_the_units_row_has_none() {
  let mut h = Harness::new();
  // root, Document Settings, Units, Part Design
  let units = browser_row(&h, 2.0);
  let spot = row_pencil_at(units, &crate::units::row_label(h.app.s.design.units));
  h.frame(vec![Event::PointerMoved(units.left_center() + vec2(80.0, 0.0))]);
  h.move_then_click(spot);
  assert!(h.app.doc_dialog.is_none(), "hovering Units shows no pencil to click");
  let part = browser_row(&h, 3.0);
  h.frame(vec![Event::PointerMoved(part.left_center() + vec2(80.0, 0.0))]);
  h.move_then_click(row_pencil_at(part, "Part Design"));
  assert!(crate::doc_dialogs::convert_open(&h.app), "Part Design's pencil is Convert Design Type");
  assert_eq!(h.app.status.as_ref().map(|s| s.0.as_str()), Some(crate::doc_dialogs::CONVERT_PROMPT));
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![]);
  assert!(h.app.doc_dialog.is_none(), "Escape closes it");
}

#[test]
fn change_active_units_is_docked_like_a_command_dialog_and_escape_closes_it() {
  let mut h = Harness::new();
  crate::doc_dialogs::open_units(&mut h.app);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let r = h.ctx.memory(|m| m.area_rect(egui::Id::new(crate::doc_dialogs::AREA))).expect("the dialog is open");
  let dock = crate::panels::dialog_dock(h.app.canvas, false);
  assert!((r.right_top() - dock).length() < 2.0, "docked where command dialogs dock: {r:?} vs {dock:?}");
  assert_eq!(h.app.status.as_ref().map(|s| s.0.as_str()), Some(crate::doc_dialogs::UNITS_PROMPT));
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![]);
  assert!(h.app.doc_dialog.is_none());
  assert!(h.app.status.is_none(), "the Units prompt goes with it");
}

#[test]
fn new_named_view_names_it_inline_as_fusion_does() {
  let mut h = Harness::new();
  let ctx = h.ctx.clone();
  crate::panels::new_named_view(&mut h.app, &ctx);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let names = |h: &Harness| h.app.s.design.named_views.iter().map(|v| v.name.clone()).collect::<Vec<_>>();
  assert_eq!(names(&h), ["NamedView"]);
  assert!(h.app.status.is_none(), "nothing is printed: {:?}", h.app.status);
  // the name is selected in its editor: typing replaces it, Enter keeps it
  let mut ev = typing("Corner");
  ev.push(key(egui::Key::Enter));
  h.frame(ev);
  h.frame(vec![]);
  assert_eq!(names(&h), ["Corner"]);
  // the next one is NamedView again; Escape keeps that name
  crate::panels::new_named_view(&mut h.app, &ctx);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![]);
  assert_eq!(names(&h), ["Corner", "NamedView"]);
  crate::panels::new_named_view(&mut h.app, &ctx);
  h.frame(vec![key(egui::Key::Escape)]);
  assert_eq!(names(&h), ["Corner", "NamedView", "NamedView(1)"]);
}

#[test]
fn the_expanded_comments_panel_shrinks_the_browser() {
  let mut h = Harness::new();
  let before = crate::panels::browser_max_height(&h.app);
  h.app.comments.open = true;
  h.frame(vec![]);
  assert_eq!(crate::panels::browser_max_height(&h.app), before - crate::comments::BODY_H);
}

// ---------------------------------------------------------------- navigation bar

/// The centre of the navigation bar's fifth button (after Orbit, its caret, Look At, Pan, Zoom).
fn zoom_window_button(h: &Harness) -> Pos2 {
  let bar = h.ctx.memory(|m| m.area_rect(egui::Id::new("navbar"))).expect("the navigation bar is drawn");
  Pos2::new(bar.left() + 28.0 + 10.0 + 28.0 * 3.0 + 14.0, bar.center().y)
}

#[test]
fn the_fifth_navigation_button_is_zoom_window_with_a_one_word_tooltip() {
  use crate::NavMode;
  let mut h = Harness::new();
  let at = zoom_window_button(&h);
  let shown = h.rest(at, 1.5);
  assert!(shown.iter().any(|t| t == "Zoom Window"), "its tooltip: {shown:?}");
  assert!(!shown.iter().any(|t| t.contains("Fit") || t.contains("drag")), "no long hint: {shown:?}");
  h.move_then_click(at);
  assert_eq!(h.app.nav_mode, Some(NavMode::ZoomWindow), "the click starts Zoom Window");
  let canvas = h.app.canvas;
  let label = h.find(vec![Event::PointerMoved(canvas.center())], "Zoom Window").expect("the tool's label shows");
  assert!(label.left() < canvas.left() + 40.0 && label.bottom() > canvas.bottom() - 120.0, "at the bottom: {label:?}");
  h.frame(vec![escape_key()]);
  h.frame(vec![]);
  assert_eq!(h.app.nav_mode, None, "Esc ends it");
  assert!(h.find(vec![], "Zoom Window").is_none(), "and its label goes");
}

/// `ui:dialogs:primitive-plane-pick-flow`: Box shows no dialog at first. The cursor asks for a
/// plane, a click on the XY plane starts the rectangle, two clicks draw it, and only then does
/// the dialog open with the box previewed.
#[test]
fn box_is_drawn_on_a_picked_plane_before_its_dialog_opens() {
  let mut h = Harness::new();
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  h.app.start(crate::dialogs::Kind::Box);
  // (the view turns to Top over a few frames)
  for _ in 0..90 {
    h.frame(vec![]);
  }
  let c = h.app.canvas.center();
  assert!(h.prompt_at(c, crate::dialogs::PICK_PLANE_HINT));
  assert!(h.ctx.memory(|m| m.area_rect(egui::Id::new("feature-dialog"))).is_none(), "no dialog yet");
  // (off the origin, where the XZ and YZ planes are seen edge on)
  h.move_then_click(c + vec2(30.0, 20.0));
  assert!(h.prompt_at(c, "Place first corner"), "the XY plane is picked");
  h.move_then_click(c);
  let b = c + vec2(60.0, -40.0);
  assert!(h.prompt_at(b, "Specify size of rectangle"));
  h.move_then_click(b);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let d = h.app.dialog.as_ref().expect("the dialog is open");
  assert!(!d.placing() && d.preview_applied(), "{:?}", d.prompt(false));
  assert!(h.ctx.memory(|m| m.area_rect(egui::Id::new("feature-dialog"))).is_some());
  assert_eq!(h.app.s.model.bodies.len(), 1, "the box is previewed");
}

/// ui-status-readouts: hovering a face prints nothing in the status bar; a click selects the edge
/// and a Ctrl+click on a face adds it, the readout counting both kinds with their least distance.
#[test]
fn ctrl_click_adds_a_face_to_an_edge_and_reads_both() {
  use fox_engine::kernel::glam::DVec3;
  use fox_engine::scene::Pick;
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  h.settle();
  let top = h.screen(DVec3::new(0.0, 0.0, 20.0));
  h.frame(vec![Event::PointerMoved(top)]);
  h.frame(vec![]);
  assert!(matches!(h.app.hover, Some(Pick::Face { .. })), "hover {:?}", h.app.hover);
  assert_eq!(crate::panels::selection_text(&h.app), None, "a hovered face reads nothing");
  let edge_at = h.screen(DVec3::new(0.0, 10.0, 20.0));
  h.move_then_click(edge_at);
  let [edge @ fox_engine::Sel::Edge { .. }] = h.app.s.selection[..] else { panic!("{:?}", h.app.s.selection) };
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("1 Edge | Length : 20.00 mm"));
  h.mods = egui::Modifiers::COMMAND;
  h.move_then_click(top);
  h.mods = egui::Modifiers::NONE;
  assert_eq!(h.app.s.selection.len(), 2, "{:?}", h.app.s.selection);
  assert_eq!(h.app.s.selection[0], edge, "the Ctrl+click keeps the edge");
  assert!(matches!(h.app.s.selection[1], fox_engine::Sel::Face { .. }));
  // the edge lies on the top face
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("1 Face 1 Edge | Min Distance : 0.00 mm"));
}

/// ui-status-readouts: Escape after clicking a timeline icon clears the selection and the readout,
/// and the icon still under the pointer doesn't light its body again until the pointer moves.
#[test]
fn escape_clears_a_timeline_pick_and_its_highlight() {
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  h.settle();
  let body = h.app.s.model.bodies[0].id;
  let feature = h.app.s.design.features[0].id;
  // find the feature's icon along the timeline at the window's foot
  let mut icon = None;
  'scan: for y in (860..900).step_by(4) {
    for x in (0..600).step_by(4) {
      let p = egui::pos2(x as f32, y as f32);
      h.frame(vec![Event::PointerMoved(p)]);
      h.frame(vec![]);
      if h.app.ctx_menu.peek == vec![body] {
        icon = Some(p + vec2(4.0, 0.0));
        break 'scan;
      }
    }
  }
  let icon = icon.expect("the timeline icon lights the body");
  assert_eq!(crate::panels::selection_text(&h.app), None, "a hovered icon reads nothing");
  h.move_then_click(icon);
  assert_eq!(h.app.s.selection, vec![fox_engine::Sel::Feature { feature }]);
  assert_eq!(crate::panels::selection_text(&h.app).as_deref(), Some("Box1"));
  h.frame(vec![key(egui::Key::Escape)]);
  h.frame(vec![]);
  assert!(h.app.s.selection.is_empty(), "{:?}", h.app.s.selection);
  assert_eq!(crate::panels::selection_text(&h.app), None);
  assert!(h.app.ctx_menu.peek.is_empty(), "nothing stays lit while the pointer rests on the icon");
  h.frame(vec![]);
  assert!(h.app.ctx_menu.peek.is_empty());
  // moving over the icon lights it again
  h.frame(vec![Event::PointerMoved(icon + vec2(1.0, 0.0))]);
  h.frame(vec![]);
  assert_eq!(h.app.ctx_menu.peek, vec![body]);
}

/// Fusion's Cancel closes a dialog with nothing selected: Combine's two bodies and Offset Face's
/// face go back to unselected (no blue, an empty status bar), and Combine's whole-body picking
/// ends with the dialog.
#[test]
fn cancel_leaves_nothing_selected() {
  use crate::dialogs::Kind;
  use fox_engine::Sel;
  let mut h = Harness::new();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  h.app.run("box", json!({"width": 20, "depth": 20, "height": 20, "at": [40, 0]})).unwrap();
  let (b1, b2) = (h.app.s.model.bodies[0].id, h.app.s.model.bodies[1].id);
  h.app.s.selection.clear();
  h.app.start(Kind::Combine);
  h.frame(vec![]);
  assert_eq!(h.app.sel_priority, crate::select::Priority::Body, "Combine picks whole bodies");
  h.app.s.selection = vec![Sel::Body { body: b1 }, Sel::Body { body: b2 }];
  for _ in 0..3 {
    h.frame(vec![]);
  }
  assert!(h.app.s.model.body(b2).is_none(), "the preview consumes the tool body");
  h.move_then_click(cancel_pos(h.dialog_rect()));
  assert!(h.app.dialog.is_none(), "Cancel closes the dialog");
  assert!(h.app.s.selection.is_empty(), "{:?}", h.app.s.selection);
  assert!(h.app.s.model.body(b2).is_some(), "the preview is withdrawn");
  assert_eq!(h.app.sel_priority, crate::select::Priority::Face);

  let b = &h.app.s.model.bodies[0];
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  h.app.s.selection = vec![Sel::Face { body: b.id, face: top }];
  h.app.start(Kind::OffsetFace);
  for _ in 0..3 {
    h.frame(vec![]);
  }
  assert!(h.app.dialog.is_some());
  h.move_then_click(cancel_pos(h.dialog_rect()));
  assert!(h.app.dialog.is_none());
  assert!(h.app.s.selection.is_empty(), "{:?}", h.app.s.selection);
}

/// A 20 mm square sketched on XY with its profile picked and Extrude open on it, its floating
/// value box shown.
fn extrude_on_a_square() -> Harness {
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  h.app.run("sketch.rect", json!({"corner1": [-10, -10], "corner2": [10, 10]})).unwrap();
  let sk = h.app.s.edit_sketch.unwrap();
  h.app.run("sketch.finish", json!({})).unwrap();
  h.app.s.selection = vec![fox_engine::Sel::Profile { sketch: sk, index: 0 }];
  h.app.start(crate::dialogs::Kind::Extrude);
  h.app.fit_view_now();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  h
}

/// `ui:manip:tab-cycles-boxes`: Tab in Extrude's floating box applies the typed distance and
/// moves the box (and the keyboard) to the taper handle with "0.0 deg" selected; typing there
/// sets the taper, and Tab brings the box back to the distance, as in Fusion.
#[test]
fn tab_moves_the_value_box_to_the_taper_handle_and_back() {
  let mut h = extrude_on_a_square();
  let field = |h: &mut Harness| h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone();
  let taper = |h: &mut Harness| h.app.dialog.as_mut().unwrap().handle_field(0).unwrap().clone();
  h.frame(typing("25"));
  h.frame(vec![]);
  assert_eq!(field(&mut h), "25 mm");
  h.frame(vec![key(egui::Key::Tab)]);
  h.frame(vec![]);
  assert_eq!(h.app.manip.value_box.at_handle, Some(0), "the box is at the taper handle");
  let focused = h.ctx.memory(|m| m.focused()).expect("the box keeps the keyboard");
  assert_eq!(h.ctx.read_response(focused).map(|r| r.layer_id), Some(crate::value_box::layer()));
  assert_eq!(taper(&mut h), "0.0 deg");
  assert_eq!(field(&mut h), "25 mm", "Tab applied the typed distance");
  // the taper's value is selected: typing replaces it, with its unit written out
  h.frame(typing("5"));
  h.frame(vec![]);
  assert_eq!(taper(&mut h), "5 deg");
  h.frame(vec![key(egui::Key::Tab)]);
  h.frame(vec![]);
  assert_eq!(h.app.manip.value_box.at_handle, None, "back at the distance arrow");
  assert!(h.ctx.memory(|m| m.focused()).is_some());
  h.frame(typing("30"));
  h.frame(vec![]);
  assert_eq!((field(&mut h), taper(&mut h)), ("30 mm".to_string(), "5 deg".to_string()));
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert!(h.app.dialog.is_none(), "Enter is OK");
  let f = h.app.s.design.features.last().unwrap();
  let fox_engine::model::FeatureKind::Extrude { extent, .. } = &f.kind else { panic!("{:?}", f.kind) };
  // Edit Feature shows the values with their units
  assert_eq!((extent.distance.as_str(), extent.taper.as_str()), ("30 mm", "5 deg"));
}

/// `ui:manip:preview-hover-highlight`: the pointer over Extrude's preview body lights nothing up.
#[test]
fn the_extrude_preview_is_not_hover_highlighted() {
  let mut h = extrude_on_a_square();
  h.frame(typing("20"));
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let preview = h.app.dialog.as_ref().unwrap().preview_bodies(&h.app.s);
  assert_eq!(preview.len(), 1, "the preview shows");
  // the canvas's middle is on the preview (the profile is centred on the origin, the view fitted)
  let c = h.app.canvas.center();
  for at in [c, c + vec2(4.0, 6.0), c + vec2(-5.0, -3.0)] {
    h.frame(vec![Event::PointerMoved(at)]);
    h.frame(vec![]);
    let on_preview = h.app.hover.as_ref().and_then(crate::select::pick_body).is_some_and(|b| preview.contains(&b));
    assert!(!on_preview, "{:?} lit up at {at:?}", h.app.hover);
  }
  // the pointer still reads as over the model: the prompt is Fusion's "Hold Ctrl to modify
  // selection"
  assert!(h.app.tstate.over_preview);
  let prompt = h.app.dialog.as_ref().unwrap().prompt(h.app.hover.is_some() || h.app.tstate.over_preview);
  assert_eq!(prompt.as_deref(), Some("Hold Ctrl to modify selection"));
}

/// `ui:surface:finish-sketch-returns-tab`: a sketch started from the SURFACE tab comes back to
/// SURFACE on Finish Sketch, and one started from SOLID to SOLID.
#[test]
fn finish_sketch_returns_to_the_tab_it_started_from() {
  use crate::panels::Tab;
  let mut h = Harness::new();
  for tab in [Tab::Surface, Tab::Solid, Tab::Surface] {
    h.app.tab = tab;
    h.frame(vec![]);
    h.app.create_sketch(json!("xy"));
    h.frame(vec![]);
    assert_eq!(h.app.tab, Tab::Sketch);
    h.app.finish_sketch();
    h.frame(vec![]);
    assert_eq!(h.app.tab, tab);
  }
}

/// Resting on a sketch dimension shows its parameter as Fusion does: "d1 = 40.0 mm" in white
/// on the dark tooltip panel, not egui's light box.
#[test]
fn a_hovered_dimensions_tooltip_is_dark() {
  use crate::theme;
  let mut h = Harness::new();
  h.app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  let line = h.app.run("sketch.line", json!({"points": [[-20, 0], [20, 0]]})).unwrap();
  let c = line["curves"][0].clone();
  h.app.run("sketch.dimension", json!({"type": "distance", "curves": [c], "value": "40"})).unwrap();
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  h.app.fit_view_now();
  for _ in 0..3 {
    h.frame(vec![]);
  }
  let label = h.find(vec![], "40.00").expect("the dimension's value is drawn");
  let shown = h.rest_rects(label.center(), 1.2);
  let tip = shown.iter().find(|(_, t)| t == "d1 = 40.0 mm").map(|(r, _)| *r);
  let tip = tip.unwrap_or_else(|| panic!("the parameter tooltip: {shown:?}"));
  assert!(tip.top() > label.top(), "under the text: {tip:?} vs {label:?}");
  let shapes = h.frame_shapes(0.1);
  assert!(
    shapes.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == theme::TOOLTIP && r.rect.contains_rect(tip))),
    "a dark panel behind the tooltip"
  );
  let color = shapes.iter().find_map(|s| match s {
    egui::Shape::Text(t) if t.galley.text() == "d1 = 40.0 mm" => Some(text_color(t)),
    _ => None,
  });
  assert_eq!(color, Some(theme::TOOLTIP_TITLE));
}
