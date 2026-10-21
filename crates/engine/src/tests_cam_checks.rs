//! Tests for tool holder and fixture collision checks, rest roughing in 3D adaptive clearing
//! and pencil finishing with more passes either side of a corner.

use fox_kernel::glam::{DVec2, DVec3};
use fox_model::cam;
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

/// A new body: a box from `lo` to `hi`.
fn slab(s: &mut Session, lo: [f64; 3], hi: [f64; 3]) -> u32 {
  run(s, "sketch.create", json!({"plane": {"origin": [0, 0, lo[2]], "normal": [0, 0, 1]}}));
  run(s, "sketch.rect", json!({"corner1": [lo[0], lo[1]], "corner2": [hi[0], hi[1]]}));
  run(s, "extrude", json!({"distance": hi[2] - lo[2], "operation": "new_body"}));
  s.model.bodies.last().unwrap().id.0
}

fn cuts(t: &cam::Toolpath) -> impl Iterator<Item = &cam::Move> {
  t.moves.iter().filter(|m| !m.rapid)
}

fn kinds(r: &Value) -> Vec<String> {
  r["warnings"].as_array().unwrap().iter().map(|w| w["kind"].as_str().unwrap().to_string()).collect()
}

/// Holder steps, from the holder's face up: a slim step long enough clears the pocket's walls
/// where a plain wide holder on the same stickout hits them, both while generating and in
/// the simulation.
#[test]
fn stepped_holders_clear_or_hit_the_walls() {
  let pocket = |holder: Value, stickout: f64| {
    let mut s = Session::default();
    pocketed(&mut s);
    run(&mut s, "cam.setup", json!({"stock_top": 0}));
    let r = run(
      &mut s,
      "cam.pocket",
      json!({"floors": [{"point": [20, 15, 6]}], "stepdown": 3,
        "tool": {"diameter": 6, "stickout": stickout, "holder": holder}}),
    );
    let generated = run(&mut s, "cam.generate", json!({}));
    let simulated = run(&mut s, "cam.simulate", json!({}));
    (s, r, generated, simulated)
  };
  // a 40 mm holder 2 mm above the tip: 12 − (6 + 2) = 4 mm into the walls at the floor
  let (_, r, g, sim) = pocket(json!("40"), 2.0);
  assert!(kinds(&r).contains(&"holder".to_string()), "{r}");
  let h = g["warnings"].as_array().unwrap().iter().find(|w| w["kind"] == "holder").unwrap_or_else(|| panic!("{g}"));
  assert!((h["depth"].as_f64().unwrap() - 4.0).abs() < 0.3, "{h}");
  assert!(h["message"].as_str().unwrap().contains("holder"), "{h}");
  let h = sim["warnings"].as_array().unwrap().iter().find(|w| w["kind"] == "holder").unwrap_or_else(|| panic!("{sim}"));
  assert!((h["depth"].as_f64().unwrap() - 4.0).abs() < 0.3, "{h}");
  // a 6 mm step 10 long, then the 40: the wide step starts 12 above the tip, at the walls' top
  let (s, r, g, sim) = pocket(json!([{"diameter": 6, "length": 10}, {"diameter": 40}]), 2.0);
  assert!(kinds(&r).is_empty() && kinds(&g).is_empty(), "{r} {g}");
  assert!(!kinds(&sim).contains(&"holder".to_string()), "{sim}");
  let tool = &s.design.cam[0].operations[0].tool;
  assert_eq!(tool.holder_sections(), vec![(2.0, 3.0), (12.0, 20.0)]);
  // written as text, only 3 long: the wide step is 1 mm into the walls
  let (_, _, g, sim) = pocket(json!("6x3, 40"), 2.0);
  let h = g["warnings"].as_array().unwrap().iter().find(|w| w["kind"] == "holder").unwrap_or_else(|| panic!("{g}"));
  assert!((h["depth"].as_f64().unwrap() - 1.0).abs() < 0.3, "{h}");
  assert!(kinds(&sim).contains(&"holder".to_string()), "{sim}");
  // a step narrower than the tool, a step without a length before the last, no stickout
  let mut s = Session::default();
  pocketed(&mut s);
  run(&mut s, "cam.setup", json!({}));
  for bad in [
    json!({"diameter": 6, "stickout": 5, "holder": "4x10, 40"}),
    json!({"diameter": 6, "stickout": 5, "holder": [[10, 0], [40, 0]]}),
    json!({"diameter": 6, "holder": "40"}),
    json!({"diameter": 6, "stickout": 5, "holder": "ten"}),
  ] {
    assert!(s.execute("cam.face", &json!({"tool": bad})).is_err(), "{bad}");
  }
}

