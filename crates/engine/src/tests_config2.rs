//! Tests for configuration rules (driven columns), configured components inserted from the hub,
//! and direct move face with rotations and curved faces.

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

/// A length × 20 × thickness plate with a hole.
fn plate(s: &mut Session) {
  run(s, "param.add", json!({"name": "length", "expr": "60 mm"}));
  run(s, "param.add", json!({"name": "thickness", "expr": "5 mm"}));
  run(s, "box", json!({"width": "length", "depth": 20, "height": "thickness", "at": [0, 0]}));
  run(s, "cylinder", json!({"radius": 3, "height": 50, "at": [0, 0], "operation": "cut"}));
}

#[test]
fn rules_drive_columns_from_each_rows_parameters() {
  let mut s = Session::default();
  plate(&mut s);
  let body = s.model.bodies[0].id.0;
  let hole = s.design.features[1].id.0;
  run(&mut s, "config.create", json!({"name": "Short"}));
  run(&mut s, "config.set", json!({"configuration": "Short", "parameter": "length", "expr": "40 mm"}));
  run(&mut s, "config.create", json!({"name": "Long"}));
  run(&mut s, "config.set", json!({"configuration": "Long", "parameter": "length", "expr": "150 mm"}));
  // thickness grows with length, short plates have no hole, long ones are blue
  run(&mut s, "config.rule", json!({"parameter": "thickness", "expr": "if(length > 100 mm, 8 mm, 5 mm)"}));
  run(&mut s, "config.rule", json!({"feature": hole, "expr": "length < 50 mm"}));
  let t = run(
    &mut s,
    "config.rule",
    json!({"body": body, "expr": "length > 100 mm", "appearance": "Aluminum - Anodized Blue", "otherwise": null}),
  );
  assert_eq!(t["columns"].as_array().unwrap().len(), 4, "{t}");
  let col = |label: &str| t["columns"].as_array().unwrap().iter().position(|c| c["label"] == label).unwrap();
  let th = col("thickness");
  assert_eq!(t["columns"][th]["rule"]["expr"], "if(length > 100 mm, 8 mm, 5 mm)");
  assert_eq!(t["rows"][0]["cells"][th], "5 mm");
  assert_eq!(t["rows"][1]["cells"][th], "8 mm");
  // a driven cell can't be typed over, and a bad rule is refused up front
  assert!(s.execute("config.set", &json!({"configuration": "Long", "parameter": "thickness", "expr": 3})).is_err());
  assert!(s.execute("config.rule", &json!({"parameter": "thickness", "expr": "if(1,"})).is_err());

  let r = run(&mut s, "config.activate", json!({"name": "Long"}));
  assert!(r["rule_errors"].as_array().unwrap().is_empty(), "{r}");
  let hole_area = std::f64::consts::PI * 9.0;
  assert!((volume(&s) - (150.0 * 20.0 - hole_area) * 8.0).abs() < 1e-3, "{}", volume(&s));
  assert!(s.design.body_color(s.model.bodies[0].id).is_some());
  run(&mut s, "config.activate", json!({"name": "Short"}));
  assert!((volume(&s) - 40.0 * 20.0 * 5.0).abs() < 1e-3, "{}", volume(&s));
  assert_eq!(s.design.body_color(s.model.bodies[0].id), None);

  // exporting a configuration evaluates its rules too
  let path = std::env::temp_dir().join(format!("fox-rules-{}.stl", std::process::id()));
  run(&mut s, "export", json!({"path": path, "configuration": "Long"}));
  assert!(std::fs::metadata(&path).unwrap().len() > 0);
  std::fs::remove_file(&path).ok();
  assert!((volume(&s) - 40.0 * 20.0 * 5.0).abs() < 1e-3);

  // a rule naming a missing parameter reports itself and leaves the design's value
  run(&mut s, "config.rule", json!({"parameter": "thickness", "expr": "missing * 2"}));
  let r = run(&mut s, "config.activate", json!({"name": "Long"}));
  assert_eq!(r["rule_errors"][0]["label"], "thickness", "{r}");
  assert!((volume(&s) - (150.0 * 20.0 - hole_area) * 5.0).abs() < 1e-3);
  // removing the rule makes the column plain again
  let t = run(&mut s, "config.rule", json!({"parameter": "thickness", "expr": null}));
  assert_eq!(t["columns"].as_array().unwrap().len(), 3, "{t}");
  assert_eq!(run(&mut s, "config.list", json!({}))["rules"].as_array().unwrap().len(), 2);
}

