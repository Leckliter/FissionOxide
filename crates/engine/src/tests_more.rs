//! Tests for measure/selection, construction geometry by method, threads, coils, pipes, ribs,
//! scale, faces, surfaces, mesh tools, appearances and the newer sketch tools.

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

fn block(s: &mut Session, w: f64, d: f64, h: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [w, d]}));
  run(s, "extrude", json!({"distance": h}));
}

fn rod(s: &mut Session, r: f64, h: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.circle", json!({"center": [0, 0], "radius": r}));
  run(s, "extrude", json!({"distance": h}));
}

#[test]
fn measure_between_faces_and_selection_tools() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 10.0);
  let b = s.model.bodies[0].id.0;
  let r = run(
    &mut s,
    "measure",
    json!({"a": {"point": [20, 10, 10], "snap": "face"}, "b": {"point": [20, 10, 0], "snap": "face"}}),
  );
  assert!((r["distance"].as_f64().unwrap() - 10.0).abs() < 1e-9, "{r}");
  assert!((r["angle_deg"].as_f64().unwrap() - 180.0).abs() < 1e-6, "{r}");
  let one = run(&mut s, "measure", json!({"a": {"body": b}}));
  assert!((one["volume_mm3"].as_f64().unwrap() - 8000.0).abs() < 1e-6);
  // a face's Loop Length is its boundary: the 40 x 20 top face's four edges
  let top = s.model.bodies[0].faces().iter().find(|f| (f.area - 800.0).abs() < 1e-6 && f.center.z > 5.0).unwrap().id;
  let face = run(&mut s, "measure", json!({"a": {"body": b, "face": top}}));
  assert!((face["loop_length"].as_f64().unwrap() - 120.0).abs() < 1e-6, "{face}");
  let side = run(
    &mut s,
    "measure",
    json!({"a": {"point": [20, 10, 10], "snap": "face"}, "b": {"point": [0, 10, 5], "snap": "face"}}),
  );
  assert!((side["angle_deg"].as_f64().unwrap() - 90.0).abs() < 1e-6, "{side}");
  run(&mut s, "select.all", json!({"kind": "faces"}));
  assert_eq!(s.selection.len(), 6);
  s.selection.truncate(2);
  run(&mut s, "select.invert", json!({}));
  assert_eq!(s.selection.len(), 4);
  let c = run(&mut s, "inspect.center_of_mass", json!({}));
  let cm = c["center"].as_array().unwrap();
  assert!((cm[0].as_f64().unwrap() - 20.0).abs() < 1e-6 && (cm[2].as_f64().unwrap() - 5.0).abs() < 1e-6, "{c}");
}

/// Measure takes sketch curves and sketch points while a sketch is open: a line reads as an edge
/// with its length, two lines give the distance and angle between them.
#[test]
fn measure_sketch_curves_and_points() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let sk = s.edit_sketch.unwrap().0;
  let a = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}))["curves"][0].clone();
  let b = run(&mut s, "sketch.line", json!({"points": [[0, 4], [10, 4]]}))["curves"][0].clone();
  let curve = |c: &Value| json!({"type": "sketch_curve", "sketch": sk, "curve": c});
  let one = run(&mut s, "measure", json!({"a": curve(&a)}));
  assert_eq!(one["kind"], "edge", "{one}");
  assert!((one["length"].as_f64().unwrap() - 10.0).abs() < 1e-9, "{one}");
  let two = run(&mut s, "measure", json!({"a": curve(&a), "b": curve(&b)}));
  assert!((two["distance"].as_f64().unwrap() - 4.0).abs() < 1e-6, "{two}");
  assert!(two["angle_deg"].as_f64().unwrap().abs() < 1e-6, "parallel lines: {two}");
  // a sketch point measures as a point in space (on the XZ plane)
  let sketch = s.design.sketch(fox_model::FeatureId(sk)).unwrap();
  let p = sketch.curve_points(fox_sketch::CurveId(b.as_u64().unwrap() as u32))[1];
  let pt = run(&mut s, "measure", json!({"a": {"type": "sketch_point", "sketch": sk, "point": p.0}}));
  assert_eq!(pt["kind"], "point", "{pt}");
  let to = run(&mut s, "measure", json!({"a": curve(&a), "b": {"type": "sketch_point", "sketch": sk, "point": p.0}}));
  assert!((to["distance"].as_f64().unwrap() - 4.0).abs() < 1e-6, "{to}");
}

#[test]
fn construction_planes_axes_and_points() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 10.0);
  // midplane between the top and bottom faces
  let mid =
    run(&mut s, "plane", json!({"method": "midplane", "a": {"point": [20, 10, 10]}, "b": {"point": [20, 10, 0]}}));
  let pl = s.model.planes[&crate::model::FeatureId(mid["feature"].as_u64().unwrap() as u32)];
  assert!((pl.origin.z - 5.0).abs() < 1e-9, "{pl:?}");
  // a sketch on it splits the block
  let three = run(
    &mut s,
    "plane",
    json!({"method": "three_points", "a": {"at": [0, 0, 3]}, "b": {"at": [1, 0, 3]}, "c": {"at": [0, 1, 3]}}),
  );
  let f3 = three["feature"].as_u64().unwrap();
  // Fusion's sketch frame: the world origin dropped onto the plane, x = Z × n (world X for +Z)
  let pl = s.model.planes[&crate::model::FeatureId(f3 as u32)];
  assert!(
    (pl.origin - fox_kernel::glam::DVec3::new(0.0, 0.0, 3.0)).length() < 1e-9
      && (pl.x - fox_kernel::glam::DVec3::X).length() < 1e-9
  );
  let tilted = run(
    &mut s,
    "plane",
    json!({"method": "three_points", "a": {"at": [40, 0, 0]}, "b": {"at": [0, 30, 0]}, "c": {"at": [0, 0, 20]}}),
  );
  let pl = s.model.planes[&crate::model::FeatureId(tilted["feature"].as_u64().unwrap() as u32)];
  assert!((pl.x - fox_kernel::glam::DVec3::new(-0.8, 0.6, 0.0)).length() < 1e-9, "{pl:?}");
  assert!((pl.origin - fox_kernel::glam::DVec3::new(5.9016, 7.8689, 11.8033)).length() < 1e-3, "{pl:?}");
  run(&mut s, "split", json!({"plane": {"feature": f3}}));
  assert_eq!(s.model.bodies.len(), 2);
  // axis along an edge, plane at 45° about it
  let ax = run(&mut s, "axis", json!({"method": "edge", "edge": {"point": [20, 0, 0]}}));
  let ax = ax["feature"].as_u64().unwrap();
  let ang = run(&mut s, "plane", json!({"method": "angle", "axis": {"axis": ax}, "plane": "xy", "angle": "45 deg"}));
  let pl = s.model.planes[&crate::model::FeatureId(ang["feature"].as_u64().unwrap() as u32)];
  assert!((pl.normal().z.abs() - 0.5f64.sqrt()).abs() < 1e-9, "{pl:?}");
  // points: three planes meet at the origin; a vertex
  let p = run(&mut s, "point", json!({"method": "three_planes", "a": "xy", "b": "xz", "c": "yz"}));
  let q = s.model.points[&crate::model::FeatureId(p["feature"].as_u64().unwrap() as u32)];
  assert!(q.length() < 1e-9);
  let v = run(&mut s, "point", json!({"method": "vertex", "point": {"vertex": {"point": [20, 0, 0]}, "end": true}}));
  let q = s.model.points[&crate::model::FeatureId(v["feature"].as_u64().unwrap() as u32)];
  assert!(q.y.abs() < 1e-9 && (q.x - 40.0).abs() < 1e-9 || q.x.abs() < 1e-9, "{q:?}");
  // a construction axis can be a revolve axis
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [50, 0], "corner2": [60, 5]}));
  let z = run(&mut s, "axis", json!({"method": "two_points", "a": {"at": [0, 0, 0]}, "b": {"at": [0, 0, 1]}}));
  let sk = s.design.features.iter().rev().find(|f| f.kind.base_name() == "Sketch").unwrap().id.0;
  run(&mut s, "revolve", json!({"sketch": sk, "axis": {"axis": z["feature"]}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
}

#[test]
fn axis_through_sketch_line_or_axis() {
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  let sk = run(&mut s, "sketch.create", json!({"plane": "xz"}))["sketch"].as_u64().unwrap();
  let line = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 10]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.finish", json!({}));
  let axis = |s: &mut Session, edge: Value| {
    let f = run(s, "axis", json!({"method": "edge", "edge": edge}))["feature"].as_u64().unwrap();
    s.model.axes[&crate::model::FeatureId(f as u32)]
  };
  let want = fox_kernel::glam::DVec3::new(1.0, 0.0, 1.0).normalize();
  // by sketch and line, by a point on the line (no body there), and along that axis
  let (o, d) = axis(&mut s, json!({"sketch": sk, "line": line}));
  assert!(o.length() < 1e-9 && (d - want).length() < 1e-9, "{o} {d}");
  let (_, d) = axis(&mut s, json!({"point": [5, 0, 5]}));
  assert!((d - want).length() < 1e-9, "{d}");
  let f = s.design.features.last().unwrap().id.0;
  let (_, d) = axis(&mut s, json!({"axis": f}));
  assert!((d - want).length() < 1e-9, "{d}");
  assert!(s.execute("axis", &json!({"method": "edge", "edge": {"point": [50, 50, 50]}})).is_err());
}

#[test]
fn point_where_a_sketch_line_or_axis_crosses_a_plane() {
  let mut s = Session::default();
  let pt = |s: &Session, v: &Value| s.model.points[&crate::model::FeatureId(v["feature"].as_u64().unwrap() as u32)];
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
  let line = run(&mut s, "sketch.line", json!({"points": [[0, -2], [20, -12]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.finish", json!({}));
  let x10 = run(&mut s, "plane.offset", json!({"plane": "yz", "offset": 10}))["feature"].as_u64().unwrap();
  // the sketch line, and the Z axis against an offset XY plane
  let a = run(
    &mut s,
    "point",
    json!({"method": "edge_plane", "edge": {"sketch": sk, "curve": line}, "plane": {"feature": x10}}),
  );
  assert!((pt(&s, &a) - fox_kernel::glam::DVec3::new(10.0, -7.0, 0.0)).length() < 1e-9, "{:?}", pt(&s, &a));
  let z7 = run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 7}))["feature"].as_u64().unwrap();
  let b = run(&mut s, "point", json!({"method": "edge_plane", "edge": "z", "plane": {"feature": z7}}));
  assert!((pt(&s, &b) - fox_kernel::glam::DVec3::new(0.0, 0.0, 7.0)).length() < 1e-9, "{:?}", pt(&s, &b));
  // a construction axis, and a line parallel to the plane
  let ax = run(&mut s, "axis", json!({"method": "two_points", "a": {"at": [1, 1, 0]}, "b": {"at": [1, 1, 1]}}));
  let c =
    run(&mut s, "point", json!({"method": "edge_plane", "edge": {"axis": ax["feature"]}, "plane": {"feature": z7}}));
  assert!((pt(&s, &c) - fox_kernel::glam::DVec3::new(1.0, 1.0, 7.0)).length() < 1e-9, "{:?}", pt(&s, &c));
  let bad = s.execute("point", &json!({"method": "edge_plane", "edge": "x", "plane": {"feature": z7}}));
  assert!(bad.as_ref().map_or(true, |v| v["error"].is_string()), "{bad:?}");
}

#[test]
fn threads_cosmetic_and_modeled() {
  let mut s = Session::default();
  rod(&mut s, 5.0, 20.0);
  let v0 = volume(&s);
  run(&mut s, "thread", json!({"faces": [{"point": [5, 0, 10]}]}));
  assert_eq!(s.model.threads.len(), 1, "cosmetic thread drawn");
  assert!((volume(&s) - v0).abs() < 1e-9);
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"modeled": true}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let v1 = volume(&s);
  assert!(v1 < v0 * 0.95 && v1 > v0 * 0.6, "{v0} → {v1}");
  assert!(s.model.threads.is_empty());
  assert!((crate::model::regen::iso_pitch(10.0) - 1.5).abs() < 1e-12);
}

#[test]
fn coil_and_pipe() {
  let mut s = Session::default();
  run(&mut s, "coil", json!({"diameter": 20, "pitch": 5, "revolutions": 3, "section_size": 2}));
  assert_eq!(s.model.bodies.len(), 1, "{:?}", s.model.errors());
  let v = volume(&s);
  // wire length ≈ turns × circumference, section π r²
  let expected = 3.0 * (std::f64::consts::PI * 20.0f64).hypot(5.0) * std::f64::consts::PI;
  assert!((v - expected).abs() / expected < 0.05, "{v} vs {expected}");
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30], [20, 30]]}));
  let path = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "pipe", json!({"path": path, "diameter": 6, "wall": 1}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let v = volume(&s);
  let ring = std::f64::consts::PI * (9.0 - 4.0);
  assert!(v > ring * 45.0 && v < ring * 52.0, "{v}");
}

