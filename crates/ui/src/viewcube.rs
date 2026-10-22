//! The ViewCube in the viewport's top-right corner, as on Fusion's: faces, edges and corners to
//! click for a view, a drag to orbit, the rotate and adjacent-face arrows of a face view, the
//! compass ring, the Home button and the cube's menu. It fades while the pointer is away.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use fox_engine::kernel::glam::{DMat3, DVec3};
use fox_engine::render::{Camera, StandardView};

use crate::App;

/// A hot spot on the cube, named by the direction from its centre in cube axes: one non-zero
/// component for a face, two for an edge, three for a corner.
pub(crate) type Spot = [i8; 3];

/// The part of a face's half-width that is its centre; the band outside it is edges and corners.
const FACE_CORE: f64 = 0.6;
/// Screen pixels per cube unit (the cube spans -1..1) at the Large size.
const SCALE: f32 = 31.0;

/// ViewCube menu ▸ ViewCube Size, as Fusion's (a preference).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CubeSize {
  /// Sized by the canvas: Tiny in a small window up to Large in a big one.
  #[default]
  Automatic,
  Tiny,
  Small,
  Normal,
  Large,
}

impl CubeSize {
  pub const ALL: [CubeSize; 5] =
    [CubeSize::Automatic, CubeSize::Tiny, CubeSize::Small, CubeSize::Normal, CubeSize::Large];

  pub fn key(self) -> &'static str {
    match self {
      CubeSize::Automatic => "automatic",
      CubeSize::Tiny => "tiny",
      CubeSize::Small => "small",
      CubeSize::Normal => "normal",
      CubeSize::Large => "large",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      CubeSize::Automatic => "Automatic",
      CubeSize::Tiny => "Tiny",
      CubeSize::Small => "Small",
      CubeSize::Normal => "Normal",
      CubeSize::Large => "Large",
    }
  }

  /// The cube's scale relative to Large, for a canvas `height` points tall.
  pub fn factor(self, height: f32) -> f32 {
    match self {
      CubeSize::Automatic => {
        if height < 480.0 {
          CubeSize::Tiny
        } else if height < 720.0 {
          CubeSize::Small
        } else if height < 1500.0 {
          CubeSize::Normal
        } else {
          CubeSize::Large
        }
      }
      .factor(height),
      CubeSize::Tiny => 0.5,
      CubeSize::Small => 0.65,
      CubeSize::Normal => 0.8,
      CubeSize::Large => 1.0,
    }
  }
}
/// Radius of the compass ring, in cube units, round the cube's bottom.
const RING: f64 = 1.55;
/// Opacity of the cube while the pointer is away from it.
const IDLE_OPACITY: f32 = 0.45;

/// The camera's projection, as the cube's menu offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Projection {
  Orthographic,
  Perspective,
  /// Perspective, but orthographic while looking straight at a face.
  OrthoFaces,
}

/// What the cube remembers between frames.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CubeState {
  /// Cube axes to world: the columns are where the cube's right (X), back (Y) and top (Z) point.
  /// Set current view as Front / Top turns it.
  pub frame: DMat3,
  /// Set current view as Home: the camera, and whether going home fits the model (else it
  /// keeps the distance). None is the standard home: the front-right-top corner, fitted.
  pub home: Option<(Camera, bool)>,
  /// Perspective with Ortho Faces is on.
  pub ortho_faces: bool,
}

impl Default for CubeState {
  fn default() -> Self {
    CubeState { frame: DMat3::IDENTITY, home: None, ortho_faces: false }
  }
}

impl CubeState {
  pub fn projection(&self, cam: &Camera) -> Projection {
    if self.ortho_faces {
      Projection::OrthoFaces
    } else if cam.ortho {
      Projection::Orthographic
    } else {
      Projection::Perspective
    }
  }

  /// A world direction in cube axes.
  fn in_cube(&self, v: DVec3) -> DVec3 {
    self.frame.transpose() * v
  }

  /// The world (eye direction, screen up) of a hot spot's view. Edges and corners keep the
  /// cube's top up; the top and bottom faces keep the front face down and up the screen.
  pub fn spot_view(&self, s: Spot) -> (DVec3, DVec3) {
    let d = spot_vec(s).normalize();
    let up = match s {
      [0, 0, z] => DVec3::new(0.0, z as f64, 0.0),
      _ => DVec3::Z,
    };
    (self.frame * d, self.frame * up)
  }

  /// The world (eye direction, screen up) of a standard view.
  pub fn standard_view(&self, v: StandardView) -> (DVec3, DVec3) {
    let (d, up) = v.direction();
    (self.frame * d, self.frame * up)
  }

