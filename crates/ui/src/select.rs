//! Selection behaviour, as in Fusion: selection priority (faces, edges, bodies or components),
//! selection filters, Select Other (everything under the cursor, front to back), window,
//! freeform (lasso) and paint selection, and picking joints and saved joint origins.

use std::collections::HashMap;

use egui::{Pos2, RichText};
use fox_engine::kernel::glam::DVec3;
use fox_engine::kernel::{CurveKind, EdgeInfo, FaceInfo, SurfaceKind};
use fox_engine::model::BodyId;
use fox_engine::scene::Pick;
use fox_engine::{Sel, Session};

use crate::{App, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
  Face,
  Edge,
  Body,
  Component,
}

/// Fusion's selection filter types, in its menu order: the key the menu toggles and the name
/// it shows. Every type is on by default.
pub const FILTER_TYPES: &[(&str, &str)] = &[
  ("bodies", "Bodies"),
  ("edges", "Body Edges"),
  ("faces", "Body Faces"),
  ("vertices", "Body Vertices"),
  ("canvas", "Canvas"),
  ("components", "Components"),
  ("construction", "Construction Geometry"),
  ("coordinate_systems", "Coordinate Systems"),
  ("custom_graphics", "Custom Graphics"),
  ("decal", "Decal"),
  ("dimension", "Dimension"),
  ("features", "Features"),
  ("joint_origins", "Joint Origins"),
  ("joints", "Joints"),
  ("mesh_bodies", "Mesh Bodies"),
  ("mesh_face_groups", "Mesh Face Groups"),
  ("mesh_faces", "Mesh Faces"),
  ("pmi", "PMI"),
  ("sketch_curves", "Sketch Curves"),
  ("sketch_constraints", "Sketch Geometry Constraint"),
  ("sketch_points", "Sketch Points"),
  ("profiles", "Sketch Profiles"),
  ("sketch_surface", "Sketch Surface"),
  ("tspline", "T-Spline Body"),
  ("text", "Text Selection"),
];

/// The filter keys with no named field: kept on or off for the menu, picked by nothing yet.
const OTHER_FILTERS: [&str; 16] = [
  "canvas",
  "coordinate_systems",
  "custom_graphics",
  "decal",
  "dimension",
  "features",
  "joint_origins",
  "joints",
  "mesh_bodies",
  "mesh_face_groups",
  "mesh_faces",
  "pmi",
  "sketch_constraints",
  "sketch_surface",
  "tspline",
  "text",
];

/// What can be picked; everything by default, as Fusion's Selection Filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filters {
  pub bodies: bool,
  pub faces: bool,
  pub edges: bool,
  /// Sketch curves.
  pub sketches: bool,
  pub sketch_points: bool,
  pub profiles: bool,
  /// Body vertices (Fusion's Body Vertices filter): on by default, as in Fusion; a vertex draws
  /// a mark only while hovered or selected.
  pub vertices: bool,
  /// Construction planes, axes and points.
  pub construction: bool,
  /// Whole components, under the component priority.
  pub components: bool,
  /// Select Through: hidden objects count in a window or freeform selection (the id buffer
  /// sees only what is drawn, so it changes nothing yet).
  pub select_through: bool,
  /// The types of [`OTHER_FILTERS`], in that order.
  others: [bool; 16],
}

impl Default for Filters {
  fn default() -> Self {
    Filters {
      bodies: true,
      faces: true,
      edges: true,
      sketches: true,
      sketch_points: true,
      profiles: true,
      vertices: true,
      construction: true,
      components: true,
      select_through: true,
      others: [true; 16],
    }
  }
}

impl Filters {
  fn slot(&mut self, name: &str) -> &mut bool {
    match name {
      "bodies" => &mut self.bodies,
      "faces" => &mut self.faces,
      "edges" => &mut self.edges,
      "sketches" | "sketch_curves" => &mut self.sketches,
      "sketch_points" => &mut self.sketch_points,
      "vertices" => &mut self.vertices,
      "construction" => &mut self.construction,
      "components" => &mut self.components,
      "through" | "select_through" => &mut self.select_through,
      "profiles" => &mut self.profiles,
      other => match OTHER_FILTERS.iter().position(|k| *k == other) {
        Some(i) => &mut self.others[i],
        None => &mut self.profiles,
      },
    }
  }

  /// Is the filter on? "all" is on when every type is.
  pub fn get(&self, name: &str) -> bool {
    if name == "all" {
      return self.all_on();
    }
    let mut copy = *self;
    *copy.slot(name)
  }

  /// Are all the object types on (Select Through aside)?
  pub fn all_on(&self) -> bool {
    FILTER_TYPES.iter().all(|(k, _)| self.get(k))
  }

  /// Flips a filter; "all" turns every type on, or off when all are on, as Fusion's Select All.
  pub fn toggle(&mut self, name: &str) {
    if name == "all" {
      let on = !self.all_on();
      for (k, _) in FILTER_TYPES {
        *self.slot(k) = on;
      }
      return;
    }
    let slot = self.slot(name);
    *slot = !*slot;
  }

  /// [`Filters::allows`], with picks on a mesh body governed by the Mesh Bodies filter (the
  /// body is picked whole, so the face and edge filters don't apply to it).
  pub fn allows_on(&self, s: &Session, p: &Pick, priority: Priority) -> bool {
    match pick_body(p) {
      Some(b) if is_mesh(s, b) => match priority {
        Priority::Component => self.components,
        _ => self.get("mesh_bodies"),
      },
      _ => self.allows(p, priority),
    }
  }

  /// Can this pick be hovered and selected with this priority?
  pub fn allows(&self, p: &Pick, priority: Priority) -> bool {
    let whole = match priority {
      Priority::Body => Some(self.bodies),
      Priority::Component => Some(self.components),
      _ => None,
    };
    match (p, whole) {
      (Pick::Face { .. } | Pick::Edge { .. }, Some(w)) => w,
      (Pick::Vertex { .. }, Some(w)) => w && self.vertices,
      (Pick::Vertex { .. }, None) => self.vertices,
      (Pick::Face { .. }, None) => self.faces && priority != Priority::Edge,
      (Pick::Edge { .. }, None) => self.edges,
      (Pick::Profile { .. }, _) => self.profiles,
      (Pick::SketchPoint { .. }, _) => self.sketch_points,
      (Pick::SketchCurve { .. }, _) => self.sketches,
    }
  }
}

/// The bodies of the component a body belongs to (just the body for a root body).
fn component_bodies(s: &Session, body: BodyId) -> Vec<BodyId> {
  match s.design.components.iter().find(|c| c.bodies.contains(&body)) {
    Some(c) => c.bodies.iter().copied().filter(|b| s.model.body(*b).is_some()).collect(),
    None => vec![body],
  }
}

