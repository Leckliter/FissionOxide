//! Replays a design's timeline through the kernel.
//!
//! The state after every feature is cached under a hash of everything that led to it, so editing
//! feature *k* only recomputes from *k* on, and re-running an unchanged design is free.

use std::cell::OnceCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use clf_kernel::glam::{DMat4, DVec3};
use clf_kernel::{BoolOp, EdgeInfo, FaceInfo, KernelError, Plane, Shape, TriMesh};
use clf_sketch::{ConstraintId, CurveKind, Profile, SolveReport, find_profiles};

use crate::{
  AxisRef, BodyId, BodySel, Design, EdgeRef, ExtentKind, FaceRef, Feature, FeatureId, FeatureKind, Operation,
  ParamKind, PlaneRef, Primitive,
};

/// New bodies one feature may create.
const MAX_BODIES_PER_FEATURE: u32 = 1000;
/// Pattern instance cap.
const MAX_INSTANCES: usize = 500;

/// A solid body produced by the timeline.
#[derive(Clone)]
pub struct Body {
  pub id: BodyId,
  pub name: String,
  pub shape: Shape,
  /// Feature that last changed this body.
  pub feature: FeatureId,
  mesh: OnceCell<Option<TriMesh>>,
  faces: OnceCell<Vec<FaceInfo>>,
  edges: OnceCell<Vec<EdgeInfo>>,
}

impl std::fmt::Debug for Body {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Body({:?} {})", self.id, self.name)
  }
}

impl Body {
  pub fn new(id: BodyId, name: String, shape: Shape, feature: FeatureId) -> Body {
    Body { id, name, shape, feature, mesh: OnceCell::new(), faces: OnceCell::new(), edges: OnceCell::new() }
  }

  fn with_shape(&self, shape: Shape, feature: FeatureId) -> Body {
    Body::new(self.id, self.name.clone(), shape, feature)
  }

  /// Display tessellation (cached).
  pub fn mesh(&self) -> Option<&TriMesh> {
    self
      .mesh
      .get_or_init(|| {
        let tol = (self.diagonal() * 0.0015).clamp(0.005, 0.5);
        self.shape.tessellate(tol).ok()
      })
      .as_ref()
  }

  pub fn faces(&self) -> &[FaceInfo] {
    self.faces.get_or_init(|| self.shape.faces().unwrap_or_default())
  }

  pub fn edges(&self) -> &[EdgeInfo] {
    self.edges.get_or_init(|| self.shape.edges().unwrap_or_default())
  }

  pub fn diagonal(&self) -> f64 {
    self.shape.mass().map(|m| (m.bbox_max - m.bbox_min).length()).unwrap_or(1.0)
  }

  fn bbox(&self) -> Option<(DVec3, DVec3)> {
    self.shape.mass().ok().map(|m| (m.bbox_min, m.bbox_max))
  }
}

