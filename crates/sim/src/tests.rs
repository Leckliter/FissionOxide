//! Checks against closed-form results.

use crate::geom::*;
use crate::study::*;
use crate::surface::{SurfaceMesh, box_surface};
use crate::*;

fn part(mesh: TetMesh, size: f64) -> Part {
  Part { name: "Body".into(), mesh, material: Material::steel(), size }
}

fn on(part: usize, faces: &[u32]) -> Vec<FaceSel> {
  faces.iter().map(|&face| FaceSel { part, face }).collect()
}

fn close(a: f64, b: f64, rel: f64) -> bool {
  (a - b).abs() <= rel * b.abs()
}

#[test]
fn element_integrals() {
  let t = fem::tables();
  let load: f64 = t.load.iter().sum();
  assert!((load - 1.0).abs() < 1e-12);
  assert!((t.load[0] + 1.0 / 20.0).abs() < 1e-12 && (t.load[4] - 0.2).abs() < 1e-12);
  let mass: f64 = t.mass.iter().flatten().sum();
  assert!((mass - 1.0).abs() < 1e-12);
  assert!(t.tri_load[0].abs() < 1e-12 && (t.tri_load[3] - 1.0 / 3.0).abs() < 1e-12);
  // rigid motions are free of strain energy
  let geo = fem::Geo::new([[0.0, 0.0, 0.0], [2.0, 0.1, 0.0], [0.3, 1.5, 0.2], [0.1, 0.4, 1.7]]);
  let k = fem::stiffness(&geo, 100.0, 80.0);
  let x = [[0.0, 0.0, 0.0], [2.0, 0.1, 0.0], [0.3, 1.5, 0.2], [0.1, 0.4, 1.7]];
  let mut nodes: Vec<V3> = x.to_vec();
  for [i, j] in mesh::TET_EDGES {
    nodes.push(mid(x[i], x[j]));
  }
  let rot: Vec<f64> = nodes.iter().flat_map(|p| cross([0.3, -0.2, 0.9], *p)).collect();
  let f: Vec<f64> = (0..30).map(|a| (0..30).map(|b| k[a][b] * rot[b]).sum()).collect();
  assert!(f.iter().all(|v| v.abs() < 1e-9), "{f:?}");
}

#[test]
fn delaunay_is_valid() {
  let mut d = delaunay::Delaunay::new([0.0; 3], [1.0; 3]);
  let mut s = 7u64;
  let mut r = || {
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    s as f64 / u64::MAX as f64
  };
  for _ in 0..2000 {
    d.insert([r(), r(), r()], 1e-12);
  }
  d.check().unwrap();
  // empty circumspheres
  let pts = d.pts.clone();
  for (_, t) in d.tets().take(300) {
    let q = t.map(|v| pts[v as usize]);
    for (i, p) in pts.iter().enumerate().skip(4) {
      if t.contains(&(i as u32)) {
        continue;
      }
      assert!(delaunay::in_sphere(q[0], q[1], q[2], q[3], *p) <= 0.0);
    }
  }
}

#[test]
fn meshes_a_box_exactly() {
  let s = box_surface([0.0; 3], [30.0, 20.0, 10.0]);
  let (m, stats) = mesh_solid(&s, &MeshOptions::new(3.0)).unwrap();
  assert_eq!(stats.unrecovered, 0);
  assert!(close(m.volume(), 6000.0, 1e-9), "{}", m.volume());
  assert!(stats.min_quality > 0.02, "{stats:?}");
  assert!(stats.mean_quality > 0.6, "{stats:?}");
  assert_eq!(m.faces(), vec![1, 2, 3, 4, 5, 6]);
  // each face's triangles cover its area
  let area: f64 =
    m.face_tris(&[1]).iter().map(|&t| fem::tri_geo([0, 1, 2].map(|k| m.nodes[m.boundary[t][k] as usize])).0).sum();
  assert!(close(area, 200.0, 1e-9));
}

