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
/// Radius on screen (points) of Extrude's taper handle, and how far (radians) its short arc runs
/// past the angle's ends.
const TAPER_PX: f64 = 36.0;
const TAPER_ARC: f64 = 0.35;
/// Pointer distance that catches a handle's head (points).
const CATCH_PX: f32 = 12.0;
/// Angle snap (radians) without Incremental Move: 1°.
const ANGLE_STEP: f64 = std::f64::consts::PI / 180.0;

const IDLE: Color32 = Color32::from_rgb(30, 110, 235);
const ACTIVE: Color32 = Color32::from_rgb(255, 140, 20);
/// Draft's disc handle and its arc at rest, and the pull arrow's outline.
const DARK: Color32 = Color32::from_gray(25);
/// Screen length (points) of Draft's pull-direction arrow.
const PULL_PX: f64 = 44.0;
/// Pointer distance from a disc handle's arc that catches it (points).
const TRACK_PX: f32 = 6.0;
/// Extrude's taper handle: its arc at rest, and its blue when it has the value box.
const TAPER_GREY: Color32 = Color32::from_rgb(150, 150, 150);
const TAPER_BLUE: Color32 = Color32::from_rgb(30, 155, 215);

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

/// How a handle's head is drawn: an arrowhead for a distance or angle, a dot for a count,
/// Fusion's white disc with a dark dot riding a dark arc for Draft's angle, and a hollow white
/// arrow (shown, not dragged) for Draft's pull direction; Extrude's taper is a white disc with a
/// black dot on a short grey arc, without a label. A spin (the sketch Circular Pattern's
/// quantity) is Fusion's rotate icon alone, without the dial's circle or spoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
  Arrow,
  Dot,
  Spin,
  Disc,
  Pull,
  Taper,
}

/// One handle: its shape, current value and the text shown by its head.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
  pub shape: Shape,
  pub value: f64,
  pub head: Head,
  pub label: String,
  /// The step (radians) an angle snaps to without Incremental Move, when not 1°.
  pub snap: Option<f64>,
}

impl Handle {
  pub fn arrow(origin: DVec3, dir: DVec3, value: f64, label: String) -> Handle {
    Handle { shape: Shape::Arrow { origin, dir }, value, head: Head::Arrow, label, snap: None }
  }

  pub fn dial(center: DVec3, axis: DVec3, zero: DVec3, radius: Option<f64>, value: f64, label: String) -> Handle {
    Handle { shape: Shape::Dial { center, axis, zero, radius }, value, head: Head::Arrow, label, snap: None }
  }

  /// The same handle drawn as a count (a dot).
  pub fn dot(self) -> Handle {
    Handle { head: Head::Dot, ..self }
  }

  /// The same dial drawn as Fusion's rotate icon alone (no circle or spoke drawn).
  pub fn spin(self) -> Handle {
    Handle { head: Head::Spin, ..self }
  }

  /// The same dial drawn as Draft's disc on an arc, turning in steps of `snap` radians.
  pub fn disc(self, snap: f64) -> Handle {
    Handle { head: Head::Disc, snap: Some(snap), ..self }
  }

  /// The same dial drawn as Extrude's taper handle: a disc on a short arc beside a profile edge.
  pub fn taper(self) -> Handle {
    Handle { head: Head::Taper, ..self }
  }

