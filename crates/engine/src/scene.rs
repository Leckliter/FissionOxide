//! Turns the session into a render scene (bodies, edges, sketches, grid) with pick ids, and maps
//! pick ids back to what was under the cursor.

use std::collections::HashMap;

use fox_kernel::glam::DVec3;
use fox_kernel::{EdgeLine, Plane, Shape, TriMesh};
use fox_model::{BodyId, FeatureId, FeatureKind};
use fox_render::{Marker, Mesh, Polyline, Scene};
use fox_sketch::{ConstraintKind, CurveKind, PointId, Sketch};

use crate::{Sel, Session};

pub const BODY_COLOR: [u8; 4] = [186, 190, 196, 255];
pub const HOVER_COLOR: [u8; 4] = [128, 190, 238, 255];
pub const SELECT_COLOR: [u8; 4] = [36, 142, 220, 255];
pub const EDGE_COLOR: [u8; 4] = [35, 38, 44, 255];
pub const PROFILE_COLOR: [u8; 4] = [246, 214, 160, 255];
pub const SKETCH_FREE: [u8; 4] = [30, 110, 235, 255];
pub const SKETCH_FIXED: [u8; 4] = [20, 20, 24, 255];
pub const SKETCH_CONSTRUCTION: [u8; 4] = [225, 130, 40, 255];
pub const SKETCH_IDLE: [u8; 4] = [60, 70, 90, 255];

/// What a pick id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pick {
  Face {
    body: BodyId,
    face: u32,
  },
  Edge {
    body: BodyId,
    edge: u32,
  },
  SketchPoint {
    sketch: FeatureId,
    point: PointId,
  },
  SketchCurve {
    sketch: FeatureId,
    curve: fox_sketch::CurveId,
  },
  Profile {
    sketch: FeatureId,
    index: usize,
  },
  /// A body vertex (shown while the vertex selection filter is on), named by an edge ending there.
  Vertex {
    body: BodyId,
    edge: u32,
    end: bool,
  },
}

impl Pick {
  pub fn to_sel(self) -> Sel {
    match self {
      Pick::Face { body, face } => Sel::Face { body, face },
      Pick::Edge { body, edge } => Sel::Edge { body, edge },
      Pick::SketchPoint { sketch, point } => Sel::SketchPoint { sketch, point },
      Pick::SketchCurve { sketch, curve } => Sel::SketchCurve { sketch, curve },
      Pick::Profile { sketch, index } => Sel::Profile { sketch, index },
      Pick::Vertex { body, edge, end } => Sel::Vertex { body, edge, end },
    }
  }

  pub fn from_sel(s: &Sel) -> Option<Pick> {
    Some(match *s {
      Sel::Face { body, face } => Pick::Face { body, face },
      Sel::Edge { body, edge } => Pick::Edge { body, edge },
      Sel::SketchPoint { sketch, point } => Pick::SketchPoint { sketch, point },
      Sel::SketchCurve { sketch, curve } => Pick::SketchCurve { sketch, curve },
      Sel::Profile { sketch, index } => Pick::Profile { sketch, index },
      Sel::Vertex { body, edge, end } => Pick::Vertex { body, edge, end },
      _ => return None,
    })
  }
}

/// Pick ids are allocated per scene build; this maps them back.
#[derive(Default)]
pub struct PickTable {
  items: Vec<Pick>,
  index: HashMap<Pick, u32>,
}

impl PickTable {
  fn id(&mut self, p: Pick) -> u32 {
    if let Some(i) = self.index.get(&p) {
      return *i;
    }
    self.items.push(p);
    let id = self.items.len() as u32; // 0 = nothing
    self.index.insert(p, id);
    id
  }

  pub fn get(&self, id: u32) -> Option<Pick> {
    if id == 0 { None } else { self.items.get(id as usize - 1).copied() }
  }

  pub fn id_of(&self, p: &Pick) -> Option<u32> {
    self.index.get(p).copied()
  }
}

/// Display options.
#[derive(Debug, Clone, Default)]
pub struct ViewOptions {
  pub hover: Option<Pick>,
  /// Bodies drawn in the hover colour: what the pointer is over in the browser or timeline.
  pub peek: Vec<BodyId>,
  pub hide_edges: bool,
  pub hide_grid: bool,
  /// Draw no sketches (renders of the finished part).
  pub hide_sketches: bool,
  /// Draw only these bodies (`None` = every visible body).
  pub only_bodies: Option<Vec<BodyId>>,
  /// Profiles are shown (and pickable) for these sketches even if consumed.
  pub show_profiles_of: Vec<FeatureId>,
  /// Sketches drawn without their profiles (the browser's Hide Profile).
  pub hide_profiles_of: Vec<FeatureId>,
  /// Section analysis: bodies are cut away on the side the plane's normal points to, and the
  /// cut is capped.
  pub section: Option<Plane>,
  pub analysis: Option<fox_render::Analysis>,
  /// Colour bodies by how sharply they curve.
  pub curvature: bool,
  /// Draw the generated toolpaths and their setup's stock (the Manufacture workspace).
  pub toolpaths: bool,
  /// Draw body vertices as pickable dots (the Vertices selection filter).
  pub show_vertices: bool,
  /// Bodies drawn see-through: (body, alpha), as Fusion's Opacity Control.
  pub opacity: Vec<(BodyId, u8)>,
}

/// Caches tessellations between frames (keyed by session revision).
#[derive(Default)]
pub struct SceneBuilder {
  revision: u64,
  edges: HashMap<BodyId, Vec<EdgeLine>>,
  profiles: HashMap<(FeatureId, usize), Option<TriMesh>>,
  /// Section caps per body, for the plane they were cut with.
  caps: HashMap<BodyId, (Plane, Option<TriMesh>)>,
  curvature: HashMap<BodyId, Vec<[u8; 4]>>,
}

