//! Tests for the Manufacture workspace: the tool library, entries and tabs, rest machining,
//! adaptive clearing, 3D finishing, the stock simulation and the post-processors.

use fox_kernel::glam::DVec3;
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

fn hub(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fox-cam-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

fn block(s: &mut Session, w: f64, d: f64, h: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [w, d]}));
  run(s, "extrude", json!({"distance": h}));
}

/// A 60 × 40 × 20 block with a 30 × 20 pocket 8 deep (floor at Z 12) and a 6 mm through hole.
fn part(s: &mut Session) {
  block(s, 60.0, 40.0, 20.0);
  run(s, "sketch.create", json!({"plane": {"point": [30, 20, 20], "normal": [0, 0, 1]}}));
  run(s, "sketch.rect", json!({"corner1": [10, 10], "corner2": [40, 30]}));
  run(s, "extrude", json!({"distance": 8, "flip": true, "operation": "cut"}));
  run(s, "sketch.create", json!({"plane": {"point": [50, 20, 20], "normal": [0, 0, 1]}}));
  run(s, "sketch.circle", json!({"center": [50, 20], "diameter": 6}));
  run(s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
}

fn cuts(t: &cam::Toolpath) -> impl Iterator<Item = &cam::Move> {
  t.moves.iter().filter(|m| !m.rapid)
}

#[test]
fn tool_library_in_the_hub() {
  let mut s = Session::new(crate::model::Design::new("Tools"));
  s.store_root = Some(hub("tools"));
  let tools = run(&mut s, "cam.tools", json!({}));
  assert_eq!(tools.as_array().unwrap().len(), 8, "the standard library");
  let t = run(
    &mut s,
    "cam.tool.add",
    json!({"number": 12, "name": "4 mm bull", "kind": "bull_nose", "diameter": 4, "corner_radius": 0.5, "rpm": 15000, "feed": 900}),
  );
  assert_eq!(t["kind"], "bull_nose");
  assert!(s.execute("cam.tool.add", &json!({"number": 13, "kind": "bull_nose", "diameter": 4})).is_err());
  assert!(s.execute("cam.tool.add", &json!({"number": 13, "kind": "spoon", "diameter": 4})).is_err());
  assert!(cam::Library::path(s.store_root.as_ref().unwrap()).exists());
  part(&mut s);
  run(&mut s, "cam.setup", json!({}));
  // a library tool by number: its shape and feeds and speeds come along
  let r = run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_number": 12}));
  assert_eq!(r["tool"], 12);
  let op = &s.design.cam[0].operations[0];
  assert_eq!(
    (op.tool.kind, op.tool.diameter, op.tool.rpm, op.tool.feed),
    (cam::ToolKind::BullNose, 4.0, 15000.0, 900.0)
  );
  // feeds given with the operation override the library's
  run(&mut s, "cam.drill", json!({"tool": 7, "plunge": 150}));
  let op = &s.design.cam[0].operations[1];
  assert_eq!((op.tool.kind, op.tool.plunge, op.tool.diameter), (cam::ToolKind::Drill, 150.0, 6.0));
  // drills don't mill
  assert!(s.execute("cam.pocket", &json!({"floors": [{"point": [25, 20, 12]}], "tool_number": 7})).is_err());
  assert!(s.execute("cam.pocket", &json!({"floors": [{"point": [25, 20, 12]}], "tool_number": 99})).is_err());
  run(&mut s, "cam.tool.delete", json!({"number": 12}));
  assert_eq!(run(&mut s, "cam.tools", json!({})).as_array().unwrap().len(), 8);
  // the operation keeps its copy of the tool
  run(&mut s, "cam.generate", json!({}));
  assert_eq!(s.toolpaths[0].tool.diameter, 4.0);
  let _ = std::fs::remove_dir_all(s.store_root.take().unwrap());
}

#[test]
fn ramp_and_helix_entries_never_plunge_into_the_material() {
  for entry in ["plunge", "ramp", "helix"] {
    let mut s = Session::default();
    part(&mut s);
    run(&mut s, "cam.setup", json!({"stock_top": 0}));
    run(
      &mut s,
      "cam.pocket",
      json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 6, "stepdown": 3, "entry": entry, "ramp_angle": 5}),
    );
    run(&mut s, "cam.generate", json!({}));
    let t = &s.toolpaths[0];
    // a plunge: straight down more steeply than the ramp, then cutting across at that depth
    let mut steep = 0;
    for w in t.moves.windows(3) {
      let (a, m, next) = (w[0].to, &w[1], &w[2]);
      let (dz, dxy) = (a.z - m.to.z, (m.to - a).truncate().length());
      if !m.rapid && dz > 1e-9 {
        // the tool centre keeps a radius off the pocket's walls
        assert!(m.to.x >= 13.0 - 1e-6 && m.to.x <= 37.0 + 1e-6 && m.to.y >= 13.0 - 1e-6 && m.to.y <= 27.0 + 1e-6);
        let flat = !next.rapid && (next.to.z - m.to.z).abs() < 1e-9 && (next.to - m.to).truncate().length() > 1e-6;
        if dz > dxy * 5f64.to_radians().tan() + 1e-6 && flat {
          steep += 1;
        }
      }
    }
    if entry == "plunge" {
      assert!(steep > 3, "plunges straight down to each level");
    } else {
      assert_eq!(steep, 0, "{entry} goes down at the ramp angle");
    }
    assert!(t.moves.iter().any(|m| (m.to.z - 12.0).abs() < 1e-9));
    if entry == "helix" {
      // a helix: points on a circle going down
      let down: Vec<DVec3> = cuts(t).map(|m| m.to).filter(|p| p.z < 20.0 - 1e-6 && p.z > 17.0 + 1e-6).collect();
      assert!(down.len() > 20);
      // the circle through three of them
      let (a, b, c) = (down[0].truncate(), down[down.len() / 3].truncate(), down[2 * down.len() / 3].truncate());
      let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
      let c = fox_kernel::glam::DVec2::new(
        (a.length_squared() * (b.y - c.y) + b.length_squared() * (c.y - a.y) + c.length_squared() * (a.y - b.y)) / d,
        (a.length_squared() * (c.x - b.x) + b.length_squared() * (a.x - c.x) + c.length_squared() * (b.x - a.x)) / d,
      );
      let r0 = (down[0].truncate() - c).length();
      assert!(r0 > 1.0, "{r0}");
      assert!(down.iter().all(|p| ((p.truncate() - c).length() - r0).abs() < 1e-6), "{c:?} {r0}");
    }
  }
}

