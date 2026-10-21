//! Configuration rules (driven columns) and direct move face with rotations and curved faces.

use super::*;
use crate::config::{Column, Configuration, Override, Rule};
use crate::direct::{DirectEdit, Rotation};
use crate::*;
use fox_kernel::SurfaceKind;
use fox_kernel::glam::DVec3;
use fox_sketch::{ConstraintKind, Dim, ORIGIN, PtSpec, Sketch};

const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };

/// A w × 20 × h block, sketched with dimensions w and 20, extruded h.
fn block() -> (Design, FeatureId, FeatureId) {
  let mut d = Design::new("block");
  d.add_parameter("w", "40 mm", ParamKind::Length, "").unwrap();
  d.add_parameter("h", "10 mm", ParamKind::Length, "").unwrap();
  let mut s = Sketch::new();
  let lines = s.add_rectangle([0.0, 0.0], [10.0, 10.0]).unwrap();
  let bl = s.curve_points(lines[0])[0];
  s.add_constraint(ConstraintKind::Coincident { a: bl, b: ORIGIN }).unwrap();
  let b = s.curve_points(lines[0]);
  s.add_constraint(ConstraintKind::Distance { a: b[0], b: b[1], value: Dim::expr("w", 10.0) }).unwrap();
  let r = s.curve_points(lines[1]);
  s.add_constraint(ConstraintKind::Distance { a: r[0], b: r[1], value: Dim::expr("20", 10.0) }).unwrap();
  let sk = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s, links: Vec::new() }).unwrap();
  let e = d
    .add_feature(FeatureKind::Extrude {
      sketch: sk,
      profiles: vec![],
      extent: Extent::distance("h"),
      operation: Operation::NewBody,
    })
    .unwrap();
  (d, sk, e)
}

fn volume(m: &Model) -> f64 {
  m.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

fn ok(m: &Model) {
  assert!(m.errors().is_empty(), "{:?}", m.errors());
}

fn row(d: &mut Design, name: &str, overrides: Vec<Override>) {
  d.configurations.list.push(Configuration { name: name.into(), overrides });
}

fn rule(column: Column, expr: &str) -> Rule {
  Rule { column, expr: expr.into(), color: None, otherwise: None }
}

#[test]
fn a_driven_parameter_follows_each_rows_values() {
  let (mut d, _, _) = block();
  row(&mut d, "Short", vec![Override::Parameter { name: "w".into(), expr: "40 mm".into() }]);
  row(&mut d, "Long", vec![Override::Parameter { name: "w".into(), expr: "120 mm".into() }]);
  // h is driven: thicker plates for long rows, even where the row sets h itself
  d.configurations.rules.push(rule(Column::Parameter { name: "h".into() }, "if(w > 100 mm, w / 10, 5 mm)"));
  d.configurations.list[1].overrides.push(Override::Parameter { name: "h".into(), expr: "99 mm".into() });
  let mut regen = Regenerator::default();
  d.configurations.active = Some("Long".into());
  let m = regen.regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 120.0 * 20.0 * 12.0).abs() < 1e-6, "{}", volume(&m));
  d.configurations.active = Some("Short".into());
  let m = regen.regenerate(&mut d);
  assert!((volume(&m) - 40.0 * 20.0 * 5.0).abs() < 1e-6);
  // the table shows the computed cell
  let r = d.resolve_configuration(&d.configurations.list[1]);
  assert!(r.errors.is_empty());
  assert!(r.overrides.contains(&Override::Parameter { name: "h".into(), expr: "12 mm".into() }));
  assert!(d.configurations.columns().contains(&Column::Parameter { name: "h".into() }));
  // no configuration: the design as modelled, rules or not
  d.configurations.active = None;
  assert!((volume(&regen.regenerate(&mut d)) - 8000.0).abs() < 1e-6);
  // rules survive a round trip and a parameter rename
  d.rename_parameter("w", "width").unwrap();
  assert_eq!(d.configurations.rules[0].expr, "if(width > 100 mm, width / 10, 5 mm)");
  let back = Design::from_json(&d.to_json()).unwrap();
  assert_eq!(back.configurations.rules, d.configurations.rules);
  let mut back = back;
  assert!(back.remove_parameter("width").is_err());
}