#[test]
fn rib_grows_to_the_floor() {
  let mut s = Session::default();
  // a U channel: base 40×20×4 with walls 4 thick, 20 tall
  block(&mut s, 40.0, 20.0, 4.0);
  run(&mut s, "sketch.create", json!({"plane": {"point": [20, 10, 4], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [4, 20]}));
  run(&mut s, "sketch.rect", json!({"corner1": [36, 0], "corner2": [40, 20]}));
  run(&mut s, "extrude", json!({"distance": 20, "operation": "join"}));
  let v0 = volume(&s);
  // a rib sketched on the XZ plane (y = 0 is the channel's side; move it to y = 10, along +Y in a new
  // design; Fusion's XZ frame has sketch y = −Z)
  run(&mut s, "plane.offset", json!({"plane": "xz", "offset": 10}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  // a line across the top between the walls; the rib grows down to the floor
  run(&mut s, "sketch.line", json!({"points": [[4, -20], [36, -20]]}));
  run(&mut s, "rib", json!({"thickness": 2}));
  let added = volume(&s) - v0;
  assert!((added - 32.0 * 16.0 * 2.0).abs() < 1.0, "rib added {added}");
}

#[test]
fn rib_thickness_on_one_side_and_a_web_that_misses() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 4.0);
  run(&mut s, "plane.offset", json!({"plane": "xz", "offset": 10}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  run(&mut s, "sketch.line", json!({"points": [[4, -20], [36, -20]]}));
  run(&mut s, "sketch.finish", json!({}));
  let v0 = volume(&s);
  let centre_y = |s: &Session| s.model.bodies[0].shape.mass().unwrap().center_of_mass.y;
  run(&mut s, "rib", json!({"thickness": 2}));
  let (sym_v, sym_y) = (volume(&s) - v0, centre_y(&s));
  run(&mut s, "undo", json!({}));
  // all 2 mm on one side of the line: the same rib, moved over by half its thickness
  run(&mut s, "rib", json!({"thickness": 2, "one_side": true}));
  let (one_v, one_y) = (volume(&s) - v0, centre_y(&s));
  assert!((one_v - sym_v).abs() < 1e-6, "{one_v} {sym_v}");
  let shift = (one_y - sym_y).abs() * volume(&s) / one_v;
  assert!((shift - 1.0).abs() < 1e-3, "rib moved {shift}");
  // a web on a line that runs past the block names itself in its error
  run(&mut s, "undo", json!({}));
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 30}));
  let high = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": high}}));
  run(&mut s, "sketch.line", json!({"points": [[60, 60], [80, 60]]}));
  run(&mut s, "sketch.finish", json!({}));
  let _ = s.execute("web", &json!({"thickness": 2}));
  let errors = s.model.errors().join("; ");
  assert!(errors.contains("the web doesn't meet the body"), "{errors}");
}

