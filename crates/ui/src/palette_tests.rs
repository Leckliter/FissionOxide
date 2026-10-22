//! The Sketch Palette's Look At, Slice and 3D Sketch, driven without a screen.

use fox_engine::Session;
use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use serde_json::json;

use crate::App;
use crate::viewport::{Tool, grid_shown, sketch_3d_enabled, slice_params};

fn sketching(plane: &str) -> App {
  let mut app = App::with_session(Session::default());
  app.create_sketch(json!(plane));
  assert!(app.s.edit_sketch.is_some());
  app
}

/// Where the camera is headed (a Look At animates), as right / up / back unit vectors.
fn heading(app: &App) -> (DVec3, DVec3, DVec3) {
  app.view_now().basis()
}

fn near(a: DVec3, b: DVec3) -> bool {
  (a - b).length() < 1e-6
}

/// Look At shows the sketch's x axis to the right and its y axis up, seen from the normal's side,
/// on every origin plane: XY from +Z with +X right and +Y up (Fusion's FRONT in a Y-up design),
/// Fusion's XZ frame (x=+X, y=-Z) from +Y and its YZ frame (x=-Z, y=+Y) from +X.
#[test]
fn look_at_puts_the_sketch_x_right_and_y_up() {
  for (name, plane) in [("xy", Plane::XY), ("xz", Plane::FUSION_XZ), ("yz", Plane::FUSION_YZ)] {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!(name));
    let sk = app.s.edit_sketch.unwrap();
    assert_eq!(app.s.model.sketches[&sk].plane, plane, "{name}");
    // the camera is turned on its way to the sketch; Look At again heads the same way
    app.look_at_selection();
    let (right, up, back) = heading(&app);
    assert!(near(right, plane.x), "{name}: right {right:?}, sketch x {:?}", plane.x);
    assert!(near(up, plane.y), "{name}: up {up:?}, sketch y {:?}", plane.y);
    assert!(near(back, plane.normal()), "{name}: eye on {back:?}, normal {:?}", plane.normal());
  }
}

/// On the XY plane from the home view the camera ends straight above the sketch with +X to the
/// right and +Y up the screen, so the grid labels read -50 … 50 left to right and bottom to top.
#[test]
fn look_at_xy_reads_upright_from_home() {
  let mut app = sketching("xy");
  app.look_at_selection();
  let cam = app.view_now();
  let eye = cam.eye();
  assert!(eye.z > 0.0 && eye.x.abs() < 1e-6 && eye.y.abs() < 1e-3, "eye {eye:?}");
  let (right, up, _) = cam.basis();
  assert!(right.x > 0.999, "+X right: {right:?}");
  assert!(up.y > 0.999, "+Y up: {up:?}");
  // the move ends where it was headed
  app.cam_move = None;
  app.cam = cam;
  let (r2, u2, _) = app.cam.basis();
  assert!(near(r2, right) && near(u2, up));
}

/// Slice is a render parameter naming the open sketch and whether the camera is behind its
/// plane, so the scene cuts away what lies between the camera and the plane; off, or outside a
/// sketch, there is none.
#[test]
fn slice_names_the_sketch_and_the_cameras_side() {
  let mut app = sketching("xy");
  assert_eq!(slice_params(&app), None, "a new sketch starts with Slice off");
  app.palette.slice = true;
  let sk = app.s.edit_sketch.unwrap();
  // the home camera (and Look At) sit above the XY plane
  assert_eq!(slice_params(&app), Some(json!({"sketch": sk.0, "flip": false})));
  // from below, the plane is turned so the near side is still cut away
  app.cam.pitch = -0.8;
  assert_eq!(slice_params(&app), Some(json!({"sketch": sk.0, "flip": true})));
  app.finish_sketch();
  assert_eq!(slice_params(&app), None, "no sketch open");
  // a fresh sketch starts with Slice off again
  app.create_sketch(json!("xy"));
  assert!(!app.palette.slice);
}

/// The slice reaches the scene: with a box on the sketch plane and Slice on, the body's triangles
/// above the plane are cut away and the cap carries Fusion's hatch and outline lines.
#[test]
fn slice_cuts_the_scene_and_hatches_the_cap() {
  let mut app = App::with_session(Session::default());
  app.run("sketch.create", json!({"plane": "xy"})).unwrap();
  app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]})).unwrap();
  app.run("extrude", json!({"distance": 10})).unwrap();
  // a sketch on the plane 5 mm up, cutting the box in half
  app.run("plane.offset", json!({"plane": "xy", "offset": 5})).unwrap();
  let plane_id = app.s.design.features.last().unwrap().id;
  app.create_sketch(json!({"feature": plane_id.0}));
  app.palette.slice = true;
  let opts = crate::viewport::scene_options(&mut app);
  assert!(opts.slice && opts.section.is_some());
  let (scene, _) = app.scene.build(&app.s, &opts);
  let body = scene.meshes.iter().find(|m| m.analyze).expect("the body");
  let used: Vec<[f32; 3]> = body.indices.iter().map(|i| body.positions[*i as usize]).collect();
  assert!(!used.is_empty() && used.iter().all(|p| p[2] <= 5.0 + 1e-3), "the top half is cut away");
  let hatch = scene.lines.iter().filter(|l| l.color == fox_engine::scene::SLICE_HATCH).count();
  let outline = scene.lines.iter().filter(|l| l.color == fox_engine::scene::SLICE_OUTLINE).count();
  assert!(hatch > 4, "hatch lines: {hatch}");
  assert!(outline >= 4, "outline segments: {outline}");
  // Slice off restores the body at once
  app.palette.slice = false;
  let opts = crate::viewport::scene_options(&mut app);
  assert!(!opts.slice && opts.section.is_none());
  let (scene, _) = app.scene.build(&app.s, &opts);
  let body = scene.meshes.iter().find(|m| m.analyze).expect("the body");
  assert!(body.indices.iter().any(|i| body.positions[*i as usize][2] > 9.0));
}

/// 3D Sketch is enabled with no tool and with Line and the dialog tools, greyed for the tools
/// with no 3D form; on, the grid hides while a tool runs and shows again with Select.
#[test]
fn sketch_3d_checkbox_and_grid() {
  let mut app = sketching("xy");
  assert!(!app.palette.sketch_3d, "a new sketch starts in 2D");
  assert!(sketch_3d_enabled(&app));
  for t in
    [Tool::Line, Tool::MidpointLine, Tool::Rect, Tool::Circle, Tool::Arc3, Tool::Dimension, Tool::Trim, Tool::Fillet]
  {
    app.set_tool(t);
    assert!(sketch_3d_enabled(&app), "{t:?}");
  }
  for t in [Tool::Ellipse, Tool::Conic, Tool::Point, Tool::Spline, Tool::ControlSpline] {
    app.set_tool(t);
    assert!(!sketch_3d_enabled(&app), "{t:?} has no 3D form");
  }
  app.set_tool(Tool::Select);
  assert!(grid_shown(&app));
  app.palette.sketch_3d = true;
  assert!(grid_shown(&app), "no tool runs");
  app.set_tool(Tool::Line);
  assert!(!grid_shown(&app), "the grid hides under the manipulator");
  app.palette.sketch_3d = false;
  assert!(grid_shown(&app));
  app.palette.sketch_3d = true;
  app.show_grid = false;
  app.set_tool(Tool::Select);
  assert!(!grid_shown(&app), "Sketch Grid off stays off");
}
