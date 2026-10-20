//! Drag manipulators in the viewport beyond the single arrow (see `viewport::manipulator`): the
//! move/rotate triad, as in Fusion's Move and Joint commands. Three arrows move along X, Y and Z,
//! three rings rotate about them and three square handles move in the plane across each axis.
//! The open dialog says where the triad sits and which handles it has
//! ([`crate::Dialog::triad`]) and takes the dragged result back into its fields
//! ([`crate::Dialog::set_triad`]), so a typed value and a drag always agree.
//!
//! Everything is drawn with the egui painter over the rendered image, so it works with any
//! renderer.

use egui::{Color32, Pos2, Stroke, Vec2};
use fox_engine::kernel::glam::DVec3;
use fox_engine::render::ray_plane;

use crate::App;
use crate::viewport::Drag;

/// Arrow length on screen (points); rings and planar handles scale with it.
const SIZE_PX: f64 = 95.0;
/// Ring radius and planar handle extent, as fractions of the arrow length.
const RING: f64 = 0.62;
const PLANE: (f64, f64) = (0.2, 0.36);
/// Pointer distance that catches a handle (points).
const CATCH_PX: f32 = 7.0;
/// Rotation snap (radians) without Incremental Move: 5°.
const ANGLE_STEP: f64 = std::f64::consts::PI / 36.0;

const AXIS_COLORS: [Color32; 3] =
  [Color32::from_rgb(220, 70, 60), Color32::from_rgb(60, 165, 70), Color32::from_rgb(45, 100, 230)];
const ACTIVE: Color32 = Color32::from_rgb(255, 140, 20);

/// A part of the triad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
  /// Move along axis `i`.
  Arrow(usize),
  /// Rotate about axis `i`.
  Ring(usize),
  /// Move in the plane across axis `i` (spanned by the other two).
  Plane(usize),
}

/// A move/rotate triad and the handles it offers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triad {
  pub origin: DVec3,
  /// Unit, mutually perpendicular axes.
  pub axes: [DVec3; 3],
  pub arrows: [bool; 3],
  pub rings: [bool; 3],
  pub planes: [bool; 3],
  /// Where the triad sits with nothing moved: the distances it shows are measured from here.
  pub home: DVec3,
  /// The current rotation (radians) about each axis: dragging a ring continues from it.
  pub angles: [f64; 3],
}

impl Triad {
  /// A full triad on the world axes.
  pub fn world(origin: DVec3) -> Triad {
    Triad {
      origin,
      axes: [DVec3::X, DVec3::Y, DVec3::Z],
      arrows: [true; 3],
      rings: [true; 3],
      planes: [true; 3],
      home: origin,
      angles: [0.0; 3],
    }
  }

  /// How far the triad has moved from home along axis `i`.
  pub fn moved(&self, i: usize) -> f64 {
    (self.origin - self.home).dot(self.axes[i])
  }

  fn has(&self, h: Handle) -> bool {
    match h {
      Handle::Arrow(i) => self.arrows[i],
      Handle::Ring(i) => self.rings[i],
      Handle::Plane(i) => self.planes[i],
    }
  }
}

/// What a triad drag asks for: put the triad's origin here, or turn it to this angle about one
/// of its axes (keeping its origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriadEdit {
  Move(DVec3),
  Rotate(usize, f64),
}

/// A triad drag in progress.
#[derive(Debug, Clone, Copy)]
pub struct TriadDrag {
  pub handle: Handle,
  start: Triad,
  /// Where the drag grabbed the handle: the axis parameter (arrows), the point on the plane
  /// (planar handles) or the angle on the ring.
  grab: f64,
  grab_pt: DVec3,
  /// Rings: the angle at the last update and the total turned since the grab (so a drag can go
  /// round more than half a turn).
  last: f64,
  turned: f64,
}

