//! Tests for the surface tools: extend, untrim, trim with a body or sketch, variable offset,
//! and surfaces trimmed or extended and stitched back into solids.

use std::f64::consts::PI;

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

fn area(s: &Session, id: u32) -> f64 {
  s.model.bodies.iter().find(|b| b.id.0 == id).unwrap().shape.mass().unwrap().area
}

fn last(s: &Session) -> u32 {
  s.model.bodies.last().unwrap().id.0
}

fn close(a: f64, b: f64, tol: f64) -> bool {
  (a - b).abs() < tol
}

/// A flat 20 × 20 surface on XY, centred on the origin.
fn square(s: &mut Session) -> u32 {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [-10, -10], "corner2": [10, 10]}));
  let sk = s.edit_sketch.unwrap().0;
  run(s, "surface.patch", json!({"sketch": sk}));
  last(s)
}

/// A vertical 20 × 20 surface in the XZ plane (x and z from -10 to 10).
fn wall(s: &mut Session) -> u32 {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.line", json!({"points": [[-10, 0], [10, 0]]}));
  run(s, "surface.extrude", json!({"distance": 10, "symmetric": true}));
  last(s)
}

#[test]
fn extend_flat_edges_three_ways() {
  for (mode, z) in [("natural", 0.0), ("tangent", 0.0), ("perpendicular", 5.0)] {
    let mut s = Session::default();
    let id = square(&mut s);
    run(&mut s, "surface.extend", json!({"edges": [{"point": [10, 0, 0]}], "distance": 5, "mode": mode}));
    assert!(s.model.errors().is_empty(), "{mode}: {:?}", s.model.errors());
    assert!(close(area(&s, id), 500.0, 1e-6), "{mode}: {}", area(&s, id));
    let m = s.model.bodies[0].shape.mass().unwrap();
    if z == 0.0 {
      // the plane carries on and the strip merges into the face
      assert!(close(m.bbox_max.x, 15.0, 1e-6), "{mode}: {m:?}");
      assert_eq!(s.model.bodies[0].faces().len(), 1, "{mode}");
    } else {
      assert!(close(m.bbox_max.z - m.bbox_min.z, z, 1e-6), "{mode}: {m:?}");
      assert_eq!(s.model.bodies[0].faces().len(), 2, "{mode}");
    }
  }
  // inner edges can't be extended
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let r = s.execute("surface.extend", &json!({"edges": [{"point": [5, 0, 10]}], "distance": 2})).unwrap();
  assert!(r["error"].as_str().unwrap().contains("open"), "{r}");
}

#[test]
fn extend_curved_surfaces() {
  // a cylinder's rim: the cylinder grows
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "surface.extrude", json!({"distance": 10}));
  let id = last(&s);
  run(&mut s, "surface.extend", json!({"edges": [{"point": [-5, 0, 10]}], "distance": 4}));
  assert!(close(area(&s, id), 2.0 * PI * 5.0 * 14.0, 1e-3), "{}", area(&s, id));
  // a quarter cylinder's straight edge: natural carries the arc on round, tangent goes straight
  for (mode, x_min) in [("natural", 10.0 * (0.5f64).cos()), ("tangent", 0.0)] {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.arc", json!({"center": [0, 0], "start": [10, 0], "end": [0, 10]}));
    run(&mut s, "surface.extrude", json!({"distance": 4}));
    let id = last(&s);
    run(&mut s, "surface.extend", json!({"edges": [{"point": [0, 10, 2]}], "distance": 5, "mode": mode}));
    assert!(s.model.errors().is_empty(), "{mode}: {:?}", s.model.errors());
    assert!(close(area(&s, id), (PI / 2.0 * 10.0 + 5.0) * 4.0, 1e-3), "{mode}: {}", area(&s, id));
    let m = s.model.bodies[0].shape.mass().unwrap();
    if mode == "natural" {
      // 5 mm of arc past the Y axis: angle 0.5 rad, so x = -10 sin 0.5
      assert!(close(m.bbox_min.x, -10.0 * (0.5f64).sin(), 1e-4) && m.bbox_max.y <= 10.0 + 1e-6, "{m:?} {x_min}");
    } else {
      assert!(close(m.bbox_min.x, -5.0, 1e-4) && close(m.bbox_max.y, 10.0, 1e-6), "{m:?}");
    }
  }
  // a spline extrusion's end: continued as a B-spline
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 4], [20, 0], [30, 3]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  let id = last(&s);
  let a0 = area(&s, id);
  run(&mut s, "surface.extend", json!({"edges": [{"point": [30, 3, 2.5]}], "distance": 6}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(area(&s, id), a0 + 30.0, 0.3), "{a0} {}", area(&s, id));
  assert!(s.model.bodies[0].shape.mass().unwrap().bbox_max.x > 34.0);
}

