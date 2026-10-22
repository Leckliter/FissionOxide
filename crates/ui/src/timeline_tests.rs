//! The timeline's rollback, playback, tooltips and drags, as Fusion's: rolled-back features leave
//! the browser, the marker's drag rolls the model as it goes, Play replays from the beginning,
//! the gear's menu, the dark item tooltip, a suppressed healthy feature's icon and the feedback of
//! a feature's drag.

use egui::{Color32, Event, Pos2, RawInput, Rect, pos2, vec2};
use fox_engine::Session;
use fox_engine::model::FeatureId;
use serde_json::{Value, json};

use crate::{App, panels, theme};

struct Harness {
  ctx: egui::Context,
  app: App,
  time: f64,
}

/// What a frame drew: its texts and every other shape, groups flattened.
struct Drawn {
  texts: Vec<(Rect, String)>,
  shapes: Vec<egui::Shape>,
}

impl Drawn {
  fn has(&self, text: &str) -> bool {
    self.texts.iter().any(|(_, t)| t == text)
  }

  /// The rects filled with `fill`.
  fn rects(&self, fill: Color32) -> Vec<Rect> {
    self
      .shapes
      .iter()
      .filter_map(|s| match s {
        egui::Shape::Rect(r) if r.fill == fill => Some(r.rect),
        _ => None,
      })
      .collect()
  }
}

impl Harness {
  fn new(app: App) -> Harness {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut h = Harness { ctx, app, time: 0.0 };
    for _ in 0..3 {
      h.frame(1.0 / 60.0, vec![]);
    }
    h
  }

  fn frame(&mut self, dt: f64, events: Vec<Event>) -> Drawn {
    fn walk(s: egui::Shape, d: &mut Drawn) {
      match s {
        egui::Shape::Vec(v) => v.into_iter().for_each(|s| walk(s, d)),
        egui::Shape::Text(t) => d.texts.push((t.visual_bounding_rect(), t.galley.text().to_string())),
        s => d.shapes.push(s),
      }
    }
    self.time += dt;
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(self.time),
      events,
      ..Default::default()
    };
    let app = &mut self.app;
    let mut out = self.ctx.run_ui(input, |ui| app.draw(ui));
    out.textures_delta.clear();
    let mut d = Drawn { texts: Vec::new(), shapes: Vec::new() };
    for c in out.shapes {
      walk(c.shape, &mut d);
    }
    d
  }

  fn settle(&mut self) -> Drawn {
    for _ in 0..3 {
      self.frame(1.0 / 60.0, vec![]);
    }
    self.frame(1.0 / 60.0, vec![])
  }

  fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
  }

  fn click(&mut self, pos: Pos2) {
    self.frame(1.0 / 60.0, vec![Event::PointerMoved(pos)]);
    self.frame(1.0 / 60.0, vec![Self::button(pos, true)]);
    self.frame(1.0 / 60.0, vec![Self::button(pos, false)]);
    self.frame(1.0 / 60.0, vec![]);
  }

  /// Presses at `a` and moves to `b` over a few frames, the button still held.
  fn drag_to(&mut self, a: Pos2, b: Pos2) -> Drawn {
    self.frame(1.0 / 60.0, vec![Event::PointerMoved(a)]);
    self.frame(1.0 / 60.0, vec![Self::button(a, true)]);
    for i in 1..=6 {
      self.frame(1.0 / 60.0, vec![Event::PointerMoved(a + (b - a) * i as f32 / 6.0)]);
    }
    self.frame(1.0 / 60.0, vec![]);
    self.frame(1.0 / 60.0, vec![])
  }

  fn release(&mut self, at: Pos2) -> Drawn {
    self.frame(1.0 / 60.0, vec![Self::button(at, false)]);
    self.settle()
  }

  /// The timeline marker's grab handle: Fusion's five-sided dark tab at the timeline's foot.
  fn marker(&mut self) -> Pos2 {
    let d = self.frame(1.0 / 60.0, vec![]);
    d.shapes
      .iter()
      .find_map(|s| match s {
        egui::Shape::Path(p) if p.points.len() == 5 && p.fill == Color32::from_gray(55) && p.points[0].y > 800.0 => {
          Some(pos2((p.points[0].x + p.points[1].x) / 2.0, p.points[0].y + 4.0))
        }
        _ => None,
      })
      .expect("the marker is drawn")
  }
}

