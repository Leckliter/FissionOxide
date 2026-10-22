//! Configurations (each kind of override) and base features (direct edits, history off).

use super::*;
use crate::assembly::{JointGeometry, JointKind, MotionAxis};
use crate::config::{Column, Configuration, Override};
use crate::direct::{BaseBody, DirectEdit};
use crate::*;
use fox_kernel::glam::DVec3;
use fox_sketch::{ConstraintKind, Dim, ORIGIN, PtSpec, Sketch};

const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };

fn rect_sketch(w: &str, d: &str) -> Sketch {
  let mut s = Sketch::new();
  let lines = s.add_rectangle([0.0, 0.0], [10.0, 10.0]).unwrap();
  let bl = s.curve_points(lines[0])[0];
  s.add_constraint(ConstraintKind::Coincident { a: bl, b: ORIGIN }).unwrap();
  let b = s.curve_points(lines[0]);
  s.add_constraint(ConstraintKind::Distance { a: b[0], b: b[1], value: Dim::expr(w, 10.0) }).unwrap();
  let r = s.curve_points(lines[1]);
  s.add_constraint(ConstraintKind::Distance { a: r[0], b: r[1], value: Dim::expr(d, 10.0) }).unwrap();
  s
}

/// A w × 20 × h block (40 × 20 × 10 by default).
fn block() -> (Design, FeatureId) {
  let mut d = Design::new("block");
  d.add_parameter("w", "40 mm", ParamKind::Length, "").unwrap();
  d.add_parameter("h", "10 mm", ParamKind::Length, "").unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: rect_sketch("w", "20"), links: Vec::new() }).unwrap();
  let e = d
    .add_feature(FeatureKind::Extrude {
      sketch: Some(s),
      faces: vec![],
      profiles: vec![],
      extent: Extent::distance("h"),
      operation: Operation::NewBody,
    })
    .unwrap();
  (d, e)
}

fn volume(m: &Model) -> f64 {
  m.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

fn ok(m: &Model) {
  assert!(m.errors().is_empty(), "{:?}", m.errors());
}

fn add_config(d: &mut Design, name: &str, overrides: Vec<Override>) {
  d.configurations.list.push(Configuration { name: name.into(), overrides });
}

fn top_face(m: &Model) -> FaceRef {
  let body = &m.bodies[0];
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  FaceRef::from_face(body.id, top, top.center)
}

#[test]
fn parameter_override_changes_the_model_not_the_design() {
  let (mut d, _) = block();
  add_config(&mut d, "Wide", vec![Override::Parameter { name: "w".into(), expr: "80 mm".into() }]);
  let mut regen = Regenerator::default();
  ok(&regen.regenerate(&mut d));
  d.configurations.active = Some("Wide".into());
  let m = regen.regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 80.0 * 20.0 * 10.0).abs() < 1e-6);
  assert!((m.params["w"] - 80.0).abs() < 1e-9);
  assert_eq!(d.parameter("w").unwrap().expr, "40 mm");
  // back to the design as modelled
  d.configurations.active = None;
  let m = regen.regenerate(&mut d);
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
  // an unknown configuration name is the design as modelled
  d.configurations.active = Some("Nope".into());
  assert!((volume(&regen.regenerate(&mut d)) - 8000.0).abs() < 1e-6);
}

#[test]
fn suppression_override() {
  let (mut d, e) = block();
  add_config(&mut d, "Bare", vec![Override::Suppress { feature: e, suppressed: true }]);
  d.configurations.active = Some("Bare".into());
  let m = regenerate(&mut d);
  ok(&m);
  assert!(m.bodies.is_empty());
  assert!(m.status.iter().any(|s| s.id == e.0 && s.suppressed));
  assert!(!d.feature(e).unwrap().suppressed);
  // a row can also unsuppress a feature the design suppresses
  d.feature_mut(e).unwrap().suppressed = true;
  d.configurations.list[0].overrides = vec![Override::Suppress { feature: e, suppressed: false }];
  assert_eq!(regenerate(&mut d).bodies.len(), 1);
}