/// What picking `p` selects under a priority.
pub fn targets(s: &Session, p: Pick, priority: Priority) -> Vec<Sel> {
  let body = match p {
    Pick::Face { body, .. } | Pick::Edge { body, .. } | Pick::Vertex { body, .. } => Some(body),
    _ => None,
  };
  match (priority, body) {
    (Priority::Component, Some(b)) => component_bodies(s, b).into_iter().map(|body| Sel::Body { body }).collect(),
    // a mesh body is picked whole whatever the priority, as Fusion's are
    (_, Some(b)) if priority == Priority::Body || is_mesh(s, b) => vec![Sel::Body { body: b }],
    _ => vec![p.to_sel()],
  }
}

/// Whether a body is a mesh body (picked whole, never by face or edge).
pub(crate) fn is_mesh(s: &Session, body: BodyId) -> bool {
  s.model.body(body).is_some_and(|b| b.is_mesh())
}

/// The bodies the pointer lights as a whole: a face, edge or vertex under it while the
/// selection priority (or a dialog's Selection Filter) picks bodies or components. Fusion lights
/// the whole body then, not the face under the pointer.
pub(crate) fn hover_bodies(app: &App) -> Vec<BodyId> {
  match app.hover {
    Some(p)
      if pick_body(&p).is_some_and(|b| {
        matches!(app.sel_priority, Priority::Body | Priority::Component)
          || is_mesh(&app.s, b) && crate::mesh_ui::face_pick(app, p).is_none()
      }) =>
    {
      targets(&app.s, p, app.sel_priority)
        .into_iter()
        .filter_map(|x| if let Sel::Body { body } = x { Some(body) } else { None })
        .collect()
    }
    _ => Vec::new(),
  }
}

/// Selects (or with `add`, toggles) what a click on `p` picks.
pub fn pick(app: &mut App, p: Pick, add: bool) {
  let items = crate::mesh_ui::face_pick(app, p).unwrap_or_else(|| targets(&app.s, p, app.sel_priority));
  if items.len() == 1 {
    crate::panels::toggle_sel(&mut app.s, items[0], add);
    return;
  }
  // a component: toggle the whole group together
  let all_in = items.iter().all(|i| app.s.selection.contains(i));
  if !add {
    app.s.selection.clear();
  }
  if add && all_in {
    app.s.selection.retain(|x| !items.contains(x));
  } else {
    for i in items {
      if !app.s.selection.contains(&i) {
        app.s.selection.push(i);
      }
    }
  }
}

/// Double-clicking an edge selects its loop, as in Fusion (with `add`, adds the loop to the
/// selection). Under the body or component priority a double-click picks as a click does.
pub fn pick_loop(app: &mut App, body: BodyId, edge: u32, add: bool) {
  if !matches!(targets(&app.s, Pick::Edge { body, edge }, app.sel_priority).as_slice(), [Sel::Edge { .. }]) {
    return;
  }
  let Some(b) = app.s.model.body(body) else { return };
  let edges = edge_loop(b.edges(), b.faces(), edge, app.cam.eye_dir());
  if !add {
    app.s.selection.clear();
  }
  for edge in edges {
    let s = Sel::Edge { body, edge };
    if !app.s.selection.contains(&s) {
      app.s.selection.push(s);
    }
  }
}

/// The edges a double-click on `edge` selects: the loop it lies on, the closed boundary of one
/// of its faces (planar faces turned towards `eye` first), or its tangent chain when it isn't
/// on a closed loop.
pub fn edge_loop(edges: &[EdgeInfo], faces: &[FaceInfo], edge: u32, eye: DVec3) -> Vec<u32> {
  let Some(e) = edges.iter().find(|x| x.id == edge) else { return Vec::new() };
  let score = |f: u32| match faces.iter().find(|x| x.id == f) {
    Some(x) if x.kind == SurfaceKind::Plane => 2.0 + x.normal.dot(eye),
    _ => 0.0,
  };
  let mut by: Vec<u32> = e.faces.iter().copied().filter(|f| *f != 0).collect();
  by.dedup();
  by.sort_by(|a, b| score(*b).total_cmp(&score(*a)));
  if let Some(lp) = by.into_iter().find_map(|f| face_loop(edges, f, edge)) {
    return lp;
  }
  let mut out = vec![edge];
  let mut i = 0;
  while i < out.len() {
    for n in fox_engine::tangent_neighbours(edges, out[i]) {
      if !out.contains(&n) {
        out.push(n);
      }
    }
    i += 1;
  }
  out
}

/// The edges of `face` connected end to end with `edge`, if they close into a loop.
fn face_loop(edges: &[EdgeInfo], face: u32, edge: u32) -> Option<Vec<u32>> {
  let own: Vec<&EdgeInfo> =
    edges.iter().filter(|e| e.faces.contains(&face) && e.kind != CurveKind::Degenerate).collect();
  let tol = |e: &EdgeInfo| 1e-6 * (1.0 + e.length);
  let meet = |p: DVec3, e: &EdgeInfo| (p - e.start).length() < tol(e) || (p - e.end).length() < tol(e);
  let mut lp = vec![*own.iter().find(|e| e.id == edge)?];
  let mut i = 0;
  while i < lp.len() {
    let a = lp[i];
    for o in &own {
      if !lp.iter().any(|x| x.id == o.id) && (meet(a.start, o) || meet(a.end, o)) {
        lp.push(o);
      }
    }
    i += 1;
  }
  // closed: every vertex is where an even number of edge ends meet
  let ends: Vec<DVec3> = lp.iter().flat_map(|e| [e.start, e.end]).collect();
  let tol = lp.iter().map(|e| tol(e)).fold(0.0, f64::max);
  let closed = ends.iter().all(|p| ends.iter().filter(|q| (**q - *p).length() < tol).count() % 2 == 0);
  closed.then(|| lp.iter().map(|e| e.id).collect())
}

/// A row of the Select Other list: what picking it selects, the pick to highlight while it is
/// hovered, and its type ("Face", "Body").
pub(crate) type OtherRow = (Vec<Sel>, Option<Pick>, String);

/// The Select Other list's tabs, as Fusion's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum OtherTab {
  /// What lies under the pointer, front to back.
  #[default]
  Depth,
  /// The bodies and components those belong to.
  Parents,
}

/// The Select Other list: what lies under a point of the viewport, front to back, with one row
/// per entity named by its type, and a Parents tab for what they belong to.
pub(crate) struct OtherMenu {
  pub pos: Pos2,
  pub tab: OtherTab,
  pub depth: Vec<OtherRow>,
  pub parents: Vec<OtherRow>,
}

