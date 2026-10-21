//! Generative design commands (`gen.*`): studies with their design space, load cases,
//! objectives, materials and manufacturing methods; generating outcomes with [`fox_gen`] (one
//! per material and method); and inserting an outcome into the design as a mesh body, converted
//! to B-rep if asked and joined with the exact preserve geometry.
//!
//! The UI runs the solves on a thread of its own: [`jobs`] builds the problems from the design,
//! [`fox_gen::solve`] runs each, and [`finish`] stores what they found.

use std::sync::Arc;

use fox_gen::{Cell, Dir, Grid, Material, Method, Problem};
use fox_kernel::TriMesh;
use fox_model::construct::MeshOp;
use fox_model::generative::{Constraint, Goal, Load, LoadCase, LoadKind, Outcome, Study, Target};
use fox_model::{BodyId, FeatureKind, MeshData, Operation};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str};
use crate::{CmdError, CmdResult, Session};

pub use fox_gen::{GenError, Outcome as Solution, Progress, materials, solve};

/// Symmetry planes by normal axis.
const PLANES: [&str; 3] = ["yz", "xz", "xy"];

/// Density of the iso-surface the outcome's body follows.
pub const ISO: f64 = 0.47;
/// Smoothing passes on the outcome's surface.
const SMOOTHING: usize = 10;

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "gen.study" => study_new(s, p),
    "gen.studies" => Ok(json!(s.design.generative.iter().map(study_json).collect::<Vec<_>>())),
    "gen.info" => study_of(s, p).map(|i| study_json(&s.design.generative[i])),
    "gen.delete" => (|| {
      let i = study_of(s, p)?;
      let id = s.design.generative[i].id;
      s.edit(|d| {
        d.generative.remove(i);
        Ok(())
      })?;
      Ok(json!({"deleted": id}))
    })(),
    "gen.preserve" | "gen.obstacle" => bodies_cmd(s, name, p),
    "gen.starting_shape" => (|| {
      let body = match p.get("body") {
        None | Some(Value::Null) => None,
        Some(v) => Some(body_id(s, v)?),
      };
      edit_study(s, p, |st| {
        st.starting_shape = body;
        Ok(())
      })
    })(),
    "gen.design_space" => design_space(s, p),
    "gen.case" => case_cmd(s, p),
    "gen.load" => load_cmd(s, p),
    "gen.constraint" => constraint_cmd(s, p),
    "gen.objective" => objective_cmd(s, p),
    "gen.materials" => materials_cmd(s, p),
    "gen.material_library" => Ok(json!(fox_gen::materials().iter().map(material_json).collect::<Vec<_>>())),
    "gen.manufacturing" => manufacturing_cmd(s, p),
    "gen.precheck" => precheck(s, p),
    "gen.generate" => generate(s, p),
    "gen.outcomes" => study_of(s, p).map(|i| outcomes_json(&s.design.generative[i])),
    "gen.insert" => insert(s, p),
    "gen.export" => export(s, p),
    _ => return None,
  })
}

fn num(v: &Value) -> Option<f64> {
  match v {
    Value::String(e) => fox_expr::eval(e, fox_expr::Kind::Length, &std::collections::HashMap::new())
      .or_else(|_| fox_expr::eval(e, fox_expr::Kind::Unitless, &std::collections::HashMap::new()))
      .ok(),
    v => v.as_f64(),
  }
}

fn opt_num(p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => num(v).map(Some).ok_or_else(|| CmdError::new(format!("'{k}' should be a number"))),
  }
}

fn vec3(v: &Value, k: &str) -> Result<[f64; 3], CmdError> {
  let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| CmdError::new(format!("'{k}' is [x, y, z]")))?;
  let mut out = [0.0; 3];
  for (o, x) in out.iter_mut().zip(a) {
    *o = num(x).ok_or_else(|| CmdError::new(format!("'{k}' is [x, y, z]")))?;
  }
  Ok(out)
}

fn body_id(s: &Session, v: &Value) -> Result<BodyId, CmdError> {
  let id = v.as_u64().ok_or(CmdError::new("a body is its id"))? as u32;
  s.model.body(BodyId(id)).ok_or_else(|| CmdError::new(format!("no body {id}")))?;
  Ok(BodyId(id))
}

fn bodies_of(s: &Session, p: &Value) -> Result<Vec<BodyId>, CmdError> {
  match (p.get("bodies"), p.get("body")) {
    (Some(Value::Array(a)), _) => a.iter().map(|v| body_id(s, v)).collect(),
    (_, Some(v)) => Ok(vec![body_id(s, v)?]),
    _ => Err(CmdError::new("'bodies' is required")),
  }
}

/// The study a command names (`study`: id or name), or the last one.
fn study_of(s: &Session, p: &Value) -> Result<usize, CmdError> {
  let st = &s.design.generative;
  match p.get("study") {
    Some(Value::Number(n)) => {
      let id = n.as_u64().unwrap_or(0) as u32;
      st.iter().position(|x| x.id == id).ok_or_else(|| CmdError::new(format!("no study {id}")))
    }
    Some(Value::String(n)) => {
      st.iter().position(|x| x.name.eq_ignore_ascii_case(n)).ok_or_else(|| CmdError::new(format!("no study '{n}'")))
    }
    _ if st.is_empty() => Err(CmdError::new("there is no generative study: gen.study first")),
    _ => Ok(st.len() - 1),
  }
}