impl TriadDrag {
  /// Grabs a handle where the pointer ray `ro + rd·s` meets it.
  pub fn begin(t: Triad, handle: Handle, ro: DVec3, rd: DVec3) -> Option<TriadDrag> {
    let mut d = TriadDrag { handle, start: t, grab: 0.0, grab_pt: t.origin, last: 0.0, turned: 0.0 };
    match handle {
      Handle::Arrow(i) => d.grab = axis_param(t.origin, t.axes[i], ro, rd)?,
      Handle::Plane(i) => d.grab_pt = ray_plane(ro, rd, t.origin, t.axes[i])?,
      Handle::Ring(i) => {
        d.grab = ring_angle(t.origin, t.axes[i], t.axes[(i + 1) % 3], ro, rd)?;
        d.last = d.grab;
      }
    }
    Some(d)
  }

  /// The edit for the pointer ray now: moves snap to `step` (world units), turns to `turn`
  /// (radians).
  pub fn update(&mut self, ro: DVec3, rd: DVec3, step: f64, turn: f64) -> Option<TriadEdit> {
    let t = &self.start;
    match self.handle {
      Handle::Arrow(i) => {
        let d = snap(axis_param(t.origin, t.axes[i], ro, rd)? - self.grab, step);
        Some(TriadEdit::Move(t.origin + t.axes[i] * d))
      }
      Handle::Plane(i) => {
        let delta = ray_plane(ro, rd, t.origin, t.axes[i])? - self.grab_pt;
        let (u, v) = (t.axes[(i + 1) % 3], t.axes[(i + 2) % 3]);
        Some(TriadEdit::Move(t.origin + u * snap(delta.dot(u), step) + v * snap(delta.dot(v), step)))
      }
      Handle::Ring(i) => {
        let a = ring_angle(t.origin, t.axes[i], t.axes[(i + 1) % 3], ro, rd)?;
        self.turned += wrap(a - self.last);
        self.last = a;
        Some(TriadEdit::Rotate(i, snap(t.angles[i] + self.turned, turn)))
      }
    }
  }
}

/// The triad drag the viewport is running, if any, and the value box the last drag left.
#[derive(Default)]
pub struct State {
  pub drag: Option<TriadDrag>,
  /// After a drag, as in Fusion: the dragged handle's value in a box that takes typing.
  pub typed: Option<Typed>,
}

/// The value box of a handle: its text while it is being typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Typed {
  pub handle: Handle,
  pub text: String,
}

/// A handle's value as the box shows it: an arrow's distance from home, a ring's angle, a
/// planar handle's two distances.
pub fn value_text(t: &Triad, h: Handle) -> String {
  let mm = |v: f64| format!("{} mm", fox_engine::sketch::Dim::new((v * 1e4).round() / 1e4).expr);
  match h {
    Handle::Arrow(i) => mm(t.moved(i)),
    Handle::Ring(i) => {
      format!("{} deg", fox_engine::sketch::Dim::new((t.angles[i].to_degrees() * 1e4).round() / 1e4).expr)
    }
    Handle::Plane(i) => format!("{}, {}", mm(t.moved((i + 1) % 3)), mm(t.moved((i + 2) % 3))),
  }
}

/// The edit a typed value asks for: values in model units (mm, radians), two of them for a
/// planar handle. None when the count is wrong or a value isn't finite.
pub fn typed_edit(t: &Triad, h: Handle, values: &[f64]) -> Option<TriadEdit> {
  if values.iter().any(|v| !v.is_finite()) {
    return None;
  }
  let to = |i: usize, v: f64| t.axes[i] * (v - t.moved(i));
  match (h, values) {
    (Handle::Arrow(i), [v]) => Some(TriadEdit::Move(t.origin + to(i, *v))),
    (Handle::Ring(i), [a]) => Some(TriadEdit::Rotate(i, *a)),
    (Handle::Plane(i), [a, b]) => Some(TriadEdit::Move(t.origin + to((i + 1) % 3, *a) + to((i + 2) % 3, *b))),
    _ => None,
  }
}