/// Holders live in the tool library with the rest of the tool, and come along to operations.
#[test]
fn holders_in_the_tool_library() {
  let dir = std::env::temp_dir().join(format!("fox-cam-holders-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let mut s = Session { store_root: Some(dir.clone()), ..Session::default() };
  let t = run(
    &mut s,
    "cam.tool.add",
    json!({"number": 21, "diameter": 6, "stickout": 18, "holder": "16x20, 25x10, 40", "flute_length": 12}),
  );
  assert_eq!(t["holder_text"], "16x20, 25x10, 40");
  assert_eq!(t["holder"].as_array().unwrap().len(), 3);
  let lib = cam::Library::load(&dir).unwrap();
  let tool = lib.get(21).unwrap();
  assert_eq!(tool.holder_sections(), vec![(18.0, 8.0), (38.0, 12.5), (48.0, 20.0)]);
  // the envelope: the cutter within its radius, then the lowest step reaching out that far
  assert_eq!(tool.envelope(2.0), Some(0.0));
  assert_eq!(tool.envelope(5.0), Some(18.0));
  assert_eq!(tool.envelope(10.0), Some(38.0));
  assert_eq!(tool.envelope(20.0), Some(48.0));
  assert_eq!(tool.envelope(21.0), None);
  pocketed(&mut s);
  run(&mut s, "cam.setup", json!({}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [20, 15, 6]}], "tool_number": 21}));
  assert_eq!(s.design.cam[0].operations[0].tool.holder.len(), 3);
  // and clearing the steps clears the holder
  let t = run(&mut s, "cam.tool.add", json!({"number": 21, "holder": ""}));
  assert_eq!(t["holder"].as_array().unwrap().len(), 0);
  let _ = std::fs::remove_dir_all(&dir);
}

/// Fixtures are bodies (or components) in the setup: not machined, not part of the stock,
/// and a tool running into one is reported by the operation, Generate and Simulate.
#[test]
fn fixtures_are_checked_for_collisions() {
  let mut s = Session::default();
  block(&mut s, 40.0, 30.0, 12.0);
  // a vise jaw 10 high, 2 mm into where the contour's 6 mm tool runs round the part
  let jaw = slab(&mut s, [-8.0, -5.0, 0.0], [-4.0, 35.0, 10.0]);
  let r = run(&mut s, "cam.setup", json!({"stock_side": 2, "stock_top": 1, "fixtures": [jaw]}));
  // the stock is around the part only
  let stock = r["stock"].as_array().unwrap();
  assert!((stock[0][0].as_f64().unwrap() + 2.0).abs() < 1e-6, "{r}");
  assert_eq!(run(&mut s, "cam.list", json!({}))[0]["fixtures"], json!([jaw]));
  // facing stays above the jaw
  let r = run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 2}));
  assert!(kinds(&r).is_empty(), "{r}");
  // the contour down the part's side runs the tool into it
  let r = run(&mut s, "cam.contour", json!({"tool_diameter": 6, "stepdown": 6}));
  assert_eq!(kinds(&r), vec!["fixture"], "{r}");
  let w = &r["warnings"][0];
  assert!(w["operation"] == "2D Contour1" && w["message"].as_str().unwrap().contains("fixture"), "{w}");
  assert!(w["at"][0].as_f64().unwrap() < 0.0 && w["depth"].as_f64().unwrap() > 1.0, "{w}");
  let g = run(&mut s, "cam.generate", json!({}));
  assert_eq!(kinds(&g), vec!["fixture"], "{g}");
  let sim = run(&mut s, "cam.simulate", json!({}));
  assert!(kinds(&sim).contains(&"fixture".to_string()), "{sim}");
  assert!(!kinds(&sim).contains(&"gouge".to_string()), "{sim}");
  // the contour's toolpath leaves the jaw alone: it isn't part of what's machined
  let paths = cam::toolpaths(&s.design.cam[0], &s.model).unwrap();
  assert!(cuts(&paths[1]).all(|m| m.to.x > -3.0 - 1e-6), "the contour stays round the part");
  // the same jaw as a component's body, and a holder clearing it high above
  let c = run(&mut s, "component.create", json!({"name": "Vise", "bodies": [jaw]}))["component"].as_u64().unwrap();
  let r = run(&mut s, "cam.fixtures", json!({"components": [c]}));
  assert_eq!(r["fixtures"], json!([jaw]));
  assert_eq!(kinds(&r), vec!["fixture"], "{r}");
  // no fixtures: the jaw is machined like any other body, so the contour goes round it too
  let r = run(&mut s, "cam.fixtures", json!({}));
  assert!(kinds(&r).is_empty(), "{r}");
  assert!(s.design.cam[0].fixtures.is_empty());
  // a jaw clear of the tool: nothing to report, its top over the face mill's path or not
  let mut f = Session::default();
  block(&mut f, 40.0, 30.0, 12.0);
  let far = slab(&mut f, [-20.0, -5.0, 0.0], [-14.0, 35.0, 10.0]);
  run(&mut f, "cam.setup", json!({"fixtures": [far]}));
  let r = run(&mut f, "cam.contour", json!({"tool_diameter": 6, "stepdown": 6}));
  assert!(kinds(&r).is_empty(), "{r}");
  // a clamp standing over the stock: the face mill's holder runs into it, not the tool
  let clamp = slab(&mut f, [10.0, 40.0, 0.0], [30.0, 50.0, 20.0]);
  run(&mut f, "cam.fixtures", json!({"fixtures": [far, clamp]}));
  let r = run(&mut f, "cam.face", json!({"tool": {"diameter": 10, "stickout": 3, "holder": "50"}, "stepdown": 2}));
  assert_eq!(kinds(&r), vec!["fixture"], "{r}");
  let r = run(&mut f, "cam.face", json!({"tool": {"diameter": 10}, "stepdown": 2}));
  assert!(kinds(&r).is_empty(), "{r}");
  // the part can't be its own fixture, nor can every body be one
  assert!(f.execute("cam.setup", &json!({"body": far, "fixtures": [far]})).is_err());
  let all: Vec<u32> = f.model.bodies.iter().map(|b| b.id.0).collect();
  assert!(f.execute("cam.setup", &json!({"fixtures": all})).is_err());
  assert!(f.execute("cam.fixtures", &json!({"fixtures": [999]})).is_err());
}

