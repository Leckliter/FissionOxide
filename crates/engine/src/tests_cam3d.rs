//! Tests for G2/G3 arcs in G-code and the 3D milling strategies: adaptive roughing, pencil,
//! steep and shallow and contour (waterline) finishing.

use fox_kernel::glam::{DVec2, DVec3};
use fox_model::cam::{self, post};
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

fn block(s: &mut Session, w: f64, d: f64, h: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [w, d]}));
  run(s, "extrude", json!({"distance": h}));
}

/// A 40 × 30 × 12 block with a 20 × 14 pocket 6 deep (floor at Z 6, walls at X 10 and 30,
/// Y 8 and 22).
fn pocketed(s: &mut Session) {
  block(s, 40.0, 30.0, 12.0);
  run(s, "sketch.create", json!({"plane": {"point": [20, 15, 12], "normal": [0, 0, 1]}}));
  run(s, "sketch.rect", json!({"corner1": [10, 8], "corner2": [30, 22]}));
  run(s, "extrude", json!({"distance": 6, "flip": true, "operation": "cut"}));
}

/// A 40 × 30 × 6 base with a 14 mm round boss 10 high on it, centred at (20, 15).
fn boss(s: &mut Session) {
  block(s, 40.0, 30.0, 6.0);
  run(s, "sketch.create", json!({"plane": {"point": [20, 15, 6], "normal": [0, 0, 1]}}));
  run(s, "sketch.circle", json!({"center": [20, 15], "diameter": 14}));
  run(s, "extrude", json!({"distance": 10}));
}

fn cuts(t: &cam::Toolpath) -> impl Iterator<Item = &cam::Move> {
  t.moves.iter().filter(|m| !m.rapid)
}

fn no_gouges(s: &mut Session) -> Value {
  let r = run(s, "cam.simulate", json!({}));
  let w = r["warnings"].as_array().unwrap();
  assert!(w.iter().all(|w| w["kind"] != "gouge" && w["kind"] != "rapid_collision"), "{r}");
  r
}

fn seg_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
  let ab = b - a;
  let t = ((p - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
  (a + ab * t - p).length()
}

/// G-code with arcs reads back on the toolpaths' own polylines, in every post and both units;
/// straight moves stay available.
#[test]
fn arcs_in_gcode_follow_the_toolpaths() {
  let mut s = Session::default();
  pocketed(&mut s);
  run(&mut s, "sketch.create", json!({"plane": {"point": [35, 15, 12], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.circle", json!({"center": [35, 15], "diameter": 6}));
  run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
  run(&mut s, "fillet", json!({"edges": [{"point": [40, 30, 6]}, {"point": [0, 30, 6]}], "radius": 5}));
  run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 1}));
  run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 1, "stepover": 0.6}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [20, 15, 6]}], "tool_diameter": 6, "entry": "helix"}));
  run(&mut s, "cam.contour", json!({"tool_diameter": 6, "stepdown": 6}));
  run(&mut s, "cam.drill", json!({"tool_diameter": 5}));
  let st = s.design.cam[0].clone();
  assert!(!st.straight, "arcs are on by default");
  let paths = cam::toolpaths(&st, &s.model).unwrap();
  let origin = cam::origin(&st, &s.model).unwrap();
  let cutting: Vec<DVec3> =
    paths.iter().flat_map(|t| t.moves.iter()).filter(|m| !m.rapid).map(|m| m.to - origin).collect();
  for p in &post::POSTS {
    for units in ["mm", "inch"] {
      let r = run(&mut s, "cam.post", json!({"post": p.id, "units": units}));
      let g = r["gcode"].as_str().unwrap();
      assert!(r["arcs"].as_u64().unwrap() >= 4, "{} {units}: {}", p.id, r["arcs"]);
      assert!(g.contains("G17"), "{}: the XY plane", p.id);
      let (g2, g3) = if p.two_digit { ("G02 ", "G03 ") } else { ("G2 ", "G3 ") };
      assert!(g.contains(g2) && g.contains(g3), "{} {units}: clockwise and counter-clockwise arcs", p.id);
      assert!(g.lines().filter(|l| l.starts_with(g3)).all(|l| l.contains(" I") && l.contains(" J")));
      // fewer lines than straight moves
      let straight = run(&mut s, "cam.post", json!({"post": p.id, "units": units, "arcs": false}));
      assert_eq!(straight["arcs"], 0);
      let sg = straight["gcode"].as_str().unwrap();
      assert!(g.lines().count() + 50 < sg.lines().count(), "{}", p.id);
      // every point read back from the arcs lies on the toolpath (within the arc tolerance,
      // the bow of the chords it replaces and the units' rounding), in order, to its end
      let read = post::read(g).unwrap_or_else(|e| panic!("{} {units}: {e}", p.id));
      let got: Vec<DVec3> = read.iter().filter(|m| !m.0).map(|m| m.1).collect();
      let tol = if units == "mm" { 0.035 } else { 0.04 };
      let mut k = 0;
      for q in &got {
        // the first segment from where the last point was that it lies on
        let near =
          (k.max(1)..(k + 400).min(cutting.len())).find(|&i| seg_distance(*q, cutting[i - 1], cutting[i]) < tol);
        let Some(best) = near else { panic!("{} {units}: {q:?} is off the toolpath after move {k}", p.id) };
        k = best;
      }
      assert!(k + 1 >= cutting.len() - 1, "{} {units}: read back {k} of {}", p.id, cutting.len());
    }
  }
  // the setup keeps straight moves when asked
  run(&mut s, "cam.setup", json!({"arcs": false}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [20, 15, 6]}], "tool_diameter": 6, "entry": "helix"}));
  assert!(s.design.cam[1].straight);
  let r = run(&mut s, "cam.post", json!({}));
  assert_eq!(r["arcs"], 0);
  assert!(!r["gcode"].as_str().unwrap().lines().any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")));
  // and a loose tolerance fits more of them
  let r = run(&mut s, "cam.post", json!({"setup": 1, "arc_tolerance": 0.1}));
  assert!(r["arcs"].as_u64().unwrap() >= 4);
}