impl SceneBuilder {
  pub fn build(&mut self, s: &Session, opts: &ViewOptions) -> (Scene, PickTable) {
    if self.revision != s.revision {
      self.edges.clear();
      self.profiles.clear();
      self.caps.clear();
      self.curvature.clear();
      self.revision = s.revision;
    }
    let mut picks = PickTable::default();
    let mut scene =
      Scene { background_top: [250, 250, 251, 255], background_bottom: [234, 236, 239, 255], ..Default::default() };
    let selected: Vec<Pick> = s.selection.iter().filter_map(Pick::from_sel).collect();
    let selected_bodies: Vec<BodyId> =
      s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(*body) } else { None }).collect();
    let editing = s.edit_sketch.is_some();

    // a version comparison replaces the bodies
    if let Some(d) = &s.diff {
      crate::diff::draw(d, &mut scene);
    }

    // bodies
    for body in &s.model.bodies {
      if s.diff.is_some() {
        continue;
      }
      let shown = match &opts.only_bodies {
        Some(only) => only.contains(&body.id),
        None => s.body_visible(body.id),
      };
      if !shown {
        continue;
      }
      let Some(m) = body.mesh() else { continue };
      let tint = |c: [u8; 4]| {
        let c = if selected_bodies.contains(&body.id) { mix(c, SELECT_COLOR, 0.45) } else { c };
        let c = if opts.peek.contains(&body.id) { mix(c, HOVER_COLOR, 0.6) } else { c };
        if editing { mix(c, [255, 255, 255, 255], 0.35) } else { c }
      };
      let base = tint(s.design.body_color(body.id).map(|c| [c[0], c[1], c[2], 255]).unwrap_or(BODY_COLOR));
      let tri_ids: Vec<u32> = m.tri_face.iter().map(|f| picks.id(Pick::Face { body: body.id, face: *f })).collect();
      // per-face highlight: hover one face; selected faces get their own meshes' colour via split
      let hover = match opts.hover {
        Some(p @ Pick::Face { body: b, .. }) if b == body.id => picks.id_of(&p),
        _ => None,
      };
      let sel_faces: Vec<u32> = selected
        .iter()
        .filter_map(|p| {
          if let Pick::Face { body: b, .. } = p { (*b == body.id).then(|| picks.id_of(p)).flatten() } else { None }
        })
        .collect();
      let first = scene.meshes.len();
      let face_cols = crate::views::body_face_colors(s, body.id);
      if !face_cols.is_empty() {
        // faces with an appearance of their own (and selected faces) draw as meshes of their colour
        let color_of = |t: usize, id: u32| {
          if sel_faces.contains(&id) {
            return SELECT_COLOR;
          }
          let f = m.tri_face.get(t).copied().unwrap_or(0);
          face_cols.get(&f).map(|c| tint([c[0], c[1], c[2], 255])).unwrap_or(base)
        };
        for (part, ids, color) in split_by_color(m, &tri_ids, color_of) {
          scene.meshes.push(mesh_of(&part, ids, color, hover));
        }
      } else if sel_faces.is_empty() {
        let mut mesh = mesh_of(m, tri_ids, base, hover);
        if opts.curvature {
          mesh.colors = self.curvature.entry(body.id).or_insert_with(|| curvature_colors(m)).clone();
        }
        scene.meshes.push(mesh);
      } else {
        // split selected faces into a second mesh so they draw in the selection colour
        let (a, b) = split_mesh(m, &tri_ids, &sel_faces);
        scene.meshes.push(mesh_of(&a.0, a.1, base, hover));
        scene.meshes.push(mesh_of(&b.0, b.1, SELECT_COLOR, hover));
      }
      let alpha = opts.opacity.iter().find(|o| o.0 == body.id).map(|o| o.1);
      for mesh in &mut scene.meshes[first..] {
        mesh.analyze = true;
        if let Some(a) = alpha {
          mesh.color[3] = a;
        }
        if let Some(pl) = &opts.section {
          clip_mesh(mesh, pl);
        }
      }
      if let Some(pl) = opts.section {
        let cap = match self.caps.get(&body.id) {
          Some((p, c)) if *p == pl => c.clone(),
          _ => {
            let c = section_cap(&body.shape, &pl, scene_size(s));
            self.caps.insert(body.id, (pl, c.clone()));
            c
          }
        };
        if let Some(c) = cap {
          let n = vec![(-pl.normal()).as_vec3().to_array(); c.positions.len()];
          let mut mesh = mesh_of(&TriMesh { normals: n, ..c }, Vec::new(), mix(base, [120, 60, 50, 255], 0.35), None);
          mesh.analyze = false;
          scene.meshes.push(mesh);
        }
      }
      if !opts.hide_edges {
        let edges = self.edges.entry(body.id).or_insert_with(|| {
          let tol = (body.diagonal() * 0.001).clamp(0.002, 0.5);
          body.shape.edge_lines(tol).unwrap_or_default()
        });
        for e in edges.iter() {
          let pick = Pick::Edge { body: body.id, edge: e.id };
          let points: Vec<[f32; 3]> = match &opts.section {
            Some(pl) => {
              let kept: Vec<_> = e.points.iter().filter(|p| (**p - pl.origin).dot(pl.normal()) <= 1e-9).collect();
              if kept.len() < 2 {
                continue;
              }
              kept.iter().map(|p| p.as_vec3().to_array()).collect()
            }
            None => e.points.iter().map(|p| p.as_vec3().to_array()).collect(),
          };
          let (color, width) = if selected.contains(&pick) {
            (SELECT_COLOR, 3.0)
          } else if opts.hover == Some(pick) {
            (HOVER_COLOR, 3.0)
          } else {
            (EDGE_COLOR, 1.0)
          };
          scene.lines.push(Polyline {
            points,
            color,
            width,
            id: picks.id(pick),
            on_top: false,
            dashed: false,
            behind: false,
          });
        }
      }
    }

    if opts.show_vertices {
      vertex_markers(&mut scene, &mut picks, s, opts, &selected);
    }

    // sketches
    let active = s.design.active_len();
    for f in s.design.features.iter().take(active) {
      let FeatureKind::Sketch { .. } = &f.kind else { continue };
      let Some(r) = s.model.sketches.get(&f.id) else { continue };
      let is_edit = s.edit_sketch == Some(f.id);
      let auto = !sketch_consumed(s, f.id) || opts.show_profiles_of.contains(&f.id);
      let show = !f.suppressed && !opts.hide_sketches && (is_edit || f.visible.unwrap_or(auto));
      if !show {
        continue;
      }
      // profiles (shaded regions)
      let profiles = if opts.hide_profiles_of.contains(&f.id) { &[][..] } else { &r.profiles[..] };
      for (i, prof) in profiles.iter().enumerate() {
        let pick = Pick::Profile { sketch: f.id, index: i };
        let mesh = self
          .profiles
          .entry((f.id, i))
          .or_insert_with(|| Shape::face(&r.plane, prof).ok().and_then(|face| face.tessellate(0.05).ok()));
        let Some(m) = mesh else { continue };
        let color = if selected.contains(&pick) {
          [120, 170, 240, 255]
        } else if opts.hover == Some(pick) {
          [250, 196, 120, 255]
        } else {
          PROFILE_COLOR
        };
        let id = picks.id(pick);
        // lift slightly off the plane so coplanar faces don't fight
        let lift = r.plane.normal().as_vec3() * 0.002;
        let mut pm = mesh_of(m, vec![id; m.indices.len() / 3], color, None);
        for p in pm.positions.iter_mut() {
          *p = (fox_kernel::glam::Vec3::from(*p) + lift).to_array();
        }
        scene.meshes.push(pm);
      }
      sketch_lines(
        &mut scene,
        &mut picks,
        f.id,
        &r.sketch,
        &r.plane,
        is_edit,
        &r.report.free_points,
        &selected,
        opts.hover,
      );
    }

    // construction planes
    for (id, pl) in &s.model.planes {
      if s.design.feature(*id).is_some_and(|f| f.suppressed) {
        continue;
      }
      let size = scene_size(s) * 0.35;
      let c = [
        pl.to_world([-size, -size]),
        pl.to_world([size, -size]),
        pl.to_world([size, size]),
        pl.to_world([-size, size]),
      ];
      scene.lines.push(Polyline {
        points: c.iter().chain(std::iter::once(&c[0])).map(|p| p.as_vec3().to_array()).collect(),
        color: [200, 140, 40, 255],
        width: 1.0,
        id: 0,
        on_top: false,
        dashed: true,
        behind: false,
      });
    }

    // construction axes (dashed, through the model), points and cosmetic threads
    let reach = scene_size(s) * 0.4;
    for (id, (o, d)) in &s.model.axes {
      if s.design.feature(*id).is_some_and(|f| f.suppressed) {
        continue;
      }
      scene.lines.push(Polyline {
        points: vec![(*o - *d * reach).as_vec3().to_array(), (*o + *d * reach).as_vec3().to_array()],
        color: [200, 140, 40, 255],
        width: 1.5,
        id: 0,
        on_top: false,
        dashed: true,
        behind: false,
      });
    }
    for (id, p) in &s.model.points {
      if s.design.feature(*id).is_some_and(|f| f.suppressed) {
        continue;
      }
      scene.markers.push(Marker {
        at: p.as_vec3().to_array(),
        color: [200, 140, 40, 255],
        size: 5.0,
        id: 0,
        on_top: true,
      });
    }
    for (_, helix) in &s.model.threads {
      scene.lines.push(Polyline {
        points: helix.iter().map(|p| p.as_vec3().to_array()).collect(),
        color: [90, 96, 108, 255],
        width: 1.0,
        id: 0,
        on_top: false,
        dashed: false,
        behind: false,
      });
    }
    // Project To Surface curves, drawn like sketch geometry on the faces
    for (id, curves) in &s.model.curves3d {
      if s.design.feature(*id).is_some_and(|f| f.suppressed || f.visible == Some(false)) {
        continue;
      }
      for line in curves.iter().filter_map(|c| c.edge_lines(0.02).ok()).flatten() {
        scene.lines.push(Polyline {
          points: line.points.iter().map(|p| p.as_vec3().to_array()).collect(),
          color: [200, 140, 40, 255],
          width: 1.5,
          id: 0,
          on_top: true,
          dashed: false,
          behind: false,
        });
      }
    }

    if !opts.hide_grid {
      grid(&mut scene, s);
    }
    if opts.toolpaths {
      toolpath_lines(&mut scene, s);
      simulated_stock(&mut scene, s);
    }
    scene.analysis = opts.analysis;
    (scene, picks)
  }
}