/// Uniaxial tension: linear fields are reproduced exactly.
#[test]
fn patch_test() {
  let mesh = TetMesh::structured_box([0.0; 3], [10.0, 4.0, 3.0], [3, 2, 2]);
  let model = Model {
    parts: vec![part(mesh, 2.0)],
    constraints: vec![
      Constraint::Frictionless { faces: on(0, &[1]) },
      Constraint::Frictionless { faces: on(0, &[3]) },
      Constraint::Frictionless { faces: on(0, &[5]) },
    ],
    loads: vec![Load::Pressure { faces: on(0, &[2]), pressure: -50.0 }],
    ..Default::default()
  };
  let s = solve_static(&model, SolveOptions::default()).unwrap();
  let e = 210_000.0;
  for (p, u) in model.nodes().iter().zip(&s.displacement) {
    assert!((u[0] - 50.0 / e * p[0]).abs() < 1e-6, "{p:?} {u:?}");
    assert!((u[1] + 0.3 * 50.0 / e * p[1]).abs() < 1e-6);
  }
  for t in &s.stress {
    assert!((t[0] - 50.0).abs() < 1e-3 && t[1].abs() < 1e-3 && t[3].abs() < 1e-3, "{t:?}");
  }
  // the three frictionless faces carry the load between them
  assert!((s.reactions[0][0] + 50.0 * 12.0).abs() < 1e-3, "{:?}", s.reactions);
}

/// 100 × 10 × 10 steel cantilever fixed at x = 0 with 100 N down on its free end.
fn cantilever(size: f64) -> Model {
  let s = box_surface([0.0, -5.0, -5.0], [100.0, 5.0, 5.0]);
  let (mesh, _) = mesh_solid(&s, &MeshOptions::new(size)).unwrap();
  Model {
    parts: vec![part(mesh, size)],
    constraints: vec![Constraint::Fixed { faces: on(0, &[1]), x: true, y: true, z: true }],
    loads: vec![Load::Force { faces: on(0, &[2]), force: [0.0, 0.0, -100.0] }],
    ..Default::default()
  }
}

#[test]
fn cantilever_deflection_and_stress() {
  let model = cantilever(2.5);
  let s = solve_static(&model, SolveOptions::default()).unwrap();
  let (e, i) = (210_000.0, 10.0f64.powi(4) / 12.0);
  // Euler–Bernoulli bending plus Timoshenko shear
  let g = e / 2.6;
  let expect = 100.0 * 100f64.powi(3) / (3.0 * e * i) + 100.0 * 100.0 / (5.0 / 6.0 * g * 100.0);
  let uz = Solution::Static(s.clone()).field("uz", 1).unwrap();
  let tip = probe(&model, &uz, [100.0, 0.0, 0.0]).unwrap();
  assert!(close(-tip, expect, 0.03), "tip {tip} expected {expect}");
  // bending stress at mid-span: M c / I
  let sxx = Solution::Static(s.clone()).field("sxx", 1).unwrap();
  let top = probe(&model, &sxx, [50.0, 0.0, 5.0]).unwrap();
  let bottom = probe(&model, &sxx, [50.0, 0.0, -5.0]).unwrap();
  let sigma = 100.0 * 50.0 * 5.0 / i;
  assert!(close(top, sigma, 0.05) && close(-bottom, sigma, 0.05), "{top} {bottom} vs {sigma}");
  // the support takes the load
  assert!(close(s.reactions[0][2], 100.0, 1e-6), "{:?}", s.reactions);
  assert!(s.safety_factor.iter().fold(f64::INFINITY, |a, b| a.min(*b)) > 1.0);
}