/// 3D adaptive roughing clears the stock around a boss down to the stock to leave, never
/// touching the part, a level at the base's flat top included.
#[test]
fn adaptive3d_roughs_the_stock_around_the_part() {
  let mut s = Session::default();
  boss(&mut s);
  run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 1}));
  let r = run(
    &mut s,
    "cam.adaptive3d",
    json!({"tool_diameter": 8, "stepover": 0.2, "stepdown": 4, "stock_to_leave": 0.3, "rpm": 12000}),
  );
  assert!(r["moves"].as_u64().unwrap() > 200, "{r}");
  let st = s.design.cam[0].clone();
  assert_eq!(st.operations[0].stock_to_leave, 0.3);
  let paths = cam::toolpaths(&st, &s.model).unwrap();
  let (slo, shi) = cam::stock(&st, &s.model).unwrap();
  let t = &paths[0];
  // the tool's centre stays within reach of the stock, and never under the part's bottom
  let reach = 4.0 + 1.6 + 2.0 + 0.5;
  for m in cuts(t) {
    let p = m.to;
    assert!(p.x > slo.x - reach && p.x < shi.x + reach && p.y > slo.y - reach && p.y < shi.y + reach, "{p:?}");
    assert!(p.z >= -1e-6 && p.z <= shi.z + 1e-6, "{p:?}");
    // never inside the boss or within the stock to leave of it at its height
    if p.z < 16.0 {
      let d = (p.truncate() - DVec2::new(20.0, 15.0)).length();
      assert!(d >= 7.0 + 4.0 + 0.3 - 0.3, "{p:?} is {d} from the boss axis");
    }
    if p.z < 6.0 {
      let q = p.truncate();
      let d = (q - q.clamp(DVec2::ZERO, DVec2::new(40.0, 30.0))).length();
      assert!(d >= 4.0 + 0.3 - 0.3, "{p:?} is {d} from the base below its top");
    }
  }
  // levels a step-down apart, and one on the base's top (6 + 0.3)
  let mut levels: Vec<f64> = cuts(t).map(|m| (m.to.z * 1000.0).round() / 1000.0).collect();
  levels.sort_by(f64::total_cmp);
  levels.dedup();
  for z in [13.0, 9.0, 6.3, 5.0, 1.0, 0.0] {
    assert!(cuts(t).filter(|m| (m.to.z - z).abs() < 1e-6).count() > 20, "a level at {z}: {levels:?}");
  }
  let r = no_gouges(&mut s);
  // most of the stock is gone: what's left is the stock to leave and the cusps
  let stock = (shi - slo).x * (shi - slo).y * (shi - slo).z;
  let part = 40.0 * 30.0 * 6.0 + std::f64::consts::PI * 49.0 * 10.0;
  let removed: f64 = r["removed"].as_array().unwrap().iter().map(|o| o["mm3"].as_f64().unwrap()).sum();
  assert!(removed > 0.9 * (stock - part), "{removed} of {}", stock - part);
  // the stock to leave is left on the part
  let (_, _, sim) = s.simulation.as_ref().unwrap();
  let (mut over, mut thin) = (0, 0);
  for (k, pz) in sim.part.z.iter().enumerate() {
    if pz.is_finite() {
      over += 1;
      if sim.stock.z[k] < pz + 0.2 {
        thin += 1;
      }
    }
  }
  assert!(thin * 50 < over, "{thin} of {over} cells finished below the stock to leave");
  // many light passes rather than a few full-width ones
  assert!(t.moves.len() > 500);
}

