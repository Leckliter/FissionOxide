//! The browser's Origin folder in the canvas, as Fusion draws and picks it: the origin point, the
//! X, Y and Z axes and the XY, XZ and YZ planes. Each has its own eye under the folder's (a child
//! shows only when both are on), Fusion's plain, hovered and selected looks, and a status readout
//! ("XY Plane", "Y Axis", "Origin Point | X: 0.00 Y: 0.00 Z: 0.00 in"). Selecting one is a
//! UI-side pick, like a plane picked in the browser (`App::picked_plane`): the model's selection
//! is cleared, and a click elsewhere or Escape clears the pick while the item stays visible.

use egui::{Color32, Pos2, Stroke};
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::OriginPlane;
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

  /// An axis's own colour (the dashed line when nobody points at it).
  fn axis_color(self) -> [u8; 4] {
    match self {
      OriginItem::X => fox_engine::scene::ORIGIN_X_COLOR,
      OriginItem::Y => fox_engine::scene::ORIGIN_Y_COLOR,
      _ => fox_engine::scene::ORIGIN_Z_COLOR,
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
/// that shows them for its picks; and no sketch being edited.
pub(crate) fn visible(app: &App, item: OriginItem) -> bool {
  let o = &app.objects;
  let row = match item {
    OriginItem::Point => o.origin_points,
    OriginItem::X | OriginItem::Y | OriginItem::Z => o.origin_axes,
    OriginItem::Xy | OriginItem::Xz | OriginItem::Yz => o.origin_planes,
  };
  (row && app.origin_shown && eye(app, item) || dialog_shows(app)) && app.s.edit_sketch.is_none()
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
/// A hovered axis: a solid blue line; selected, a deeper blue with a pale halo.
pub(crate) const AXIS_HOVER: [u8; 3] = [74, 132, 246];
pub(crate) const AXIS_SELECTED: [u8; 3] = [2, 126, 246];
/// The origin point's ring (grey, hollow) and its selected fill (blue).
pub(crate) const POINT_RING: [u8; 3] = [128, 132, 138];
pub(crate) const POINT_SELECTED: [u8; 3] = AXIS_SELECTED;
/// How wide a hovered or selected axis draws, and a plane's edge.
pub(crate) const AXIS_LIT_WIDTH: f32 = 4.0;
/// The origin point's radius on screen.
pub(crate) const POINT_RADIUS: f32 = 4.5;
/// How see-through a plane's square is.
const PLANE_ALPHA: f32 = 230.0 / 255.0;
/// The canvas background the planes blend over, mid-way down its gradient.
const CANVAS_BG: [f32; 3] = [242.0, 243.0, 245.0];

/// A plane's fill: the colour that, blended at [`PLANE_ALPHA`] over the canvas background, shows
/// as `sample` (the colours above were sampled on screen).
pub(crate) fn plane_fill(sample: [u8; 3]) -> Color32 {
  let c =
    |i: usize| ((sample[i] as f32 - CANVAS_BG[i] * (1.0 - PLANE_ALPHA)) / PLANE_ALPHA).round().clamp(0.0, 255.0) as u8;
  Color32::from_rgba_unmultiplied(c(0), c(1), c(2), (PLANE_ALPHA * 255.0).round() as u8)
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

pub(crate) fn look(app: &App, item: OriginItem) -> Look {
  let dialog_axis = app.dialog.as_ref().and_then(|d| d.origin_axis());
  if picked(app) == Some(item) || dialog_axis == Some(item) {
    Look::Selected
  } else if app.hover_origin == Some(item) {
    Look::Hovered
  } else {
    Look::Plain
  }
}

/// The corners of a plane's square, `half` either side of the origin.
fn plane_corners(app: &App, plane: OriginPlane, half: f64) -> [DVec3; 4] {
  let pl = plane.plane(app.s.design.frames());
  crate::viewport::plane_square(&pl, pl.origin, half)
}

/// Draws the visible origin items over the rendered scene: planes as translucent squares, axes
/// as dashed lines in their colours, the origin as a hollow ring; the hovered and selected ones
/// in Fusion's grey and blue. Nothing while Create Sketch offers its orange planes instead.
pub(crate) fn overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let half = crate::viewport::origin_plane_half_size(app);
  let reach = half * crate::viewport::ORIGIN_AXIS_REACH;
  for item in OriginItem::ALL {
    if !visible(app, item) {
      continue;
    }
    let look = look(app, item);
    if let Some(plane) = item.plane() {
      let Some(pts) = plane_corners(app, plane, half).iter().map(|p| project(*p)).collect::<Option<Vec<Pos2>>>() else {
        continue;
      };
      let sample = match look {
        Look::Plain => PLANE_PLAIN,
        Look::Hovered => PLANE_HOVER,
        Look::Selected => PLANE_SELECTED,
      };
      painter.add(egui::Shape::convex_polygon(pts, plane_fill(sample), Stroke::new(1.0, rgb(sample))));
    } else if let Some(dir) = item.axis() {
      let (Some(a), Some(b)) = (project(DVec3::ZERO), project(dir * reach)) else { continue };
      match look {
        Look::Plain => {
          let [r, g, bl, _] = item.axis_color();
          painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.5, Color32::from_rgb(r, g, bl)), 4.0, 3.0));
        }
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
        Look::Plain => {
          painter.circle(c, POINT_RADIUS, Color32::from_white_alpha(160), Stroke::new(1.0, rgb(POINT_RING)))
        }
        Look::Hovered => painter.circle(c, POINT_RADIUS, Color32::WHITE, Stroke::new(1.5, rgb(POINT_RING))),
        Look::Selected => painter.circle_filled(c, POINT_RADIUS, rgb(POINT_SELECTED)),
      };
    }
  }
}

