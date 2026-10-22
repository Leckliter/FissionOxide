//! Tests for mesh bodies: what Insert Mesh leaves (a mesh body named after the file) and how it
//! stays one until Convert Mesh.

use std::path::PathBuf;

use fox_model::regen::{BodyKind, MESH_NOT_CLOSED, MESH_NOT_POSITIVE};
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

/// The L-prism the MESH tab probe inserted in both apps (12 triangles, mm).
fn lprism() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference/464-stl-millimeter/lprism.stl")
}

/// An ASCII STL of a 10 mm cube's triangles, less the top two when `open`.
fn cube_stl(open: bool) -> String {
  let p = |i: usize| [(i & 1) as f32 * 10.0, ((i >> 1) & 1) as f32 * 10.0, ((i >> 2) & 1) as f32 * 10.0];
  // outward-facing triangles of the six faces (bottom, top, front, back, left, right)
  let mut tris = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3]];
  tris.extend([[0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]]);
  if open {
    tris.drain(2..4);
  }
  let mut out = String::from("solid cube\n");
  for t in tris {
    out += " facet normal 0 0 0\n  outer loop\n";
    for v in t {
      let [x, y, z] = p(v);
      out += &format!("   vertex {x} {y} {z}\n");
    }
    out += "  endloop\n endfacet\n";
  }
  out + "endsolid cube\n"
}

fn write_tmp(name: &str, text: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fox-meshbody-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join(name);
  std::fs::write(&path, text).unwrap();
  path
}

#[test]
fn inserted_stl_is_a_mesh_body_named_after_the_file() {
  let mut s = Session::default();
  let r = run(&mut s, "import", json!({"path": lprism()}));
  assert_eq!(s.model.bodies.len(), 1, "{r}");
  let b = &s.model.bodies[0];
  assert!(b.is_mesh());
  assert_eq!(b.kind, BodyKind::Mesh);
  assert_eq!(s.body_name(b.id), "lprism");
  assert!(b.mesh_problems().is_empty(), "a closed prism is sound: {:?}", b.mesh_problems());
  // the next solid is still Body1: a mesh body takes its file's name, not a number
  run(&mut s, "box", json!({"width": 5, "depth": 5, "height": 5}));
  let solid = s.model.bodies.iter().find(|b| !b.is_mesh()).unwrap();
  assert_eq!(s.body_name(solid.id), "Body1");
  assert!(solid.mesh_problems().is_empty());
  // the design summary and Properties tell the kinds apart
  let id = s.model.bodies[0].id.0;
  let props = run(&mut s, "inspect.properties", json!({"items": [{"type": "body", "body": id}]}));
  assert_eq!(props["items"][0]["kind"], "mesh", "{props}");
}

#[test]
fn mesh_tools_keep_a_mesh_body_and_convert_makes_it_a_solid() {
  let mut s = Session::default();
  run(&mut s, "import", json!({"path": lprism()}));
  let b = s.model.bodies[0].id.0;
  run(&mut s, "mesh.smooth", json!({"bodies": [b], "passes": 1}));
  assert!(s.model.bodies[0].is_mesh(), "Smooth leaves a mesh");
  assert_eq!(s.body_name(s.model.bodies[0].id), "lprism");
  // Convert Mesh makes a new B-rep body beside the mesh, which it hides (as Fusion does)
  run(&mut s, "mesh.convert", json!({"bodies": [b]}));
  assert_eq!(s.model.bodies.len(), 2);
  assert!(s.model.bodies[0].is_mesh() && !s.body_visible(s.model.bodies[0].id));
  assert!(!s.model.bodies[1].is_mesh(), "Convert Mesh makes a B-rep body");
}

#[test]
fn an_open_mesh_reports_fusions_warning_lines() {
  let mut s = Session::default();
  run(&mut s, "import", json!({"path": write_tmp("open.stl", &cube_stl(true))}));
  let b = &s.model.bodies[0];
  assert!(b.is_mesh());
  assert_eq!(s.body_name(b.id), "open");
  assert_eq!(b.mesh_problems(), [MESH_NOT_CLOSED, MESH_NOT_POSITIVE]);
  assert_eq!(MESH_NOT_CLOSED, "Mesh is not closed.");
  assert_eq!(MESH_NOT_POSITIVE, "Mesh does not have positive volume.");
  let mut t = Session::default();
  run(&mut t, "import", json!({"path": write_tmp("closed.stl", &cube_stl(false))}));
  assert!(t.model.bodies[0].mesh_problems().is_empty(), "{:?}", t.model.bodies[0].mesh_problems());
}