#[test]
fn scale_remove_faces_and_curved_press_pull() {
  let mut s = Session::default();
  block(&mut s, 10.0, 10.0, 10.0);
  run(&mut s, "scale", json!({"factor": 2}));
  assert!((volume(&s) - 8000.0).abs() < 1e-6, "{}", volume(&s));
  run(&mut s, "scale", json!({"factors": [0.5, 1, 1]}));
  assert!((volume(&s) - 4000.0).abs() < 1e-6, "{}", volume(&s));
  // a hole, then remove its face: the block is whole again
  let mut s = Session::default();
  block(&mut s, 30.0, 30.0, 10.0);
  run(&mut s, "sketch.create", json!({"plane": {"point": [15, 15, 10], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.circle", json!({"center": [15, 15], "radius": 5}));
  run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
  let holed = volume(&s);
  // press-pull the hole's wall outward by 1: the hole gets bigger
  run(&mut s, "press_pull", json!({"faces": [{"point": [20, 15, 5]}], "distance": -1}));
  let bigger = volume(&s);
  let ring = std::f64::consts::PI * (36.0 - 25.0) * 10.0;
  assert!(((holed - bigger) - ring).abs() < 1.0, "{holed} {bigger}");
  run(&mut s, "remove_faces", json!({"faces": [{"point": [21, 15, 5]}]}));
  assert!((volume(&s) - 9000.0).abs() < 1e-3, "{}", volume(&s));
}

#[test]
fn surfaces_patch_stitch_thicken() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0], [20, 10]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  assert_eq!(s.model.bodies.len(), 1);
  let b = s.model.bodies[0].id.0;
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.area - 30.0 * 5.0).abs() < 1e-6, "{}", m.area);
  run(&mut s, "thicken", json!({"bodies": [b], "thickness": 1}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  // 30 × 5 × 1, less the 1 × 1 × 5 where the two slabs overlap in the inside corner
  assert!((volume(&s) - 145.0).abs() < 1e-3, "{}", volume(&s));
  // a patch over a sketch rectangle
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.patch", json!({"sketch": sk}));
  assert!((s.model.bodies[0].shape.mass().unwrap().area - 100.0).abs() < 1e-6);
  // unstitch a box into six faces, stitch them back into a solid
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let b = s.model.bodies[0].id.0;
  run(&mut s, "surface.unstitch", json!({"bodies": [b]}));
  assert_eq!(s.model.bodies.len(), 6);
  let all: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  run(&mut s, "surface.stitch", json!({"bodies": all}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - 1000.0).abs() < 1e-3, "{}", volume(&s));
}

#[test]
fn mesh_tools_and_appearance() {
  let mut s = Session::default();
  run(&mut s, "sphere", json!({"radius": 10}));
  let dir = std::env::temp_dir().join(format!("fox-mesh-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let stl = dir.join("ball.stl");
  run(&mut s, "export", json!({"path": stl}));
  let mut s = Session::default();
  run(&mut s, "import", json!({"path": stl}));
  let b = s.model.bodies[0].id.0;
  let faces0 = s.model.bodies[0].faces().len();
  run(&mut s, "mesh.reduce", json!({"bodies": [b], "keep": 0.3}));
  let faces1 = s.model.bodies[0].faces().len();
  assert!(faces1 < faces0, "{faces0} → {faces1}");
  run(&mut s, "mesh.smooth", json!({"bodies": [b], "passes": 2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  run(&mut s, "body.appearance", json!({"body": b, "appearance": "ABS (Red)"}));
  assert_eq!(s.design.body_settings(fox_model::BodyId(b)).unwrap().color, Some([208, 50, 46]));
  assert!(s.execute("body.appearance", &json!({"body": b, "appearance": "Unobtainium"})).is_err());
  run(&mut s, "body.appearance", json!({"body": b}));
  assert_eq!(s.design.body_settings(fox_model::BodyId(b)).unwrap().color, None);
}

/// Extrudes an ellipse drawn through a point on it; with `legacy` its sketch has no behaviour
/// flags, as one from a file saved before them. The body's (face count, volume).
fn extruded_ellipse(legacy: bool) -> (usize, f64) {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.ellipse", json!({"center": [0, 0], "major": [20, 0], "point": [15, 5]}));
  if legacy {
    for f in &mut s.design.features {
      if let fox_model::FeatureKind::Sketch { sketch, .. } = &mut f.kind {
        sketch.behaviours = fox_sketch::Behaviours::legacy();
      }
    }
  }
  run(&mut s, "extrude", json!({"distance": 1}));
  assert_eq!(s.model.bodies.len(), 1);
  (s.model.bodies[0].faces().len(), volume(&s))
}

#[test]
fn a_new_ellipse_extrudes_to_one_side_face_and_a_legacy_one_to_two() {
  let want = std::f64::consts::PI * 20.0 * 5.0 / (1.0f64 - 0.5625).sqrt();
  // a new design: one elliptical side face, top and bottom, as in Fusion
  let (faces, v) = extruded_ellipse(false);
  assert_eq!(faces, 3);
  assert!((v - want).abs() < 0.05, "{v} vs {want}");
  // a legacy sketch keeps main's two half-ellipse sides
  let (faces, v) = extruded_ellipse(true);
  assert_eq!(faces, 4);
  assert!((v - want).abs() < 0.05, "{v} vs {want}");
}

#[test]
fn sketch_tools_make_profiles() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.ellipse", json!({"center": [0, 0], "major": [20, 0], "minor": 10}));
  assert_eq!(r["profiles"], 1, "{r}");
  run(&mut s, "extrude", json!({"distance": 1}));
  let area = volume(&s);
  assert!((area - std::f64::consts::PI * 200.0).abs() < 0.05, "{area}");
  // or through a point on it (Fusion's third pick): minor = 5 / sqrt(1 - (15/20)^2)
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.ellipse", json!({"center": [0, 0], "major": [20, 0], "point": [15, 5]}));
  run(&mut s, "extrude", json!({"distance": 1}));
  let want = std::f64::consts::PI * 20.0 * 5.0 / (1.0f64 - 0.5625).sqrt();
  assert!((volume(&s) - want).abs() < 0.05, "{} vs {want}", volume(&s));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let bad = json!({"center": [0, 0], "major": [20, 0], "point": [25, 5]});
  assert!(s.execute("sketch.ellipse", &bad).is_err(), "beyond the end of the axis");
  // 3-point circle, 2-point circle, 3-point rectangle, polygons, slots
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"through": [[10, 0], [0, 10], [-10, 0]]}));
  run(&mut s, "sketch.circle", json!({"through": [[40, 0], [50, 0]]}));
  run(&mut s, "sketch.rect", json!({"points": [[100, 0], [110, 10], [100, 20]]}));
  run(&mut s, "sketch.polygon", json!({"center": [0, 50], "vertex": [10, 50], "sides": 6, "circumscribed": true}));
  run(&mut s, "sketch.polygon", json!({"edge": [[50, 50], [60, 50]], "sides": 4}));
  run(&mut s, "sketch.slot", json!({"start": [100, 50], "end": [130, 50], "width": 10, "kind": "overall"}));
  run(&mut s, "sketch.slot", json!({"arc_center": [0, 100], "start": [20, 100], "end": [0, 120], "width": 4}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["profiles"].as_array().map(Vec::len).unwrap_or(0), 7, "{}", info["profiles"]);
  run(&mut s, "extrude", json!({"distance": 1}));
  let pi = std::f64::consts::PI;
  let hex = 6.0 * 10.0 * 10.0 / 3f64.sqrt();
  let slot = 20.0 * 10.0 + pi * 25.0;
  let arc_slot = pi / 2.0 * 20.0 * 4.0 + pi * 4.0;
  let expected = pi * 100.0 + pi * 25.0 + 200.0 * 2f64.sqrt() * 2f64.sqrt() / 2.0 * 1.0 + hex + 100.0 + slot + arc_slot;
  // 3-point rect: side √200, height 10√2·… = 20/√2·√2 → area √200 × √200/… checked loosely
  let v = volume(&s);
  assert!(v > expected * 0.8 && v < expected * 1.25, "{v} vs ~{expected}");
  // tangent arc off a line end, control spline, conic, text
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let l = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  let line = l["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.arc", json!({"tangent": line, "start": [10, 0], "end": [10, 10]}));
  run(&mut s, "sketch.control_spline", json!({"points": [[0, 20], [5, 30], [15, 10], [20, 20]]}));
  run(&mut s, "sketch.conic", json!({"start": [0, 40], "end": [20, 40], "apex": [10, 50], "rho": 0.5}));
  let t = run(&mut s, "sketch.text", json!({"at": [0, 60], "text": "Ob", "height": 10}));
  assert!(t["profiles"].as_u64().unwrap() >= 2, "{t}");
  assert!(s.model.sketches.values().all(|r| r.report.ok()));
}

#[test]
fn section_view_is_capped() {
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  block(&mut s, 40.0, 20.0, 10.0);
  let opts = crate::scene::view_options(&s, &json!({"section": {"plane": "xz", "offset": -10}})).unwrap();
  let (scene, _) = crate::scene::SceneBuilder::default().build(&s, &opts);
  // the body mesh, clipped, and the cap
  let caps: Vec<_> = scene.meshes.iter().filter(|m| !m.analyze && !m.indices.is_empty()).collect();
  assert!(!caps.is_empty(), "no cap");
  let cut = scene.meshes.iter().find(|m| m.analyze).unwrap();
  assert!(cut.positions.iter().enumerate().all(|(i, p)| !cut.indices.contains(&(i as u32)) || p[1] >= 10.0 - 1e-4));
}

/// The Sketch Palette's Slice: `slice: {sketch, flip}` sections at the sketch's plane (turned
/// when the camera is behind it), the cut capped in Fusion's pale fill with a magenta hatch and a
/// black outline rather than the analysis cap.
#[test]
fn slice_sections_at_the_sketch_plane_with_a_hatched_cap() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 10.0);
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 5}));
  let plane = s.design.features.last().unwrap().id;
  run(&mut s, "sketch.create", json!({"plane": {"feature": plane.0}}));
  let sk = s.edit_sketch.unwrap();
  let opts = crate::scene::view_options(&s, &json!({"slice": {"sketch": sk.0}})).unwrap();
  assert!(opts.slice);
  let pl = opts.section.expect("a section plane");
  assert!((pl.origin.z - 5.0).abs() < 1e-9 && pl.normal().z > 0.999);
  let (scene, _) = crate::scene::SceneBuilder::default().build(&s, &opts);
  let cut = scene.meshes.iter().find(|m| m.analyze).unwrap();
  assert!(cut.indices.iter().all(|i| cut.positions[*i as usize][2] <= 5.0 + 1e-4), "the top is cut away");
  let cap = scene.meshes.iter().find(|m| !m.analyze && !m.indices.is_empty()).expect("the cap");
  assert_eq!(cap.color, crate::scene::SLICE_FILL);
  let hatch: Vec<_> = scene.lines.iter().filter(|l| l.color == crate::scene::SLICE_HATCH).collect();
  let outline: Vec<_> = scene.lines.iter().filter(|l| l.color == crate::scene::SLICE_OUTLINE).collect();
  // 45° lines across the 40 × 20 cut, a quarter grid step apart, and its four sides
  assert!(hatch.len() > 10, "{} hatch lines", hatch.len());
  for l in &hatch {
    let (a, b) = (l.points[0], l.points[1]);
    assert!(((b[0] - a[0]) + (b[1] - a[1])).abs() < 1e-3, "diagonal: {a:?} {b:?}");
    assert!((a[2] - 5.0).abs() < 0.1 && (b[2] - 5.0).abs() < 0.1);
  }
  assert!(outline.len() >= 4, "{} outline segments", outline.len());
  assert!(
    outline
      .iter()
      .all(|l| l.points.iter().all(|p| p[0] < 1e-3 || p[0] > 40.0 - 1e-3 || p[1] < 1e-3 || p[1] > 20.0 - 1e-3))
  );
  // from below (flip) the bottom is cut away instead
  let opts = crate::scene::view_options(&s, &json!({"slice": {"sketch": sk.0, "flip": true}})).unwrap();
  let (scene, _) = crate::scene::SceneBuilder::default().build(&s, &opts);
  let cut = scene.meshes.iter().find(|m| m.analyze).unwrap();
  assert!(cut.indices.iter().all(|i| cut.positions[*i as usize][2] >= 5.0 - 1e-4));
  // section analysis wins over Slice while both are asked for
  let both = json!({"slice": {"sketch": sk.0}, "section": {"plane": "xz", "offset": -10}});
  let opts = crate::scene::view_options(&s, &both).unwrap();
  assert!(!opts.slice && opts.section.is_some());
  assert!(crate::scene::view_options(&s, &json!({"slice": {"sketch": 9999}})).is_err());
}

#[test]
fn parameter_change_regenerates_in_the_background() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "h", "expr": "10 mm"}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  run(&mut s, "extrude", json!({"distance": "h"}));
  let rev = s.revision;
  let (_, job) = s.begin("param.set", &json!({"name": "h", "expr": "25 mm"})).unwrap();
  let job = job.unwrap();
  // the old model stays until the job is taken back
  assert!((volume(&s) - 1000.0).abs() < 1e-6);
  s.finish_rebuild(job);
  assert!(s.revision > rev);
  assert!((volume(&s) - 2500.0).abs() < 1e-6, "{}", volume(&s));
  assert!(s.begin("param.set", &json!({"name": "h", "expr": "nope +"})).is_err());
  assert!(s.undo());
  assert!((volume(&s) - 1000.0).abs() < 1e-6);
  let (_, job) = s.begin("compute_all", &json!({})).unwrap();
  s.finish_rebuild(job.unwrap());
  assert!((volume(&s) - 1000.0).abs() < 1e-6);
}

