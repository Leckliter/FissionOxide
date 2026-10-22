//! Fusion's mesh tools with their dialogs' settings: Reduce, Smooth, Convert Mesh and Plane Cut.

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().map(|m| m.volume).unwrap_or(0.0)).sum()
}

/// A session holding `make`'s model exported to STL and inserted back: one faceted body.
fn meshed(name: &str, make: impl FnOnce(&mut Session)) -> Session {
  let mut s = Session::default();
  make(&mut s);
  // a file of its own each time: the tests run side by side
  static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
  let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fox-mesh-tools-{}-{name}-{n}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let stl = dir.join(format!("{name}.stl"));
  run(&mut s, "export", json!({"path": stl}));
  let mut s = Session::default();
  run(&mut s, "import", json!({"path": stl}));
  assert_eq!(s.model.bodies.len(), 1);
  s
}

/// A 10 mm cube from the origin, meshed.
fn cube() -> Session {
  meshed("cube", |s| {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    run(s, "extrude", json!({"distance": 10}));
  })
}

fn triangles(s: &Session) -> usize {
  s.model.bodies[0].mesh().map(|m| m.indices.len() / 3).unwrap_or(0)
}

#[test]
fn reduce_leaves_a_box_whole_and_thins_a_ball() {
  // Fusion's Proportion 25% of a 12-triangle box: nothing can go without changing its shape
  let mut s = cube();
  let b = s.model.bodies[0].id.0;
  let r = run(&mut s, "mesh.reduce", json!({"body": b, "type": "proportion", "proportion": 25}));
  assert_eq!(r["name"], "Reduce1");
  assert!((volume(&s) - 1000.0).abs() < 1e-6, "the box stays a box: {}", volume(&s));
  assert!(s.model.bodies[0].shape.solids().unwrap().len() == 1);
  // a ball loses triangles but stays a closed ball
  let mut s = meshed("ball", |s| {
    run(s, "sphere", json!({"radius": 10}));
  });
  let b = s.model.bodies[0].id.0;
  let (n0, v0) = (triangles(&s), volume(&s));
  run(&mut s, "mesh.reduce", json!({"body": b, "proportion": 25}));
  let n1 = triangles(&s);
  assert!(n1 < n0 / 2, "{n0} → {n1}");
  assert!((volume(&s) - v0).abs() < 0.1 * v0, "{v0} → {}", volume(&s));
  run(&mut s, "undo", json!({}));
  run(&mut s, "mesh.reduce", json!({"body": b, "type": "face_count", "face_count": n0 / 2, "remesh": "uniform"}));
  assert!(triangles(&s) < n0, "{n0} → {}", triangles(&s));
  assert!(s.execute("mesh.reduce", &json!({"body": b, "type": "sideways"})).is_err());
}

#[test]
fn smooth_rounds_a_box_off_and_keeps_its_volume() {
  let mut s = cube();
  let b = s.model.bodies[0].id.0;
  let r = run(&mut s, "mesh.smooth", json!({"body": b, "smoothness": 1}));
  assert_eq!(r["name"], "Smooth1");
  assert!((volume(&s) - 1000.0).abs() < 10.0, "no shrinking: {}", volume(&s));
  let m = s.model.bodies[0].mesh().unwrap();
  // the corners come in towards the centre
  let reach =
    m.positions.iter().map(|p| p.iter().map(|c| (c - 5.0).powi(2)).sum::<f32>().sqrt()).fold(0.0f32, f32::max);
  assert!(reach < 75f32.sqrt() - 0.1, "{reach}");
}