/// Reads a value box: comma-separated lengths (angles for a ring), with the design's
/// parameters.
fn parse_typed(s: &fox_engine::Session, h: Handle, text: &str) -> Option<Vec<f64>> {
  use fox_engine::expr::{Kind, eval};
  let kind = if matches!(h, Handle::Ring(_)) { Kind::Angle } else { Kind::Length };
  text.split(',').map(|p| eval(p.trim(), kind, &s.model.params).ok()).collect()
}

/// Parameter along the axis `o + d·t` of its point closest to the ray `ro + rd·s` (None when
/// they are parallel).
pub fn axis_param(o: DVec3, d: DVec3, ro: DVec3, rd: DVec3) -> Option<f64> {
  let w0 = o - ro;
  let (a, b, c) = (d.dot(d), d.dot(rd), rd.dot(rd));
  let (dd, e) = (d.dot(w0), rd.dot(w0));
  let den = a * c - b * b;
  if den.abs() <= 1e-9 * a * c {
    return None;
  }
  Some((b * e - c * dd) / den)
}

/// Angle (radians, from `x` towards `axis × x`) of where the ray meets the plane through
/// `center` across `axis`.
pub fn ring_angle(center: DVec3, axis: DVec3, x: DVec3, ro: DVec3, rd: DVec3) -> Option<f64> {
  let v = ray_plane(ro, rd, center, axis)? - center;
  let x = (x - axis * x.dot(axis)).try_normalize()?;
  let y = axis.cross(x);
  if v.length_squared() < 1e-18 {
    return None;
  }
  Some(v.dot(y).atan2(v.dot(x)))
}

/// `v` rounded to a multiple of `step` (unchanged for a step of zero).
pub fn snap(v: f64, step: f64) -> f64 {
  if step > 0.0 { (v / step).round() * step } else { v }
}

/// An angle brought into (-π, π].
fn wrap(a: f64) -> f64 {
  let t = std::f64::consts::TAU;
  let r = a.rem_euclid(t);
  if r > std::f64::consts::PI { r - t } else { r }
}

/// Two unit vectors across `axis`.
fn across(axis: DVec3, hint: DVec3) -> (DVec3, DVec3) {
  let x = (hint - axis * hint.dot(axis)).try_normalize().unwrap_or_else(|| axis.any_orthonormal_vector());
  (x, axis.cross(x))
}

/// The triad's handles on screen.
struct Layout {
  arrows: [Option<(Pos2, Pos2)>; 3],
  rings: [Vec<Pos2>; 3],
  planes: [Option<[Pos2; 4]>; 3],
}

fn layout(t: &Triad, size: f64, project: &dyn Fn(DVec3) -> Option<Pos2>) -> Layout {
  let o = t.origin;
  let arrows = [0, 1, 2].map(|i| {
    if !t.arrows[i] {
      return None;
    }
    Some((project(o + t.axes[i] * size * 0.12)?, project(o + t.axes[i] * size)?))
  });
  let rings = [0, 1, 2].map(|i| {
    if !t.rings[i] {
      return Vec::new();
    }
    let (x, y) = across(t.axes[i], t.axes[(i + 1) % 3]);
    (0..=64)
      .filter_map(|k| {
        let a = std::f64::consts::TAU * k as f64 / 64.0;
        project(o + (x * a.cos() + y * a.sin()) * size * RING)
      })
      .collect()
  });
  let planes = [0, 1, 2].map(|i| {
    if !t.planes[i] {
      return None;
    }
    let (u, v) = (t.axes[(i + 1) % 3] * size, t.axes[(i + 2) % 3] * size);
    let (a, b) = PLANE;
    Some([
      project(o + u * a + v * a)?,
      project(o + u * b + v * a)?,
      project(o + u * b + v * b)?,
      project(o + u * a + v * b)?,
    ])
  });
  Layout { arrows, rings, planes }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
  crate::select::seg_dist([p.x as f64, p.y as f64], [a.x as f64, a.y as f64], [b.x as f64, b.y as f64]) as f32
}

