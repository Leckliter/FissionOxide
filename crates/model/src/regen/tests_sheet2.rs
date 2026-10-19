//! Lofted flanges, miter flanges, corner closures and reliefs, bends running into bends.

use super::*;
use crate::*;
use fox_kernel::glam::DVec3;
use fox_sketch::{PtSpec, Sketch};
use std::f64::consts::PI;

const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };
const YZ: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Yz };

/// t = 2, r = 3, K = 0.5.
fn sheet_design() -> Design {
  let mut d = Design::new("sheet");
  d.set_sheet_rule(SheetMetalRule {
    name: "Steel".into(),
    thickness: "2 mm".into(),
    bend_radius: "3 mm".into(),
    k_factor: "0.5".into(),
    ..SheetMetalRule::default()
  })
  .unwrap();
  d
}

fn ok(m: &Model) {
  assert!(m.errors().is_empty(), "{:?}", m.errors());
}

fn volume(m: &Model) -> f64 {
  m.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

/// Flat pattern size, smallest first.
fn flat_dims(f: &crate::sheet::FlatPattern) -> [f64; 3] {
  let m = f.shape.mass().unwrap();
  let s = m.bbox_max - m.bbox_min;
  let mut d = [s.x, s.y, s.z];
  d.sort_by(f64::total_cmp);
  d
}

/// Flat pattern size in its own plane, smaller first.
fn flat_size(f: &crate::sheet::FlatPattern) -> [f64; 2] {
  let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
  for l in f.shape.edge_lines(0.01).unwrap() {
    for q in l.points {
      let p = f.plane.to_local(q);
      for k in 0..2 {
        lo[k] = lo[k].min(p[k]);
        hi[k] = hi[k].max(p[k]);
      }
    }
  }
  let mut d = [hi[0] - lo[0], hi[1] - lo[1]];
  d.sort_by(f64::total_cmp);
  d
}

fn edge_near(m: &Model, p: [f64; 3]) -> EdgeRef {
  let b = &m.bodies[0];
  let e = b
    .edges()
    .iter()
    .min_by(|a, c| (a.mid - DVec3::from(p)).length().total_cmp(&(c.mid - DVec3::from(p)).length()))
    .unwrap();
  assert!((e.mid - DVec3::from(p)).length() < 1e-6, "no edge at {p:?}");
  EdgeRef::from_edge(b.id, e, b.faces())
}

/// A 100 × 50 plate on XY, 2 thick (z 0..2).
fn plate(d: &mut Design) {
  let mut s = Sketch::new();
  s.add_rectangle([0.0, 0.0], [100.0, 50.0]).unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Flange {
    sketch: s,
    profiles: vec![],
    rule: String::new(),
    flip: false,
    operation: Operation::NewBody,
  })
  .unwrap();
}

fn corner_flanges(d: &mut Design, corners: CornerMode) -> Model {
  plate(d);
  let m = regenerate(d);
  let edges = vec![edge_near(&m, [50.0, 50.0, 2.0]), edge_near(&m, [100.0, 25.0, 2.0])];
  d.add_feature(FeatureKind::EdgeFlange {
    edges,
    height: "20".into(),
    angle: "90 deg".into(),
    flip: false,
    position: BendPosition::Adjacent,
    offset_start: "0".into(),
    offset_end: "0".into(),
    corners,
  })
  .unwrap();
  regenerate(d)
}

#[test]
fn bends_running_into_bends_unfold() {
  // an S: straight, a left arc straight into a right arc, straight
  let mut d = sheet_design();
  let mut s = Sketch::new();
  s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(20.0, 0.0)).unwrap();
  s.add_arc_center(PtSpec::At(20.0, 10.0), PtSpec::At(20.0, 0.0), [30.0, 10.0]).unwrap();
  s.add_arc_center(PtSpec::At(40.0, 10.0), PtSpec::At(40.0, 20.0), [30.0, 10.0]).unwrap();
  s.add_line(PtSpec::At(40.0, 20.0), PtSpec::At(60.0, 20.0)).unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::ContourFlange {
    sketch: s,
    curves: vec![],
    distance: "40".into(),
    symmetric: false,
    flip: false,
    flip_direction: false,
    rule: String::new(),
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 2);
  // the material is on the left: the left arc's inside radius is 10 - t, the right arc's 10;
  // each unrolls to angle × (R + K × T)
  let (t, k) = (2.0, 0.5);
  let len = 20.0 + PI / 2.0 * (8.0 + k * t) + PI / 2.0 * (10.0 + k * t) + 20.0;
  let dims = flat_dims(&flat);
  assert!(
    (dims[0] - t).abs() < 1e-3 && (dims[1] - 40.0).abs() < 1e-3 && (dims[2] - len).abs() < 1e-3,
    "{dims:?} {len}"
  );
  assert!((flat.shape.mass().unwrap().volume - volume(&m)).abs() < 0.05);
}