/// How close (points) the pointer must come to the origin point or an axis.
const PICK_PX: f32 = 6.0;

/// The visible origin item under the pointer, the point before the axes before the planes, and
/// none when a body face is nearer along the pointer's ray.
pub(crate) fn under(
  app: &App,
  pos: Pos2,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> Option<OriginItem> {
  if !OriginItem::ALL.iter().any(|i| visible(app, *i)) {
    return None;
  }
  let half = crate::viewport::origin_plane_half_size(app);
  let reach = half * crate::viewport::ORIGIN_AXIS_REACH;
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
    if !visible(app, item) {
      continue;
    }
    let pl = item.plane()?.plane(app.s.design.frames());
    let Some(hit) = fox_engine::render::ray_plane(o, d, pl.origin, pl.normal()) else { continue };
    let t = (hit - o).dot(d);
    let [u, v] = pl.to_local(hit);
    if in_front(t) && u.abs() <= half && v.abs() <= half && best.is_none_or(|(bt, _)| t < bt) {
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
    // Object Visibility's Origin Planes, Origin Axes and Origin Points rows hide their items; so
    // does an open sketch
    app.objects.origin_planes = false;
    assert!(!visible(&app, OriginItem::Xy) && visible(&app, OriginItem::X) && visible(&app, OriginItem::Point));
    app.objects.origin_axes = false;
    app.objects.origin_points = false;
    assert!(OriginItem::ALL.iter().all(|i| !visible(&app, *i)));
    app.objects.show_origin();
    app.create_sketch(json!("xy"));
    assert!(!visible(&app, OriginItem::Xy));
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

  /// A plane's fill blended over the canvas shows the colour sampled in Fusion.
  #[test]
  fn plane_fills_blend_to_the_sampled_colours() {
    for sample in [PLANE_PLAIN, PLANE_HOVER, PLANE_SELECTED] {
      let f = plane_fill(sample);
      let a = f.a() as f32 / 255.0;
      // Color32 stores premultiplied channels: what shows is the channel plus the background's rest
      for (i, c) in [f.r(), f.g(), f.b()].into_iter().enumerate() {
        let shown = c as f32 + CANVAS_BG[i] * (1.0 - a);
        assert!((shown - sample[i] as f32).abs() <= 1.5, "{sample:?} channel {i}: {shown}");
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
    let reach = crate::viewport::origin_plane_half_size(&app) * crate::viewport::ORIGIN_AXIS_REACH;
    assert_eq!(under(&app, at(DVec3::X * reach * 0.6), &project, &ray), Some(OriginItem::X));
    assert_eq!(under(&app, at(DVec3::Z * reach * 0.6), &project, &ray), Some(OriginItem::Z));
    // the nearest plane along the ray wins: with only XY shown, a point in its square finds it
    let half = crate::viewport::origin_plane_half_size(&app);
    set_eye(&mut app, OriginItem::Xz, false);
    set_eye(&mut app, OriginItem::Yz, false);
    assert_eq!(under(&app, at(DVec3::new(half * 0.5, half * 0.5, 0.0)), &project, &ray), Some(OriginItem::Xy));
    assert_eq!(under(&app, at(DVec3::new(half * 3.0, half * 3.0, 0.0)), &project, &ray), None, "outside the square");
    set_planes(&mut app, true);
    assert!(
      under(&app, at(DVec3::new(half * 0.5, half * 0.5, 0.0)), &project, &ray).is_some_and(|p| p.plane().is_some())
    );
    // a hidden axis isn't picked: a plane behind it is
    set_eye(&mut app, OriginItem::X, false);
    assert!(under(&app, at(DVec3::X * reach * 0.6), &project, &ray).is_some_and(|p| p.plane().is_some()));
    // a body over the origin is nearer than the origin point
    app.run("box", json!({"width": 100, "depth": 100, "height": 100, "at": [0, 0]}));
    let hits = fox_engine::scene::ray_hits(&app.s, ray(at(DVec3::ZERO)).0, ray(at(DVec3::ZERO)).1);
    assert!(!hits.is_empty(), "the box covers the origin");
    assert_eq!(under(&app, at(DVec3::ZERO), &project, &ray), None);
  }
}
