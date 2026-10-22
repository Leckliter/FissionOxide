//! The browser's Origin folder in the canvas, as Fusion draws and picks it: the origin point, the
//! X, Y and Z axes and the XY, XZ and YZ planes. Each has its own eye under the folder's (a child
//! shows only when both are on), Fusion's plain, hovered and selected looks, and a status readout
//! ("XY Plane", "Y Axis", "Origin Point | X: 0.00 Y: 0.00 Z: 0.00 in"). Selecting one is a
//! UI-side pick, like a plane picked in the browser (`App::picked_plane`): the model's selection
//! is cleared, and a click elsewhere or Escape clears the pick while the item stays visible.

use egui::{Color32, Pos2, Stroke};
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{FeatureId, OriginPlane};
use fox_engine::render::Quad;
use serde_json::{Value, json};

use crate::App;
use crate::icons::Icon;

/// The Origin folder's children, in browser order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginItem {
  Point,
  X,
  Y,
  Z,
  Xy,
  Xz,
  Yz,
}

impl OriginItem {
  pub const ALL: [OriginItem; 7] =
    [OriginItem::Point, OriginItem::X, OriginItem::Y, OriginItem::Z, OriginItem::Xy, OriginItem::Xz, OriginItem::Yz];
  pub const AXES: [OriginItem; 3] = [OriginItem::X, OriginItem::Y, OriginItem::Z];
  pub const PLANES: [OriginItem; 3] = [OriginItem::Xy, OriginItem::Xz, OriginItem::Yz];

  fn index(self) -> usize {
    OriginItem::ALL.iter().position(|i| *i == self).unwrap_or(0)
  }

  /// The browser row's label.
  pub fn label(self) -> &'static str {
    match self {
      OriginItem::Point => "O",
      OriginItem::X => "X",
      OriginItem::Y => "Y",
      OriginItem::Z => "Z",
      OriginItem::Xy => "XY",
      OriginItem::Xz => "XZ",
      OriginItem::Yz => "YZ",
    }
  }

  /// The status bar's name for it.
  pub fn status(self) -> &'static str {
    match self {
      OriginItem::Point => "Origin Point",
      OriginItem::X => "X Axis",
      OriginItem::Y => "Y Axis",
      OriginItem::Z => "Z Axis",
      OriginItem::Xy => "XY Plane",
      OriginItem::Xz => "XZ Plane",
      OriginItem::Yz => "YZ Plane",
    }
  }

  pub fn icon(self) -> Icon {
    match self {
      OriginItem::Point => Icon::Origin,
      OriginItem::X | OriginItem::Y | OriginItem::Z => Icon::Axis,
      _ => Icon::Plane,
    }
  }

  pub fn plane(self) -> Option<OriginPlane> {
    match self {
      OriginItem::Xy => Some(OriginPlane::Xy),
      OriginItem::Xz => Some(OriginPlane::Xz),
      OriginItem::Yz => Some(OriginPlane::Yz),
      _ => None,
    }
  }

  /// A plane as `sketch.create` names it ("xy"), the form `App::picked_plane` holds.
  pub fn sketch_plane(self) -> Option<Value> {
    match self {
      OriginItem::Xy => Some(json!("xy")),
      OriginItem::Xz => Some(json!("xz")),
      OriginItem::Yz => Some(json!("yz")),
      _ => None,
    }
  }

  pub fn from_sketch_plane(v: &Value) -> Option<OriginItem> {
    OriginItem::PLANES.into_iter().find(|p| p.sketch_plane().as_ref() == Some(v))
  }

  /// An axis's world direction.
  pub fn axis(self) -> Option<DVec3> {
    match self {
      OriginItem::X => Some(DVec3::X),
      OriginItem::Y => Some(DVec3::Y),
      OriginItem::Z => Some(DVec3::Z),
      _ => None,
    }
  }

  /// An axis's own colour (the line when nobody points at it).
  fn axis_color(self) -> [u8; 3] {
    match self {
      OriginItem::X => AXIS_COLORS[0],
      OriginItem::Y => AXIS_COLORS[1],
      _ => AXIS_COLORS[2],
    }
  }
}

// ---------------------------------------------------------------- eyes

