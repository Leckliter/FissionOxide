//! Tests for the viewport scene: vertex picks, joint glyphs and object visibility.

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

#[test]
fn object_visibility_and_grid_spacing() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 30}));
  let mut b = SceneBuilder::default();
  let lines = |b: &mut SceneBuilder, o: &ViewOptions| b.build(&s, o).0.lines;
  let dashed = |l: &[fox_render::Polyline]| l.iter().filter(|l| l.dashed).count();
  let plain = lines(&mut b, &ViewOptions::default());
  assert!(dashed(&plain) > 0, "the offset plane is drawn");
  let no_construction = lines(&mut b, &ViewOptions { hide_construction: true, ..Default::default() });
  assert_eq!(dashed(&no_construction), 0);

  // the origin axes are short stubs from the origin (Fusion draws no lines across the grid),
  // reaching as far as the UI asks (its origin planes' size), else twice the grid step
  let axis = |l: &fox_render::Polyline| l.color == scene::ORIGIN_X_COLOR;
  let x_axis: Vec<_> = plain.iter().filter(|l| axis(l)).collect();
  assert_eq!(x_axis.len(), 1);
  let step = scene::grid_step(&s) as f32;
  assert_eq!(x_axis[0].points, vec![[0.0, 0.0, 0.0], [2.0 * step, 0.0, 0.0]]);
  let sized = lines(&mut b, &ViewOptions { origin_axis_len: Some(7.5), ..Default::default() });
  let stub = sized.iter().find(|l| l.color == scene::ORIGIN_Z_COLOR).unwrap();
  assert_eq!(stub.points, vec![[0.0, 0.0, 0.0], [0.0, 0.0, 7.5]]);
  let no_origin = lines(&mut b, &ViewOptions { hide_origin: true, ..Default::default() });
  assert_eq!(no_origin.iter().filter(|l| axis(l)).count(), 0);
  assert_eq!(no_origin.len(), plain.len() - 3);

  // fixed spacing: twice the step, about half the grid lines
  let grid = |l: &[fox_render::Polyline]| l.iter().filter(|l| l.behind).count();
  let step = scene::grid_step(&s);
  let wide = lines(&mut b, &ViewOptions { grid_step: Some(step * 2.0), ..Default::default() });
  assert!(grid(&wide) < grid(&plain) * 2 / 3, "{} {}", grid(&wide), grid(&plain));
  let bad = lines(&mut b, &ViewOptions { grid_step: Some(0.0), ..Default::default() });
  assert_eq!(grid(&bad), grid(&plain), "a zero step adapts");

  // hidden sketches stay shown while one is edited
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [5, 5]}));
  let editing = b.build(&s, &ViewOptions { hide_sketches: true, ..Default::default() }).0;
  assert!(editing.lines.iter().any(|l| l.on_top), "the open sketch is drawn");
}

/// While a sketch is edited, its fully constrained curves draw black and the free ones blue,
/// as in Fusion.
#[test]
fn constrained_sketch_curves_draw_black_and_free_ones_blue() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let fixed = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}))["curves"][0].clone();
  let free = run(&mut s, "sketch.line", json!({"points": [[0, 5], [10, 8]]}))["curves"][0].clone();
  let circle = run(&mut s, "sketch.circle", json!({"center": [20, 0], "radius": 3}))["curves"][0].clone();
  let open = run(&mut s, "sketch.circle", json!({"center": [30, 0], "radius": 3}))["curves"][0].clone();
  let pinned = run(&mut s, "sketch.circle", json!({"center": [40, 0], "radius": 3}))["curves"][0].clone();
  run(&mut s, "sketch.constrain", json!({"type": "fix", "curves": [fixed, pinned]}));
  // a circle is fixed once its centre and its size are
  let sk = s.edit_sketch.unwrap();
  let centre = |s: &Session, c: &Value| {
    s.design.sketch(sk).unwrap().curve_points(fox_sketch::CurveId(c.as_u64().unwrap() as u32))[0].0
  };
  let (cc, oc) = (centre(&s, &circle), centre(&s, &open));
  run(&mut s, "sketch.constrain", json!({"type": "fix", "points": [cc]}));
  run(&mut s, "sketch.constrain", json!({"type": "fix", "points": [oc]}));
  run(&mut s, "sketch.dimension", json!({"type": "diameter", "curves": [circle], "value": "6"}));

  let (scene, picks) = SceneBuilder::default().build(&s, &ViewOptions::default());
  let colour = |c: &Value| {
    let curve = fox_sketch::CurveId(c.as_u64().unwrap() as u32);
    let l = scene.lines.iter().find(|l| picks.get(l.id) == Some(Pick::SketchCurve { sketch: sk, curve })).unwrap();
    l.color
  };
  assert_eq!(colour(&fixed), scene::SKETCH_FIXED);
  assert_eq!(colour(&free), scene::SKETCH_FREE);
  assert_eq!(colour(&circle), scene::SKETCH_FIXED);
  assert_eq!(colour(&open), scene::SKETCH_FREE, "its radius is still free");
  assert_eq!(colour(&pinned), scene::SKETCH_FIXED, "Fix holds a circle's radius too");
}

