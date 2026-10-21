//! The SIMP loop: design variables -> density filter -> Heaviside projection -> manufacturing
//! constraints -> physical densities -> finite elements -> sensitivities back along the same
//! chain -> optimality-criteria update.

use std::sync::atomic::Ordering;

use crate::fe::{System, element_energies, element_stiffness, von_mises};
use crate::{Cell, GenError, Grid, Method, Objective, Outcome, Problem, Progress};

/// Soft material, as a fraction of the solid's modulus.
const E_MIN: f64 = 1e-6;
/// Heaviside threshold.
const ETA: f64 = 0.5;

/// Runs the optimisation.
pub fn solve(p: &Problem, progress: &Progress) -> Result<Outcome, GenError> {
  check(p)?;
  let mut run = Run::new(p);
  match p.objective {
    Objective::MaxStiffness { volume_fraction } => {
      let vf = volume_fraction.clamp(0.01, 1.0);
      progress.total.store(p.settings.max_iterations, Ordering::Relaxed);
      run.init(vf);
      let (iters, converged) = run.optimise(vf, p.settings.max_iterations, true, progress);
      Ok(run.outcome(iters, converged))
    }
    Objective::MinMass { start } => {
      let mut vf = start.clamp(0.02, 1.0);
      let follow = (p.settings.max_iterations / 4).max(8);
      let stages = (0..).map(|k| start * 0.8f64.powi(k)).take_while(|f| *f >= 0.03).count().max(1);
      progress.total.store(p.settings.max_iterations + follow * (stages - 1), Ordering::Relaxed);
      run.init(vf);
      let (mut iters, mut converged) = run.optimise(vf, p.settings.max_iterations, true, progress);
      let mut best = run.outcome(iters, converged);
      if !best.meets_limits {
        return Ok(best);
      }
      let mut x_best = run.x.clone();
      loop {
        vf *= 0.8;
        if vf < 0.03 || progress.cancelled() {
          break;
        }
        let (i, c) = run.optimise(vf, follow, false, progress);
        iters += i;
        converged = c;
        let o = run.outcome(iters, converged);
        if !o.meets_limits {
          break;
        }
        best = o;
        x_best.clone_from(&run.x);
      }
      run.x = x_best;
      best.iterations = iters;
      Ok(best)
    }
  }
}

fn check(p: &Problem) -> Result<(), GenError> {
  let g = &p.grid;
  if g.elements() == 0 || g.h.is_nan() || g.h <= 0.0 {
    return Err(GenError("the grid is empty".into()));
  }
  if p.cells.len() != g.elements() {
    return Err(GenError("one cell type per element is needed".into()));
  }
  if !p.cells.contains(&Cell::Design) {
    return Err(GenError("nothing is left to optimise: the design space is empty".into()));
  }
  if p.cases.is_empty() {
    return Err(GenError("a load case is needed".into()));
  }
  let nn = g.nodes();
  for c in &p.cases {
    if c.fixed.is_empty() {
      return Err(GenError(format!("load case '{}' has no fixed constraint", c.name)));
    }
    if c.forces.iter().all(|(_, f)| f.iter().all(|v| *v == 0.0)) {
      return Err(GenError(format!("load case '{}' has no load", c.name)));
    }
    if c.forces.iter().any(|(n, _)| *n >= nn) || c.fixed.iter().any(|(n, _)| *n >= nn) {
      return Err(GenError("a load or constraint is off the grid".into()));
    }
  }
  if p.material.youngs.is_nan() || p.material.youngs <= 0.0 || !(-1.0..0.5).contains(&p.material.poisson) {
    return Err(GenError(format!("material '{}' needs E > 0 and Poisson's ratio below 0.5", p.material.name)));
  }
  Ok(())
}

/// Neighbours of the density filter: offsets and cone weights.
struct Filter {
  offsets: Vec<([isize; 3], f64)>,
  sums: Vec<f64>,
}