/// The item's own eye (on by default).
pub(crate) fn eye(app: &App, item: OriginItem) -> bool {
  app.origin_eyes[item.index()]
}

pub(crate) fn set_eye(app: &mut App, item: OriginItem, on: bool) {
  app.origin_eyes[item.index()] = on;
}

pub(crate) fn toggle_eye(app: &mut App, item: OriginItem) {
  let on = eye(app, item);
  set_eye(app, item, !on);
}

/// Shown in the canvas: its Object Visibility row on (Origin Planes, Origin Axes or Origin
/// Points), the folder's eye on and the item's own (Fusion's folder-and-child rule), or a dialog
/// that shows them for its picks, in a sketch as outside one.
pub(crate) fn visible(app: &App, item: OriginItem) -> bool {
  let o = &app.objects;
  let row = match item {
    OriginItem::Point => o.origin_points,
    OriginItem::X | OriginItem::Y | OriginItem::Z => o.origin_axes,
    OriginItem::Xy | OriginItem::Xz | OriginItem::Yz => o.origin_planes,
  };
  row && app.origin_shown && eye(app, item) || dialog_shows(app)
}

/// The open dialog shows the origin planes and axes for its picks (Revolve, Loft), as Fusion
/// does whatever the Origin folder's eyes say.
fn dialog_shows(app: &App) -> bool {
  app.dialog.as_ref().is_some_and(|d| d.shows_origin())
}

/// All three plane eyes are on (the folder's Show/Hide Planes).
pub(crate) fn planes_shown(app: &App) -> bool {
  OriginItem::PLANES.iter().all(|p| eye(app, *p))
}

/// All three axis eyes are on (the folder's Show/Hide Axes).
pub(crate) fn axes_shown(app: &App) -> bool {
  OriginItem::AXES.iter().all(|a| eye(app, *a))
}

pub(crate) fn set_planes(app: &mut App, on: bool) {
  for p in OriginItem::PLANES {
    set_eye(app, p, on);
  }
}

pub(crate) fn set_axes(app: &mut App, on: bool) {
  for a in OriginItem::AXES {
    set_eye(app, a, on);
  }
}

// ---------------------------------------------------------------- picking

/// The origin item picked in the browser or the canvas: a plane lives in `App::picked_plane`
/// (so Create Sketch starts on it), the point and axes in `App::picked_origin`.
pub(crate) fn picked(app: &App) -> Option<OriginItem> {
  app.picked_origin.or_else(|| app.picked_plane.as_ref().and_then(OriginItem::from_sketch_plane))
}

/// Selects the item (clearing the model's selection), or clears it when it is the one picked.
pub(crate) fn pick(app: &mut App, item: OriginItem) {
  let again = picked(app) == Some(item);
  app.s.selection.clear();
  clear(app);
  if !again {
    match item.sketch_plane() {
      Some(p) => app.picked_plane = Some(p),
      None => app.picked_origin = Some(item),
    }
  }
}

/// Forgets any picked origin item or plane (a click elsewhere, Escape, a new selection).
pub(crate) fn clear(app: &mut App) {
  app.picked_plane = None;
  app.picked_origin = None;
}

/// The status bar's readout for the picked origin item (a hovered one reads nothing, as in Fusion).
pub(crate) fn status(app: &App) -> Option<String> {
  let item = picked(app)?;
  Some(match item {
    OriginItem::Point => format!("{} | {}", item.status(), coords(app, DVec3::ZERO)),
    _ => item.status().to_string(),
  })
}

/// A coordinate as Fusion's status bar shows it: three decimals, trailing zeros trimmed down to
/// two (`1.741`, `0.00`, `2.50`).
pub(crate) fn coord(v: f64) -> String {
  let mut s = format!("{v:.3}");
  let decimals = |s: &str| s.len() - s.find('.').map_or(s.len(), |d| d + 1);
  while s.ends_with('0') && decimals(&s) > 2 {
    s.pop();
  }
  s
}

/// A point (mm) as "X: … Y: … Z: … in" in the design's units.
pub(crate) fn coords(app: &App, p: DVec3) -> String {
  let u = app.s.design.units;
  format!("X: {} Y: {} Z: {} {}", coord(u.from_mm(p.x)), coord(u.from_mm(p.y)), coord(u.from_mm(p.z)), u.symbol())
}