impl OtherMenu {
  /// Builds the list from the picks under the cursor (nearest first): Depth holds the edge (or
  /// sketch entity) under the cursor then each face, Parents the bodies and components they
  /// belong to.
  pub fn new(s: &Session, pos: Pos2, hover: Option<Pick>, hits: &[(f64, Pick)]) -> OtherMenu {
    let mut depth: Vec<OtherRow> = Vec::new();
    let mut bodies: Vec<BodyId> = Vec::new();
    if let Some(
      h @ (Pick::Edge { .. }
      | Pick::SketchCurve { .. }
      | Pick::SketchPoint { .. }
      | Pick::Profile { .. }
      | Pick::Vertex { .. }),
    ) = hover
    {
      depth.push((vec![h.to_sel()], Some(h), type_name(h).to_string()));
      if let Pick::Edge { body, .. } | Pick::Vertex { body, .. } = h {
        bodies.push(body);
      }
    }
    for (_, p) in hits {
      depth.push((vec![p.to_sel()], Some(*p), type_name(*p).to_string()));
      if let Pick::Face { body, .. } = p
        && !bodies.contains(body)
      {
        bodies.push(*body);
      }
    }
    let mut parents: Vec<OtherRow> = Vec::new();
    for b in &bodies {
      parents.push((vec![Sel::Body { body: *b }], None, "Body".to_string()));
    }
    let mut seen = Vec::new();
    for b in &bodies {
      if let Some(c) = s.design.components.iter().find(|c| c.bodies.contains(b))
        && !seen.contains(&c.id)
      {
        seen.push(c.id);
        let all = c.bodies.iter().map(|body| Sel::Body { body: *body }).collect();
        parents.push((all, None, "Component".to_string()));
      }
    }
    OtherMenu { pos, tab: OtherTab::Depth, depth, parents }
  }

  /// The rows of the open tab.
  pub fn rows(&self) -> &[OtherRow] {
    match self.tab {
      OtherTab::Depth => &self.depth,
      OtherTab::Parents => &self.parents,
    }
  }
}

/// The type a Select Other row names a pick by.
fn type_name(p: Pick) -> &'static str {
  match p {
    Pick::Face { .. } => "Face",
    Pick::Edge { .. } => "Edge",
    Pick::Vertex { .. } => "Vertex",
    Pick::SketchCurve { .. } => "Sketch Curve",
    Pick::SketchPoint { .. } => "Sketch Point",
    Pick::Profile { .. } => "Profile",
  }
}

/// How long a still press on the model takes to open the Select Other list (seconds).
pub const HOLD_SECS: f64 = 0.5;

/// Shows the Select Other list; picking an entry selects it (Shift/Ctrl adds), hovering
/// highlights it in the viewport. The list stays until a row is picked, a click lands outside
/// it or Escape.
pub(crate) fn show_other(app: &mut App, ctx: &egui::Context) {
  let Some(mut menu) = app.other.take() else { return };
  let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
  let mut keep = true;
  let mut chosen: Option<Vec<Sel>> = None;
  let mut hovered: Option<Pick> = None;
  let resp = egui::Area::new(egui::Id::new("select-other"))
    .fixed_pos(menu.pos + egui::vec2(8.0, 8.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      crate::panels::dialog_frame().show(ui, |ui| {
        ui.set_min_width(120.0);
        ui.horizontal(|ui| {
          for (tab, name) in [(OtherTab::Depth, "Depth"), (OtherTab::Parents, "Parents")] {
            let on = menu.tab == tab;
            let r = ui.selectable_label(on, RichText::new(name).size(12.0));
            if on {
              let b = r.rect.bottom();
              ui.painter().line_segment(
                [egui::pos2(r.rect.left(), b), egui::pos2(r.rect.right(), b)],
                egui::Stroke::new(2.0, theme::ACCENT),
              );
            }
            if r.clicked() {
              menu.tab = tab;
            }
          }
        });
        ui.separator();
        if menu.rows().is_empty() {
          ui.label(RichText::new("Nothing under the cursor").color(theme::MUTED));
        }
        for (sel, pick, text) in menu.rows() {
          let r = ui.selectable_label(sel.iter().all(|x| app.s.selection.contains(x)), text);
          if r.hovered() {
            hovered = *pick;
          }
          if r.clicked() {
            chosen = Some(sel.clone());
          }
        }
      });
    })
    .response;
  if let Some(h) = hovered {
    app.hover = Some(h);
  }
  let picked = chosen.is_some();
  if let Some(sel) = chosen {
    if !add {
      app.s.selection.clear();
    }
    for x in sel {
      crate::panels::toggle_sel(&mut app.s, x, true);
    }
    keep = false;
  }
  if ctx.input(|i| i.pointer.any_pressed()) && !resp.hovered() && !picked {
    keep = false;
  }
  if keep {
    app.other = Some(menu);
  }
}

// ---------------------------------------------------------------- window, freeform and paint selection

/// What a left-drag with the select tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
  /// A box (left to right: what is inside; right to left: what it touches).
  #[default]
  Window,
  /// A freehand loop, with the same rule by the direction the drag sets off in.
  Freeform,
  /// A brush: everything it passes over.
  Paint,
}

/// Paint selection's brush radius (points).
pub const BRUSH: f32 = 10.0;

/// The freeform loop or paint stroke being dragged (screen points).
#[derive(Default)]
pub struct Region {
  pub points: Vec<Pos2>,
}

/// Is `p` inside the closed polygon `poly` (even-odd rule)?
pub fn point_in_polygon(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
  let mut inside = false;
  let n = poly.len();
  for i in 0..n {
    let (a, b) = (poly[i], poly[(i + n - 1) % n]);
    if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
      inside = !inside;
    }
  }
  inside
}