#[test]
fn driven_suppression_appearance_and_errors() {
  let (mut d, _, e) = block();
  let m = regenerate(&mut d);
  let b = m.bodies[0].id;
  row(&mut d, "Small", vec![Override::Parameter { name: "w".into(), expr: "10 mm".into() }]);
  row(&mut d, "Big", vec![Override::Parameter { name: "w".into(), expr: "200 mm".into() }]);
  // too small to make: suppress the extrude; big ones are red, the rest blue
  d.configurations.rules.push(rule(Column::Suppress { feature: e }, "w < 20 mm"));
  d.configurations.rules.push(Rule {
    column: Column::Appearance { body: b },
    expr: "w > 100 mm".into(),
    color: Some([255, 0, 0]),
    otherwise: Some([0, 0, 255]),
  });
  d.configurations.active = Some("Small".into());
  let m = regenerate(&mut d);
  ok(&m);
  assert!(m.bodies.is_empty());
  assert_eq!(d.body_color(b), Some([0, 0, 255]));
  d.configurations.active = Some("Big".into());
  assert_eq!(regenerate(&mut d).bodies.len(), 1);
  assert_eq!(d.body_color(b), Some([255, 0, 0]));
  // a rule that can't be evaluated leaves its cell at the design's value and says why
  d.configurations.rules[0].expr = "nope > 1".into();
  let r = d.active_overrides().unwrap();
  assert_eq!(r.errors.len(), 1);
  assert!(r.errors[0].1.contains("nope"));
  assert_eq!(regenerate(&mut d).bodies.len(), 1);
}

fn plate_with_hole() -> (Design, Model) {
  let (mut d, _, _) = block();
  d.set_parameter("w", "60 mm").unwrap();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  let top = FaceRef::from_face(body.id, top, top.center);
  let mut hole = Sketch::new();
  hole.add_circle(PtSpec::At(15.0, 10.0), 3.0).unwrap();
  let hs = d
    .add_feature(FeatureKind::Sketch { plane: PlaneRef::Face { face: top, x: None }, sketch: hole, links: Vec::new() })
    .unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: hs,
    profiles: vec![],
    extent: Extent { kind: ExtentKind::ThroughAll, flip: true, ..Extent::distance("0") },
    operation: Operation::Cut,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  (d, m)
}

#[test]
fn move_face_slides_a_hole_and_replays_after_upstream_changes() {
  let (mut d, m) = plate_with_hole();
  let v0 = volume(&m);
  let body = &m.bodies[0];
  let cyl = body.faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap();
  let face = FaceRef::from_face(body.id, cyl, cyl.center + DVec3::new(3.0, 0.0, 0.0));
  d.add_direct_edit(DirectEdit::MoveFaces { faces: vec![face], translate: [20.0, 0.0, 0.0], rotate: None }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - v0).abs() < 1e-3);
  let c = m.bodies[0].faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).copied().unwrap();
  // the sketch's x runs along the top face's own axis; either way the hole moved by 20
  assert!(((c.origin - cyl.origin).length() - 20.0).abs() < 1e-6, "{:?} {:?}", c.origin, cyl.origin);
  // a thicker plate upstream: the hole still moves, through the whole plate
  d.set_parameter("h", "14 mm").unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - (60.0 * 20.0 * 14.0 - std::f64::consts::PI * 9.0 * 14.0)).abs() < 1e-2, "{}", volume(&m));
  let mut back = Design::from_json(&d.to_json()).unwrap();
  ok(&regenerate(&mut back));
}

#[test]
fn move_face_rotates_a_flat_face() {
  let (mut d, _, _) = block();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  let face = FaceRef::from_face(body.id, top, top.center);
  // the top turned 10° about its own middle line: as much rises as sinks
  let rotate = Some(Rotation { origin: [20.0, 10.0, 10.0], axis: [0.0, 1.0, 0.0], angle: 10.0 });
  d.add_direct_edit(DirectEdit::MoveFaces { faces: vec![face], translate: [0.0, 0.0, 2.0], rotate }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 40.0 * 20.0 * 12.0).abs() < 1e-3, "{}", volume(&m));
  let hi = m.bodies[0].shape.mass().unwrap().bbox_max.z;
  assert!((hi - (12.0 + 20.0 * 10f64.to_radians().tan())).abs() < 1e-4, "{hi}");
  let back = Design::from_json(&d.to_json()).unwrap();
  assert_eq!(back.features, d.features);
}