#[test]
fn miter_flange_along_two_edges() {
  let mut d = sheet_design();
  plate(&mut d);
  // up 20 from the top edge (y = 50, z = 2), then 10 outwards, drawn on YZ (across the edge)
  let mut s = Sketch::new();
  s.add_line(PtSpec::At(50.0, 2.0), PtSpec::At(50.0, 22.0)).unwrap();
  s.add_line(PtSpec::At(50.0, 22.0), PtSpec::At(60.0, 22.0)).unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: YZ, sketch: s, links: Vec::new() }).unwrap();
  let m = regenerate(&mut d);
  let edges = vec![edge_near(&m, [50.0, 50.0, 2.0]), edge_near(&m, [100.0, 25.0, 2.0])];
  d.add_feature(FeatureKind::MiterFlange { sketch: s, curves: vec![], edges, corners: CornerMode::Miter }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  assert!(m.bodies[0].shape.is_valid());
  let (lo, hi) = m.bounds().unwrap();
  assert!((hi.z - 22.0).abs() < 1e-6 && (hi.y - 60.0).abs() < 1e-6 && (hi.x - 110.0).abs() < 1e-6, "{lo:?} {hi:?}");
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 4);
  assert!(flat.shape.is_valid());
  // each flange: a 90° bend (inside radius r on the path), 20 - r - (r + t) up, a 90° bend the
  // other way (the path is its outside), 10 - (r + t) out
  let (t, r, k) = (2.0, 3.0, 0.5);
  let ba = PI / 2.0 * (r + k * t);
  let flange = ba + (20.0 - r - (r + t)) + ba + (10.0 - (r + t));
  let dims = flat_dims(&flat);
  assert!((dims[0] - t).abs() < 1e-3, "{dims:?}");
  // the profile lands where it was drawn, so the plate is trimmed back by the first bend (r)
  assert!((dims[1] - (50.0 - r + flange)).abs() < 1e-3 && (dims[2] - (100.0 - r + flange)).abs() < 1e-3, "{dims:?}");
}

#[test]
fn corner_closures() {
  let miter = corner_flanges(&mut sheet_design(), CornerMode::Miter);
  ok(&miter);
  let mut d = sheet_design();
  let closed = corner_flanges(&mut d, CornerMode::Closed);
  ok(&closed);
  assert!(closed.bodies[0].shape.is_valid());
  // the gap at the seam is filled
  assert!(volume(&closed) > volume(&miter) + 1.0, "{} {}", volume(&closed), volume(&miter));
  assert_eq!(closed.bodies[0].flat_pattern(None).unwrap().bends.len(), 2);

  let mut d = sheet_design();
  let over = corner_flanges(&mut d, CornerMode::Overlap);
  ok(&over);
  assert!(over.bodies[0].shape.is_valid());
  let mm = over.bodies[0].shape.mass().unwrap();
  // the first flange runs over the end of the second: out to its outside face
  assert!((mm.bbox_max.x - 105.0).abs() < 1e-6 && (mm.bbox_max.y - 55.0).abs() < 1e-6, "{mm:?}");
  let flat = over.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 2);
  assert!(flat.shape.is_valid());
}

#[test]
fn corner_reliefs() {
  let base = volume(&corner_flanges(&mut sheet_design(), CornerMode::Miter));
  for relief in [CornerRelief::Round, CornerRelief::Square, CornerRelief::Tear] {
    let mut d = sheet_design();
    d.sheet_metal_rules[0].corner_relief = relief;
    let m = corner_flanges(&mut d, CornerMode::Miter);
    ok(&m);
    assert!(volume(&m) < base - 0.1, "{relief:?}");
    assert!(m.bodies[0].flat_pattern(None).is_ok(), "{relief:?}");
  }
  // a square relief of the given size through the sheet at the corner (the bends are cut too)
  let mut d = sheet_design();
  d.sheet_metal_rules[0].corner_relief = CornerRelief::Square;
  d.sheet_metal_rules[0].corner_relief_size = "4 mm".into();
  let m = corner_flanges(&mut d, CornerMode::Open);
  ok(&m);
  let open = volume(&corner_flanges(&mut sheet_design(), CornerMode::Open));
  assert!(volume(&m) < open - 2.0 * 2.0 * 2.0 + 1e-6, "{} {open}", volume(&m));
}

