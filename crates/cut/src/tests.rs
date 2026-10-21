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

#[test]
fn lead_points_pick_the_start() {
  // without a pick the outline starts near the last hole; with one on the far left edge it
  // starts there, the lead-in coming from outside the part
  let pick = [0.0, 40.0];
  let p = plan(&contours(), &CutParams { lead_points: vec![pick], ..params() });
  let outline = p.passes.iter().find(|q| !q.hole).unwrap();
  let start = outline.segs.iter().find(|s| s.1 == SegKind::Cut).unwrap().0.a;
  assert!(dist(start, [-1.0, 40.0]) < 1e-6, "{start:?}");
  assert!(outline.start()[0] < -1.0, "lead-in from the scrap: {:?}", outline.start());
  // a pick in the round hole moves only that hole's start
  let q = plan(&contours(), &CutParams { lead_points: vec![[30.0, 20.5]], ..params() });
  let hole = q.passes.iter().find(|s| s.hole && s.segs.iter().any(|x| !x.0.is_line())).unwrap();
  let start = hole.segs.iter().find(|s| s.1 == SegKind::Cut).unwrap().0.a;
  assert!(dist(start, [30.0, 21.0]) < 1e-6, "{start:?}");
  // too far from any contour: ignored, with a warning
  let r = plan(&contours(), &CutParams { lead_points: vec![[500.0, 500.0]], ..params() });
  assert!(r.warnings.iter().any(|w| w.contains("lead point")), "{:?}", r.warnings);
  assert_eq!(r.passes.len(), plan(&contours(), &params()).passes.len());
}

fn plate_part(quantity: u32) -> crate::NestPart {
  let parts = crate::parts_of(&contours());
  assert_eq!(parts.len(), 1);
  assert_eq!(parts[0].len(), 3, "outline and two holes");
  crate::NestPart { name: "plate".into(), contours: parts[0].clone(), quantity, lead_points: vec![[0.0, 40.0]] }
}

/// Every placed outline inside the sheet's margins and no two closer than the spacing (checked
/// on the polygons, so arcs count).
fn check_nest(n: &crate::Nest, parts: &[crate::NestPart], np: &crate::NestParams) {
  for k in 0..n.sheets.len() {
    let outlines: Vec<Contour> = n.sheets[k].placements.iter().map(|p| p.contour(&parts[p.part].contours[0])).collect();
    for o in &outlines {
      let (lo, hi) = o.bounds();
      let m = np.margin - 0.06;
      assert!(lo[0] >= m && lo[1] >= m, "{lo:?}");
      assert!(hi[0] <= np.sheet_width - m && hi[1] <= np.sheet_height - m, "{hi:?}");
    }
    for i in 0..outlines.len() {
      for j in i + 1..outlines.len() {
        let pa = outlines[i].polygon(0.01);
        let near = pa.iter().map(|p| outlines[j].closest(*p).2).fold(f64::INFINITY, f64::min);
        assert!(near >= np.spacing - 0.15, "parts {i} and {j} on sheet {k} are {near} apart");
        assert!(!outlines[j].contains(pa[0]) && !outlines[i].contains(outlines[j].segs[0].a));
      }
    }
  }
}

#[test]
fn nesting_fills_sheets() {
  // 100 x 60 plates on a 500 x 300 sheet: 4 x 4 fit with 3 mm gaps inside 5 mm margins
  let parts = vec![plate_part(20)];
  let np = crate::NestParams { sheet_width: 500.0, sheet_height: 300.0, ..Default::default() };
  let n = crate::nest(&parts, &np).unwrap();
  assert!(n.unplaced.is_empty(), "{:?}", n.warnings);
  assert_eq!(n.placed(), 20);
  assert_eq!(n.sheets.len(), 2, "{:?}", n.sheets.iter().map(|s| s.placements.len()).collect::<Vec<_>>());
  assert!(n.sheets[0].placements.len() >= 16);
  check_nest(&n, &parts, &np);
  let u = n.utilisation(&parts);
  let area = parts[0].area();
  assert!((area - (6000.0 - 100.0 * std::f64::consts::PI - 100.0)).abs() < 0.5, "{area}");
  assert!((u - 20.0 * area / (2.0 * 500.0 * 300.0)).abs() < 1e-9);
  assert!(n.sheets[0].utilisation > n.sheets[1].utilisation);

  // the sheet's contours: three per part, and their lead points moved with them
  let (cs, leads) = n.sheet(&parts, 0);
  assert_eq!(cs.len(), 3 * n.sheets[0].placements.len());
  assert_eq!(leads.len(), n.sheets[0].placements.len());
  let p = plan(&cs, &params());
  assert_eq!(p.paths.len(), cs.len());
  assert_eq!(p.paths.iter().filter(|c| c.1).count(), 2 * n.sheets[0].placements.len());
}