#[test]
fn holding_tabs_bridge_the_contour_bottom() {
  let mut s = Session::default();
  block(&mut s, 60.0, 40.0, 10.0);
  run(&mut s, "cam.setup", json!({}));
  run(
    &mut s,
    "cam.contour",
    json!({"tool_diameter": 6, "stepdown": 2.5, "tabs": {"count": 4, "width": 6, "height": 3}, "entry": "ramp"}),
  );
  run(&mut s, "cam.generate", json!({}));
  let t = &s.toolpaths[0];
  // levels below the tab top (Z 3) lift over four bridges each, a tab plus the tool wide
  let bottom: Vec<&cam::Move> = cuts(t).filter(|m| m.to.z < 3.0 - 1e-6).collect();
  assert!(!bottom.is_empty() && t.moves.iter().any(|m| m.to.z < 0.0));
  let mut bridges = Vec::new();
  let mut start: Option<DVec3> = None;
  let mut at = t.moves[0].to;
  for m in &t.moves {
    let up = |p: DVec3| (p.z - 3.0).abs() < 1e-9;
    if !m.rapid && up(m.to) && !up(at) && at.z < 3.0 && (m.to - at).truncate().length() < 1e-9 {
      start = Some(m.to);
    }
    if !m.rapid
      && up(at)
      && !up(m.to)
      && (m.to - at).truncate().length() < 1e-9
      && m.to.z < 3.0
      && let Some(a) = start.take()
    {
      bridges.push((a, m.to));
    }
    at = m.to;
  }
  // the levels at Z 1 and -0.5 (from the stock top at 11) cut below the tabs: four bridges each
  assert_eq!(bridges.len(), 8, "{bridges:?}");
  for (a, b) in &bridges {
    let d = (b.truncate() - a.truncate()).length();
    assert!((d - 12.0).abs() < 0.5, "{d}");
  }
  // the tabs survive the simulation: stock stands up to the tab top next to the part
  let sim = run(&mut s, "cam.simulate", json!({}));
  assert!(sim["warnings"].as_array().unwrap().is_empty(), "{sim}");
  let (_, _, sim) = s.simulation.as_ref().unwrap();
  let hf = &sim.stock;
  let tab = (0..hf.nx * hf.ny).filter(|k| {
    let c = hf.centre(k % hf.nx, k / hf.nx);
    let outside = c.x < -0.5 || c.x > 60.5 || c.y < -0.5 || c.y > 40.5;
    let near = c.x > -1.5 && c.x < 61.5 && c.y > -1.5 && c.y < 41.5;
    outside && near && (hf.z[*k] - 3.0).abs() < 0.05
  });
  assert!(tab.count() > 20);
}

