//! Tests for Remove Duplicates and Clean Up.

use fox_model::FeatureId;
use fox_sketch::CurveKind;
use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn new_sketch(s: &mut Session) -> FeatureId {
  FeatureId(run(s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap() as u32)
}

fn count(s: &Session, sk: FeatureId, f: fn(&CurveKind) -> bool) -> usize {
  s.design.sketch(sk).unwrap().curves.iter().filter(|c| f(&c.kind)).count()
}

fn lines(s: &Session, sk: FeatureId) -> usize {
  count(s, sk, |k| matches!(k, CurveKind::Line { .. }))
}

#[test]
fn remove_duplicates_keeps_corners_and_the_first_copy() {
  let mut s = Session::default();
  let sk = new_sketch(&mut s);
  // a square, then copies of two of its sides (one reversed, one 0.0005" off) and a corner
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0], [10, 10], [0, 10]], "closed": true}));
  run(&mut s, "sketch.line", json!({"points": [[10, 0], [0, 0]]}));
  run(&mut s, "sketch.line", json!({"points": [[10.0127, 10], [10, 0.0127]]}));
  // a line inside the bottom edge, and one overlapping the left edge part way (sticking out)
  run(&mut s, "sketch.line", json!({"points": [[2, 0], [5, 0]]}));
  run(&mut s, "sketch.line", json!({"points": [[0, 5], [0, 15]]}));
  // a line meeting the square at a corner only, and one continuing an edge end to end
  run(&mut s, "sketch.line", json!({"points": [[10, 10], [20, 20]]}));
  run(&mut s, "sketch.line", json!({"points": [[10, 0], [20, 0]]}));
  // circles and arcs: a copy each, and a smaller concentric circle that stays
  run(&mut s, "sketch.circle", json!({"center": [30, 0], "radius": 5}));
  run(&mut s, "sketch.circle", json!({"center": [30, 0], "radius": 5}));
  run(&mut s, "sketch.circle", json!({"center": [30, 0], "radius": 4}));
  run(&mut s, "sketch.arc", json!({"start": [40, 0], "mid": [45, 5], "end": [50, 0]}));
  run(&mut s, "sketch.arc", json!({"start": [50, 0], "mid": [45, 5], "end": [40, 0]}));
  // the other half of the same circle is not a duplicate
  run(&mut s, "sketch.arc", json!({"start": [40, 0], "mid": [45, -5], "end": [50, 0]}));
  // splines: a reversed copy goes
  run(&mut s, "sketch.spline", json!({"points": [[0, 30], [5, 35], [10, 30]]}));
  run(&mut s, "sketch.spline", json!({"points": [[10, 30], [5, 35], [0, 30]]}));
  // ellipses: the major axis end may be either end
  run(&mut s, "sketch.ellipse", json!({"center": [60, 0], "major": [66, 0], "minor": 3}));
  run(&mut s, "sketch.ellipse", json!({"center": [60, 0], "major": [54, 0], "minor": 3}));
  let before = s.design.sketch(sk).unwrap().curves.len();
  assert_eq!(lines(&s, sk), 10);

  // the dry run only reports
  let dry = run(&mut s, "sketch.remove_duplicates", json!({"dry_run": true}));
  assert_eq!(dry["counts"]["lines"], 3, "{dry}");
  assert_eq!(dry["counts"]["circles"], 1, "{dry}");
  assert_eq!(dry["counts"]["arcs"], 1, "{dry}");
  assert_eq!(dry["counts"]["splines"], 1, "{dry}");
  assert_eq!(dry["counts"]["ellipses"], 1, "{dry}");
  assert_eq!(dry["trimmed"].as_array().unwrap().len(), 1, "{dry}");
  assert!(dry["summary"].as_str().unwrap().ends_with("to remove"), "{dry}");
  assert_eq!(s.design.sketch(sk).unwrap().curves.len(), before);

  // an exact tolerance misses the copy 0.0005" off
  let exact = run(&mut s, "sketch.remove_duplicates", json!({"tolerance": "0.00001 in", "dry_run": true}));
  assert_eq!(exact["counts"]["lines"], 2, "{exact}");

  let done = run(&mut s, "sketch.remove_duplicates", json!({}));
  assert_eq!(done["removed"], 7, "{done}");
  assert_eq!(s.design.sketch(sk).unwrap().curves.len(), before - 7);
  assert_eq!(lines(&s, sk), 7);
  // the overlapping line now runs from the square's corner up
  let trimmed = done["trimmed"][0].as_u64().unwrap() as u32;
  let sketch = s.design.sketch(sk).unwrap();
  let Some(CurveKind::Line { a, b }) = sketch.curve(fox_sketch::CurveId(trimmed)).map(|c| c.kind.clone()) else {
    panic!("no trimmed line")
  };
  let mut ys = [sketch.pos(a).unwrap()[1], sketch.pos(b).unwrap()[1]];
  ys.sort_by(f64::total_cmp);
  assert!((ys[0] - 10.0).abs() < 1e-9 && (ys[1] - 15.0).abs() < 1e-9, "{ys:?}");
  // nothing more to do, and one undo brings them all back
  let again = run(&mut s, "sketch.remove_duplicates", json!({}));
  assert_eq!(again["removed"], 0, "{again}");
  assert_eq!(again["summary"], "No duplicates");
  run(&mut s, "undo", json!({}));
  assert_eq!(s.design.sketch(sk).unwrap().curves.len(), before);
}

