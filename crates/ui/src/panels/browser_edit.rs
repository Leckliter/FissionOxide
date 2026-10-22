//! Browser editing as in Fusion: renaming a row in place (double-click or slow-click its name,
//! Enter commits, Escape cancels) and dragging bodies and components onto components.

use egui::{Color32, Rect, Stroke, pos2, vec2};
use fox_engine::Sel;
use fox_engine::model::{BodyId, ComponentId};
use serde_json::json;

use super::RowEvents;
use crate::{App, RenameTarget, theme};

/// The row being renamed in place: (row, what it names, the text so far, focused yet).
#[derive(Clone)]
struct Editing {
  row: egui::Id,
  target: RenameTarget,
  text: String,
  started: bool,
}

/// A click on a row that was already selected: (row, what it names, its name, when).
#[derive(Clone)]
struct SlowClick {
  row: egui::Id,
  target: RenameTarget,
  name: String,
  at: f64,
}

/// How long a slow click waits for a double click before it starts renaming.
const SLOW_CLICK_SECS: f64 = 0.5;

fn editing_id() -> egui::Id {
  egui::Id::new("tree-rename")
}

fn slow_id() -> egui::Id {
  egui::Id::new("tree-slow-click")
}

/// Opens the in-place rename box on `row`.
pub(super) fn begin(ui: &egui::Ui, row: egui::Id, target: RenameTarget, name: &str) {
  begin_in(ui.ctx(), row, target, name);
}

/// [`begin`], from outside a browser row (New Named View).
pub(super) fn begin_in(ctx: &egui::Context, row: egui::Id, target: RenameTarget, name: &str) {
  let e = Editing { row, target, text: name.to_string(), started: false };
  ctx.data_mut(|d| {
    d.insert_temp(editing_id(), e);
    d.remove::<SlowClick>(slow_id());
  });
}

/// Whether `row`'s name is being edited.
pub(super) fn is_editing(ui: &egui::Ui, row: egui::Id) -> bool {
  ui.data(|d| d.get_temp::<Editing>(editing_id())).is_some_and(|e| e.row == row)
}

/// Slow-click rename: clicking the name of a row that was already selected (and not clicking
/// again soon, which would be a double click) starts renaming it.
pub(super) fn slow_click(ui: &egui::Ui, ev: &RowEvents, row: egui::Id, target: RenameTarget, name: &str, was: bool) {
  let pending = ui.data(|d| d.get_temp::<SlowClick>(slow_id()));
  let plain = ui.input(|i| i.modifiers.is_none());
  if ev.double_clicked || ((ev.clicked || ev.double_clicked) && pending.as_ref().is_some_and(|p| p.row != row)) {
    ui.data_mut(|d| d.remove::<SlowClick>(slow_id()));
  }
  if ev.clicked && was && plain && !is_editing(ui, row) {
    let at = ui.input(|i| i.time);
    ui.data_mut(|d| d.insert_temp(slow_id(), SlowClick { row, target, name: name.to_string(), at }));
  }
}

/// Starts the rename a slow click is waiting on once no double click came, if `row` is still
/// selected. Called for each row after its selection is updated.
pub(super) fn slow_click_due(ui: &egui::Ui, row: egui::Id, selected: bool) {
  let Some(p) = ui.data(|d| d.get_temp::<SlowClick>(slow_id())).filter(|p| p.row == row) else { return };
  if !selected {
    ui.data_mut(|d| d.remove::<SlowClick>(slow_id()));
    return;
  }
  let wait = p.at + SLOW_CLICK_SECS - ui.input(|i| i.time);
  if wait <= 0.0 {
    begin(ui, row, p.target, &p.name);
  } else {
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
  }
}