#[test]
fn rest_machining_cuts_only_the_corners() {
  let mut s = Session::default();
  part(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 10, "stepdown": 4}));
  run(
    &mut s,
    "cam.pocket",
    json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 4, "stepdown": 4, "rest": true}),
  );
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 4, "stepdown": 4}));
  run(&mut s, "cam.generate", json!({}));
  let len = |t: &cam::Toolpath| t.stats(t.moves[0].to).0;
  let (rest, full) = (&s.toolpaths[1], &s.toolpaths[2]);
  assert!(len(rest) > 0.0 && len(rest) < 0.4 * len(full), "{} {}", len(rest), len(full));
  // every rest cut is near a corner the 10 mm tool couldn't reach (the corner, plus the small
  // tool and half of it before and after the material)
  let corners = [(10.0, 10.0), (40.0, 10.0), (40.0, 30.0), (10.0, 30.0)];
  for m in cuts(rest) {
    let near = corners.iter().any(|(x, y)| (m.to.x - x).abs() <= 8.5 && (m.to.y - y).abs() <= 8.5);
    assert!(near, "{:?}", m.to);
    assert!(m.to.x >= 12.0 - 1e-6 && m.to.x <= 38.0 + 1e-6 && m.to.y >= 12.0 - 1e-6 && m.to.y <= 28.0 + 1e-6);
  }
  // the big tool and the rest pass leave about what the big tool and a full small pass leave
  let left = |s: &mut Session, ops: &[usize]| {
    let mut st = s.design.cam[0].clone();
    st.operations = ops.iter().map(|i| st.operations[*i].clone()).collect();
    let paths = cam::toolpaths(&st, &s.model).unwrap();
    let sim = cam::sim::simulate(&st, &s.model, &paths).unwrap();
    assert!(sim.warnings.is_empty(), "{:?}", sim.warnings);
    sim.remaining()
  };
  let (big, with_rest, with_full) = (left(&mut s, &[0]), left(&mut s, &[0, 1]), left(&mut s, &[0, 2]));
  assert!(with_rest < big - 50.0, "{big} {with_rest}");
  assert!((with_rest - with_full).abs() < 0.1 * (big - with_full), "{big} {with_rest} {with_full}");
}