#[test]
fn milling_setup_toolpaths_and_gcode() {
  let mut s = Session::default();
  // a 60 × 40 × 20 block with a 30 × 20 pocket 8 deep and a through hole
  block(&mut s, 60.0, 40.0, 20.0);
  run(&mut s, "sketch.create", json!({"plane": {"point": [30, 20, 20], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.rect", json!({"corner1": [10, 10], "corner2": [40, 30]}));
  run(&mut s, "extrude", json!({"distance": 8, "flip": true, "operation": "cut"}));
  run(&mut s, "sketch.create", json!({"plane": {"point": [50, 20, 20], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.circle", json!({"center": [50, 20], "diameter": 6}));
  run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
  let st = run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 2}));
  assert_eq!(st["stock"][1][2].as_f64().unwrap(), 22.0);
  run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 1}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 6, "stepdown": 2}));
  run(&mut s, "cam.contour", json!({"tool_diameter": 6, "stepdown": 5}));
  run(&mut s, "cam.drill", json!({"tool_diameter": 6, "peck": 3}));
  let g = run(&mut s, "cam.generate", json!({}));
  assert_eq!(g["operations"].as_array().unwrap().len(), 4, "{g}");
  assert!(g["minutes"].as_f64().unwrap() > 0.0);
  // the pocket stays inside its walls: the tool centre keeps a radius from them
  let pocket = &s.toolpaths[1];
  for m in pocket.moves.iter().filter(|m| !m.rapid) {
    assert!(
      m.to.x >= 13.0 - 1e-6 && m.to.x <= 37.0 + 1e-6 && m.to.y >= 13.0 - 1e-6 && m.to.y <= 27.0 + 1e-6,
      "{:?}",
      m.to
    );
    assert!(m.to.z >= 12.0 - 1e-6);
  }
  assert!(pocket.moves.iter().any(|m| (m.to.z - 12.0).abs() < 1e-6), "reaches the floor");
  // the contour runs a radius outside the part, below its bottom
  let contour = &s.toolpaths[2];
  for m in contour.moves.iter().filter(|m| !m.rapid && m.to.z < 19.0) {
    // distance from the 60 × 40 outline: the tool radius (rounded corners included)
    let dx = (-m.to.x).max(m.to.x - 60.0).max(0.0);
    let dy = (-m.to.y).max(m.to.y - 40.0).max(0.0);
    assert!((dx.hypot(dy) - 3.0).abs() < 0.05, "{:?}", m.to);
  }
  assert!(contour.moves.iter().any(|m| m.to.z < 0.0));
  let drill = s.toolpaths[3].drill.as_ref().unwrap();
  assert_eq!(drill.0.len(), 1);
  let code = run(&mut s, "cam.post", json!({}))["gcode"].as_str().unwrap().to_string();
  assert!(code.contains("T1 M6") && code.contains("T2 M6") && code.contains("G83") && code.ends_with("%\n"), "{code}");
  // work coordinates: nothing cuts above the stock top (Z0)
  for l in code.lines().filter(|l| l.starts_with("G1")) {
    if let Some(z) = l.split_whitespace().find(|w| w.starts_with('Z')) {
      assert!(z[1..].parse::<f64>().unwrap() <= 1e-9, "{l}");
    }
  }
  // operations follow the model: a deeper pocket moves the toolpath
  let f = s.design.features.iter().find(|f| f.name == "Extrude2").unwrap().id.0;
  run(
    &mut s,
    "feature.edit",
    json!({"feature": f, "set": {"extent": {"kind": "one_side", "distance": "12", "flip": true}}}),
  );
  run(&mut s, "cam.generate", json!({}));
  assert!(s.toolpaths[1].moves.iter().any(|m| (m.to.z - 8.0).abs() < 1e-6));
  let saved: crate::model::Design = serde_json::from_str(&serde_json::to_string(&s.design).unwrap()).unwrap();
  assert_eq!(saved.cam.len(), 1);
}

#[test]
fn ruled_offset_and_trim_surfaces() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  // a ruled surface between the top front and top back edges is the top face again
  run(&mut s, "surface.ruled", json!({"a": {"point": [0, -5, 5]}, "b": {"point": [0, 5, 5]}}));
  let ruled = s.model.bodies.last().unwrap();
  assert!((ruled.shape.mass().unwrap().area - 200.0).abs() < 1e-6);
  let id = ruled.id.0;
  run(&mut s, "surface.offset", json!({"bodies": [id], "distance": 3}));
  let off = s.model.bodies.last().unwrap().shape.mass().unwrap();
  assert!(
    (off.area - 200.0).abs() < 1e-6 && (off.bbox_min.z - 8.0).abs() < 1e-3 || (off.bbox_min.z - 2.0).abs() < 1e-3,
    "{off:?}"
  );
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": "yz"}));
  let b = s.model.bodies.iter().find(|b| b.id.0 == id).unwrap();
  assert!((b.shape.mass().unwrap().area - 100.0).abs() < 1e-6, "{}", b.shape.mass().unwrap().area);
}

#[test]
fn threads_iso_size_and_class() {
  use std::f64::consts::PI;
  // cosmetic M8x1.25 6g: the rod is sized to the class-mean major diameter, 7.866
  let mut s = Session::default();
  rod(&mut s, 4.0, 16.0);
  run(&mut s, "thread", json!({"faces": [{"point": [4, 0, 8]}], "designation": "M8x1.25"}));
  let want = PI * 3.933f64.powi(2) * 16.0;
  assert!((volume(&s) - want).abs() < 1e-6 * want, "{} vs {want}", volume(&s));
  assert_eq!(s.model.threads.len(), 1);
  // modeled M10x1.5 6g, 10 long from 5 above the low end: one crest face, flanks, root and the
  // two ends of the groove, square to the axis
  let mut s = Session::default();
  rod(&mut s, 5.0, 30.0);
  let t = run(
    &mut s,
    "thread",
    json!({"faces": [{"point": [5, 0, 15]}], "designation": "M10x1.5", "modeled": true, "length": 10, "offset": 5}),
  );
  assert!(s.model.errors().is_empty(), "{:?} {t}", s.model.errors());
  let b = &s.model.bodies[0];
  assert_eq!(b.faces().len(), 8);
  let m = b.shape.mass().unwrap();
  assert!((m.center_of_mass.z - 15.0).abs() > 0.05 && m.center_of_mass.z > 15.0, "groove low: {m:?}");
  assert!((m.bbox_max.x - 4.925).abs() < 1e-4, "{m:?}");
  assert!(m.center_of_mass.truncate().length() < 0.05, "{m:?}");
  // an internal thread sizes the bore to the class-mean minor diameter (8.526 for 6H)
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 4.25}));
  run(&mut s, "extrude", json!({"distance": 15, "profiles": [[7, 0]]}));
  run(&mut s, "thread", json!({"faces": [{"point": [4.25, 0, 7.5]}], "designation": "M10x1.5", "class": "6H"}));
  let want = PI * (100.0 - 4.263f64.powi(2)) * 15.0;
  assert!((volume(&s) - want).abs() < 1e-4 * want, "{} vs {want}", volume(&s));
  let f = s.design.features.last().unwrap().id.0;
  let _ = s.execute("feature.edit", &json!({"feature": f, "set": {"class": "6x"}}));
  assert!(!s.model.errors().is_empty(), "a bad class is an error");
}

#[test]
fn threads_in_designs_without_iso_threads_rebuild_as_before() {
  use fox_model::Behaviours;
  let rod_h = |s: &mut Session, h: f64| {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
    run(s, "extrude", json!({"distance": h}));
  };
  // a design saved before the ISO_THREADS flag keeps the sharp V groove, measured from the face's
  // own axis end: faces, volume and centre of mass as main built them
  let cases = [
    (
      30.0,
      json!({"faces": [{"point": [5, 0, 15]}], "modeled": true, "length": 10, "offset": 5}),
      13,
      2260.104747716,
      14.787423409,
    ),
    (
      30.0,
      json!({"faces": [{"point": [5, 0, 15]}], "modeled": true, "length": 10, "offset": 5, "flip": true}),
      13,
      2260.104266635,
      15.212579535,
    ),
    (
      -30.0,
      json!({"faces": [{"point": [5, 0, -15]}], "modeled": true, "length": 10, "offset": 5}),
      13,
      2260.107454316,
      -14.787431974,
    ),
    // Measured with #81's adaptive mass properties: same 27-face solid as main built, but the
    // helical face integrates more accurately (z was 14.99937 before; the thread is centred at 15).
    (30.0, json!({"faces": [{"point": [5, 0, 15]}], "modeled": true}), 27, 2069.648853055, 15.000006770),
  ];
  for (i, (h, t, faces, vol, z)) in cases.into_iter().enumerate() {
    let mut s = Session::default();
    s.design.behaviours = Behaviours::legacy();
    rod_h(&mut s, h);
    run(&mut s, "thread", t);
    assert!(s.model.errors().is_empty(), "{i}: {:?}", s.model.errors());
    let b = &s.model.bodies[0];
    let m = b.shape.mass().unwrap();
    assert_eq!(b.faces().len(), faces, "{i}");
    // Volume to 1e-4 relative and centre of mass to 1e-4 mm: a different groove profile or a
    // moved or flipped thread changes these by far more (tens of mm³, millimetres), while the
    // mass-property integration's own accuracy on helical faces moves them by less.
    assert!((m.volume - vol).abs() < 1e-4 * vol, "{i}: {} vs {vol}", m.volume);
    assert!((m.center_of_mass.z - z).abs() < 1e-4, "{i}: {m:?}");
    if i == 0 {
      // the same thread in a new design is the ISO profile: crest, flanks, root, square ends
      s.design.behaviours = Behaviours::current();
      s.rebuild();
      assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
      let b = &s.model.bodies[0];
      assert_eq!(b.faces().len(), 8);
      let m = b.shape.mass().unwrap();
      assert!((m.volume - vol).abs() > 1.0, "{m:?}");
      assert!(m.center_of_mass.z > 15.0, "groove from the low end: {m:?}");
    }
  }
  // a cosmetic thread in an old design starts from the face's own axis end (here the top) ...
  let cosmetic = json!({"faces": [{"point": [5, 0, 15]}], "length": 10, "offset": 5});
  let mut s = Session::default();
  s.design.behaviours = Behaviours::legacy();
  rod_h(&mut s, 30.0);
  run(&mut s, "thread", cosmetic.clone());
  assert!(s.model.threads[0].1[0].distance(fox_model::kernel::glam::DVec3::new(5.0, 0.0, 25.0)) < 1e-9);
  // ... and in a new design from the low end
  let mut s = Session::default();
  rod_h(&mut s, 30.0);
  run(&mut s, "thread", cosmetic);
  assert!((s.model.threads[0].1[0].z - 5.0).abs() < 1e-6, "{:?}", s.model.threads[0].1[0]);
}