  /// A direction shown as a hollow arrow of fixed size on screen from `origin` along `dir`
  /// (Draft's pull direction): drawn, never dragged.
  pub fn pull(origin: DVec3, dir: DVec3) -> Handle {
    Handle { shape: Shape::Arrow { origin, dir }, value: 0.0, head: Head::Pull, label: String::new(), snap: None }
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
  /// The whole circle of a dial, drawn faintly (a disc handle's only while hovered, dashed).
  circle: Vec<Pos2>,
  head: Pos2,
  dir: Vec2,
  /// Can the pointer take it (not Draft's pull arrow), and does a press on its arc (`line`)
  /// take it as well as one on its head (a disc handle)?
  grab: bool,
  on_track: bool,
}

/// World length that spans `px` points on screen at `at`.
fn world_size(project: &dyn Fn(DVec3) -> Option<Pos2>, right: DVec3, at: DVec3, px: f64) -> Option<f64> {
  let (a, b) = (project(at)?, project(at + right)?);
  let per_unit = (b - a).length() as f64;
  (per_unit > 1e-9).then(|| px / per_unit)
}

fn place(h: &Handle, right: DVec3, project: &dyn Fn(DVec3) -> Option<Pos2>) -> Option<Placed> {
  let grab = h.head != Head::Pull;
  match h.shape {
    Shape::Arrow { origin, dir } if h.head == Head::Pull => {
      let (a, b) = (project(origin)?, project(origin + dir * world_size(project, right, origin, PULL_PX)?)?);
      let d = (b - a).normalized();
      let d = if d.is_finite() && (b - a).length() > 1.0 { d } else { Vec2::new(0.0, -1.0) };
      Some(Placed { line: vec![a, b], circle: Vec::new(), head: b, dir: d, grab, on_track: false })
    }
    Shape::Arrow { origin, dir } => {
      let tip = origin + dir * h.value;
      let (a, b) = (project(origin)?, project(tip)?);
      // the way the value grows, flipped for a negative value
      let along = (project(origin + dir * world_size(project, right, origin, 20.0)?)? - a).normalized();
      let along = if along.is_finite() { along } else { Vec2::new(0.0, -1.0) };
      let d = if h.value < 0.0 { -along } else { along };
      Some(Placed { line: vec![a, b], circle: Vec::new(), head: b, dir: d, grab, on_track: false })
    }
    Shape::Dial { center, axis, zero, radius } => {
      let taper = h.head == Head::Taper;
      let r = match radius {
        Some(r) if r > 1e-9 => r,
        _ => world_size(project, right, center, if taper { TAPER_PX } else { DIAL_PX })?,
      };
      let at = |a: f64| project(dial_point(center, axis, zero, r, a));
      if taper {
        // a short arc through the head, from the angle's start (the side face at no taper)
        let (a0, a1) = (h.value.min(0.0) - TAPER_ARC, h.value.max(0.0) + TAPER_ARC);
        let line: Vec<Pos2> = (0..=24).filter_map(|k| at(a0 + (a1 - a0) * k as f64 / 24.0)).collect();
        let head = at(h.value)?;
        let d = (at(h.value + 0.05)? - head).normalized();
        let dir = if d.is_finite() { d } else { Vec2::new(1.0, 0.0) };
        return Some(Placed { line, circle: Vec::new(), head, dir, grab, on_track: false });
      }
      let spin = h.head == Head::Spin;
      let circle: Vec<Pos2> =
        if spin { Vec::new() } else { (0..=64).filter_map(|k| at(std::f64::consts::TAU * k as f64 / 64.0)).collect() };
      let disc = h.head == Head::Disc;
      let line: Vec<Pos2> = if spin {
        Vec::new()
      } else if disc {
        // the disc rides a half circle either side of zero (Draft's pull direction)
        let half = std::f64::consts::FRAC_PI_2;
        (0..=32).filter_map(|k| at(-half + 2.0 * half * k as f64 / 32.0)).collect()
      } else {
        let sweep = h.value.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
        let n = ((sweep.abs() / std::f64::consts::TAU * 64.0).ceil() as usize).max(1);
        let mut line: Vec<Pos2> = (0..=n).filter_map(|k| at(sweep * k as f64 / n as f64)).collect();
        if let Some(c) = project(center) {
          line.insert(0, c);
        }
        line
      };
      let head = at(h.value)?;
      // the arc's tangent, the way the value grows
      let ahead = at(h.value + if h.value < 0.0 { -0.05 } else { 0.05 })?;
      let d = (ahead - head).normalized();
      let dir = if d.is_finite() { d } else { Vec2::new(1.0, 0.0) };
      Some(Placed { line, circle, head, dir, grab, on_track: disc })
    }
  }
}

/// The handle whose head is nearest a screen point, within reach; a disc handle is also caught
/// anywhere on its arc. Draft's pull arrow is never caught.
fn hit(placed: &[Option<Placed>], p: Pos2) -> Option<usize> {
  let reach = |x: &Placed| {
    let head = (x.head - p).length();
    if head <= CATCH_PX {
      return Some(head);
    }
    // a press on the arc counts as one just beyond the head's reach (heads win where both are)
    let track = x.line.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f32::INFINITY, f32::min);
    (x.on_track && track <= TRACK_PX).then_some(CATCH_PX + track)
  };
  placed
    .iter()
    .enumerate()
    .filter_map(|(i, x)| x.as_ref().filter(|x| x.grab).and_then(|x| Some((i, reach(x)?))))
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(i, _)| i)
}