/// Rest roughing cuts only the pocket's corners a 10 mm tool left, and leaves about what a
/// full pass of the small tool would.
#[test]
fn adaptive3d_rest_roughing_cuts_what_the_larger_tool_left() {
  let mut s = Session::default();
  pocketed(&mut s);
  run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 1}));
  let rough = json!({"stepover": 0.2, "stepdown": 3, "stock_to_leave": 0.3});
  let with = |extra: Value| {
    let mut p = rough.clone();
    for (k, v) in extra.as_object().unwrap() {
      p[k] = v.clone();
    }
    p
  };
  run(&mut s, "cam.adaptive3d", with(json!({"tool_diameter": 10})));
  run(&mut s, "cam.adaptive3d", with(json!({"tool_diameter": 4, "rest": true})));
  run(&mut s, "cam.adaptive3d", with(json!({"tool_diameter": 4})));
  assert_eq!(s.design.cam[0].operations[1].kind, cam::OpKind::Adaptive3d { rest: true });
  run(&mut s, "cam.generate", json!({}));
  let len = |t: &cam::Toolpath| t.stats(t.moves[0].to).0;
  let (rest, full) = (&s.toolpaths[1], &s.toolpaths[2]);
  assert!(len(rest) > 0.0 && len(rest) < 0.3 * len(full), "{} {}", len(rest), len(full));
  // every rest cut is near one of the pocket's corners (the big tool's reach into a corner, the
  // small tool and some lead-in), or hugs a wall where the big tool's grid kept it a little
  // further off than the stock to leave
  let corners = [(10.0, 8.0), (30.0, 8.0), (30.0, 22.0), (10.0, 22.0)];
  let (mut at_corners, mut hugging) = (0, 0);
  for m in cuts(rest) {
    let p = m.to;
    let q = p.truncate();
    let outside = (q - q.clamp(DVec2::ZERO, DVec2::new(40.0, 30.0))).length();
    let inside = (q.x - 10.0).abs().min((q.x - 30.0).abs()).min((q.y - 8.0).abs()).min((q.y - 22.0).abs());
    let in_pocket = q.x > 10.0 && q.x < 30.0 && q.y > 8.0 && q.y < 22.0;
    if in_pocket && corners.iter().any(|(x, y)| (p.x - x).abs() <= 8.0 && (p.y - y).abs() <= 8.0) {
      at_corners += 1;
    } else if (outside > 2.3 - 1e-6 && outside < 4.0) || (in_pocket && inside > 2.3 - 1e-6 && inside < 4.0) {
      hugging += 1;
    } else {
      panic!("{p:?} is neither in a corner nor by a wall");
    }
    assert!(p.z < 12.3 + 1e-6, "{p:?}");
  }
  assert!(at_corners > 20 && hugging > 0, "{at_corners} {hugging}");
  // the big tool and the rest pass leave about what the big tool and a full small pass leave
  let left = |s: &Session, ops: &[usize]| {
    let mut st = s.design.cam[0].clone();
    st.operations = ops.iter().map(|i| st.operations[*i].clone()).collect();
    let paths = cam::toolpaths(&st, &s.model).unwrap();
    let sim = cam::sim::simulate(&st, &s.model, &paths).unwrap();
    assert!(sim.warnings.is_empty(), "{:?}", sim.warnings);
    sim.remaining()
  };
  let (big, with_rest, with_full) = (left(&s, &[0]), left(&s, &[0, 1]), left(&s, &[0, 2]));
  assert!(with_rest < big - 20.0, "{big} {with_rest}");
  assert!((with_rest - with_full).abs() < 0.15 * (big - with_full), "{big} {with_rest} {with_full}");
  // the same tool again finds nothing left
  assert!(s.execute("cam.adaptive3d", &with(json!({"tool_diameter": 10, "rest": true, "setup": 1}))).is_err());
  // older designs without the flag still read
  let old: cam::OpKind = serde_json::from_value(json!({"type": "adaptive3d"})).unwrap();
  assert_eq!(old, cam::OpKind::Adaptive3d { rest: false });
}