#[test]
fn appearance_override() {
  let (mut d, _) = block();
  let m = regenerate(&mut d);
  let b = m.bodies[0].id;
  d.body_settings_mut(b).color = Some([1, 2, 3]);
  add_config(&mut d, "Red", vec![Override::Appearance { body: b, color: Some([255, 0, 0]) }]);
  add_config(&mut d, "Plain", vec![Override::Appearance { body: b, color: None }]);
  assert_eq!(d.body_color(b), Some([1, 2, 3]));
  d.configurations.active = Some("Red".into());
  assert_eq!(d.body_color(b), Some([255, 0, 0]));
  d.configurations.active = Some("Plain".into());
  assert_eq!(d.body_color(b), None);
  // appearance only: nothing to regenerate differently
  assert!(config::effective(&d).is_none());
  assert_eq!(d.body_settings(b).unwrap().color, Some([1, 2, 3]));
}

#[test]
fn joint_override_moves_the_component_and_keeps_the_joint() {
  let (mut d, _) = block();
  let m = regenerate(&mut d);
  let b = m.bodies[0].id;
  let c = d.add_component("Block", vec![b]).unwrap();
  let at = JointGeometry::Point { point: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0] };
  let j = d
    .add_feature(FeatureKind::Joint {
      moving: c,
      moving_origin: at.clone(),
      fixed: None,
      fixed_origin: at,
      kind: JointKind::Slider,
      angle: "0".into(),
      offset: "0".into(),
      flip: false,
      as_built: true,
      rotation: "0".into(),
      slide: "0".into(),
      slide2: "0".into(),
      pitch: "0".into(),
      limits: Default::default(),
      lead: "0".into(),
    })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!(m.bounds().unwrap().0.z.abs() < 1e-6);
  add_config(&mut d, "Raised", vec![Override::Joint { joint: j, motion: MotionAxis::Slide, value: "h * 2".into() }]);
  d.configurations.active = Some("Raised".into());
  let m = regenerate(&mut d);
  ok(&m);
  assert!((m.bounds().unwrap().0.z - 20.0).abs() < 1e-6, "{:?}", m.bounds());
  let FeatureKind::Joint { slide, .. } = &d.feature(j).unwrap().kind else { panic!() };
  assert_eq!(slide, "0");
}

#[test]
fn configurations_survive_save_load_and_old_files_are_unchanged() {
  let (mut d, e) = block();
  // no configurations: the file has no configuration keys at all
  let plain = d.to_json();
  assert!(!plain.contains("configurations") && !plain.contains("history_off"));
  add_config(
    &mut d,
    "Long",
    vec![
      Override::Parameter { name: "w".into(), expr: "100 mm".into() },
      Override::Suppress { feature: e, suppressed: false },
    ],
  );
  d.configurations.active = Some("Long".into());
  let back = Design::from_json(&d.to_json()).unwrap();
  assert_eq!(back, d);
  assert_eq!(back.configurations.columns().len(), 2);
  assert_eq!(back.configurations.columns()[0], Column::Parameter { name: "w".into() });
  // renaming the parameter renames the column; removing it is refused while a row's expression uses it
  d.rename_parameter("w", "width").unwrap();
  assert!(d.configurations.get("Long").unwrap().cell(&Column::Parameter { name: "width".into() }).is_some());
  d.configurations.get_mut("Long").unwrap().set(Override::Parameter { name: "h".into(), expr: "width / 4".into() });
  assert!(d.remove_parameter("width").is_err());
  let mut d2 = d.clone();
  let m = regenerate(&mut d2);
  ok(&m);
  assert!((volume(&m) - 100.0 * 20.0 * 25.0).abs() < 1e-6);
}

#[test]
fn every_example_design_still_loads() {
  let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
  let mut n = 0;
  for entry in std::fs::read_dir(dir).unwrap() {
    let path = entry.unwrap().path();
    if path.extension().is_some_and(|x| x == "fox" || x == "clf") {
      let mut d = Design::load(&path).unwrap();
      assert!(d.configurations.is_empty() && !d.history_off);
      ok(&regenerate(&mut d));
      n += 1;
    }
  }
  assert!(n > 0);
}

