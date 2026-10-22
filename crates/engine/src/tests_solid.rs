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
  // a taper grows the circle by 50 tan(taper): a cone frustum again
  let taper = (5.0f64 / 50.0).atan().to_degrees();
  run(&mut s, "feature.edit", json!({"feature": sweep, "set": {"twist": "", "taper": format!("{taper} deg")}}));
  assert!(close(volume(&s), frustum, 1e-4), "{}", volume(&s));
  // the rail without scaling only steers the circle: a cylinder
  let rail = json!({"sketch": paths, "curves": [rc]});
  run(&mut s, "feature.edit", json!({"feature": sweep, "set": {"taper": "", "rail": rail, "profile_scaling": "none"}}));
  assert!(close(volume(&s), PI * 25.0 * 50.0, 1e-3), "{}", volume(&s));
  let bad = s.execute("sweep", &json!({"path": paths, "path_curves": [pc], "orientation": "sideways"}));
  assert!(bad.is_err());

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

/// Draft's Tangent Chain: a picked face brings the faces joined to it across smooth edges (the
/// rounds at the +x end and the long sides they run into), as Fusion's does.
#[test]
fn draft_tangent_chain() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 40, "depth": 20, "height": 10}));
  let b = &s.model.bodies[0];
  let edges: Vec<Value> = b
    .edges()
    .iter()
    .filter(|e| (e.mid.x - 20.0).abs() < 1e-6 && (e.mid.y.abs() - 10.0).abs() < 1e-6)
    .map(|e| json!({"body": b.id.0, "edge": e.id}))
    .collect();
  run(&mut s, "fillet", json!({"edges": edges, "radius": 5}));
  let b = &s.model.bodies[0];
  let end = b.faces().iter().find(|f| f.normal.x > 0.9).unwrap().id;
  let pick = json!({"body": b.id.0, "face": end});
  let v0 = volume(&s);
  run(&mut s, "draft", json!({"faces": [pick], "plane": "xy", "angle": "3 deg", "tangent_chain": true}));
  let f = s.design.features.last().unwrap();
  assert!(matches!(&f.kind, fox_model::FeatureKind::Draft { faces, .. } if faces.len() == 5), "{:?}", f.kind);
  assert!(volume(&s) < v0, "{} of {v0}", volume(&s));
}

#[test]
fn bounding_solid_box_cylinder_and_frame() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let bx = s.design.features.last().unwrap().id.0;
  let last = |s: &Session| s.model.bodies.last().unwrap().shape.mass().unwrap();
  run(&mut s, "bounding_solid", json!({"offset": 1}));
  assert_eq!(s.model.bodies.len(), 2);
  let m = last(&s);
  assert!(close(m.volume, 22.0 * 12.0 * 7.0, 1e-9), "{}", m.volume);
  assert!((m.bbox_min - fox_kernel::glam::DVec3::new(-11.0, -6.0, -1.0)).length() < 1e-6);
  let bound = s.design.features.last().unwrap().id.0;
  // it follows the body it encloses
  run(
    &mut s,
    "feature.edit",
    json!({"feature": bx, "set": {"shape": {"type": "box", "width": "30", "depth": "10", "height": "5"}}}),
  );
  assert!(close(last(&s).volume, 32.0 * 12.0 * 7.0, 1e-9), "{}", last(&s).volume);
  // per-side offsets
  run(&mut s, "feature.edit", json!({"feature": bound, "set": {"sides": ["0", "1", "0", "0", "2", "0"]}}));
  assert!(close(last(&s).volume, 31.0 * 10.0 * 7.0, 1e-9), "{}", last(&s).volume);

  // a cylinder about Z around the 20 x 10 x 5 box: radius to its corners
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  run(&mut s, "bounding_solid", json!({"shape": "cylinder", "offset": 1}));
  let r = 125f64.sqrt() + 1.0;
  assert!(close(last(&s).volume, PI * r * r * 7.0, 1e-6), "{}", last(&s).volume);
  // about Y (the XZ plane's normal), only that body
  let body = s.model.bodies[0].id.0;
  run(&mut s, "bounding_solid", json!({"bodies": [body], "shape": "cylinder", "plane": "xz"}));
  assert!(close(last(&s).volume, PI * 106.25 * 10.0, 1e-6), "{}", last(&s).volume);
  assert!(s.execute("bounding_solid", &json!({"shape": "sphere"})).is_err());
  assert!(s.execute("bounding_solid", &json!({"sides": [1, 2]})).is_err());
}

