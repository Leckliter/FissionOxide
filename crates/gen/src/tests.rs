use std::sync::atomic::Ordering;

use crate::fe::{System, element_stiffness};
use crate::surface::extract;
use crate::*;

fn progress() -> Progress {
  Progress::default()
}

#[test]
fn element_stiffness_is_symmetric_and_rigid_motions_cost_nothing() {
  let k = element_stiffness(2.0, 0.3);
  for r in 0..24 {
    for c in 0..24 {
      assert!((k[r * 24 + c] - k[c * 24 + r]).abs() < 1e-12);
    }
    assert!(k[r * 25] > 0.0);
  }
  // a translation and a small rotation about z
  let corners = fe::CORNERS;
  for u in [
    (0..24).map(|d| if d % 3 == 0 { 1.0 } else { 0.0 }).collect::<Vec<f64>>(),
    (0..24)
      .map(|d| {
        let c = corners[d / 3].map(|v| v as f64);
        match d % 3 {
          0 => -c[1],
          1 => c[0],
          _ => 0.0,
        }
      })
      .collect(),
  ] {
    for r in 0..24 {
      let f: f64 = (0..24).map(|c| k[r * 24 + c] * u[c]).sum();
      assert!(f.abs() < 1e-9, "row {r}: {f}");
    }
  }
}

/// A bar fixed at one end and pulled at the other stretches by F L / (E A).
#[test]
fn bar_in_tension_matches_hand_calculation() {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [20, 2, 2] };
  let ke = element_stiffness(g.h, 0.0);
  let e = vec![1000.0; g.elements()];
  let mut fixed = vec![false; 3 * g.nodes()];
  let mut f = vec![0.0; 3 * g.nodes()];
  for k in 0..=2 {
    for j in 0..=2 {
      let n = g.node(0, j, k);
      fixed[3 * n..3 * n + 3].fill(true);
      // consistent nodal loads of a uniform traction on the end face
      let w = [0.25, 0.5, 0.25][j] * [0.25, 0.5, 0.25][k];
      f[3 * g.node(20, j, k)] = 400.0 * w;
    }
  }
  let sys = System { grid: &g, ke: &ke, e: &e, fixed: &fixed, threads: 2 };
  let mut u = vec![0.0; f.len()];
  sys.solve(&f, &mut u, 1e-10, 5000);
  let tip = u[3 * g.node(20, 1, 1)];
  let exact = 400.0 * 20.0 / (1000.0 * 4.0);
  assert!((tip - exact).abs() < 0.02 * exact, "tip {tip} vs {exact}");
  // the Jacobi-preconditioned solve agrees
  let mut v = vec![0.0; f.len()];
  sys.solve_jacobi(&f, &mut v, 1e-10, 5000);
  assert!((v[3 * g.node(20, 1, 1)] - tip).abs() < 1e-6 * exact);
}

/// Multigrid takes far fewer iterations than Jacobi on a grid with coarse levels, high
/// contrast included, and finds the same displacements.
#[test]
fn multigrid_preconditioner() {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [24, 12, 9] };
  let ke = element_stiffness(g.h, 0.3);
  // a checkerboard of solid and soft blocks
  let e: Vec<f64> = (0..g.elements())
    .map(|el| {
      let [i, j, k] = g.elem_ijk(el);
      if (i / 4 + j / 4 + k / 3) % 2 == 0 || j < 2 { 1.0 } else { 1e-6 }
    })
    .collect();
  let mut fixed = vec![false; 3 * g.nodes()];
  for k in 0..=9 {
    for j in 0..=12 {
      let n = g.node(0, j, k);
      fixed[3 * n..3 * n + 3].fill(true);
    }
  }
  // one roller node on its own
  fixed[3 * g.node(24, 0, 0) + 1] = true;
  let mut f = vec![0.0; 3 * g.nodes()];
  f[3 * g.node(24, 0, 4) + 2] = -1.0;
  let sys = System { grid: &g, ke: &ke, e: &e, fixed: &fixed, threads: 2 };
  let (mut a, mut b) = (vec![0.0; f.len()], vec![0.0; f.len()]);
  let mg = sys.solve(&f, &mut a, 1e-8, 5000);
  let jac = sys.solve_jacobi(&f, &mut b, 1e-8, 20_000);
  assert!(mg * 5 < jac, "multigrid {mg} vs Jacobi {jac} iterations");
  let peak = b.iter().fold(0.0f64, |m, v| m.max(v.abs()));
  let diff = a.iter().zip(&b).fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
  assert!(diff < 1e-4 * peak, "{diff} vs {peak}");
}

