//! Feature handles: the arrows and angle dials a dialog shows in the viewport beside its single
//! drag arrow (see `viewport::manipulator`) and the move/rotate triad (see
//! [`crate::manipulators`]), as in Fusion: Shell's thickness arrows, Draft's angle dial and the
//! distance, count and angle handles of the rectangular and circular patterns. The open dialog
//! lists them ([`crate::Dialog::handles`]) and takes a dragged value back into its fields
//! ([`crate::Dialog::set_handle`]), so a typed value and a drag always agree.

use egui::{Color32, Pos2, Stroke, Vec2};
use fox_engine::kernel::glam::DVec3;

use crate::App;
use crate::manipulators::{axis_param, ring_angle};
use crate::viewport::Drag;

/// Radius on screen (points) of a dial without a radius of its own.
const DIAL_PX: f64 = 70.0;
/// Pointer distance that catches a handle's head (points).
const CATCH_PX: f32 = 12.0;
/// Angle snap (radians) without Incremental Move: 1°.
const ANGLE_STEP: f64 = std::f64::consts::PI / 180.0;

const IDLE: Color32 = Color32::from_rgb(30, 110, 235);
const ACTIVE: Color32 = Color32::from_rgb(255, 140, 20);

/// Where a handle sits and how its value moves it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
  /// A distance along unit `dir` from `origin`: the head sits at `origin + dir·value`.
  Arrow { origin: DVec3, dir: DVec3 },
  /// An angle (radians) about unit `axis` through `center`, from unit `zero` (across the axis)
  /// towards `axis × zero`: the head sits `value` round a circle of `radius` (world units; None
  /// for a fixed size on screen).
  Dial { center: DVec3, axis: DVec3, zero: DVec3, radius: Option<f64> },
}

/// How a handle's head is drawn: an arrowhead for a distance or angle, a dot for a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
  Arrow,
  Dot,
}

/// One handle: its shape, current value and the text shown by its head.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
  pub shape: Shape,
  pub value: f64,
  pub head: Head,
  pub label: String,
}

impl Handle {
  pub fn arrow(origin: DVec3, dir: DVec3, value: f64, label: String) -> Handle {
    Handle { shape: Shape::Arrow { origin, dir }, value, head: Head::Arrow, label }
  }

  pub fn dial(center: DVec3, axis: DVec3, zero: DVec3, radius: Option<f64>, value: f64, label: String) -> Handle {
    Handle { shape: Shape::Dial { center, axis, zero, radius }, value, head: Head::Arrow, label }
  }

  /// The same handle drawn as a count (a dot).
  pub fn dot(self) -> Handle {
    Handle { head: Head::Dot, ..self }
  }

  /// Is this an angle (snapped to angle steps) rather than a length?
  pub fn angular(&self) -> bool {
    matches!(self.shape, Shape::Dial { .. })
  }
}

/// The world point a dial of radius `r` reaches at angle `a`.
fn dial_point(center: DVec3, axis: DVec3, zero: DVec3, r: f64, a: f64) -> DVec3 {
  let y = axis.cross(zero);
  center + (zero * a.cos() + y * a.sin()) * r
}

/// A handle drag in progress: the value follows the pointer from where it grabbed the head.
#[derive(Debug, Clone, Copy)]
pub struct HandleDrag {
  pub index: usize,
  shape: Shape,
  start: f64,
  /// Arrows: the axis parameter grabbed. Dials: the angle at the last update.
  grab: f64,
  /// Dials: the total turned since the grab (so a drag can go round more than half a turn).
  turned: f64,
}

impl HandleDrag {
  /// Grabs handle `index` where the pointer ray `ro + rd·s` meets it.
  pub fn begin(index: usize, h: &Handle, ro: DVec3, rd: DVec3) -> Option<HandleDrag> {
    let grab = match h.shape {
      Shape::Arrow { origin, dir } => axis_param(origin, dir, ro, rd)?,
      Shape::Dial { center, axis, zero, .. } => ring_angle(center, axis, zero, ro, rd)?,
    };
    Some(HandleDrag { index, shape: h.shape, start: h.value, grab, turned: 0.0 })
  }