#[test]
fn cantilever_natural_frequency() {
  let model = cantilever(3.0);
  let m = solve_modal(&model, SolveOptions { modes: 4, ..Default::default() }).unwrap();
  let (e, i, a, l, rho): (f64, f64, f64, f64, f64) = (210_000.0, 10.0f64.powi(4) / 12.0, 100.0, 100.0, 7.85e-9);
  let f1 = 1.875104f64.powi(2) / (2.0 * std::f64::consts::PI) * (e * i / (rho * a * l.powi(4))).sqrt();
  let f2 = 4.694091f64.powi(2) / (2.0 * std::f64::consts::PI) * (e * i / (rho * a * l.powi(4))).sqrt();
  // the square section bends the same both ways: two equal first modes; shear and rotary
  // inertia (Timoshenko) lower them slightly
  assert!(close(m.modes[0].frequency, f1, 0.03) && close(m.modes[1].frequency, f1, 0.03), "{:?} vs {f1}", freqs(&m));
  assert!(close(m.modes[2].frequency, f2, 0.06) || close(m.modes[3].frequency, f2, 0.06), "{:?} vs {f2}", freqs(&m));
  assert!(close(m.mass, 7.85 * 10.0 / 1000.0, 1e-6), "{}", m.mass);
  let z = m.modes[0].participation[2] + m.modes[1].participation[2];
  assert!(z > 0.5 && z < 0.7, "{:?}", m.modes[0].participation);
}

fn freqs(m: &study::ModalSolution) -> Vec<f64> {
  m.modes.iter().map(|m| m.frequency).collect()
}

#[test]
fn free_body_has_rigid_modes() {
  let mesh = TetMesh::structured_box([0.0; 3], [20.0, 10.0, 5.0], [4, 2, 1]);
  let model = Model { parts: vec![part(mesh, 5.0)], ..Default::default() };
  let m = solve_modal(&model, SolveOptions { modes: 7, ..Default::default() }).unwrap();
  let f = freqs(&m);
  assert!(f[..6].iter().all(|x| *x < 1e-2 * f[6]), "{f:?}");
}

#[test]
fn conduction_through_a_bar() {
  let s = box_surface([0.0, -5.0, -5.0], [100.0, 5.0, 5.0]);
  let (mesh, _) = mesh_solid(&s, &MeshOptions::new(4.0)).unwrap();
  let model = Model {
    parts: vec![part(mesh, 4.0)],
    thermal: vec![
      ThermalLoad::Temperature { faces: on(0, &[1]), temperature: 100.0 },
      ThermalLoad::Temperature { faces: on(0, &[2]), temperature: 20.0 },
    ],
    ..Default::default()
  };
  let t = solve_thermal(&model, SolveOptions::default()).unwrap();
  for (p, v) in model.nodes().iter().zip(&t.temperature) {
    assert!((v - (100.0 - 0.8 * p[0])).abs() < 1e-6, "{p:?} {v}");
  }
  // q = k A ΔT / L = 0.05 W/(mm·K) · 100 mm² · 80 K / 100 mm
  assert!(close(t.heat_flow[0], 4.0, 1e-6) && close(t.heat_flow[1], -4.0, 1e-6), "{:?}", t.heat_flow);
  assert!(t.flux.iter().all(|q| (q[0] - 0.04).abs() < 1e-8));
  // heated end, convection at the other: ΔT = q L / k, T(L) = T∞ + q / h
  let model = Model {
    thermal: vec![
      ThermalLoad::HeatPower { faces: on(0, &[1]), power: 2.0 },
      ThermalLoad::Convection { faces: on(0, &[2]), coefficient: 1e-4, ambient: 25.0 },
    ],
    ..model
  };
  let t = solve_thermal(&model, SolveOptions::default()).unwrap();
  let temp = |x: f64| probe(&model, &t.temperature, [x, 0.0, 0.0]).unwrap();
  let q = 2.0 / 100.0;
  assert!(close(temp(100.0), 25.0 + q / 1e-4, 1e-6), "{}", temp(100.0));
  assert!(close(temp(0.0) - temp(100.0), q * 100.0 / 0.05, 1e-6));
  assert!(close(t.heat_flow[1], 2.0, 1e-6));
}