#[test]
fn nesting_rotates_to_fit() {
  // a 100 x 60 plate fits a 70 x 120 sheet only turned a quarter
  let parts = vec![plate_part(1)];
  let tall = crate::NestParams { sheet_width: 70.0, sheet_height: 120.0, margin: 2.0, ..Default::default() };
  let n = crate::nest(&parts, &tall).unwrap();
  assert_eq!(n.placed(), 1, "{:?}", n.warnings);
  let r = n.sheets[0].placements[0].rotation;
  assert!((r - 90.0).abs() < 1e-3 || (r - 270.0).abs() < 1e-3, "{r}");
  check_nest(&n, &parts, &tall);
  // as drawn only: it doesn't fit
  let n = crate::nest(&parts, &crate::NestParams { rotations: 1, ..tall.clone() }).unwrap();
  assert_eq!(n.placed(), 0);
  assert_eq!(n.unplaced, vec![(0, 1)]);
  assert!(n.sheets.is_empty());
  assert!(n.warnings[0].contains("doesn't fit"), "{:?}", n.warnings);
  // a sheet limit leaves copies out
  let n = crate::nest(&[plate_part(3)], &crate::NestParams { max_sheets: 2, ..tall }).unwrap();
  assert_eq!(n.placed(), 2);
  assert_eq!(n.unplaced, vec![(0, 1)]);
}

#[test]
fn nesting_mixed_parts_and_mirror() {
  // an L-shaped bracket (mirroring changes it) and small discs
  let l = vec![
    Edge::Line { a: [0.0, 0.0], b: [80.0, 0.0] },
    Edge::Line { a: [80.0, 0.0], b: [80.0, 20.0] },
    Edge::Line { a: [80.0, 20.0], b: [20.0, 20.0] },
    Edge::Line { a: [20.0, 20.0], b: [20.0, 60.0] },
    Edge::Line { a: [20.0, 60.0], b: [0.0, 60.0] },
    Edge::Line { a: [0.0, 60.0], b: [0.0, 0.0] },
  ];
  let (lc, _) = chain(&l, JOIN_TOL);
  let (dc, _) = chain(&[Edge::Circle { center: [0.0, 0.0], radius: 8.0 }], JOIN_TOL);
  let parts = vec![
    crate::NestPart { name: "L".into(), contours: lc, quantity: 6, lead_points: Vec::new() },
    crate::NestPart { name: "disc".into(), contours: dc, quantity: 10, lead_points: Vec::new() },
  ];
  let np = crate::NestParams { sheet_width: 250.0, sheet_height: 200.0, spacing: 2.0, mirror: true, ..Default::default() };
  let n = crate::nest(&parts, &np).unwrap();
  assert_eq!(n.placed(), 16, "{:?}", n.warnings);
  assert_eq!(n.sheets.len(), 1);
  check_nest(&n, &parts, &np);
  // mirrored copies keep their arcs' sense consistent: a mirrored disc is still a disc
  for p in &n.sheets[0].placements {
    let c = p.contour(&parts[p.part].contours[0]);
    assert!((c.area().abs() - parts[p.part].contours[0].area().abs()).abs() < 1e-6);
  }
}

#[test]
fn nested_dxf_has_the_sheet() {
  let parts = vec![plate_part(2)];
  let n = crate::nest(&parts, &crate::NestParams { sheet_width: 300.0, sheet_height: 200.0, ..Default::default() }).unwrap();
  let (cs, _) = n.sheet(&parts, 0);
  let dx = find("dxf").unwrap();
  let dp = dx.process(None).unwrap();
  let mut p = plan(&cs, &dp);
  p.sheet = Some(([0.0, 0.0], [300.0, 200.0]));
  let d = dx.output(&p, &dp, "nest", false);
  assert!(d.contains("SHEET"));
  assert_eq!(d.matches("\n8\nSHEET\n").count(), 4);
  assert_eq!(d.matches("\nCIRCLE\n").count() + d.matches("\nARC\n").count() >= 2, true);
  // FireControl is there, marked untested
  let fc = find("firecontrol").unwrap();
  assert!(fc.name.contains("untested"));
  let fp = fc.process(None).unwrap();
  let g = fc.gcode(&plan(&cs, &fp), &fp, "nest", false);
  assert!(g.contains("M3\nG4 P0.5\n") && g.ends_with("M30\n"), "{g}");
}