fn edit_study(s: &mut Session, p: &Value, f: impl FnOnce(&mut Study) -> Result<(), CmdError>) -> CmdResult {
  let i = study_of(s, p)?;
  s.edit(|d| f(&mut d.generative[i]))?;
  Ok(study_json(&s.design.generative[i]))
}

fn study_new(s: &mut Session, p: &Value) -> CmdResult {
  let id = s.design.generative.iter().map(|x| x.id).max().unwrap_or(0) + 1;
  let name = opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("Study {id}"));
  let mut st = Study::new(id, name);
  st.materials = vec![fox_gen::material("Aluminum AlSi10Mg").ok_or(CmdError::new("no default material"))?];
  st.methods = vec![Method::Unrestricted];
  if let Some(r) = opt_num(p, "resolution")? {
    st.resolution = r.max(0.0);
  }
  s.edit(|d| {
    d.generative.push(st);
    Ok(())
  })?;
  Ok(study_json(s.design.generative.last().ok_or(CmdError::new("no study"))?))
}

fn bodies_cmd(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let bodies = bodies_of(s, p)?;
  let remove = p.get("remove").and_then(Value::as_bool).unwrap_or(false);
  let replace = p.get("replace").and_then(Value::as_bool).unwrap_or(false);
  let preserve = name == "gen.preserve";
  edit_study(s, p, |st| {
    let (list, other) =
      if preserve { (&mut st.preserve, &mut st.obstacles) } else { (&mut st.obstacles, &mut st.preserve) };
    if replace {
      list.clear();
    }
    for b in bodies {
      if remove {
        list.retain(|x| *x != b);
      } else if !list.contains(&b) {
        // a body is one or the other
        other.retain(|x| *x != b);
        list.push(b);
      }
    }
    Ok(())
  })
}

fn design_space(s: &mut Session, p: &Value) -> CmdResult {
  let margin = opt_num(p, "margin")?;
  let bounds = match (p.get("min"), p.get("max")) {
    (Some(a), Some(b)) => Some((vec3(a, "min")?, vec3(b, "max")?)),
    _ => None,
  };
  let clear = p.get("clear_bounds").and_then(Value::as_bool).unwrap_or(false);
  let resolution = opt_num(p, "resolution")?;
  let elements = opt_num(p, "elements")?;
  let iterations = opt_num(p, "iterations")?;
  if let Some((lo, hi)) = bounds
    && (0..3).any(|a| hi[a] <= lo[a])
  {
    return Err(CmdError::new("the design space's max must be above its min on every axis"));
  }
  edit_study(s, p, |st| {
    if let Some(m) = margin {
      st.margin = m.max(0.0);
    }
    if bounds.is_some() {
      st.bounds = bounds;
    } else if clear {
      st.bounds = None;
    }
    if let Some(r) = resolution {
      st.resolution = r.max(0.0);
    }
    if let Some(e) = elements {
      st.elements = (e as usize).clamp(100, 2_000_000);
    }
    if let Some(i) = iterations {
      st.iterations = (i as usize).clamp(1, 1000);
    }
    Ok(())
  })
}

/// The load case a command names (`case`: index or name, created when new), default the first.
fn case_index(st: &mut Study, p: &Value) -> usize {
  match p.get("case") {
    Some(Value::Number(n)) => (n.as_u64().unwrap_or(0) as usize).min(st.cases.len().saturating_sub(1)),
    Some(Value::String(name)) => match st.cases.iter().position(|c| c.name.eq_ignore_ascii_case(name)) {
      Some(i) => i,
      None => {
        // a new case keeps the first case's constraints, as Fusion's copies do
        let constraints = st.cases.first().map(|c| c.constraints.clone()).unwrap_or_default();
        st.cases.push(LoadCase { name: name.clone(), loads: Vec::new(), constraints });
        st.cases.len() - 1
      }
    },
    _ => {
      if st.cases.is_empty() {
        st.cases.push(LoadCase { name: "Load Case 1".into(), loads: Vec::new(), constraints: Vec::new() });
      }
      0
    }
  }
}

fn case_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let delete = p.get("delete").and_then(Value::as_bool).unwrap_or(false);
  let clear = p.get("clear").and_then(Value::as_bool).unwrap_or(false);
  let rename = opt_str(p, "rename").map(str::to_string);
  edit_study(s, p, |st| {
    let i = case_index(st, p);
    if delete {
      if st.cases.len() == 1 {
        return Err(CmdError::new("a study keeps at least one load case"));
      }
      st.cases.remove(i);
      return Ok(());
    }
    if clear {
      st.cases[i].loads.clear();
      st.cases[i].constraints.clear();
    }
    if let Some(n) = rename {
      st.cases[i].name = n;
    }
    Ok(())
  })
}

