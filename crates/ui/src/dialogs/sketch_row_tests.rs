//! A sketch row's Redefine Sketch Plane and Export DXF, driven from the browser menu.

use fox_engine::model::{FeatureId, LengthUnit};
use fox_engine::{Sel, Session};
use serde_json::json;

use super::{Dialog, Kind, redefine};
use crate::App;
use crate::panels::{RowCmd, run_sketch_cmd};

/// A box, then a sketch on XZ with a circle, then an extrusion of it.
fn design() -> (App, FeatureId, fox_engine::model::BodyId, u32) {
  let mut app = App::with_session(Session::default());
  app.run("box", json!({"width": 30, "depth": 20, "height": 10}));
  let b = app.s.model.bodies[0].id;
  let top = app.s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  app.create_sketch(json!("xz"));
  let sk = app.s.edit_sketch.unwrap();
  app.run("sketch.circle", json!({"center": [5, 5], "radius": 2}));
  app.finish_sketch();
  app.run("extrude", json!({"sketch": sk.0, "distance": 3}));
  assert_eq!(app.s.model.bodies.len(), 2);
  (app, sk, b, top)
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

fn plane_z(app: &App, sk: FeatureId) -> f64 {
  app.s.model.sketches[&sk].plane.origin.z
}

#[test]
fn redefine_sketch_plane_rolls_back_previews_on_the_pick_and_cancel_restores() {
  let (mut app, sk, b, top) = design();
  let all = app.s.design.features.len();
  let normal_y = |app: &App| app.s.model.sketches[&sk].plane.normal().y.abs() > 0.9;
  assert!(normal_y(&app));
  run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::RedefinePlane);
  let mut d = app.dialog.take().expect("the dialog opens");
  assert_eq!((d.kind, d.kind.title()), (Kind::RedefinePlane, "REDEFINE SKETCH PLANE"));
  // the timeline is rolled back to the sketch: the extrusion leaves the canvas
  assert_eq!(app.s.design.active_len(), app.s.design.index_of(sk).unwrap() + 1);
  assert_eq!(app.s.model.bodies.len(), 1);
  // OK waits for a pick, and the cursor prompt asks for one
  assert!(show_once(&mut app, &mut d));
  assert!(!d.ok_enabled());
  assert_eq!(d.prompt(false).as_deref(), Some(redefine::HINT));
  assert_eq!(redefine::HINT, "Select a planar face or a work plane");

  // picking the box's top face: OK lights and the sketch shows on the face
  app.s.selection = vec![Sel::Face { body: b, face: top }];
  assert!(show_once(&mut app, &mut d));
  assert!(d.ok_enabled() && d.applied && d.error.is_none(), "{:?}", d.error);
  assert!((plane_z(&app, sk) - 10.0).abs() < 1e-6);
  assert_eq!(app.s.selection, vec![Sel::Face { body: b, face: top }], "the face stays selected");

  // Cancel: the sketch, its plane and the timeline are as they were
  d.cancel(&mut app.s);
  assert_eq!(app.s.design.active_len(), all);
  assert!(normal_y(&app) && app.s.model.bodies.len() == 2);
  assert!(!app.s.can_redo());
}

#[test]
fn redefine_sketch_plane_ok_moves_the_sketch_in_one_undo_step() {
  let (mut app, sk, b, top) = design();
  let all = app.s.design.features.len();
  run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::RedefinePlane);
  let mut d = app.dialog.take().unwrap();
  app.s.selection = vec![Sel::Face { body: b, face: top }];
  assert!(show_once(&mut app, &mut d));
  // the chip's x clears the pick (and OK greys again); a new pick takes it back
  d.redefine.picked = None;
  app.s.selection.clear();
  d.dirty = true;
  assert!(show_once(&mut app, &mut d));
  assert!(!d.ok_enabled() && !d.applied);
  app.s.selection = vec![Sel::Face { body: b, face: top }];
  assert!(show_once(&mut app, &mut d));
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d), "OK closes the dialog");
  d.leave(&mut app);
  // the whole timeline again, the sketch on the face and the extrusion on top of it
  assert_eq!(app.s.design.active_len(), all);
  assert!((plane_z(&app, sk) - 10.0).abs() < 1e-6);
  assert_eq!(app.s.model.bodies.len(), 2);
  assert!(app.s.model.errors().is_empty(), "{:?}", app.s.model.errors());
  assert!(!app.s.can_redo());
  // one undo puts the sketch back on XZ
  app.s.undo();
  assert!(app.s.model.sketches[&sk].plane.normal().y.abs() > 0.9);
  assert_eq!(app.s.design.active_len(), all);
}

#[test]
fn export_dxf_opens_its_dialog_then_fusions_save_window() {
  let (mut app, sk, _, _) = design();
  app.s.design.units = LengthUnit::In;
  run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::SaveAsDxf);
  let mut d = app.dialog.take().expect("a docked dialog, not the system's file dialog");
  assert_eq!((d.kind, d.kind.title()), (Kind::ExportDxf, "EXPORT DXF"));
  // the design's unit, and every kind of geometry on
  assert_eq!(d.dxf.units, "in");
  assert!(d.dxf.points && d.dxf.projected && d.dxf.construction);
  assert!(show_once(&mut app, &mut d));
  assert!(d.ok_enabled());
  assert!(app.export_dxf.is_none(), "nothing asks where to save before OK");
  d.dxf.construction = false;
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d), "OK closes the dialog");
  let mut w = app.export_dxf.clone().expect("OK opens the Export DXF window");
  assert_eq!(w.name, "Sketch1");
  assert!(w.location == crate::file_dialogs::downloads() && w.local);
  assert_eq!((w.units, w.construction), ("in", false));

  let dir = std::env::temp_dir().join(format!("fox-ui-export-dxf-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  w.location = dir.display().to_string();
  let (path, p) = w.export();
  assert_eq!(path, dir.join("Sketch1.dxf"));
  assert_eq!(p["construction"], json!(false));
  assert!(w.save(&mut app));
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("$INSUNITS\n 70\n1\n"), "an inch file");
  let _ = std::fs::remove_dir_all(dir);
}