// ---------------------------------------------------------------- looks

/// A visible origin plane: translucent peach, as sampled on screen in Fusion.
pub(crate) const PLANE_PLAIN: [u8; 3] = [254, 228, 177];
/// A hovered origin plane: grey.
pub(crate) const PLANE_HOVER: [u8; 3] = [232, 229, 222];
/// A selected origin plane: blue.
pub(crate) const PLANE_SELECTED: [u8; 3] = [127, 174, 216];
/// The axes' own colours: pure red, green and blue.
pub(crate) const AXIS_COLORS: [[u8; 3]; 3] = [[255, 0, 0], [0, 255, 0], [0, 0, 255]];
/// How wide a plain axis draws.
pub(crate) const AXIS_WIDTH: f32 = 2.0;
/// A hovered axis: a solid blue line; selected, a deeper blue with a pale halo.
pub(crate) const AXIS_HOVER: [u8; 3] = [74, 132, 246];
pub(crate) const AXIS_SELECTED: [u8; 3] = [2, 126, 246];
/// The origin point's ring (grey, filled white) and its selected fill (blue).
pub(crate) const POINT_RING: [u8; 3] = [128, 132, 138];
pub(crate) const POINT_SELECTED: [u8; 3] = AXIS_SELECTED;
/// How wide a hovered or selected axis draws.
pub(crate) const AXIS_LIT_WIDTH: f32 = 4.0;
/// The origin point's radius on screen (a ring about 7 px across).
pub(crate) const POINT_RADIUS: f32 = 3.5;
/// How far (a fraction of the axis) a plain axis runs solid before its dash-dot.
const AXIS_SOLID: f32 = 1.0 / 3.0;
/// A plain axis's dash-dot pattern past its solid start (px): long dash, gap, short dash, gap.
const AXIS_DASHES: [f32; 4] = [10.0, 3.0, 3.0, 3.0];
/// How see-through a plane's square is: the grid and the other planes show through it.
const PLANE_ALPHA: f32 = 0.5;
/// How opaque a plane's edge is.
const EDGE_ALPHA: u8 = 200;
/// The canvas background the planes blend over, mid-way down its gradient.
const CANVAS_BG: [f32; 3] = [242.0, 243.0, 245.0];

/// A plane's fill (unmultiplied RGBA): the colour that, blended at [`PLANE_ALPHA`] over the
/// canvas background, shows as `sample` (the colours above were sampled on screen), or as near
/// as a half see-through fill gets (the peach's red is past it).
pub(crate) fn plane_rgba(sample: [u8; 3]) -> [u8; 4] {
  let c =
    |i: usize| ((sample[i] as f32 - CANVAS_BG[i] * (1.0 - PLANE_ALPHA)) / PLANE_ALPHA).round().clamp(0.0, 255.0) as u8;
  [c(0), c(1), c(2), (PLANE_ALPHA * 255.0).round() as u8]
}

fn rgb(c: [u8; 3]) -> Color32 {
  Color32::from_rgb(c[0], c[1], c[2])
}

/// How an item draws now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
  Plain,
  Hovered,
  Selected,
}

/// A browser row under the pointer that lights its item in the canvas, as Fusion does: an origin
/// item, or a construction plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowHover {
  Origin(OriginItem),
  Plane(FeatureId),
}

/// Hovered in the canvas or by its browser row.
pub(crate) fn look(app: &App, item: OriginItem) -> Look {
  let dialog_axis = app.dialog.as_ref().and_then(|d| d.origin_axis());
  if picked(app) == Some(item) || dialog_axis == Some(item) {
    Look::Selected
  } else if app.hover_origin == Some(item) || app.row_hover == Some(RowHover::Origin(item)) {
    Look::Hovered
  } else {
    Look::Plain
  }
}

/// The two world axes an origin plane spans; its square lies where both are positive.
pub(crate) fn plane_axes(plane: OriginPlane) -> (DVec3, DVec3) {
  match plane {
    OriginPlane::Xy => (DVec3::X, DVec3::Y),
    OriginPlane::Xz => (DVec3::X, DVec3::Z),
    OriginPlane::Yz => (DVec3::Y, DVec3::Z),
  }
}