/// Whether a feature uses the sketch's profiles or curves (a used sketch hides itself unless
/// shown with `sketch.visible`).
pub fn sketch_consumed(s: &Session, id: FeatureId) -> bool {
  s.design.features.iter().take(s.design.active_len()).any(|g| match &g.kind {
    FeatureKind::Extrude { sketch, .. }
    | FeatureKind::Revolve { sketch, .. }
    | FeatureKind::Flange { sketch, .. }
    | FeatureKind::ContourFlange { sketch, .. }
    | FeatureKind::Hole { sketch, .. } => *sketch == id,
    FeatureKind::Sweep { sketch, path_sketch, .. } => *sketch == id || *path_sketch == id,
    FeatureKind::Loft { sections, .. } => sections.iter().any(|s| s.sketch == id),
    _ => false,
  })
}

/// Whether a sketch is drawn: shown or hidden with `sketch.visible`, else drawn until a feature
/// uses it, and always while it's being edited.
pub fn sketch_visible(s: &Session, id: FeatureId) -> bool {
  let Some(f) = s.design.feature(id) else { return false };
  !f.suppressed && (s.edit_sketch == Some(id) || f.visible.unwrap_or_else(|| !sketch_consumed(s, id)))
}

fn scene_size(s: &Session) -> f64 {
  s.model.bounds().map(|(a, b)| (b - a).length()).unwrap_or(100.0).max(50.0)
}

fn grid(scene: &mut Scene, s: &Session) {
  let plane = match s.edit_sketch.and_then(|e| s.model.sketches.get(&e)) {
    Some(r) => r.plane,
    None => Plane::XY,
  };
  let size = scene_size(s);
  let step = nice_step(size / 10.0);
  let n = ((size / step).ceil() as i64).clamp(2, 60);
  let ext = n as f64 * step;
  for i in -n..=n {
    let t = i as f64 * step;
    let major = i % 5 == 0;
    let col = if major { [190, 193, 198, 255] } else { [218, 220, 224, 255] };
    for (a, b) in [([t, -ext], [t, ext]), ([-ext, t], [ext, t])] {
      scene.lines.push(Polyline {
        points: vec![plane.to_world(a).as_vec3().to_array(), plane.to_world(b).as_vec3().to_array()],
        color: col,
        width: 1.0,
        id: 0,
        on_top: false,
        dashed: false,
        behind: true,
      });
    }
  }
  // origin axes: X and Y run across the whole grid, as in Fusion; Z is a short stub
  let l = ext as f32;
  let z = (ext * 0.25) as f32;
  for (a, b, col) in [
    ([-l, 0.0, 0.0], [l, 0.0, 0.0], [232, 96, 92, 255]),
    ([0.0, -l, 0.0], [0.0, l, 0.0], [96, 190, 96, 255]),
    ([0.0, 0.0, 0.0], [0.0, 0.0, z], [70, 110, 230, 255]),
  ] {
    scene.lines.push(Polyline {
      points: vec![a, b],
      color: col,
      width: 1.5,
      id: 0,
      on_top: false,
      dashed: false,
      behind: true,
    });
  }
}