  /// The value for the pointer ray now (not snapped).
  pub fn update(&mut self, ro: DVec3, rd: DVec3) -> Option<f64> {
    match self.shape {
      Shape::Arrow { origin, dir } => Some(self.start + axis_param(origin, dir, ro, rd)? - self.grab),
      Shape::Dial { center, axis, zero, .. } => {
        let a = ring_angle(center, axis, zero, ro, rd)?;
        self.turned += wrap(a - self.grab);
        self.grab = a;
        Some(self.start + self.turned)
      }
    }
  }
}

/// An angle brought into (-π, π].
fn wrap(a: f64) -> f64 {
  let t = std::f64::consts::TAU;
  let r = a.rem_euclid(t);
  if r > std::f64::consts::PI { r - t } else { r }
}

/// A handle on screen: the shaft or arc, and the head with the direction it points.
struct Placed {
  line: Vec<Pos2>,
  /// The whole circle of a dial, drawn faintly.
  circle: Vec<Pos2>,
  head: Pos2,
  dir: Vec2,
}

/// World length that spans `px` points on screen at `at`.
fn world_size(project: &dyn Fn(DVec3) -> Option<Pos2>, right: DVec3, at: DVec3, px: f64) -> Option<f64> {
  let (a, b) = (project(at)?, project(at + right)?);
  let per_unit = (b - a).length() as f64;
  (per_unit > 1e-9).then(|| px / per_unit)
}

fn place(h: &Handle, right: DVec3, project: &dyn Fn(DVec3) -> Option<Pos2>) -> Option<Placed> {
  match h.shape {
    Shape::Arrow { origin, dir } => {
      let tip = origin + dir * h.value;
      let (a, b) = (project(origin)?, project(tip)?);
      // the way the value grows, flipped for a negative value
      let along = (project(origin + dir * world_size(project, right, origin, 20.0)?)? - a).normalized();
      let along = if along.is_finite() { along } else { Vec2::new(0.0, -1.0) };
      let d = if h.value < 0.0 { -along } else { along };
      Some(Placed { line: vec![a, b], circle: Vec::new(), head: b, dir: d })
    }
    Shape::Dial { center, axis, zero, radius } => {
      let r = match radius {
        Some(r) if r > 1e-9 => r,
        _ => world_size(project, right, center, DIAL_PX)?,
      };
      let at = |a: f64| project(dial_point(center, axis, zero, r, a));
      let circle: Vec<Pos2> = (0..=64).filter_map(|k| at(std::f64::consts::TAU * k as f64 / 64.0)).collect();
      let sweep = h.value.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
      let n = ((sweep.abs() / std::f64::consts::TAU * 64.0).ceil() as usize).max(1);
      let mut line: Vec<Pos2> = (0..=n).filter_map(|k| at(sweep * k as f64 / n as f64)).collect();
      if let Some(c) = project(center) {
        line.insert(0, c);
      }
      let head = at(h.value)?;
      // the arc's tangent, the way the value grows
      let ahead = at(h.value + if h.value < 0.0 { -0.05 } else { 0.05 })?;
      let d = (ahead - head).normalized();
      Some(Placed { line, circle, head, dir: if d.is_finite() { d } else { Vec2::new(1.0, 0.0) } })
    }
  }
}

/// The handle whose head is nearest a screen point, within reach.
fn hit(placed: &[Option<Placed>], p: Pos2) -> Option<usize> {
  placed
    .iter()
    .enumerate()
    .filter_map(|(i, x)| Some((i, (x.as_ref()?.head - p).length())))
    .filter(|(_, d)| *d <= CATCH_PX)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(i, _)| i)
}

/// Where a handle's label (or floating value box) sits: beside the head.
fn label_at(p: &Placed) -> Pos2 {
  let (b, d) = (p.head, p.dir);
  let side = Vec2::new(-d.y, d.x);
  b + side * 14.0 + d * 4.0
}

