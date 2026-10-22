//! Failed and suppressed timeline items, as Fusion shows them: the error badge and the Error
//! window, the items' right-click menus, the transient lower-left message and the timeline items
//! a single hole adds.

use egui::{Event, Pos2, RawInput, Rect, vec2};
use fox_engine::Session;
use fox_engine::model::FeatureId;
use serde_json::{Value, json};

use crate::panels::{Hidden, TimelineEntry};
use crate::{App, error_dialog, file_dialogs, panels};

/// A 20 mm box with a 50 mm chamfer on one edge: the chamfer fails.
fn failed_chamfer() -> (App, FeatureId) {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  let b = &app.s.model.bodies[0];
  let e = b.edges()[0].id;
  let r = app.s.execute("chamfer", &json!({"edges": [{"body": b.id.0, "edge": e}], "distance": 50})).unwrap();
  let id = FeatureId(r["feature"].as_u64().or_else(|| r["id"].as_u64()).unwrap() as u32);
  assert!(app.s.model.status(id).is_some_and(|s| s.error.is_some()), "{r}");
  (app, id)
}

fn labels(app: &App, id: FeatureId, suppressed: bool) -> Vec<&'static str> {
  panels::timeline_entries(app, id, suppressed).into_iter().map(|e| e.map_or("--", |e| e.label(suppressed))).collect()
}

#[test]
fn a_failed_items_menu_is_fusions() {
  let (app, id) = failed_chamfer();
  assert_eq!(
    labels(&app, id, false),
    vec![
      "Create Selection Set",
      "--",
      "Edit Feature",
      "Configure",
      "--",
      "Delete",
      "Rename",
      "--",
      "Roll Timeline Marker Here",
      "Convert to DM Feature",
      "Suppress Features",
      "Review warning",
      "--",
      "Find in Browser",
      "Find in Window",
    ]
  );
  // the box computed: no Review warning
  let bx = app.s.design.features[0].id;
  assert!(!panels::timeline_entries(&app, bx, false).contains(&Some(TimelineEntry::ReviewWarning)));
  let list = error_dialog::failures(&app);
  assert_eq!(list.len(), 1);
  assert_eq!((list[0].feature, list[0].name.as_str(), list[0].error), (id, "Chamfer1", true));
  assert_eq!(error_dialog::title(&list), "Error");
  assert!(
    error_dialog::report(&list).starts_with("Chamfer1\n  Compute Failed\n    "),
    "{}",
    error_dialog::report(&list)
  );
}

#[test]
fn a_suppressed_items_menu_has_no_edit_or_review_and_clears_the_badge() {
  let (mut app, id) = failed_chamfer();
  app.run("feature.suppress", json!({"feature": id.0}));
  let l = labels(&app, id, true);
  assert!(!l.contains(&"Edit Feature") && !l.contains(&"Review warning"), "{l:?}");
  assert!(l.contains(&"Unsuppress Features"), "{l:?}");
  assert!(error_dialog::failures(&app).is_empty(), "nothing failed is computed: no badge");
  // unsuppressed, the error is back
  app.run("feature.suppress", json!({"feature": id.0}));
  assert_eq!(error_dialog::failures(&app).len(), 1);
}

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
}

impl Harness {
  fn new(app: App) -> Harness {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut h = Harness { ctx, app, time: 0.0 };
    for _ in 0..3 {
      h.frame(1.0 / 60.0, vec![]);
    }
    h
  }

  fn frame(&mut self, dt: f64, events: Vec<Event>) {
    self.time += dt;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    self.ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
  }

  fn click(&mut self, pos: Pos2) {
    let button = |pressed| Event::PointerButton {
      pos,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    self.frame(1.0 / 60.0, vec![Event::PointerMoved(pos)]);
    self.frame(1.0 / 60.0, vec![button(true)]);
    self.frame(1.0 / 60.0, vec![button(false)]);
    self.frame(1.0 / 60.0, vec![]);
  }
}

#[test]
fn the_red_badge_opens_the_error_window() {
  let (app, _) = failed_chamfer();
  let mut h = Harness::new(app);
  let badge = h.ctx.memory(|m| m.area_rect(egui::Id::new("error-badge"))).expect("the badge shows");
  assert!(!error_dialog::is_open(&h.ctx));
  h.click(badge.center());
  assert!(error_dialog::is_open(&h.ctx));
  h.frame(1.0 / 60.0, vec![]);
  assert!(h.ctx.memory(|m| m.area_rect(file_dialogs::floating_id(error_dialog::NAME))).is_some());
}

#[test]
fn lower_left_messages_are_transient() {
  assert!(!panels::status_expired("x", false, None, 1.0));
  assert!(panels::status_expired("x", false, None, panels::STATUS_SECS + 0.5));
  assert!(!panels::status_expired("x", true, None, 60.0), "an error stays until the next command succeeds");
  assert!(!panels::status_expired("x", false, Some("x"), 60.0), "a running tool's prompt stays");
  let mut h = Harness::new(App::with_session(Session::default()));
  h.app.status = Some(("This feature has no editable properties yet".into(), false));
  h.frame(1.0, vec![]);
  assert!(h.app.status.is_some());
  for _ in 0..8 {
    h.frame(1.0, vec![]);
  }
  assert_eq!(h.app.status, None);
  // the next command's start replaces it at once
  h.app.status = Some(("This feature has no editable properties yet".into(), false));
  h.app.start(crate::dialogs::Kind::Box);
  assert_ne!(h.app.status, Some(("This feature has no editable properties yet".into(), false)));
}

#[test]
fn a_single_hole_adds_only_its_hole_to_the_timeline() {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 60, "depth": 40, "height": 10})).unwrap();
  let b = &app.s.model.bodies[0];
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let face = json!({"body": b.id.0, "face": top});
  app.s.execute("hole", &json!({"face": face, "at": [20, 20, 10], "diameter": 4, "depth": 5})).unwrap();
  let n = app.s.design.features.len();
  assert_eq!(n, 3, "the engine keeps the hole's placement sketch");
  let hidden: Hidden = panels::hidden_items(&app, app.s.design.active_len());
  assert_eq!(hidden.shown(n), 2, "Box and Hole1");
  let hole = hidden.item(1);
  assert_eq!(app.s.design.features[hole].name, "Hole1");
  // Delete takes the sketch with it
  let id = app.s.design.features[hole].id;
  let r: Value = app.run("feature.delete", json!({"feature": id.0})).unwrap();
  assert_eq!(app.s.design.features.len(), 1, "{r}");
}