#[test]
fn pattern_on_path_copies_bodies_and_features_along_a_sketch_path() {
  // a 6 mm cube at (60, 40), patterned back along the chain (0,0)-(60,0)-(60,40) from its end
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [57, 37], "corner2": [63, 43]}));
  let cube = s.edit_sketch.unwrap().0;
  run(&mut s, "extrude", json!({"sketch": cube, "distance": 6}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [60, 0]]}));
  run(&mut s, "sketch.line", json!({"points": [[60, 0], [60, 40]]}));
  let path = s.edit_sketch.unwrap().0;
  let p = json!({"path": path, "count": 4, "spacing": 25, "start": 1, "flip": true, "join": false});
  run(&mut s, "pattern.path", p);
  let mut centres: Vec<[i64; 2]> = s
    .model
    .bodies
    .iter()
    .map(|b| b.shape.mass().unwrap().center_of_mass)
    .map(|c| [c.x.round() as i64, c.y.round() as i64])
    .collect();
  centres.sort();
  assert_eq!(centres, vec![[25, 0], [50, 0], [60, 15], [60, 40]]);
  let pat = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": pat, "set": {"flip": false, "start": "0", "symmetric": true}}));
  assert_eq!(s.model.bodies.len(), 4);

  // a hole re-cut every 20 mm along an arc of a plate
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [120, 60]}));
  let plate = s.edit_sketch.unwrap().0;
  run(&mut s, "extrude", json!({"sketch": plate, "distance": 5}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let arc = run(&mut s, "sketch.arc", json!({"start": [100, 15], "mid": [60, 45], "end": [20, 15]}));
  run(&mut s, "sketch.circle", json!({"center": [100, 15], "radius": 3}));
  let sk = s.edit_sketch.unwrap().0;
  let hole =
    run(&mut s, "extrude", json!({"sketch": sk, "profiles": [[100, 15]], "extent": "through_all", "operation": "cut"}));
  let p = json!({"features": [hole["feature"]], "path": sk, "path_curves": arc["curves"], "count": 5, "distance": 80, "extent": true});
  run(&mut s, "pattern.path", p);
  assert!(close(volume(&s), 120.0 * 60.0 * 5.0 - 5.0 * PI * 9.0 * 5.0, 1e-6), "{}", volume(&s));
  let bad = s.execute("pattern.path", &json!({"path": sk, "count": 2, "spacing": 5, "orientation": "sideways"}));
  assert!(bad.is_err());
}

/// A 40 mm sweep up +Z of a section on XY drawn by `section`, along a line on XZ, with a rail
/// from (5, 0, 0) to (10, 0, 40) or a 90° twist; the swept volume, or the sweep's error.
fn rail_or_twist_sweep(s: &mut Session, section: (&str, Value), rail: bool) -> Result<f64, String> {
  let xz = json!({"origin": [0.0, 0.0, 0.0], "normal": [0.0, 1.0, 0.0], "x": [1.0, 0.0, 0.0]});
  run(s, "sketch.create", json!({"plane": xz}));
  let path = run(s, "sketch.line", json!({"points": [[0, 0], [0, -40]]}));
  let guide = run(s, "sketch.line", json!({"points": [[5, 0], [10, -40]]}));
  let paths = s.edit_sketch.unwrap().0;
  run(s, "sketch.finish", json!({}));
  let (pc, rc) = (path["curves"][0].as_u64().unwrap(), guide["curves"][0].as_u64().unwrap());
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, section.0, section.1);
  let prof = s.edit_sketch.unwrap().0;
  run(s, "sketch.finish", json!({}));
  let mut p = json!({"sketch": prof, "path": paths, "path_curves": [pc], "operation": "new_body"});
  if rail {
    p["rail"] = json!({"sketch": paths, "curves": [rc]});
  } else {
    p["twist"] = json!("90 deg");
  }
  let r = s.execute("sweep", &p).map_err(|e| e.to_string())?;
  match r.get("error").and_then(Value::as_str) {
    Some(e) => Err(e.to_string()),
    None => Ok(volume(s)),
  }
}