/// 1, 2, 5 × 10^k
pub fn nice_step(raw: f64) -> f64 {
  if !(raw.is_finite() && raw > 0.0) {
    return 10.0;
  }
  let k = raw.log10().floor();
  let base = 10f64.powf(k);
  let m = raw / base;
  base
    * if m < 1.5 {
      1.0
    } else if m < 3.5 {
      2.0
    } else if m < 7.5 {
      5.0
    } else {
      10.0
    }
}

#[allow(clippy::too_many_arguments)]
fn sketch_lines(
  scene: &mut Scene,
  picks: &mut PickTable,
  sid: FeatureId,
  sk: &Sketch,
  plane: &Plane,
  editing: bool,
  free: &[PointId],
  selected: &[Pick],
  hover: Option<Pick>,
) {
  let w = |p: [f64; 2]| plane.to_world(p).as_vec3().to_array();
  for c in &sk.curves {
    let pts = curve_polyline(sk, &c.kind);
    if pts.len() < 2 {
      continue;
    }
    let pick = Pick::SketchCurve { sketch: sid, curve: c.id };
    let is_free = fox_sketch::curve_points(&c.kind).iter().any(|p| free.contains(p))
      || matches!(c.kind, CurveKind::Circle { .. }) && !fully_dimensioned_circle(sk, c.id, free);
    let color = if selected.contains(&pick) {
      SELECT_COLOR
    } else if hover == Some(pick) {
      HOVER_COLOR
    } else if !editing {
      SKETCH_IDLE
    } else if c.construction {
      SKETCH_CONSTRUCTION
    } else if is_free {
      SKETCH_FREE
    } else {
      SKETCH_FIXED
    };
    scene.lines.push(Polyline {
      points: pts.into_iter().map(w).collect(),
      color,
      width: if editing { 2.0 } else { 1.0 },
      id: if editing { picks.id(pick) } else { 0 },
      on_top: editing,
      dashed: c.construction,
      behind: false,
    });
  }
  // included 3D geometry, at its world position
  for c in &sk.curves3d {
    let pick = Pick::SketchCurve { sketch: sid, curve: c.id };
    let color = if selected.contains(&pick) {
      SELECT_COLOR
    } else if hover == Some(pick) {
      HOVER_COLOR
    } else if editing {
      SKETCH_FIXED
    } else {
      SKETCH_IDLE
    };
    scene.lines.push(Polyline {
      points: c.kind.polyline(48).into_iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect(),
      color,
      width: if editing { 2.0 } else { 1.0 },
      id: if editing { picks.id(pick) } else { 0 },
      on_top: editing,
      dashed: c.construction,
      behind: false,
    });
  }
  if editing {
    // spline tangent handles and Bézier handles: a thin line from the curve to each handle
    for c in &sk.curves {
      let mut legs: Vec<(PointId, PointId)> = Vec::new();
      match &c.kind {
        CurveKind::Spline { points, start_handle, end_handle, start_curvature, end_curvature } => {
          legs.extend(points.first().copied().zip(*start_handle));
          legs.extend(points.last().copied().zip(*end_handle));
          legs.extend(points.first().copied().zip(*start_curvature));
          legs.extend(points.last().copied().zip(*end_curvature));
        }
        CurveKind::Bezier { points } => {
          for k in (0..points.len().saturating_sub(1)).step_by(3) {
            legs.push((points[k], points[k + 1]));
            legs.extend(points.get(k + 3).map(|e| (*e, points[k + 2])));
          }
        }
        _ => {}
      }
      for (e, h) in legs {
        if let (Some(e), Some(h)) = (sk.pos(e), sk.pos(h)) {
          scene.lines.push(Polyline {
            points: vec![w(e), w(h)],
            color: SKETCH_CONSTRUCTION,
            width: 1.0,
            id: 0,
            on_top: true,
            dashed: true,
            behind: false,
          });
        }
      }
    }
    // a text's glyph points are its own business: only its anchor is shown
    let glyph_points: std::collections::HashSet<PointId> =
      sk.texts.iter().flat_map(|t| t.points.iter().copied()).collect();
    for p in sk.points.iter().filter(|p| !glyph_points.contains(&p.id)) {
      let pick = Pick::SketchPoint { sketch: sid, point: p.id };
      let color = if selected.contains(&pick) {
        SELECT_COLOR
      } else if hover == Some(pick) {
        HOVER_COLOR
      } else if p.id == fox_sketch::ORIGIN {
        [200, 60, 40, 255]
      } else if free.contains(&p.id) {
        SKETCH_FREE
      } else {
        SKETCH_FIXED
      };
      scene.markers.push(Marker { at: w([p.x, p.y]), color, size: 3.0, id: picks.id(pick), on_top: true });
    }
  }
}

/// A circle's radius is "free" when the solver reports it so; we approximate with the presence of
/// a radius/diameter/equal/tangent constraint once its centre is fixed.
fn fully_dimensioned_circle(sk: &Sketch, c: fox_sketch::CurveId, free: &[PointId]) -> bool {
  let centre_fixed = sk.curve_points(c).first().is_some_and(|p| !free.contains(p));
  centre_fixed
    && sk.constraints.iter().any(|k| match &k.kind {
      ConstraintKind::Radius { curve, .. } | ConstraintKind::Diameter { curve, .. } => *curve == c,
      ConstraintKind::Equal { a, b }
      | ConstraintKind::Tangent { a, b }
      | ConstraintKind::SymmetricCurves { a, b, .. } => *a == c || *b == c,
      _ => false,
    })
}