/// The pixel columns of row `y` (its centre line at `y + 0.5`) inside the closed polygon, as
/// half-open `[x0, x1)` spans clamped to `0..width`.
pub fn polygon_spans(poly: &[[f64; 2]], y: i64, width: i64) -> Vec<(i64, i64)> {
  let yc = y as f64 + 0.5;
  let n = poly.len();
  let mut xs: Vec<f64> = Vec::new();
  for i in 0..n {
    let (a, b) = (poly[i], poly[(i + 1) % n]);
    if (a[1] > yc) != (b[1] > yc) {
      xs.push(a[0] + (yc - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
    }
  }
  xs.sort_by(f64::total_cmp);
  // pixel x is inside when its centre x + 0.5 lies in a span
  let col = |x: f64| ((x - 0.5).ceil() as i64).clamp(0, width);
  xs.chunks_exact(2).map(|c| (col(c[0]), col(c[1]))).filter(|(a, b)| b > a).collect()
}

/// Does a freeform loop select only what lies wholly inside it? As with the window, that is when
/// the drag sets off to the right (its first clear sideways move).
pub fn lasso_window(points: &[Pos2]) -> bool {
  let Some(first) = points.first() else { return true };
  points.iter().map(|p| p.x - first.x).find(|dx| dx.abs() >= 8.0).is_none_or(|dx| dx > 0.0)
}

/// Pixels (with their counts per pick id) of a frame's id buffer covered by a closed polygon
/// given in frame pixels.
pub fn ids_in_polygon(ids: &[u32], width: usize, height: usize, poly: &[[f64; 2]]) -> HashMap<u32, usize> {
  let mut out: HashMap<u32, usize> = HashMap::new();
  if poly.len() < 3 {
    return out;
  }
  let lo = poly.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
  let hi = poly.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max).ceil().min(height as f64) as i64;
  for y in lo..hi {
    for (x0, x1) in polygon_spans(poly, y, width as i64) {
      for x in x0..x1 {
        if let Some(&id) = ids.get(y as usize * width + x as usize)
          && id != 0
        {
          *out.entry(id).or_default() += 1;
        }
      }
    }
  }
  out
}

/// Pixels of a frame's id buffer under a round brush dragged from `a` to `b` (frame pixels).
pub fn ids_under_brush(
  ids: &[u32],
  width: usize,
  height: usize,
  a: [f64; 2],
  b: [f64; 2],
  r: f64,
) -> HashMap<u32, usize> {
  let mut out: HashMap<u32, usize> = HashMap::new();
  let x0 = (a[0].min(b[0]) - r).floor().max(0.0) as usize;
  let x1 = ((a[0].max(b[0]) + r).ceil().max(0.0) as usize).min(width);
  let y0 = (a[1].min(b[1]) - r).floor().max(0.0) as usize;
  let y1 = ((a[1].max(b[1]) + r).ceil().max(0.0) as usize).min(height);
  for y in y0..y1 {
    for x in x0..x1 {
      let p = [x as f64 + 0.5, y as f64 + 0.5];
      if seg_dist(p, a, b) <= r
        && let Some(&id) = ids.get(y * width + x)
        && id != 0
      {
        *out.entry(id).or_default() += 1;
      }
    }
  }
  out
}

/// Distance from `p` to the segment `a`–`b`.
pub fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l2 = dx * dx + dy * dy;
  let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
  (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// Selects what the covered pixels (`inside`: pixel count per pick id) pick. With `window` only
/// what lies wholly inside counts; otherwise anything touched. Priority and filters decide what a
/// covered pick selects (faces, edges, bodies or components; sketch geometry while sketching).
/// The body a canvas pick lies on.
pub(crate) fn pick_body(p: &Pick) -> Option<BodyId> {
  match *p {
    Pick::Face { body, .. } | Pick::Edge { body, .. } | Pick::Vertex { body, .. } => Some(body),
    _ => None,
  }
}

pub(crate) fn select_ids(app: &mut App, inside: &HashMap<u32, usize>, window: bool, add: bool) {
  let Some(frame) = &app.frame else {
    // nothing drawn to pick from yet: the box still replaces the selection, as in Fusion
    if !add {
      app.s.selection.clear();
    }
    return;
  };
  let mut total: HashMap<u32, usize> = HashMap::new();
  if window {
    for id in &frame.ids {
      if inside.contains_key(id) {
        *total.entry(*id).or_default() += 1;
      }
    }
  }
  let sketching = app.s.edit_sketch.is_some();
  let (filter, priority) = (app.sel_filter, app.sel_priority);
  // per target: pixels inside and pixels anywhere
  let mut hits: Vec<(Sel, usize, usize)> = Vec::new();
  let mut whole: HashMap<BodyId, usize> = HashMap::new();
  for (&id, &n) in inside {
    let Some(p) = app.picks.get(id) else { continue };
    let keep = if sketching {
      matches!(p, Pick::SketchCurve { .. } | Pick::SketchPoint { .. })
    } else {
      filter.allows_on(&app.s, &p, priority)
        && !pick_body(&p).is_some_and(|b| app.ctx_menu.unselectable.contains(&b))
        && !matches!(
          (priority, p),
          (Priority::Face, Pick::Edge { .. } | Pick::Vertex { .. })
            | (Priority::Edge, Pick::Face { .. } | Pick::Vertex { .. })
        )
    };
    if !keep {
      continue;
    }
    let all = total.get(&id).copied().unwrap_or(n);
    let targets = if sketching { vec![p.to_sel()] } else { targets(&app.s, p, priority) };
    for t in targets {
      match hits.iter_mut().find(|h| h.0 == t) {
        Some(h) => {
          h.1 += n;
          h.2 += all;
        }
        None => hits.push((t, n, all)),
      }
    }
  }
  if window {
    // a body is inside only if all of its pixels are, edges included
    for (&id, &all) in &total {
      if let Some(Pick::Face { body, .. } | Pick::Edge { body, .. }) = app.picks.get(id) {
        *whole.entry(body).or_default() += all;
      }
    }
    let mut body_px: HashMap<BodyId, usize> = HashMap::new();
    for id in &frame.ids {
      if let Some(Pick::Face { body, .. } | Pick::Edge { body, .. }) = app.picks.get(*id)
        && whole.contains_key(&body)
      {
        *body_px.entry(body).or_default() += 1;
      }
    }
    hits.retain(|(t, n, all)| match t {
      Sel::Body { body } => whole.get(body).copied().unwrap_or(0) >= body_px.get(body).copied().unwrap_or(usize::MAX),
      _ => n >= all,
    });
  }
  if !add {
    app.s.selection.clear();
  }
  for (t, ..) in hits {
    if !app.s.selection.contains(&t) {
      app.s.selection.push(t);
    }
  }
}

/// Freeform and paint selection: starts, extends and (paint) applies a drag with the select tool.
/// `to_px` maps screen points to frame pixels. Returns true while such a drag runs.
pub(crate) fn drag_region(
  app: &mut App,
  resp: &egui::Response,
  ctx: &egui::Context,
  pos: Pos2,
  to_px: &dyn Fn(Pos2) -> [f64; 2],
) -> bool {
  use crate::viewport::Drag;
  if resp.drag_started_by(egui::PointerButton::Primary)
    && app.drag.is_none()
    && app.tool == crate::Tool::Select
    && app.sel_mode != Mode::Window
  {
    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
    app.region.points = vec![start];
    if app.sel_mode == Mode::Paint {
      let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command) || app.dialog.is_some();
      if !add {
        app.s.selection.clear();
      }
      app.drag = Some(Drag::Paint);
    } else {
      app.drag = Some(Drag::Lasso);
    }
  }
  match app.drag {
    Some(Drag::Lasso) => {
      if app.region.points.last().is_none_or(|l| (*l - pos).length() >= 2.0) {
        app.region.points.push(pos);
      }
      true
    }
    Some(Drag::Paint) => {
      let last = app.region.points.last().copied().unwrap_or(pos);
      if let Some(frame) = &app.frame {
        let r = BRUSH as f64 * frame.width as f64 / app.canvas.width().max(1.0) as f64;
        let hit = ids_under_brush(&frame.ids, frame.width as usize, frame.height as usize, to_px(last), to_px(pos), r);
        select_ids(app, &hit, false, true);
      }
      if (last - pos).length() >= 1.0 {
        app.region.points.push(pos);
      }
      true
    }
    _ => false,
  }
}

/// Ends a freeform loop: selects what it encloses (or touches, see [`lasso_window`]).
pub(crate) fn finish_lasso(app: &mut App, add: bool, to_px: &dyn Fn(Pos2) -> [f64; 2]) {
  let points = std::mem::take(&mut app.region.points);
  let Some(frame) = &app.frame else { return };
  let poly: Vec<[f64; 2]> = points.iter().map(|p| to_px(*p)).collect();
  let inside = ids_in_polygon(&frame.ids, frame.width as usize, frame.height as usize, &poly);
  select_ids(app, &inside, lasso_window(&points), add);
}

/// Draws the freeform loop or the paint stroke being dragged.
pub(crate) fn region_overlay(app: &App, painter: &egui::Painter) {
  use crate::viewport::Drag;
  let pts = &app.region.points;
  match app.drag {
    Some(Drag::Lasso) if pts.len() > 1 => {
      let col =
        if lasso_window(pts) { egui::Color32::from_rgb(40, 120, 230) } else { egui::Color32::from_rgb(40, 170, 90) };
      if pts.len() > 2 {
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
          points: pts.clone(),
          closed: true,
          fill: egui::Color32::TRANSPARENT,
          stroke: egui::Stroke::new(1.0, col).into(),
        }));
        // a concave loop can't be filled as one convex shape: shade it as a fan of thin strips
        let c = pts[0];
        let mut mesh = egui::Mesh::default();
        for w in pts.windows(2) {
          let i = mesh.vertices.len() as u32;
          for p in [c, w[0], w[1]] {
            mesh.colored_vertex(p, col.gamma_multiply(0.08));
          }
          mesh.add_triangle(i, i + 1, i + 2);
        }
        painter.add(mesh);
      }
    }
    Some(Drag::Paint) => {
      // orange, so the trail shows over the blue of what it has selected
      let col = egui::Color32::from_rgb(255, 140, 20);
      if pts.len() > 1 {
        painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(BRUSH * 2.0, col.gamma_multiply(0.3))));
      }
      if let Some(p) = pts.last() {
        painter.circle_stroke(*p, BRUSH, egui::Stroke::new(1.5, col));
      }
    }
    _ => {}
  }
}

