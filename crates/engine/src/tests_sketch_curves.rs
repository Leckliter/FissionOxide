//! Tests for elliptical arcs (trim, the command, extrude to exact ellipse faces, DXF ELLIPSE
//! round trips), fit-point splines (Fusion's curve, one face each, closed ones, Trim and Extend),
//! and text fonts from disk, driven through commands.

use serde_json::{Value, json};

use crate::Session;
use fox_sketch::CurveKind;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn sketch(s: &Session) -> &fox_sketch::Sketch {
  let id = s.design.features.iter().rev().find(|f| s.model.sketches.contains_key(&f.id)).unwrap().id;
  &s.model.sketches[&id].sketch
}

/// Makes the sketch being edited a legacy one, as read from a file saved before behaviour flags
/// (Catmull-Rom fit-point splines, one edge per piece).
fn make_legacy(s: &mut Session) {
  let id = s.edit_sketch.expect("a sketch being edited");
  s.design.sketch_mut(id).unwrap().behaviours = fox_sketch::Behaviours::legacy();
}

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().map(|m| m.volume).unwrap_or(0.0)).sum()
}

#[test]
fn a_trimmed_ellipse_extrudes_with_an_exact_elliptic_face() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let e = run(&mut s, "sketch.ellipse", json!({"center": [0, 0], "major": [20, 0], "minor": 10}))["curves"][0]
    .as_u64()
    .unwrap();
  run(&mut s, "sketch.line", json!({"points": [[-30, 0], [30, 0]]}));
  run(&mut s, "sketch.trim", json!({"curve": e, "at": [0, 10]}));
  assert!(sketch(&s).curves.iter().any(|c| matches!(c.kind, CurveKind::EllipticalArc { .. })));
  let r = run(&mut s, "sketch.finish", json!({}));
  let _ = r;
  run(&mut s, "extrude", json!({"profiles": [[0, -5]], "distance": 4}));
  let half = std::f64::consts::PI * 20.0 * 10.0 / 2.0 * 4.0;
  let v = volume(&s);
  assert!((v - half).abs() < 1e-6 * half, "{v} vs {half}");
  // top, bottom, the flat side and one elliptic side: no Bézier facets
  let body = &s.model.bodies[0];
  assert_eq!(body.faces().len(), 4, "{:?}", body.faces().len());
}

#[test]
fn elliptical_arc_command_and_dxf_round_trip() {
  let dir = std::env::temp_dir().join(format!("fox-engine-ellarc-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  // minor larger than "major": the DXF axes swap, the arc must not move
  let r = run(
    &mut s,
    "sketch.elliptical_arc",
    json!({"center": [0, 0], "major": [10, 0], "minor": 15, "start": [10, 0], "end": [-10, 0]}),
  );
  assert_eq!(r["curves"].as_array().map(Vec::len), Some(1), "{r}");
  run(&mut s, "sketch.line", json!({"points": [[-10, 0], [10, 0]]}));
  let r = run(&mut s, "sketch.finish", json!({}));
  let _ = r;
  let before: Vec<[[f64; 2]; 4]> = sketch(&s).curves.iter().flat_map(|c| sketch(&s).beziers(c.id)).collect();
  let path = dir.join("arc.dxf");
  run(&mut s, "export", json!({"path": path}));
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("ELLIPSE"), "an ELLIPSE entity");
  let mut t = Session::default();
  run(&mut t, "import", json!({"path": path}));
  let sk = sketch(&t);
  let arc = sk.curves.iter().find(|c| matches!(c.kind, CurveKind::EllipticalArc { .. })).expect("an elliptical arc");
  let after = sk.beziers(arc.id);
  assert!(
    after.iter().flatten().zip(before.iter().flatten()).all(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-6),
    "{after:?} vs {before:?}"
  );
  // its ends joined the line's
  let CurveKind::EllipticalArc { start, end, .. } = arc.kind else { panic!() };
  let line = sk.curves.iter().find_map(|c| match c.kind {
    CurveKind::Line { a, b } => Some([a, b]),
    _ => None,
  });
  let line = line.unwrap();
  assert!(line.contains(&start) && line.contains(&end));
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_fit_point_spline_extrudes_to_one_side_face() {
  // Fusion's three-point arch: 3630.17 mm³ (tests/reference/198-open-three-point-arch)
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 0], [20, 20], [40, 0]]}));
  run(&mut s, "sketch.line", json!({"points": [[40, 0], [40, -5], [0, -5], [0, 0]]}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": 1, "distance": 5}));
  let v = volume(&s);
  assert!((v - 3_630.170_487_570_409).abs() < 1e-3 * v, "{v}");
  assert_eq!(s.model.bodies[0].faces().len(), 6, "one face for the spline, as in Fusion");
}

#[test]
fn closed_fit_point_splines_have_no_corner() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.spline", json!({"points": [[0, 0], [30, -5], [35, 20], [5, 25]], "closed": true}));
  assert_eq!(r["profiles"], 1, "{r}");
  let sk = sketch(&s);
  let CurveKind::Spline { points, .. } = &sk.curves[0].kind else { panic!("a fit-point spline") };
  assert_eq!(points.first(), points.last(), "closed on its first point");
  // smooth through the start: the pieces meeting there leave and arrive the same way
  let bz = sk.beziers(sk.curves[0].id);
  let (a, b) = (bz.last().unwrap(), bz.first().unwrap());
  let (t1, t0) = ([a[3][0] - a[2][0], a[3][1] - a[2][1]], [b[1][0] - b[0][0], b[1][1] - b[0][1]]);
  assert!((t1[0] * t0[1] - t1[1] * t0[0]).abs() < 1e-9 * t1[0].hypot(t1[1]) * t0[0].hypot(t0[1]));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": 1, "distance": 5}));
  // Fusion: 6079.60 mm³, top, bottom and one side (tests/reference/201-closed-four-point)
  let v = volume(&s);
  assert!((v - 6_079.604_296_233_5).abs() < 1e-3 * v, "{v}");
  assert_eq!(s.model.bodies[0].faces().len(), 3);
}

