use std::f64::consts::TAU;

use crate::plan::{CutParams, Lead, LeadKind, SegKind, plan};
use crate::post::{builtin_machines, find, fmt};
use crate::{Contour, Edge, JOIN_TOL, Seg, chain, dist, nesting};

/// A 100 x 60 plate with a 20 mm hole at (30, 30) and a 10 x 10 square hole at (70, 25).
fn plate() -> Vec<Edge> {
  let r = |a: [f64; 2], b: [f64; 2]| Edge::Line { a, b };
  vec![
    // outline, edges out of order and some reversed
    r([100.0, 0.0], [100.0, 60.0]),
    r([0.0, 0.0], [100.0, 0.0]),
    r([0.0, 60.0], [0.0, 0.0]),
    r([0.0, 60.0], [100.0, 60.0]),
    Edge::Circle { center: [30.0, 30.0], radius: 10.0 },
    r([70.0, 25.0], [80.0, 25.0]),
    r([80.0, 35.0], [80.0, 25.0]),
    r([80.0, 35.0], [70.0, 35.0]),
    r([70.0, 35.0], [70.0, 25.0]),
  ]
}

fn contours() -> Vec<Contour> {
  let (c, open) = chain(&plate(), JOIN_TOL);
  assert_eq!(open, 0);
  c
}

#[test]
fn seg_geometry() {
  let s = Seg::arc([0.0, 0.0], 10.0, 0.0, TAU / 4.0);
  let (c, r) = s.center_radius();
  assert!(dist(c, [0.0, 0.0]) < 1e-9 && (r - 10.0).abs() < 1e-9);
  assert!((s.length() - 10.0 * TAU / 4.0).abs() < 1e-9);
  assert!(dist(s.at(0.5), [10.0 * 0.5f64.sqrt(), 10.0 * 0.5f64.sqrt()]) < 1e-9);
  // counter-clockwise at the start: straight up
  assert!(dist(s.tangent(0.0), [0.0, 1.0]) < 1e-9);
  let (x, y) = s.split(0.25);
  assert!((x.length() + y.length() - s.length()).abs() < 1e-9);
  let cw = s.reversed();
  assert!(dist(cw.tangent(0.0), [1.0, 0.0]) < 1e-9);
  assert!(dist(cw.center_radius().0, [0.0, 0.0]) < 1e-9);
  let (f, d) = s.closest([20.0, 20.0]);
  assert!((f - 0.5).abs() < 1e-9 && (d - (20.0 * 2f64.sqrt() - 10.0)).abs() < 1e-9);
}

#[test]
fn chains_and_nests() {
  let c = contours();
  assert_eq!(c.len(), 3);
  let (depth, parent) = nesting(&c);
  let outer = c.iter().position(|c| c.area().abs() > 5000.0).unwrap();
  for (i, d) in depth.iter().enumerate() {
    assert_eq!(*d, usize::from(i != outer));
  }
  assert_eq!(parent.iter().filter(|p| **p == Some(outer)).count(), 2);
  let circle = c.iter().find(|c| (c.area().abs() - std::f64::consts::PI * 100.0).abs() < 1e-6);
  assert!(circle.is_some(), "the circle's area is exact with arcs");
  // an open chain is reported
  let (_, open) = chain(&[Edge::Line { a: [0.0, 0.0], b: [1.0, 0.0] }], JOIN_TOL);
  assert_eq!(open, 1);
}

fn params() -> CutParams {
  CutParams {
    kerf: 2.0,
    lead_in: Lead { kind: LeadKind::Arc, length: 3.0 },
    lead_out: Lead { kind: LeadKind::Line, length: 2.0 },
    ..CutParams::default()
  }
}

#[test]
fn kerf_on_the_scrap_side() {
  let p = plan(&contours(), &params());
  assert!(p.warnings.is_empty(), "{:?}", p.warnings);
  assert_eq!(p.paths.len(), 3);
  for (c, hole) in &p.paths {
    let (lo, hi) = c.bounds();
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    if !hole {
      // outline grows by the kerf: 1 mm each side
      assert!((w - 102.0).abs() < 1e-6 && (h - 62.0).abs() < 1e-6, "{w} x {h}");
      // clockwise (part on the right)
      assert!(c.area() < 0.0);
    } else if (w - 18.0).abs() < 1e-6 {
      // round hole shrinks and stays round: true arcs
      assert!(c.segs.iter().all(|s| !s.is_line()));
      assert!((c.area() - std::f64::consts::PI * 81.0).abs() < 1e-6, "counter-clockwise, radius 9");
    } else {
      assert!((w - 8.0).abs() < 1e-6 && (h - 8.0).abs() < 1e-6, "{w} x {h}");
      assert!(c.area() > 0.0);
    }
  }
  // the other way round
  let q = plan(&contours(), &CutParams { part_on_right: false, ..params() });
  for (c, hole) in &q.paths {
    assert_eq!(c.area() > 0.0, !hole);
    let (lo, hi) = c.bounds();
    if !hole {
      assert!((hi[0] - lo[0] - 102.0).abs() < 1e-6);
    }
  }
}