/// A rectangular plate with a round hole through its middle (a polygon of `facets` sides,
/// θ = 0 mid-side), faces: top 1, bottom 2, sides −X 3, +X 4, −Y 5, +Y 6, the hole 7.
fn plate_with_hole(w: f64, l: f64, t: f64, r: f64, facets: usize, rings: usize) -> SurfaceMesh {
  let (hw, hl) = (w / 2.0, l / 2.0);
  // angles: the facets' corners and the plate's corners
  let mut angles: Vec<f64> = (0..facets).map(|k| (k as f64 + 0.5) * std::f64::consts::TAU / facets as f64).collect();
  for c in [[hw, hl], [-hw, hl], [-hw, -hl], [hw, -hl]] {
    angles.push(f64::atan2(c[1], c[0]).rem_euclid(std::f64::consts::TAU));
  }
  angles.sort_by(f64::total_cmp);
  // where the ray at an angle leaves the rectangle, and the side (face) it's on
  let rect = |a: f64| {
    let (c, s) = (a.cos(), a.sin());
    let tx = if c.abs() > 1e-12 { hw / c.abs() } else { f64::INFINITY };
    let ty = if s.abs() > 1e-12 { hl / s.abs() } else { f64::INFINITY };
    let k = tx.min(ty);
    [c * k, s * k]
  };
  let side = |p: [f64; 2], q: [f64; 2]| {
    let e = 1e-9;
    if (p[0] - hw).abs() < e && (q[0] - hw).abs() < e {
      4
    } else if (p[0] + hw).abs() < e && (q[0] + hw).abs() < e {
      3
    } else if (p[1] - hl).abs() < e && (q[1] - hl).abs() < e {
      6
    } else {
      5
    }
  };
  let n = angles.len();
  let ring = |i: usize, j: usize| {
    let a = angles[i % n];
    let p = rect(a);
    // the hole: on the polygon through the facet corners (plate corners' rays land on its sides)
    let k = ((a / (std::f64::consts::TAU / facets as f64)) - 0.5).floor();
    let a0 = (k + 0.5) * std::f64::consts::TAU / facets as f64;
    let a1 = a0 + std::f64::consts::TAU / facets as f64;
    let (p0, p1) = ([r * a0.cos(), r * a0.sin()], [r * a1.cos(), r * a1.sin()]);
    // intersect the ray with the chord p0–p1
    let d = [a.cos(), a.sin()];
    let e = [p1[0] - p0[0], p1[1] - p0[1]];
    let den = d[0] * e[1] - d[1] * e[0];
    let u = (p0[0] * e[1] - p0[1] * e[0]) / den;
    let c = [d[0] * u, d[1] * u];
    let f = j as f64 / rings as f64;
    [c[0] + (p[0] - c[0]) * f, c[1] + (p[1] - c[1]) * f]
  };
  let mut points = Vec::new();
  let mut id = std::collections::HashMap::new();
  let mut pt = |i: usize, j: usize, z: f64, points: &mut Vec<V3>| -> u32 {
    *id.entry((i % n, j, z.to_bits())).or_insert_with(|| {
      let q = ring(i, j);
      points.push([q[0], q[1], z]);
      points.len() as u32 - 1
    })
  };
  let mut tris = Vec::new();
  let mut face = Vec::new();
  for i in 0..n {
    for j in 0..rings {
      for (z, f) in [(t, 1), (0.0, 2)] {
        let a = pt(i, j, z, &mut points);
        let b = pt(i + 1, j, z, &mut points);
        let c = pt(i + 1, j + 1, z, &mut points);
        let d = pt(i, j + 1, z, &mut points);
        if f == 1 {
          tris.extend([[a, d, c], [a, c, b]]);
        } else {
          tris.extend([[a, b, c], [a, c, d]]);
        }
        face.extend([f, f]);
      }
    }
    let a = pt(i, rings, 0.0, &mut points);
    let b = pt(i + 1, rings, 0.0, &mut points);
    let c = pt(i + 1, rings, t, &mut points);
    let d = pt(i, rings, t, &mut points);
    tris.extend([[a, b, c], [a, c, d]]);
    face.extend([side(rect(angles[i]), rect(angles[(i + 1) % n])); 2]);
    let a = pt(i, 0, 0.0, &mut points);
    let b = pt(i + 1, 0, 0.0, &mut points);
    let c = pt(i + 1, 0, t, &mut points);
    let d = pt(i, 0, t, &mut points);
    tris.extend([[a, c, b], [a, d, c]]);
    face.extend([7, 7]);
  }
  let m = SurfaceMesh::weld(&points, &tris.iter().flatten().copied().collect::<Vec<_>>(), &face, 1e-9);
  assert!(m.is_closed());
  assert!(m.volume() > 0.0);
  m
}