  /// The face the camera looks straight at, if it does.
  pub fn face_view(&self, cam: &Camera) -> Option<Spot> {
    let d = self.in_cube(cam.eye_dir());
    let s = snap(d);
    (spot_vec(s).dot(d) > 0.9999).then_some(s)
  }

  /// `v` moved onto the nearest of the cube's axes (in world).
  fn snap_world(&self, v: DVec3) -> DVec3 {
    self.frame * spot_vec(snap(self.in_cube(v)))
  }

  /// From a face view, the view one face over: the face at the top, bottom, right or left of
  /// the screen (`dx`, `dy` with y up) turned to the front.
  pub fn adjacent(&self, cam: &Camera, dx: i32, dy: i32) -> (DVec3, DVec3) {
    let (right, up, back) = cam.basis();
    let (dir, up) = match (dx, dy) {
      (0, 1) => (up, -back),
      (0, _) => (-up, back),
      (1, _) => (right, up),
      _ => (-right, up),
    };
    (self.snap_world(dir), self.snap_world(up))
  }

  /// The face view turned a quarter about the view axis: counter-clockwise on screen when
  /// `ccw`.
  pub fn rolled(&self, cam: &Camera, ccw: bool) -> (DVec3, DVec3) {
    let (right, _, back) = cam.basis();
    (self.snap_world(back), self.snap_world(if ccw { right } else { -right }))
  }

  /// Set current view as Front (`top` false) or Top: the cube turns so that face is the one
  /// looked at, with the screen's up kept as the face's up.
  pub fn set_current_as(&mut self, cam: &Camera, top: bool) {
    let (_, up, back) = cam.basis();
    let (x, y, z) = if top { (up.cross(back), up, back) } else { ((-back).cross(up), -back, up) };
    self.frame = DMat3::from_cols(x, y, z);
  }
}

fn spot_vec(s: Spot) -> DVec3 {
  DVec3::new(s[0] as f64, s[1] as f64, s[2] as f64)
}

/// The face whose normal is nearest `v`.
fn snap(v: DVec3) -> Spot {
  let a = v.abs();
  let i = if a.x >= a.y && a.x >= a.z {
    0
  } else if a.y >= a.z {
    1
  } else {
    2
  };
  let mut s = [0; 3];
  s[i] = if v[i] < 0.0 { -1 } else { 1 };
  s
}

fn unit(i: usize) -> DVec3 {
  let mut v = DVec3::ZERO;
  v[i] = 1.0;
  v
}

/// The camera's right, up and back in cube axes, for drawing and hit-testing.
#[derive(Clone, Copy)]
struct Basis {
  right: DVec3,
  up: DVec3,
  back: DVec3,
  /// Screen pixels per cube unit (the ViewCube Size).
  scale: f32,
}

impl Basis {
  fn new(state: &CubeState, cam: &Camera) -> Self {
    let (right, up, back) = cam.basis();
    Basis { right: state.in_cube(right), up: state.in_cube(up), back: state.in_cube(back), scale: SCALE }
  }

  /// Screen offset from the cube's centre of a point in cube axes (y down).
  fn proj(&self, v: DVec3) -> Vec2 {
    Vec2::new(v.dot(self.right) as f32, -v.dot(self.up) as f32) * self.scale
  }

  /// The faces turned towards the viewer: (axis, sign, how squarely it faces).
  fn faces(&self) -> Vec<(usize, i8, f64)> {
    let mut out = Vec::new();
    for i in 0..3 {
      for s in [1i8, -1] {
        let facing = self.back[i] * s as f64;
        if facing > 0.02 {
          out.push((i, s, facing));
        }
      }
    }
    out.sort_by(|a, b| a.2.total_cmp(&b.2));
    out
  }

  /// Where offset `p` lands on face (`i`, `s`) in its own axes (each -1..1 on the face).
  fn on_face(&self, p: Vec2, i: usize, s: i8) -> Option<(f64, f64)> {
    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
    let c = self.proj(unit(i) * s as f64);
    let (a, b) = (self.proj(unit(j)), self.proj(unit(k)));
    let det = (a.x * b.y - a.y * b.x) as f64;
    if det.abs() < 1e-3 {
      return None;
    }
    let q = p - c;
    let alpha = (q.x * b.y - q.y * b.x) as f64 / det;
    let beta = (a.x * q.y - a.y * q.x) as f64 / det;
    (alpha.abs() <= 1.0 + 1e-6 && beta.abs() <= 1.0 + 1e-6).then_some((alpha, beta))
  }

