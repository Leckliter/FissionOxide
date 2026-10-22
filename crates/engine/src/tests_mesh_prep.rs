//! Tests for MESH > Tessellate, Repair, Generate and Combine Face Groups, Create Mesh Section
//! Sketch and Direct Mesh Editing's start and finish.

use fox_model::{BodyId, FeatureKind};
use serde_json::{Value, json};

use crate::Session;
use crate::mesh_prep::is_mesh_body;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// A session with the reference L-shaped prism (30 × 20 × 5 mm, 20 triangles) inserted.
fn lprism() -> (Session, BodyId) {
  let mut s = Session::default();
  let path =
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference/464-stl-millimeter/lprism.stl");
  run(&mut s, "import", json!({"path": path}));
  let b = s.model.bodies[0].id;
  (s, b)
}

#[test]
fn tessellate_turns_a_solid_into_a_mesh_body_to_its_refinement() {
  let mut s = Session::default();
  run(&mut s, "cylinder", json!({"radius": 10, "height": 5}));
  let b = s.model.bodies[0].id;
  assert!(!is_mesh_body(&s, b), "a solid isn't a mesh body");
  let v0 = s.model.bodies[0].shape.mass().unwrap().volume;
  let r = run(&mut s, "mesh.tessellate", json!({"body": b.0}));
  assert_eq!(r["name"], "Tessellate1");
  assert!(is_mesh_body(&s, b), "the tessellated body is a mesh body");
  let medium = s.model.bodies[0].faces().len();
  assert!(medium > 20, "triangles for faces: {medium}");
  let v1 = s.model.bodies[0].shape.mass().unwrap().volume;
  assert!((v1 - v0).abs() / v0 < 0.02, "{v0} → {v1}");
  // Low makes fewer triangles than High
  s.undo();
  run(&mut s, "mesh.tessellate", json!({"body": b.0, "refinement": "high"}));
  let high = s.model.bodies[0].faces().len();
  s.undo();
  run(&mut s, "mesh.tessellate", json!({"body": b.0, "refinement": "low"}));
  let low = s.model.bodies[0].faces().len();
  assert!(low < high, "low {low}, high {high}");
  // a custom maximum edge splits the long edges
  s.undo();
  run(&mut s, "mesh.tessellate", json!({"body": b.0, "refinement": "custom", "max_edge": "2 mm"}));
  let m = s.model.bodies[0].mesh().unwrap().clone();
  let (pos, tris) = fox_model::meshops::weld(&m);
  let longest = tris
    .chunks_exact(3)
    .flat_map(|t| (0..3).map(move |k| (t[k], t[(k + 1) % 3])))
    .map(|(a, c)| (pos[a as usize] - pos[c as usize]).length())
    .fold(0.0, f64::max);
  assert!(longest <= 2.0 + 1e-3, "longest edge {longest}");
  assert!(s.execute("mesh.tessellate", &json!({"body": b.0, "refinement": "fine"})).is_err());
}