/// The handle under a screen point: arrows and planar handles before rings, nearest first.
fn hit(l: &Layout, p: Pos2) -> Option<Handle> {
  let nearest = |offers: Vec<(f32, Handle)>| {
    offers.into_iter().filter(|(d, _)| *d <= CATCH_PX).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, h)| h)
  };
  let mut first: Vec<(f32, Handle)> = Vec::new();
  for (i, a) in l.arrows.iter().enumerate() {
    if let Some((base, tip)) = a {
      first.push((seg_dist(p, *base, *tip).min((p - *tip).length() - 4.0), Handle::Arrow(i)));
    }
  }
  for (i, q) in l.planes.iter().enumerate() {
    if let Some(q) = q
      && crate::select::point_in_polygon([p.x as f64, p.y as f64], &q.map(|c| [c.x as f64, c.y as f64]))
    {
      first.push((0.0, Handle::Plane(i)));
    }
  }
  nearest(first).or_else(|| {
    let rings = l
      .rings
      .iter()
      .enumerate()
      .map(|(i, r)| (r.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f32::INFINITY, f32::min), Handle::Ring(i)));
    nearest(rings.collect())
  })
}

fn draw(painter: &egui::Painter, l: &Layout, lit: Option<Handle>) {
  let color = |h: Handle, i: usize| if lit == Some(h) { ACTIVE } else { AXIS_COLORS[i] };
  for (i, r) in l.rings.iter().enumerate() {
    if r.len() > 1 {
      let w = if lit == Some(Handle::Ring(i)) { 3.0 } else { 1.5 };
      painter.add(egui::Shape::line(r.clone(), Stroke::new(w, color(Handle::Ring(i), i))));
    }
  }
  for (i, q) in l.planes.iter().enumerate() {
    if let Some(q) = q {
      let c = color(Handle::Plane(i), i);
      let fill = if lit == Some(Handle::Plane(i)) { c.gamma_multiply(0.6) } else { c.gamma_multiply(0.25) };
      painter.add(egui::Shape::convex_polygon(q.to_vec(), fill, Stroke::new(1.0, c)));
    }
  }
  for (i, a) in l.arrows.iter().enumerate() {
    let Some((base, tip)) = a else { continue };
    let c = color(Handle::Arrow(i), i);
    painter.line_segment([*base, *tip], Stroke::new(if lit == Some(Handle::Arrow(i)) { 3.0 } else { 2.0 }, c));
    let dir = (*tip - *base).normalized();
    if dir.is_finite() && (*tip - *base).length() > 2.0 {
      let side = Vec2::new(-dir.y, dir.x);
      painter.add(egui::Shape::convex_polygon(
        vec![*tip + dir * 11.0, *tip + side * 5.5, *tip - side * 5.5],
        c,
        Stroke::NONE,
      ));
    } else {
      painter.circle_filled(*tip, 5.0, c);
    }
  }
}

/// World length that spans `px` points on screen at `at`.
fn world_size(project: &dyn Fn(DVec3) -> Option<Pos2>, right: DVec3, at: DVec3, px: f64) -> Option<f64> {
  let (a, b) = (project(at)?, project(at + right)?);
  let per_unit = (b - a).length() as f64;
  (per_unit > 1e-9).then(|| px / per_unit)
}