#[test]
fn adaptive_clearing_through_the_command_set() {
  let mut s = Session::default();
  part(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  let r = run(
    &mut s,
    "cam.adaptive",
    json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 6, "stepover": 0.12, "stepdown": 8}),
  );
  assert!(r["moves"].as_u64().unwrap() > 50);
  run(&mut s, "cam.generate", json!({}));
  let t = &s.toolpaths[0];
  for m in cuts(t) {
    assert!(
      m.to.x >= 13.0 - 1e-6 && m.to.x <= 37.0 + 1e-6 && m.to.y >= 13.0 - 1e-6 && m.to.y <= 27.0 + 1e-6,
      "{:?}",
      m
    );
    assert!(m.to.z >= 12.0 - 1e-9);
  }
  assert!(t.moves.iter().any(|m| (m.to.z - 12.0).abs() < 1e-9));
  let sim = run(&mut s, "cam.simulate", json!({}));
  assert!(sim["warnings"].as_array().unwrap().is_empty(), "{sim}");
  // the pocket is cleared to within the cell size, corners aside
  let (_, _, sim) = s.simulation.as_ref().unwrap();
  let hf = &sim.stock;
  for j in 0..hf.ny {
    for i in 0..hf.nx {
      let c = hf.centre(i, j);
      if c.x > 13.5 && c.x < 36.5 && c.y > 13.5 && c.y < 26.5 {
        assert!(hf.z[j * hf.nx + i] < 12.0 + 0.05, "{c:?} {}", hf.z[j * hf.nx + i]);
      }
    }
  }
  let saved: crate::model::Design = serde_json::from_str(&serde_json::to_string(&s.design).unwrap()).unwrap();
  assert_eq!(saved.cam[0].operations[0], s.design.cam[0].operations[0]);
}

#[test]
fn simulation_finds_gouges_rapids_and_short_flutes() {
  let mut s = Session::default();
  part(&mut s);
  run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 2}));
  run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 1, "stepover": 0.6}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 6, "entry": "ramp"}));
  run(&mut s, "cam.contour", json!({"tool_diameter": 6, "stepdown": 5}));
  run(&mut s, "cam.drill", json!({"tool_diameter": 6, "peck": 3}));
  let r = run(&mut s, "cam.simulate", json!({}));
  assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
  let removed: f64 = r["removed"].as_array().unwrap().iter().map(|o| o["mm3"].as_f64().unwrap()).sum();
  // stock 66 × 46 × 22 less the part (60 × 40 × 20, the pocket and hole cut out of it)
  let part_volume = 60.0 * 40.0 * 20.0 - 30.0 * 20.0 * 8.0 - std::f64::consts::PI * 9.0 * 20.0;
  let expected = 66.0 * 46.0 * 22.0 - part_volume;
  assert!((removed + r["remaining_mm3"].as_f64().unwrap() - expected).abs() < 0.03 * expected, "{removed} {r}");
  // what's left is the pocket's rounded corners and the like: a little
  assert!(r["remaining_mm3"].as_f64().unwrap() < 0.02 * expected, "{r}");
  let (_, _, sim) = s.simulation.as_ref().unwrap();
  assert!(sim.mesh(100).indices.len() > 1000);

  // cutting 1 mm into the part's sides gouges it
  let mut g = Session::default();
  part(&mut g);
  run(&mut g, "cam.setup", json!({"stock_top": 0}));
  run(&mut g, "cam.contour", json!({"tool_diameter": 6, "stepdown": 5, "stock_to_leave": -1}));
  let r = run(&mut g, "cam.simulate", json!({}));
  let w = r["warnings"].as_array().unwrap();
  assert!(w.iter().any(|w| w["kind"] == "gouge" && (w["depth"].as_f64().unwrap() > 1.0)), "{r}");
  assert!(g.simulation.as_ref().unwrap().2.gouged.iter().any(|x| *x));

  // without facing, the drill's rapids to its retract plane (3 mm over the hole) hit the stock
  let mut c = Session::default();
  part(&mut c);
  run(&mut c, "cam.setup", json!({"stock_top": 5}));
  run(&mut c, "cam.drill", json!({"tool_diameter": 6}));
  let r = run(&mut c, "cam.simulate", json!({}));
  assert!(r["warnings"].as_array().unwrap().iter().any(|w| w["kind"] == "rapid_collision"), "{r}");

  // 5 mm step-downs with 4 mm flutes bury the shank
  let mut f = Session::default();
  part(&mut f);
  run(&mut f, "cam.setup", json!({"stock_top": 0}));
  run(&mut f, "cam.contour", json!({"tool": {"diameter": 6, "flute_length": 4}, "stepdown": 5}));
  let r = run(&mut f, "cam.simulate", json!({}));
  assert!(r["warnings"].as_array().unwrap().iter().any(|w| w["kind"] == "shank"), "{r}");
}