/// Kt of a hole in a plate in tension (Heywood: 2 + (1 − d/W)³ on the net section stress).
#[test]
fn plate_with_hole_stress_concentration() {
  let (w, l, t, r) = (40.0, 120.0, 4.0, 4.0);
  let s = plate_with_hole(w, l, t, r, 36, 3);
  let opts = MeshOptions { size: 4.0, refine: vec![Refinement { faces: vec![7], size: 0.6 }], grading: 0.3 };
  let (mesh, stats) = mesh_solid(&s, &opts).unwrap();
  assert_eq!(stats.unrecovered, 0);
  let model = Model {
    parts: vec![part(mesh, 4.0)],
    constraints: vec![Constraint::Fixed { faces: on(0, &[5]), x: true, y: true, z: true }],
    loads: vec![Load::Pressure { faces: on(0, &[6]), pressure: -10.0 }],
    ..Default::default()
  };
  let sol = solve_static(&model, SolveOptions::default()).unwrap();
  let nodes = model.nodes();
  // the hole's edge across the load, away from the plate's faces: the nodes of the facets
  // either side of θ = 0 and θ = π
  let edge: Vec<f64> = nodes
    .iter()
    .zip(&sol.stress)
    .filter(|(p, _)| p[1].abs() < 0.4 && (p[0].hypot(p[1]) - r).abs() < 0.05 && p[2] > 0.5 && p[2] < t - 0.5)
    .map(|(_, s)| s[1])
    .collect();
  let peak = edge.iter().sum::<f64>() / edge.len() as f64;
  // far from the hole the stress is the applied one
  let syy: Vec<f64> = sol.stress.iter().map(|t| t[1]).collect();
  assert!(close(probe(&model, &syy, [0.0, 40.0, 2.0]).unwrap(), 10.0, 0.02));
  let d_w = 2.0 * r / w;
  let expect = (2.0 + (1.0 - d_w).powi(3)) * 10.0 / (1.0 - d_w);
  assert!(close(peak, expect, 0.07), "peak {peak} expected {expect} ({stats:?})");
}

#[test]
fn bonded_parts_act_as_one() {
  let a = box_surface([0.0, -5.0, -5.0], [50.0, 5.0, 5.0]);
  let b = box_surface([50.0, -5.0, -5.0], [100.0, 5.0, 5.0]);
  let (ma, _) = mesh_solid(&a, &MeshOptions::new(2.5)).unwrap();
  let (mb, _) = mesh_solid(&b, &MeshOptions::new(2.0)).unwrap();
  let model = Model {
    parts: vec![part(ma, 2.5), part(mb, 2.0)],
    constraints: vec![Constraint::Fixed { faces: on(0, &[1]), x: true, y: true, z: true }],
    loads: vec![Load::Force { faces: on(1, &[2]), force: [0.0, 0.0, -100.0] }],
    ..Default::default()
  };
  assert!(!model.ties().is_empty());
  let s = solve_static(&model, SolveOptions::default()).unwrap();
  let single = cantilever(2.5);
  let one = solve_static(&single, SolveOptions::default()).unwrap();
  let tip = |m: &Model, s: &StaticSolution| {
    let uz: Vec<f64> = s.displacement.iter().map(|d| d[2]).collect();
    probe(m, &uz, [100.0, 0.0, 0.0]).unwrap()
  };
  let (t2, t1) = (tip(&model, &s), tip(&single, &one));
  assert!(close(t2, t1, 0.02), "{t2} vs {t1}");
}

