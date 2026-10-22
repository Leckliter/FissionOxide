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
}

impl Harness {
  fn new() -> Harness {
    Harness::with_width(1440.0)
  }

  fn with_width(width: f32) -> Harness {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness { ctx, app: App::with_session(Session::default()), time: 0.0, width };
    for _ in 0..3 {
      h.frame(vec![]);
    }
    h
  }

  fn frame(&mut self, events: Vec<Event>) {
    self.frame_in(vec2(self.width, 900.0), events);
  }

  /// A frame in a window of `size` (the window was resized or maximised).
  fn frame_in(&mut self, size: egui::Vec2, events: Vec<Event>) {
    self.time += 1.0 / 60.0;
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

  /// Opens a command dialog (Box: it needs no picks) and lets it settle.
  fn open_dialog(&mut self) {
    self.app.start(crate::dialogs::Kind::Box);
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
  // egui ids a window by its title
  let w = h.ctx.memory(|m| m.area_rect(egui::Id::new(Some("Save")))).expect("the Save window is shown");
  // Save is the first button on the window's bottom row
  h.move_then_click(w.left_bottom() + vec2(30.0, -18.0));
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
      let c = s.curves.last().unwrap().id;
      let [a, b] = [0, 1].map(|k| s.pos(s.curve_points(c)[k]).unwrap());
      let ang = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
      assert!(d == [40.0] && (ang - 20.0).abs() < 1e-6, "{tool:?}: {d:?} at {ang}°");
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
/// the canvas (and the scene's short axes), each about 120 px wide.
#[test]
fn the_origin_folder_shows_camera_sized_planes() {
  use fox_engine::kernel::glam::DVec3;
  let mut h = Harness::new();
  assert!(!h.app.origin_shown, "Origin is hidden by default");
  h.app.origin_shown = true;
  h.frame(vec![]);
  let half = crate::viewport::origin_plane_half_size(&h.app);
  let a = on_screen(&h.app, DVec3::new(-half, 0.0, 0.0));
  let b = on_screen(&h.app, DVec3::new(half, 0.0, 0.0));
  // the XY square's width along X, foreshortened by the home view's turn
  let w = (b - a).length();
  assert!((60.0..=125.0).contains(&w), "the origin plane spans {w} px");
  // looked at straight on it is the full 120 px
  h.app.cam.set_view(fox_engine::render::StandardView::Top);
  let a = on_screen(&h.app, DVec3::new(-half, 0.0, 0.0));
  let b = on_screen(&h.app, DVec3::new(half, 0.0, 0.0));
  assert!(((b - a).length() - 120.0).abs() < 1.0, "{}", (b - a).length());
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
  let distance = h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone();
  assert_eq!(distance, "25");
  let features = h.app.s.design.features.len();
  h.frame(vec![key(egui::Key::Enter)]);
  h.frame(vec![]);
  assert!(h.app.dialog.is_none(), "Enter in the box is OK");
  assert_eq!(h.app.s.design.features.len(), features, "the previewed extrude stayed");
  let top = h.app.s.model.bodies[0].shape.mass().unwrap().bbox_max.z;
  assert!((top - 25.0).abs() < 1e-6, "extruded by the typed 25 mm: {top}");
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
  assert_eq!(h.app.dialog.as_mut().unwrap().value_box_field().unwrap().clone(), ".1");
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