#[test]
fn convert_mesh_adds_a_solid_and_hides_the_mesh() {
  let mut s = cube();
  let b = s.model.bodies[0].id;
  let r = run(&mut s, "mesh.convert", json!({"body": b.0, "operation": "parametric", "method": "prismatic"}));
  assert_eq!(r["name"], "Convert Mesh1");
  assert_eq!(s.model.bodies.len(), 2, "the mesh stays beside the new solid");
  assert!(!s.body_visible(b), "the mesh is hidden");
  let solid = s.model.bodies.iter().find(|x| x.id != b).unwrap();
  assert!(s.body_visible(solid.id));
  assert_eq!(solid.faces().len(), 6, "prismatic: one face per side");
  run(&mut s, "undo", json!({}));
  assert!(s.body_visible(b) && s.model.bodies.len() == 1, "undo takes back both");
  // faceted: a face per triangle; organic isn't there yet
  run(&mut s, "mesh.convert", json!({"body": b.0}));
  assert_eq!(s.model.bodies.iter().find(|x| x.id != b).unwrap().faces().len(), 12);
  run(&mut s, "undo", json!({}));
  let r = s.execute("mesh.convert", &json!({"body": b.0, "method": "organic"})).unwrap();
  assert!(r["error"].as_str().is_some_and(|e| e.contains("Organic")), "{r}");
  assert!(s.body_visible(b), "a failed conversion leaves the mesh showing");
}

#[test]
fn plane_cut_trims_splits_and_fills() {
  let cut = |p: Value| {
    let mut s = cube();
    let b = s.model.bodies[0].id.0;
    let mut p = p;
    p["body"] = json!(b);
    let r = run(&mut s, "mesh.plane_cut", p);
    assert_eq!(r["name"], "MeshPlaneCut1");
    s
  };
  // the XY plane 4 mm up: the part in front of it (above) goes
  let s = cut(json!({"plane": "xy", "distance": 4}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - 400.0).abs() < 1e-3, "{}", volume(&s));
  let s = cut(json!({"plane": "xy", "distance": 4, "flip": true, "fill": "minimal"}));
  assert!((volume(&s) - 600.0).abs() < 1e-3, "{}", volume(&s));
  // split body: both parts, closed
  let s = cut(json!({"plane": "xy", "distance": 4, "type": "split_body"}));
  assert_eq!(s.model.bodies.len(), 2);
  assert!((volume(&s) - 1000.0).abs() < 1e-3, "{}", volume(&s));
  // split faces: one body, the same volume, more triangles
  let s = cut(json!({"plane": "xy", "distance": 4, "type": "split_faces"}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - 1000.0).abs() < 1e-3);
  assert!(triangles(&s) > 12);
  // no fill: an open mesh
  let s = cut(json!({"plane": "xy", "distance": 4, "fill": "none"}));
  assert!(s.model.bodies[0].shape.solids().unwrap().is_empty(), "open");
  // turned 45° about the plane's X (the world X) through (0, 0, 5): the cut runs from z = 5 at
  // y = 0 to a corner edge, leaving 875 or 125 mm³ whichever way it turns
  let s = cut(json!({"plane": "xy", "distance": 5, "y_angle": "45 deg"}));
  let v = volume(&s);
  assert!((v - 875.0).abs() < 1e-3 || (v - 125.0).abs() < 1e-3, "{v}");
  // a plane past the body trims everything: an error, the feature stays red
  let mut s = cube();
  let b = s.model.bodies[0].id.0;
  let r = s.execute("mesh.plane_cut", &json!({"body": b, "plane": "xy", "distance": -1})).unwrap();
  assert!(r["error"].as_str().is_some_and(|e| e.contains("Flip")), "{r}");
}

#[test]
fn older_mesh_features_rebuild_as_before() {
  let mut s = cube();
  let b = s.model.bodies[0].id.0;
  run(&mut s, "mesh.smooth", json!({"bodies": [b], "passes": 2}));
  let f = s.design.features.last().unwrap();
  assert!(matches!(f.kind, fox_model::FeatureKind::Mesh { settings: None, .. }));
  // the old Laplacian passes shrink the box
  assert!(volume(&s) < 900.0, "{}", volume(&s));
}
