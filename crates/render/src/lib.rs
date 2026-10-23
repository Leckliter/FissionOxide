//! A small software rasterizer for the modeling viewport.
//!
//! It draws shaded triangle meshes, depth-tested polylines (B-rep edges, sketch curves, the
//! grid) and points into an RGBA frame, alongside a depth buffer and an id buffer used for
//! picking (what's under the cursor). Running on the CPU keeps the app portable and lets the CLI
//! render the exact same images headless.
//!
//! World space is Z-up, millimetres.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

use glam::{DMat4, DVec3, DVec4, Vec3};

mod post;
pub use post::{
  AA_EDGE, AO_RADII, AO_STRENGTH, Dome, Post, SHADOW_BIAS, SHADOW_DARKEN, SHADOW_LEAN, SHADOW_MIN_COS, SHADOW_SIZE,
  ShadowMap, ao_at, casts_shadow, shadow_light, shadow_texel, to_light,
};

/// Hostile-size guard for frames.
pub const MAX_DIM: u32 = 8192;

/// Id value meaning "nothing here".
pub const NO_ID: u32 = 0;

/// Orbit camera around a target point.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Camera {
  pub target: [f64; 3],
  /// Rotation about +Z (radians); 0 looks from +Y... see [`Camera::eye_dir`].
  pub yaw: f64,
  /// Elevation above the XY plane (radians), clamped to ±89.9°.
  pub pitch: f64,
  pub distance: f64,
  /// Vertical field of view in radians; ignored when `ortho`.
  pub fov: f64,
  pub ortho: bool,
  /// Turn of the picture about the view axis (radians, counter-clockwise on screen): the
  /// ViewCube's rotate arrows, Free Orbit, and views whose up isn't world Z.
  #[serde(default)]
  pub roll: f64,
}

impl Default for Camera {
  /// Fusion's "home" view: isometric-ish from the front-right-top.
  fn default() -> Self {
    Camera {
      target: [0.0, 0.0, 0.0],
      yaw: std::f64::consts::FRAC_PI_4,
      pitch: 0.6155,
      distance: 200.0,
      fov: 30f64.to_radians(),
      ortho: false,
      roll: 0.0,
    }
  }
}

/// Named standard views (the ViewCube faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardView {
  Home,
  Top,
  Bottom,
  Front,
  Back,
  Left,
  Right,
}

impl StandardView {
  pub fn parse(s: &str) -> Option<Self> {
    Some(match s.to_ascii_lowercase().as_str() {
      "home" | "iso" => StandardView::Home,
      "top" => StandardView::Top,
      "bottom" => StandardView::Bottom,
      "front" => StandardView::Front,
      "back" => StandardView::Back,
      "left" => StandardView::Left,
      "right" => StandardView::Right,
      _ => return None,
    })
  }

  /// The view's (eye direction, screen up) in the ViewCube's own axes: Z up, the front face
  /// facing -Y. Top and bottom keep the front face at the bottom and top of the screen.
  pub fn direction(self) -> (DVec3, DVec3) {
    match self {
      StandardView::Home => (DVec3::new(1.0, -1.0, 1.0).normalize(), DVec3::Z),
      StandardView::Top => (DVec3::Z, DVec3::Y),
      StandardView::Bottom => (-DVec3::Z, -DVec3::Y),
      StandardView::Front => (-DVec3::Y, DVec3::Z),
      StandardView::Back => (DVec3::Y, DVec3::Z),
      StandardView::Right => (DVec3::X, DVec3::Z),
      StandardView::Left => (-DVec3::X, DVec3::Z),
    }
  }
}

impl Camera {
  /// Unit vector from the target towards the eye.
  pub fn eye_dir(&self) -> DVec3 {
    let (sp, cp) = self.pitch.sin_cos();
    let (sy, cy) = self.yaw.sin_cos();
    // yaw = 0 → eye on -Y (looking at the front face), rotating counter-clockwise about +Z.
    DVec3::new(cp * sy, -cp * cy, sp)
  }

  pub fn eye(&self) -> DVec3 {
    DVec3::from(self.target) + self.eye_dir() * self.distance
  }

  /// Camera right / up vectors.
  pub fn basis(&self) -> (DVec3, DVec3, DVec3) {
    let back = self.eye_dir();
    // Z × back, normalized; pitch stays short of ±90°, so this is right even looking straight
    // down, where the cross product vanishes and the yaw would otherwise be lost
    let (sy, cy) = self.yaw.sin_cos();
    let right = DVec3::new(cy, sy, 0.0);
    let up = back.cross(right).normalize();
    if self.roll == 0.0 {
      return (right, up, back);
    }
    let (sr, cr) = self.roll.sin_cos();
    (right * cr + up * sr, up * cr - right * sr, back)
  }

  /// Turns the camera to look along `-back` with `right` across the screen (both unit length and
  /// at right angles), as yaw, pitch and roll.
  pub fn set_basis(&mut self, right: DVec3, back: DVec3) {
    let lim = std::f64::consts::FRAC_PI_2 - 1e-6;
    if back.z.abs() < lim.sin() {
      self.yaw = back.x.atan2(-back.y).rem_euclid(std::f64::consts::TAU);
      self.pitch = back.z.asin();
      self.roll = 0.0;
      let (right0, up0, _) = self.basis();
      self.roll = right.dot(up0).atan2(right.dot(right0));
    } else {
      // straight down or up: the yaw alone turns the view
      self.yaw = right.y.atan2(right.x).rem_euclid(std::f64::consts::TAU);
      self.pitch = lim.copysign(back.z);
      self.roll = 0.0;
    }
  }

  /// Free orbit by screen-space drag deltas (radians): turns about the screen's vertical and
  /// horizontal axes, so the view can roll and go over the top.
  pub fn free_orbit(&mut self, dx: f64, dy: f64) {
    let (right, up, back) = self.basis();
    let q = glam::DQuat::from_axis_angle(up, dx) * glam::DQuat::from_axis_angle(right, -dy);
    self.set_basis((q * right).normalize(), (q * back).normalize());
  }

  /// Looks from direction `dir` (target towards the eye) with `up` as near the top of the
  /// screen as it can be. Straight down or up the yaw carries the turn, so orbiting from there
  /// behaves; elsewhere an up off world Z becomes [`Camera::roll`].
  pub fn look_from(&mut self, dir: DVec3, up: DVec3) {
    let d = dir.normalize_or_zero();
    if d == DVec3::ZERO {
      return;
    }
    let lim = std::f64::consts::FRAC_PI_2 - 1e-6;
    self.pitch = d.z.clamp(-1.0, 1.0).asin().clamp(-lim, lim);
    self.yaw = if d.z > 0.999 {
      (-up.x).atan2(up.y)
    } else if d.z < -0.999 {
      up.x.atan2(-up.y)
    } else {
      d.x.atan2(-d.y)
    };
    self.roll = 0.0;
    let (right, screen_up, back) = self.basis();
    let u = up - back * up.dot(back);
    if u.length_squared() > 1e-12 {
      let r = (-u.dot(right)).atan2(u.dot(screen_up));
      self.roll = if r.abs() < 1e-9 { 0.0 } else { r };
    }
  }

  pub fn set_view(&mut self, v: StandardView) {
    let (dir, up) = v.direction();
    self.look_from(dir, up);
  }

  /// Rotates by screen-space drag deltas (radians).
  pub fn orbit(&mut self, dyaw: f64, dpitch: f64) {
    self.yaw = (self.yaw + dyaw).rem_euclid(std::f64::consts::TAU);
    let lim = std::f64::consts::FRAC_PI_2 - 1e-6;
    self.pitch = (self.pitch + dpitch).clamp(-lim, lim);
  }

  /// Pans by a screen-space delta in pixels for a viewport of height `h`.
  pub fn pan(&mut self, dx_px: f64, dy_px: f64, h: f64) {
    let (right, up, _) = self.basis();
    let per_px = self.view_height() / h.max(1.0);
    let t = DVec3::from(self.target) - right * dx_px * per_px + up * dy_px * per_px;
    self.target = t.to_array();
  }

  /// World-space height visible at the target distance.
  pub fn view_height(&self) -> f64 {
    2.0 * self.distance * (self.fov / 2.0).tan()
  }

  /// Zooms by `factor` (< 1 zooms in), keeping the world point under the cursor fixed when given.
  pub fn zoom(&mut self, factor: f64, toward: Option<DVec3>) {
    let factor = factor.clamp(0.01, 100.0);
    let new_d = (self.distance * factor).clamp(1e-3, 1e7);
    if let Some(p) = toward {
      let t = DVec3::from(self.target);
      let k = 1.0 - new_d / self.distance;
      // move the target along (p - t) projected on the view plane
      let (_, _, back) = self.basis();
      let v = p - t;
      let v = v - back * v.dot(back);
      self.target = (t + v * k).to_array();
    }
    self.distance = new_d;
  }

  /// Frames an axis-aligned box.
  pub fn fit(&mut self, min: DVec3, max: DVec3, aspect: f64) {
    let c = (min + max) / 2.0;
    let r = ((max - min).length() / 2.0).max(1.0);
    self.target = c.to_array();
    let half_fov = (self.fov / 2.0).min((self.fov / 2.0).tan().atan2(1.0 / aspect.max(0.1)));
    self.distance = (r / half_fov.tan().max(1e-3)) * 1.15;
  }

  pub fn view_matrix(&self) -> DMat4 {
    DMat4::look_at_rh(self.eye(), DVec3::from(self.target), self.basis().1)
  }

  pub fn proj_matrix(&self, aspect: f64) -> DMat4 {
    let near = (self.distance * 0.01).max(1e-4);
    let far = self.distance * 100.0 + 1e4;
    if self.ortho {
      let h = self.view_height() / 2.0;
      let w = h * aspect;
      DMat4::orthographic_rh(-w, w, -h, h, -far, far)
    } else {
      DMat4::perspective_rh(self.fov, aspect, near, far)
    }
  }

  pub fn view_proj(&self, w: u32, h: u32) -> DMat4 {
    self.proj_matrix(w as f64 / h.max(1) as f64) * self.view_matrix()
  }

  /// Projects a world point to pixel coordinates and NDC depth (None if behind the camera).
  pub fn project(&self, p: DVec3, w: u32, h: u32) -> Option<(f64, f64, f64)> {
    let c = self.view_proj(w, h) * p.extend(1.0);
    if c.w <= 1e-9 {
      return None;
    }
    let n = c.truncate() / c.w;
    Some(((n.x + 1.0) * 0.5 * w as f64, (1.0 - n.y) * 0.5 * h as f64, n.z))
  }

  /// World-space ray through a pixel.
  pub fn ray(&self, px: f64, py: f64, w: u32, h: u32) -> (DVec3, DVec3) {
    let inv = self.view_proj(w, h).inverse();
    let nx = px / w.max(1) as f64 * 2.0 - 1.0;
    let ny = 1.0 - py / h.max(1) as f64 * 2.0;
    let a = inv * DVec4::new(nx, ny, -1.0, 1.0);
    let b = inv * DVec4::new(nx, ny, 1.0, 1.0);
    let a = a.truncate() / a.w;
    let b = b.truncate() / b.w;
    (a, (b - a).normalize_or_zero())
  }
}

/// Intersects a ray with a plane (point + normal); returns the hit point.
pub fn ray_plane(origin: DVec3, dir: DVec3, p0: DVec3, n: DVec3) -> Option<DVec3> {
  let d = dir.dot(n);
  if d.abs() < 1e-12 {
    return None;
  }
  let t = (p0 - origin).dot(n) / d;
  (t.is_finite()).then(|| origin + dir * t)
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
  pub positions: Vec<[f32; 3]>,
  pub normals: Vec<[f32; 3]>,
  pub indices: Vec<u32>,
  /// Pick id per triangle (indices.len() / 3 entries), or empty to use `id` for all.
  pub tri_ids: Vec<u32>,
  pub id: u32,
  pub color: [u8; 4],
  /// Highlighted triangles' id (drawn tinted), e.g. the hovered face.
  pub highlight: Option<u32>,
  pub highlight_color: [u8; 4],
  /// Per-vertex colours (e.g. a curvature map); empty = `color`.
  pub colors: Vec<[u8; 4]>,
  /// Shown with the scene's surface analysis (bodies, not sketch profiles).
  pub analyze: bool,
  /// A body surface the analysis leaves alone: one not picked for it (drawn plainly).
  pub unanalyzed: bool,
  /// Drawn in its colour without lighting (a sketch profile, which Fusion shades the same from
  /// every angle).
  pub unlit: bool,
}