/// Distance on screen from `p` to the segment `a`–`b`.
fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
  let ab = b - a;
  let t = if ab.length_sq() > 1e-9 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
  (p - (a + ab * t)).length()
}

/// Where a handle's label sits: beside the head.
fn label_at(p: &Placed) -> Pos2 {
  let (b, d) = (p.head, p.dir);
  let side = Vec2::new(-d.y, d.x);
  b + side * 14.0 + d * 4.0
}

/// The floating value box's size on screen (points), near enough to keep it off the arrow.
const BOX_SIZE: Vec2 = Vec2::new(96.0, 22.0);
/// The gap kept between the box and the arrow it belongs to (points).
const BOX_GAP: f32 = 4.0;

/// Where Fusion's box goes when the shaft runs below and to the right of the head: about 110 px
/// right of and 140 px below the head (its top-left corner from the head, points).
const BOX_FAR: Vec2 = Vec2::new(108.0, 132.0);

/// Where the floating value box goes beside an arrow from `a` to its head at `b` pointing `d`
/// (a unit screen direction): its left-centre, as [`crate::value_box::show`] takes it. As in
/// Fusion it sits apart from the arrow, below and to the right of the head, so the arrow stays
/// visible and can be grabbed: just off the head where the shaft leaves room, else further out
/// the same way ([`BOX_FAR`]) until it clears the shaft, never round to the other side (onto the
/// body the arrow stands on). Only a canvas edge sends it to another corner; it stays inside
/// `canvas` whatever the arrow's length.
pub(crate) fn box_beside(a: Pos2, b: Pos2, d: Vec2, canvas: egui::Rect) -> Pos2 {
  let side = Vec2::new(-d.y, d.x);
  let head = egui::Rect::from_points(&[b, b + d * 12.0, b + side * 6.0, b - side * 6.0]).expand(BOX_GAP);
  let shaft = |r: egui::Rect| {
    let r = r.expand(BOX_GAP);
    let n = ((b - a).length() / 2.0).ceil().clamp(1.0, 400.0) as usize;
    (0..=n).any(|k| r.contains(a + (b - a) * (k as f32 / n as f32)))
  };
  let clear = |r: &egui::Rect| !shaft(*r) && !r.intersects(head);
  let below_right = std::iter::once(egui::Rect::from_min_size(head.right_bottom(), BOX_SIZE))
    .chain((0..8).map(|k| egui::Rect::from_min_size(b + BOX_FAR * (1.0 + 0.5 * k as f32), BOX_SIZE)));
  let corners = [
    egui::Rect::from_min_size(head.left_bottom() - Vec2::new(BOX_SIZE.x, 0.0), BOX_SIZE),
    egui::Rect::from_min_size(head.right_top() - Vec2::new(0.0, BOX_SIZE.y), BOX_SIZE),
    egui::Rect::from_min_size(head.left_top() - BOX_SIZE, BOX_SIZE),
  ];
  let first = egui::Rect::from_min_size(head.right_bottom(), BOX_SIZE);
  let r = below_right
    .clone()
    .find(|r| clear(r) && canvas.contains_rect(*r))
    .or_else(|| corners.iter().copied().find(|r| clear(r) && canvas.contains_rect(*r)))
    .or_else(|| below_right.clone().find(clear))
    .unwrap_or(first);
  // kept inside the canvas (an arrow running off it takes its box no further than the edge)
  let min = canvas.min;
  let max = (canvas.max - BOX_SIZE).max(min);
  let top_left = r.min.clamp(min, max);
  top_left + Vec2::new(0.0, BOX_SIZE.y / 2.0)
}

