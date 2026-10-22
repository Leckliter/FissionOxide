//! Batch ui-leftovers-dialogs: Flip as Fusion's icon toggle, Revolve waiting for a profile, the
//! edit dialogs' live selection chips (Revolve, Shell, Offset Plane, Mirror, the patterns), REMOVE
//! taking components, Pipe's arrows and the primitives' drawing step.

use egui::{Pos2, RawInput, Rect, vec2};
use fox_engine::model::{AxisRef, BodySel, FeatureId, FeatureKind, OriginPlane, PlaneRef};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::primitive::{Base, Hud, hud_point, typed_mm};
use super::*;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
}

/// One frame of the whole app: the texts drawn, with where each starts.
fn drawn(ctx: &egui::Context, app: &mut App, time: &mut f64) -> Vec<(String, Pos2)> {
  fn walk(s: &egui::Shape, out: &mut Vec<(String, Pos2)>) {
    match s {
      egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos)),
      egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
      _ => {}
    }
  }
  *time += 1.0 / 60.0;
  let input = RawInput {
    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
    time: Some(*time),
    ..Default::default()
  };
  let out = ctx.run_ui(input, |ui| app.draw(ui));
  let mut all = Vec::new();
  for c in &out.shapes {
    walk(&c.shape, &mut all);
  }
  out.drop_without_applying_deltas();
  all
}

fn dialog_text(app: &mut App) -> Vec<(String, Pos2)> {
  let ctx = egui::Context::default();
  let mut time = 0.0;
  drawn(&ctx, app, &mut time);
  drawn(&ctx, app, &mut time)
}

fn x_of(text: &[(String, Pos2)], label: &str) -> f32 {
  text.iter().find(|(t, _)| t == label).unwrap_or_else(|| panic!("{label} in {text:?}")).1.x
}

/// Joint's Flip is Fusion's icon toggle beside a "Flip" row label (in the labels' column), not
/// a checkbox in the values' column.
#[test]
fn joint_flips_with_an_icon_toggle() {
  let mut app = App::with_session(Session::default());
  let d = Dialog::open(&mut app, Kind::Joint);
  app.dialog = Some(d);
  let t = dialog_text(&mut app);
  assert!((x_of(&t, "Flip") - x_of(&t, "Angle")).abs() < 0.5, "Flip is a row label: {t:?}");
}

/// The flip toggle: a click on the icon flips, and marks the dialog changed.
#[test]
fn the_flip_toggle_flips_on_a_click() {
  let ctx = egui::Context::default();
  let raw =
    || RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))), ..Default::default() };
  let (mut flip, mut dirty) = (false, false);
  let draw = |ctx: &egui::Context, input: RawInput, flip: &mut bool, dirty: &mut bool| {
    ctx
      .run_ui(input, |ui| {
        ui.horizontal(|ui| super::icon_row::flip_toggle(ui, "Flip", "Flip", flip, dirty));
      })
      .drop_without_applying_deltas();
  };
  draw(&ctx, raw(), &mut flip, &mut dirty);
  assert!(!flip && !dirty);
  // the icon follows the label in the row
  let mut probe = Pos2::ZERO;
  ctx
    .run_ui(raw(), |ui| {
      ui.horizontal(|ui| {
        ui.label("Flip");
        probe = ui.cursor().min;
      });
    })
    .drop_without_applying_deltas();
  // (a click hits what the frame before laid out)
  draw(&ctx, raw(), &mut flip, &mut dirty);
  let click = probe + vec2(12.0, 12.0);
  let mut input = raw();
  input.events.push(egui::Event::PointerMoved(click));
  for pressed in [true, false] {
    input.events.push(egui::Event::PointerButton {
      pos: click,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    });
  }
  draw(&ctx, input, &mut flip, &mut dirty);
  assert!(flip && dirty);
}

/// A sketch on XZ with two separate squares beside the Z axis: (session, sketch).
fn two_squares() -> (Session, FeatureId) {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [9, 4]}));
  run(&mut s, "sketch.rect", json!({"corner1": [15, 0], "corner2": [19, 4]}));
  let sk = s.edit_sketch.unwrap();
  run(&mut s, "sketch.finish", json!({}));
  (s, sk)
}