/// Distance from a point to a triangle.
fn tri_distance(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> f64 {
  let n = (b - a).cross(c - a);
  if n.length() < 1e-12 {
    return f64::INFINITY;
  }
  let n = n.normalize();
  let q = p - n * (p - a).dot(n);
  let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, v)| (*v - *u).cross(q - *u).dot(n) >= -1e-12);
  if inside {
    return (p - q).length();
  }
  let seg = |u: DVec3, v: DVec3| {
    let t = ((p - u).dot(v - u) / (v - u).length_squared()).clamp(0.0, 1.0);
    (u + (v - u) * t - p).length()
  };
  seg(a, b).min(seg(b, c)).min(seg(c, a))
}

#[test]
fn parallel_and_scallop_finishing_follow_the_surface() {
  let mut s = Session::default();
  block(&mut s, 40.0, 30.0, 10.0);
  run(
    &mut s,
    "fillet",
    json!({"edges": [{"point": [20, 0, 10]}, {"point": [40, 15, 10]}, {"point": [20, 30, 10]}, {"point": [0, 15, 10]}], "radius": 6}),
  );
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(&mut s, "cam.parallel", json!({"tool": {"diameter": 6, "kind": "ball"}, "stepover": 0.2}));
  run(&mut s, "cam.scallop", json!({"tool_kind": "ball", "tool_diameter": 6, "stepover": 0.2}));
  run(&mut s, "cam.generate", json!({}));
  let b = &s.model.bodies[0];
  let m = b.mesh().unwrap();
  let v = |i: u32| {
    let p = m.positions[i as usize];
    DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
  };
  for t in &s.toolpaths {
    let pts: Vec<DVec3> = cuts(t).map(|m| m.to).collect();
    assert!(pts.len() > 100, "{}", t.name);
    // on the flat top, the ball's tip touches it
    assert!(pts.iter().any(|p| p.x > 5.5 && p.x < 34.5 && p.y > 6.0 && p.y < 24.0 && (p.z - 10.0).abs() < 1e-6));
    // down the fillets
    assert!(pts.iter().any(|p| p.z < 8.0 && p.z > 4.5));
    // the ball never cuts into the part: its centre stays a radius off every triangle
    for p in pts.iter().step_by(3) {
      let c = *p + DVec3::Z * 3.0;
      let d =
        m.indices.chunks_exact(3).map(|t| tri_distance(c, v(t[0]), v(t[1]), v(t[2]))).fold(f64::INFINITY, f64::min);
      assert!(d >= 3.0 - 0.03, "{}: {p:?} {d}", t.name);
    }
  }
  let r = run(&mut s, "cam.simulate", json!({}));
  assert!(r["warnings"].as_array().unwrap().iter().all(|w| w["kind"] != "gouge"), "{r}");
}