#[test]
fn moment_gravity_and_pin() {
  // an end moment bends the cantilever by M L² / (2 E I)
  let mut model = cantilever(3.0);
  model.loads = vec![Load::Moment { faces: on(0, &[2]), moment: [0.0, 10_000.0, 0.0] }];
  let s = solve_static(&model, SolveOptions::default()).unwrap();
  let uz: Vec<f64> = s.displacement.iter().map(|d| d[2]).collect();
  let tip = probe(&model, &uz, [100.0, 0.0, 0.0]).unwrap();
  let expect = 10_000.0 * 100.0f64.powi(2) / (2.0 * 210_000.0 * 10.0f64.powi(4) / 12.0);
  assert!(close(-tip, expect, 0.04), "{tip} vs {expect}");
  assert!(norm(s.applied) < 1e-9);
  // the support carries the weight under gravity
  model.loads = vec![Load::Gravity { acceleration: [0.0, 0.0, -9806.65] }];
  let s = solve_static(&model, SolveOptions::default()).unwrap();
  let weight = 7.85e-9 * 10_000.0 * 9806.65;
  assert!(close(s.reactions[0][2], weight, 1e-6), "{:?} vs {weight}", s.reactions);
  // a fitted pin axis
  let tris: Vec<(V3, V3, f64)> = (0..12)
    .map(|k| {
      let a = k as f64 * std::f64::consts::TAU / 12.0;
      ([1.0 + 3.0 * a.cos(), 2.0 + 3.0 * a.sin(), k as f64], [a.cos(), a.sin(), 0.0], 1.0)
    })
    .collect();
  let (o, d) = fit_axis(&tris).unwrap();
  assert!((d[2].abs() - 1.0).abs() < 1e-9 && (o[0] - 1.0).abs() < 1e-9 && (o[1] - 2.0).abs() < 1e-9, "{o:?} {d:?}");
}

#[test]
fn bearing_load_pushes_one_side() {
  let mesh = TetMesh::structured_box([0.0; 3], [10.0, 10.0, 10.0], [2, 2, 2]);
  let model = Model {
    parts: vec![part(mesh, 5.0)],
    constraints: vec![Constraint::Fixed { faces: on(0, &[1]), x: true, y: true, z: true }],
    loads: vec![Load::Bearing { faces: on(0, &[2, 4, 6]), force: [-30.0, 0.0, 0.0] }],
    ..Default::default()
  };
  let s = solve_static(&model, SolveOptions::default()).unwrap();
  // only the +X face faces a load along −X
  assert!((s.applied[0] + 30.0).abs() < 1e-9 && s.applied[1].abs() < 1e-12, "{:?}", s.applied);
}

#[test]
fn report_lists_results() {
  let model = cantilever(5.0);
  let sol = Solution::Static(solve_static(&model, SolveOptions::default()).unwrap());
  let input = results::ReportInput { title: "Beam".into(), study: "Static Stress".into(), ..Default::default() };
  let html = report(&input, &model, &sol, false);
  assert!(html.contains("Maximum von mises stress") && html.contains("<table>"));
  let md = report(&input, &model, &sol, true);
  assert!(md.starts_with("# Beam"));
  assert_eq!(colormap(0.0), [0, 0, 255, 255]);
  assert_eq!(colormap(1.0), [255, 0, 0, 255]);
}