#[test]
fn thread_designation_must_suit_the_drawn_diameter() {
  // an M10 thread on a Ø20 rod is an error, not a rod silently shrunk to Ø9.85
  let mut s = Session::default();
  rod(&mut s, 10.0, 20.0);
  let _ = s.execute("thread", &json!({"faces": [{"point": [10, 0, 10]}], "designation": "M10x1.5"}));
  let e = s.model.errors();
  assert!(e.iter().any(|e| e.contains("Ø10") && e.contains("Ø20")), "{e:?}");
  assert!((volume(&s) - std::f64::consts::PI * 100.0 * 20.0).abs() < 1e-6, "the rod is left as drawn");
  // an M10 inside a Ø10 bore too: the bore is drawn at the tap-drill (minor) size
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "extrude", json!({"distance": 15, "profiles": [[7, 0]]}));
  let _ = s.execute("thread", &json!({"faces": [{"point": [5, 0, 7.5]}], "designation": "M10x1.5", "class": "6H"}));
  assert!(s.model.errors().iter().any(|e| e.contains("hole")), "{:?}", s.model.errors());
}

#[test]
fn center_arc_slot_takes_the_shorter_arc_unless_told() {
  let pi = std::f64::consts::PI;
  // the area of a width-4 slot along `sweep` radians of an R20 arc, extruded 1
  let slot = |sweep: f64| sweep * 20.0 * 4.0 + pi * 4.0;
  for (ccw, sweep) in [(None, pi / 2.0), (Some(true), 1.5 * pi), (Some(false), pi / 2.0)] {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let mut p = json!({"arc_center": [0, 0], "start": [0, 20], "end": [20, 0], "width": 4});
    if let Some(ccw) = ccw {
      p["ccw"] = json!(ccw);
    }
    run(&mut s, "sketch.slot", p);
    run(&mut s, "extrude", json!({"distance": 1}));
    let v = volume(&s);
    assert!((v - slot(sweep)).abs() < 1e-3 * slot(sweep), "{ccw:?}: {v} vs {}", slot(sweep));
  }
}

#[test]
fn pipe_sections_and_partial_paths() {
  // a straight 40 mm path up z, in a sketch whose normal is +y
  let path = |s: &mut Session| {
    run(s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, 1, 0], "x": [1, 0, 0]}}));
    run(s, "sketch.line", json!({"points": [[0, 0], [0, -40]]}));
    let path = s.edit_sketch.unwrap().0;
    run(s, "sketch.finish", json!({}));
    path
  };
  let pipe = |p: Value| {
    let mut s = Session::default();
    let id = path(&mut s);
    let mut p = p;
    p["path"] = json!(id);
    run(&mut s, "pipe", p);
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    s
  };
  let near = |s: &Session, want: f64| {
    let v = volume(s);
    assert!((v - want).abs() < want * 1e-6, "{v} vs {want}");
  };
  near(&pipe(json!({"size": 6, "section": "square"})), 36.0 * 40.0);
  near(&pipe(json!({"size": 6, "section": "square", "wall": 1})), (36.0 - 16.0) * 40.0);
  // a triangle of height 6 has side 4√3; half the path
  let s = pipe(json!({"size": 6, "section": "triangular", "distance": 0.5}));
  near(&s, 12.0 * 3f64.sqrt() * 20.0);
  // the base lies 3 below the path along the sketch normal (y), the apex 3 above
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_min.y + 3.0).abs() < 1e-3 && (m.bbox_max.y - 3.0).abs() < 1e-3, "{m:?}");
  assert!((m.center_of_mass.y + 1.0).abs() < 1e-6 && (m.bbox_max.z - 20.0).abs() < 1e-3, "{m:?}");
  // hollow: the triangle with inradius 2 - 1, so a quarter of the area is the hole
  near(&pipe(json!({"size": 6, "section": "triangular", "wall": 1})), 0.75 * 12.0 * 3f64.sqrt() * 40.0);

  // half a spline path holds half the volume of the whole one
  let spline = |d: Option<f64>| {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, -20], [0, -40], [15, -60]]}));
    let id = s.edit_sketch.unwrap().0;
    run(&mut s, "sketch.finish", json!({}));
    let mut p = json!({"path": id, "diameter": 2});
    if let Some(d) = d {
      p["distance"] = json!(d);
    }
    run(&mut s, "pipe", p);
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    volume(&s)
  };
  let (half, whole) = (spline(Some(0.5)), spline(None));
  assert!((half / whole - 0.5).abs() < 2e-3, "{half} / {whole}");

  // keys it doesn't know are refused rather than ignored
  let mut s = Session::default();
  let id = path(&mut s);
  assert!(s.execute("pipe", &json!({"path": id, "diameter": 6, "shape": "square"})).is_err());
  assert!(s.execute("pipe", &json!({"path": id, "diameter": 6, "section": "hexagon"})).is_err());
  assert!(s.execute("pipe", &json!({"path": id, "diameter": 6, "size": 4})).is_err());

  // a round pipe rebuilds exactly as older files did: the same edges (its seam, the side face's
  // one straight edge, included) as the old construction, a circle on
  // Plane::from_normal(start, tangent, sketch normal) swept along the path
  use fox_kernel::glam::DVec3;
  use fox_kernel::{Plane, Shape};
  use fox_sketch::{Loop, Profile, Seg};
  let s = pipe(json!({"diameter": 6}));
  let new = s.model.bodies[0].shape.edges().unwrap();
  let sketch = Plane { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::NEG_Z };
  let line = [Seg::Line { a: [0.0, 0.0], b: [0.0, -40.0] }];
  let sec = Plane::from_normal(DVec3::ZERO, DVec3::Z, sketch.normal()).unwrap();
  let circle = Loop::new(vec![Seg::Circle { center: [0.0, 0.0], radius: 3.0, ccw: true }]);
  let old = Shape::face(&sec, &Profile { outer: circle, holes: vec![] })
    .unwrap()
    .sweep(&Shape::path(&sketch, &line).unwrap())
    .unwrap()
    .edges()
    .unwrap();
  assert_eq!(new.len(), old.len(), "{new:?} vs {old:?}");
  for (a, b) in new.iter().zip(&old) {
    let same = a.kind == b.kind
      && [(a.start, b.start), (a.mid, b.mid), (a.end, b.end)].iter().all(|(p, q)| p.distance(*q) < 1e-6);
    assert!(same, "{a:?} vs {b:?}");
  }
  let seam = new.iter().find(|e| e.kind == fox_kernel::CurveKind::Line).expect("a seam");
  assert!(seam.mid.distance(DVec3::new(3.0, 0.0, 20.0)) < 1e-6, "{seam:?}");
}

#[test]
fn deleting_a_used_sketch_keeps_its_extrude_on_cached_geometry() {
  let mut s = Session::default();
  let sk = run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 5], "normal": [0, 0, 1], "x": [1, 0, 0]}}))["sketch"].clone();
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 30]}));
  run(&mut s, "sketch.finish", json!({}));
  let ex = run(&mut s, "extrude", json!({"distance": 20}))["feature"].clone();
  let r = run(&mut s, "feature.delete", json!({"feature": sk}));
  assert_eq!(r["removed"], json!([sk]));
  assert_eq!(r["cached"], json!([ex]));
  assert_eq!(s.design.features.len(), 1);
  assert!(s.model.sketches.is_empty(), "the cached sketch isn't shown");
  assert!((volume(&s) - 24000.0).abs() < 1e-6, "{}", volume(&s));
  let st = &s.model.status[0];
  assert!(st.error.is_none() && st.warning.as_deref().is_some_and(|w| w.contains("cached geometry")), "{st:?}");
  assert_eq!(s.model.bounds().unwrap().0.z, 5.0);
  // the extrude stays editable, and the cache survives a save
  run(&mut s, "feature.edit", json!({"feature": ex, "set": {"extent": {"kind": "one_side", "distance": "10"}}}));
  assert!((volume(&s) - 12000.0).abs() < 1e-6, "{}", volume(&s));
  let d: fox_model::Design = serde_json::from_str(&serde_json::to_string(&s.design).unwrap()).unwrap();
  assert_eq!(d, s.design);
  // cascade deletes the extrude too, as before
  assert!(s.undo() && s.undo());
  assert_eq!(run(&mut s, "feature.delete", json!({"feature": sk, "cascade": true}))["removed"], json!([sk, ex]));
  assert!(s.model.bodies.is_empty());
}