struct Run<'a> {
  p: &'a Problem,
  g: Grid,
  ke: Vec<f64>,
  filter: Filter,
  design: Vec<bool>,
  n_design: f64,
  x: Vec<f64>,
  beta: f64,
  /// Displacements per case (warm starts for the solver).
  u: Vec<Vec<f64>>,
  forces: Vec<Vec<f64>>,
  fixed: Vec<Vec<bool>>,
  threads: usize,
  history: Vec<(f64, f64)>,
}

impl<'a> Run<'a> {
  fn new(p: &'a Problem) -> Run<'a> {
    let g = p.grid;
    let threads = match p.settings.threads {
      0 => std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
      t => t,
    };
    let r = p.settings.filter_radius.max(1.5 * g.h);
    let reach = (r / g.h).ceil() as isize;
    let mut offsets = Vec::new();
    for k in -reach..=reach {
      for j in -reach..=reach {
        for i in -reach..=reach {
          let d = ((i * i + j * j + k * k) as f64).sqrt() * g.h;
          if d < r {
            offsets.push(([i, j, k], r - d));
          }
        }
      }
    }
    let mut filter = Filter { offsets, sums: Vec::new() };
    filter.sums = (0..g.elements()).map(|e| neighbours(&g, &filter, e).map(|(_, w)| w).sum()).collect();
    let nd = 3 * g.nodes();
    let forces = p
      .cases
      .iter()
      .map(|c| {
        let mut f = vec![0.0; nd];
        for (n, v) in &c.forces {
          for a in 0..3 {
            f[3 * n + a] += v[a];
          }
        }
        f
      })
      .collect();
    let fixed = p
      .cases
      .iter()
      .map(|c| {
        let mut f = vec![false; nd];
        for (n, axes) in &c.fixed {
          for a in 0..3 {
            f[3 * n + a] |= axes[a];
          }
        }
        f
      })
      .collect();
    let design: Vec<bool> = p.cells.iter().map(|c| *c == Cell::Design).collect();
    Run {
      p,
      g,
      ke: element_stiffness(g.h, p.material.poisson),
      filter,
      n_design: design.iter().filter(|d| **d).count() as f64,
      design,
      x: Vec::new(),
      beta: 1.0,
      u: vec![vec![0.0; nd]; p.cases.len()],
      forces,
      fixed,
      threads,
      history: Vec::new(),
    }
  }

  fn init(&mut self, vf: f64) {
    self.x = (0..self.g.elements())
      .map(|e| match self.p.cells[e] {
        Cell::Preserve => 1.0,
        Cell::Void => 0.0,
        Cell::Design => match &self.p.start {
          Some(s) => (s.get(e).copied().unwrap_or(0.0) as f64).clamp(0.05, 1.0),
          None => vf,
        },
      })
      .collect();
    if self.p.start.is_some() {
      self.symmetrise(true);
    }
  }

  /// Averages mirrored design variables (`values`) or sensitivities.
  fn symmetrise(&mut self, values: bool) {
    if values {
      let mut x = std::mem::take(&mut self.x);
      self.mirror_avg(&mut x);
      self.x = x;
    }
  }

  fn mirror_avg(&self, v: &mut [f64]) {
    let g = self.g;
    for axis in 0..3 {
      if !self.p.symmetry[axis] {
        continue;
      }
      for e in 0..g.elements() {
        let mut ijk = g.elem_ijk(e);
        ijk[axis] = g.n[axis] - 1 - ijk[axis];
        let m = g.elem(ijk[0], ijk[1], ijk[2]);
        if m > e {
          let a = 0.5 * (v[e] + v[m]);
          v[e] = a;
          v[m] = a;
        }
      }
    }
  }

  /// Physical densities of design variables `x`, with what the chain rule needs: the filtered
  /// field and, per element, the element whose projected density it took (milling).
  fn physical(&self, x: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<usize>) {
    let g = &self.g;
    let xt: Vec<f64> = (0..g.elements())
      .map(|e| neighbours(g, &self.filter, e).map(|(n, w)| w * x[n]).sum::<f64>() / self.filter.sums[e])
      .collect();
    let mut xb: Vec<f64> = xt.iter().map(|v| heaviside(*v, self.beta)).collect();
    for (e, c) in self.p.cells.iter().enumerate() {
      match c {
        Cell::Preserve => xb[e] = 1.0,
        Cell::Void => xb[e] = 0.0,
        Cell::Design => {}
      }
    }
    let (mut rho, src) = mill(g, &xb, &self.p.method);
    for (e, c) in self.p.cells.iter().enumerate() {
      match c {
        Cell::Preserve => rho[e] = 1.0,
        Cell::Void => rho[e] = 0.0,
        Cell::Design => {}
      }
    }
    (rho, xt, src)
  }

  fn volume(&self, rho: &[f64]) -> f64 {
    rho.iter().zip(&self.design).filter(|(_, d)| **d).map(|(r, _)| r).sum::<f64>() / self.n_design
  }

  /// Back from d/d(physical density) to d/d(design variable).
  fn chain(&self, d_rho: &[f64], xt: &[f64], src: &[usize]) -> Vec<f64> {
    let g = &self.g;
    let mut d_xb = vec![0.0; g.elements()];
    for e in 0..g.elements() {
      if self.design[e] {
        d_xb[src[e]] += d_rho[e];
      }
    }
    for e in 0..g.elements() {
      d_xb[e] = if self.design[e] { d_xb[e] * heaviside_d(xt[e], self.beta) / self.filter.sums[e] } else { 0.0 };
    }
    let mut d_x: Vec<f64> = (0..g.elements())
      .map(|j| if self.design[j] { neighbours(g, &self.filter, j).map(|(n, w)| w * d_xb[n]).sum() } else { 0.0 })
      .collect();
    self.mirror_avg(&mut d_x);
    d_x
  }

  /// Young's moduli of densities.
  fn moduli(&self, rho: &[f64], penalty: f64) -> Vec<f64> {
    let e0 = self.p.material.youngs;
    rho.iter().map(|r| e0 * (E_MIN + r.powf(penalty) * (1.0 - E_MIN))).collect()
  }

  /// Solves every load case; returns the compliance and the summed element energies.
  fn analyse(&mut self, e: &[f64], tol: f64) -> (f64, Vec<f64>) {
    let mut c = 0.0;
    let mut energy = vec![0.0; self.g.elements()];
    for k in 0..self.p.cases.len() {
      let sys = System { grid: &self.g, ke: &self.ke, e, fixed: &self.fixed[k], threads: self.threads };
      let mut u = std::mem::take(&mut self.u[k]);
      sys.solve(&self.forces[k], &mut u, tol, 20_000);
      c += self.forces[k].iter().zip(&u).map(|(f, u)| f * u).sum::<f64>();
      for (a, b) in energy.iter_mut().zip(element_energies(&self.g, &self.ke, &u, self.threads)) {
        *a += b;
      }
      self.u[k] = u;
    }
    (c, energy)
  }

  /// Optimisation iterations at a volume target. `continuation` raises the projection sharpness
  /// on the way; otherwise it stays where it is. Returns (iterations, converged).
  fn optimise(&mut self, vf: f64, iterations: usize, continuation: bool, progress: &Progress) -> (usize, bool) {
    let s = self.p.settings;
    let beta_max = s.beta_max.max(1.0);
    let steps = beta_max.log2().ceil().max(0.0) as usize;
    let stage = if continuation { (iterations / (steps + 2)).max(1) } else { usize::MAX };
    if continuation {
      self.beta = 1.0;
    }
    let mut since = 0;
    for it in 0..iterations {
      if progress.cancelled() {
        return (it, false);
      }
      let (rho, xt, src) = self.physical(&self.x);
      let e = self.moduli(&rho, s.penalty);
      let (c, energy) = self.analyse(&e, 1e-4);
      let e0 = self.p.material.youngs;
      let dc: Vec<f64> =
        rho.iter().zip(&energy).map(|(r, en)| -s.penalty * r.powf(s.penalty - 1.0) * e0 * (1.0 - E_MIN) * en).collect();
      let dc = self.chain(&dc, &xt, &src);
      let dv = self.chain(&vec![1.0; rho.len()], &xt, &src);
      self.history.push((c, self.volume(&rho)));
      let change = self.update(&dc, &dv, vf, s.move_limit);
      progress.iteration.fetch_add(1, Ordering::Relaxed);
      since += 1;
      let sharp = self.beta >= beta_max;
      if sharp && change < 0.01 && it > 4 {
        return (it + 1, true);
      }
      if !sharp && (since >= stage || change < 0.005) {
        self.beta = (self.beta * 2.0).min(beta_max);
        since = 0;
      }
    }
    (iterations, false)
  }

  /// Optimality-criteria update with a bisection on the volume constraint. Returns the largest
  /// change of a design variable.
  fn update(&mut self, dc: &[f64], dv: &[f64], vf: f64, mv: f64) -> f64 {
    let scale = dc.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
    let (mut l1, mut l2) = (0.0, 1e9);
    let mut xnew = self.x.clone();
    for _ in 0..200 {
      if (l2 - l1) / (l1 + l2) <= 1e-4 {
        break;
      }
      let lmid = 0.5 * (l1 + l2);
      for e in 0..xnew.len() {
        if !self.design[e] {
          continue;
        }
        let b = (-dc[e] / scale).max(1e-30) / (dv[e].max(1e-12) * lmid);
        let x = self.x[e];
        xnew[e] = (x * b.sqrt()).clamp((x - mv).max(0.0), (x + mv).min(1.0));
      }
      let (rho, _, _) = self.physical(&xnew);
      if self.volume(&rho) > vf {
        l1 = lmid;
      } else {
        l2 = lmid;
      }
    }
    let change = xnew.iter().zip(&self.x).fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    self.x = xnew;
    change
  }

  /// The crisp design (densities over 0.5 solid) analysed with the full material.
  fn outcome(&mut self, iterations: usize, converged: bool) -> Outcome {
    let (rho, _, _) = self.physical(&self.x);
    let solid: Vec<f64> = rho.iter().map(|r| if *r > 0.5 { 1.0 } else { 0.0 }).collect();
    let e = self.moduli(&solid, 1.0);
    let (compliance, _) = self.analyse(&e, 1e-7);
    let g = self.g;
    // nodes on solid elements: the rest float in the soft material
    let mut on_solid = vec![false; g.nodes()];
    for el in 0..g.elements() {
      if solid[el] > 0.5 {
        for n in g.elem_nodes(el) {
          on_solid[n] = true;
        }
      }
    }
    let mut max_disp = 0.0f64;
    let mut peak = 0.0f64;
    for u in &self.u {
      for (n, s) in on_solid.iter().enumerate() {
        if *s {
          max_disp = max_disp.max((u[3 * n].powi(2) + u[3 * n + 1].powi(2) + u[3 * n + 2].powi(2)).sqrt());
        }
      }
      let vm = von_mises(&g, u, self.p.material.youngs, self.p.material.poisson);
      let mut st: Vec<f64> = vm.iter().zip(&solid).filter(|(_, s)| **s > 0.5).map(|(v, _)| *v).collect();
      if !st.is_empty() {
        st.sort_by(f64::total_cmp);
        let i = ((st.len() as f64 * 0.995).floor() as usize).min(st.len() - 1);
        peak = peak.max(st[i]);
      }
    }
    let safety_factor = if peak > 0.0 { self.p.material.yield_strength / peak } else { f64::INFINITY };
    let volume = solid.iter().sum::<f64>() * g.h.powi(3);
    let lim = self.p.limits;
    let meets_limits = safety_factor >= lim.safety_factor && lim.max_displacement.is_none_or(|d| max_disp <= d);
    Outcome {
      density: rho.iter().map(|r| *r as f32).collect(),
      volume_fraction: self.volume(&solid),
      volume,
      mass: volume * self.p.material.density / 1000.0,
      compliance,
      max_displacement: max_disp,
      peak_stress: peak,
      safety_factor: safety_factor.min(1e9),
      meets_limits,
      iterations,
      converged,
      history: self.history.clone(),
    }
  }
}

fn neighbours<'f>(g: &Grid, f: &'f Filter, e: usize) -> impl Iterator<Item = (usize, f64)> + 'f {
  let ijk = g.elem_ijk(e);
  let (n, g) = (g.n, *g);
  f.offsets.iter().filter_map(move |(o, w)| {
    let i = ijk[0] as isize + o[0];
    let j = ijk[1] as isize + o[1];
    let k = ijk[2] as isize + o[2];
    (i >= 0 && j >= 0 && k >= 0 && (i as usize) < n[0] && (j as usize) < n[1] && (k as usize) < n[2])
      .then(|| (g.elem(i as usize, j as usize, k as usize), *w))
  })
}