#[test]
fn holes_before_outline_and_nearest_first() {
  let p = plan(&contours(), &params());
  assert_eq!(p.passes.len(), 3);
  assert!(p.passes[0].hole && p.passes[1].hole && !p.passes[2].hole);
  // the round hole is nearer the origin than the square one
  let first = &p.paths[p.passes[0].contour].0;
  let (lo, _) = first.bounds();
  assert!(lo[0] < 50.0);
  // an island in a hole is cut before the hole, which is cut before the outline
  let mut edges = plate();
  edges.push(Edge::Circle { center: [30.0, 30.0], radius: 4.0 });
  let (c, _) = chain(&edges, JOIN_TOL);
  let p = plan(&c, &params());
  let order: Vec<(bool, f64)> = p.passes.iter().map(|s| (s.hole, p.paths[s.contour].0.area().abs())).collect();
  let island = order.iter().position(|o| !o.0 && o.1 < 100.0).unwrap();
  let big_hole = order.iter().position(|o| o.0 && o.1 > 200.0).unwrap();
  let outline = order.iter().position(|o| !o.0 && o.1 > 5000.0).unwrap();
  assert!(island < big_hole && big_hole < outline, "{order:?}");
}

#[test]
fn leads_in_the_scrap() {
  let p = plan(&contours(), &params());
  for pass in &p.passes {
    let (c, hole) = &p.paths[pass.contour];
    let lead_in = pass.segs.iter().find(|s| s.1 == SegKind::LeadIn).expect("lead-in").0;
    let lead_out = pass.segs.iter().find(|s| s.1 == SegKind::LeadOut).expect("lead-out").0;
    // pierce point in the scrap: inside the hole, outside the outline
    assert_eq!(c.contains(lead_in.a), *hole);
    assert_eq!(c.contains(lead_out.b), *hole);
    // the lead-in is tangent to the cut where it joins it
    let cut = pass.segs.iter().find(|s| s.1 == SegKind::Cut).unwrap().0;
    assert!(dist(lead_in.b, cut.a) < 1e-9);
    assert!(dist(lead_in.tangent(1.0), cut.tangent(0.0)) < 1e-6);
    // the kerf path stays the kerf/2 away from the part: the pierce is further out still
    let (_, _, d) = c.closest(lead_in.a);
    assert!(d > 0.5, "pierce {d} mm from the path");
  }
  // a hole too small for the lead gets a shorter one
  let small = vec![
    Edge::Line { a: [0.0, 0.0], b: [50.0, 0.0] },
    Edge::Line { a: [50.0, 0.0], b: [50.0, 50.0] },
    Edge::Line { a: [50.0, 50.0], b: [0.0, 50.0] },
    Edge::Line { a: [0.0, 50.0], b: [0.0, 0.0] },
    Edge::Circle { center: [25.0, 25.0], radius: 3.0 },
  ];
  let (c, _) = chain(&small, JOIN_TOL);
  let p = plan(&c, &CutParams { lead_in: Lead { kind: LeadKind::Arc, length: 5.0 }, ..params() });
  let hole = p.passes.iter().find(|s| s.hole).unwrap();
  let l = hole.segs[0];
  assert_eq!(l.1, SegKind::LeadIn);
  assert!(dist(l.0.a, [25.0, 25.0]) < 2.0, "pierce near the hole's middle");
}

#[test]
fn hole_smaller_than_kerf_is_dropped() {
  let edges = vec![
    Edge::Line { a: [0.0, 0.0], b: [20.0, 0.0] },
    Edge::Line { a: [20.0, 0.0], b: [20.0, 20.0] },
    Edge::Line { a: [20.0, 20.0], b: [0.0, 20.0] },
    Edge::Line { a: [0.0, 20.0], b: [0.0, 0.0] },
    Edge::Circle { center: [10.0, 10.0], radius: 0.5 },
  ];
  let (c, _) = chain(&edges, JOIN_TOL);
  let p = plan(&c, &params());
  assert_eq!(p.paths.len(), 1);
  assert_eq!(p.warnings.len(), 1);
}

#[test]
fn micro_joints() {
  let p = plan(&contours(), &CutParams { tabs: 4, tab_width: 1.0, ..params() });
  let outline: Vec<_> = p.passes.iter().filter(|s| !s.hole).collect();
  assert_eq!(outline.len(), 5, "four bridges split the outline into five runs");
  let cut: f64 = outline.iter().flat_map(|s| s.segs.iter()).filter(|s| s.1 == SegKind::Cut).map(|s| s.0.length()).sum();
  let total = p.paths.iter().find(|c| !c.1).unwrap().0.length();
  assert!((cut - (total - 4.0)).abs() < 1e-6, "{cut} vs {total}");
  // holes keep no bridges unless asked
  assert_eq!(p.passes.iter().filter(|s| s.hole).count(), 2);
  let q = plan(&contours(), &CutParams { tabs: 2, tab_width: 1.0, tabs_on_holes: true, ..params() });
  assert_eq!(q.passes.len(), 9);
}