/// The half MBB beam of Sigmund's 99-line code, as a one-element-thick slab.
fn mbb(nx: usize, ny: usize, vf: f64) -> Problem {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [nx, ny, 1] };
  let mut case = LoadCase { name: "MBB".into(), ..Default::default() };
  for k in 0..=1 {
    case.forces.push((g.node(0, ny, k), [0.0, -0.5, 0.0]));
    case.fixed.push((g.node(nx, 0, k), [false, true, false]));
    for j in 0..=ny {
      case.fixed.push((g.node(0, j, k), [true, false, false]));
    }
  }
  for n in 0..g.nodes() {
    case.fixed.push((n, [false, false, true]));
  }
  Problem {
    grid: g,
    cells: vec![Cell::Design; g.elements()],
    start: None,
    cases: vec![case],
    material: Material { name: "unit".into(), youngs: 1.0, poisson: 0.3, density: 1.0, yield_strength: 1.0 },
    objective: Objective::MaxStiffness { volume_fraction: vf },
    limits: Limits { safety_factor: 0.0, max_displacement: None },
    method: Method::Unrestricted,
    symmetry: [false; 3],
    settings: Settings { filter_radius: 1.5, max_iterations: 60, threads: 1, ..Settings::default() },
  }
}

fn at(g: &Grid, d: &[f32], i: usize, j: usize) -> f32 {
  d[g.elem(i, j, 0)]
}

#[test]
fn mbb_beam_compliance_falls_volume_met_and_classic_truss() {
  let p = mbb(60, 20, 0.5);
  let pr = progress();
  let o = solve(&p, &pr).unwrap();
  assert!(pr.iteration.load(Ordering::Relaxed) > 5);
  let h = &o.history;
  // compliance falls from the uniform grey start
  assert!(h.last().unwrap().0 < 0.5 * h[0].0, "{:?} -> {:?}", h[0], h.last());
  // the volume target is met
  for (_, v) in &h[3..] {
    assert!(*v <= 0.5 + 0.01, "volume {v}");
  }
  assert!((o.volume_fraction - 0.5).abs() < 0.06, "crisp volume fraction {}", o.volume_fraction);
  let g = &p.grid;
  let d = &o.density;
  // a crisp design
  let grey = d.iter().filter(|v| (0.1..0.9).contains(&(**v as f64))).count();
  assert!((grey as f64) < 0.15 * d.len() as f64, "{grey} grey elements");
  // the classic shape: a top chord from the load, a bottom chord to the support, diagonals
  // between them and the corner above the support cut away
  let top = (0..40).filter(|i| at(g, d, *i, 19) > 0.5).count();
  assert!(top >= 36, "top chord {top}/40");
  let bottom = (0..60).filter(|i| at(g, d, *i, 0) > 0.5).count();
  assert!(bottom >= 54, "bottom chord {bottom}/60");
  let corner = (54..60).flat_map(|i| (14..20).map(move |j| (i, j))).filter(|(i, j)| at(g, d, *i, *j) > 0.5).count();
  assert!(corner <= 4, "the top right corner is empty: {corner}/36");
  let middle = (0..60).filter(|i| at(g, d, *i, 10) > 0.5).count();
  assert!(middle < 30, "the middle row is mostly holes: {middle}/60");
  assert!(holes(g, d) >= 3, "{} holes", holes(g, d));
  // load at the top left, support at the bottom right: both solid
  assert!(at(g, d, 0, 19) > 0.5 && at(g, d, 59, 0) > 0.5);
}

/// Empty regions not connected to the border of a one-layer grid.
fn holes(g: &Grid, d: &[f32]) -> usize {
  let (nx, ny) = (g.n[0], g.n[1]);
  let mut seen = vec![false; nx * ny];
  let mut count = 0;
  for start in 0..nx * ny {
    if seen[start] || d[start] > 0.5 {
      continue;
    }
    let mut stack = vec![start];
    seen[start] = true;
    let mut border = false;
    let mut size = 0;
    while let Some(c) = stack.pop() {
      size += 1;
      let (i, j) = (c % nx, c / nx);
      if i == 0 || j == 0 || i == nx - 1 || j == ny - 1 {
        border = true;
      }
      let mut push = |i: usize, j: usize| {
        let n = i + nx * j;
        if !seen[n] && d[n] <= 0.5 {
          seen[n] = true;
          stack.push(n);
        }
      };
      if i > 0 {
        push(i - 1, j);
      }
      if i + 1 < nx {
        push(i + 1, j);
      }
      if j > 0 {
        push(i, j - 1);
      }
      if j + 1 < ny {
        push(i, j + 1);
      }
    }
    if !border && size >= 3 {
      count += 1;
    }
  }
  count
}