impl Mesh {
  /// Drawn see-through (colour alpha below 255): after the opaque geometry, blended, without
  /// writing depth. One without a pick id ([`NO_ID`]) leaves the ids behind it alone too.
  pub fn transparent(&self) -> bool {
    self.color[3] < 255
  }
}

/// A flat see-through quad drawn with the scene's transparent meshes: hidden by what is in front
/// of it, blended over what is behind, unlit, writing neither depth nor pick ids. The viewport's
/// origin planes are these: they keep their size on screen, so they change with the camera and
/// are handed to the renderer apart from the scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
  pub corners: [[f32; 3]; 4],
  pub color: [u8; 4],
}

impl Quad {
  /// The quad as a mesh of two triangles.
  pub fn mesh(&self) -> Mesh {
    let [a, b, c, _] = self.corners.map(Vec3::from);
    let n = (b - a).cross(c - a).normalize_or_zero().to_array();
    Mesh {
      positions: self.corners.to_vec(),
      normals: vec![n; 4],
      indices: vec![0, 1, 2, 0, 2, 3],
      id: NO_ID,
      color: self.color,
      unlit: true,
      ..Default::default()
    }
  }
}

/// A surface analysis drawn on the meshes that ask for it, as in Fusion's INSPECT menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Analysis {
  /// Reflected stripes: kinks and steps in the stripes show where surfaces don't join smoothly.
  /// `vertical`: the stripes run up and down rather than across.
  Zebra { stripes: f32, vertical: bool },
  /// By draft angle along the pull direction (radians): green at `hi` and above, red at `lo` and
  /// below, shading through yellow in between; within `tol` of no draft (when above 0) yellow.
  Draft { pull: [f32; 3], lo: f32, hi: f32, tol: f32 },
  /// Green where a tool coming along `dir` reaches the face, red where it can't.
  Accessibility { dir: [f32; 3] },
  /// A studio environment reflected in the surface (Fusion's Environment Map): bright sky over a
  /// dark floor, turned `rotate` radians about Z; `mirror` reflects the sky into the floor;
  /// `sharp` adds bright light strips.
  Environment { rotate: f32, mirror: bool, sharp: bool },
}

/// The colour a surface analysis gives a point with normal `n` (facing the viewer) seen along
/// `view`, lit by `diff` (see `raster_tri`). The GL mesh shader repeats this arithmetic.
pub fn analysis_tone(a: Analysis, n: Vec3, view: Vec3, diff: f32) -> Vec3 {
  let green = Vec3::new(0.2, 0.75, 0.25);
  let red = Vec3::new(0.85, 0.2, 0.2);
  let yellow = Vec3::new(0.95, 0.85, 0.2);
  match a {
    Analysis::Zebra { stripes, vertical } => {
      // stripes of a striped environment reflected in the surface
      let r = n * (2.0 * n.dot(view)) - view;
      let s = if vertical { r.x } else { r.z };
      let band = ((s * 0.5 + 0.5) * stripes).floor() as i64;
      let tone = if band % 2 == 0 { 0.1 } else { 0.95 };
      Vec3::splat(tone * (0.75 + 0.25 * diff))
    }
    Analysis::Draft { pull, lo, hi, tol } => {
      let d = n.dot(Vec3::from(pull).normalize_or_zero()).clamp(-1.0, 1.0);
      let ang = d.asin();
      let tone = if ang >= hi {
        green
      } else if ang <= lo {
        red
      } else if tol > 0.0 && ang.abs() <= tol {
        yellow
      } else if ang > 0.0 {
        yellow.lerp(green, ang / hi)
      } else {
        yellow.lerp(red, ang / lo)
      };
      tone * (0.45 + diff)
    }
    Analysis::Accessibility { dir } => {
      let d = n.dot(Vec3::from(dir).normalize_or_zero());
      let tone = if d > 1e-3 { green } else { red };
      tone * (0.45 + diff)
    }
    Analysis::Environment { rotate, mirror, sharp } => {
      let r = n * (2.0 * n.dot(view)) - view;
      let (s, c) = rotate.sin_cos();
      let (x, y) = (r.x * c - r.y * s, r.x * s + r.y * c);
      let z = if mirror { r.z.abs() } else { r.z };
      let mut tone = if z >= 0.0 { 0.55 + 0.4 * z } else { 0.22 + 0.12 * z };
      if sharp && z > 0.2 && z < 0.45 && (3.0 * y.atan2(x)).cos() > 0.5 {
        tone = 1.0;
      }
      Vec3::splat(tone * (0.85 + 0.15 * diff))
    }
  }
}

#[derive(Debug, Clone)]
pub struct Polyline {
  pub points: Vec<[f32; 3]>,
  pub color: [u8; 4],
  /// Line width in pixels (1–4).
  pub width: f32,
  pub id: u32,
  /// Drawn on top of everything (sketch geometry in sketch mode).
  pub on_top: bool,
  /// Dashed (construction geometry): 9 px dashes with 3 px gaps, see [`dash_gap`].
  pub dashed: bool,
  /// Dash-dot (a sketch centerline): a long and a short dash in turn, see [`dash_gap`]. Only
  /// read while `dashed` is set.
  pub dash_dot: bool,
  /// Pushed back in depth so coincident faces/profiles cover it (the ground grid).
  #[doc(hidden)]
  pub behind: bool,
}

#[derive(Debug, Clone)]
pub struct Marker {
  pub at: [f32; 3],
  pub color: [u8; 4],
  pub size: f32,
  pub id: u32,
  pub on_top: bool,
  /// An open circle: a one-pixel ring in `color` around a white disc (Fusion's sketch point),
  /// instead of a filled disc.
  pub hollow: bool,
}

/// Fusion's visual styles (Display Settings ▸ Visual Style): how body surfaces and their edges
/// are drawn, plus two line-drawing styles (`view.render` only; Fusion's menu has six rows).
///
/// Fusion's default, Shaded, draws the bodies' edges in light grey and lets them be picked, so
/// [`VisualStyle::Shaded`] draws them too. Shaded with Visible Edges Only draws them dark instead
/// (see [`Scene::restyle_edges`]) and outlines curved faces with their silhouettes, the crisp
/// line look of Fusion's older default; Shaded with Hidden Edges draws the same and adds the
/// edges behind the surfaces, faint dark dashes through the shading.
///
/// Body surfaces are the meshes marked [`Mesh::analyze`]; their edges are the depth-tested lines
/// with a pick id (sketch profiles, the grid and construction geometry draw as they always do).
/// The styles that don't shade the surfaces also draw their silhouettes (see [`fold_edges`]), so
/// curved faces keep their outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualStyle {
  #[default]
  Shaded,
  ShadedVisibleEdges,
  ShadedHiddenEdges,
  /// Every edge, nothing hidden: the surfaces aren't drawn at all.
  Wireframe,
  WireframeHiddenEdges,
  WireframeVisibleEdges,
  /// A line drawing: plain white surfaces, visible edges and silhouettes solid, hidden ones dashed.
  HiddenLine,
  /// A line drawing with only what can be seen: plain white surfaces, visible edges and silhouettes.
  VisibleLine,
}

impl VisualStyle {
  pub const ALL: [VisualStyle; 8] = [
    VisualStyle::Shaded,
    VisualStyle::ShadedVisibleEdges,
    VisualStyle::ShadedHiddenEdges,
    VisualStyle::Wireframe,
    VisualStyle::WireframeHiddenEdges,
    VisualStyle::WireframeVisibleEdges,
    VisualStyle::HiddenLine,
    VisualStyle::VisibleLine,
  ];

  /// The name in the menu (Fusion's, for the styles Fusion has).
  pub fn label(self) -> &'static str {
    match self {
      VisualStyle::Shaded => "Shaded",
      VisualStyle::ShadedVisibleEdges => "Shaded with Visible Edges Only",
      VisualStyle::ShadedHiddenEdges => "Shaded with Hidden Edges",
      VisualStyle::Wireframe => "Wireframe",
      VisualStyle::WireframeHiddenEdges => "Wireframe with Hidden Edges",
      VisualStyle::WireframeVisibleEdges => "Wireframe with Visible Edges Only",
      VisualStyle::HiddenLine => "Hidden Line",
      VisualStyle::VisibleLine => "Visible Lines Only",
    }
  }

  /// The `snake_case` name, as it serializes and as `view.render` takes it.
  pub fn name(self) -> &'static str {
    match self {
      VisualStyle::Shaded => "shaded",
      VisualStyle::ShadedVisibleEdges => "shaded_visible_edges",
      VisualStyle::ShadedHiddenEdges => "shaded_hidden_edges",
      VisualStyle::Wireframe => "wireframe",
      VisualStyle::WireframeHiddenEdges => "wireframe_hidden_edges",
      VisualStyle::WireframeVisibleEdges => "wireframe_visible_edges",
      VisualStyle::HiddenLine => "hidden_line",
      VisualStyle::VisibleLine => "visible_line",
    }
  }

  pub fn parse(s: &str) -> Option<Self> {
    VisualStyle::ALL.into_iter().find(|v| v.name().eq_ignore_ascii_case(s.trim()))
  }

  /// Body surfaces are shaded (else they only hide what's behind them, or nothing).
  pub fn shaded(self) -> bool {
    matches!(self, VisualStyle::Shaded | VisualStyle::ShadedVisibleEdges | VisualStyle::ShadedHiddenEdges)
  }

  /// Body surfaces are filled plain white, unlit, and pick like shaded ones (the line drawings).
  pub fn line_drawing(self) -> bool {
    matches!(self, VisualStyle::HiddenLine | VisualStyle::VisibleLine)
  }

  /// Body edges are drawn (in every style, as Fusion's Shaded draws them too).
  pub fn edges(self) -> bool {
    true
  }

  /// Body silhouettes are drawn as edges: in every style that doesn't shade the surfaces, and in
  /// the shaded styles with dark edges, whose outlines set them apart from Shaded.
  pub fn silhouettes(self) -> bool {
    !self.shaded() || self.dark_edges()
  }

  /// Body edges are drawn dark ([`SILHOUETTE_COLOR`]) rather than in the scene's light grey: in
  /// Shaded with Visible Edges Only and in Shaded with Hidden Edges, whose hidden edges would
  /// vanish into the shading in the light grey.
  pub fn dark_edges(self) -> bool {
    matches!(self, VisualStyle::ShadedVisibleEdges | VisualStyle::ShadedHiddenEdges)
  }

  /// Edges behind the surfaces show, dashed.
  pub fn hidden_edges(self) -> bool {
    matches!(self, VisualStyle::ShadedHiddenEdges | VisualStyle::WireframeHiddenEdges | VisualStyle::HiddenLine)
  }

  /// The opacity hidden edges are drawn at: faint over shading and wireframes, firm in a line
  /// drawing, where the dashes alone set them apart.
  pub fn hidden_alpha(self) -> f32 {
    if self.line_drawing() { HIDDEN_LINE_ALPHA } else { HIDDEN_EDGE_ALPHA }
  }

  /// Body surfaces hide what's behind them (all but plain Wireframe).
  pub fn occludes(self) -> bool {
    self != VisualStyle::Wireframe
  }
}