// ---------------------------------------------------------------- SELECT > Selection Tools

/// The Selection Tools commands that open a dialog, as Fusion's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelTool {
  ByName,
  ByBoundary,
  BySize,
  SeedAndBoundary,
}

impl SelTool {
  /// The command's name, as the menu shows it.
  pub fn name(self) -> &'static str {
    match self {
      SelTool::ByName => "Select By Name",
      SelTool::ByBoundary => "Select By Boundary",
      SelTool::BySize => "Select By Size",
      SelTool::SeedAndBoundary => "Seed And Boundary",
    }
  }

  /// The dialog's title (upper case, as Fusion's).
  fn title(self) -> &'static str {
    match self {
      SelTool::ByName => "SELECT BY NAME",
      SelTool::ByBoundary => "SELECT BY BOUNDARY",
      SelTool::BySize => "SELECT BY SIZE",
      SelTool::SeedAndBoundary => "SEED AND BOUNDARY",
    }
  }

  /// The engine command the dialog's OK runs.
  pub fn command(self) -> &'static str {
    match self {
      SelTool::ByName => "select.by_name",
      SelTool::ByBoundary => "select.by_boundary",
      SelTool::BySize => "select.by_size",
      SelTool::SeedAndBoundary => "select.seed_boundary",
    }
  }

  /// The cursor prompt while the dialog waits for picks.
  pub fn prompt(self) -> &'static str {
    match self {
      SelTool::ByName => "Type a name (* and ? are wildcards), then OK",
      SelTool::ByBoundary => "Select the edges of a closed boundary, then OK",
      SelTool::BySize => "Enter the largest size to select, then OK",
      SelTool::SeedAndBoundary => "Select a seed face and the boundary edges, then OK",
    }
  }
}

/// An open Selection Tools dialog and its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDialog {
  pub tool: SelTool,
  /// Select By Name: the pattern.
  pub name: String,
  /// Select By Name: all, bodies or components; Select By Size: bodies or faces.
  pub kind: &'static str,
  /// Select By Size: the largest size (an expression in the design's unit).
  pub size: String,
  pub error: Option<String>,
}

impl ToolDialog {
  pub fn new(tool: SelTool) -> ToolDialog {
    let kind = if tool == SelTool::ByName { "all" } else { "bodies" };
    ToolDialog { tool, name: String::new(), kind, size: "10 mm".into(), error: None }
  }

  /// The command's parameters from the fields (the picks come from the selection).
  pub fn params(&self) -> serde_json::Value {
    match self.tool {
      SelTool::ByName => serde_json::json!({"name": self.name, "kind": self.kind}),
      SelTool::BySize => serde_json::json!({"max": self.size, "kind": self.kind}),
      SelTool::ByBoundary | SelTool::SeedAndBoundary => serde_json::json!({}),
    }
  }
}

/// Opens a Selection Tools dialog (closing the Select Other list).
pub(crate) fn open_tool(app: &mut App, tool: SelTool) {
  app.other = None;
  app.sel_tool = Some(ToolDialog::new(tool));
  app.hint(tool.prompt());
}

/// Runs the open Selection Tools dialog's command: on success the dialog closes with the
/// result selected; an error stays in the dialog.
pub(crate) fn apply_tool(app: &mut App) {
  let Some(d) = app.sel_tool.as_ref() else { return };
  let (cmd, params) = (d.tool.command(), d.params());
  match app.s.execute(cmd, &params) {
    Ok(v) => {
      app.sel_tool = None;
      app.end_hint();
      let n = v.get("selected").and_then(serde_json::Value::as_u64).unwrap_or(0);
      app.status = Some((format!("{n} selected"), false));
    }
    Err(e) => {
      if let Some(d) = app.sel_tool.as_mut() {
        d.error = Some(e.to_string());
      }
    }
  }
}

/// Shows the open Selection Tools dialog where command dialogs dock.
pub(crate) fn show_tool(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.sel_tool.take() else { return };
  let mut ok = false;
  let mut cancel = false;
  let (edges, faces) = app.s.selection.iter().fold((0, 0), |(e, f), x| match x {
    Sel::Edge { .. } => (e + 1, f),
    Sel::Face { .. } => (e, f + 1),
    _ => (e, f),
  });
  egui::Area::new(egui::Id::new("selection-tool"))
    .fixed_pos(app.canvas.right_top() + egui::vec2(-34.0 - 260.0, 10.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      crate::panels::dialog_frame().show(ui, |ui| {
        ui.set_min_width(260.0);
        ui.label(RichText::new(d.tool.title()).small().color(theme::GROUP_LABEL));
        ui.separator();
        egui::Grid::new("selection-tool-fields").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| match d.tool {
          SelTool::ByName => {
            ui.label("Name");
            ui.text_edit_singleline(&mut d.name);
            ui.end_row();
            ui.label("Object Type");
            egui::ComboBox::from_id_salt("by-name-kind").selected_text(kind_name(d.kind)).show_ui(ui, |ui| {
              for k in ["all", "bodies", "components"] {
                ui.selectable_value(&mut d.kind, k, kind_name(k));
              }
            });
            ui.end_row();
          }
          SelTool::BySize => {
            ui.label("Object Type");
            egui::ComboBox::from_id_salt("by-size-kind").selected_text(kind_name(d.kind)).show_ui(ui, |ui| {
              for k in ["bodies", "faces"] {
                ui.selectable_value(&mut d.kind, k, kind_name(k));
              }
            });
            ui.end_row();
            ui.label("Size");
            ui.text_edit_singleline(&mut d.size);
            ui.end_row();
          }
          SelTool::ByBoundary => {
            ui.label("Boundary");
            ui.label(picked(edges, "edge"));
            ui.end_row();
          }
          SelTool::SeedAndBoundary => {
            ui.label("Seed");
            ui.label(picked(faces, "face"));
            ui.end_row();
            ui.label("Boundary");
            ui.label(picked(edges, "edge"));
            ui.end_row();
          }
        });
        if let Some(e) = &d.error {
          ui.label(RichText::new(e).color(theme::ERROR));
        }
        ui.separator();
        ui.horizontal(|ui| {
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel = ui.button("Cancel").clicked();
            ok = ui.button("OK").clicked();
          });
        });
      });
    });
  app.sel_tool = Some(d);
  if cancel {
    app.sel_tool = None;
    app.end_hint();
  } else if ok {
    apply_tool(app);
  }
}

