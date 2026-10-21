//! Tests for form (T-spline) bodies: primitives, cage edits through the commands, conversion to
//! valid solids and surfaces, pull, symmetry, and saving.

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

  // inconsistently wound or pinched cages are refused; open ones make surfaces
  let pinched = json!({"vertices": v, "faces": [[0, 2, 3, 1], [3, 7, 6, 4]]});
  assert!(s.execute("form.cage", &pinched).is_err());
  let open = json!({"vertices": v, "faces": [[0, 2, 3, 1], [0, 1, 5, 4]]});
  run(&mut s, "form.cage", open);
  assert_eq!(s.model.bodies.len(), 2);
  assert_eq!(s.model.bodies[1].shape.solid_count(), 0);
  let flipped = json!({"vertices": v, "faces": [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]]});
  assert!(s.execute("form.cage", &flipped).is_err());
  assert!(s.execute("form.box", &json!({"length": 10, "width": 10, "height": 10, "length_faces": 0})).is_err());
}

fn info(s: &mut Session) -> Value {
  run(s, "form.info", json!({}))
}

#[test]
fn round_primitives_convert_to_valid_solids() {
  for (cmd, p, volume) in [
    ("form.sphere", json!({"diameter": 40}), 4.0 / 3.0 * std::f64::consts::PI * 8000.0),
    ("form.quadball", json!({"diameter": 40, "faces": 3}), 4.0 / 3.0 * std::f64::consts::PI * 8000.0),
    ("form.torus", json!({"diameter": 60, "tube_diameter": 16}), 2.0 * std::f64::consts::PI.powi(2) * 30.0 * 64.0),
    ("form.cylinder", json!({"diameter": 20, "height": 30, "capped": true}), 0.0),
  ] {
    let mut s = Session::default();
    run(&mut s, cmd, p);
    let b = &s.model.bodies[0];
    assert!(b.shape.is_valid(), "{cmd}");
    assert_eq!(b.shape.solid_count(), 1, "{cmd}");
    let v = b.shape.mass().unwrap().volume;
    assert!(v > 0.0);
    if volume > 0.0 {
      assert!((v / volume - 1.0).abs() < 0.05, "{cmd}: {v} vs {volume}");
    }
  }
  // a plane is an open surface
  let mut s = Session::default();
  run(&mut s, "form.plane", json!({"length": 40, "width": 30, "length_faces": 3}));
  let b = &s.model.bodies[0];
  assert_eq!(b.shape.solid_count(), 0);
  assert!(b.shape.is_valid());
  let i = info(&mut s);
  assert_eq!(i["closed"], json!(false));
  assert_eq!(i["patches"], json!(1));
  let mut s = Session::default();
  run(&mut s, "form.face", json!({"points": [[0, 0, 0], [20, 0, 0], [20, 20, 5], [0, 20, 0]]}));
  assert_eq!(s.model.bodies[0].faces().len(), 1);
}

#[test]
fn sculpting_a_box_through_the_commands() {
  let mut s = Session::default();
  run(&mut s, "form.box", json!({"length": 60, "width": 60, "height": 20, "length_faces": 3, "width_faces": 3, "height_faces": 1}));
  let v0 = volume(&s);
  // pull the top's middle face up with a soft falloff
  let r = run(&mut s, "form.edit", json!({"faces": [[0, 0, 20]], "move": [0, 0, 15], "soft": 25}));
  assert!(r["moved"].as_u64().unwrap() > 4, "{r}");
  let v1 = volume(&s);
  assert!(v1 > v0 + 5000.0, "{v0} {v1}");
  // a new loop round the box's middle, then crease its top rim
  let r = run(&mut s, "form.insert_edge", json!({"edges": [[30, 10, 10]], "t": 0.5}));
  assert!(r["new_vertices"].as_array().unwrap().len() >= 12, "{r}");
  run(&mut s, "form.crease", json!({"edges": [[30, 0, 20], [-30, 0, 20]]}));
  assert_eq!(info(&mut s)["creases"], json!(2));
  // extrude a side face out
  run(&mut s, "form.extrude", json!({"faces": [[30, 0, 15]], "distance": 10}));
  let b = &s.model.bodies[0];
  assert!(b.shape.is_valid());
  assert_eq!(b.shape.solid_count(), 1);
  let m = b.shape.mass().unwrap();
  assert!(m.bbox_max.x > 35.0, "{m:?}");
  // the B-rep matches the limit surface's volume
  let cage = match &s.design.features.last().unwrap().kind {
    fox_model::FeatureKind::Form { cage, .. } => cage.clone(),
    _ => unreachable!(),
  };
  let (pos, quads) = fox_form::subd::limit_mesh(&cage, 4).unwrap();
  let mesh_v: f64 = quads
    .iter()
    .map(|q| {
      let [a, b, c, d] = q.map(|i| pos[i as usize]);
      (a.dot(b.cross(c)) + a.dot(c.cross(d))) / 6.0
    })
    .sum();
  assert!((m.volume / mesh_v - 1.0).abs() < 0.01, "{} vs {mesh_v}", m.volume);
  // subdivide, then display as the box cage (Alt+1)
  let faces = info(&mut s)["faces"].as_u64().unwrap();
  run(&mut s, "form.subdivide", json!({"faces": [[0, 0, 35]]}));
  assert_eq!(info(&mut s)["faces"].as_u64().unwrap(), faces + 3);
  run(&mut s, "form.display", json!({"mode": "box"}));
  assert_eq!(info(&mut s)["display"], json!("box"));
  assert!(s.model.bodies[0].shape.is_valid());
  // each edit is one undo step
  s.execute("undo", &json!({})).unwrap();
  s.execute("undo", &json!({})).unwrap();
  assert_eq!(info(&mut s)["faces"].as_u64().unwrap(), faces);
}