/// A sketch after solving.
#[derive(Debug, Clone)]
pub struct SketchResult {
  pub plane: Plane,
  /// The solved sketch.
  pub sketch: clf_sketch::Sketch,
  pub profiles: Vec<Profile>,
  pub report: SolveReport,
  /// Dimensions whose expressions failed to evaluate.
  pub dim_errors: Vec<(ConstraintId, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct FeatureStatus {
  pub id: u32,
  pub name: String,
  pub error: Option<String>,
  pub warning: Option<String>,
  pub rolled_back: bool,
  pub suppressed: bool,
}

#[derive(Clone, Default)]
struct State {
  bodies: Vec<Body>,
  sketches: BTreeMap<FeatureId, SketchResult>,
  planes: BTreeMap<FeatureId, Plane>,
  body_counter: u32,
}

/// The regenerated design.
#[derive(Clone, Default)]
pub struct Model {
  pub bodies: Vec<Body>,
  pub sketches: BTreeMap<FeatureId, SketchResult>,
  pub planes: BTreeMap<FeatureId, Plane>,
  /// Status per feature, in timeline order.
  pub status: Vec<FeatureStatus>,
  /// Evaluated parameter values (mm / rad).
  pub params: HashMap<String, f64>,
  pub param_errors: HashMap<String, String>,
}

impl Model {
  pub fn body(&self, id: BodyId) -> Option<&Body> {
    self.bodies.iter().find(|b| b.id == id)
  }

  pub fn status(&self, id: FeatureId) -> Option<&FeatureStatus> {
    self.status.iter().find(|s| s.id == id.0)
  }

  /// Bounding box of every body.
  pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
    let mut acc: Option<(DVec3, DVec3)> = None;
    for b in &self.bodies {
      if let Some((lo, hi)) = b.bbox() {
        acc = Some(match acc {
          Some((a, c)) => (a.min(lo), c.max(hi)),
          None => (lo, hi),
        });
      }
    }
    acc
  }

  /// Errors anywhere in the timeline.
  pub fn errors(&self) -> Vec<String> {
    self.status.iter().filter_map(|s| s.error.as_ref().map(|e| format!("{}: {e}", s.name))).collect()
  }
}

#[derive(Default)]
pub struct Regenerator {
  cache: Vec<(u64, State)>,
}

/// Regenerates without a cache (tests, the CLI).
pub fn regenerate(design: &mut Design) -> Model {
  Regenerator::default().regenerate(design)
}

impl Regenerator {
  pub fn clear(&mut self) {
    self.cache.clear();
  }

  /// Replays the design. Sketch positions are written back after solving, and face/edge
  /// references are refreshed to the faces/edges they resolved to.
  pub fn regenerate(&mut self, design: &mut Design) -> Model {
    let (params, param_errors) = eval_params(design);
    let mut key = {
      let mut h = DefaultHasher::new();
      serde_json::to_string(&design.parameters).unwrap_or_default().hash(&mut h);
      h.finish()
    };
    let active = design.active_len();
    let mut state = State::default();
    let mut status = Vec::with_capacity(design.features.len());
    let mut valid_prefix = true;
    for i in 0..design.features.len() {
      let Some(feature) = design.features.get(i) else { break };
      let mut st = FeatureStatus { id: feature.id.0, name: feature.name.clone(), ..Default::default() };
      if i >= active {
        st.rolled_back = true;
        status.push(st);
        continue;
      }
      let pre_key = chain(key, feature);
      if valid_prefix
        && let Some((k, cached)) = self.cache.get(i)
        && *k == pre_key
      {
        state = cached.clone();
        key = pre_key;
        st.suppressed = feature.suppressed;
        status.push(restore_status(st, &state, feature));
        continue;
      }
      valid_prefix = false;
      self.cache.truncate(i);
      st.suppressed = feature.suppressed;
      if !feature.suppressed {
        let mut ctx = Ctx { params: &params, state: &mut state };
        let id = feature.id;
        let mut kind = feature.kind.clone();
        match ctx.apply(id, &mut kind) {
          Ok(w) => st.warning = w,
          Err(e) => st.error = Some(e),
        }
        if let Some(f) = design.features.get_mut(i) {
          f.kind = kind;
        }
      }
      let Some(feature) = design.features.get(i) else { break };
      key = chain(key, feature);
      self.cache.push((key, state.clone()));
      if let Some(e) = &st.error {
        log::info!("{} failed: {e}", st.name);
      }
      status.push(st);
    }
    // cached statuses must carry their warnings/errors too
    Model { bodies: state.bodies, sketches: state.sketches, planes: state.planes, status, params, param_errors }
  }
}

fn chain(prev: u64, f: &Feature) -> u64 {
  let mut h = DefaultHasher::new();
  prev.hash(&mut h);
  serde_json::to_string(f).unwrap_or_default().hash(&mut h);
  h.finish()
}

/// Rebuilds a cached feature's status from the cached state (sketch warnings).
fn restore_status(mut st: FeatureStatus, state: &State, f: &Feature) -> FeatureStatus {
  if let Some(err) = state_error(state, f) {
    st.error = Some(err);
  }
  if let FeatureKind::Sketch { .. } = f.kind
    && let Some(r) = state.sketches.get(&f.id)
  {
    st.warning = sketch_warning(r);
  }
  st
}

fn state_error(state: &State, f: &Feature) -> Option<String> {
  match &f.kind {
    FeatureKind::Sketch { .. } if !f.suppressed && !state.sketches.contains_key(&f.id) => {
      Some("the sketch plane could not be found".into())
    }
    _ => None,
  }
}

fn sketch_warning(r: &SketchResult) -> Option<String> {
  if !r.dim_errors.is_empty() {
    return Some(format!("{} dimension(s) have invalid expressions", r.dim_errors.len()));
  }
  if !r.report.ok() {
    return Some("the sketch constraints conflict".into());
  }
  None
}

fn eval_params(design: &Design) -> (HashMap<String, f64>, HashMap<String, String>) {
  let list: Vec<(String, String, ParamKind)> =
    design.parameters.iter().map(|p| (p.name.clone(), p.expr.clone(), p.kind)).collect();
  let mut ok = HashMap::new();
  let mut err = HashMap::new();
  for (k, v) in clf_expr::eval_parameters(&list) {
    match v {
      Ok(v) => {
        ok.insert(k, v);
      }
      Err(e) => {
        err.insert(k, e.to_string());
      }
    }
  }
  (ok, err)
}

struct Ctx<'a> {
  params: &'a HashMap<String, f64>,
  state: &'a mut State,
}