  /// The hot spot under offset `p`.
  fn spot_at(&self, p: Vec2) -> Option<Spot> {
    let band = |t: f64| {
      if t > FACE_CORE {
        1
      } else if t < -FACE_CORE {
        -1
      } else {
        0
      }
    };
    self.faces().into_iter().rev().find_map(|(i, s, _)| {
      let (alpha, beta) = self.on_face(p, i, s)?;
      let mut spot = [0; 3];
      spot[i] = s;
      spot[(i + 1) % 3] = band(alpha);
      spot[(i + 2) % 3] = band(beta);
      Some(spot)
    })
  }

  /// The outline of the part of face (`i`, `s`) that belongs to `spot`, if any does.
  fn spot_patch(&self, spot: Spot, i: usize, s: i8) -> Option<[Vec2; 4]> {
    if spot[i] != s {
      return None;
    }
    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
    let range = |c: i8| match c {
      0 => (-FACE_CORE, FACE_CORE),
      1 => (FACE_CORE, 1.0),
      _ => (-1.0, -FACE_CORE),
    };
    let ((a0, a1), (b0, b1)) = (range(spot[j]), range(spot[k]));
    let n = unit(i) * s as f64;
    let at = |a: f64, b: f64| self.proj(n + unit(j) * a + unit(k) * b);
    Some([at(a0, b0), at(a1, b0), at(a1, b1), at(a0, b1)])
  }

  /// The compass ring round the cube's bottom.
  fn ring(&self) -> Vec<Vec2> {
    (0..48)
      .map(|n| {
        let t = n as f64 / 48.0 * std::f64::consts::TAU;
        self.proj(DVec3::new(RING * t.cos(), RING * t.sin(), -1.0))
      })
      .collect()
  }

  /// Whether offset `p` is on the compass ring and not on the cube.
  fn on_ring(&self, p: Vec2) -> bool {
    if self.spot_at(p).is_some() {
      return false;
    }
    let ring = self.ring();
    (0..ring.len()).any(|n| {
      let (a, b) = (ring[n], ring[(n + 1) % ring.len()]);
      let ab = b - a;
      let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
      (a + ab * t - p).length() < 6.0
    })
  }
}

/// Fusion's face names, and the direction along which each label reads (cube axes).
fn face_label(i: usize, s: i8) -> (&'static str, DVec3) {
  match (i, s) {
    (2, 1) => ("TOP", DVec3::X),
    (2, _) => ("BOTTOM", DVec3::X),
    (1, -1) => ("FRONT", DVec3::X),
    (1, _) => ("BACK", -DVec3::X),
    (0, 1) => ("RIGHT", DVec3::Y),
    _ => ("LEFT", -DVec3::Y),
  }
}

/// What a press on the cube widget started dragging.
#[derive(Clone, Copy, PartialEq)]
enum Grab {
  Cube,
  Ring,
}

/// Keeps Perspective with Ortho Faces in step: orthographic exactly while a face is looked at
/// head-on and the camera is still.
pub(crate) fn keep_projection(app: &mut App) {
  if app.cube.ortho_faces && app.cam_move.is_none() {
    app.cam.ortho = app.cube.face_view(&app.cam).is_some();
  }
}