/// How a hidden edge is drawn: its colour at this opacity, dashed.
pub const HIDDEN_EDGE_ALPHA: f32 = 0.45;
/// A hidden edge's opacity in [`VisualStyle::HiddenLine`].
pub const HIDDEN_LINE_ALPHA: f32 = 0.8;
/// The fill of body surfaces in the line-drawing styles.
pub const PAPER: [u8; 4] = [255, 255, 255, 255];
/// The colour silhouettes are drawn in (the scene builder's edge colour).
pub const SILHOUETTE_COLOR: [u8; 4] = [35, 38, 44, 255];

/// A mesh edge shared by exactly two triangles that don't lie in one plane: part of the
/// silhouette whenever one of them faces the eye and the other faces away. Built once per mesh
/// by [`fold_edges`]; which ones are silhouettes depends on the camera ([`FoldEdge::silhouette`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoldEdge {
  pub a: [f32; 3],
  pub b: [f32; 3],
  /// The two triangles' normals, turned to the side their vertex normals face.
  pub normals: [[f32; 3]; 2],
  /// The two triangles' pick ids.
  pub ids: [u32; 2],
}

/// Where the eye is, for which way a triangle faces.
#[derive(Debug, Clone, Copy)]
pub enum Eye {
  /// Perspective: the eye point.
  Point(Vec3),
  /// Orthographic: the direction towards the eye.
  Toward(Vec3),
}

impl Eye {
  pub fn of(cam: &Camera) -> Eye {
    if cam.ortho { Eye::Toward(cam.basis().2.as_vec3()) } else { Eye::Point(cam.eye().as_vec3()) }
  }

  /// Whether a plane through `at` with normal `n` faces the eye.
  pub fn faces(self, n: Vec3, at: Vec3) -> bool {
    match self {
      Eye::Point(e) => n.dot(e - at) > 0.0,
      Eye::Toward(d) => n.dot(d) > 0.0,
    }
  }
}

impl FoldEdge {
  /// The pick id of the triangle facing the eye when this edge is on the silhouette.
  pub fn silhouette(&self, eye: Eye) -> Option<u32> {
    let a = Vec3::from(self.a);
    let f = self.normals.map(|n| eye.faces(Vec3::from(n), a));
    (f[0] != f[1]).then(|| if f[0] { self.ids[0] } else { self.ids[1] })
  }
}

/// The edges of a mesh that can be on its silhouette. Vertices at the same position are joined
/// first (tessellations are per face, so faces meet at copies of the same points); edges with one
/// triangle or more than two, and edges inside a plane, are left out.
pub fn fold_edges(m: &Mesh) -> Vec<FoldEdge> {
  // sorting, not hashing: a few times faster on meshes of millions of triangles
  let n = m.positions.len();
  let key = |p: [f32; 3]| p.map(|c| if c == 0.0 { 0 } else { c.to_bits() });
  let mut by_pos: Vec<([u32; 3], u32)> = m.positions.iter().enumerate().map(|(i, p)| (key(*p), i as u32)).collect();
  by_pos.sort_unstable();
  let mut canon = vec![0u32; n];
  for run in by_pos.chunk_by(|a, b| a.0 == b.0) {
    for &(_, i) in run {
      canon[i as usize] = run[0].1;
    }
  }
  drop(by_pos);
  let tris = m.indices.len() / 3;
  let mut normals: Vec<Vec3> = Vec::with_capacity(tris);
  // (low vertex, high vertex) and the triangle, for every triangle side
  let mut sides: Vec<(u64, u32)> = Vec::with_capacity(tris * 3);
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if tri.iter().any(|&i| i as usize >= n) {
      normals.push(Vec3::ZERO);
      continue;
    }
    let p = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.positions[i as usize]));
    let mut nr = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
    let vn: Vec3 = tri.iter().filter_map(|&i| m.normals.get(i as usize)).map(|v| Vec3::from(*v)).sum();
    if nr.dot(vn) < 0.0 {
      nr = -nr;
    }
    normals.push(nr);
    if nr == Vec3::ZERO {
      continue;
    }
    for k in 0..3 {
      let (a, b) = (canon[tri[k] as usize], canon[tri[(k + 1) % 3] as usize]);
      if a != b {
        sides.push(((a.min(b) as u64) << 32 | a.max(b) as u64, t as u32));
      }
    }
  }
  sides.sort_unstable();
  let id = |t: u32| m.tri_ids.get(t as usize).copied().unwrap_or(m.id);
  let mut out = Vec::new();
  for run in sides.chunk_by(|x, y| x.0 == y.0) {
    let [(edge, t0), (_, t1)] = run else { continue };
    let (n0, n1) = (normals[*t0 as usize], normals[*t1 as usize]);
    if t0 == t1 || n0.dot(n1) >= 1.0 - 1e-6 {
      continue;
    }
    let (a, b) = ((edge >> 32) as usize, (edge & 0xffff_ffff) as usize);
    out.push(FoldEdge {
      a: m.positions[a],
      b: m.positions[b],
      normals: [n0.to_array(), n1.to_array()],
      ids: [id(*t0), id(*t1)],
    });
  }
  out
}

/// [`fold_edges`], remembered for the meshes of the last few renders (keyed by their content), so
/// the CPU rasterizer doesn't rebuild them every frame.
pub fn fold_edges_cached(m: &Mesh) -> std::sync::Arc<Vec<FoldEdge>> {
  use std::hash::Hasher;
  use std::sync::{Arc, Mutex};
  static CACHE: Mutex<Vec<(u64, Arc<Vec<FoldEdge>>)>> = Mutex::new(Vec::new());
  let mut h = std::collections::hash_map::DefaultHasher::new();
  let mix =
    |v: &[u32]| v.iter().fold(0u64, |acc, &x| acc.rotate_left(5) ^ x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
  let flat: Vec<u32> = m.positions.iter().flat_map(|p| p.map(f32::to_bits)).collect();
  h.write_u64(mix(&flat));
  h.write_u64(mix(&m.indices));
  h.write_u64(mix(&m.tri_ids));
  h.write_usize(flat.len() ^ m.indices.len() << 1 ^ m.tri_ids.len() << 2);
  h.write_u32(m.id);
  let normals: Vec<u32> = m.normals.iter().flat_map(|p| p.map(f32::to_bits)).collect();
  h.write_u64(mix(&normals));
  let key = h.finish();
  if let Ok(c) = CACHE.lock()
    && let Some((_, f)) = c.iter().find(|(k, _)| *k == key)
  {
    return f.clone();
  }
  let f = Arc::new(fold_edges(m));
  if let Ok(mut c) = CACHE.lock() {
    if c.len() >= 64 {
      c.remove(0);
    }
    c.push((key, f.clone()));
  }
  f
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
  pub meshes: Vec<Mesh>,
  pub lines: Vec<Polyline>,
  pub markers: Vec<Marker>,
  pub background_top: [u8; 4],
  pub background_bottom: [u8; 4],
  pub analysis: Option<Analysis>,
  /// How far the analysis colours fade back to the plain shading (1 − Fusion's Opacity / 100).
  pub analysis_fade: f32,
  pub style: VisualStyle,
  /// The effects drawn into the picture (Environment Dome, Object Shadow, Ambient Occlusion,
  /// Anti-Aliasing; see [`Post`]).
  pub post: Post,
}

impl Scene {
  /// Draws the body edges in `plain` (the scene builder's light grey; hovered and selected edges
  /// keep their colours) as the style wants them: dark in Shaded with Visible Edges Only.
  pub fn restyle_edges(&mut self, plain: [u8; 4]) {
    if !self.style.dark_edges() {
      return;
    }
    for l in self.lines.iter_mut().filter(|l| is_edge(l) && l.color == plain) {
      l.color = SILHOUETTE_COLOR;
    }
  }
}

pub struct Frame {
  pub width: u32,
  pub height: u32,
  pub rgba: Vec<u8>,
  pub depth: Vec<f32>,
  pub ids: Vec<u32>,
}

impl Frame {
  pub fn new(width: u32, height: u32) -> Self {
    let (w, h) = (width.clamp(1, MAX_DIM), height.clamp(1, MAX_DIM));
    let n = (w * h) as usize;
    Frame { width: w, height: h, rgba: vec![0; n * 4], depth: vec![f32::INFINITY; n], ids: vec![NO_ID; n] }
  }

  /// Id under a pixel, searching a small radius so thin edges are easy to hit.
  pub fn pick(&self, x: f64, y: f64, radius: i32, prefer: impl Fn(u32) -> bool) -> u32 {
    let (cx, cy) = (x as i32, y as i32);
    let mut best = NO_ID;
    let mut best_d = i32::MAX;
    for dy in -radius..=radius {
      for dx in -radius..=radius {
        let (px, py) = (cx + dx, cy + dy);
        if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
          continue;
        }
        let id = self.ids.get((py as u32 * self.width + px as u32) as usize).copied().unwrap_or(NO_ID);
        if id == NO_ID {
          continue;
        }
        let d = dx * dx + dy * dy;
        let d = if prefer(id) { d } else { d + 1000 };
        if d < best_d {
          best_d = d;
          best = id;
        }
      }
    }
    best
  }

  pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
    image::save_buffer(path, &self.rgba, self.width, self.height, image::ExtendedColorType::Rgba8)
      .map_err(|e| e.to_string())
  }

  /// The frame encoded as PNG.
  pub fn to_png(&self) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::write_buffer_with_format(
      &mut out,
      &self.rgba,
      self.width,
      self.height,
      image::ExtendedColorType::Rgba8,
      image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
  }
}

/// Display Settings ▸ Effects: a ground under the body surfaces (the meshes marked
/// [`Mesh::analyze`]) at their lowest point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Effects {
  /// The bodies' outline cast onto the ground by a light high above, darkened.
  pub ground_shadow: bool,
  /// The bodies mirrored under a glossy ground (Fusion's Ground Reflection).
  pub reflections: bool,
  /// A see-through ground plane.
  pub ground_plane: bool,
  /// The environment's dome behind the model (see [`Post`]).
  #[serde(default)]
  pub environment_dome: bool,
  /// Bodies casting shadows onto each other (see [`Post`]).
  #[serde(default)]
  pub object_shadow: bool,
  /// Ambient occlusion (see [`Post`]).
  #[serde(default)]
  pub ambient_occlusion: bool,
  /// Anti-aliasing (see [`Post`]).
  #[serde(default)]
  pub anti_aliasing: bool,
}

impl Effects {
  /// Adds the ground, shadow and reflections to a built scene, the ground at the bodies' lowest
  /// point. They have no pick id.
  pub fn apply(self, scene: &mut Scene) {
    self.apply_at(scene, None);
  }

  /// [`Effects::apply`] with Ground Plane Offset: the ground `offset` below the bodies' lowest
  /// point (`None`: Adaptive, at it). The effects drawn into the picture go to
  /// [`Scene::post`], with `dome` for Environment Dome.
  pub fn apply_with(self, scene: &mut Scene, offset: Option<f32>, dome: Dome) {
    scene.post = Post {
      dome: self.environment_dome.then_some(dome),
      object_shadow: self.object_shadow,
      ambient_occlusion: self.ambient_occlusion,
      anti_aliasing: self.anti_aliasing,
    };
    self.apply_at(scene, offset);
  }