/// Fusion's Revolve opens with Profile: Select and previews nothing until a profile is picked,
/// even with the axis in.
#[test]
fn revolve_waits_for_a_profile() {
  let (s, sk) = two_squares();
  let mut app = App::with_session(s);
  let mut d = Dialog::open(&mut app, Kind::Revolve);
  assert_eq!(d.sketch, Some(sk));
  assert!(d.profiles.is_empty());
  assert_eq!(d.prompt(false).as_deref(), Some("Select sketch profiles or planar faces to revolve"));
  d.axis = "z".into();
  assert!(!d.ready() && !d.ok_enabled(), "no profile: nothing revolves");
  d.preview(&mut app.s);
  assert!(app.s.model.bodies.is_empty(), "no preview of every region");
  app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
  d.sync_selection(&app.s);
  assert_eq!(d.profiles.len(), 1);
  assert!(d.ready());
  d.preview(&mut app.s);
  assert_eq!(app.s.model.bodies.len(), 1);
  d.cancel(&mut app.s);
}

/// Edit Revolve: its profile is picked again (selected in the sketch) and its axis shows; a
/// new profile pick revolves that one.
#[test]
fn an_edited_revolve_picks_its_profile_again() {
  let (mut s, sk) = two_squares();
  let regions = s.model.sketches[&sk].profiles.clone();
  let first = regions.iter().position(|r| r.contains([7.0, 2.0])).unwrap();
  let other = 1 - first;
  run(&mut s, "revolve", json!({"sketch": sk.0, "profiles": [[7, 2]], "axis": "z", "angle": "360 deg"}));
  let id = s.design.features.last().unwrap().id;
  let mut app = App::with_session(s);
  crate::panels::edit_feature(&mut app, id);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks && !d.picks_fixed());
  assert_eq!(d.profiles.len(), 1);
  assert_eq!(app.s.selection, vec![Sel::Profile { sketch: sk, index: first }]);
  assert_eq!(d.axis, "z");
  assert!(d.ready());
  app.s.selection = vec![Sel::Profile { sketch: sk, index: other }];
  d.sync_selection(&app.s);
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?} {:?}", d.error, d.command());
  let FeatureKind::Revolve { profiles, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
  assert!(regions[other].contains(profiles[0]), "{profiles:?}");
  d.cancel(&mut app.s);
}

/// A 20 mm cube.
fn cube() -> (Session, u32) {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  let b = s.model.bodies[0].id.0;
  (s, b)
}

fn face_where(s: &Session, b: u32, f: impl Fn(&fox_engine::kernel::FaceInfo) -> bool) -> u32 {
  s.model.body(fox_engine::model::BodyId(b)).unwrap().faces().iter().find(|x| f(x)).unwrap().id
}

/// Edit Shell: its open face is picked again; another face picked opens that one instead.
#[test]
fn an_edited_shell_picks_its_faces_again() {
  let (mut s, b) = cube();
  let top = face_where(&s, b, |f| f.normal.z > 0.9);
  run(&mut s, "shell", json!({"faces": [{"body": b, "face": top}], "thickness": "1 mm"}));
  let id = s.design.features.last().unwrap().id;
  let mut app = App::with_session(s);
  crate::panels::edit_feature(&mut app, id);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks);
  d.withdraw(&mut app.s);
  assert_eq!(d.faces, vec![json!({"body": b, "face": top})]);
  assert!(app.s.selection.contains(&Sel::Face { body: fox_engine::model::BodyId(b), face: top }));
  let bottom = face_where(&app.s, b, |f| f.normal.z < -0.9);
  app.s.selection = vec![Sel::Face { body: fox_engine::model::BodyId(b), face: bottom }];
  d.sync_selection(&app.s);
  assert_eq!(d.faces, vec![json!({"body": b, "face": bottom})]);
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let FeatureKind::Shell { faces, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
  assert!(faces[0].point[2].abs() < 1e-6, "the bottom face now: {:?}", faces[0].point);
  d.cancel(&mut app.s);
}