/// Pencil passes run along the pocket's floor corners, the ball touching floor and wall.
#[test]
fn pencil_follows_the_concave_corners() {
  let mut s = Session::default();
  pocketed(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(&mut s, "cam.pencil", json!({"tool_kind": "ball", "tool_diameter": 4}));
  run(&mut s, "cam.generate", json!({}));
  let pts: Vec<DVec3> = cuts(&s.toolpaths[0]).map(|m| m.to).collect();
  assert!(pts.len() > 20, "{}", pts.len());
  // the ball's centre 2 off the walls of the pocket, its tip on the floor
  let near = |p: &DVec3| {
    let (dx, dy) = ((p.x - 10.0).abs().min((p.x - 30.0).abs()), (p.y - 8.0).abs().min((p.y - 22.0).abs()));
    let inside = p.x > 11.0 && p.x < 29.0 && p.y > 9.0 && p.y < 21.0;
    inside && ((dx - 2.0).abs() < 0.5 || (dy - 2.0).abs() < 0.5)
  };
  // along the floor's edges, about the inner loop's length (16 + 10, twice)
  let t = &s.toolpaths[0];
  let mut along = 0.0;
  let mut at = t.moves[0].to;
  for m in &t.moves {
    if !m.rapid && near(&m.to) && near(&at) && (m.to.z - 6.0).abs() < 0.05 && (at.z - 6.0).abs() < 0.05 {
      along += (m.to - at).length();
    }
    at = m.to;
  }
  assert!(along > 45.0 && along < 60.0, "{along}");
  // the rest climbs the four vertical corners, on their diagonals
  for p in &pts {
    let (dx, dy) = ((p.x - 10.0).abs().min((p.x - 30.0).abs()), (p.y - 8.0).abs().min((p.y - 22.0).abs()));
    assert!(near(p) || (dx - dy).abs() < 0.5, "{p:?}");
  }
  // all four sides
  for (x, y) in [(20.0, 10.0), (20.0, 20.0), (12.0, 15.0), (28.0, 15.0)] {
    let c = DVec3::new(x, y, 6.0);
    assert!(pts.windows(2).any(|w| seg_distance(c, w[0], w[1]) < 1.0), "no pass near ({x}, {y})");
  }
  no_gouges(&mut s);
  // a part without inside corners has nothing to pencil
  let mut f = Session::default();
  block(&mut f, 20.0, 20.0, 5.0);
  run(&mut f, "cam.setup", json!({"stock_top": 0}));
  assert!(f.execute("cam.pencil", &json!({"tool_kind": "ball", "tool_diameter": 4})).is_err());
}

/// Contour passes ring the boss at constant heights; steep and shallow adds rows on the flats.
#[test]
fn contour_and_steep_shallow_finishing() {
  let mut s = Session::default();
  boss(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(&mut s, "cam.contour3d", json!({"tool_diameter": 6, "stepdown": 1, "min_slope": 30}));
  run(&mut s, "cam.generate", json!({}));
  let t = &s.toolpaths[0];
  let ring: Vec<DVec3> = cuts(t).map(|m| m.to).filter(|p| p.z > 6.2 && p.z < 15.8).collect();
  assert!(ring.len() > 100);
  for p in &ring {
    let d = (p.truncate() - DVec2::new(20.0, 15.0)).length();
    assert!((d - 10.0).abs() < 0.4, "{p:?}: {d} from the axis");
  }
  let mut levels: Vec<i64> = ring.iter().map(|p| (p.z * 1000.0).round() as i64).collect();
  levels.sort_unstable();
  levels.dedup();
  assert!(levels.len() >= 9, "{levels:?}");
  // the base's sides below
  assert!(cuts(t).any(|m| m.to.z < 5.5 && (m.to.x + 3.0).abs() < 0.4));
  // nothing on the flats
  assert!(!cuts(t).any(|m| (m.to.z - 16.0).abs() < 0.01 && (m.to.truncate() - DVec2::new(20.0, 15.0)).length() < 5.0));
  no_gouges(&mut s);

  let mut s = Session::default();
  boss(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(
    &mut s,
    "cam.steep_shallow",
    json!({"tool_kind": "ball", "tool_diameter": 6, "stepdown": 1, "stepover": 0.15, "threshold": 45}),
  );
  run(&mut s, "cam.generate", json!({}));
  let pts: Vec<DVec3> = cuts(&s.toolpaths[0]).map(|m| m.to).collect();
  // waterlines round the boss
  let ring = pts.iter().filter(|p| p.z > 7.0 && p.z < 15.0).count();
  assert!(ring > 100, "{ring}");
  // rows on the boss's top and the base's top, the ball's tip on them
  let flat = |z: f64| pts.windows(2).filter(|w| (w[0].z - z).abs() < 1e-3 && (w[1].z - z).abs() < 1e-3).count();
  assert!(flat(16.0) > 10 && flat(6.0) > 30, "{} {}", flat(16.0), flat(6.0));
  no_gouges(&mut s);
  assert!(s.execute("cam.steep_shallow", &json!({"tool_diameter": 6, "threshold": 95})).is_err());
}

/// A holder on a short stickout runs into the pocket's walls; on a long one it clears them.
#[test]
fn holder_collisions_are_reported() {
  let mut s = Session::default();
  pocketed(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  // the pocket is 6 deep: 4 mm out of a 30 mm holder hits its rim
  run(
    &mut s,
    "cam.pocket",
    json!({"floors": [{"point": [20, 15, 6]}], "tool": {"diameter": 6, "holder_diameter": 30, "stickout": 4}, "stepdown": 3}),
  );
  let r = run(&mut s, "cam.simulate", json!({}));
  let w = r["warnings"].as_array().unwrap();
  let h = w.iter().find(|w| w["kind"] == "holder").unwrap_or_else(|| panic!("{r}"));
  assert!(h["depth"].as_f64().unwrap() > 1.5 && h["depth"].as_f64().unwrap() < 2.1, "{h}");
  assert!(w.iter().all(|w| w["kind"] != "gouge"), "{r}");
  // sticking out 7 mm, it clears the 6 mm walls
  let mut l = Session::default();
  pocketed(&mut l);
  run(&mut l, "cam.setup", json!({"stock_top": 0}));
  run(
    &mut l,
    "cam.pocket",
    json!({"floors": [{"point": [20, 15, 6]}], "tool": {"diameter": 6, "holder_diameter": 30, "stickout": 7}, "stepdown": 3}),
  );
  let r = run(&mut l, "cam.simulate", json!({}));
  assert!(r["warnings"].as_array().unwrap().iter().all(|w| w["kind"] != "holder"), "{r}");
  // a holder narrower than the tool or without a stickout is refused
  assert!(l.execute("cam.face", &json!({"tool": {"diameter": 6, "holder_diameter": 4, "stickout": 10}})).is_err());
  assert!(l.execute("cam.face", &json!({"tool": {"diameter": 6, "holder_diameter": 20}})).is_err());
}