#[test]
fn lower_volume_target_gives_less_stiff_design() {
  let mut c = Vec::new();
  for vf in [0.3, 0.5] {
    let mut p = mbb(30, 10, vf);
    p.settings.max_iterations = 40;
    c.push(solve(&p, &progress()).unwrap().compliance);
  }
  assert!(c[0] > c[1], "{c:?}");
}

/// A cantilever block fixed on its -x face, loaded down at the +x end.
fn cantilever(n: [usize; 3]) -> Problem {
  let g = Grid { origin: [0.0; 3], h: 1.0, n };
  let mut case = LoadCase { name: "tip".into(), ..Default::default() };
  for k in 0..=n[2] {
    for j in 0..=n[1] {
      case.fixed.push((g.node(0, j, k), [true; 3]));
    }
  }
  for k in 0..=n[2] {
    case.forces.push((g.node(n[0], n[1] / 2, k), [0.0, 0.0, -10.0 / (n[2] + 1) as f64]));
  }
  Problem {
    grid: g,
    cells: vec![Cell::Design; g.elements()],
    start: None,
    cases: vec![case],
    material: material("Aluminum 6061").unwrap(),
    objective: Objective::MaxStiffness { volume_fraction: 0.3 },
    limits: Limits::default(),
    method: Method::Unrestricted,
    symmetry: [false; 3],
    settings: Settings { max_iterations: 25, ..Settings::default() },
  }
}

#[test]
fn milling_from_above_leaves_no_undercuts() {
  let mut p = cantilever([16, 6, 6]);
  p.method = Method::Milling3 { directions: vec![Dir::parse("+z").unwrap()] };
  let o = solve(&p, &progress()).unwrap();
  let g = p.grid;
  for i in 0..g.n[0] {
    for j in 0..g.n[1] {
      for k in 0..g.n[2] - 1 {
        // never more material above than below
        assert!(o.density[g.elem(i, j, k)] + 1e-6 >= o.density[g.elem(i, j, k + 1)]);
      }
    }
  }
  let mut p = cantilever([16, 6, 6]);
  p.method = Method::Milling25 { direction: Dir::parse("-y").unwrap() };
  let o = solve(&p, &progress()).unwrap();
  for i in 0..g.n[0] {
    for k in 0..g.n[2] {
      let col: Vec<f32> = (0..g.n[1]).map(|j| o.density[g.elem(i, j, k)]).collect();
      assert!(col.iter().all(|v| (v - col[0]).abs() < 1e-6), "{col:?}");
    }
  }
}

#[test]
fn symmetry_plane_mirrors_the_design() {
  let mut p = cantilever([14, 7, 6]);
  p.symmetry = [false, true, false];
  let o = solve(&p, &progress()).unwrap();
  let g = p.grid;
  for e in 0..g.elements() {
    let [i, j, k] = g.elem_ijk(e);
    let m = g.elem(i, g.n[1] - 1 - j, k);
    assert!((o.density[e] - o.density[m]).abs() < 1e-3);
  }
}

#[test]
fn preserve_stays_solid_obstacles_stay_empty() {
  let mut p = cantilever([16, 6, 6]);
  let g = p.grid;
  for k in 0..6 {
    for j in 0..6 {
      p.cells[g.elem(15, j, k)] = Cell::Preserve;
      p.cells[g.elem(8, j, k)] = if (2..4).contains(&j) && (2..4).contains(&k) { Cell::Void } else { Cell::Design };
    }
  }
  let o = solve(&p, &progress()).unwrap();
  for (e, c) in p.cells.iter().enumerate() {
    match c {
      Cell::Preserve => assert_eq!(o.density[e], 1.0),
      Cell::Void => assert_eq!(o.density[e], 0.0),
      Cell::Design => {}
    }
  }
  assert!(o.mass > 0.0 && o.safety_factor > 0.0 && o.max_displacement > 0.0);
}