/// Edit Offset Plane: the base plane's chip is live, and a new plane moves the offset onto it.
#[test]
fn an_edited_offset_plane_takes_a_new_base() {
  let mut s = Session::default();
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
  let id = s.design.features.last().unwrap().id;
  let mut app = App::with_session(s);
  crate::panels::edit_feature(&mut app, id);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks && d.takes_plane());
  assert_eq!(d.plane, json!("xy"));
  d.plane = json!("yz");
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let FeatureKind::ConstructionPlane { base, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
  assert_eq!(*base, PlaneRef::Origin { plane: OriginPlane::Yz });
  d.cancel(&mut app.s);
}

/// Edit Mirror: Objects and Mirror Plane are live chips filled with the stored picks.
#[test]
fn an_edited_mirror_has_live_objects_and_plane() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [20, 0]}));
  let b = s.model.bodies[0].id.0;
  run(&mut s, "mirror", json!({"plane": "yz", "bodies": [b], "join": false}));
  let id = s.design.features.last().unwrap().id;
  let mut app = App::with_session(s);
  crate::panels::edit_feature(&mut app, id);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks && d.wants_plane());
  assert_eq!((d.bodies.clone(), d.plane.clone()), (vec![b], json!("yz")));
  d.take_plane(json!("xz"));
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let FeatureKind::Mirror { plane, bodies, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
  assert_eq!(*plane, PlaneRef::Origin { plane: OriginPlane::Xz });
  assert_eq!(*bodies, BodySel::Bodies { ids: vec![fox_engine::model::BodyId(b)] });
  d.cancel(&mut app.s);
}

/// Edit Circular Pattern: Objects and Axis are live, the Distribution loaded; another axis
/// patterns about it. Edit Rectangular Pattern: its Objects are live.
#[test]
fn edited_patterns_have_live_objects() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [20, 0]}));
  run(&mut s, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [-20, 0], "operation": "new_body"}));
  let (a, b) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
  run(&mut s, "pattern.circular", json!({"bodies": [a], "axis": "z", "count": 3, "join": false}));
  let id = s.design.features.last().unwrap().id;
  let mut app = App::with_session(s);
  crate::panels::edit_feature(&mut app, id);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks);
  assert_eq!((d.bodies.clone(), d.axis.as_str(), d.choice), (vec![a], "z", "full"));
  assert_eq!(app.s.selection, vec![Sel::Body { body: fox_engine::model::BodyId(a) }]);
  d.axis = "x".into();
  d.bodies = vec![b];
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let FeatureKind::CircularPattern { axis, bodies, angle, .. } = &app.s.design.feature(id).unwrap().kind else {
    panic!()
  };
  assert_eq!((axis, angle.as_str()), (&AxisRef::X, "360 deg"));
  assert_eq!(*bodies, BodySel::Bodies { ids: vec![fox_engine::model::BodyId(b)] });
  d.cancel(&mut app.s);

  run(&mut app.s, "pattern.rectangular", json!({"bodies": [a], "direction": [0, 1, 0], "count": 2, "spacing": 10}));
  let id = app.s.design.features.last().unwrap().id;
  crate::panels::edit_feature(&mut app, id);
  let mut d = app.dialog.take().unwrap();
  assert!(d.repicks);
  assert_eq!(d.bodies, vec![a]);
  d.bodies = vec![b];
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let FeatureKind::RectangularPattern { bodies, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
  assert_eq!(*bodies, BodySel::Bodies { ids: vec![fox_engine::model::BodyId(b)] });
  d.cancel(&mut app.s);
}