/// Shows the open dialog's triad and runs its drags. `project` maps world points to the screen,
/// `ray` screen points to pointer rays, and moves snap to `step`. Returns true while the triad
/// has the pointer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn triad(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  ctx: &egui::Context,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
  step: f64,
) -> bool {
  if let Some(d) = app.dialog.as_mut() {
    d.refresh_manip(&app.s);
  }
  let dragging = matches!(app.drag, Some(Drag::Triad));
  if !dragging && let Some(done) = app.manip.drag.take() {
    // the drag let go: its value stays in a box that takes typing
    app.manip.typed = Some(Typed { handle: done.handle, text: String::new() });
  }
  let Some(t) = app.dialog.as_ref().and_then(|d| d.triad(&app.s)) else {
    app.manip.typed = None;
    return false;
  };
  let (right, _, _) = app.cam.basis();
  let Some(size) = world_size(project, right, t.origin, SIZE_PX) else { return false };
  if dragging && let Some(drag) = app.manip.drag.as_mut() {
    let handle = drag.handle;
    if let Some(p) = resp.interact_pointer_pos() {
      let (o, d) = ray(p);
      let turn = app.increments.angular_step(ANGLE_STEP);
      if let Some(edit) = drag.update(o, d, step, turn)
        && let Some(dlg) = app.dialog.as_mut()
      {
        dlg.set_triad(&app.s, edit);
      }
    }
    let shown = app.dialog.as_ref().and_then(|d| d.triad(&app.s)).unwrap_or(t);
    let l = layout(&shown, size, project);
    draw(painter, &l, Some(handle));
    readout(painter, &shown, handle, &l, project);
    ctx.request_repaint();
    return true;
  }
  let l = layout(&t, size, project);
  let hot = resp.hover_pos().and_then(|p| hit(&l, p)).filter(|h| t.has(*h));
  let press = ctx.input(|i| i.pointer.press_origin());
  let pressed = press.and_then(|p| hit(&l, p)).filter(|h| t.has(*h));
  if let (Some(h), Some(p)) = (pressed, press)
    && resp.drag_started_by(egui::PointerButton::Primary)
    && app.drag.is_none()
  {
    let (o, d) = ray(p);
    if let Some(dr) = TriadDrag::begin(t, h, o, d) {
      app.manip.drag = Some(dr);
      app.manip.typed = None;
      app.drag = Some(Drag::Triad);
    }
  }
  draw(painter, &l, hot.or(app.manip.typed.as_ref().map(|b| b.handle)));
  value_box(app, ctx, &t, &l, project);
  hot.is_some() || pressed.is_some() && ctx.input(|i| i.pointer.primary_down())
}

/// Where a handle's value shows: past the arrow's tip, by the planar handle's outer corner, at
/// the centre for a ring.
fn value_at(t: &Triad, h: Handle, l: &Layout, project: &dyn Fn(DVec3) -> Option<Pos2>) -> Option<Pos2> {
  match h {
    Handle::Ring(_) => project(t.origin),
    Handle::Arrow(i) => l.arrows[i].map(|a| a.1),
    Handle::Plane(i) => l.planes[i].map(|q| q[2]),
  }
}

/// The box a finished drag leaves next to its handle: it shows the value and takes a typed one
/// (a length or angle expression; "x, y" for a planar handle). Enter applies it, Esc closes it.
fn value_box(app: &mut App, ctx: &egui::Context, t: &Triad, l: &Layout, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let Some(mut b) = app.manip.typed.take() else { return };
  if !t.has(b.handle) {
    return;
  }
  let Some(at) = value_at(t, b.handle, l, project) else { return };
  // a new box starts with the value, selected, so typing replaces it
  let fresh = b.text.is_empty();
  if fresh {
    b.text = value_text(t, b.handle);
  }
  let (mut commit, mut cancel) = (false, false);
  egui::Area::new(egui::Id::new("triad-value"))
    .fixed_pos(at + Vec2::new(14.0, -26.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::popup(ui.style()).inner_margin(3.0).stroke(Stroke::new(1.0, ACTIVE)).show(ui, |ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut b.text).desired_width(110.0));
        if fresh {
          crate::viewport::focus_text(ctx, &r, &b.text, true);
        }
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        cancel = esc && (r.has_focus() || r.lost_focus());
        commit = enter && !esc && (r.has_focus() || r.lost_focus());
      });
    });
  if commit {
    // the Enter is this box's, not the dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    if let Some(edit) = parse_typed(&app.s, b.handle, &b.text).and_then(|v| typed_edit(t, b.handle, &v))
      && let Some(dlg) = app.dialog.as_mut()
    {
      dlg.set_triad(&app.s, edit);
    }
    // show the value as applied
    b.text.clear();
  }
  if !cancel {
    app.manip.typed = Some(b);
  }
}

