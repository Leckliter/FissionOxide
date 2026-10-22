//! Replays a design's timeline through the kernel.
//!
//! The state after every feature is cached under a hash of everything that led to it, so editing
//! feature *k* only recomputes from *k* on, and re-running an unchanged design is free.

use std::cell::OnceCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use fox_kernel::glam::{DMat4, DVec3};
use fox_kernel::{BoolOp, EdgeInfo, FaceInfo, KernelError, Plane, Shape, TriMesh};
use fox_sketch::behaviour::REVOLVE_AXIS_UNDIRECTED;
use fox_sketch::{ConstraintId, CurveKind, Profile, SolveReport, find_profiles};

use crate::ThinSide;
use crate::assembly;
use crate::naming;
use crate::sheet::{self, FlatPattern, SheetInfo, Unfolded};
use crate::{
  AxisRef, BendPosition, BodyId, BodySel, CornerMode, Design, EdgeRef, ExtentKind, FaceRef, Feature, FeatureId,
  FeatureKind, Frames, HoleKind, Operation, ParamKind, PlaneRef, Primitive, ReliefShape,
};

/// New bodies one feature may create.
pub(crate) const MAX_BODIES_PER_FEATURE: u32 = 1000;
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
  /// Set on sheet metal bodies.
  pub sheet: Option<SheetInfo>,
  /// Set on unfolded sheets (for Refold).
  pub unfolded: Option<Box<Unfolded>>,
  /// Its faces' names (see [`crate::naming`]); the shape carries them as labels.
  names: naming::Shared,
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
  /// A body made by `feature`, its faces named from the shape's history.
  pub fn new(id: BodyId, name: String, shape: Shape, feature: FeatureId) -> Body {
    Body::named(id, name, shape, feature, None)
  }

  /// [`Body::new`]; faces the history can't name take the names of the faces of `before` they are.
  fn named(id: BodyId, name: String, mut shape: Shape, feature: FeatureId, before: Option<&Body>) -> Body {
    let names = naming::from_history(&shape, feature)
      .or_else(|| before.map(|b| naming::from_identity(&shape, &b.shape, &b.names)))
      .unwrap_or_default();
    naming::label(&mut shape, &names);
    Body {
      id,
      name,
      shape,
      feature,
      sheet: None,
      unfolded: None,
      names: std::sync::Arc::new(names),
      mesh: OnceCell::new(),
      faces: OnceCell::new(),
      edges: OnceCell::new(),
    }
  }

  fn with_shape(&self, shape: Shape, feature: FeatureId) -> Body {
    Body {
      sheet: self.sheet.clone(),
      unfolded: self.unfolded.clone(),
      ..Body::named(self.id, self.name.clone(), shape, feature, Some(self))
    }
  }

  /// Its faces' names.
  pub fn names(&self) -> &naming::Names {
    &self.names
  }

  /// Renames its faces (and relabels the shape for the features after).
  pub fn set_names(&mut self, names: naming::Names) {
    naming::label(&mut self.shape, &names);
    self.names = std::sync::Arc::new(names);
  }

  /// The face a reference stands for: the face its name is on, or among several (the pieces of a
  /// split face), the one its signature matches best; by signature alone when no face has its name.
  pub fn find_face(&self, fr: &FaceRef) -> Option<u32> {
    let named = naming::faces_named(&self.names, fr.name);
    match named.as_slice() {
      [] => fr.resolve(self.faces(), self.mesh(), self.diagonal()),
      [one] => Some(*one),
      _ => {
        let cands: Vec<FaceInfo> = self.faces().iter().filter(|f| named.contains(&f.id)).copied().collect();
        fr.resolve(&cands, self.mesh(), self.diagonal())
          .or_else(|| naming::nearest(self.faces(), &named, DVec3::from(fr.point)))
      }
    }
  }

  /// Every face a reference stands for: all the faces with its name (a split face's pieces), or
  /// as its signature finds them when no face has it.
  pub fn find_faces(&self, fr: &FaceRef) -> Vec<u32> {
    let named = naming::faces_named(&self.names, fr.name);
    if named.len() < 2 {
      return if named.is_empty() { fr.resolve_all(self.faces(), self.mesh(), self.diagonal()) } else { named };
    }
    // the pieces of one face lie on one surface: the best match's
    let Some(best) = self.find_face(fr).and_then(|id| self.faces().iter().find(|f| f.id == id)) else { return named };
    let scale = 1e-6 * self.diagonal().max(1.0);
    self
      .faces()
      .iter()
      .filter(|f| named.contains(&f.id) && naming::same_surface(f, best, scale))
      .map(|f| f.id)
      .collect()
  }

  /// The edge a reference stands for: one between the faces it names, or among several, its
  /// signature's best match; by signature alone when no edge lies between them.
  pub fn find_edge(&self, er: &EdgeRef) -> Option<u32> {
    let named = naming::edges_named(&self.names, self.edges(), er.names);
    match named.as_slice() {
      [] => er.resolve(self.edges(), self.faces(), self.diagonal()),
      [one] => Some(*one),
      _ => {
        let cands: Vec<EdgeInfo> = self.edges().iter().filter(|e| named.contains(&e.id)).copied().collect();
        er.resolve(&cands, self.faces(), self.diagonal()).or_else(|| named.first().copied())
      }
    }
  }

  /// Every edge a reference stands for: all the edges between the faces it names (a split edge's
  /// pieces), or as its signature finds them when there are none.
  pub fn find_edges(&self, er: &EdgeRef) -> Vec<u32> {
    let named = naming::edges_named(&self.names, self.edges(), er.names);
    if named.len() < 2 {
      return if named.is_empty() { er.resolve_all(self.edges(), self.faces(), self.diagonal()) } else { named };
    }
    // two faces can meet in separate edges (two crossing holes); the pieces of one edge lie on
    // one curve: the best match's
    let Some(best) = self.find_edge(er).and_then(|id| self.edges().iter().find(|e| e.id == id)) else { return named };
    let scale = 1e-6 * self.diagonal().max(1.0);
    self.edges().iter().filter(|e| named.contains(&e.id) && naming::same_curve(e, best, scale)).map(|e| e.id).collect()
  }

  /// A reference to an edge of this body, with the names of the faces along it (keeping the ones
  /// `had` gives where the faces still have them).
  pub fn edge_ref(&self, e: &EdgeInfo, had: [u64; 2]) -> EdgeRef {
    EdgeRef { names: naming::pick_pair(&self.names, e, had), ..EdgeRef::from_edge(self.id, e, self.faces()) }
  }

  /// A reference to a face of this body picked at `at`, with its name (the one `had` gives if the
  /// face still has it).
  pub fn face_ref(&self, f: &FaceInfo, at: DVec3, had: u64) -> FaceRef {
    FaceRef { name: naming::pick(&self.names, f.id, had), ..FaceRef::from_face(self.id, f, at) }
  }

  /// The flat pattern of a sheet metal body, unfolded onto `face` (default: the largest planar
  /// face).
  pub fn flat_pattern(&self, face: Option<u32>) -> Result<FlatPattern, String> {
    let sheet = self.sheet.as_ref().ok_or("the body is not sheet metal")?;
    // a lofted flange brings its own flat pattern (while the body is as it was built)
    if let Some(dev) = &sheet.developed {
      let volume = self.shape.mass().map(|m| m.volume).unwrap_or(0.0);
      if (volume - dev.volume).abs() <= 1e-6 * dev.volume.abs().max(1.0) {
        return dev.flat_pattern(sheet.thickness);
      }
      // cut since: carry the cuts into the developed flat
      if dev.built.is_some() {
        return crate::sheet3::developed_with_cuts(dev, sheet.thickness, &self.shape);
      }
    }
    let face = face.or_else(|| sheet::default_face(self.faces())).ok_or("the body has no planar face")?;
    sheet::flat_pattern(&self.shape, self.faces(), face, sheet)
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
  pub sketch: fox_sketch::Sketch,
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
  axes: BTreeMap<FeatureId, (DVec3, DVec3)>,
  points: BTreeMap<FeatureId, DVec3>,
  threads: Vec<(FeatureId, Vec<DVec3>)>,
  canvases: BTreeMap<FeatureId, Plane>,
  forms: BTreeMap<FeatureId, Plane>,
  body_counter: u32,
  /// Saved joint origins: component and frame (as the component is now).
  joint_origins: BTreeMap<FeatureId, (crate::ComponentId, DMat4)>,
  /// How far joints have moved each component.
  placed: BTreeMap<crate::ComponentId, DMat4>,
  /// Rigid groups so far.
  groups: Vec<Vec<crate::ComponentId>>,
  /// 3D curves (Project To Surface), as wires.
  curves3d: BTreeMap<FeatureId, Vec<Shape>>,
  /// The tool solids each feature applied, with their operation and targets (feature mirrors
  /// and patterns re-apply them).
  tools: BTreeMap<FeatureId, Vec<pathpattern::Tool>>,
  /// Intersection curves: their wires (drawn with the 3D curves; pipes follow them).
  intersections: BTreeMap<FeatureId, Vec<Shape>>,
}

/// The regenerated design.
#[derive(Clone, Default)]
pub struct Model {
  pub bodies: Vec<Body>,
  pub sketches: BTreeMap<FeatureId, SketchResult>,
  pub planes: BTreeMap<FeatureId, Plane>,
  /// Construction axes: a point and a unit direction.
  pub axes: BTreeMap<FeatureId, (DVec3, DVec3)>,
  pub points: BTreeMap<FeatureId, DVec3>,
  /// Cosmetic threads, as helix polylines to draw on their faces.
  pub threads: Vec<(FeatureId, Vec<DVec3>)>,
  /// Canvases (reference images): the plane each one lies on.
  pub canvases: BTreeMap<FeatureId, Plane>,
  /// Form features: the plane each one's control cage is in.
  pub forms: BTreeMap<FeatureId, Plane>,
  /// Status per feature, in timeline order.
  pub status: Vec<FeatureStatus>,
  /// Evaluated parameter values (mm / rad).
  pub params: HashMap<String, f64>,
  pub param_errors: HashMap<String, String>,
  /// Saved joint origins: component and frame, where the component ended up.
  pub joint_origins: BTreeMap<FeatureId, (crate::ComponentId, DMat4)>,
  /// How far joints have moved each component (identity if they haven't).
  pub placed: BTreeMap<crate::ComponentId, DMat4>,
  /// 3D curves (Project To Surface), as wires.
  pub curves3d: BTreeMap<FeatureId, Vec<Shape>>,
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
    // an active configuration regenerates a copy with its overrides (see config.rs)
    if let Some(mut eff) = crate::config::effective(design) {
      let model = self.regenerate(&mut eff);
      crate::config::write_back(design, &eff);
      crate::newcomp::sync(design, &model.bodies);
      return model;
    }
    let (params, param_errors) = eval_params(design);
    let mates = assemble::mates(design, &params);
    let mut key = {
      let mut h = DefaultHasher::new();
      serde_json::to_string(&design.parameters).unwrap_or_default().hash(&mut h);
      serde_json::to_string(&design.sheet_metal_rules).unwrap_or_default().hash(&mut h);
      serde_json::to_string(&design.components).unwrap_or_default().hash(&mut h);
      mates.key().hash(&mut h);
      design.behaviours.hash(&mut h);
      h.finish()
    };
    let active = design.active_len();
    let rules = design.clone_rules();
    let components = design.components.clone();
    let behaviours = design.behaviours.clone();
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
        let targets = feature.targets.clone();
        let mut ctx = Ctx {
          params: &params,
          rules: &rules,
          components: &components,
          targets: &targets,
          state: &mut state,
          mates: &mates,
          behaviours: &behaviours,
        };
        let id = feature.id;
        let mut kind = feature.kind.clone();
        let seeded = ctx.seed_cached(&feature.cached);
        let result = ctx.apply(id, &mut kind);
        for s in &seeded {
          ctx.state.sketches.remove(s);
        }
        match result.map(|w| w.or_else(|| (!seeded.is_empty()).then(|| LOST_PROFILE.to_string()))) {
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
    // New Component features put their new bodies in their components, which joints then move
    if crate::newcomp::sync(design, &state.bodies) {
      return self.regenerate(design);
    }
    // cached statuses must carry their warnings/errors too
    Model {
      bodies: state.bodies,
      sketches: state.sketches,
      planes: state.planes,
      axes: state.axes,
      points: state.points,
      threads: state.threads,
      canvases: state.canvases,
      forms: state.forms,
      status,
      params,
      param_errors,
      joint_origins: state.joint_origins,
      placed: state.placed,
      curves3d: state.curves3d.into_iter().chain(state.intersections).collect(),
    }
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
  if !f.suppressed && !f.cached.is_empty() && st.error.is_none() && st.warning.is_none() {
    st.warning = Some(LOST_PROFILE.into());
  }
  st
}

/// The warning on a feature building from a deleted sketch's cached geometry (Fusion's words).
const LOST_PROFILE: &str = "The profile reference is lost and this feature is using cached geometry";

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
  let mut list: Vec<(String, String, ParamKind)> =
    design.parameters.iter().map(|p| (p.name.clone(), p.expr.clone(), p.kind)).collect();
  // the model parameters (d1, d2, ...) are in scope too, as in Fusion, so a dimension or a
  // feature can read "d1 * 2"; a user parameter of the same name wins
  let users = list.len();
  for m in design.model_parameters() {
    if !list[..users].iter().any(|(n, ..)| *n == m.name) {
      list.push((m.name, m.expr, m.kind));
    }
  }
  let mut ok = HashMap::new();
  let mut err = HashMap::new();
  for (k, v) in fox_expr::eval_parameters(&list) {
    match v {
      Ok(v) => {
        ok.insert(k, v);
      }
      // a model parameter's bad expression is its own feature's warning
      Err(e) if design.parameters.iter().any(|p| p.name == k) => {
        err.insert(k, e.to_string());
      }
      Err(_) => {}
    }
  }
  (ok, err)
}