#[test]
fn minimise_mass_meets_the_safety_factor() {
  let mut p = cantilever([16, 6, 6]);
  p.objective = Objective::MinMass { start: 0.5 };
  p.limits = Limits { safety_factor: 2.0, max_displacement: None };
  p.settings.max_iterations = 20;
  let o = solve(&p, &progress()).unwrap();
  assert!(o.meets_limits, "{o:?}");
  assert!(o.safety_factor >= 2.0);
  // much lighter than the half-full start
  assert!(o.volume_fraction < 0.3, "{}", o.volume_fraction);
}

#[test]
fn several_load_cases_and_cancel() {
  let mut p = cantilever([12, 6, 6]);
  let mut up = p.cases[0].clone();
  up.name = "side".into();
  for f in &mut up.forces {
    f.1 = [0.0, 3.0, 0.0];
  }
  p.cases.push(up);
  let o = solve(&p, &progress()).unwrap();
  assert!(o.compliance > 0.0);
  let pr = progress();
  pr.cancel.store(true, Ordering::Relaxed);
  let o = solve(&p, &pr).unwrap();
  assert_eq!(o.iterations, 0);
}

#[test]
fn bad_problems_are_refused() {
  let mut p = cantilever([4, 4, 4]);
  p.cases[0].fixed.clear();
  assert!(solve(&p, &progress()).is_err());
  let mut p = cantilever([4, 4, 4]);
  p.cells.fill(Cell::Void);
  assert!(solve(&p, &progress()).is_err());
}

fn cube_mesh(lo: [f32; 3], hi: [f32; 3]) -> (Vec<[f32; 3]>, Vec<u32>) {
  let p = (0..8).map(|c| [0, 1, 2].map(|a| if c >> a & 1 == 1 { hi[a] } else { lo[a] })).collect();
  // outward quads as corner bit patterns
  let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
  let t = quads.iter().flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]]).collect();
  (p, t)
}

#[test]
fn voxelising_a_closed_mesh() {
  let g = Grid::around([0.0; 3], [10.0, 10.0, 10.0], Some(1.0), 0, 1);
  assert_eq!(g.n, [12, 12, 12]);
  let (p, t) = cube_mesh([2.0, 2.0, 2.0], [6.0, 7.0, 8.0]);
  let inside = g.inside(&p, &t);
  assert_eq!(inside.iter().filter(|v| **v).count(), 4 * 5 * 6);
  let near = g.nodes_near(&p, &t[..6], 0.01);
  // the z = 2 face: 5 x 6 nodes
  assert_eq!(near.len(), 30);
}

#[test]
fn surface_of_a_block_is_closed_with_its_volume() {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [10, 8, 6] };
  let mut d = vec![0.0f32; g.elements()];
  for e in 0..g.elements() {
    let [i, j, k] = g.elem_ijk(e);
    if (2..8).contains(&i) && (2..6).contains(&j) && k < 6 {
      d[e] = 1.0;
    }
  }
  let m = extract(&g, &d, 0.47, 0);
  assert_eq!(m.open_edges(), 0);
  // the iso-surface cuts the block's edges and corners off a little
  let v = m.volume();
  assert!(v > 0.85 * 144.0 && v < 1.05 * 144.0, "volume {v}");
  let s = extract(&g, &d, 0.47, 10);
  assert_eq!(s.open_edges(), 0);
  assert!((s.volume() - v).abs() < 0.1 * v, "smoothing keeps the volume: {} vs {v}", s.volume());
}

#[test]
fn outcome_surface_of_the_mbb_beam() {
  let p = mbb(30, 10, 0.5);
  let o = solve(&p, &progress()).unwrap();
  let m = extract(&p.grid, &o.density, 0.47, 8);
  assert_eq!(m.open_edges(), 0);
  assert!(m.triangles() > 100);
  assert!(m.volume() > 0.0);
}

#[test]
fn material_library_lookup() {
  assert_eq!(material("titanium").unwrap().name, "Titanium 6Al-4V");
  assert_eq!(material("aluminum 6061").unwrap().youngs, 68_900.0);
  assert!(material("aluminum").is_none(), "ambiguous");
  assert_eq!(material("steel").unwrap().name, "Steel AISI 1020");
  assert_eq!(material("316").unwrap().name, "Stainless Steel 316L");
  assert!(material("unobtainium").is_none());
  let j = serde_json::to_string(&materials()[0]).unwrap();
  assert!(j.contains("\"yield\""));
}