  fn apply_at(self, scene: &mut Scene, offset: Option<f32>) {
    if !(self.ground_shadow || self.reflections || self.ground_plane) {
      return;
    }
    let bodies: Vec<&Mesh> = scene.meshes.iter().filter(|m| m.analyze).collect();
    let pts = || bodies.iter().flat_map(|m| m.positions.iter());
    let Some(first) = pts().next() else { return };
    let (mut lo, mut hi) = (Vec3::from(*first), Vec3::from(*first));
    for p in pts() {
      lo = lo.min(Vec3::from(*p));
      hi = hi.max(Vec3::from(*p));
    }
    let size = (hi - lo).max_element().max(1.0);
    let z0 = lo.z - offset.filter(|o| o.is_finite()).unwrap_or(0.0).max(0.0);
    let lift = size * 1e-3;
    let flat = |positions: Vec<[f32; 3]>, indices: Vec<u32>, color: [u8; 4]| Mesh {
      normals: vec![[0.0, 0.0, 1.0]; positions.len()],
      positions,
      indices,
      color,
      ..Default::default()
    };
    let mut added = Vec::new();
    if self.reflections && scene.style.shaded() {
      for m in &bodies {
        let mut r = Mesh {
          positions: m.positions.iter().map(|p| [p[0], p[1], 2.0 * z0 - p[2] - 2.0 * lift]).collect(),
          normals: m.normals.iter().map(|n| [n[0], n[1], -n[2]]).collect(),
          tri_ids: Vec::new(),
          id: NO_ID,
          highlight: None,
          colors: Vec::new(),
          analyze: false,
          unanalyzed: false,
          ..(*m).clone()
        };
        r.color = [r.color[0], r.color[1], r.color[2], 255];
        added.push(r);
      }
    }
    if self.ground_plane || self.reflections {
      let (c, e) = ((lo + hi) / 2.0, size * 1.5);
      let z = z0 - lift;
      let corners = vec![[c.x - e, c.y - e, z], [c.x + e, c.y - e, z], [c.x + e, c.y + e, z], [c.x - e, c.y + e, z]];
      // over the reflections the ground is nearly opaque, so they read as a sheen
      let color = if self.reflections { [222, 226, 232, 190] } else { [190, 196, 205, 100] };
      added.push(flat(corners, vec![0, 1, 2, 0, 2, 3], color));
    }
    if self.ground_shadow {
      // the light leans a little, so a body's sides cast some shadow beside its footprint
      let [sx, sy] = SHADOW_LEAN;
      let hull = convex_hull(pts().map(|p| [p[0] + (p[2] - z0) * sx, p[1] + (p[2] - z0) * sy]).collect());
      if hull.len() >= 3 {
        let z = z0 - lift * 0.5;
        let positions: Vec<[f32; 3]> = hull.iter().map(|p| [p[0], p[1], z]).collect();
        let indices = (1..hull.len() as u32 - 1).flat_map(|i| [0, i, i + 1]).collect();
        added.push(flat(positions, indices, [40, 44, 52, 90]));
      }
    }
    scene.meshes.extend(added);
  }
}

/// The convex hull of points, counter-clockwise (Andrew's monotone chain).
fn convex_hull(mut p: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
  p.retain(|q| q[0].is_finite() && q[1].is_finite());
  p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
  p.dedup();
  if p.len() < 3 {
    return p;
  }
  let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  let mut h: Vec<[f32; 2]> = Vec::with_capacity(p.len() * 2);
  for pass in 0..2 {
    let start = h.len();
    let it: Box<dyn Iterator<Item = &[f32; 2]>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
    for &q in it {
      while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
        h.pop();
      }
      h.push(q);
    }
    h.pop();
  }
  h
}

/// Renders a scene from a camera into a new frame.
pub fn render(scene: &Scene, cam: &Camera, width: u32, height: u32) -> Frame {
  let mut f = Frame::new(width, height);
  let (w, h) = (f.width, f.height);
  // background gradient, or the environment's dome
  for y in 0..h {
    let t = y as f32 / (h.max(2) - 1) as f32;
    let c = lerp_color(scene.background_top, scene.background_bottom, t);
    for x in 0..w {
      let i = ((y * w + x) * 4) as usize;
      let c = match scene.post.dome {
        Some(d) => d.color(Dome::ray(cam, w, h, x as f32 + 0.5, y as f32 + 0.5)),
        None => c,
      };
      if let Some(px) = f.rgba.get_mut(i..i + 4) {
        px.copy_from_slice(&c);
      }
    }
  }
  let vp = cam.view_proj(w, h);
  let eye = cam.eye();
  let (_, _, back) = cam.basis();
  // light: a key light from upper-left of the view plus the headlight
  let (right, up, _) = cam.basis();
  let key = (back * 0.8 + up * 0.5 - right * 0.3).normalize().as_vec3();
  let head = back.as_vec3();
  let fill = (right * 0.7 - up * 0.2 + back * 0.3).normalize().as_vec3();
  let style = scene.style;
  let lights = [key, head, fill];
  // Object Shadow: on shaded body surfaces
  let shadows = (scene.post.object_shadow && style.shaded()).then(|| ShadowMap::build(&scene.meshes)).flatten();
  let shade_of = |m: &Mesh| Shade {
    vertex_colors: false,
    analysis: scene.analysis.filter(|_| m.analyze && !m.unanalyzed),
    fade: scene.analysis_fade,
    paper: m.analyze && style.line_drawing(),
    unlit: m.unlit,
    shadow: shadows.as_ref().filter(|_| casts_shadow(m)),
  };
  for m in scene.meshes.iter().filter(|m| !m.transparent()) {
    if m.analyze && !style.shaded() && !style.line_drawing() {
      if style.occludes() {
        draw_depth(&mut f, m, &vp);
      }
      continue;
    }
    draw_mesh(&mut f, m, &vp, cam, eye, lights, shade_of(m));
  }
  if scene.post.ambient_occlusion && (style.shaded() || style.line_drawing()) {
    post::ambient_occlusion(&mut f, cam);
  }
  let silhouettes = if style.silhouettes() { silhouette_lines(scene, cam, h) } else { Vec::new() };
  let edges = || scene.lines.iter().filter(|l| is_edge(l)).chain(&silhouettes);
  if style.hidden_edges() {
    // before any line writes depth, so only the surfaces hide them
    for l in edges() {
      draw_hidden(&mut f, l, &vp, style.hidden_alpha());
    }
  }
  for l in scene.lines.iter().filter(|l| !l.on_top).chain(&silhouettes) {
    draw_polyline(&mut f, l, &vp);
  }
  for mk in scene.markers.iter().filter(|m| !m.on_top) {
    draw_marker(&mut f, mk, &vp);
  }
  // transparent meshes blend over everything behind them, edges included, and hide nothing
  let see_through = |m: &&Mesh| m.transparent() && (style.shaded() || style.line_drawing() || !m.analyze);
  for m in scene.meshes.iter().filter(see_through) {
    draw_mesh(&mut f, m, &vp, cam, eye, lights, shade_of(m));
  }
  for l in scene.lines.iter().filter(|l| l.on_top) {
    draw_polyline(&mut f, l, &vp);
  }
  for mk in scene.markers.iter().filter(|m| m.on_top) {
    draw_marker(&mut f, mk, &vp);
  }
  if scene.post.anti_aliasing {
    post::anti_alias(&mut f);
  }
  f
}

/// How far (pixels) a silhouette is moved off the surface it outlines. Next to its outline a
/// curved surface turns towards the eye so fast that, drawn on the edge itself, the line would
/// lose the depth test to its own surface; just outside, only what's in front of it hides it.
pub const SILHOUETTE_NUDGE_PX: f32 = 0.75;

/// The body surfaces' silhouettes from a camera, for a picture `height` pixels high, as edges:
/// one segment per fold edge on the outline (nudged [`SILHOUETTE_NUDGE_PX`] outwards), picking
/// as the triangle that faces the eye.
pub fn silhouette_lines(scene: &Scene, cam: &Camera, height: u32) -> Vec<Polyline> {
  let eye = Eye::of(cam);
  let per_px = cam.view_height() as f32 / cam.distance as f32 / height.max(1) as f32;
  let nudge = |p: [f32; 3], out: Vec3| {
    let p = Vec3::from(p);
    let world_px = match eye {
      Eye::Point(e) => per_px * p.distance(e),
      Eye::Toward(_) => per_px * cam.distance as f32,
    };
    (p + out * world_px * SILHOUETTE_NUDGE_PX).to_array()
  };
  let mut out = Vec::new();
  for m in scene.meshes.iter().filter(|m| m.analyze) {
    for e in fold_edges_cached(m).iter() {
      if let Some(id) = e.silhouette(eye) {
        let n = (Vec3::from(e.normals[0]) + Vec3::from(e.normals[1])).normalize_or_zero();
        out.push(Polyline {
          points: vec![nudge(e.a, n), nudge(e.b, n)],
          color: SILHOUETTE_COLOR,
          width: 1.0,
          id,
          on_top: false,
          dashed: false,
          dash_dot: false,
          behind: false,
        });
      }
    }
  }
  out
}

/// A body edge, for the visual styles: depth-tested and pickable.
pub fn is_edge(l: &Polyline) -> bool {
  !l.on_top && l.id != NO_ID
}

/// A mesh into the depth buffer alone (wireframe surfaces still hide what's behind them).
fn draw_depth(f: &mut Frame, m: &Mesh, vp: &DMat4) {
  let (w, h) = (f.width as f64, f.height as f64);
  for tri in m.indices.chunks_exact(3) {
    let mut v = [CV { c: DVec4::ZERO, n: Vec3::Z, wpos: Vec3::ZERO, col: Vec3::ZERO }; 3];
    let mut ok = true;
    for k in 0..3 {
      let Some(p) = m.positions.get(tri[k] as usize) else {
        ok = false;
        break;
      };
      let wp = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
      v[k].c = *vp * wp.extend(1.0);
    }
    if !ok {
      continue;
    }
    let poly = clip_near(&v);
    for i in 1..poly.len().saturating_sub(1) {
      let s = [poly[0].c, poly[i].c, poly[i + 1].c].map(|c| {
        let iw = 1.0 / c.w;
        ((c.x * iw + 1.0) * 0.5 * w, (1.0 - c.y * iw) * 0.5 * h, c.z * iw)
      });
      let area = (s[1].0 - s[0].0) * (s[2].1 - s[0].1) - (s[2].0 - s[0].0) * (s[1].1 - s[0].1);
      if !area.is_finite() || area.abs() < 1e-12 {
        continue;
      }
      let x0 = s.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
      let x1 = s.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max).ceil().min(w - 1.0) as i64;
      let y0 = s.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
      let y1 = s.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).ceil().min(h - 1.0) as i64;
      for y in y0..=y1 {
        for x in x0..=x1 {
          let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
          let e = |a: (f64, f64, f64), b: (f64, f64, f64)| (b.0 - a.0) * (py - a.1) - (px - a.0) * (b.1 - a.1);
          let (b0, b1, b2) = (e(s[1], s[2]) / area, e(s[2], s[0]) / area, e(s[0], s[1]) / area);
          if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
            continue;
          }
          let z = (b0 * s[0].2 + b1 * s[1].2 + b2 * s[2].2) as f32;
          if let Some(d) = f.depth.get_mut((y * f.width as i64 + x) as usize)
            && z < *d
          {
            *d = z;
          }
        }
      }
    }
  }
}

/// The parts of an edge behind the surfaces, faint, thin and dashed; depth and ids are left alone.
fn draw_hidden(f: &mut Frame, l: &Polyline, vp: &DMat4, opacity: f32) {
  let ghost = Polyline { width: 1.0, dashed: true, dash_dot: false, ..l.clone() };
  let alpha = (l.color[3] as f32 * opacity) as u32;
  let mut hidden: Vec<usize> = Vec::new();
  let (w, h) = (f.width as i64, f.height as i64);
  rasterize_line(&ghost, vp, f.width, f.height, &mut |x, y, z| {
    if x >= 0 && y >= 0 && x < w && y < h {
      let i = (y * w + x) as usize;
      if f.depth.get(i).is_some_and(|d| z > *d) {
        hidden.push(i);
      }
    }
  });
  hidden.sort_unstable();
  hidden.dedup();
  for i in hidden {
    if let Some(p) = f.rgba.get_mut(i * 4..i * 4 + 4) {
      for (c, ink) in p.iter_mut().zip(l.color).take(3) {
        *c = ((ink as u32 * alpha + *c as u32 * (255 - alpha)) / 255) as u8;
      }
    }
  }
}

fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
  let mut o = [0u8; 4];
  for i in 0..4 {
    o[i] = (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round().clamp(0.0, 255.0) as u8;
  }
  o
}

/// Clip-space vertex.
#[derive(Clone, Copy)]
struct CV {
  c: DVec4,
  n: Vec3,
  wpos: Vec3,
  /// Vertex colour (0–1), when the mesh has per-vertex colours.
  col: Vec3,
}