#[test]
fn project_to_surface_drapes_curves_a_pipe_follows() {
  // an r5 circle on the plane x = 20 projected onto an r10 h30 cylinder's side
  let start = || {
    let mut s = Session::new(fox_model::Design::new("t"));
    run(&mut s, "cylinder", json!({"radius": 10, "height": 30}));
    let plane = json!({"origin": [20, 0, 0], "normal": [1, 0, 0], "x": [0, 1, 0]});
    run(&mut s, "sketch.create", json!({"plane": plane}));
    run(&mut s, "sketch.circle", json!({"center": [0, 15], "radius": 5}));
    run(&mut s, "sketch.finish", json!({}));
    s
  };
  let side = json!([{"point": [-10, 0, 5]}]);

  // along X: a saddle loop front and back, two grooves
  let mut s = start();
  let v = run(
    &mut s,
    "sketch.project_to_surface",
    json!({"sketch": 2, "faces": side, "type": "along_vector", "direction": "x"}),
  );
  let proj = fox_model::FeatureId(v["feature"].as_u64().unwrap() as u32);
  assert_eq!(s.model.curves3d[&proj].len(), 2);
  run(&mut s, "pipe", json!({"path": proj.0, "diameter": 2, "operation": "cut"}));
  assert!((volume(&s) - 9326.976).abs() < 10.0, "{}", volume(&s));
  assert_eq!(s.model.bodies[0].faces().len(), 13);

  // to the closest point: one loop on the front, one groove
  let mut s = start();
  run(&mut s, "sketch.project_to_surface", json!({"sketch": 2, "faces": side}));
  run(&mut s, "pipe", json!({"path": 3, "diameter": 2, "operation": "cut"}));
  assert!((volume(&s) - 9388.016).abs() < 10.0, "{}", volume(&s));
  assert_eq!(s.model.bodies[0].faces().len(), 6);

  // a direction is needed along a vector; a 3D path takes a round section only
  let mut s = start();
  assert!(
    s.execute("sketch.project_to_surface", &json!({"sketch": 2, "faces": side, "type": "along_vector"})).is_err()
  );
  run(&mut s, "sketch.project_to_surface", json!({"sketch": 2, "faces": side}));
  let v = s.execute("pipe", &json!({"path": 3, "diameter": 2, "section": "square", "operation": "cut"})).unwrap();
  assert!(v["error"].is_string(), "{v}");
}

/// SELECT > Selection Tools: Select By Name (wildcards), Select By Size, Select By Boundary and
/// Seed And Boundary.
#[test]
fn selection_tools_pick_by_name_size_and_boundary() {
  use crate::Sel;
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let b = s.model.bodies[0].id;
  assert!(crate::inspect::name_matches("bo*", "Body1") && crate::inspect::name_matches("b?dy1", "Body1"));
  assert!(!crate::inspect::name_matches("Body", "Body1") && !crate::inspect::name_matches("*2", "Body1"));
  let r = run(&mut s, "select.by_name", json!({"name": "body*"}));
  assert_eq!((r["selected"].as_u64(), s.selection.as_slice()), (Some(1), &[Sel::Body { body: b }][..]));
  run(&mut s, "select.by_name", json!({"name": "wheel*"}));
  assert!(s.selection.is_empty());
  assert!(s.execute("select.by_name", &json!({})).is_err(), "a name is required");
  // the body's bounding box diagonal is sqrt(20² + 10² + 5²) ≈ 22.9 mm
  run(&mut s, "select.by_size", json!({"max": 30}));
  assert_eq!(s.selection, [Sel::Body { body: b }]);
  run(&mut s, "select.by_size", json!({"max": "20 mm"}));
  assert!(s.selection.is_empty());
  // faces by the square root of their area: the two 10 × 5 ends are under 8 mm
  run(&mut s, "select.by_size", json!({"kind": "faces", "max": 8}));
  assert_eq!(s.selection.len(), 2);
  assert!(s.selection.iter().all(|x| matches!(x, Sel::Face { .. })));
  // the top face's four edges bound it: the one face inside, not the five outside
  let body = s.model.body(b).unwrap();
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let side = body.faces().iter().find(|f| f.normal.x > 0.9).unwrap().id;
  let rim: Vec<Value> =
    body.edges().iter().filter(|e| e.faces.contains(&top)).map(|e| json!({"body": b.0, "edge": e.id})).collect();
  assert_eq!(rim.len(), 4);
  run(&mut s, "select.by_boundary", json!({"edges": rim}));
  assert_eq!(s.selection, [Sel::Face { body: b, face: top }]);
  // the selected edges are the default boundary; one edge doesn't close a boundary
  s.selection = vec![Sel::Edge { body: b, edge: rim[0]["edge"].as_u64().unwrap() as u32 }];
  assert!(s.execute("select.by_boundary", &json!({})).is_err());
  // a seed face spreads over every face it can reach without crossing the boundary
  run(&mut s, "select.seed_boundary", json!({"seed": {"body": b.0, "face": top}}));
  assert_eq!(s.selection.len(), 6);
  run(&mut s, "select.seed_boundary", json!({"seed": {"body": b.0, "face": top}, "edges": rim}));
  assert_eq!(s.selection, [Sel::Face { body: b, face: top }]);
  s.selection = vec![Sel::Face { body: b, face: side }];
  run(&mut s, "select.seed_boundary", json!({"edges": rim}));
  assert_eq!(s.selection.len(), 5, "the selected face seeds the outside of the rim");
}

/// Measure reads a face's loop length; Interference with `coincident` also lists bodies that only
/// touch; a section's angles turn its plane and `hatch` and `color` reach the view options.
#[test]
fn loop_length_coincident_interference_and_tilted_sections() {
  let mut s = Session::default();
  for at in [[0, 0], [5, 0], [-10, 0]] {
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": at, "operation": "new_body"}));
  }
  let ids: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  assert_eq!(ids.len(), 3);
  let b = s.model.body(fox_model::BodyId(ids[0])).unwrap();
  let face = b.faces()[0].id;
  let m = run(&mut s, "measure", json!({"a": {"body": ids[0], "face": face}}));
  assert!((m["loop_length"].as_f64().unwrap() - 40.0).abs() < 1e-6, "{m}");

  let plain = run(&mut s, "assembly.interference", json!({"bodies": ids}));
  assert_eq!(plain.as_array().unwrap().len(), 1, "{plain}");
  let touching = run(&mut s, "assembly.interference", json!({"bodies": ids, "coincident": true}));
  let rows = touching.as_array().unwrap();
  assert_eq!(rows.len(), 2, "{touching}");
  assert!(rows.iter().any(|r| r["coincident"] == json!(true) && r["volume_mm3"] == json!(0.0)));

  let flat = crate::scene::view_options(&s, &json!({"section": {"plane": "xy", "offset": 5.0}})).unwrap();
  assert!(!flat.section_hatch && flat.section_color.is_none());
  let o = crate::scene::view_options(
    &s,
    &json!({"section": {"plane": "xy", "offset": 5.0, "angle1": 90.0, "hatch": true, "color": "single", "center": [3, 0, 0]}}),
  )
  .unwrap();
  let pl = o.section.unwrap();
  assert!(pl.normal().dot(fox_kernel::glam::DVec3::Z).abs() < 1e-9, "turned a quarter about x");
  assert!((pl.origin - fox_kernel::glam::DVec3::new(3.0, 0.0, 5.0)).length() < 1e-9, "about the centre on the plane");
  assert!(o.section_hatch && o.section_color.is_some());
}