fn heaviside(x: f64, beta: f64) -> f64 {
  if beta <= 1.0 {
    return x;
  }
  let t = (beta * ETA).tanh();
  (t + (beta * (x - ETA)).tanh()) / (t + (beta * (1.0 - ETA)).tanh())
}

fn heaviside_d(x: f64, beta: f64) -> f64 {
  if beta <= 1.0 {
    return 1.0;
  }
  let t = (beta * ETA).tanh();
  beta * (1.0 - (beta * (x - ETA)).tanh().powi(2)) / (t + (beta * (1.0 - ETA)).tanh())
}

/// Densities a milling method can make from `xb`, and the element each one was taken from.
fn mill(g: &Grid, xb: &[f64], method: &Method) -> (Vec<f64>, Vec<usize>) {
  let ne = g.elements();
  let ident = || (xb.to_vec(), (0..ne).collect::<Vec<_>>());
  match method {
    Method::Unrestricted => ident(),
    Method::Milling3 { directions } if directions.is_empty() => ident(),
    Method::Milling3 { directions } => {
      let mut best: Option<(Vec<f64>, Vec<usize>)> = None;
      for d in directions {
        let (r, s) = column_max(g, xb, d.axis, d.positive, false);
        best = Some(match best {
          None => (r, s),
          Some((mut br, mut bs)) => {
            for e in 0..ne {
              if r[e] < br[e] {
                br[e] = r[e];
                bs[e] = s[e];
              }
            }
            (br, bs)
          }
        });
      }
      best.unwrap_or_else(ident)
    }
    Method::Milling25 { direction } => column_max(g, xb, direction.axis, direction.positive, true),
  }
}

/// Running maximum along `axis` coming from the `positive` (or negative) end; `whole` takes the
/// whole column's maximum instead.
fn column_max(g: &Grid, xb: &[f64], axis: usize, positive: bool, whole: bool) -> (Vec<f64>, Vec<usize>) {
  let ne = g.elements();
  let mut rho = vec![0.0; ne];
  let mut src = vec![0; ne];
  let (a1, a2) = ((axis + 1) % 3, (axis + 2) % 3);
  let len = g.n[axis];
  for v in 0..g.n[a2] {
    for u in 0..g.n[a1] {
      let at = |t: usize| {
        let mut ijk = [0; 3];
        ijk[axis] = if positive { len - 1 - t } else { t };
        ijk[a1] = u;
        ijk[a2] = v;
        g.elem(ijk[0], ijk[1], ijk[2])
      };
      let mut m = (f64::NEG_INFINITY, at(0));
      for t in 0..len {
        let e = at(t);
        if xb[e] > m.0 {
          m = (xb[e], e);
        }
        rho[e] = m.0;
        src[e] = m.1;
      }
      if whole {
        for t in 0..len {
          let e = at(t);
          rho[e] = m.0;
          src[e] = m.1;
        }
      }
    }
  }
  (rho, src)
}