/// Timeline icons are 26 px wide with 1 px between them; the marker is 12 px wide.
const ITEM: f32 = 27.0;

/// The centre of timeline item `k` of `n` when the marker (handle at `m`) is at the end.
fn item(m: Pos2, n: usize, k: usize) -> Pos2 {
  pos2(m.x - 7.0 - (n - 1 - k) as f32 * ITEM - 13.0, m.y + 12.0)
}

fn feature(r: &Value) -> FeatureId {
  FeatureId(r["feature"].as_u64().or_else(|| r["sketch"].as_u64()).expect("a feature id") as u32)
}

/// Sketch1 (a rectangle on XY), Extrude1, Sketch2 on XZ, Plane1 offset from XY and Sketch3 on it.
fn rolled_design() -> App {
  let mut app = App::with_session(Session::default());
  let s = &mut app.s;
  let r = s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
  let sk = feature(&r);
  s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [20, 10]})).unwrap();
  s.execute("sketch.finish", &json!({})).unwrap();
  s.execute("extrude", &json!({"sketch": sk.0, "distance": 5})).unwrap();
  s.execute("sketch.create", &json!({"plane": "xz"})).unwrap();
  s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [5, 5]})).unwrap();
  s.execute("sketch.finish", &json!({})).unwrap();
  let r = s.execute("plane.offset", &json!({"plane": "xy", "offset": 30})).unwrap();
  let plane = feature(&r);
  s.execute("sketch.create", &json!({"plane": {"feature": plane.0}})).unwrap();
  s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [4, 4]})).unwrap();
  s.execute("sketch.finish", &json!({})).unwrap();
  let names: Vec<_> = s.design.features.iter().map(|f| f.name.as_str()).collect();
  assert_eq!(names, ["Sketch1", "Extrude1", "Sketch2", "Plane1", "Sketch3"]);
  app
}

/// ui:timeline:rollback-browser-rows: rolling the marker back to just after Extrude1 takes
/// Sketch2, Sketch3 and Plane1 (and the Construction folder) out of the browser; rolling
/// forward brings them back.
#[test]
fn rolled_back_features_leave_the_browser() {
  let mut h = Harness::new(rolled_design());
  h.app.browser_sketches = Some(true);
  // the Construction folder starts collapsed: open it
  h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("tree-construction").with("open"), true));
  let d = h.settle();
  for t in ["Sketch1", "Sketch2", "Sketch3", "Construction", "Plane1"] {
    assert!(d.has(t), "{t} is listed");
  }
  h.app.run("timeline.marker", json!({"at": 2}));
  let d = h.settle();
  assert!(d.has("Sketch1") && d.has("Sketches"));
  for t in ["Sketch2", "Sketch3", "Construction", "Plane1"] {
    assert!(!d.has(t), "{t} is rolled back");
  }
  h.app.run("timeline.marker", json!({"at": null}));
  let d = h.settle();
  for t in ["Sketch2", "Sketch3", "Construction", "Plane1"] {
    assert!(d.has(t), "{t} is back");
  }
}