fn draw_mesh(f: &mut Frame, m: &Mesh, vp: &DMat4, cam: &Camera, eye: DVec3, lights: [Vec3; 3], shade: Shade<'_>) {
  let tris = m.indices.len() / 3;
  let rgb = |c: [u8; 4]| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0;
  let per_vertex = !m.colors.is_empty();
  for t in 0..tris {
    let idx = [m.indices[t * 3], m.indices[t * 3 + 1], m.indices[t * 3 + 2]];
    let mut v = [CV { c: DVec4::ZERO, n: Vec3::Z, wpos: Vec3::ZERO, col: Vec3::ZERO }; 3];
    let mut ok = true;
    for k in 0..3 {
      let Some(p) = m.positions.get(idx[k] as usize) else {
        ok = false;
        break;
      };
      let n = m.normals.get(idx[k] as usize).copied().unwrap_or([0.0, 0.0, 1.0]);
      let wp = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
      let col = m.colors.get(idx[k] as usize).map(|c| rgb(*c)).unwrap_or_else(|| rgb(m.color));
      v[k] = CV { c: *vp * wp.extend(1.0), n: Vec3::from(n), wpos: Vec3::from(*p), col };
    }
    if !ok {
      continue;
    }
    let id = m.tri_ids.get(t).copied().unwrap_or(m.id);
    let fill = if shade.paper { [PAPER[0], PAPER[1], PAPER[2], m.color[3]] } else { m.color };
    let color = if m.highlight == Some(id) { m.highlight_color } else { fill };
    let color = if m.transparent() { color } else { [color[0], color[1], color[2], 255] };
    // near-plane clipping (z > -w in GL clip space)
    let poly = clip_near(&v);
    if poly.len() < 3 {
      continue;
    }
    // a highlighted (hovered) triangle shows its highlight, not the vertex colours
    let shade = Shade { vertex_colors: per_vertex && !shade.paper && m.highlight != Some(id), ..shade };
    for i in 1..poly.len() - 1 {
      raster_tri(f, [poly[0], poly[i], poly[i + 1]], id, color, cam, eye, lights, shade);
    }
  }
}

fn clip_near(v: &[CV; 3]) -> Vec<CV> {
  let inside = |p: &CV| p.c.z + p.c.w > 0.0;
  if v.iter().all(inside) {
    return v.to_vec();
  }
  let mut out = Vec::with_capacity(4);
  for i in 0..3 {
    let a = v[i];
    let b = v[(i + 1) % 3];
    let (ia, ib) = (inside(&a), inside(&b));
    if ia {
      out.push(a);
    }
    if ia != ib {
      let da = a.c.z + a.c.w;
      let db = b.c.z + b.c.w;
      let t = da / (da - db);
      out.push(CV {
        c: a.c + (b.c - a.c) * t,
        n: a.n + (b.n - a.n) * t as f32,
        wpos: a.wpos + (b.wpos - a.wpos) * t as f32,
        col: a.col + (b.col - a.col) * t as f32,
      });
    }
  }
  out
}

/// How a triangle's pixels are coloured beyond plain lighting.
#[derive(Clone, Copy)]
struct Shade<'a> {
  vertex_colors: bool,
  analysis: Option<Analysis>,
  /// The analysis colours' fade to plain shading ([`Scene::analysis_fade`]).
  fade: f32,
  /// Flat and unlit (the line-drawing styles' surfaces).
  paper: bool,
  /// Unlit in its own colour ([`Mesh::unlit`]).
  unlit: bool,
  /// Object Shadow's map, for a body surface.
  shadow: Option<&'a ShadowMap>,
}

#[allow(clippy::too_many_arguments)]
fn raster_tri(
  f: &mut Frame,
  v: [CV; 3],
  id: u32,
  color: [u8; 4],
  cam: &Camera,
  eye: DVec3,
  lights: [Vec3; 3],
  shade: Shade<'_>,
) {
  let [key, head, fill] = lights;
  let (w, h) = (f.width as f64, f.height as f64);
  let mut sx = [0.0f64; 3];
  let mut sy = [0.0f64; 3];
  let mut sz = [0.0f64; 3];
  let mut inv_w = [0.0f64; 3];
  for k in 0..3 {
    let c = v[k].c;
    if c.w.abs() < 1e-12 {
      return;
    }
    let iw = 1.0 / c.w;
    sx[k] = (c.x * iw + 1.0) * 0.5 * w;
    sy[k] = (1.0 - c.y * iw) * 0.5 * h;
    sz[k] = c.z * iw;
    inv_w[k] = iw;
  }
  let area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (sy[1] - sy[0]);
  if area.abs() < 1e-12 || !area.is_finite() {
    return;
  }
  let minx = sx.iter().cloned().fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
  let maxx = sx.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil().min(w - 1.0) as i64;
  let miny = sy.iter().cloned().fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
  let maxy = sy.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil().min(h - 1.0) as i64;
  if minx > maxx || miny > maxy {
    return;
  }
  // Flat-ish shading per triangle with the face normal blended with vertex normals per pixel.
  let base = Vec3::new(color[0] as f32, color[1] as f32, color[2] as f32) / 255.0;
  let fw = f.width as i64;
  let persp = !cam.ortho;
  for py in miny..=maxy {
    let y = py as f64 + 0.5;
    for px in minx..=maxx {
      let x = px as f64 + 0.5;
      let w0 = ((sx[1] - x) * (sy[2] - y) - (sx[2] - x) * (sy[1] - y)) / area;
      let w1 = ((sx[2] - x) * (sy[0] - y) - (sx[0] - x) * (sy[2] - y)) / area;
      let w2 = 1.0 - w0 - w1;
      if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
        continue;
      }
      let z = w0 * sz[0] + w1 * sz[1] + w2 * sz[2];
      if !(-1.0..=1.0).contains(&z) {
        continue;
      }
      let i = (py * fw + px) as usize;
      let Some(d) = f.depth.get_mut(i) else { continue };
      let zf = z as f32;
      if zf >= *d {
        continue;
      }
      if color[3] == 255 {
        *d = zf;
      }
      // perspective-correct normal interpolation
      let (b0, b1, b2) = if persp {
        let a0 = w0 * inv_w[0];
        let a1 = w1 * inv_w[1];
        let a2 = w2 * inv_w[2];
        let s = a0 + a1 + a2;
        (a0 / s, a1 / s, a2 / s)
      } else {
        (w0, w1, w2)
      };
      let n = (v[0].n * b0 as f32 + v[1].n * b1 as f32 + v[2].n * b2 as f32).normalize_or_zero();
      let wpos = v[0].wpos * b0 as f32 + v[1].wpos * b1 as f32 + v[2].wpos * b2 as f32;
      let view = if persp { (eye.as_vec3() - wpos).normalize_or_zero() } else { head };
      // two-sided: flip normals facing away
      let n = if n.dot(view) < 0.0 { -n } else { n };
      // soft shading, as Fusion's default look: a good deal of ambient light, so the sides of a
      // body stay light grey rather than falling into shadow
      let diff = 0.36 * n.dot(key).max(0.0) + 0.24 * n.dot(head).max(0.0) + 0.15 * n.dot(fill).max(0.0);
      let half = (key + view).normalize_or_zero();
      let spec = n.dot(half).max(0.0).powf(40.0) * 0.12;
      let base =
        if shade.vertex_colors { v[0].col * b0 as f32 + v[1].col * b1 as f32 + v[2].col * b2 as f32 } else { base };
      let c = match shade.analysis {
        _ if shade.paper || shade.unlit => base,
        None => base * (0.5 + diff) + Vec3::splat(spec),
        Some(a) => {
          let tone = analysis_tone(a, n, view, diff);
          tone + (base * (0.5 + diff) + Vec3::splat(spec) - tone) * shade.fade
        }
      };
      let c = match shade.shadow {
        Some(sm) if !(shade.paper || shade.unlit) => sm.shade(c, wpos, n),
        _ => c,
      };
      if let Some(p) = f.rgba.get_mut(i * 4..i * 4 + 4) {
        let a = color[3] as f32 / 255.0;
        for (k, v) in [c.x, c.y, c.z].into_iter().enumerate() {
          p[k] = ((v * 255.0).clamp(0.0, 255.0) * a + p[k] as f32 * (1.0 - a)) as u8;
        }
        p[3] = 255;
      }
      if (color[3] == 255 || id != NO_ID)
        && let Some(slot) = f.ids.get_mut(i)
      {
        *slot = id;
      }
    }
  }
}

fn draw_polyline(f: &mut Frame, l: &Polyline, vp: &DMat4) {
  let (w, h) = (f.width, f.height);
  rasterize_line(l, vp, w, h, &mut |x, y, z| plot(f, x, y, z, l.color, l.id, l.on_top));
}

/// Whether a dashed line is in a gap `along` pixels from its start. Fusion's construction
/// pattern is a 9 px dash and a 3 px gap; its centerline pattern (`dash_dot`) a 9 px dash, a
/// 3 px gap, a 6 px dash and a 3 px gap. The GL line shader repeats this arithmetic.
pub fn dash_gap(along: f64, dash_dot: bool) -> bool {
  let along = along.max(0.0);
  if dash_dot {
    let t = along % 21.0;
    (9.0..12.0).contains(&t) || t >= 18.0
  } else {
    along % 12.0 >= 9.0
  }
}

/// Every pixel a polyline covers (with its depth, biased as [`Polyline::behind`] asks), in
/// order; dashes are left out.
fn rasterize_line(l: &Polyline, vp: &DMat4, width: u32, height: u32, put: &mut dyn FnMut(i64, i64, f32)) {
  let mut dash_phase = 0.0f64;
  for seg in l.points.windows(2) {
    let a = DVec3::new(seg[0][0] as f64, seg[0][1] as f64, seg[0][2] as f64);
    let b = DVec3::new(seg[1][0] as f64, seg[1][1] as f64, seg[1][2] as f64);
    let mut ca = *vp * a.extend(1.0);
    let mut cb = *vp * b.extend(1.0);
    // clip to near plane
    let da = ca.z + ca.w;
    let db = cb.z + cb.w;
    if da <= 0.0 && db <= 0.0 {
      continue;
    }
    if da <= 0.0 {
      ca = ca + (cb - ca) * (da / (da - db));
    } else if db <= 0.0 {
      cb = ca + (cb - ca) * (da / (da - db));
    }
    let to_screen = |c: DVec4| {
      let iw = 1.0 / c.w;
      ((c.x * iw + 1.0) * 0.5 * width as f64, (1.0 - c.y * iw) * 0.5 * height as f64, c.z * iw)
    };
    let (x0, y0, z0) = to_screen(ca);
    let (x1, y1, z1) = to_screen(cb);
    let len = (x1 - x0).hypot(y1 - y0);
    if !len.is_finite() || len > 1e6 {
      continue;
    }
    let steps = len.ceil().max(1.0) as usize;
    let half = (l.width.clamp(1.0, 6.0) as f64 - 1.0) / 2.0;
    for s in 0..=steps {
      let t = s as f64 / steps as f64;
      if l.dashed && dash_gap(dash_phase + t * len, l.dash_dot) {
        continue;
      }
      let x = x0 + (x1 - x0) * t;
      let y = y0 + (y1 - y0) * t;
      // lines win depth ties against the faces they lie on (the grid loses them)
      let bias = if l.behind { 2e-5 } else { -2e-5 };
      let z = (z0 + (z1 - z0) * t) as f32 + bias;
      let r = half.ceil() as i64;
      for oy in -r..=r {
        for ox in -r..=r {
          put(x as i64 + ox, y as i64 + oy, z);
        }
      }
    }
    dash_phase += len;
  }
}

/// The inside of a hollow marker.
const MARKER_FILL: [u8; 4] = [255, 255, 255, 255];