#[test]
fn trim_with_bodies() {
  // a solid tool: the part of the wall inside the box goes, or (flipped) stays
  let mut s = Session::default();
  let w = wall(&mut s);
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let tool = last(&s);
  run(&mut s, "surface.trim", json!({"bodies": [w], "tool": {"body": tool}}));
  assert!(close(area(&s, w), 300.0, 1e-6), "{}", area(&s, w));
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"flip": true}}));
  assert!(close(area(&s, w), 100.0, 1e-6), "{}", area(&s, w));
  // keep points pick pieces instead
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"flip": false, "keep": [[8, 0, 5]]}}));
  assert!(close(area(&s, w), 300.0, 1e-6), "{}", area(&s, w));
  assert!(s.model.body(fox_model::BodyId(tool)).is_some(), "the tool stays");
  // the tool can't trim itself
  assert!(s.execute("surface.trim", &json!({"bodies": [tool], "tool": {"body": tool}})).is_err());

  // a surface tool: one side of a horizontal sheet at z = 3 stays, flip keeps the other
  let mut s = Session::default();
  let w = wall(&mut s);
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 3}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  run(&mut s, "sketch.rect", json!({"corner1": [-20, -20], "corner2": [20, 20]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.patch", json!({"sketch": sk}));
  let sheet = last(&s);
  run(&mut s, "surface.trim", json!({"bodies": [w], "tool": {"body": sheet}}));
  let a = area(&s, w);
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"flip": true}}));
  let b = area(&s, w);
  assert!(close(a + b, 400.0, 1e-6) && (close(a, 260.0, 1e-6) || close(a, 140.0, 1e-6)), "{a} {b}");
}

#[test]
fn trim_with_sketches_and_untrim() {
  // a circle cut out of a flat sheet, then the hole untrimmed again
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [2, 1], "radius": 3}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": {"sketch": sk}}));
  assert!(close(area(&s, id), 400.0 - 9.0 * PI, 1e-6), "{}", area(&s, id));
  let face = s.model.bodies[0].faces()[0].id;
  run(&mut s, "surface.untrim", json!({"faces": [{"body": id, "face": face}], "loops": "inner"}));
  assert!(close(area(&s, id), 400.0, 1e-6), "{}", area(&s, id));

  // an open sketch line: the side to its left (here -x) goes; flip keeps it
  let mut s = Session::default();
  let w = wall(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[2, -5], [2, 5]]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.trim", json!({"bodies": [w], "tool": {"sketch": sk}}));
  assert!(close(area(&s, w), 160.0, 1e-6), "{}", area(&s, w));
  assert!(s.model.bodies[0].shape.mass().unwrap().bbox_min.x > 1.99);
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"flip": true}}));
  assert!(close(area(&s, w), 240.0, 1e-6), "{}", area(&s, w));

  // a half cylinder untrimmed back to the whole cylinder
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "surface.extrude", json!({"distance": 10}));
  let id = last(&s);
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": "yz"}));
  assert!(close(area(&s, id), PI * 5.0 * 10.0, 1e-3), "{}", area(&s, id));
  run(&mut s, "surface.untrim", json!({"bodies": [id]}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(area(&s, id), 2.0 * PI * 5.0 * 10.0, 1e-3), "{}", area(&s, id));
}

#[test]
fn variable_offset() {
  // an L of two faces: 20 × 5 at y = 0 and 10 × 5 at x = 20
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0], [20, 10]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  let id = last(&s);
  run(
    &mut s,
    "surface.offset",
    json!({"bodies": [id], "distance": 1, "face_distances": [{"face": {"point": [10, 0, 2.5]}, "distance": 3}]}),
  );
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let off = s.model.bodies.last().unwrap();
  assert_ne!(off.id.0, id);
  let faces = off.faces();
  let long = faces.iter().find(|f| f.normal.y.abs() > 0.99).unwrap();
  let short = faces.iter().find(|f| f.normal.x.abs() > 0.99).unwrap();
  assert!(close(long.origin.y.abs(), 3.0, 1e-6), "{long:?}");
  assert!(close((short.origin.x - 20.0).abs(), 1.0, 1e-6), "{short:?}");
  // the per-face distance is an expression like any other: it follows a parameter
  run(&mut s, "param.add", json!({"name": "gap", "expr": "4 mm"}));
  let f = s.design.features.last().unwrap().id.0;
  let mut kind = serde_json::to_value(&s.design.feature(fox_model::FeatureId(f)).unwrap().kind).unwrap();
  kind["face_distances"][0]["distance"] = json!("gap");
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"face_distances": kind["face_distances"]}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let long = *s.model.bodies.last().unwrap().faces().iter().find(|g| g.normal.y.abs() > 0.99).unwrap();
  assert!(close(long.origin.y.abs(), 4.0, 1e-6), "{long:?}");
}