#[test]
fn a_legacy_fit_point_spline_keeps_its_catmull_rom_shape_and_faces() {
  // a sketch from a file saved before the quintic: main's Catmull-Rom curve, one face per piece
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  make_legacy(&mut s);
  run(&mut s, "sketch.spline", json!({"points": [[0, 0], [20, 20], [40, 0]]}));
  run(&mut s, "sketch.line", json!({"points": [[40, 0], [40, -5], [0, -5], [0, 0]]}));
  let sk = sketch(&s);
  let sp = sk.curves.iter().find(|c| matches!(c.kind, CurveKind::Spline { .. })).unwrap().id;
  let catmull_rom = fox_sketch::spline_beziers(&[[0.0, 0.0], [20.0, 20.0], [40.0, 0.0]]);
  assert_eq!(sk.beziers(sp), catmull_rom, "exactly main's pieces");
  assert_eq!(catmull_rom.len(), 2);
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": 1, "distance": 5}));
  assert_eq!(s.model.bodies[0].faces().len(), 7, "a side face per spline piece, as on main");
}

#[test]
fn fit_point_splines_stay_fit_point_through_trim_and_extend() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  make_legacy(&mut s);
  let sp =
    run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 8], [20, 0], [30, 5]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.line", json!({"points": [[25, -10], [25, 20]]}));
  run(&mut s, "sketch.trim", json!({"curve": sp, "at": [29, 4]}));
  run(&mut s, "sketch.line", json!({"points": [[-10, -20], [-10, 20]]}));
  run(&mut s, "sketch.extend", json!({"curve": sp, "at": [0, 0]}));
  let sk = sketch(&s);
  let c = sk.curves.iter().find(|c| c.id.0 as u64 == sp).unwrap();
  let CurveKind::Spline { points, .. } = &c.kind else { panic!("still a fit-point spline: {:?}", c.kind) };
  let first = sk.pos(points[0]).unwrap();
  let last = sk.pos(*points.last().unwrap()).unwrap();
  assert!((first[0] + 10.0).abs() < 1e-6, "{first:?}");
  assert!((last[0] - 25.0).abs() < 1e-6, "{last:?}");
}

/// A quintic fit-point spline stays one through Trim; Extend then keeps its shape and runs its end
/// on, as Fusion's does (tests/reference/280-extend-spline-to-line).
#[test]
fn quintic_fit_point_splines_trim_and_extend() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let sp =
    run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 8], [20, 0], [30, 5]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.line", json!({"points": [[25, -10], [25, 20]]}));
  run(&mut s, "sketch.trim", json!({"curve": sp, "at": [29, 4]}));
  let id = fox_sketch::CurveId(sp as u32);
  assert!(matches!(sketch(&s).curve(id).map(|c| &c.kind), Some(CurveKind::Spline { .. })), "a fit-point spline");
  let before = sketch(&s).beziers(id);
  // the quintic leaves (0, 0) more steeply than the Catmull-Rom curve (about 2.2 up for 1
  // across), so it reaches x = -10 near y = -22, below the legacy test's line
  run(&mut s, "sketch.line", json!({"points": [[-10, -40], [-10, 20]]}));
  run(&mut s, "sketch.extend", json!({"curve": sp, "at": [0, 0]}));
  let after = sketch(&s).beziers(id);
  let (first, last) = (after[0][0], after.last().unwrap()[3]);
  assert!((first[0] + 10.0).abs() < 1e-6, "{first:?}");
  assert!((last[0] - 25.0).abs() < 1e-6, "{last:?}");
  // the old part is where it was
  assert!(before.iter().all(|b| after.contains(b)), "{before:?} {after:?}");
}

#[test]
fn text_uses_a_font_file_from_disk() {
  let dir = std::env::temp_dir().join(format!("fox-engine-font-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let font = dir.join("FoxEngineTest.ttf");
  std::fs::write(&font, epaint_default_fonts::HACK_REGULAR).unwrap();
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let t = run(&mut s, "sketch.text", json!({"at": [0, 0], "text": "O", "height": 10, "font": font}));
  assert_eq!(t["profiles"], 2, "{t}");
  let f = run(&mut s, "sketch.fonts", json!({}));
  assert_eq!(f["builtin"], json!(["sans", "mono"]));
  assert!(f["system"].is_array());
  let _ = std::fs::remove_dir_all(dir);
}
