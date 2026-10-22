//! Tests for turning (lathe setups, operations, simulation and lathe posts) and 4/5-axis milling
//! (3+2, the rotary 4th axis, simultaneous swarf, kinematics in the posts and the simulation).

use fox_kernel::SurfaceKind;
use fox_kernel::glam::DVec3;
use fox_model::cam::axes::{Angles, Kinematics, Machine};
use fox_model::cam::{self, turn};
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

fn session(name: &str) -> Session {
  let mut s = Session::new(crate::model::Design::new(name));
  let dir = std::env::temp_dir().join(format!("fox-cam5-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  s.store_root = Some(dir);
  s
}

/// A shaft down -Z (the XZ sketch's Y is -Z): Ø30 for 40 mm with a 4 mm groove down to Ø24
/// 10–14 mm in, then Ø20 to Z -60.
fn shaft(s: &mut Session) {
  run(s, "sketch.create", json!({"plane": "xz"}));
  let pts = [[0, 0], [15, 0], [15, 10], [12, 10], [12, 14], [15, 14], [15, 40], [10, 40], [10, 60], [0, 60]];
  run(s, "sketch.line", json!({"points": pts, "closed": true}));
  run(s, "sketch.finish", json!({}));
  run(s, "revolve", json!({"axis": "z", "angle": 360}));
}

fn warnings(r: &Value, kind: &str) -> usize {
  r["warnings"].as_array().unwrap().iter().filter(|w| w["kind"] == kind).count()
}

#[test]
fn turning_a_shaft() {
  let mut s = session("turn");
  shaft(&mut s);
  let (lo, hi) = {
    // the part is round about Z, its thin end towards -Z
    let auto = run(&mut s, "cam.setup", json!({"type": "turning"}));
    assert_eq!(auto["axis"], json!([0.0, 0.0, 1.0]), "{auto}");
    run(&mut s, "cam.delete", json!({}));
    let st = run(&mut s, "cam.setup", json!({"type": "turning", "name": "Lathe", "axis": "-z"}));
    assert_eq!(st["type"], "turning");
    assert_eq!(st["bar_diameter"], json!(34.0), "Ø30 and 2 mm a side: {st}");
    // Z0 on the front face, X0 on the axis
    let o = &st["origin"];
    assert!((o[2].as_f64().unwrap() + 60.0).abs() < 1e-6 && o[0].as_f64().unwrap().abs() < 1e-6, "{st}");
    (st["stock"][0].clone(), st["stock"][1].clone())
  };
  // the bar runs from 1 mm in front of the part back past the chuck gap and grip
  assert!((lo[2].as_f64().unwrap() + 61.0).abs() < 1e-3 && (hi[2].as_f64().unwrap() - 20.0).abs() < 1e-3, "{lo} {hi}");
  // milling operations don't go on a lathe, nor lathe operations on a mill
  assert!(s.execute("cam.face", &json!({"tool_diameter": 10})).is_err());
  run(&mut s, "cam.turn_face", json!({"stepdown": 0.5}));
  run(&mut s, "cam.turn_rough", json!({}));
  run(&mut s, "cam.turn_finish", json!({}));
  run(&mut s, "cam.groove", json!({}));
  run(&mut s, "cam.thread", json!({"face": {"point": [10, 0, -50]}, "pitch": 1.5}));
  run(&mut s, "cam.part_off", json!({}));
  // a mill tool can't turn
  assert!(s.execute("cam.turn_rough", &json!({"tool_diameter": 10})).is_err());
  let g = run(&mut s, "cam.generate", json!({}));
  let ops = g["operations"].as_array().unwrap();
  assert_eq!(ops.len(), 6);
  assert!(ops.iter().all(|o| o["moves"].as_u64().unwrap() > 2), "{g}");
  let tools: Vec<u64> = ops.iter().map(|o| o["tool"].as_u64().unwrap()).collect();
  assert_eq!(tools, [11, 11, 12, 13, 15, 14], "library tools by kind, the finishing insert for finishing");

  let st = s.design.cam[0].clone();
  let lathe = st.lathe.clone().unwrap();
  let paths = cam::toolpaths(&st, &s.model).unwrap();
  // the finishing pass: the tip on the part's outline, never inside it
  let finish = &paths[2];
  let rn = finish.tool.corner_radius;
  for m in finish.turn.iter().filter(|m| !m.rapid) {
    // nose centre, in axis coordinates (front at 60)
    let (z, x) = (m.z + 60.0 + rn, m.x + rn);
    let part = |z: f64| -> f64 {
      if !(0.0..=60.0).contains(&z) {
        f64::NEG_INFINITY
      } else if z > 40.0 {
        10.0
      } else {
        15.0
      }
    };
    // the closest the nose comes to the part's profile (seen from the front, grooves filled)
    let mut worst = f64::INFINITY;
    for k in -39..=39 {
      let dz = rn * k as f64 / 40.0;
      let r = part(z + dz);
      if r.is_finite() {
        worst = worst.min(x - (rn * rn - dz * dz).sqrt() - r);
      }
    }
    assert!(worst > -0.02, "the finishing pass cuts into the part at Z{} X{}: {worst}", m.z, m.x);
  }
  // it reaches down the Ø20 and Ø30 diameters and the front face's centre
  let tips: Vec<(f64, f64)> = finish.turn.iter().filter(|m| !m.rapid).map(|m| (m.z, m.x)).collect();
  let along = |x: f64, from: f64, to: f64| {
    tips.windows(2).any(|w| (w[0].1 - x).abs() < 1e-3 && (w[1].1 - x).abs() < 1e-3 && w[0].0 > from && w[1].0 < to)
  };
  assert!(along(10.0, -1.0, -19.0) && along(15.0, -21.0, -59.0), "{tips:?}");
  assert!(tips.iter().any(|t| t.0.abs() < 1e-6 && t.1 <= 0.0), "{tips:?}");
  // the groove plunges to Ø24, the part-off to the centre, the thread 0.92 mm deep in passes
  assert!(paths[3].turn.iter().any(|m| (m.x - 12.0).abs() < 1e-3 && (m.z + 50.0) > -0.01 && m.z + 50.0 < 1.01));
  assert!(paths[5].turn.iter().any(|m| m.x < 0.0 && (m.z + 63.0).abs() < 1e-6), "parted at the back face");
  let threads: Vec<f64> = paths[4].turn.iter().filter(|m| m.pitch > 0.0).map(|m| m.x).collect();
  assert!(threads.len() > 4 && (threads.iter().copied().fold(f64::INFINITY, f64::min) - (10.0 - 0.92)).abs() < 0.01, "{threads:?}");

  // the simulation: everything cut, nothing gouged, the chuck left alone
  let r = run(&mut s, "cam.simulate", json!({}));
  assert_eq!(r["kind"], "turned");
  assert_eq!(warnings(&r, "gouge") + warnings(&r, "chuck") + warnings(&r, "rapid_collision"), 0, "{r}");
  let removed: Vec<f64> = r["removed"].as_array().unwrap().iter().map(|v| v["mm3"].as_f64().unwrap()).collect();
  assert!(removed.iter().all(|v| *v > 1.0), "{removed:?}");
  // a little left in the inside corners under the nose radius
  let left = r["remaining_mm3"].as_f64().unwrap();
  assert!(left < 30.0, "{left}");
  assert!(s.simulation.as_ref().is_some_and(|x| !x.2.mesh(100).indices.is_empty()));

  // a move into the part and one into the chuck are reported
  let mut bad = paths.clone();
  bad[2].turn.push(turn::LatheMove { rapid: false, z: -30.0, x: 13.0, feed: 0.1, pitch: 0.0 });
  bad[2].turn.push(turn::LatheMove { rapid: false, z: -70.0, x: 18.0, feed: 0.1, pitch: 0.0 });
  let sim = turn::simulate(&lathe, &st, &s.model, &bad).unwrap();
  let kinds: Vec<&str> = sim.warnings.iter().map(|w| w.kind.id()).collect();
  assert!(kinds.contains(&"gouge") && kinds.contains(&"chuck"), "{kinds:?}");

  // a part-off blade wider than the gap before the chuck
  let mut t = session("turn-gap");
  shaft(&mut t);
  run(&mut t, "cam.setup", json!({"type": "turning", "chuck_gap": 2, "axis": "-z", "post": "haas"}));
  assert!(t.execute("cam.part_off", &json!({})).is_err());
  assert_eq!(t.design.cam[0].post, "haas_lathe");
}

#[test]
fn lathe_posts_read_back() {
  let mut s = session("lathe-post");
  shaft(&mut s);
  run(&mut s, "cam.setup", json!({"type": "turning", "axis": "-z"}));
  run(&mut s, "cam.turn_rough", json!({"stepdown": 2}));
  run(&mut s, "cam.turn_finish", json!({}));
  run(&mut s, "cam.thread", json!({"z_start": -5, "z_end": -18, "pitch": 1.5}));
  let st = s.design.cam[0].clone();
  let paths = cam::toolpaths(&st, &s.model).unwrap();
  let want: Vec<turn::LatheMove> = paths.iter().flat_map(|t| t.turn.iter().copied()).collect();
  for (post, units) in [
    ("fanuc_lathe", "mm"),
    ("haas_lathe", "mm"),
    ("linuxcnc_lathe", "mm"),
    ("fanuc_lathe", "inch"),
    ("linuxcnc", "inch"),
  ] {
    let g = run(&mut s, "cam.post", json!({"post": post, "units": units}));
    let code = g["gcode"].as_str().unwrap();
    assert!(code.is_ascii(), "{post}: plain ASCII for the control");
    let got = turn::read(code).unwrap_or_else(|e| panic!("{post}: {e}\n{code}"));
    assert_eq!(got.len(), want.len(), "{post} {units}");
    let tol = if units == "inch" { 0.0026 } else { 0.0011 };
    for (a, b) in got.iter().zip(&want) {
      assert!(a.rapid == b.rapid && (a.z - b.z).abs() < tol && (a.x - b.x).abs() < tol, "{post}: {a:?} vs {b:?}");
      assert!((a.pitch - b.pitch).abs() < 1e-3, "{post}: {a:?} vs {b:?}");
      if !b.rapid && b.pitch == 0.0 {
        assert!((a.feed - b.feed).abs() < 2e-3, "{post}: {a:?} vs {b:?}");
      }
    }
    match post {
      "fanuc_lathe" => {
        assert!(code.starts_with("%\nO2000") && code.contains("T1111") && code.contains("G50 S3000"));
        assert!(code.contains("G96 S") && code.contains("G97 S800 M3") && code.contains("G32 ") && code.contains("G99"));
      }
      "haas_lathe" => assert!(code.contains("T1111") && code.contains("G97 S800 M03") && code.contains("G28 U0.")),
      _ => assert!(code.contains("G7") && code.contains("G33 ") && code.contains("G96 D3000") && code.ends_with("M2\n")),
    }
    if units == "inch" {
      assert!(code.contains("G20") && code.contains(" S820 M3"), "250 m/min is 820 ft/min");
    }
  }
  // a milling post for a lathe setup is its lathe counterpart; there's no GRBL lathe
  assert!(s.execute("cam.post", &json!({"post": "grbl"})).is_err());
  let posts = run(&mut s, "cam.posts", json!({}));
  assert_eq!(posts.as_array().unwrap().iter().filter(|p| p["machine"] == "lathe").count(), 3);
}

/// Reads a multi-axis program back: tool tips and axes relative to the work origin, through the
/// machine's kinematics. Straight moves only.
fn read5(g: &str, m: &Machine) -> Vec<(bool, DVec3, DVec3)> {
  let (tilt, turn) = m.kinematics.letters();
  let mut out = Vec::new();
  let (mut pos, mut a, mut motion) = (DVec3::ZERO, Angles::default(), 0);
  for raw in g.lines() {
    let line: String = raw.split('(').next().unwrap_or("").trim().to_string();
    let skip = ["G28", "G53", "G43", "G81", "G83", "G80"];
    if line.is_empty() || skip.iter().any(|g| line.contains(g)) {
      continue;
    }
    let tip = line.contains(['X', 'Y', 'Z']);
    let mut moved = false;
    for w in line.split_whitespace() {
      let (l, v) = w.split_at(1);
      let Ok(v) = v.parse::<f64>() else { continue };
      match l.chars().next().unwrap() {
        'G' if v == 0.0 || v == 1.0 => motion = v as i32,
        'X' => (pos.x, moved) = (v, true),
        'Y' => (pos.y, moved) = (v, true),
        'Z' => (pos.z, moved) = (v, true),
        c if c == tilt => (a.tilt, moved) = (v, true),
        c if Some(c) == turn => (a.turn, moved) = (v, true),
        _ => {}
      }
    }
    if moved && tip {
      let (tip, axis) = m.from_machine(pos, a);
      out.push((motion == 0, tip, axis));
    }
  }
  out
}

/// A 40 × 40 × 20 block with a 6 mm chamfer along its top front edge.
fn chamfered(s: &mut Session) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 40]}));
  run(s, "extrude", json!({"distance": 20}));
  run(s, "chamfer", json!({"edges": [{"point": [20, 0, 20]}], "distance": 6}));
}