#[test]
fn trimmed_and_extended_surfaces_stitch_back_into_a_solid() {
  // a box's sides cut down to z = 6 by a solid, extended back up, and stitched with the
  // untouched top and bottom: the box again
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let b = last(&s);
  run(&mut s, "surface.unstitch", json!({"bodies": [b]}));
  let faces: Vec<(u32, f64)> = s
    .model
    .bodies
    .iter()
    .map(|b| (b.id.0, b.shape.mass().unwrap().bbox_max.z - b.shape.mass().unwrap().bbox_min.z))
    .collect();
  let sides: Vec<u32> = faces.iter().filter(|f| f.1 > 1.0).map(|f| f.0).collect();
  let all: Vec<u32> = faces.iter().map(|f| f.0).collect();
  assert_eq!(sides.len(), 4);
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 6}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "box", json!({"plane": {"feature": pl}, "width": 30, "depth": 30, "height": 10}));
  let tool = last(&s);
  run(&mut s, "surface.trim", json!({"bodies": sides, "tool": {"body": tool}}));
  for &id in &sides {
    assert!(close(area(&s, id), 60.0, 1e-6), "{}", area(&s, id));
  }
  run(&mut s, "body.remove", json!({"bodies": [tool]}));
  let edges: Vec<Value> =
    [[5.0, 0.0], [-5.0, 0.0], [0.0, 5.0], [0.0, -5.0]].iter().map(|p| json!({"point": [p[0], p[1], 6.0]})).collect();
  run(&mut s, "surface.extend", json!({"edges": edges, "distance": 4}));
  for &id in &sides {
    assert!(close(area(&s, id), 100.0, 1e-6), "{}", area(&s, id));
  }
  run(&mut s, "surface.stitch", json!({"bodies": all}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert_eq!(s.model.bodies.len(), 1);
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!(s.model.bodies[0].shape.solid_count() == 1 && close(m.volume, 1000.0, 1e-3), "{m:?}");

  // a sheet trimmed by a sketch circle, the hole patched, then thickened: a plain plate
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 4}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": {"sketch": sk}}));
  run(&mut s, "surface.patch", json!({"edges": [{"point": [4, 0, 0]}]}));
  let plug = last(&s);
  run(&mut s, "surface.stitch", json!({"bodies": [id, plug]}));
  run(&mut s, "thicken", json!({"bodies": [id], "thickness": 2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let v: f64 = s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum();
  assert!(close(v, 800.0, 1e-3), "{v}");
}

#[test]
fn thicken_an_l_surface_with_sharp_and_rounded_corners() {
  // an open L ((0,0)-(30,0)-(30,20)) extruded 25 mm: a two-face surface with a 90° corner
  let thick = |p: Value| {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [30, 0]]}));
    run(&mut s, "sketch.line", json!({"points": [[30, 0], [30, 20]]}));
    run(&mut s, "surface.extrude", json!({"distance": 25}));
    let id = last(&s);
    let mut p = p;
    p["bodies"] = json!([id]);
    p["thickness"] = json!(2);
    run(&mut s, "thicken", p);
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    let solids: Vec<_> = s.model.bodies.iter().filter(|b| b.shape.solid_count() > 0).collect();
    assert_eq!(solids.len(), 1);
    let m = solids[0].shape.mass().unwrap();
    (m.volume, solids[0].shape.faces().unwrap().len())
  };
  // symmetric is 2 mm on each side, the outer corner filled square: an L section of 200 mm²
  let (v, n) = thick(json!({"symmetric": true}));
  assert!(close(v, 5000.0, 1e-3) && n == 8, "{v} {n}");
  // one side each way: outside (30·2 + 20·2 + 2·2 = 104 mm²) and inside (96 mm²)
  let (a, na) = thick(json!({}));
  let (b, nb) = thick(json!({"flip": true}));
  assert!(close(a + b, 5000.0, 1e-3) && na == 8 && nb == 8, "{a} {b} {na} {nb}");
  assert!(close(a.max(b), 2600.0, 1e-3), "{a} {b}");
  // rounded: the outside corner is a quarter disc of radius 2 instead of a square
  let (a, _) = thick(json!({"corners": "round"}));
  let (b, _) = thick(json!({"corners": "round", "flip": true}));
  assert!(close(a.max(b), (100.0 + PI) * 25.0, 1e-3) && close(a.min(b), 2400.0, 1e-3), "{a} {b}");
  let (v, _) = thick(json!({"corners": "round", "symmetric": true}));
  assert!(close(v, (196.0 + PI) * 25.0, 1e-3), "{v}");
}