#[test]
fn overcut_and_small_holes() {
  let p = plan(&contours(), &CutParams { overcut: 1.5, small_hole_diameter: 25.0, small_hole_feed: 0.5, ..params() });
  for pass in &p.passes {
    let len: f64 = pass.segs.iter().filter(|s| s.1 == SegKind::Cut).map(|s| s.0.length()).sum();
    let c = &p.paths[pass.contour].0;
    assert!((len - c.length() - 1.5).abs() < 1e-6);
    assert_eq!(pass.feed_scale, if pass.hole { 0.5 } else { 1.0 });
  }
}

#[test]
fn posts() {
  let c = contours();
  for m in builtin_machines() {
    let params = m.process(None).unwrap();
    let p = plan(&c, &params);
    let out = m.output(&p, &params, "plate", false);
    assert!(!out.contains('{'), "{}: unfilled placeholder\n{out}", m.id);
    assert!(out.len() > 100);
  }
  let grbl = find("grbl_laser").unwrap();
  let params = CutParams { pierce_delay: 0.2, ..params() };
  let p = plan(&c, &params);
  let g = grbl.gcode(&p, &params, "plate", false);
  assert!(g.contains("G21") && g.contains("M4 S1000") && g.contains("G4 P0.2"));
  assert_eq!(g.matches("M4 S").count(), 3, "one pierce per contour");
  assert!(g.contains("G3 ") || g.contains("G2 "), "true arcs");
  // the round hole: counter-clockwise arcs, centre 9 mm away
  assert!(g.lines().any(|l| l.starts_with("G3") && l.contains(" I")));
  let wazer = find("wazer").unwrap();
  let w = wazer.gcode(&p, &params, "plate", false);
  assert!(!w.lines().any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")), "lines only");
  assert!(w.contains("M3") && w.contains("M5"));
  let q = find("qtplasmac").unwrap();
  let qp = CutParams { material: 2, ..q.process(Some("6 mm mild steel, 45 A")).unwrap() };
  let qg = q.gcode(&plan(&c, &qp), &qp, "plate", false);
  assert!(qg.contains("M190 P2") && qg.contains("M3 $0 S1") && qg.contains("M5 $0") && qg.contains("F1300"));
  let z = find("plasma_z").unwrap();
  let zp = z.process(Some("0")).unwrap();
  let zg = z.gcode(&plan(&c, &zp), &zp, "plate", true);
  assert!(zg.contains("G20") && zg.contains("G0 Z0.15") && zg.contains("G4 P0.5"));
  assert!(z.process(Some("titanium")).is_err());
  let dx = find("dxf").unwrap();
  let dp = dx.process(None).unwrap();
  let d = dx.output(&plan(&c, &dp), &dp, "plate", false);
  assert!(d.contains("AC1009") && d.contains("INNER") && d.contains("ARC") && !d.contains("LEAD\n10"));
  assert_eq!(fmt(-0.0001, 3), "0");
  assert_eq!(fmt(10.0, 3), "10");
  assert_eq!(fmt(1.25, 3), "1.25");
}

#[test]
fn custom_machine_from_json() {
  let m: crate::Machine = serde_json::from_value(serde_json::json!({
    "id": "my_laser", "name": "My laser",
    "header": ["G21", "?pierce_delay (dwell {pierce_delay_ms} ms)"],
    "pierce": ["M3 S{power}"], "off": ["M5"], "footer": ["M2"],
    "processes": [{"name": "fast", "kerf": 0.1, "feed": 2000}]
  }))
  .unwrap();
  assert!(m.arcs);
  let params = m.process(Some("FAST")).unwrap();
  assert_eq!(params.feed, 2000.0);
  assert!(params.compensate, "fields left out take their defaults");
  let g = m.gcode(&plan(&contours(), &params), &params, "x", false);
  assert!(g.starts_with("G21\nG0 X"), "{g}");
  assert!(g.contains("F2000"));
}

#[test]
fn time_and_lengths() {
  let none = Lead { kind: LeadKind::None, length: 0.0 };
  let params = CutParams { kerf: 0.0, lead_in: none, lead_out: none, ..params() };
  let p = plan(&contours(), &params);
  let expect = 320.0 + 20.0 * std::f64::consts::PI + 40.0;
  assert!((p.cut_length() - expect).abs() < 1e-6, "{}", p.cut_length() - expect);
  assert!(p.rapid_length() > 0.0);
  assert!(p.time(&params, 6000.0) > p.cut_length() / params.feed * 60.0);
}