#[test]
fn three_plus_two_on_a_tilted_face() {
  let mut s = session("3p2");
  chamfered(&mut s);
  let b = &s.model.bodies[0];
  let face = b
    .faces()
    .iter()
    .find(|f| f.kind == SurfaceKind::Plane && f.normal.z.abs() > 0.5 && f.normal.z.abs() < 0.9)
    .unwrap_or_else(|| panic!("no chamfer: {:?}", b.faces().iter().map(|f| f.normal).collect::<Vec<_>>()));
  // facing up and out of the block
  let n = face.normal * face.normal.z.signum();
  let (bid, fid, c) = (b.id.0, face.id, face.center);
  // without a 4/5-axis machine a tilted axis is refused
  run(&mut s, "cam.setup", json!({}));
  assert!(s.execute("cam.face", &json!({"tool_diameter": 10, "tool_axis": [0, -1, 1]})).is_err());
  run(&mut s, "cam.delete", json!({}));
  let st = run(&mut s, "cam.setup", json!({"machine": "table_table"}));
  assert_eq!(st["machine"], "table_table");
  // the origin: the middle of the stock's bottom
  let o = DVec3::new(20.0, 20.0, 0.0);
  assert!((DVec3::from(serde_json::from_value::<[f64; 3]>(st["origin"].clone()).unwrap()) - o).length() < 1e-6, "{st}");
  run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 2}));
  run(&mut s, "cam.face", json!({"tool_diameter": 10, "work_plane": {"body": bid, "face": fid}, "stepdown": 3}));
  let setup = s.design.cam[0].clone();
  let paths = cam::toolpaths(&setup, &s.model).unwrap_or_else(|e| panic!("{e}"));
  let tilted = &paths[1];
  assert!(tilted.axes.len() == tilted.moves.len() && tilted.axes.iter().all(|a| (*a - n).length() < 1e-9));
  // every cut at or above the chamfer's plane, the last level on it
  let height = |p: DVec3| (p - c).dot(n);
  let cuts: Vec<f64> = tilted.moves.iter().filter(|m| !m.rapid).map(|m| height(m.to)).collect();
  assert!(cuts.iter().all(|h| *h > -1e-6), "{cuts:?}");
  assert!(cuts.iter().any(|h| h.abs() < 2e-3), "{cuts:?}");
  let r = run(&mut s, "cam.simulate", json!({}));
  assert_eq!(r["kind"], "voxels");
  assert_eq!(warnings(&r, "gouge") + warnings(&r, "axis_limit"), 0, "{r}");
  let removed: Vec<f64> = r["removed"].as_array().unwrap().iter().map(|v| v["mm3"].as_f64().unwrap()).collect();
  // the chamfer's prism is 6 × 6 / 2 × 40
  assert!(removed[1] > 0.5 * 720.0, "{removed:?}");
  let a = &r["rotary"]["A"];
  let (a0, a1) = (a[0].as_f64().unwrap(), a[1].as_f64().unwrap());
  assert!((a0 + 45.0).abs() < 1e-6 || (a1 - 45.0).abs() < 1e-6, "{r}");

  // the post: the table turned once to A45, the moves in its frame
  let m = setup.machine.unwrap();
  let g = run(&mut s, "cam.post", json!({"post": "fanuc", "arcs": false}));
  let code = g["gcode"].as_str().unwrap();
  assert!(code.contains("G0 A45. C180.") || code.contains("G0 A-45. C0."), "{code}");
  let back = read5(code, &m);
  let want: Vec<(bool, DVec3)> =
    paths[..2].iter().flat_map(|t| t.moves.iter().map(|m| (m.rapid, m.to - o))).collect();
  assert_eq!(back.len(), want.len());
  for (g, (rapid, p)) in back.iter().zip(&want) {
    assert!(g.0 == *rapid && (g.1 - *p).length() < 2e-3, "{g:?} vs {p}");
  }
  assert!(s.execute("cam.post", &json!({"post": "grbl"})).is_err(), "GRBL has no rotary axes");

  // beyond the machine's tilt
  let mut t = session("3p2-tight");
  chamfered(&mut t);
  run(&mut t, "cam.setup", json!({"machine": "table_table", "tilt_min": -30, "tilt_max": 30}));
  assert!(t.execute("cam.face", &json!({"tool_diameter": 10, "tool_axis": [0, -1, 1]})).is_err());
}