struct Ctx<'a> {
  params: &'a HashMap<String, f64>,
  rules: &'a crate::Rules,
  components: &'a [crate::Component],
  /// The current feature's join/cut targets (empty = any body it touches).
  targets: &'a [BodyId],
  state: &'a mut State,
  mates: &'a assemble::Mates,
  /// The design's behaviour flags: gate a feature-level geometry change on
  /// `self.behaviours.has(..)` (sketch-level ones on the sketch's own set). See
  /// [`fox_sketch::behaviour`].
  behaviours: &'a crate::Behaviours,
}

type FResult = Result<Option<String>, String>;

impl Ctx<'_> {
  fn len(&self, e: &str) -> Result<f64, String> {
    fox_expr::eval(e, fox_expr::Kind::Length, self.params).map_err(|err| format!("'{e}': {err}"))
  }

  fn angle(&self, e: &str) -> Result<f64, String> {
    fox_expr::eval(e, fox_expr::Kind::Angle, self.params).map_err(|err| format!("'{e}': {err}"))
  }

  fn count(&self, e: &str) -> Result<usize, String> {
    let v = fox_expr::eval(e, fox_expr::Kind::Unitless, self.params).map_err(|err| format!("'{e}': {err}"))?;
    let n = v.round();
    if !(1.0..=MAX_INSTANCES as f64).contains(&n) {
      return Err(format!("count must be between 1 and {MAX_INSTANCES}"));
    }
    Ok(n as usize)
  }

  /// Puts a feature's cached copies of deleted sketches where it looks its sketches up; returns
  /// the ids seeded, to take out again once the feature is built.
  fn seed_cached(&mut self, cached: &[crate::CachedSketch]) -> Vec<FeatureId> {
    let mut seeded = Vec::new();
    for c in cached {
      if self.state.sketches.contains_key(&c.sketch) {
        continue;
      }
      let Ok(plane) = self.resolve_plane(&mut c.plane.clone()) else { continue };
      let mut sketch = c.geometry.clone();
      let report = sketch.solve();
      let profiles = find_profiles(&sketch);
      self.state.sketches.insert(c.sketch, SketchResult { plane, sketch, profiles, report, dim_errors: Vec::new() });
      seeded.push(c.sketch);
    }
    seeded
  }

  fn apply(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::Sketch { plane, sketch, links } => {
        let pl = self.resolve_plane(plane)?;
        let lost =
          self.refresh_links(&pl, sketch, links) + crate::include::refresh_sources(&self.state.sketches, sketch);
        let spun = if sketch.spuns.is_empty() { None } else { self.refresh_spuns(&pl, sketch) };
        let flat = if sketch.flats.is_empty() { None } else { self.refresh_flats(plane, &pl, sketch) };
        let dim_errors: Vec<(ConstraintId, String)> =
          sketch.evaluate_dims(self.params).into_iter().map(|(c, e)| (c, e.to_string())).collect();
        let report = sketch.solve();
        let profiles = find_profiles(sketch);
        let r = SketchResult { plane: pl, sketch: sketch.clone(), profiles, report, dim_errors };
        let w =
          sketch_warning(&r).or_else(|| (lost > 0).then(|| format!("{lost} projected edge(s) could not be found")));
        let w = flat.or(spun).or(w);
        self.state.sketches.insert(id, r);
        Ok(w)
      }
      FeatureKind::Extrude { sketch, profiles, extent, operation } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (chosen, missing) = pick_profiles(&sk, profiles);
        if chosen.is_empty() {
          return Err(if sk.profiles.is_empty() {
            "the sketch has no closed profiles".into()
          } else {
            "the selected profiles no longer exist".into()
          });
        }
        let mut sk = sk;
        let lift = self.extrude_start(extent, &sk.plane)?;
        sk.plane.origin += sk.plane.normal() * lift;
        let mut n = sk.plane.normal() * if extent.flip { -1.0 } else { 1.0 };
        let mut trim = None;
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
          ExtentKind::ThroughAllBoth => {
            let d = self.through_all(&sk.plane);
            (-d, 2.0 * d)
          }
          ExtentKind::ToFace => {
            let fr = extent.to.as_mut().ok_or("pick the face to extrude to")?;
            let bi = self.body_index(fr.body)?;
            let body = &self.state.bodies[bi];
            let fid = body.find_face(fr).ok_or("the target face could not be found")?;
            refresh_face(fr, body, fid);
            let info = body.faces().iter().find(|f| f.id == fid).ok_or("the target face could not be found")?;
            if info.kind != fox_kernel::SurfaceKind::Plane || info.normal.cross(n).length() > 1e-6 {
              // a curved or tilted face: extrude through and trim to it
              let offset = self.len(&extent.distance)?;
              if (info.center - sk.plane.origin).dot(n) < 0.0 {
                n = -n;
              }
              trim = Some(solid::Trim::Face { body: bi, face: fid, offset });
              (0.0, self.through_all(&sk.plane))
            } else {
              let mut d = (info.origin - sk.plane.origin).dot(n);
              if d < 0.0 {
                n = -n;
                d = -d;
              }
              (0.0, d + self.len(&extent.distance)?)
            }
          }
          ExtentKind::ToBody => {
            let fr = extent.to.as_mut().ok_or("pick the body to extrude to")?;
            let bi = self.body_index(fr.body)?;
            let center = self.state.bodies[bi].shape.mass().map_err(ke)?.center_of_mass;
            if (center - sk.plane.origin).dot(n) < 0.0 {
              n = -n;
            }
            trim = Some(solid::Trim::Body { body: bi });
            (0.0, self.through_all(&sk.plane))
          }
          ExtentKind::ToVertex => {
            let pr = extent.to_point.as_mut().ok_or("pick the vertex or point to extrude to")?;
            let at = self.point_of(pr)?;
            let mut d = (at - sk.plane.origin).dot(n);
            if d.abs() < 1e-9 {
              return Err("the point lies in the sketch plane".into());
            }
            if d < 0.0 {
              n = -n;
              d = -d;
            }
            (0.0, d + self.len(&extent.distance)?)
          }
        };
        let taper = if extent.taper.trim().is_empty() { 0.0 } else { self.angle(&extent.taper)? };
        // the far side of a two-sided extent tapers on its own (symmetric: as the first side)
        let taper2 = match extent.kind {
          ExtentKind::TwoSides if !extent.taper2.trim().is_empty() => self.angle(&extent.taper2)?,
          _ => taper,
        };
        let thin = if extent.thin.trim().is_empty() { 0.0 } else { self.len(&extent.thin)? };
        if thin < 0.0 {
          return Err("the thin wall thickness must be positive".into());
        }
        let along = n.dot(sk.plane.normal()).signum();
        let tapered = taper.abs() > 1e-12 || taper2.abs() > 1e-12;
        // `height` is signed along `n`
        let side = |face: &Shape, height: f64, angle: f64| -> Result<Shape, String> {
          if angle.abs() > 1e-12 {
            face.extrude_tapered(height * along, angle).map_err(ke)
          } else {
            face.extrude(n * height).map_err(ke)
          }
        };
        let solid = |face: &Shape| -> Result<Shape, String> {
          if tapered && start < 0.0 {
            // each side drafts away from the profile plane on its own, as in Fusion
            let mut sides = vec![side(face, start, taper2)?];
            if length + start > 1e-9 {
              sides.push(side(face, length + start, taper)?);
            }
            return union_all(sides);
          }
          let face = if start.abs() > 0.0 { face.translated(n * start).map_err(ke)? } else { face.clone() };
          side(&face, length, taper)
        };
        let mut tools = Vec::new();
        for p in &chosen {
          let mut face = Shape::face(&sk.plane, p).map_err(ke)?;
          naming::label_profile(&mut face, &sk.plane, p, id);
          if thin > 0.0 {
            let (outer, inner) = match extent.thin_side {
              ThinSide::Inside => (face.clone(), face.offset_face(-thin).map_err(ke)?),
              ThinSide::Outside => (face.offset_face(thin).map_err(ke)?, face.clone()),
              ThinSide::Center => {
                (face.offset_face(thin / 2.0).map_err(ke)?, face.offset_face(-thin / 2.0).map_err(ke)?)
              }
            };
            let wall = solid(&outer)?.boolean(&solid(&inner)?, BoolOp::Subtract).map_err(ke)?;
            tools.push(wall);
          } else {
            tools.push(solid(&face)?);
          }
        }
        let mut tool = union_all(tools)?;
        if let Some(t) = trim {
          tool = self.trim_to_object(tool, t, &sk.plane)?;
        }
        self.operate(id, tool, *operation)?;
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::Revolve { sketch, profiles, axis, angle, symmetric, angle2, operation } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (chosen, missing) = pick_profiles(&sk, profiles);
        if chosen.is_empty() {
          return Err("no profiles to revolve".into());
        }
        let (o, mut d) = self.resolve_axis(axis, Some(*sketch))?;
        if matches!(axis, AxisRef::SketchLine { .. }) && self.behaviours.has(REVOLVE_AXIS_UNDIRECTED) {
          // as in Fusion, the way the axis line was drawn doesn't matter: the profile turns
          // towards the sketch normal, as it does about a world axis lying in the sketch (older
          // designs turn right-handed about the line from its start to its end)
          let p = sk.plane.to_world(chosen[0].interior_point());
          if d.cross(p - o).dot(sk.plane.normal()) < 0.0 {
            d = -d;
          }
        }
        let a = self.angle(angle)?;
        // where the sweep starts (turned back from the profile) and how far it turns
        let (back, total) = match angle2 {
          _ if *symmetric => (a, 2.0 * a),
          Some(a2) => {
            let a2 = self.angle(a2)?;
            (a2, a + a2)
          }
          None => (0.0, a),
        };
        let mut tools = Vec::new();
        for p in &chosen {
          let mut face = Shape::face(&sk.plane, p).map_err(ke)?;
          naming::label_profile(&mut face, &sk.plane, p, id);
          if back.abs() > 1e-12 {
            face = face.rotated(o, d, -back).map_err(ke)?;
          }
          tools.push(face.revolve(o, d, total).map_err(ke)?);
        }
        let tool = union_all(tools)?;
        self.operate(id, tool, *operation)?;
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::Fillet { edges, radius, radius2, setback, chord, radii, sets, distance2, flipped } => {
        let r = self.len(radius)?;
        if let Some(d2) = distance2 {
          if !sets.is_empty() || radius2.is_some() || !radii.is_empty() || setback.is_some() || *chord {
            return Err("an asymmetric fillet has one selection set and two offsets".into());
          }
          let d2 = self.len(d2)?;
          let (a, b) = if *flipped { (d2, r) } else { (r, d2) };
          return self.edge_op(id, edges, |s, ids| fillet2::asymmetric(s, ids, a, b));
        }
        let sb = match setback.as_deref().map(str::trim) {
          Some(e) if !e.is_empty() => Some(self.len(e)?),
          _ => None,
        };
        let round = |s: &Shape, f: Shape, ids: &[u32]| match sb {
          Some(sb) => solid::setback(s, &f, ids, sb),
          None => Ok(f),
        };
        if !sets.is_empty() {
          if radius2.is_some() || !radii.is_empty() {
            return Err("a variable radius works with one selection set".into());
          }
          // every set's edges in one rounding, each edge with its set's radius
          let mut all = edges.clone();
          let mut set_r = vec![r; edges.len()];
          for set in sets.iter() {
            let rs = self.len(&set.radius)?;
            all.extend(set.edges.iter().cloned());
            set_r.extend(std::iter::repeat_n(rs, set.edges.len()));
          }
          let chord = *chord;
          let out = self.edge_op_from(id, &mut all, |s, ids, from| {
            let mut list = Vec::new();
            for (eid, k) in ids.iter().zip(from) {
              let r = if chord { chord_radii(s, &[*eid], set_r[*k])?[0].1 } else { set_r[*k] };
              list.push((*eid, r));
            }
            round(s, s.fillet_each(&list)?, ids)
          });
          // keep the refreshed references
          let mut rest = all.into_iter();
          for e in edges.iter_mut() {
            *e = rest.next().ok_or("internal: lost a fillet edge")?;
          }
          for set in sets.iter_mut() {
            for e in set.edges.iter_mut() {
              *e = rest.next().ok_or("internal: lost a fillet edge")?;
            }
          }
          return out;
        }
        let r2 = radius2.as_deref().map(|e| self.len(e)).transpose()?;
        if !radii.is_empty() || r2.is_some_and(|r2| r2 <= 1e-9) || (r2.is_some() && r <= 1e-9) {
          let mut law = vec![(0.0, r)];
          for p in radii {
            law.push((p.at, self.len(&p.radius)?));
          }
          law.push((1.0, r2.unwrap_or(r)));
          return self.edge_op(id, edges, |s, ids| round(s, s.fillet_law(ids, &law)?, ids));
        }
        match radius2 {
          Some(r2) => {
            let r2 = self.len(r2)?;
            self.edge_op(id, edges, |s, ids| round(s, s.fillet_variable(ids, r, r2)?, ids))
          }
          None if *chord => self.edge_op(id, edges, |s, ids| {
            let radii = chord_radii(s, ids, r)?;
            round(s, s.fillet_each(&radii)?, ids)
          }),
          None => self.edge_op(id, edges, |s, ids| round(s, s.fillet(ids, r)?, ids)),
        }
      }
      FeatureKind::Chamfer { edges, distance, distance2, angle, flipped } => {
        let flip = *flipped;
        let d = self.len(distance)?;
        // designs from before CHAMFER_FORWARD_FACE measure d1 on each edge's first adjacent face
        let forward = self.behaviours.has(fox_sketch::behaviour::CHAMFER_FORWARD_FACE);
        let two = move |s: &Shape, ids: &[u32], d2: f64, angle: bool| {
          if forward { s.chamfer_two(ids, d, d2, angle, flip) } else { s.chamfer_two_legacy(ids, d, d2, angle, flip) }
        };
        match (distance2, angle) {
          (_, Some(a)) => {
            let a = self.angle(a)?;
            self.edge_op(id, edges, |s, ids| two(s, ids, a, true))
          }
          (Some(d2), None) => {
            let d2 = self.len(d2)?;
            self.edge_op(id, edges, |s, ids| two(s, ids, d2, false))
          }
          (None, None) => self.edge_op(id, edges, |s, ids| s.chamfer(ids, d)),
        }
      }
      FeatureKind::OffsetFaces { faces, distance } => {
        let dist = self.len(distance)?;
        if dist.abs() < 1e-9 {
          return Err("the distance must not be zero".into());
        }
        let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
        for (i, f) in faces.iter().enumerate() {
          by_body.entry(f.body).or_default().push(i);
        }
        if by_body.is_empty() {
          return Err("select faces to push or pull".into());
        }
        for (bid, idxs) in by_body {
          let bi = self.body_index(bid)?;
          let body = &self.state.bodies[bi];
          let mut shape = body.shape.clone();
          let mut picked = Vec::new();
          for i in idxs {
            let Some(fr) = faces.get_mut(i) else { continue };
            let found = resolve_face_pieces(fr, body);
            if found.is_empty() {
              return Err("a face could not be found".into());
            }
            picked.extend(found);
          }
          let extended = self.offset_faces_extending(body, &picked, dist);
          if let Some(s) = extended {
            shape = s;
          } else {
            for fid in picked {
              let info = body.faces().iter().find(|f| f.id == fid).ok_or("a face could not be found")?;
              let face = body.shape.face_shape(fid).map_err(ke)?;
              // flat faces move along their normal; curved ones are offset (a hole grows or shrinks)
              let tool = if info.kind == fox_kernel::SurfaceKind::Plane {
                face.extrude(info.normal * dist).map_err(ke)?
              } else {
                face.thicken(dist).map_err(ke)?
              };
              let op = if dist > 0.0 { BoolOp::Union } else { BoolOp::Subtract };
              shape = shape.boolean(&tool, op).map_err(ke)?;
            }
          }
          let shape = shape.unified().map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
          self.split_multi(id, bid);
        }
        Ok(None)
      }
      FeatureKind::SplitByBody { bodies, tool, remove_tool } => {
        let ti = self.body_index(*tool)?;
        let tool_shape = self.state.bodies[ti].shape.clone();
        let targets: Vec<BodyId> =
          self.select(bodies)?.into_iter().map(|i| self.state.bodies[i].id).filter(|b| b != tool).collect();
        let mut split_any = false;
        for bid in targets {
          let bi = self.body_index(bid)?;
          let pieces = self.state.bodies[bi].shape.split_by(&tool_shape).map_err(ke)?;
          if pieces.len() < 2 {
            continue;
          }
          split_any = true;
          let sheet = self.state.bodies[bi].sheet.clone();
          let mut it = pieces.into_iter();
          if let Some(first) = it.next() {
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(first, id);
          }
          for piece in it {
            let nid = self.new_body(id, piece);
            self.set_sheet(nid, sheet.clone());
          }
        }
        if *remove_tool {
          self.state.bodies.retain(|b| b.id != *tool);
        }
        Ok((!split_any).then(|| "the tool doesn't cross the bodies".to_string()))
      }
      FeatureKind::Shell { faces, thickness, round, direction, thickness_out, bodies } => {
        let t = self.len(thickness)?;
        let (inside, outside) = match direction {
          crate::ShellDirection::Inside => (t, 0.0),
          crate::ShellDirection::Outside => (0.0, t),
          crate::ShellDirection::Both => {
            (t, if thickness_out.trim().is_empty() { t } else { self.len(thickness_out)? })
          }
        };
        let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
        for (i, f) in faces.iter().enumerate() {
          by_body.entry(f.body).or_default().push(i);
        }
        // whole bodies: hollow and closed
        for b in bodies.iter() {
          by_body.entry(*b).or_default();
        }
        if by_body.is_empty() {
          return Err("select faces to remove or a body".into());
        }
        let mut missing = false;
        for (bid, idxs) in by_body {
          let bi = self.body_index(bid)?;
          let body = &self.state.bodies[bi];
          let mut ids = Vec::new();
          for i in idxs {
            let Some(fr) = faces.get_mut(i) else { continue };
            let found = resolve_face_pieces(fr, body);
            missing |= found.is_empty();
            ids.extend(found.into_iter().filter(|f| !ids.contains(f)).collect::<Vec<_>>());
          }
          let shape = body.shape.shell_sides(&ids, inside, outside, *round).map_err(ke)?;
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
        // a primitive's faces always come in the same order
        let mut local = local;
        let count = local.faces().map_err(ke)?.len();
        naming::label(&mut local, &naming::by_index(count, id));
        let m = plane_matrix(&pl) * DMat4::from_translation(DVec3::new(at[0], at[1], 0.0));
        let tool = local.transformed(m).map_err(ke)?;
        self.operate(id, tool, *operation)?;
        Ok(None)
      }
      FeatureKind::ConstructionPlane { base, offset } => {
        let pl = self.resolve_plane(base)?;
        let d = self.len(offset)?;
        // offsets from XZ go along +Y in designs made with XZ_OFFSET_PLUS_Y, as in Fusion, in either
        // frame (designs from before it: along -Y)
        let sign = if let PlaneRef::Origin { plane } = base { plane.offset_sign(self.behaviours) } else { 1.0 };
        self.state.planes.insert(id, pl.offset(sign * d));
        Ok(None)
      }
      FeatureKind::Mirror { plane, features, .. } if !features.is_empty() => {
        let pl = self.resolve_plane(plane)?;
        for f in features.iter() {
          let tools = self.state.tools.get(f).cloned().ok_or("a mirrored feature failed or has no solid to mirror")?;
          for (k, t) in tools.into_iter().enumerate() {
            let mut copy = t.shape.mirrored(pl.origin, pl.normal()).map_err(ke)?;
            naming::relabel_copy(&mut copy, id, (u64::from(f.0) << 20) | k as u64);
            // the mirror's own targets, else the bodies the feature itself was limited to
            let targets = if self.targets.is_empty() { t.targets } else { self.targets.to_vec() };
            self.operate_on(id, copy, t.op, &targets)?;
          }
        }
        Ok(None)
      }
      FeatureKind::Mirror { bodies, plane, join, .. } => {
        let pl = self.resolve_plane(plane)?;
        let targets = self.select(bodies)?;
        for bi in targets {
          let body = self.state.bodies[bi].clone();
          let copy = body.shape.mirrored(pl.origin, pl.normal()).map_err(ke)?;
          self.add_copies(id, bi, vec![copy], *join)?;
        }
        Ok(None)
      }
      FeatureKind::RectangularPattern {
        bodies,
        direction,
        count,
        spacing,
        direction2,
        count2,
        spacing2,
        symmetric,
        symmetric2,
        extent,
        join,
      } => {
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
        let (o1, k1) = pattern_offsets(n1, s1, *symmetric, *extent);
        let (o2, k2) = pattern_offsets(n2, s2, *symmetric2 && direction2.is_some(), *extent);
        for bi in self.select(bodies)? {
          let body = self.state.bodies[bi].clone();
          let mut copies = Vec::new();
          for (i, &t1) in o1.iter().enumerate() {
            for (j, &t2) in o2.iter().enumerate() {
              if i == k1 && j == k2 {
                continue;
              }
              copies.push(body.shape.translated(d1 * t1 + d2 * t2).map_err(ke)?);
            }
          }
          self.add_copies(id, bi, copies, *join)?;
        }
        Ok(None)
      }
      FeatureKind::CircularPattern { bodies, axis, count, angle, symmetric, join } => {
        let (o, d) = self.resolve_axis(axis, None)?;
        let n = self.count(count)?;
        let total = self.angle(angle)?;
        let full = (total.abs() - std::f64::consts::TAU).abs() < 1e-9;
        let step = if full || n < 2 { total / n as f64 } else { total / (n - 1) as f64 };
        // Symmetric (short of a full turn): the instances span -total/2..total/2.
        let start = if *symmetric && !full && n >= 2 { -total / 2.0 } else { 0.0 };
        let angles: Vec<f64> = (0..n).map(|k| start + step * k as f64).collect();
        // The original stays put when an instance lands on it; otherwise (an even symmetric
        // count) it moves to the first instance.
        let stay = angles.iter().position(|a| a.abs() < 1e-12);
        for bi in self.select(bodies)? {
          let body = self.state.bodies[bi].clone();
          let rest: Vec<f64> = match stay {
            Some(i) => angles.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, a)| *a).collect(),
            None => {
              let moved = body.shape.rotated(o, d, angles[0]).map_err(ke)?;
              self.state.bodies[bi] = body.with_shape(moved, id);
              angles[1..].to_vec()
            }
          };
          let copies: Result<Vec<Shape>, String> =
            rest.iter().map(|a| body.shape.rotated(o, d, *a).map_err(ke)).collect();
          self.add_copies(id, bi, copies?, *join)?;
        }
        Ok(None)
      }
      FeatureKind::Move { bodies, translate, axis, angle, rotate, pivot, copy } => {
        let v = DVec3::new(self.len(&translate[0])?, self.len(&translate[1])?, self.len(&translate[2])?);
        let rot = match axis {
          Some(a) => Some((self.resolve_axis(a, None)?, self.angle(angle)?)),
          None => None,
        };
        let turns = match rotate {
          Some(r) => [self.angle(&r[0])?, self.angle(&r[1])?, self.angle(&r[2])?],
          None => [0.0; 3],
        };
        let c = pivot.map(DVec3::from).unwrap_or(DVec3::ZERO);
        let mut copies = Vec::new();
        for bi in self.select(bodies)? {
          let mut s = self.state.bodies[bi].shape.clone();
          for (a, d) in turns.iter().zip([DVec3::X, DVec3::Y, DVec3::Z]) {
            if *a != 0.0 {
              s = s.rotated(c, d, *a).map_err(ke)?;
            }
          }
          s = s.translated(v).map_err(ke)?;
          if let Some(((o, d), a)) = rot {
            s = s.rotated(o, d, a).map_err(ke)?;
          }
          if *copy {
            copies.push((bi, s));
          } else {
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(s, id);
          }
        }
        for (bi, s) in copies {
          self.add_copies(id, bi, vec![s], false)?;
        }
        Ok(None)
      }
      FeatureKind::Combine { target, tools, operation, keep_tools } => {
        let ti = self.body_index(*target)?;
        let mut shape = self.state.bodies[ti].shape.clone();
        let op = match operation {
          Operation::Join | Operation::NewBody | Operation::NewComponent => BoolOp::Union,
          Operation::Cut => BoolOp::Subtract,
          Operation::Intersect => BoolOp::Intersect,
        };
        if op == BoolOp::Intersect && self.behaviours.has(fox_sketch::behaviour::INTERSECT_ANY_TOOL) {
          // Fusion keeps the part of the target inside any tool, target ∩ (t1 ∪ t2 ∪ …),
          // not just the part inside every tool
          let mut union: Option<Shape> = None;
          for t in tools.iter() {
            if t == target {
              continue;
            }
            let s = &self.state.bodies[self.body_index(*t)?].shape;
            union = Some(match union {
              None => s.clone(),
              Some(u) => u.boolean(s, BoolOp::Union).map_err(ke)?,
            });
          }
          if let Some(u) = union {
            shape = shape.boolean(&u, BoolOp::Intersect).map_err(ke)?;
          }
        } else {
          // cutting or joining the tools one at a time equals doing it with their union; designs
          // from before INTERSECT_ANY_TOOL intersect with each tool in turn, target ∩ t1 ∩ t2 ∩ …
          for t in tools.iter() {
            if t == target {
              continue;
            }
            let i = self.body_index(*t)?;
            shape = shape.boolean(&self.state.bodies[i].shape, op).map_err(ke)?;
          }
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
      FeatureKind::Loft { sections, ruled, closed, operation, rails, centerline } => {
        let mut faces = Vec::new();
        for (i, sec) in sections.iter().enumerate() {
          let sk = self.state.sketches.get(&sec.sketch).ok_or(format!("the sketch of section {} failed", i + 1))?;
          let prof = match sec.profile {
            Some(p) => pick_profiles(sk, &[p]).0.into_iter().next(),
            None => sk.profiles.first().cloned(),
          };
          match (prof, lone_point(&sk.sketch)) {
            (Some(prof), _) => faces.push(Shape::face(&sk.plane, &prof).map_err(ke)?),
            // a sketch holding just a point: the loft runs to (or from) that point
            (None, Some(p)) => faces.push(Shape::vertex(sk.plane.to_world(p)).map_err(ke)?),
            (None, None) => return Err(format!("section {} has no profile", i + 1)),
          }
        }
        let tool = self.loft_ends(sections, &faces, *ruled, *closed, rails, centerline.as_ref())?;
        self.operate(id, tool, *operation)?;
        Ok(None)
      }
      FeatureKind::Sweep {
        sketch,
        profiles,
        path_sketch,
        path_curves,
        operation,
        rail,
        twist,
        taper,
        orientation,
        profile_scaling,
        rail2,
        guide_surface,
      } => {
        let sk = self.state.sketches.get(sketch).ok_or("the profile sketch failed")?.clone();
        let ps = self.state.sketches.get(path_sketch).ok_or("the path sketch failed")?;
        let path = crate::include::sketch_path(ps, path_curves)?;
        let (chosen, missing) = pick_profiles(&sk, profiles);
        if chosen.is_empty() {
          return Err("no profiles to sweep".into());
        }
        let rails = solid::Rails { rail: rail.as_ref(), rail2: rail2.as_ref(), surface: guide_surface.as_mut() };
        let guide = self.sweep_guide(rails, twist, taper, *orientation, *profile_scaling)?;
        let mut tools = Vec::new();
        for p in &chosen {
          let mut face = Shape::face(&sk.plane, p).map_err(ke)?;
          naming::label_profile(&mut face, &sk.plane, p, id);
          tools.push(solid::sweep(&face, &path, &guide)?);
        }
        self.operate(id, union_all(tools)?, *operation)?;
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::Hole {
        sketch,
        points,
        kind: hole_kind,
        diameter,
        depth,
        through_all,
        to,
        tip_angle,
        head_diameter,
        head_depth,
        head_angle,
        flip,
        placed: _,
      } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let r = self.len(diameter)? / 2.0;
        if !(r > 1e-6) {
          return Err("the hole diameter must be positive".into());
        }
        let target = match to {
          Some(pl) if !*through_all => Some(self.resolve_plane(pl)?),
          _ => None,
        };
        let deep = if *through_all {
          self.through_all(&sk.plane)
        } else if target.is_some() {
          f64::INFINITY // worked out per centre below
        } else {
          self.len(depth)?
        };
        if !(deep > 1e-6) {
          return Err("the hole depth must be positive".into());
        }
        let tip = if *through_all { 0.0 } else { self.angle(tip_angle)? };
        let head = match hole_kind {
          HoleKind::Simple => sheet::HoleHead::None,
          HoleKind::Counterbore => {
            sheet::HoleHead::Counterbore { radius: self.len(head_diameter)? / 2.0, depth: self.len(head_depth)? }
          }
          HoleKind::Countersink => {
            sheet::HoleHead::Countersink { radius: self.len(head_diameter)? / 2.0, angle: self.angle(head_angle)? }
          }
        };
        let shared = if target.is_none() { Some(sheet::hole_section(r, deep, tip, head)?) } else { None };
        let centres: Vec<[f64; 2]> = if points.is_empty() {
          let used: Vec<fox_sketch::PointId> =
            sk.sketch.curves.iter().flat_map(|c| sk.sketch.curve_points(c.id)).collect();
          sk.sketch
            .points
            .iter()
            .filter(|p| p.id != fox_sketch::ORIGIN && !used.contains(&p.id))
            .map(|p| [p.x, p.y])
            .collect()
        } else {
          points.iter().filter_map(|p| sk.sketch.pos(*p)).collect()
        };
        if centres.is_empty() {
          return Err("the sketch has no points to drill (add sketch points)".into());
        }
        let down = if *flip { sk.plane.normal() } else { -sk.plane.normal() };
        let mut tools = Vec::new();
        for c in centres {
          let o = sk.plane.to_world(c);
          let own;
          let section = match (&shared, &target) {
            (Some(s), _) => s,
            (None, Some(t)) => {
              // 'To': the full diameter runs from the centre to the plane; the tip goes past it
              let along = down.dot(t.normal());
              let d = if along.abs() > 1e-9 { (t.origin - o).dot(t.normal()) / along } else { f64::NAN };
              if !(d > 1e-6) {
                return Err("the 'to' plane is not ahead of the hole".into());
              }
              own = sheet::hole_section(r, d, tip, head)?;
              &own
            }
            (None, None) => return Err("the hole has no depth".into()),
          };
          let plane = Plane { origin: o, x: sk.plane.x, y: down };
          let face = Shape::face(&plane, section).map_err(ke)?;
          tools.push(face.revolve(o, down, std::f64::consts::TAU).map_err(ke)?);
        }
        self.operate(id, union_all(tools)?, Operation::Cut)?;
        Ok(None)
      }
      FeatureKind::SplitBody { bodies, plane } => {
        let pl = self.resolve_plane(plane)?;
        let targets: Vec<BodyId> = self.select(bodies)?.into_iter().map(|i| self.state.bodies[i].id).collect();
        let mut split_any = false;
        for bid in targets {
          let bi = self.body_index(bid)?;
          let shape = &self.state.bodies[bi].shape;
          let mut pieces = shape.split(pl.origin, pl.normal()).map_err(ke)?;
          if pieces.len() < 2 {
            continue;
          }
          // A cut along an existing face leaves the new face beside the old one; Fusion merges
          // them. Only merge when the body had no coplanar neighbours of its own (split faces
          // stay split). Designs made before this keep the pieces as cut.
          let face_count = |s: &Shape| s.faces().map(|f| f.len()).unwrap_or(0);
          if self.behaviours.has(fox_sketch::behaviour::UNIFY_SPLIT_PIECES)
            && shape.unified().is_ok_and(|u| face_count(&u) == face_count(shape))
          {
            pieces = pieces.into_iter().map(|p| p.unified().unwrap_or(p)).collect();
          }
          split_any = true;
          let sheet = self.state.bodies[bi].sheet.clone();
          let mut it = pieces.into_iter();
          if let Some(first) = it.next() {
            self.state.bodies[bi] = self.state.bodies[bi].with_shape(first, id);
          }
          for piece in it {
            let nid = self.new_body(id, piece);
            self.set_sheet(nid, sheet.clone());
          }
        }
        Ok((!split_any).then(|| "the plane doesn't cross the bodies".to_string()))
      }
      FeatureKind::Draft { faces, plane, angle, flip, symmetric, angle2 } => {
        let pl = self.resolve_plane(plane)?;
        let a = self.angle(angle)?;
        // symmetric and two-angle drafts split the faces at the plane and taper both sides
        let a2 = match angle2 {
          Some(e) => Some(self.angle(e)?),
          None => symmetric.then_some(a),
        };
        let pull = if *flip { -pl.normal() } else { pl.normal() };
        let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
        for (i, f) in faces.iter().enumerate() {
          by_body.entry(f.body).or_default().push(i);
        }
        if by_body.is_empty() {
          return Err("select faces to draft".into());
        }
        let mut missing = false;
        for (bid, idxs) in by_body {
          let bi = self.body_index(bid)?;
          let body = &self.state.bodies[bi];
          let mut ids = Vec::new();
          for i in idxs {
            let Some(fr) = faces.get_mut(i) else { continue };
            let found = resolve_face_pieces(fr, body);
            missing |= found.is_empty();
            ids.extend(found.into_iter().filter(|f| !ids.contains(f)).collect::<Vec<_>>());
          }
          let shape = match a2 {
            Some(a2) => body.shape.draft_two_sided(&ids, pl.origin, pull, a, a2),
            None => body.shape.draft(&ids, pl.origin, pull, a),
          }
          .map_err(ke)?;
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        }
        Ok(missing.then(|| "some faces could not be found".to_string()))
      }
      FeatureKind::Flange { sketch, profiles, rule, flip, operation } => {
        let info = self.sheet_info(rule)?;
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let (chosen, missing) = pick_profiles(&sk, profiles);
        if chosen.is_empty() {
          return Err(if sk.profiles.is_empty() {
            "the sketch has no closed profiles (use Contour Flange for open lines)".into()
          } else {
            "the selected profiles no longer exist".into()
          });
        }
        let n = sk.plane.normal() * if *flip { -info.thickness } else { info.thickness };
        let mut tools = Vec::new();
        for p in &chosen {
          tools.push(Shape::face(&sk.plane, p).map_err(ke)?.extrude(n).map_err(ke)?);
        }
        self.operate(id, union_all(tools)?, *operation)?;
        self.tag_sheet(id, &info);
        Ok(missing.then(|| "some selected profiles no longer exist".to_string()))
      }
      FeatureKind::ContourFlange { sketch, curves, distance, symmetric, flip, flip_direction, rule } => {
        let info = self.sheet_info(rule)?;
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let path = sheet::open_path(&sk.sketch, curves)?;
        let section = sheet::contour_section(&path, info.thickness, info.bend_radius, *flip)?;
        let d = self.len(distance)?;
        if !(d > 1e-9) {
          return Err("the flange width must be positive".into());
        }
        let n = sk.plane.normal() * if *flip_direction { -1.0 } else { 1.0 };
        let face = Shape::face(&sk.plane, &section).map_err(ke)?;
        let tool = if *symmetric {
          face.translated(-n * d).map_err(ke)?.extrude(n * 2.0 * d).map_err(ke)?
        } else {
          face.extrude(n * d).map_err(ke)?
        };
        self.operate(id, tool, Operation::NewBody)?;
        self.tag_sheet(id, &info);
        Ok(None)
      }
      FeatureKind::EdgeFlange { edges, height, angle, flip, position, offset_start, offset_end, corners } => {
        let h = self.len(height)?;
        let a = self.angle(angle)?;
        let offsets = (self.len(offset_start)?, self.len(offset_end)?);
        if !(a > 1e-6 && a < std::f64::consts::PI - 1e-6) {
          return Err("the flange angle must be between 0 and 180 degrees (use Hem for 180)".into());
        }
        let position = *position;
        self.edge_flanges(id, edges, *flip, offsets, *corners, |info| {
          let (t, r) = (info.thickness, info.bend_radius);
          let half = (a / 2.0).tan();
          let straight = h - (r + t) * half;
          if straight < -1e-9 {
            return Err(format!("the height must be at least {:.3} mm for this bend", (r + t) * half));
          }
          let shift = match position {
            BendPosition::Adjacent => 0.0,
            BendPosition::Inside => -(r + t) * half,
            BendPosition::Outside => -r * half,
          };
          Ok((sheet::flange_section(r, t, a, straight.max(0.0)), shift, crate::sheet2::bend_top(r, t, a)))
        })
      }
      FeatureKind::Hem { edges, length, gap, flip } => {
        let (l, g) = (self.len(length)?, self.len(gap)?);
        if !(g >= 0.01) {
          return Err("the hem gap must be at least 0.01 mm (a closed hem would fuse to the sheet)".into());
        }
        self.edge_flanges(id, edges, *flip, (0.0, 0.0), CornerMode::Miter, |info| {
          let (t, r) = (info.thickness, g / 2.0);
          let straight = l - (r + t);
          if straight < -1e-9 {
            return Err(format!("the hem must be at least {:.3} mm long", r + t));
          }
          let top = crate::sheet2::bend_top(r, t, std::f64::consts::PI);
          Ok((sheet::flange_section(r, t, std::f64::consts::PI, straight.max(0.0)), 0.0, top))
        })
      }
      FeatureKind::Bend { sketch, line, angle, flip, flip_side } => {
        let sk = self.state.sketches.get(sketch).ok_or("its sketch failed")?.clone();
        let Some(CurveKind::Line { a, b }) = sk.sketch.curve(*line).map(|c| &c.kind) else {
          return Err("the bend line no longer exists".into());
        };
        let (Some(pa), Some(pb)) = (sk.sketch.pos(*a), sk.sketch.pos(*b)) else {
          return Err("the bend line no longer exists".into());
        };
        let alpha = self.angle(angle)?;
        if !(alpha > 1e-6 && alpha < std::f64::consts::PI - 1e-6) {
          return Err("the bend angle must be between 0 and 180 degrees".into());
        }
        let (p0, p1) = (sk.plane.to_world(pa), sk.plane.to_world(pb));
        let n = sk.plane.normal();
        let u = (p1 - p0).try_normalize().ok_or("the bend line has no length")?;
        let side = n.cross(u) * if *flip_side { -1.0 } else { 1.0 };
        // the sheet whose skin the sketch lies on
        let bi = (0..self.state.bodies.len())
          .find(|&i| {
            let b = &self.state.bodies[i];
            b.sheet.as_ref().is_some_and(|s| !s.flat)
              && b.faces().iter().any(|f| {
                f.kind == fox_kernel::SurfaceKind::Plane
                  && f.normal.dot(n).abs() > 0.9999
                  && (f.origin - p0).dot(n).abs() < 1e-6
              })
          })
          .ok_or("draw the bend line on a flat face of a sheet metal body")?;
        let body = self.state.bodies[bi].clone();
        let info = body.sheet.clone().ok_or("not sheet metal")?;
        let (t, r) = (info.thickness, info.bend_radius);
        // is the material behind the sketch plane (−n) or in front of it?
        let behind = body.faces().iter().any(|f| {
          f.kind == fox_kernel::SurfaceKind::Plane && f.normal.dot(n) > 0.9999 && (f.origin - p0).dot(n).abs() < 1e-6
        });
        let into = if behind { -n } else { n };
        let up = if *flip { into } else { -into };
        // skin on the inside of the bend
        let inner = if *flip { p0 + into * t } else { p0 };
        let ba = info.bend_allowance(alpha);
        let start = p0 - side * (ba / 2.0);
        let pieces_a = body.shape.split(start, side).map_err(ke)?;
        let s_of = |s: &Shape| s.mass().map(|m| (m.center_of_mass - start).dot(side)).unwrap_or(0.0);
        let (fixed, rest): (Vec<Shape>, Vec<Shape>) = pieces_a.into_iter().partition(|s| s_of(s) < 0.0);
        let stop = p0 + side * (ba / 2.0);
        let mut moving = Vec::new();
        for piece in rest {
          for q in piece.split(stop, side).map_err(ke)? {
            if q.mass().map(|m| (m.center_of_mass - stop).dot(side)).unwrap_or(0.0) > 0.0 {
              moving.push(q);
            }
          }
        }
        if fixed.is_empty() || moving.is_empty() {
          return Err("the bend line must cross the sheet".into());
        }
        let axis_pt = inner + (start - p0) + up * r;
        let axis = side.cross(up);
        let mut acc = union_all(fixed.clone())?;
        // revolve the cut faces at the start of the bend
        for f in fixed.iter() {
          for fi in f.faces().map_err(ke)? {
            if fi.kind == fox_kernel::SurfaceKind::Plane
              && fi.normal.dot(side) > 0.9999
              && (fi.origin - start).dot(side).abs() < 1e-6
            {
              let face = f.face_shape(fi.id).map_err(ke)?;
              acc = acc.boolean(&face.revolve(axis_pt, axis, alpha).map_err(ke)?, BoolOp::Union).map_err(ke)?;
            }
          }
        }
        for m in moving {
          let m = m.translated(-side * ba).map_err(ke)?.rotated(axis_pt, axis, alpha).map_err(ke)?;
          acc = acc.boolean(&m, BoolOp::Union).map_err(ke)?;
        }
        let shape = acc.unified().map_err(ke)?;
        self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
        Ok(None)
      }
      FeatureKind::ConvertToSheetMetal { body, face, rule, bends } => {
        let mut info = self.sheet_info(rule)?;
        let bi = self.body_index(*body)?;
        let fid = self.pick_face(bi, face.as_mut())?;
        let b = &self.state.bodies[bi];
        let t = sheet::measure_thickness(b.faces(), fid)
          .ok_or("there is no parallel face behind the selected face; is the body a sheet?")?;
        let warn = ((t - info.thickness).abs() > 1e-3).then(|| {
          format!("the body is {t:.3} mm thick, the rule says {:.3} mm; using the body's thickness", info.thickness)
        });
        info.thickness = t;
        let mut shape = b.shape.clone();
        if *bends && !sheet::sharp_corners(b.faces(), b.edges(), t, info.bend_radius).is_empty() {
          if info.bend_radius <= 1e-9 {
            return Err("the rule's bend radius must be above zero to turn corners into bends".into());
          }
          // where two bends meet, cut a square corner relief so each bend ends on a flat face
          for relief in sheet::corner_reliefs(b.faces(), b.edges(), &info, b.diagonal())? {
            shape = shape.boolean(&relief, BoolOp::Subtract).map_err(ke)?;
          }
          let (faces, edges) = (shape.faces().map_err(ke)?, shape.edges().map_err(ke)?);
          let corners = sheet::sharp_corners(&faces, &edges, t, info.bend_radius);
          shape = shape.fillet_each(&corners).map_err(|e| format!("turning corners into bends: {e}"))?;
        }
        let mut nb = self.state.bodies[bi].with_shape(shape, id);
        nb.sheet = Some(info);
        self.state.bodies[bi] = nb;
        Ok(warn)
      }
      k @ FeatureKind::Joint { .. } => self.joint(id, id, k),
      FeatureKind::CopyComponent { source, translate, transform, .. } => {
        let comp = self.components.iter().find(|c| c.id == *source).ok_or("the source component no longer exists")?;
        let v = DVec3::new(self.len(&translate[0])?, self.len(&translate[1])?, self.len(&translate[2])?);
        let m = DMat4::from_translation(v) * transform.map(|t| DMat4::from_cols_array(&t)).unwrap_or(DMat4::IDENTITY);
        if !fox_kernel::is_rigid(m) {
          return Err("a copy can only be moved and rotated".into());
        }
        let src: Vec<usize> = comp.bodies.iter().filter_map(|b| self.body_index(*b).ok()).collect();
        if src.is_empty() {
          // a sub-assembly's copy is its sub-components' copies
          if self.components.iter().any(|c| c.parent == Some(*source)) {
            return Ok(None);
          }
          return Err(format!("{} has no bodies", comp.name));
        }
        for bi in src {
          let shape = self.state.bodies[bi].shape.transformed(m).map_err(ke)?;
          let sheet = self.state.bodies[bi].sheet.clone();
          let nid = self.new_body(id, shape);
          self.set_sheet(nid, sheet);
        }
        Ok(None)
      }
      FeatureKind::Rip { edges, gap } => {
        let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
        for (i, e) in edges.iter().enumerate() {
          by_body.entry(e.body).or_default().push(i);
        }
        if by_body.is_empty() {
          return Err("select edges to rip".into());
        }
        for (bid, idxs) in by_body {
          let bi = self.body_index(bid)?;
          let body = &self.state.bodies[bi];
          let mut tools = Vec::new();
          for i in idxs {
            let Some(er) = edges.get_mut(i) else { continue };
            let eid = body.find_edge(er).ok_or("an edge could not be found")?;
            let Some(e) = body.edges().iter().find(|e| e.id == eid).copied() else { continue };
            *er = body.edge_ref(&e, er.names);
            let face = |id: u32| body.faces().iter().find(|f| f.id == id && f.kind == fox_kernel::SurfaceKind::Plane);
            let (Some(f1), Some(f2)) = (face(e.faces[0]), face(e.faces[1])) else {
              return Err("rip along a straight edge between two flat faces".into());
            };
            if e.kind != fox_kernel::CurveKind::Line {
              return Err("rip along a straight edge between two flat faces".into());
            }
            let t = match &body.sheet {
              Some(s) => s.thickness,
              None => sheet::measure_thickness(body.faces(), f1.id).ok_or("the faces at the edge aren't a sheet")?,
            };
            let g = if gap.trim().is_empty() {
              match &body.sheet {
                Some(s) => s.gap,
                None => t / 2.0,
              }
            } else {
              self.len(gap)?
            };
            if !(g > 1e-6) {
              return Err("the rip gap must be positive".into());
            }
            let inward = -(f1.normal + f2.normal).try_normalize().ok_or("the faces at the edge are flat")?;
            let across = (f1.normal - f2.normal).normalize_or_zero();
            let dir = (e.end - e.start).normalize_or_zero();
            let plane = Plane { origin: e.start, x: across, y: inward };
            let rect = sheet::rectangle([-g / 2.0, -g], [g / 2.0, 4.0 * t]);
            tools.push(Shape::face(&plane, &rect).map_err(ke)?.extrude(dir * e.length).map_err(ke)?);
          }
          let mut shape = body.shape.clone();
          for tool in &tools {
            shape = shape.boolean(tool, BoolOp::Subtract).map_err(ke)?;
          }
          self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
          self.split_multi(id, bid);
        }
        Ok(None)
      }
      FeatureKind::Refold { body } => {
        let bi = self.body_index(*body)?;
        let b = &self.state.bodies[bi];
        let st = b.unfolded.clone().ok_or("the body isn't unfolded")?;
        let mut info = b.sheet.clone().ok_or("the body is not sheet metal")?;
        let mut folded = st.folded.clone();
        match st.flat.boolean(&b.shape, BoolOp::Subtract) {
          Ok(removed) => {
            let tool = st.folded.refold_cuts(st.face, info.thickness, info.k_factor, &removed).map_err(ke)?;
            // the pieces overlap a little, so cut them one at a time
            for piece in tool.solids().map_err(ke)? {
              folded = folded.boolean(&piece, BoolOp::Subtract).map_err(ke)?;
            }
          }
          Err(KernelError::Empty) => {}
          Err(e) => return Err(e.to_string()),
        }
        let added = b.shape.boolean(&st.flat, BoolOp::Subtract).is_ok_and(|a| a.mass().is_ok_and(|m| m.volume > 1e-6));
        info.flat = false;
        let mut nb = self.state.bodies[bi].with_shape(folded, id);
        nb.sheet = Some(info);
        nb.unfolded = None;
        self.state.bodies[bi] = nb;
        Ok(added.then(|| "material added while unfolded isn't carried into the folded part".to_string()))
      }
      FeatureKind::Unfold { body, face, keep_folded } => {
        let bi = self.body_index(*body)?;
        let mut info =
          self.state.bodies[bi].sheet.clone().ok_or("the body is not sheet metal (use Convert to Sheet Metal)")?;
        if info.flat {
          return Err("the body is already flat".into());
        }
        let fid = self.pick_face(bi, face.as_mut())?;
        let flat = self.state.bodies[bi].flat_pattern(Some(fid))?;
        info.flat = true;
        let state = Unfolded { folded: self.state.bodies[bi].shape.clone(), face: fid, flat: flat.shape.clone() };
        if *keep_folded {
          let nid = self.new_body(id, flat.shape);
          if let Some(b) = self.state.bodies.iter_mut().find(|b| b.id == nid) {
            b.sheet = Some(info);
            b.unfolded = Some(Box::new(state));
          }
        } else {
          let mut nb = self.state.bodies[bi].with_shape(flat.shape, id);
          nb.sheet = Some(info);
          nb.unfolded = Some(Box::new(state));
          self.state.bodies[bi] = nb;
        }
        Ok(None)
      }
      kind @ (FeatureKind::Emboss { .. } | FeatureKind::BoundaryFill { .. } | FeatureKind::ReplaceFace { .. }) => {
        self.apply_solid(id, kind)
      }
      other => self.apply_extra(id, other),
    }
  }

  // ---- sheet metal ----

  fn sheet_info(&self, rule: &str) -> Result<SheetInfo, String> {
    sheet::eval_rule(&self.rules.get(rule)?, self.params)
  }

  /// Marks the bodies `feature` just made as sheet metal.
  fn tag_sheet(&mut self, feature: FeatureId, info: &SheetInfo) {
    for b in self.state.bodies.iter_mut().filter(|b| b.feature == feature && b.sheet.is_none()) {
      b.sheet = Some(info.clone());
    }
  }

  /// A planar face of body `bi`: the referenced one (refreshed) or the largest.
  fn pick_face(&self, bi: usize, face: Option<&mut FaceRef>) -> Result<u32, String> {
    let body = &self.state.bodies[bi];
    let fid = match face {
      Some(fr) => {
        let fid = body.find_face(fr).ok_or("the face could not be found")?;
        refresh_face(fr, body, fid);
        fid
      }
      None => sheet::default_face(body.faces()).ok_or("the body has no planar face")?,
    };
    match body.faces().iter().find(|f| f.id == fid) {
      Some(f) if f.kind == fox_kernel::SurfaceKind::Plane => Ok(fid),
      _ => Err("the face must be planar".into()),
    }
  }

  /// Edge flanges and hems: for each straight sheet edge, `section` gives the bend's section (in
  /// the plane across the edge, see [`sheet::flange_section`]) and how far back from the edge
  /// the bend starts; the sheet is trimmed back that far and the bends are joined on. Flanges
  /// that stop short of an edge's end get bend reliefs; flanges meeting at a corner are closed
  /// as `corners` says, with the rule's corner relief. `section` also gives the height of the
  /// bend (what a corner relief clears).
  fn edge_flanges(
    &mut self,
    feature: FeatureId,
    edges: &mut [EdgeRef],
    flip: bool,
    (o1, o2): (f64, f64),
    corners: CornerMode,
    section: impl Fn(&SheetInfo) -> Result<(fox_sketch::Profile, f64, f64), String>,
  ) -> FResult {
    struct Flange {
      start: DVec3,
      end: DVec3,
      dir: DVec3,
      len: f64,
      /// Bend-side skin point at the edge start.
      base: DVec3,
      w: DVec3,
      b: DVec3,
      /// Extend past the start / end into a miter.
      ext: [bool; 2],
      /// Keep-sides of miter planes (origin, normal).
      cuts: Vec<(DVec3, DVec3)>,
      /// Butt corners at the start / end: where the bend and the wall stop, from that end.
      butt: [Option<(f64, f64)>; 2],
    }
    let mut by_body: BTreeMap<BodyId, Vec<usize>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
      by_body.entry(e.body).or_default().push(i);
    }
    if by_body.is_empty() {
      return Err("select sheet edges".into());
    }
    let mut missing = false;
    for (bid, idxs) in by_body {
      let bi = self.body_index(bid)?;
      let body = &self.state.bodies[bi];
      let info = body.sheet.clone().ok_or("flanges go on sheet metal bodies")?;
      if info.flat {
        return Err("the body is unfolded".into());
      }
      let t = info.thickness;
      let (profile, shift, bend_top) = section(&info)?;
      let mut list: Vec<Flange> = Vec::new();
      for i in idxs {
        let Some(er) = edges.get_mut(i) else { continue };
        let Some(eid) = body.find_edge(er) else {
          missing = true;
          continue;
        };
        let Some(e) = body.edges().iter().find(|e| e.id == eid).copied() else { continue };
        *er = body.edge_ref(&e, er.names);
        if e.kind != fox_kernel::CurveKind::Line {
          return Err("flanges need straight edges".into());
        }
        let (panel, wall) = sheet::edge_faces(body.faces(), e.faces, e.length, t)
          .ok_or("pick an edge where a face of the sheet meets a square side")?;
        if e.length - o1 - o2 < 1e-6 {
          return Err("the offsets leave no flange".into());
        }
        let b = if flip { -panel.normal } else { panel.normal };
        let w = wall.normal;
        list.push(Flange {
          start: e.start,
          end: e.end,
          dir: (e.end - e.start).normalize_or_zero(),
          len: e.length,
          base: e.start + if flip { -panel.normal * t } else { DVec3::ZERO },
          w,
          b: (b - w * b.dot(w)).normalize_or_zero(),
          ext: [false, false],
          cuts: Vec::new(),
          butt: [None, None],
        });
      }
      if list.is_empty() {
        return Err("the selected edges could not be found".into());
      }
      // corners shared by two flanges bending the same way
      let (xout, xin) = crate::sheet2::section_extents(&profile, bend_top);
      let mut relief_at: Vec<(DVec3, DVec3, DVec3, DVec3)> = Vec::new();
      {
        for i in 0..list.len() {
          for j in i + 1..list.len() {
            if list[i].b.dot(list[j].b) < 0.9999 {
              continue;
            }
            for (ei, pi) in [(0usize, list[i].start), (1, list[i].end)] {
              for (ej, pj) in [(0usize, list[j].start), (1, list[j].end)] {
                let offset_free = |end: usize| if end == 0 { o1 < 1e-9 } else { o2 < 1e-9 };
                if (pi - pj).length() > 1e-6 || !offset_free(ei) || !offset_free(ej) {
                  continue;
                }
                let (wi, wj) = (list[i].w, list[j].w);
                let corner = pi + (wi + wj) * shift;
                relief_at.push((list[i].base + (pi - list[i].start) + (wi + wj) * shift, wi, wj, list[i].b));
                let diag = wi + wj;
                let Some(n) = diag.cross(list[i].b).try_normalize() else { continue };
                // flange i lies on the side its edge runs to from the corner
                let into_i = if ei == 0 { list[i].dir } else { -list[i].dir };
                let n = if (into_i + wi).dot(n) > 0.0 { n } else { -n };
                match corners {
                  CornerMode::Open => continue,
                  CornerMode::Miter | CornerMode::Closed => {
                    let gap = if corners == CornerMode::Closed { 0.0 } else { info.gap };
                    list[i].cuts.push((corner + n * (gap / 2.0), n));
                    list[j].cuts.push((corner - n * (gap / 2.0), -n));
                  }
                  // i runs out to j's outside; j stops the gap short of i's wall
                  CornerMode::Overlap => {
                    list[i].cuts.push((pi + wj * (shift + xout), -wj));
                    list[j].cuts.push((pi + wi * (shift + xin - info.gap), -wi));
                  }
                  CornerMode::Butt => {
                    let into_j = if ej == 0 { list[j].dir } else { -list[j].dir };
                    let (fi, fj) = ((into_i, wi), (into_j, wj));
                    list[i].butt[ei] = crate::sheet3::butt_corner(fi, fj, shift, shift + xin, info.gap);
                    list[j].butt[ej] = crate::sheet3::butt_corner(fj, fi, shift, shift + xin, info.gap);
                    continue;
                  }
                }
                list[i].ext[ei] = true;
                list[j].ext[ej] = true;
              }
            }
          }
        }
      }
      let reach = (body.diagonal() + 10.0 * t) * 2.0;
      let mut tools = Vec::new();
      let mut cuts = Vec::new();
      let build = |f: &Flange, s0: f64, s1: f64| -> Result<Shape, String> {
        let plane = Plane { origin: f.base + f.w * shift + f.dir * s0, x: f.w, y: f.b };
        let mut tool = Shape::face(&plane, &profile).map_err(ke)?.extrude(f.dir * (s1 - s0)).map_err(ke)?;
        for (o, n) in &f.cuts {
          tool = tool.boolean(&sheet::half_space(*o, *n, 4.0 * reach)?, BoolOp::Intersect).map_err(ke)?;
        }
        // past a butt corner's bend end only the wall goes on
        for (k, end) in f.butt.iter().enumerate() {
          let Some((bend, _)) = end else { continue };
          let (lo, hi) = if k == 0 { (s0, *bend) } else { (f.len - bend, s1) };
          if hi - lo > 1e-9 {
            let plane = Plane { origin: f.base + f.w * shift + f.dir * lo, x: f.w, y: f.b };
            let rect = sheet::rectangle([-reach, -3.0 * t], [reach, bend_top]);
            let under = Shape::face(&plane, &rect).map_err(ke)?.extrude(f.dir * (hi - lo)).map_err(ke)?;
            tool = tool.boolean(&under, BoolOp::Subtract).map_err(ke)?;
          }
        }
        Ok(tool)
      };
      let mut spans = Vec::new();
      for f in &list {
        let s0 = if f.ext[0] {
          -reach
        } else if let Some((_, wall)) = f.butt[0] {
          wall
        } else {
          o1
        };
        let s1 = if f.ext[1] {
          f.len + reach
        } else if let Some((_, wall)) = f.butt[1] {
          f.len - wall
        } else {
          f.len - o2
        };
        spans.push((s0, s1));
        if shift < -1e-9 {
          let (c0, c1) = (o1.max(0.0), f.len - o2.max(0.0));
          let plane = Plane { origin: f.base + f.w * shift + f.dir * c0, x: f.w, y: f.b };
          let rect = sheet::rectangle([0.0, -2.0 * t], [t - shift, t]);
          cuts.push(Shape::face(&plane, &rect).map_err(ke)?.extrude(f.dir * (c1 - c0)).map_err(ke)?);
          // an earlier flange's bend past an end of the edge is trimmed by the setback too
          for (free, at, out) in [(o1 < 1e-9, f.start, -f.dir), (o2 < 1e-9, f.end, f.dir)] {
            let (r, edges) = (info.bend_radius, body.edges());
            if let Some((pl, prof)) = free.then(|| crate::sheet3::bend_past_end(edges, at, out, f.w, r, t)).flatten() {
              let pl = Plane { origin: pl.origin + f.w * shift, ..pl };
              cuts.push(Shape::face(&pl, &prof).map_err(ke)?.extrude(f.w * (t - shift)).map_err(ke)?);
            }
          }
        }
        // bend reliefs where the flange stops short of the edge's end
        if info.relief != ReliefShape::None {
          let round = info.relief == ReliefShape::Round;
          let pl = Plane { origin: f.base + f.b * t, x: f.dir, y: f.w };
          let inner = shift.min(0.0) - info.relief_depth;
          for (on, span) in
            [(o1 > 1e-9, (o1 - info.relief_width, o1)), (o2 > 1e-9, (f.len - o2, f.len - o2 + info.relief_width))]
          {
            if !on {
              continue;
            }
            let span = (span.0.max(0.0), span.1.min(f.len));
            if span.1 - span.0 < 1e-6 {
              continue;
            }
            let prof = sheet::relief_profile(span, inner, t, round);
            cuts.push(Shape::face(&pl, &prof).map_err(ke)?.extrude(-f.b * (3.0 * t)).map_err(ke)?);
          }
        }
      }
      let mut shape = body.shape.clone();
      for c in &cuts {
        shape = shape.boolean(c, BoolOp::Subtract).map_err(ke)?;
      }
      // a flange that runs into material already there (a flange from an earlier feature) is
      // pulled back from it by the rule's gap, leaving an open corner
      for (f, (s0, s1)) in list.iter().zip(spans.iter_mut()) {
        let mut tool = build(f, *s0, *s1)?;
        if let Ok(clash) = tool.boolean(&shape, BoolOp::Intersect)
          && let Ok(m) = clash.mass()
          && m.volume > 1e-6
        {
          let (lo, hi) = (m.bbox_min, m.bbox_max);
          let along: Vec<f64> = (0..8)
            .map(|k| {
              let c = DVec3::new(
                if k & 1 == 0 { lo.x } else { hi.x },
                if k & 2 == 0 { lo.y } else { hi.y },
                if k & 4 == 0 { lo.z } else { hi.z },
              );
              (c - f.base).dot(f.dir)
            })
            .collect();
          let (cmin, cmax) = along.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
          if (cmin + cmax) / 2.0 < (*s0 + *s1) / 2.0 {
            *s0 = s0.max(cmax + info.gap);
          } else {
            *s1 = s1.min(cmin - info.gap);
          }
          if *s1 - *s0 < 1e-3 {
            return Err("the flange would overlap another flange along its whole length".into());
          }
          tool = build(f, *s0, *s1)?;
        }
        tools.push(tool);
      }
      for f in &tools {
        shape = shape.boolean(f, BoolOp::Union).map_err(ke)?;
      }
      // corner reliefs (overlapping bends need one: square unless the rule says otherwise)
      let relief = match (info.corner_relief, corners) {
        (crate::CornerRelief::None, CornerMode::Overlap) => crate::CornerRelief::Square,
        (r, _) => r,
      };
      for (at, wi, wj, up) in &relief_at {
        if let Some(cut) = crate::sheet2::corner_relief(*at, *wi, *wj, *up, bend_top, &info, relief)? {
          shape = shape.boolean(&cut, BoolOp::Subtract).map_err(ke)?;
        }
      }
      let shape = shape.unified().map_err(ke)?;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, feature);
    }
    Ok(missing.then(|| "some edges could not be found".to_string()))
  }

  // ---- projected sketch geometry ----

  /// Moves projected sketch curves onto their edges' current positions. Returns how many edges
  /// were not found.
  fn refresh_links(&self, plane: &Plane, sketch: &mut fox_sketch::Sketch, links: &mut [crate::SketchLink]) -> usize {
    let mut lost = 0;
    for link in links.iter_mut() {
      let Ok(bi) = self.body_index(link.edge.body) else {
        lost += 1;
        continue;
      };
      let body = &self.state.bodies[bi];
      let Some(eid) = body.find_edge(&link.edge) else {
        lost += 1;
        continue;
      };
      let Some(e) = body.edges().iter().find(|e| e.id == eid).copied() else { continue };
      link.edge = body.edge_ref(&e, link.edge.names);
      if let Some(c) = sketch.curve3d_mut(link.curve) {
        if let Ok(k) = crate::include::edge_curve3d(&e) {
          c.kind = k;
        }
        continue;
      }
      let l = |v: DVec3| plane.to_local(v);
      let Some(kind) = sketch.curve(link.curve).map(|c| c.kind.clone()) else { continue };
      let put = |sketch: &mut fox_sketch::Sketch, p: fox_sketch::PointId, at: [f64; 2]| {
        if let Some(q) = sketch.point_mut(p) {
          q.x = at[0];
          q.y = at[1];
        }
      };
      match kind {
        CurveKind::Line { a, b } => {
          let (s, t) = (l(e.start), l(e.end));
          // keep the line's direction: match its ends to the nearer edge ends
          let near_a = sketch.pos(a).map(|p| (p[0] - s[0]).hypot(p[1] - s[1]) <= (p[0] - t[0]).hypot(p[1] - t[1]));
          let (pa, pb) = if near_a.unwrap_or(true) { (s, t) } else { (t, s) };
          put(sketch, a, pa);
          put(sketch, b, pb);
        }
        CurveKind::Circle { center, .. } => {
          put(sketch, center, l(e.center));
          if let Some(c) = sketch.curve_mut(link.curve)
            && let CurveKind::Circle { radius, .. } = &mut c.kind
          {
            *radius = e.radius;
          }
          for c in &mut sketch.constraints {
            if let fox_sketch::ConstraintKind::Radius { curve, value } = &mut c.kind
              && *curve == link.curve
            {
              *value = fox_sketch::Dim::new(e.radius);
            }
          }
        }
        CurveKind::Arc { center, start, end } => {
          let (s, m, t) = (l(e.start), l(e.mid), l(e.end));
          let Some(c) = fox_sketch::circumcenter(s, m, t) else { continue };
          // the sketch arc runs counter-clockwise from start to end
          let ccw = (s[0] - c[0]) * (m[1] - c[1]) - (s[1] - c[1]) * (m[0] - c[0]) > 0.0;
          let (ps, pe) = if ccw { (s, t) } else { (t, s) };
          put(sketch, center, c);
          put(sketch, start, ps);
          put(sketch, end, pe);
        }
        CurveKind::Spline { .. }
        | CurveKind::ControlSpline { .. }
        | CurveKind::Ellipse { .. }
        | CurveKind::Bezier { .. }
        | CurveKind::EllipticalArc { .. }
        | CurveKind::Conic { .. }
        | CurveKind::QuinticSpline { .. } => {}
      }
    }
    lost
  }

  // ---- assemblies ----

  /// A joint origin's frame, resolving (and refreshing) its face or edge reference.
  fn joint_frame(&self, g: &mut assembly::JointGeometry) -> Result<DMat4, String> {
    match g {
      assembly::JointGeometry::Point { point, z, x } => {
        assembly::frame(DVec3::from(*point), DVec3::from(*z), DVec3::from(*x))
      }
      assembly::JointGeometry::Face { face } => {
        let bi = self.body_index(face.body)?;
        let body = &self.state.bodies[bi];
        let fid = body.find_face(face).ok_or("a joint face could not be found")?;
        refresh_face(face, body, fid);
        let info = body.faces().iter().find(|f| f.id == fid).ok_or("a joint face could not be found")?;
        assembly::face_frame(info)
      }
      assembly::JointGeometry::Edge { edge } => {
        let bi = self.body_index(edge.body)?;
        let body = &self.state.bodies[bi];
        let eid = body.find_edge(edge).ok_or("a joint edge could not be found")?;
        let info = body.edges().iter().find(|e| e.id == eid).ok_or("a joint edge could not be found")?;
        *edge = body.edge_ref(info, edge.names);
        assembly::edge_frame(info, body.faces())
      }
      assembly::JointGeometry::Origin { origin } => self.origin_frame(*origin),
    }
  }

  // ---- references ----

  /// How this design frames origin planes and downward planes.
  fn frames(&self) -> Frames {
    Frames::of(self.behaviours)
  }

  fn resolve_plane(&mut self, p: &mut PlaneRef) -> Result<Plane, String> {
    self.resolve_plane_framed(p, self.frames())
  }

  /// [`Ctx::resolve_plane`] with the origin and downward planes framed as `frames` says.
  fn resolve_plane_framed(&mut self, p: &mut PlaneRef, frames: Frames) -> Result<Plane, String> {
    match p {
      PlaneRef::Origin { plane } => Ok(plane.plane(frames)),
      PlaneRef::Construction { feature } => {
        self.state.planes.get(feature).copied().ok_or_else(|| "its construction plane failed".to_string())
      }
      PlaneRef::Face { face, x } => {
        let bi = self.body_index(face.body)?;
        let body = &self.state.bodies[bi];
        let fid = body.find_face(face).ok_or("the face could not be found")?;
        let info = body.faces().iter().find(|f| f.id == fid).copied().ok_or("the face could not be found")?;
        refresh_face(face, body, fid);
        if info.kind != fox_kernel::SurfaceKind::Plane {
          return Err("the face is not planar".into());
        }
        let pl = sketch_plane_on(info.normal, info.origin, frames)?;
        match x.map(DVec3::from).filter(|x| x.cross(info.normal).length() > 1e-9) {
          Some(x) => Plane::from_normal(pl.origin, info.normal, x).map_err(|e| e.to_string()),
          None => Ok(pl),
        }
      }
      PlaneRef::Free { origin, normal, x } => {
        let (o, n) = (DVec3::from(*origin), DVec3::from(*normal));
        let auto = sketch_plane_on(n, o, frames)?.x;
        let x = x.map(DVec3::from).filter(|x| x.cross(n).length() > 1e-9).unwrap_or(auto);
        Plane::from_normal(o, n, x).map_err(|e| e.to_string())
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
        self.sketch_line_axis(sid, *curve)
      }
      AxisRef::SketchCurve { sketch, curve } => self.sketch_line_axis(*sketch, *curve),
      AxisRef::Construction { feature } => {
        self.state.axes.get(feature).copied().ok_or_else(|| "its construction axis failed".to_string())
      }
      AxisRef::Edge { edge } => {
        let bi = self.body_index(edge.body)?;
        let body = &self.state.bodies[bi];
        let eid = body.find_edge(edge).ok_or("the axis edge could not be found")?;
        let e = body.edges().iter().find(|e| e.id == eid).copied().ok_or("the axis edge could not be found")?;
        *edge = body.edge_ref(&e, edge.names);
        match e.kind {
          fox_kernel::CurveKind::Line => Ok((e.start, (e.end - e.start).normalize_or_zero())),
          fox_kernel::CurveKind::Circle => Ok((e.center, e.axis)),
          _ => Err("the axis edge is not straight or circular".into()),
        }
      }
    }
  }

  /// A sketch line as a world-space axis: its start point and unit direction.
  fn sketch_line_axis(&self, sid: FeatureId, curve: fox_sketch::CurveId) -> Result<(DVec3, DVec3), String> {
    let sk = self.state.sketches.get(&sid).ok_or("its sketch failed")?;
    let Some(CurveKind::Line { a, b }) = sk.sketch.curve(curve).map(|c| &c.kind) else {
      return Err("the axis line no longer exists".into());
    };
    let (Some(pa), Some(pb)) = (sk.sketch.pos(*a), sk.sketch.pos(*b)) else {
      return Err("the axis line no longer exists".into());
    };
    let (pa, pb) = (sk.plane.to_world(pa), sk.plane.to_world(pb));
    Ok((pa, (pb - pa).try_normalize().ok_or("the axis line has no length")?))
  }

  fn body_index(&self, id: BodyId) -> Result<usize, String> {
    self.state.bodies.iter().position(|b| b.id == id).ok_or_else(|| format!("body {} no longer exists", id.0))
  }

  fn select(&self, sel: &BodySel) -> Result<Vec<usize>, String> {
    match sel {
      BodySel::All => Ok((0..self.state.bodies.len()).collect()),
      BodySel::Bodies { ids } => ids.iter().map(|id| self.body_index(*id)).collect(),
      BodySel::Component { id } => {
        let c = self.components.iter().find(|c| c.id == *id).ok_or(format!("component {} no longer exists", id.0))?;
        Ok(c.bodies.iter().filter_map(|b| self.body_index(*b).ok()).collect())
      }
    }
  }

  /// How far along the sketch normal an extrusion starts: its start face (flat and parallel to
  /// the sketch) plus its start offset.
  fn extrude_start(&self, extent: &mut crate::Extent, plane: &Plane) -> Result<f64, String> {
    let offset = if extent.start.trim().is_empty() { 0.0 } else { self.len(&extent.start)? };
    let Some(fr) = extent.start_face.as_mut() else { return Ok(offset) };
    let body = &self.state.bodies[self.body_index(fr.body)?];
    let fid = body.find_face(fr).ok_or("the start face could not be found")?;
    refresh_face(fr, body, fid);
    let info = body.faces().iter().find(|f| f.id == fid).ok_or("the start face could not be found")?;
    if info.kind != fox_kernel::SurfaceKind::Plane || info.normal.cross(plane.normal()).length() > 1e-6 {
      return Err("the start face must be flat and parallel to the sketch".into());
    }
    Ok((info.origin - plane.origin).dot(plane.normal()) + offset)
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
    let sheet = self.state.bodies[i].sheet.clone();
    for s in it {
      let nid = self.new_body(feature, s);
      self.set_sheet(nid, sheet.clone());
    }
  }

  fn set_sheet(&mut self, id: BodyId, sheet: Option<SheetInfo>) {
    if let Some(b) = self.state.bodies.iter_mut().find(|b| b.id == id) {
      b.sheet = sheet;
    }
  }

  /// Applies a new/join/cut/intersect operation with a tool solid.
  /// The tool is kept so a later mirror can apply it again.
  fn operate(&mut self, feature: FeatureId, tool: Shape, op: Operation) -> Result<(), String> {
    self.record_tool(feature, &tool, op);
    let targets = self.targets.to_vec();
    self.operate_on(feature, tool, op, &targets)
  }

  /// [`Self::operate`] limited to `targets` (empty = any body the tool touches).
  fn operate_on(&mut self, feature: FeatureId, tool: Shape, op: Operation, targets: &[BodyId]) -> Result<(), String> {
    let tool_box = tool.mass().map(|m| (m.bbox_min, m.bbox_max)).map_err(ke)?;
    let overlapping: Vec<usize> = (0..self.state.bodies.len())
      .filter(|&i| targets.is_empty() || targets.contains(&self.state.bodies[i].id))
      .filter(|&i| self.state.bodies[i].bbox().is_some_and(|b| boxes_touch(b, tool_box)))
      .collect();
    match op {
      Operation::NewBody | Operation::NewComponent => {
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
  fn add_copies(&mut self, feature: FeatureId, bi: usize, mut copies: Vec<Shape>, join: bool) -> Result<(), String> {
    for (k, c) in copies.iter_mut().enumerate() {
      naming::relabel_copy(c, feature, k as u64 + 1);
    }
    if join {
      let mut acc = self.state.bodies[bi].shape.clone();
      for c in &copies {
        acc = acc.boolean(c, BoolOp::Union).map_err(ke)?;
      }
      let id = self.state.bodies[bi].id;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(acc, feature);
      self.split_multi(feature, id);
    } else {
      let sheet = self.state.bodies[bi].sheet.clone();
      for c in copies {
        let nid = self.new_body(feature, c);
        self.set_sheet(nid, sheet.clone());
      }
    }
    Ok(())
  }

  /// Fillet/chamfer: resolves edge references per body and applies `op`.
  fn edge_op(
    &mut self,
    feature: FeatureId,
    edges: &mut [EdgeRef],
    op: impl Fn(&Shape, &[u32]) -> fox_kernel::Result<Shape>,
  ) -> FResult {
    self.edge_op_from(feature, edges, |s, ids, _| op(s, ids))
  }

  /// [`Self::edge_op`], also telling `op` which of `edges` each edge id came from.
  fn edge_op_from(
    &mut self,
    feature: FeatureId,
    edges: &mut [EdgeRef],
    op: impl Fn(&Shape, &[u32], &[usize]) -> fox_kernel::Result<Shape>,
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
      let (mut ids, mut from) = (Vec::new(), Vec::new());
      for i in idxs {
        let Some(er) = edges.get_mut(i) else { continue };
        let found = body.find_edges(er);
        if found.is_empty() {
          missing = true;
          continue;
        }
        let pieces: Vec<&EdgeInfo> = body.edges().iter().filter(|e| found.contains(&e.id)).collect();
        if let Some(r) = EdgeRef::from_pieces(bid, &pieces, body.faces()) {
          *er = EdgeRef { names: pieces.first().map_or([0; 2], |e| body.edge_ref(e, er.names).names), ..r };
        }
        for eid in found {
          if !ids.contains(&eid) {
            ids.push(eid);
            from.push(i);
          }
        }
      }
      if ids.is_empty() {
        return Err("the selected edges could not be found".into());
      }
      let shape = op(&body.shape, &ids, &from).map_err(ke)?;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, feature);
    }
    Ok(missing.then(|| "some edges could not be found".to_string()))
  }
}