#[test]
fn holes_bridges_and_symmetry() {
  let mut s = Session::default();
  run(&mut s, "form.box", json!({"length": 40, "width": 20, "height": 20, "length_faces": 2, "width_faces": 1, "height_faces": 1}));
  // cut it in half on x = 0, mirror it back as one body that stays symmetric
  run(&mut s, "form.delete", json!({"faces": [[-10, 0, 0], [-10, 0, 20], [-10, -10, 10], [-10, 10, 10], [-20, 0, 10]]}));
  assert_eq!(info(&mut s)["closed"], json!(false));
  assert_eq!(s.model.bodies[0].shape.solid_count(), 0);
  run(&mut s, "form.mirror_duplicate", json!({"plane": "yz"}));
  let i = info(&mut s);
  assert_eq!(i["closed"], json!(true));
  assert_eq!(i["symmetry"]["kind"], json!("mirror"));
  let v0 = volume(&s);
  run(&mut s, "form.edit", json!({"vertices": [[20, 10, 20]], "move": [5, 0, 5]}));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_max.x + m.bbox_min.x).abs() < 1e-3, "not symmetric: {m:?}");
  assert!(volume(&s) > v0);

  // open the top, then fill it again with a star
  let mut s = Session::default();
  run(&mut s, "form.cylinder", json!({"diameter": 20, "height": 30}));
  assert_eq!(info(&mut s)["open_edges"], json!(16));
  run(&mut s, "form.fill_hole", json!({"edge": [10, 0, 30], "mode": "star"}));
  run(&mut s, "form.fill_hole", json!({"edge": [10, 0, 0], "mode": "polygon"}));
  assert_eq!(info(&mut s)["closed"], json!(true));
  assert_eq!(s.model.bodies[0].shape.solid_count(), 1);
  assert!(s.model.bodies[0].shape.is_valid());
}

#[test]
fn pull_a_form_onto_a_solid() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 100, "depth": 100, "height": 10}));
  run(&mut s, "form.plane", json!({"length": 40, "width": 40, "at": [0, 0, 25]}));
  let r = run(&mut s, "form.pull", json!({"vertices": [[0, 0, 25], [20, 20, 25], [-20, 20, 25]]}));
  assert!(r["gap"].as_f64().unwrap() < 1e-3, "{r}");
  let cage = match &s.design.features.last().unwrap().kind {
    fox_model::FeatureKind::Form { cage, .. } => cage.clone(),
    _ => unreachable!(),
  };
  let (lim, _) = fox_form::subd::limit_mesh(&cage, 1).unwrap();
  let centre = cage.nearest_vertex([0.0, 0.0, 0.0]).unwrap();
  // the box (centred on the origin, base on z = 0) has its top at z = 10
  assert!((lim[centre].z - 10.0).abs() < 1e-3, "{:?}", lim[centre]);
  run(&mut s, "form.make_uniform", json!({}));
  run(&mut s, "form.flip", json!({}));
  run(&mut s, "form.weld", json!({}));
}

#[test]
fn forms_save_and_load_with_their_cages() {
  let dir = std::env::temp_dir().join(format!("fox-form-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("form.fox");
  let mut s = Session::default();
  run(&mut s, "form.quadball", json!({"diameter": 30}));
  run(&mut s, "form.crease", json!({"edges": [[15, 0, 0]]}));
  run(&mut s, "form.symmetry", json!({"mirror": ["yz", "xz"]}));
  let before = info(&mut s);
  run(&mut s, "save", json!({"path": path}));
  let mut t = Session::default();
  run(&mut t, "open", json!({"path": path}));
  let after = info(&mut t);
  assert_eq!(before["creases"], after["creases"]);
  assert_eq!(before["symmetry"], after["symmetry"]);
  assert!((volume(&s) - volume(&t)).abs() < 1e-6);
  std::fs::remove_dir_all(&dir).ok();
}