/// Draws the rename box over the name of `row` (laid out as `super::tree_row` does at `depth`)
/// while it is being renamed. Enter or clicking away commits, Escape cancels.
pub(super) fn edit_box(app: &mut App, ui: &mut egui::Ui, ev: &RowEvents, row: egui::Id, depth: usize) {
  let Some(mut e) = ui.data(|d| d.get_temp::<Editing>(editing_id())).filter(|e| e.row == row) else { return };
  let Some(resp) = &ev.response else { return };
  let rect = resp.rect;
  let x = rect.min.x + 8.0 + depth as f32 * 16.0 + 44.0 + 11.0;
  let r = Rect::from_min_max(pos2(x, rect.center().y - 10.5), pos2(rect.max.x - 6.0, rect.center().y + 10.5));
  let id = row.with("rename");
  let edit = egui::TextEdit::singleline(&mut e.text).id(id).margin(vec2(3.0, 2.0)).desired_width(r.width());
  // Escape makes the box give up focus as a click away would; other handlers may have consumed
  // the key by now, so look at the frame's raw events
  let escape = ui.input(|i| {
    i.raw.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Escape, pressed: true, .. }))
  });
  let out = ui.place(r, edit);
  let was_started = e.started;
  if !e.started {
    out.request_focus();
    // the whole name is selected, so typing replaces it
    if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
      let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(e.text.len()));
      st.cursor.set_char_range(Some(all));
      st.store(ui.ctx(), id);
    }
    e.started = true;
  }
  if escape {
    ui.data_mut(|d| d.remove::<Editing>(editing_id()));
  } else if out.lost_focus() || (was_started && !out.has_focus()) {
    // Enter, or a click somewhere else
    ui.data_mut(|d| d.remove::<Editing>(editing_id()));
    commit(app, e.target, e.text.trim());
  } else {
    ui.data_mut(|d| d.insert_temp(editing_id(), e));
  }
}

fn commit(app: &mut App, target: RenameTarget, name: &str) {
  if name.is_empty() {
    return;
  }
  match target {
    RenameTarget::Feature(f) => app.run("feature.rename", json!({"feature": f, "name": name})),
    RenameTarget::Body(b) => {
      if app.s.body_name(BodyId(b)) == name {
        return;
      }
      app.run("body.rename", json!({"body": b, "name": name}))
    }
    RenameTarget::Component(c) => app.run("component.rename", json!({"component": c, "name": name})),
    RenameTarget::Group(g) => app.run("timeline.group_rename", json!({"group": g, "name": name})),
    RenameTarget::NamedView(i) => {
      let Some(old) = app.s.design.named_views.get(i as usize).map(|v| v.name.clone()) else { return };
      if old == name {
        return;
      }
      app.run("view.named.rename", json!({"name": old, "to": name}))
    }
  };
}

/// Opacity Control: a submenu with a slider that makes bodies see-through (a view state, like
/// Isolate; the design doesn't change).
pub(super) fn opacity_menu(app: &mut App, ui: &mut egui::Ui, bodies: &[BodyId]) {
  ui.menu_button("Opacity Control", |ui| {
    let alpha = bodies.first().and_then(|b| app.ctx_menu.opacity.iter().find(|o| o.0 == *b)).map_or(255, |o| o.1);
    let mut pct = (alpha as f32 / 2.55).round();
    let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").integer());
    if r.changed() {
      set_opacity(app, bodies, (pct * 2.55).round() as u8);
    }
  });
}

/// Sets bodies' opacity (255 = opaque, the default).
pub(crate) fn set_opacity(app: &mut App, bodies: &[BodyId], alpha: u8) {
  let o = &mut app.ctx_menu.opacity;
  o.retain(|(b, _)| !bodies.contains(b));
  if alpha < 255 {
    o.extend(bodies.iter().map(|b| (*b, alpha)));
  }
}

// ---------------------------------------------------------------- drag and drop

/// What a browser row drags.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Dragged {
  Bodies(Vec<BodyId>),
  Component(u32),
}

/// Where a dragged row can land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DropOn {
  /// Into a component.
  Component(u32),
  /// The Components folder: back to the top level.
  TopLevel,
}

/// The bodies a body row drags: the selected bodies when it is one of them, else just it.
pub(super) fn dragged_bodies(app: &App, body: BodyId) -> Dragged {
  let sel: Vec<BodyId> =
    app.s.selection.iter().filter_map(|s| if let Sel::Body { body } = s { Some(*body) } else { None }).collect();
  Dragged::Bodies(if sel.contains(&body) { sel } else { vec![body] })
}