/// Flattened curve in sketch coordinates.
pub fn curve_polyline(sk: &Sketch, kind: &CurveKind) -> Vec<[f64; 2]> {
  match kind {
    CurveKind::Spline { .. }
    | CurveKind::ControlSpline { .. }
    | CurveKind::Bezier { .. }
    | CurveKind::EllipticalArc { .. }
    | CurveKind::Conic { .. }
    | CurveKind::QuinticSpline { .. } => {
      let bz = sk.kind_beziers(kind);
      let mut out = Vec::new();
      for b in &bz {
        for k in 0..24 {
          out.push(fox_sketch::bezier_at(b, k as f64 / 24.0));
        }
      }
      out.extend(bz.last().map(|b| b[3]));
      out
    }
    CurveKind::Ellipse { center, major, minor } => match (sk.pos(*center), sk.pos(*major)) {
      (Some(c), Some(m)) => {
        let bz = fox_sketch::ellipse_beziers(c, m, *minor);
        let mut out: Vec<[f64; 2]> =
          bz.iter().flat_map(|b| (0..16).map(move |k| fox_sketch::bezier_at(b, k as f64 / 16.0))).collect();
        out.extend(bz.first().map(|b| b[0]));
        out
      }
      _ => vec![],
    },
    CurveKind::Line { a, b } => match (sk.pos(*a), sk.pos(*b)) {
      (Some(a), Some(b)) => vec![a, b],
      _ => vec![],
    },
    CurveKind::Circle { center, radius } => {
      let Some(c) = sk.pos(*center) else { return vec![] };
      (0..=96)
        .map(|i| {
          let t = std::f64::consts::TAU * i as f64 / 96.0;
          [c[0] + radius * t.cos(), c[1] + radius * t.sin()]
        })
        .collect()
    }
    CurveKind::Arc { center, start, end } => {
      let (Some(c), Some(s), Some(e)) = (sk.pos(*center), sk.pos(*start), sk.pos(*end)) else { return vec![] };
      let r = (s[0] - c[0]).hypot(s[1] - c[1]);
      let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
      let a1 = (e[1] - c[1]).atan2(e[0] - c[0]);
      let mut sweep = (a1 - a0).rem_euclid(std::f64::consts::TAU);
      if sweep < 1e-12 {
        sweep = std::f64::consts::TAU;
      }
      let n = ((sweep / std::f64::consts::TAU) * 96.0).ceil().max(4.0) as usize;
      (0..=n)
        .map(|i| {
          let t = a0 + sweep * i as f64 / n as f64;
          [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect()
    }
  }
}

fn mesh_of(m: &TriMesh, tri_ids: Vec<u32>, color: [u8; 4], hover: Option<u32>) -> Mesh {
  Mesh {
    positions: m.positions.clone(),
    normals: m.normals.clone(),
    indices: m.indices.clone(),
    tri_ids,
    id: 0,
    color,
    highlight: hover,
    highlight_color: HOVER_COLOR,
    colors: Vec::new(),
    analyze: false,
  }
}

/// Cuts away the triangles (and parts of triangles) in front of a section plane (on its
/// normal's side), keeping pick ids.
fn clip_mesh(m: &mut Mesh, pl: &Plane) {
  let n = pl.normal();
  let side = |p: [f32; 3]| (DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64) - pl.origin).dot(n);
  let mut indices = Vec::with_capacity(m.indices.len());
  let mut ids = Vec::with_capacity(m.tri_ids.len());
  let per_tri = !m.tri_ids.is_empty();
  let tris = m.indices.len() / 3;
  for t in 0..tris {
    let tri = [m.indices[t * 3], m.indices[t * 3 + 1], m.indices[t * 3 + 2]];
    let Some(d) = tri.iter().map(|&i| m.positions.get(i as usize).map(|p| side(*p))).collect::<Option<Vec<f64>>>()
    else {
      continue;
    };
    let id = m.tri_ids.get(t).copied();
    if d.iter().all(|x| *x <= 0.0) {
      indices.extend_from_slice(&tri);
      ids.extend(id);
      continue;
    }
    if d.iter().all(|x| *x > 0.0) {
      continue;
    }
    // split: walk the edges keeping the inside part (a triangle or a quad)
    let mut poly: Vec<u32> = Vec::new();
    for k in 0..3 {
      let (a, b) = (tri[k], tri[(k + 1) % 3]);
      let (da, db) = (d[k], d[(k + 1) % 3]);
      if da <= 0.0 {
        poly.push(a);
      }
      if (da <= 0.0) != (db <= 0.0) {
        let s = (da / (da - db)) as f32;
        let (pa, pb) = (m.positions[a as usize], m.positions[b as usize]);
        let (na, nb) = (
          m.normals.get(a as usize).copied().unwrap_or([0.0, 0.0, 1.0]),
          m.normals.get(b as usize).copied().unwrap_or([0.0, 0.0, 1.0]),
        );
        let lerp =
          |x: [f32; 3], y: [f32; 3]| [x[0] + (y[0] - x[0]) * s, x[1] + (y[1] - x[1]) * s, x[2] + (y[2] - x[2]) * s];
        m.positions.push(lerp(pa, pb));
        m.normals.push(lerp(na, nb));
        if !m.colors.is_empty() {
          let (ca, cb) = (m.colors[a as usize], m.colors[b as usize]);
          m.colors.push(if s < 0.5 { ca } else { cb });
        }
        poly.push((m.positions.len() - 1) as u32);
      }
    }
    for k in 1..poly.len().saturating_sub(1) {
      indices.extend_from_slice(&[poly[0], poly[k], poly[k + 1]]);
      ids.extend(id);
    }
  }
  m.indices = indices;
  if per_tri {
    m.tri_ids = ids;
  }
}

/// The cut face where a section plane passes through a body, tessellated.
fn section_cap(shape: &Shape, pl: &Plane, size: f64) -> Option<TriMesh> {
  use fox_sketch::{Loop, Profile, Seg};
  let r = size * 4.0;
  let sq = Loop {
    segs: vec![
      Seg::Line { a: [-r, -r], b: [r, -r] },
      Seg::Line { a: [r, -r], b: [r, r] },
      Seg::Line { a: [r, r], b: [-r, r] },
      Seg::Line { a: [-r, r], b: [-r, -r] },
    ],
    src: Vec::new(),
  };
  let face = Shape::face(pl, &Profile { outer: sq, holes: vec![] }).ok()?;
  let cut = shape.section_inside(&face).ok()?;
  cut.tessellate((size * 0.001).clamp(0.01, 0.5)).ok()
}

/// Per-vertex colours from how sharply the surface curves there: blue flat, through green and
/// yellow, to red tight.
fn curvature_colors(m: &TriMesh) -> Vec<[u8; 4]> {
  let pos: Vec<DVec3> = m.positions.iter().map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect();
  let nor: Vec<DVec3> = m.normals.iter().map(|n| DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)).collect();
  let mut sum = vec![0.0f64; pos.len()];
  let mut cnt = vec![0.0f64; pos.len()];
  for t in m.indices.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      let (a, b) = (a as usize, b as usize);
      let (Some(pa), Some(pb), Some(na), Some(nb)) = (pos.get(a), pos.get(b), nor.get(a), nor.get(b)) else {
        continue;
      };
      let l = (*pb - *pa).length();
      if l > 1e-9 {
        // normal turn per unit length: about 1/radius
        let k = (*nb - *na).length() / l;
        for v in [a, b] {
          sum[v] += k;
          cnt[v] += 1.0;
        }
      }
    }
  }
  let ks: Vec<f64> = sum.iter().zip(&cnt).map(|(s, c)| if *c > 0.0 { s / c } else { 0.0 }).collect();
  let mut sorted: Vec<f64> = ks.iter().copied().filter(|k| *k > 1e-6).collect();
  sorted.sort_by(f64::total_cmp);
  let top = sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(1.0).max(1e-6);
  ks.iter()
    .map(|k| {
      let t = (k / top).clamp(0.0, 1.0) as f32;
      let ramp = [[40.0, 80.0, 220.0], [40.0, 190.0, 90.0], [240.0, 210.0, 40.0], [225.0, 50.0, 40.0]];
      let x = t * 3.0;
      let i = (x.floor() as usize).min(2);
      let f = x - i as f32;
      let c: [f32; 3] = [0, 1, 2].map(|j| ramp[i][j] + (ramp[i + 1][j] - ramp[i][j]) * f);
      [c[0] as u8, c[1] as u8, c[2] as u8, 255]
    })
    .collect()
}

