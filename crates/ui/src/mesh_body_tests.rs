//! Mesh bodies in the browser and the canvas, driven without a screen: the row icon, whole-body
//! picking and the Mesh Bodies selection filter.

use fox_engine::scene::Pick;
use fox_engine::{Sel, Session};
use serde_json::json;

use crate::App;
use crate::icons::Icon;
use crate::select::{self, Priority};

/// The L-prism the MESH tab probe inserted in both apps.
fn lprism() -> std::path::PathBuf {
  std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference/464-stl-millimeter/lprism.stl")
}

/// A design holding the inserted L-prism (a mesh body) and a box (a solid).
pub(crate) fn mesh_and_box() -> App {
  let mut app = App::with_session(Session::default());
  app.run("import", json!({"path": lprism()}));
  app.run("box", json!({"width": 5, "depth": 5, "height": 5}));
  assert_eq!(app.s.model.bodies.len(), 2);
  app
}

fn mesh_body(app: &App) -> fox_engine::model::BodyId {
  app.s.model.bodies.iter().find(|b| b.is_mesh()).expect("a mesh body").id
}

fn solid_body(app: &App) -> fox_engine::model::BodyId {
  app.s.model.bodies.iter().find(|b| !b.is_mesh()).expect("a solid").id
}

#[test]
fn a_mesh_body_row_has_the_mesh_icon_and_the_files_name() {
  let app = mesh_and_box();
  let (m, b) = (mesh_body(&app), solid_body(&app));
  assert_eq!(crate::panels::body_icon(&app, m), Icon::MeshBody);
  assert_eq!(crate::panels::body_icon(&app, b), Icon::Body);
  assert_eq!(app.s.body_name(m), "lprism");
  assert_eq!(app.s.body_name(b), "Body1");
}

#[test]
fn a_click_on_a_mesh_body_picks_it_whole() {
  let mut app = mesh_and_box();
  let (m, b) = (mesh_body(&app), solid_body(&app));
  let face = Pick::Face { body: m, face: 0 };
  for priority in [Priority::Face, Priority::Edge, Priority::Body] {
    assert_eq!(select::targets(&app.s, face, priority), [Sel::Body { body: m }], "{priority:?}");
  }
  // a solid's face is still a face under the face priority
  assert_eq!(
    select::targets(&app.s, Pick::Face { body: b, face: 0 }, Priority::Face),
    [Sel::Face { body: b, face: 0 }]
  );
  select::pick(&mut app, face, false);
  assert_eq!(app.s.selection, [Sel::Body { body: m }]);
  // the pointer lights the whole body, not the face under it
  app.hover = Some(face);
  assert_eq!(select::hover_bodies(&app), [m]);
  app.hover = Some(Pick::Face { body: b, face: 0 });
  assert!(select::hover_bodies(&app).is_empty());
}

#[test]
fn the_mesh_bodies_filter_governs_mesh_picks() {
  let mut app = mesh_and_box();
  let (m, b) = (mesh_body(&app), solid_body(&app));
  let (on_mesh, on_solid) = (Pick::Face { body: m, face: 0 }, Pick::Face { body: b, face: 0 });
  let f = app.sel_filter;
  assert!(f.allows_on(&app.s, &on_mesh, Priority::Face) && f.allows_on(&app.s, &on_solid, Priority::Face));
  app.sel_filter.toggle("mesh_bodies");
  let f = app.sel_filter;
  assert!(!f.allows_on(&app.s, &on_mesh, Priority::Face), "Mesh Bodies off: the mesh can't be picked");
  assert!(f.allows_on(&app.s, &on_solid, Priority::Face));
  app.sel_filter.toggle("mesh_bodies");
  app.sel_filter.toggle("faces");
  assert!(app.sel_filter.allows_on(&app.s, &on_mesh, Priority::Face), "Body Faces off: the mesh still picks whole");
}

#[test]
fn a_mesh_body_draws_no_edges() {
  let app = mesh_and_box();
  let (m, b) = (mesh_body(&app), solid_body(&app));
  let mut cache = fox_engine::scene::SceneBuilder::default();
  let (scene, picks) = cache.build(&app.s, &fox_engine::scene::ViewOptions::default());
  let edge_of =
    |body| scene.lines.iter().any(|l| matches!(picks.get(l.id), Some(Pick::Edge { body: x, .. }) if x == body));
  assert!(!edge_of(m), "no face edges on a mesh body");
  assert!(edge_of(b), "a solid keeps its edges");
}

#[test]
fn insert_mesh_adds_base_mesh_feature1_with_the_mesh_feature_icon() {
  let app = mesh_and_box();
  let f = &app.s.design.features[0];
  assert_eq!(f.name, "Base Mesh Feature1");
  assert_eq!(crate::panels::feature_icon(&f.kind), Icon::MeshFeature);
  // a STEP-style import keeps its own name and icon
  let solid = &app.s.design.features[1];
  assert_ne!(crate::panels::feature_icon(&solid.kind), Icon::MeshFeature);
}