/// Total length of cutting moves at floor height `z` whose ends both satisfy `on`.
fn length_where(t: &cam::Toolpath, z: f64, on: &dyn Fn(DVec3) -> bool) -> f64 {
  let mut along = 0.0;
  let mut at = t.moves[0].to;
  for m in &t.moves {
    if !m.rapid && on(m.to) && on(at) && (m.to.z - z).abs() < 0.05 && (at.z - z).abs() < 0.05 {
      along += (m.to - at).length();
    }
    at = m.to;
  }
  along
}

/// Pencil passes either side of the pocket's floor corners: on the floor, a step-over apart
/// inside the corner pass; none up the walls or on the part's top (the tool would have to
/// jump a cliff to get there).
#[test]
fn pencil_passes_either_side_of_the_corner() {
  let mut s = Session::default();
  pocketed(&mut s);
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(&mut s, "cam.pencil", json!({"tool_kind": "ball", "tool_diameter": 4}));
  run(&mut s, "cam.pencil", json!({"tool_kind": "ball", "tool_diameter": 4, "passes": 2, "stepover": 0.2}));
  assert!(matches!(s.design.cam[0].operations[1].kind, cam::OpKind::Pencil { passes: 2, .. }));
  run(&mut s, "cam.generate", json!({}));
  let (one, three) = (&s.toolpaths[0], &s.toolpaths[1]);
  // centres 2 (the corner), 2.8 and 3.6 off the walls, the ball's tip on the floor
  let ring = |d: f64| {
    move |p: DVec3| {
      let (dx, dy) = ((p.x - 10.0).abs().min((p.x - 30.0).abs()), (p.y - 8.0).abs().min((p.y - 22.0).abs()));
      let inside = p.x > 10.0 && p.x < 30.0 && p.y > 8.0 && p.y < 22.0;
      inside && (((dx - d).abs() < 0.2 && dy > d - 0.2) || ((dy - d).abs() < 0.2 && dx > d - 0.2))
    }
  };
  // the corner pass, 2 off the walls (to the relief grid's node past it)
  let d0 = (0..=8)
    .map(|k| 1.9 + k as f64 * 0.05)
    .max_by(|a, b| length_where(one, 6.0, &ring(*a)).total_cmp(&length_where(one, 6.0, &ring(*b))))
    .unwrap();
  assert!((2.0 - 1e-9..=2.25 + 1e-9).contains(&d0), "{d0}");
  for (k, perimeter) in [(0.0, 52.0), (1.0, 45.6), (2.0, 39.2)] {
    let d = d0 + 0.8 * k;
    let l = length_where(three, 6.0, &ring(d));
    assert!(l > 0.75 * perimeter && l < 1.2 * perimeter, "{d}: {l} of {perimeter}");
  }
  assert!(length_where(one, 6.0, &ring(d0 + 0.8)) < 1.0, "no offsets without passes");
  // nothing up the walls or on the top: off the floor, only round the pocket's vertical corners
  for m in cuts(three).filter(|m| m.to.z > 6.05) {
    let p = m.to;
    let corner = [(10.0, 8.0), (30.0, 8.0), (30.0, 22.0), (10.0, 22.0)]
      .iter()
      .any(|(x, y)| (p.x - x).abs() < 3.5 && (p.y - y).abs() < 3.5);
    assert!(corner, "{p:?}");
  }
  let r = run(&mut s, "cam.simulate", json!({}));
  assert!(kinds(&r).iter().all(|k| k != "gouge" && k != "rapid_collision"), "{r}");
  assert!(s.execute("cam.pencil", &json!({"tool_diameter": 4, "passes": 51})).is_err());
  assert!(s.execute("cam.pencil", &json!({"tool_diameter": 4, "passes": 1.5})).is_err());
}

