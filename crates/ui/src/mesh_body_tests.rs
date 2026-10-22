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