/// REMOVE takes a component: all of its bodies picked (a click on its browser row) remove it,
/// and its row leaves the browser; a body alone is removed as a body.
#[test]
fn remove_takes_a_component() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [30, 0], "operation": "new_body"}));
  let (a, b) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
  let c = run(&mut s, "component.create", json!({"name": "Part", "bodies": [a]}))["component"].as_u64().unwrap() as u32;
  assert_eq!(super::remove_params(&s, &[a]), json!({"bodies": [], "components": [c]}));
  assert_eq!(super::remove_params(&s, &[b]), json!({"bodies": [b]}));
  let mut app = App::with_session(s);
  app.s.selection.clear();
  crate::panels::Cmd::RemoveBodies.run(&mut app);
  let mut d = app.dialog.take().expect("REMOVE opens");
  app.s.selection = vec![Sel::Body { body: fox_engine::model::BodyId(a) }];
  d.sync_selection(&app.s);
  let (cmd, p) = d.command().unwrap();
  assert_eq!((cmd, p["components"].clone()), ("body.remove", json!([c])));
  assert_eq!(crate::panels::component_rows(&app), vec![(c, 0)]);
  app.run(cmd, p);
  assert_eq!(app.s.model.bodies.len(), 1);
  assert!(crate::panels::component_rows(&app).is_empty(), "the component's row is gone");
  // undone, it is back
  app.s.undo();
  assert_eq!(crate::panels::component_rows(&app), vec![(c, 0)]);
}

/// Pipe's arrows: one at the path's end for the Distance, one out from the start for the
/// Section Size; dragging them sets the rows.
#[test]
fn pipe_has_its_arrows() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [40, 0]]}));
  let path = s.edit_sketch.unwrap();
  run(&mut s, "sketch.finish", json!({}));
  let f = &crate::forms::PIPE;
  let mut d = Dialog::blank(Kind::Form(f));
  let key = |k: &str| f.inputs.iter().position(|i| i.key() == k).unwrap();
  d.form.values[key("path")] = json!(path.0);
  d.form.values[key("diameter")] = json!("6 mm");
  let h = d.handles(&s);
  assert_eq!(h.len(), 2, "the Distance and Section Size arrows");
  assert!((h[0].value - 40.0).abs() < 1e-9, "at the path's end: {}", h[0].value);
  assert!((h[1].value - 3.0).abs() < 1e-9);
  d.set_handle(&s, 0, 20.0, 0.1);
  assert_eq!(d.form.values[key("distance")], json!("0.50"));
  d.set_handle(&s, 1, 5.0, 0.1);
  assert_eq!(d.form.values[key("diameter")], json!("10 mm"));
}

/// The drawing step: a temporary sketch is listed while the base is drawn and the dialog is
/// open; typed sizes place the base; the preview body is drawn as the grey ghost.
#[test]
fn primitive_drawing_step_extras() {
  let mut app = App::with_session(Session::default());
  app.start(Kind::Box);
  assert!(crate::dialogs::temp_sketch_name(&app).is_none(), "no sketch until the plane is picked");
  let d = app.dialog.as_mut().unwrap();
  d.place_on(&app.s, json!("xy"));
  assert_eq!(crate::dialogs::temp_sketch_name(&app).as_deref(), Some("Sketch1"));
  // typed sizes: 30 by 12 from the first corner, towards the pointer
  let unit = app.s.design.units;
  let sizes = [typed_mm(&app.s, unit, "30"), typed_mm(&app.s, unit, "12 mm")];
  assert_eq!(sizes, [Some(30.0), Some(12.0)]);
  assert_eq!(hud_point(Base::Rect, [0.0, 0.0], [-5.0, 3.0], sizes), [-30.0, 12.0]);
  assert_eq!(hud_point(Base::Circle, [0.0, 0.0], [0.0, 9.0], [Some(20.0), None]), [0.0, 10.0]);
  assert_eq!(hud_point(Base::Rect, [0.0, 0.0], [5.0, 3.0], [None, None]), [5.0, 3.0]);
  assert_eq!(Hud::default().active, 0);
  let d = app.dialog.as_mut().unwrap();
  d.place_click([0.0, 0.0]);
  d.place_click([30.0, 12.0]);
  assert!(!d.placing());
  let mut d = app.dialog.take().unwrap();
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  assert_eq!(d.ghost_bodies().len(), 1, "the new body previews grey");
  app.dialog = Some(d);
  assert_eq!(crate::dialogs::temp_sketch_name(&app).as_deref(), Some("Sketch1"), "until OK");
  app.cancel_dialog();
  assert!(crate::dialogs::temp_sketch_name(&app).is_none());
}