/// How far off each axis an origin plane's square starts, for a square reaching `size`.
fn plane_gap(size: f64) -> f64 {
  size * crate::viewport::ORIGIN_PLANE_GAP_PX / crate::viewport::ORIGIN_PLANE_PX
}

/// The corners of an origin plane's square, as Fusion draws it: in the positive quadrant of its
/// two axes, from a small gap off each axis out to `size`, so the three squares never overlap and
/// the axes run through the gaps.
pub(crate) fn plane_corners(plane: OriginPlane, size: f64) -> [DVec3; 4] {
  let (a, b) = plane_axes(plane);
  let g = plane_gap(size);
  [a * g + b * g, a * size + b * g, a * size + b * size, a * g + b * size]
}

/// Whether a point of the plane lies in its square ([`plane_corners`]).
pub(crate) fn in_square(plane: OriginPlane, p: DVec3, size: f64) -> bool {
  let (a, b) = plane_axes(plane);
  let span = plane_gap(size)..=size;
  span.contains(&p.dot(a)) && span.contains(&p.dot(b))
}

/// Create Sketch, a primitive before its base is drawn, or Mirror waits for a plane (each offers
/// its own orange squares).
fn picking_plane(app: &App) -> bool {
  app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::PickPlane || d.picks_plane() || d.wants_plane())
}

/// The visible origin planes as see-through squares for the renderer, which hides them behind
/// bodies and blends them over the grid and each other (overlaps read darker), each with a thin
/// edge, in Fusion's plain, hovered or selected colour. While Create Sketch offers its own
/// squares, none. A construction plane whose browser row is hovered draws grey over its square.
pub(crate) fn plane_quads(app: &App) -> Vec<Quad> {
  let mut out = Vec::new();
  let size = crate::viewport::origin_plane_size(app);
  let px = size / crate::viewport::ORIGIN_PLANE_PX;
  if !picking_plane(app) {
    for item in OriginItem::PLANES {
      let Some(plane) = item.plane().filter(|_| visible(app, item)) else { continue };
      let sample = match look(app, item) {
        Look::Plain => PLANE_PLAIN,
        Look::Hovered => PLANE_HOVER,
        Look::Selected => PLANE_SELECTED,
      };
      square(&mut out, plane_corners(plane, size), sample, px);
    }
  }
  if let Some(RowHover::Plane(id)) = app.row_hover
    && app.objects.construction_planes
    && !app.s.design.feature(id).is_some_and(|f| f.suppressed)
    && let Some(pl) = app.s.model.planes.get(&id)
  {
    let half = crate::viewport::plane_half_size(app);
    square(&mut out, crate::viewport::plane_square(pl, pl.origin, half), PLANE_HOVER, px);
  }
  out
}

/// A square's fill and its edge (`px` world units wide, inside the square).
pub(crate) fn square(out: &mut Vec<Quad>, c: [DVec3; 4], sample: [u8; 3], px: f64) {
  let f = |p: DVec3| p.as_vec3().to_array();
  out.push(Quad { corners: c.map(f), color: plane_rgba(sample) });
  let [r, g, b] = sample;
  for i in 0..4 {
    let (a, z) = (c[i], c[(i + 1) % 4]);
    let inward = (c[(i + 3) % 4] - a).normalize_or_zero() * px;
    out.push(Quad { corners: [a, z, z + inward, a + inward].map(f), color: [r, g, b, EDGE_ALPHA] });
  }
}

/// A plain axis from `a` (the origin) to `b`: solid for its first stretch, then long-dash /
/// short-dash, ending in a dot.
fn plain_axis(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
  let stroke = Stroke::new(AXIS_WIDTH, color);
  let len = (b - a).length();
  if len < 1.0 {
    return;
  }
  let dir = (b - a) / len;
  let solid = len * AXIS_SOLID;
  painter.line_segment([a, a + dir * solid], stroke);
  let mut t = solid;
  let mut i = 0;
  while t < len {
    let step = AXIS_DASHES[i % 4];
    if i % 2 == 0 {
      painter.line_segment([a + dir * t, a + dir * (t + step).min(len)], stroke);
    }
    t += step;
    i += 1;
  }
  painter.circle_filled(b, AXIS_WIDTH * 1.25, color);
}