fn draw(painter: &egui::Painter, h: &Handle, p: &Placed, lit: bool, active: bool, label: bool) {
  let col = if lit { ACTIVE } else { IDLE };
  if p.circle.len() > 1 {
    painter.add(egui::Shape::line(p.circle.clone(), Stroke::new(1.0, col.gamma_multiply(0.35))));
  }
  if p.line.len() > 1 {
    painter.add(egui::Shape::line(p.line.clone(), Stroke::new(if lit { 2.5 } else { 2.0 }, col)));
  }
  let (b, d) = (p.head, p.dir);
  match h.head {
    Head::Arrow => {
      let side = Vec2::new(-d.y, d.x);
      painter.add(egui::Shape::convex_polygon(vec![b + d * 12.0, b + side * 6.0, b - side * 6.0], col, Stroke::NONE));
    }
    Head::Dot => {
      painter.circle_filled(b, 6.0, col);
      painter.circle_stroke(b, 6.0, Stroke::new(1.5, Color32::WHITE));
    }
  }
  let at = label_at(p);
  if !label {
    return;
  }
  if active {
    let galley = painter.layout_no_wrap(h.label.clone(), egui::FontId::proportional(12.0), Color32::WHITE);
    let r = egui::Rect::from_min_size(
      at + Vec2::new(0.0, -galley.size().y / 2.0 - 2.0),
      galley.size() + Vec2::new(10.0, 4.0),
    );
    painter.rect_filled(r, 3.0, ACTIVE);
    painter.galley(r.min + Vec2::new(5.0, 2.0), galley, Color32::WHITE);
  } else {
    painter.text(at, egui::Align2::LEFT_CENTER, &h.label, egui::FontId::proportional(12.0), Color32::from_gray(40));
  }
}

