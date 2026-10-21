//! Edits the way people make them in Fusion (change a sketch dimension, roll the timeline back and
//! insert a feature, delete one), checking that later features stay on the faces and edges they
//! were put on. Volumes are worked out by hand.

use fox_engine::Session;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn run(s: &mut Session, c: &str, p: Value) -> Value {
  s.execute(c, &p).unwrap_or_else(|e| panic!("{c} {p}: {}", e.0))
}
fn vol(s: &Session) -> f64 {
  s.model.bodies.iter().filter(|b| b.shape.solid_count() > 0).map(|b| b.shape.mass().unwrap().volume).sum()
}
fn check(what: &str, s: &Session, want: f64) {
  assert!(s.model.errors().is_empty(), "{what}: {:?}", s.model.errors());
  let warnings: Vec<_> = s.model.status.iter().filter_map(|st| st.warning.clone()).collect();
  assert!(warnings.is_empty(), "{what}: {warnings:?}");
  let v = vol(s);
  assert!((v - want).abs() < 1e-6 * want, "{what}: volume {v:.3}, want {want:.3}");
}

#[test]
fn later_features_follow_edits() {
  // a quarter-round fillet of radius r takes (1 - pi/4) r^2 per length of edge
  let k = 1.0 - PI / 4.0;
  // 1: double the sketch width under a fillet
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
    let info = run(&mut s, "sketch.info", json!({}));
    let bottom = info["curves"].as_array().unwrap().iter().find(|c| c["kind"]["type"] == "line").unwrap()["id"].clone();
    let d = run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [bottom], "value": "60"}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    run(&mut s, "fillet", json!({"edges": [{"point": [30, 0, 20]}], "radius": 3}));
    check("1 before", &s, 60.0 * 40.0 * 20.0 - k * 9.0 * 60.0);
    let cid = d["constraint"].clone();
    run(&mut s, "sketch.edit", json!({"sketch": 1}));
    run(&mut s, "sketch.set_dimension", json!({"constraint": cid, "value": "120"}));
    run(&mut s, "sketch.finish", json!({}));
    check("1 width 60 -> 120", &s, 120.0 * 40.0 * 20.0 - k * 9.0 * 120.0);
  }
  // 2: insert a slot before the fillet, splitting the filleted edge in two
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    run(&mut s, "fillet", json!({"edges": [{"point": [30, 0, 20]}], "radius": 3}));
    run(&mut s, "timeline.marker", json!({"at": 2}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.rect", json!({"corner1": [25, -5], "corner2": [35, 10]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 5, "flip": true, "operation": "cut"}));
    run(&mut s, "timeline.marker", json!({"at": null}));
    check("2 slot inserted before fillet", &s, 60.0 * 40.0 * 20.0 - 10.0 * 10.0 * 5.0 - k * 9.0 * 50.0);
    let slot = s.design.features.iter().find(|f| f.name == "Extrude2").unwrap().id.0;
    let sk2 = s.design.features.iter().find(|f| f.name == "Sketch2").unwrap().id.0;
    run(&mut s, "feature.delete", json!({"feature": slot}));
    run(&mut s, "feature.delete", json!({"feature": sk2}));
    check("4 slot deleted again", &s, 60.0 * 40.0 * 20.0 - k * 9.0 * 60.0);
  }
  // 3: chamfer a hole's top edge, then move the hole by its dimension
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.circle", json!({"center": [20, 20], "radius": 5}));
    let info = run(&mut s, "sketch.info", json!({}));
    let circle = info["curves"].as_array().unwrap().iter().find(|c| c["kind"]["type"] == "circle").unwrap().clone();
    let center = circle["kind"]["center"].clone();
    let dim = run(&mut s, "sketch.dimension", json!({"type": "horizontal", "points": [0, center], "value": "20"}));
    run(&mut s, "sketch.dimension", json!({"type": "vertical", "points": [0, center], "value": "20"}));
    run(&mut s, "sketch.finish", json!({}));
    let sk = s.design.features.last().unwrap().id.0;
    run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
    run(&mut s, "chamfer", json!({"edges": [{"point": [25, 20, 20]}], "distance": 1}));
    // a 45 degree chamfer ring on a hole of radius r: volume pi*d^2*(r + d/3)
    let ring = PI * 1.0 * (5.0 + 1.0 / 3.0);
    let hole = PI * 25.0 * 20.0;
    check("3 before", &s, 48000.0 - hole - ring);
    run(&mut s, "sketch.edit", json!({"sketch": sk}));
    run(&mut s, "sketch.set_dimension", json!({"constraint": dim["constraint"], "value": "40"}));
    run(&mut s, "sketch.finish", json!({}));
    check("3 hole moved x 20 -> 40", &s, 48000.0 - hole - ring);
  }
}

fn block(s: &mut Session) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
  run(s, "sketch.finish", json!({}));
  run(s, "extrude", json!({"distance": 20}));
}