#[test]
fn extend_closes_corners_between_adjacent_edges() {
  // two edges of the square meeting at (10, -10), extended by 5: the corner gap is filled and,
  // flat, everything merges into one 25 x 25 face
  let edges = json!([{"point": [10, 0, 0]}, {"point": [0, -10, 0]}]);
  for mode in ["natural", "tangent"] {
    let mut s = Session::default();
    let id = square(&mut s);
    run(&mut s, "surface.extend", json!({"edges": edges, "distance": 5, "mode": mode, "close_corners": true}));
    assert!(s.model.errors().is_empty(), "{mode}: {:?}", s.model.errors());
    assert!(close(area(&s, id), 625.0, 1e-6), "{mode}: {}", area(&s, id));
    assert_eq!(s.model.bodies[0].faces().len(), 1, "{mode}");
    // without closing, the notch stays
    let f = s.design.features.last().unwrap().id.0;
    run(&mut s, "feature.edit", json!({"feature": f, "set": {"close_corners": false}}));
    assert!(close(area(&s, id), 600.0, 1e-6), "{mode}: {}", area(&s, id));
  }
  // all four edges: a 30 x 30 square
  let mut s = Session::default();
  let id = square(&mut s);
  let all = json!([{"point": [10, 0, 0]}, {"point": [0, -10, 0]}, {"point": [-10, 0, 0]}, {"point": [0, 10, 0]}]);
  run(&mut s, "surface.extend", json!({"edges": all, "distance": 5, "close_corners": true}));
  assert!(close(area(&s, id), 900.0, 1e-6), "{}", area(&s, id));
  assert_eq!(s.model.bodies[0].faces().len(), 1);
  // perpendicular walls already meet along their side: nothing to close
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "surface.extend", json!({"edges": edges, "distance": 5, "mode": "perpendicular", "close_corners": true}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(area(&s, id), 600.0, 1e-6), "{}", area(&s, id));
  // a quarter cylinder's straight edge and rim: the corner carries the cylinder on round
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.arc", json!({"center": [0, 0], "start": [40, 0], "end": [0, 40]}));
  run(&mut s, "surface.extrude", json!({"distance": 30}));
  let id = last(&s);
  let arc = PI / 2.0 * 40.0;
  let rim = 40.0 * (PI / 4.0).cos();
  let edges = json!([{"point": [40, 0, 15]}, {"point": [rim, rim, 30]}]);
  run(&mut s, "surface.extend", json!({"edges": edges, "distance": 5, "close_corners": true}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(area(&s, id), (arc + 5.0) * 35.0, 1e-2), "{}", area(&s, id));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!(close(m.bbox_max.z, 35.0, 1e-6) && m.bbox_min.y < -4.0, "{m:?}");
  // the feature saves its corners and older files open without them
  let f = s.design.features.last().unwrap();
  assert!(serde_json::to_string(&f.kind).unwrap().contains("close_corners"));
}

#[test]
fn untrim_with_an_extension() {
  // a holed square untrimmed and grown by 3 all round: 26 x 26
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [2, 1], "radius": 3}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": {"sketch": sk}}));
  run(&mut s, "param.add", json!({"name": "grow", "expr": "3 mm"}));
  run(&mut s, "surface.untrim", json!({"bodies": [id], "extension": "grow"}));
  assert!(close(area(&s, id), 676.0, 1e-6), "{}", area(&s, id));
  // the extension follows its parameter
  run(&mut s, "param.set", json!({"name": "grow", "expr": "1 mm"}));
  assert!(close(area(&s, id), 484.0, 1e-6), "{}", area(&s, id));
  // a holes-only untrim keeps its boundary, so it takes no extension
  let f = s.design.features.last().unwrap().id.0;
  let r = s.execute("feature.edit", &json!({"feature": f, "set": {"loops": "inner"}})).unwrap();
  assert!(r["error"].as_str().is_some_and(|e| e.contains("outer boundary")), "{r}");
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"loops": "inner", "extension": ""}}));
  assert!(close(area(&s, id), 400.0, 1e-6), "{}", area(&s, id));

  // an untrimmed square grows too
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "surface.untrim", json!({"bodies": [id], "extension": 2}));
  assert!(close(area(&s, id), 576.0, 1e-6), "{}", area(&s, id));

  // a half cylinder: the whole cylinder again (it closes round), 2 mm longer at each end
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "surface.extrude", json!({"distance": 10}));
  let id = last(&s);
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": "yz"}));
  run(&mut s, "surface.untrim", json!({"bodies": [id], "extension": 2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(area(&s, id), 2.0 * PI * 5.0 * 14.0, 1e-3), "{}", area(&s, id));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!(close(m.bbox_min.z, -2.0, 1e-6) && close(m.bbox_max.z, 12.0, 1e-6), "{m:?}");

  // a spline extrusion: the B-spline is carried on past its ends
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 4], [20, 0], [30, 3]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  let id = last(&s);
  run(&mut s, "surface.untrim", json!({"bodies": [id], "extension": 2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let m = s.model.bodies[0].shape.mass().unwrap();
  // 2 mm along the spline at each end, which leaves it at an angle
  assert!(m.bbox_max.x > 31.2 && m.bbox_min.x < -1.2, "{m:?}");
  assert!(close(m.bbox_min.z, -2.0, 1e-3) && close(m.bbox_max.z, 7.0, 1e-3), "{m:?}");
}

#[test]
fn trim_removes_picked_pieces() {
  // a circle in a square: by default the disc goes; picking the outside removes that instead
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [2, 1], "radius": 3}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": {"sketch": sk}, "remove": [[-8, -8, 0]]}));
  assert!(close(area(&s, id), 9.0 * PI, 1e-6), "{}", area(&s, id));
  // the picks are saved with the feature and can be edited
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"remove": [[2, 1, 0]]}}));
  assert!(close(area(&s, id), 400.0 - 9.0 * PI, 1e-6), "{}", area(&s, id));
  let r = s.execute("surface.trim", &json!({"bodies": [id], "tool": {"sketch": sk}, "remove": [[1, 2]]}));
  assert!(r.is_err());

  // a wall cut by two sketch lines into three strips: the middle one goes
  let mut s = Session::default();
  let w = wall(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[-3, -5], [-3, 5]]}));
  run(&mut s, "sketch.line", json!({"points": [[3, -5], [3, 5]]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.trim", json!({"bodies": [w], "tool": {"sketch": sk}, "remove": [[0, 0, 4]]}));
  assert!(close(area(&s, w), 280.0, 1e-6), "{}", area(&s, w));
  assert_eq!(s.model.bodies[0].faces().len(), 2);

  // a plane trim: the piece behind the plane, which would stay, is picked to go instead
  let mut s = Session::default();
  let id = square(&mut s);
  let plane = json!({"origin": [4, 0, 0], "normal": [1, 0, 0]});
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": plane, "remove": [[0, 0, 0]]}));
  assert!(close(area(&s, id), 120.0, 1e-6), "{}", area(&s, id));
  assert!(s.model.bodies[0].shape.mass().unwrap().bbox_min.x > 3.99);
  // and with a body tool
  let mut s = Session::default();
  let id = square(&mut s);
  run(&mut s, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [0, 0], "operation": "new_body"}));
  let tool = last(&s);
  run(&mut s, "move", json!({"bodies": [tool], "translate": [0, 0, -2]}));
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": {"body": tool}, "remove": [[9, 9, 0]]}));
  assert!(close(area(&s, id), 16.0, 1e-6), "{}", area(&s, id));
}

/// Runs a JSONL example script.
fn run_example(script: &str) -> Session {
  let mut s = Session::default();
  for line in script.lines().filter(|l| l.trim_start().starts_with('{')) {
    let mut v: Value = serde_json::from_str(line).unwrap();
    let cmd = v["cmd"].as_str().unwrap().to_string();
    v.as_object_mut().unwrap().remove("cmd");
    run(&mut s, &cmd, v);
  }
  s
}

#[test]
fn surface_examples_build() {
  // the panel: (78 x 58 - the vent - the 6 x 44 notch) x 2 mm
  let s = run_example(include_str!("../../../examples/surface_panel.jsonl"));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert_eq!(s.model.bodies.len(), 1);
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!(close(m.volume, 2.0 * (78.0 * 58.0 - 36.0 * PI - 6.0 * 44.0), 1e-3), "{}", m.volume);
  assert!(close(m.bbox_min.x, -4.0, 1e-6) && close(m.bbox_max.y, 54.0, 1e-6), "{m:?}");
  let s = run_example(include_str!("../../../examples/surface_shroud.jsonl"));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(s.model.bodies[0].shape.mass().unwrap().volume > 0.0);
}