/// `face` ({body, face} or {point}) or `point` [x, y, z] with an optional `radius`.
fn target(s: &Session, p: &Value) -> Result<Target, CmdError> {
  if let Some(f) = p.get("face") {
    return Ok(Target::Face { face: face_ref(s, f)? });
  }
  if let Some(v) = p.get("point") {
    let at = vec3(v, "point")?;
    return Ok(Target::Point { at, radius: opt_num(p, "radius")?.unwrap_or(0.0).max(0.0) });
  }
  Err(CmdError::new("a 'face' ({body, face}) or a 'point' [x, y, z] is needed"))
}

fn targets(s: &Session, p: &Value) -> Result<Vec<Target>, CmdError> {
  match p.get("faces") {
    Some(Value::Array(a)) => a.iter().map(|f| Ok(Target::Face { face: face_ref(s, f)? })).collect(),
    _ => Ok(vec![target(s, p)?]),
  }
}

fn load_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let targets = targets(s, p)?;
  let kind = match (p.get("force"), opt_num(p, "pressure")?) {
    (Some(f), None) => {
      let mut force = match f {
        Value::Array(_) => vec3(f, "force")?,
        // a magnitude along `direction` (default -z)
        v => {
          let m = num(v).ok_or(CmdError::new("'force' is [fx, fy, fz] or a magnitude"))?;
          let d = match p.get("direction") {
            Some(Value::String(d)) => {
              let d = Dir::parse(d).ok_or(CmdError::new("'direction' is +x .. -z or [x, y, z]"))?;
              let mut v = [0.0; 3];
              v[d.axis] = if d.positive { 1.0 } else { -1.0 };
              v
            }
            Some(d) => vec3(d, "direction")?,
            None => [0.0, 0.0, -1.0],
          };
          let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
          if l < 1e-12 {
            return Err(CmdError::new("'direction' has no length"));
          }
          d.map(|c| c / l * m)
        }
      };
      // spread over several faces
      force = force.map(|c| c / targets.len() as f64);
      LoadKind::Force { force }
    }
    (None, Some(pressure)) => {
      if targets.iter().any(|t| !matches!(t, Target::Face { .. })) {
        return Err(CmdError::new("a pressure acts on faces"));
      }
      LoadKind::Pressure { pressure }
    }
    _ => return Err(CmdError::new("a load is a 'force' or a 'pressure'")),
  };
  edit_study(s, p, |st| {
    let i = case_index(st, p);
    for t in targets {
      st.cases[i].loads.push(Load { target: t, kind: kind.clone() });
    }
    Ok(())
  })
}

fn constraint_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let targets = targets(s, p)?;
  let fix = match opt_str(p, "fix").unwrap_or("xyz").to_ascii_lowercase() {
    f if !f.is_empty() && f.chars().all(|c| "xyz".contains(c)) => [f.contains('x'), f.contains('y'), f.contains('z')],
    f => return Err(CmdError::new(format!("'fix' is the fixed axes, like xyz or z (not '{f}')"))),
  };
  let all = p.get("case").is_none() && p.get("all_cases").and_then(Value::as_bool).unwrap_or(true);
  edit_study(s, p, |st| {
    let cases: Vec<usize> = if all {
      if st.cases.is_empty() {
        case_index(st, p);
      }
      (0..st.cases.len()).collect()
    } else {
      vec![case_index(st, p)]
    };
    for i in cases {
      for t in &targets {
        st.cases[i].constraints.push(Constraint { target: t.clone(), fix });
      }
    }
    Ok(())
  })
}

fn objective_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let goal = match opt_str(p, "objective") {
    None => None,
    Some("max_stiffness" | "stiffness" | "maximize_stiffness") => Some(Goal::MaxStiffness),
    Some("min_mass" | "mass" | "minimize_mass") => Some(Goal::MinMass),
    Some(o) => return Err(CmdError::new(format!("unknown objective '{o}' (max_stiffness, min_mass)"))),
  };
  let target = opt_num(p, "mass_target")?;
  if let Some(t) = target
    && !(0.01..=1.0).contains(&t)
  {
    return Err(CmdError::new("'mass_target' is the solid fraction of the design space, 0.01 to 1"));
  }
  let sf = opt_num(p, "safety_factor")?;
  let disp = p.get("max_displacement").map(|v| if v.is_null() { Ok(None) } else { num(v).map(Some).ok_or(()) });
  let disp = disp.transpose().map_err(|_| CmdError::new("'max_displacement' is a length or null"))?;
  edit_study(s, p, |st| {
    if let Some(g) = goal {
      st.objective = g;
    }
    if let Some(t) = target {
      st.mass_target = t;
    }
    if let Some(f) = sf {
      st.safety_factor = f.max(0.0);
    }
    if let Some(d) = disp {
      st.max_displacement = d.filter(|d| *d > 0.0);
    }
    Ok(())
  })
}

fn material_of(v: &Value) -> Result<Material, CmdError> {
  match v {
    Value::String(n) => fox_gen::material(n).ok_or_else(|| {
      CmdError::new(format!(
        "unknown or ambiguous material '{n}' ({})",
        fox_gen::materials().iter().map(|m| m.name.clone()).collect::<Vec<_>>().join(", ")
      ))
    }),
    Value::Object(_) => {
      let m: Material = serde_json::from_value(v.clone()).map_err(|e| CmdError::new(format!("bad material: {e}")))?;
      if !(m.youngs > 0.0 && m.density > 0.0 && m.yield_strength > 0.0 && (-1.0..0.5).contains(&m.poisson)) {
        return Err(CmdError::new("a material needs youngs, density and yield above 0 and poisson below 0.5"));
      }
      Ok(m)
    }
    _ => Err(CmdError::new("a material is a library name or {name, youngs, poisson, density, yield}")),
  }
}

