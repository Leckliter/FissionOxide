//! Tests for the SURFACE tab's Create commands as Fusion's dialogs run them: Extrude and Revolve
//! of profiles with Start and Direction, Patch with continuity, Ruled from one edge, Offset with
//! an Offset Type, and the surface Sweep and Loft.

use std::f64::consts::PI;

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      assert!(s.model.errors().is_empty(), "{cmd} {p}: {:?}", s.model.errors());
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn mass(s: &Session) -> fox_kernel::MassProps {
  s.model.bodies.last().unwrap().shape.mass().unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
  (a - b).abs() < tol
}

/// A 10 × 10 square sketch on XY; its id.
fn square(s: &mut Session) -> u64 {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  let sk = s.edit_sketch.unwrap().0 as u64;
  run(s, "sketch.finish", json!({}));
  sk
}

#[test]
fn extrude_a_profile_two_sides_and_from_an_offset() {
  let mut s = Session::default();
  let sk = square(&mut s);
  run(&mut s, "surface.extrude", json!({"sketch": sk, "profiles": [[5, 5]], "distance": 4}));
  let m = mass(&s);
  // the four walls of the square, open top and bottom
  assert_eq!(s.model.bodies[0].shape.solid_count(), 0);
  assert!(close(m.area, 160.0, 1e-6) && close(m.bbox_max.z, 4.0, 1e-6), "{m:?}");
  let mut s = Session::default();
  let sk = square(&mut s);
  run(&mut s, "surface.extrude", json!({"sketch": sk, "profiles": [0], "distance": 4, "distance2": 2, "offset": 1}));
  let m = mass(&s);
  assert!(close(m.area, 240.0, 1e-6) && close(m.bbox_min.z, -1.0, 1e-6) && close(m.bbox_max.z, 5.0, 1e-6), "{m:?}");
  assert!(
    s.execute("surface.extrude", &json!({"sketch": sk, "distance": 4, "distance2": 2, "symmetric": true})).is_err()
  );
}

#[test]
fn revolve_a_profile_symmetric() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [10, 4]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  run(
    &mut s,
    "surface.revolve",
    json!({"sketch": sk, "profiles": [[7, 2]], "axis": "y", "angle": "90 deg", "symmetric": true}),
  );
  let full = 2.0 * PI * (10.0 * 4.0 + 5.0 * 4.0) + 2.0 * PI * (100.0 - 25.0);
  assert!(close(mass(&s).area, full / 2.0, 1e-3), "{} {}", mass(&s).area, full / 2.0);
}

/// A cone narrowing from radius 10 at z = 0 to 5 at z = 10, open at both ends (its seam edge
/// has the cone on one side only, like the rims).
fn cone(s: &mut Session) {
  run(s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, -1, 0], "x": [1, 0, 0]}}));
  run(s, "sketch.line", json!({"points": [[10, 0], [5, 10]]}));
  run(s, "surface.revolve", json!({"axis": "z"}));
}

#[test]
fn patch_continuity_domes_the_cone_end() {
  let disc = PI * 25.0;
  let mut s = Session::default();
  cone(&mut s);
  run(&mut s, "surface.patch", json!({"edges": [{"point": [5, 0, 10]}], "chain": true}));
  assert!(close(mass(&s).area, disc, 0.5), "{}", mass(&s).area);
  let mut s = Session::default();
  cone(&mut s);
  run(&mut s, "surface.patch", json!({"edges": [{"point": [5, 0, 10]}], "chain": true, "continuity": ["tangent"]}));
  assert!(mass(&s).area > disc + 5.0, "{}", mass(&s).area);
  let fid = s.design.features.last().unwrap().id;
  let k = &s.design.feature(fid).unwrap().kind;
  let tangent = [fox_model::surface::Continuity::Tangent];
  assert!(matches!(k, fox_model::FeatureKind::Patch { continuity, .. } if continuity == &tangent), "{k:?}");
}