#[test]
fn rotary_wraps_around_the_fourth_axis() {
  let mut s = session("rotary");
  // a Ø20 bar along X, 30 long
  run(&mut s, "sketch.create", json!({"plane": "yz"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "diameter": 20}));
  run(&mut s, "extrude", json!({"distance": 30}));
  let st = run(&mut s, "cam.setup", json!({"machine": "rotary", "stock_side": 2}));
  assert_eq!(st["machine"], "rotary");
  assert!(s.execute("cam.swarf", &json!({"faces": [{"body": 1, "face": 1}], "tool_diameter": 6})).is_err());
  run(&mut s, "cam.rotary", json!({"tool_kind": "ball", "tool_diameter": 6, "stepover": 0.3, "stepdown": 1.5}));
  run(&mut s, "cam.rotary", json!({"tool_kind": "ball", "tool_diameter": 6, "stepover": 0.3, "around": true, "stepdown": 5}));
  let setup = s.design.cam[0].clone();
  let paths = cam::toolpaths(&setup, &s.model).unwrap();
  let o = cam::origin(&setup, &s.model).unwrap();
  let x0 = s.model.bodies[0].shape.mass().unwrap().bbox_min.x;
  assert!((o.y.abs() + o.z.abs()) < 1e-3 && (o.x - x0).abs() < 1e-3, "on the axis: {o}");
  for tp in &paths {
    assert_eq!(tp.axes.len(), tp.moves.len());
    for (m, a) in tp.moves.iter().zip(&tp.axes) {
      let radial = DVec3::new(0.0, m.to.y, m.to.z);
      // the tool square to the axis, pointing away from it
      assert!(a.x.abs() < 1e-9 && (radial.length() < 1e-9 || radial.normalize().dot(*a) > 1.0 - 1e-9));
      if !m.rapid {
        assert!(radial.length() > 10.0 - 0.08, "{} cuts into the bar at {}", tp.name, m.to);
      }
    }
    // the last level finishes on the bar
    assert!(tp.moves.iter().any(|m| !m.rapid && (DVec3::new(0.0, m.to.y, m.to.z).length() - 10.0).abs() < 0.08));
  }
  let r = run(&mut s, "cam.simulate", json!({}));
  assert_eq!(warnings(&r, "gouge") + warnings(&r, "axis_limit") + warnings(&r, "rapid_collision"), 0, "{r}");
  // the stock cylinder (r 12) down to the bar (r 10)
  let left = r["remaining_mm3"].as_f64().unwrap();
  let full = std::f64::consts::PI * (144.0 - 100.0) * 30.0;
  assert!(left < 0.3 * full, "{left} of {full}");
  let m = setup.machine.unwrap();
  let g = run(&mut s, "cam.post", json!({"post": "linuxcnc", "arcs": false}));
  let code = g["gcode"].as_str().unwrap();
  assert!(code.contains(" A") && code.contains("G93") && !code.contains(" C"), "around the axis moves A as it cuts");
  let back = read5(code, &m);
  let want: Vec<(bool, DVec3, DVec3)> = paths
    .iter()
    .flat_map(|t| t.moves.iter().zip(&t.axes).map(|(mv, a)| (mv.rapid, mv.to - o, *a)))
    .collect();
  assert_eq!(back.len(), want.len());
  for (g, w) in back.iter().zip(&want) {
    assert!(g.0 == w.0 && (g.1 - w.1).length() < 2e-3 && (g.2 - w.2).length() < 1e-4, "{g:?} vs {w:?}");
  }
}