fn materials_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let list = p.get("materials").and_then(Value::as_array).ok_or(CmdError::new("'materials' is a list"))?;
  let ms = list.iter().map(material_of).collect::<Result<Vec<_>, _>>()?;
  if ms.is_empty() {
    return Err(CmdError::new("a study needs at least one material"));
  }
  edit_study(s, p, |st| {
    st.materials = ms;
    Ok(())
  })
}

fn method_of(v: &Value) -> Result<Method, CmdError> {
  let (kind, dirs) = match v {
    Value::String(k) => (k.as_str(), Vec::new()),
    Value::Object(_) => {
      let dirs: Vec<String> = match v.get("directions").or_else(|| v.get("direction")) {
        Some(Value::String(d)) => vec![d.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
      };
      (v.get("kind").and_then(Value::as_str).unwrap_or("unrestricted"), dirs)
    }
    _ => return Err(CmdError::new("a method is unrestricted | milling_3axis | milling_2_5axis or {kind, directions}")),
  };
  let dirs = dirs
    .iter()
    .map(|d| Dir::parse(d).ok_or_else(|| CmdError::new(format!("unknown tool direction '{d}' (+x .. -z)"))))
    .collect::<Result<Vec<_>, _>>()?;
  let z = Dir { axis: 2, positive: true };
  Ok(match kind {
    "unrestricted" | "additive" => Method::Unrestricted,
    "milling_3axis" | "3axis" | "3_axis" | "milling3" => {
      Method::Milling3 { directions: if dirs.is_empty() { vec![z] } else { dirs } }
    }
    "milling_2_5axis" | "2_5axis" | "2.5axis" | "milling25" | "extrusion" => {
      Method::Milling25 { direction: dirs.first().copied().unwrap_or(z) }
    }
    k => return Err(CmdError::new(format!("unknown method '{k}' (unrestricted, milling_3axis, milling_2_5axis)"))),
  })
}

fn manufacturing_cmd(s: &mut Session, p: &Value) -> CmdResult {
  let methods = match p.get("methods") {
    Some(Value::Array(a)) => Some(a.iter().map(method_of).collect::<Result<Vec<_>, _>>()?),
    Some(v) => Some(vec![method_of(v)?]),
    None => None,
  };
  if methods.as_ref().is_some_and(Vec::is_empty) {
    return Err(CmdError::new("a study needs at least one manufacturing method"));
  }
  let symmetry = match p.get("symmetry") {
    Some(Value::Array(a)) => Some(
      a.iter()
        .map(|v| {
          let (plane, offset) = match v {
            Value::String(pl) => (pl.as_str(), 0.0),
            v => (v.get("plane").and_then(Value::as_str).unwrap_or(""), v.get("offset").and_then(num).unwrap_or(0.0)),
          };
          let axis = match plane.to_ascii_lowercase().as_str() {
            "yz" | "x" => 0,
            "xz" | "zx" | "y" => 1,
            "xy" | "z" => 2,
            pl => return Err(CmdError::new(format!("unknown symmetry plane '{pl}' (yz, xz, xy)"))),
          };
          Ok((axis, offset))
        })
        .collect::<Result<Vec<_>, CmdError>>()?,
    ),
    _ => None,
  };
  let min_member = opt_num(p, "min_member")?;
  edit_study(s, p, |st| {
    if let Some(m) = methods {
      st.methods = m;
    }
    if let Some(sy) = symmetry {
      st.symmetry = sy;
    }
    if let Some(m) = min_member {
      st.min_member = m.max(0.0);
    }
    Ok(())
  })
}

fn material_json(m: &Material) -> Value {
  json!({"name": m.name, "youngs": m.youngs, "poisson": m.poisson, "density": m.density, "yield": m.yield_strength})
}

fn target_json(t: &Target) -> Value {
  match t {
    Target::Face { face } => json!({"body": face.body.0, "point": face.point}),
    Target::Point { at, radius } => json!({"point": at, "radius": radius}),
  }
}

fn study_json(st: &Study) -> Value {
  json!({
    "study": st.id, "name": st.name,
    "preserve": st.preserve.iter().map(|b| b.0).collect::<Vec<_>>(),
    "obstacles": st.obstacles.iter().map(|b| b.0).collect::<Vec<_>>(),
    "starting_shape": st.starting_shape.map(|b| b.0),
    "margin": st.margin, "bounds": st.bounds,
    "cases": st.cases.iter().map(|c| json!({
      "name": c.name,
      "loads": c.loads.iter().map(|l| {
        let mut j = target_json(&l.target);
        match &l.kind {
          LoadKind::Force { force } => j["force"] = json!(force),
          LoadKind::Pressure { pressure } => j["pressure"] = json!(pressure),
        }
        j
      }).collect::<Vec<_>>(),
      "constraints": c.constraints.iter().map(|k| {
        let mut j = target_json(&k.target);
        j["fix"] = json!(["x", "y", "z"].iter().zip(k.fix).filter(|(_, f)| *f).map(|(a, _)| *a).collect::<String>());
        j
      }).collect::<Vec<_>>(),
    })).collect::<Vec<_>>(),
    "objective": match st.objective { Goal::MaxStiffness => "max_stiffness", Goal::MinMass => "min_mass" },
    "mass_target": st.mass_target, "safety_factor": st.safety_factor, "max_displacement": st.max_displacement,
    "materials": st.materials.iter().map(|m| m.name.clone()).collect::<Vec<_>>(),
    "methods": st.methods.iter().map(Method::label).collect::<Vec<_>>(),
    "symmetry": st.symmetry.iter().map(|(a, o)| json!({"plane": PLANES[*a], "offset": o})).collect::<Vec<_>>(),
    "min_member": st.min_member, "resolution": st.resolution, "elements": st.elements, "iterations": st.iterations,
    "outcomes": st.outcomes.len(),
  })
}

fn outcome_json(o: &Outcome) -> Value {
  json!({
    "outcome": o.id, "material": o.material.name, "method": o.method.label(),
    "mass": o.mass, "volume": o.volume, "volume_fraction": o.volume_fraction,
    "compliance": o.compliance, "stiffness": if o.compliance > 0.0 { 1.0 / o.compliance } else { 0.0 },
    "max_displacement": o.max_displacement, "peak_stress": o.peak_stress, "safety_factor": o.safety_factor,
    "meets_limits": o.meets_limits, "converged": o.converged, "iterations": o.iterations,
    "triangles": o.mesh.triangles(),
  })
}

fn outcomes_json(st: &Study) -> Value {
  json!(st.outcomes.iter().map(outcome_json).collect::<Vec<_>>())
}

/// One solve of a study: a material and a manufacturing method.
#[derive(Debug, Clone)]
pub struct Job {
  pub study: u32,
  pub problem: Problem,
}

/// What the grid of a study looks like, for the precheck.
struct Built {
  grid: Grid,
  cells: Vec<Cell>,
  start: Option<Vec<f32>>,
  cases: Vec<fox_gen::LoadCase>,
  symmetry: [bool; 3],
  warnings: Vec<String>,
  /// Nodes picked per load and constraint, by case.
  picked: Vec<(Vec<usize>, Vec<usize>)>,
}

fn mesh_of(s: &Session, b: BodyId) -> Result<&TriMesh, CmdError> {
  let body = s.model.body(b).ok_or_else(|| CmdError::new(format!("body {} no longer exists", b.0)))?;
  body.mesh().ok_or_else(|| CmdError::new(format!("body {} can't be meshed", b.0)))
}

fn bounds_of(m: &TriMesh, lo: &mut [f64; 3], hi: &mut [f64; 3]) {
  for p in &m.positions {
    for a in 0..3 {
      lo[a] = lo[a].min(p[a] as f64);
      hi[a] = hi[a].max(p[a] as f64);
    }
  }
}

fn build(s: &Session, st: &Study) -> Result<Built, CmdError> {
  let mut warnings = Vec::new();
  if st.preserve.is_empty() && st.starting_shape.is_none() && st.bounds.is_none() {
    return Err(CmdError::new("the study has no design space: add preserve geometry, a starting shape or bounds"));
  }
  let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
  match st.bounds {
    Some((l, h)) => (lo, hi) = (l, h),
    None => {
      for b in st.preserve.iter().chain(st.starting_shape.iter()) {
        bounds_of(mesh_of(s, *b)?, &mut lo, &mut hi);
      }
      lo = lo.map(|v| v - st.margin);
      hi = hi.map(|v| v + st.margin);
    }
  }
  let mut symmetry = [false; 3];
  for (axis, off) in &st.symmetry {
    let a = (*axis).min(2);
    // centre the grid on the plane
    let half = (off - lo[a]).max(hi[a] - off);
    if half <= 0.0 {
      return Err(CmdError::new("a symmetry plane is outside the design space"));
    }
    lo[a] = off - half;
    hi[a] = off + half;
    symmetry[a] = true;
  }
  let res = (st.resolution > 0.0).then_some(st.resolution);
  let grid = Grid::around(lo, hi, res, st.elements, 1);
  if grid.elements() > 3_000_000 {
    return Err(CmdError::new(format!(
      "{} x {} x {} elements is too many: use a coarser resolution",
      grid.n[0], grid.n[1], grid.n[2]
    )));
  }
  let mut cells = vec![Cell::Design; grid.elements()];
  // the outer layer stays empty, so the outcome's surface closes
  for (e, c) in cells.iter_mut().enumerate() {
    let ijk = grid.elem_ijk(e);
    if (0..3).any(|a| ijk[a] == 0 || ijk[a] + 1 == grid.n[a]) {
      *c = Cell::Void;
    }
  }
  for b in &st.preserve {
    let m = mesh_of(s, *b)?;
    let inside = grid.inside(&m.positions, &m.indices);
    let mut n = 0;
    for (c, i) in cells.iter_mut().zip(&inside) {
      if *i && *c == Cell::Design {
        *c = Cell::Preserve;
        n += 1;
      }
    }
    if n == 0 {
      warnings
        .push(format!("preserve body {} is thinner than the elements ({:.3} mm): use a finer resolution", b.0, grid.h));
    }
  }
  for b in &st.obstacles {
    let m = mesh_of(s, *b)?;
    for (c, i) in cells.iter_mut().zip(grid.inside(&m.positions, &m.indices)) {
      if i {
        *c = Cell::Void;
      }
    }
  }
  let start = match st.starting_shape {
    Some(b) => {
      let m = mesh_of(s, b)?;
      Some(grid.inside(&m.positions, &m.indices).iter().map(|i| if *i { 1.0 } else { 0.05 }).collect())
    }
    None => None,
  };
  // nodes on something solid can carry loads
  let mut solid = vec![false; grid.nodes()];
  for (e, c) in cells.iter().enumerate() {
    if *c != Cell::Void {
      for n in grid.elem_nodes(e) {
        solid[n] = true;
      }
    }
  }
  let mut cases = Vec::new();
  let mut picked = Vec::new();
  for case in &st.cases {
    let mut out = fox_gen::LoadCase { name: case.name.clone(), ..Default::default() };
    let mut pk = (Vec::new(), Vec::new());
    for l in &case.loads {
      let (nodes, area_force) = nodes_of(s, &grid, &solid, &l.target)?;
      let total = match &l.kind {
        LoadKind::Force { force } => *force,
        LoadKind::Pressure { pressure } => area_force.map(|c| -c * pressure),
      };
      pk.0.push(nodes.len());
      for n in &nodes {
        out.forces.push((*n, total.map(|c| c / nodes.len() as f64)));
      }
    }
    for k in &case.constraints {
      let (nodes, _) = nodes_of(s, &grid, &solid, &k.target)?;
      pk.1.push(nodes.len());
      out.fixed.extend(nodes.iter().map(|n| (*n, k.fix)));
    }
    if case.loads.is_empty() {
      warnings.push(format!("load case '{}' has no loads", case.name));
    }
    if case.constraints.is_empty() {
      warnings.push(format!("load case '{}' has no constraints", case.name));
    }
    cases.push(out);
    picked.push(pk);
  }
  Ok(Built { grid, cells, start, cases, symmetry, warnings, picked })
}

/// Grid nodes of a target (on solid elements), and for a face its area-weighted outward normal
/// (the force of a unit pressure pulling outward).
fn nodes_of(s: &Session, g: &Grid, solid: &[bool], t: &Target) -> Result<(Vec<usize>, [f64; 3]), CmdError> {
  let keep = |v: Vec<usize>| v.into_iter().filter(|n| solid[*n]).collect::<Vec<_>>();
  let nearest = |at: [f64; 3]| -> Option<usize> {
    (0..g.nodes()).filter(|n| solid[*n]).min_by(|a, b| {
      let d = |n: usize| {
        let p = g.node_pos(n);
        (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f64>()
      };
      d(*a).total_cmp(&d(*b))
    })
  };
  match t {
    Target::Point { at, radius } => {
      let mut v = keep(g.nodes_within(*at, radius.max(0.75 * g.h)));
      if v.is_empty() {
        v.extend(nearest(*at));
      }
      Ok((v, [0.0; 3]))
    }
    Target::Face { face } => {
      let b = s.model.body(face.body).ok_or_else(|| CmdError::new(format!("body {} no longer exists", face.body.0)))?;
      let m = b.mesh().ok_or(CmdError::new("a load's body can't be meshed"))?;
      let id = face.resolve(b.faces(), Some(m), b.diagonal()).ok_or(CmdError::new("a load's face can't be found"))?;
      let mut idx = Vec::new();
      let mut an = [0.0; 3];
      let mut centre = [0.0; 3];
      let mut area = 0.0;
      for (t, f) in m.indices.chunks_exact(3).zip(&m.tri_face) {
        if *f != id {
          continue;
        }
        idx.extend_from_slice(t);
        let p = |i: u32| m.positions[i as usize].map(f64::from);
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let (u, v) = ([0, 1, 2].map(|i| b[i] - a[i]), [0, 1, 2].map(|i| c[i] - a[i]));
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let ar = 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        for i in 0..3 {
          an[i] += 0.5 * n[i];
          centre[i] += ar * (a[i] + b[i] + c[i]) / 3.0;
        }
        area += ar;
      }
      let mut v = keep(g.nodes_near(&m.positions, &idx, 0.6 * g.h));
      if v.is_empty() && area > 0.0 {
        v.extend(nearest(centre.map(|c| c / area)));
      }
      Ok((v, an))
    }
  }
}

/// The solves a study needs: every material with every manufacturing method.
pub fn jobs(s: &Session, study: Option<u32>) -> Result<Vec<Job>, CmdError> {
  let i = study_of(s, &study.map(|id| json!({"study": id})).unwrap_or(json!({})))?;
  let st = &s.design.generative[i];
  let b = build(s, st)?;
  if b.cases.iter().all(|c| c.forces.is_empty()) {
    return Err(CmdError::new("the study has no loads: gen.load first"));
  }
  let materials = if st.materials.is_empty() {
    vec![fox_gen::material("Aluminum AlSi10Mg").unwrap_or_else(|| fox_gen::materials()[0].clone())]
  } else {
    st.materials.clone()
  };
  let methods = if st.methods.is_empty() { vec![Method::Unrestricted] } else { st.methods.clone() };
  let objective = match st.objective {
    Goal::MaxStiffness => fox_gen::Objective::MaxStiffness { volume_fraction: st.mass_target },
    Goal::MinMass => fox_gen::Objective::MinMass { start: st.mass_target.max(0.4) },
  };
  let settings = fox_gen::Settings {
    filter_radius: st.min_member / 2.0,
    max_iterations: st.iterations,
    ..fox_gen::Settings::default()
  };
  let mut out = Vec::new();
  for m in &materials {
    for method in &methods {
      out.push(Job {
        study: st.id,
        problem: Problem {
          grid: b.grid,
          cells: b.cells.clone(),
          start: b.start.clone(),
          cases: b.cases.clone(),
          material: m.clone(),
          objective,
          limits: fox_gen::Limits { safety_factor: st.safety_factor, max_displacement: st.max_displacement },
          method: method.clone(),
          symmetry: b.symmetry,
          settings,
        },
      });
    }
  }
  Ok(out)
}

/// Stores solved jobs as the study's outcomes (replacing the old ones), in one undo step.
pub fn finish(s: &mut Session, results: Vec<(Job, fox_gen::Outcome)>) -> CmdResult {
  let Some(study) = results.first().map(|(j, _)| j.study) else {
    return Err(CmdError::new("nothing was generated"));
  };
  let i = s.design.generative.iter().position(|x| x.id == study).ok_or(CmdError::new("the study is gone"))?;
  let outcomes: Vec<Outcome> = results
    .into_iter()
    .enumerate()
    .map(|(k, (job, o))| {
      let p = job.problem;
      let mesh = fox_gen::surface::extract(&p.grid, &o.density, ISO, SMOOTHING);
      let mesh = fox_gen::surface::drop_islands(&mesh, 0.01);
      let volume = mesh.volume();
      Outcome {
        id: s.design.generative[i].next_outcome + 1 + k as u32,
        mass: volume * p.material.density / 1000.0,
        material: p.material,
        method: p.method,
        volume,
        volume_fraction: o.volume_fraction,
        compliance: o.compliance,
        max_displacement: o.max_displacement,
        peak_stress: o.peak_stress,
        safety_factor: o.safety_factor,
        meets_limits: o.meets_limits,
        converged: o.converged,
        iterations: o.iterations,
        grid: p.grid,
        density: Arc::new(o.density),
        mesh: Arc::new(mesh),
      }
    })
    .collect();
  let n = outcomes.len() as u32;
  s.edit(|d| {
    let st = &mut d.generative[i];
    st.outcomes = outcomes;
    st.next_outcome += n;
    Ok(())
  })?;
  Ok(json!({"study": study, "outcomes": outcomes_json(&s.design.generative[i])}))
}

fn generate(s: &mut Session, p: &Value) -> CmdResult {
  let i = study_of(s, p)?;
  let jobs = jobs(s, Some(s.design.generative[i].id))?;
  let mut results = Vec::new();
  for job in jobs {
    let o = fox_gen::solve(&job.problem, &Progress::default()).map_err(|e| CmdError::new(e.0))?;
    results.push((job, o));
  }
  finish(s, results)
}

fn precheck(s: &mut Session, p: &Value) -> CmdResult {
  let i = study_of(s, p)?;
  let st = &s.design.generative[i];
  let b = build(s, st)?;
  let count = |c: Cell| b.cells.iter().filter(|x| **x == c).count();
  let mut warnings = b.warnings.clone();
  for (case, (loads, fixed)) in st.cases.iter().zip(&b.picked) {
    if loads.contains(&0) {
      warnings.push(format!("a load of '{}' touches no element: move it onto preserve geometry", case.name));
    }
    if fixed.contains(&0) {
      warnings.push(format!("a constraint of '{}' touches no element", case.name));
    }
  }
  Ok(json!({
    "study": st.id,
    "grid": b.grid.n, "element_size": b.grid.h, "origin": b.grid.origin,
    "elements": b.grid.elements(), "design": count(Cell::Design), "preserve": count(Cell::Preserve),
    "void": count(Cell::Void),
    "cases": st.cases.iter().zip(&b.picked).map(|(c, (l, k))| json!({"name": c.name, "load_nodes": l, "constraint_nodes": k})).collect::<Vec<_>>(),
    "outcomes_to_generate": st.materials.len().max(1) * st.methods.len().max(1),
    "ready": warnings.is_empty(),
    "warnings": warnings,
  }))
}

/// The last study's inputs (what `gen.info` returns), without running a command.
pub fn info(s: &Session) -> Option<Value> {
  s.design.generative.last().map(study_json)
}

/// The last study's outcomes (what `gen.outcomes` returns).
pub fn outcomes(s: &Session) -> Vec<Value> {
  s.design.generative.last().map(|st| st.outcomes.iter().map(outcome_json).collect()).unwrap_or_default()
}

fn outcome_of<'a>(s: &'a Session, p: &Value) -> Result<(&'a Study, &'a Outcome), CmdError> {
  let st = &s.design.generative[study_of(s, p)?];
  let o = match p.get("outcome").and_then(Value::as_u64) {
    Some(id) => {
      st.outcomes.iter().find(|o| o.id as u64 == id).ok_or_else(|| CmdError::new(format!("no outcome {id}")))?
    }
    // the stiffest that meets the limits
    None => st
      .outcomes
      .iter()
      .filter(|o| o.meets_limits)
      .min_by(|a, b| a.mass.total_cmp(&b.mass))
      .or_else(|| st.outcomes.first())
      .ok_or(CmdError::new("the study has no outcomes: gen.generate first"))?,
  };
  Ok((st, o))
}

fn insert(s: &mut Session, p: &Value) -> CmdResult {
  let (st, o) = outcome_of(s, p)?;
  let brep = p.get("brep").and_then(Value::as_bool).unwrap_or(false);
  let merge = p.get("merge_preserve").and_then(Value::as_bool).unwrap_or(false);
  let keep = p.get("reduce").and_then(num).filter(|k| *k > 0.0 && *k < 1.0);
  let target = match (keep, p.get("triangles").and_then(Value::as_u64)) {
    (_, Some(t)) => Some((t as usize).max(100)),
    (Some(k), None) => Some(((o.mesh.triangles() as f64 * k) as usize).max(100)),
    _ => None,
  };
  let reduced = target.map(|t| fox_gen::surface::decimate(&o.mesh, t));
  let surface = reduced.as_ref().unwrap_or(&o.mesh);
  let (preserve, obstacles) = (st.preserve.clone(), st.obstacles.clone());
  let name = format!("{} outcome {} ({})", st.name, o.id, o.material.name);
  let mesh = MeshData {
    name: name.clone(),
    positions: surface.positions.iter().flatten().copied().collect(),
    indices: surface.indices.clone(),
    merge_coplanar: false,
  };
  let triangles = surface.triangles();
  let label = name.clone();
  s.begin_transaction()?;
  let r = (|| {
    let id = s.edit(|d| {
      Ok(d.add_feature(FeatureKind::Import { file_name: label, breps: Vec::new(), meshes: vec![mesh] })?)
    })?;
    if let Some(e) = s.model.status(id).and_then(|st| st.error.clone()) {
      return Err(CmdError::new(format!("the outcome's mesh didn't make a body: {e}")));
    }
    let body =
      s.model.bodies.iter().find(|b| b.feature == id).map(|b| b.id).ok_or(CmdError::new("no body was made"))?;
    let mut steps = vec!["mesh body"];
    if reduced.is_some() {
      steps.push("reduce");
    }
    let mut feature = |s: &mut Session, kind: FeatureKind, what: &'static str| -> Result<(), CmdError> {
      let f = s.edit(|d| Ok(d.add_feature(kind)?))?;
      if let Some(e) = s.model.status(f).and_then(|st| st.error.clone()) {
        return Err(CmdError::new(format!("{what} failed: {e}")));
      }
      steps.push(what);
      Ok(())
    };
    if brep || merge {
      feature(s, FeatureKind::Mesh { bodies: vec![body], op: MeshOp::Convert, amount: "1".into() }, "B-rep")?;
    }
    if merge {
      let live = |v: &[BodyId]| v.iter().copied().filter(|b| s.model.body(*b).is_some()).collect::<Vec<_>>();
      let (pre, obs) = (live(&preserve), live(&obstacles));
      if !pre.is_empty() {
        feature(
          s,
          FeatureKind::Combine { target: body, tools: pre, operation: Operation::Join, keep_tools: true },
          "join preserve",
        )?;
      }
      if !obs.is_empty() {
        feature(
          s,
          FeatureKind::Combine { target: body, tools: obs, operation: Operation::Cut, keep_tools: true },
          "cut obstacles",
        )?;
      }
    }
    let b = s.model.body(body).ok_or(CmdError::new("the outcome's body is gone"))?;
    let mass = b.shape.mass().ok();
    Ok(json!({
      "body": body.0, "name": name, "steps": steps,
      "volume": mass.as_ref().map(|m| m.volume), "faces": b.faces().len(), "triangles": triangles,
    }))
  })();
  match r {
    Ok(v) => {
      s.commit_transaction()?;
      Ok(v)
    }
    Err(e) => {
      s.rollback_transaction()?;
      Err(e)
    }
  }
}

fn export(s: &mut Session, p: &Value) -> CmdResult {
  let (_, o) = outcome_of(s, p)?;
  let path = std::path::PathBuf::from(opt_str(p, "path").ok_or(CmdError::new("'path' is required"))?);
  let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
  let mesh = fox_exchange::Mesh {
    name: format!("outcome {}", o.id),
    positions: o.mesh.positions.clone(),
    normals: Vec::new(),
    indices: o.mesh.indices.clone(),
  };
  let bytes = fox_exchange::write_mesh(&ext, &[mesh]).map_err(|e| CmdError::new(e.to_string()))?;
  std::fs::write(&path, bytes)?;
  Ok(json!({"path": path.display().to_string(), "triangles": o.mesh.triangles()}))
}