#[test]
fn patch_chaining_follows_an_open_boundary() {
  // a strip's four open edges: one picked edge brings the whole boundary loop
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  run(&mut s, "surface.patch", json!({"edges": [{"point": [5, 0, 5]}], "chain": true}));
  let fid = s.design.features.last().unwrap().id;
  let k = &s.design.feature(fid).unwrap().kind;
  assert!(matches!(k, fox_model::FeatureKind::Patch { edges, .. } if edges.len() == 4), "{k:?}");
}

#[test]
fn ruled_from_a_strip_edge_normal_and_tangent() {
  for (kind, z) in [("normal", 5.0), ("tangent", 9.0)] {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
    run(&mut s, "surface.extrude", json!({"distance": 5}));
    run(&mut s, "surface.ruled", json!({"edges": [{"point": [10, 0, 5]}], "distance": 4, "kind": kind}));
    let m = mass(&s);
    assert!(close(m.area, 80.0, 1e-3) && close(m.bbox_max.z, z, 1e-6), "{kind}: {m:?}");
  }
  // the two-edge form still works
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  run(&mut s, "surface.ruled", json!({"a": {"point": [10, 0, 0]}, "b": {"point": [10, 0, 5]}}));
}

#[test]
fn offset_type_joins_the_picked_faces() {
  let faces = json!([{"point": [5, 5, 10]}, {"point": [5, 0, 5]}]);
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [5, 5]}));
  run(&mut s, "surface.offset", json!({"faces": faces, "distance": 2}));
  assert_eq!(s.model.bodies.len(), 3, "each face on its own");
  for (corners, faces_n) in [("sharp", 2), ("round", 3)] {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [5, 5]}));
    run(&mut s, "surface.offset", json!({"faces": faces, "distance": 2, "corners": corners}));
    assert_eq!(s.model.bodies.len(), 2, "{corners}: one body for both faces");
    assert_eq!(s.model.bodies[1].faces().len(), faces_n, "{corners}");
  }
}

#[test]
fn sweep_and_loft_make_open_surfaces() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0], [10, 10]]}));
  let path = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  // a vertical line across the path's start
  run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [1, 0, 0], "x": [0, 0, 1]}}));
  run(&mut s, "sketch.line", json!({"points": [[-2, 0], [2, 0]]}));
  let prof = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "surface.sweep", json!({"sketch": prof, "path": path}));
  let m = mass(&s);
  assert_eq!(s.model.bodies.last().unwrap().shape.solid_count(), 0);
  assert!(close(m.area, 80.0, 1e-2), "{m:?}");

  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  let a = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 6}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  let b = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "surface.loft", json!({"sections": [a, {"sketch": b}]}));
  let m = mass(&s);
  assert!(close(m.area, 60.0, 1e-3), "{m:?}");
  assert!(s.execute("surface.loft", &json!({"sections": [a]})).is_err());
}

/// Patch's Interior Rails/Points: a point above the cone's end lifts the patch through it; a
/// sketch point and a sketch curve are taken too.
#[test]
fn patch_passes_through_interior_points() {
  let mut s = Session::default();
  cone(&mut s);
  run(
    &mut s,
    "surface.patch",
    json!({"edges": [{"point": [5, 0, 10]}], "chain": true, "interior": [{"at": [0, 0, 13]}]}),
  );
  let top = s.model.bodies.last().unwrap().shape.mass().unwrap().bbox_max.z;
  assert!(top > 12.0, "{top}");
  let fid = s.design.features.last().unwrap().id;
  let k = &s.design.feature(fid).unwrap().kind;
  assert!(matches!(k, fox_model::FeatureKind::Patch { interior, .. } if interior.len() == 1), "{k:?}");
  // a sketch curve that isn't there is refused
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [1, 0]]}));
  let sk = s.design.features.last().unwrap().id;
  let bad = json!({"edges": [{"point": [5, 0, 10]}], "chain": true, "interior": [{"sketch": sk.0, "curve": 999}]});
  assert!(s.execute("surface.patch", &bad).is_err());
}