fn refresh_face(fr: &mut FaceRef, body: &Body, fid: u32) {
  refresh_face_pieces(fr, body, &[fid]);
}

/// Refreshes a face reference to the faces it now stands for (the pieces of a split face): the
/// point moves onto them and the bounds cover them all.
fn refresh_face_pieces(fr: &mut FaceRef, body: &Body, ids: &[u32]) {
  let Some(info) = ids.first().and_then(|id| body.faces().iter().find(|f| f.id == *id)) else { return };
  let p = DVec3::from(fr.point);
  let q =
    if info.kind == fox_kernel::SurfaceKind::Plane { p - info.normal * (p - info.origin).dot(info.normal) } else { p };
  *fr = FaceRef { body: fr.body, ..body.face_ref(info, q, fr.name) };
  let bounds = body.mesh().map(|m| ids.iter().filter_map(|id| crate::refs::face_bounds(m, *id)).collect::<Vec<_>>());
  if let Some((lo, hi)) = bounds.unwrap_or_default().into_iter().reduce(|(a, b), (c, d)| (a.min(c), b.max(d))) {
    fr.lo = lo.to_array();
    fr.hi = hi.to_array();
  }
}

/// Every face a face reference stands for on `body` (all pieces of a split face), refreshing it.
fn resolve_face_pieces(fr: &mut FaceRef, body: &Body) -> Vec<u32> {
  let ids = body.find_faces(fr);
  if !ids.is_empty() {
    refresh_face_pieces(fr, body, &ids);
  }
  ids
}