/// Splits a mesh's triangles into (not in `ids`, in `ids`) keeping per-triangle pick ids.
fn split_mesh(m: &TriMesh, tri_ids: &[u32], ids: &[u32]) -> ((TriMesh, Vec<u32>), (TriMesh, Vec<u32>)) {
  let mut a =
    (TriMesh { positions: m.positions.clone(), normals: m.normals.clone(), ..Default::default() }, Vec::new());
  let mut b =
    (TriMesh { positions: m.positions.clone(), normals: m.normals.clone(), ..Default::default() }, Vec::new());
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    let id = tri_ids.get(t).copied().unwrap_or(0);
    let target = if ids.contains(&id) { &mut b } else { &mut a };
    target.0.indices.extend_from_slice(tri);
    target.1.push(id);
  }
  (a, b)
}

/// Splits a mesh into one per colour (`color_of(triangle, pick id)`), keeping every vertex so
/// the indices stay valid.
fn split_by_color(
  m: &TriMesh,
  tri_ids: &[u32],
  color_of: impl Fn(usize, u32) -> [u8; 4],
) -> Vec<(TriMesh, Vec<u32>, [u8; 4])> {
  let mut out: Vec<(TriMesh, Vec<u32>, [u8; 4])> = Vec::new();
  for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
    let id = tri_ids.get(t).copied().unwrap_or(0);
    let c = color_of(t, id);
    let i = match out.iter().position(|g| g.2 == c) {
      Some(i) => i,
      None => {
        let mesh = TriMesh { positions: m.positions.clone(), normals: m.normals.clone(), ..Default::default() };
        out.push((mesh, Vec::new(), c));
        out.len() - 1
      }
    };
    out[i].0.indices.extend_from_slice(tri);
    out[i].1.push(id);
  }
  out
}

fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
  let mut o = [255u8; 4];
  for i in 0..3 {
    o[i] = (a[i] as f32 * (1.0 - t) + b[i] as f32 * t).round() as u8;
  }
  o
}

/// Projects a world point into a sketch plane (for clicking in the viewport).
pub fn to_sketch(plane: &Plane, p: DVec3) -> [f64; 2] {
  plane.to_local(p)
}

/// Renders the design to a PNG for `view.render`: `{width?, height?, view?, yaw?, pitch?,
/// target?, distance?, ortho?, fov?, bodies?, sketches?, grid?, edges?, background?}` plus the
/// section and analysis options of [`view_options`]. Without
/// `distance` the camera fits the bodies shown.
pub fn render_png(s: &Session, p: &serde_json::Value) -> Result<Vec<u8>, crate::CmdError> {
  use crate::CmdError;
  use fox_render::{Camera, StandardView};
  use serde_json::Value;
  let size = |k: &str, d: u64| p.get(k).and_then(Value::as_u64).unwrap_or(d).clamp(16, 8192) as u32;
  let (w, h) = (size("width", 1000), size("height", 700));
  let flag = |k: &str, d: bool| p.get(k).and_then(Value::as_bool).unwrap_or(d);
  let scope = s.model.params.clone();
  let angle = |k: &str| -> Result<Option<f64>, CmdError> {
    crate::commands::opt_expr(p, k)
      .map(|e| fox_expr::eval(&e, fox_expr::Kind::Angle, &scope))
      .transpose()
      .map_err(CmdError::from)
  };
  let mut cam = Camera::default();
  if let Some(v) = p.get("view").and_then(Value::as_str) {
    cam.set_view(StandardView::parse(v).ok_or_else(|| {
      CmdError::new(format!("unknown view '{v}' (home, iso, top, bottom, front, back, left, right)"))
    })?);
  }
  if let Some(y) = angle("yaw")? {
    cam.yaw = y;
  }
  if let Some(pt) = angle("pitch")? {
    cam.pitch = pt.clamp(-89.9f64.to_radians(), 89.9f64.to_radians());
  }
  if let Some(f) = angle("fov")? {
    cam.fov = f.clamp(1f64.to_radians(), 120f64.to_radians());
  }
  cam.ortho = flag("ortho", cam.ortho);
  let only: Option<Vec<BodyId>> =
    p.get("bodies").map(|_| crate::commands::ids(p, "bodies").into_iter().map(BodyId).collect());
  if let Some(only) = &only
    && let Some(b) = only.iter().find(|b| s.model.body(**b).is_none())
  {
    return Err(CmdError::new(format!("no body {}", b.0)));
  }
  let shown = |id: BodyId| match &only {
    Some(o) => o.contains(&id),
    None => s.body_visible(id),
  };
  let mut bounds: Option<(DVec3, DVec3)> = None;
  for b in s.model.bodies.iter().filter(|b| shown(b.id)) {
    if let Ok(m) = b.shape.mass() {
      let (lo, hi) = bounds.get_or_insert((m.bbox_min, m.bbox_max));
      *lo = lo.min(m.bbox_min);
      *hi = hi.max(m.bbox_max);
    }
  }
  let (lo, hi) = bounds.or_else(|| s.model.bounds()).unwrap_or((DVec3::splat(-50.0), DVec3::splat(50.0)));
  cam.fit(lo, hi, w as f64 / h as f64);
  if p.get("target").is_some() {
    cam.target = crate::commands::req_vec3(p, "target")?;
  }
  if let Some(d) = crate::commands::opt_expr(p, "distance") {
    cam.distance = fox_expr::eval(&d, fox_expr::Kind::Length, &scope)?.max(1e-3);
  }
  let opts = ViewOptions {
    hide_sketches: !flag("sketches", false),
    hide_grid: !flag("grid", false),
    hide_edges: !flag("edges", true),
    only_bodies: only,
    ..view_options(s, p).map_err(CmdError::new)?
  };
  let (mut scene, _) = SceneBuilder::default().build(s, &opts);
  if let Some(bg) = p.get("background") {
    let c = crate::commands::parse_color(bg)?;
    scene.background_top = [c[0], c[1], c[2], 255];
    scene.background_bottom = scene.background_top;
  }
  fox_render::render(&scene, &cam, w, h).to_png().map_err(CmdError::new)
}

