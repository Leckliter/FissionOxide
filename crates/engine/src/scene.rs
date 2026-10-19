//! Turns the session into a render scene (bodies, edges, sketches, grid) with pick ids, and maps
//! pick ids back to what was under the cursor.

use std::collections::HashMap;

use clf_kernel::glam::DVec3;
use clf_kernel::{EdgeLine, Plane, Shape, TriMesh};
use clf_model::{BodyId, FeatureId, FeatureKind};
use clf_render::{Marker, Mesh, Polyline, Scene};
use clf_sketch::{ConstraintKind, CurveKind, PointId, Sketch};

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
  Face { body: BodyId, face: u32 },
  Edge { body: BodyId, edge: u32 },
  SketchPoint { sketch: FeatureId, point: PointId },
  SketchCurve { sketch: FeatureId, curve: clf_sketch::CurveId },
  Profile { sketch: FeatureId, index: usize },
}

impl Pick {
  pub fn to_sel(self) -> Sel {
    match self {
      Pick::Face { body, face } => Sel::Face { body, face },
      Pick::Edge { body, edge } => Sel::Edge { body, edge },
      Pick::SketchPoint { sketch, point } => Sel::SketchPoint { sketch, point },
      Pick::SketchCurve { sketch, curve } => Sel::SketchCurve { sketch, curve },
      Pick::Profile { sketch, index } => Sel::Profile { sketch, index },
    }
  }

  pub fn from_sel(s: &Sel) -> Option<Pick> {
    Some(match *s {
      Sel::Face { body, face } => Pick::Face { body, face },
      Sel::Edge { body, edge } => Pick::Edge { body, edge },
      Sel::SketchPoint { sketch, point } => Pick::SketchPoint { sketch, point },
      Sel::SketchCurve { sketch, curve } => Pick::SketchCurve { sketch, curve },
      Sel::Profile { sketch, index } => Pick::Profile { sketch, index },
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
  pub hide_edges: bool,
  pub hide_grid: bool,
  /// Profiles are shown (and pickable) for these sketches even if consumed.
  pub show_profiles_of: Vec<FeatureId>,
}

/// Caches tessellations between frames (keyed by session revision).
#[derive(Default)]
pub struct SceneBuilder {
  revision: u64,
  edges: HashMap<BodyId, Vec<EdgeLine>>,
  profiles: HashMap<(FeatureId, usize), Option<TriMesh>>,
}

impl SceneBuilder {
  pub fn build(&mut self, s: &Session, opts: &ViewOptions) -> (Scene, PickTable) {
    if self.revision != s.revision {
      self.edges.clear();
      self.profiles.clear();
      self.revision = s.revision;
    }
    let mut picks = PickTable::default();
    let mut scene =
      Scene { background_top: [250, 250, 251, 255], background_bottom: [234, 236, 239, 255], ..Default::default() };
    let selected: Vec<Pick> = s.selection.iter().filter_map(Pick::from_sel).collect();
    let selected_bodies: Vec<BodyId> =
      s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(*body) } else { None }).collect();
    let editing = s.edit_sketch.is_some();

    // bodies
    for body in &s.model.bodies {
      if !s.body_visible(body.id) {
        continue;
      }
      let Some(m) = body.mesh() else { continue };
      let base =
        s.design.body_settings(body.id).and_then(|b| b.color).map(|c| [c[0], c[1], c[2], 255]).unwrap_or(BODY_COLOR);
      let base = if selected_bodies.contains(&body.id) { mix(base, SELECT_COLOR, 0.45) } else { base };
      let base = if editing { mix(base, [255, 255, 255, 255], 0.35) } else { base };
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
      if sel_faces.is_empty() {
        scene.meshes.push(mesh_of(m, tri_ids, base, hover));
      } else {
        // split selected faces into a second mesh so they draw in the selection colour
        let (a, b) = split_mesh(m, &tri_ids, &sel_faces);
        scene.meshes.push(mesh_of(&a.0, a.1, base, hover));
        scene.meshes.push(mesh_of(&b.0, b.1, SELECT_COLOR, hover));
      }
      if !opts.hide_edges {
        let edges = self.edges.entry(body.id).or_insert_with(|| {
          let tol = (body.diagonal() * 0.001).clamp(0.002, 0.5);
          body.shape.edge_lines(tol).unwrap_or_default()
        });
        for e in edges.iter() {
          let pick = Pick::Edge { body: body.id, edge: e.id };
          let (color, width) = if selected.contains(&pick) {
            (SELECT_COLOR, 3.0)
          } else if opts.hover == Some(pick) {
            (HOVER_COLOR, 3.0)
          } else {
            (EDGE_COLOR, 1.0)
          };
          scene.lines.push(Polyline {
            points: e.points.iter().map(|p| p.as_vec3().to_array()).collect(),
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

    // sketches
    let active = s.design.active_len();
    for f in s.design.features.iter().take(active) {
      let FeatureKind::Sketch { .. } = &f.kind else { continue };
      let Some(r) = s.model.sketches.get(&f.id) else { continue };
      let is_edit = s.edit_sketch == Some(f.id);
      let consumed = s.design.features.iter().take(active).any(|g| match &g.kind {
        FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } => *sketch == f.id,
        _ => false,
      });
      let show = !f.suppressed && (is_edit || !consumed || opts.show_profiles_of.contains(&f.id));
      if !show {
        continue;
      }
      // profiles (shaded regions)
      for (i, prof) in r.profiles.iter().enumerate() {
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
          *p = (clf_kernel::glam::Vec3::from(*p) + lift).to_array();
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

    if !opts.hide_grid {
      grid(&mut scene, s);
    }
    (scene, picks)
  }
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
    let is_free = clf_sketch::curve_points(&c.kind).iter().any(|p| free.contains(p))
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
  if editing {
    for p in &sk.points {
      let pick = Pick::SketchPoint { sketch: sid, point: p.id };
      let color = if selected.contains(&pick) {
        SELECT_COLOR
      } else if hover == Some(pick) {
        HOVER_COLOR
      } else if p.id == clf_sketch::ORIGIN {
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
fn fully_dimensioned_circle(sk: &Sketch, c: clf_sketch::CurveId, free: &[PointId]) -> bool {
  let centre_fixed = sk.curve_points(c).first().is_some_and(|p| !free.contains(p));
  centre_fixed
    && sk.constraints.iter().any(|k| match &k.kind {
      ConstraintKind::Radius { curve, .. } | ConstraintKind::Diameter { curve, .. } => *curve == c,
      ConstraintKind::Equal { a, b } | ConstraintKind::Tangent { a, b } => *a == c || *b == c,
      _ => false,
    })
}

/// Flattened curve in sketch coordinates.
pub fn curve_polyline(sk: &Sketch, kind: &CurveKind) -> Vec<[f64; 2]> {
  match kind {
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
  }
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