/// Draws the visible origin axes and point over the rendered scene (the planes are drawn by the
/// renderer, see [`plane_quads`], so these sit on top of them): each axis along its positive
/// direction only, pure red, green or blue, solid then dash-dot to a dot past the squares; the
/// origin a small grey ring filled white; the hovered and selected ones in Fusion's blues. Drawn
/// in a sketch too.
pub(crate) fn overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let reach = crate::viewport::origin_plane_size(app) * crate::viewport::ORIGIN_AXIS_REACH;
  for item in OriginItem::AXES.into_iter().chain([OriginItem::Point]) {
    if !visible(app, item) {
      continue;
    }
    let look = look(app, item);
    if let Some(dir) = item.axis() {
      let (Some(a), Some(b)) = (project(DVec3::ZERO), project(dir * reach)) else { continue };
      match look {
        Look::Plain => plain_axis(painter, a, b, rgb(item.axis_color())),
        Look::Hovered => {
          painter.line_segment([a, b], Stroke::new(AXIS_LIT_WIDTH, rgb(AXIS_HOVER)));
        }
        Look::Selected => {
          painter.line_segment([a, b], Stroke::new(AXIS_LIT_WIDTH * 2.5, rgb(AXIS_SELECTED).gamma_multiply(0.25)));
          painter.line_segment([a, b], Stroke::new(AXIS_LIT_WIDTH, rgb(AXIS_SELECTED)));
        }
      }
    } else {
      let Some(c) = project(DVec3::ZERO) else { continue };
      match look {
        Look::Plain => painter.circle(c, POINT_RADIUS, Color32::WHITE, Stroke::new(1.0, rgb(POINT_RING))),
        Look::Hovered => painter.circle(c, POINT_RADIUS, Color32::WHITE, Stroke::new(2.0, rgb(AXIS_HOVER))),
        Look::Selected => painter.circle_filled(c, POINT_RADIUS, rgb(POINT_SELECTED)),
      };
    }
  }
}

/// How close (points) the pointer must come to the origin point or an axis.
const PICK_PX: f32 = 6.0;

