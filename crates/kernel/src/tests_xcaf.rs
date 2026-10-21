//! STEP assembly trees: nested sub-assemblies, instances of one prototype, face colours.

use super::*;
use glam::{DMat4, DQuat};

fn temp(name: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("fox-kernel-xcaf-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&d);
  std::fs::create_dir_all(&d).unwrap();
  d
}

#[test]
fn step_tree_keeps_nesting_instances_and_face_colours() {
  let dir = temp("tree");
  let path = dir.join("tree.step");
  let base = Shape::make_box(DVec3::new(-60.0, -20.0, -10.0), DVec3::new(60.0, 120.0, 0.0)).unwrap();
  let shaft = Shape::make_box(DVec3::new(-45.0, -2.0, 2.0), DVec3::new(45.0, 2.0, 6.0)).unwrap();
  let wheel = Shape::cylinder(DVec3::ZERO, DVec3::Y, 8.0, 4.0).unwrap();
  // the base's top face (z = 0) red, the base grey
  let top = base.faces().unwrap().into_iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let mut w = StepWriter::new("Cart").unwrap();
  let b = w.part("Base", &base, Some([128, 128, 128]), &[(top, [255, 0, 0])]).unwrap();
  w.place(StepWriter::ROOT, b, "Base", DMat4::IDENTITY).unwrap();
  let axle = w.assembly("Axle").unwrap();
  let s = w.part("Shaft", &shaft, None, &[]).unwrap();
  w.place(axle, s, "Shaft", DMat4::IDENTITY).unwrap();
  let wh = w.part("Wheel", &wheel, Some([0, 64, 255]), &[]).unwrap();
  w.place(axle, wh, "Wheel:1", DMat4::from_translation(DVec3::new(-50.0, 0.0, 4.0))).unwrap();
  w.place(axle, wh, "Wheel:2", DMat4::from_translation(DVec3::new(50.0, 0.0, 4.0))).unwrap();
  w.place(StepWriter::ROOT, axle, "Axle:1", DMat4::IDENTITY).unwrap();
  let turned = DMat4::from_rotation_translation(DQuat::from_rotation_z(std::f64::consts::PI), DVec3::new(0.0, 100.0, 0.0));
  w.place(StepWriter::ROOT, axle, "Axle:2", turned).unwrap();
  assert!(w.place(StepWriter::ROOT, axle, "bad", DMat4::from_scale(DVec3::splat(2.0))).is_err(), "rigid only");
  w.write(&path).unwrap();

  let nodes = read_step_tree(&path).unwrap();
  let named = |n: &str| nodes.iter().enumerate().filter(|(_, x)| x.instance == n).map(|(i, _)| i).collect::<Vec<_>>();
  let axles: Vec<usize> = nodes.iter().enumerate().filter(|(_, n)| n.assembly).map(|(i, _)| i).collect();
  assert_eq!(axles.len(), 2, "{nodes:#?}");
  assert_eq!(nodes[axles[0]].proto, nodes[axles[1]].proto, "one prototype placed twice");
  assert_eq!(nodes[axles[0]].uses, 2);
  assert_eq!(nodes[axles[0]].name, "Axle");
  assert!(nodes[axles[0]].parent.is_none() && nodes[axles[1]].parent.is_none());
  assert!(nodes[axles[1]].placement.abs_diff_eq(turned, 1e-9), "{:?}", nodes[axles[1]].placement);
  // four wheel occurrences of one part, used twice by the axle
  let wheels: Vec<&StepNode> = nodes.iter().filter(|n| n.name == "Wheel").collect();
  assert_eq!(wheels.len(), 4);
  assert!(wheels.iter().all(|n| n.proto == wheels[0].proto && n.uses == 2 && n.bodies.len() == 1));
  assert!(wheels.iter().all(|n| n.parent.is_some_and(|p| nodes[p].assembly)));
  assert!(wheels.iter().all(|n| n.bodies[0].color == Some([0, 64, 255])));
  assert!(wheels.iter().all(|n| ["Axle:1", "Axle:2"].contains(&n.bodies[0].component.as_str())));
  // placed: the turned axle's wheels swap sides
  let mut at: Vec<[i64; 2]> = wheels
    .iter()
    .map(|n| {
      let c = n.bodies[0].shape.mass().unwrap().center_of_mass;
      [c.x.round() as i64, c.y.round() as i64]
    })
    .collect();
  at.sort();
  assert_eq!(at, [[-50, 2], [-50, 98], [50, 2], [50, 98]]);
  // the base: grey with one red face, the top
  let base_node = &nodes[named("Base")[0]];
  let body = &base_node.bodies[0];
  assert_eq!(body.color, Some([128, 128, 128]));
  assert_eq!(body.face_colors.len(), 1, "{:?}", body.face_colors);
  let (id, c) = body.face_colors[0];
  assert_eq!(c, [255, 0, 0]);
  let f = body.shape.faces().unwrap().into_iter().find(|f| f.id == id).unwrap();
  assert!(f.normal.z > 0.9 && f.origin.z.abs() < 1e-9, "{f:?}");
  // flattened, as before: each body with its top-level sub-assembly
  let flat = read_step_assembly(&path).unwrap();
  assert_eq!(flat.len(), 1 + 2 * 3);
  assert_eq!(flat.iter().filter(|b| b.component == "Axle:2").count(), 3);
  // the prototypes are written once: three solids for seven bodies
  let text = std::fs::read_to_string(&path).unwrap();
  assert_eq!(text.matches("MANIFOLD_SOLID_BREP").count(), 3);
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn step_parts_keep_their_own_names_when_they_share_geometry() {
  let dir = temp("shared");
  let path = dir.join("shared.step");
  let a = Shape::make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
  let moved = a.translated(DVec3::new(30.0, 0.0, 0.0)).unwrap();
  let parts = [
    StepPart { name: "Left", component: "", color: Some([255, 0, 0]), shape: &a },
    StepPart { name: "Right", component: "", color: Some([0, 255, 0]), shape: &moved },
  ];
  write_step_assembly("Pair", &parts, &path).unwrap();
  let back = read_step_assembly(&path).unwrap();
  let find = |n: &str| back.iter().find(|b| b.name == n).unwrap_or_else(|| panic!("{n} in {back:?}"));
  assert_eq!(find("Left").color, Some([255, 0, 0]));
  assert_eq!(find("Right").color, Some([0, 255, 0]));
  assert!((find("Right").shape.mass().unwrap().center_of_mass.x - 35.0).abs() < 1e-9);
  // a part moved by its location comes back as a part, not a sub-assembly
  assert!(read_step_tree(&path).unwrap().iter().all(|n| !n.assembly));
  let _ = std::fs::remove_dir_all(&dir);
}