#[test]
fn remove_duplicates_skips_construction_unless_asked() {
  let mut s = Session::default();
  let sk = new_sketch(&mut s);
  let a = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  let b = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  let c = b["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.construction", json!({"curves": [c], "on": true}));
  let _ = a;
  let r = run(&mut s, "sketch.remove_duplicates", json!({}));
  assert_eq!(r["removed"], 0, "{r}");
  let r = run(&mut s, "sketch.remove_duplicates", json!({"include_construction": true}));
  assert_eq!(r["removed"], 1, "{r}");
  assert_eq!(lines(&s, sk), 1);
  assert!(s.execute("sketch.remove_duplicates", &json!({"tolerance": "-1 mm"})).is_err());
}

#[test]
fn cleanup_breaks_links_deletes_constraints_and_closes_gaps() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let sk = new_sketch(&mut s);
  let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).map(|f| f.id).unwrap();
  run(&mut s, "sketch.project", json!({"body": s.model.bodies[0].id.0, "face": top}));
  let linked = |s: &Session| match &s.design.feature(sk).unwrap().kind {
    fox_model::FeatureKind::Sketch { links, .. } => links.len(),
    _ => 0,
  };
  assert_eq!(linked(&s), 4);
  // an open path whose ends miss each other by 0.0005", with a dimension and a constraint
  let l1 = run(&mut s, "sketch.line", json!({"points": [[30, 0], [40, 0]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.line", json!({"points": [[40.0127, 0], [40, 10]]}));
  run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [l1], "value": "10 mm"}));
  run(&mut s, "sketch.constrain", json!({"type": "horizontal", "curves": [l1]}));
  run(&mut s, "sketch.point", json!({"at": [50, 50]}));
  // a spline to turn into arcs, and construction geometry
  run(&mut s, "sketch.spline", json!({"points": [[0, 30], [10, 40], [20, 30], [30, 40]]}));
  let cl = run(&mut s, "sketch.line", json!({"points": [[0, -20], [10, -20]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.construction", json!({"curves": [cl], "on": true}));
  let cons = s.design.sketch(sk).unwrap().constraints.len();

  let r = run(
    &mut s,
    "sketch.cleanup",
    json!({"merge_gaps": "0.001 in", "splines_to": "arcs", "construction_to_normal": true}),
  );
  assert_eq!(r["links_broken"], 4, "{r}");
  assert_eq!(linked(&s), 0);
  assert_eq!(r["points_purged"], 1, "{r}");
  assert_eq!(r["gaps_merged"], 1, "{r}");
  assert_eq!(r["splines_converted"], 1, "{r}");
  assert_eq!(r["construction_to_normal"], 1, "{r}");
  assert_eq!(r["dimensions_deleted"], 0, "{r}");
  let sketch = s.design.sketch(sk).unwrap();
  // the projected rectangle's points are free now, and their constraints went
  assert!(sketch.points.iter().filter(|p| p.id != fox_sketch::ORIGIN).all(|p| !p.fixed));
  assert!(sketch.constraints.len() < cons + 1);
  assert!(sketch.curves.iter().all(|c| !c.kind.is_freeform() && !c.construction));
  // the spline became arcs or lines within the tolerance, still ending where it did
  let added = r["curves"].as_array().unwrap().len();
  assert!((1..40).contains(&added), "{r}");
  // the gap is closed: both ends sit in the middle
  let near: Vec<[f64; 2]> =
    sketch.points.iter().map(|p| [p.x, p.y]).filter(|p| (p[0] - 40.0).abs() < 0.1 && p[1].abs() < 0.1).collect();
  assert_eq!(near.len(), 2, "{near:?}");
  assert!((near[0][0] - near[1][0]).abs() < 1e-6 && (near[0][1] - near[1][1]).abs() < 1e-6, "{near:?}");

  // all constraints and dimensions, and splines to a fixed number of lines
  run(&mut s, "undo", json!({}));
  let r = run(
    &mut s,
    "sketch.cleanup",
    json!({"delete_constraints": "all", "delete_dimensions": true, "splines_to": "lines", "segments": 8, "purge_points": false}),
  );
  assert_eq!(r["points_purged"], 0, "{r}");
  assert!(r["dimensions_deleted"].as_u64().unwrap() >= 1, "{r}");
  assert_eq!(r["curves"].as_array().unwrap().len(), 8, "{r}");
  assert!(s.design.sketch(sk).unwrap().constraints.is_empty());
  assert!(s.execute("sketch.cleanup", &json!({"segments": 2, "splines_to": "lines"})).is_err());
  assert!(s.execute("sketch.cleanup", &json!({"delete_constraints": "some"})).is_err());
}
