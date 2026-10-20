//! Tests for guided and twisted sweeps, lofts with rails or a centre line, setback fillets,
//! extrude to a curved face or a body, emboss/deboss, boundary fill and replace face.

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

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().map(|m| m.volume).unwrap_or(0.0)).sum()
}

fn close(a: f64, b: f64, rel: f64) -> bool {
  (a - b).abs() <= rel * b.abs()
}

#[test]
fn sweep_rail_twist_and_loft_rails_by_command() {
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let path = run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 50]]}));
  let rail = run(&mut s, "sketch.line", json!({"points": [[5, 0], [10, 50]]}));
  let paths = s.edit_sketch.unwrap().0;
  let (pc, rc) = (path["curves"][0].as_u64().unwrap(), rail["curves"][0].as_u64().unwrap());
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "sweep", json!({"path": paths, "path_curves": [pc], "rail": {"sketch": paths, "curves": [rc]}}));
  let frustum = PI * 50.0 / 3.0 * (25.0 + 50.0 + 100.0);
  assert!(close(volume(&s), frustum, 0.01), "{}", volume(&s));
  // twist instead of the rail
  let sweep = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": sweep, "set": {"rail": null, "twist": "180 deg"}}));
  assert!(close(volume(&s), PI * 25.0 * 50.0, 0.01), "{}", volume(&s));

  // a loft between two circles, its sides bowed out by two rails
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  let a = s.edit_sketch.unwrap().0;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 40}));
  let top = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": top}}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  let b = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.arc", json!({"start": [10, 0], "mid": [15, 20], "end": [10, 40]}));
  let r1 = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.arc", json!({"start": [-10, 0], "mid": [-15, 20], "end": [-10, 40]}));
  let r2 = s.edit_sketch.unwrap().0;
  run(&mut s, "loft", json!({"sections": [a, b], "rails": [r1, r2]}));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_max.x - 15.0).abs() < 0.3 && (m.bbox_max.y - 10.0).abs() < 0.3, "{m:?}");
  let loft = s.design.features.last().unwrap().id;
  assert!(s.design.feature(loft).unwrap().kind.depends_on().contains(&fox_model::FeatureId(r2)));
  let err = s.execute("loft", &json!({"sections": [a, b], "rails": [r1], "centerline": r2})).unwrap();
  assert!(err["error"].is_string(), "{err}");
}

#[test]
fn setback_fillet_and_extrude_to_a_body() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  let edges: Vec<Value> =
    [[0.0, 10.0, 20.0], [10.0, 0.0, 20.0], [10.0, 10.0, 10.0]].iter().map(|p| json!({"point": p})).collect();
  run(&mut s, "fillet", json!({"edges": edges, "radius": 3}));
  let plain = volume(&s);
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"setback": "7 mm"}}));
  assert!(volume(&s) < plain - 0.5 && volume(&s) > plain - 60.0, "{} vs {plain}", volume(&s));

  // a cylinder lying along X, a bar extruded up from below until it meets it
  let mut s = Session::default();
  run(&mut s, "cylinder", json!({"plane": "yz", "radius": 20, "height": 40}));
  let cyl = s.model.bodies[0].id.0;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": -40}));
  let below = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": below}}));
  run(&mut s, "sketch.rect", json!({"corner1": [10, -5], "corner2": [30, 5]}));
  let v0 = volume(&s);
  run(&mut s, "extrude", json!({"extent": "to_body", "to": {"body": cyl}}));
  let under = 5.0 * 375.0f64.sqrt() + 400.0 * 0.25f64.asin();
  assert!(close(volume(&s) - v0, 20.0 * (400.0 - under), 1e-3), "{}", volume(&s) - v0);
  // or to its curved face, joined
  let e = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": e, "set": {"operation": "join"}}));
  assert_eq!(s.model.bodies.len(), 1);
}

#[test]
fn emboss_text_and_deboss_on_a_cylinder() {
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  run(&mut s, "cylinder", json!({"radius": 20, "height": 40}));
  let v0 = volume(&s);
  run(&mut s, "plane.offset", json!({"plane": "xz", "offset": 30}));
  let front = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": front}}));
  run(&mut s, "sketch.text", json!({"at": [-12, 15], "text": "CL", "height": 10}));
  run(&mut s, "emboss", json!({"face": {"point": [0, -20, 20]}, "depth": "1 mm", "wrap": true}));
  assert_eq!(s.model.bodies.len(), 1);
  let up = volume(&s) - v0;
  assert!(up > 10.0, "{up}");
  let e = s.design.features.last().unwrap();
  assert_eq!(e.name, "Emboss1");
  let e = e.id.0;
  run(&mut s, "feature.edit", json!({"feature": e, "set": {"deboss": true}}));
  let down = v0 - volume(&s);
  assert!(down > 10.0 && down < up, "{down} {up}");
  run(&mut s, "feature.edit", json!({"feature": e, "set": {"wrap": false, "depth": "0.5 mm"}}));
  assert!(v0 - volume(&s) > 3.0 && v0 - volume(&s) < down);
  assert!(s.execute("emboss", &json!({"depth": 1})).is_err(), "a face is required");
}