/// A V-groove's two 45° flanks both get passes, the ball riding up each a step-over at a time.
#[test]
fn pencil_passes_ride_both_flanks_of_a_groove() {
  let mut s = Session::default();
  block(&mut s, 40.0, 30.0, 12.0);
  // a 90° V along Y, 5 deep, its bottom at X 20
  run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, -1, 0], "x": [1, 0, 0]}}));
  run(&mut s, "sketch.line", json!({"points": [[15, 12], [20, 7], [25, 12], [25, 20], [15, 20]], "closed": true}));
  run(&mut s, "extrude", json!({"extent": "through_all_both", "operation": "cut"}));
  let bb = s.model.bodies[0].shape.mass().unwrap();
  assert!((bb.bbox_max.z - 12.0).abs() < 1e-6 && s.model.bodies.len() == 1, "{bb:?}");
  run(&mut s, "cam.setup", json!({"stock_top": 0}));
  run(&mut s, "cam.pencil", json!({"tool_kind": "ball", "tool_diameter": 6, "passes": 2, "stepover": 0.1}));
  run(&mut s, "cam.generate", json!({}));
  let t = &s.toolpaths[0];
  // cutting moves along the groove at `x` (within 0.2), as (start, end)
  let along = |x: f64| -> Vec<(DVec3, DVec3)> {
    let mut out = Vec::new();
    let mut at = t.moves[0].to;
    for m in &t.moves {
      if !m.rapid && (m.to.x - x).abs() < 0.2 && (at.x - x).abs() < 0.2 {
        out.push((at, m.to));
      }
      at = m.to;
    }
    out
  };
  // the length of them between Y 5 and 25, well inside the block's ends
  let inside = |x: f64| -> f64 {
    along(x)
      .iter()
      .map(|(a, b)| {
        let (lo, hi) = (a.y.min(b.y).max(5.0), a.y.max(b.y).min(25.0));
        (hi - lo).max(0.0)
      })
      .sum()
  };
  // the tip's height where they cross Y 15
  let tip = |x: f64| -> f64 {
    let zs: Vec<f64> = along(x)
      .iter()
      .filter(|(a, b)| (a.y - 15.0) * (b.y - 15.0) <= 0.0 && (a.y - b.y).abs() > 1e-9)
      .map(|(a, b)| a.lerp(*b, (15.0 - a.y) / (b.y - a.y)).z)
      .collect();
    assert!(!zs.is_empty(), "no pass at {x}");
    zs.iter().sum::<f64>() / zs.len() as f64
  };
  // the corner pass at the bottom, and two more either side a step-over (0.6) apart
  let x0 = [19.8, 19.9, 20.0, 20.1, 20.2].into_iter().max_by(|a, b| inside(*a).total_cmp(&inside(*b))).unwrap();
  for k in [-2.0, -1.0, 0.0, 1.0, 2.0] {
    let x = x0 + 0.6 * k;
    assert!(inside(x) > 15.0, "{x}: {}", inside(x));
  }
  // the ball riding up each 45° flank: its tip rising 0.6 for each 0.6 out
  let (l2, l1, c, r1, r2) = (tip(x0 - 1.2), tip(x0 - 0.6), tip(x0), tip(x0 + 0.6), tip(x0 + 1.2));
  assert!(l1 > c + 0.2 && r1 > c + 0.2, "{l2} {l1} {c} {r1} {r2}");
  assert!((l2 - l1 - 0.6).abs() < 0.1 && (r2 - r1 - 0.6).abs() < 0.1, "{l2} {l1} {c} {r1} {r2}");
  let r = run(&mut s, "cam.simulate", json!({}));
  assert!(kinds(&r).iter().all(|k| k != "gouge" && k != "rapid_collision"), "{r}");
}