/// The block's top face, as a picked face.
fn top_face(s: &Session) -> Value {
  let b = &s.model.bodies[0];
  let f = b.faces().iter().filter(|f| f.normal.z > 0.99).max_by(|a, c| a.origin.z.total_cmp(&c.origin.z)).unwrap().id;
  json!({"body": b.id.0, "face": f})
}

#[test]
fn faces_split_by_inserted_features_keep_their_features() {
  // 5: boss sketched on the top face; a slot across the top inserted before it
  {
    let mut s = Session::default();
    block(&mut s);
    let tf = top_face(&s);
    run(&mut s, "sketch.create", json!({"plane": tf}));
    run(&mut s, "sketch.circle", json!({"center": [10, 10], "radius": 4}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 10, "operation": "join"}));
    let boss = std::f64::consts::PI * 16.0 * 10.0;
    check("5 before", &s, 48000.0 + boss);
    run(&mut s, "timeline.marker", json!({"at": 2}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.rect", json!({"corner1": [25, -5], "corner2": [35, 45]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 5, "flip": true, "operation": "cut"}));
    run(&mut s, "timeline.marker", json!({"at": null}));
    check("5 slot inserted before the boss", &s, 48000.0 - 10.0 * 40.0 * 5.0 + boss);
  }
  // 6: shell open at the top; a slot across the top inserted before it splits the top in two
  {
    let mut s = Session::default();
    block(&mut s);
    let tf = top_face(&s);
    run(&mut s, "shell", json!({"faces": [tf], "thickness": 2}));
    check("6 before", &s, 48000.0 - 56.0 * 36.0 * 18.0);
    run(&mut s, "timeline.marker", json!({"at": 2}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.rect", json!({"corner1": [25, -5], "corner2": [35, 45]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 5, "flip": true, "operation": "cut"}));
    run(&mut s, "timeline.marker", json!({"at": null}));
    // both halves open: a tray (walls and floor 2) bridged across by the slot as a U channel 2 thick
    // tray: 48000 - 56*36*18; slot part removed from the tray's walls: 10*2*5 on each long side;
    // channel: (14 wide x 7 tall - 10 x 5 hollow) over the 36 inside the walls
    let tray = 48000.0 - 56.0 * 36.0 * 18.0 - 2.0 * (10.0 * 2.0 * 5.0);
    let channel = (14.0 * 7.0 - 10.0 * 5.0) * 36.0;
    check("6 slot inserted before the shell", &s, tray + channel);
  }
}

/// Volume a convex fillet of radius `r` takes off the rim of a round boss of radius `big`: the
/// corner left outside a quarter circle, (1 - pi/4) r^2, swept round at its centroid (Pappus).
fn rim_fillet(big: f64, r: f64) -> f64 {
  let k = 1.0 - PI / 4.0;
  k * r * r * 2.0 * PI * (big - r + r / (6.0 * k))
}

/// A circle's centre point id in the sketch being edited.
fn circle_centre(s: &mut Session, at: [f64; 2]) -> Value {
  let info = run(s, "sketch.info", json!({}));
  info["curves"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|c| c["kind"]["type"] == "circle")
    .find(|c| {
      let id = &c["kind"]["center"];
      let p = info["points"].as_array().unwrap().iter().find(|p| p["id"] == *id).unwrap();
      (p["at"][0].as_f64().unwrap() - at[0]).abs() < 1e-6 && (p["at"][1].as_f64().unwrap() - at[1]).abs() < 1e-6
    })
    .unwrap()["kind"]["center"]
    .clone()
}

// 7-9: edits that leave a look-alike where the referenced face or edge was; position lookup alone
// would pick the look-alike, the faces' names (from the kernel's history) keep the right one.

#[test]
fn fillet_stays_on_a_moved_boss() {
  // 7: a fillet round a boss's rim, then the boss moved past a second boss: the fillet stays on
  // the boss it was put on (the second boss's rim is now the closest look-alike to where the
  // filleted rim was)
  {
    let mut s = Session::default();
    block(&mut s);
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.circle", json!({"center": [10, 20], "radius": 4}));
    run(&mut s, "sketch.circle", json!({"center": [30, 20], "radius": 5}));
    let c = circle_centre(&mut s, [10.0, 20.0]);
    let dim = run(&mut s, "sketch.dimension", json!({"type": "horizontal", "points": [0, c], "value": "10"}));
    run(&mut s, "sketch.dimension", json!({"type": "vertical", "points": [0, c], "value": "20"}));
    run(&mut s, "sketch.finish", json!({}));
    let sk = s.design.features.last().unwrap().id.0;
    run(&mut s, "extrude", json!({"distance": 10, "operation": "join"}));
    run(&mut s, "fillet", json!({"edges": [{"point": [10, 24, 30]}], "radius": 1}));
    let bosses = PI * 16.0 * 10.0 + PI * 25.0 * 10.0;
    check("7 before", &s, 48000.0 + bosses - rim_fillet(4.0, 1.0));
    run(&mut s, "sketch.edit", json!({"sketch": sk}));
    run(&mut s, "sketch.set_dimension", json!({"constraint": dim["constraint"], "value": "50"}));
    run(&mut s, "sketch.finish", json!({}));
    check("7 boss moved x 10 -> 50", &s, 48000.0 + bosses - rim_fillet(4.0, 1.0));
  }
}

#[test]
fn sketch_stays_on_a_shortened_boss() {
  // 8: a sketch on a boss's top, then the boss made shorter: the sketch stays on the boss (a
  // wide boss's top is now in the plane the sketch was on, right next to where it was)
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    let top = json!({"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]});
    run(&mut s, "sketch.create", json!({"plane": top}));
    run(&mut s, "sketch.circle", json!({"center": [10, 20], "radius": 4}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 10, "operation": "join"}));
    let small = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": top}));
    run(&mut s, "sketch.rect", json!({"corner1": [30, 5], "corner2": [70, 35]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 10, "operation": "join"}));
    run(&mut s, "sketch.create", json!({"plane": {"point": [10, 20, 30], "normal": [0, 0, 1]}}));
    run(&mut s, "sketch.circle", json!({"center": [10, 20], "radius": 2}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 5, "operation": "join"}));
    let pin = PI * 4.0 * 5.0;
    check("8 before", &s, 64000.0 + PI * 16.0 * 10.0 + 12000.0 + pin);
    assert!((s.model.bounds().unwrap().1.z - 35.0).abs() < 1e-6);
    run(&mut s, "feature.edit", json!({"feature": small, "set": {"extent": {"kind": "one_side", "distance": "4"}}}));
    check("8 boss 10 -> 4 tall", &s, 64000.0 + PI * 16.0 * 4.0 + 12000.0 + pin);
    // the pin stands on the shorter boss: 24 + 5 (on the wide boss's plane it would float at 30..35)
    assert_eq!(s.model.bodies.len(), 1);
    let pin_top = s.model.bounds().unwrap().1.z;
    assert!((pin_top - 30.0).abs() < 1e-6, "the highest point is the wide boss's top, not a pin at {pin_top}");
  }
}

#[test]
fn chamfer_stays_on_a_moved_hole() {
  // 9: a chamfer on the inner rim of a hole, then the hole moved next to a second, equal hole:
  // the chamfer stays on its hole
  {
    let mut s = Session::default();
    block(&mut s);
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.circle", json!({"center": [10, 20], "radius": 3}));
    run(&mut s, "sketch.circle", json!({"center": [25, 20], "radius": 4}));
    let c = circle_centre(&mut s, [10.0, 20.0]);
    let dim = run(&mut s, "sketch.dimension", json!({"type": "horizontal", "points": [0, c], "value": "10"}));
    run(&mut s, "sketch.dimension", json!({"type": "vertical", "points": [0, c], "value": "20"}));
    run(&mut s, "sketch.finish", json!({}));
    let sk = s.design.features.last().unwrap().id.0;
    run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
    run(&mut s, "chamfer", json!({"edges": [{"point": [10, 23, 20]}], "distance": 1}));
    // a 45 degree chamfer ring on a hole of radius r: volume pi*d^2*(r + d/3)
    let holes = PI * 9.0 * 20.0 + PI * 16.0 * 20.0;
    let ring = PI * (3.0 + 1.0 / 3.0);
    check("9 before", &s, 48000.0 - holes - ring);
    run(&mut s, "sketch.edit", json!({"sketch": sk}));
    run(&mut s, "sketch.set_dimension", json!({"constraint": dim["constraint"], "value": "50"}));
    run(&mut s, "sketch.finish", json!({}));
    check("9 hole moved x 10 -> 50", &s, 48000.0 - holes - ring);
  }
}

#[test]
fn drawing_dimension_follows_its_edge() {
  // 10: a drawing dimension across the block's top front edge, then the sketch made wider: the
  // dimension measures the edge where it is now
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
  let info = run(&mut s, "sketch.info", json!({}));
  let bottom = info["curves"].as_array().unwrap().iter().find(|c| c["kind"]["type"] == "line").unwrap()["id"].clone();
  let d = run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [bottom], "value": "60"}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"distance": 20}));
  run(&mut s, "drawing.create", json!({"views": ["front"]}));
  let view = s.design.drawings[0].views[0].id;
  run(
    &mut s,
    "drawing.dimension",
    json!({"view": view, "a": [0, 0, 20], "b": [60, 0, 20], "orientation": "horizontal"}),
  );
  let texts = |s: &Session| {
    let p = fox_model::drawing::render(&s.design.drawings[0], &s.design, &s.model).unwrap();
    p.texts().into_iter().map(str::to_string).collect::<Vec<_>>()
  };
  assert!(texts(&s).iter().any(|t| t.starts_with("60")), "{:?}", texts(&s));
  run(&mut s, "sketch.edit", json!({"sketch": 1}));
  run(&mut s, "sketch.set_dimension", json!({"constraint": d["constraint"], "value": "120"}));
  run(&mut s, "sketch.finish", json!({}));
  assert!(texts(&s).iter().any(|t| t.starts_with("120")), "{:?}", texts(&s));
}