#[test]
fn boundary_fill_cells_and_replace_face() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  run(&mut s, "box", json!({"at": [10, 0], "width": 20, "depth": 20, "height": 20, "operation": "new_body"}));
  let tools: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  assert_eq!(tools.len(), 2);
  let cells = run(&mut s, "boundary_fill.cells", json!({"tools": tools}));
  let cells = cells["cells"].as_array().unwrap().clone();
  assert_eq!(cells.len(), 3);
  let mid = cells.iter().find(|c| (c["volume"].as_f64().unwrap() - 4000.0).abs() < 1e-3).unwrap();
  run(&mut s, "boundary_fill", json!({"tools": tools, "cells": [mid["point"]], "remove_tools": true}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - 4000.0).abs() < 1e-3);

  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
  run(&mut s, "extrude", json!({"distance": 10}));
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 15}));
  let up = s.design.features.last().unwrap().id.0;
  run(&mut s, "replace_face", json!({"faces": [{"point": [20, 10, 10]}], "plane": {"feature": up}}));
  assert!((volume(&s) - 12_000.0).abs() < 1e-3);
  run(&mut s, "feature.edit", json!({"feature": up, "set": {"offset": "4 mm"}}));
  assert!((volume(&s) - 3200.0).abs() < 1e-3, "{}", volume(&s));
  assert!(s.execute("replace_face", &json!({"faces": [{"point": [20, 10, 4]}]})).is_err());
}

#[test]
fn shell_directions_whole_bodies_and_tangent_chain() {
  let cube = |s: &mut Session| {
    run(s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let b = &s.model.bodies[0];
    let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    json!({"body": b.id.0, "face": top})
  };
  for (dir, extra, want) in [
    ("inside", json!({}), 8000.0 - 16.0 * 16.0 * 18.0),
    ("outside", json!({}), 24.0 * 24.0 * 22.0 - 8000.0),
    ("both", json!({"outside_thickness": 2}), 24.0 * 24.0 * 22.0 - 18.0 * 18.0 * 19.0),
  ] {
    let mut s = Session::default();
    let top = cube(&mut s);
    let mut p = json!({"faces": [top], "thickness": if dir == "both" { 1 } else { 2 }, "direction": dir});
    for (k, v) in extra.as_object().unwrap() {
      p[k] = v.clone();
    }
    run(&mut s, "shell", p);
    assert!(close(volume(&s), want, 1e-6), "{dir}: {} vs {want}", volume(&s));
    // the direction survives a save and an edit of the thickness
    let id = s.design.features.last().unwrap().id.0;
    run(
      &mut s,
      "feature.edit",
      json!({"feature": id, "set": {"thickness": if dir == "both" { "1 mm" } else { "2 mm" }}}),
    );
    assert!(close(volume(&s), want, 1e-6), "{dir} after edit: {}", volume(&s));
  }
  // a whole body: hollow and closed
  let mut s = Session::default();
  cube(&mut s);
  let b = s.model.bodies[0].id.0;
  run(&mut s, "shell", json!({"bodies": [b], "thickness": 2}));
  assert!(close(volume(&s), 8000.0 - 4096.0, 1e-6), "{}", volume(&s));
  assert!(s.execute("shell", &json!({"thickness": 2})).is_err(), "nothing picked");
  assert!(s.execute("shell", &json!({"bodies": [b], "thickness": 2, "direction": "up"})).is_err());

  // tangent chain: a 40 x 20 block rounded at the +x end; the end face brings the rounds and
  // the long sides they run into, not the square -x end, top or bottom
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 40, "depth": 20, "height": 10}));
  let b = &s.model.bodies[0];
  let edges: Vec<Value> = b
    .edges()
    .iter()
    .filter(|e| (e.mid.x - 20.0).abs() < 1e-6 && (e.mid.y.abs() - 10.0).abs() < 1e-6)
    .map(|e| json!({"body": b.id.0, "edge": e.id}))
    .collect();
  assert_eq!(edges.len(), 2);
  run(&mut s, "fillet", json!({"edges": edges, "radius": 5}));
  let b = &s.model.bodies[0];
  let end = b.faces().iter().find(|f| f.normal.x > 0.9).unwrap().id;
  let v0 = volume(&s);
  let pick = json!({"body": b.id.0, "face": end});
  run(&mut s, "shell", json!({"faces": [pick], "thickness": 1, "tangent_chain": true}));
  let f = s.design.features.last().unwrap();
  assert!(matches!(&f.kind, fox_model::FeatureKind::Shell { faces, .. } if faces.len() == 5), "{:?}", f.kind);
  assert!(volume(&s) < v0 * 0.5, "{} of {v0}", volume(&s));
}