/// Draws the cube and handles it. Returns true when it took the click or is being dragged.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect) -> bool {
  // the ViewCube Size scales the cube and its buttons together
  let k = app.preferences.values.cube_size.factor(rect.height());
  let scale = SCALE * k;
  let center = rect.right_top() + Vec2::new(-92.0, 80.0) * k;
  let at = |v: Vec2| center + v;
  let area = Rect::from_center_size(center, Vec2::splat(124.0 * k));
  let home = Rect::from_center_size(center + Vec2::new(-62.0, -50.0) * k, Vec2::splat(18.0));
  let menu = Rect::from_center_size(center + Vec2::new(46.0, 64.0) * k, Vec2::splat(16.0));
  let id = ui.id().with("viewcube");
  // the cube with its buttons is a widget like any other, so a dialog, menu or panel drawn over
  // it takes the pointer instead; the buttons come after the cube so they're on top of it
  let resp = ui.interact(area, id, Sense::click_and_drag());
  let home_resp = ui.interact(home, id.with("home"), Sense::click());
  let menu_resp = ui.interact(menu, id.with("menu"), Sense::click());
  let moving = app.cam_move.is_some();

  let mut b = Basis::new(&app.cube, &app.cam);
  b.scale = scale;
  let pointer = if resp.dragged() { None } else { resp.hover_pos() };
  let off = pointer.map(|p| p - center);
  let face = if moving { None } else { app.cube.face_view(&app.cam) };
  let arrows = face.map(|_| face_arrows(center, scale)).unwrap_or_default();
  let rolls = face.map(|_| roll_arrows(center, scale)).unwrap_or_default();
  let arrow_hit = pointer.and_then(|p| arrows.iter().position(|(r, ..)| r.contains(p)));
  let roll_hit = pointer.and_then(|p| rolls.iter().position(|(r, ..)| r.contains(p)));
  let over_arrow = arrow_hit.is_some() || roll_hit.is_some();
  let spot = if over_arrow { None } else { off.and_then(|o| b.spot_at(o)) };
  let ring_hot = !over_arrow && spot.is_none() && off.is_some_and(|o| b.on_ring(o));

  // dragging the cube orbits, as on Fusion's; dragging the ring spins about up
  let grab_id = id.with("grab");
  if resp.drag_started_by(egui::PointerButton::Primary) {
    let on_ring = resp.interact_pointer_pos().is_some_and(|p| b.on_ring(p - center));
    ui.ctx().data_mut(|d| d.insert_temp(grab_id, if on_ring { Grab::Ring } else { Grab::Cube }));
  }
  if resp.dragged_by(egui::PointerButton::Primary) {
    let d = resp.drag_delta();
    app.stop_cam_move();
    match ui.ctx().data(|m| m.get_temp::<Grab>(grab_id)) {
      Some(Grab::Ring) => {
        let tilt = b.up.z.abs();
        let spin = match resp.interact_pointer_pos() {
          Some(p) if tilt < 0.9 => {
            // the angle swept round the ring's centre; seen from below it turns the other way
            let c = at(b.proj(-DVec3::Z));
            let (now, before) = (p - c, p - d - c);
            let a = now.y.atan2(now.x) - before.y.atan2(before.x);
            let a = (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            a as f64 * if b.back.z >= 0.0 { 1.0 } else { -1.0 }
          }
          _ => -d.x as f64 * 0.008,
        };
        app.cam.orbit(spin, 0.0);
      }
      _ => app.cam.orbit(-d.x as f64 * 0.008, d.y as f64 * 0.008),
    }
  }

  // faded while the pointer is away, as on Fusion's
  let near = ui.ctx().pointer_hover_pos().is_some_and(|p| area.expand(30.0).contains(p))
    || resp.dragged()
    || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&menu_resp));
  let opacity = IDLE_OPACITY + (1.0 - IDLE_OPACITY) * ui.ctx().animate_bool_with_time(id.with("fade"), near, 0.2);
  let mut painter = painter.clone();
  painter.multiply_opacity(opacity);
  draw(&painter, center, &b, spot, ring_hot);
  for (n, (_, pts, _)) in arrows.iter().enumerate() {
    let c = if arrow_hit == Some(n) { crate::theme::ACCENT } else { Color32::from_gray(120) };
    painter.add(egui::Shape::convex_polygon(pts.to_vec(), c, Stroke::NONE));
  }
  for (n, (_, pts, _)) in rolls.iter().enumerate() {
    let c = if roll_hit == Some(n) { crate::theme::ACCENT } else { Color32::from_gray(110) };
    draw_roll_arrow(&painter, pts, c);
  }
  // the ViewCube menu button under the cube
  let menu_hover = menu_resp.hovered();
  crate::icons::draw(
    &painter,
    menu,
    crate::icons::Icon::Component,
    if menu_hover { crate::theme::ACCENT } else { Color32::from_gray(70) },
  );
  let hc = if home_resp.hovered() { crate::theme::ACCENT } else { Color32::from_gray(90) };
  draw_home(&painter, home, hc);
  egui::Popup::menu(&menu_resp).show(|ui| cube_menu(app, ui));

  if home_resp.clicked() {
    app.go_home();
    return true;
  }
  if menu_resp.clicked() {
    return true;
  }
  let clicked = resp.clicked();
  if clicked {
    let to = if let Some(n) = arrow_hit {
      let (dx, dy) = arrows[n].2;
      Some(app.cube.adjacent(&app.cam, dx, dy))
    } else if let Some(n) = roll_hit {
      Some(app.cube.rolled(&app.cam, rolls[n].2))
    } else {
      spot.map(|s| app.cube.spot_view(s))
    };
    if let Some((dir, up)) = to {
      app.fly_view(dir, up);
    }
  }
  clicked || resp.dragged()
}

/// The four triangles round a face view that step to the next face: (hot area, triangle,
/// (dx, dy) with y up).
/// The ViewCube of a view that isn't active (Multiple Views): drawn as `cam` sees it, without
/// its buttons; a click in the view makes it active first.
pub(crate) fn paint_passive(app: &App, painter: &egui::Painter, rect: Rect, cam: &Camera) {
  if !app.show_viewcube {
    return;
  }
  let k = app.preferences.values.cube_size.factor(rect.height());
  let mut b = Basis::new(&app.cube, cam);
  b.scale = SCALE * k;
  draw(painter, rect.right_top() + Vec2::new(-92.0, 80.0) * k, &b, None, false);
}

