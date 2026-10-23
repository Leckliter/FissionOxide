//! Batch ui-leftovers-dialogs: REMOVE taking components, and `feature.edit`'s picks for Revolve,
//! Shell, Offset Plane, Mirror and the patterns.

use fox_model::{AxisRef, BodyId, BodySel, ComponentId, FeatureKind, OriginPlane, PlaneRef};
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

#[test]
fn remove_takes_components_and_those_below_them() {
  let mut s = Session::default();
  for x in [0, 30, 60] {
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [x, 0], "operation": "new_body"}));
  }
  let ids: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  let top = run(&mut s, "component.create", json!({"name": "Top", "bodies": [ids[0]]}))["component"].as_u64().unwrap();
  run(&mut s, "component.create", json!({"name": "Sub", "bodies": [ids[1]], "parent": top}));
  let r = run(&mut s, "body.remove", json!({"components": [top]}));
  let id = fox_model::FeatureId(r["feature"].as_u64().unwrap() as u32);
  assert_eq!(s.model.bodies.iter().map(|b| b.id.0).collect::<Vec<_>>(), [ids[2]], "the component and its child go");
  assert!(matches!(&s.design.feature(id).unwrap().kind,
    FeatureKind::RemoveBodies { components, .. } if *components == [ComponentId(top as u32)]));
  assert!(s.execute("body.remove", &json!({"components": [99]})).is_err());
}

#[test]
fn edit_picks_of_revolve_shell_planes_and_patterns() {
  let mut s = Session::default();
  // a revolve's profile and axis picked again
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [9, 4]}));
  run(&mut s, "sketch.rect", json!({"corner1": [15, 0], "corner2": [19, 4]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  let rev =
    run(&mut s, "revolve", json!({"sketch": sk, "profiles": [[7, 2]], "axis": "z", "angle": "90 deg"}))["feature"]
      .clone();
  run(
    &mut s,
    "feature.edit",
    json!({"feature": rev, "set": {}, "picks": {"sketch": sk, "profiles": [[17, 2]], "axis": "x"}}),
  );
  let f =
    |s: &Session, v: &Value| s.design.feature(fox_model::FeatureId(v.as_u64().unwrap() as u32)).unwrap().kind.clone();
  let FeatureKind::Revolve { profiles, axis, .. } = f(&s, &rev) else { panic!() };
  assert_eq!((profiles, axis), (vec![[17.0, 2.0]], AxisRef::X));
  assert!(s.execute("feature.edit", &json!({"feature": rev, "set": {}, "picks": {"profiles": []}})).is_err());

  // a shell's open face and an offset plane's base
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  let shell = run(&mut s, "shell", json!({"faces": [{"point": [0, 0, 20]}], "thickness": 1}))["feature"].clone();
  run(&mut s, "timeline.marker", json!({"at": 1}));
  run(
    &mut s,
    "feature.edit",
    json!({"feature": shell, "set": {}, "picks": {"faces": [{"point": [0, 0, 0]}]}, "marker_after": true}),
  );
  let FeatureKind::Shell { faces, .. } = f(&s, &shell) else { panic!() };
  assert!(faces[0].point[2].abs() < 1e-9, "{:?}", faces[0].point);
  run(&mut s, "timeline.marker", json!({"at": null}));
  let plane = run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 5}))["feature"].clone();
  run(&mut s, "feature.edit", json!({"feature": plane, "set": {}, "picks": {"plane": "yz"}}));
  let FeatureKind::ConstructionPlane { base, .. } = f(&s, &plane) else { panic!() };
  assert_eq!(base, PlaneRef::Origin { plane: OriginPlane::Yz });

  // a mirror's plane and objects, a circular pattern's objects and axis
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [20, 0]}));
  run(&mut s, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [-20, 0], "operation": "new_body"}));
  let (a, b) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
  let m = run(&mut s, "mirror", json!({"plane": "yz", "bodies": [a], "join": false}))["feature"].clone();
  run(&mut s, "feature.edit", json!({"feature": m, "set": {}, "picks": {"plane": "xz", "bodies": [b]}}));
  let FeatureKind::Mirror { plane, bodies, .. } = f(&s, &m) else { panic!() };
  assert_eq!(plane, PlaneRef::Origin { plane: OriginPlane::Xz });
  assert_eq!(bodies, BodySel::Bodies { ids: vec![BodyId(b)] });
  let c =
    run(&mut s, "pattern.circular", json!({"bodies": [a], "axis": "z", "count": 3, "join": false}))["feature"].clone();
  run(&mut s, "feature.edit", json!({"feature": c, "set": {}, "picks": {"bodies": [b], "axis": "y"}}));
  let FeatureKind::CircularPattern { axis, bodies, .. } = f(&s, &c) else { panic!() };
  assert_eq!((axis, bodies), (AxisRef::Y, BodySel::Bodies { ids: vec![BodyId(b)] }));
  assert!(s.execute("feature.edit", &json!({"feature": c, "set": {}, "picks": {"bodies": [999]}})).is_err());
}

/// Batch ui-reverify-dialogs-2: Edit Draft's picks made again on the rolled-back model: other
/// faces to draft and another pull direction.
#[test]
fn edit_picks_of_draft() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  let d = run(&mut s, "draft", json!({"faces": [{"point": [0, -10, 10]}], "plane": "xy", "angle": "5 deg"}))["feature"]
    .clone();
  run(&mut s, "timeline.marker", json!({"at": 1}));
  let picks = json!({"faces": [{"point": [10, 0, 10]}], "plane": {"point": [0, 0, 20]}, "tangent_chain": true});
  run(&mut s, "feature.edit", json!({"feature": d, "set": {}, "picks": picks, "marker_after": true}));
  let FeatureKind::Draft { faces, plane, .. } =
    &s.design.feature(fox_model::FeatureId(d.as_u64().unwrap() as u32)).unwrap().kind
  else {
    panic!()
  };
  assert_eq!(faces.len(), 1);
  assert!((faces[0].point[0] - 10.0).abs() < 1e-9, "{:?}", faces[0].point);
  assert!(matches!(plane, PlaneRef::Face { .. }), "{plane:?}");
  assert!(s.execute("feature.edit", &json!({"feature": d, "set": {}, "picks": {"faces": []}})).is_err());
}