fn kind_name(kind: &str) -> &'static str {
  match kind {
    "bodies" => "Bodies",
    "components" => "Components",
    "faces" => "Faces",
    _ => "All",
  }
}

/// "1 edge selected", "2 faces selected", "Select" when none.
fn picked(n: usize, what: &str) -> String {
  match n {
    0 => "Select".to_string(),
    1 => format!("1 {what} selected"),
    n => format!("{n} {what}s selected"),
  }
}

// ---------------------------------------------------------------- joints and saved joint origins

/// Glyph radius of a joint or joint origin (points).
const JOINT_R: f32 = 7.0;

/// Draws the joints and saved joint origins and lets a click pick one: into the open Joint
/// dialog (a saved origin for the next side to fill), Drive Joints / Motion Study (the joint),
/// or the selection. Returns true when the pointer is on a glyph.
pub(crate) fn joint_glyphs(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  project: &dyn Fn(fox_engine::kernel::glam::DVec3) -> Option<Pos2>,
) -> bool {
  if app.s.edit_sketch.is_some() {
    return false;
  }
  // Object Visibility ▸ Joints and Joint Origins
  let (joints, origins) = (app.objects.joints, app.objects.joint_origins);
  let mut marks = fox_engine::scene::joint_marks(&app.s);
  marks.retain(|m| if m.joint { joints } else { origins });
  if marks.is_empty() {
    return false;
  }
  let hover = resp.hover_pos();
  let mut hot: Option<(f32, fox_engine::scene::JointMark)> = None;
  let mut drawn: Vec<(Pos2, fox_engine::scene::JointMark)> = Vec::new();
  for m in &marks {
    let o = m.frame.w_axis.truncate();
    let Some(c) = project(o) else { continue };
    if let Some(h) = hover {
      let d = (h - c).length();
      if d <= JOINT_R + 3.0 && hot.is_none_or(|(best, _)| d < best) {
        hot = Some((d, *m));
      }
    }
    drawn.push((c, *m));
  }
  let selected = |m: &fox_engine::scene::JointMark| app.s.selection.contains(&Sel::Feature { feature: m.feature });
  for (c, m) in &drawn {
    let is_hot = hot.is_some_and(|(_, h)| h.feature == m.feature);
    let ring = if is_hot {
      egui::Color32::from_rgb(255, 140, 20)
    } else if selected(m) {
      egui::Color32::from_rgb(36, 142, 220)
    } else {
      egui::Color32::from_gray(60)
    };
    // the frame's axes, as short stubs (X red, Y green, Z blue); a joint origin's only with
    // Object Visibility ▸ Joint Origin Axes on
    let o = m.frame.w_axis.truncate();
    let len = joint_axis_len(project, o);
    let stubs = m.joint || app.objects.joint_origin_axes;
    for (axis, col) in [
      (m.frame.x_axis.truncate(), egui::Color32::from_rgb(220, 70, 60)),
      (m.frame.y_axis.truncate(), egui::Color32::from_rgb(70, 170, 70)),
      (m.frame.z_axis.truncate(), egui::Color32::from_rgb(50, 100, 230)),
    ] {
      if let Some(t) = project(o + axis * len).filter(|_| stubs) {
        painter.line_segment([*c, t], egui::Stroke::new(1.5, col));
      }
    }
    painter.circle_filled(*c, JOINT_R, egui::Color32::WHITE);
    painter.circle_stroke(*c, JOINT_R, egui::Stroke::new(if is_hot { 2.5 } else { 1.5 }, ring));
    if m.joint {
      // a joint: two half discs, as Fusion's joint glyph
      painter.circle_filled(*c, JOINT_R * 0.45, ring);
    } else {
      painter.circle_stroke(*c, JOINT_R * 0.45, egui::Stroke::new(1.0, ring));
    }
  }
  let Some((_, m)) = hot else { return false };
  let name = app.s.design.feature(m.feature).map(|f| f.name.clone()).unwrap_or_default();
  if let Some(h) = hover {
    painter.text(
      h + egui::vec2(12.0, -12.0),
      egui::Align2::LEFT_BOTTOM,
      &name,
      egui::FontId::proportional(12.0),
      egui::Color32::from_gray(30),
    );
  }
  if resp.clicked() {
    let picked = match app.dialog.as_mut() {
      Some(d) => d.pick_joint_mark(&mut app.s, &m, &name),
      None => false,
    };
    if picked {
      app.status = Some((format!("Picked {name}"), false));
    } else if app.dialog.is_none() {
      crate::panels::toggle_sel(&mut app.s, Sel::Feature { feature: m.feature }, resp.ctx.input(|i| i.modifiers.shift));
    } else {
      app.status = Some((format!("{name} can't be used in this command"), true));
    }
  }
  true
}