/// Two circles of radius 20, 30 apart.
fn two_circles(d: &mut Design) -> (FeatureId, FeatureId) {
  let mut s = Sketch::new();
  s.add_circle(PtSpec::At(0.0, 0.0), 20.0).unwrap();
  let a = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s.clone(), links: Vec::new() }).unwrap();
  let pl = d.add_feature(FeatureKind::ConstructionPlane { base: XY, offset: "30".into() }).unwrap();
  let b = d
    .add_feature(FeatureKind::Sketch { plane: PlaneRef::Construction { feature: pl }, sketch: s, links: Vec::new() })
    .unwrap();
  (a, b)
}

fn lofted(d: &mut Design, a: FeatureId, b: FeatureId, method: LoftMethod, bends: &str) -> Model {
  d.add_feature(FeatureKind::LoftedFlange {
    from: LoftProfile { sketch: a, curves: vec![] },
    to: LoftProfile { sketch: b, curves: vec![] },
    method,
    bends: bends.into(),
    flip: false,
    rule: String::new(),
  })
  .unwrap();
  regenerate(d)
}

#[test]
fn die_formed_lofted_flange_develops_to_its_neutral_layer() {
  let mut d = sheet_design();
  let (a, b) = two_circles(&mut d);
  let m = lofted(&mut d, a, b, LoftMethod::DieFormed, "8");
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  let (t, r, k, gap) = (2.0, 20.0, 0.5, 1.0);
  // material outside the circles (the profiles are the inside of the bend)
  let mm = m.bodies[0].shape.mass().unwrap();
  // (the seam is where the circles start, at +x)
  assert!((mm.bbox_max.z - 30.0).abs() < 1e-6 && (mm.bbox_min.x + r + t).abs() < 1e-3, "{mm:?}");
  let sweep = 2.0 * PI - gap / r;
  assert!((mm.volume - sweep / 2.0 * ((r + t) * (r + t) - r * r) * 30.0).abs() < 0.005 * mm.volume, "{}", mm.volume);
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert!(flat.bends.is_empty(), "die-formed: no bend lines");
  let dims = flat_size(&flat);
  let width = sweep * (r + k * t);
  assert!((dims[0] - 30.0).abs() < 1e-6 && (dims[1] - width).abs() < 0.05, "{dims:?} {width}");
  assert!((flat.shape.mass().unwrap().volume - 30.0 * width * t).abs() < 0.01 * mm.volume);
}

#[test]
fn press_brake_lofted_flanges() {
  // a cylinder from circles: 11 bends between 12 flat facets
  let mut d = sheet_design();
  let (a, b) = two_circles(&mut d);
  let m = lofted(&mut d, a, b, LoftMethod::PressBrake, "11");
  ok(&m);
  assert!(m.bodies[0].shape.is_valid());
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 11);
  assert!(flat.bends.iter().all(|b| (b.angle - 2.0 * PI / 12.0).abs() < 0.01 && b.inner), "{:?}", flat.bends);
  // K = 0.5: the flat has the folded volume
  let fv = flat.shape.mass().unwrap().volume;
  assert!((fv - volume(&m)).abs() < 0.01 * fv, "{fv} {}", volume(&m));

  // a square to a smaller square: four flat trapezoids, four bends (the seam is mid-side)
  let mut d = sheet_design();
  let mut s = Sketch::new();
  s.add_rectangle([-20.0, -20.0], [20.0, 20.0]).unwrap();
  let a = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s, links: Vec::new() }).unwrap();
  let pl = d.add_feature(FeatureKind::ConstructionPlane { base: XY, offset: "30".into() }).unwrap();
  let mut s = Sketch::new();
  s.add_rectangle([-10.0, -10.0], [10.0, 10.0]).unwrap();
  let b = d
    .add_feature(FeatureKind::Sketch { plane: PlaneRef::Construction { feature: pl }, sketch: s, links: Vec::new() })
    .unwrap();
  let m = lofted(&mut d, a, b, LoftMethod::PressBrake, "3");
  ok(&m);
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 4, "{:?}", flat.bends);
  let fv = flat.shape.mass().unwrap().volume;
  assert!((fv - volume(&m)).abs() < 0.02 * fv, "{fv} {}", volume(&m));
}

#[test]
fn rule_corner_relief_survives_a_round_trip() {
  let mut d = sheet_design();
  d.sheet_metal_rules[0].corner_relief = CornerRelief::Tear;
  let json = serde_json::to_string(&d).unwrap();
  assert!(json.contains("\"corner_relief\":\"tear\""));
  // files written before corner reliefs and lofted / miter flanges existed still open
  let old = json.replace(",\"corner_relief\":\"tear\",\"corner_relief_size\":\"\"", "");
  let back: Design = serde_json::from_str(&old).unwrap();
  assert_eq!(back.sheet_metal_rules[0].corner_relief, CornerRelief::None);
}
