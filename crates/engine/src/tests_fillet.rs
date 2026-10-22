//! Variable fillets with radii at points along the edge, and radii running to zero.

use serde_json::{Value, json};

use crate::Session;
use fox_model::FeatureKind;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().map(|m| m.volume).unwrap_or(0.0)).sum()
}

// What a quarter round of radius r removes per unit length.
fn cut(r: f64) -> f64 {
  (1.0 - std::f64::consts::PI / 4.0) * r * r
}

#[test]
fn fillet_radii_at_points_and_down_to_zero() {
  // radius 4 running straight down to 0 along a 20 mm edge
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  run(&mut s, "fillet", json!({"edges": [{"point": [10, 0, 20]}], "radius": 4, "radius2": 0}));
  let removed = 8000.0 - volume(&s);
  // quarter-round sections would cut (1 - pi/4) L a^2 / 3; the rolling ball cuts a little more
  assert!(removed > cut(4.0) * 20.0 / 3.0 && removed < 1.1 * cut(4.0) * 20.0 / 3.0, "{removed}");

  // 1 at the ends, 3 in the middle
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  let p = json!({"edges": [{"point": [10, 0, 20]}], "radius": 1, "radii": [[0.5, "3 mm"]]});
  run(&mut s, "fillet", p);
  let removed = 8000.0 - volume(&s);
  assert!(removed > cut(1.0) * 20.0 && removed < cut(3.0) * 20.0, "{removed}");
  let FeatureKind::Fillet { radii, .. } = &s.design.features.last().unwrap().kind else { panic!() };
  assert_eq!(radii.len(), 1);
  assert_eq!(radii[0].radius, "3 mm");

  // the points are checked
  let bad = s.execute("fillet", &json!({"edges": [{"point": [0, 10, 20]}], "radius": 1, "radii": [[2, 3]]}));
  assert!(bad.is_err() || !bad.unwrap()["error"].is_null());
}

fn top_face(s: &Session, body: usize) -> Value {
  let b = &s.model.bodies[body];
  json!({"body": b.id.0, "face": b.faces().iter().find(|f| f.normal.z > 0.9 && f.center.z > 19.0).unwrap().id})
}

fn fillet_edges(s: &Session) -> usize {
  let FeatureKind::Fillet { edges, sets, .. } = &s.design.features.last().unwrap().kind else { panic!() };
  edges.len() + sets.iter().map(|x| x.edges.len()).sum::<usize>()
}

#[test]
fn fillet_faces_tangent_chains_and_sets() {
  // a face rounds all its edges, like picking its four edges
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  let top = top_face(&s, 0);
  run(&mut s, "fillet", json!({"faces": [top], "radius": 2}));
  assert_eq!(fillet_edges(&s), 4);
  let by_face = volume(&s);
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  let top: Vec<Value> =
    [[10, 0], [20, 10], [10, 20], [0, 10]].iter().map(|[x, y]| json!({"point": [x, y, 20]})).collect();
  run(&mut s, "fillet", json!({"edges": top, "radius": 2}));
  assert!((volume(&s) - by_face).abs() < 1e-6);

  // after rounding a vertical edge, a top edge chains on through the round's arc to the next edge
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  run(&mut s, "fillet", json!({"edges": [{"point": [20, 20, 10]}], "radius": 5}));
  run(&mut s, "fillet", json!({"edges": [{"point": [10, 20, 20]}], "radius": 1}));
  assert_eq!(fillet_edges(&s), 1);
  s.undo();
  run(&mut s, "fillet", json!({"edges": [{"point": [10, 20, 20]}], "radius": 1, "tangent_chain": true}));
  assert_eq!(fillet_edges(&s), 3);

  // two sets with their own radii on opposite top edges
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  let sets = json!([
    {"edges": [{"point": [10, 0, 20]}], "radius": 1},
    {"edges": [{"point": [10, 20, 20]}, {"point": [10, 0, 20]}], "radius": "3 mm"},
  ]);
  run(&mut s, "fillet", json!({"sets": sets}));
  let FeatureKind::Fillet { edges, sets, .. } = &s.design.features.last().unwrap().kind else { panic!() };
  assert_eq!((edges.len(), sets.len(), sets[0].edges.len()), (1, 1, 1), "an edge stays in its first set");
  let removed = 8000.0 - volume(&s);
  assert!((removed - 20.0 * (cut(1.0) + cut(3.0))).abs() < 1e-3, "{removed}");
  // the set radii are parameters like the first
  let id = s.design.features.last().unwrap().id.0;
  let mut set = serde_json::to_value(&s.design.features.last().unwrap().kind).unwrap()["sets"].clone();
  set[0]["radius"] = json!("2 mm");
  run(&mut s, "feature.edit", json!({"feature": id, "set": {"sets": set}}));
  let removed = 8000.0 - volume(&s);
  assert!((removed - 20.0 * (cut(1.0) + cut(2.0))).abs() < 1e-3, "{removed}");
}