/// World length of a joint glyph's axis stubs: about 22 points on screen.
fn joint_axis_len(
  project: &dyn Fn(fox_engine::kernel::glam::DVec3) -> Option<Pos2>,
  o: fox_engine::kernel::glam::DVec3,
) -> f64 {
  let Some(c) = project(o) else { return 1.0 };
  // probe along each world axis: the longest projection tells the scale even edge-on
  let px = [fox_engine::kernel::glam::DVec3::X, fox_engine::kernel::glam::DVec3::Y, fox_engine::kernel::glam::DVec3::Z]
    .iter()
    .filter_map(|a| project(o + *a).map(|p| (p - c).length()))
    .fold(0.0f32, f32::max);
  if px > 1e-6 { 22.0 / px as f64 } else { 1.0 }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn cube() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", serde_json::json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
    assert_eq!(app.s.model.bodies.len(), 1, "no box");
    app
  }

  /// The edge with its midpoint at `mid`.
  fn edge_at(app: &App, mid: [f64; 3]) -> u32 {
    let b = &app.s.model.bodies[0];
    b.edges().iter().find(|e| (e.mid - DVec3::from(mid)).length() < 1e-3).expect("no edge there").id
  }

  /// The midpoints of a body's edges, sorted.
  fn mids(app: &App, ids: &[u32]) -> Vec<[i64; 3]> {
    let b = &app.s.model.bodies[0];
    let mut out: Vec<[i64; 3]> = ids
      .iter()
      .map(|id| {
        let m = b.edges().iter().find(|e| e.id == *id).unwrap().mid;
        [m.x.round() as i64, m.y.round() as i64, m.z.round() as i64]
      })
      .collect();
    out.sort();
    out
  }

  /// Fusion's filters: every type on by default, Select All flips them all and reads as on only
  /// while all are, Select Through stands apart.
  #[test]
  fn filters_default_on_with_select_all() {
    let mut f = Filters::default();
    assert!(f.all_on() && f.get("all") && f.vertices && f.select_through);
    for (k, _) in FILTER_TYPES {
      assert!(f.get(k), "{k} starts on");
    }
    f.toggle("all");
    assert!(!f.all_on() && FILTER_TYPES.iter().all(|(k, _)| !f.get(k)));
    assert!(f.select_through, "Select All leaves Select Through alone");
    f.toggle("all");
    assert!(f.all_on());
    f.toggle("tspline");
    assert!(!f.get("tspline") && !f.get("all"));
    f.toggle("tspline");
    assert!(f.get("all"));
    f.toggle("through");
    assert!(!f.select_through && f.get("all"));
    // the old key still names the sketch curves
    f.toggle("sketches");
    assert!(!f.sketches && !f.get("sketch_curves"));
    // vertices pick under the face priority now that they are on by default
    let f = Filters::default();
    let v = Pick::Vertex { body: BodyId(1), edge: 1, end: false };
    assert!(f.allows(&v, Priority::Face) && f.allows(&v, Priority::Body));
    let mut g = f;
    g.components = false;
    let face = Pick::Face { body: BodyId(1), face: 1 };
    assert!(g.allows(&face, Priority::Body) && !g.allows(&face, Priority::Component));
  }

  /// The Select Other list names each row by type on its Depth tab, and lists the body and the
  /// component on Parents.
  #[test]
  fn select_other_rows_by_type_with_parents() {
    let mut app = cube();
    let b = app.s.model.bodies[0].id;
    let hits = fox_engine::scene::ray_hits(&app.s, DVec3::new(10.0, 10.0, 50.0), DVec3::new(0.0, 0.0, -1.0));
    assert_eq!(hits.len(), 2, "top and bottom faces: {hits:?}");
    let m = OtherMenu::new(&app.s, Pos2::ZERO, None, &hits);
    assert_eq!(m.tab, OtherTab::Depth);
    assert_eq!(m.rows().iter().map(|r| r.2.as_str()).collect::<Vec<_>>(), ["Face", "Face"]);
    assert_eq!(m.parents.iter().map(|r| r.2.as_str()).collect::<Vec<_>>(), ["Body"]);
    assert_eq!(m.parents[0].0, [Sel::Body { body: b }]);
    // the hovered edge leads; a component joins the parents
    app.run("component.create", serde_json::json!({"name": "Bracket", "bodies": [b.0]}));
    let edge = edge_at(&app, [10.0, 0.0, 20.0]);
    let m = OtherMenu::new(&app.s, Pos2::ZERO, Some(Pick::Edge { body: b, edge }), &hits);
    assert_eq!(m.depth.iter().map(|r| r.2.as_str()).collect::<Vec<_>>(), ["Edge", "Face", "Face"]);
    assert_eq!(m.parents.iter().map(|r| r.2.as_str()).collect::<Vec<_>>(), ["Body", "Component"]);
  }

  /// A Selection Tools dialog runs its command on OK and keeps the result selected; an error
  /// stays in the dialog.
  #[test]
  fn selection_tool_dialogs_run_their_commands() {
    let mut app = cube();
    let b = app.s.model.bodies[0].id;
    open_tool(&mut app, SelTool::ByName);
    assert_eq!(app.sel_tool.as_ref().map(|d| d.tool), Some(SelTool::ByName));
    assert_eq!(app.status.as_ref().map(|(t, _)| t.as_str()), Some(SelTool::ByName.prompt()));
    app.sel_tool.as_mut().unwrap().name = "bod*".into();
    apply_tool(&mut app);
    assert!(app.sel_tool.is_none(), "closed on success");
    assert_eq!(app.s.selection, [Sel::Body { body: b }]);
    open_tool(&mut app, SelTool::BySize);
    app.sel_tool.as_mut().unwrap().size = "nonsense".into();
    apply_tool(&mut app);
    assert!(app.sel_tool.as_ref().is_some_and(|d| d.error.is_some()), "the error stays in the dialog");
    app.sel_tool.as_mut().unwrap().size = "40 mm".into();
    apply_tool(&mut app);
    assert!(app.sel_tool.is_none() && app.s.selection == [Sel::Body { body: b }]);
    // Seed And Boundary takes the selected face and edges
    let body = app.s.model.body(b).unwrap();
    let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let rim: Vec<Sel> =
      body.edges().iter().filter(|e| e.faces.contains(&top)).map(|e| Sel::Edge { body: b, edge: e.id }).collect();
    app.s.selection = vec![Sel::Face { body: b, face: top }];
    app.s.selection.extend(rim);
    open_tool(&mut app, SelTool::SeedAndBoundary);
    apply_tool(&mut app);
    assert_eq!(app.s.selection, [Sel::Face { body: b, face: top }]);
  }

  #[test]
  fn double_click_selects_a_box_edges_loop() {
    let app = cube();
    let b = &app.s.model.bodies[0];
    let front_top = edge_at(&app, [10.0, 0.0, 20.0]);
    // seen from above: the top face's loop
    let lp = edge_loop(b.edges(), b.faces(), front_top, DVec3::Z);
    assert_eq!(mids(&app, &lp), vec![[0, 10, 20], [10, 0, 20], [10, 20, 20], [20, 10, 20]]);
    // seen from the front: the front face's loop
    let lp = edge_loop(b.edges(), b.faces(), front_top, -DVec3::Y);
    assert_eq!(mids(&app, &lp), vec![[0, 0, 10], [10, 0, 0], [10, 0, 20], [20, 0, 10]]);
  }

  #[test]
  fn double_click_selects_a_filleted_loop() {
    let mut app = cube();
    app.run("fillet", serde_json::json!({"edges": [{"point": [20, 20, 10]}], "radius": 5}));
    let b = &app.s.model.bodies[0];
    // the round's arc on the top: the top face's loop, four lines and the arc
    let arc = b.edges().iter().find(|e| e.kind == CurveKind::Circle && e.mid.z > 19.0).expect("no arc on the top").id;
    let lp = edge_loop(b.edges(), b.faces(), arc, DVec3::new(1.0, 1.0, 1.0).normalize());
    assert_eq!(lp.len(), 5, "{:?}", mids(&app, &lp));
    assert!(lp.contains(&arc));
    assert!(lp.iter().all(|id| b.edges().iter().any(|e| e.id == *id && (e.mid.z - 20.0).abs() < 1e-6)));
    // the round's straight side edge on x = 20, seen from +X: the side face's loop
    let side = edge_at(&app, [20.0, 15.0, 10.0]);
    let lp = edge_loop(b.edges(), b.faces(), side, DVec3::X);
    assert_eq!(lp.len(), 4, "{:?}", mids(&app, &lp));
    assert!(lp.iter().all(|id| b.edges().iter().any(|e| e.id == *id && (e.mid.x - 20.0).abs() < 1e-6)));
  }

  #[test]
  fn double_click_replaces_or_adds_to_the_selection() {
    let mut app = cube();
    app.cam.pitch = 1.2;
    let body = app.s.model.bodies[0].id;
    let bottom = Sel::Edge { body, edge: edge_at(&app, [10.0, 0.0, 0.0]) };
    let front_top = edge_at(&app, [10.0, 0.0, 20.0]);
    app.s.selection = vec![bottom];
    pick_loop(&mut app, body, front_top, false);
    assert_eq!(app.s.selection.len(), 4);
    assert!(!app.s.selection.contains(&bottom), "a plain double-click replaces the selection");
    // shift/ctrl: the first click of the pair toggled the edge out again; the loop comes back whole
    app.s.selection = vec![bottom];
    pick_loop(&mut app, body, front_top, true);
    assert_eq!(app.s.selection.len(), 5);
    assert_eq!(app.s.selection[0], bottom);
    // body priority: a double-click picks as a click does
    app.s.selection = vec![bottom];
    app.sel_priority = Priority::Body;
    pick_loop(&mut app, body, front_top, false);
    assert_eq!(app.s.selection, vec![bottom]);
  }

  fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
  }

  #[test]
  fn polygon_inside_and_spans() {
    // a concave "L": the notch is outside
    let l = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 4.0], [4.0, 4.0], [4.0, 10.0], [0.0, 10.0]];
    assert!(point_in_polygon([2.0, 2.0], &l));
    assert!(point_in_polygon([2.0, 8.0], &l));
    assert!(!point_in_polygon([8.0, 8.0], &l));
    assert!(!point_in_polygon([-1.0, 2.0], &l));
    assert_eq!(polygon_spans(&l, 1, 100), vec![(0, 10)]);
    assert_eq!(polygon_spans(&l, 6, 100), vec![(0, 4)]);
    assert_eq!(polygon_spans(&l, 12, 100), vec![]);
    // clamped to the frame
    assert_eq!(polygon_spans(&square(-5.0, 0.0, 50.0, 3.0), 1, 20), vec![(0, 20)]);
    // a U-shaped loop: two spans on a row through both arms
    let two = vec![[0.0, 0.0], [3.0, 0.0], [3.0, 2.0], [6.0, 2.0], [6.0, 0.0], [9.0, 0.0], [9.0, 4.0], [0.0, 4.0]];
    assert_eq!(polygon_spans(&two, 1, 100), vec![(0, 3), (6, 9)]);
    // every pixel the spans cover agrees with the point test at its centre
    for y in -1..12 {
      let spans = polygon_spans(&l, y, 100);
      for x in -1..12i64 {
        let covered = spans.iter().any(|(a, b)| (*a..*b).contains(&x));
        let inside = x >= 0 && point_in_polygon([x as f64 + 0.5, y as f64 + 0.5], &l);
        assert_eq!(covered, inside, "pixel {x},{y}");
      }
    }
  }

  /// A 10×10 id buffer: id 1 fills columns 1..=2, id 2 a block at 6..=8 × 6..=8, id 3 one pixel.
  fn buffer() -> Vec<u32> {
    let mut ids = vec![0u32; 100];
    for y in 0..10 {
      for x in 1..=2 {
        ids[y * 10 + x] = 1;
      }
    }
    for y in 6..=8 {
      for x in 6..=8 {
        ids[y * 10 + x] = 2;
      }
    }
    ids[3 * 10 + 7] = 3;
    ids
  }

  #[test]
  fn lasso_counts_what_it_covers() {
    let ids = buffer();
    // a loop round the block and the single pixel: both wholly inside, the column only partly
    let lasso = vec![[5.0, 2.0], [9.5, 2.0], [9.5, 9.5], [0.0, 9.5], [0.0, 5.0], [5.0, 5.0]];
    let got = ids_in_polygon(&ids, 10, 10, &lasso);
    assert_eq!(got.get(&2), Some(&9));
    assert_eq!(got.get(&3), Some(&1));
    // the column: rows 5..=8 of columns 1..=2
    assert_eq!(got.get(&1), Some(&8));
    // a loop that only touches the block's corner
    let touch = square(8.0, 8.0, 9.5, 9.5);
    let got = ids_in_polygon(&ids, 10, 10, &touch);
    assert_eq!(got.get(&2), Some(&1));
    assert!(!got.contains_key(&1));
    // degenerate loops cover nothing
    assert!(ids_in_polygon(&ids, 10, 10, &[[0.0, 0.0], [9.0, 9.0]]).is_empty());
  }

  /// Before anything was drawn to pick from, a selection box still replaces the selection (a
  /// Shift box keeps it), so a stale pick never survives a box over empty space.
  #[test]
  fn a_box_with_nothing_drawn_yet_still_replaces_the_selection() {
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", serde_json::json!({"plane": "xy"}));
    let c = app.run("sketch.line", serde_json::json!({"points": [[0, 0], [10, 0]]})).unwrap()["curves"][0].clone();
    let sk = app.s.edit_sketch.unwrap();
    let curve = fox_engine::sketch::CurveId(c.as_u64().unwrap() as u32);
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve }];
    assert!(app.frame.is_none());
    select_ids(&mut app, &HashMap::new(), false, true);
    assert_eq!(app.s.selection.len(), 1, "an additive box keeps the selection");
    select_ids(&mut app, &HashMap::new(), false, false);
    assert!(app.s.selection.is_empty());
  }

  #[test]
  fn lasso_direction_picks_window_or_crossing() {
    let p = |x: f32, y: f32| Pos2::new(x, y);
    assert!(lasso_window(&[p(0.0, 0.0), p(2.0, 5.0), p(12.0, 5.0), p(0.0, 20.0)]));
    assert!(!lasso_window(&[p(0.0, 0.0), p(-3.0, 5.0), p(-12.0, 5.0), p(10.0, 20.0)]));
    // a purely vertical scribble counts as a window
    assert!(lasso_window(&[p(0.0, 0.0), p(1.0, 30.0)]));
  }

  #[test]
  fn paint_brush_sweeps_a_capsule() {
    let ids = buffer();
    // a stroke down column 0 with a 1.5 px brush reaches column 1 only
    let got = ids_under_brush(&ids, 10, 10, [0.5, 0.5], [0.5, 9.5], 1.5);
    assert_eq!(got.get(&1), Some(&10));
    assert!(!got.contains_key(&2));
    // a dab on one pixel
    let got = ids_under_brush(&ids, 10, 10, [7.5, 3.5], [7.5, 3.5], 0.5);
    assert_eq!(got.len(), 1);
    assert_eq!(got.get(&3), Some(&1));
    // a brush partly off the frame is clipped, not wrapped round
    let got = ids_under_brush(&ids, 10, 10, [-3.0, 7.5], [-3.0, 7.5], 4.0);
    assert!(got.is_empty());
    assert!((seg_dist([0.0, 5.0], [0.0, 0.0], [0.0, 2.0]) - 3.0).abs() < 1e-12);
    assert!((seg_dist([3.0, 1.0], [0.0, 0.0], [0.0, 2.0]) - 3.0).abs() < 1e-12);
  }
}