#[test]
fn legacy_designs_keep_the_old_rail_and_twist_sweeps() {
  let square = ("sketch.rect", json!({"corner1": [-5, -5], "corner2": [5, 5]}));
  let circle = ("sketch.circle", json!({"center": [0, 0], "radius": 5}));
  // (section, rail?, the new sweep's volume, main's volume before SWEEP_UNIFIED: None = it failed)
  let cases = [
    // 90° twist of a 10 mm square: main sampled the path at 120 points
    (square.clone(), false, 4000.0, Some(4000.0015588640877)),
    // a square scaled by a rail: a pyramid frustum, which BRepFill's contact on border can't build
    (square, true, 40.0 / 3.0 * (100.0 + 200.0 + 400.0), None),
    // a circle scaled by the rail: a cone frustum both ways, main's off by 0.02 mm³
    (circle, true, PI * 40.0 / 3.0 * (25.0 + 50.0 + 100.0), Some(7330.400859408512)),
  ];
  for (section, rail, new, old) in cases {
    let mut s = Session::default();
    assert!(s.design.behaviours.has(fox_sketch::behaviour::SWEEP_UNIFIED));
    let got = rail_or_twist_sweep(&mut s, section.clone(), rail).unwrap();
    assert!(close(got, new, 1e-4), "new design, rail {rail}: {got} vs {new}");
    // the same design without the behaviour, as a file from before it rebuilds
    s.design.behaviours = fox_model::Behaviours::legacy();
    s.rebuild();
    let sweep = s.design.features.last().unwrap().id;
    let error = s.model.status(sweep).and_then(|st| st.error.clone());
    match old {
      Some(v) => {
        assert!(error.is_none(), "legacy, rail {rail}: {error:?}");
        assert!((volume(&s) - v).abs() < 1e-6, "legacy, rail {rail}: {} vs main's {v}", volume(&s));
      }
      None => assert!(error.is_some_and(|e| e.contains("sweep failed")), "legacy square rail sweep: {:?}", volume(&s)),
    }
  }
}

#[test]
fn sweep_between_two_rails_and_along_a_guide_surface_by_command() {
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let path = run(&mut s, "sketch.line", json!({"points": [[5, 0], [5, 50]]}))["curves"][0].clone();
  let rail = run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 50]]}))["curves"][0].clone();
  let rail2 = run(&mut s, "sketch.line", json!({"points": [[10, 0], [20, 50]]}))["curves"][0].clone();
  let paths = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, -2], "corner2": [10, 2]}));
  let rails = json!({
    "path": paths, "path_curves": [path],
    "rail": {"sketch": paths, "curves": [rail]}, "rail2": {"sketch": paths, "curves": [rail2]},
  });
  // stretched across the rails by default
  run(&mut s, "sweep", rails.clone());
  assert!(close(volume(&s), 3000.0, 0.01), "{}", volume(&s));
  let sw = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": sw, "set": {"profile_scaling": "scale"}}));
  assert!(close(volume(&s), 40.0 * 50.0 * 7.0 / 3.0, 0.01), "{}", volume(&s));
  let mut bad = rails.clone();
  bad.as_object_mut().unwrap().remove("rail");
  assert!(s.execute("sweep", &bad).is_err(), "a second rail needs a first");

  // round a cylinder halfway up, its side face the guide surface
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  run(&mut s, "cylinder", json!({"radius": 20, "height": 20}));
  let b = &s.model.bodies[0];
  let side = b.faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap().id;
  let side = json!({"body": b.id.0, "face": side});
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
  let mid = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": mid}}));
  run(&mut s, "sketch.arc", json!({"center": [0, 0], "start": [20, 0], "end": [0, 20]}));
  let path = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [20, 9], "corner2": [22, 11]}));
  let v0 = volume(&s);
  run(&mut s, "sweep", json!({"path": path, "guide_surface": side, "operation": "new_body"}));
  let expected = PI / 2.0 * 21.0 * 4.0;
  assert!(close(volume(&s) - v0, expected, 0.01), "{} vs {expected}", volume(&s) - v0);
  let r = s.execute("sweep", &json!({"path": path, "guide_surface": side, "twist": "10 deg", "operation": "new_body"}));
  assert!(r.is_err() || r.unwrap()["error"].as_str().is_some_and(|e| e.contains("twist")));
}