/// The visible origin item under the pointer, the point before the axes before the planes, and
/// none when a body face is nearer along the pointer's ray. None in a sketch, where they show
/// but the sketch's tools have the pointer.
pub(crate) fn under(
  app: &App,
  pos: Pos2,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> Option<OriginItem> {
  if app.s.edit_sketch.is_some() || !OriginItem::ALL.iter().any(|i| visible(app, *i)) {
    return None;
  }
  let size = crate::viewport::origin_plane_size(app);
  let reach = size * crate::viewport::ORIGIN_AXIS_REACH;
  let (o, d) = ray(pos);
  let face = fox_engine::scene::ray_hits(&app.s, o, d).first().map(|h| h.0);
  // the depth along the ray of the point nearest to the item (the origin for the point)
  let in_front = |t: f64| t > 0.0 && face.is_none_or(|f| t <= f + 1e-9);
  let origin_t = (DVec3::ZERO - o).dot(d);
  if visible(app, OriginItem::Point)
    && project(DVec3::ZERO).is_some_and(|c| (c - pos).length() <= PICK_PX)
    && in_front(origin_t)
  {
    return Some(OriginItem::Point);
  }
  for axis in OriginItem::AXES {
    if !visible(app, axis) {
      continue;
    }
    let dir = axis.axis()?;
    let (Some(a), Some(b)) = (project(DVec3::ZERO), project(dir * reach)) else { continue };
    let ab = b - a;
    let len2 = ab.length_sq();
    let u = if len2 > 1e-6 { ((pos - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    if (a + ab * u - pos).length() <= PICK_PX && in_front(((dir * (reach * u as f64)) - o).dot(d)) {
      return Some(axis);
    }
  }
  let mut best: Option<(f64, OriginItem)> = None;
  for item in OriginItem::PLANES {
    let Some(plane) = item.plane().filter(|_| visible(app, item)) else { continue };
    let (a, b) = plane_axes(plane);
    let Some(hit) = fox_engine::render::ray_plane(o, d, DVec3::ZERO, a.cross(b)) else { continue };
    let t = (hit - o).dot(d);
    if in_front(t) && in_square(plane, hit, size) && best.is_none_or(|(bt, _)| t < bt) {
      best = Some((t, item));
    }
  }
  best.map(|(_, i)| i)
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, Rect, Vec2};
  use fox_engine::{Sel, Session};

  use super::*;

  fn app() -> App {
    let mut app = App::with_session(Session::default());
    app.canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(1456.0, 800.0));
    app
  }

  #[test]
  fn rows_follow_fusion() {
    assert_eq!(OriginItem::ALL.map(|i| i.label()), ["O", "X", "Y", "Z", "XY", "XZ", "YZ"]);
    assert_eq!(OriginItem::Xz.status(), "XZ Plane");
    assert_eq!(OriginItem::Y.status(), "Y Axis");
    assert_eq!(OriginItem::Point.status(), "Origin Point");
    assert_eq!(OriginItem::from_sketch_plane(&json!("yz")), Some(OriginItem::Yz));
    assert_eq!(OriginItem::from_sketch_plane(&json!({"feature": 3})), None);
    assert_eq!(OriginItem::Z.axis(), Some(DVec3::Z));
    assert_eq!(
      [OriginItem::Point.icon(), OriginItem::X.icon(), OriginItem::Xy.icon()],
      [Icon::Origin, Icon::Axis, Icon::Plane]
    );
  }

  /// A child shows only when its eye and the folder's are both on; the eyes start on and the
  /// folder off, so nothing shows until the folder is shown.
  #[test]
  fn a_child_shows_only_with_its_eye_and_the_folders() {
    let mut app = app();
    assert!(OriginItem::ALL.iter().all(|i| eye(&app, *i)), "the children's eyes start on");
    assert!(!app.origin_shown && OriginItem::ALL.iter().all(|i| !visible(&app, *i)));
    app.origin_shown = true;
    assert!(OriginItem::ALL.iter().all(|i| visible(&app, *i)));
    toggle_eye(&mut app, OriginItem::Y);
    assert!(!visible(&app, OriginItem::Y) && visible(&app, OriginItem::X) && !axes_shown(&app));
    set_planes(&mut app, false);
    assert!(!planes_shown(&app) && !visible(&app, OriginItem::Xy) && visible(&app, OriginItem::Point));
    set_planes(&mut app, true);
    set_axes(&mut app, true);
    assert!(planes_shown(&app) && axes_shown(&app));
    // Object Visibility's Origin Planes, Origin Axes and Origin Points rows hide their items; an
    // open sketch doesn't
    app.objects.origin_planes = false;
    assert!(!visible(&app, OriginItem::Xy) && visible(&app, OriginItem::X) && visible(&app, OriginItem::Point));
    app.objects.origin_axes = false;
    app.objects.origin_points = false;
    assert!(OriginItem::ALL.iter().all(|i| !visible(&app, *i)));
    app.objects.show_origin();
    app.create_sketch(json!("xy"));
    assert!(OriginItem::ALL.iter().all(|i| visible(&app, *i)), "shown in a sketch too");
    assert_eq!(plane_quads(&app).len(), 15);
  }

  /// Each square lies in the positive quadrant of its plane's two axes, a small gap off each, so
  /// the three never overlap; they are see-through, in the plain, hovered (in the canvas or by
  /// the browser row) and selected colours, and Create Sketch replaces them with its own.
  #[test]
  fn planes_sit_in_their_positive_quadrant() {
    let mut app = app();
    app.origin_shown = true;
    let size = crate::viewport::origin_plane_size(&app);
    for item in OriginItem::PLANES {
      let plane = item.plane().unwrap();
      let (a, b) = plane_axes(plane);
      for c in plane_corners(plane, size) {
        assert!(c.dot(a) > 0.0 && c.dot(b) > 0.0 && c.dot(a.cross(b)).abs() < 1e-12, "{item:?} {c:?}");
        assert!(c.dot(a) <= size + 1e-9 && c.dot(b) <= size + 1e-9);
      }
      assert!(in_square(plane, (a + b) * size * 0.5, size));
      assert!(!in_square(plane, -(a + b) * size * 0.5, size), "{item:?}: not the negative quadrant");
      assert!(!in_square(plane, a * size * 0.5, size), "{item:?}: the gap along the axis");
    }
    let fills = |app: &App| plane_quads(app).into_iter().step_by(5).map(|q| q.color).collect::<Vec<_>>();
    assert_eq!(fills(&app), vec![plane_rgba(PLANE_PLAIN); 3]);
    assert!(plane_rgba(PLANE_PLAIN)[3] < 160, "see-through");
    app.hover_origin = Some(OriginItem::Xz);
    assert_eq!(fills(&app)[1], plane_rgba(PLANE_HOVER));
    app.hover_origin = None;
    app.row_hover = Some(RowHover::Origin(OriginItem::Yz));
    assert_eq!(fills(&app)[2], plane_rgba(PLANE_HOVER), "the browser row lights the plane");
    assert_eq!(look(&app, OriginItem::Yz), Look::Hovered);
    pick(&mut app, OriginItem::Xy);
    assert_eq!(fills(&app)[0], plane_rgba(PLANE_SELECTED));
    clear(&mut app);
    app.new_sketch();
    assert!(picking_plane(&app));
    assert!(plane_quads(&app).is_empty(), "Create Sketch offers its own squares");
  }

  /// Hovering a construction plane's browser row draws its square grey in the canvas.
  #[test]
  fn a_construction_row_hover_lights_its_plane() {
    let mut app = app();
    app.run("plane.offset", json!({"plane": "xy", "offset": 10}));
    let id = *app.s.model.planes.keys().next().expect("a construction plane");
    assert!(plane_quads(&app).is_empty());
    app.row_hover = Some(RowHover::Plane(id));
    let quads = plane_quads(&app);
    assert_eq!(quads.len(), 5);
    assert_eq!(quads[0].color, plane_rgba(PLANE_HOVER));
    assert!(quads[0].corners.iter().all(|c| (c[2] - 10.0).abs() < 1e-4), "on the plane");
    app.objects.construction_planes = false;
    assert!(plane_quads(&app).is_empty(), "not while construction geometry is hidden");
  }

  /// Picking an item clears the model's selection, picking it again clears the pick, and the
  /// status bar names it; a plane pick is the browser's picked plane, so Create Sketch starts on it.
  #[test]
  fn picks_toggle_and_read_out() {
    let mut app = app();
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let body = app.s.model.bodies[0].id;
    app.s.selection = vec![Sel::Body { body }];
    pick(&mut app, OriginItem::Xy);
    assert!(app.s.selection.is_empty());
    assert_eq!(picked(&app), Some(OriginItem::Xy));
    assert_eq!(app.picked_plane, Some(json!("xy")), "a picked plane is the browser's picked plane");
    assert_eq!(status(&app).as_deref(), Some("XY Plane"));
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("XY Plane"));
    pick(&mut app, OriginItem::Y);
    assert_eq!((picked(&app), app.picked_plane.clone()), (Some(OriginItem::Y), None));
    assert_eq!(status(&app).as_deref(), Some("Y Axis"));
    pick(&mut app, OriginItem::Y);
    assert_eq!(picked(&app), None, "picking it again clears it");
    assert_eq!(status(&app), None);
    pick(&mut app, OriginItem::Point);
    assert_eq!(status(&app).as_deref(), Some("Origin Point | X: 0.00 Y: 0.00 Z: 0.00 mm"));
    // hovered, with nothing picked, the status reads nothing (ui-status-readouts)
    clear(&mut app);
    app.hover_origin = Some(OriginItem::Xz);
    assert_eq!(status(&app), None);
    assert_eq!(crate::panels::selection_text(&app), None);
    // a model selection wins over a hovered origin item
    app.s.selection = vec![Sel::Body { body }];
    assert_eq!(crate::panels::selection_text(&app), Some(app.s.body_name(body)));
    // a picked plane starts Create Sketch on it
    app.s.selection.clear();
    pick(&mut app, OriginItem::Xz);
    app.new_sketch();
    assert!(app.s.edit_sketch.is_some() && picked(&app).is_none());
  }

  #[test]
  fn coordinates_read_as_fusions() {
    assert_eq!(coord(1.741), "1.741");
    assert_eq!(coord(0.0), "0.00");
    assert_eq!(coord(-1.974), "-1.974");
    assert_eq!(coord(2.5), "2.50");
    assert_eq!(coord(12.3456), "12.346");
    let mut app = app();
    app.s.design.units = fox_engine::model::LengthUnit::In;
    assert_eq!(coords(&app, DVec3::new(25.4, 0.0, -12.7)), "X: 1.00 Y: 0.00 Z: -0.50 in");
  }

  /// A plane's fill blended over the canvas shows the colour sampled in Fusion (the peach's red
  /// within a few levels: a half see-through fill can't reach it).
  #[test]
  fn plane_fills_blend_to_the_sampled_colours() {
    for sample in [PLANE_PLAIN, PLANE_HOVER, PLANE_SELECTED] {
      let f = plane_rgba(sample);
      let a = f[3] as f32 / 255.0;
      for i in 0..3 {
        let shown = f[i] as f32 * a + CANVAS_BG[i] * (1.0 - a);
        assert!((shown - sample[i] as f32).abs() <= 6.0, "{sample:?} channel {i}: {shown}");
      }
    }
  }

  /// With the folder shown, the pointer over the origin finds the point, along an axis the axis,
  /// and inside a plane's square the plane; a face in front of the origin hides them.
  #[test]
  fn hit_testing_prefers_the_point_then_axes_then_planes() {
    let mut app = app();
    let (w, h) = (1456u32, 800u32);
    let cam = app.cam;
    let project = move |p: DVec3| cam.project(p, w, h).map(|(x, y, _)| Pos2::new(x as f32, y as f32));
    let ray = move |p: Pos2| cam.ray(p.x as f64, p.y as f64, w, h);
    let at = |p: DVec3| project(p).unwrap();
    assert_eq!(under(&app, at(DVec3::ZERO), &project, &ray), None, "the folder is hidden");
    app.origin_shown = true;
    assert_eq!(under(&app, at(DVec3::ZERO), &project, &ray), Some(OriginItem::Point));
    let reach = crate::viewport::origin_plane_size(&app) * crate::viewport::ORIGIN_AXIS_REACH;
    assert_eq!(under(&app, at(DVec3::X * reach * 0.6), &project, &ray), Some(OriginItem::X));
    assert_eq!(under(&app, at(DVec3::Z * reach * 0.6), &project, &ray), Some(OriginItem::Z));
    // the nearest plane along the ray wins: with only XY shown, a point in its square finds it
    let half = crate::viewport::origin_plane_size(&app);
    set_eye(&mut app, OriginItem::Xz, false);
    set_eye(&mut app, OriginItem::Yz, false);
    assert_eq!(under(&app, at(DVec3::new(half * 0.5, half * 0.5, 0.0)), &project, &ray), Some(OriginItem::Xy));
    assert_eq!(under(&app, at(DVec3::new(half * 3.0, half * 3.0, 0.0)), &project, &ray), None, "outside the square");
    assert_eq!(under(&app, at(DVec3::new(-half * 0.5, -half * 0.5, 0.0)), &project, &ray), None, "the other quadrant");
    set_planes(&mut app, true);
    assert!(
      under(&app, at(DVec3::new(half * 0.5, half * 0.5, 0.0)), &project, &ray).is_some_and(|p| p.plane().is_some())
    );
    // a hidden axis isn't picked, and the squares leave a gap along it
    set_eye(&mut app, OriginItem::X, false);
    assert_eq!(under(&app, at(DVec3::X * reach * 0.6), &project, &ray), None);
    // a body over the origin is nearer than the origin point
    app.run("box", json!({"width": 100, "depth": 100, "height": 100, "at": [0, 0]}));
    let hits = fox_engine::scene::ray_hits(&app.s, ray(at(DVec3::ZERO)).0, ray(at(DVec3::ZERO)).1);
    assert!(!hits.is_empty(), "the box covers the origin");
    assert_eq!(under(&app, at(DVec3::ZERO), &project, &ray), None);
    // in a sketch they show but the sketch's tools have the pointer
    app.run("feature.delete", json!({"feature": app.s.design.features[0].id.0}));
    assert!(under(&app, at(DVec3::ZERO), &project, &ray).is_some());
    app.create_sketch(json!("xz"));
    assert_eq!(under(&app, at(DVec3::ZERO), &project, &ray), None);
  }
}