/// Every body face a ray passes through, nearest first (one hit per face): for "Select Other",
/// which lists what lies under the cursor behind the front-most face.
pub fn ray_hits(s: &Session, origin: DVec3, dir: DVec3) -> Vec<(f64, Pick)> {
  let mut out: Vec<(f64, Pick)> = Vec::new();
  for body in s.model.bodies.iter().filter(|b| s.body_visible(b.id)) {
    let Some(m) = body.mesh() else { continue };
    let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
    for (t, tri) in m.indices.chunks_exact(3).enumerate() {
      let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) else { continue };
      let Some(d) = ray_triangle(origin, dir, a, b, c) else { continue };
      let Some(&face) = m.tri_face.get(t) else { continue };
      let pick = Pick::Face { body: body.id, face };
      match out.iter_mut().find(|(_, p)| *p == pick) {
        Some(e) => e.0 = e.0.min(d),
        None => out.push((d, pick)),
      }
    }
  }
  out.sort_by(|a, b| a.0.total_cmp(&b.0));
  out
}

/// Möller–Trumbore: distance along the ray to the triangle, if it is hit in front.
fn ray_triangle(o: DVec3, d: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
  let (e1, e2) = (b - a, c - a);
  let p = d.cross(e2);
  let det = e1.dot(p);
  if det.abs() < 1e-12 {
    return None;
  }
  let inv = 1.0 / det;
  let t0 = o - a;
  let u = t0.dot(p) * inv;
  if !(0.0..=1.0).contains(&u) {
    return None;
  }
  let q = t0.cross(e1);
  let v = d.dot(q) * inv;
  if v < 0.0 || u + v > 1.0 {
    return None;
  }
  let t = e2.dot(q) * inv;
  (t > 0.0).then_some(t)
}

/// A world plane from a JSON plane value: "xy" | "xz" | "yz" | {"feature": construction plane}
/// | {"body", "face"} (a planar face), moved `offset` along its normal and flipped if asked.
pub fn view_plane(s: &Session, v: &serde_json::Value, offset: f64, flip: bool) -> Result<Plane, String> {
  let base = if let Some(n) = v.as_str() {
    match n.to_ascii_lowercase().as_str() {
      "xy" | "top" => Plane::XY,
      "xz" | "front" => Plane::XZ,
      "yz" | "right" => Plane::YZ,
      o => return Err(format!("unknown plane '{o}'")),
    }
  } else if let Some(f) = v.get("feature").and_then(serde_json::Value::as_u64) {
    *s.model.planes.get(&FeatureId(f as u32)).ok_or("that feature is not a construction plane")?
  } else {
    let body =
      BodyId(v.get("body").and_then(serde_json::Value::as_u64).ok_or("a plane needs 'body' and 'face'")? as u32);
    let face = v.get("face").and_then(serde_json::Value::as_u64).ok_or("a plane needs 'face'")? as u32;
    let f = s.model.body(body).and_then(|b| b.faces().iter().find(|f| f.id == face).copied()).ok_or("no such face")?;
    f.plane().ok_or("the face is not planar")?
  };
  let pl = base.offset(offset);
  Ok(if flip { Plane { origin: pl.origin, x: pl.x, y: -pl.y } } else { pl })
}

/// View options from render parameters: `section: {plane, offset?, flip?}` and
/// `analysis: zebra | draft | accessibility | curvature` (with `pull`: "x" | "y" | "z" | [x,y,z]
/// and `angle` in degrees for draft).
pub fn view_options(s: &Session, p: &serde_json::Value) -> Result<ViewOptions, String> {
  let mut o = ViewOptions::default();
  if let Some(sec) = p.get("section") {
    let plane = sec.get("plane").cloned().unwrap_or(serde_json::json!("xz"));
    let offset = sec.get("offset").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let flip = sec.get("flip").and_then(serde_json::Value::as_bool).unwrap_or(false);
    o.section = Some(view_plane(s, &plane, offset, flip)?);
  }
  o.toolpaths = p.get("toolpaths").is_some_and(|t| !t.is_null() && t != &serde_json::json!(false));
  let pull = match p.get("pull") {
    Some(serde_json::Value::String(a)) => match a.as_str() {
      "x" => [1.0, 0.0, 0.0],
      "y" => [0.0, 1.0, 0.0],
      _ => [0.0, 0.0, 1.0],
    },
    Some(serde_json::Value::Array(a)) if a.len() == 3 => [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.0) as f32),
    _ => [0.0, 0.0, 1.0],
  };
  match p.get("analysis").and_then(serde_json::Value::as_str) {
    None => {}
    Some("zebra") => o.analysis = Some(fox_render::Analysis::Zebra { stripes: 24.0 }),
    Some("draft") => {
      let a = p.get("angle").and_then(serde_json::Value::as_f64).unwrap_or(1.0).to_radians() as f32;
      o.analysis = Some(fox_render::Analysis::Draft { pull, angle: a })
    }
    Some("accessibility") => o.analysis = Some(fox_render::Analysis::Accessibility { dir: pull }),
    Some("curvature") => o.curvature = true,
    Some(other) => return Err(format!("unknown analysis '{other}' (zebra, draft, accessibility, curvature)")),
  }
  Ok(o)
}

/// Toolpaths (cutting moves blue, rapids dashed orange) and the stock box of their setup.
fn toolpath_lines(scene: &mut Scene, s: &Session) {
  let line = |scene: &mut Scene, pts: Vec<[f32; 3]>, color: [u8; 4], dashed: bool, width: f32| {
    scene.lines.push(Polyline { points: pts, color, width, id: 0, on_top: false, dashed, behind: false });
  };
  if let Some((id, _)) = s.toolpaths_of
    && let Some(st) = s.design.cam.iter().find(|c| c.id == id)
    && let Ok((lo, hi)) = fox_model::cam::stock(st, &s.model)
  {
    let c = |x: f64, y: f64, z: f64| DVec3::new(x, y, z).as_vec3().to_array();
    for z in [lo.z, hi.z] {
      line(
        scene,
        vec![c(lo.x, lo.y, z), c(hi.x, lo.y, z), c(hi.x, hi.y, z), c(lo.x, hi.y, z), c(lo.x, lo.y, z)],
        [120, 130, 150, 255],
        true,
        1.0,
      );
    }
    for (x, y) in [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y)] {
      line(scene, vec![c(x, y, lo.z), c(x, y, hi.z)], [120, 130, 150, 255], true, 1.0);
    }
  }
  for tp in &s.toolpaths {
    let mut run: Vec<[f32; 3]> = Vec::new();
    let mut rapid = false;
    let mut at: Option<DVec3> = None;
    for m in &tp.moves {
      let p = m.to.as_vec3().to_array();
      if m.rapid != rapid && !run.is_empty() {
        let color = if rapid { [235, 150, 40, 255] } else { [30, 100, 230, 255] };
        line(scene, std::mem::take(&mut run), color, rapid, if rapid { 1.0 } else { 1.5 });
      }
      if run.is_empty()
        && let Some(a) = at
      {
        run.push(a.as_vec3().to_array());
      }
      rapid = m.rapid;
      run.push(p);
      at = Some(m.to);
    }
    if run.len() > 1 {
      let color = if rapid { [235, 150, 40, 255] } else { [30, 100, 230, 255] };
      line(scene, run, color, rapid, if rapid { 1.0 } else { 1.5 });
    }
  }
}