#[test]
fn remove_features_finds_fillets_and_holes_and_removes_them() {
  let mut s = Session::default();
  block(&mut s, 40.0, 30.0, 10.0);
  let plain = volume(&s);
  // a through hole d10 and a 3 mm round on the edge along x at y 0, z 10
  run(&mut s, "sketch.create", json!({"plane": {"point": [0, 0, 10], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.circle", json!({"center": [12, 15], "radius": 5}));
  run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
  run(&mut s, "fillet", json!({"edges": [{"point": [30, 0, 10]}], "radius": 3}));
  let b = s.model.bodies[0].id.0;
  let found = run(&mut s, "remove_features", json!({"bodies": [b], "dry_run": true}))["features"].clone();
  let of = |t: &str| found.as_array().unwrap().iter().filter(|f| f["type"] == t).cloned().collect::<Vec<_>>();
  assert_eq!(of("fillet").len(), 1, "{found}");
  assert!((of("fillet")[0]["size"].as_f64().unwrap() - 3.0).abs() < 1e-6, "{found}");
  assert_eq!(of("hole").len(), 1, "{found}");
  assert!((of("hole")[0]["size"].as_f64().unwrap() - 10.0).abs() < 1e-6, "{found}");
  // only the ticked types: the hole goes, the round stays
  let before = volume(&s);
  run(&mut s, "remove_features", json!({"bodies": [b], "types": ["hole"]}));
  let hole = std::f64::consts::PI * 25.0 * 10.0;
  assert!((volume(&s) - (before + hole)).abs() < 1e-3, "{} {}", volume(&s), before + hole);
  let left = run(&mut s, "remove_features", json!({"bodies": [b], "dry_run": true}))["features"].clone();
  assert_eq!(left.as_array().unwrap().len(), 1, "{left}");
  // then the round: the block is whole again
  run(&mut s, "remove_features", json!({"bodies": [b], "types": ["fillet"]}));
  assert!((volume(&s) - plain).abs() < 1e-3, "{} {plain}", volume(&s));
  // nothing left to find
  assert!(s.execute("remove_features", &json!({"bodies": [b]})).is_err());
  assert!(s.execute("remove_features", &json!({"bodies": [b], "types": ["slot"]})).is_err());
}

#[test]
fn replace_with_primitives_swaps_each_body_for_a_box_or_cylinder() {
  let mut s = Session::default();
  rod(&mut s, 5.0, 20.0);
  let b = s.model.bodies[0].id.0;
  run(&mut s, "replace_with_primitives", json!({"bodies": [b]}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!(s.model.bodies[0].id.0 != b);
  assert!((volume(&s) - 10.0 * 10.0 * 20.0).abs() < 1e-6, "{}", volume(&s));
  // one undo brings the rod back
  s.undo();
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - std::f64::consts::PI * 25.0 * 20.0).abs() < 1e-3, "{}", volume(&s));
  // a block as a cylinder: around its corners, along Z
  let mut s = Session::default();
  block(&mut s, 10.0, 10.0, 4.0);
  let b = s.model.bodies[0].id.0;
  run(&mut s, "replace_with_primitives", json!({"bodies": [b], "shape": "cylinder"}));
  let r2 = 50.0;
  assert!((volume(&s) - std::f64::consts::PI * r2 * 4.0).abs() < 1e-3, "{}", volume(&s));
  assert!(s.execute("replace_with_primitives", &json!({"bodies": []})).is_err());
  assert!(s.execute("replace_with_primitives", &json!({"bodies": [s.model.bodies[0].id.0], "shape": "cone"})).is_err());
}

/// A sketch on the plane `z` up (sketch x along world X) with the polylines `lines`, finished:
/// the sketch's id and its curves'.
fn lines_at(s: &mut Session, z: f64, lines: &[Value]) -> (u32, Vec<u32>) {
  run(s, "sketch.create", json!({"plane": {"origin": [0, 0, z], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
  let sk = s.design.features.last().unwrap().id;
  for pts in lines {
    run(s, "sketch.line", json!({ "points": pts }));
  }
  run(s, "sketch.finish", json!({}));
  (sk.0, s.design.sketch(sk).unwrap().curves.iter().map(|c| c.id.0).collect())
}

/// Did the command run, with its feature regenerating?
fn works(s: &mut Session, cmd: &str, p: Value) -> bool {
  s.execute(cmd, &p).is_ok_and(|v| v.get("error").is_none_or(Value::is_null))
}

/// Fusion's Split Face: a face of another body splits as its surface, extended across the face
/// or (Extend Splitting Tool off) as it is; sketch curves split with their surface, along a
/// vector or at the closest point, and lines that stop short reach the edges when extended.
#[test]
fn split_face_with_surfaces_and_curves_by_each_split_type() {
  let top = json!([{"point": [15, 15, 10]}]);
  let faces = |s: &Session| s.model.bodies[0].faces().len();
  let fresh = || {
    let mut s = Session::default();
    block(&mut s, 30.0, 30.0, 10.0);
    s
  };
  // a face of another body standing off the block, square to the top along x = 10
  let mut s = fresh();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [10, 40], "corner2": [20, 50]}));
  run(&mut s, "extrude", json!({"distance": 10, "operation": "new_body"}));
  let wall = json!({"point": [10, 45, 5]});
  let n = faces(&s);
  assert!(!works(&mut s, "split_face", json!({"faces": top, "tool": [wall], "extend": false})), "it doesn't reach");
  s.undo();
  run(&mut s, "split_face", json!({"faces": top, "tool": [wall]}));
  assert_eq!(faces(&s), n + 1);
  assert_eq!(s.model.bodies.len(), 2, "the block stays one body");

  // a sketch line above the top, across it
  let across = || json!([[10, -10], [10, 40]]);
  for (how, dir, splits) in [
    ("surface", Value::Null, true),
    ("along_vector", json!("z"), true),
    // swept along x the line's sheet lies flat above the top
    ("along_vector", json!("x"), false),
  ] {
    let mut s = fresh();
    let (sk, c) = lines_at(&mut s, 20.0, &[across()]);
    let tool = json!([{"sketch": sk, "curve": c[0]}]);
    let p = json!({"faces": top, "tool": tool, "split_type": how, "direction": dir});
    assert_eq!(works(&mut s, "split_face", p), splits, "{how} {dir}");
    if splits {
      assert_eq!(faces(&s), 7, "{how}");
      assert!((volume(&s) - 9000.0).abs() < 1e-6);
    }
  }
  // Along Vector with curves needs its direction
  let mut s = fresh();
  let (sk, c) = lines_at(&mut s, 20.0, &[across()]);
  assert!(
    s.execute(
      "split_face",
      &json!({"faces": top, "tool": [{"sketch": sk, "curve": c[0]}], "split_type": "along_vector"})
    )
    .is_err()
  );

  // a short line: as drawn it stops inside the top; extended it reaches the edges
  let mut s = fresh();
  let (sk, c) = lines_at(&mut s, 20.0, &[json!([[10, 10], [10, 20]])]);
  let tool = json!([{"sketch": sk, "curve": c[0]}]);
  assert!(!works(&mut s, "split_face", json!({"faces": top, "tool": tool, "extend": false})));
  s.undo();
  run(&mut s, "split_face", json!({"faces": top, "tool": tool}));
  assert_eq!(faces(&s), 7);
  // an L of two lines extends at its open ends only: the top splits in two, not three
  let mut s = fresh();
  let (sk, c) = lines_at(&mut s, 20.0, &[json!([[10, 5], [10, 20], [20, 20]])]);
  let tool: Vec<Value> = c.iter().map(|c| json!({"sketch": sk, "curve": c})).collect();
  run(&mut s, "split_face", json!({"faces": top, "tool": tool}));
  assert_eq!(faces(&s), 7);

  // Closest Point: a line from edge to edge, dropped onto the top
  let mut s = fresh();
  let (sk, c) = lines_at(&mut s, 20.0, &[json!([[10, 0], [10, 30]])]);
  let tool = json!([{"sketch": sk, "curve": c[0]}]);
  run(&mut s, "split_face", json!({"faces": top, "tool": tool, "split_type": "closest_point"}));
  assert_eq!(faces(&s), 7);
  assert!((volume(&s) - 9000.0).abs() < 1e-6);
  // ... and it follows a saved design
  let kind = &s.design.features.last().unwrap().kind;
  let saved: fox_model::FeatureKind = serde_json::from_str(&serde_json::to_string(kind).unwrap()).unwrap();
  assert_eq!(&saved, kind);

  // a design saved before the tool could be faces or curves still reads
  let old: fox_model::FeatureKind =
    serde_json::from_value(json!({"type": "split_face", "faces": [], "tool": {"type": "origin", "plane": "yz"}}))
      .unwrap_or_else(|e| panic!("{e}"));
  let fox_model::FeatureKind::SplitFace { tool: Some(_), extend: true, split_type, .. } = old else {
    panic!("{old:?}")
  };
  assert_eq!(split_type, fox_model::SplitFaceType::Surface);
}

/// Remove Features finds chamfers, extruded bosses and pockets, revolved bumps and other ones
/// as well as fillets and holes, and removes the ticked types.
#[test]
fn remove_features_finds_chamfers_extrudes_revolves_and_others() {
  let mut s = Session::default();
  block(&mut s, 60.0, 40.0, 10.0);
  let plain = volume(&s);
  let on_top = json!({"origin": [0, 0, 10], "normal": [0, 0, 1], "x": [1, 0, 0]});
  // a 2 mm chamfer on the front top edge
  run(&mut s, "chamfer", json!({"edges": [{"point": [30, 0, 10]}], "distance": 2}));
  // a square boss 5 tall and a square pocket 4 deep
  run(&mut s, "sketch.create", json!({ "plane": on_top }));
  run(&mut s, "sketch.rect", json!({"corner1": [5, 10], "corner2": [15, 20]}));
  run(&mut s, "extrude", json!({"distance": 5, "operation": "join"}));
  run(&mut s, "sketch.create", json!({ "plane": on_top }));
  run(&mut s, "sketch.rect", json!({"corner1": [20, 10], "corner2": [30, 20]}));
  run(&mut s, "extrude", json!({"distance": 4, "flip": true, "operation": "cut"}));
  // a dome (half a ball standing on the top)
  run(&mut s, "sphere", json!({"plane": on_top, "at": [45, 20], "radius": 4, "operation": "join"}));
  // a wedge lying along x
  run(&mut s, "sketch.create", json!({"plane": {"origin": [40, 0, 0], "normal": [1, 0, 0], "x": [0, 1, 0]}}));
  run(&mut s, "sketch.line", json!({"points": [[25, 10], [35, 10], [25, 16]], "closed": true}));
  run(&mut s, "extrude", json!({"distance": 10, "operation": "join"}));
  let b = s.model.bodies[0].id.0;
  let found = run(&mut s, "remove_features", json!({"bodies": [b], "dry_run": true}))["features"].clone();
  let count = |t: &str| found.as_array().unwrap().iter().filter(|f| f["type"] == t).count();
  assert_eq!(["fillet", "hole", "chamfer", "extrude", "revolve", "other"].map(count), [0, 0, 1, 2, 1, 1], "{found}");
  // the extrudes go: the boss's volume off, the pocket's back
  let before = volume(&s);
  run(&mut s, "remove_features", json!({"bodies": [b], "types": ["extrude"]}));
  assert!((volume(&s) - (before - 500.0 + 400.0)).abs() < 1e-3, "{} {before}", volume(&s));
  // then the rest: the block is whole again
  run(&mut s, "remove_features", json!({"bodies": [b], "types": ["chamfer", "revolve", "other"]}));
  assert!((volume(&s) - plain).abs() < 1e-3, "{} {plain}", volume(&s));
  assert_eq!(s.model.bodies[0].faces().len(), 6);
}

/// Replace with Primitives' Primitive Size: the box stands `offset` off the body.
#[test]
fn replace_with_primitives_offsets_the_primitive() {
  let mut s = Session::default();
  block(&mut s, 10.0, 10.0, 4.0);
  let b = s.model.bodies[0].id.0;
  run(&mut s, "replace_with_primitives", json!({"bodies": [b], "offset": "1 mm", "object_type": "bodies"}));
  assert!((volume(&s) - 12.0 * 12.0 * 6.0).abs() < 1e-6, "{}", volume(&s));
}

/// Fusion's Plane at Angle Offset row moves the plane along its normal (an older plane, with no
/// offset, keeps its place); Point At Center takes a circular edge as well as a round face.
#[test]
fn angle_plane_offset_and_point_at_a_circular_edges_centre() {
  let mut s = Session::default();
  let plane = |s: &Session, v: &Value| s.model.planes[&crate::model::FeatureId(v["feature"].as_u64().unwrap() as u32)];
  let a = run(&mut s, "plane", json!({"method": "angle", "axis": "x", "plane": "xy", "angle": "90 deg"}));
  let b =
    run(&mut s, "plane", json!({"method": "angle", "axis": "x", "plane": "xy", "angle": "90 deg", "offset": "5 mm"}));
  let (pa, pb) = (plane(&s, &a), plane(&s, &b));
  assert!(pa.normal().y.abs() > 0.999 && pa.origin.length() < 1e-9, "{pa:?}");
  assert!(((pb.origin - pa.origin).dot(pb.normal()) - 5.0).abs() < 1e-9, "{pb:?}");
  let saved = |v: &Value| {
    let f = crate::model::FeatureId(v["feature"].as_u64().unwrap() as u32);
    serde_json::to_string(&s.design.feature(f).unwrap().kind).unwrap()
  };
  assert!(saved(&b).contains(r#""offset":"5 mm""#), "{}", saved(&b));
  assert!(!saved(&a).contains("offset"), "no offset written when there is none: {}", saved(&a));

  run(&mut s, "cylinder", json!({"radius": 5, "height": 20}));
  let b = &s.model.bodies[0];
  let rim = b.edges().iter().find(|e| e.kind == fox_kernel::CurveKind::Circle && e.center.z > 1.0).unwrap();
  let (body, edge, centre) = (b.id.0, rim.id, rim.center);
  let p = run(&mut s, "point", json!({"method": "center", "edge": {"body": body, "edge": edge}}));
  let q = s.model.points[&crate::model::FeatureId(p["feature"].as_u64().unwrap() as u32)];
  assert!((q - centre).length() < 1e-9, "{q:?}");
}

/// MESH > MODIFY on the inserted `lprism.stl` (an L-shaped prism, 2000 mm³): Remesh, Shell,
/// Scale Mesh, Combine and Separate.
#[test]
fn mesh_remesh_shell_combine_separate_and_scale() {
  let stl = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/reference/464-stl-millimeter/lprism.stl");
  let insert = |s: &mut Session| {
    run(s, "import", json!({"path": stl}));
    s.model.bodies.last().unwrap().id.0
  };
  let tris = |s: &Session, b: u32| s.model.body(fox_model::BodyId(b)).unwrap().mesh().unwrap().indices.len() / 3;
  // Remesh: denser, the same solid; Uniform makes more faces than Adaptive on flat sides
  let mut s = Session::default();
  let b = insert(&mut s);
  let before = tris(&s, b);
  run(&mut s, "mesh.remesh", json!({"bodies": [b], "type": "uniform", "density": 0.25, "sharp": true}));
  let uniform = tris(&s, b);
  assert!(uniform > before * 4, "{before} → {uniform}");
  assert!((volume(&s) - 2000.0).abs() < 1.0, "{}", volume(&s));
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"adaptive": true}}));
  assert!(tris(&s, b) < uniform);
  run(&mut s, "mesh.remesh", json!({"bodies": [b], "method": "edge_length", "edge_length": "2 mm"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(s.model.bodies[0].is_mesh(), "a remeshed mesh body stays a mesh body");
  assert_eq!(s.design.features.last().unwrap().name, "Remesh2");
  // Shell: a closed mesh keeps its outside and loses a 1 mm thick inside
  let mut s = Session::default();
  let b = insert(&mut s);
  run(&mut s, "mesh.shell", json!({"bodies": [b], "thickness": "1 mm"}));
  let v = volume(&s);
  assert!(v > 500.0 && v < 1800.0, "{v}");
  assert!((s.model.bodies[0].shape.mass().unwrap().area - 1300.0) > 300.0);
  // Scale Mesh about the body's centre
  let mut s = Session::default();
  let b = insert(&mut s);
  run(&mut s, "mesh.scale", json!({"bodies": [b], "factor": 2}));
  assert!((volume(&s) - 16000.0).abs() < 1e-3, "{}", volume(&s));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_min.x + 15.0).abs() < 1e-6 && (m.bbox_max.x - 45.0).abs() < 1e-6, "{m:?}");
  // Combine: Merge keeps both as one body, Separate splits it into its shells again
  let mut s = Session::default();
  let a = insert(&mut s);
  let b = insert(&mut s);
  run(&mut s, "move", json!({"bodies": [b], "translate": [100, 0, 0]}));
  assert!(s.execute("mesh.combine", &json!({"target": a, "tools": [a]})).is_err());
  run(&mut s, "mesh.combine", json!({"target": a, "tools": [b], "operation": "merge"}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - 4000.0).abs() < 1.0, "{}", volume(&s));
  run(&mut s, "mesh.separate", json!({"body": a}));
  assert_eq!(s.model.bodies.len(), 2);
  assert!(s.model.bodies.iter().all(|b| (b.shape.mass().unwrap().volume - 2000.0).abs() < 1.0));
  // the shells stay mesh bodies named after the one they came from
  let names: Vec<&str> = s.model.bodies.iter().map(|b| b.name.as_str()).collect();
  assert_eq!(names, ["lprism (1)", "lprism (2)"]);
  assert!(s.model.bodies.iter().all(|b| b.is_mesh()));
  // a single shell has nothing to separate
  let one = s.model.bodies[0].id.0;
  let r = s.execute("mesh.separate", &json!({"body": one, "keep_body": true})).unwrap();
  assert!(r["error"].as_str().is_some_and(|e| e.contains("single shell")), "{r}");
  // Combine Join of two overlapping copies, the tool kept
  let mut s = Session::default();
  let a = insert(&mut s);
  let b = insert(&mut s);
  run(&mut s, "move", json!({"bodies": [b], "translate": [0, 0, 2.5]}));
  run(&mut s, "mesh.combine", json!({"target": a, "tools": [b], "operation": "join", "keep_tools": true}));
  assert_eq!(s.model.bodies.len(), 2);
  assert!((s.model.body(fox_model::BodyId(a)).unwrap().shape.mass().unwrap().volume - 3000.0).abs() < 1.0);
}

/// `ui:sketch:vertical-on-horizontal-line`: Vertical on a line already constrained Horizontal
/// (or the reverse, or the same two points) is refused as a conflict and the line stays.
#[test]
fn vertical_on_a_horizontal_line_is_refused() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 3]]}));
  let line = r["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.constrain", json!({"type": "horizontal", "curves": [line]}));
  let before = run(&mut s, "sketch.info", json!({}));
  let e = s.execute("sketch.constrain", &json!({"type": "vertical", "curves": [line]})).unwrap_err();
  assert!(e.to_string().contains("conflicts"), "{e}");
  let after = run(&mut s, "sketch.info", json!({}));
  assert_eq!(before["geometry"], after["geometry"], "the line keeps its length");
  assert_eq!(before["dof"], after["dof"]);
  // the same through the line's two points, and a vertical line made horizontal
  let sk = s.edit_sketch.unwrap();
  let ends = s.design.sketch(sk).unwrap().curve_points(fox_sketch::CurveId(line as u32));
  let pts = [ends[1].0, ends[0].0];
  assert!(s.execute("sketch.constrain", &json!({"type": "vertical", "points": pts})).is_err());
  let r = run(&mut s, "sketch.line", json!({"points": [[40, 0], [41, 15]]}));
  let upright = r["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.constrain", json!({"type": "vertical", "curves": [upright]}));
  assert!(s.execute("sketch.constrain", &json!({"type": "horizontal", "curves": [upright]})).is_err());
  // Horizontal on the horizontal line again is not a conflict of this kind
  assert!(
    !s.design
      .sketch(sk)
      .unwrap()
      .contradicts(&fox_sketch::ConstraintKind::Horizontal { line: fox_sketch::CurveId(line as u32) })
  );
}