/// ui:timeline:marker-drag-live and ui:timeline:item-tooltip: the dragged marker rolls the
/// model at each slot (before the button is released), a dark tooltip above it names the
/// feature left of it and its component; the drag is one undo step; resting on the marker
/// shows no tooltip; resting on an item shows the dark tooltip with its name and component.
#[test]
fn the_marker_drag_rolls_the_model_live() {
  let mut h = Harness::new(rolled_design());
  let m = h.marker();
  // resting on the marker shows nothing
  h.frame(1.0 / 60.0, vec![Event::PointerMoved(m + vec2(0.0, 8.0))]);
  let mut d = h.frame(1.0, vec![]);
  for _ in 0..5 {
    d = h.frame(0.2, vec![]);
  }
  assert!(!d.texts.iter().any(|(_, t)| t.contains("roll the timeline")), "no marker tooltip");
  // resting on Extrude1: the dark tooltip with its name and component
  let e = item(m, 5, 1);
  h.frame(1.0 / 60.0, vec![Event::PointerMoved(e)]);
  for _ in 0..10 {
    d = h.frame(0.1, vec![]);
  }
  assert!(d.has("Extrude1") && d.has("(Unsaved)"), "{:?}", d.texts);
  assert!(!d.rects(theme::TOOLTIP).is_empty(), "a dark panel");
  // drag the marker to between Extrude1 and Sketch2
  let to = pos2(e.x + ITEM / 2.0 + 1.0, m.y);
  let d = h.drag_to(m, to);
  assert_eq!(h.app.s.design.active_len(), 2, "the model follows the drag");
  assert!(panels::rolled_back(&h.app, h.app.s.design.features[2].id));
  assert!(d.has("Extrude1") && d.has("(Unsaved)"), "the drag's tooltip: {:?}", d.texts);
  h.release(to);
  assert_eq!(h.app.s.design.active_len(), 2);
  assert!(!h.app.s.in_transaction());
  // one undo step brings the marker back to the end
  assert!(h.app.s.undo());
  assert_eq!(h.app.s.design.active_len(), 5);
}

/// ui:timeline:play-from-start: with the marker after the third item, Play jumps it to the
/// beginning and steps it forward to where it was, then stops; the button is a stop square
/// meanwhile.
#[test]
fn play_replays_from_the_beginning_to_the_marker() {
  let mut h = Harness::new(rolled_design());
  h.app.run("timeline.marker", json!({"at": 3}));
  h.settle();
  panels::timeline_bar::play(&mut h.app, h.time);
  assert_eq!(h.app.s.design.active_len(), 0, "Play starts at the beginning");
  let mut seen = vec![0];
  for _ in 0..20 {
    h.frame(0.25, vec![]);
    let a = h.app.s.design.active_len();
    if seen.last() != Some(&a) {
      seen.push(a);
    }
  }
  assert_eq!(seen, [0, 1, 2, 3], "it steps to where the marker was and stops");
  assert!(h.app.playing.is_none());
  // a stop square shows while playing; clicking it stops
  panels::timeline_bar::play(&mut h.app, h.time);
  let d = h.frame(1.0 / 60.0, vec![]);
  let square = d
    .rects(theme::GROUP_LABEL)
    .into_iter()
    .find(|r| r.width() == 9.0 && r.height() == 9.0 && r.top() > 800.0)
    .expect("the stop square");
  h.click(square.center());
  assert!(h.app.playing.is_none(), "the stop square stops the playback");
}

/// ui:timeline:settings-menu: the gear opens Change to Direct Modeling and the Component Color
/// Swatch (off) and Hide all inactive features (on) checkboxes.
#[test]
fn the_gear_opens_fusions_timeline_settings() {
  let mut h = Harness::new(rolled_design());
  assert_eq!(
    panels::timeline_bar::settings(&h.ctx),
    panels::timeline_bar::Settings { swatch: false, hide_inactive: true }
  );
  h.click(pos2(1440.0 - 8.0 - 12.0, 882.0));
  let d = h.settle();
  for t in ["Change to Direct Modeling", "Component Color Swatch", "Hide all inactive features"] {
    assert!(d.texts.iter().any(|(_, s)| s.trim() == t), "{t}: {:?}", d.texts);
  }
  assert!(!d.has("Roll to end") && !d.has("Roll to beginning"));
  let swatch = d.texts.iter().find(|(_, s)| s == "Component Color Swatch").unwrap().0;
  h.click(swatch.center());
  assert!(panels::timeline_bar::settings(&h.ctx).swatch);
}