type FResult = Result<Option<String>, String>;

impl Ctx<'_> {
  fn len(&self, e: &str) -> Result<f64, String> {
    clf_expr::eval(e, clf_expr::Kind::Length, self.params).map_err(|err| format!("'{e}': {err}"))
  }

  fn angle(&self, e: &str) -> Result<f64, String> {
    clf_expr::eval(e, clf_expr::Kind::Angle, self.params).map_err(|err| format!("'{e}': {err}"))
  }

  fn count(&self, e: &str) -> Result<usize, String> {
    let v = clf_expr::eval(e, clf_expr::Kind::Unitless, self.params).map_err(|err| format!("'{e}': {err}"))?;
    let n = v.round();
    if !(1.0..=MAX_INSTANCES as f64).contains(&n) {
      return Err(format!("count must be between 1 and {MAX_INSTANCES}"));
    }
    Ok(n as usize)
  }

  fn apply(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::Sketch { plane, sketch } => {
        let pl = self.resolve_plane(plane)?;
        let dim_errors: Vec<(ConstraintId, String)> =
          sketch.evaluate_dims(self.params).into_iter().map(|(c, e)| (c, e.to_string())).collect();
        let report = sketch.solve();
        let profiles = find_profiles(sketch);
        let r = SketchResult { plane: pl, sketch: sketch.clone(), profiles, report, dim_errors };
        let w = sketch_warning(&r);
        self.state.sketches.insert(id, r);
        Ok(w)
      }
      FeatureKind::Extrude { sketch, profiles, extent, operation } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (chosen, missing) = pick_profiles(&sk.profiles, profiles);
        if chosen.is_empty() {
          return Err(if sk.profiles.is_empty() {
            "the sketch has no closed profiles".into()
          } else {
            "the selected profiles no longer exist".into()
          });
        }
        let n = sk.plane.normal() * if extent.flip { -1.0 } else { 1.0 };
        let (start, length) = match extent.kind {
          ExtentKind::OneSide => (0.0, self.len(&extent.distance)?),
          ExtentKind::Symmetric => {
            let d = self.len(&extent.distance)?;
            (-d, 2.0 * d)
          }
          ExtentKind::TwoSides => {
            let (d1, d2) = (self.len(&extent.distance)?, self.len(&extent.distance2)?);
            (-d2, d1 + d2)
          }
          ExtentKind::ThroughAll => (0.0, self.through_all(&sk.plane)),
        };
        let mut tools = Vec::new();
        for p in &chosen {
          let face = Shape::face(&sk.plane, p).map_err(ke)?;
          let face = if start.abs() > 0.0 { face.translated(n * start).map_err(ke)? } else { face };
          tools.push(face.extrude(n * length).map_err(ke)?);
        }
        let tool = union_all(tools)?;
        self.operate(id, tool, *operation)?;
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::Revolve { sketch, profiles, axis, angle, operation } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (chosen, missing) = pick_profiles(&sk.profiles, profiles);
        if chosen.is_empty() {
          return Err("no profiles to revolve".into());
        }
        let (o, d) = self.resolve_axis(axis, Some(*sketch))?;
        let a = self.angle(angle)?;
        let mut tools = Vec::new();
        for p in &chosen {
          let face = Shape::face(&sk.plane, p).map_err(ke)?;
          tools.push(face.revolve(o, d, a).map_err(ke)?);
        }
        let tool = union_all(tools)?;
        self.operate(id, tool, *operation)?;
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::Fillet { edges, radius } => {
        let r = self.len(radius)?;
        self.edge_op(id, edges, |s, ids| s.fillet(ids, r))
      }
      FeatureKind::Chamfer { edges, distance } => {
        let d = self.len(distance)?;
        self.edge_op(id, edges, |s, ids| s.chamfer(ids, d))
      }
      FeatureKind::Shell { faces, thickness } => {
        let t = self.len(thickness)?;
        let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
        for (i, f) in faces.iter().enumerate() {
          by_body.entry(f.body).or_default().push(i);
        }
        if by_body.is_empty() {
          return Err("select faces to remove".into());
        }
        let mut missing = false;
        for (bid, idxs) in by_body {
          let bi = self.body_index(bid)?;
          let body = &self.state.bodies[bi];
          let scale = body.diagonal();
          let mut ids = Vec::new();
          for i in idxs {
            let Some(fr) = faces.get_mut(i) else { continue };
            match fr.resolve(body.faces(), body.mesh(), scale) {
              Some(fid) => {
                refresh_face(fr, body, fid);
                ids.push(fid);
              }
              None => missing = true,
            }
          }
          let shape = body.shape.shell(&ids, t).map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(missing.then(|| "some faces could not be found".to_string()))
      }
      FeatureKind::Primitive { shape, plane, at, operation } => {
        let pl = self.resolve_plane(plane)?;
        let local = match shape {
          Primitive::Box { width, depth, height } => {
            let (w, d, h) = (self.len(width)?, self.len(depth)?, self.len(height)?);
            Shape::make_box(DVec3::new(-w / 2.0, -d / 2.0, 0.0), DVec3::new(w / 2.0, d / 2.0, h)).map_err(ke)?
          }
          Primitive::Cylinder { radius, height } => {
            Shape::cylinder(DVec3::ZERO, DVec3::Z, self.len(radius)?, self.len(height)?).map_err(ke)?
          }
          Primitive::Sphere { radius } => Shape::sphere(DVec3::ZERO, self.len(radius)?).map_err(ke)?,
          Primitive::Torus { major, minor } => {
            Shape::torus(DVec3::ZERO, DVec3::Z, self.len(major)?, self.len(minor)?).map_err(ke)?
          }
        };
        let m = plane_matrix(&pl) * DMat4::from_translation(DVec3::new(at[0], at[1], 0.0));
        let tool = local.transformed(m).map_err(ke)?;
        self.operate(id, tool, *operation)?;
        Ok(None)
      }
      FeatureKind::ConstructionPlane { base, offset } => {
        let pl = self.resolve_plane(base)?;
        let d = self.len(offset)?;
        self.state.planes.insert(id, pl.offset(d));
        Ok(None)
      }
      FeatureKind::Mirror { bodies, plane, join } => {
        let pl = self.resolve_plane(plane)?;
        let targets = self.select(bodies)?;
        for bi in targets {
          let body = self.state.bodies[bi].clone();
          let copy = body.shape.mirrored(pl.origin, pl.normal()).map_err(ke)?;
          self.add_copies(id, bi, vec![copy], *join)?;
        }
        Ok(None)
      }
      FeatureKind::RectangularPattern { bodies, direction, count, spacing, direction2, count2, spacing2, join } => {
        let d1 = DVec3::from(*direction).try_normalize().ok_or("the pattern direction is zero")?;
        let (n1, s1) = (self.count(count)?, self.len(spacing)?);
        let (n2, s2, d2) = match direction2 {
          Some(d) => {
            (self.count(count2)?, self.len(spacing2)?, DVec3::from(*d).try_normalize().ok_or("direction 2 is zero")?)
          }
          None => (1, 0.0, DVec3::ZERO),
        };
        if n1 * n2 > MAX_INSTANCES {
          return Err(format!("at most {MAX_INSTANCES} instances"));
        }
        for bi in self.select(bodies)? {
          let body = self.state.bodies[bi].clone();
          let mut copies = Vec::new();
          for i in 0..n1 {
            for j in 0..n2 {
              if i == 0 && j == 0 {
                continue;
              }
              let v = d1 * s1 * i as f64 + d2 * s2 * j as f64;
              copies.push(body.shape.translated(v).map_err(ke)?);
            }
          }
          self.add_copies(id, bi, copies, *join)?;
        }
        Ok(None)
      }
      FeatureKind::CircularPattern { bodies, axis, count, angle, join } => {
        let (o, d) = self.resolve_axis(axis, None)?;
        let n = self.count(count)?;
        let total = self.angle(angle)?;
        let full = (total.abs() - std::f64::consts::TAU).abs() < 1e-9;
        let step = if full || n < 2 { total / n as f64 } else { total / (n - 1) as f64 };
        for bi in self.select(bodies)? {
          let body = self.state.bodies[bi].clone();
          let copies: Result<Vec<Shape>, String> =
            (1..n).map(|k| body.shape.rotated(o, d, step * k as f64).map_err(ke)).collect();
          self.add_copies(id, bi, copies?, *join)?;
        }
        Ok(None)
      }
      FeatureKind::Move { bodies, translate, axis, angle } => {
        let v = DVec3::new(self.len(&translate[0])?, self.len(&translate[1])?, self.len(&translate[2])?);
        let rot = match axis {
          Some(a) => Some((self.resolve_axis(a, None)?, self.angle(angle)?)),
          None => None,
        };
        for bi in self.select(bodies)? {
          let mut s = self.state.bodies[bi].shape.translated(v).map_err(ke)?;
          if let Some(((o, d), a)) = rot {
            s = s.rotated(o, d, a).map_err(ke)?;
          }
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
        }
        Ok(None)
      }
      FeatureKind::Combine { target, tools, operation, keep_tools } => {
        let ti = self.body_index(*target)?;
        let mut shape = self.state.bodies[ti].shape.clone();
        let op = match operation {
          Operation::Join | Operation::NewBody => BoolOp::Union,
          Operation::Cut => BoolOp::Subtract,
          Operation::Intersect => BoolOp::Intersect,
        };
        for t in tools.iter() {
          if t == target {
            continue;
          }
          let i = self.body_index(*t)?;
          shape = shape.boolean(&self.state.bodies[i].shape, op).map_err(ke)?;
        }
        let new_target = self.state.bodies[ti].with_shape(shape, id);
        self.state.bodies[ti] = new_target;
        if !*keep_tools {
          self.state.bodies.retain(|b| b.id == *target || !tools.contains(&b.id));
        }
        self.split_multi(id, *target);
        Ok(None)
      }
      FeatureKind::Import { breps, meshes, .. } => {
        let mut shapes = Vec::new();
        for b in breps.iter() {
          shapes.push(Shape::from_brep(b).map_err(ke)?);
        }
        for m in meshes.iter() {
          let pos: Vec<[f32; 3]> = m.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
          shapes.push(Shape::from_mesh(&pos, &m.indices, m.merge_coplanar).map_err(ke)?);
        }
        for s in shapes {
          for body in s.bodies().map_err(ke)? {
            self.new_body(id, body);
          }
        }
        Ok(None)
      }
      FeatureKind::RemoveBodies { bodies } => {
        self.state.bodies.retain(|b| !bodies.contains(&b.id));
        Ok(None)
      }
    }
  }

  // ---- references ----

  fn resolve_plane(&mut self, p: &mut PlaneRef) -> Result<Plane, String> {
    match p {
      PlaneRef::Origin { plane } => Ok(plane.plane()),
      PlaneRef::Construction { feature } => {
        self.state.planes.get(feature).copied().ok_or_else(|| "its construction plane failed".to_string())
      }
      PlaneRef::Face { face } => {
        let bi = self.body_index(face.body)?;
        let body = &self.state.bodies[bi];
        let fid = face.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or("the face could not be found")?;
        let info = body.faces().iter().find(|f| f.id == fid).copied().ok_or("the face could not be found")?;
        refresh_face(face, body, fid);
        if info.kind != clf_kernel::SurfaceKind::Plane {
          return Err("the face is not planar".into());
        }
        sketch_plane_on(info.normal, info.origin)
      }
    }
  }

  fn resolve_axis(&mut self, a: &mut AxisRef, sketch: Option<FeatureId>) -> Result<(DVec3, DVec3), String> {
    match a {
      AxisRef::X => Ok((DVec3::ZERO, DVec3::X)),
      AxisRef::Y => Ok((DVec3::ZERO, DVec3::Y)),
      AxisRef::Z => Ok((DVec3::ZERO, DVec3::Z)),
      AxisRef::SketchLine { curve } => {
        let sid = sketch.ok_or("a sketch line axis only works in a revolve")?;
        let sk = self.state.sketches.get(&sid).ok_or("its sketch failed")?;
        let Some(CurveKind::Line { a, b }) = sk.sketch.curve(*curve).map(|c| &c.kind) else {
          return Err("the axis line no longer exists".into());
        };
        let (Some(pa), Some(pb)) = (sk.sketch.pos(*a), sk.sketch.pos(*b)) else {
          return Err("the axis line no longer exists".into());
        };
        let (pa, pb) = (sk.plane.to_world(pa), sk.plane.to_world(pb));
        Ok((pa, (pb - pa).try_normalize().ok_or("the axis line has no length")?))
      }
      AxisRef::Edge { edge } => {
        let bi = self.body_index(edge.body)?;
        let body = &self.state.bodies[bi];
        let eid =
          edge.resolve(body.edges(), body.faces(), body.diagonal()).ok_or("the axis edge could not be found")?;
        let e = body.edges().iter().find(|e| e.id == eid).copied().ok_or("the axis edge could not be found")?;
        *edge = EdgeRef::from_edge(edge.body, &e, body.faces());
        match e.kind {
          clf_kernel::CurveKind::Line => Ok((e.start, (e.end - e.start).normalize_or_zero())),
          clf_kernel::CurveKind::Circle => Ok((e.center, e.axis)),
          _ => Err("the axis edge is not straight or circular".into()),
        }
      }
    }
  }

  fn body_index(&self, id: BodyId) -> Result<usize, String> {
    self.state.bodies.iter().position(|b| b.id == id).ok_or_else(|| format!("body {} no longer exists", id.0))
  }

  fn select(&self, sel: &BodySel) -> Result<Vec<usize>, String> {
    match sel {
      BodySel::All => Ok((0..self.state.bodies.len()).collect()),
      BodySel::Bodies { ids } => ids.iter().map(|id| self.body_index(*id)).collect(),
    }
  }

  fn through_all(&self, plane: &Plane) -> f64 {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for b in &self.state.bodies {
      if let Some((a, c)) = b.bbox() {
        lo = lo.min(a);
        hi = hi.max(c);
      }
    }
    if !lo.is_finite() {
      return 100.0;
    }
    let center = (lo + hi) / 2.0;
    2.0 * ((hi - lo).length() + (center - plane.origin).length()) + 1.0
  }

  // ---- body bookkeeping ----

  fn new_body(&mut self, feature: FeatureId, shape: Shape) -> BodyId {
    let created = self.state.bodies.iter().filter(|b| b.id.0 / MAX_BODIES_PER_FEATURE == feature.0).count() as u32;
    let mut k = created;
    let id = loop {
      let cand = BodyId(feature.0.saturating_mul(MAX_BODIES_PER_FEATURE).saturating_add(k % MAX_BODIES_PER_FEATURE));
      if !self.state.bodies.iter().any(|b| b.id == cand) || k > 2 * MAX_BODIES_PER_FEATURE {
        break cand;
      }
      k += 1;
    };
    self.state.body_counter += 1;
    let name = format!("Body{}", self.state.body_counter);
    self.state.bodies.push(Body::new(id, name, shape, feature));
    id
  }

  /// Splits a body whose shape holds several solids into one body per solid.
  fn split_multi(&mut self, feature: FeatureId, id: BodyId) {
    let Some(i) = self.state.bodies.iter().position(|b| b.id == id) else { return };
    let Ok(solids) = self.state.bodies[i].shape.solids() else { return };
    if solids.len() <= 1 {
      return;
    }
    let mut it = solids.into_iter();
    if let Some(first) = it.next() {
      self.state.bodies[i] = self.state.bodies[i].with_shape(first, feature);
    }
    for s in it {
      self.new_body(feature, s);
    }
  }

  /// Applies a new/join/cut/intersect operation with a tool solid.
  fn operate(&mut self, feature: FeatureId, tool: Shape, op: Operation) -> Result<(), String> {
    let tool_box = tool.mass().map(|m| (m.bbox_min, m.bbox_max)).map_err(ke)?;
    let overlapping: Vec<usize> = (0..self.state.bodies.len())
      .filter(|&i| self.state.bodies[i].bbox().is_some_and(|b| boxes_touch(b, tool_box)))
      .collect();
    match op {
      Operation::NewBody => {
        for s in tool.solids().map_err(ke)? {
          self.new_body(feature, s);
        }
      }
      Operation::Join => {
        if overlapping.is_empty() {
          for s in tool.solids().map_err(ke)? {
            self.new_body(feature, s);
          }
          return Ok(());
        }
        let mut acc = tool;
        for &i in &overlapping {
          acc = self.state.bodies[i].shape.boolean(&acc, BoolOp::Union).map_err(ke)?;
        }
        let keep = overlapping[0];
        let keep_id = self.state.bodies[keep].id;
        self.state.bodies[keep] = self.state.bodies[keep].with_shape(acc, feature);
        let drop: Vec<BodyId> = overlapping.iter().skip(1).map(|&i| self.state.bodies[i].id).collect();
        self.state.bodies.retain(|b| !drop.contains(&b.id));
        self.split_multi(feature, keep_id);
      }
      Operation::Cut | Operation::Intersect => {
        let bop = if op == Operation::Cut { BoolOp::Subtract } else { BoolOp::Intersect };
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for &i in &overlapping {
          match self.state.bodies[i].shape.boolean(&tool, bop) {
            Ok(s) => {
              self.state.bodies[i] = self.state.bodies[i].with_shape(s, feature);
              changed.push(self.state.bodies[i].id);
            }
            Err(KernelError::Empty) => removed.push(self.state.bodies[i].id),
            Err(e) => return Err(e.to_string()),
          }
        }
        if op == Operation::Intersect && overlapping.is_empty() {
          return Err("the result is empty".into());
        }
        if overlapping.is_empty() {
          return Err("nothing to cut".into());
        }
        self.state.bodies.retain(|b| !removed.contains(&b.id));
        for id in changed {
          self.split_multi(feature, id);
        }
      }
    }
    Ok(())
  }

  /// Adds pattern/mirror copies of body `bi`: joined into it, or as new bodies.
  fn add_copies(&mut self, feature: FeatureId, bi: usize, copies: Vec<Shape>, join: bool) -> Result<(), String> {
    if join {
      let mut acc = self.state.bodies[bi].shape.clone();
      for c in &copies {
        acc = acc.boolean(c, BoolOp::Union).map_err(ke)?;
      }
      let id = self.state.bodies[bi].id;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(acc, feature);
      self.split_multi(feature, id);
    } else {
      for c in copies {
        self.new_body(feature, c);
      }
    }
    Ok(())
  }

  /// Fillet/chamfer: resolves edge references per body and applies `op`.
  fn edge_op(
    &mut self,
    feature: FeatureId,
    edges: &mut [EdgeRef],
    op: impl Fn(&Shape, &[u32]) -> clf_kernel::Result<Shape>,
  ) -> FResult {
    let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
      by_body.entry(e.body).or_default().push(i);
    }
    if by_body.is_empty() {
      return Err("select edges".into());
    }
    let mut missing = false;
    for (bid, idxs) in by_body {
      let bi = self.body_index(bid)?;
      let body = &self.state.bodies[bi];
      let scale = body.diagonal();
      let mut ids = Vec::new();
      for i in idxs {
        let Some(er) = edges.get_mut(i) else { continue };
        match er.resolve(body.edges(), body.faces(), scale) {
          Some(eid) => {
            if let Some(info) = body.edges().iter().find(|e| e.id == eid) {
              *er = EdgeRef::from_edge(bid, info, body.faces());
            }
            if !ids.contains(&eid) {
              ids.push(eid);
            }
          }
          None => missing = true,
        }
      }
      if ids.is_empty() {
        return Err("the selected edges could not be found".into());
      }
      let shape = op(&body.shape, &ids).map_err(ke)?;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, feature);
    }
    Ok(missing.then(|| "some edges could not be found".to_string()))
  }
}