#[test]
fn configured_component_switches_with_its_source() {
  let dir = std::env::temp_dir().join(format!("fox-engine-configured-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  // the source: a plate in two configurations, saved to the hub
  let mut src = Session { store_root: Some(dir.clone()), ..Session::default() };
  plate(&mut src);
  run(&mut src, "config.create", json!({"name": "Long"}));
  run(&mut src, "config.set", json!({"configuration": "Long", "parameter": "length", "expr": "150 mm"}));
  run(&mut src, "config.create", json!({"name": "Thick"}));
  run(&mut src, "config.set", json!({"configuration": "Thick", "parameter": "thickness", "expr": "10 mm"}));
  let item = run(&mut src, "hub.save", json!({"name": "Plate"}))["item"].as_str().unwrap().to_string();
  let hole = std::f64::consts::PI * 9.0;

  // inserted in the Long configuration
  let mut s = Session { store_root: Some(dir.clone()), ..Session::default() };
  // by its name (unique in the hub) as well as its id
  let r = run(&mut s, "component.insert", json!({"item": "Plate", "configuration": "Long"}));
  assert_eq!(r["configuration"], "Long");
  let c = r["component"].as_u64().unwrap();
  assert!((volume(&s) - (150.0 * 20.0 - hole) * 5.0).abs() < 1e-3, "{}", volume(&s));
  assert!(s.execute("component.insert", &json!({"item": item, "configuration": "Nope"})).is_err());
  let links = run(&mut s, "component.links", json!({}));
  assert_eq!(links[0]["configuration"], "Long");
  // its source's configurations, then a switch
  let l = run(&mut s, "component.configuration", json!({"component": c}));
  assert_eq!(l["configurations"], json!(["Long", "Thick"]));
  assert_eq!(l["configuration"], "Long");
  run(&mut s, "component.configuration", json!({"component": c, "configuration": "Thick"}));
  assert!((volume(&s) - (60.0 * 20.0 - hole) * 10.0).abs() < 1e-3, "{}", volume(&s));
  // undo goes back to the previous configuration
  s.undo();
  assert!((volume(&s) - (150.0 * 20.0 - hole) * 5.0).abs() < 1e-3);
  s.redo();
  run(&mut s, "component.configuration", json!({"component": c, "configuration": null}));
  assert!((volume(&s) - (60.0 * 20.0 - hole) * 5.0).abs() < 1e-3);
  assert!(s.execute("component.configuration", &json!({"component": c, "configuration": "Nope"})).is_err());
  run(&mut s, "component.configuration", json!({"component": c, "configuration": "Thick"}));

  // a new source version keeps the component's configuration
  run(&mut src, "config.set", json!({"configuration": "Thick", "parameter": "thickness", "expr": "12 mm"}));
  run(&mut src, "hub.save", json!({}));
  let up = run(&mut s, "component.update_link", json!({}));
  assert_eq!(up["updated"].as_array().unwrap().len(), 1, "{up}");
  assert!((volume(&s) - (60.0 * 20.0 - hole) * 12.0).abs() < 1e-3, "{}", volume(&s));
  // stored in the design: reopened without the hub, the bodies and the choice are kept
  let back = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert!((volume(&back) - (60.0 * 20.0 - hole) * 12.0).abs() < 1e-3);
  assert_eq!(run(&mut s, "component.links", json!({}))[0]["configuration"], "Thick");
  // an unlinked copy can't switch
  run(&mut s, "component.break_link", json!({"component": c}));
  assert!(s.execute("component.configuration", &json!({"component": c, "configuration": "Long"})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn move_face_turns_and_moves_curved_faces() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 40, "depth": 20, "height": 10}));
  run(&mut s, "design.history", json!({"capture": false}));
  // the top turned 10° about its middle line along y
  let r = run(
    &mut s,
    "direct.move_face",
    json!({"faces": [{"point": [0, 0, 10]}], "rotate": {"axis": [0, 1, 0], "origin": [0, 0, 10], "angle": "10 deg"}}),
  );
  assert!(r["warning"].is_null(), "{r}");
  assert!((volume(&s) - 8000.0).abs() < 1e-3, "{}", volume(&s));
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.bbox_max.z - (10.0 + 20.0 * 10f64.to_radians().tan())).abs() < 1e-4);
  assert!(s.execute("direct.move_face", &json!({"faces": [{"point": [0, 0, 10]}]})).is_err());
  assert!(
    s.execute("direct.move_face", &json!({"faces": [{"point": [0, 0, 10]}], "rotate": {"axis": [0, 0, 0], "angle": 5}}))
      .is_err()
  );

  // a plate with a pin hole: the hole's wall moves sideways, then turns
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 10, "at": [0, 0]}));
  run(&mut s, "cylinder", json!({"radius": 4, "height": 30, "at": [-15, 0], "operation": "cut"}));
  let v0 = volume(&s);
  run(&mut s, "direct.move_face", json!({"faces": [{"point": [-11, 0, 5]}], "translate": [25, 5, 0]}));
  assert!((volume(&s) - v0).abs() < 1e-3);
  let cyl = |s: &Session| {
    s.model.bodies[0].faces().iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).copied().collect::<Vec<_>>()
  };
  let c = cyl(&s);
  assert_eq!(c.len(), 1);
  assert!((c[0].origin.x - 10.0).abs() < 1e-6 && (c[0].origin.y - 5.0).abs() < 1e-6, "{:?}", c[0].origin);
  run(
    &mut s,
    "direct.move_face",
    json!({"faces": [{"point": [14, 5, 5]}], "rotate": {"axis": [1, 0, 0], "origin": [10, 5, 5], "angle": 15}}),
  );
  let c = cyl(&s);
  assert_eq!(c.len(), 1, "{c:?}");
  assert!((c[0].normal.z.abs() - 15f64.to_radians().cos()).abs() < 1e-6);
  // the tilted hole still runs right through: no skin was left over either opening
  let faces = s.model.bodies[0].faces().len();
  assert_eq!(faces, 7, "{faces}");
  // all in one base feature, replayed after save and reopen
  assert_eq!(s.design.features.iter().filter(|f| matches!(f.kind, fox_model::FeatureKind::BaseFeature { .. })).count(), 1);
  let back = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert!(back.model.errors().is_empty(), "{:?}", back.model.errors());
  assert!((volume(&back) - volume(&s)).abs() < 1e-6);
}