fn face_arrows(center: Pos2, scale: f32) -> Vec<(Rect, [Pos2; 3], (i32, i32))> {
  let reach = scale + 12.0;
  [(0, 1), (0, -1), (1, 0), (-1, 0)]
    .into_iter()
    .map(|(dx, dy)| {
      let dir = Vec2::new(dx as f32, -dy as f32);
      let side = Vec2::new(-dir.y, dir.x);
      let tip = center + dir * (reach + 6.0);
      let base = center + dir * reach;
      let tri = [tip, base + side * 6.0, base - side * 6.0];
      (Rect::from_center_size(center + dir * (reach + 3.0), Vec2::splat(14.0)), tri, (dx, dy))
    })
    .collect()
}

/// The two curved arrows at a face view's top right that turn it a quarter: (hot area, arc
/// points ending at the arrowhead, counter-clockwise).
fn roll_arrows(center: Pos2, scale: f32) -> Vec<(Rect, Vec<Pos2>, bool)> {
  let r = scale + 22.0;
  // screen angles, y down: the first sweeps up towards the top, the second down to the right
  [(-0.95f32, -1.35f32, true), (-0.6, -0.2, false)]
    .into_iter()
    .map(|(a0, a1, ccw)| {
      let pts: Vec<Pos2> =
        (0..=8).map(|n| a0 + (a1 - a0) * n as f32 / 8.0).map(|a| center + Vec2::new(a.cos(), a.sin()) * r).collect();
      let mid = center + Vec2::angled((a0 + a1) / 2.0) * r;
      (Rect::from_center_size(mid, Vec2::splat(18.0)), pts, ccw)
    })
    .collect()
}

fn draw_roll_arrow(painter: &egui::Painter, pts: &[Pos2], c: Color32) {
  painter.add(egui::Shape::line(pts.to_vec(), Stroke::new(2.0, c)));
  if let [.., a, b] = pts {
    let d = (*b - *a).normalized();
    let side = Vec2::new(-d.y, d.x);
    painter.add(egui::Shape::convex_polygon(vec![*b + d * 5.0, *b + side * 4.0, *b - side * 4.0], c, Stroke::NONE));
  }
}

fn draw_home(painter: &egui::Painter, home: Rect, c: Color32) {
  let (hx, hy) = (home.center().x, home.center().y);
  painter.add(egui::Shape::closed_line(
    vec![
      Pos2::new(hx - 7.0, hy - 1.0),
      Pos2::new(hx, hy - 7.0),
      Pos2::new(hx + 7.0, hy - 1.0),
      Pos2::new(hx + 5.0, hy - 1.0),
      Pos2::new(hx + 5.0, hy + 6.0),
      Pos2::new(hx - 5.0, hy + 6.0),
      Pos2::new(hx - 5.0, hy - 1.0),
    ],
    Stroke::new(1.5, c),
  ));
}