fn refresh_face(fr: &mut FaceRef, body: &Body, fid: u32) {
  if let Some(info) = body.faces().iter().find(|f| f.id == fid) {
    let p = DVec3::from(fr.point);
    let q = if info.kind == clf_kernel::SurfaceKind::Plane {
      p - info.normal * (p - info.origin).dot(info.normal)
    } else {
      p
    };
    *fr = FaceRef::from_face(fr.body, info, q);
  }
}

/// The sketch plane Fusion would put on a planar face: origin = the world origin projected onto
/// the face, axes aligned with the world (X right; up = Z for vertical faces).
pub fn sketch_plane_on(normal: DVec3, point: DVec3) -> Result<Plane, String> {
  let n = normal.try_normalize().ok_or("zero normal")?;
  let origin = n * point.dot(n);
  let (x, _y) = if n.z.abs() > 0.9 {
    (DVec3::X, DVec3::Y)
  } else {
    let up = (DVec3::Z - n * DVec3::Z.dot(n)).normalize_or_zero();
    (up.cross(n), up)
  };
  Plane::from_normal(origin, n, x).map_err(|e| e.to_string())
}

/// Local → world matrix of a plane (local Z = plane normal).
pub fn plane_matrix(p: &Plane) -> DMat4 {
  DMat4::from_cols(p.x.extend(0.0), p.y.extend(0.0), p.normal().extend(0.0), p.origin.extend(1.0))
}

