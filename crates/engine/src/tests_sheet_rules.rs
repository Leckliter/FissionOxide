//! Tests for the sheet metal rule library: the default rule, rules copied into the design and
//! Switch Rule.

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

fn plate(s: &mut Session, rule: Option<&str>) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
  run(s, "sketch.finish", json!({}));
  let p = match rule {
    Some(r) => json!({"rule": r}),
    None => json!({}),
  };
  run(s, "sheet.flange", p);
}

fn thickness(s: &Session) -> f64 {
  s.model.bodies.iter().find_map(|b| b.sheet.as_ref()).unwrap().thickness
}

#[test]
fn first_flange_takes_the_library_default() {
  let mut s = Session::default();
  assert!(s.design.sheet_metal_rules.is_empty());
  assert!(s.design.component_sheet_rule().is_none());
  plate(&mut s, None);
  // the design records the rule it uses: Fusion's .063 Aluminum, also in a millimetre design
  let names: Vec<&str> = s.design.sheet_metal_rules.iter().map(|r| r.name.as_str()).collect();
  assert_eq!(names, [".063 Aluminum"]);
  assert_eq!(s.design.component_sheet_rule().unwrap().name, ".063 Aluminum");
  assert!((thickness(&s) - 0.063 * 25.4).abs() < 1e-9);
  let info = s.model.bodies[0].sheet.as_ref().unwrap();
  assert!((info.bend_radius - 0.035 * 25.4).abs() < 1e-9);
  assert!((info.k_factor - 0.4).abs() < 1e-12);
  assert_eq!(info.relief, fox_model::ReliefShape::Round);
  // the values in terms of Thickness: 0.063 / 0.032 / 0.126 in reliefs, gap 0.063, 2-bend 0.252
  let t = info.thickness;
  for (v, k) in [(info.relief_width, 1.0), (info.relief_depth, 0.5), (info.relief_remnant, 2.0), (info.gap, 1.0)] {
    assert!((v - k * t).abs() < 1e-9, "{v} {k}");
  }
  assert!((info.corner_relief_size() - 4.0 * t).abs() < 1e-9);
  assert_eq!(info.corner3_relief, fox_model::Corner3Relief::RoundWithRadius);
  assert!((info.corner3_radius - t).abs() < 1e-9);
  // undo takes the rule out with the flange
  s.undo();
  assert!(s.design.sheet_metal_rules.is_empty());
}

#[test]
fn library_rules_and_switch_rule() {
  let mut s = Session::default();
  let lib = run(&mut s, "sheet.library", json!({}));
  assert_eq!(lib["default"], ".063 Aluminum");
  let rules = lib["rules"].as_array().unwrap();
  assert_eq!(rules.len(), 19);
  assert_eq!(rules[0], "Steel (mm)");
  // a flange naming a library rule copies it into the design
  plate(&mut s, Some("Steel (mm)"));
  assert_eq!(s.design.sheet_metal_rules.len(), 1);
  assert!((thickness(&s) - 2.5).abs() < 1e-9);
  let info = s.model.bodies[0].sheet.as_ref().unwrap();
  assert!((info.k_factor - 0.44).abs() < 1e-12 && (info.gap - 2.5).abs() < 1e-9);
  assert!(s.execute("sheet.flange", &json!({"rule": "No such rule"})).is_err());
  // Switch Rule: every sheet metal feature takes the new rule, which becomes the design's default
  run(&mut s, "sheet.switch_rule", json!({"rule": ".125 Aluminum"}));
  assert_eq!(s.design.sheet_metal_rules[0].name, ".125 Aluminum");
  assert_eq!(s.design.component_sheet_rule().unwrap().name, ".125 Aluminum");
  assert!((thickness(&s) - 0.125 * 25.4).abs() < 1e-9);
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  // the old rule is no longer in use
  run(&mut s, "sheet.rule_remove", json!({"name": "Steel (mm)"}));
  // Edit Rule renames a rule: the features follow it
  run(&mut s, "sheet.rule", json!({"from": ".125 Aluminum", "name": "Plate", "thickness": "3 mm"}));
  let names: Vec<&str> = s.design.sheet_metal_rules.iter().map(|r| r.name.as_str()).collect();
  assert_eq!(names, ["Plate"]);
  assert_eq!(s.design.component_sheet_rule().unwrap().name, "Plate");
  assert!((thickness(&s) - 3.0).abs() < 1e-9);
  assert!(s.execute("sheet.switch_rule", &json!({"rule": "nope"})).is_err());
}

#[test]
fn rule_values_may_use_thickness() {
  let mut s = Session::default();
  run(
    &mut s,
    "sheet.rule",
    json!({"name": "Thick", "thickness": "3 mm", "bend_radius": "Thickness * 2", "gap": "Thickness / 3"}),
  );
  plate(&mut s, Some("Thick"));
  let info = s.model.bodies[0].sheet.as_ref().unwrap();
  assert!((info.bend_radius - 6.0).abs() < 1e-9 && (info.gap - 1.0).abs() < 1e-9, "{info:?}");
  // the thickness itself can't refer to itself
  assert!(s.execute("sheet.rule", &json!({"name": "Thick", "thickness": "Thickness"})).is_err());
}

#[test]
fn old_files_keep_their_one_millimetre_rule() {
  let mut s = Session::default();
  plate(&mut s, None);
  let mut d = s.design.clone();
  d.sheet_metal_rules.clear();
  d.version = 2;
  let d = fox_model::Design::from_json(&d.to_json()).unwrap();
  assert_eq!(d.sheet_metal_rules.len(), 1);
  assert_eq!(d.sheet_metal_rules[0].name, "Default");
  let s = Session::new(d);
  assert!((thickness(&s) - 1.0).abs() < 1e-9);
}
