//! STEP assemblies: nested sub-assemblies and instanced parts as components and linked copies,
//! colours on single faces, both ways.

use std::path::{Path, PathBuf};

use fox_model::FeatureKind;
use serde_json::json;

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: serde_json::Value) -> serde_json::Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn temp_dir(name: &str) -> PathBuf {
  let d = std::env::temp_dir().join(format!("fox-engine-step-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&d);
  std::fs::create_dir_all(&d).unwrap();
  d
}

/// A hand-written AP214 file (not from OCCT): Cart with a grey Base (top face red) and an Axle
/// sub-assembly placed twice, the second turned 180° about Z; the Axle holds a Shaft and a blue
/// Wheel part placed twice.
fn cart() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/cart.step")
}

fn component<'a>(s: &'a Session, name: &str) -> &'a fox_model::Component {
  s.design.components.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{name} in {:?}", s.design.components))
}

fn parent_name(s: &Session, name: &str) -> Option<String> {
  component(s, name).parent.and_then(|p| s.design.component(p)).map(|c| c.name.clone())
}

fn copies(s: &Session) -> Vec<(String, String)> {
  s.design
    .features
    .iter()
    .filter_map(|f| match &f.kind {
      FeatureKind::CopyComponent { source, component, .. } => Some((
        s.design.component(component.unwrap()).unwrap().name.clone(),
        s.design.component(*source).unwrap().name.clone(),
      )),
      _ => None,
    })
    .collect()
}

/// Every body as (name, rounded centre of mass, rounded volume), sorted.
fn bodies(s: &Session) -> Vec<(String, [i64; 3], i64)> {
  let mut v: Vec<_> = s
    .model
    .bodies
    .iter()
    .map(|b| {
      let m = b.shape.mass().unwrap();
      let c = m.center_of_mass;
      (s.body_name(b.id), [c.x, c.y, c.z].map(|x| x.round() as i64), m.volume.round() as i64)
    })
    .collect();
  v.sort();
  v
}

fn color_of(s: &Session, name: &str) -> Vec<Option<[u8; 3]>> {
  s.model
    .bodies
    .iter()
    .filter(|b| s.body_name(b.id) == name)
    .map(|b| s.design.body_settings(b.id).and_then(|x| x.color))
    .collect()
}

/// The faces with an appearance of their own: (body name, face normal, colour).
fn face_colours(s: &Session) -> Vec<(String, [i64; 3], [u8; 3])> {
  let mut v: Vec<_> = crate::views::face_colors(s)
    .into_iter()
    .map(|(b, f, c, _)| {
      let body = s.model.body(b).unwrap();
      let n = body.faces().iter().find(|x| x.id == f).unwrap().normal;
      (s.body_name(b), [n.x, n.y, n.z].map(|x| x.round() as i64), c)
    })
    .collect();
  v.sort();
  v
}

fn check_cart(s: &Session) {
  let mut names: Vec<&str> = s.design.components.iter().map(|c| c.name.as_str()).collect();
  names.sort();
  assert_eq!(names, ["Axle:1", "Axle:2", "Wheel:1", "Wheel:2", "Wheel:3", "Wheel:4"]);
  assert_eq!(parent_name(s, "Axle:1"), None);
  assert_eq!(parent_name(s, "Axle:2"), None);
  assert_eq!(parent_name(s, "Wheel:1").as_deref(), Some("Axle:1"));
  assert_eq!(parent_name(s, "Wheel:2").as_deref(), Some("Axle:1"));
  assert_eq!(parent_name(s, "Wheel:3").as_deref(), Some("Axle:2"));
  assert_eq!(parent_name(s, "Wheel:4").as_deref(), Some("Axle:2"));
  // the second wheel and the whole second axle are linked copies
  let mut c = copies(s);
  c.sort();
  assert_eq!(
    c,
    [
      ("Axle:2".to_string(), "Axle:1".to_string()),
      ("Wheel:2".into(), "Wheel:1".into()),
      ("Wheel:3".into(), "Wheel:1".into()),
      ("Wheel:4".into(), "Wheel:2".into())
    ]
  );
  let shaft = |c: &str| component(s, c).bodies.iter().map(|b| s.body_name(*b)).collect::<Vec<_>>();
  assert_eq!(shaft("Axle:1"), ["Shaft"]);
  assert_eq!(shaft("Axle:2"), ["Shaft"]);
  assert_eq!(shaft("Wheel:4"), ["Wheel"]);
  // everything where the file put it (the second axle turned: its wheels swap sides)
  assert_eq!(
    bodies(s),
    [
      ("Base".to_string(), [0, 50, -5], 168000),
      ("Shaft".into(), [0, 0, 4], 1440),
      ("Shaft".into(), [0, 100, 4], 1440),
      ("Wheel".into(), [-50, 0, 8], 2560),
      ("Wheel".into(), [-50, 100, 8], 2560),
      ("Wheel".into(), [50, 0, 8], 2560),
      ("Wheel".into(), [50, 100, 8], 2560),
    ]
  );
  assert_eq!(color_of(s, "Wheel"), [Some([0, 64, 255]); 4]);
  assert_eq!(color_of(s, "Base"), [Some([128, 128, 128])]);
  assert_eq!(color_of(s, "Shaft"), [None, None]);
  // the base's top face is red, the rest of it grey
  assert_eq!(face_colours(s), [("Base".to_string(), [0, 0, 1], [255, 0, 0])]);
}

