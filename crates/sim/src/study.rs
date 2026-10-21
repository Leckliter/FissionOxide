//! A simulation model — meshed parts with materials, constraints, loads and bonded contacts — and
//! its three studies: linear static stress, modal frequencies and steady-state heat transfer.
//!
//! Units: mm, N, MPa, tonne, s, W, °C (so conductivity is W/(mm·K), heat flux W/mm², a
//! convection coefficient W/(mm²·K) and gravity mm/s²). [`Material`] keeps the usual units and
//! converts.
//!
//! Parts are meshed separately; their nodes are numbered one after another ([`Model::offsets`]),
//! and every per-node result is in that global numbering. Faces are picked by part and CAD face
//! id ([`FaceSel`]). Fixed constraints remove degrees of freedom; pin and frictionless
//! constraints (along directions that change from node to node) and bonded contacts (each node of
//! one part's contact surface tied to the other part's surface) are imposed with stiff penalty
//! springs.

use serde::{Deserialize, Serialize};

use crate::fem::{self, Geo};
use crate::geom::*;
use crate::linalg::{self, SymMatrix};
use crate::material::Material;
use crate::mesh::TetMesh;

/// A CAD face of one part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FaceSel {
  pub part: usize,
  pub face: u32,
}

fn yes() -> bool {
  true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
  /// Held in place (in the chosen directions).
  Fixed {
    faces: Vec<FaceSel>,
    #[serde(default = "yes")]
    x: bool,
    #[serde(default = "yes")]
    y: bool,
    #[serde(default = "yes")]
    z: bool,
  },
  /// A cylindrical face on a pin: held radially and/or axially and/or tangentially (turning
  /// about the axis). The axis is fitted to the faces unless given.
  Pin {
    faces: Vec<FaceSel>,
    #[serde(default = "yes")]
    radial: bool,
    #[serde(default = "yes")]
    axial: bool,
    #[serde(default)]
    tangential: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    axis: Option<(V3, V3)>,
  },
  /// Can't move along its normal, slides freely in its plane.
  Frictionless { faces: Vec<FaceSel> },
}