#[test]
fn extrude_start_offset_start_face_and_all_both_ways() {
  let bbox = |s: &Session, i: usize| {
    let m = s.model.bodies[i].shape.mass().unwrap();
    (m.bbox_min.z, m.bbox_max.z)
  };
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  run(&mut s, "extrude", json!({"distance": 10, "start": "5 mm"}));
  let (lo, hi) = bbox(&s, 0);
  assert!((lo - 5.0).abs() < 1e-6 && (hi - 15.0).abs() < 1e-6, "{lo} {hi}");

  // from the top of a block, on a sketch on the ground
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [2, 2], "corner2": [8, 8]}));
  let top = top_face(&s, 0);
  run(&mut s, "extrude", json!({"distance": 5, "start_face": top, "operation": "new_body"}));
  let (lo, hi) = bbox(&s, 1);
  assert!((lo - 20.0).abs() < 1e-6 && (hi - 25.0).abs() < 1e-6, "{lo} {hi}");

  // cut all the way through, both ways, from a sketch in the middle
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
  let mid = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": mid}}));
  run(&mut s, "sketch.rect", json!({"corner1": [2, 2], "corner2": [8, 8]}));
  run(&mut s, "extrude", json!({"extent": "through_all_both", "operation": "cut"}));
  assert!((volume(&s) - (8000.0 - 36.0 * 20.0)).abs() < 1e-6, "{}", volume(&s));
}

#[test]
fn asymmetric_and_full_round_fillets() {
  // 6 down the front face (it sorts first), 2 into the top: a quarter ellipse along 20 mm
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  run(&mut s, "fillet", json!({"edges": [{"point": [10, 0, 20]}], "distance": 6, "distance2": 2}));
  let removed = 8000.0 - volume(&s);
  assert!((removed - 12.0 * (1.0 - std::f64::consts::PI / 4.0) * 20.0).abs() < 1e-3, "{removed}");
  let FeatureKind::Fillet { distance2, flipped, .. } = &s.design.features.last().unwrap().kind else { panic!() };
  assert_eq!((distance2.as_deref(), *flipped), (Some("2"), false));

  // the top of a 10 mm wall becomes a half-cylinder
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 20, "at": [10, 5]}));
  let round = json!({"full_round": [{
    "center": {"point": [10, 5, 20], "normal": [0, 0, 1]},
    "side_one": [{"point": [10, 0, 10], "normal": [0, -1, 0]}],
    "side_two": {"point": [10, 10, 10], "normal": [0, 1, 0]},
  }]});
  run(&mut s, "fillet", round);
  assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::FullRoundFillet { .. }));
  let v = volume(&s);
  assert!((v - (20.0 * 10.0 * 15.0 + std::f64::consts::PI * 12.5 * 20.0)).abs() < 1e-3, "{v}");
  assert_eq!(s.model.bodies[0].faces().len(), 6);
}

/// `feature.edit`'s picks give an edited fillet or chamfer its edges again, resolved on the model
/// before it (the marker rolled back), and `marker_after` leaves the marker just after it, as
/// Edit Feature shows it; a fillet too big for its edges fails with Fusion's message.
#[test]
fn edit_picks_marker_after_and_the_too_big_message() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  let f = run(&mut s, "fillet", json!({"edges": [{"point": [10, 0, 20]}], "radius": 2}))["feature"].as_u64().unwrap();
  run(&mut s, "chamfer", json!({"edges": [{"point": [0, 10, 20]}], "distance": 1}));
  // Edit Feature on the fillet: the marker before it, then the edit puts it just after it
  run(&mut s, "timeline.marker", json!({"at": 1}));
  let v = run(
    &mut s,
    "feature.edit",
    json!({"feature": f, "set": {}, "picks": {"edges": [{"point": [10, 20, 20]}], "radius": 3}, "marker_after": true}),
  );
  assert_eq!(v["feature"], json!(f));
  assert_eq!(s.design.marker, Some(2));
  let FeatureKind::Fillet { edges, radius, .. } = &s.design.features[1].kind else { panic!() };
  assert_eq!(edges.len(), 1);
  assert!((edges[0].point[1] - 20.0).abs() < 1e-6, "the back edge now: {:?}", edges[0].point);
  assert!(radius.starts_with('3'), "{radius}");
  // a chamfer's picks, and nothing else takes them
  let c = s.design.features[2].id.0;
  run(&mut s, "timeline.marker", json!({"at": 2}));
  run(&mut s, "feature.edit", json!({"feature": c, "set": {}, "picks": {"edges": [{"point": [20, 10, 20]}]}}));
  let FeatureKind::Chamfer { edges, .. } = &s.design.features[2].kind else { panic!() };
  assert!((edges[0].point[0] - 20.0).abs() < 1e-6, "{:?}", edges[0].point);
  let b = s.design.features[0].id.0;
  assert!(s.execute("feature.edit", &json!({"feature": b, "set": {}, "picks": {}})).is_err());
  // too big: Fusion's message, without the operation's name in front
  run(&mut s, "timeline.marker", json!({"at": null}));
  let v = s.execute("feature.edit", &json!({"feature": f, "set": {"radius": "50"}})).unwrap();
  let e = v["error"].as_str().unwrap_or_default();
  assert!(e.starts_with("The fillet/chamfer could not be created at the requested size."), "{v}");
}
