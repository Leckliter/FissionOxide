//! Tests for form (T-spline) bodies: box and raw cages, and pulling a cage vertex.

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

#[test]
fn form_box_is_a_smooth_solid_with_a_face_per_side() {
  let mut s = Session::default();
  run(&mut s, "form.box", json!({"length": 80, "width": 60, "height": "20 mm"}));
  assert_eq!(s.model.bodies.len(), 1);
  let b = &s.model.bodies[0];
  assert!(b.shape.is_valid());
  assert_eq!(b.faces().len(), 6);
  let m = b.shape.mass().unwrap();
  // the Catmull-Clark limit of the cage: rounded, touching the cage's sides at their centres
  assert!((m.volume - 75_538.0).abs() < 20.0, "{}", m.volume);
  assert!((m.bbox_max.z - 20.0).abs() < 1e-3 && (m.bbox_max.x - 40.0).abs() < 1e-3, "{m:?}");

  // pulling the top centre vertex up raises a dome 4/9 as high
  let r = run(&mut s, "form.move_vertex", json!({"point": [0, 0, 20], "by": [0, 0, 20]}));
  assert!(r["vertex"].is_u64(), "{r}");
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_max.z - (20.0 + 20.0 * 4.0 / 9.0)).abs() < 1e-3, "{m:?}");
  assert!(m.volume > 90_000.0, "{}", m.volume);
  s.execute("undo", &json!({})).unwrap();
  assert!((volume(&s) - 75_538.0).abs() < 20.0);
}

#[test]
fn form_cage_on_a_plane() {
  // a cube cage: every vertex is extraordinary (valence 3), so each cage face is a face
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  let v: Vec<[f64; 3]> = (0..8)
    .map(|i| [if i & 1 == 0 { 0.0 } else { 10.0 }, if i & 2 == 0 { 0.0 } else { 10.0 }, (i >> 2) as f64 * 10.0])
    .collect();
  let faces = json!([[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]]);
  run(&mut s, "form.cage", json!({"plane": "xz", "vertices": v, "faces": faces}));
  let b = &s.model.bodies[0];
  assert!(b.shape.is_valid());
  assert_eq!(b.faces().len(), 6);
  let m = b.shape.mass().unwrap();
  assert!(m.volume > 100.0 && m.volume < 1000.0, "{}", m.volume);
  // the cage's z is the XZ plane's normal (-Y)
  assert!(m.bbox_max.y < 0.0 && m.bbox_min.y > -10.0, "{m:?}");

  // open or inconsistently wound cages are refused
  let open = json!({"vertices": v, "faces": [[0, 2, 3, 1], [4, 5, 7, 6]]});
  assert!(s.execute("form.cage", &open).is_err());
  let flipped = json!({"vertices": v, "faces": [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]]});
  assert!(s.execute("form.cage", &flipped).is_err());
  assert!(s.execute("form.box", &json!({"length": 10, "width": 10, "height": 10, "length_faces": 0})).is_err());
}