/// The sketch plane Fusion would put on a planar face: origin = the world origin projected onto
/// the face, axes aligned with the world (X right; up = Z for vertical faces). On a horizontal
/// face Y is up in Fusion's frames, so a downward face has X pointing left; legacy frames keep
/// X = +X there (and Y = −Y).
pub fn sketch_plane_on(normal: DVec3, point: DVec3, frames: Frames) -> Result<Plane, String> {
  let n = normal.try_normalize().ok_or("zero normal")?;
  let origin = n * point.dot(n);
  let (x, _y) = if n.z.abs() > 0.9 {
    (if n.z < 0.0 && frames == Frames::Fusion { DVec3::NEG_X } else { DVec3::X }, DVec3::Y)
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

/// The regions of a sketch that `points` pick (the smallest holding each point), and whether a
/// point missed. No points means every region except glyph counters (a text's profile is its ink,
/// as in Fusion).
fn pick_profiles(sk: &SketchResult, points: &[[f64; 2]]) -> (Vec<Profile>, bool) {
  let all = &sk.profiles;
  if points.is_empty() {
    if sk.sketch.texts.is_empty() {
      return (all.to_vec(), false);
    }
    return (all.iter().filter(|p| !sk.sketch.is_text_counter(p)).cloned().collect(), false);
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

mod extra;
pub use extra::iso_pitch;
mod solid;
pub use solid::boundary_cells;
mod flatsketch;
mod sheet2;
mod sheetmod;
mod spun;
pub use spun::outline as spun_outline;
mod intersect;
pub use intersect::outline as intersect_outline;
mod surface;
/// Offsets along one direction of a rectangular pattern, the original's (zero) included, as
/// Fusion places them: one way from the original, or with `symmetric` the original at index
/// `(n - 1) / 2` (an even count puts the extra instance on the positive side). With `extent`,
/// `distance` reaches from the original to the farthest instance on the positive side. Returns
/// the offsets and the original's index among them.
fn pattern_offsets(n: usize, distance: f64, symmetric: bool, extent: bool) -> (Vec<f64>, usize) {
  let back = if symmetric { n.saturating_sub(1) / 2 } else { 0 };
  let ahead = n.saturating_sub(1) - back;
  let step = match (extent, ahead) {
    (false, _) => distance,
    (true, 0) => 0.0,
    (true, k) => distance / k as f64,
  };
  ((0..n).map(|k| step * (k as f64 - back as f64)).collect(), back)
}

#[cfg(test)]
mod tests_solid;

mod assemble;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_sheet2;

/// The radius rounding each edge to a round `chord` wide. Across an edge whose faces' normals
/// turn by α the round is 2 r sin(α/2) wide; edges next to a curved face are taken as square.
fn chord_radii(s: &Shape, ids: &[u32], chord: f64) -> fox_kernel::Result<Vec<(u32, f64)>> {
  let (faces, edges) = (s.faces()?, s.edges()?);
  let normal = |id: u32| faces.iter().find(|f| f.id == id).and_then(|f| f.plane()).map(|p| p.normal());
  Ok(
    ids
      .iter()
      .map(|&id| {
        let turn = edges
          .iter()
          .find(|e| e.id == id)
          .and_then(|e| Some(normal(e.faces[0])?.angle_between(normal(e.faces[1])?)))
          .filter(|a| *a > 1e-6)
          .unwrap_or(std::f64::consts::FRAC_PI_2);
        (id, chord / (2.0 * (turn / 2.0).sin()))
      })
      .collect(),
  )
}

/// The sketch's only point that no curve uses (the origin aside), if it has exactly one.
fn lone_point(sk: &fox_sketch::Sketch) -> Option<[f64; 2]> {
  let used: Vec<_> = sk.curves.iter().flat_map(|c| fox_sketch::curve_points(&c.kind)).collect();
  let mut lone = sk.points.iter().filter(|p| p.id != fox_sketch::ORIGIN && !used.contains(&p.id));
  match (lone.next(), lone.next()) {
    (Some(p), None) => Some([p.x, p.y]),
    _ => None,
  }
}

mod arrange;
mod bound;
mod direct;
mod pathpattern;
mod pcb;
mod project;
mod subasm;
#[cfg(test)]
mod tests_config;
#[cfg(test)]
mod tests_config2;
#[cfg(test)]
mod tests_project;
#[cfg(test)]
mod tests_subasm;

mod curves3d;
mod fillet2;
mod form;
#[cfg(test)]
mod tests_fillet2;

mod offset;
#[cfg(test)]
mod tests_offset;

mod made;
pub use made::Made;