#[test]
fn swarf_along_a_drafted_boss() {
  for (kin, pivot) in [("table_table", 0.0), ("head_table", 120.0)] {
    let mut s = session(&format!("swarf-{kin}"));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [-25, -25], "corner2": [25, 25]}));
    run(&mut s, "extrude", json!({"distance": 5}));
    run(&mut s, "sketch.create", json!({"plane": {"point": [0, 0, 5], "normal": [0, 0, 1]}}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "diameter": 30}));
    run(&mut s, "extrude", json!({"distance": 15, "taper": 10, "operation": "join"}));
    let b = &s.model.bodies[0];
    let (body, cone) = (b.id.0, b.faces().iter().find(|f| f.kind == SurfaceKind::Cone).expect("a conical wall").id);
    run(&mut s, "cam.setup", json!({"machine": kin, "pivot": pivot, "stock_top": 0, "stock_side": 0}));
    assert!(s.execute("cam.rotary", &json!({"tool_diameter": 6})).is_err(), "rotary needs a 4th axis");
    assert!(s.execute("cam.swarf", &json!({"faces": [{"body": body, "face": cone}], "tool_kind": "ball", "tool_diameter": 6})).is_err());
    run(&mut s, "cam.swarf", json!({"faces": [{"body": body, "face": cone}], "tool_diameter": 6, "stepdown": 8}));
    let setup = s.design.cam[0].clone();
    let paths = cam::toolpaths(&setup, &s.model).unwrap();
    let tp = &paths[0];
    // the tool leans with the 10° draft, its side on the wall
    for (m, a) in tp.moves.iter().zip(&tp.axes).filter(|(m, _)| !m.rapid) {
      assert!((a.angle_between(DVec3::Z).to_degrees() - 10.0).abs() < 0.5, "{a}");
      let p = m.to;
      let rho = DVec3::new(p.x, p.y, 0.0).length();
      // the wall's radius at the tip's height is 15 - (z - 5) tan 10°, and the tip sits a tool
      // radius out from it, across the wall
      let wall = 15.0 - (p.z - 5.0) * 10f64.to_radians().tan();
      assert!((rho - wall - 3.0 / 10f64.to_radians().cos()).abs() < 0.1, "{p}: {rho} vs {wall}");
    }
    let angles: Vec<f64> = tp.axes.iter().map(|a| a.y.atan2(a.x)).collect();
    let spread = angles.iter().copied().fold(f64::NEG_INFINITY, f64::max) - angles.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(spread > 5.0, "the axis turns around the boss");
    let r = run(&mut s, "cam.simulate", json!({}));
    assert_eq!(warnings(&r, "gouge") + warnings(&r, "axis_limit"), 0, "{kin}: {r}");
    let m = setup.machine.unwrap();
    assert_eq!(m.kinematics, if kin == "head_table" { Kinematics::HeadTable } else { Kinematics::TableTable });
    let o = cam::origin(&setup, &s.model).unwrap();
    for post in ["haas", "fanuc", "linuxcnc", "mach3"] {
      let g = run(&mut s, "cam.post", json!({"post": post}));
      let code = g["gcode"].as_str().unwrap();
      assert!(code.contains("G93") && code.contains(" C"), "{post}: simultaneous moves at inverse-time feeds");
      let back = read5(code, &m);
      assert_eq!(back.len(), tp.moves.len(), "{post}");
      for ((rapid, tip, axis), (mv, a)) in back.iter().zip(tp.moves.iter().zip(&tp.axes)) {
        assert!(*rapid == mv.rapid && (*tip - (mv.to - o)).length() < 3e-3 && (*axis - *a).length() < 1e-4, "{post}");
      }
    }
  }
}