/// ui:timeline:suppressed-healthy-icon: a suppressed feature that computes only fades; a
/// suppressed failure keeps the pink square; neither tooltip says "suppressed".
#[test]
fn only_a_suppressed_failure_has_the_pink_square() {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 20, "depth": 20, "height": 20})).unwrap();
  let b = &app.s.model.bodies[0];
  let (body, e) = (b.id.0, b.edges()[0].id);
  let r = app.s.execute("fillet", &json!({"edges": [{"body": body, "edge": e}], "radius": 1})).unwrap();
  let fillet = feature(&r);
  assert!(app.s.model.status(fillet).is_some_and(|s| s.error.is_none()));
  let mut h = Harness::new(app);
  h.app.run("feature.suppress", json!({"feature": fillet.0}));
  let d = h.settle();
  assert!(d.rects(panels::SUPPRESSED_FILL).is_empty(), "a healthy suppressed feature only fades");
  // a failing chamfer, then suppressed
  h.app.run("feature.suppress", json!({"feature": fillet.0, "suppressed": false}));
  let b = &h.app.s.model.bodies[0];
  let (body, e) = (b.id.0, b.edges()[1].id);
  let r = h.app.run("chamfer", json!({"edges": [{"body": body, "edge": e}], "distance": 50})).unwrap();
  let chamfer = feature(&r);
  h.settle();
  h.app.run("feature.suppress", json!({"feature": chamfer.0}));
  let d = h.settle();
  assert_eq!(d.rects(panels::SUPPRESSED_FILL).len(), 1, "the suppressed failure's pink square");
  assert!(!d.has("(suppressed)"));
}

/// ui:timeline:drag-feedback: dragging a feature draws a black insertion bar where it can go,
/// with the tooltip of the feature left of the slot; where it can't there is no bar, the
/// tooltip says why, and the drop leaves nothing at the lower left.
#[test]
fn a_feature_drag_shows_fusions_feedback() {
  let mut app = App::with_session(Session::default());
  for w in [10, 20, 30] {
    app.s.execute("box", &json!({"width": w, "depth": 10, "height": 10})).unwrap();
  }
  let r = app.s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
  let sk = feature(&r);
  app.s.execute("sketch.rect", &json!({"corner1": [50, 0], "corner2": [60, 10]})).unwrap();
  app.s.execute("sketch.finish", &json!({})).unwrap();
  app.s.execute("extrude", &json!({"sketch": sk.0, "distance": 5})).unwrap();
  let names = |a: &App| a.s.design.features.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
  assert_eq!(names(&app), ["Box1", "Box2", "Box3", "Sketch1", "Extrude1"]);
  let mut h = Harness::new(app);
  let m = h.marker();
  // Extrude1 before Sketch1: refused, no bar, the tooltip says why
  let from = item(m, 5, 4);
  let to = pos2(item(m, 5, 3).x - ITEM / 2.0 + 1.0, from.y);
  let d = h.drag_to(from, to);
  assert!(d.rects(Color32::BLACK).iter().all(|r| r.width() != 3.0), "no insertion bar");
  assert!(d.texts.iter().any(|(_, t)| t.starts_with("Can't move there")), "{:?}", d.texts);
  h.release(to);
  assert_eq!(names(&h.app), ["Box1", "Box2", "Box3", "Sketch1", "Extrude1"]);
  assert!(h.app.status.as_ref().is_none_or(|(s, _)| !s.starts_with("Can't move")), "{:?}", h.app.status);
  // Box3 between Box1 and Box2: a black bar and Box1's tooltip, then the move
  let from = item(m, 5, 2);
  let to = pos2(item(m, 5, 1).x - ITEM / 2.0 + 1.0, from.y);
  let d = h.drag_to(from, to);
  assert!(d.rects(Color32::BLACK).iter().any(|r| r.width() == 3.0), "the insertion bar");
  assert!(d.has("Box1") && d.has("(Unsaved)"), "{:?}", d.texts);
  h.release(to);
  assert_eq!(names(&h.app), ["Box1", "Box3", "Box2", "Sketch1", "Extrude1"]);
}
