//! Construction planes in the canvas and the browser's Construction folder, as Fusion draws and
//! picks them: a translucent peach square (the origin planes' colour) twice an origin plane's
//! size, the same on screen whatever the zoom, centred on the plane's origin; grey under the
//! pointer, blue when selected, with the plane's name as the status readout ("Plane1"). The
//! folder and each row have an eye (a plane shows only when both are on, V toggles the row's);
//! a hidden plane stays drawn while it is selected. Selecting one is a UI-side pick, as an origin
//! plane's is (`App::picked_plane` holds `{feature}`), so Create Sketch starts on it.

use egui::{Color32, Pos2, Stroke};
use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{FeatureId, FeatureKind};
use serde_json::json;

use crate::App;
use crate::dialogs::Kind;
use crate::origin::{Look, PLANE_HOVER, PLANE_PLAIN, PLANE_SELECTED, plane_fill};

/// A plane's edge: a faint darker peach, a darker grey under the pointer, a darker blue selected.
const EDGE_PLAIN: [u8; 3] = [226, 196, 142];
const EDGE_HOVER: [u8; 3] = [186, 182, 174];
const EDGE_SELECTED: [u8; 3] = [70, 126, 186];

/// The design's construction planes, in timeline order with their names: committed ones only
/// (not the plane an open Offset Plane is previewing, which joins the browser on OK).
pub(crate) fn planes(app: &App) -> Vec<(FeatureId, String)> {
  let preview = app.dialog.as_ref().and_then(|d| d.previewed_plane());
  app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::ConstructionPlane { .. }) && Some(f.id) != preview)
    .map(|f| (f.id, f.name.clone()))
    .collect()
}

/// Whether the feature is a construction plane, axis or point.
pub(crate) fn is_construction(app: &App, id: FeatureId) -> bool {
  app.s.design.feature(id).is_some_and(|f| {
    matches!(
      f.kind,
      FeatureKind::ConstructionPlane { .. }
        | FeatureKind::ConstructionAxis { .. }
        | FeatureKind::ConstructionPoint { .. }
    )
  })
}

/// The row's own eye: on unless the plane (axis, point) was hidden.
pub(crate) fn eye(app: &App, id: FeatureId) -> bool {
  app.s.design.feature(id).is_none_or(|f| f.visible != Some(false))
}

/// Flips a construction plane's, axis's or point's own eye (its row's eye, Show/Hide, V).
pub(crate) fn toggle_eye(app: &mut App, id: FeatureId) {
  let on = eye(app, id);
  app.run("construction.visible", json!({"feature": id.0, "visible": !on}));
}

/// Shown in the canvas: Object Visibility ▸ Construction (and its Planes) on, the folder's eye
/// and the plane's own on, and the plane built and not suppressed.
pub(crate) fn visible(app: &App, id: FeatureId) -> bool {
  app.objects.construction
    && app.objects.construction_planes
    && app.construction_shown
    && eye(app, id)
    && app.s.model.planes.contains_key(&id)
    && !app.s.design.feature(id).is_some_and(|f| f.suppressed)
}

/// The construction plane picked in the browser or the canvas.
pub(crate) fn picked(app: &App) -> Option<FeatureId> {
  app.picked_plane.as_ref()?.get("feature")?.as_u64().map(|f| FeatureId(f as u32))
}

/// How a plane draws now.
pub(crate) fn look(app: &App, id: FeatureId) -> Look {
  if picked(app) == Some(id) {
    Look::Selected
  } else if app.hover_cplane == Some(id) {
    Look::Hovered
  } else {
    Look::Plain
  }
}

/// The status bar's readout: the picked plane's name, else the hovered one's.
pub(crate) fn status(app: &App) -> Option<String> {
  let id = picked(app).or(app.hover_cplane)?;
  app.s.design.feature(id).map(|f| f.name.clone())
}

/// The planes drawn this frame: the visible ones and a picked hidden one (it stays until it is
/// deselected); none while Create Sketch offers its own orange planes, and not the one an open
/// Offset Plane previews (the viewport fills that itself).
fn drawn(app: &App) -> Vec<(FeatureId, Plane)> {
  if app.dialog.as_ref().is_some_and(|d| d.kind == Kind::PickPlane) {
    return Vec::new();
  }
  let preview = app.dialog.as_ref().and_then(|d| d.previewed_plane());
  let picked = picked(app);
  app
    .s
    .model
    .planes
    .iter()
    .filter(|(id, _)| Some(**id) != preview)
    .filter(|(id, _)| {
      visible(app, **id) || picked == Some(**id) && !app.s.design.feature(**id).is_some_and(|f| f.suppressed)
    })
    .map(|(id, pl)| (*id, *pl))
    .collect()
}

/// Draws the construction planes over the rendered scene in Fusion's plain, hovered and selected
/// looks.
pub(crate) fn overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let half = crate::viewport::plane_half_size(app);
  for (id, pl) in drawn(app) {
    let corners = crate::viewport::plane_square(&pl, pl.origin, half);
    let Some(pts) = corners.iter().map(|p| project(*p)).collect::<Option<Vec<Pos2>>>() else { continue };
    let (sample, edge) = match look(app, id) {
      Look::Plain => (PLANE_PLAIN, EDGE_PLAIN),
      Look::Hovered => (PLANE_HOVER, EDGE_HOVER),
      Look::Selected => (PLANE_SELECTED, EDGE_SELECTED),
    };
    let [r, g, b] = edge;
    painter.add(egui::Shape::convex_polygon(pts, plane_fill(sample), Stroke::new(1.0, Color32::from_rgb(r, g, b))));
  }
}