/// The dragged handle's value, next to it.
fn readout(painter: &egui::Painter, t: &Triad, h: Handle, l: &Layout, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let Some(at) = value_at(t, h, l, project) else { return };
  let text = value_text(t, h);
  let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), Color32::WHITE);
  let r = egui::Rect::from_min_size(at + Vec2::new(14.0, -24.0), galley.size() + Vec2::new(10.0, 4.0));
  painter.rect_filled(r, 3.0, ACTIVE);
  painter.galley(r.min + Vec2::new(5.0, 2.0), galley, Color32::WHITE);
}

#[cfg(test)]
mod tests {
  use super::*;

  const EPS: f64 = 1e-9;

  /// An orthographic view down -Z: world (x, y) → screen (100 + x, 100 - y).
  fn top(p: DVec3) -> Option<Pos2> {
    Some(Pos2::new(100.0 + p.x as f32, 100.0 - p.y as f32))
  }

  /// The pointer ray of `top` through a screen point.
  fn top_ray(p: Pos2) -> (DVec3, DVec3) {
    (DVec3::new(p.x as f64 - 100.0, 100.0 - p.y as f64, 50.0), DVec3::NEG_Z)
  }

  #[test]
  fn axis_and_ring_math() {
    // a ray straight down at x = 7 meets the X axis at 7, the Y axis at 0
    let (o, d) = (DVec3::new(7.0, 3.0, 10.0), DVec3::NEG_Z);
    assert!((axis_param(DVec3::ZERO, DVec3::X, o, d).unwrap() - 7.0).abs() < EPS);
    assert!((axis_param(DVec3::ZERO, DVec3::Y, o, d).unwrap() - 3.0).abs() < EPS);
    // along the axis itself there is no closest point
    assert!(axis_param(DVec3::ZERO, DVec3::Z, o, d).is_none());
    // a slanted ray: the closest approach, not the plane hit
    let p = axis_param(DVec3::new(0.0, 0.0, 5.0), DVec3::X, DVec3::new(2.0, -10.0, 5.0), DVec3::Y).unwrap();
    assert!((p - 2.0).abs() < EPS);
    // angles about Z from X towards Y
    let ang = |x: f64, y: f64| ring_angle(DVec3::ZERO, DVec3::Z, DVec3::X, DVec3::new(x, y, 9.0), DVec3::NEG_Z);
    assert!((ang(0.0, 4.0).unwrap() - std::f64::consts::FRAC_PI_2).abs() < EPS);
    assert!((ang(-4.0, 0.0).unwrap().abs() - std::f64::consts::PI).abs() < EPS);
    assert!(ang(0.0, 0.0).is_none());
    // edge-on: the ray never meets the ring's plane
    assert!(ring_angle(DVec3::ZERO, DVec3::Z, DVec3::X, DVec3::new(0.0, 0.0, 1.0), DVec3::X).is_none());
    assert!((snap(7.26, 0.5) - 7.5).abs() < EPS && snap(-1.2, 0.0) == -1.2);
    assert!((wrap(3.0 * std::f64::consts::PI) - std::f64::consts::PI).abs() < EPS);
    assert!((wrap(-0.25) + 0.25).abs() < EPS);
  }