/// The simulated stock (`cam.simulate`) while it matches the design: gouges in red.
fn simulated_stock(scene: &mut Scene, s: &Session) {
  let Some((_, rev, sim)) = &s.simulation else { return };
  if *rev != s.revision {
    return;
  }
  let m = sim.mesh(220);
  // a hair above the part, so machined faces show the stock's colour instead of flickering
  let positions = m.positions.iter().map(|p| [p[0], p[1], p[2] + 0.005]).collect();
  scene.meshes.push(Mesh {
    positions,
    normals: m.normals,
    indices: m.indices,
    tri_ids: Vec::new(),
    id: 0,
    color: [196, 170, 120, 255],
    highlight: None,
    highlight_color: [0, 0, 0, 0],
    colors: m.colors,
    analyze: false,
  });
}

// ---------------------------------------------------------------- vertices, joint glyphs

/// A body's vertices: one `(edge, end, position)` per distinct edge end point, in edge order.
pub fn body_vertices(body: &fox_model::regen::Body) -> Vec<(u32, bool, DVec3)> {
  let tol = (body.diagonal() * 1e-7).max(1e-9);
  let mut out: Vec<(u32, bool, DVec3)> = Vec::new();
  for e in body.edges() {
    for (end, p) in [(false, e.start), (true, e.end)] {
      if !out.iter().any(|v| v.2.distance(p) <= tol) {
        out.push((e.id, end, p));
      }
    }
  }
  out
}

/// Where a vertex pick lies.
pub fn vertex_pos(s: &Session, body: BodyId, edge: u32, end: bool) -> Option<DVec3> {
  let e = s.model.body(body)?.edges().iter().find(|e| e.id == edge)?;
  Some(if end { e.end } else { e.start })
}

const VERTEX_COLOR: [u8; 4] = [40, 44, 52, 255];

fn vertex_markers(scene: &mut Scene, picks: &mut PickTable, s: &Session, opts: &ViewOptions, selected: &[Pick]) {
  for body in &s.model.bodies {
    let shown = match &opts.only_bodies {
      Some(only) => only.contains(&body.id),
      None => s.body_visible(body.id),
    };
    if !shown {
      continue;
    }
    for (edge, end, p) in body_vertices(body) {
      if opts.section.is_some_and(|pl| (p - pl.origin).dot(pl.normal()) > 1e-9) {
        continue;
      }
      let pick = Pick::Vertex { body: body.id, edge, end };
      let (color, size) = if selected.contains(&pick) {
        (SELECT_COLOR, 4.5)
      } else if opts.hover == Some(pick) {
        (HOVER_COLOR, 4.5)
      } else {
        (VERTEX_COLOR, 3.0)
      };
      scene.markers.push(Marker { at: p.as_vec3().to_array(), color, size, id: picks.id(pick), on_top: false });
    }
  }
}

/// A saved joint origin or a joint, drawn as a glyph the viewport can pick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointMark {
  pub feature: FeatureId,
  /// A joint (placed at its fixed side's origin) rather than a saved joint origin.
  pub joint: bool,
  /// X, Y, Z axes and origin (world).
  pub frame: fox_kernel::glam::DMat4,
}

/// The saved joint origins and joints of the active timeline, where they are now.
pub fn joint_marks(s: &Session) -> Vec<JointMark> {
  let active = s.design.active_len();
  let mut out = Vec::new();
  for f in s.design.features.iter().take(active).filter(|f| !f.suppressed) {
    match &f.kind {
      FeatureKind::JointOrigin { .. } => {
        if let Some((_, m)) = s.model.joint_origins.get(&f.id) {
          out.push(JointMark { feature: f.id, joint: false, frame: *m });
        }
      }
      FeatureKind::Joint { fixed_origin, angle, offset, .. } => {
        let Some(base) = geometry_frame(s, fixed_origin) else { continue };
        let len = |e: &str| fox_expr::eval(e, fox_expr::Kind::Length, &s.model.params).unwrap_or(0.0);
        let ang = fox_expr::eval(angle, fox_expr::Kind::Angle, &s.model.params).unwrap_or(0.0);
        let m = base
          * fox_kernel::glam::DMat4::from_translation(DVec3::new(0.0, 0.0, len(offset)))
          * fox_kernel::glam::DMat4::from_rotation_z(ang);
        out.push(JointMark { feature: f.id, joint: true, frame: m });
      }
      _ => {}
    }
  }
  out
}

/// The frame a joint geometry names, on the bodies as they are now.
pub fn geometry_frame(s: &Session, g: &fox_model::assembly::JointGeometry) -> Option<fox_kernel::glam::DMat4> {
  use fox_model::assembly::{self, JointGeometry};
  match g {
    JointGeometry::Origin { origin } => s.model.joint_origins.get(origin).map(|(_, m)| *m),
    JointGeometry::Point { point, z, x } => assembly::frame(DVec3::from(*point), DVec3::from(*z), DVec3::from(*x)).ok(),
    JointGeometry::Face { face } => {
      let b = s.model.body(face.body)?;
      let id = face.resolve(b.faces(), b.mesh(), b.diagonal())?;
      assembly::face_frame(b.faces().iter().find(|f| f.id == id)?).ok()
    }
    JointGeometry::Edge { edge } => {
      let b = s.model.body(edge.body)?;
      let id = edge.resolve(b.edges(), b.faces(), b.diagonal())?;
      assembly::edge_frame(b.edges().iter().find(|e| e.id == id)?, b.faces()).ok()
    }
  }
}