/// A sketch's closed profiles shade light blue seen from outside the sketch and pale blue-white
/// while it is edited, as Fusion's do; a selected profile fills blue.
#[test]
fn profiles_shade_blue_outside_and_pale_inside_the_sketch() {
  let mut s = Session::default();
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap() as u32;
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
  let mut b = SceneBuilder::default();
  let profile_color = |b: &mut SceneBuilder, s: &Session| {
    let (scene, picks) = b.build(s, &ViewOptions::default());
    let m = scene
      .meshes
      .iter()
      .find(|m| matches!(m.tri_ids.first().and_then(|id| picks.get(*id)), Some(scene::Pick::Profile { .. })))
      .expect("the profile is shaded");
    m.color
  };
  assert_eq!(profile_color(&mut b, &s), scene::PROFILE_COLOR_EDIT);
  run(&mut s, "sketch.finish", json!({}));
  assert_eq!(profile_color(&mut b, &s), scene::PROFILE_COLOR);
  s.selection = vec![crate::Sel::Profile { sketch: crate::model::FeatureId(sk), index: 0 }];
  assert_eq!(profile_color(&mut b, &s), scene::PROFILE_SELECTED);
}

/// A free sketch point draws as an open circle; a curve's ends draw filled, as do points two
/// curves share or a constraint ties (Fusion's look); the origin stays a filled dot.
#[test]
fn sketch_points_draw_hollow_until_joined() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[5, 5], [15, 5]]}));
  run(&mut s, "sketch.line", json!({"points": [[15, 5], [15, 15]]}));
  run(&mut s, "sketch.point", json!({"at": [20, 20]}));
  let sk = s.edit_sketch.unwrap();
  let joined = scene::joined_points(s.design.sketch(sk).unwrap());
  assert_eq!(joined.len(), 1, "{joined:?}");
  let (scene, picks) = SceneBuilder::default().build(&s, &ViewOptions::default());
  let points: Vec<_> =
    scene.markers.iter().filter(|m| matches!(picks.get(m.id), Some(Pick::SketchPoint { .. }))).collect();
  assert_eq!(points.len(), 5, "origin, three line ends, one point");
  assert_eq!(points.iter().filter(|m| m.hollow).count(), 1, "only the free point is open");
  assert!(points.iter().all(|m| m.color == scene::SKETCH_POINT || m.color == [200, 60, 40, 255]), "dark-grey dots");
}