/// Draft's handles as Fusion draws them: the angle's white disc with a dark dot on a dark arc
/// (both blue while dragged, a dashed circle round the dial while hovered), the pull direction
/// a hollow white arrow outlined dark.
fn draw_draft(painter: &egui::Painter, h: &Handle, p: &Placed, lit: bool, active: bool) {
  let (b, d) = (p.head, p.dir);
  if h.head == Head::Pull {
    let a = p.line[0];
    let side = Vec2::new(-d.y, d.x);
    let neck = b - d * 14.0;
    let shaft = [a + side * 3.0, neck + side * 3.0, neck - side * 3.0, a - side * 3.0];
    let tip = [b, neck + side * 9.0, neck - side * 9.0];
    painter.add(egui::Shape::convex_polygon(shaft.to_vec(), Color32::WHITE, Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(tip.to_vec(), Color32::WHITE, Stroke::NONE));
    let outline = vec![
      a + side * 3.0,
      neck + side * 3.0,
      neck + side * 9.0,
      b,
      neck - side * 9.0,
      neck - side * 3.0,
      a - side * 3.0,
    ];
    painter.add(egui::Shape::closed_line(outline, Stroke::new(1.5, DARK)));
    return;
  }
  let ink = if active { IDLE } else { DARK };
  if lit && !active && p.circle.len() > 1 {
    painter.extend(egui::Shape::dashed_line(&p.circle, Stroke::new(1.0, DARK), 5.0, 4.0));
  }
  if p.line.len() > 1 {
    painter.add(egui::Shape::line(p.line.clone(), Stroke::new(if active { 2.5 } else { 2.0 }, ink)));
  }
  painter.circle_filled(b, 7.0, if active { IDLE } else { Color32::WHITE });
  painter.circle_stroke(b, 7.0, Stroke::new(1.5, ink));
  painter.circle_filled(b, 2.5, if active { Color32::WHITE } else { DARK });
}

/// Extrude's taper handle as Fusion draws it: a white disc with a black dot on a short grey arc;
/// blue in a white ring on a blue arc with white ticks while it has the value box or is
/// dragged; faded while the distance arrow is dragged.
fn draw_taper(painter: &egui::Painter, p: &Placed, blue: bool, faded: bool) {
  let fade = |c: Color32| if faded { c.gamma_multiply(0.3) } else { c };
  if p.line.len() > 1 {
    let col = if blue { TAPER_BLUE } else { TAPER_GREY };
    painter.add(egui::Shape::line(p.line.clone(), Stroke::new(if blue { 3.0 } else { 2.0 }, fade(col))));
    if blue {
      for w in p.line.windows(2).step_by(4) {
        let d = (w[1] - w[0]).normalized();
        if d.is_finite() {
          let n = d.rot90() * 2.0;
          painter.line_segment([w[0] - n, w[0] + n], Stroke::new(1.0, fade(Color32::WHITE)));
        }
      }
    }
  }
  let b = p.head;
  if blue {
    painter.circle_filled(b, 7.0, fade(Color32::WHITE));
    painter.circle_filled(b, 5.0, fade(TAPER_BLUE));
  } else {
    painter.circle_filled(b, 6.0, fade(Color32::WHITE));
    painter.circle_stroke(b, 6.0, Stroke::new(1.0, fade(Color32::from_gray(110))));
    painter.circle_filled(b, 2.0, fade(Color32::BLACK));
  }
}

/// Fusion's rotate icon: a curved arrow three quarters of the way round, on a white disc.
fn draw_spin(painter: &egui::Painter, at: Pos2, ink: Color32) {
  painter.circle_filled(at, 10.0, Color32::WHITE);
  painter.circle_stroke(at, 10.0, Stroke::new(1.0, Color32::from_gray(150)));
  let r = 5.5;
  let (a0, a1) = (-0.25 * std::f32::consts::PI, 1.25 * std::f32::consts::PI);
  let pt = |a: f32| at + Vec2::new(a.cos(), -a.sin()) * r;
  let arc: Vec<Pos2> = (0..=16).map(|k| pt(a0 + (a1 - a0) * k as f32 / 16.0)).collect();
  painter.add(egui::Shape::line(arc, Stroke::new(1.6, ink)));
  // the arrowhead at the arc's end, along its tangent
  let tip = pt(a1);
  let along = Vec2::new(-a1.sin(), -a1.cos());
  let side = Vec2::new(-along.y, along.x);
  painter.add(egui::Shape::convex_polygon(
    vec![tip + along * 4.0, tip + side * 3.0, tip - side * 3.0],
    ink,
    Stroke::NONE,
  ));
}

fn draw(painter: &egui::Painter, h: &Handle, p: &Placed, lit: bool, active: bool, label: bool) {
  if h.head == Head::Taper {
    return draw_taper(painter, p, active, false);
  }
  if matches!(h.head, Head::Disc | Head::Pull) {
    return draw_draft(painter, h, p, lit, active);
  }
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
    Head::Spin => draw_spin(painter, b, if lit { ACTIVE } else { Color32::from_gray(70) }),
    Head::Disc | Head::Pull | Head::Taper => {}
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
    // Tab put the box at a handle that is gone: it goes back to its first place
    if app.manip.value_box.at_handle.is_some() {
      app.manip.value_box.hide();
    }
    return false;
  }
  let (right, _, _) = app.cam.basis();
  if dragging && let Some(drag) = app.manip.handle.as_mut() {
    let i = drag.index;
    if let Some(p) = resp.interact_pointer_pos() {
      let (o, d) = ray(p);
      let h = handles.get(i);
      let angular = h.is_some_and(Handle::angular);
      let turn = h.and_then(|h| h.snap).unwrap_or(ANGLE_STEP);
      let snap = if angular { app.increments.angular_step(turn) } else { step };
      if let Some(v) = drag.update(o, d)
        && let Some(dlg) = app.dialog.as_mut()
      {
        dlg.set_handle(&app.s, i, v, snap);
      }
    }
    let now = app.dialog.as_ref().map(|d| d.handles(&app.s)).unwrap_or_default();
    let canvas = resp.rect;
    let params = crate::value_box::params_of(&app.s);
    for (k, h) in now.iter().enumerate() {
      let Some(p) = place(h, right, project) else { continue };
      // a handle whose box opens with the drag (Draft's angle) shows it beside the head
      let mut boxed = false;
      if k == i
        && h.head == Head::Disc
        && let Some(dlg) = app.dialog.as_mut()
        && let Some(field) = dlg.handle_field(k)
      {
        let at = box_beside(p.head - p.dir * 20.0, p.head, p.dir, canvas);
        if crate::value_box::show_nth(ctx, &mut app.manip.value_box, 0, at, field, &params).changed {
          dlg.value_box_edited();
        }
        boxed = true;
      }
      draw(painter, h, &p, k == i, k == i, !boxed);
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
  // a dialog with a drag arrow has the box there instead (see `viewport::manipulator`), until
  // Tab moves it to a handle (Extrude's taper)
  let at_handle = app.manip.value_box.at_handle;
  let arrow_box = at_handle.is_none() && app.dialog.as_mut().is_some_and(|d| d.value_box_field().is_some());
  let every = app.dialog.as_ref().is_some_and(|d| d.boxes_every_handle());
  // the distance arrow is being dragged: the taper handle fades, as in Fusion
  let faded = matches!(app.drag, Some(Drag::Manip));
  let params = crate::value_box::params_of(&app.s);
  let mut shown = 0;
  let mut tabbed = false;
  for (k, (h, p)) in handles.iter().zip(&placed).enumerate() {
    let Some(p) = p else { continue };
    let mut boxed = false;
    let wants_box = match at_handle {
      Some(j) => j == k,
      None => !arrow_box && (shown == 0 || every),
    };
    if wants_box
      && let Some(dlg) = app.dialog.as_mut()
      && let Some(field) = dlg.handle_field(k)
    {
      let at = match h.shape {
        Shape::Arrow { .. } => box_beside(p.line[0], p.head, p.dir, resp.rect),
        Shape::Dial { .. } => box_beside(p.head - p.dir * 20.0, p.head, p.dir, resp.rect),
      };
      let out = crate::value_box::show_nth(ctx, &mut app.manip.value_box, shown, at, field, &params);
      if out.changed {
        dlg.value_box_edited();
      }
      tabbed |= out.tab;
      shown += 1;
      boxed = true;
    }
    if h.head == Head::Taper {
      draw_taper(painter, p, boxed, faded);
    } else {
      draw(painter, h, p, hot == Some(k), false, !boxed);
    }
  }
  if shown == 0 && (at_handle.is_some() || !arrow_box) {
    app.manip.value_box.hide();
  }
  if tabbed {
    tab(app);
  }
  hot.is_some() || pressed.is_some() && ctx.input(|i| i.pointer.primary_down())
}

/// Tab in the floating value box: it moves to the command's next value in Fusion's order (the
/// drag arrow's, then each handle's with a field: Extrude's distance, then its taper angle) and
/// round again, the value selected. An empty angle opens as Fusion shows it, "0.0 deg".
pub(crate) fn tab(app: &mut App) {
  let Some(dlg) = app.dialog.as_mut() else { return };
  if dlg.boxes_every_handle() {
    return;
  }
  let handles = dlg.handles(&app.s);
  let arrow = dlg.value_box_field().is_some();
  let fields: Vec<usize> = (0..handles.len()).filter(|k| dlg.handle_field(*k).is_some()).collect();
  // None is the box's first place: the arrow, else the first handle with a field
  let mut stops = vec![None];
  stops.extend(fields.iter().skip(usize::from(!arrow)).map(|k| Some(*k)));
  if stops.len() < 2 {
    return;
  }
  let now = stops.iter().position(|s| *s == app.manip.value_box.at_handle).unwrap_or(0);
  let next = stops[(now + 1) % stops.len()];
  if let Some(k) = next
    && handles[k].angular()
    && let Some(f) = dlg.handle_field(k)
    && f.trim().is_empty()
  {
    *f = "0.0 deg".into();
  }
  app.manip.value_box.refocus();
  app.manip.value_box.at_handle = next;
}

/// The open dialog's handles on screen: each one's head and its shaft or arc (tests press them).
#[cfg(test)]
pub(crate) fn placed_points(app: &App, project: &dyn Fn(DVec3) -> Option<Pos2>) -> Vec<Option<(Pos2, Vec<Pos2>)>> {
  let (right, _, _) = app.cam.basis();
  let handles = app.dialog.as_ref().map(|d| d.handles(&app.s)).unwrap_or_default();
  handles.iter().map(|h| place(h, right, project).map(|p| (p.head, p.line))).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `ui:manip:value-box-covers-arrow`: whichever way an arrow points, its box sits clear of the
  /// shaft and head (below and to the right of the head where it can), and inside the canvas.
  #[test]
  fn the_value_box_keeps_off_its_arrow_and_inside_the_canvas() {
    let canvas = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
    let b = Pos2::new(400.0, 300.0);
    let rect = |at: Pos2| egui::Rect::from_min_size(at - Vec2::new(0.0, BOX_SIZE.y / 2.0), BOX_SIZE);
    for k in 0..16 {
      let t = std::f32::consts::TAU * k as f32 / 16.0;
      let d = Vec2::new(t.cos(), t.sin());
      let a = b - d * 80.0;
      let r = rect(box_beside(a, b, d, canvas));
      let side = Vec2::new(-d.y, d.x);
      for q in [b, b + d * 12.0, b + side * 6.0, b - side * 6.0, a, a + (b - a) * 0.5] {
        assert!(!r.contains(q), "{d:?}: the box {r:?} covers {q:?}");
      }
      assert!(canvas.contains_rect(r));
    }
    // an arrow pointing up and to the right gets its box below and to the right of the head
    let d = Vec2::new(1.0, -1.0).normalized();
    let r = rect(box_beside(b - d * 80.0, b, d, canvas));
    assert!(r.left() > b.x && r.top() > b.y, "{r:?}");
    // one pointing up and to the left (Fillet's on a box's top front edge, its shaft running
    // below-right of the head) keeps it below-right, further out as Fusion's: about 110 px right
    // and 140 px down, not round to the left on the body
    for len in [80.0, 200.0] {
      let d = Vec2::new(-1.0, -1.2).normalized();
      let r = rect(box_beside(b - d * len, b, d, canvas));
      assert!(r.left() > b.x + 100.0 && r.center().y > b.y + 130.0, "{len}: {r:?}");
      for k in 0..=20 {
        assert!(!r.contains(b - d * len * (k as f32 / 20.0)), "{len}: the box {r:?} is on the shaft");
      }
    }
    // a head off the canvas keeps its box at the edge
    let r = rect(box_beside(Pos2::new(700.0, 300.0), Pos2::new(1500.0, 900.0), Vec2::new(0.8, 0.6), canvas));
    assert!(canvas.contains_rect(r), "{r:?}");
  }

  /// `ui:manip:draft-angle-handle`: Draft's disc is caught on its arc as well as its head; the
  /// pull arrow is never caught.
  #[test]
  fn a_disc_is_caught_on_its_arc_and_the_pull_arrow_never() {
    let right = DVec3::X;
    let disc = Handle::dial(DVec3::ZERO, DVec3::Z, DVec3::Y, Some(40.0), 0.1, "".into()).disc(0.1);
    let pull = Handle::pull(DVec3::ZERO, DVec3::Y);
    let placed: Vec<Option<Placed>> = [&disc, &pull].iter().map(|h| place(h, right, &top)).collect();
    // the arc runs a quarter turn either way of zero (screen (100, 60)): (140, 100) is on it
    assert_eq!(hit(&placed, Pos2::new(141.0, 101.0)), Some(0));
    assert_eq!(hit(&placed, Pos2::new(59.0, 100.0)), Some(0));
    // not the arc's far side, nor the pull arrow's head (PULL_PX up from the centre)
    assert_eq!(hit(&placed, Pos2::new(100.0, 140.0)), None);
    let tip = placed[1].as_ref().unwrap().head;
    assert!((tip - Pos2::new(100.0, 100.0 - PULL_PX as f32)).length() < 1e-3, "{tip:?}");
    let arrow = Handle::arrow(DVec3::ZERO, DVec3::Y, 30.0, "".into());
    let placed: Vec<Option<Placed>> = [&pull, &arrow].iter().map(|h| place(h, right, &top)).collect();
    assert_eq!(hit(&placed, Pos2::new(100.0, 70.0)), Some(1), "an arrow under the pull arrow is caught");
    assert_eq!(hit(&placed, tip), None);
  }

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