fn draw_marker(f: &mut Frame, m: &Marker, vp: &DMat4) {
  let p = DVec3::new(m.at[0] as f64, m.at[1] as f64, m.at[2] as f64);
  let c = *vp * p.extend(1.0);
  if c.w <= 1e-9 {
    return;
  }
  let x = (c.x / c.w + 1.0) * 0.5 * f.width as f64;
  let y = (1.0 - c.y / c.w) * 0.5 * f.height as f64;
  let z = (c.z / c.w) as f32 - 5e-5;
  let r = m.size.clamp(1.0, 12.0) as i64;
  for oy in -r..=r {
    for ox in -r..=r {
      let d2 = ox * ox + oy * oy;
      if d2 <= r * r {
        // a hollow marker's inside is white; its ring is the outermost pixel of the disc
        let color = if m.hollow && d2 <= (r - 1) * (r - 1) { MARKER_FILL } else { m.color };
        plot(f, x as i64 + ox, y as i64 + oy, z, color, m.id, m.on_top);
      }
    }
  }
}

fn plot(f: &mut Frame, x: i64, y: i64, z: f32, color: [u8; 4], id: u32, on_top: bool) {
  if x < 0 || y < 0 || x >= f.width as i64 || y >= f.height as i64 {
    return;
  }
  let i = (y * f.width as i64 + x) as usize;
  let Some(d) = f.depth.get_mut(i) else { return };
  if !on_top {
    if z > *d {
      return;
    }
    *d = z;
  }
  if let Some(p) = f.rgba.get_mut(i * 4..i * 4 + 4) {
    let a = color[3] as u32;
    for k in 0..3 {
      p[k] = ((color[k] as u32 * a + p[k] as u32 * (255 - a)) / 255) as u8;
    }
    p[3] = 255;
  }
  if id != NO_ID
    && let Some(slot) = f.ids.get_mut(i)
  {
    *slot = id;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn quad(z: f32, color: [u8; 4], id: u32) -> Mesh {
    Mesh {
      positions: vec![[-10.0, -10.0, z], [10.0, -10.0, z], [10.0, 10.0, z], [-10.0, 10.0, z]],
      normals: vec![[0.0, 0.0, 1.0]; 4],
      indices: vec![0, 1, 2, 0, 2, 3],
      tri_ids: vec![],
      id,
      color,
      highlight: None,
      highlight_color: [0; 4],
      colors: vec![],
      analyze: false,
      unanalyzed: false,
      unlit: false,
    }
  }

  #[test]
  fn orbiting_turns_the_top_view() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    let (right, ..) = cam.basis();
    assert!((right - DVec3::X).length() < 1e-9);
    cam.orbit(0.5, 0.2);
    let (turned, up, back) = cam.basis();
    assert!((turned - DVec3::new(0.5f64.cos(), 0.5f64.sin(), 0.0)).length() < 1e-9, "{turned}");
    assert!(turned.dot(up).abs() < 1e-9 && turned.dot(back).abs() < 1e-9);
  }

  #[test]
  fn depth_test_and_picking() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    let scene =
      Scene { meshes: vec![quad(0.0, [255, 0, 0, 255], 1), quad(5.0, [0, 0, 255, 255], 2)], ..Default::default() };
    let f = render(&scene, &cam, 200, 100);
    assert_eq!(f.pick(100.0, 50.0, 0, |_| true), 2, "the upper quad is in front from the top");
    assert_eq!(f.pick(2.0, 2.0, 0, |_| true), NO_ID);
    cam.set_view(StandardView::Bottom);
    let f = render(&scene, &cam, 200, 100);
    assert_eq!(f.pick(100.0, 50.0, 0, |_| true), 1);
  }

  /// A hollow marker is a ring of its colour around a white disc (Fusion's sketch point); it
  /// still picks across the whole disc.
  #[test]
  fn hollow_markers_are_rings_around_white() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    let marker =
      |hollow| Marker { at: [0.0, 0.0, 0.0], color: [0, 0, 255, 255], size: 4.0, id: 7, on_top: true, hollow };
    let f = render(&Scene { markers: vec![marker(true)], ..Default::default() }, &cam, 200, 100);
    fn px(f: &Frame, x: usize, y: usize) -> &[u8] {
      &f.rgba[(y * 200 + x) * 4..(y * 200 + x) * 4 + 3]
    }
    assert_eq!(px(&f, 100, 50), &[255, 255, 255], "white inside");
    assert_eq!(px(&f, 104, 50), &[0, 0, 255], "the ring at the disc's edge");
    assert_eq!(f.pick(100.0, 50.0, 0, |_| true), 7, "the inside still picks");
    let f = render(&Scene { markers: vec![marker(false)], ..Default::default() }, &cam, 200, 100);
    assert_eq!(px(&f, 100, 50), &[0, 0, 255], "a filled marker is solid");
  }

  #[test]
  fn transparent_meshes_blend_and_hide_nothing() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    // the see-through quad comes first but still draws after (and over) the opaque one
    let scene = Scene {
      meshes: vec![quad(5.0, [0, 0, 255, 128], 2), quad(0.0, [255, 0, 0, 255], 1)],
      lines: vec![Polyline {
        points: vec![[-10.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
        color: [0, 255, 0, 255],
        width: 1.0,
        id: 3,
        on_top: false,
        dashed: false,
        dash_dot: false,
        behind: false,
      }],
      ..Default::default()
    };
    let f = render(&scene, &cam, 200, 100);
    assert_eq!(f.pick(100.0, 40.0, 0, |_| true), 2, "it picks");
    let px = &f.rgba[(40 * 200 + 100) * 4..(40 * 200 + 100) * 4 + 3];
    assert!(px[0] > 90 && px[2] > 90 && px[1] < 40, "blended red and blue: {px:?}");
    // the edge under it shows through, tinted
    let row = (0..100).find(|&y| f.rgba[(y * 200 + 100) * 4 + 1] > 60).expect("the line shows");
    assert!(f.rgba[(row * 200 + 100) * 4 + 2] > 60);
  }

  /// A [`Quad`] (an origin plane) is hidden by a body in front of it, blends over one behind it,
  /// and leaves that body's pick id alone.
  #[test]
  fn quads_hide_behind_bodies_and_pick_nothing() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    let q = |z: f32| Quad {
      corners: [[-10.0, -10.0, z], [10.0, -10.0, z], [10.0, 10.0, z], [-10.0, 10.0, z]],
      color: [0, 0, 255, 128],
    };
    let px = |f: &Frame| f.rgba[(50 * 200 + 100) * 4..(50 * 200 + 100) * 4 + 3].to_vec();
    // behind the body: hidden
    let f = render(
      &Scene { meshes: vec![quad(5.0, [255, 0, 0, 255], 1), q(0.0).mesh()], ..Default::default() },
      &cam,
      200,
      100,
    );
    let body =
      px(&render(&Scene { meshes: vec![quad(5.0, [255, 0, 0, 255], 1)], ..Default::default() }, &cam, 200, 100));
    assert_eq!(px(&f), body, "the body covers it");
    // in front: blended over the body, which still picks
    let f = render(
      &Scene { meshes: vec![quad(0.0, [255, 0, 0, 255], 1), q(5.0).mesh()], ..Default::default() },
      &cam,
      200,
      100,
    );
    let c = px(&f);
    assert!(c[0] > 90 && c[2] > 90, "blended: {c:?}");
    assert_eq!(f.pick(100.0, 50.0, 0, |_| true), 1, "the body behind it still picks");
  }

  /// A body face at z = 5 over an edge at z = 0, from the top.
  fn styled(style: VisualStyle) -> Frame {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    let mut face = quad(5.0, [255, 0, 0, 255], 1);
    face.analyze = true;
    let edge = Polyline {
      points: vec![[-10.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
      color: [0, 0, 0, 255],
      width: 1.0,
      id: 3,
      on_top: false,
      dashed: false,
      dash_dot: false,
      behind: false,
    };
    let scene = Scene {
      meshes: vec![face],
      lines: vec![edge],
      background_top: [255; 4],
      background_bottom: [255; 4],
      style,
      ..Default::default()
    };
    render(&scene, &cam, 200, 100)
  }

  /// The pixels of the edge's row, and one well inside the face away from it.
  fn row_and_face(f: &Frame) -> (Vec<[u8; 3]>, [u8; 3]) {
    let px = |x: u32, y: u32| {
      let i = ((y * f.width + x) * 4) as usize;
      [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    // the rows the edge covers: the one with the most ink
    let ink = |y: u32| (85..115).map(|x| px(x, y).iter().map(|&c| 255 - c as u32).sum::<u32>()).sum::<u32>();
    let y = (45..55).max_by_key(|&y| ink(y)).unwrap_or(50);
    ((85..115).map(|x| px(x, y)).collect(), px(100, 40))
  }

  #[test]
  fn visual_styles_shade_and_hide_as_fusion_does() {
    let red = |c: [u8; 3]| c[0] > 150 && c[1] < 120;
    let dark = |c: &[u8; 3]| c.iter().all(|&v| v < 60);
    let faint = |c: &[u8; 3]| !dark(c) && c.iter().map(|&v| v as u32).sum::<u32>() < 3 * 235;

    // shaded with visible edges: the face hides the edge under it
    let (row, face) = row_and_face(&styled(VisualStyle::ShadedVisibleEdges));
    assert!(red(face) && row.iter().all(|c| red(*c)), "{row:?}");
    // shaded, hidden edges: the edge shows through, faint and dashed
    let (row, face) = row_and_face(&styled(VisualStyle::ShadedHiddenEdges));
    assert!(red(face));
    assert!(row.iter().any(|c| !red(*c)) && row.iter().any(|c| red(*c)), "dashed: {row:?}");
    assert!(row.iter().all(|c| !dark(c)), "faint: {row:?}");
    // wireframe: no face colour, the edge is drawn whole and in full
    let f = styled(VisualStyle::Wireframe);
    let (row, face) = row_and_face(&f);
    assert_eq!(face, [255, 255, 255]);
    assert!(row.iter().all(dark), "{row:?}");
    assert_eq!(f.pick(100.0, 40.0, 0, |_| true), NO_ID, "wireframe faces aren't drawn");
    // wireframe, visible edges only: the face is unseen but still hides the edge
    let (row, face) = row_and_face(&styled(VisualStyle::WireframeVisibleEdges));
    assert_eq!(face, [255, 255, 255]);
    assert!(row.iter().all(|c| *c == [255, 255, 255]), "{row:?}");
    // wireframe, hidden edges: the hidden edge is faint and dashed
    let (row, _) = row_and_face(&styled(VisualStyle::WireframeHiddenEdges));
    assert!(row.iter().any(faint) && row.contains(&[255, 255, 255]), "{row:?}");
    assert!(row.iter().all(|c| !dark(c)));
  }

  /// A closed cylinder standing on the XY plane: side and caps as separate faces (ids 1, 2, 3)
  /// with their own copies of the rim points, as a B-rep tessellation has them.
  fn cylinder(r: f32, h: f32, seg: u32) -> Mesh {
    let mut m = Mesh { color: [200, 0, 0, 255], analyze: true, ..Default::default() };
    let ring = |k: u32| {
      let a = std::f32::consts::TAU * (k % seg) as f32 / seg as f32;
      (a.cos(), a.sin())
    };
    for k in 0..seg {
      let (c, s) = ring(k);
      m.positions.extend_from_slice(&[[r * c, r * s, 0.0], [r * c, r * s, h]]);
      m.normals.extend_from_slice(&[[c, s, 0.0], [c, s, 0.0]]);
    }
    for k in 0..seg {
      let (i, j) = (k * 2, (k + 1) % seg * 2);
      m.indices.extend_from_slice(&[i, j, i + 1, i + 1, j, j + 1]);
      m.tri_ids.extend_from_slice(&[1, 1]);
    }
    for (z, nz, id) in [(0.0, -1.0, 2), (h, 1.0, 3)] {
      let c0 = m.positions.len() as u32;
      m.positions.push([0.0, 0.0, z]);
      m.normals.push([0.0, 0.0, nz]);
      for k in 0..seg {
        let (c, s) = ring(k);
        m.positions.push([r * c, r * s, z]);
        m.normals.push([0.0, 0.0, nz]);
      }
      for k in 0..seg {
        m.indices.extend_from_slice(&[c0, c0 + 1 + k, c0 + 1 + (k + 1) % seg]);
        m.tri_ids.push(id);
      }
    }
    m
  }

  #[test]
  fn fold_edges_and_silhouettes_of_a_cylinder() {
    let m = cylinder(10.0, 20.0, 32);
    let folds = fold_edges(&m);
    // the side's 32 generators and the 64 rim edges; the caps' spokes lie in their planes
    assert_eq!(folds.len(), 96);
    assert!(folds.iter().all(|e| e.ids.contains(&1)));
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    cam.target = [0.0, 0.0, 10.0];
    for ortho in [true, false] {
      cam.ortho = ortho;
      let sil: Vec<(FoldEdge, u32)> =
        folds.iter().filter_map(|e| e.silhouette(Eye::of(&cam)).map(|id| (*e, id))).collect();
      // the two outermost generators, picking as the side
      let generators: Vec<_> = sil.iter().filter(|(e, _)| e.a[2] != e.b[2]).collect();
      assert_eq!(generators.len(), 2, "ortho {ortho}");
      assert!(generators.iter().all(|(e, id)| e.a[0].abs() > 9.9 && *id == 1));
      if ortho {
        // edge-on, both caps face away: their front rims are outline
        assert!(sil.iter().filter(|(e, _)| e.a[2] == e.b[2]).all(|(e, id)| e.a[1] <= 0.0 && *id == 1));
      }
    }
    let scene = Scene { meshes: vec![m.clone()], style: VisualStyle::Wireframe, ..Default::default() };
    let lines = silhouette_lines(&scene, &cam, 100);
    assert!(lines.iter().all(|l| l.points.len() == 2 && l.color == SILHOUETTE_COLOR && is_edge(l)));
    // nudged off the side, outwards
    let mut sides = lines.iter().filter(|l| (l.points[0][2] - l.points[1][2]).abs() > 1.0);
    assert!(sides.clone().count() == 2 && sides.all(|l| l.points[0][0].abs() > 10.0), "{lines:?}");
    assert!(silhouette_lines(&Scene { meshes: vec![Mesh { analyze: false, ..m }], ..scene }, &cam, 100).is_empty());
  }

  #[test]
  fn line_drawings_outline_curved_faces_and_dash_what_is_hidden() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    cam.target = [0.0, 0.0, 10.0];
    cam.ortho = true;
    cam.distance = 60.0;
    let draw = |style| {
      let scene = Scene {
        meshes: vec![cylinder(10.0, 20.0, 48)],
        background_top: [200; 4],
        background_bottom: [200; 4],
        style,
        ..Default::default()
      };
      render(&scene, &cam, 200, 200)
    };
    // the cylinder spans x ±10 mm: its outline is where the side's silhouette is drawn
    let ink = |f: &Frame, x: u32| (60..140).filter(|&y| f.rgba[((y * 200 + x) * 4) as usize] < 120).count();
    let outline = |f: &Frame| {
      let (l, r) = cam
        .project(DVec3::new(-10.0, 0.0, 10.0), 200, 200)
        .zip(cam.project(DVec3::new(10.0, 0.0, 10.0), 200, 200))
        .unwrap();
      let near = |x: f64| ((x - 1.0) as u32..=(x + 1.0) as u32).map(|x| ink(f, x)).max().unwrap_or(0);
      (near(l.0), near(r.0))
    };
    for style in
      [VisualStyle::Wireframe, VisualStyle::WireframeVisibleEdges, VisualStyle::HiddenLine, VisualStyle::VisibleLine]
    {
      let f = draw(style);
      let (l, r) = outline(&f);
      assert!(l > 60 && r > 60, "{style:?}: the outline is drawn ({l}, {r})");
    }
    // Shaded leaves the outline to the shading; Shaded with Visible Edges Only draws it dark
    let f = draw(VisualStyle::Shaded);
    assert!(f.rgba[((100 * 200 + 100) * 4) as usize] > 120, "red, not ink, in the middle");
    let (l, r) = outline(&f);
    assert!(l < 60 && r < 60, "Shaded: no outline ({l}, {r})");
    let f = draw(VisualStyle::ShadedVisibleEdges);
    assert!(f.rgba[((100 * 200 + 100) * 4) as usize] > 120, "red, not ink, in the middle");
    let (l, r) = outline(&f);
    assert!(l > 60 && r > 60, "Shaded with Visible Edges Only: the outline is drawn ({l}, {r})");
    // line drawings: plain white surfaces that pick as the face under them
    for style in [VisualStyle::HiddenLine, VisualStyle::VisibleLine] {
      let f = draw(style);
      assert_eq!(&f.rgba[(100 * 200 + 100) * 4..(100 * 200 + 100) * 4 + 3], &[255, 255, 255], "{style:?}");
      assert_eq!(f.pick(100.0, 100.0, 0, |_| true), 1);
    }
  }

  /// Fusion's dash patterns: 9 on, 3 off for construction; 9 on, 3 off, 6 on, 3 off for a
  /// centerline.
  #[test]
  fn dash_patterns_follow_fusion() {
    let row =
      |dash_dot: bool| (0..24).map(|x| if dash_gap(x as f64, dash_dot) { ' ' } else { '#' }).collect::<String>();
    assert_eq!(row(false), "#########   #########   ");
    assert_eq!(row(true), "#########   ######   ###");
    assert!(!dash_gap(-1.0, false));
  }

  #[test]
  fn hidden_line_dashes_hidden_edges_firmly() {
    let white = |c: &[u8; 3]| *c == [255, 255, 255];
    // the face is paper, the edge under it shows dashed
    let f = styled(VisualStyle::HiddenLine);
    let (row, face) = row_and_face(&f);
    assert_eq!(face, [255, 255, 255]);
    assert!(row.iter().any(white) && row.iter().any(|c| !white(c)), "dashed: {row:?}");
    // firmer than the faint hidden edges of the other styles
    let darkest = |row: &[[u8; 3]]| row.iter().map(|c| c[0]).min().unwrap_or(255);
    let (faint, _) = row_and_face(&styled(VisualStyle::WireframeHiddenEdges));
    assert!(darkest(&row) + 40 < darkest(&faint), "{} vs {}", darkest(&row), darkest(&faint));
    assert_eq!(f.pick(100.0, 40.0, 0, |_| true), 1, "the face picks");
    // visible lines only: nothing of the edge shows
    let (row, face) = row_and_face(&styled(VisualStyle::VisibleLine));
    assert!(white(&face) && row.iter().all(white), "{row:?}");
  }

  #[test]
  fn the_style_flags_match_the_menu() {
    for s in VisualStyle::ALL {
      assert_eq!(s.label().starts_with("Shaded"), s.shaded());
      assert_eq!(s.label().contains("Hidden"), s.hidden_edges());
    }
    assert!(VisualStyle::Shaded.edges() && !VisualStyle::Wireframe.occludes());
    assert_eq!(VisualStyle::default(), VisualStyle::Shaded);
    for s in VisualStyle::ALL {
      assert_eq!(VisualStyle::parse(s.name()), Some(s));
      assert_eq!(
        s.silhouettes(),
        !s.shaded() || matches!(s, VisualStyle::ShadedVisibleEdges | VisualStyle::ShadedHiddenEdges)
      );
      assert!(!(s.shaded() && s.line_drawing()));
    }
    assert_eq!(VisualStyle::parse("Hidden_Line"), Some(VisualStyle::HiddenLine));
    assert_eq!(VisualStyle::parse("sketchy"), None);
  }

  #[test]
  fn free_orbit_rolls_and_goes_over_the_top() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    // straight up the screen past the top: constrained orbit would stop at the pole
    for _ in 0..40 {
      cam.free_orbit(0.0, 0.05);
    }
    let (right, up, back) = cam.basis();
    assert!(back.z > 0.4 && up.z < 0.0, "over the top, upside down: {back} {up}");
    assert!((right.length() - 1.0).abs() < 1e-9 && right.dot(up).abs() < 1e-9 && right.dot(back).abs() < 1e-9);
    // a diagonal drag leaves the view rolled
    let mut cam = Camera::default();
    cam.free_orbit(0.6, 0.6);
    cam.free_orbit(-0.6, -0.6);
    assert!(cam.roll.abs() > 1e-3, "rolled: {}", cam.roll);
    // set_basis takes back what basis gives
    let (r, _, b) = cam.basis();
    let mut again = Camera { roll: 0.0, ..cam };
    again.set_basis(r, b);
    let (r2, _, b2) = again.basis();
    assert!((r - r2).length() < 1e-9 && (b - b2).length() < 1e-9);
    // a standard view takes the roll off
    cam.set_view(StandardView::Home);
    assert_eq!(cam.roll, 0.0);
  }

  #[test]
  fn effects_put_a_ground_under_the_bodies() {
    let mut body = quad(5.0, [255, 0, 0, 255], 1);
    body.analyze = true;
    body.positions.push([0.0, 0.0, 2.0]);
    let profile = quad(-50.0, [0, 0, 255, 255], 2);
    let base = Scene { meshes: vec![body, profile], ..Default::default() };
    let mut scene = base.clone();
    Effects::default().apply(&mut scene);
    assert_eq!(scene.meshes.len(), 2, "no effects, nothing added");

    let mut scene = base.clone();
    Effects { ground_shadow: true, reflections: true, ground_plane: true, ..Default::default() }.apply(&mut scene);
    let added = &scene.meshes[2..];
    assert_eq!(added.len(), 3, "reflection, ground, shadow");
    let (refl, ground, shadow) = (&added[0], &added[1], &added[2]);
    // the lowest body point (z = 2) is the ground: the profile at z = -50 doesn't count
    assert!(ground.positions.iter().all(|p| (p[2] - 2.0).abs() < 0.1) && ground.transparent());
    assert!(refl.positions.iter().any(|p| p[2] < 0.0) && !refl.transparent() && refl.id == NO_ID);
    assert!(shadow.transparent() && shadow.positions.iter().all(|p| p[2] > ground.positions[0][2]));
    // the shadow is the square's outline: the inner point isn't on it
    assert_eq!(shadow.positions.len(), 4);
    assert_eq!(shadow.indices.len(), 6);
    assert!(added.iter().all(|m| !m.analyze), "the effects aren't body surfaces");

    // the shaded image shows the ground's tint beside the body
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    let mut plain = Scene { background_top: [255; 4], background_bottom: [255; 4], ..base.clone() };
    plain.meshes.truncate(1);
    let mut grounded = plain.clone();
    Effects { ground_plane: true, ..Default::default() }.apply(&mut grounded);
    let at = |f: &Frame| f.rgba[(50 * 200 + 60) * 4];
    assert_eq!(at(&render(&plain, &cam, 200, 100)), 255);
    assert!(at(&render(&grounded, &cam, 200, 100)) < 250);
  }

  #[test]
  fn project_and_ray_round_trip() {
    let cam = Camera::default();
    let p = DVec3::new(10.0, -5.0, 3.0);
    let (x, y, _) = cam.project(p, 800, 600).unwrap();
    let (o, d) = cam.ray(x, y, 800, 600);
    let dist = (p - o).cross(d).length();
    assert!(dist < 1e-6, "{dist}");
    let hit = ray_plane(o, d, DVec3::new(0.0, 0.0, 3.0), DVec3::Z).unwrap();
    assert!((hit - p).length() < 1e-6);
  }

  #[test]
  fn camera_behind_geometry_and_hostile_sizes() {
    // inside the quad's extent: near-plane clipping kicks in
    let cam = Camera { distance: 1.0, pitch: 0.0, ..Camera::default() };
    let scene = Scene { meshes: vec![quad(0.0, [255, 0, 0, 255], 1)], ..Default::default() };
    let _ = render(&scene, &cam, 64, 64);
    let f = render(&scene, &cam, 0, u32::MAX);
    assert!(f.width >= 1 && f.height <= MAX_DIM);
  }

  #[test]
  fn standard_views_look_along_axes() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    assert!((cam.eye_dir() - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
    cam.set_view(StandardView::Right);
    assert!((cam.eye_dir() - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
    cam.set_view(StandardView::Top);
    assert!(cam.eye_dir().z > 0.999);
  }

  #[test]
  fn look_from_any_direction_with_any_up() {
    let mut cam = Camera::default();
    let corner = DVec3::new(-1.0, 1.0, -1.0).normalize();
    cam.look_from(corner, DVec3::Z);
    let (right, up, back) = cam.basis();
    assert!((back - corner).length() < 1e-9, "{back}");
    assert!(up.z > 0.0 && right.z.abs() < 1e-9 && cam.roll == 0.0, "Z stays up: {up}");
    // the front view turned a quarter: +X at the top of the screen
    cam.look_from(-DVec3::Y, DVec3::X);
    let (right, up, back) = cam.basis();
    assert!((back + DVec3::Y).length() < 1e-9 && (up - DVec3::X).length() < 1e-9, "{up}");
    assert!((right + DVec3::Z).length() < 1e-9, "{right}");
    // straight down the turn goes into the yaw
    cam.look_from(DVec3::Z, -DVec3::X);
    let (_, up, _) = cam.basis();
    assert!((up + DVec3::X).length() < 1e-6 && cam.roll == 0.0, "{up} {}", cam.roll);
    let home = Camera::default();
    cam.set_view(StandardView::Home);
    assert!((cam.eye_dir() - home.eye_dir()).length() < 1e-3);
  }

  /// Shaded with Visible Edges Only draws the plain (light grey) body edges dark; hovered and
  /// selected edges, and every other style, keep their colours.
  #[test]
  fn shaded_with_visible_edges_only_darkens_the_edges() {
    let plain = [179, 178, 173, 255];
    let edge = |color: [u8; 4]| Polyline {
      points: vec![[0.0; 3], [1.0, 0.0, 0.0]],
      color,
      width: 1.0,
      id: 5,
      on_top: false,
      dashed: false,
      dash_dot: false,
      behind: false,
    };
    let lines = vec![edge(plain), edge([10, 131, 252, 255]), Polyline { id: NO_ID, ..edge(plain) }];
    let mut s = Scene { lines: lines.clone(), style: VisualStyle::ShadedVisibleEdges, ..Default::default() };
    s.restyle_edges(plain);
    assert_eq!(s.lines[0].color, SILHOUETTE_COLOR, "a plain edge turns dark");
    assert_eq!(s.lines[1].color, [10, 131, 252, 255], "a selected edge keeps its blue");
    assert_eq!(s.lines[2].color, plain, "the grid and other unpickable lines stay");
    let mut s = Scene { lines: lines.clone(), style: VisualStyle::ShadedHiddenEdges, ..Default::default() };
    s.restyle_edges(plain);
    assert_eq!(s.lines[0].color, SILHOUETTE_COLOR, "Shaded with Hidden Edges darkens them too");
    let mut s = Scene { lines, style: VisualStyle::Shaded, ..Default::default() };
    s.restyle_edges(plain);
    assert_eq!(s.lines[0].color, plain, "Shaded keeps them light grey");
  }

  /// Shaded with Hidden Edges, as the app dresses it (the light grey body edges restyled): the
  /// edge behind a body face shows through it as faint dashes that stand out from the shading,
  /// where Shaded hides it.
  #[test]
  fn shaded_with_hidden_edges_shows_the_hidden_edges_through_the_body() {
    let plain = [179, 178, 173, 255];
    let draw = |style: VisualStyle| {
      let mut cam = Camera::default();
      cam.set_view(StandardView::Top);
      cam.distance = 100.0;
      let mut face = quad(5.0, [205, 200, 190, 255], 1);
      face.analyze = true;
      let edge = Polyline {
        points: vec![[-10.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
        color: plain,
        width: 1.0,
        id: 3,
        on_top: false,
        dashed: false,
        dash_dot: false,
        behind: false,
      };
      let mut scene = Scene {
        meshes: vec![face],
        lines: vec![edge],
        background_top: [255; 4],
        background_bottom: [255; 4],
        style,
        ..Default::default()
      };
      scene.restyle_edges(plain);
      render(&scene, &cam, 200, 100)
    };
    let sum = |c: &[u8; 3]| c.iter().map(|&v| v as i32).sum::<i32>();
    let (hidden, _) = row_and_face(&draw(VisualStyle::ShadedHiddenEdges));
    let (shaded, _) = row_and_face(&draw(VisualStyle::Shaded));
    // Shaded: only the body's smooth shading along the edge's row
    let (lo, hi) = (shaded.iter().map(sum).min().unwrap(), shaded.iter().map(sum).max().unwrap());
    assert!(hi - lo < 12, "Shaded hides it: {shaded:?}");
    // the same pixels in Shaded with Hidden Edges: dark dashes with the shading between them
    let ink: Vec<i32> = shaded.iter().zip(&hidden).map(|(a, b)| sum(a) - sum(b)).collect();
    assert!(ink.iter().any(|&d| d > 120), "the hidden edge stands out: {hidden:?} over {shaded:?}");
    assert!(ink.iter().any(|&d| d.abs() < 6), "dashed: {hidden:?}");
  }

  /// Two boxes, one standing on a wide slab, from the side and above.
  fn slab_and_block() -> (Scene, Camera) {
    let cuboid = |lo: [f32; 3], hi: [f32; 3], id: u32| {
      let mut m = Mesh { color: [200, 200, 200, 255], analyze: true, id, ..Default::default() };
      let faces: [([usize; 3], f32, [f32; 3]); 6] = [
        ([0, 1, 2], lo[2], [0.0, 0.0, -1.0]),
        ([0, 1, 2], hi[2], [0.0, 0.0, 1.0]),
        ([0, 2, 1], lo[1], [0.0, -1.0, 0.0]),
        ([0, 2, 1], hi[1], [0.0, 1.0, 0.0]),
        ([1, 2, 0], lo[0], [-1.0, 0.0, 0.0]),
        ([1, 2, 0], hi[0], [1.0, 0.0, 0.0]),
      ];
      for (axes, at, n) in faces {
        let [a, b, c] = axes;
        let base = m.positions.len() as u32;
        for (u, v) in [(lo[a], lo[b]), (hi[a], lo[b]), (hi[a], hi[b]), (lo[a], hi[b])] {
          let mut p = [0.0; 3];
          p[a] = u;
          p[b] = v;
          p[c] = at;
          m.positions.push(p);
          m.normals.push(n);
        }
        m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
      }
      m
    };
    let scene = Scene {
      meshes: vec![cuboid([-40.0, -40.0, 0.0], [40.0, 40.0, 5.0], 1), cuboid([-8.0, -8.0, 5.0], [8.0, 8.0, 30.0], 2)],
      background_top: [255; 4],
      background_bottom: [255; 4],
      ..Default::default()
    };
    let cam = Camera { distance: 160.0, target: [0.0, 0.0, 10.0], ..Camera::default() };
    (scene, cam)
  }

  fn brightness(f: &Frame) -> u64 {
    f.rgba.chunks_exact(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum()
  }

  /// Object Shadow darkens the slab where the block stands in the light's way; Ambient
  /// Occlusion darkens the crease where the block meets the slab and leaves flat faces alone;
  /// Anti-Aliasing softens the edges; none of them changes what picks.
  #[test]
  fn object_shadow_ambient_occlusion_and_anti_aliasing_change_the_picture() {
    let (plain, cam) = slab_and_block();
    let base = render(&plain, &cam, 240, 180);
    let with = |post: Post| render(&Scene { post, ..plain.clone() }, &cam, 240, 180);
    let shadow = with(Post { object_shadow: true, ..Default::default() });
    assert!(brightness(&shadow) < brightness(&base), "a shadow on the slab");
    assert_eq!(shadow.ids, base.ids);
    let map = ShadowMap::build(&plain.meshes).expect("bodies");
    // the slab's top just beside the block, away from the light, is shadowed; far off it isn't
    let lean = Vec3::new(SHADOW_LEAN[0], SHADOW_LEAN[1], 0.0).normalize();
    assert!(map.occlusion(Vec3::new(0.0, 0.0, 5.0) + lean * 12.0 + Vec3::Z * 0.05) > 0.5);
    assert_eq!(map.occlusion(Vec3::new(-35.0, 35.0, 5.05)), 0.0);
    let ao = with(Post { ambient_occlusion: true, ..Default::default() });
    assert!(brightness(&ao) < brightness(&base), "the crease darkens");
    assert_eq!(ao.ids, base.ids);
    // a flat face alone takes none
    let flat = Scene { meshes: vec![plain.meshes[0].clone()], ..plain.clone() };
    let a = render(&flat, &cam, 240, 180);
    let b =
      render(&Scene { post: Post { ambient_occlusion: true, ..Default::default() }, ..flat.clone() }, &cam, 240, 180);
    let changed = a.rgba.iter().zip(&b.rgba).filter(|(x, y)| x.abs_diff(**y) > 2).count();
    assert!(changed < a.rgba.len() / 200, "{changed} channels changed on a flat face");
    let aa = with(Post { anti_aliasing: true, ..Default::default() });
    assert_ne!(aa.rgba, base.rgba, "edges are smoothed");
    assert_eq!(aa.ids, base.ids);
  }

  /// Environment Dome replaces the background with the environment's sky over its ground,
  /// turning with the camera: looking up shows sky, looking down at the horizon ground.
  #[test]
  fn the_environment_dome_turns_with_the_camera() {
    let dome = Dome { zenith: [40, 90, 200, 255], horizon: [220, 225, 230, 255], ground: [90, 80, 70, 255] };
    assert_eq!(dome.color(Vec3::Z), dome.zenith);
    assert_eq!(dome.color(Vec3::X), dome.horizon);
    assert_eq!(dome.color(-Vec3::Z), dome.ground);
    let scene = Scene { post: Post { dome: Some(dome), ..Default::default() }, ..Default::default() };
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    let f = render(&scene, &cam, 100, 100);
    let px = |f: &Frame, x: u32, y: u32| {
      let i = ((y * f.width + x) * 4) as usize;
      [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    // seen from the front: sky in the top row, ground in the bottom one
    assert!(px(&f, 50, 0)[2] > px(&f, 50, 0)[0], "sky above: {:?}", px(&f, 50, 0));
    assert!(px(&f, 50, 99)[0] > px(&f, 50, 99)[2], "ground below: {:?}", px(&f, 50, 99));
    cam.set_view(StandardView::Top);
    let f = render(&scene, &cam, 100, 100);
    assert!(px(&f, 50, 50)[0] > px(&f, 50, 50)[2], "looking straight down: ground");
  }

  /// Ground Plane Offset: with Adaptive off, the ground and its shadow sit the offset below the
  /// bodies' lowest point.
  #[test]
  fn the_ground_follows_the_offset() {
    let (plain, _) = slab_and_block();
    let effects = Effects { ground_plane: true, ground_shadow: true, ..Default::default() };
    let dome = Dome { zenith: [0; 4], horizon: [0; 4], ground: [0; 4] };
    let ground_z = |offset: Option<f32>| {
      let mut s = plain.clone();
      effects.apply_with(&mut s, offset, dome);
      s.meshes[2..].iter().flat_map(|m| m.positions.iter().map(|p| p[2])).fold(f32::NEG_INFINITY, f32::max)
    };
    assert!(ground_z(None).abs() < 0.5, "adaptive: at the lowest point");
    assert!((ground_z(Some(20.0)) + 20.0).abs() < 0.5, "20 below it");
    let mut s = plain.clone();
    Effects { object_shadow: true, environment_dome: true, ..effects }.apply_with(&mut s, None, dome);
    assert_eq!(s.post, Post { dome: Some(dome), object_shadow: true, ..Default::default() });
  }
}