/// In a sketch a selected curve draws wide in Fusion's selection blue and a hovered one wide in
/// its darker hover blue, while a plain curve keeps its width and colour; the canvas behind the
/// sketch is white, and goes back to the gradient when the sketch is finished.
#[test]
fn selected_and_hovered_sketch_curves_draw_wide_blue_on_a_white_canvas() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let id = |v: &Value| fox_sketch::CurveId(v["curves"][0].as_u64().unwrap() as u32);
  let picked = id(&run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]})));
  let hovered = id(&run(&mut s, "sketch.line", json!({"points": [[0, 5], [10, 5]]})));
  let plain = id(&run(&mut s, "sketch.line", json!({"points": [[0, 10], [10, 10]]})));
  let sk = s.edit_sketch.unwrap();
  s.selection = vec![Sel::SketchCurve { sketch: sk, curve: picked }];
  let opts = ViewOptions { hover: Some(Pick::SketchCurve { sketch: sk, curve: hovered }), ..Default::default() };
  let (scene, picks) = SceneBuilder::default().build(&s, &opts);
  let line = |curve| {
    let l = scene.lines.iter().find(|l| picks.get(l.id) == Some(Pick::SketchCurve { sketch: sk, curve })).unwrap();
    (l.color, l.width)
  };
  assert_eq!(line(picked), (scene::SKETCH_SELECTED, scene::SKETCH_CURVE_WIDTH_HOT));
  assert_eq!(line(hovered), (scene::SKETCH_HOVER, scene::SKETCH_CURVE_WIDTH_HOT));
  assert_eq!(line(plain), (scene::SKETCH_FREE, scene::SKETCH_CURVE_WIDTH));
  assert_eq!((scene.background_top, scene.background_bottom), (scene::SKETCH_BACKGROUND, scene::SKETCH_BACKGROUND));
  run(&mut s, "sketch.finish", json!({}));
  let (scene, _) = SceneBuilder::default().build(&s, &ViewOptions::default());
  assert_ne!(scene.background_top, scene::SKETCH_BACKGROUND, "the gradient is back outside the sketch");
}

/// A sketch point under the cursor draws as a hollow ring, a selected one as a light-blue disc
/// inside a dark ring (both a size larger than the plain dot), and a visible sketch's points are
/// still drawn and pickable outside the sketch.
#[test]
fn sketch_points_show_hover_ring_and_selected_fill_inside_and_outside_the_sketch() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let c = run(&mut s, "sketch.line", json!({"points": [[5, 5], [15, 5]]}))["curves"][0].as_u64().unwrap() as u32;
  let sk = s.edit_sketch.unwrap();
  let ends = s.design.sketch(sk).unwrap().curve_points(fox_sketch::CurveId(c));
  let (a, b) = (ends[0], ends[1]);
  s.selection = vec![Sel::SketchPoint { sketch: sk, point: a }];
  let opts = ViewOptions { hover: Some(Pick::SketchPoint { sketch: sk, point: b }), ..Default::default() };
  let (scene, picks) = SceneBuilder::default().build(&s, &opts);
  let marks = |scene: &fox_render::Scene, picks: &scene::PickTable, point| -> Vec<(f32, bool, [u8; 4])> {
    scene
      .markers
      .iter()
      .filter(|m| picks.get(m.id) == Some(Pick::SketchPoint { sketch: sk, point }))
      .map(|m| (m.size, m.hollow, m.color))
      .collect()
  };
  assert_eq!(
    marks(&scene, &picks, a),
    vec![(4.0, false, scene::SKETCH_POINT), (3.0, false, scene::SKETCH_POINT_SELECTED)],
    "a dark ring around a light-blue disc"
  );
  assert_eq!(marks(&scene, &picks, b), vec![(4.0, true, scene::SKETCH_POINT)], "a hollow ring");
  assert!(
    scene.markers.iter().any(|m| picks.get(m.id) == Some(Pick::SketchPoint { sketch: sk, point: fox_sketch::ORIGIN }))
  );
  // outside the sketch (still visible: nothing consumed it) the dots stay, pickable, without the origin
  s.selection.clear();
  run(&mut s, "sketch.finish", json!({}));
  let (scene, picks) = SceneBuilder::default().build(&s, &ViewOptions::default());
  assert_eq!(marks(&scene, &picks, a), vec![(3.0, false, scene::SKETCH_POINT)]);
  assert!(
    !scene.markers.iter().any(|m| picks.get(m.id) == Some(Pick::SketchPoint { sketch: sk, point: fox_sketch::ORIGIN }))
  );
}
