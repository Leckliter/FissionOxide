//! INSERT MESH, driven without a screen: the dialog the file pick opens, its preview, unit,
//! placement and triad, OK and Cancel.

use fox_engine::kernel::glam::DVec3;
use fox_engine::{Sel, Session};
use serde_json::json;

use super::insert_mesh::{NUMERIC, UNITS};
use super::{Dialog, Kind};
use crate::App;
use crate::manipulators::TriadEdit;

/// The L-prism the MESH tab probe inserted in both apps (30 x 20 x 5 mm from the origin).
fn lprism() -> std::path::PathBuf {
  std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference/464-stl-millimeter/lprism.stl")
}

/// The dialog Insert Mesh opens on the L-prism, taken out of the app to drive.
fn opened(units: Option<&str>) -> (App, Dialog) {
  let mut app = App::with_session(Session::default());
  if let Some(u) = units {
    app.run("design.units", json!({"unit": u}));
  }
  app.import_path(&lprism());
  let d = app.dialog.take().expect("INSERT MESH opens after the file pick");
  (app, d)
}

/// Draws the dialog for one frame; returns whether it stays open.
fn show_once(app: &mut App, d: &mut Dialog) -> bool {
  let ctx = egui::Context::default();
  let raw = egui::RawInput {
    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
    ..Default::default()
  };
  let mut keep = false;
  let mut out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
  out.textures_delta.clear();
  keep
}

/// The previewed mesh's box.
fn bbox(app: &App) -> (DVec3, DVec3) {
  let m = app.s.model.bodies.iter().find(|b| b.is_mesh()).expect("the mesh").shape.mass().unwrap();
  (m.bbox_min, m.bbox_max)
}

fn near(a: DVec3, b: DVec3) -> bool {
  (a - b).length() < 1e-3
}

#[test]
fn the_file_pick_opens_insert_mesh_previewing_the_mesh_selected() {
  let (mut app, mut d) = opened(None);
  assert_eq!((d.kind, d.heading().as_str()), (Kind::InsertMesh, "INSERT MESH"));
  assert_eq!(d.mesh.units, "mm", "the design's unit is ticked first");
  assert!(!d.mesh.flip && !d.mesh.numeric_open, "Numerical Inputs starts closed");
  assert!(d.preview_applied() && d.error.is_none(), "{:?}", d.error);
  // the mesh shows, selected, but isn't in the browser or timeline until OK
  let mesh = app.s.model.bodies[0].id;
  assert_eq!(d.preview_bodies(&app.s), [mesh]);
  assert_eq!(d.preview_features(&app.s).len(), 1);
  assert_eq!(app.s.selection, [Sel::Body { body: mesh }]);
  assert!(near(bbox(&app).1, DVec3::new(30.0, 20.0, 5.0)));
  // the triad sits on the mesh's centre with arrows, planes and rings
  let t = d.triad(&app.s).expect("a move/rotate triad");
  assert!(near(t.origin, DVec3::new(15.0, 10.0, 2.5)), "{:?}", t.origin);
  assert!(t.arrows.iter().chain(&t.rings).chain(&t.planes).all(|x| *x));
  assert_eq!(UNITS.map(|u| u.1), ["Centimeter", "Millimeter", "Meter", "Inch", "Foot"]);
  assert_eq!(NUMERIC, ["X Distance", "Y Distance", "Z Distance", "X Angle", "Y Angle", "Z Angle"]);
  assert_eq!(d.mesh.translate, ["0.00 mm", "0.00 mm", "0.00 mm"]);
  assert_eq!(d.mesh.rotate, ["0.0 deg", "0.0 deg", "0.0 deg"]);
  // the rows draw; OK inserts the mesh as Base Mesh Feature1 and closes
  assert!(show_once(&mut app, &mut d));
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d), "OK closes the dialog");
  assert_eq!(app.s.model.bodies.len(), 1);
  assert_eq!(app.s.body_name(app.s.model.bodies[0].id), "lprism");
  assert_eq!(app.s.design.features.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Base Mesh Feature1"]);
  assert!(app.s.selection.is_empty());
}