/// The visible construction plane under the pointer, the nearest along its ray, and none when a
/// body face is nearer.
pub(crate) fn under(app: &App, pos: Pos2, ray: &dyn Fn(Pos2) -> (DVec3, DVec3)) -> Option<FeatureId> {
  let planes: Vec<(FeatureId, Plane)> = drawn(app).into_iter().filter(|(id, _)| visible(app, *id)).collect();
  if planes.is_empty() {
    return None;
  }
  let half = crate::viewport::plane_half_size(app);
  let (o, d) = ray(pos);
  let mut best: Option<(f64, FeatureId)> = None;
  for (id, pl) in planes {
    let Some(hit) = fox_engine::render::ray_plane(o, d, pl.origin, pl.normal()) else { continue };
    let t = (hit - o).dot(d);
    let [u, v] = pl.to_local(hit);
    if t > 0.0 && u.abs() <= half && v.abs() <= half && best.is_none_or(|(bt, _)| t < bt) {
      best = Some((t, id));
    }
  }
  let (t, id) = best?;
  let face = fox_engine::scene::ray_hits(&app.s, o, d).first().map(|h| h.0);
  face.is_none_or(|f| t <= f + 1e-9).then_some(id)
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, Rect, Vec2};
  use fox_engine::Session;
  use fox_engine::kernel::glam::DVec3;
  use serde_json::json;

  use super::*;

  fn app() -> App {
    let mut app = App::with_session(Session::default());
    app.canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(1456.0, 800.0));
    app
  }

  fn offset(app: &mut App, mm: f64) -> FeatureId {
    let v = app.run("plane.offset", json!({"plane": "xy", "offset": mm})).expect("plane.offset");
    FeatureId(v["feature"].as_u64().unwrap() as u32)
  }

  /// A construction plane is twice an origin plane's square, the same size on screen whatever the
  /// model (an empty design shows it too), and the scene leaves it to the overlay.
  #[test]
  fn planes_are_twice_an_origin_square_on_screen() {
    let mut app = app();
    let id = offset(&mut app, 25.0);
    let origin = crate::viewport::origin_plane_half_size(&app);
    assert!((crate::viewport::plane_half_size(&app) - 2.0 * origin).abs() < 1e-9);
    assert_eq!(drawn(&app).len(), 1, "drawn in an empty design");
    assert!(crate::viewport::scene_options(&mut app).planes_drawn_by_caller);
    // a big body doesn't grow it
    app.run("box", json!({"width": 2000, "depth": 2000, "height": 10}));
    assert!((crate::viewport::plane_half_size(&app) - 2.0 * origin).abs() < 1e-9);
    // zooming out keeps it the same on screen: bigger in the world
    app.cam.distance *= 2.0;
    assert!(crate::viewport::plane_half_size(&app) > 3.0 * origin);
    assert_eq!(look(&app, id), Look::Plain);
  }

  /// The pointer over a plane's square finds it (grey, its name in the status bar); a pick turns
  /// it blue and reads out its name; hidden, it isn't picked but stays drawn while selected.
  #[test]
  fn hover_pick_and_hide() {
    let mut app = app();
    let id = offset(&mut app, 25.0);
    let (w, h) = (1456u32, 800u32);
    let cam = app.cam;
    let project = move |p: DVec3| cam.project(p, w, h).map(|(x, y, _)| Pos2::new(x as f32, y as f32));
    let ray = move |p: Pos2| cam.ray(p.x as f64, p.y as f64, w, h);
    let half = crate::viewport::plane_half_size(&app);
    let inside = project(DVec3::new(half * 0.5, half * 0.5, 25.0)).unwrap();
    let outside = project(DVec3::new(half * 3.0, half * 3.0, 25.0)).unwrap();
    assert_eq!(under(&app, inside, &ray), Some(id));
    assert_eq!(under(&app, outside, &ray), None);
    app.hover_cplane = Some(id);
    assert_eq!(look(&app, id), Look::Hovered);
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("Plane1"));
    app.hover_cplane = None;
    crate::panels::pick_plane(&mut app, json!({"feature": id.0}));
    assert_eq!((picked(&app), look(&app, id)), (Some(id), Look::Selected));
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("Plane1"));

    // V hides it: still drawn (blue) while picked, gone once deselected, and not under the pointer
    app.toggle_visibility();
    assert!(!eye(&app, id) && picked(&app) == Some(id));
    assert_eq!(drawn(&app).len(), 1);
    assert_eq!(under(&app, inside, &ray), None);
    crate::origin::clear(&mut app);
    assert!(drawn(&app).is_empty());
    // the folder's eye hides every plane; Object Visibility too
    app.run("construction.visible", json!({"feature": id.0, "visible": true}));
    assert_eq!(drawn(&app).len(), 1);
    app.construction_shown = false;
    assert!(drawn(&app).is_empty());
    app.construction_shown = true;
    app.objects.construction = false;
    assert!(drawn(&app).is_empty());
  }
}