/// Makes a row draggable; while dragged its name follows the pointer.
pub(super) fn drag_source(ui: &egui::Ui, ev: &RowEvents, what: Dragged, label: &str) {
  let Some(r) = &ev.response else { return };
  if !r.dragged() {
    return;
  }
  r.dnd_set_drag_payload(what);
  ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
  if let Some(p) = ui.ctx().pointer_interact_pos() {
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tree-drag")));
    let font = egui::FontId::proportional(13.0);
    let galley = painter.layout_no_wrap(label.to_string(), font, theme::TEXT);
    let at = p + vec2(14.0, 6.0);
    let bg = Rect::from_min_size(at, galley.size() + vec2(12.0, 6.0));
    painter.rect(bg, 2.0, theme::PILL, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
    painter.galley(at + vec2(6.0, 3.0), galley, theme::TEXT);
  }
}

/// Whether `what` can be dropped on `on`.
fn accepts(app: &App, what: &Dragged, on: DropOn) -> bool {
  let d = &app.s.design;
  match (what, on) {
    (Dragged::Bodies(bodies), DropOn::Component(c)) => {
      d.component(ComponentId(c)).is_some_and(|comp| bodies.iter().any(|b| !comp.bodies.contains(b)))
    }
    (Dragged::Bodies(_), DropOn::TopLevel) => false,
    (Dragged::Component(x), DropOn::Component(c)) => {
      // not into itself or anything inside it, nor where it already is
      let mut p = Some(ComponentId(c));
      for _ in 0..64 {
        match p {
          Some(id) if id.0 == *x => return false,
          Some(id) => p = d.component(id).and_then(|c| c.parent),
          None => break,
        }
      }
      d.component(ComponentId(*x)).is_some_and(|comp| comp.parent != Some(ComponentId(c)))
    }
    (Dragged::Component(x), DropOn::TopLevel) => d.component(ComponentId(*x)).is_some_and(|c| c.parent.is_some()),
  }
}

/// Makes a row a drop target: an accent outline shows where a dragged row would land, and
/// releasing it there moves it.
pub(super) fn drop_target(app: &mut App, ui: &egui::Ui, ev: &RowEvents, on: DropOn) {
  let Some(r) = &ev.response else { return };
  let Some(what) = r.dnd_hover_payload::<Dragged>() else { return };
  if !accepts(app, &what, on) {
    ui.ctx().set_cursor_icon(egui::CursorIcon::NoDrop);
    return;
  }
  ui.painter().rect(
    r.rect.shrink2(vec2(2.0, 1.0)),
    3.0,
    Color32::from_rgba_unmultiplied(0, 150, 255, 30),
    Stroke::new(1.5, theme::ACCENT),
    egui::StrokeKind::Inside,
  );
  if let Some(what) = r.dnd_release_payload::<Dragged>() {
    drop(app, &what, on);
  }
}

/// Moves dragged rows: bodies into a component, a component into another or to the top level.
fn drop(app: &mut App, what: &Dragged, on: DropOn) {
  match (what, on) {
    (Dragged::Bodies(bodies), DropOn::Component(c)) => {
      let ids: Vec<u32> = bodies.iter().map(|b| b.0).collect();
      app.run("component.add_bodies", json!({"component": c, "bodies": ids}));
    }
    (Dragged::Component(x), DropOn::Component(c)) => {
      app.run("component.parent", json!({"component": x, "parent": c}));
    }
    (Dragged::Component(x), DropOn::TopLevel) => {
      app.run("component.parent", json!({"component": x, "parent": null}));
    }
    (Dragged::Bodies(_), DropOn::TopLevel) => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn app_with_components() -> (App, Vec<BodyId>, u32, u32) {
    let mut app = App::with_session(Session::default());
    for x in [0, 20] {
      for (c, p) in [
        ("sketch.create", json!({"plane": "xy"})),
        ("sketch.rect", json!({"corner1": [x, 0], "corner2": [x + 10, 10]})),
        ("extrude", json!({"distance": 5, "operation": "new_body"})),
      ] {
        app.run(c, p).unwrap();
      }
    }
    let bodies: Vec<BodyId> = app.s.model.bodies.iter().map(|b| b.id).collect();
    assert_eq!(bodies.len(), 2, "two bodies to drag");
    let a = app.run("component.create", json!({"bodies": [bodies[0].0]})).unwrap()["component"].as_u64().unwrap();
    let b = app.run("component.create", json!({"bodies": [bodies[1].0]})).unwrap()["component"].as_u64().unwrap();
    (app, bodies, a as u32, b as u32)
  }

  #[test]
  fn dropping_a_body_on_a_component_moves_it_there() {
    let (mut app, bodies, a, b) = app_with_components();
    let what = Dragged::Bodies(vec![bodies[0]]);
    assert!(!accepts(&app, &what, DropOn::Component(a)), "already there");
    assert!(accepts(&app, &what, DropOn::Component(b)));
    assert!(!accepts(&app, &what, DropOn::TopLevel));
    drop(&mut app, &what, DropOn::Component(b));
    let comp = app.s.design.component(ComponentId(b)).unwrap();
    assert!(comp.bodies.contains(&bodies[0]) && comp.bodies.contains(&bodies[1]));
    assert!(!app.s.design.component(ComponentId(a)).unwrap().bodies.contains(&bodies[0]));
  }

  #[test]
  fn components_nest_by_drag_and_come_back_out_on_the_folder() {
    let (mut app, _, a, b) = app_with_components();
    assert!(!accepts(&app, &Dragged::Component(a), DropOn::Component(a)), "not into itself");
    assert!(!accepts(&app, &Dragged::Component(a), DropOn::TopLevel), "already top level");
    drop(&mut app, &Dragged::Component(b), DropOn::Component(a));
    assert_eq!(app.s.design.component(ComponentId(b)).unwrap().parent, Some(ComponentId(a)));
    assert!(!accepts(&app, &Dragged::Component(a), DropOn::Component(b)), "not into its own child");
    assert!(accepts(&app, &Dragged::Component(b), DropOn::TopLevel));
    drop(&mut app, &Dragged::Component(b), DropOn::TopLevel);
    assert_eq!(app.s.design.component(ComponentId(b)).unwrap().parent, None);
  }

  #[test]
  fn the_selected_bodies_drag_together() {
    let (mut app, bodies, _, _) = app_with_components();
    app.s.selection = bodies.iter().map(|b| Sel::Body { body: *b }).collect();
    assert_eq!(dragged_bodies(&app, bodies[1]), Dragged::Bodies(bodies.clone()));
    app.s.selection = vec![Sel::Body { body: bodies[0] }];
    assert_eq!(dragged_bodies(&app, bodies[1]), Dragged::Bodies(vec![bodies[1]]));
  }

  #[test]
  fn opacity_makes_bodies_see_through_until_set_back() {
    let (mut app, bodies, _, _) = app_with_components();
    set_opacity(&mut app, &bodies, 128);
    assert_eq!(app.ctx_menu.opacity.len(), 2);
    let opts = fox_engine::scene::ViewOptions { opacity: app.ctx_menu.opacity.clone(), ..Default::default() };
    let (scene, _) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
    assert!(scene.meshes.iter().filter(|m| m.analyze).all(|m| m.color[3] == 128));
    set_opacity(&mut app, &bodies[..1], 255);
    assert_eq!(app.ctx_menu.opacity, vec![(bodies[1], 128)]);
  }

  #[test]
  fn committing_a_rename_renames_the_body() {
    let (mut app, bodies, a, _) = app_with_components();
    commit(&mut app, RenameTarget::Body(bodies[0].0), "Bracket");
    assert_eq!(app.s.body_name(bodies[0]), "Bracket");
    commit(&mut app, RenameTarget::Component(a), "Arm");
    assert_eq!(app.s.design.component(ComponentId(a)).unwrap().name, "Arm");
    // an empty name keeps the old one
    commit(&mut app, RenameTarget::Body(bodies[0].0), "");
    assert_eq!(app.s.body_name(bodies[0]), "Bracket");
  }
}

/// The browser driven through a real egui context: double-clicks, slow clicks, typing and drags.
#[cfg(test)]
mod input_tests {
  use egui::{Event, Pos2, RawInput, Rect, vec2};
  use fox_engine::Session;
  use fox_engine::model::{BodyId, ComponentId};
  use serde_json::json;

  use crate::App;

  struct Harness {
    ctx: egui::Context,
    app: App,
    time: f64,
  }

  impl Harness {
    fn new(boxes: usize) -> Harness {
      let ctx = egui::Context::default();
      crate::theme::apply(&ctx);
      let mut app = App::with_session(Session::default());
      for i in 0..boxes {
        let x = 20 * i as i64;
        for (c, p) in [
          ("sketch.create", json!({"plane": "xy"})),
          ("sketch.rect", json!({"corner1": [x, 0], "corner2": [x + 10, 10]})),
          ("extrude", json!({"distance": 5, "operation": "new_body"})),
        ] {
          app.run(c, p).unwrap();
        }
      }
      let mut h = Harness { ctx, app, time: 0.0 };
      h.idle(3);
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

    fn idle(&mut self, frames: usize) {
      for _ in 0..frames {
        self.frame(vec![]);
      }
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
      Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    fn click(&mut self, pos: Pos2) {
      self.frame(vec![Event::PointerMoved(pos)]);
      self.frame(vec![Self::button(pos, true)]);
      self.frame(vec![Self::button(pos, false)]);
    }

    fn double_click(&mut self, pos: Pos2) {
      self.click(pos);
      self.frame(vec![Self::button(pos, true)]);
      self.frame(vec![Self::button(pos, false)]);
      self.idle(2);
    }

    fn drag(&mut self, a: Pos2, b: Pos2) {
      self.frame(vec![Event::PointerMoved(a)]);
      self.frame(vec![Self::button(a, true)]);
      for i in 1..=8 {
        self.frame(vec![Event::PointerMoved(a + (b - a) * i as f32 / 8.0)]);
      }
      self.frame(vec![Self::button(b, false)]);
      self.idle(2);
    }

    fn key(&mut self, key: egui::Key) {
      let ev = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() };
      self.frame(vec![ev(true), ev(false)]);
      self.idle(2);
    }

    fn type_text(&mut self, text: &str) {
      self.frame(vec![Event::Text(text.into())]);
    }

    /// The middle of a row's name, found from its expander or eye (laid out last frame).
    fn name_of(&self, row: egui::Id, part: &str, dx: f32) -> Pos2 {
      let r = self.ctx.read_response(row.with(part)).unwrap_or_else(|| panic!("row {row:?} is drawn"));
      r.rect.center() + vec2(dx, 0.0)
    }

    fn body_name(&self, b: BodyId) -> Pos2 {
      self.name_of(egui::Id::new("tree-bodies").with(b.0), "eye", 45.0)
    }

    fn editing(&self) -> bool {
      self.ctx.data(|d| d.get_temp::<super::Editing>(super::editing_id())).is_some()
    }
  }

  #[test]
  fn double_clicking_a_body_renames_it_in_place() {
    let mut h = Harness::new(1);
    let b = h.app.s.model.bodies[0].id;
    let at = h.body_name(b);
    h.double_click(at);
    assert!(h.editing(), "a rename box opens on the row");
    h.type_text("Bracket");
    h.key(egui::Key::Enter);
    assert!(!h.editing());
    assert_eq!(h.app.s.body_name(b), "Bracket", "Enter commits");
  }

  #[test]
  fn escape_cancels_the_rename() {
    let mut h = Harness::new(1);
    let b = h.app.s.model.bodies[0].id;
    let before = h.app.s.body_name(b);
    h.double_click(h.body_name(b));
    h.type_text("Nope");
    h.key(egui::Key::Escape);
    assert!(!h.editing());
    assert_eq!(h.app.s.body_name(b), before, "Escape keeps the old name");
  }

  #[test]
  fn a_slow_click_on_a_selected_body_renames_it() {
    let mut h = Harness::new(1);
    let b = h.app.s.model.bodies[0].id;
    let at = h.body_name(b);
    h.click(at);
    assert!(!h.editing(), "the first click selects");
    h.idle(40);
    h.click(at);
    assert!(!h.editing(), "it waits for a possible double click");
    h.idle(40);
    assert!(h.editing(), "then the rename box opens");
  }

  #[test]
  fn dragging_a_body_onto_a_component_moves_it() {
    let mut h = Harness::new(2);
    let bodies: Vec<BodyId> = h.app.s.model.bodies.iter().map(|b| b.id).collect();
    let c = h.app.run("component.create", json!({"bodies": [bodies[0].0]})).unwrap()["component"].as_u64().unwrap();
    h.idle(3);
    let target = h.name_of(egui::Id::new("tree-components").with(c as u32), "tri", 60.0);
    h.drag(h.body_name(bodies[1]), target);
    let comp = h.app.s.design.component(ComponentId(c as u32)).unwrap();
    assert!(comp.bodies.contains(&bodies[1]), "the dropped body joined the component: {:?}", comp.bodies);
  }
}