#[test]
fn step_sub_assemblies_and_instances_come_in_as_components_and_linked_copies() {
  let mut s = Session::default();
  let r = run(&mut s, "import", json!({"path": cart()}));
  assert_eq!(r["bodies"].as_array().unwrap().len(), 7, "{r}");
  assert_eq!(r["linked_bodies"], 4, "only the originals' geometry is imported: {r}");
  check_cart(&s);
  // one undo removes the whole import
  s.undo();
  assert!(s.design.features.is_empty() && s.design.components.is_empty() && s.design.face_appearances.is_empty());
}

#[test]
fn step_round_trip_keeps_sub_assemblies_linked_copies_and_face_colours() {
  let dir = temp_dir("round-trip");
  let mut s = Session::default();
  run(&mut s, "import", json!({"path": cart()}));
  let path = dir.join("cart-again.step");
  run(&mut s, "export", json!({"path": path}));
  let text = std::fs::read_to_string(&path).unwrap();
  assert_eq!(text.matches("MANIFOLD_SOLID_BREP").count(), 3, "each original written once, copies as instances");
  // the file's six placements, plus the wheel part inside the Wheel component's sub-assembly
  assert_eq!(text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE").count(), 7);
  let mut t = Session::default();
  run(&mut t, "import", json!({"path": path}));
  check_cart(&t);
  // and again, from the re-imported design
  let path2 = dir.join("cart-third.step");
  run(&mut t, "export", json!({"path": path2}));
  let mut u = Session::default();
  run(&mut u, "import", json!({"path": path2}));
  check_cart(&u);
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn step_export_writes_component_copies_as_instances() {
  let dir = temp_dir("copies");
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let b = s.model.bodies[0].id;
  run(&mut s, "body.rename", json!({"body": b.0, "name": "Plate"}));
  run(&mut s, "body.appearance", json!({"body": b.0, "color": [40, 40, 200]}));
  let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  run(&mut s, "face.appearance", json!({"faces": [{"body": b.0, "face": top}], "color": [250, 200, 0]}));
  let c = run(&mut s, "component.create", json!({"name": "Bracket", "bodies": [b.0]}))["component"].as_u64().unwrap();
  run(&mut s, "component.copy", json!({"component": c, "translate": [40, 0, 0]}));
  run(&mut s, "component.copy", json!({"component": c, "translate": [0, 30, 0], "name": "Spare"}));
  let path = dir.join("copies.step");
  run(&mut s, "export", json!({"path": path}));
  let tree = fox_kernel::read_step_tree(&path).unwrap();
  let brackets: Vec<_> = tree.iter().filter(|n| n.assembly).collect();
  assert_eq!(brackets.len(), 3, "{tree:#?}");
  assert!(brackets.iter().all(|n| n.proto == brackets[0].proto && n.name == "Bracket"));
  let mut inst: Vec<&str> = brackets.iter().map(|n| n.instance.as_str()).collect();
  inst.sort();
  assert_eq!(inst, ["Bracket", "Bracket:2", "Spare"]);
  assert_eq!(std::fs::read_to_string(&path).unwrap().matches("MANIFOLD_SOLID_BREP").count(), 1);

  let mut t = Session::default();
  run(&mut t, "import", json!({"path": path}));
  let mut c = copies(&t);
  c.sort();
  assert_eq!(c, [("Bracket:2".to_string(), "Bracket".to_string()), ("Spare".into(), "Bracket".into())]);
  // where they were (the copies take their original body's name)
  let geometry = |s: &Session| {
    let mut v: Vec<_> = bodies(s).into_iter().map(|(_, c, v)| (c, v)).collect();
    v.sort();
    v
  };
  assert_eq!(geometry(&t), geometry(&s));
  assert_eq!(color_of(&t, "Plate"), [Some([40, 40, 200]); 3]);
  // instances of one part share its face colours, so the copies' tops come back yellow too
  assert_eq!(face_colours(&s), [("Plate".to_string(), [0, 0, 1], [250, 200, 0])]);
  assert_eq!(face_colours(&t), vec![("Plate".to_string(), [0, 0, 1], [250, 200, 0]); 3]);
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn step_export_writes_out_copies_it_cant_link() {
  let dir = temp_dir("unlinked");
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let b = s.model.bodies[0].id;
  let c = run(&mut s, "component.create", json!({"name": "Block", "bodies": [b.0]}))["component"].as_u64().unwrap();
  let r = run(&mut s, "component.copy", json!({"component": c, "translate": [40, 0, 0]}));
  let copy = r["component"].as_u64().unwrap() as u32;
  let copy_body = s.design.component(fox_model::ComponentId(copy)).unwrap().bodies[0];
  // only the copy: its source isn't there to place, so its own geometry is written
  let path = dir.join("copy-only.step");
  run(&mut s, "export", json!({"path": path, "bodies": [copy_body.0]}));
  let mut t = Session::default();
  run(&mut t, "import", json!({"path": path}));
  assert!(copies(&t).is_empty());
  assert_eq!(t.model.bodies.len(), 1);
  let m = t.model.bodies[0].shape.mass().unwrap();
  assert!((m.center_of_mass.x - 40.0).abs() < 1e-6 && (m.volume - 1000.0).abs() < 1e-6, "{m:?}");
  assert_eq!(t.design.components.len(), 1);
  assert_eq!(t.design.components[0].name, "Block:2");
  let _ = std::fs::remove_dir_all(dir);
}
