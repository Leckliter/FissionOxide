//! Tests for the viewport scene: vertex picks and joint glyphs.

use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::scene::{self, Pick, SceneBuilder, ViewOptions};
use crate::{Sel, Session};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

#[test]
fn vertex_picks_show_only_with_the_filter_and_measure_as_points() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
  let body = &s.model.bodies[0];
  let verts = scene::body_vertices(body);
  assert_eq!(verts.len(), 8, "{verts:?}");
  let id = body.id;

  let mut b = SceneBuilder::default();
  let (plain, picks) = b.build(&s, &ViewOptions::default());
  let vertex_ids =
    |picks: &scene::PickTable, n: u32| (1..=n).filter(|i| matches!(picks.get(*i), Some(Pick::Vertex { .. }))).count();
  assert_eq!(vertex_ids(&picks, 10_000), 0);
  let (with, picks) = b.build(&s, &ViewOptions { show_vertices: true, ..Default::default() });
  assert_eq!(with.markers.len(), plain.markers.len() + 8);
  assert_eq!(vertex_ids(&picks, 10_000), 8);

  // a selected vertex round-trips through the selection and measures as a point
  let (edge, end, at) = verts[3];
  let sel = Sel::Vertex { body: id, edge, end };
  assert_eq!(Pick::from_sel(&sel).map(Pick::to_sel), Some(sel));
  assert_eq!(scene::vertex_pos(&s, id, edge, end), Some(at));
  s.selection = vec![sel];
  let m = run(&mut s, "measure", json!({}));
  let p: Vec<f64> = serde_json::from_value(m["at"].clone()).unwrap_or_default();
  assert!(p.len() == 3 && (DVec3::from_slice(&p) - at).length() < 1e-9, "{m}");
}

#[test]
fn joint_marks_sit_on_saved_origins_and_fixed_sides() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
  let base = s.model.bodies.last().unwrap().id.0;
  run(&mut s, "component.create", json!({"name": "Base", "bodies": [base]}));
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 5, "at": [0, 200]}));
  let arm = s.model.bodies.last().unwrap().id.0;
  let arm_c = run(&mut s, "component.create", json!({"name": "Arm", "bodies": [arm]}))["component"].clone();
  run(&mut s, "joint_origin", json!({"component": arm_c, "at": [0, 200, 0], "name": "Foot"}));
  run(
    &mut s,
    "joint",
    json!({"moving_origin": {"origin": "Foot"}, "fixed_origin": {"at": [50, 30, 10]}, "offset": "2 mm"}),
  );
  let marks = scene::joint_marks(&s);
  assert_eq!(marks.len(), 2, "{marks:?}");
  let origin = marks.iter().find(|m| !m.joint).unwrap();
  let joint = marks.iter().find(|m| m.joint).unwrap();
  // the joint sits on its fixed side, lifted by its offset; the saved origin moved with the arm
  assert!((joint.frame.w_axis.truncate() - DVec3::new(50.0, 30.0, 12.0)).length() < 1e-6, "{joint:?}");
  assert!((origin.frame.w_axis.truncate() - DVec3::new(50.0, 30.0, 12.0)).length() < 1e-6, "{origin:?}");
}

#[test]
fn named_views_save_restore_and_round_trip_through_a_file() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
  let cam =
    json!({"target": [1.0, 2.0, 3.0], "yaw": 0.25, "pitch": -0.5, "distance": 150.0, "fov": 0.4, "ortho": true});
  let r = run(&mut s, "view.named.save", cam.clone());
  assert_eq!(r["name"], "Named View 1");
  let mut named = cam.clone();
  named["name"] = json!("Detail");
  run(&mut s, "view.named.save", named);
  assert_eq!(run(&mut s, "view.named.save", cam.clone())["name"], "Named View 2");
  // standard names and duplicates are refused; so are bad cameras
  let top = json!({"name": "Top", "target": [0, 0, 0], "yaw": 0, "pitch": 0, "distance": 1});
  assert!(s.execute("view.named.save", &top).is_err());
  assert!(s.execute("view.named.rename", &json!({"name": "Detail", "to": "named view 1"})).is_err());
  assert!(s.execute("view.named.save", &json!({"target": [0, 0, 0], "yaw": 0, "pitch": 0, "distance": 0})).is_err());

  run(&mut s, "view.named.rename", json!({"name": "Named View 2", "to": "Back corner"}));
  run(&mut s, "view.named.delete", json!({"name": "Named View 1"}));
  let names: Vec<String> = s.design.named_views.iter().map(|v| v.name.clone()).collect();
  assert_eq!(names, ["Detail", "Back corner"]);
  let got = run(&mut s, "view.named.get", json!({"name": "detail"}));
  assert_eq!(got["target"], json!([1.0, 2.0, 3.0]));
  assert_eq!(got["yaw"], json!(0.25));
  assert_eq!(got["ortho"], json!(true));

  let dir = std::env::temp_dir().join(format!("fox-named-views-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("views.fox");
  s.save_to(&path).unwrap();
  let back = Session::open(&path).unwrap();
  assert_eq!(back.design.named_views, s.design.named_views);
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn face_appearances_draw_over_the_body_and_clear() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
  let body = s.model.bodies[0].id;
  let face = s.model.bodies[0].faces()[0].id;
  let copper = [190, 112, 72, 255];
  let colors = |s: &Session| -> Vec<[u8; 4]> {
    let (scene, _) = SceneBuilder::default().build(s, &ViewOptions::default());
    scene.meshes.iter().map(|m| m.color).collect()
  };
  assert!(!colors(&s).contains(&copper));
  let one = json!([{"body": body.0, "face": face}]);
  run(&mut s, "face.appearance", json!({"faces": one, "appearance": "Copper"}));
  assert_eq!(s.design.face_appearances.len(), 1);
  let c = colors(&s);
  assert!(c.contains(&copper) && c.contains(&scene::BODY_COLOR), "{c:?}");
  // a second appearance on the same face replaces the first
  run(&mut s, "face.appearance", json!({"faces": one, "color": "#ff0000"}));
  assert_eq!(s.design.face_appearances.len(), 1);
  assert!(colors(&s).contains(&[255, 0, 0, 255]));
  // the body's appearance stays under it
  run(&mut s, "body.appearance", json!({"body": body.0, "appearance": "Copper"}));
  let c = colors(&s);
  assert!(c.contains(&[255, 0, 0, 255]) && c.contains(&copper), "{c:?}");
  run(&mut s, "face.appearance", json!({"faces": one}));
  assert!(s.design.face_appearances.is_empty());
  assert!(s.execute("face.appearance", &json!({"faces": one, "appearance": "Unobtainium"})).is_err());
}