#[test]
fn repair_closes_a_holed_mesh_and_quick_analysis_sees_it() {
  let (mut s, b) = lprism();
  assert!(is_mesh_body(&s, b), "an inserted STL is a mesh body");
  let a = run(&mut s, "mesh.analyze", json!({"body": b.0}));
  assert_eq!(a["clean"], true, "{a}");
  // a mesh with a face missing: open edges, one hole
  let mut s2 = Session::default();
  let dir = std::env::temp_dir().join(format!("fox-repair-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let text = std::fs::read_to_string(
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference/464-stl-millimeter/lprism.stl"),
  )
  .unwrap();
  // drop the first facet (a bottom triangle)
  let start = text.find(" facet").unwrap();
  let end = text.find("endfacet").unwrap() + "endfacet".len();
  let holed = format!("{}{}", &text[..start], &text[end..]);
  std::fs::write(dir.join("holed.stl"), holed).unwrap();
  run(&mut s2, "import", json!({"path": dir.join("holed.stl")}));
  let h = s2.model.bodies[0].id;
  let a = run(&mut s2, "mesh.analyze", json!({"body": h.0}));
  assert_eq!(a["clean"], false, "{a}");
  assert_eq!(a["holes"], 1, "{a}");
  let r = run(&mut s2, "mesh.repair", json!({"bodies": [h.0], "type": "close_holes"}));
  assert_eq!(r["name"], "MeshRepair1");
  let a = run(&mut s2, "mesh.analyze", json!({"body": h.0}));
  assert_eq!(a["clean"], true, "{a}");
  assert_eq!(s2.model.bodies[0].shape.solids().unwrap().len(), 1, "the closed mesh is a solid again");
  // every repair type runs; an unknown one is refused
  for t in ["stitch_and_remove", "wrap", "one_touch_fix", "rebuild"] {
    run(&mut s, "mesh.repair", json!({"bodies": [b.0], "type": t}));
  }
  assert!(s.execute("mesh.repair", &json!({"bodies": [b.0], "type": "glue"})).is_err());
}

#[test]
fn face_groups_follow_the_prism_faces_and_combine() {
  let (mut s, b) = lprism();
  let r = run(&mut s, "mesh.face_groups", json!({"bodies": [b.0]}));
  assert_eq!(r["name"], "GenerateFaceGroups1");
  // two L ends and six sides
  assert_eq!(r["face_groups"][0]["groups"], 8, "{r}");
  // two side faces meeting at an edge
  let body = s.model.body(b).unwrap();
  let x0 = body.faces().iter().find(|f| f.normal.x < -0.9).map(|f| f.id).unwrap();
  let y0 = body.faces().iter().find(|f| f.normal.y < -0.9).map(|f| f.id).unwrap();
  let r =
    run(&mut s, "mesh.combine_face_groups", json!({"faces": [{"body": b.0, "face": x0}, {"body": b.0, "face": y0}]}));
  assert_eq!(r["name"], "CombineFaceGroups1");
  assert_eq!(r["face_groups"][0]["groups"], 7, "{r}");
  assert_eq!(s.model.face_group(b, x0), s.model.face_group(b, y0));
  // the timeline items
  let kinds: Vec<&str> = s.design.features.iter().map(|f| f.kind.base_name()).collect();
  assert!(kinds.ends_with(&["GenerateFaceGroups", "CombineFaceGroups"]), "{kinds:?}");
}

#[test]
fn a_mesh_section_sketch_holds_the_outline_where_the_plane_cuts() {
  let (mut s, b) = lprism();
  let n0 = s.design.features.len();
  let r = run(&mut s, "mesh.section_sketch", json!({"body": b.0, "plane": "xy", "offset": "2.5 mm"}));
  let sk = fox_model::FeatureId(r["sketch"].as_u64().unwrap() as u32);
  let curves = s.design.sketch(sk).unwrap().curves.len();
  assert!(curves >= 6, "the L outline: {curves} curves");
  assert!(s.edit_sketch.is_none(), "the sketch isn't left open");
  assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::Sketch { .. }));
  // one undo takes the plane and the sketch away
  s.undo();
  assert_eq!(s.design.features.len(), n0);
  // a plane that misses the body is an error, and leaves nothing behind
  assert!(s.execute("mesh.section_sketch", &json!({"body": b.0, "plane": "xy", "offset": "50 mm"})).is_err());
  assert_eq!(s.design.features.len(), n0);
}

#[test]
fn direct_mesh_editing_leaves_an_edited_copy_in_a_base_feature() {
  let (mut s, b) = lprism();
  let r = run(&mut s, "mesh.edit.start", json!({}));
  let fid = r["feature"].as_u64().unwrap();
  assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::BaseFeature { .. }));
  let r = run(&mut s, "mesh.edit.finish", json!({"feature": fid, "body": b.0}));
  let edited = BodyId(r["body"].as_u64().unwrap() as u32);
  let e = s.model.body(edited).unwrap();
  assert_eq!(e.name, format!("{}_Edited", s.model.body(b).unwrap().name));
  assert!(is_mesh_body(&s, edited));
  assert!((e.shape.mass().unwrap().volume - 2000.0).abs() < 1e-3);
  // Create Base Mesh Feature finishes without a body
  let r = run(&mut s, "mesh.edit.start", json!({}));
  let r = run(&mut s, "mesh.edit.finish", json!({"feature": r["feature"]}));
  assert!(r["body"].is_null());
}