/// `ui:sketch:project-specified-edge`: Project with edges given projects those edges alone,
/// linked to them (they follow the body), rather than the body's silhouette.
#[test]
fn project_specified_edges_alone() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 30, "depth": 20, "height": 10}));
  let body = &s.model.bodies[0];
  let b = body.id.0;
  // the top face's edge along x at the back, and a vertical edge
  let top_back = body
    .edges()
    .iter()
    .find(|e| e.start.z > 9.9 && e.end.z > 9.9 && (e.start.y - e.end.y).abs() < 1e-9 && e.start.y > 1.0)
    .unwrap()
    .id;
  let upright = body.edges().iter().find(|e| (e.start.z - e.end.z).abs() > 9.9).unwrap().id;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 25}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  let r = run(&mut s, "sketch.project", json!({"body": b, "edges": [top_back]}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 1, "that edge alone: {r}");
  let sk = s.edit_sketch.unwrap();
  let links = match &s.design.feature(sk).unwrap().kind {
    fox_model::FeatureKind::Sketch { links, .. } => links.len(),
    _ => 0,
  };
  assert_eq!(links, 1, "linked to the edge");
  let info = run(&mut s, "sketch.info", json!({}));
  let length = info["geometry"][0]["length"].as_f64().unwrap();
  assert!((length - 30.0).abs() < 1e-6, "{info}");
  // an edge along the sketch normal projects to a point: nothing to add, but no silhouette either
  let r = run(&mut s, "sketch.project", json!({"body": b, "edges": [upright]}));
  assert!(r["curves"].as_array().unwrap().is_empty(), "{r}");
}