fn pick_profiles(all: &[Profile], points: &[[f64; 2]]) -> (Vec<Profile>, bool) {
  if points.is_empty() {
    return (all.to_vec(), false);
  }
  let mut out: Vec<Profile> = Vec::new();
  let mut missing = false;
  for p in points {
    // smallest region containing the point
    let hit = all.iter().filter(|r| r.contains(*p)).min_by(|a, b| a.area().total_cmp(&b.area()));
    match hit {
      Some(r) if !out.contains(r) => out.push(r.clone()),
      Some(_) => {}
      None => missing = true,
    }
  }
  (out, missing)
}

fn union_all(mut tools: Vec<Shape>) -> Result<Shape, String> {
  let first = tools.drain(..1).next().ok_or("nothing to build")?;
  let mut acc = first;
  for t in tools {
    acc = acc.boolean(&t, BoolOp::Union).map_err(ke)?;
  }
  Ok(acc)
}

fn boxes_touch(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
  let eps = 1e-6;
  a.0.x <= b.1.x + eps
    && b.0.x <= a.1.x + eps
    && a.0.y <= b.1.y + eps
    && b.0.y <= a.1.y + eps
    && a.0.z <= b.1.z + eps
    && b.0.z <= a.1.z + eps
}

fn ke(e: KernelError) -> String {
  e.to_string()
}

#[cfg(test)]
mod tests;