/// Shows the open dialog's handles and runs their drags. `project` maps world points to the
/// screen, `ray` screen points to pointer rays, and lengths snap to `step`. Returns true while a
/// handle has the pointer.
pub(crate) fn show(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  ctx: &egui::Context,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
  step: f64,
) -> bool {
  let dragging = matches!(app.drag, Some(Drag::Handle));
  if !dragging {
    app.manip.handle = None;
  }
  let handles = app.dialog.as_ref().map(|d| d.handles(&app.s)).unwrap_or_default();
  if handles.is_empty() {
    return false;
  }
  let (right, _, _) = app.cam.basis();
  if dragging && let Some(drag) = app.manip.handle.as_mut() {
    let i = drag.index;
    if let Some(p) = resp.interact_pointer_pos() {
      let (o, d) = ray(p);
      let angular = handles.get(i).is_some_and(Handle::angular);
      let snap = if angular { app.increments.angular_step(ANGLE_STEP) } else { step };
      if let Some(v) = drag.update(o, d)
        && let Some(dlg) = app.dialog.as_mut()
      {
        dlg.set_handle(&app.s, i, v, snap);
      }
    }
    let now = app.dialog.as_ref().map(|d| d.handles(&app.s)).unwrap_or_default();
    for (k, h) in now.iter().enumerate() {
      if let Some(p) = place(h, right, project) {
        draw(painter, h, &p, k == i, k == i, true);
      }
    }
    ctx.request_repaint();
    return true;
  }
  let placed: Vec<Option<Placed>> = handles.iter().map(|h| place(h, right, project)).collect();
  let hot = resp.hover_pos().and_then(|p| hit(&placed, p));
  let press = ctx.input(|i| i.pointer.press_origin());
  let pressed = press.and_then(|p| hit(&placed, p));
  if let (Some(i), Some(p)) = (pressed, press)
    && resp.drag_started_by(egui::PointerButton::Primary)
    && app.drag.is_none()
  {
    let (o, d) = ray(p);
    if let Some(dr) = HandleDrag::begin(i, &handles[i], o, d) {
      app.manip.handle = Some(dr);
      app.drag = Some(Drag::Handle);
    }
  }
  // Fusion's floating value box beside the first handle that edits a field (Shell's thickness);
  // a dialog with a drag arrow has the box there instead (see `viewport::manipulator`)
  let arrow_box = app.dialog.as_mut().is_some_and(|d| d.value_box_field().is_some());
  let params = crate::value_box::params_of(&app.s);
  let mut shown = None;
  for (k, (h, p)) in handles.iter().zip(&placed).enumerate() {
    let Some(p) = p else { continue };
    let mut boxed = false;
    if !arrow_box
      && shown.is_none()
      && let Some(dlg) = app.dialog.as_mut()
      && let Some(field) = dlg.handle_field(k)
    {
      let at = label_at(p);
      if crate::value_box::show(ctx, &mut app.manip.value_box, at, field, &params).changed {
        dlg.value_box_edited();
      }
      shown = Some(k);
      boxed = true;
    }
    draw(painter, h, p, hot == Some(k), false, !boxed);
  }
  if !arrow_box && shown.is_none() {
    app.manip.value_box.hide();
  }
  hot.is_some() || pressed.is_some() && ctx.input(|i| i.pointer.primary_down())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// An orthographic view down -Z: world (x, y) → screen (100 + x, 100 - y).
  fn top(p: DVec3) -> Option<Pos2> {
    Some(Pos2::new(100.0 + p.x as f32, 100.0 - p.y as f32))
  }

  #[test]
  fn arrows_follow_the_pointer_from_the_grab() {
    let h = Handle::arrow(DVec3::ZERO, DVec3::X, 10.0, String::new());
    // grabbed at its head (x = 10) and dragged to x = 14.5: the value goes up by as much
    let mut d = HandleDrag::begin(0, &h, DVec3::new(10.0, 3.0, 9.0), DVec3::NEG_Z).unwrap();
    assert!((d.update(DVec3::new(14.5, -2.0, 9.0), DVec3::NEG_Z).unwrap() - 14.5).abs() < 1e-9);
    // grabbed off the head, the value moves by the pointer's travel, not to where it points
    let mut d = HandleDrag::begin(0, &h, DVec3::new(8.0, 0.0, 9.0), DVec3::NEG_Z).unwrap();
    assert!((d.update(DVec3::new(5.0, 0.0, 9.0), DVec3::NEG_Z).unwrap() - 7.0).abs() < 1e-9);
    // along the line of sight there is nothing to drag
    let up = Handle::arrow(DVec3::ZERO, DVec3::Z, 1.0, String::new());
    assert!(HandleDrag::begin(0, &up, DVec3::new(0.0, 0.0, 9.0), DVec3::NEG_Z).is_none());
  }

  #[test]
  fn dials_turn_past_half_a_turn() {
    let h = Handle::dial(DVec3::ZERO, DVec3::Z, DVec3::X, Some(10.0), 0.5, String::new());
    let at = |deg: f64| {
      let a = deg.to_radians();
      (DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 9.0), DVec3::NEG_Z)
    };
    let (o, d) = at(0.0);
    let mut drag = HandleDrag::begin(0, &h, o, d).unwrap();
    let mut v = 0.0;
    for deg in (30..=330).step_by(30) {
      let (o, d) = at(deg as f64);
      v = drag.update(o, d).unwrap();
    }
    assert!((v - 0.5 - 330f64.to_radians()).abs() < 1e-9, "{v}");
    // and back the other way, below zero
    for deg in (-60..=300).rev().step_by(30) {
      let (o, d) = at(deg as f64);
      v = drag.update(o, d).unwrap();
    }
    assert!((v - 0.5 + 60f64.to_radians()).abs() < 1e-9, "{v}");
  }

  #[test]
  fn heads_sit_at_their_values_and_are_hit_on_screen() {
    let right = DVec3::X;
    let arrow = Handle::arrow(DVec3::ZERO, DVec3::X, 30.0, "30 mm".into());
    let count = Handle::arrow(DVec3::ZERO, DVec3::Y, 50.0, "3".into()).dot();
    // a quarter turn about Z from X, radius 40: the head is at (0, 40)
    let dial = Handle::dial(DVec3::ZERO, DVec3::Z, DVec3::X, Some(40.0), -std::f64::consts::FRAC_PI_2, "".into());
    let placed: Vec<Option<Placed>> = [&arrow, &count, &dial].iter().map(|h| place(h, right, &top)).collect();
    assert_eq!(placed[0].as_ref().unwrap().head, Pos2::new(130.0, 100.0));
    assert_eq!(placed[1].as_ref().unwrap().head, Pos2::new(100.0, 50.0));
    let dh = placed[2].as_ref().unwrap().head;
    assert!((dh - Pos2::new(100.0, 140.0)).length() < 1e-3, "{dh:?}");
    assert_eq!(hit(&placed, Pos2::new(133.0, 104.0)), Some(0));
    assert_eq!(hit(&placed, Pos2::new(101.0, 52.0)), Some(1));
    assert_eq!(hit(&placed, Pos2::new(98.0, 137.0)), Some(2));
    assert_eq!(hit(&placed, Pos2::new(60.0, 60.0)), None);
    // a dial without a radius of its own keeps a fixed size on screen
    let free = Handle::dial(DVec3::ZERO, DVec3::Z, DVec3::X, None, 0.0, "".into());
    let p = place(&free, right, &top).unwrap();
    assert!((p.head - Pos2::new(100.0 + DIAL_PX as f32, 100.0)).length() < 1e-3);
  }
}
