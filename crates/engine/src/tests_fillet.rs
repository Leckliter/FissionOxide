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