#[test]
fn the_browser_mesh_menu_is_the_canvas_list_without_find_in_browser() {
  let app = mesh_and_box();
  let m = mesh_body(&app);
  let labels = |canvas| -> Vec<String> {
    crate::panels::mesh_body_menu(&app, m, canvas).into_iter().flatten().map(|i| i.label).collect()
  };
  let (row, canvas) = (labels(false), labels(true));
  assert_eq!(canvas.iter().filter(|l| *l != "Find in Browser").cloned().collect::<Vec<_>>(), row);
  assert!(!row.iter().any(|l| l == "Create Components from Bodies" || l == "Isolate"));
  // separators as Fusion's groups them
  let groups: Vec<usize> =
    crate::panels::mesh_body_menu(&app, m, false).split(Option::is_none).map(<[_]>::len).collect();
  assert_eq!(groups, [3, 2, 4, 1, 5, 1, 1, 1, 1, 2]);
}

#[test]
fn an_open_mesh_row_warns_and_a_closed_one_does_not() {
  let mut app = mesh_and_box();
  assert!(crate::panels::body_warning(&app, mesh_body(&app)).is_empty());
  assert!(crate::panels::body_warning(&app, solid_body(&app)).is_empty());
  // a mesh with its top left open
  let dir = std::env::temp_dir().join(format!("fox-openmesh-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("tray.stl");
  let tri = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
    let v = |p: [f32; 3]| format!("   vertex {} {} {}\n", p[0], p[1], p[2]);
    format!(" facet normal 0 0 0\n  outer loop\n{}{}{}  endloop\n endfacet\n", v(a), v(b), v(c))
  };
  let (o, x, y, xy) = ([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [10.0, 10.0, 0.0]);
  let up = |p: [f32; 3]| [p[0], p[1], 5.0];
  let mut text = String::from("solid tray\n");
  for (a, b, c) in [(o, y, xy), (o, xy, x), (o, x, up(x)), (o, up(x), up(o)), (y, up(y), up(xy)), (y, up(xy), xy)] {
    text += &tri(a, b, c);
  }
  text += &tri(o, up(o), up(y));
  text += &tri(o, up(y), y);
  text += &tri(x, xy, up(xy));
  text += &tri(x, up(xy), up(x));
  std::fs::write(&path, text + "endsolid tray\n").unwrap();
  app.run("import", json!({"path": path}));
  let tray = app.s.model.bodies.iter().find(|b| b.is_mesh() && app.s.body_name(b.id) == "tray").unwrap().id;
  assert_eq!(crate::panels::body_warning(&app, tray), ["Mesh is not closed.", "Mesh does not have positive volume."]);
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn mesh_bodies_draw_smooth_unless_flat_shaded() {
  let mut app = mesh_and_box();
  let normals = |app: &App, m: fox_engine::model::BodyId, flat: Vec<fox_engine::model::BodyId>| -> usize {
    let opts = fox_engine::scene::ViewOptions { flat_meshes: flat, ..Default::default() };
    let (scene, picks) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
    let mesh = scene
      .meshes
      .iter()
      .find(|x| x.tri_ids.first().and_then(|id| picks.get(*id)).is_some_and(|p| select::pick_body(&p) == Some(m)))
      .unwrap();
    let mut seen: Vec<[i32; 3]> = mesh.normals.iter().map(|n| n.map(|c| (c * 100.0).round() as i32)).collect();
    seen.sort();
    seen.dedup();
    seen.len()
  };
  // the L-prism's edges are all 90 deg: smooth shading keeps its 8 flat faces and crisp edges
  let m = mesh_body(&app);
  let (smooth, flat) = (normals(&app, m, Vec::new()), normals(&app, m, vec![m]));
  assert!(flat <= 8 && smooth == flat, "flat {flat}, smooth {smooth}");
  // a tessellated cylinder is smooth round its side, its rims stay sharp
  app.run("cylinder", json!({"radius": 10, "height": 5}));
  let c = app.s.model.bodies.last().unwrap().id;
  app.run("mesh.tessellate", json!({"body": c.0}));
  assert!(app.s.model.body(c).unwrap().is_mesh());
  let (smooth, flat) = (normals(&app, c, Vec::new()), normals(&app, c, vec![c]));
  assert!(smooth > flat, "flat {flat}, smooth {smooth}");
  let opts = fox_engine::scene::ViewOptions::default();
  let (scene, picks) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
  let mesh = scene
    .meshes
    .iter()
    .find(|x| x.tri_ids.first().and_then(|id| picks.get(*id)).is_some_and(|p| select::pick_body(&p) == Some(c)))
    .unwrap();
  // every normal is the axis (a cap's) or square to it (the side's): none leans across a rim
  let crisp = |k: usize| mesh.normals.iter().all(|n| n[k].abs() > 0.999 || n[k].abs() < 1e-3);
  assert!((0..3).any(crisp), "a rim is rounded");
  crate::panels::run_mesh_cmd(&mut app, m, crate::panels::MeshCmd::FlatShaded);
  assert_eq!(app.ctx_menu.flat_shaded, [m]);
}