  #[test]
  fn arrow_and_plane_drags_move_the_origin() {
    let t = Triad::world(DVec3::new(10.0, 0.0, 0.0));
    // grab the X arrow at x = 30 and drag to x = 41.3: moves 11.3, snapped to 0.5 → 11.5
    let mut d = TriadDrag::begin(t, Handle::Arrow(0), DVec3::new(30.0, 0.0, 50.0), DVec3::NEG_Z).unwrap();
    let e = d.update(DVec3::new(41.3, 6.0, 50.0), DVec3::NEG_Z, 0.5, ANGLE_STEP).unwrap();
    assert_eq!(e, TriadEdit::Move(DVec3::new(21.5, 0.0, 0.0)));
    // the XY planar handle (across Z) follows the pointer in both directions
    let mut d = TriadDrag::begin(t, Handle::Plane(2), DVec3::new(12.0, 2.0, 50.0), DVec3::NEG_Z).unwrap();
    let e = d.update(DVec3::new(15.0, -4.0, 50.0), DVec3::NEG_Z, 1.0, ANGLE_STEP).unwrap();
    assert_eq!(e, TriadEdit::Move(DVec3::new(13.0, -6.0, 0.0)));
  }

  #[test]
  fn ring_drags_turn_past_half_a_turn_and_continue_the_current_angle() {
    let mut t = Triad::world(DVec3::ZERO);
    let at = |deg: f64| {
      let a = deg.to_radians();
      (DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 50.0), DVec3::NEG_Z)
    };
    // grab the Z ring at 0° and sweep round to 200° in steps: 200°, not -160°
    let (o, d) = at(0.0);
    let mut drag = TriadDrag::begin(t, Handle::Ring(2), o, d).unwrap();
    let mut last = None;
    for deg in (20..=200).step_by(20) {
      let (o, d) = at(deg as f64);
      last = drag.update(o, d, 1.0, ANGLE_STEP);
    }
    let Some(TriadEdit::Rotate(2, a)) = last else { panic!("{last:?}") };
    assert!((a.to_degrees() - 200.0).abs() < 1e-6);
    // a ring already turned 30° continues from there; snapping to 5°
    t.angles[2] = 30f64.to_radians();
    let (o, d) = at(90.0);
    let mut drag = TriadDrag::begin(t, Handle::Ring(2), o, d).unwrap();
    let (o, d) = at(101.0);
    let Some(TriadEdit::Rotate(2, a)) = drag.update(o, d, 1.0, ANGLE_STEP) else { panic!() };
    assert!((a.to_degrees() - 40.0).abs() < 1e-6, "{}", a.to_degrees());
    // another ring starts from zero
    let (o, d) = (DVec3::new(50.0, 0.0, 10.0), DVec3::NEG_X);
    let mut drag = TriadDrag::begin(t, Handle::Ring(0), o, d).unwrap();
    let e = drag.update(DVec3::new(50.0, 0.0, 10.0), DVec3::NEG_X, 1.0, ANGLE_STEP);
    assert_eq!(e, Some(TriadEdit::Rotate(0, 0.0)));
  }

  #[test]
  fn value_box_shows_and_takes_the_handles_values() {
    // a triad moved (10, -4, 2) from home and turned 45° about Y
    let mut t = Triad::world(DVec3::new(10.0, -4.0, 2.0));
    t.home = DVec3::ZERO;
    t.angles[1] = 45f64.to_radians();
    assert_eq!(value_text(&t, Handle::Arrow(0)), "10 mm");
    assert_eq!(value_text(&t, Handle::Arrow(1)), "-4 mm");
    assert_eq!(value_text(&t, Handle::Ring(1)), "45 deg");
    assert_eq!(value_text(&t, Handle::Ring(2)), "0 deg");
    // the XY planar handle (across Z) shows X and Y
    assert_eq!(value_text(&t, Handle::Plane(2)), "10 mm, -4 mm");
    // a typed distance is the total from home: X to 25 keeps Y and Z
    assert_eq!(typed_edit(&t, Handle::Arrow(0), &[25.0]), Some(TriadEdit::Move(DVec3::new(25.0, -4.0, 2.0))));
    assert_eq!(
      typed_edit(&t, Handle::Plane(0), &[1.0, 3.0]),
      Some(TriadEdit::Move(DVec3::new(10.0, 1.0, 3.0))),
      "the YZ handle takes Y then Z"
    );
    assert_eq!(typed_edit(&t, Handle::Ring(2), &[0.5]), Some(TriadEdit::Rotate(2, 0.5)));
    // wrong counts and non-numbers are refused
    assert_eq!(typed_edit(&t, Handle::Plane(2), &[1.0]), None);
    assert_eq!(typed_edit(&t, Handle::Arrow(2), &[f64::NAN]), None);
    // a tilted triad measures along its own axes
    let tilted = Triad {
      axes: [DVec3::new(1.0, 1.0, 0.0).normalize(), DVec3::new(-1.0, 1.0, 0.0).normalize(), DVec3::Z],
      ..Triad::world(DVec3::new(3.0, 3.0, 0.0))
    };
    let tilted = Triad { home: DVec3::ZERO, ..tilted };
    assert!((tilted.moved(0) - 18f64.sqrt()).abs() < 1e-9 && tilted.moved(1).abs() < 1e-9);
    let Some(TriadEdit::Move(p)) = typed_edit(&tilted, Handle::Arrow(0), &[0.0]) else { panic!() };
    assert!(p.length() < 1e-9);
    // typed text: expressions with units and parameters, two values for a plane
    let mut s = fox_engine::Session::default();
    s.execute("param.add", &serde_json::json!({"name": "w", "expr": "12 mm"})).unwrap();
    assert_eq!(parse_typed(&s, Handle::Arrow(0), "w / 2"), Some(vec![6.0]));
    assert_eq!(parse_typed(&s, Handle::Plane(0), "1 cm, -2"), Some(vec![10.0, -2.0]));
    let a = parse_typed(&s, Handle::Ring(0), "90").unwrap();
    assert!((a[0] - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    assert_eq!(parse_typed(&s, Handle::Arrow(0), "nope"), None);
  }

  #[test]
  fn handles_are_hit_on_screen() {
    let t = Triad::world(DVec3::ZERO);
    let l = layout(&t, 100.0, &top);
    // seen from the top the Z arrow collapses to a point and its ring is edge-on... but the
    // Z ring is the circle round the origin
    assert_eq!(hit(&l, Pos2::new(180.0, 101.0)), Some(Handle::Arrow(0)));
    assert_eq!(hit(&l, Pos2::new(99.0, 30.0)), Some(Handle::Arrow(1)));
    // inside the XY square (between 20 and 36 along X and Y)
    assert_eq!(hit(&l, Pos2::new(128.0, 72.0)), Some(Handle::Plane(2)));
    // on the Z ring (radius 62), away from the arrows
    let a = 45f32.to_radians();
    assert_eq!(hit(&l, Pos2::new(100.0 + 62.0 * a.cos(), 100.0 - 62.0 * a.sin())), Some(Handle::Ring(2)));
    assert_eq!(hit(&l, Pos2::new(150.0, 150.0 + 30.0)), None);
    // a triad without planar handles or rings offers only its arrows
    let only = Triad { rings: [false; 3], planes: [false; 3], ..t };
    let l = layout(&only, 100.0, &top);
    assert_eq!(hit(&l, Pos2::new(128.0, 72.0)), None);
    assert!(l.rings.iter().all(Vec::is_empty));
    // grabbing through the screen: the ray of a screen point drives the drag
    let (o, d) = top_ray(Pos2::new(150.0, 100.0));
    let mut drag = TriadDrag::begin(t, Handle::Arrow(0), o, d).unwrap();
    let (o, d) = top_ray(Pos2::new(162.0, 90.0));
    assert_eq!(drag.update(o, d, 1.0, ANGLE_STEP), Some(TriadEdit::Move(DVec3::new(12.0, 0.0, 0.0))));
    assert!((world_size(&top, DVec3::X, DVec3::ZERO, 95.0).unwrap() - 95.0).abs() < 1e-6);
  }
}