/// Every post writes G-code that reads back as the toolpaths' cutting moves, in mm or inches.
#[test]
fn posts_write_the_toolpaths_moves() {
  let mut s = Session::default();
  part(&mut s);
  run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 1}));
  run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 1, "stepover": 0.6}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 6, "entry": "helix"}));
  run(&mut s, "cam.contour", json!({"tool_diameter": 6, "stepdown": 7, "tabs": 2}));
  run(&mut s, "cam.drill", json!({"tool_diameter": 5, "peck": 4}));
  let st = s.design.cam[0].clone();
  let paths = cam::toolpaths(&st, &s.model).unwrap();
  let origin = cam::origin(&st, &s.model).unwrap();
  let expected: Vec<(DVec3, f64)> =
    paths.iter().flat_map(|t| t.moves.iter()).filter(|m| !m.rapid).map(|m| (m.to - origin, m.feed)).collect();
  assert_eq!(run(&mut s, "cam.posts", json!({})).as_array().unwrap().len(), 5);
  for p in &post::POSTS {
    for units in ["mm", "inch"] {
      let r = run(&mut s, "cam.post", json!({"post": p.id, "units": units}));
      assert_eq!(r["post"], p.id);
      let g = r["gcode"].as_str().unwrap();
      let read = post::read(g).unwrap_or_else(|e| panic!("{} {units}: {e}", p.id));
      let got: Vec<(DVec3, f64)> = read.iter().filter(|m| !m.0).map(|m| (m.1, m.2)).collect();
      // inches round to 0.0001", and pecks add up their rounded steps
      let tol = if units == "mm" { 1e-3 } else { 4e-3 };
      // arcs stand in for runs of short moves (and read back as finer steps): the program passes
      // through every toolpath point in order at its feed, and never strays from the toolpath
      // further than an arc may sag from its chords
      let off = |q: DVec3, a: DVec3, b: DVec3| {
        let ab = b - a;
        let t = if ab.length_squared() > 0.0 { ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        (q - (a + ab * t)).length()
      };
      let mut k = 0;
      for (n, (b, fb)) in expected.iter().enumerate() {
        let on = |k: usize| {
          (got[k].0 - *b).length() < tol || (k > 0 && off(*b, got[k - 1].0, got[k].0) < tol + cam::arcs::FIT_TOL)
        };
        while k < got.len() && !on(k) {
          k += 1;
        }
        assert!(k < got.len(), "{} {units}: toolpath point {n} {b:?} is not on the program's path", p.id);
        let fa = got[k].1;
        assert!((fa - fb).abs() < 0.1 + 1e-3 * fb, "{} {units} point {n}: F{fa} vs F{fb}", p.id);
      }
      let path: Vec<DVec3> = expected.iter().map(|e| e.0).collect();
      for (q, _) in &got {
        let d = path.windows(2).map(|w| off(*q, w[0], w[1])).fold(f64::INFINITY, f64::min);
        assert!(d <= cam::arcs::MAX_SAG + tol, "{} {units}: {q:?} strays {d} from the toolpath", p.id);
      }
      assert!(
        g.lines()
          .any(|l| l.starts_with("G2 ") || l.starts_with("G3 ") || l.starts_with("G02 ") || l.starts_with("G03 ")),
        "{} {units}: circles go out as arcs",
        p.id
      );
      let lines: Vec<&str> = g.lines().collect();
      assert!(lines.iter().any(|l| l.contains(if units == "mm" { "G21" } else { "G20" })));
      match p.id {
        "fanuc" => {
          assert!(lines[0] == "%" && lines[1].starts_with("O1000") && g.ends_with("M30\n%\n"));
          assert!(g.contains("T1 M6") && g.contains("G43 H1") && g.contains("G83") && g.contains("G28 G91 Z0."));
          // whole numbers keep their decimal point
          assert!(
            lines
              .iter()
              .filter(|l| ["G0 ", "G1 ", "G2 ", "G3 "].iter().any(|g| l.starts_with(g)))
              .all(|l| { l.split_whitespace().skip(1).all(|w| w.contains('.')) })
          );
        }
        "haas" => assert!(g.contains("T1 M06") && g.contains("G43 H01") && g.contains("G00 ") && g.contains("M03")),
        "grbl" => {
          assert!(!g.contains("M6") && !g.contains("G43") && !g.contains("G83") && !g.contains("G28"));
          assert!(g.contains("M0\n") && !g.starts_with('%'));
        }
        "linuxcnc" => assert!(g.contains("G64 P0.01") && g.ends_with("M2\n") && g.contains("G53 G0 Z0")),
        _ => assert!(g.contains("T1 M6") && g.ends_with("M30\n") && !g.contains('%')),
      }
    }
  }
  // the setup's own post and units are the default
  run(&mut s, "cam.setup", json!({"post": "grbl", "units": "inch"}));
  run(&mut s, "cam.face", json!({"tool_diameter": 20}));
  let r = run(&mut s, "cam.post", json!({}));
  assert_eq!((r["post"].as_str(), r["units"].as_str()), (Some("grbl"), Some("inch")));
  assert!(s.execute("cam.post", &json!({"post": "nope"})).is_err());
  assert!(s.execute("cam.setup", &json!({"units": "furlongs"})).is_err());
}