#[test]
fn the_design_unit_is_ticked_first_and_the_unit_rescales_the_preview() {
  let (mut app, mut d) = opened(Some("in"));
  assert_eq!(d.mesh.units, "in");
  assert!(near(bbox(&app).1, DVec3::new(30.0, 20.0, 5.0) * 25.4));
  d.insert_mesh_units("mm");
  d.preview(&mut app.s);
  assert!(near(bbox(&app).1, DVec3::new(30.0, 20.0, 5.0)));
  d.insert_mesh_units("cm");
  d.preview(&mut app.s);
  assert!(near(bbox(&app).1, DVec3::new(300.0, 200.0, 50.0)));
  assert_eq!(app.s.model.bodies.len(), 1, "one preview at a time");
}

#[test]
fn cancel_adds_nothing() {
  let (mut app, mut d) = opened(None);
  d.cancel(&mut app.s);
  assert!(app.s.model.bodies.is_empty() && app.s.design.features.is_empty());
  assert!(app.s.selection.is_empty());
  assert!(!app.s.can_undo());
}

#[test]
fn flip_center_ground_and_reset_place_the_mesh() {
  let (mut app, mut d) = opened(None);
  // Flip Up Direction: the file's Y becomes Z
  d.insert_mesh_flip();
  d.preview(&mut app.s);
  let (lo, hi) = bbox(&app);
  assert!(near(hi - lo, DVec3::new(30.0, 5.0, 20.0)), "{:?}", hi - lo);
  // Center: the mesh's centre on the origin; Move To Ground: its bottom on the XY plane
  d.insert_mesh_center(&app.s);
  d.preview(&mut app.s);
  let (lo, hi) = bbox(&app);
  assert!(near((lo + hi) / 2.0, DVec3::ZERO), "{lo:?} {hi:?}");
  d.insert_mesh_ground(&app.s);
  d.preview(&mut app.s);
  let (lo, hi) = bbox(&app);
  assert!(lo.z.abs() < 1e-3 && (hi.z - 20.0).abs() < 1e-3, "{lo:?} {hi:?}");
  assert!(near(DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, 0.0), DVec3::ZERO));
  // Reset: back where the file puts it (still flipped)
  d.insert_mesh_reset();
  d.preview(&mut app.s);
  let (lo, _) = bbox(&app);
  assert!(near(lo, DVec3::new(0.0, -5.0, 0.0)), "{lo:?}");
}

#[test]
fn the_triad_and_numerical_inputs_move_and_turn_the_mesh() {
  let (mut app, mut d) = opened(None);
  // dragging the triad's centre moves the mesh by as much
  d.set_triad(&app.s, TriadEdit::Move(DVec3::new(25.0, 10.0, 2.5)));
  assert_eq!(d.mesh.translate, ["10 mm", "0 mm", "0 mm"]);
  d.preview(&mut app.s);
  assert!(near(bbox(&app).0, DVec3::new(10.0, 0.0, 0.0)));
  // a ring turns it about its centre: a quarter turn about Z swaps its width and depth
  d.set_triad(&app.s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_2));
  assert_eq!(d.mesh.rotate[2], "90 deg");
  d.preview(&mut app.s);
  let (lo, hi) = bbox(&app);
  assert!(near(hi - lo, DVec3::new(20.0, 30.0, 5.0)), "{:?}", hi - lo);
  assert!(near((lo + hi) / 2.0, DVec3::new(25.0, 10.0, 2.5)), "turned about its centre");
  // the triad follows: on the moved centre, its Z ring at the angle
  let t = d.triad(&app.s).unwrap();
  assert!(near(t.origin, DVec3::new(25.0, 10.0, 2.5)) && (t.angles[2] - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
  // the Numerical Inputs boxes take expressions in the design's unit
  d.mesh.translate = ["0 mm".into(), "1 cm".into(), "0 mm".into()];
  d.mesh.rotate = ["0 deg".into(), "0 deg".into(), "0 deg".into()];
  d.preview(&mut app.s);
  assert!(near(bbox(&app).0, DVec3::new(0.0, 10.0, 0.0)));
}