/// A block frozen into a base feature, as turning history off does.
fn frozen_block() -> (Design, BodyId) {
  let (mut d, _) = block();
  let m = regenerate(&mut d);
  let bodies: Vec<BaseBody> = m
    .bodies
    .iter()
    .map(|b| BaseBody { id: b.id, name: b.name.clone(), brep: b.shape.to_brep().unwrap(), mesh: false })
    .collect();
  let id = m.bodies[0].id;
  d.collapse_history(bodies);
  (d, id)
}

#[test]
fn collapsed_history_keeps_bodies_and_survives_save_load() {
  let (mut d, b) = frozen_block();
  assert_eq!(d.features.len(), 1);
  assert!(matches!(d.features[0].kind, FeatureKind::BaseFeature { .. }));
  assert_eq!(d.open_base_feature(), Some(d.features[0].id));
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies[0].id, b);
  assert_eq!(m.bodies[0].name, "Body1");
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
  // a later parametric feature gets fresh ids
  let mut back = Design::from_json(&d.to_json()).unwrap();
  assert_eq!(back, d);
  let m = regenerate(&mut back);
  ok(&m);
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
  let s = back.add_feature(FeatureKind::Sketch { plane: XY, sketch: rect_sketch("5", "5"), links: vec![] }).unwrap();
  assert!(s.0 > back.features[0].id.0);
}

#[test]
fn direct_edits_replay_after_save_load() {
  let (mut d, _) = frozen_block();
  let m = regenerate(&mut d);
  let top = top_face(&m);
  d.add_direct_edit(DirectEdit::PressPull { faces: vec![top.clone()], distance: 5.0 }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 40.0 * 20.0 * 15.0).abs() < 1e-6);
  // a move along the normal moves the face; the sideways part does nothing
  let top = top_face(&m);
  d.add_direct_edit(DirectEdit::MoveFaces { faces: vec![top.clone()], translate: [3.0, 0.0, -5.0], rotate: None })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
  d.add_direct_edit(DirectEdit::MoveFaces { faces: vec![top_face(&m)], translate: [3.0, 0.0, 0.0], rotate: None })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let st = m.status.last().unwrap();
  assert!(st.warning.as_deref().unwrap_or("").contains("slide"), "{st:?}");
  // all edits landed in the one base feature
  assert_eq!(d.features.len(), 1);
  let FeatureKind::BaseFeature { edits, .. } = &d.features[0].kind else { panic!() };
  assert_eq!(edits.len(), 3);
  let mut back = Design::from_json(&d.to_json()).unwrap();
  let m = regenerate(&mut back);
  ok(&m);
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
}

#[test]
fn delete_face_and_base_feature_in_a_parametric_timeline() {
  // a parametric block with a hole, then a base feature that deletes the hole's face
  let (mut d, _) = block();
  let m = regenerate(&mut d);
  let top = top_face(&m);
  let mut hole = Sketch::new();
  hole.add_circle(PtSpec::At(20.0, 10.0), 3.0).unwrap();
  let hs = d
    .add_feature(FeatureKind::Sketch { plane: PlaneRef::Face { face: top, x: None }, sketch: hole, links: Vec::new() })
    .unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: Some(hs),
    faces: vec![],
    profiles: vec![],
    extent: Extent { kind: ExtentKind::ThroughAll, flip: true, ..Extent::distance("0") },
    operation: Operation::Cut,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!(volume(&m) < 8000.0 - 1.0);
  assert!(d.open_base_feature().is_none());
  let body = &m.bodies[0];
  let cyl = body.faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap();
  let face = FaceRef::from_face(body.id, cyl, cyl.center + DVec3::new(3.0, 0.0, 0.0));
  let base = d.add_direct_edit(DirectEdit::DeleteFaces { faces: vec![face] }).unwrap();
  assert_eq!(d.open_base_feature(), Some(base));
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 8000.0).abs() < 1e-3, "{}", volume(&m));
  // upstream parameters still drive what the base feature edits
  d.set_parameter("w", "60 mm").unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 60.0 * 20.0 * 10.0).abs() < 1e-3);
  let mut back = Design::from_json(&d.to_json()).unwrap();
  assert!((volume(&regenerate(&mut back)) - 12000.0).abs() < 1e-3);
}