/// The cube itself: compass ring, axis stubs, shaded faces with their labels, and the hot spot
/// under the pointer tinted.
fn draw(painter: &egui::Painter, center: Pos2, b: &Basis, spot: Option<Spot>, ring_hot: bool) {
  let at = |v: DVec3| center + b.proj(v);
  let ring: Vec<Pos2> = b.ring().into_iter().map(|v| center + v).collect();
  let ring_col = if ring_hot { crate::theme::ACCENT } else { Color32::from_gray(150) };
  painter.add(egui::Shape::closed_line(ring, Stroke::new(if ring_hot { 2.5 } else { 1.5 }, ring_col)));
  // lettered axis stubs leave the cube from its silhouette, as on Fusion's ViewCube; the ones
  // pointing away from the viewer go behind the cube
  let sgn = |bit: usize| if bit != 0 { 1.0 } else { -1.0 };
  let axes = [
    (DVec3::X, "X", Color32::from_rgb(222, 40, 36)),
    (DVec3::Y, "Y", Color32::from_rgb(40, 170, 50)),
    (DVec3::Z, "Z", Color32::from_rgb(30, 70, 220)),
  ];
  let stub = |axis: DVec3, label: &str, col: Color32| {
    // the silhouette corner furthest along the axis's on-screen direction
    let d = b.proj(axis);
    let base = (0..8)
      .map(|i| DVec3::new(sgn(i & 1), sgn(i & 2), sgn(i & 4)))
      .max_by(|x, y| (b.proj(*x).dot(d) + x.dot(axis) as f32).total_cmp(&(b.proj(*y).dot(d) + y.dot(axis) as f32)))
      .unwrap_or(DVec3::ZERO);
    painter.line_segment([at(base), at(base + axis * 0.55)], Stroke::new(2.5, col));
    painter.text(at(base + axis * 0.85), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), col);
  };
  for (axis, label, col) in axes {
    if axis.dot(b.back) < 0.0 {
      stub(axis, label, col);
    }
  }
  // soft shadow under the cube
  painter.add(egui::Shape::ellipse_filled(
    center + Vec2::new(4.0, b.scale * 1.53),
    Vec2::new(b.scale * 1.3, b.scale * 0.26),
    Color32::from_black_alpha(28),
  ));
  let hot = Color32::from_rgb(170, 208, 240);
  for (i, s, facing) in b.faces() {
    let n = unit(i) * s as f64;
    let (j, k) = (unit((i + 1) % 3), unit((i + 2) % 3));
    let pts = vec![at(n + j + k), at(n - j + k), at(n - j - k), at(n + j - k)];
    // grey, lit from above-left like Fusion's cube
    let light = (n.dot(b.up) * 0.6 - n.dot(b.right) * 0.25 + 0.55).clamp(0.0, 1.0) as f32;
    let shade = (196.0 + 48.0 * light) as u8;
    painter.add(egui::Shape::convex_polygon(pts.clone(), Color32::from_gray(shade), Stroke::NONE));
    if let Some(patch) = spot.and_then(|sp| b.spot_patch(sp, i, s)) {
      painter.add(egui::Shape::convex_polygon(patch.map(|p| center + p).to_vec(), hot, Stroke::NONE));
    }
    painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.0, Color32::from_gray(140))));
    if facing > 0.25 {
      // the label lies in the face: rotate it along the face's horizontal
      let (label, h) = face_label(i, s);
      let dir = b.proj(n + h) - b.proj(n);
      let angle = dir.y.atan2(dir.x);
      let c = at(n);
      let color = Color32::from_gray(60).gamma_multiply((facing as f32 * 1.5).min(1.0));
      let galley = painter.layout_no_wrap(label.to_string(), egui::FontId::proportional(10.5), color);
      let half = galley.size() / 2.0;
      let rot = egui::emath::Rot2::from_angle(angle);
      let pos = c - rot * half;
      painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(angle));
    }
  }
  for (axis, label, col) in axes {
    if axis.dot(b.back) >= 0.0 {
      stub(axis, label, col);
    }
  }
}