#[test]
fn extrude_to_a_vertex_or_point() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  let b = &s.model.bodies[0];
  let (bid, top) = (b.id.0, b.edges().iter().find(|e| e.end.z > 19.0).unwrap().id);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [30, 0], "corner2": [40, 10]}));
  let sk = s.edit_sketch.unwrap().0;
  let to = json!({"vertex": {"body": bid, "edge": top}, "end": true});
  run(&mut s, "extrude", json!({"sketch": sk, "extent": "to_vertex", "to": to, "operation": "new_body"}));
  let last = |s: &Session| s.model.bodies.last().unwrap().shape.mass().unwrap();
  assert!((last(&s).volume - 2000.0).abs() < 1e-3, "{:?}", last(&s));
  // the extent follows the box: a taller box extrudes further
  let bx = s.design.features[0].id.0;
  let taller = json!({"shape": {"type": "box", "width": "20", "depth": "20", "height": "30"}});
  run(&mut s, "feature.edit", json!({"feature": bx, "set": taller}));
  assert!((last(&s).bbox_max.z - 30.0).abs() < 1e-6, "{:?}", last(&s));
  // a point below the sketch goes the other way, plus an offset past it
  run(
    &mut s,
    "extrude",
    json!({"sketch": sk, "extent": "to_vertex", "to": {"at": [0, 0, -15]}, "distance": 5, "operation": "new_body"}),
  );
  let m = last(&s);
  assert!((m.bbox_min.z + 20.0).abs() < 1e-6 && m.bbox_max.z.abs() < 1e-6, "{m:?}");
  assert!(s.execute("extrude", &json!({"sketch": sk, "extent": "to_vertex"})).is_err(), "a point is needed");
  let r = s.execute("extrude", &json!({"sketch": sk, "extent": "to_vertex", "to": [5, 5, 0], "operation": "new_body"}));
  assert!(r.is_err() || r.unwrap()["error"].as_str().is_some_and(|e| e.contains("sketch plane")));
}

#[test]
fn replace_a_curved_face() {
  // a 40 × 20 block whose top arches from z = 10 at the sides to 14 in the middle
  let mut s = Session::default();
  s.design.behaviours = fox_model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.line", json!({"points": [[40, 10], [40, 0], [0, 0], [0, 10]]}));
  run(&mut s, "sketch.arc", json!({"start": [0, 10], "mid": [20, 14], "end": [40, 10]}));
  run(&mut s, "extrude", json!({"distance": 20}));
  let b = &s.model.bodies[0];
  let arch = b.faces().iter().find(|f| f.kind != fox_kernel::SurfaceKind::Plane).unwrap().id;
  let arch = json!({"body": b.id.0, "face": arch});
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 15}));
  let up = s.design.features.last().unwrap().id.0;
  run(&mut s, "replace_face", json!({"faces": [arch], "plane": {"feature": up}}));
  assert!((volume(&s) - 12_000.0).abs() < 1e-3, "{}", volume(&s));
  // below the arch: the block is cut down flat
  run(&mut s, "feature.edit", json!({"feature": up, "set": {"offset": "8 mm"}}));
  assert!((volume(&s) - 6400.0).abs() < 1e-3, "{}", volume(&s));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_max.z - 8.0).abs() < 1e-6, "{m:?}");
}