#[test]
#[ignore]
fn print_mbb() {
  let p = mbb(60, 20, 0.5);
  let o = solve(&p, &progress()).unwrap();
  for j in (0..20).rev() {
    let row: String = (0..60).map(|i| if at(&p.grid, &o.density, i, j) > 0.5 { '#' } else { '.' }).collect();
    println!("{row}");
  }
  println!("{} iterations, vf {}, c {}", o.iterations, o.volume_fraction, o.compliance);
  println!("{:?}", o.history.iter().map(|h| h.0 as i64).collect::<Vec<_>>());
}

#[test]
#[ignore]
fn bench_cantilever() {
  let mut p = cantilever([60, 30, 10]);
  p.settings.max_iterations = 60;
  let t = std::time::Instant::now();
  let o = solve(&p, &progress()).unwrap();
  println!("{} iterations in {:?}, vf {}, c {}", o.iterations, t.elapsed(), o.volume_fraction, o.compliance);
}

#[test]
#[ignore]
fn bench_apply() {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [60, 30, 10] };
  let ke = element_stiffness(1.0, 0.3);
  let e = vec![1.0; g.elements()];
  let fixed = vec![false; 3 * g.nodes()];
  for threads in [1, 4] {
    let sys = System { grid: &g, ke: &ke, e: &e, fixed: &fixed, threads };
    let x = vec![1.0; 3 * g.nodes()];
    let mut y = vec![0.0; x.len()];
    let t = std::time::Instant::now();
    for _ in 0..50 {
      sys.apply(&x, &mut y);
    }
    println!("{threads} threads: {:?} per apply", t.elapsed() / 50);
  }
}

#[test]
#[ignore]
fn bench_vcycle() {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [60, 30, 10] };
  let ke = element_stiffness(1.0, 0.3);
  let e: Vec<f64> = (0..g.elements()).map(|i| if i % 3 == 0 { 1.0 } else { 0.01 }).collect();
  let mut fixed = vec![false; 3 * g.nodes()];
  for n in 0..g.nodes() {
    if g.node_ijk(n)[0] == 0 {
      fixed[3 * n..3 * n + 3].fill(true);
    }
  }
  let sys = System { grid: &g, ke: &ke, e: &e, fixed: &fixed, threads: 4 };
  let t = std::time::Instant::now();
  let mg = crate::mg::Multigrid::new(&sys);
  println!("setup {:?}", t.elapsed());
  let r = vec![1.0; 3 * g.nodes()];
  let mut z = vec![0.0; r.len()];
  let t = std::time::Instant::now();
  for _ in 0..10 {
    mg.precondition(&r, &mut z);
  }
  println!("v-cycle {:?}", t.elapsed() / 10);
}

#[test]
fn decimation_keeps_the_mesh_closed_and_its_volume() {
  let p = mbb(30, 10, 0.5);
  let o = solve(&p, &progress()).unwrap();
  let m = surface::drop_islands(&extract(&p.grid, &o.density, 0.47, 8), 0.01);
  let d = surface::decimate(&m, m.triangles() / 4);
  assert!(d.triangles() <= m.triangles() / 4 + 2, "{} -> {}", m.triangles(), d.triangles());
  assert_eq!(d.open_edges(), 0);
  assert!((d.volume() - m.volume()).abs() < 0.03 * m.volume(), "{} vs {}", d.volume(), m.volume());
}

#[test]
fn islands_are_dropped() {
  let g = Grid { origin: [0.0; 3], h: 1.0, n: [12, 6, 6] };
  let mut d = vec![0.0f32; g.elements()];
  for e in 0..g.elements() {
    let [i, j, k] = g.elem_ijk(e);
    let big = (1..7).contains(&i) && (1..5).contains(&j) && (1..5).contains(&k);
    let small = (9..11).contains(&i) && (2..4).contains(&j) && (2..4).contains(&k);
    if big || small {
      d[e] = 1.0;
    }
  }
  let m = extract(&g, &d, 0.47, 0);
  let kept = surface::drop_islands(&m, 0.2);
  assert!(kept.triangles() < m.triangles());
  assert_eq!(kept.open_edges(), 0);
  let (_, hi) = kept.bounds();
  assert!(hi[0] < 8.0);
}
