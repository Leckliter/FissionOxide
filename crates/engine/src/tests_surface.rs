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

/// A sketch on the XY plane offset by `z`, holding a half circle of radius 10 over the X axis
/// drawn from x = `from` to the other end.
fn half_circle(s: &mut Session, z: f64, from: f64) -> u32 {
  if z == 0.0 {
    run(s, "sketch.create", json!({"plane": "xy"}));
  } else {
    run(s, "plane.offset", json!({"plane": "xy", "offset": z}));
    let plane = s.design.features.last().unwrap().id.0;
    run(s, "sketch.create", json!({"plane": {"feature": plane}}));
  }
  run(s, "sketch.arc", json!({"start": [from, 0], "mid": [0, 10], "end": [-from, 0]}));
  s.edit_sketch.unwrap().0
}

#[test]
fn surface_loft_between_open_arcs() {
  // two half circles 20 apart: half a cylinder's side, π·10·20, whichever way each runs
  for from in [10.0, -10.0] {
    let mut s = Session::default();
    let a = half_circle(&mut s, 0.0, 10.0);
    let b = half_circle(&mut s, 20.0, from);
    run(&mut s, "surface.loft", json!({"sections": [a, {"sketch": b}]}));
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    assert_eq!(s.model.bodies.len(), 1);
    let body = &s.model.bodies[0];
    assert_eq!(body.shape.solid_count(), 0, "a surface, not a solid");
    let m = body.shape.mass().unwrap();
    assert!(close(m.area, PI * 10.0 * 20.0, 1e-3), "{from}: {}", m.area);
    assert!(close(m.bbox_max.z - m.bbox_min.z, 20.0, 1e-6) && close(m.bbox_max.y, 10.0, 1e-6), "{m:?}");
    let loft = s.design.features.last().unwrap();
    assert!(matches!(loft.kind, fox_model::FeatureKind::SurfaceLoft { .. }));
    assert!(loft.kind.depends_on().contains(&fox_model::FeatureId(b)));
  }
  // a loft needs two sections
  let mut s = Session::default();
  let a = half_circle(&mut s, 0.0, 10.0);
  assert!(s.execute("surface.loft", &json!({"sections": [a]})).is_err());
}

#[test]
fn surface_sweep_open_profile_along_paths() {
  // a 10 long line across the start of the path, swept along a straight 30 line and a quarter
  // circle of radius 20: flat strips of 10 × the path length
  for (straight, len) in [(true, 30.0), (false, PI / 2.0 * 20.0)] {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    if straight {
      run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
    } else {
      let h = 20.0 * (0.5f64).sqrt();
      run(&mut s, "sketch.arc", json!({"start": [0, 0], "mid": [20.0 - h, h], "end": [20, 20]}));
    }
    let p = s.edit_sketch.unwrap().0;
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[-5, 0], [5, 0]]}));
    let prof = s.edit_sketch.unwrap().0;
    run(&mut s, "surface.sweep", json!({"sketch": prof, "path": p}));
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    assert_eq!(s.model.bodies.len(), 1);
    let body = &s.model.bodies[0];
    assert_eq!(body.shape.solid_count(), 0, "a surface, not a solid");
    let m = body.shape.mass().unwrap();
    assert!(close(m.area, 10.0 * len, 1e-3), "{len}: {}", m.area);
    assert!(close(m.bbox_max.z - m.bbox_min.z, 0.0, 1e-6), "flat in the path's plane: {m:?}");
  }
  // an open arc profile along a straight path: half a cylinder's side, either orientation
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
  let p = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.arc", json!({"start": [10, 0], "mid": [0, 10], "end": [-10, 0]}));
  let prof = s.edit_sketch.unwrap().0;
  for orientation in ["perpendicular", "parallel"] {
    run(&mut s, "surface.sweep", json!({"sketch": prof, "path": p, "orientation": orientation}));
    assert!(s.model.errors().is_empty(), "{orientation}: {:?}", s.model.errors());
    let m = s.model.bodies.last().unwrap().shape.mass().unwrap();
    assert_eq!(s.model.bodies.last().unwrap().shape.solid_count(), 0);
    assert!(close(m.area, PI * 10.0 * 30.0, 1e-3), "{orientation}: {}", m.area);
  }
  assert!(s.execute("surface.sweep", &json!({"sketch": prof, "path": p, "orientation": "sideways"})).is_err());
}