impl Constraint {
  pub fn faces(&self) -> &[FaceSel] {
    match self {
      Constraint::Fixed { faces, .. } | Constraint::Pin { faces, .. } | Constraint::Frictionless { faces } => faces,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Load {
  /// A total force (N) spread evenly over the faces' area.
  Force { faces: Vec<FaceSel>, force: V3 },
  /// A pressure (MPa) pushing into the faces (negative pulls).
  Pressure { faces: Vec<FaceSel>, pressure: f64 },
  /// A bearing load (N): a shaft or pin pushing on a cylindrical face along the force, spread
  /// over the half facing it (most at the middle, as cos θ).
  Bearing { faces: Vec<FaceSel>, force: V3 },
  /// A moment (N·mm) about the faces' centroid, spread as a twist.
  Moment { faces: Vec<FaceSel>, moment: V3 },
  /// Gravity or another uniform acceleration (mm/s²) on every part.
  Gravity { acceleration: V3 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThermalLoad {
  /// Faces held at a temperature (°C).
  Temperature { faces: Vec<FaceSel>, temperature: f64 },
  /// Heat flux into the faces (W/mm²).
  HeatFlux { faces: Vec<FaceSel>, flux: f64 },
  /// A total heat input (W) spread over the faces.
  HeatPower { faces: Vec<FaceSel>, power: f64 },
  /// Convection to a fluid at `ambient` °C with a film coefficient (W/(mm²·K)).
  Convection { faces: Vec<FaceSel>, coefficient: f64, ambient: f64 },
  /// Heat generated inside a part (W in total).
  InternalHeat { part: usize, power: f64 },
}

impl ThermalLoad {
  pub fn faces(&self) -> &[FaceSel] {
    match self {
      ThermalLoad::Temperature { faces, .. }
      | ThermalLoad::HeatFlux { faces, .. }
      | ThermalLoad::HeatPower { faces, .. }
      | ThermalLoad::Convection { faces, .. } => faces,
      ThermalLoad::InternalHeat { .. } => &[],
    }
  }
}

/// How parts are joined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Contacts {
  /// Bond every pair of parts whose surfaces touch (within the tolerance, mm; `None` = a tenth
  /// of the smaller mesh size).
  #[default]
  Automatic,
  /// Bond only these pairs.
  Bonded { pairs: Vec<(usize, usize)>, tolerance: Option<f64> },
  /// Parts don't interact.
  None,
}

#[derive(Debug, Clone)]
pub struct Part {
  pub name: String,
  pub mesh: TetMesh,
  pub material: Material,
  /// The mesh size it was made with (for contact tolerances).
  pub size: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
  pub parts: Vec<Part>,
  pub constraints: Vec<Constraint>,
  pub loads: Vec<Load>,
  pub thermal: Vec<ThermalLoad>,
  pub contacts: Contacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolveOptions {
  /// Modes to find.
  pub modes: usize,
  /// Unknowns up to which the sparse direct solver is used (conjugate gradients above).
  pub direct_limit: usize,
}

impl Default for SolveOptions {
  fn default() -> Self {
    SolveOptions { modes: 8, direct_limit: 400_000 }
  }
}

/// One bonded tie: a node held to a point of another part's boundary triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tie {
  pub node: u32,
  pub master: [u32; 6],
  pub weights: [f64; 6],
}

impl Model {
  /// Where each part's nodes start in the global numbering (one more entry: the total).
  pub fn offsets(&self) -> Vec<usize> {
    let mut o = vec![0];
    for p in &self.parts {
      o.push(o.last().unwrap_or(&0) + p.mesh.nodes.len());
    }
    o
  }

  pub fn node_count(&self) -> usize {
    self.parts.iter().map(|p| p.mesh.nodes.len()).sum()
  }

  pub fn element_count(&self) -> usize {
    self.parts.iter().map(|p| p.mesh.tets.len()).sum()
  }

  /// Every node's position, globally numbered.
  pub fn nodes(&self) -> Vec<V3> {
    self.parts.iter().flat_map(|p| p.mesh.nodes.iter().copied()).collect()
  }

  /// The part each global node belongs to.
  pub fn node_part(&self) -> Vec<usize> {
    self.parts.iter().enumerate().flat_map(|(i, p)| std::iter::repeat_n(i, p.mesh.nodes.len())).collect()
  }

  pub fn check(&self) -> Result<(), String> {
    if self.parts.is_empty() {
      return Err("there is nothing to simulate: the study has no bodies".into());
    }
    for p in &self.parts {
      p.material.validate()?;
      if p.mesh.tets.is_empty() {
        return Err(format!("{} has no mesh", p.name));
      }
    }
    let faces = |sel: &[FaceSel]| -> Result<(), String> {
      for f in sel {
        let Some(p) = self.parts.get(f.part) else { return Err(format!("no part {}", f.part)) };
        if !p.mesh.boundary_face.contains(&f.face) {
          return Err(format!("{} has no face {}", p.name, f.face));
        }
      }
      Ok(())
    };
    for c in &self.constraints {
      faces(c.faces())?;
    }
    for l in &self.loads {
      match l {
        Load::Force { faces: f, .. }
        | Load::Pressure { faces: f, .. }
        | Load::Bearing { faces: f, .. }
        | Load::Moment { faces: f, .. } => faces(f)?,
        Load::Gravity { .. } => {}
      }
    }
    for t in &self.thermal {
      faces(t.faces())?;
    }
    Ok(())
  }

  /// The boundary triangles (part, triangle) on faces.
  fn tris_of(&self, sel: &[FaceSel]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (pi, p) in self.parts.iter().enumerate() {
      let faces: Vec<u32> = sel.iter().filter(|f| f.part == pi).map(|f| f.face).collect();
      if faces.is_empty() {
        continue;
      }
      out.extend(p.mesh.face_tris(&faces).into_iter().map(|t| (pi, t)));
    }
    out
  }

  /// Global nodes on faces.
  fn nodes_of(&self, sel: &[FaceSel]) -> Vec<u32> {
    let off = self.offsets();
    let mut out: Vec<u32> = self
      .tris_of(sel)
      .into_iter()
      .flat_map(|(p, t)| self.parts[p].mesh.boundary[t].map(|n| n + off[p] as u32))
      .collect();
    out.sort();
    out.dedup();
    out
  }

  /// A boundary triangle's corner positions and global node numbers.
  fn tri(&self, off: &[usize], p: usize, t: usize) -> ([V3; 3], [u32; 6]) {
    let m = &self.parts[p].mesh;
    let b = m.boundary[t];
    ([0, 1, 2].map(|k| m.nodes[b[k] as usize]), b.map(|n| n + off[p] as u32))
  }

  /// Bonded ties between touching parts.
  pub fn ties(&self) -> Vec<Tie> {
    let pairs: Vec<(usize, usize, Option<f64>)> = match &self.contacts {
      Contacts::None => Vec::new(),
      Contacts::Bonded { pairs, tolerance } => pairs
        .iter()
        .filter(|(a, b)| a != b && *a < self.parts.len() && *b < self.parts.len())
        .map(|&(a, b)| (a, b, *tolerance))
        .collect(),
      Contacts::Automatic => {
        let mut v = Vec::new();
        for a in 0..self.parts.len() {
          for b in a + 1..self.parts.len() {
            v.push((a, b, None));
          }
        }
        v
      }
    };
    let off = self.offsets();
    let mut out = Vec::new();
    for (a, b, tol) in pairs {
      let tol = tol.unwrap_or(0.1 * self.parts[a].size.min(self.parts[b].size)).max(1e-9);
      let (la, ha) = bbox(&self.parts[a].mesh.nodes);
      let (lb, hb) = bbox(&self.parts[b].mesh.nodes);
      if (0..3).any(|k| la[k] > hb[k] + tol || lb[k] > ha[k] + tol) {
        continue;
      }
      let ab = self.tie_side(&off, b, a, tol);
      let ba = self.tie_side(&off, a, b, tol);
      // the side with more nodes in contact is the finer one: tie its nodes to the other
      out.extend(if ab.len() >= ba.len() { ab } else { ba });
    }
    out
  }

  /// Ties of `slave`'s boundary nodes to `master`'s boundary within `tol`.
  fn tie_side(&self, off: &[usize], slave: usize, master: usize, tol: f64) -> Vec<Tie> {
    let ms = &self.parts[master].mesh;
    let centroids: Vec<V3> = ms
      .boundary
      .iter()
      .map(|b| scale(add(add(ms.nodes[b[0] as usize], ms.nodes[b[1] as usize]), ms.nodes[b[2] as usize]), 1.0 / 3.0))
      .collect();
    if centroids.is_empty() {
      return Vec::new();
    }
    let reach = ms
      .boundary
      .iter()
      .map(|b| {
        let c = [0, 1, 2].map(|k| ms.nodes[b[k] as usize]);
        dist(c[0], c[1]).max(dist(c[1], c[2])).max(dist(c[2], c[0]))
      })
      .fold(0.0, f64::max);
    let tree = KdTree::new(&centroids);
    let ss = &self.parts[slave].mesh;
    let mut nodes: Vec<u32> = ss.boundary.iter().flat_map(|b| b.iter().copied()).collect();
    nodes.sort();
    nodes.dedup();
    let mut out = Vec::new();
    for n in nodes {
      let p = ss.nodes[n as usize];
      let mut best: Option<(f64, usize, [f64; 3])> = None;
      for t in tree.within(p, reach + tol) {
        let b = ms.boundary[t];
        let (q, l) = closest_on_triangle(p, ms.nodes[b[0] as usize], ms.nodes[b[1] as usize], ms.nodes[b[2] as usize]);
        let d = dist(p, q);
        if d <= tol && best.is_none_or(|x| d < x.0) {
          best = Some((d, t, l));
        }
      }
      if let Some((_, t, l)) = best {
        let b = ms.boundary[t];
        out.push(Tie {
          node: n + off[slave] as u32,
          master: b.map(|m| m + off[master] as u32),
          weights: fem::tri_shape_values(l),
        });
      }
    }
    out
  }
}

/// The system for `dofs` unknowns per node: the free unknowns' numbering and the matrix pattern.
struct System {
  /// Free unknown of (node, component), or `None` where fixed.
  dof: Vec<Option<u32>>,
  per: usize,
  free: usize,
  pattern: SymMatrix,
}

impl System {
  fn new(model: &Model, per: usize, fixed: &[bool], ties: &[Tie]) -> System {
    let n = model.node_count();
    let mut dof = vec![None; n * per];
    let mut free = 0u32;
    for (i, d) in dof.iter_mut().enumerate() {
      if !fixed[i] {
        *d = Some(free);
        free += 1;
      }
    }
    // node adjacency
    let off = model.offsets();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (pi, p) in model.parts.iter().enumerate() {
      for t in &p.mesh.tets {
        for &a in t {
          let ga = a as usize + off[pi];
          for &b in t {
            adj[ga].push(b + off[pi] as u32);
          }
        }
      }
    }
    for tie in ties {
      let mut group = vec![tie.node];
      group.extend(tie.master);
      for &a in &group {
        for &b in &group {
          adj[a as usize].push(b);
        }
      }
    }
    for a in &mut adj {
      a.sort_unstable();
      a.dedup();
    }
    let mut rows: Vec<Vec<u32>> = vec![Vec::new(); free as usize];
    for (node, a) in adj.iter().enumerate() {
      for c in 0..per {
        let Some(r) = dof[node * per + c] else { continue };
        let row = &mut rows[r as usize];
        for &m in a {
          for c2 in 0..per {
            if let Some(col) = dof[m as usize * per + c2] {
              row.push(col);
            }
          }
        }
      }
    }
    System { dof, per, free: free as usize, pattern: SymMatrix::with_pattern(rows) }
  }

  /// Adds an element block: `k[a*per+i][b*per+j]` for its nodes; fixed columns with known values
  /// go to the right-hand side.
  fn add(&self, m: &mut SymMatrix, rhs: Option<(&mut [f64], &[f64])>, nodes: &[u32], k: &dyn Fn(usize, usize) -> f64) {
    let per = self.per;
    let mut rhs = rhs;
    for (a, &na) in nodes.iter().enumerate() {
      for i in 0..per {
        let Some(r) = self.dof[na as usize * per + i] else { continue };
        for (b, &nb) in nodes.iter().enumerate() {
          for j in 0..per {
            let v = k(a * per + i, b * per + j);
            if v == 0.0 {
              continue;
            }
            match self.dof[nb as usize * per + j] {
              Some(c) => m.add(r as usize, c as usize, v),
              None => {
                if let Some((f, known)) = rhs.as_mut() {
                  f[r as usize] -= v * known[nb as usize * per + j];
                }
              }
            }
          }
        }
      }
    }
  }

  fn expand(&self, x: &[f64], known: &[f64]) -> Vec<f64> {
    self.dof.iter().enumerate().map(|(i, d)| d.map(|d| x[d as usize]).unwrap_or(known[i])).collect()
  }

  fn restrict(&self, full: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; self.free];
    for (i, d) in self.dof.iter().enumerate() {
      if let Some(d) = d {
        out[*d as usize] += full[i];
      }
    }
    out
  }
}

/// Per-node penalty directions: the 3 × 3 matrix `Σ d dᵀ` to add, scaled.
type Penalties = Map<u32, [[f64; 3]; 3]>;

fn add_dir(p: &mut Penalties, node: u32, d: V3) {
  let e = p.entry(node).or_insert([[0.0; 3]; 3]);
  for i in 0..3 {
    for j in 0..3 {
      e[i][j] += d[i] * d[j];
    }
  }
}

/// Fits a cylinder axis (point, unit direction) to boundary triangles' centroids and normals.
pub fn fit_axis(tris: &[(V3, V3, f64)]) -> Option<(V3, V3)> {
  let mut s = [[0.0; 3]; 3];
  for (_, n, a) in tris {
    for i in 0..3 {
      for j in 0..3 {
        s[i][j] += a * n[i] * n[j];
      }
    }
  }
  let (vals, vecs) = sym_eigen3(s);
  if !(vals[0] > 0.0) {
    return None;
  }
  let axis = unit(column(&vecs, 2));
  // in the plane across the axis, every normal line passes through the centre
  let u = unit(if axis[0].abs() < 0.9 { cross(axis, [1.0, 0.0, 0.0]) } else { cross(axis, [0.0, 1.0, 0.0]) });
  let v = cross(axis, u);
  let (mut a11, mut a12, mut a22, mut b1, mut b2) = (0.0, 0.0, 0.0, 0.0, 0.0);
  for (x, n, w) in tris {
    let (xu, xv, nu, nv) = (dot(*x, u), dot(*x, v), dot(*n, u), dot(*n, v));
    // [nv, −nu] · c = xu nv − xv nu
    let (r1, r2, rhs) = (nv, -nu, xu * nv - xv * nu);
    a11 += w * r1 * r1;
    a12 += w * r1 * r2;
    a22 += w * r2 * r2;
    b1 += w * r1 * rhs;
    b2 += w * r2 * rhs;
  }
  let det = a11 * a22 - a12 * a12;
  if det.abs() < 1e-300 {
    return None;
  }
  let cu = (b1 * a22 - b2 * a12) / det;
  let cv = (a11 * b2 - a12 * b1) / det;
  Some((add(scale(u, cu), scale(v, cv)), axis))
}

/// Static results; per-node values are in the model's global numbering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StaticSolution {
  pub displacement: Vec<V3>,
  /// Stress tensors (xx, yy, zz, xy, yz, zx), averaged at the nodes (MPa).
  pub stress: Vec<[f64; 6]>,
  pub von_mises: Vec<f64>,
  /// Yield strength over von Mises stress, capped at 15.
  pub safety_factor: Vec<f64>,
  /// The reaction force on each constraint (N).
  pub reactions: Vec<V3>,
  /// Strain energy of each element (N·mm), parts' elements one after another.
  pub element_energy: Vec<f64>,
  /// fᵀu (N·mm).
  pub compliance: f64,
  /// Total applied load (N).
  pub applied: V3,
}

/// A linear static problem, set up once and solvable repeatedly with element stiffness scale
/// factors (for topology optimization).
pub struct StaticProblem<'a> {
  model: &'a Model,
  sys: System,
  ties: Vec<Tie>,
  penalties: Penalties,
  force: Vec<f64>,
  fixed_by: Vec<Vec<u32>>,
  options: SolveOptions,
}

/// Penalty stiffness relative to the stiffest diagonal entry.
const PENALTY: f64 = 1e6;
/// Bonded ties' stiffness relative to the tied node's own.
const TIE: f64 = 1e4;

impl<'a> StaticProblem<'a> {
  pub fn new(model: &'a Model, options: SolveOptions) -> Result<StaticProblem<'a>, String> {
    model.check()?;
    let n = model.node_count();
    let off = model.offsets();
    let mut fixed = vec![false; n * 3];
    let mut penalties: Penalties = Map::default();
    let mut fixed_by = Vec::new();
    for c in &model.constraints {
      let nodes = model.nodes_of(c.faces());
      fixed_by.push(nodes.clone());
      match c {
        Constraint::Fixed { x, y, z, .. } => {
          for &nd in &nodes {
            for (k, on) in [*x, *y, *z].iter().enumerate() {
              if *on {
                fixed[nd as usize * 3 + k] = true;
              }
            }
          }
        }
        Constraint::Frictionless { faces } => {
          let mut s: Map<u32, ([[f64; 3]; 3], f64)> = Map::default();
          for (p, t) in model.tris_of(faces) {
            let (x, g) = model.tri(&off, p, t);
            let (a, nrm) = fem::tri_geo(x);
            for &nd in &g {
              let e = s.entry(nd).or_insert(([[0.0; 3]; 3], 0.0));
              for i in 0..3 {
                for j in 0..3 {
                  e.0[i][j] += a * nrm[i] * nrm[j];
                }
              }
              e.1 += a;
            }
          }
          for (nd, (m, w)) in s {
            let (vals, vecs) = sym_eigen3(m.map(|r| r.map(|v| v / w)));
            for k in 0..3 {
              if vals[k] > 0.25 {
                add_dir(&mut penalties, nd, column(&vecs, k));
              }
            }
          }
        }
        Constraint::Pin { faces, radial, axial, tangential, axis } => {
          let tris: Vec<(V3, V3, f64)> = model
            .tris_of(faces)
            .into_iter()
            .map(|(p, t)| {
              let (x, _) = model.tri(&off, p, t);
              let (a, nrm) = fem::tri_geo(x);
              (scale(add(add(x[0], x[1]), x[2]), 1.0 / 3.0), nrm, a)
            })
            .collect();
          let (o, d) = axis.or_else(|| fit_axis(&tris)).ok_or("can't find the pin's axis: pick cylindrical faces")?;
          let d = unit(d);
          let all = model.nodes();
          for &nd in &nodes {
            let p = all[nd as usize];
            let r = sub(p, o);
            let radial_dir = unit(sub(r, scale(d, dot(r, d))));
            if *radial {
              add_dir(&mut penalties, nd, radial_dir);
            }
            if *axial {
              add_dir(&mut penalties, nd, d);
            }
            if *tangential {
              add_dir(&mut penalties, nd, unit(cross(d, radial_dir)));
            }
          }
        }
      }
    }
    let ties = model.ties();
    let sys = System::new(model, 3, &fixed, &ties);
    let force = load_vector(model)?;
    Ok(StaticProblem { model, sys, ties, penalties, force, fixed_by, options })
  }

  /// The free unknowns.
  pub fn unknowns(&self) -> usize {
    self.sys.free
  }

  /// The stiffness matrix with penalties, each element's stiffness times its scale.
  fn stiffness(&self, element_scale: Option<&[f64]>) -> SymMatrix {
    let model = self.model;
    let mut k = self.sys.pattern.clone();
    let off = model.offsets();
    let mut e = 0;
    for (pi, p) in model.parts.iter().enumerate() {
      let (lambda, mu) = p.material.lame();
      for t in &p.mesh.tets {
        let s = element_scale.and_then(|s| s.get(e)).copied().unwrap_or(1.0);
        e += 1;
        if s == 0.0 {
          continue;
        }
        let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
        let ke = fem::stiffness(&geo, lambda * s, mu * s);
        let nodes = t.map(|x| x + off[pi] as u32);
        self.sys.add(&mut k, None, &nodes, &|a, b| ke[a][b]);
      }
    }
    let diag = k.diag();
    let kmax = diag.iter().fold(0.0f64, |a, b| a.max(*b)).max(1e-30);
    let alpha = PENALTY * kmax;
    for (&nd, m) in &self.penalties {
      self.sys.add(&mut k, None, &[nd], &|a, b| alpha * m[a][b]);
    }
    for tie in &self.ties {
      let own =
        (0..3).filter_map(|c| self.sys.dof[tie.node as usize * 3 + c]).map(|d| diag[d as usize]).fold(0.0, f64::max);
      let alpha = TIE * if own > 0.0 { own } else { kmax };
      let mut nodes = vec![tie.node];
      nodes.extend(tie.master);
      let mut c = vec![1.0];
      c.extend(tie.weights.iter().map(|w| -w));
      self.sys.add(&mut k, None, &nodes, &|a, b| if a % 3 == b % 3 { alpha * c[a / 3] * c[b / 3] } else { 0.0 });
    }
    k
  }

  pub fn solve(&self, element_scale: Option<&[f64]>) -> Result<StaticSolution, String> {
    if self.sys.free == 0 {
      return Err("every node is fixed: there is nothing to solve".into());
    }
    let k = self.stiffness(element_scale);
    let f = self.sys.restrict(&self.force);
    if f.iter().all(|v| *v == 0.0) {
      return Err("the study has no loads".into());
    }
    let x = linalg::solve(&k, &f, self.options.direct_limit)?;
    let u = self.sys.expand(&x, &vec![0.0; self.force.len()]);
    Ok(self.post(&u, element_scale))
  }

  fn post(&self, u: &[f64], element_scale: Option<&[f64]>) -> StaticSolution {
    let model = self.model;
    let n = model.node_count();
    let off = model.offsets();
    let disp: Vec<V3> = (0..n).map(|i| [u[3 * i], u[3 * i + 1], u[3 * i + 2]]).collect();
    let mut stress = vec![[0.0; 6]; n];
    let mut weight = vec![0.0; n];
    let mut fint = vec![0.0; 3 * n];
    let mut energy = Vec::with_capacity(model.element_count());
    let coords = fem::node_coords();
    let mut e = 0;
    for (pi, p) in model.parts.iter().enumerate() {
      let (lambda, mu) = p.material.lame();
      for t in &p.mesh.tets {
        let s = element_scale.and_then(|s| s.get(e)).copied().unwrap_or(1.0);
        e += 1;
        let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
        let ue: [V3; 10] = t.map(|x| disp[x as usize + off[pi]]);
        let ke = fem::stiffness(&geo, lambda * s, mu * s);
        let flat: Vec<f64> = ue.iter().flat_map(|v| v.iter().copied()).collect();
        let mut en = 0.0;
        for a in 0..30 {
          let fa: f64 = (0..30).map(|b| ke[a][b] * flat[b]).sum();
          fint[3 * (t[a / 3] as usize + off[pi]) + a % 3] += fa;
          en += 0.5 * flat[a] * fa;
        }
        energy.push(en);
        for (k, l) in coords.iter().enumerate() {
          let sig = fem::stress(lambda * s, mu * s, fem::strain(&geo, *l, &ue));
          let g = t[k] as usize + off[pi];
          let w = geo.volume.abs();
          for c in 0..6 {
            stress[g][c] += w * sig[c];
          }
          weight[g] += w;
        }
      }
    }
    for (s, w) in stress.iter_mut().zip(&weight) {
      if *w > 0.0 {
        *s = s.map(|v| v / w);
      }
    }
    let von_mises: Vec<f64> = stress.iter().map(|s| fem::von_mises(*s)).collect();
    let part = model.node_part();
    let safety_factor = von_mises
      .iter()
      .zip(&part)
      .map(|(v, p)| (model.parts[*p].material.yield_strength / v.max(1e-300)).min(15.0))
      .collect();
    let reactions = self
      .fixed_by
      .iter()
      .map(|nodes| {
        let mut r = [0.0; 3];
        for &nd in nodes {
          for c in 0..3 {
            r[c] += fint[3 * nd as usize + c] - self.force[3 * nd as usize + c];
          }
        }
        r
      })
      .collect();
    let compliance = linalg::dot(&self.force, u);
    let mut applied = [0.0; 3];
    for i in 0..n {
      for c in 0..3 {
        applied[c] += self.force[3 * i + c];
      }
    }
    StaticSolution {
      displacement: disp,
      stress,
      von_mises,
      safety_factor,
      reactions,
      element_energy: energy,
      compliance,
      applied,
    }
  }
}

/// The external force vector (3 per node).
fn load_vector(model: &Model) -> Result<Vec<f64>, String> {
  let n = model.node_count();
  let off = model.offsets();
  let mut f = vec![0.0; 3 * n];
  fn put(f: &mut [f64], nodes: &[u32; 6], w: [f64; 6], t: V3) {
    for k in 0..6 {
      for c in 0..3 {
        f[3 * nodes[k] as usize + c] += w[k] * t[c];
      }
    }
  }
  for l in &model.loads {
    match l {
      Load::Force { faces, force } => {
        let tris = model.tris_of(faces);
        let area: f64 = tris.iter().map(|&(p, t)| fem::tri_geo(model.tri(&off, p, t).0).0).sum();
        if area <= 0.0 {
          return Err("a force's faces have no area".into());
        }
        let tr = scale(*force, 1.0 / area);
        for (p, t) in tris {
          let (x, g) = model.tri(&off, p, t);
          put(&mut f, &g, fem::tri_load(fem::tri_geo(x).0), tr);
        }
      }
      Load::Pressure { faces, pressure } => {
        for (p, t) in model.tris_of(faces) {
          let (x, g) = model.tri(&off, p, t);
          let (a, nrm) = fem::tri_geo(x);
          put(&mut f, &g, fem::tri_load(a), scale(nrm, -pressure));
        }
      }
      Load::Bearing { faces, force } => {
        let d = unit(*force);
        let tris = model.tris_of(faces);
        let geo: Vec<(f64, V3)> = tris.iter().map(|&(p, t)| fem::tri_geo(model.tri(&off, p, t).0)).collect();
        let total: f64 = geo.iter().map(|(a, n)| a * (-dot(*n, d)).max(0.0)).sum();
        if total <= 0.0 {
          return Err("no part of the bearing load's faces faces the load".into());
        }
        for ((p, t), (a, nrm)) in tris.into_iter().zip(geo) {
          let w = (-dot(nrm, d)).max(0.0);
          if w == 0.0 {
            continue;
          }
          let (_, g) = model.tri(&off, p, t);
          put(&mut f, &g, fem::tri_load(a), scale(*force, w / total));
        }
      }
      Load::Moment { faces, moment } => {
        let tris = model.tris_of(faces);
        let cents: Vec<(V3, f64)> = tris
          .iter()
          .map(|&(p, t)| {
            let x = model.tri(&off, p, t).0;
            (scale(add(add(x[0], x[1]), x[2]), 1.0 / 3.0), fem::tri_geo(x).0)
          })
          .collect();
        let area: f64 = cents.iter().map(|c| c.1).sum();
        if area <= 0.0 {
          return Err("a moment's faces have no area".into());
        }
        let c = scale(cents.iter().fold([0.0; 3], |s, (x, a)| add(s, scale(*x, *a))), 1.0 / area);
        let mut j = [[0.0; 3]; 3];
        for (x, a) in &cents {
          let r = sub(*x, c);
          let rr = dot(r, r);
          for i in 0..3 {
            for k in 0..3 {
              j[i][k] += a * (if i == k { rr } else { 0.0 } - r[i] * r[k]);
            }
          }
        }
        // Ω = J⁻¹ M through the eigen decomposition (directions J can't twist about drop out)
        let (vals, vecs) = sym_eigen3(j);
        let mut omega = [0.0; 3];
        for k in 0..3 {
          if vals[k] > 1e-9 * vals[0] {
            let v = column(&vecs, k);
            omega = add(omega, scale(v, dot(v, *moment) / vals[k]));
          }
        }
        for ((p, t), (x, a)) in tris.into_iter().zip(cents) {
          let (_, g) = model.tri(&off, p, t);
          put(&mut f, &g, fem::tri_load(a), cross(omega, sub(x, c)));
        }
      }
      Load::Gravity { acceleration } => {
        for (pi, p) in model.parts.iter().enumerate() {
          let rho = p.material.density_t_mm3();
          for t in &p.mesh.tets {
            let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
            let w = fem::volume_load(&geo);
            for k in 0..10 {
              for c in 0..3 {
                f[3 * (t[k] as usize + off[pi]) + c] += rho * w[k] * acceleration[c];
              }
            }
          }
        }
      }
    }
  }
  Ok(f)
}

pub fn solve_static(model: &Model, options: SolveOptions) -> Result<StaticSolution, String> {
  StaticProblem::new(model, options)?.solve(None)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mode {
  /// Hz.
  pub frequency: f64,
  /// The mode shape, scaled so its largest displacement is 1.
  pub shape: Vec<V3>,
  /// Effective mass in X, Y and Z as a fraction of the total.
  pub participation: V3,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModalSolution {
  pub modes: Vec<Mode>,
  /// Total mass (kg).
  pub mass: f64,
}

/// The lowest natural frequencies and mode shapes. Loads are ignored; with too few constraints
/// the first modes are rigid-body motions near 0 Hz.
pub fn solve_modal(model: &Model, options: SolveOptions) -> Result<ModalSolution, String> {
  let mut quiet = model.clone();
  quiet.loads.clear();
  let prob = StaticProblem::new(&quiet, options)?;
  if prob.sys.free == 0 {
    return Err("every node is fixed: there is nothing to vibrate".into());
  }
  let k = prob.stiffness(None);
  let off = model.offsets();
  let mut m = prob.sys.pattern.clone();
  let mut mass_total = 0.0;
  for (pi, p) in model.parts.iter().enumerate() {
    let rho = p.material.density_t_mm3();
    if rho <= 0.0 {
      return Err(format!("{} has no density", p.name));
    }
    for t in &p.mesh.tets {
      let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
      mass_total += rho * geo.volume;
      let me = fem::mass(&geo);
      let nodes = t.map(|x| x + off[pi] as u32);
      prob.sys.add(&mut m, None, &nodes, &|a, b| if a % 3 == b % 3 { rho * me[a / 3][b / 3] } else { 0.0 });
    }
  }
  // shift a little below zero so free-floating models (zero eigenvalues) factorize
  let kd = k.diag();
  let md = m.diag();
  let mut ratios: Vec<f64> = kd.iter().zip(&md).filter(|(_, m)| **m > 0.0).map(|(k, m)| k / m).collect();
  ratios.sort_by(f64::total_cmp);
  let shift = -1e-6 * ratios.get(ratios.len() / 2).copied().unwrap_or(1.0);
  let pairs = linalg::eigen_lowest(&k, &m, options.modes.max(1), shift)?;
  let zeros = vec![0.0; 3 * model.node_count()];
  // participation: Γ = φᵀ M r for rigid translations r
  let rigid: Vec<Vec<f64>> = (0..3)
    .map(|c| {
      let mut full = vec![0.0; 3 * model.node_count()];
      for i in 0..model.node_count() {
        full[3 * i + c] = 1.0;
      }
      prob.sys.restrict(&full)
    })
    .collect();
  let mr: Vec<Vec<f64>> = rigid.iter().map(|r| m.mul(r)).collect();
  let modes = pairs
    .into_iter()
    .map(|(lambda, phi)| {
      let participation = [0, 1, 2].map(|c| {
        let g = linalg::dot(&phi, &mr[c]);
        g * g / mass_total.max(1e-300)
      });
      let full = prob.sys.expand(&phi, &zeros);
      let shape: Vec<V3> = full.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
      let big = shape.iter().map(|v| norm(*v)).fold(0.0, f64::max);
      let shape = if big > 0.0 { shape.iter().map(|v| scale(*v, 1.0 / big)).collect() } else { shape };
      Mode { frequency: lambda.max(0.0).sqrt() / (2.0 * std::f64::consts::PI), shape, participation }
    })
    .collect();
  Ok(ModalSolution { modes, mass: mass_total * 1000.0 })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThermalSolution {
  /// °C at each node.
  pub temperature: Vec<f64>,
  /// Heat flux −k∇T at each node (W/mm²).
  pub flux: Vec<V3>,
  /// Heat flowing out through each thermal load's faces (W): what holds a temperature or
  /// convects away.
  pub heat_flow: Vec<f64>,
}

/// Steady-state heat conduction.
pub fn solve_thermal(model: &Model, options: SolveOptions) -> Result<ThermalSolution, String> {
  model.check()?;
  let n = model.node_count();
  let off = model.offsets();
  let mut fixed = vec![false; n];
  let mut known = vec![0.0; n];
  let mut anchored = false;
  for l in &model.thermal {
    match l {
      ThermalLoad::Temperature { faces, temperature } => {
        for nd in model.nodes_of(faces) {
          fixed[nd as usize] = true;
          known[nd as usize] = *temperature;
        }
        anchored = true;
      }
      ThermalLoad::Convection { coefficient, .. } if *coefficient > 0.0 => anchored = true,
      _ => {}
    }
  }
  if !anchored {
    return Err("the temperature can't settle: add a temperature or convection condition".into());
  }
  let ties = model.ties();
  let sys = System::new(model, 1, &fixed, &ties);
  if sys.free == 0 {
    return Ok(ThermalSolution {
      temperature: known,
      flux: vec![[0.0; 3]; n],
      heat_flow: vec![0.0; model.thermal.len()],
    });
  }
  let mut k = sys.pattern.clone();
  let mut f = vec![0.0; sys.free];
  // full-length load for the heat balance afterwards
  let mut q_ext = vec![0.0; n];
  for (pi, p) in model.parts.iter().enumerate() {
    let cond = p.material.conductivity_mm();
    if cond <= 0.0 {
      return Err(format!("{} doesn't conduct heat", p.name));
    }
    for t in &p.mesh.tets {
      let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
      let ke = fem::conduction(&geo, cond);
      let nodes = t.map(|x| x + off[pi] as u32);
      sys.add(&mut k, Some((&mut f, &known)), &nodes, &|a, b| ke[a][b]);
    }
  }
  let put = |f: &mut [f64], q: &mut [f64], nodes: &[u32], w: &[f64]| {
    for (k, &nd) in nodes.iter().enumerate() {
      q[nd as usize] += w[k];
      if let Some(d) = sys.dof[nd as usize] {
        f[d as usize] += w[k];
      }
    }
  };
  for l in &model.thermal {
    match l {
      ThermalLoad::Temperature { .. } => {}
      ThermalLoad::HeatFlux { faces, flux } => {
        for (p, t) in model.tris_of(faces) {
          let (x, g) = model.tri(&off, p, t);
          put(&mut f, &mut q_ext, &g, &fem::tri_load(fem::tri_geo(x).0).map(|w| w * flux));
        }
      }
      ThermalLoad::HeatPower { faces, power } => {
        let tris = model.tris_of(faces);
        let area: f64 = tris.iter().map(|&(p, t)| fem::tri_geo(model.tri(&off, p, t).0).0).sum();
        if area <= 0.0 {
          return Err("a heat load's faces have no area".into());
        }
        for (p, t) in tris {
          let (x, g) = model.tri(&off, p, t);
          put(&mut f, &mut q_ext, &g, &fem::tri_load(fem::tri_geo(x).0).map(|w| w * power / area));
        }
      }
      ThermalLoad::Convection { faces, coefficient, ambient } => {
        for (p, t) in model.tris_of(faces) {
          let (x, g) = model.tri(&off, p, t);
          let a = fem::tri_geo(x).0;
          let mm = fem::tri_mass(a);
          sys.add(&mut k, Some((&mut f, &known)), &g, &|i, j| coefficient * mm[i][j]);
          put(&mut f, &mut q_ext, &g, &fem::tri_load(a).map(|w| w * coefficient * ambient));
        }
      }
      ThermalLoad::InternalHeat { part, power } => {
        let Some(p) = model.parts.get(*part) else { return Err(format!("no part {part}")) };
        let vol = p.mesh.volume();
        for t in &p.mesh.tets {
          let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
          let nodes: Vec<u32> = t.iter().map(|x| x + off[*part] as u32).collect();
          put(&mut f, &mut q_ext, &nodes, &fem::volume_load(&geo).map(|w| w * power / vol));
        }
      }
    }
  }
  let kmax = k.diag().iter().fold(0.0f64, |a, b| a.max(*b)).max(1e-30);
  for tie in &ties {
    let alpha = TIE * kmax;
    let mut nodes = vec![tie.node];
    nodes.extend(tie.master);
    let mut c = vec![1.0];
    c.extend(tie.weights.iter().map(|w| -w));
    sys.add(&mut k, Some((&mut f, &known)), &nodes, &|a, b| alpha * c[a] * c[b]);
  }
  let x = linalg::solve(&k, &f, options.direct_limit)?;
  let temperature = sys.expand(&x, &known);
  // flux and the heat balance
  let mut flux = vec![[0.0; 3]; n];
  let mut weight = vec![0.0; n];
  let mut q_int = vec![0.0; n];
  let coords = fem::node_coords();
  for (pi, p) in model.parts.iter().enumerate() {
    let cond = p.material.conductivity_mm();
    for t in &p.mesh.tets {
      let geo = Geo::new([0, 1, 2, 3].map(|c| p.mesh.nodes[t[c] as usize]));
      let te: Vec<f64> = t.iter().map(|x| temperature[*x as usize + off[pi]]).collect();
      let ke = fem::conduction(&geo, cond);
      for a in 0..10 {
        q_int[t[a] as usize + off[pi]] += (0..10).map(|b| ke[a][b] * te[b]).sum::<f64>();
      }
      for (kk, l) in coords.iter().enumerate() {
        let g = geo.shape_gradients(*l);
        let mut grad = [0.0; 3];
        for a in 0..10 {
          grad = add(grad, scale(g[a], te[a]));
        }
        let gi = t[kk] as usize + off[pi];
        let w = geo.volume.abs();
        flux[gi] = add(flux[gi], scale(grad, -cond * w));
        weight[gi] += w;
      }
    }
  }
  for (q, w) in flux.iter_mut().zip(&weight) {
    if *w > 0.0 {
      *q = scale(*q, 1.0 / w);
    }
  }
  let heat_flow = model
    .thermal
    .iter()
    .map(|l| match l {
      ThermalLoad::Temperature { faces, .. } => {
        // what the held faces take out: internal conduction minus applied loads there
        model.nodes_of(faces).iter().map(|&nd| q_int[nd as usize] - q_ext[nd as usize]).sum()
      }
      ThermalLoad::Convection { faces, coefficient, ambient } => model
        .tris_of(faces)
        .iter()
        .map(|&(p, t)| {
          let (x, g) = model.tri(&off, p, t);
          let w = fem::tri_load(fem::tri_geo(x).0);
          (0..6).map(|k| w[k] * coefficient * (temperature[g[k] as usize] - ambient)).sum::<f64>()
        })
        .sum(),
      _ => 0.0,
    })
    .collect();
  Ok(ThermalSolution { temperature, flux, heat_flow })
}