/// The menu under the cube.
fn cube_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(220.0);
  if ui.button("Go Home").clicked() {
    app.go_home();
    ui.close();
  }
  ui.separator();
  let p = app.cube.projection(&app.cam);
  for (label, mode) in [
    ("Orthographic", Projection::Orthographic),
    ("Perspective", Projection::Perspective),
    ("Perspective with Ortho Faces", Projection::OrthoFaces),
  ] {
    if ui.radio(p == mode, label).clicked() {
      crate::navbar::set_projection(app, mode);
      ui.close();
    }
  }
  ui.separator();
  ui.menu_button("Set current view as Home", |ui| {
    for (label, fit) in [("Fixed distance", false), ("Fit to view", true)] {
      if ui.button(label).clicked() {
        app.cube.home = Some((app.view_now(), fit));
        app.status = Some((format!("Home view set ({})", label.to_lowercase()), false));
        ui.close();
      }
    }
  });
  if ui.add_enabled(app.cube.home.is_some(), egui::Button::new("Reset Home")).clicked() {
    app.cube.home = None;
    ui.close();
  }
  ui.separator();
  ui.menu_button("Set current view as", |ui| {
    for (label, top) in [("Front", false), ("Top", true)] {
      if ui.button(label).clicked() {
        let cam = app.view_now();
        app.cube.set_current_as(&cam, top);
        ui.close();
      }
    }
  });
  if ui.add_enabled(app.cube.frame != DMat3::IDENTITY, egui::Button::new("Reset Front")).clicked() {
    app.cube.frame = DMat3::IDENTITY;
    ui.close();
  }
  ui.menu_button("ViewCube Size", |ui| {
    ui.set_min_width(140.0);
    let now = app.preferences.values.cube_size;
    for s in CubeSize::ALL {
      if ui.radio(now == s, s.label()).clicked() {
        app.preferences.values.cube_size = s;
        app.preferences.save();
        ui.close();
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn basis(state: &CubeState, v: StandardView) -> (Camera, Basis) {
    let mut cam = Camera::default();
    let (d, up) = state.standard_view(v);
    cam.look_from(d, up);
    (cam, Basis::new(state, &cam))
  }

  fn cam_for(state: &CubeState, s: Spot) -> Camera {
    let mut cam = Camera::default();
    let (d, up) = state.spot_view(s);
    cam.look_from(d, up);
    cam
  }

  #[test]
  fn a_face_view_has_centre_edge_and_corner_zones() {
    let state = CubeState::default();
    let (_, b) = basis(&state, StandardView::Front);
    assert_eq!(b.spot_at(Vec2::ZERO), Some([0, -1, 0]));
    // the band along the top of the front face is the front-top edge
    assert_eq!(b.spot_at(Vec2::new(0.0, -0.85 * SCALE)), Some([0, -1, 1]));
    assert_eq!(b.spot_at(Vec2::new(0.85 * SCALE, 0.0)), Some([1, -1, 0]));
    assert_eq!(b.spot_at(Vec2::new(-0.85 * SCALE, 0.85 * SCALE)), Some([-1, -1, -1]));
    assert_eq!(b.spot_at(Vec2::new(0.85 * SCALE, -0.85 * SCALE)), Some([1, -1, 1]));
    assert_eq!(b.spot_at(Vec2::new(1.2 * SCALE, 0.0)), None, "off the cube");
  }

  #[test]
  fn the_front_top_right_corner_of_the_home_view_is_a_corner() {
    let state = CubeState::default();
    let (_, b) = basis(&state, StandardView::Home);
    // the corner shared by the three visible faces sits at the cube's centre on screen
    assert_eq!(b.spot_at(Vec2::new(0.0, 0.0)), Some([1, -1, 1]));
    assert_eq!(b.spot_at(b.proj(DVec3::new(0.0, -1.0, 0.0))), Some([0, -1, 0]), "the front face's centre");
    assert_eq!(b.spot_at(b.proj(DVec3::new(0.0, 0.0, 1.0))), Some([0, 0, 1]), "the top's");
    assert_eq!(b.spot_at(b.proj(DVec3::new(0.0, -0.9, 1.0))), Some([0, -1, 1]), "the edge between");
    // the far corner of the top face
    assert_eq!(b.spot_at(b.proj(DVec3::new(-0.95, 0.95, 1.0))), Some([-1, 1, 1]));
  }

  #[test]
  fn viewcube_sizes_scale_the_cube() {
    assert_eq!(CubeSize::Large.factor(900.0), 1.0);
    assert!(CubeSize::Tiny.factor(900.0) < CubeSize::Small.factor(900.0));
    assert!(CubeSize::Small.factor(900.0) < CubeSize::Normal.factor(900.0));
    assert!(CubeSize::Normal.factor(900.0) < 1.0, "Fusion's normal cube is smaller than the large one");
    // Automatic follows the canvas height
    assert_eq!(CubeSize::Automatic.factor(300.0), CubeSize::Tiny.factor(300.0));
    assert_eq!(CubeSize::Automatic.factor(600.0), CubeSize::Small.factor(600.0));
    assert_eq!(CubeSize::Automatic.factor(900.0), CubeSize::Normal.factor(900.0));
    assert_eq!(CubeSize::Automatic.factor(2000.0), CubeSize::Large.factor(2000.0));
    let state = CubeState::default();
    let (cam, mut b) = basis(&state, StandardView::Front);
    b.scale = SCALE * 0.5;
    assert_eq!(b.spot_at(Vec2::new(0.0, -0.85 * SCALE * 0.5)), Some([0, -1, 1]), "hit testing follows the size");
    assert_eq!(b.spot_at(Vec2::new(0.0, -0.85 * SCALE)), None, "the old band is off a tiny cube");
    let _ = cam;
  }

  #[test]
  fn hot_spots_go_to_their_views() {
    let state = CubeState::default();
    let cam = cam_for(&state, [1, -1, 1]);
    assert!((cam.eye_dir() - DVec3::new(1.0, -1.0, 1.0).normalize()).length() < 1e-9);
    assert!(cam.basis().1.z > 0.0 && cam.roll == 0.0, "Z up");
    let cam = cam_for(&state, [0, -1, 1]);
    let h = std::f64::consts::FRAC_1_SQRT_2;
    assert!((cam.eye_dir() - DVec3::new(0.0, -h, h)).length() < 1e-9, "45° between front and top");
    let cam = cam_for(&state, [-1, 0, -1]);
    assert!((cam.eye_dir() - DVec3::new(-h, 0.0, -h)).length() < 1e-9);
    let top = cam_for(&state, [0, 0, 1]);
    assert!((top.basis().1 - DVec3::Y).length() < 1e-6, "the front face at the bottom of the top view");
    assert_eq!(state.face_view(&top), Some([0, 0, 1]));
    assert_eq!(state.face_view(&cam), None);
    // every spot's view is the hot spot itself in the middle of the cube
    for x in -1..=1i8 {
      for y in -1..=1i8 {
        for z in -1..=1i8 {
          if (x, y, z) == (0, 0, 0) {
            continue;
          }
          let cam = cam_for(&state, [x, y, z]);
          assert_eq!(Basis::new(&state, &cam).spot_at(Vec2::ZERO), Some([x, y, z]));
        }
      }
    }
  }

  #[test]
  fn hovering_tints_only_the_hot_spot() {
    let state = CubeState::default();
    let (_, b) = basis(&state, StandardView::Home);
    // a corner touches the three visible faces, an edge two, a face only itself
    let patches = |s: Spot| b.faces().iter().filter(|(i, sg, _)| b.spot_patch(s, *i, *sg).is_some()).count();
    assert_eq!(patches([1, -1, 1]), 3);
    assert_eq!(patches([0, -1, 1]), 2);
    assert_eq!(patches([0, -1, 0]), 1);
    let centre = b.spot_patch([0, -1, 0], 1, -1).expect("on the front face");
    let area = |q: [Vec2; 4]| ((q[2] - q[0]).x * (q[3] - q[1]).y - (q[2] - q[0]).y * (q[3] - q[1]).x).abs() / 2.0;
    let face = [[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]].map(|[x, z]| b.proj(DVec3::new(x, -1.0, z)));
    let ratio = area(centre) / area(face);
    assert!((ratio - (FACE_CORE * FACE_CORE) as f32).abs() < 1e-3, "only the face's centre: {ratio}");
  }

  #[test]
  fn face_view_arrows_step_and_roll() {
    let state = CubeState::default();
    let (front, _) = basis(&state, StandardView::Front);
    let look = |(d, up): (DVec3, DVec3)| {
      let mut c = front;
      c.look_from(d, up);
      c
    };
    let top = look(state.adjacent(&front, 0, 1));
    assert_eq!(state.face_view(&top), Some([0, 0, 1]));
    assert!((top.basis().1 - DVec3::Y).length() < 1e-6, "the front face comes from below");
    assert_eq!(state.face_view(&look(state.adjacent(&front, 0, -1))), Some([0, 0, -1]));
    assert_eq!(state.face_view(&look(state.adjacent(&front, 1, 0))), Some([1, 0, 0]));
    assert_eq!(state.face_view(&look(state.adjacent(&front, -1, 0))), Some([-1, 0, 0]));
    // a quarter turn counter-clockwise brings what was on the right to the top
    let rolled = look(state.rolled(&front, true));
    assert_eq!(state.face_view(&rolled), Some([0, -1, 0]));
    assert!((rolled.basis().1 - DVec3::X).length() < 1e-6, "{:?}", rolled.basis());
    let back = look(state.rolled(&front, false));
    assert!((back.basis().1 + DVec3::X).length() < 1e-6);
    // four quarter turns come back round
    let mut c = front;
    for _ in 0..4 {
      let (d, up) = state.rolled(&c, true);
      c.look_from(d, up);
    }
    assert!((c.basis().1 - DVec3::Z).length() < 1e-6);
  }

  #[test]
  fn current_view_as_front_or_top_turns_the_cube() {
    let mut state = CubeState::default();
    let (right, _) = basis(&state, StandardView::Right);
    state.set_current_as(&right, false);
    let (d, up) = state.standard_view(StandardView::Front);
    assert!((d - DVec3::X).length() < 1e-6 && (up - DVec3::Z).length() < 1e-6, "{d} {up}");
    let (cam, b) = basis(&state, StandardView::Front);
    assert_eq!(b.spot_at(Vec2::ZERO), Some([0, -1, 0]), "the cube's front faces the viewer");
    assert!((cam.eye_dir() - DVec3::X).length() < 1e-6);
    let mut state = CubeState::default();
    let (front, _) = basis(&state, StandardView::Front);
    state.set_current_as(&front, true);
    let (d, up) = state.standard_view(StandardView::Top);
    assert!((d + DVec3::Y).length() < 1e-6 && (up - DVec3::Z).length() < 1e-6, "{d} {up}");
  }

  #[test]
  fn the_ring_is_round_the_cube_not_on_it() {
    let state = CubeState::default();
    let (_, b) = basis(&state, StandardView::Top);
    assert!(b.on_ring(Vec2::new(RING as f32 * SCALE, 0.0)));
    assert!(!b.on_ring(Vec2::ZERO));
    assert!(!b.on_ring(Vec2::new(1.25 * SCALE, 0.0)));
  }
}
