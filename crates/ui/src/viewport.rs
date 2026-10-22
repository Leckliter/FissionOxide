//! The 3D viewport: rendering, camera navigation, picking, sketch tools, dimension labels and the
//! ViewCube.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use fox_engine::Sel;
use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::FeatureId;
use fox_engine::render::{self, ray_plane};
use fox_engine::scene::{Pick, ViewOptions};
use fox_engine::sketch::{ConstraintKind, CurveKind, Ellipse2, PointId, Sketch};
use serde_json::json;

use crate::preferences::NavAction;
use crate::{App, RenderKey};

/// Screen-space snap radius for sketch points (points, logical pixels).
const SNAP_PX: f32 = 9.0;
/// Selecting in a sketch, a curve takes the pointer within this many logical pixels of its
/// stroke (Fusion needs about one), while points keep the snap radius.
const CURVE_PICK_PX: f32 = 1.0;
/// Lines within this angle of horizontal/vertical snap to it (degrees).
const HV_SNAP_DEG: f64 = 2.5;
/// The cursor lines up with a sketch point within this many logical pixels (inference lines).
const ALIGN_PX: f64 = 6.0;
/// Max rendered pixels (keeps software rendering interactive on HiDPI screens).
const MAX_PIXELS: f32 = 3_000_000.0;

/// Navigation bar mode: while set, a left-drag orbits, pans or zooms instead of selecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
  Orbit,
  Pan,
  Zoom,
  /// Drag a box to zoom into it (once).
  ZoomWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
  Select,
  Line,
  Rect,
  CenterRect,
  Circle,
  Arc3,
  CenterArc,
  Polygon,
  Slot,
  Point,
  Dimension,
  Spline,
  Trim,
  Extend,
  Rect3,
  Circle2P,
  Circle3P,
  TangentArc,
  CircPolygon,
  EdgePolygon,
  OverallSlot,
  CenterSlot,
  ArcSlot3,
  ArcSlotCenter,
  ControlSpline,
  Ellipse,
  Conic,
  MidpointLine,
  Break,
  Circle2T,
  Circle3T,
  Blend,
  /// Sketch Fillet: a pick-pick tool (no dialog), applied on the second pick.
  Fillet,
  /// Equal Distance Chamfer.
  Chamfer,
  /// Distance and Angle Chamfer.
  ChamferAngle,
  /// Two Distance Chamfer.
  Chamfer2,
  /// A sketch constraint as Fusion's pick command (see [`crate::constrain`]): the constraint's
  /// kind as `sketch.constrain` names it ("hv" for Horizontal/Vertical).
  Constrain(&'static str),
}

impl Tool {
  /// Tools that click curves (where on the curve matters) rather than place points.
  pub fn picks_curves(self) -> bool {
    matches!(
      self,
      Tool::Trim
        | Tool::Extend
        | Tool::Break
        | Tool::Circle2T
        | Tool::Circle3T
        | Tool::Blend
        | Tool::Fillet
        | Tool::Chamfer
        | Tool::ChamferAngle
        | Tool::Chamfer2
    )
  }

  /// The chamfer tools, in the order of Fusion's Feature Options row.
  pub fn is_chamfer(self) -> bool {
    matches!(self, Tool::Chamfer | Tool::ChamferAngle | Tool::Chamfer2)
  }
}

/// The window box (left to right), as Fusion draws it: a peach fill with a solid purple-grey
/// 1 px border.
pub const WINDOW_BOX: (Color32, Color32) = (Color32::from_rgb(253, 223, 185), Color32::from_rgb(170, 152, 195));
/// The crossing box (right to left): a pale yellow fill with a dashed dark border.
pub const CROSSING_BOX: (Color32, Color32) = (Color32::from_rgb(255, 253, 202), Color32::from_rgb(55, 61, 47));

#[derive(Debug, Clone, Copy)]
pub enum Drag {
  Orbit,
  Pan,
  Zoom,
  /// Dragging a sketch point; the design before the drag is restored and replaced by one command.
  SketchPoint {
    sketch: FeatureId,
    point: PointId,
  },
  /// Window selection from `start` to `now`: left to right selects what is fully inside, right to
  /// left what the box touches. (A press held still opens Select Other before a box can start;
  /// see [`crate::select::HOLD_SECS`].)
  Box {
    start: Pos2,
    now: Pos2,
  },
  /// Dragging a command dialog's arrow (extrude distance, plane offset).
  Manip,
  /// Freeform (lasso) selection; the loop is in `App::region`.
  Lasso,
  /// Paint selection: whatever the brush passes over is selected as it goes.
  Paint,
  /// Dragging a handle of the dialog's move/rotate triad (see [`crate::manipulators`]).
  Triad,
  /// Dragging a component (see [`crate::comp_drag`]).
  Component,
  /// Dragging one of the dialog's feature handles (see [`crate::handles`]).
  Handle,
  /// Zoom Window: the box being dragged.
  ZoomWindow {
    start: Pos2,
    now: Pos2,
  },
  /// Dragging the conic's rho arrow (see [`conic_rho_manip`]).
  ConicRho,
  /// Dragging the arrow of an applied sketch fillet or chamfer (see [`crate::sketch_modify`]).
  ToolManip,
  /// A right-drag the preset doesn't navigate with: Fusion's marking gesture from `start`, run
  /// when the button is let go (see [`crate::marking::flick`]).
  Flick {
    start: Pos2,
    now: Pos2,
  },
}

/// Clicks collected by the active sketch tool (sketch coordinates).
#[derive(Default)]
pub struct ToolState {
  pts: Vec<[f64; 2]>,
  /// Dimension tool: the geometry picked so far (a point waits for a second pick).
  dim_picks: Vec<Pick>,
  /// Dimension tool: the dimension being placed (it follows the cursor until a click places it).
  dim_place: Option<DimPlace>,
  /// Design before a sketch-point drag started.
  drag_backup: Option<fox_engine::model::Design>,
  /// Dimension command waiting for its value.
  pending_dim: Option<serde_json::Value>,
  /// Curves picked so far by a curve-picking tool, with where each was clicked.
  pub(crate) curves: Vec<(u32, [f64; 2])>,
  /// Heads-up value boxes of the shape being drawn.
  hud: HeadsUp,
  /// Center Point Arc: the cursor's last angle around the centre and the signed sweep it has
  /// wound from the start, so the arc runs the way the cursor went (negative = clockwise).
  arc_wind: Option<(f64, f64)>,
  /// Line: a press on the end of the last segment is being dragged; it draws a tangent arc.
  line_arc: bool,
  /// Values typed in the heads-up boxes of the shape's earlier steps (a slot's length, an edge
  /// polygon's edge), locked with dimensions when the shape is drawn.
  held: Vec<(Field, String)>,
  /// Conic: the rho its arrow was dragged to (None: [`CONIC_RHO`], or what its box holds).
  rho: Option<f64>,
  /// Where a curve-picking tool's last click made its result: nothing is hovered until the
  /// pointer leaves that spot, so the result draws like any other curve.
  hover_hold: Option<Pos2>,
  /// The pointer is over a dialog's preview, which doesn't light up but still reads as being over
  /// the model for the cursor prompt ("Hold Ctrl to modify selection", as in Fusion).
  pub(crate) over_preview: bool,
  /// Fillet / Chamfer: the one just applied, while its value box and arrow show.
  pub(crate) applied: Option<crate::sketch_modify::Applied>,
  /// Constraint command: how many picks it holds (they live in the selection), and whether
  /// anything was picked since it started.
  pub(crate) con_picks: usize,
  pub(crate) con_any: bool,
  /// A dimension's value being dragged: its constraint and the grab's offset from the value's
  /// centre (screen points).
  pub(crate) dim_drag: Option<(u32, Vec2)>,
  /// Measure was opened from a dimension box's menu: its measurement goes into the box when it
  /// closes.
  pub(crate) measure_fill: bool,
  /// 3D Sketch: where the Line chain's last point is in the world (off the sketch plane when it
  /// was taken from a model vertex).
  pub(crate) last3d: Option<DVec3>,
}

/// Fusion's rho for a new conic curve.
const CONIC_RHO: f64 = 0.75;

/// A dimension the Dimension tool is placing: its geometry, the command that creates it and its
/// measured value, drawn as a preview that follows the cursor until a click places it.
struct DimPlace {
  kind: ConstraintKind,
  cmd: serde_json::Value,
  /// The measured value (mm, or degrees for an angle).
  current: Option<f64>,
  /// Where the placement click put it (sketch coordinates), once placed.
  placed: Option<[f64; 2]>,
}

/// Fusion's heads-up dimension input: boxes that follow the shape being drawn. Typing sets the
/// active box, Tab moves to the next, Enter places the point; typed values become driving
/// dimensions.
#[derive(Default)]
struct HeadsUp {
  /// What was typed in each box ("" = the box follows the cursor).
  typed: [String; 2],
  /// The box with the keyboard.
  active: usize,
  /// The active box's text while it is being typed in.
  buf: Option<String>,
  /// Give the active box the keyboard this frame (selecting its text when true).
  focus: Option<bool>,
  /// Tab (false) or Shift+Tab (true) was pressed for the boxes this frame.
  tab: Option<bool>,
  /// The last cursor position on the sketch plane, and the point it snapped to.
  cursor: Option<([f64; 2], Option<PointId>)>,
}

/// What a heads-up box sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
  Length,
  Angle,
  Width,
  Height,
  Diameter,
  /// A polygon's radius: centre to a vertex (inscribed) or to the middle of a side (circumscribed).
  Radius,
  /// A polygon's number of sides.
  Sides,
  /// A conic's rho (how far its middle lies from the chord towards the apex).
  Rho,
}

/// The heads-up boxes of `tool` once it has `n` points.
fn hud_fields(tool: Tool, n: usize) -> &'static [Field] {
  match (tool, n) {
    (Tool::Line, 1..) => &[Field::Length, Field::Angle],
    // Fusion's 2-point rectangle focuses the box beside the cursor first (the height); the
    // width's dimension sits under the bottom edge
    (Tool::Rect, 1) => &[Field::Height, Field::Width],
    (Tool::CenterRect, 1) => &[Field::Width, Field::Height],
    (Tool::Circle, 1) => &[Field::Diameter],
    // Fusion's polygon boxes: the radius (or the edge's length) and the number of sides
    (Tool::Polygon | Tool::CircPolygon, 1) => &[Field::Radius, Field::Sides],
    (Tool::EdgePolygon, 1) => &[Field::Length, Field::Sides],
    // the rest of Fusion's heads-up boxes, by tool and step
    (Tool::MidpointLine, 1) => &[Field::Length, Field::Angle],
    (Tool::Rect3, 1) => &[Field::Length],
    (Tool::Rect3, 2) => &[Field::Width],
    (Tool::Circle2P, 1) | (Tool::Circle3P, 2) => &[Field::Diameter],
    // (the 3-point arc's third step has no box: the arc just follows the pointer)
    (Tool::CenterArc | Tool::TangentArc | Tool::ArcSlotCenter, 1) | (Tool::ArcSlot3, 2) => &[Field::Radius],
    (Tool::CenterArc | Tool::ArcSlotCenter, 2) => &[Field::Angle],
    (Tool::Ellipse, 1 | 2) => &[Field::Length],
    (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot | Tool::ArcSlot3, 1) => &[Field::Length],
    (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot, 2) | (Tool::ArcSlot3 | Tool::ArcSlotCenter, 3) => {
      &[Field::Width]
    }
    (Tool::Conic, 3) => &[Field::Rho],
    // the 2-tangent circle's radius box, once both tangents are picked
    (Tool::Circle2T, 2) => &[Field::Radius],
    _ => &[],
  }
}

/// The step the heads-up boxes are keyed on: points placed, or for the 2-tangent circle the
/// curves picked.
fn hud_step(app: &App) -> usize {
  if app.tool == Tool::Circle2T { app.tstate.curves.len() } else { app.tstate.pts.len() }
}

impl ToolState {
  pub fn is_empty(&self) -> bool {
    self.pts.is_empty()
      && self.dim_picks.is_empty()
      && self.dim_place.is_none()
      && self.curves.is_empty()
      && self.applied.is_none()
  }

  /// The curves a curve-picking tool has picked so far.
  pub(crate) fn picked_curves(&self) -> Vec<u32> {
    self.curves.iter().map(|c| c.0).collect()
  }
}

/// Tests elsewhere in the crate place a tool's point without a click.
#[cfg(test)]
pub(crate) fn push_point_for_test(app: &mut App, p: [f64; 2]) {
  app.tstate.pts.push(p);
}

pub fn reset_tool_state(app: &mut App) {
  // a pick-pick tool's first pick was selected (drawn bold blue): it isn't any more
  crate::sketch_modify::unselect_picks(app);
  app.tstate.applied = None;
  app.tstate.pts.clear();
  app.tstate.dim_picks.clear();
  app.tstate.dim_place = None;
  app.tstate.pending_dim = None;
  app.tstate.curves.clear();
  app.tstate.hud = HeadsUp::default();
  app.tstate.arc_wind = None;
  app.tstate.line_arc = false;
  app.tstate.held.clear();
  app.tstate.rho = None;
  app.tstate.con_picks = 0;
  app.tstate.con_any = false;
}

/// The conic's rho as the tool has it now: typed in its box, dragged with its arrow, or Fusion's
/// default.
fn conic_rho(app: &App) -> f64 {
  let typed = hud_fields(app.tool, hud_step(app)).iter().position(|f| *f == Field::Rho).and_then(|i| hud_value(app, i));
  typed.or(app.tstate.rho).unwrap_or(CONIC_RHO)
}

/// Enter accepts the shape a tool is still shaping: the spline being drawn, or the conic whose
/// rho is being set. Returns whether there was one.
pub fn accept_tool(app: &mut App) -> bool {
  if matches!(app.tool, Tool::Spline | Tool::ControlSpline) {
    return finish_spline(app);
  }
  if app.tool == Tool::Conic && app.tstate.pts.len() == 3 {
    let Some(sk) = app.s.edit_sketch else { return false };
    let apex = app.tstate.pts[2];
    place_point(app, sk, apex, None);
    return true;
  }
  false
}

/// Center Point Arc: follows the cursor around the centre once the start is placed, so the arc
/// sweeps the way the cursor went, as in Fusion.
fn track_arc_wind(app: &mut App, resp: &egui::Response, view: &View) {
  let wind = match (app.tool, app.tstate.pts.as_slice()) {
    (Tool::CenterArc, [c, a]) => {
      let (c, a) = (*c, *a);
      resp.hover_pos().and_then(|pos| sketch_cursor(app, view, pos)).map(|(cur, _)| {
        let cur = hud_apply(app, cur);
        let ang = (cur[1] - c[1]).atan2(cur[0] - c[0]);
        let (last, sweep) = app.tstate.arc_wind.unwrap_or(((a[1] - c[1]).atan2(a[0] - c[0]), 0.0));
        let step = (ang - last + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        // past a full turn either way the direction stays as it was
        (ang, (sweep + step).clamp(-std::f64::consts::TAU, std::f64::consts::TAU))
      })
    }
    _ => None,
  };
  if wind.is_some() || app.tstate.pts.len() != 2 || app.tool != Tool::CenterArc {
    app.tstate.arc_wind = wind;
  }
}

/// Viewport geometry for one frame: maps between egui points, frame pixels and the world.
pub(crate) struct View {
  rect: Rect,
  ppp: f32,
  w: u32,
  h: u32,
}

impl View {
  fn to_px(&self, p: Pos2) -> (f64, f64) {
    (((p.x - self.rect.min.x) * self.ppp) as f64, ((p.y - self.rect.min.y) * self.ppp) as f64)
  }

  pub(crate) fn to_screen(&self, cam: &render::Camera, p: DVec3) -> Option<Pos2> {
    let (x, y, z) = cam.project(p, self.w, self.h)?;
    if !(-1.0..=1.0).contains(&z) {
      return None;
    }
    Some(Pos2::new(self.rect.min.x + x as f32 / self.ppp, self.rect.min.y + y as f32 / self.ppp))
  }

  pub(crate) fn ray(&self, cam: &render::Camera, p: Pos2) -> (DVec3, DVec3) {
    let (x, y) = self.to_px(p);
    cam.ray(x, y, self.w, self.h)
  }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  // with Multiple Views the active view's quarter of the canvas, the others drawn beside it
  let full = ui.available_rect_before_wrap();
  let rect = crate::viewports::show_others(app, ui, full);
  app.canvas = rect;
  let resp = ui.allocate_rect(rect, Sense::click_and_drag());
  let ctx = ui.ctx().clone();
  let mut ppp = ctx.pixels_per_point();
  let area = rect.width() * rect.height() * ppp * ppp;
  if area > MAX_PIXELS {
    ppp *= (MAX_PIXELS / area).sqrt();
  }
  let view = View {
    rect,
    ppp,
    w: ((rect.width() * ppp).round() as u32).max(1),
    h: ((rect.height() * ppp).round() as u32).max(1),
  };
  app.canvas_px = [view.w, view.h];

  // a navigation drag that ends this frame (Zoom Window turns its mode off) isn't a click
  let navigating = app.nav_mode.is_some();
  navigate(app, &resp, &view, &ctx);
  update_hover(app, &resp, &view);
  appearance_drop(app, &resp, &view, &ctx);
  let planes = plane_pick(app, &resp, &view);
  render_if_needed(app, &view, &ctx);

  let painter = ui.painter_at(rect);
  let gpu_drawn = crate::gpu::paint(app, &painter, rect, [view.w, view.h], &ctx);
  if !gpu_drawn && let Some(tex) = &app.tex {
    painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
  }

  let cam = app.cam;
  crate::canvas_ui::overlay(app, &painter, &|p| view.to_screen(&cam, p));
  crate::trace::overlay(app, &painter, &|p| view.to_screen(&cam, p));

  // overlays that take clicks come first
  let mut consumed = false;
  shown_dims_overlay(app, &painter, &view);
  consumed |= crate::canvas_ui::calibrate_clicks(app, &resp, &|p| view.ray(&cam, p));
  consumed |= crate::cut_ui::lead_clicks(app, &resp, &|p| view.ray(&cam, p));
  consumed |=
    crate::library_ui::place_clicks(app, &resp, &painter, &|p| view.to_screen(&cam, p), &|p| view.ray(&cam, p));
  // the Origin folder's axes and point, under the sketch's own drawing
  crate::origin::overlay(app, &painter, &|p| view.to_screen(&cam, p));
  plane_names_overlay(app, &painter, &|p| view.to_screen(&cam, p));
  if (app.s.edit_sketch.is_some() || app.grid_preview.is_some()) && grid_shown(app) && app.grid.reference_numbers {
    grid_labels(app, &painter, &view);
  }
  if app.s.edit_sketch.is_some() {
    consumed |= sketch_overlay(app, ui, &painter, &view);
  } else {
    app.glyph_hover.clear();
  }
  let hot_plane = sketch_planes_overlay(app, planes, &painter, &view);
  offset_plane_overlay(app, &painter, &view);
  section_plane_overlay(app, &painter, &view);
  crate::dialogs::plane_cut_overlay(app, &painter, &|p| view.to_screen(&cam, p));
  // Preferences → General → Enable camera pivot: the orbit's centre while orbiting
  if app.preferences.values.camera_pivot
    && matches!(app.drag, Some(Drag::Orbit))
    && let Some(at) = view.to_screen(&cam, DVec3::from(app.cam.target))
  {
    pivot_marker(&painter, at);
  }
  if app.show_viewcube {
    consumed |= crate::viewcube::show(app, ui, &painter, view.rect);
  }
  if !consumed && let Some(plane) = hot_plane {
    consumed = true;
    if resp.clicked() {
      match app.dialog.as_mut() {
        // a primitive's base is drawn on the plane
        Some(d) if d.picks_plane() => d.place_on(&app.s, plane),
        // Mirror takes the plane
        Some(d) if d.wants_plane() => d.take_plane(plane),
        // Section Analysis's cut plane
        Some(d) if d.wants_cut_plane() => d.set_cut_plane(&app.s, plane),
        // Draft's pull direction
        Some(d) if d.wants_pull_plane() => d.pick_pull_plane(&mut app.s, plane),
        _ => {
          app.cancel_dialog();
          app.create_sketch(plane);
        }
      }
    }
  }
  // a click on a visible origin plane, axis or point selects it (again: clears it), as in Fusion;
  // with Mirror or Circular Pattern open it is the plane or axis they ask for, and an axis
  // clicked for Revolve's Axis is Revolve's
  if !consumed && let Some(item) = app.hover_origin {
    consumed = true;
    if resp.clicked() && !app.dialog.as_mut().is_some_and(|d| d.take_origin(item) || d.pick_origin_axis(item)) {
      match item.sketch_plane() {
        // an open Offset Plane takes it as its plane
        Some(plane) if app.dialog.as_ref().is_some_and(|d| d.takes_plane()) => crate::panels::pick_plane(app, plane),
        _ => crate::origin::pick(app, item),
      }
    }
  }
  // and so does one on a construction plane (or Mirror, Offset Plane take it)
  if !consumed && let Some(id) = app.hover_cplane {
    consumed = true;
    if resp.clicked() {
      crate::panels::pick_plane(app, serde_json::json!({"feature": id.0}));
    }
  }
  let step = app.increments.linear_step(grid_step(app, &view) / 10.0);
  let (project, ray) = (|p| view.to_screen(&cam, p), |p| view.ray(&cam, p));
  consumed |= !consumed && crate::dialogs::place_overlay(app, &resp, &painter, &project, &ray, step);
  consumed |= crate::manipulators::triad(app, &resp, &painter, &ctx, &project, &ray, step);
  consumed |= crate::form_ui::overlay(app, &resp, &painter, &ctx, &project, &ray);
  consumed |= crate::handles::show(app, &resp, &painter, &ctx, &project, &ray, step);
  crate::dialogs::surface::patch_overlay(app, &painter, &project);
  consumed |= manipulator(app, &resp, &painter, &view, &ctx);
  if app.objects.joints || app.objects.joint_origins {
    consumed |= crate::select::joint_glyphs(app, &resp, &painter, &project);
  }
  dim_editor(app, &ctx);
  track_arc_wind(app, &resp, &view);
  offset_follow(app, &resp, &view);
  tool_preview(app, &painter, &resp, &view);
  sketch_3d_manipulator(app, &painter, &view);
  crate::cam_ui::setup_preview(app, &painter, &project);
  trim_preview(app, &painter, &resp, &view);
  crate::sketch_modify::preview(app, &painter, &resp, &view);
  consumed |= crate::sketch_modify::manipulator(app, &resp, &painter, &view, &ctx);
  extend_preview(app, &painter, &resp, &view);
  break_preview(app, &painter, &resp, &view);
  dim_place_preview(app, &painter, &resp, &view);
  consumed |= spline_accept(app, ui, &painter, &view);
  consumed |= conic_rho_manip(app, &resp, &painter, &view);
  heads_up(app, &painter, &resp, &view, &ctx);
  measure_overlay(app, &painter, &view);
  cursor_prompt(app, &resp);
  if let Some(Drag::Box { start, now }) = app.drag {
    let r = Rect::from_two_pos(start, now);
    // as Fusion's: left to right, the window box is peach with a solid purple-grey border; right
    // to left, the crossing box is pale yellow with a dashed dark border
    if now.x >= start.x {
      painter.rect(r, 0.0, WINDOW_BOX.0, Stroke::new(1.0, WINDOW_BOX.1), egui::StrokeKind::Inside);
    } else {
      painter.rect_filled(r, 0.0, CROSSING_BOX.0);
      let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
      painter.add(egui::Shape::dashed_line(&corners, Stroke::new(1.0, CROSSING_BOX.1), 4.0, 3.0));
    }
  }
  // the marking gesture's trail, from where the right button went down
  if let Some(Drag::Flick { start, now }) = app.drag {
    painter.circle(start, 4.0, Color32::from_white_alpha(170), Stroke::new(1.0, Color32::from_gray(110)));
    painter.line_segment([start, now], Stroke::new(2.0, crate::theme::ACCENT));
  }
  if let Some(Drag::ZoomWindow { start, now }) = app.drag {
    let col = Color32::from_rgb(40, 120, 230);
    painter.rect_stroke(Rect::from_two_pos(start, now), 0.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
  }
  text_frame_overlay(app, &painter, &view, resp.hover_pos());
  crate::select::region_overlay(app, &painter);
  crate::select::show_other(app, &ctx);
  crate::select::show_tool(app, &ctx);

  // right-click without dragging opens the marking menu; while it's open it owns the clicks
  if resp.secondary_clicked()
    && let Some(p) = resp.interact_pointer_pos()
  {
    // a right-click on a canvas (with nothing nearer under the pointer) is for the canvas
    if app.hover.is_none() {
      let (o, d) = view.ray(&cam, p);
      if let Some((canvas, _)) = crate::canvas_ui::canvas_at(app, o, d) {
        app.s.selection = vec![Sel::Feature { feature: canvas }];
      }
    }
    crate::marking::open(app, p);
  } else if !consumed && !navigating && app.nav_mode.is_none() && app.marking.is_none() && !left_navigating(app, &resp)
  {
    handle_clicks(app, &resp, &view, &ctx);
  }
}

// ---------------------------------------------------------------- camera

/// A left drag moving the camera (the Alias preset's Shift+Alt+left orbit): it selects nothing.
fn left_navigating(app: &App, resp: &egui::Response) -> bool {
  matches!(app.drag, Some(Drag::Orbit | Drag::Pan | Drag::Zoom))
    && (resp.dragged_by(egui::PointerButton::Primary) || resp.drag_stopped_by(egui::PointerButton::Primary))
}

/// The sign the wheel and zoom drags scale by. Fusion's default (Reverse zoom direction off):
/// rolling the wheel back, toward you, zooms in, and so does a zoom drag down.
pub(crate) fn zoom_sign(reverse: bool) -> f64 {
  if reverse { 1.0 } else { -1.0 }
}

/// The camera's zoom factor (below 1 zooms in) for a wheel scroll of `scroll_y` points; egui
/// reports rolling the wheel back (toward you) as a negative `y`.
pub(crate) fn wheel_zoom_factor(scroll_y: f32, zoom_sign: f64) -> f64 {
  (-scroll_y as f64 * 0.0015 * zoom_sign).exp()
}

fn navigate(app: &mut App, resp: &egui::Response, view: &View, ctx: &egui::Context) {
  let (mods, scroll, zoom) = ctx.input(|i| (i.modifiers, i.smooth_scroll_delta(), i.zoom_delta()));
  // Preferences → Navigation: the preset's buttons, the zoom direction, zooming to the cursor
  let prefs = &app.preferences.values;
  let (preset, zoom_sign, to_cursor) = (prefs.nav, zoom_sign(prefs.reverse_zoom), prefs.zoom_to_cursor);
  // the canvas still zooms with the pointer over its own overlays (the ViewCube), not over
  // panels and dialogs drawn above it
  if ctx.rect_contains_pointer(resp.layer_id, resp.rect) {
    let mut factor = 1.0;
    if scroll.y.abs() > 0.0 {
      factor *= wheel_zoom_factor(scroll.y, zoom_sign);
    }
    if (zoom - 1.0).abs() > 1e-4 {
      factor /= zoom as f64;
    }
    if (factor - 1.0f64).abs() > 1e-6 {
      let toward = ctx.pointer_latest_pos().filter(|_| to_cursor).and_then(|p| {
        let (o, d) = view.ray(&app.cam, p);
        let (_, _, back) = app.cam.basis();
        ray_plane(o, d, DVec3::from(app.cam.target), back)
      });
      app.stop_cam_move();
      app.cam.zoom(factor, toward);
    }
  }
  // double-clicking the wheel fits the whole model, gliding there as Fit (F6) does
  if resp.double_clicked_by(egui::PointerButton::Middle) {
    app.fit_view();
  }
  for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary, egui::PointerButton::Middle] {
    if resp.drag_started_by(button)
      && let Some(action) = preset.action(button, mods)
    {
      app.drag = Some(match action {
        NavAction::Orbit => Drag::Orbit,
        NavAction::Pan => Drag::Pan,
        NavAction::Zoom => Drag::Zoom,
      });
    }
  }
  // a right-drag that doesn't move the camera is a marking gesture (Fusion's preset orbits
  // with Shift+middle and leaves the right button to the marking menu)
  if resp.drag_started_by(egui::PointerButton::Secondary)
    && preset.action(egui::PointerButton::Secondary, mods).is_none()
    && app.drag.is_none()
  {
    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or_default();
    app.marking = None;
    app.drag = Some(Drag::Flick { start, now: start });
  }
  if let Some(Drag::Flick { now, .. }) = &mut app.drag
    && let Some(p) = ctx.pointer_latest_pos()
  {
    *now = p;
  }
  if let Some(mode) = app.nav_mode
    && resp.drag_started_by(egui::PointerButton::Primary)
  {
    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or_default();
    app.drag = Some(match mode {
      NavMode::Orbit => Drag::Orbit,
      NavMode::Pan => Drag::Pan,
      NavMode::Zoom => Drag::Zoom,
      NavMode::ZoomWindow => Drag::ZoomWindow { start, now: start },
    });
  }
  if let Some(Drag::ZoomWindow { now, .. }) = &mut app.drag
    && let Some(p) = resp.interact_pointer_pos()
  {
    *now = p;
  }
  let d = resp.drag_delta();
  if d != Vec2::ZERO && matches!(app.drag, Some(Drag::Orbit | Drag::Pan | Drag::Zoom)) {
    app.stop_cam_move();
  }
  match app.drag {
    Some(Drag::Orbit) if app.free_orbit => app.cam.free_orbit(-d.x as f64 * 0.008, d.y as f64 * 0.008),
    Some(Drag::Orbit) => app.cam.orbit(-d.x as f64 * 0.008, d.y as f64 * 0.008),
    Some(Drag::Pan) => app.cam.pan(d.x as f64 * view.ppp as f64, d.y as f64 * view.ppp as f64, view.h as f64),
    Some(Drag::Zoom) => app.cam.zoom((d.y as f64 * 0.006 * zoom_sign).exp(), None),
    _ => {}
  }
  if resp.drag_stopped()
    && let Some(drag) = app.drag.take()
  {
    match drag {
      Drag::SketchPoint { sketch, point } => finish_point_drag(app, sketch, point),
      Drag::Component => crate::comp_drag::finish(app),
      Drag::Flick { start, now } => crate::marking::flick(app, ctx, start, now),
      Drag::Box { start, now } => {
        let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command) || app.dialog.is_some();
        finish_box(app, view, start, now, add);
      }
      Drag::Lasso => {
        let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command) || app.dialog.is_some();
        crate::select::finish_lasso(app, add, &|p| view.to_px(p).into());
      }
      Drag::ZoomWindow { start, now } => {
        let bx = Rect::from_two_pos(start, now);
        let (o, d) = view.ray(&app.cam, bx.center());
        let hit = fox_engine::scene::ray_hits(&app.s, o, d).first().map(|(t, _)| o + d * *t);
        if let Some(to) = zoom_window(&app.cam, view, bx, hit) {
          app.fly_to(to);
        }
        // one box, as in Fusion
        app.nav_mode = None;
      }
      _ => {}
    }
  }
}

// ---------------------------------------------------------------- picking

/// A swatch from the Appearance or Physical Material palette over the canvas: the face under it
/// lights up, and letting go applies the appearance to that face or its body, or the material to
/// the body's component.
fn appearance_drop(app: &mut App, resp: &egui::Response, view: &View, ctx: &egui::Context) {
  let Some(drag) = egui::DragAndDrop::payload::<crate::dialogs::SwatchDrag>(ctx) else { return };
  let Some(pos) = ctx.pointer_latest_pos().filter(|p| resp.rect.contains(*p)) else { return };
  let Some(frame) = &app.frame else { return };
  let (x, y) = view.to_px(pos);
  let under = app.picks.get(frame.pick(x, y, 0, |_| true)).filter(|p| matches!(p, Pick::Face { .. }));
  app.hover = under;
  if ctx.input(|i| i.pointer.any_released()) {
    egui::DragAndDrop::clear_payload(ctx);
    if let Some(Pick::Face { body, face }) = under {
      crate::material_ui::drop_swatch(app, &drag, body, face);
    }
  }
}

fn update_hover(app: &mut App, resp: &egui::Response, view: &View) {
  if app.drag.is_some() {
    return;
  }
  app.tstate.over_preview = false;
  let Some(pos) = resp.hover_pos() else {
    app.hover = None;
    return;
  };
  if app.tstate.hover_hold == Some(pos) {
    app.hover = None;
    return;
  }
  app.tstate.hover_hold = None;
  let Some(frame) = &app.frame else { return };
  let (x, y) = view.to_px(pos);
  let sketching = app.s.edit_sketch.is_some();
  let r = ((if sketching { SNAP_PX } else { 5.0 }) * view.ppp).round() as i32;
  // selecting or dimensioning in a sketch, a curve needs the pointer within about a pixel of
  // its stroke
  let curve_r = if sketching && matches!(app.tool, Tool::Select | Tool::Dimension) {
    (CURVE_PICK_PX * view.ppp).round() as i32
  } else {
    r
  };
  let picks = &app.picks;
  // preferred kinds win within the search radius
  let pick_within = |r: i32, want: fn(&Pick) -> bool| -> Option<Pick> {
    let id = frame.pick(x, y, r, |id| picks.get(id).is_some_and(|p| want(&p)));
    picks.get(id).filter(want)
  };
  let pick_kind = |want: fn(&Pick) -> bool| pick_within(r, want);
  let exact = picks.get(frame.pick(x, y, 0, |_| true));
  app.hover = preferred_pick(sketching, pick_kind, |want| pick_within(curve_r, want), exact);
  // while a sketch tool draws, only points matter for snapping
  if sketching && app.tool.picks_curves() {
    // trim / extend / break / tangent circles / blend pick curves
    app.hover = pick_kind(|p| matches!(p, Pick::SketchCurve { .. }));
  } else if sketching
    && app.tool != Tool::Select
    && app.tool != Tool::Dimension
    && !matches!(app.tool, Tool::Constrain(_))
  {
    app.hover = app.hover.filter(|p| matches!(p, Pick::SketchPoint { .. }));
  } else if !sketching {
    let (filter, priority) = (app.sel_filter, app.sel_priority);
    let locked = &app.ctx_menu.unselectable;
    let s = &app.s;
    let skip_mesh = app.dialog.as_ref().is_some_and(|d| d.skips_mesh_bodies());
    let ok = |p: &Pick| {
      filter.allows_on(s, p, priority)
        && !crate::select::pick_body(p)
          .is_some_and(|b| locked.contains(&b) || skip_mesh && crate::select::is_mesh(s, b))
    };
    if !app.hover.as_ref().is_some_and(ok) {
      // the front-most pick is filtered out: the nearest allowed one around the cursor wins
      app.hover = picks.get(frame.pick(x, y, r, |id| picks.get(id).is_some_and(|p| ok(&p)))).filter(ok);
    }
  }
  // a dialog's preview isn't model geometry: nothing on it lights up, as in Fusion (the nearest
  // allowed pick off it does)
  let previewed = app.dialog.as_ref().map(|d| d.preview_bodies(&app.s)).unwrap_or_default();
  let on_preview = |p: &Pick| crate::select::pick_body(p).is_some_and(|b| previewed.contains(&b));
  app.tstate.over_preview = app.hover.as_ref().is_some_and(on_preview);
  if app.tstate.over_preview {
    let (filter, priority) = (app.sel_filter, app.sel_priority);
    let ok = |p: &Pick| !on_preview(p) && (sketching || filter.allows(p, priority));
    app.hover = picks.get(frame.pick(x, y, r, |id| picks.get(id).is_some_and(|p| ok(&p)))).filter(ok);
  }
}

/// Which pick near the cursor wins: `nearest(want)` is the closest pick of a kind within the
/// search radius, `nearest_curve(want)` the same within the curve radius (tighter in a sketch, where
/// Fusion needs the pointer within about a pixel of the stroke) and `exact` whatever is under the
/// pixel. In a sketch, points beat curves beat the rest. Outside one, a visible sketch's points
/// beat vertices (drawn only with their filter on), which beat sketch curves, which beat edges,
/// which beat faces, so a visible sketch's curve on a face is picked over the face, as in Fusion.
pub(crate) fn preferred_pick(
  sketching: bool,
  nearest: impl Fn(fn(&Pick) -> bool) -> Option<Pick>,
  nearest_curve: impl Fn(fn(&Pick) -> bool) -> Option<Pick>,
  exact: Option<Pick>,
) -> Option<Pick> {
  if sketching {
    nearest(|p| matches!(p, Pick::SketchPoint { .. }))
      .or_else(|| nearest_curve(|p| matches!(p, Pick::SketchCurve { .. })))
      .or(exact)
  } else {
    nearest(|p| matches!(p, Pick::SketchPoint { .. }))
      .or_else(|| nearest(|p| matches!(p, Pick::Vertex { .. })))
      .or_else(|| nearest(|p| matches!(p, Pick::SketchCurve { .. })))
      .or_else(|| nearest(|p| matches!(p, Pick::Edge { .. })))
      .or(exact)
  }
}

/// The options the viewport builds its scene with now: what is shown, hidden, highlighted and
/// analysed (Capture Image renders the same scene at another size).
pub(crate) fn scene_options(app: &mut App) -> ViewOptions {
  let profiles_of: Vec<FeatureId> = app.dialog.as_ref().map(|d| d.profile_sketches()).unwrap_or_default();
  let analysis = match fox_engine::scene::view_options(&app.s, &view_params(app)) {
    Ok(o) => o,
    Err(e) => {
      app.status = Some((e, true));
      ViewOptions::default()
    }
  };
  let vis = app.objects;
  let rendering = crate::render_ui::shading(app);
  // under the Bodies priority the body under the pointer lights as a whole, not its face
  let hover_bodies = crate::select::hover_bodies(app);
  let mut peek = app.ctx_menu.peek.clone();
  peek.extend(hover_bodies.iter().filter(|b| !app.ctx_menu.peek.contains(b)));
  ViewOptions {
    hover: if hover_bodies.is_empty() { app.hover } else { None },
    hide_edges: !app.visual_style.edges(),
    hide_grid: !grid_shown(app),
    // the origin axes (dashed, each with its own eye under the Origin folder's) are drawn over
    // the image by `crate::origin`, so the scene draws none of its own
    hide_origin: true,
    origin_axis_len: Some(origin_plane_size(app) * ORIGIN_AXIS_REACH),
    // the Sketches folder's eye off draws no sketch but the one being edited
    // In-canvas Render hides sketches and planes while it runs, as Fusion's
    hide_sketches: !vis.sketches || analysis.hide_sketches || app.sketch_rows.folder_hidden || rendering,
    hide_construction_planes: !vis.construction_planes || rendering,
    hide_construction_axes: !vis.construction_axes,
    hide_construction_points: !vis.construction_points,
    grid_step: app.grid.step(),
    grid_plane: app.grid_preview,
    // the Layout Grid carries Fusion's red X and green Y axis lines across it
    grid_axes: app.s.edit_sketch.is_none() && vis.origin_axes,
    show_profiles_of: profiles_of,
    hide_profiles_of: app.hidden_profiles.clone(),
    hide_projected_of: app.sketch_rows.hidden_projected.clone(),
    hide_construction_of: app.sketch_rows.hidden_construction.clone(),
    show_vertices: app.sel_filter.vertices && app.s.edit_sketch.is_none(),
    peek,
    peek_faces: app.ctx_menu.peek_faces.clone(),
    peek_sketches: app.ctx_menu.peek_sketches.clone(),
    // the Bodies folder's eye off draws none
    only_bodies: if app.ctx_menu.bodies_hidden {
      Some(Vec::new())
    } else {
      app.ctx_menu.isolate.clone().or(analysis.only_bodies.clone())
    },
    opacity: app.ctx_menu.opacity.iter().copied().chain(crate::mesh_ui::ghosted(app)).collect(),
    flat_meshes: app.ctx_menu.flat_shaded.clone(),
    // Offset Plane's preview is filled over the image (see `offset_plane_overlay`), and so are
    // the construction planes (see `crate::construction`)
    hide_planes: app.dialog.as_ref().and_then(|d| d.previewed_plane()).into_iter().collect(),
    planes_drawn_by_caller: true,
    hover_more: app.glyph_hover.clone(),
    preview_curves: app.dialog.as_ref().map(|d| d.preview_curves().to_vec()).unwrap_or_default(),
    sketch_show: app.palette.show(),
    ghosts: app.dialog.as_ref().map(|d| d.ghost_bodies()).unwrap_or_default(),
    // a base flange's previewed sheet is Fusion's solid preview blue
    preview: app.dialog.as_ref().map(|d| d.sheet_preview_bodies(&app.s)).unwrap_or_default(),
    tint_faces: app.dialog.as_ref().map(|d| d.tint_faces(&app.s)).unwrap_or_default(),
    component_colors: app.ctx_menu.component_colors,
    // the palette's Slice reaches the scene as the `slice` render parameter (see `slice_params`)
    ..analysis
  }
}

/// The sketch plane as a section plane with its normal towards `eye`, so the section cuts away
/// what is between the camera and the sketch.
pub(crate) fn slice_plane(plane: Plane, eye: DVec3) -> Plane {
  if (eye - plane.origin).dot(plane.normal()) >= 0.0 {
    plane
  } else {
    Plane { origin: plane.origin, x: plane.y, y: plane.x }
  }
}

fn render_if_needed(app: &mut App, view: &View, ctx: &egui::Context) {
  let profiles_of: Vec<FeatureId> = app.dialog.as_ref().map(|d| d.profile_sketches()).unwrap_or_default();
  let key = RenderKey {
    revision: app.s.revision,
    cam: app.cam,
    size: [view.w, view.h],
    hover: app.hover,
    selection: app.s.selection.clone(),
    edit_sketch: app.s.edit_sketch,
    style: app.visual_style,
    objects: app.objects,
    effects: app.effects,
    show_grid: grid_shown(app),
    grid_step: app.grid.step(),
    visible: app.s.design.bodies.iter().map(|b| (b.body.0, b.visible)).collect(),
    profiles_of,
    hidden_profiles: app.hidden_profiles.clone(),
    sketch_rows: app.sketch_rows.clone(),
    view: dark_view(view_params(app), ctx),
    vertices: app.sel_filter.vertices && app.s.edit_sketch.is_none(),
    peek: app.ctx_menu.peek.iter().chain(&crate::select::hover_bodies(app)).map(|b| b.0).collect(),
    isolate: app.ctx_menu.isolate.as_ref().map(|v| v.iter().map(|b| b.0).collect()),
    opacity: app.ctx_menu.opacity.iter().copied().chain(crate::mesh_ui::ghosted(app)).map(|(b, a)| (b.0, a)).collect(),
    flat: app.ctx_menu.flat_shaded.iter().map(|b| b.0).collect(),
    peek_faces: app.ctx_menu.peek_faces.iter().map(|(b, f)| (b.0, *f)).collect(),
    peek_sketches: app.ctx_menu.peek_sketches.iter().map(|s| s.0).collect(),
    glyph_hover: app.glyph_hover.clone(),
    palette: app.palette,
    ghosts: app.dialog.as_ref().map(|d| d.ghost_bodies().iter().map(|b| b.0).collect()).unwrap_or_default(),
    grid_plane: app.grid_preview,
    component_colors: app.ctx_menu.component_colors,
    quads: if crate::render_ui::shading(app) {
      Vec::new()
    } else {
      [crate::origin::plane_quads(app), crate::construction::quads(app)].concat()
    },
    tints: app
      .dialog
      .as_ref()
      .map(|d| d.tint_faces(&app.s).into_iter().map(|(b, f, _)| (b.0, f)).collect())
      .unwrap_or_default(),
    display: (app.display.ground(), app.display.environment),
    bodies_hidden: app.ctx_menu.bodies_hidden,
  };
  if crate::gpu::active(app) {
    crate::gpu::set_quads(app, &key.quads);
  }
  if crate::gpu::active(app) && !crate::gpu::scene_changed(app, &key) {
    return;
  }
  if app.render_key.as_ref() == Some(&key) && app.tex.is_some() {
    return;
  }
  let opts = scene_options(app);
  let (mut scene, picks) = app.scene.build(&app.s, &opts);
  if let Some(d) = &app.dialog {
    d.add_cut_ghost(&mut scene);
  }
  crate::navbar::dress(app, &mut scene);
  if let Some(c) = crate::render_ui::canvas_background(app) {
    // the Render workspace's own background, not the dome
    scene.post.dome = None;
    scene.background_top = [c[0], c[1], c[2], 255];
    scene.background_bottom = scene.background_top;
  } else if crate::theme::is_dark(ctx) {
    // Theme: Dark darkens the canvas too; a sketch's plain background stays plain
    let (top, bottom) = crate::theme::dark_canvas();
    let plain = scene.background_top == scene.background_bottom;
    scene.background_top = top;
    scene.background_bottom = if plain { top } else { bottom };
  }
  if crate::gpu::active(app) {
    // the Render workspace shades the bodies with their materials
    let look = crate::render_ui::viewport_look(app, &mut scene, &picks);
    crate::gpu::set_look(app, look);
    crate::gpu::set_scene(app, scene, picks, key);
    return;
  }
  scene.meshes.extend(key.quads.iter().map(|q| q.mesh()));
  let frame = render::render(&scene, &app.cam, view.w, view.h);
  let img = egui::ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &frame.rgba);
  match &mut app.tex {
    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
    None => app.tex = Some(ctx.load_texture("viewport", img, egui::TextureOptions::LINEAR)),
  }
  app.frame = Some(frame);
  app.picks = picks;
  app.render_key = Some(key);
}

/// The Sketch Palette's Slice as a render parameter: the open sketch and whether the camera is
/// behind its plane (`flip`), so the scene cuts away the part of each body between the camera
/// and the plane. None while Slice is off or no sketch is open.
pub(crate) fn slice_params(app: &App) -> Option<serde_json::Value> {
  if !app.palette.slice {
    return None;
  }
  let (sk, plane) = sketch_plane(app)?;
  let flip = slice_plane(plane, app.cam.eye()).normal() != plane.normal();
  Some(json!({"sketch": sk.0, "flip": flip}))
}

/// Whether the grid is drawn. Outside a sketch it is Grid and Snaps ▸ Layout Grid (off by
/// default, as in Fusion). In a sketch it is the palette's Sketch Grid, except while 3D Sketch is
/// on and a tool with a 3D form runs, when Fusion hides it under the tool's 3D manipulator.
pub(crate) fn grid_shown(app: &App) -> bool {
  if app.s.edit_sketch.is_none() {
    return app.layout_grid;
  }
  let tool_3d = app.tool != Tool::Select && sketch_3d_enabled(app);
  app.show_grid && !(app.palette.sketch_3d && tool_3d)
}

/// Whether the palette's 3D Sketch checkbox takes clicks: Fusion greys it while a tool with no
/// 3D form runs (Ellipse, Conic Curve, Point, Text, the splines) and enables it otherwise,
/// dialog tools (Mirror, patterns, Project, Sketch Dimension, Fillet, Offset, ...) included.
pub(crate) fn sketch_3d_enabled(app: &App) -> bool {
  if app.dialog.as_ref().is_some_and(|d| matches!(d.kind, crate::dialogs::Kind::Form(f) if f.cmd == "sketch.text.form"))
  {
    return false;
  }
  !matches!(app.tool, Tool::Ellipse | Tool::Conic | Tool::Point | Tool::Spline | Tool::ControlSpline)
}

/// 3D Sketch on: the Line tool's 3D manipulator at the start of the segment being drawn (the
/// last placed point), a plane handle across the sketch normal and the three axis arrows, as
/// Fusion shows it. Drawn only: the solver keeps the sketch in its plane.
fn sketch_3d_manipulator(app: &App, painter: &egui::Painter, view: &View) {
  if !app.palette.sketch_3d || app.tool != Tool::Line {
    return;
  }
  let Some((_, plane)) = sketch_plane(app) else { return };
  let Some(last) = app.tstate.pts.last() else { return };
  let project = |p: DVec3| view.to_screen(&app.cam, p);
  // a point taken off the plane: the manipulator sits there, and the segment runs from it
  let at = app.tstate.last3d.filter(|w| plane.to_local(*w) == *last).unwrap_or_else(|| plane.to_world(*last));
  if (at - plane.origin).dot(plane.normal()).abs() > 1e-6
    && let Some(cur) = app.tstate.hud.cursor.map(|c| plane.to_world(c.0))
    && let (Some(a), Some(b)) = (project(at), project(cur))
  {
    painter.line_segment([a, b], Stroke::new(1.5, Color32::from_rgb(6, 150, 214)));
  }
  crate::manipulators::sketch_axes(painter, &project, &app.cam, at, plane.normal());
}

/// The render parameters with the theme: Theme: Dark draws the canvas dark (see
/// [`render_if_needed`]), so switching themes draws it again.
fn dark_view(mut v: serde_json::Value, ctx: &egui::Context) -> serde_json::Value {
  if crate::theme::is_dark(ctx) {
    v["dark_theme"] = json!(true);
  }
  v
}

/// The section and analysis settings as render parameters.
fn view_params(app: &App) -> serde_json::Value {
  let mut v = crate::analyses::render_params(app).unwrap_or(json!({}));
  // a Sweep's or Loft's Analysis tab shows its analysis while the dialog is open
  if let Some(a) = app.dialog.as_ref().and_then(|d| d.tab_analysis())
    && v.get("analysis").is_none()
  {
    v = json!({"analysis": a, "pull": "z", "angle": 1.0});
  }
  // the red preview of a cut's tool (its revision re-renders the canvas when it changes)
  if let Some(g) = app.dialog.as_ref().map(|d| d.cut_ghost_key()).filter(|g| *g > 0) {
    v["cut_ghost"] = json!(g);
  }
  crate::flat_ui::view_params(app, &mut v);
  if app.manufacture && !app.cam_setups_hidden {
    v["toolpaths"] = json!(app.s.toolpaths_of.map(|t| t.1));
  }
  // New Setup shows its stock translucent (see `cam_ui::setup_preview`), not as the outline
  if crate::cam_ui::setup_preview_geometry(app).is_some() {
    v["hide_stock"] = json!(true);
  }
  if app.manufacture || app.cut_window.is_some() {
    v["cut"] = json!(app.s.cut_preview.as_ref().map(|c| [c.job as u64, c.revision]));
  }
  if let Some(sim) = crate::sim_ui::view_json(app) {
    v["sim"] = sim;
  }
  // the open Section Analysis, else the newest kept one that shows
  if let Some(sec) = app.section.as_ref().or_else(|| crate::dialogs::shown_section(app)) {
    v["section"] = sec.clone();
  }
  if let Some(sl) = slice_params(app) {
    v["slice"] = sl;
  }
  if crate::render_ui::shading(app) {
    v["render_look"] = crate::render_ui::look_key(app);
  }
  if let Some(c) = crate::render_ui::canvas_background(app) {
    v["render_canvas"] = json!(c);
  }
  v
}

// ---------------------------------------------------------------- clicks & tools

pub(crate) fn sketch_plane(app: &App) -> Option<(FeatureId, Plane)> {
  let sk = app.s.edit_sketch?;
  Some((sk, app.s.model.sketches.get(&sk)?.plane))
}

/// Cursor position on the sketch plane, snapped to nearby points and H/V alignment.
fn sketch_cursor(app: &App, view: &View, pos: Pos2) -> Option<([f64; 2], Option<PointId>)> {
  sketch_inference(app, view, pos).map(|(p, snapped, _)| (p, snapped))
}

/// [`sketch_cursor`], plus the points the cursor lines up with in x and in y (the inference
/// lines' far ends).
fn sketch_inference(app: &App, view: &View, pos: Pos2) -> Option<([f64; 2], Option<PointId>, Aligned)> {
  let (sk, plane) = sketch_plane(app)?;
  // snap to the hovered point (exact coordinates)
  if let Some(Pick::SketchPoint { sketch, point }) = app.hover
    && sketch == sk
    && let Some(p) = app.s.design.sketch(sk).ok().and_then(|s| s.pos(point))
  {
    return Some((p, Some(point), [None, None]));
  }
  let (o, d) = view.ray(&app.cam, pos);
  let hit = ray_plane(o, d, plane.origin, plane.normal())?;
  let mut p = plane.to_local(hit);
  // H/V inference against the previous click
  let mut locked = [false, false];
  if let Some(prev) = app.tstate.pts.last() {
    let (dx, dy) = (p[0] - prev[0], p[1] - prev[1]);
    let ang = dy.atan2(dx).to_degrees().abs();
    if ang < HV_SNAP_DEG || (180.0 - ang) < HV_SNAP_DEG {
      p[1] = prev[1];
      locked[1] = true;
    } else if (ang - 90.0).abs() < HV_SNAP_DEG {
      p[0] = prev[0];
      locked[0] = true;
    }
  }
  // tidy coordinates: round to a precision that matches the zoom; Snap goes to the grid (a
  // quarter cell of the adaptive one, or the set spacing)
  let step = match (app.snap, app.grid.step()) {
    (true, Some(fixed)) => fixed,
    (true, None) => grid_step(app, view) / 4.0,
    (false, _) => grid_step(app, view) / 100.0,
  };
  p = [round_to(p[0], step), round_to(p[1], step)];
  // inference lines: line up exactly with sketch points in line with the cursor
  let world_per_px = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let (p, aligned) = align_to_points(p, &inference_points(app, sk), ALIGN_PX * world_per_px, locked);
  Some((p, None, aligned))
}

/// The sketch points the cursor lines up with in x (a vertical guide) and in y (horizontal).
type Aligned = [Option<[f64; 2]>; 2];

/// Points the cursor infers alignment from: the sketch's points (ends, centres), line midpoints,
/// and the clicks of the shape being drawn except the last (H/V against that is the line's own).
fn inference_points(app: &App, sk: FeatureId) -> Vec<[f64; 2]> {
  let mut out: Vec<[f64; 2]> = Vec::new();
  if let Ok(s) = app.s.design.sketch(sk) {
    out.extend(s.points.iter().map(|q| [q.x, q.y]));
    for c in &s.curves {
      if let CurveKind::Line { a, b } = c.kind
        && let (Some(a), Some(b)) = (s.pos(a), s.pos(b))
      {
        out.push([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
      }
    }
  }
  if let [rest @ .., _] = app.tstate.pts.as_slice() {
    out.extend_from_slice(rest);
  }
  if let Some(last) = app.tstate.pts.last() {
    out.retain(|q| (q[0] - last[0]).hypot(q[1] - last[1]) > 1e-9);
  }
  out
}

/// Snaps `p`'s x (and y) to the nearest point in `pts` whose x (y) is within `tol`, leaving
/// `locked` axes alone. Of the points that line up, the one nearest the cursor anchors the guide.
fn align_to_points(mut p: [f64; 2], pts: &[[f64; 2]], tol: f64, locked: [bool; 2]) -> ([f64; 2], Aligned) {
  let mut aligned: Aligned = [None, None];
  for axis in 0..2 {
    if locked[axis] {
      continue;
    }
    let other = 1 - axis;
    let off = |q: &[f64; 2]| ((q[axis] - p[axis]).abs(), (q[other] - p[other]).abs());
    let best = pts.iter().filter(|q| off(q).0 <= tol).min_by(|a, b| {
      let ((da, oa), (db, ob)) = (off(a), off(b));
      da.total_cmp(&db).then(oa.total_cmp(&ob))
    });
    if let Some(q) = best {
      p[axis] = q[axis];
      aligned[axis] = Some(*q);
    }
  }
  (p, aligned)
}

/// Dotted inference lines from the points the cursor lines up with, as Fusion draws them.
fn paint_inference_lines(app: &App, painter: &egui::Painter, view: &View, pos: Pos2) {
  let Some((_, plane)) = sketch_plane(app) else { return };
  let Some((cur, _, aligned)) = sketch_inference(app, view, pos) else { return };
  let s = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let col = Color32::from_rgb(110, 110, 120);
  for q in aligned.into_iter().flatten() {
    if let (Some(a), Some(b)) = (s(q), s(cur)) {
      painter.extend(egui::Shape::dotted_line(&[a, b], col, 5.0, 1.0));
    }
  }
}

/// Zoom Window: the camera that fills the view with `bx` (screen points), centred on it: on `hit`
/// (the model under the box's centre) when given, else on the target's depth. None for a box too
/// small to mean anything.
fn zoom_window(cam: &render::Camera, view: &View, bx: Rect, hit: Option<DVec3>) -> Option<render::Camera> {
  if bx.width() < 4.0 || bx.height() < 4.0 || view.rect.width() < 1.0 || view.rect.height() < 1.0 {
    return None;
  }
  let (o, d) = view.ray(cam, bx.center());
  let (_, _, back) = cam.basis();
  let centre = match hit {
    Some(h) => h,
    None => ray_plane(o, d, DVec3::from(cam.target), back)?,
  };
  // the box spans this much of the view at the centre's depth
  let depth = (cam.eye() - centre).dot(back).max(1e-3);
  let k = (bx.width() / view.rect.width()).max(bx.height() / view.rect.height()) as f64;
  let mut to = *cam;
  to.target = centre.to_array();
  to.distance = if cam.ortho { cam.distance * k } else { depth * k }.clamp(1e-3, 1e7);
  Some(to)
}

fn round_to(v: f64, step: f64) -> f64 {
  if step > 0.0 { (v / step).round() * step } else { v }
}

pub(crate) fn grid_step(app: &App, view: &View) -> f64 {
  let world_per_px = app.cam.view_height() / view.h.max(1) as f64;
  fox_engine::scene::nice_step(world_per_px * 40.0)
}

fn handle_clicks(app: &mut App, resp: &egui::Response, view: &View, ctx: &egui::Context) {
  let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) else { return };
  let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);

  // start dragging a sketch point with the select tool
  if resp.drag_started_by(egui::PointerButton::Primary)
    && app.tool == Tool::Select
    && let Some(Pick::SketchPoint { sketch, point }) = app.hover
    && point != fox_engine::sketch::ORIGIN
  {
    app.tstate.drag_backup = Some(app.s.design.clone());
    app.drag = Some(Drag::SketchPoint { sketch, point });
  }
  if let Some(Drag::SketchPoint { sketch, point }) = app.drag
    && resp.dragged_by(egui::PointerButton::Primary)
  {
    drag_point_live(app, view, pos, sketch, point);
    return;
  }
  // freeform and paint selection
  if crate::select::drag_region(app, resp, ctx, pos, &|p| view.to_px(p).into()) {
    return;
  }
  // dragging a component, else window selection with the select tool
  let cam = app.cam;
  if crate::comp_drag::handle(app, resp, ctx, pos, &|p| view.ray(&cam, p)) {
    return;
  }
  // Select Other, as in Fusion: a press held still on the model opens the list while the
  // button is down, and the release then selects nothing (a sketch point is for dragging)
  if app.tool == Tool::Select
    && app.drag.is_none()
    && app.other.is_none()
    && app.hover.is_some_and(|h| !matches!(h, Pick::SketchPoint { .. }))
    && !app.dialog.as_ref().is_some_and(|d| d.wants_text_frame())
  {
    let held = ctx.input(|i| {
      let origin = i.pointer.press_origin()?;
      let still = i.pointer.primary_down() && resp.rect.contains(origin) && (pos - origin).length() < 4.0;
      still.then_some((origin, i.pointer.press_start_time().unwrap_or(i.time), i.time))
    });
    if let Some((origin, since, now)) = held {
      if now - since >= crate::select::HOLD_SECS {
        open_select_other(app, view, origin);
        return;
      }
      ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
  }
  if resp.drag_started_by(egui::PointerButton::Primary)
    && app.drag.is_none()
    && app.other.is_none()
    && app.tool == Tool::Select
  {
    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
    app.drag = Some(Drag::Box { start, now: pos });
  }
  if let Some(Drag::Box { now, .. }) = &mut app.drag {
    *now = pos;
    return;
  }
  if line_arc_drag(app, resp, view, ctx, pos) || trim_drag(app, resp, view) {
    return;
  }

  if !resp.clicked() {
    return;
  }
  // the release of the press that opened Select Other picks nothing
  if app.other.is_some() {
    return;
  }
  app.last_click = Some(pos);
  // Trim picking pieces: a click on a trimmed surface marks the piece under the cursor to remove
  if let Some(Pick::Face { body, .. }) = app.hover
    && let Some(d) = app.dialog.as_mut()
    && d.picks_piece_of(body)
  {
    let (o, dir) = view.ray(&app.cam, pos);
    let hits = fox_engine::scene::ray_hits(&app.s, o, dir);
    if let Some((t, _)) = hits.iter().find(|(_, p)| matches!(p, Pick::Face { body: b, .. } if *b == body)) {
      d.add_piece((o + dir * *t).to_array());
    }
    return;
  }
  if let Some(d) = app.dialog.as_mut()
    && d.previewing_model()
  {
    // pick on the model without the preview (it comes back once the picks change)
    app.hover = d.unpreview_pick(&mut app.s, app.hover);
    if app.hover.is_none() {
      return;
    }
  }
  let sketching = app.s.edit_sketch.is_some();
  // Sketch Offset follows the cursor until a click places it (a click on another curve still
  // adds it to the offset)
  if let Some(d) = app.dialog.as_mut()
    && d.follows_cursor()
    && !matches!(app.hover, Some(Pick::SketchCurve { .. }))
  {
    d.stop_following();
    return;
  }
  // Text waits for its frame: a click on the sketch places a corner (two clicks place the
  // frame; a drag's release places nothing), as in Fusion
  if sketching && app.dialog.as_ref().is_some_and(|d| d.wants_text_frame()) {
    place_text_corner(app, view, pos);
    return;
  }
  // a constraint command picks the hovered point, curve or profile
  if let Tool::Constrain(kind) = app.tool
    && sketching
  {
    crate::constrain::click(app, kind);
    return;
  }
  if !sketching || app.tool == Tool::Select {
    // with a command dialog open, picks add to (or toggle out of) its selection, as in Fusion
    select_click(app, add || app.dialog.is_some() || app.sel_tool.is_some());
    // double-clicking an edge selects its loop
    if resp.double_clicked()
      && let Some(Pick::Edge { body, edge }) = app.hover
    {
      crate::select::pick_loop(app, body, edge, add || app.dialog.is_some());
    }
    // double-clicking a letter of a text edits the text
    if resp.double_clicked()
      && app.dialog.is_none()
      && let Some(Pick::SketchCurve { sketch, curve }) = app.hover
      && app.s.design.sketch(sketch).is_ok_and(|s| s.text_of(curve).is_some())
    {
      app.s.selection = vec![Sel::SketchCurve { sketch, curve }];
      app.start(crate::dialogs::Kind::Form(&crate::forms::SK_TEXT_EDIT));
    }
    return;
  }
  if app.tool == Tool::Dimension {
    dimension_click(app, pos, add);
    return;
  }
  let Some((sk, plane)) = sketch_plane(app) else { return };
  if matches!(app.tool, Tool::Trim | Tool::Extend) {
    let (o, d) = view.ray(&app.cam, pos);
    let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
    let mut params = json!({"sketch": sk.0, "at": plane.to_local(hit)});
    if let Some(Pick::SketchCurve { curve, .. }) = app.hover {
      params["curve"] = json!(curve.0);
    }
    let cmd = if app.tool == Tool::Trim { "sketch.trim" } else { "sketch.extend" };
    app.run(cmd, params);
    return;
  }
  if app.tool.picks_curves() {
    curve_tool_click(app, view, pos);
    return;
  }
  let Some((p, snapped)) = sketch_cursor(app, view, pos) else { return };
  if app.tool == Tool::Line && app.palette.sketch_3d && line_3d_click(app, view, pos, sk, &plane, p) {
    return;
  }
  if matches!(app.tool, Tool::Spline | Tool::ControlSpline) {
    // clicking the last point again finishes the spline
    if app.tstate.pts.last().is_some_and(|l| (l[0] - p[0]).hypot(l[1] - p[1]) < 1e-9) {
      finish_spline(app);
    } else {
      app.tstate.pts.push(p);
    }
    return;
  }
  place_point(app, sk, p, snapped);
}

/// How far from the screen point a model vertex still takes a 3D Sketch point (points).
const VERTEX_SNAP_PX: f32 = 8.0;

/// The model vertex nearest the screen point `pos` within [`VERTEX_SNAP_PX`], in the world.
fn vertex_near(app: &App, view: &View, pos: Pos2) -> Option<DVec3> {
  let mut best: Option<(f32, DVec3)> = None;
  for b in app.s.model.bodies.iter().filter(|b| app.s.body_visible(b.id) && !b.is_mesh()) {
    for (_, _, w) in fox_engine::scene::body_vertices(b) {
      let Some(at) = view.to_screen(&app.cam, w) else { continue };
      let d = at.distance(pos);
      if d <= VERTEX_SNAP_PX && best.is_none_or(|(bd, _)| d < bd) {
        best = Some((d, w));
      }
    }
  }
  best.map(|b| b.1)
}

/// The Line tool with the palette's 3D Sketch on, as Fusion runs it: a click near a model
/// vertex takes the vertex where it is in 3D, off the sketch plane, and a segment with an end
/// off the plane is a 3D sketch line (`sketch.line3d`). Returns false for a click the plane
/// tool takes as usual (both ends on the plane); `p` is the plane cursor.
fn line_3d_click(app: &mut App, view: &View, pos: Pos2, sk: FeatureId, plane: &Plane, p: [f64; 2]) -> bool {
  let target = vertex_near(app, view, pos).unwrap_or_else(|| plane.to_world(p));
  let off = |w: DVec3| (w - plane.origin).dot(plane.normal()).abs() > 1e-6;
  let prev = if app.tstate.pts.is_empty() { None } else { app.tstate.last3d };
  app.tstate.last3d = Some(target);
  if !off(target) && prev.is_none_or(|w| !off(w)) {
    return false;
  }
  app.tstate.hud = HeadsUp::default();
  if let Some(a) = prev {
    let made = app.run("sketch.line3d", json!({"sketch": sk.0, "points": [a.to_array(), target.to_array()]}));
    if made.is_none() {
      app.tstate.last3d = Some(a);
      return true;
    }
  }
  // the chain carries on from here (its plane tools see the point's shadow on the plane)
  app.tstate.pts.push(plane.to_local(target));
  true
}

/// Adds the active tool's next point; the last one draws the shape. Values typed in the heads-up
/// boxes override the cursor and become driving dimensions.
fn place_point(app: &mut App, sk: FeatureId, p: [f64; 2], snapped: Option<PointId>) {
  let step_locks = hud_locks(app);
  // a value typed in a heads-up box joins a dimension box's recent values, as in Fusion
  for (f, t) in &step_locks {
    if !matches!(f, Field::Sides | Field::Rho) {
      remember_dim_value(app, t);
    }
  }
  let (p, snapped) = if step_locks.is_empty() { (p, snapped) } else { (hud_apply(app, p), None) };
  let rho = conic_rho(app);
  app.tstate.hud = HeadsUp::default();
  // values typed at earlier steps are locked along with this step's when the shape is drawn
  app.tstate.held.extend(step_locks);
  let locks = app.tstate.held.clone();
  let pts = &mut app.tstate.pts;
  pts.push(p);
  let n = pts.len();
  let first = pts[0];
  let sid = sk.0;
  let chain_from = (n >= 2).then(|| pts[n - 2]);
  let cmd: Option<(&str, serde_json::Value)> = match app.tool {
    Tool::Line if n >= 2 => {
      let a = pts[n - 2];
      // clicking the chain's first point closes the loop and ends the chain
      let closes = n > 2 && (p[0] - first[0]).abs() < 1e-9 && (p[1] - first[1]).abs() < 1e-9;
      let ends_on_existing = snapped.is_some() && n > 2;
      let r = Some(("sketch.line", json!({"sketch": sid, "points": [a, p]})));
      if closes || ends_on_existing {
        pts.clear();
      }
      r
    }
    Tool::Rect if n == 2 => Some(("sketch.rect", json!({"sketch": sid, "corner1": first, "corner2": p}))),
    Tool::CenterRect if n == 2 => Some(("sketch.rect", json!({"sketch": sid, "center": first, "corner": p}))),
    Tool::Circle if n == 2 => match locks.iter().find(|l| l.0 == Field::Diameter) {
      // a typed diameter is drawn with its driving dimension
      Some((_, d)) => Some(("sketch.circle", json!({"sketch": sid, "center": first, "diameter": d}))),
      None => {
        let r = (p[0] - first[0]).hypot(p[1] - first[1]);
        Some(("sketch.circle", json!({"sketch": sid, "center": first, "radius": r})))
      }
    },
    Tool::Arc3 if n == 3 => Some(("sketch.arc", json!({"sketch": sid, "start": first, "end": pts[1], "mid": p}))),
    Tool::CenterArc if n == 3 => {
      // the arc runs the way the cursor wound around the centre
      let clockwise = app.tstate.arc_wind.take().is_some_and(|(_, sweep)| sweep < 0.0);
      Some(("sketch.arc", json!({"sketch": sid, "center": first, "start": pts[1], "end": p, "clockwise": clockwise})))
    }
    Tool::Polygon if n == 2 => {
      Some(("sketch.polygon", json!({"sketch": sid, "center": first, "vertex": p, "sides": hud_sides(&locks)})))
    }
    Tool::Slot if n == 3 => {
      let (a, b) = (first, pts[1]);
      let w = 2.0 * point_line_dist(p, a, b);
      Some(("sketch.slot", json!({"sketch": sid, "start": a, "end": b, "width": w.max(0.1)})))
    }
    Tool::Point => Some(("sketch.point", json!({"sketch": sid, "at": p}))),
    Tool::Rect3 if n == 3 => Some(("sketch.rect", json!({"sketch": sid, "points": [first, pts[1], p]}))),
    Tool::Circle2P if n == 2 => Some(("sketch.circle", json!({"sketch": sid, "through": [first, p]}))),
    Tool::Circle3P if n == 3 => Some(("sketch.circle", json!({"sketch": sid, "through": [first, pts[1], p]}))),
    Tool::CircPolygon if n == 2 => Some((
      "sketch.polygon",
      json!({"sketch": sid, "center": first, "vertex": p, "sides": hud_sides(&locks), "circumscribed": true}),
    )),
    Tool::EdgePolygon if n == 3 => {
      // the third click says which side of the edge the polygon goes (the engine builds it on
      // the edge's left)
      let (a, b) = (first, pts[1]);
      let left = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.0;
      let edge = if left { [a, b] } else { [b, a] };
      Some(("sketch.polygon", json!({"sketch": sid, "edge": edge, "sides": hud_sides(&locks)})))
    }
    Tool::OverallSlot | Tool::CenterSlot if n == 3 => {
      let w = 2.0 * point_line_dist(p, first, pts[1]);
      let mut q = json!({"sketch": sid, "end": pts[1], "width": w.max(0.1)});
      if app.tool == Tool::CenterSlot {
        q["center"] = json!(first);
      } else {
        q["start"] = json!(first);
        q["kind"] = json!("overall");
      }
      Some(("sketch.slot", q))
    }
    Tool::ArcSlot3 | Tool::ArcSlotCenter if n == 4 => {
      // the width is twice the last click's distance from the arc; a 3-point arc slot's clicks
      // run start, end, then a point on the arc, as in Fusion
      let (a, b, c) = (first, pts[1], pts[2]);
      let (center, r) = if app.tool == Tool::ArcSlotCenter {
        (Some(a), (b[0] - a[0]).hypot(b[1] - a[1]))
      } else {
        let o = fox_engine::sketch::circumcenter(a, c, b);
        (o, o.map(|o| (a[0] - o[0]).hypot(a[1] - o[1])).unwrap_or(0.0))
      };
      let w = center.map(|o| 2.0 * ((p[0] - o[0]).hypot(p[1] - o[1]) - r).abs()).unwrap_or(1.0).max(0.1);
      let q = if app.tool == Tool::ArcSlotCenter {
        json!({"sketch": sid, "arc_center": a, "start": b, "end": c, "width": w})
      } else {
        json!({"sketch": sid, "start": a, "mid": c, "end": b, "width": w})
      };
      Some(("sketch.slot", q))
    }
    Tool::Ellipse if n == 3 => match Ellipse2::minor_through(first, pts[1], p) {
      // the ellipse passes through the third pick, as in Fusion
      Some(_) => Some(("sketch.ellipse", json!({"sketch": sid, "center": first, "major": pts[1], "point": p}))),
      None => {
        app.status = Some(("Ellipse: the third point must lie off the axis and within its length".into(), true));
        app.tstate.pts.pop();
        None
      }
    },
    // the vertex click leaves the conic shaping (its rho box and arrow); Enter or a click commits
    Tool::Conic if n == 4 => {
      Some(("sketch.conic", json!({"sketch": sid, "start": first, "end": pts[1], "apex": pts[2], "rho": rho})))
    }
    Tool::MidpointLine if n == 2 => {
      // the engine mirrors the end through the midpoint and keeps the midpoint on the line's middle
      Some(("sketch.line", json!({"sketch": sid, "mid": first, "end": p})))
    }
    Tool::TangentArc if n == 2 => match tangent_curve(app, sk, first) {
      Some(curve) => Some(("sketch.arc", json!({"sketch": sid, "tangent": curve, "start": first, "end": p}))),
      None => {
        app.status = Some(("Tangent arc: start on the end of a line or arc".into(), true));
        app.tstate.pts.clear();
        None
      }
    },
    _ => None,
  };
  if let Some((c, params)) = cmd {
    let multi = app.tool == Tool::Line;
    if !multi {
      app.tstate.pts.clear();
    }
    app.tstate.held.clear();
    app.tstate.rho = None;
    // the shape and the dimensions locking its typed values are one undo step
    let grouped = !locks.is_empty() && app.run("transaction.begin", json!({})).is_some();
    let made = app.run(c, params);
    if let Some(r) = &made {
      lock_typed(app, sk, r, &locks, chain_from);
    }
    if grouped {
      app.run("transaction.commit", json!({}));
    }
    if made.is_none() && multi {
      // a failed segment drops its point so the chain continues from the last good one
      app.tstate.pts.pop();
    }
  }
}

/// The heads-up values typed for the shape being drawn, with their text.
fn hud_locks(app: &App) -> Vec<(Field, String)> {
  let fields = hud_fields(app.tool, hud_step(app));
  fields
    .iter()
    .enumerate()
    .filter(|(i, _)| hud_value(app, *i).is_some())
    .map(|(i, f)| (*f, hud_text(app, *f, &app.tstate.hud.typed[i])))
    .collect()
}

/// Heads-up box text as an expression, spelt as Fusion accepts it (see [`accepted_text`]): a bare
/// length means the design's unit.
fn hud_text(app: &App, field: Field, text: &str) -> String {
  accepted_text(app.s.design.units, field, text)
}

/// Fusion's spelling of a value once a box accepts it (Tab, Enter): its tokens spaced
/// ("d1 * 2", "3 in / 2"), a bare number given the field's unit ("2 in", "45 deg") and a bare
/// expression multiplied by one unit ("( 1 / 2 ) * 1 in"). Text that doesn't parse is kept.
fn accepted_text(unit: fox_engine::model::LengthUnit, field: Field, text: &str) -> String {
  use fox_engine::expr::{Empty, eval_raw, spaced};
  let t = text.trim();
  let spaced = spaced(t).unwrap_or_else(|| t.to_string());
  let sym = match field {
    Field::Angle => "deg",
    Field::Sides | Field::Rho => return spaced,
    _ => unit.symbol(),
  };
  let bare = matches!(eval_raw(t, &Empty), Ok(v) if !v.has_unit);
  if !bare {
    return spaced;
  }
  if t.parse::<f64>().is_ok() { format!("{spaced} {sym}") } else { format!("( {spaced} ) * 1 {sym}") }
}

/// The value typed in the heads-up box setting `field`, if any (see [`hud_value`]).
fn hud_field_value(app: &App, field: Field) -> Option<f64> {
  let i = hud_fields(app.tool, hud_step(app)).iter().position(|f| *f == field)?;
  hud_value(app, i)
}

/// What Tab writes back into heads-up box `i`: its typed text in Fusion's accepted spelling, if it
/// evaluates (a value Fusion refuses is left as typed, padlock and all).
fn hud_accepted(app: &App, i: usize) -> Option<String> {
  let field = *hud_fields(app.tool, hud_step(app)).get(i)?;
  let text = app.tstate.hud.typed.get(i)?.trim();
  if text.is_empty() || hud_value(app, i).is_none() {
    return None;
  }
  Some(accepted_text(app.s.design.units, field, text))
}

/// The number of sides typed in a polygon's heads-up box (Fusion's default is 6).
fn hud_sides(locks: &[(Field, String)]) -> u64 {
  let typed = locks.iter().find(|l| l.0 == Field::Sides).map(|(_, t)| t.as_str());
  typed
    .and_then(|t| fox_engine::expr::eval(t, fox_engine::expr::Kind::Unitless, &fox_engine::expr::Empty).ok())
    .map_or(6, |v| v.round() as u64)
}

/// The value typed in heads-up box `i` (mm, or radians for an angle), if any.
fn hud_value(app: &App, i: usize) -> Option<f64> {
  let field = *hud_fields(app.tool, hud_step(app)).get(i)?;
  let text = app.tstate.hud.typed.get(i)?.trim();
  if text.is_empty() {
    return None;
  }
  let kind = match field {
    Field::Angle => fox_engine::expr::Kind::Angle,
    Field::Sides | Field::Rho => fox_engine::expr::Kind::Unitless,
    _ => fox_engine::expr::Kind::Length,
  };
  let v = fox_engine::expr::eval(&hud_text(app, field, text), kind, &app.s.model.params).ok()?;
  match field {
    Field::Angle => Some(v),
    // the polygon command takes 3 to 256 sides
    Field::Sides => (3.0..=256.0).contains(&v).then_some(v.round()),
    Field::Rho => fox_engine::sketch::RHO_RANGE.contains(&v).then_some(v),
    _ => (v > 1e-9).then_some(v),
  }
}

/// The cursor moved to where the typed heads-up values put it.
fn hud_apply(app: &App, cur: [f64; 2]) -> [f64; 2] {
  let pts = app.tstate.pts.as_slice();
  if app.tool == Tool::Circle2T {
    return cur;
  }
  let Some(&last) = pts.last() else { return cur };
  if hud_fields(app.tool, pts.len()).is_empty() {
    return cur;
  }
  let v = |i| hud_value(app, i);
  let (dx, dy) = (cur[0] - last[0], cur[1] - last[1]);
  let sign = |d: f64| if d < 0.0 { -1.0 } else { 1.0 };
  // the point `r` from `o` in the cursor's direction from `o`
  let out = |o: [f64; 2], r: f64| {
    let (dx, dy) = (cur[0] - o[0], cur[1] - o[1]);
    let l = dx.hypot(dy);
    let (ux, uy) = if l > 1e-12 { (dx / l, dy / l) } else { (1.0, 0.0) };
    [o[0] + r * ux, o[1] + r * uy]
  };
  // the point `d` off the line `a`→`b` (its middle `m`), on the cursor's side
  let beside = |a: [f64; 2], b: [f64; 2], m: [f64; 2], d: f64| {
    let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
    let l = ex.hypot(ey).max(1e-12);
    let (nx, ny) = (-ey / l, ex / l);
    let side = sign((cur[0] - m[0]) * nx + (cur[1] - m[1]) * ny);
    [m[0] + nx * d * side, m[1] + ny * d * side]
  };
  match (app.tool, pts) {
    (Tool::Line, _) => {
      let len = v(0).unwrap_or(dx.hypot(dy));
      let ang = v(1).unwrap_or(dy.atan2(dx));
      [last[0] + len * ang.cos(), last[1] + len * ang.sin()]
    }
    // the box holds the whole line's length; the end is half of it from the midpoint
    (Tool::MidpointLine, _) => {
      let len = v(0).map_or(dx.hypot(dy), |l| l / 2.0);
      let ang = v(1).unwrap_or(dy.atan2(dx));
      [last[0] + len * ang.cos(), last[1] + len * ang.sin()]
    }
    (Tool::Rect | Tool::CenterRect, _) => {
      // a centre rectangle's boxes hold its whole width and height
      let k = if app.tool == Tool::CenterRect { 0.5 } else { 1.0 };
      let (w, h) = (hud_field_value(app, Field::Width), hud_field_value(app, Field::Height));
      [w.map_or(cur[0], |w| last[0] + sign(dx) * w * k), h.map_or(cur[1], |h| last[1] + sign(dy) * h * k)]
    }
    (Tool::Rect3, [a, b]) => v(0).map_or(cur, |h| beside(*a, *b, *b, h)),
    (Tool::Circle, _) => v(0).map_or(cur, |d| out(last, d / 2.0)),
    (Tool::Circle2P, _) => v(0).map_or(cur, |d| out(last, d)),
    (Tool::Circle3P, [a, b]) => v(0).map_or(cur, |d| on_circle_through(*a, *b, d / 2.0, cur)),
    (Tool::Arc3 | Tool::ArcSlot3, [a, b]) => v(0).map_or(cur, |r| on_circle_through(*a, *b, r, cur)),
    (Tool::CenterArc | Tool::ArcSlotCenter, [c, a]) => v(0).map_or(cur, |ang| {
      // the end at the typed sweep from the start, the way the cursor went (counter-clockwise
      // until it moves)
      let r = (a[0] - c[0]).hypot(a[1] - c[1]);
      let dir = app.tstate.arc_wind.map_or(1.0, |(_, s)| sign(s));
      let t = (a[1] - c[1]).atan2(a[0] - c[0]) + dir * ang;
      [c[0] + r * t.cos(), c[1] + r * t.sin()]
    }),
    (Tool::TangentArc, [a]) => v(0).map_or(cur, |r| tangent_arc_end(app, *a, r, cur)),
    (Tool::Ellipse, [c, m]) => v(0).map_or(cur, |b| beside(*c, *m, *c, b)),
    (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot, [a, b]) => {
      let m = if app.tool == Tool::CenterSlot { *a } else { [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0] };
      v(0).map_or(cur, |w| beside(*a, *b, m, w / 2.0))
    }
    (Tool::ArcSlot3, [a, b, c]) => match (v(0), fox_engine::sketch::circumcenter(*a, *c, *b)) {
      (Some(w), Some(o)) => out(o, (a[0] - o[0]).hypot(a[1] - o[1]) + w / 2.0),
      _ => cur,
    },
    (Tool::ArcSlotCenter, [c, s, _]) => v(0).map_or(cur, |w| out(*c, (s[0] - c[0]).hypot(s[1] - c[1]) + w / 2.0)),
    // a typed length, radius or diameter holds the point that far from the last click
    (Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon, _)
    | (Tool::Rect3, _)
    | (Tool::CenterArc | Tool::ArcSlotCenter, _)
    | (Tool::Ellipse, _)
    | (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot | Tool::ArcSlot3, _) => v(0).map_or(cur, |r| out(last, r)),
    _ => cur,
  }
}

/// The point on the circle of radius `r` through `a` and `b` nearest the cursor, with the circle's
/// centre on the cursor's side of the chord (the cursor itself when `r` can't span the chord).
fn on_circle_through(a: [f64; 2], b: [f64; 2], r: f64, cur: [f64; 2]) -> [f64; 2] {
  let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
  let l = ex.hypot(ey);
  if l < 1e-12 || r < l / 2.0 {
    return cur;
  }
  let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
  let (nx, ny) = (-ey / l, ex / l);
  let side = if (cur[0] - m[0]) * nx + (cur[1] - m[1]) * ny < 0.0 { -1.0 } else { 1.0 };
  let h = (r * r - l * l / 4.0).max(0.0).sqrt();
  let c = [m[0] + nx * h * side, m[1] + ny * h * side];
  let (dx, dy) = (cur[0] - c[0], cur[1] - c[1]);
  let d = dx.hypot(dy);
  if d < 1e-12 { [c[0] - nx * r * side, c[1] - ny * r * side] } else { [c[0] + r * dx / d, c[1] + r * dy / d] }
}

/// The end of the tangent arc of radius `r` from `start` that bends towards the cursor, at the
/// cursor's angle around the arc's centre (the cursor itself when nothing ends at `start`).
fn tangent_arc_end(app: &App, start: [f64; 2], r: f64, cur: [f64; 2]) -> [f64; 2] {
  let Some(dir) = tangent_direction(app, start) else { return cur };
  let n = [-dir[1], dir[0]];
  let side = if (cur[0] - start[0]) * n[0] + (cur[1] - start[1]) * n[1] < 0.0 { -1.0 } else { 1.0 };
  let c = [start[0] + n[0] * r * side, start[1] + n[1] * r * side];
  let (dx, dy) = (cur[0] - c[0], cur[1] - c[1]);
  let d = dx.hypot(dy);
  if d < 1e-12 { cur } else { [c[0] + r * dx / d, c[1] + r * dy / d] }
}

/// The values the heads-up boxes show for a cursor at `cur` (mm, degrees, a count or rho).
fn hud_live(app: &App, cur: [f64; 2]) -> [f64; 2] {
  let pts = app.tstate.pts.as_slice();
  if app.tool == Tool::Circle2T {
    let curves: Vec<u32> = app.tstate.curves.iter().map(|c| c.0).collect();
    return [app.s.edit_sketch.map_or(0.0, |sk| tangent_radius(app, sk, &curves, cur)), 0.0];
  }
  let Some(&last) = pts.last() else { return [0.0; 2] };
  let (dx, dy) = (cur[0] - last[0], cur[1] - last[1]);
  let d = dx.hypot(dy);
  let ang = dy.atan2(dx).to_degrees().rem_euclid(360.0);
  let radius_through = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
    fox_engine::sketch::circumcenter(a, b, c).map_or(0.0, |o| (a[0] - o[0]).hypot(a[1] - o[1]))
  };
  match (app.tool, pts) {
    (Tool::Line, _) => [d, ang],
    (Tool::MidpointLine, _) => [2.0 * d, ang],
    (Tool::Rect, _) => [dy.abs(), dx.abs()],
    (Tool::CenterRect, _) => [2.0 * dx.abs(), 2.0 * dy.abs()],
    (Tool::Rect3, [a, b]) => [point_line_dist(cur, *a, *b), 0.0],
    (Tool::Circle, _) => [2.0 * d, 0.0],
    (Tool::Circle2P, _) => [d, 0.0],
    (Tool::Circle3P, [a, b]) => [2.0 * radius_through(*a, *b, cur), 0.0],
    (Tool::Arc3 | Tool::ArcSlot3, [a, b]) => [radius_through(*a, cur, *b), 0.0],
    (Tool::CenterArc, [_, _]) => [app.tstate.arc_wind.map_or(0.0, |(_, s)| s.to_degrees().abs()), 0.0],
    (Tool::ArcSlotCenter, [c, s]) => {
      let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
      [((cur[1] - c[1]).atan2(cur[0] - c[0]) - a0).rem_euclid(std::f64::consts::TAU).to_degrees(), 0.0]
    }
    (Tool::TangentArc, [a]) => [tangent_arc_geometry(app, *a, cur).map_or(0.0, |(_, r, _, _)| r), 0.0],
    (Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon, _) => [d, 6.0],
    (Tool::Ellipse, [c, m]) => {
      [Ellipse2::minor_through(*c, *m, cur).unwrap_or_else(|| point_line_dist(cur, *c, *m)), 0.0]
    }
    (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot, [a, b]) => [2.0 * point_line_dist(cur, *a, *b), 0.0],
    (Tool::ArcSlot3, [a, b, c]) => {
      let w = fox_engine::sketch::circumcenter(*a, *c, *b)
        .map_or(0.0, |o| 2.0 * ((cur[0] - o[0]).hypot(cur[1] - o[1]) - (a[0] - o[0]).hypot(a[1] - o[1])).abs());
      [w, 0.0]
    }
    (Tool::ArcSlotCenter, [c, s, _]) => {
      [2.0 * ((cur[0] - c[0]).hypot(cur[1] - c[1]) - (s[0] - c[0]).hypot(s[1] - c[1])).abs(), 0.0]
    }
    (Tool::Conic, [_, _, _]) => [conic_rho(app), 0.0],
    // a length, radius or diameter from the last click
    (Tool::Rect3 | Tool::CenterArc | Tool::ArcSlotCenter | Tool::Ellipse, _)
    | (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot | Tool::ArcSlot3, _) => [d, 0.0],
    _ => [0.0; 2],
  }
}

/// The corners of a regular `n`-gon about `c`, `r` from the centre, the first at angle `a0`.
fn polygon_points(c: [f64; 2], r: f64, a0: f64, n: usize) -> Vec<[f64; 2]> {
  (0..n)
    .map(|i| {
      let a = a0 + std::f64::consts::TAU * i as f64 / n as f64;
      [c[0] + r * a.cos(), c[1] + r * a.sin()]
    })
    .collect()
}

/// The regular `n`-gon with `a`→`b` as one side, its centre on the side's left (as `sketch.polygon`
/// builds an edge polygon).
fn edge_polygon_points(a: [f64; 2], b: [f64; 2], n: usize) -> Vec<[f64; 2]> {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let side = dx.hypot(dy);
  if side < 1e-12 {
    return Vec::new();
  }
  let half = std::f64::consts::PI / n as f64;
  let apothem = side / (2.0 * half.tan());
  let c = [(a[0] + b[0]) / 2.0 - dy / side * apothem, (a[1] + b[1]) / 2.0 + dx / side * apothem];
  polygon_points(c, side / (2.0 * half.sin()), (a[1] - c[1]).atan2(a[0] - c[0]), n)
}

/// The outline of a slot between the end centres `a` and `b`, `w` wide: two lines and two round
/// caps, as one closed polyline.
fn slot_outline(a: [f64; 2], b: [f64; 2], w: f64) -> Vec<[f64; 2]> {
  let l = (b[0] - a[0]).hypot(b[1] - a[1]);
  if l < 1e-12 || w <= 0.0 {
    return Vec::new();
  }
  let r = w / 2.0;
  let n = [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
  let an = n[1].atan2(n[0]);
  let pi = std::f64::consts::PI;
  let mut out = vec![[a[0] - n[0] * r, a[1] - n[1] * r]];
  // the end cap round the far side, back along the other line, the start cap round the near side
  out.extend(arc_points(b, r, an - pi, pi));
  out.extend(arc_points(a, r, an, pi));
  out
}

/// The outline of a slot `w` wide along the arc about `c` of radius `r` from `a0` sweeping
/// `sweep`: outer and inner arcs joined by round caps, as one closed polyline.
fn arc_slot_outline(c: [f64; 2], r: f64, a0: f64, sweep: f64, w: f64) -> Vec<[f64; 2]> {
  let (ro, ri) = (r + w / 2.0, (r - w / 2.0).max(0.0));
  let pi = std::f64::consts::PI.copysign(sweep);
  let a1 = a0 + sweep;
  let cap = |ang: f64, from: f64| arc_points([c[0] + r * ang.cos(), c[1] + r * ang.sin()], w / 2.0, from, pi);
  let mut out = arc_points(c, ro, a0, sweep);
  out.extend(cap(a1, a1));
  out.extend(arc_points(c, ri, a1, -sweep));
  out.extend(cap(a0, a0 + std::f64::consts::PI));
  out
}

/// The number of sides the polygon being drawn will have: what its heads-up box holds, else 6.
fn hud_sides_live(app: &App) -> usize {
  let i = hud_fields(app.tool, hud_step(app)).iter().position(|f| *f == Field::Sides);
  i.and_then(|i| hud_value(app, i)).map_or(6, |v| v as usize)
}

/// A heads-up box's live value as Fusion shows it: a length with its unit at the document's
/// display precision ("0.492 in"), an angle to one decimal with "deg" ("336.8 deg"), a count as it
/// is ("6"). Each is also a valid expression to type back.
fn hud_display(unit: fox_engine::model::LengthUnit, field: Field, v: f64) -> String {
  match field {
    Field::Angle => format!("{v:.p$} deg", p = crate::units::angle_decimals()),
    Field::Sides => format!("{}", v.round() as i64),
    Field::Rho => format!("{v:.2}"),
    _ => unit.fixed(v, display_decimals(unit)),
  }
}

/// The decimals lengths are shown with: Preferences' General precision, else Fusion's default
/// display precision for the unit (see [`crate::units::length_decimals`]).
pub(crate) fn display_decimals(unit: fox_engine::model::LengthUnit) -> usize {
  crate::units::length_decimals(unit)
}

/// Adds the driving dimensions that lock the typed values of a shape just drawn (`made` is the
/// draw command's result; a line starts at `from`).
fn lock_typed(
  app: &mut App,
  sk: FeatureId,
  made: &serde_json::Value,
  locks: &[(Field, String)],
  from: Option<[f64; 2]>,
) {
  let curves: Vec<u32> = made["curves"]
    .as_array()
    .map(|a| a.iter().filter_map(|v| v.as_u64()).map(|v| v as u32).collect())
    .unwrap_or_default();
  if matches!(app.tool, Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon) {
    lock_polygon(app, sk, &curves, locks);
    return;
  }
  for (field, text) in locks {
    // rectangles are drawn bottom, right, top, left: the bottom sets the width, the right the
    // height (a 3-point rectangle's first side, then its second); a circle's diameter and an
    // arc's radius dimension the curve itself
    let (kind, curve) = match (app.tool, field) {
      (Tool::Line | Tool::MidpointLine | Tool::Rect | Tool::CenterRect | Tool::Rect3, Field::Width | Field::Length)
        if !(app.tool == Tool::Rect3 && *field == Field::Width) =>
      {
        ("distance", curves.first())
      }
      (Tool::Rect | Tool::CenterRect, Field::Height) | (Tool::Rect3, Field::Width) => ("distance", curves.get(1)),
      // (the Circle tool draws a typed diameter with its dimension itself: `sketch.circle {diameter}`)
      (Tool::Circle2P | Tool::Circle3P, Field::Diameter) => ("diameter", curves.first()),
      (Tool::Arc3 | Tool::CenterArc | Tool::TangentArc, Field::Radius) => ("radius", curves.first()),
      _ => ("", None),
    };
    if let Some(c) = curve {
      app.run("sketch.dimension", json!({"sketch": sk.0, "type": kind, "curves": [c], "value": text}));
    }
    if *field == Field::Angle
      && app.tool == Tool::Line
      && let (Some(&line), Some(from)) = (curves.first(), from)
    {
      lock_angle(app, sk, line, from, text);
    }
  }
}

/// Locks a polygon's typed radius as Fusion dimensions it: centre to the first vertex (inscribed)
/// or centre to the first side (circumscribed); an edge polygon's typed length dimensions its first
/// side. `curves` is `sketch.polygon`'s result, the sides; the centre is its Polygon constraint's.
fn lock_polygon(app: &mut App, sk: FeatureId, curves: &[u32], locks: &[(Field, String)]) {
  let Some(&side) = curves.first() else { return };
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let side_id = fox_engine::sketch::CurveId(side);
  let center = sketch.constraints.iter().find_map(|c| match &c.kind {
    ConstraintKind::Polygon { center, lines } if lines.contains(&side_id) => Some(*center),
    _ => None,
  });
  let (Some(center), Some(vertex)) = (center, sketch.curve_points(side_id).first().copied()) else { return };
  for (field, text) in locks {
    let mut params = match (app.tool, field) {
      (Tool::Polygon, Field::Radius) => json!({"type": "distance", "points": [center.0, vertex.0], "value": text}),
      (Tool::CircPolygon, Field::Radius) => {
        json!({"type": "point_line", "points": [center.0], "curves": [side], "value": text})
      }
      (Tool::EdgePolygon, Field::Length) => json!({"type": "distance", "curves": [side], "value": text}),
      _ => continue,
    };
    params["sketch"] = json!(sk.0);
    app.run("sketch.dimension", params);
  }
}

/// Locks a line's typed angle the way Fusion does: a dashed horizontal construction line as long
/// as the segment runs from its start (`from`), and an angle dimension holds `text` between that
/// reference and the line. A horizontal or vertical segment is locked by its H/V constraint
/// already, so it gets neither.
fn lock_angle(app: &mut App, sk: FeatureId, line: u32, from: [f64; 2], text: &str) {
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let ends = sketch.curve_points(fox_engine::sketch::CurveId(line));
  let (Some(p), Some(q)) = (ends.first().and_then(|p| sketch.pos(*p)), ends.get(1).and_then(|p| sketch.pos(*p))) else {
    return;
  };
  if (p[0] - q[0]).abs() < 1e-9 || (p[1] - q[1]).abs() < 1e-9 {
    return;
  }
  let len = (q[0] - p[0]).hypot(q[1] - p[1]);
  let reference = [from, [from[0] + len, from[1]]];
  let Some(made) = app.run("sketch.line", json!({"sketch": sk.0, "points": reference})) else { return };
  let Some(reference) = made["curves"][0].as_u64() else { return };
  app.run("sketch.construction", json!({"sketch": sk.0, "curves": [reference], "on": true}));
  app.run("sketch.dimension", json!({"sketch": sk.0, "type": "angle", "curves": [reference, line], "value": text}));
}

/// Commits the spline being drawn (Enter, its check-mark button, or clicking its last point
/// again). Returns whether there was a spline to commit.
pub fn finish_spline(app: &mut App) -> bool {
  if !matches!(app.tool, Tool::Spline | Tool::ControlSpline) || app.tstate.pts.len() < 2 {
    return false;
  }
  let pts = std::mem::take(&mut app.tstate.pts);
  let Some(sk) = app.s.edit_sketch else { return false };
  let cmd = if app.tool == Tool::Spline { "sketch.spline" } else { "sketch.control_spline" };
  app.run(cmd, json!({"sketch": sk.0, "points": pts}));
  true
}

/// The line or arc that ends at a sketch point (a tangent arc starts there).
fn tangent_curve(app: &App, sk: FeatureId, at: [f64; 2]) -> Option<u32> {
  let sketch = app.s.design.sketch(sk).ok()?;
  let near = |id: PointId| sketch.pos(id).is_some_and(|q| (q[0] - at[0]).hypot(q[1] - at[1]) < 1e-6);
  sketch.curves.iter().rev().find_map(|c| match c.kind {
    CurveKind::Line { a, b } if near(a) || near(b) => Some(c.id.0),
    CurveKind::Arc { start, end, .. } if near(start) || near(end) => Some(c.id.0),
    _ => None,
  })
}

/// Line: pressing on the end of the last segment and dragging draws a tangent arc to where the
/// drag ends, as in Fusion; the chain then carries on with lines from the arc's end. Returns true
/// while such a drag has the pointer.
fn line_arc_drag(app: &mut App, resp: &egui::Response, view: &View, ctx: &egui::Context, pos: Pos2) -> bool {
  if app.tool != Tool::Line {
    return false;
  }
  let Some((sk, plane)) = sketch_plane(app) else { return false };
  if resp.drag_started_by(egui::PointerButton::Primary)
    && let Some(&last) = app.tstate.pts.last()
    && tangent_curve(app, sk, last).is_some()
    && let Some(at) = view.to_screen(&app.cam, plane.to_world(last))
    && ctx.input(|i| i.pointer.press_origin()).is_some_and(|p| (p - at).length() <= SNAP_PX)
  {
    app.tstate.line_arc = true;
  }
  if !app.tstate.line_arc {
    return false;
  }
  if resp.drag_stopped() {
    app.tstate.line_arc = false;
    if let Some((end, _)) = sketch_cursor(app, view, pos) {
      line_tangent_arc(app, sk, end);
    }
  }
  true
}

/// Draws the Line tool's tangent arc from the chain's last point to `end`; the chain continues
/// from `end`.
fn line_tangent_arc(app: &mut App, sk: FeatureId, end: [f64; 2]) {
  let Some(&start) = app.tstate.pts.last() else { return };
  if (end[0] - start[0]).hypot(end[1] - start[1]) < 1e-9 {
    return;
  }
  let Some(curve) = tangent_curve(app, sk, start) else { return };
  app.tstate.hud = HeadsUp::default();
  if app.run("sketch.arc", json!({"sketch": sk.0, "tangent": curve, "start": start, "end": end})).is_some() {
    app.tstate.pts.push(end);
  }
}

/// The direction a line or arc ending at `start` leaves it in (a tangent arc carries on that way).
fn tangent_direction(app: &App, start: [f64; 2]) -> Option<[f64; 2]> {
  let sk = app.s.edit_sketch?;
  let curve = fox_engine::sketch::CurveId(tangent_curve(app, sk, start)?);
  let sketch = app.s.design.sketch(sk).ok()?;
  let dir = match sketch.curve(curve)?.kind {
    CurveKind::Line { a, b } => {
      let (pa, pb) = (sketch.pos(a)?, sketch.pos(b)?);
      let at_b = (pb[0] - start[0]).hypot(pb[1] - start[1]) <= (pa[0] - start[0]).hypot(pa[1] - start[1]);
      if at_b { [pb[0] - pa[0], pb[1] - pa[1]] } else { [pa[0] - pb[0], pa[1] - pb[1]] }
    }
    CurveKind::Arc { center, end: e, .. } => {
      let c = sketch.pos(center)?;
      let r = [start[0] - c[0], start[1] - c[1]];
      let ccw = [-r[1], r[0]];
      let at_end = sketch.pos(e).is_some_and(|e| (e[0] - start[0]).hypot(e[1] - start[1]) < 1e-6);
      if at_end { ccw } else { [-ccw[0], -ccw[1]] }
    }
    _ => return None,
  };
  let l = dir[0].hypot(dir[1]);
  (l >= 1e-12).then(|| [dir[0] / l, dir[1] / l])
}

/// The tangent arc from `start` (the end of a line or arc) to `end`, as the engine's
/// `sketch.arc {tangent}` draws it: centre, radius, start angle and signed sweep, or None when
/// nothing ends at `start` or the end lies straight ahead.
fn tangent_arc_geometry(app: &App, start: [f64; 2], end: [f64; 2]) -> Option<([f64; 2], f64, f64, f64)> {
  let dir = tangent_direction(app, start)?;
  let n = [-dir[1], dir[0]];
  let chord = [end[0] - start[0], end[1] - start[1]];
  let den = 2.0 * (chord[0] * n[0] + chord[1] * n[1]);
  if den.abs() < 1e-12 {
    return None;
  }
  let t = (chord[0] * chord[0] + chord[1] * chord[1]) / den;
  let c = [start[0] + n[0] * t, start[1] + n[1] * t];
  let r = t.abs();
  let (a0, a1) = ((start[1] - c[1]).atan2(start[0] - c[0]), (end[1] - c[1]).atan2(end[0] - c[0]));
  let tau = std::f64::consts::TAU;
  // counter-clockwise when the centre is on the left of the way the curve runs
  let sweep = if t > 0.0 { (a1 - a0).rem_euclid(tau) } else { -(a0 - a1).rem_euclid(tau) };
  Some((c, r, a0, sweep))
}

/// Points along the tangent arc from `start` to `end` (see [`tangent_arc_geometry`]).
fn tangent_arc_points(app: &App, start: [f64; 2], end: [f64; 2]) -> Option<Vec<[f64; 2]>> {
  let (c, r, a0, sweep) = tangent_arc_geometry(app, start, end)?;
  Some(arc_points(c, r, a0, sweep))
}

/// Points along the arc of radius `r` about `c` from angle `a0` sweeping `sweep` (signed).
fn arc_points(c: [f64; 2], r: f64, a0: f64, sweep: f64) -> Vec<[f64; 2]> {
  (0..=64)
    .map(|i| {
      let a = a0 + sweep * i as f64 / 64.0;
      [c[0] + r * a.cos(), c[1] + r * a.sin()]
    })
    .collect()
}

/// The arc from `a` to `b` through `m`: centre, radius, start angle and signed sweep.
fn arc_through(a: [f64; 2], m: [f64; 2], b: [f64; 2]) -> Option<([f64; 2], f64, f64, f64)> {
  let o = fox_engine::sketch::circumcenter(a, m, b)?;
  let r = (a[0] - o[0]).hypot(a[1] - o[1]);
  let ang = |p: [f64; 2]| (p[1] - o[1]).atan2(p[0] - o[0]);
  let tau = std::f64::consts::TAU;
  let (a0, ccw) = (ang(a), (ang(b) - ang(a)).rem_euclid(tau));
  // the sweep runs the way round that passes through `m`
  let sweep = if (ang(m) - a0).rem_euclid(tau) <= ccw { ccw } else { ccw - tau };
  Some((o, r, a0, sweep))
}

/// Trim: dragging across curves trims each one the pointer crosses (once per drag), as in
/// Fusion. Returns true while such a drag is going on.
fn trim_drag(app: &mut App, resp: &egui::Response, view: &View) -> bool {
  if app.tool != Tool::Trim {
    return false;
  }
  if resp.drag_started_by(egui::PointerButton::Primary) {
    app.tstate.curves.clear();
  }
  if resp.drag_stopped() {
    app.tstate.curves.clear();
    return true;
  }
  if !resp.dragged_by(egui::PointerButton::Primary) {
    return false;
  }
  let Some((sk, plane)) = sketch_plane(app) else { return true };
  if let Some(Pick::SketchCurve { sketch, curve }) = app.hover
    && sketch == sk
    && !app.tstate.curves.iter().any(|c| c.0 == curve.0)
    && let Some(pos) = resp.interact_pointer_pos()
  {
    let (o, d) = view.ray(&app.cam, pos);
    let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return true };
    let at = plane.to_local(hit);
    app.tstate.curves.push((curve.0, at));
    if let Some(r) = app.run("sketch.trim", json!({"sketch": sk.0, "curve": curve.0, "at": at})) {
      // the pieces left are done for this drag too (the frame under the pointer is a step behind)
      for c in r["curves"].as_array().into_iter().flatten().filter_map(|v| v.as_u64()) {
        app.tstate.curves.push((c as u32, at));
      }
    }
  }
  true
}

/// The piece of `curve` that Trim at `at` removes (between the nearest crossings, or the whole
/// curve), as polylines in sketch coordinates.
pub(crate) fn trim_piece(sketch: &Sketch, curve: fox_engine::sketch::CurveId, at: [f64; 2]) -> Vec<Vec<[f64; 2]>> {
  let Some(c) = sketch.curve(curve) else { return Vec::new() };
  let line = fox_engine::scene::curve_polyline(sketch, &c.kind);
  let mut after = sketch.clone();
  let Ok(kept) = after.trim(curve, at) else { return Vec::new() };
  let tol = polyline_tol(&line);
  let removed = |p: [f64; 2]| !kept.iter().any(|k| after.distance_to_curve(*k, p).is_some_and(|d| d <= tol));
  runs_of(&line, removed)
}

/// The piece Extend at `at` adds to `curve` (from its end nearest `at` to the curve it reaches),
/// as polylines in sketch coordinates; empty when there is nothing to extend to.
pub(crate) fn extend_piece(sketch: &Sketch, curve: fox_engine::sketch::CurveId, at: [f64; 2]) -> Vec<Vec<[f64; 2]>> {
  let mut after = sketch.clone();
  if after.extend(curve, at).is_err() {
    return Vec::new();
  }
  let Some(c) = after.curve(curve) else { return Vec::new() };
  let line = fox_engine::scene::curve_polyline(&after, &c.kind);
  let tol = polyline_tol(&line);
  let added = |p: [f64; 2]| !sketch.distance_to_curve(curve, p).is_some_and(|d| d <= tol);
  runs_of(&line, added)
}

/// The points Break at `at` splits `curve` at (its nearest crossings on each side of `at`, or
/// `at` itself when nothing crosses it), in sketch coordinates; empty when it can't be broken.
pub(crate) fn break_points(sketch: &Sketch, curve: fox_engine::sketch::CurveId, at: [f64; 2]) -> Vec<[f64; 2]> {
  let mut after = sketch.clone();
  if after.break_curve(curve, at, false, 0.0).is_err() {
    return Vec::new();
  }
  after.points.iter().filter(|p| sketch.pos(p.id).is_none()).map(|p| [p.x, p.y]).collect()
}

/// How close to a curve a point counts as on it, for a curve of this polyline's size.
fn polyline_tol(line: &[[f64; 2]]) -> f64 {
  let size = line.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f64>();
  (size * 1e-4).max(1e-9)
}

/// The runs of a curve's polyline where `removed` holds (sampled along each chord, each run
/// ending where `removed` flips, found by bisection), as polylines.
fn runs_of(line: &[[f64; 2]], removed: impl Fn([f64; 2]) -> bool) -> Vec<Vec<[f64; 2]>> {
  const SPLIT: usize = 8;
  let mut samples: Vec<[f64; 2]> = Vec::new();
  for w in line.windows(2) {
    for i in 0..SPLIT {
      let k = i as f64 / SPLIT as f64;
      samples.push([w[0][0] + (w[1][0] - w[0][0]) * k, w[0][1] + (w[1][1] - w[0][1]) * k]);
    }
  }
  samples.extend(line.last().copied());
  let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
  let mut cur: Vec<[f64; 2]> = Vec::new();
  // where the cut is between a kept sample and a removed one (the chord near the curve)
  let cut = |kept: [f64; 2], gone: [f64; 2]| -> [f64; 2] {
    let (mut a, mut b) = (kept, gone);
    for _ in 0..40 {
      let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
      if removed(m) {
        b = m;
      } else {
        a = m;
      }
    }
    b
  };
  let mut prev: Option<[f64; 2]> = None;
  for p in samples {
    if removed(p) {
      if cur.is_empty()
        && let Some(q) = prev
      {
        cur.push(cut(q, p));
      }
      cur.push(p);
    } else if let Some(&last) = cur.last() {
      cur.push(cut(p, last));
      runs.push(std::mem::take(&mut cur));
    }
    prev = Some(p);
  }
  runs.push(cur);
  runs.retain(|r| r.len() >= 2);
  runs
}

/// Trim: the piece the click would remove, highlighted under the pointer in Fusion's dark blue.
fn trim_preview(app: &App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  if app.tool != Tool::Trim {
    return;
  }
  let Some((sk, plane)) = sketch_plane(app) else { return };
  let (Some(Pick::SketchCurve { sketch, curve }), Some(pos)) = (app.hover, resp.hover_pos()) else { return };
  if sketch != sk {
    return;
  }
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let Ok(s) = app.s.design.sketch(sk) else { return };
  let stroke = Stroke::new(3.0, crate::theme::TRIM_PIECE);
  for run in trim_piece(s, curve, plane.to_local(hit)) {
    let pts: Vec<Pos2> = run.iter().filter_map(|p| view.to_screen(&app.cam, plane.to_world(*p))).collect();
    painter.add(egui::Shape::line(pts, stroke));
  }
}

/// The sketch curve under the pointer for a curve-picking Modify tool, with the point under
/// the pointer in sketch coordinates.
fn hovered_curve(
  app: &App,
  resp: &egui::Response,
  view: &View,
) -> Option<(FeatureId, Plane, fox_engine::sketch::CurveId, [f64; 2])> {
  let (sk, plane) = sketch_plane(app)?;
  let (Some(Pick::SketchCurve { sketch, curve }), Some(pos)) = (app.hover, resp.hover_pos()) else { return None };
  if sketch != sk {
    return None;
  }
  let (o, d) = view.ray(&app.cam, pos);
  let hit = ray_plane(o, d, plane.origin, plane.normal())?;
  Some((sk, plane, curve, plane.to_local(hit)))
}

/// Draws a sketch curve bold blue, as Fusion highlights the curve Extend or Break would act on.
fn bold_curve(
  app: &App,
  painter: &egui::Painter,
  view: &View,
  s: &Sketch,
  plane: &Plane,
  curve: fox_engine::sketch::CurveId,
) {
  let Some(c) = s.curve(curve) else { return };
  let line = fox_engine::scene::curve_polyline(s, &c.kind);
  let pts: Vec<Pos2> = line.iter().filter_map(|p| view.to_screen(&app.cam, plane.to_world(*p))).collect();
  painter.add(egui::Shape::line(pts, Stroke::new(3.0, crate::theme::MODIFY_HOVER)));
}

/// Extend: the hovered curve bold blue and the piece the click would add in light red, as Fusion
/// previews it.
fn extend_preview(app: &App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  if app.tool != Tool::Extend {
    return;
  }
  let Some((sk, plane, curve, at)) = hovered_curve(app, resp, view) else { return };
  let Ok(s) = app.s.design.sketch(sk) else { return };
  bold_curve(app, painter, view, s, &plane, curve);
  let stroke = Stroke::new(3.0, crate::theme::EXTEND_PIECE);
  for run in extend_piece(s, curve, at) {
    let pts: Vec<Pos2> = run.iter().filter_map(|p| view.to_screen(&app.cam, plane.to_world(*p))).collect();
    painter.add(egui::Shape::line(pts, stroke));
  }
}

/// Break: the hovered curve bold blue with a small red x at each point the click would split it
/// at, as Fusion previews it.
fn break_preview(app: &App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  if app.tool != Tool::Break {
    return;
  }
  let Some((sk, plane, curve, at)) = hovered_curve(app, resp, view) else { return };
  let Ok(s) = app.s.design.sketch(sk) else { return };
  bold_curve(app, painter, view, s, &plane, curve);
  let stroke = Stroke::new(1.5, crate::theme::BREAK_MARK);
  for p in break_points(s, curve, at) {
    let Some(c) = view.to_screen(&app.cam, plane.to_world(p)) else { continue };
    let r = 4.0;
    painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], stroke);
    painter.line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], stroke);
  }
}

/// Text's frame on the sketch: while the second corner is awaited, a dashed rubber-band frame
/// from the first corner to the pointer; once placed, the frame round the text (its rotate
/// handle is a feature handle, see `handles`).
fn text_frame_overlay(app: &App, painter: &egui::Painter, view: &View, pointer: Option<Pos2>) {
  let Some(d) = app.dialog.as_ref() else { return };
  let Some((_, plane)) = sketch_plane(app) else { return };
  let corners = |a: [f64; 2], b: [f64; 2]| -> Vec<Pos2> {
    [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]]
      .iter()
      .filter_map(|q| view.to_screen(&app.cam, plane.to_world(*q)))
      .collect()
  };
  if let Some(a) = d.text_frame_start()
    && let Some(p) = pointer
  {
    let (o, dir) = view.ray(&app.cam, p);
    if let Some(hit) = ray_plane(o, dir, plane.origin, plane.normal()) {
      let mut pts = corners(a, plane.to_local(hit));
      if pts.len() == 4 {
        pts.push(pts[0]);
        painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, crate::theme::ACCENT), 5.0, 4.0));
      }
    }
  }
  if let Some((a, b, _)) = d.text_frame() {
    let pts = corners(a, b);
    if pts.len() == 4 {
      painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, crate::theme::ACCENT)));
    }
  }
}

/// Text: a click at `pos` places a corner of the frame (sketch coordinates), the second with
/// the angle the text reads at on screen.
fn place_text_corner(app: &mut App, view: &View, pos: Pos2) {
  let Some((_, plane)) = sketch_plane(app) else { return };
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let (right, _, _) = app.cam.basis();
  let angle = reading_angle(&plane, right);
  if let Some(dlg) = app.dialog.as_mut() {
    dlg.place_text_corner(plane.to_local(hit), angle);
  }
}

/// The angle (degrees, a multiple of 90) at which text on `plane` reads left to right on a screen
/// whose right-hand direction is `right`: 0 when the sketch's x axis points right, as on the XY
/// plane, else turned so the text is never drawn up the screen.
pub(crate) fn reading_angle(plane: &Plane, right: DVec3) -> f64 {
  let along = |deg: f64| {
    let (s, c) = deg.to_radians().sin_cos();
    (plane.x * c + plane.y * s).dot(right)
  };
  let mut best = 0.0;
  for deg in [90.0, 180.0, 270.0] {
    if along(deg) > along(best) + 1e-9 {
      best = deg;
    }
  }
  best
}

/// Sketch Offset: until a click places it, the offset's distance and side follow the cursor.
fn offset_follow(app: &mut App, resp: &egui::Response, view: &View) {
  if app.drag.is_some() || !app.dialog.as_ref().is_some_and(|d| d.follows_cursor()) {
    return;
  }
  let Some((_, plane)) = sketch_plane(app) else { return };
  let Some(pos) = resp.hover_pos() else { return };
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let step = grid_step(app, view) / 10.0;
  if let Some(dlg) = app.dialog.as_mut() {
    dlg.follow_cursor(&app.s, plane.to_local(hit), step);
  }
}

/// Break, 2/3-tangent circle and blend: clicks pick curves (a 2-tangent circle's last click sets
/// its size and place).
fn curve_tool_click(app: &mut App, view: &View, pos: Pos2) {
  let Some((sk, plane)) = sketch_plane(app) else { return };
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let at = plane.to_local(hit);
  let hovered = match app.hover {
    Some(Pick::SketchCurve { curve, .. }) => Some(curve.0),
    _ => None,
  };
  if app.tool == Tool::Break {
    let snap = SNAP_PX as f64 * app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
    let mut params = json!({"sketch": sk.0, "at": at, "snap": snap});
    if let Some(c) = hovered {
      params["curve"] = json!(c);
    }
    app.run("sketch.break", params);
    return;
  }
  if matches!(app.tool, Tool::Fillet | Tool::Blend) || app.tool.is_chamfer() {
    if crate::sketch_modify::click(app, sk, at, hovered, app.hover) {
      // the result draws like any other curve until the pointer moves (nothing is hovered)
      app.tstate.hover_hold = Some(pos);
    }
    return;
  }
  let picked = app.tstate.curves.len();
  // the 2-tangent circle's third click is anywhere: it places the circle
  if app.tool == Tool::Circle2T && picked == 2 {
    place_tangent_circle(app, sk, at);
    app.tstate.hover_hold = Some(pos);
    return;
  }
  // a click on nothing picks nothing (the cursor prompt still says what to click)
  let Some(c) = hovered else { return };
  if app.tstate.curves.iter().any(|x| x.0 == c) {
    return;
  }
  app.tstate.curves.push((c, at));
  let picks = app.tstate.curves.clone();
  let curves: Vec<u32> = picks.iter().map(|c| c.0).collect();
  if let (Tool::Circle3T, [a, b, c]) = (app.tool, picks.as_slice()) {
    app.tstate.curves.clear();
    let near = [(a.1[0] + b.1[0] + c.1[0]) / 3.0, (a.1[1] + b.1[1] + c.1[1]) / 3.0];
    app.run("sketch.tangent_circle", json!({"sketch": sk.0, "curves": curves, "near": near}));
    // the new circle draws like any other curve until the pointer moves (nothing is hovered)
    app.tstate.hover_hold = Some(pos);
  }
}

/// A 2-tangent circle's radius for a cursor at `at`: what its box holds, else the cursor's
/// mean distance to the two curves.
fn tangent_circle_radius(app: &App, sk: FeatureId, at: [f64; 2]) -> f64 {
  let curves: Vec<u32> = app.tstate.curves.iter().map(|c| c.0).collect();
  hud_value(app, 0).unwrap_or_else(|| tangent_radius(app, sk, &curves, at))
}

/// The 2-tangent circle the tool would place for a cursor at `at`: its centre and radius.
fn tangent_circle_geometry(app: &App, sk: FeatureId, at: [f64; 2]) -> Option<([f64; 2], f64)> {
  let curves: Vec<u32> = app.tstate.curves.iter().map(|c| c.0).collect();
  if curves.len() != 2 {
    return None;
  }
  let r = tangent_circle_radius(app, sk, at);
  let mut sketch = app.s.design.sketch(sk).ok()?.clone();
  let ids: Vec<_> = curves.iter().map(|c| fox_engine::sketch::CurveId(*c)).collect();
  let c = sketch.add_tangent_circle(&ids, Some(fox_engine::sketch::Dim::new(r)), at).ok()?;
  let center = sketch.curve_points(c).first().and_then(|p| sketch.pos(*p))?;
  Some((center, r))
}

/// Places the 2-tangent circle at the cursor (or Enter in its radius box): tangent to both
/// picks, sized by the box, and, as in Fusion, without a radius dimension. The tool then waits
/// for the next first tangent.
fn place_tangent_circle(app: &mut App, sk: FeatureId, at: [f64; 2]) {
  let curves: Vec<u32> = app.tstate.curves.iter().map(|c| c.0).collect();
  let r = tangent_circle_radius(app, sk, at);
  app.tstate.curves.clear();
  app.tstate.hud = HeadsUp::default();
  let params = json!({"sketch": sk.0, "curves": curves, "radius": r, "near": at, "dimension": false});
  app.run("sketch.tangent_circle", params);
}

/// A 2-tangent circle's radius for a cursor at `at`: its mean distance to the two curves.
fn tangent_radius(app: &App, sk: FeatureId, curves: &[u32], at: [f64; 2]) -> f64 {
  let Ok(sketch) = app.s.design.sketch(sk) else { return 1.0 };
  let d: Vec<f64> =
    curves.iter().filter_map(|c| sketch.distance_to_curve(fox_engine::sketch::CurveId(*c), at)).collect();
  let r = d.iter().sum::<f64>() / d.len().max(1) as f64;
  let step = 10f64.powf(r.max(1e-6).log10().floor() - 2.0);
  ((r / step).round() * step).max(1e-3)
}

fn point_line_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l = dx.hypot(dy);
  if l < 1e-12 { (p[0] - a[0]).hypot(p[1] - a[1]) } else { ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / l }
}

fn select_click(app: &mut App, add: bool) {
  crate::origin::clear(app);
  match app.hover {
    // in a sketch, sketch geometry picks as itself; the model under it follows the selection
    // priority (Project's Bodies filter picks the whole body, as in Fusion)
    Some(p) if app.s.edit_sketch.is_some() && crate::select::pick_body(&p).is_none() => {
      crate::panels::toggle_sel(&mut app.s, p.to_sel(), add)
    }
    Some(p) => crate::select::pick(app, p, add),
    None if !add => app.s.selection.clear(),
    None => {}
  }
}

fn drag_point_live(app: &mut App, view: &View, pos: Pos2, sketch: FeatureId, point: PointId) {
  let Some(plane) = app.s.model.sketches.get(&sketch).map(|r| r.plane) else { return };
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let to = plane.to_local(hit);
  let Ok(sk) = app.s.design.sketch_mut(sketch) else { return };
  let r = sk.drag(point, to);
  let solved = sk.clone();
  if let Some(res) = app.s.model.sketches.get_mut(&sketch) {
    res.profiles = fox_engine::sketch::find_profiles(&solved);
    res.sketch = solved;
    res.report = r;
    app.s.revision += 1;
  }
}

fn finish_point_drag(app: &mut App, sketch: FeatureId, point: PointId) {
  let Some(before) = app.tstate.drag_backup.take() else { return };
  let to = app.s.design.sketch(sketch).ok().and_then(|s| s.pos(point));
  app.s.design = before;
  if let Some(to) = to {
    app.run("sketch.drag", json!({"sketch": sketch.0, "point": point.0, "to": to}));
  } else {
    app.s.rebuild();
  }
}

/// Dimension tool, as Fusion runs it: a pick (line → length, circle → diameter, arc → radius;
/// a point then a point or line → distance; a line then a line (or Shift+click a line) → angle)
/// shows the dimension following the cursor; the next click places it and opens its value box.
/// Clicking more geometry while placing changes the dimension (a line then a point → distance).
fn dimension_click(app: &mut App, pos: Pos2, shift: bool) {
  let Some(sk) = app.s.edit_sketch else { return };
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let sketch = sketch.clone();
  let picked = app.hover.filter(|p| matches!(p, Pick::SketchCurve { .. } | Pick::SketchPoint { .. }));
  if let Some(h) = picked
    && !app.tstate.dim_picks.contains(&h)
  {
    let mut picks = app.tstate.dim_picks.clone();
    picks.push(h);
    match dimension_of(&sketch, &picks, shift) {
      Some((cmd, kind, current)) => {
        app.tstate.dim_picks = picks;
        app.tstate.dim_place = Some(DimPlace { kind, cmd, current, placed: None });
      }
      // a point (or a Shift-clicked line) waits for the second pick; a pair that makes no
      // dimension starts over from this pick
      None if picks.len() == 1 => {
        app.tstate.dim_picks = picks;
        app.tstate.dim_place = None;
      }
      None => {
        let fresh = vec![h];
        app.tstate.dim_place = dimension_of(&sketch, &fresh, shift).map(|(cmd, kind, current)| DimPlace {
          kind,
          cmd,
          current,
          placed: None,
        });
        app.tstate.dim_picks = fresh;
      }
    }
    return;
  }
  let Some(place) = app.tstate.dim_place.as_mut() else { return };
  if place.placed.is_some() {
    return;
  }
  // the placement click: the dimension stays where the cursor put it (see [`dim_place_preview`])
  // and its value box opens there
  place.placed = app.tstate.hud.cursor.map(|c| c.0);
  let angle = place.kind.is_angle();
  let unit = app.s.design.units;
  let text = place.current.map(|v| dim_full_text(unit, angle, v)).unwrap_or_default();
  app.tstate.pending_dim = Some(place.cmd.clone());
  app.dim_edit = Some((0, text, pos));
}

/// The dimension `picks` make (its command, the constraint it adds and the measured value in mm
/// or degrees), or None when they make none yet (a point alone, a Shift-clicked line).
fn dimension_of(
  sketch: &Sketch,
  picks: &[Pick],
  shift: bool,
) -> Option<(serde_json::Value, ConstraintKind, Option<f64>)> {
  use fox_engine::sketch::Dim;
  let dim = |v: Option<f64>| Dim::new(v.unwrap_or(0.0));
  let kind_of = |c: fox_engine::sketch::CurveId| sketch.curve(c).map(|c| c.kind.clone());
  match picks {
    [Pick::SketchCurve { curve, .. }] => match kind_of(*curve)? {
      CurveKind::Line { a, b } => {
        if shift {
          return None;
        }
        let d = dist(sketch, a, b);
        Some((json!({"type": "distance", "points": [a.0, b.0]}), ConstraintKind::Distance { a, b, value: dim(d) }, d))
      }
      CurveKind::Circle { radius, .. } => Some((
        json!({"type": "diameter", "curves": [curve.0]}),
        ConstraintKind::Diameter { curve: *curve, value: Dim::new(2.0 * radius) },
        Some(2.0 * radius),
      )),
      CurveKind::Arc { .. } => {
        let r = sketch.radius(*curve);
        Some((
          json!({"type": "radius", "curves": [curve.0]}),
          ConstraintKind::Radius { curve: *curve, value: dim(r) },
          r,
        ))
      }
      _ => None,
    },
    [Pick::SketchPoint { point: a, .. }, Pick::SketchPoint { point: b, .. }] if a != b => {
      let d = dist(sketch, *a, *b);
      Some((
        json!({"type": "distance", "points": [a.0, b.0]}),
        ConstraintKind::Distance { a: *a, b: *b, value: dim(d) },
        d,
      ))
    }
    [Pick::SketchPoint { point, .. }, Pick::SketchCurve { curve, .. }]
    | [Pick::SketchCurve { curve, .. }, Pick::SketchPoint { point, .. }] => {
      if !matches!(kind_of(*curve)?, CurveKind::Line { .. }) {
        return None;
      }
      let d = sketch.pos(*point).and_then(|p| sketch.distance_to_curve(*curve, p));
      Some((
        json!({"type": "point_line", "points": [point.0], "curves": [curve.0]}),
        ConstraintKind::PointLineDistance { p: *point, line: *curve, value: dim(d) },
        d,
      ))
    }
    [Pick::SketchCurve { curve: a, .. }, Pick::SketchCurve { curve: b, .. }] if a != b => {
      if !matches!(kind_of(*a)?, CurveKind::Line { .. }) || !matches!(kind_of(*b)?, CurveKind::Line { .. }) {
        return None;
      }
      let ang = line_angle(sketch, *a, *b);
      Some((
        json!({"type": "angle", "curves": [a.0, b.0]}),
        ConstraintKind::Angle { a: *a, b: *b, value: dim(ang) },
        ang.map(f64::to_degrees),
      ))
    }
    _ => None,
  }
}

/// A new dimension's value as its box opens: the full measurement with its unit, as Fusion's
/// "0.4787837888 in" (an angle in degrees).
fn dim_full_text(unit: fox_engine::model::LengthUnit, angle: bool, v: f64) -> String {
  let (n, sym) = if angle { (v, "deg") } else { (unit.from_mm(v), unit.symbol()) };
  let s = format!("{n:.10}");
  let s = s.trim_end_matches('0').trim_end_matches('.');
  format!("{} {sym}", if s == "-0" || s.is_empty() { "0" } else { s })
}

/// A dimension being placed, as Fusion previews it: the value at document precision with its
/// prefix ("0.479", "R0.228", "⌀0.479", "90.0°") and extension lines out to the cursor.
fn dim_place_preview(app: &mut App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  if app.tool != Tool::Dimension {
    return;
  }
  let Some((sk, plane)) = sketch_plane(app) else { return };
  // the cursor on the sketch plane (unsnapped: the placement is free), kept for the placing click
  if let Some(pos) = resp.hover_pos() {
    let (o, d) = view.ray(&app.cam, pos);
    if let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) {
      let at = plane.to_local(hit);
      app.tstate.hud.cursor = Some((at, None));
    }
  }
  let Some(place) = app.tstate.dim_place.as_ref() else { return };
  let Some(cur) = place.placed.or(app.tstate.hud.cursor.map(|c| c.0)) else { return };
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let unit = app.s.design.units;
  let label = match (&place.kind, place.current) {
    (_, None) => String::new(),
    (ConstraintKind::Radius { .. }, Some(v)) => format!("R{}", fixed_number(unit, v)),
    (ConstraintKind::Diameter { .. }, Some(v)) => format!("⌀{}", fixed_number(unit, v)),
    (ConstraintKind::Angle { .. }, Some(v)) => format!("{v:.p$}°", p = crate::units::angle_decimals()),
    (_, Some(v)) => fixed_number(unit, v),
  };
  let font = egui::FontId::proportional(12.0);
  let text = DimText::new(painter, &label, font);
  let size = text.size() + Vec2::new(8.0, 4.0);
  let wpp = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let Some(shape) = dim_shape_at(sketch, &place.kind, cur, size, wpp, &to_screen) else { return };
  paint_dim(painter, &shape, DIM_INK);
  let rect = Rect::from_center_size(shape.label, size);
  text.paint(painter, rect.min + Vec2::new(4.0, 2.0), DIM_TEXT);
}

/// A length (mm) in the design's unit at Fusion's document precision, without the unit.
fn fixed_number(unit: fox_engine::model::LengthUnit, mm: f64) -> String {
  format!("{:.*}", display_decimals(unit), unit.from_mm(mm))
}

/// The leader graphics of a dimension placed by the cursor at `cur` (sketch coordinates): a
/// linear dimension's line runs through the cursor's level on its side, a radial one's leader
/// ends at the cursor, an angle's value sits at the cursor.
fn dim_shape_at(
  s: &Sketch,
  k: &ConstraintKind,
  cur: [f64; 2],
  size: Vec2,
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let unit = |p: [f64; 2], q: [f64; 2]| -> Option<[f64; 2]> {
    let l = (q[0] - p[0]).hypot(q[1] - p[1]);
    (l > 1e-12).then(|| [(q[0] - p[0]) / l, (q[1] - p[1]) / l])
  };
  let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
  // a linear dimension from p to q along dir, its line at the cursor's offset on the cursor's side
  let linear_at = |p: [f64; 2], q: [f64; 2], dir: [f64; 2]| {
    let mut n = [-dir[1], dir[0]];
    let mid = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
    if dot(n, [cur[0] - mid[0], cur[1] - mid[1]]) < 0.0 {
      n = [-n[0], -n[1]];
    }
    let off = (dot(cur, n) - dot(p, n).max(dot(q, n))).max(4.0 * wpp);
    linear_shape(p, q, dir, [mid[0] - n[0], mid[1] - n[1]], off, wpp, to_screen)
  };
  match k {
    ConstraintKind::Distance { a, b, .. } => {
      let (p, q) = (s.pos(*a)?, s.pos(*b)?);
      linear_at(p, q, unit(p, q)?)
    }
    ConstraintKind::HorizontalDistance { a, b, .. } => linear_at(s.pos(*a)?, s.pos(*b)?, [1.0, 0.0]),
    ConstraintKind::VerticalDistance { a, b, .. } => linear_at(s.pos(*a)?, s.pos(*b)?, [0.0, 1.0]),
    ConstraintKind::PointLineDistance { p, line, .. } => {
      let pts = s.curve_points(*line);
      let (l0, l1, pp) = (s.pos(*pts.first()?)?, s.pos(*pts.get(1)?)?, s.pos(*p)?);
      let d = unit(l0, l1)?;
      let t = (pp[0] - l0[0]) * d[0] + (pp[1] - l0[1]) * d[1];
      let foot = [l0[0] + d[0] * t, l0[1] + d[1] * t];
      linear_at(foot, pp, unit(foot, pp)?)
    }
    ConstraintKind::Radius { curve, .. } | ConstraintKind::Diameter { curve, .. } => {
      let c = s.pos(*s.curve_points(*curve).first()?)?;
      let r = s.radius(*curve)?;
      let diameter = matches!(k, ConstraintKind::Diameter { .. });
      let outside = (cur[0] - c[0]).hypot(cur[1] - c[1]) > r;
      // a diameter's value inside the circle sits on the diameter line towards it
      if diameter && !outside {
        return diameter_shape_towards(c, r, cur, to_screen);
      }
      let mut shape = radial_shape(c, r, cur, diameter, size, to_screen)?;
      // the leader carries on to the cursor once it is outside the circle
      if outside {
        let at = to_screen(cur)?;
        let on = shape.arrows.first()?.0;
        let u = (at - on).normalized();
        if u.is_finite() {
          let half = (u.x.abs() * size.x + u.y.abs() * size.y) / 2.0;
          let start = shape.lines.first().map_or(on, |l| l[0]);
          shape.lines = vec![[start, at - u * half]];
          shape.label = at;
        }
      }
      Some(shape)
    }
    ConstraintKind::Angle { a, b, value } => {
      let mut shape = angular_shape(s, *a, *b, value.value, wpp, to_screen)?;
      shape.label = to_screen(cur)?;
      Some(shape)
    }
    _ => None,
  }
}

/// Escape on a new dimension's value box keeps the dimension at its current value, as Fusion
/// does; on an existing dimension's it drops the edit. Either way the box closes.
pub fn settle_dim_edit(app: &mut App) {
  if let Some((0, text, _)) = app.dim_edit.take() {
    commit_pending_dim(app, &text);
  }
  app.dim_edit = None;
  app.tstate.dim_place = None;
  app.tstate.dim_picks.clear();
}

/// Creates the dimension the Dimension tool placed, with `text` as its value.
fn commit_pending_dim(app: &mut App, text: &str) {
  let (Some(sk), Some(mut cmd)) = (app.s.edit_sketch, app.tstate.pending_dim.take()) else { return };
  let unit = app.s.design.units;
  let angle = cmd["type"] == "angle";
  // an emptied box (Escape) keeps the dimension at its measured value
  let measured = app.tstate.dim_place.as_ref().and_then(|p| p.current).map(|v| dim_full_text(unit, angle, v));
  let text = if text.trim().is_empty() { measured.unwrap_or_default() } else { text.to_string() };
  let value = accepted_text(unit, if angle { Field::Angle } else { Field::Length }, &text);
  cmd["sketch"] = json!(sk.0);
  cmd["value"] = json!(value);
  // the value stays where the placing click put it
  if let Some(at) = app.tstate.dim_place.as_ref().and_then(|p| p.placed) {
    cmd["at"] = json!(at);
  }
  app.run("sketch.dimension", cmd);
}

fn dist(s: &Sketch, a: PointId, b: PointId) -> Option<f64> {
  let (p, q) = (s.pos(a)?, s.pos(b)?);
  Some((p[0] - q[0]).hypot(p[1] - q[1]))
}

fn line_angle(s: &Sketch, a: fox_engine::sketch::CurveId, b: fox_engine::sketch::CurveId) -> Option<f64> {
  let dir = |c| -> Option<[f64; 2]> {
    let pts = s.curve_points(c);
    let (p, q) = (s.pos(*pts.first()?)?, s.pos(*pts.get(1)?)?);
    Some([q[0] - p[0], q[1] - p[1]])
  };
  let (u, v) = (dir(a)?, dir(b)?);
  let ang = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
  Some(ang.rem_euclid(std::f64::consts::PI))
}

/// The floating value box for new or edited dimensions.
fn dim_editor(app: &mut App, ctx: &egui::Context) {
  let Some((cid, mut text, pos)) = app.dim_edit.take() else { return };
  // an existing dimension names its model parameter ("d7 : 0.4 in"), as Fusion's tooltip does
  let param = if cid == 0 { None } else { dim_param_name(app, cid) };
  let mut measure = false;
  let recent = app.dim_recent.clone();
  // Enter on an emptied box is refused: the box stays, with a red underline until a value is typed
  let empty_id = egui::Id::new("dim-editor-empty");
  let mut refused = ctx.data(|d| d.get_temp::<bool>(empty_id)).unwrap_or(false);
  // a box that wasn't showing last frame (or shows another edit) takes the keyboard with its
  // text selected
  let shown_id = egui::Id::new("dim-editor-shown");
  let pass = ctx.cumulative_pass_nr();
  let key = (cid, pos.x.to_bits(), pos.y.to_bits());
  let last = ctx.data(|d| d.get_temp::<((u32, u32, u32), u64)>(shown_id));
  let fresh = last.is_none_or(|(k, p)| k != key || p + 1 < pass);
  ctx.data_mut(|d| d.insert_temp(shown_id, (key, pass)));
  let (mut commit, mut cancel) = (false, false);
  egui::Area::new(egui::Id::new("dim-editor"))
    .fixed_pos(pos + Vec2::new(8.0, -10.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::popup(ui.style()).inner_margin(3.0).show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 2.0;
          // a driven value (an expression with parameters) carries Fusion's greyed "fx" tag
          if !text.trim().is_empty() && fox_engine::expr::eval_raw(&text, &fox_engine::expr::Empty).is_err() {
            ui.label(egui::RichText::new("fx").italics().size(11.0).color(Color32::from_gray(150)));
          }
          let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0).hint_text("value or expression"));
          if refused && text.trim().is_empty() {
            let (a, b) = (r.rect.left_bottom(), r.rect.right_bottom());
            ui.painter().line_segment([a, b], Stroke::new(1.5, Color32::from_rgb(214, 48, 48)));
          } else {
            refused = false;
          }
          let r = match &param {
            Some(name) => r.on_hover_text(format!("{name} : {text}")),
            None => r,
          };
          if fresh {
            focus_text(ctx, &r, &text, true);
          } else if !r.has_focus() && !r.lost_focus() && !egui::Popup::is_any_open(ctx) {
            r.request_focus();
          }
          // Enter commits, as in Fusion (no OK button); Esc keeps a new dimension at its current
          // value and drops an edit of an existing one; ⋮ holds the dimension's menu
          let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
          let accept = enter || (esc && cid == 0);
          commit = accept && (r.has_focus() || r.lost_focus());
          if commit && enter && text.trim().is_empty() {
            commit = false;
            refused = true;
            // Enter dropped the box's focus: it keeps the keyboard, as Fusion's does
            r.request_focus();
          }
          cancel = esc && !commit;
          let m = ui.add(egui::Button::new("").min_size(Vec2::new(14.0, 20.0)).frame(false));
          let dot = Color32::from_gray(60);
          for dy in [-4.0, 0.0, 4.0] {
            ui.painter().circle_filled(m.rect.center() + Vec2::new(0.0, dy), 1.3, dot);
          }
          // Fusion's menu: the values entered lately (picking one fills the box), then Measure
          egui::Popup::menu(&m).show(|ui| {
            for v in &recent {
              if ui.button(v).clicked() {
                text = v.clone();
              }
            }
            if !recent.is_empty() {
              ui.separator();
            }
            ui.horizontal(|ui| {
              let (ir, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
              crate::icons::draw(ui.painter(), ir, crate::icons::Icon::Measure, Color32::from_gray(70));
              if ui.button("Measure").clicked() {
                measure = true;
              }
            });
          });
        });
      });
    });
  ctx.data_mut(|d| d.insert_temp(empty_id, refused && !commit && !cancel));
  if measure {
    // Measure opens beside the box, as in Fusion; what it measures fills the box when it closes
    let d = crate::dialogs::Dialog::open(app, crate::dialogs::Kind::Inspect);
    app.dialog = Some(d);
    app.tstate.measure_fill = true;
  }
  if commit {
    // the Enter is this box's, not the open dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    if let Some(sk) = app.s.edit_sketch {
      // a bare length means the design's unit
      let unit = app.s.design.units;
      if cid == 0 {
        commit_pending_dim(app, &text);
        app.tstate.dim_place = None;
        app.tstate.dim_picks.clear();
      } else {
        let angle = app
          .s
          .design
          .sketch(sk)
          .ok()
          .and_then(|s| s.constraint(fox_engine::sketch::ConstraintId(cid)))
          .is_some_and(|c| c.kind.is_angle());
        let value = accepted_text(unit, if angle { Field::Angle } else { Field::Length }, &text);
        app.run("sketch.set_dimension", json!({"sketch": sk.0, "constraint": cid, "value": value}));
      }
      remember_dim_value(app, &text);
    }
  }
  if cancel {
    app.tstate.pending_dim = None;
    app.tstate.dim_place = None;
    app.tstate.dim_picks.clear();
  }
  if !commit && !cancel {
    app.dim_edit = Some((cid, text, pos));
  }
}

/// Measure, opened from a dimension box's menu, is closing: the distance it measured (if any)
/// goes into the box, in full with its unit, as Fusion writes it back.
pub(crate) fn fill_dim_from_measure(app: &mut App) {
  if !std::mem::take(&mut app.tstate.measure_fill) {
    return;
  }
  let Some((a, b, _)) = &app.measure else { return };
  let d = (*b - *a).length();
  let unit = app.s.design.units;
  if let Some((_, text, _)) = app.dim_edit.as_mut() {
    *text = dim_full_text(unit, false, d);
  }
}

/// Keeps a value entered in a dimension box for the box menu's list of recent values, newest
/// first, as Fusion does.
fn remember_dim_value(app: &mut App, text: &str) {
  let t = text.trim();
  if t.is_empty() {
    return;
  }
  app.dim_recent.retain(|v| v != t);
  app.dim_recent.insert(0, t.to_string());
  app.dim_recent.truncate(8);
}

/// The model parameter that drives the open sketch's dimension `cid` ("d7"), as the Parameters
/// dialog names it.
fn dim_param_name(app: &App, cid: u32) -> Option<String> {
  let sk = app.s.edit_sketch?;
  let sketch = app.s.design.sketch(sk).ok()?;
  let index = sketch.constraints.iter().filter(|c| c.kind.dim().is_some()).position(|c| c.id.0 == cid)?;
  app.s.design.model_parameters().into_iter().find(|p| p.feature == sk && p.index == index).map(|p| p.name)
}

/// What the dimension editor opens with: a plain value in full with its unit ("0.2815284896 in",
/// "17.220379275 deg") as Fusion shows it; an expression with parameters as it was typed.
fn dim_edit_text(k: &ConstraintKind, dim: &fox_engine::sketch::Dim, unit: fox_engine::model::LengthUnit) -> String {
  if fox_engine::expr::eval_raw(&dim.expr, &fox_engine::expr::Empty).is_err() {
    return dim.expr.clone();
  }
  let angle = k.is_angle();
  dim_full_text(unit, angle, if angle { dim.value.to_degrees() } else { dim.value })
}

/// Gives a text box the keyboard, with all its text selected (typing replaces it) or the caret
/// at the end.
pub(crate) fn focus_text(ctx: &egui::Context, r: &egui::Response, text: &str, select_all: bool) {
  r.request_focus();
  select_text(ctx, r.id, text, select_all);
}

/// Sets the text edit `id`'s cursor for `text`: the whole text selected (so typing replaces it)
/// or the cursor at its end.
pub(crate) fn select_text(ctx: &egui::Context, id: egui::Id, text: &str, select_all: bool) {
  use egui::text::{CCursor, CCursorRange};
  let n = text.chars().count();
  let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
  let range =
    if select_all { CCursorRange::two(CCursor::new(0), CCursor::new(n)) } else { CCursorRange::one(CCursor::new(n)) };
  state.cursor.set_char_range(Some(range));
  state.store(ctx, id);
}

// ---------------------------------------------------------------- overlays

/// The dimensions of sketches the browser's Show Dimensions turned on, drawn (not editable)
/// while they're not being edited.
fn shown_dims_overlay(app: &App, painter: &egui::Painter, view: &View) {
  let wpp = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let font = egui::FontId::proportional(12.0);
  for sk in &app.shown_dims {
    if app.s.edit_sketch == Some(*sk) || !crate::panels::sketch_drawn(app, *sk) {
      continue;
    }
    let Some(r) = app.s.model.sketches.get(sk) else { continue };
    let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, r.plane.to_world(p));
    for c in &r.sketch.constraints {
      let Some(dim) = c.kind.dim() else { continue };
      let Some((anchor, label)) = dim_label(&r.sketch, &c.kind, dim, app.s.design.units) else { continue };
      let text = DimText::new(painter, &label, font.clone());
      let size = text.size() + Vec2::new(8.0, 4.0);
      let shape = dim_shape(&r.sketch, &c.kind, dim, c.label, size, wpp, &to_screen);
      let at = match &shape {
        Some(s) => s.label,
        None => match to_screen(anchor) {
          Some(p) => p,
          None => continue,
        },
      };
      if let Some(s) = &shape {
        paint_dim(painter, s, DIM_INK);
      }
      let rect = Rect::from_center_size(at, size);
      text.paint(painter, rect.min + Vec2::new(4.0, 2.0), DIM_TEXT);
    }
  }
}

/// Where the screen point `p` falls on `plane` (sketch coordinates).
fn screen_to_sketch(app: &App, view: &View, plane: &Plane, p: Pos2) -> Option<[f64; 2]> {
  let (o, d) = view.ray(&app.cam, p);
  ray_plane(o, d, plane.origin, plane.normal()).map(|hit| plane.to_local(hit))
}

/// Moves dimension `c`'s value to `to` (sketch coordinates) in the solved sketch while its value
/// is dragged; letting go stores it with `sketch.dimension_label`.
pub(crate) fn drag_dim_label(
  s: &mut fox_engine::Session,
  sk: FeatureId,
  c: fox_engine::sketch::ConstraintId,
  to: [f64; 2],
) {
  let Some(r) = s.model.sketches.get_mut(&sk) else { return };
  if let Some(c) = r.sketch.constraints.iter_mut().find(|x| x.id == c) {
    c.label = Some(to);
    s.revision += 1;
  }
}

/// Dimension labels and constraint glyphs of the sketch being edited. Returns true when a click
/// was consumed.
fn sketch_overlay(app: &mut App, ui: &mut egui::Ui, painter: &egui::Painter, view: &View) -> bool {
  let Some((sk, plane)) = sketch_plane(app) else { return false };
  let Some(sketch) = app.s.model.sketches.get(&sk).map(|r| r.sketch.clone()) else { return false };
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let wpp = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let mut consumed = false;
  let font = egui::FontId::proportional(12.0);
  let mut glyph_count: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
  // a Coincident glyph only shows while its merged point is hovered
  let hovered_point = match app.hover {
    Some(Pick::SketchPoint { sketch, point }) if sketch == sk => Some(point),
    _ => None,
  };
  let mut right_click: Option<(fox_engine::sketch::ConstraintId, Pos2)> = None;
  // a dimension value let go after a drag, and where it was put
  let mut dropped: Option<(fox_engine::sketch::ConstraintId, [f64; 2])> = None;
  for c in &sketch.constraints {
    if c.kind.dim().is_some() && !app.show_dims || c.kind.dim().is_none() && !app.show_constraints {
      continue;
    }
    let selected = app.s.selection.contains(&Sel::SketchConstraint { sketch: sk, constraint: c.id });
    if let Some(dim) = c.kind.dim() {
      let Some((anchor, label)) = dim_label(&sketch, &c.kind, dim, app.s.design.units) else { continue };
      let text = DimText::new(painter, &label, font.clone());
      let size = text.size() + Vec2::new(8.0, 4.0);
      // extension lines, dimension line and arrows, offset from the geometry as in Fusion
      let shape = dim_shape(&sketch, &c.kind, dim, c.label, size, wpp, &to_screen);
      let at = match &shape {
        Some(s) => s.label,
        None => match to_screen(anchor) {
          Some(p) => p,
          None => continue,
        },
      };
      let r = Rect::from_center_size(at, size);
      let resp = ui.interact(r, ui.id().with(("dim", c.id.0)), Sense::click_and_drag());
      // dragging the value moves it, as in Fusion: its new spot sets the offset and side
      if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
      {
        app.tstate.dim_drag = Some((c.id.0, at - p));
      }
      if let (Some((id, grab)), Some(p)) = (app.tstate.dim_drag, resp.interact_pointer_pos())
        && id == c.id.0
        && resp.dragged()
        && let Some(to) = screen_to_sketch(app, view, &plane, p + grab)
      {
        drag_dim_label(&mut app.s, sk, c.id, to);
        consumed = true;
      }
      if resp.drag_stopped() && app.tstate.dim_drag.is_some_and(|d| d.0 == c.id.0) {
        app.tstate.dim_drag = None;
        dropped = app
          .s
          .model
          .sketches
          .get(&sk)
          .and_then(|r| r.sketch.constraint(c.id))
          .and_then(|x| x.label)
          .map(|to| (c.id, to));
        consumed = true;
      }
      // plain dark text with no box, as Fusion draws it; the text and line turn blue under the
      // cursor or selected
      let hot = selected || resp.hovered();
      if let Some(s) = &shape {
        paint_dim(painter, s, if hot { DIM_INK_HOT } else { DIM_INK });
      }
      text.paint(painter, r.min + Vec2::new(4.0, 2.0), if hot { DIM_TEXT_HOT } else { DIM_TEXT });
      // Fusion's tooltip under the text names the parameter with its full value, light text on
      // its dark tooltip panel
      let resp = match dim_param_name(app, c.id.0) {
        Some(name) => crate::navbar::dark_tip(resp, &dim_hover_tip(&name, c.kind.is_angle(), dim, app.s.design.units)),
        None => resp,
      };
      if resp.double_clicked() || (resp.clicked() && app.tool == Tool::Dimension) {
        let text = dim_edit_text(&c.kind, dim, app.s.design.units);
        app.dim_edit = Some((c.id.0, text, at));
        consumed = true;
      } else if resp.clicked() {
        let add = ui.input(|i| i.modifiers.shift);
        crate::panels::toggle_sel(&mut app.s, Sel::SketchConstraint { sketch: sk, constraint: c.id }, add);
        consumed = true;
      }
      continue;
    }
    // Fix has no glyph: fixed geometry is drawn green instead, as in Fusion
    if matches!(c.kind, ConstraintKind::Fix { .. } | ConstraintKind::FixRadius { .. }) {
      continue;
    }
    // geometric constraint glyphs: Fusion's small grey line-art symbols where it puts them (one
    // per line for Parallel, Collinear and Equal), in a row with the others on the same curve or
    // point
    let places = crate::glyphs::places(&sketch, &c.kind, hovered_point, &to_screen);
    let spots: Vec<(Pos2, Rect, egui::Response)> = places
      .iter()
      .enumerate()
      .map(|(i, p)| {
        let k = glyph_count.entry(p.key).or_default();
        let at = p.at + p.dir * (14.0 * *k as f32);
        *k += 1;
        let r = Rect::from_center_size(at, Vec2::splat(2.0 * crate::glyphs::HALF + 3.0));
        (at, r, ui.interact(r, ui.id().with(("glyph", c.id.0, i)), Sense::click()))
      })
      .collect();
    // hovering one glyph lights every glyph of the constraint: a pale blue box with dark ink;
    // the geometry stays as it is and there is no tooltip
    let hovered = spots.iter().any(|(_, _, resp)| resp.hovered());
    for (at, r, resp) in &spots {
      if hovered {
        painter.rect_filled(*r, 2.0, GLYPH_HOVER_FILL);
      }
      let col = if selected {
        crate::theme::ACCENT
      } else if hovered {
        GLYPH_HOVER_INK
      } else {
        GLYPH_INK
      };
      crate::glyphs::paint(painter, *at, &c.kind, col);
      if resp.clicked() {
        let add = ui.input(|i| i.modifiers.shift);
        crate::panels::toggle_sel(&mut app.s, Sel::SketchConstraint { sketch: sk, constraint: c.id }, add);
        consumed = true;
      }
      if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
      {
        right_click = Some((c.id, p));
      }
    }
  }
  // Fusion's glyph hover leaves the constrained geometry unhighlighted
  app.glyph_hover.clear();
  // a right-click on a glyph selects its constraint and opens the marking menu for it
  if let Some((constraint, p)) = right_click {
    let sel = Sel::SketchConstraint { sketch: sk, constraint };
    if !app.s.selection.contains(&sel) {
      app.s.selection = vec![sel];
    }
    app.hover = None;
    crate::marking::open(app, p);
    consumed = true;
  }
  if let Some((constraint, to)) = dropped {
    app.run("sketch.dimension_label", json!({"sketch": sk.0, "constraint": constraint.0, "at": to}));
  }
  consumed
}

/// Constraint glyph ink: Fusion's grey.
const GLYPH_INK: Color32 = Color32::from_rgb(105, 105, 110);
/// The dashed reference line an angle is measured from: Fusion's darker orange.
const ANGLE_REFERENCE: Color32 = Color32::from_rgb(182, 125, 93);
/// A hovered glyph's box fill and ink, as sampled in Fusion.
pub(crate) const GLYPH_HOVER_FILL: Color32 = Color32::from_rgb(189, 215, 226);
pub(crate) const GLYPH_HOVER_INK: Color32 = Color32::from_rgb(68, 69, 69);

/// A dimension's value as drawn. The font has no ⌀ (U+2300), so a diameter's glyph is painted
/// the way Fusion draws it, a circle with a slash, in front of the number.
struct DimText {
  /// A driven dimension's "fx: " mark, ahead of the glyph.
  lead: Option<std::sync::Arc<egui::Galley>>,
  glyph: bool,
  galley: std::sync::Arc<egui::Galley>,
  em: f32,
}

impl DimText {
  fn new(painter: &egui::Painter, label: &str, font: egui::FontId) -> DimText {
    let (lead, label) = match label.strip_prefix("fx: ") {
      Some(rest) => (Some(painter.layout_no_wrap("fx: ".to_string(), font.clone(), Color32::PLACEHOLDER)), rest),
      None => (None, label),
    };
    let (glyph, rest) = match label.strip_prefix('⌀') {
      Some(rest) => (true, rest),
      None => (false, label),
    };
    let galley = painter.layout_no_wrap(rest.to_string(), font.clone(), Color32::PLACEHOLDER);
    DimText { lead, glyph, em: font.size, galley }
  }

  fn glyph_width(&self) -> f32 {
    if self.glyph { self.em * 0.8 } else { 0.0 }
  }

  fn lead_width(&self) -> f32 {
    self.lead.as_ref().map_or(0.0, |g| g.size().x)
  }

  fn size(&self) -> Vec2 {
    self.galley.size() + Vec2::new(self.lead_width() + self.glyph_width(), 0.0)
  }

  fn paint(&self, painter: &egui::Painter, at: Pos2, col: Color32) {
    if let Some(lead) = &self.lead {
      painter.galley(at, lead.clone(), col);
    }
    let at = at + Vec2::new(self.lead_width(), 0.0);
    if self.glyph {
      let r = self.em * 0.27;
      let c = at + Vec2::new(self.glyph_width() / 2.0 - 1.0, self.galley.size().y / 2.0);
      painter.circle_stroke(c, r, Stroke::new(1.0, col));
      let d = Vec2::new(r * 1.5, -r * 1.5);
      painter.line_segment([c - d, c + d], Stroke::new(1.0, col));
    }
    painter.galley(at + Vec2::new(self.glyph_width(), 0.0), self.galley.clone(), col);
  }
}

/// The positions (sketch units, mm) of the grid's major lines, which carry labels: every fifth
/// line of the grid the scene draws for this step, within its extent.
pub(crate) fn grid_label_positions(step: f64, size: f64) -> Vec<f64> {
  let n = ((size / step).ceil() as i64).clamp(2, 60);
  (-n..=n).filter(|i| i % 5 == 0).map(|i| i as f64 * step).collect()
}

/// A grid label as laid out: its text (a bare number in the design's unit), the point on its
/// axis it is centred across, whether it sits along the sketch's X axis (just above it) or its
/// Y axis (just left of it), and the screen directions of the sketch's X and Y axes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GridLabel {
  pub text: String,
  pub at: Pos2,
  pub x_axis: bool,
  pub right: Vec2,
  pub up: Vec2,
}

/// The sketch grid's coordinate labels, as Fusion prints them: one on every major line of the
/// grid that is on screen, along both axes, a bare number with no unit symbol and none at the
/// origin. Each label reads along the sketch's Y axis (rotated 90° when the sketch is faced).
/// Create Sketch's preview of a plane labels its grid the same way.
pub(crate) fn grid_label_layout(app: &App, view: &View) -> Vec<GridLabel> {
  let Some(plane) = sketch_plane(app).map(|(_, p)| p).or(app.grid_preview) else { return Vec::new() };
  let step = app.grid.step().unwrap_or_else(|| fox_engine::scene::grid_step(&app.s));
  let size = app.s.model.bounds().map(|(a, b)| (b - a).length()).unwrap_or(100.0).max(50.0);
  let unit = app.s.design.units;
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let Some(o) = to_screen([0.0, 0.0]) else { return Vec::new() };
  // the axes' directions on screen, from a step along each
  let dir = |p: [f64; 2], fallback: Vec2| {
    to_screen(p).map(|q| (q - o).normalized()).filter(|d| d.is_finite() && *d != Vec2::ZERO).unwrap_or(fallback)
  };
  let right = dir([step, 0.0], Vec2::new(1.0, 0.0));
  let up = dir([0.0, step], Vec2::new(0.0, -1.0));
  let mut out = Vec::new();
  for t in grid_label_positions(step, size) {
    if t == 0.0 {
      continue;
    }
    let text = unit.number(t);
    for (p, x_axis) in [([t, 0.0], true), ([0.0, t], false)] {
      if let Some(at) = to_screen(p)
        && view.rect.contains(at)
      {
        out.push(GridLabel { text: text.clone(), at, x_axis, right, up });
      }
    }
  }
  out
}

/// Paints [`grid_label_layout`]: grey text rotated to read along the sketch's Y axis, each X
/// label just above its axis and each Y label just left of its axis, centred on its line.
fn grid_labels(app: &App, painter: &egui::Painter, view: &View) {
  let ink = Color32::from_gray(120);
  let font = egui::FontId::proportional(11.0);
  const GAP: f32 = 3.0;
  for l in grid_label_layout(app, view) {
    let galley = painter.layout_no_wrap(l.text, font.clone(), ink);
    let size = galley.size();
    let angle = l.up.y.atan2(l.up.x);
    // the text runs along `up`, its height across it
    let centre = if l.x_axis { l.at + l.up * (GAP + size.x / 2.0) } else { l.at - l.right * (GAP + size.y / 2.0) };
    painter
      .add(egui::epaint::TextShape::new(centre, galley, ink).with_angle_and_anchor(angle, egui::Align2::CENTER_CENTER));
  }
}

/// A dimension's leader graphics in screen space: extension and dimension lines, an arc for an
/// angle, arrowheads (tip and pointing direction) and where the value goes.
struct DimShape {
  lines: Vec<[Pos2; 2]>,
  arc: Vec<Pos2>,
  arrows: Vec<(Pos2, Vec2)>,
  label: Pos2,
}

/// Dimension ink (Fusion draws dimensions in black).
/// Fusion's dimension colours: the line and the text, and both under the cursor or selected.
const DIM_INK: Color32 = Color32::from_rgb(64, 64, 64);
const DIM_INK_HOT: Color32 = Color32::from_rgb(69, 92, 149);
const DIM_TEXT: Color32 = Color32::from_rgb(43, 43, 43);
const DIM_TEXT_HOT: Color32 = Color32::from_rgb(70, 113, 219);

/// A dimension's tooltip, as Fusion shows it: its model parameter and the full value with the
/// unit ("d9 = 0.4787837888 in", "d3 = 45.0 deg"). `value` is millimetres, or radians for an
/// angle.
pub(crate) fn dim_tooltip(name: &str, angle: bool, value: f64, unit: fox_engine::model::LengthUnit) -> String {
  let (value, symbol) = if angle { (value.to_degrees(), "deg") } else { (unit.from_mm(value), unit.symbol()) };
  format!("{name} = {} {symbol}", full_number(value))
}

/// The hovered dimension's tooltip: a driven one (an expression with parameters) names its
/// expression as Fusion does ("d21 : d1 * 2"), any other its full value (see [`dim_tooltip`]).
fn dim_hover_tip(
  name: &str,
  angle: bool,
  dim: &fox_engine::sketch::Dim,
  unit: fox_engine::model::LengthUnit,
) -> String {
  if fox_engine::expr::eval_raw(&dim.expr, &fox_engine::expr::Empty).is_err() {
    format!("{name} : {}", dim.expr)
  } else {
    dim_tooltip(name, angle, dim.value, unit)
  }
}

/// A value to ten decimals without trailing zeros, one decimal at least ("0.4787837888", "0.5").
fn full_number(v: f64) -> String {
  let s = format!("{v:.10}");
  let s = s.trim_end_matches('0');
  if s.ends_with('.') { format!("{s}0") } else { s.to_string() }
}
/// How far a linear dimension sits off its geometry (points).
const DIM_OFFSET_PX: f64 = 30.0;

fn paint_dim(painter: &egui::Painter, s: &DimShape, col: Color32) {
  let stroke = Stroke::new(1.0, col);
  for l in &s.lines {
    painter.line_segment(*l, stroke);
  }
  if s.arc.len() > 1 {
    painter.add(egui::Shape::line(s.arc.clone(), stroke));
  }
  for (tip, dir) in &s.arrows {
    let d = dir.normalized();
    if !d.is_finite() || d == Vec2::ZERO {
      continue;
    }
    let (base, n) = (*tip - d * 8.0, d.rot90() * 2.8);
    painter.add(egui::Shape::convex_polygon(vec![*tip, base + n, base - n], col, Stroke::NONE));
  }
}

/// A linear dimension measuring from `p1` to `p2` along `dir` (unit, sketch coordinates): extension
/// lines out to a dimension line `off` beyond the further point, on the side away from `away`.
fn linear_shape(
  p1: [f64; 2],
  p2: [f64; 2],
  dir: [f64; 2],
  away: [f64; 2],
  off: f64,
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
  let mut n = [-dir[1], dir[0]];
  let mid = [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0];
  if dot(n, [mid[0] - away[0], mid[1] - away[1]]) < 0.0 {
    n = [-n[0], -n[1]];
  }
  let level = dot(p1, n).max(dot(p2, n)) + off;
  let along = |p: [f64; 2], t: f64| [p[0] + n[0] * t, p[1] + n[1] * t];
  let (gap, over) = (3.0 * wpp, 4.0 * wpp);
  let mut lines = Vec::new();
  let mut feet = Vec::new();
  for p in [p1, p2] {
    let h = level - dot(p, n);
    let foot = along(p, h);
    if h > gap {
      lines.push([to_screen(along(p, gap))?, to_screen(along(p, h + over))?]);
    }
    feet.push(to_screen(foot)?);
  }
  let (a, b) = (feet[0], feet[1]);
  let d = b - a;
  let mut arrows = vec![(a, -d), (b, d)];
  lines.push([a, b]);
  if d.length() < 26.0 {
    // too short for arrows inside: they point in from outside
    let u = d.normalized();
    arrows = vec![(a, u), (b, -u)];
    lines.push([a - u * 14.0, a]);
    lines.push([b, b + u * 14.0]);
  }
  Some(DimShape { lines, arc: Vec::new(), arrows, label: a + d / 2.0 })
}

/// A radius or diameter dimension of the circle around `center` through `on`: arrows on the
/// circle and a leader out to the value beyond it.
fn radial_shape(
  center: [f64; 2],
  r: f64,
  on: [f64; 2],
  diameter: bool,
  label: Vec2,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let (dx, dy) = (on[0] - center[0], on[1] - center[1]);
  let l = dx.hypot(dy);
  if l.is_nan() || l <= 1e-12 {
    return None;
  }
  let (ux, uy) = (dx / l, dy / l);
  let c = to_screen(center)?;
  let p = to_screen([center[0] + r * ux, center[1] + r * uy])?;
  let u = (p - c).normalized();
  if !u.is_finite() {
    return None;
  }
  // the value sits clear of the circle, its near edge 10 points out
  let half = (u.x.abs() * label.x + u.y.abs() * label.y) / 2.0;
  let at = p + u * (half + 10.0);
  let start = if diameter { to_screen([center[0] - r * ux, center[1] - r * uy])? } else { c };
  let mut arrows = vec![(p, u)];
  if diameter {
    arrows.push((start, -u));
  }
  Some(DimShape { lines: vec![[start, at - u * half]], arc: Vec::new(), arrows, label: at })
}

/// Fusion's diameter dimension of a circle: a horizontal line through the centre with arrowheads
/// on the circle at both ends, the value sitting on the line at the centre.
fn diameter_shape(center: [f64; 2], r: f64, to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>) -> Option<DimShape> {
  let a = to_screen([center[0] - r, center[1]])?;
  let b = to_screen([center[0] + r, center[1]])?;
  let c = to_screen(center)?;
  let d = b - a;
  if !d.is_finite() || d.length() < 1e-6 {
    return None;
  }
  Some(DimShape { lines: vec![[a, b]], arc: Vec::new(), arrows: vec![(a, -d), (b, d)], label: c })
}

/// A diameter line through the centre in the direction of `at` (inside the circle), arrowheads
/// on the circle, with the value at `at`.
fn diameter_shape_towards(
  center: [f64; 2],
  r: f64,
  at: [f64; 2],
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let (dx, dy) = (at[0] - center[0], at[1] - center[1]);
  let l = dx.hypot(dy);
  let (ux, uy) = if l > 1e-9 { (dx / l, dy / l) } else { (1.0, 0.0) };
  let a = to_screen([center[0] - r * ux, center[1] - r * uy])?;
  let b = to_screen([center[0] + r * ux, center[1] + r * uy])?;
  let d = b - a;
  if !d.is_finite() || d.length() < 1e-6 {
    return None;
  }
  Some(DimShape { lines: vec![[a, b]], arc: Vec::new(), arrows: vec![(a, -d), (b, d)], label: to_screen(at)? })
}

/// An angle between two lines: an arc between them around where they meet, with extension lines
/// when the arc lands beyond a line's ends.
fn angular_shape(
  s: &Sketch,
  a: fox_engine::sketch::CurveId,
  b: fox_engine::sketch::CurveId,
  value: f64,
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let ends = |c| -> Option<([f64; 2], [f64; 2])> {
    let pts = s.curve_points(c);
    Some((s.pos(*pts.first()?)?, s.pos(*pts.get(1)?)?))
  };
  let ((a0, a1), (b0, b1)) = (ends(a)?, ends(b)?);
  let (da, db) = ([a1[0] - a0[0], a1[1] - a0[1]], [b1[0] - b0[0], b1[1] - b0[1]]);
  let cross = da[0] * db[1] - da[1] * db[0];
  if cross.abs() < 1e-12 {
    return None;
  }
  // where the (extended) lines cross
  let t = ((b0[0] - a0[0]) * db[1] - (b0[1] - a0[1]) * db[0]) / cross;
  let o = [a0[0] + da[0] * t, a0[1] + da[1] * t];
  let dist = |p: [f64; 2]| (p[0] - o[0]).hypot(p[1] - o[1]);
  let far = |p: [f64; 2], q: [f64; 2]| if dist(p) > dist(q) { p } else { q };
  let unit = |p: [f64; 2]| {
    let l = dist(p).max(1e-12);
    [(p[0] - o[0]) / l, (p[1] - o[1]) / l]
  };
  let (fa, fb) = (far(a0, a1), far(b0, b1));
  let u = unit(fa);
  let mut v = unit(fb);
  // the side of line b that shows the dimensioned value (not its supplement)
  let between = |p: [f64; 2], q: [f64; 2]| (p[0] * q[1] - p[1] * q[0]).atan2(p[0] * q[0] + p[1] * q[1]);
  if (between(u, v).abs() - value).abs() > (std::f64::consts::PI - between(u, v).abs() - value).abs() {
    v = [-v[0], -v[1]];
  }
  let sweep = between(u, v);
  let r = (0.7 * dist(fa).min(dist(fb))).clamp(30.0 * wpp, 90.0 * wpp);
  let start = u[1].atan2(u[0]);
  let at = |ang: f64, rr: f64| [o[0] + rr * ang.cos(), o[1] + rr * ang.sin()];
  let arc: Vec<Pos2> = (0..=32).filter_map(|k| to_screen(at(start + sweep * k as f64 / 32.0, r))).collect();
  if arc.len() < 2 {
    return None;
  }
  let k = arc.len();
  let arrows = vec![(arc[0], arc[0] - arc[1]), (arc[k - 1], arc[k - 1] - arc[k - 2])];
  // extension lines from a line's nearer end out to an arc end beyond it
  let mut lines = Vec::new();
  for ((p, q), end) in [((a0, a1), at(start, r)), ((b0, b1), at(start + sweep, r))] {
    let d = [q[0] - p[0], q[1] - p[1]];
    let len2 = (d[0] * d[0] + d[1] * d[1]).max(1e-24);
    let t = ((end[0] - p[0]) * d[0] + (end[1] - p[1]) * d[1]) / len2;
    if !(0.0..=1.0).contains(&t) {
      let near = if t < 0.0 { p } else { q };
      lines.push([to_screen(near)?, to_screen(end)?]);
    }
  }
  let mid = to_screen(at(start + sweep / 2.0, r))?;
  let os = to_screen(o)?;
  let w = (mid - os).normalized();
  let label = if w.is_finite() { mid + w * 16.0 } else { mid };
  Some(DimShape { lines, arc, arrows, label })
}

/// The leader graphics of dimension `k` (see [`DimShape`]); `size` is its value's box. A
/// dimension placed by the Dimension tool keeps its value where the click put it (`label`).
fn dim_shape(
  s: &Sketch,
  k: &ConstraintKind,
  dim: &fox_engine::sketch::Dim,
  label: Option<[f64; 2]>,
  size: Vec2,
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  if let Some(at) = label {
    return dim_shape_at(s, k, at, size, wpp, to_screen);
  }
  let center = sketch_center(s).unwrap_or([0.0, 0.0]);
  let off = DIM_OFFSET_PX * wpp;
  let unit = |p: [f64; 2], q: [f64; 2]| -> Option<[f64; 2]> {
    let l = (q[0] - p[0]).hypot(q[1] - p[1]);
    (l > 1e-12).then(|| [(q[0] - p[0]) / l, (q[1] - p[1]) / l])
  };
  match k {
    ConstraintKind::Distance { a, b, .. } => {
      let (p, q) = (s.pos(*a)?, s.pos(*b)?);
      linear_shape(p, q, unit(p, q)?, center, off, wpp, to_screen)
    }
    ConstraintKind::HorizontalDistance { a, b, .. } => {
      linear_shape(s.pos(*a)?, s.pos(*b)?, [1.0, 0.0], center, off, wpp, to_screen)
    }
    ConstraintKind::VerticalDistance { a, b, .. } => {
      linear_shape(s.pos(*a)?, s.pos(*b)?, [0.0, 1.0], center, off, wpp, to_screen)
    }
    ConstraintKind::PointLineDistance { p, line, .. } => {
      let pts = s.curve_points(*line);
      let (l0, l1, pp) = (s.pos(*pts.first()?)?, s.pos(*pts.get(1)?)?, s.pos(*p)?);
      let d = unit(l0, l1)?;
      let t = (pp[0] - l0[0]) * d[0] + (pp[1] - l0[1]) * d[1];
      let foot = [l0[0] + d[0] * t, l0[1] + d[1] * t];
      linear_shape(foot, pp, unit(foot, pp)?, center, off, wpp, to_screen)
    }
    ConstraintKind::Radius { curve, .. } | ConstraintKind::Diameter { curve, .. } => {
      let c = s.pos(*s.curve_points(*curve).first()?)?;
      let r = s.radius(*curve)?;
      let circle = matches!(s.curve(*curve).map(|c| &c.kind), Some(CurveKind::Circle { .. }));
      if circle && matches!(k, ConstraintKind::Diameter { .. }) {
        return diameter_shape(c, r, to_screen);
      }
      // radii: circles are dimensioned at 45°, arcs at their middle
      let on = match s.curve(*curve).map(|c| &c.kind) {
        Some(CurveKind::Arc { .. }) => {
          let pts = fox_engine::scene::curve_polyline(s, &s.curve(*curve)?.kind);
          *pts.get(pts.len() / 2)?
        }
        _ => [c[0] + r * std::f64::consts::FRAC_1_SQRT_2, c[1] + r * std::f64::consts::FRAC_1_SQRT_2],
      };
      radial_shape(c, r, on, matches!(k, ConstraintKind::Diameter { .. }), size, to_screen)
    }
    ConstraintKind::Angle { a, b, .. } => angular_shape(s, *a, *b, dim.value, wpp, to_screen),
    _ => None,
  }
}

fn dim_label(
  s: &Sketch,
  k: &ConstraintKind,
  dim: &fox_engine::sketch::Dim,
  unit: fox_engine::model::LengthUnit,
) -> Option<([f64; 2], String)> {
  // a plain value ("10", "1 in") shows as the number; an expression with parameters as
  // "fx: 2.00", Fusion's mark for a driven dimension
  let numeric = fox_engine::expr::eval_raw(&dim.expr, &fox_engine::expr::Empty).is_ok();
  let val = if k.is_angle() {
    // one decimal with the degree sign, as Fusion reads an angle ("17.2°")
    format!("{:.*}°", crate::units::angle_decimals(), dim.value.to_degrees())
  } else {
    // in the design's unit at the document precision, without the unit (as Fusion labels
    // sketch dimensions: "0.479", "12.00")
    fixed_number(unit, dim.value)
  };
  let shown = |prefix: &str| if numeric { format!("{prefix}{val}") } else { format!("fx: {prefix}{val}") };
  let mid = |a: PointId, b: PointId| -> Option<[f64; 2]> {
    let (p, q) = (s.pos(a)?, s.pos(b)?);
    Some([(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0])
  };
  match k {
    ConstraintKind::Distance { a, b, .. }
    | ConstraintKind::HorizontalDistance { a, b, .. }
    | ConstraintKind::VerticalDistance { a, b, .. } => Some((mid(*a, *b)?, shown(""))),
    ConstraintKind::PointLineDistance { p, line, .. } => {
      let pts = s.curve_points(*line);
      let q = mid(*pts.first()?, *pts.get(1)?)?;
      let pp = s.pos(*p)?;
      Some(([(pp[0] + q[0]) / 2.0, (pp[1] + q[1]) / 2.0], shown("")))
    }
    ConstraintKind::Radius { curve, .. } | ConstraintKind::Diameter { curve, .. } => {
      let c = s.pos(*s.curve_points(*curve).first()?)?;
      let r = s.radius(*curve)?;
      let a = std::f64::consts::FRAC_PI_4;
      let prefix = if matches!(k, ConstraintKind::Radius { .. }) { "R" } else { "⌀" };
      Some(([c[0] + r * a.cos(), c[1] + r * a.sin()], shown(prefix)))
    }
    ConstraintKind::Angle { a, .. } => {
      let pts = s.curve_points(*a);
      Some((mid(*pts.first()?, *pts.get(1)?)?, shown("")))
    }
    _ => None,
  }
}

fn sketch_center(s: &Sketch) -> Option<[f64; 2]> {
  let (lo, hi) = s.bounds()?;
  Some([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0])
}

/// The curves a curve-picking sketch tool has picked so far, for the status bar's readout.
pub(crate) fn picked_curves(app: &App) -> Vec<Sel> {
  let Some(sk) = app.s.edit_sketch else { return Vec::new() };
  app.tstate.curves.iter().map(|c| Sel::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(c.0) }).collect()
}

/// The least distance between two curves (Fusion's "Min Distance" readout for two selected
/// sketch curves): the nearest of samples along each to the other, zero when two lines cross.
pub(crate) fn curve_min_distance(
  s: &Sketch,
  a: fox_engine::sketch::CurveId,
  b: fox_engine::sketch::CurveId,
) -> Option<f64> {
  let sample = |from: fox_engine::sketch::CurveId, to: fox_engine::sketch::CurveId| -> Option<f64> {
    let pts = fox_engine::scene::curve_polyline(s, &s.curve(from)?.kind);
    let mut best = f64::INFINITY;
    for w in pts.windows(2) {
      for k in 0..=16 {
        let t = k as f64 / 16.0;
        best =
          best.min(s.distance_to_curve(to, [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t])?);
      }
    }
    best.is_finite().then_some(best)
  };
  let d = sample(a, b)?.min(sample(b, a)?);
  if let (Some(CurveKind::Line { a: a0, b: a1 }), Some(CurveKind::Line { a: b0, b: b1 })) =
    (s.curve(a).map(|c| &c.kind), s.curve(b).map(|c| &c.kind))
  {
    let (p0, p1, q0, q1) = (s.pos(*a0)?, s.pos(*a1)?, s.pos(*b0)?, s.pos(*b1)?);
    let cross = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0]);
    let (d1, d2) = (cross(p0, p1, q0), cross(p0, p1, q1));
    let (d3, d4) = (cross(q0, q1, p0), cross(q0, q1, p1));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
      return Some(0.0);
    }
  }
  Some(d)
}

/// Rubber-band preview of the shape being drawn.
fn tool_preview(app: &App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  let Some((_, plane)) = sketch_plane(app) else { return };
  if matches!(app.tool, Tool::Select | Tool::Dimension | Tool::Constrain(_))
    || app.tool.picks_curves() && app.tool != Tool::Circle2T
  {
    return;
  }
  let Some(pos) = resp.hover_pos() else { return };
  if app.tool == Tool::Circle2T {
    tangent_circle_preview(app, painter, view, pos, plane);
    return;
  }
  let Some((cur, snapped)) = sketch_cursor(app, view, pos) else { return };
  // values typed in the heads-up boxes hold the shape to them
  let cur = hud_apply(app, cur);
  let s = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let stroke = Stroke::new(1.5, Color32::from_rgb(30, 110, 235));
  let line = |a: [f64; 2], b: [f64; 2]| {
    if let (Some(a), Some(b)) = (s(a), s(b)) {
      painter.line_segment([a, b], stroke);
    }
  };
  let poly = |pts: &[[f64; 2]], closed: bool| {
    for w in pts.windows(2) {
      line(w[0], w[1]);
    }
    if closed && pts.len() > 2 {
      line(pts[pts.len() - 1], pts[0]);
    }
  };
  // construction geometry the tool will keep: grey dashed, as Fusion previews it
  let dashed = |pts: &[[f64; 2]]| {
    let v: Vec<Pos2> = pts.iter().filter_map(|p| s(*p)).collect();
    if v.len() >= 2 {
      painter.extend(egui::Shape::dashed_line(&v, Stroke::new(1.0, Color32::from_gray(128)), 6.0, 4.0));
    }
  };
  let circle = |c: [f64; 2], r: f64| {
    let pts: Vec<[f64; 2]> = (0..=64)
      .map(|i| {
        let t = std::f64::consts::TAU * i as f64 / 64.0;
        [c[0] + r * t.cos(), c[1] + r * t.sin()]
      })
      .collect();
    poly(&pts, false);
  };
  let pts = &app.tstate.pts;
  match (app.tool, pts.as_slice()) {
    (Tool::Line, [.., last]) if app.tstate.line_arc => match tangent_arc_points(app, *last, cur) {
      Some(arc) => poly(&arc, false),
      None => line(*last, cur),
    },
    (Tool::Line, [.., last]) => line(*last, cur),
    (Tool::TangentArc, [a]) if tangent_arc_points(app, *a, cur).is_some() => {
      poly(&tangent_arc_points(app, *a, cur).unwrap_or_default(), false)
    }
    (Tool::Spline | Tool::ControlSpline, [_, ..]) => {
      // the curve itself, smooth, through (or shaped by) the clicks and the cursor
      let mut all = pts.clone();
      all.push(cur);
      let bz = spline_beziers(app, app.tool, &all);
      poly(&flatten_beziers(&bz), false);
      if app.tool == Tool::ControlSpline {
        // the control polygon, dashed
        let ctrl: Vec<Pos2> = all.iter().filter_map(|p| s(*p)).collect();
        painter.extend(egui::Shape::dashed_line(&ctrl, Stroke::new(1.0, Color32::from_gray(140)), 4.0, 3.0));
      } else {
        // the tangent comb: the curve's direction at each fit point
        for (p, t) in spline_tangents(&bz) {
          if let (Some(a), Some(b)) = (s(p), s([p[0] + t[0], p[1] + t[1]])) {
            let d = (b - a).normalized() * 12.0;
            if d.is_finite() {
              painter.line_segment([a - d, a + d], Stroke::new(1.0, Color32::from_gray(140)));
            }
          }
        }
      }
    }
    (Tool::Conic, [a]) => line(*a, cur),
    (Tool::Conic, [a, b]) => {
      line(*a, *b);
      poly(&conic_points(*a, cur, *b, conic_rho(app)), false);
    }
    // the vertex is down: the conic takes its shape from the rho box and arrow, not the cursor
    (Tool::Conic, [a, b, apex]) => poly(&conic_points(*a, *apex, *b, conic_rho(app)), false),
    (Tool::ArcSlot3, [a]) | (Tool::ArcSlotCenter, [a]) => line(*a, cur),
    (Tool::ArcSlot3, [a, b]) => {
      if let Some((c, r, a0, sweep)) = arc_through(*a, cur, *b) {
        poly(&arc_points(c, r, a0, sweep), false);
      }
    }
    (Tool::ArcSlotCenter, [c, st]) => {
      let r = (st[0] - c[0]).hypot(st[1] - c[1]);
      let a0 = (st[1] - c[1]).atan2(st[0] - c[0]);
      let sweep = ((cur[1] - c[1]).atan2(cur[0] - c[0]) - a0).rem_euclid(std::f64::consts::TAU);
      poly(&arc_points(*c, r, a0, sweep), false);
    }
    (Tool::ArcSlot3, [a, b, m]) | (Tool::ArcSlotCenter, [a, b, m]) => {
      // the slot: its centre arc and the two edges half the width either side
      let geo = if app.tool == Tool::ArcSlot3 {
        arc_through(*a, *m, *b)
      } else {
        let r = (b[0] - a[0]).hypot(b[1] - a[1]);
        let a0 = (b[1] - a[1]).atan2(b[0] - a[0]);
        Some((*a, r, a0, ((m[1] - a[1]).atan2(m[0] - a[0]) - a0).rem_euclid(std::f64::consts::TAU)))
      };
      if let Some((c, r, a0, sweep)) = geo {
        // the finished outline (concentric arcs and round caps) about the dashed centreline
        let w = 2.0 * ((cur[0] - c[0]).hypot(cur[1] - c[1]) - r).abs();
        dashed(&arc_points(c, r, a0, sweep));
        poly(&arc_slot_outline(c, r, a0, sweep, w), true);
      }
    }
    (Tool::Rect3, [a]) => line(*a, cur),
    (Tool::Rect3, [a, b]) => {
      let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
      let l = ux.hypot(uy).max(1e-12);
      let (nx, ny) = (-uy / l, ux / l);
      let h = (cur[0] - a[0]) * nx + (cur[1] - a[1]) * ny;
      poly(&[*a, *b, [b[0] + nx * h, b[1] + ny * h], [a[0] + nx * h, a[1] + ny * h]], true);
    }
    (Tool::Circle2P, [a]) => {
      circle([(a[0] + cur[0]) / 2.0, (a[1] + cur[1]) / 2.0], (cur[0] - a[0]).hypot(cur[1] - a[1]) / 2.0)
    }
    (Tool::Circle3P, [a]) => line(*a, cur),
    (Tool::Circle3P, [a, b]) => {
      if let Some(c) = fox_engine::sketch::circumcenter(*a, *b, cur) {
        circle(c, (a[0] - c[0]).hypot(a[1] - c[1]));
      }
    }
    (Tool::MidpointLine, [a]) => line([2.0 * a[0] - cur[0], 2.0 * a[1] - cur[1]], cur),
    (Tool::TangentArc, [a]) => line(*a, cur),
    (Tool::Polygon | Tool::CircPolygon, [c]) => {
      // the polygon and the circle it is inscribed in (or circumscribes), as in Fusion
      let n = hud_sides_live(app);
      let (dx, dy) = (cur[0] - c[0], cur[1] - c[1]);
      let d = dx.hypot(dy);
      let half = std::f64::consts::PI / n as f64;
      let (r, a0) =
        if app.tool == Tool::CircPolygon { (d / half.cos(), dy.atan2(dx) + half) } else { (d, dy.atan2(dx)) };
      circle(*c, d);
      poly(&polygon_points(*c, r, a0, n), true);
    }
    (Tool::EdgePolygon, [a]) => poly(&edge_polygon_points(*a, cur, hud_sides_live(app)), true),
    (Tool::EdgePolygon, [a, b]) => {
      // the polygon on whichever side of the edge the cursor is, until the click fixes it
      let n = hud_sides_live(app);
      let left = (b[0] - a[0]) * (cur[1] - a[1]) - (b[1] - a[1]) * (cur[0] - a[0]) >= 0.0;
      poly(&if left { edge_polygon_points(*a, *b, n) } else { edge_polygon_points(*b, *a, n) }, true);
    }
    (Tool::OverallSlot | Tool::CenterSlot, [a]) => line(*a, cur),
    // the ellipse's major axis, kept as construction
    (Tool::Ellipse, [a]) => dashed(&[*a, cur]),
    (Tool::Ellipse, [a, b]) => {
      // through the cursor; off the ends of the axis, as far from it as the cursor
      let w = Ellipse2::minor_through(*a, *b, cur).unwrap_or(point_line_dist(cur, *a, *b));
      let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
      let r = dx.hypot(dy).max(1e-12);
      let (c, s) = (dx / r, dy / r);
      let pts: Vec<[f64; 2]> = (0..=64)
        .map(|i| {
          let t = std::f64::consts::TAU * i as f64 / 64.0;
          let (x, y) = (r * t.cos(), w * t.sin());
          [a[0] + x * c - y * s, a[1] + x * s + y * c]
        })
        .collect();
      poly(&pts, false);
      dashed(&[[2.0 * a[0] - b[0], 2.0 * a[1] - b[1]], *b]);
      dashed(&[[a[0] + w * s, a[1] - w * c], [a[0] - w * s, a[1] + w * c]]);
    }
    (Tool::OverallSlot, [a, b]) => {
      // the round ends fit inside the overall length
      let w = 2.0 * point_line_dist(cur, *a, *b);
      let l = (b[0] - a[0]).hypot(b[1] - a[1]);
      if l > w {
        let k = w / 2.0 / l;
        let d = [(b[0] - a[0]) * k, (b[1] - a[1]) * k];
        let (c1, c2) = ([a[0] + d[0], a[1] + d[1]], [b[0] - d[0], b[1] - d[1]]);
        dashed(&[c1, c2]);
        poly(&slot_outline(c1, c2, w), true);
      } else {
        line(*a, *b);
      }
    }
    (Tool::CenterSlot, [c, e]) => {
      let o = [2.0 * c[0] - e[0], 2.0 * c[1] - e[1]];
      let w = 2.0 * point_line_dist(cur, o, *e);
      dashed(&[o, *e]);
      poly(&slot_outline(o, *e, w), true);
    }
    (Tool::Rect, [a]) => poly(&[*a, [cur[0], a[1]], cur, [a[0], cur[1]]], true),
    (Tool::CenterRect, [c]) => {
      let o = [2.0 * c[0] - cur[0], 2.0 * c[1] - cur[1]];
      poly(&[o, [cur[0], o[1]], cur, [o[0], cur[1]]], true);
      // the two construction diagonals the rectangle keeps
      dashed(&[o, cur]);
      dashed(&[[cur[0], o[1]], [o[0], cur[1]]]);
    }
    (Tool::Circle, [c]) => circle(*c, (cur[0] - c[0]).hypot(cur[1] - c[1])),
    (Tool::Arc3 | Tool::CenterArc | Tool::Slot, [a]) => line(*a, cur),
    (Tool::Arc3, [a, b]) => {
      // the arc through the pointer, with Fusion's cross at each end and at the pointer
      if let Some((c, r, a0, sweep)) = arc_through(*a, cur, *b) {
        poly(&arc_points(c, r, a0, sweep), false);
      } else {
        line(*a, *b);
      }
      for p in [*a, *b, cur] {
        if let Some(q) = s(p) {
          painter.line_segment([q + Vec2::new(-5.0, 0.0), q + Vec2::new(5.0, 0.0)], stroke);
          painter.line_segment([q + Vec2::new(0.0, -5.0), q + Vec2::new(0.0, 5.0)], stroke);
        }
      }
    }
    (Tool::CenterArc, [c, a]) => {
      // the arc so far, the way the cursor wound; a full circle until it moves
      let r = (a[0] - c[0]).hypot(a[1] - c[1]);
      match app.tstate.arc_wind {
        Some((_, sweep)) if sweep.abs() > 1e-9 => {
          let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
          let pts: Vec<[f64; 2]> = (0..=64)
            .map(|i| {
              let t = a0 + sweep * i as f64 / 64.0;
              [c[0] + r * t.cos(), c[1] + r * t.sin()]
            })
            .collect();
          poly(&pts, false);
        }
        _ => circle(*c, r),
      }
    }
    (Tool::Slot, [a, b]) => {
      // the finished outline while the width is set, and the centreline it keeps
      let w = 2.0 * point_line_dist(cur, *a, *b);
      dashed(&[*a, *b]);
      poly(&slot_outline(*a, *b, w), true);
    }
    _ => {}
  }
  paint_inference_lines(app, painter, view, pos);
  // Fusion's snap marker: a small square on the sketch point the cursor is on (the cursor itself
  // is a crosshair, with no coordinate readout)
  if snapped.is_some()
    && let Some(p) = s(cur)
  {
    paint_snap_marker(painter, p);
  }
}

/// The square Fusion draws on a sketch point a tool's cursor has snapped to.
fn paint_snap_marker(painter: &egui::Painter, at: Pos2) {
  let r = Rect::from_center_size(at, Vec2::splat(8.0));
  painter.rect(
    r,
    0.0,
    Color32::from_rgba_unmultiplied(255, 255, 255, 200),
    Stroke::new(1.5, Color32::from_gray(50)),
    egui::StrokeKind::Middle,
  );
}

/// The spline through (fit-point) or shaped by (control-point) `pts`, as cubic Bézier pieces: the
/// curve the tool is drawing, before any of its points exist in the sketch.
fn spline_beziers(app: &App, tool: Tool, pts: &[[f64; 2]]) -> Vec<[[f64; 2]; 4]> {
  let Some(sk) = app.s.edit_sketch else { return Vec::new() };
  let Ok(mut sketch) = app.s.design.sketch(sk).cloned() else { return Vec::new() };
  if pts.len() < 2 {
    return Vec::new();
  }
  let ids: Vec<PointId> = pts.iter().map(|p| sketch.add_point(p[0], p[1])).collect();
  let kind = if tool == Tool::ControlSpline { CurveKind::control_spline(ids) } else { CurveKind::spline(ids) };
  sketch.kind_beziers(&kind)
}

/// Bézier pieces flattened to one polyline.
fn flatten_beziers(bz: &[[[f64; 2]; 4]]) -> Vec<[f64; 2]> {
  let mut out: Vec<[f64; 2]> =
    bz.iter().flat_map(|b| (0..16).map(move |k| fox_engine::sketch::bezier_at(b, k as f64 / 16.0))).collect();
  out.extend(bz.last().map(|b| b[3]));
  out
}

/// Where each Bézier piece starts (and the last ends), with the curve's direction there.
fn spline_tangents(bz: &[[[f64; 2]; 4]]) -> Vec<([f64; 2], [f64; 2])> {
  let mut out: Vec<([f64; 2], [f64; 2])> = bz.iter().map(|b| (b[0], [b[1][0] - b[0][0], b[1][1] - b[0][1]])).collect();
  out.extend(bz.last().map(|b| (b[3], [b[3][0] - b[2][0], b[3][1] - b[2][1]])));
  out
}

/// Points along the conic from `a` to `b` whose end tangents meet at `apex`, shaped by `rho`.
fn conic_points(a: [f64; 2], apex: [f64; 2], b: [f64; 2], rho: f64) -> Vec<[f64; 2]> {
  let w = fox_engine::sketch::conic_weight(rho);
  (0..=64).map(|i| fox_engine::sketch::conic_at(&[a, apex, b], w, i as f64 / 64.0)).collect()
}

/// Digits typed with no box focused go to the heads-up box of the shape being drawn, or to the
/// open dialog's primary value, as in Fusion. Runs before the shortcuts, so a digit can't also
/// switch the selection mode.
pub fn route_typing(app: &mut App, ctx: &egui::Context) {
  take_hud_tab(app, ctx);
  if ctx.egui_wants_keyboard_input() || app.drawing.is_some() || app.dim_edit.is_some() || app.marking.is_some() {
    return;
  }
  if ctx.input(|i| i.modifiers.command || i.modifiers.alt) {
    return;
  }
  let typed: String = ctx.input(|i| {
    i.events
      .iter()
      .filter_map(|e| match e {
        egui::Event::Text(t) => Some(t.as_str()),
        _ => None,
      })
      .collect()
  });
  let value_start = typed.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '-');
  let to_hud = app.s.edit_sketch.is_some() && !hud_fields(app.tool, hud_step(app)).is_empty();
  // the focused heads-up box takes any character, letters included ("d1*2" goes straight in, as
  // in Fusion); a dialog's value takes what starts a number
  let expr_start = typed.starts_with(|c: char| c.is_alphanumeric() || "(.-".contains(c));
  if !(value_start || (to_hud && expr_start)) {
    return;
  }
  use egui::Key;
  if to_hud {
    let hud = &mut app.tstate.hud;
    hud.typed[hud.active] = typed.clone();
    hud.buf = Some(typed);
    hud.focus = Some(false);
    // the keys that typed it are the box's, not shortcuts
    ctx.input_mut(|i| {
      i.events.retain(|e| {
        !matches!(e, egui::Event::Key { key, .. } if !matches!(key,
          Key::Escape | Key::Enter | Key::Tab | Key::Backspace | Key::Delete | Key::ArrowLeft | Key::ArrowRight
            | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End))
      })
    });
    return;
  }
  if app.dialog.is_none() {
    return;
  }
  crate::dialogs::type_into(ctx, typed);
  ctx.input_mut(|i| {
    i.events.retain(|e| {
      !matches!(e, egui::Event::Key { key, .. } if matches!(key,
        Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8
          | Key::Num9 | Key::Minus | Key::Period))
    })
  });
}

/// Tab belongs to the heads-up boxes while a shape with more than one is drawn (and no other
/// text box has the keyboard). It's taken before any widget runs, so egui's focus traversal
/// can't also move the keyboard (to a panel's field, or back to the box it came from).
fn take_hud_tab(app: &mut App, ctx: &egui::Context) {
  let has_boxes = !hud_fields(app.tool, hud_step(app)).is_empty();
  let hud = &mut app.tstate.hud;
  hud.tab = None;
  // a text box elsewhere has the keyboard: Tab moves egui's focus between fields
  if hud.buf.is_none() && ctx.egui_wants_keyboard_input() {
    return;
  }
  let (tab, shift_tab) = ctx.input_mut(|i| {
    (i.consume_key(egui::Modifiers::NONE, egui::Key::Tab), i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
  });
  if !(tab || shift_tab) {
    return;
  }
  // with nothing focused, egui would give the keyboard to the first widget drawn (the Data Panel
  // button, which Enter then presses); in Fusion Tab never reaches the toolbar
  ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
  if app.s.edit_sketch.is_some()
    && has_boxes
    && app.drawing.is_none()
    && app.dim_edit.is_none()
    && app.marking.is_none()
  {
    hud.tab = Some(shift_tab);
  }
}

/// The heads-up dimensions of the shape being drawn: each value's dimension follows the cursor
/// with its box. Typing sets the active box, Tab moves to the next, Enter places the point.
fn heads_up(app: &mut App, painter: &egui::Painter, resp: &egui::Response, view: &View, ctx: &egui::Context) {
  let tab = app.tstate.hud.tab.take();
  let fields = hud_fields(app.tool, hud_step(app));
  let Some((sk, plane)) = sketch_plane(app).filter(|_| !fields.is_empty()) else {
    if app.tstate.hud.buf.is_some() || !app.tstate.hud.typed.iter().all(String::is_empty) {
      app.tstate.hud = HeadsUp::default();
    }
    return;
  };
  if let Some(pos) = resp.hover_pos()
    && let Some(c) = sketch_cursor(app, view, pos)
  {
    app.tstate.hud.cursor = Some(c);
  }
  let Some((raw, snapped)) = app.tstate.hud.cursor else { return };
  // the 2-tangent circle has placed no point yet: its box follows the circle under the cursor
  let last = app.tstate.pts.last().copied().unwrap_or(raw);
  let cur = hud_apply(app, raw);
  let live = hud_live(app, cur);
  let wpp = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let ink = Color32::from_rgb(30, 110, 235);
  let off = 30.0 * wpp;
  // each box's dimension graphics, ending where its box goes
  let mut at: Vec<Option<Pos2>> = Vec::new();
  let pts = app.tstate.pts.clone();
  // a length from `o` to `q`, its dimension on the left of o→q
  let along = |o: [f64; 2], q: [f64; 2]| {
    let (dx, dy) = (q[0] - o[0], q[1] - o[1]);
    let l = dx.hypot(dy).max(1e-12);
    let away = [(o[0] + q[0]) / 2.0 + dy / l, (o[1] + q[1]) / 2.0 - dx / l];
    linear_shape(o, q, [dx / l, dy / l], away, off, wpp, &to_screen)
  };
  // a radius box beside the circle through `on` about `c`
  let radial = |c: [f64; 2], r: f64, on: [f64; 2]| radial_shape(c, r, on, false, Vec2::new(60.0, 18.0), &to_screen);
  // a width across the line a→b at `foot`, dimensioned beyond `foot` away from `mid`
  let across = |a: [f64; 2], b: [f64; 2], mid: [f64; 2], foot: [f64; 2], w: f64| {
    let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
    let l = ex.hypot(ey).max(1e-12);
    let n = [-ey / l, ex / l];
    let (p, q) =
      ([foot[0] - n[0] * w / 2.0, foot[1] - n[1] * w / 2.0], [foot[0] + n[0] * w / 2.0, foot[1] + n[1] * w / 2.0]);
    linear_shape(p, q, n, mid, off, wpp, &to_screen)
  };
  let mut rho_at: Option<Pos2> = None;
  for f in fields {
    let shape = match (app.tool, f, pts.as_slice()) {
      (Tool::Line, Field::Length, _) => hud_length_shape(last, cur, off, wpp, &to_screen),
      (Tool::EdgePolygon, Field::Length, _) => along(last, cur),
      (Tool::MidpointLine, Field::Length, _) => along([2.0 * last[0] - cur[0], 2.0 * last[1] - cur[1]], cur),
      // the angle's arc starts from a dashed horizontal reference at the line's start, in
      // Fusion's darker orange for a dimension's reference
      (Tool::Line | Tool::MidpointLine, Field::Angle, _) => {
        hud_angle_shape(last, cur, wpp, &to_screen).map(|(s, r)| {
          painter.extend(egui::Shape::dashed_line(&r, Stroke::new(1.0, ANGLE_REFERENCE), 9.0, 3.0));
          s
        })
      }
      (Tool::Rect3, Field::Length, _) => along(last, cur),
      (Tool::Rect3, Field::Width, [a, b]) => {
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let l = ex.hypot(ey).max(1e-12);
        let n = [-ey / l, ex / l];
        let h = (cur[0] - a[0]) * n[0] + (cur[1] - a[1]) * n[1];
        along(*b, [b[0] + n[0] * h, b[1] + n[1] * h])
      }
      // a diameter line through the circle, with the value on it
      (Tool::Circle2P, Field::Diameter, _) => chord_shape(last, cur, &to_screen),
      (Tool::Circle3P, Field::Diameter, [a, b]) => match fox_engine::sketch::circumcenter(*a, *b, cur) {
        Some(c) => {
          let r = (a[0] - c[0]).hypot(a[1] - c[1]);
          let (dx, dy) = (cur[0] - c[0], cur[1] - c[1]);
          let l = dx.hypot(dy).max(1e-12);
          chord_shape([c[0] - r * dx / l, c[1] - r * dy / l], [c[0] + r * dx / l, c[1] + r * dy / l], &to_screen)
        }
        None => None,
      },
      (Tool::Arc3 | Tool::ArcSlot3, Field::Radius, [a, b]) => {
        arc_through(*a, cur, *b).and_then(|(c, r, _, _)| radial(c, r, cur))
      }
      (Tool::CenterArc | Tool::ArcSlotCenter | Tool::Polygon | Tool::CircPolygon, Field::Radius, _) => {
        radial(last, (cur[0] - last[0]).hypot(cur[1] - last[1]), cur)
      }
      (Tool::TangentArc, Field::Radius, [a]) => {
        tangent_arc_geometry(app, *a, cur).and_then(|(c, r, _, _)| radial(c, r, cur))
      }
      (Tool::CenterArc | Tool::ArcSlotCenter, Field::Angle, [c, st]) => {
        let r = (st[0] - c[0]).hypot(st[1] - c[1]);
        let a0 = (st[1] - c[1]).atan2(st[0] - c[0]);
        let sweep = if app.tool == Tool::CenterArc {
          app.tstate.arc_wind.map_or(0.0, |(_, s)| s)
        } else {
          ((cur[1] - c[1]).atan2(cur[0] - c[0]) - a0).rem_euclid(std::f64::consts::TAU)
        };
        hud_sweep_shape(*c, r, a0, sweep, &to_screen)
      }
      (Tool::Ellipse, Field::Length, [c, m]) => {
        let b = Ellipse2::minor_through(*c, *m, cur).unwrap_or_else(|| point_line_dist(cur, *c, *m));
        let (ex, ey) = (m[0] - c[0], m[1] - c[1]);
        let l = ex.hypot(ey).max(1e-12);
        let side = if (cur[0] - c[0]) * -ey + (cur[1] - c[1]) * ex < 0.0 { -1.0 } else { 1.0 };
        along(*c, [c[0] - ey / l * b * side, c[1] + ex / l * b * side])
      }
      (Tool::Ellipse | Tool::Slot | Tool::OverallSlot | Tool::CenterSlot | Tool::ArcSlot3, Field::Length, _) => {
        along(last, cur)
      }
      (Tool::Slot | Tool::OverallSlot | Tool::CenterSlot, Field::Width, [a, b]) => {
        let mid = if app.tool == Tool::CenterSlot { *a } else { [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0] };
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let l2 = (ex * ex + ey * ey).max(1e-12);
        let t = ((cur[0] - a[0]) * ex + (cur[1] - a[1]) * ey) / l2;
        let foot = [a[0] + ex * t, a[1] + ey * t];
        across(*a, *b, mid, foot, 2.0 * point_line_dist(cur, *a, *b))
      }
      (Tool::ArcSlot3 | Tool::ArcSlotCenter, Field::Width, [a, b, m]) => {
        let geo = if app.tool == Tool::ArcSlot3 {
          arc_through(*a, *m, *b).map(|(c, r, _, _)| (c, r))
        } else {
          Some((*a, (b[0] - a[0]).hypot(b[1] - a[1])))
        };
        match geo {
          Some((c, r)) => {
            let (dx, dy) = (cur[0] - c[0], cur[1] - c[1]);
            let l = dx.hypot(dy).max(1e-12);
            let u = [dx / l, dy / l];
            let half = (l - r).abs();
            let (p, q) = (
              [c[0] + u[0] * (r - half), c[1] + u[1] * (r - half)],
              [c[0] + u[0] * (r + half), c[1] + u[1] * (r + half)],
            );
            linear_shape(p, q, u, c, off, wpp, &to_screen)
          }
          None => None,
        }
      }
      // the rho box sits under the conic's middle, by its arrow
      (Tool::Conic, Field::Rho, [a, b, apex]) => {
        let m = fox_engine::sketch::conic_at(&[*a, *apex, *b], fox_engine::sketch::conic_weight(conic_rho(app)), 0.5);
        rho_at = to_screen(m).map(|p| p + Vec2::new(0.0, 30.0));
        None
      }
      // the 2-point rectangle's focused box floats beside the cursor (placed below)
      (Tool::Rect, _, _) if fields.iter().position(|x| x == f) == Some(app.tstate.hud.active) => None,
      (Tool::Rect | Tool::CenterRect, Field::Width | Field::Height, _) => {
        // the width under the bottom edge and the height beside the right edge, between
        // extension lines, whichever way the rectangle was dragged
        let o = if app.tool == Tool::CenterRect { [2.0 * last[0] - cur[0], 2.0 * last[1] - cur[1]] } else { last };
        let (x0, x1) = (o[0].min(cur[0]), o[0].max(cur[0]));
        let (y0, y1) = (o[1].min(cur[1]), o[1].max(cur[1]));
        let mid = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        if *f == Field::Width {
          linear_shape([x0, y0], [x1, y0], [1.0, 0.0], mid, off, wpp, &to_screen)
        } else {
          linear_shape([x1, y0], [x1, y1], [0.0, 1.0], mid, off, wpp, &to_screen)
        }
      }
      (Tool::Circle, Field::Diameter, _) => {
        let r = (cur[0] - last[0]).hypot(cur[1] - last[1]);
        radial_shape(last, r, cur, true, Vec2::new(60.0, 18.0), &to_screen)
      }
      // the radius box beside the tangent circle, on the cursor's side of its centre
      (Tool::Circle2T, Field::Radius, _) => tangent_circle_geometry(app, sk, cur).and_then(|(c, r)| {
        let on = if (cur[0] - c[0]).hypot(cur[1] - c[1]) > 1e-9 { cur } else { [c[0], c[1] + r] };
        radial(c, r, on)
      }),
      _ => None,
    };
    if let Some(s) = &shape {
      paint_dim(painter, s, ink);
    }
    at.push(shape.map(|s| s.label).or(rho_at));
  }
  // a polygon's sides box sits under its radius box, as in Fusion
  if let Some(i) = fields.iter().position(|f| *f == Field::Sides) {
    let under = at.first().copied().flatten().map(|p| p + Vec2::new(0.0, 26.0));
    at[i] = under;
  }
  let n = fields.len();
  // the 2-point rectangle's focused box sits just left of the cursor, as Fusion's does
  if app.tool == Tool::Rect
    && let Some(slot) = at.get_mut(app.tstate.hud.active.min(n - 1))
  {
    *slot = to_screen(raw).map(|p| p + Vec2::new(-56.0, 0.0));
  }
  // Tab accepts the box's text and writes it back as Fusion spells it ("( 1 / 2 ) * 1 in")
  if tab.is_some()
    && let Some(accepted) = hud_accepted(app, app.tstate.hud.active.min(n - 1))
  {
    let i = app.tstate.hud.active.min(n - 1);
    app.tstate.hud.typed[i] = accepted;
  }
  // lengths in the design's unit (a bare number typed back means that unit, see [`hud_text`])
  let unit = app.s.design.units;
  let fmt = |i: usize| hud_display(unit, fields[i], live[i]);
  let hud = &mut app.tstate.hud;
  hud.active = hud.active.min(n - 1);
  let mut commit = false;
  // Tab keeps what was typed and moves on to the next box (Shift+Tab: the previous one), or with
  // no box focused starts typing in the next box; either way the box's value is selected
  if let Some(back) = tab {
    hud.active = if back { (hud.active + n - 1) % n } else { (hud.active + 1) % n };
    let t = if hud.typed[hud.active].is_empty() { fmt(hud.active) } else { hud.typed[hud.active].clone() };
    hud.buf = Some(t);
    hud.focus = Some(true);
  }
  for (i, p) in at.iter().enumerate() {
    let Some(p) = *p else { continue };
    let typed = !hud.typed[i].is_empty();
    if i == hud.active
      && let Some(mut buf) = hud.buf.clone()
    {
      let mut keep = true;
      egui::Area::new(egui::Id::new(("hud-box", i)))
        .fixed_pos(p)
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
          ui.horizontal(|ui| {
            // a fixed id: the padlock appearing by the box must not change it (and drop focus)
            let r = ui.add(
              egui::TextEdit::singleline(&mut buf)
                .id(egui::Id::new(("hud-edit", i)))
                .desired_width(62.0)
                .font(egui::FontId::proportional(12.0))
                .margin(egui::Margin::symmetric(3, 1))
                .lock_focus(true),
            );
            // a typed value is locked: Fusion's yellow padlock to the right of the box, which
            // stays editable until Enter
            if typed {
              let (lr, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
              paint_lock(ui.painter(), lr.center(), HUD_LOCK);
            }
            if let Some(select) = hud.focus.take() {
              focus_text(ctx, &r, &buf, select);
            }
            if r.changed() {
              hud.typed[i] = buf.clone();
            }
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if enter && (r.has_focus() || r.lost_focus()) {
              commit = true;
              keep = false;
            } else if esc || (!r.has_focus() && hud.focus.is_none() && r.lost_focus()) {
              // Esc (or a click elsewhere) stops typing; the typed values stay until the point is placed
              keep = false;
            }
          });
        });
      hud.buf = keep.then_some(buf).filter(|_| !commit);
      continue;
    }
    // the other boxes show the live value, or what was typed (locked, with its unit) with a
    // padlock to their right
    let text = if typed { accepted_text(unit, fields[i], &hud.typed[i]) } else { fmt(i) };
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text, font, if typed { Color32::BLACK } else { Color32::from_gray(60) });
    let r = Rect::from_center_size(p, galley.size() + Vec2::new(10.0, 5.0));
    let active = i == hud.active;
    // as Fusion's: no border until the box has the keyboard
    painter.rect(
      r,
      2.0,
      Color32::from_rgba_unmultiplied(255, 255, 255, 235),
      if active { Stroke::new(1.5, ink) } else { Stroke::NONE },
      egui::StrokeKind::Inside,
    );
    if typed {
      paint_lock(painter, r.right_center() + Vec2::new(9.0, 0.0), HUD_LOCK);
    }
    let text_at = r.min + Vec2::new(5.0, 2.5);
    if active && !typed {
      // the box with the keyboard shows its value selected, as Fusion's does: typing replaces it
      painter.rect_filled(Rect::from_min_size(text_at, galley.size()), 0.0, Color32::from_rgb(190, 214, 248));
    }
    let size = galley.size();
    painter.galley(text_at, galley, Color32::BLACK);
    if active && !typed {
      // and Fusion's blue underline under the focused value
      let y = text_at.y + size.y + 1.0;
      painter.line_segment([Pos2::new(text_at.x, y), Pos2::new(text_at.x + size.x, y)], Stroke::new(1.5, ink));
    }
  }
  if commit {
    // this Enter places the point; it isn't the open dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let ends = matches!(app.tool, Tool::Rect | Tool::CenterRect) && app.tstate.pts.len() == 1;
    if app.tool == Tool::Circle2T {
      place_tangent_circle(app, sk, raw);
    } else {
      place_point(app, sk, raw, snapped);
    }
    // Enter on typed values finishes the rectangle and its command; a click-drawn one keeps the
    // tool, as in Fusion
    if ends && app.tstate.pts.is_empty() {
      app.set_tool(Tool::Select);
    }
  }
}

/// The padlock's colour: Fusion's is yellow.
const HUD_LOCK: Color32 = crate::theme::WARNING;

/// A small padlock: the glyph Fusion puts beside a heads-up value once it's typed.
fn paint_lock(painter: &egui::Painter, c: Pos2, col: Color32) {
  let body = Rect::from_center_size(c + Vec2::new(0.0, 2.0), Vec2::new(8.0, 6.0));
  painter.rect_filled(body, 1.0, col);
  // the shackle: an arc over the body
  let n = 8;
  let pts: Vec<Pos2> = (0..=n)
    .map(|i| {
      let a = std::f32::consts::PI * (1.0 - i as f32 / n as f32);
      Pos2::new(c.x + 2.6 * a.cos(), c.y - 1.0 - 3.0 * a.sin())
    })
    .collect();
  painter.add(egui::Shape::line(pts, Stroke::new(1.3, col)));
}

/// A line's heads-up angle, as Fusion draws it: an arc from the horizontal to the line about its
/// start, its box just past the arc's end on the line, and (returned apart, to draw dashed) the
/// horizontal reference the arc starts from.
fn hud_angle_shape(
  from: [f64; 2],
  to: [f64; 2],
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<(DimShape, [Pos2; 2])> {
  let ang = (to[1] - from[1]).atan2(to[0] - from[0]);
  let r = 42.0 * wpp;
  let arc: Vec<Pos2> = (0..=24)
    .filter_map(|k| {
      let t = ang * k as f64 / 24.0;
      to_screen([from[0] + r * t.cos(), from[1] + r * t.sin()])
    })
    .collect();
  let o = to_screen(from)?;
  let reference = [o, to_screen([from[0] + r * 1.6, from[1]])?];
  // the box sits beyond the arc's end, inside the angle so it stays clear of the line
  let t = ang - ang.signum() * (16.0 * wpp / r).min(0.5);
  let d = r + 24.0 * wpp;
  let label = to_screen([from[0] + d * t.cos(), from[1] + d * t.sin()])?;
  let mut arrows = Vec::new();
  if arc.len() > 2 && ang.abs() > 0.15 {
    let k = arc.len();
    arrows.push((arc[0], arc[0] - arc[1]));
    arrows.push((arc[k - 1], arc[k - 1] - arc[k - 2]));
  }
  Some((DimShape { lines: Vec::new(), arc, arrows, label }, reference))
}

/// A line's heads-up length, as Fusion draws it: a linear dimension above the segment's middle,
/// between extension lines with arrows.
fn hud_length_shape(
  from: [f64; 2],
  to: [f64; 2],
  off: f64,
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
  let l = dx.hypot(dy).max(1e-12);
  let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
  // a vertical segment's dimension goes on its left, the way `along` puts every other one
  let below = if dx.abs() < 1e-12 { [mid[0] + 1.0, mid[1]] } else { [mid[0], mid[1] - 1.0] };
  linear_shape(from, to, [dx / l, dy / l], below, off, wpp, to_screen)
}

/// A diameter (or any length) drawn as a line from `a` to `b` with arrowheads at both ends and
/// the value on the line at its middle, as Fusion draws a circle's diameter.
fn chord_shape(a: [f64; 2], b: [f64; 2], to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>) -> Option<DimShape> {
  let (p, q) = (to_screen(a)?, to_screen(b)?);
  let d = q - p;
  if !d.is_finite() || d.length() < 1e-6 {
    return None;
  }
  Some(DimShape { lines: vec![[p, q]], arc: Vec::new(), arrows: vec![(p, -d), (q, d)], label: p + d / 2.0 })
}

/// An arc's heads-up angle: an arc glyph inside the arc, from its start through its sweep, with
/// the value beyond its middle.
fn hud_sweep_shape(
  c: [f64; 2],
  r: f64,
  a0: f64,
  sweep: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let rr = r * 0.6;
  let along = arc_points(c, rr, a0, sweep);
  let arc: Vec<Pos2> = along.iter().step_by(2).filter_map(|p| to_screen(*p)).collect();
  let at = |ang: f64, d: f64| to_screen([c[0] + d * ang.cos(), c[1] + d * ang.sin()]);
  let o = to_screen(c)?;
  let mid = at(a0 + sweep / 2.0, rr)?;
  let w = (mid - o).normalized();
  let label = if w.is_finite() { mid + w * 20.0 } else { mid + Vec2::new(0.0, 20.0) };
  let mut arrows = Vec::new();
  if arc.len() > 2 && sweep.abs() > 0.15 {
    let k = arc.len();
    arrows.push((arc[0], arc[0] - arc[1]));
    arrows.push((arc[k - 1], arc[k - 1] - arc[k - 2]));
  }
  Some(DimShape { lines: vec![[o, at(a0, rr)?], [o, at(a0 + sweep, rr)?]], arc, arrows, label })
}

/// The 2-tangent circle the next click would place.
fn tangent_circle_preview(app: &App, painter: &egui::Painter, view: &View, pos: Pos2, plane: Plane) {
  let Some(sk) = app.s.edit_sketch else { return };
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let at = plane.to_local(hit);
  let Some((center, r)) = tangent_circle_geometry(app, sk, at) else { return };
  let stroke = Stroke::new(1.5, Color32::from_rgb(30, 110, 235));
  let pts: Vec<Pos2> = (0..=64)
    .filter_map(|i| {
      let t = std::f64::consts::TAU * i as f64 / 64.0;
      view.to_screen(&app.cam, plane.to_world([center[0] + r * t.cos(), center[1] + r * t.sin()]))
    })
    .collect();
  for w in pts.windows(2) {
    painter.line_segment([w[0], w[1]], stroke);
  }
}

/// The round check-mark button that floats under the spline being drawn, as in Fusion: clicking
/// it commits the spline (so does Enter; Escape discards it). Returns true while the pointer is
/// on the button, so the click doesn't also place a point.
fn spline_accept(app: &mut App, ui: &mut egui::Ui, painter: &egui::Painter, view: &View) -> bool {
  if !matches!(app.tool, Tool::Spline | Tool::ControlSpline) || app.tstate.pts.len() < 2 {
    return false;
  }
  let Some((_, plane)) = sketch_plane(app) else { return false };
  let curve = flatten_beziers(&spline_beziers(app, app.tool, &app.tstate.pts));
  let on_screen: Vec<Pos2> = curve.iter().filter_map(|p| view.to_screen(&app.cam, plane.to_world(*p))).collect();
  let Some(bounds) =
    on_screen.iter().fold(None::<Rect>, |r, p| Some(r.map_or(Rect::from_pos(*p), |r| r.union(Rect::from_pos(*p)))))
  else {
    return false;
  };
  let at = Pos2::new(bounds.center().x, bounds.max.y + 26.0);
  let r = Rect::from_center_size(at, Vec2::splat(24.0));
  let resp = ui.interact(r, ui.id().with("spline-accept"), Sense::click());
  let (hovered, clicked) = (resp.hovered(), resp.clicked());
  let (fill, edge) = if hovered {
    (crate::theme::HOVER, crate::theme::ACCENT)
  } else {
    (Color32::from_rgba_unmultiplied(255, 255, 255, 235), Color32::from_gray(150))
  };
  painter.circle(at, 11.0, fill, Stroke::new(1.0, edge));
  let tick = Stroke::new(2.0, Color32::from_rgb(40, 150, 80));
  painter.line_segment([at + Vec2::new(-5.0, 0.5), at + Vec2::new(-1.5, 4.0)], tick);
  painter.line_segment([at + Vec2::new(-1.5, 4.0), at + Vec2::new(5.5, -4.0)], tick);
  resp.on_hover_text("OK (Enter)");
  if clicked {
    finish_spline(app);
  }
  hovered || clicked
}

/// The conic's rho arrow, once its vertex is placed: an arrow on the curve's middle pointing at
/// the vertex; dragging it along that line sets rho (where the middle lies between the chord
/// and the vertex), a click on it commits the conic. Returns true while the arrow has the
/// pointer, so the click doesn't also place a point.
fn conic_rho_manip(app: &mut App, resp: &egui::Response, painter: &egui::Painter, view: &View) -> bool {
  let (Tool::Conic, [a, b, apex]) = (app.tool, app.tstate.pts.as_slice()) else { return false };
  let (a, b, apex) = (*a, *b, *apex);
  let Some((sk, plane)) = sketch_plane(app) else { return false };
  let rho = conic_rho(app);
  let m = fox_engine::sketch::conic_at(&[a, apex, b], fox_engine::sketch::conic_weight(rho), 0.5);
  let chord = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let (Some(tail), Some(toward)) = (to_screen(m), to_screen(apex)) else { return false };
  let u = (toward - tail).normalized();
  if !u.is_finite() {
    return false;
  }
  let tip = tail + u * 28.0;
  let dragging = matches!(app.drag, Some(Drag::ConicRho));
  let near = resp.hover_pos().is_some_and(|p| {
    let d = tip - tail;
    let t = ((p - tail).dot(d) / d.length_sq()).clamp(0.0, 1.0);
    (tail + d * t - p).length() <= 8.0
  });
  let ink = if near || dragging { crate::theme::ACCENT } else { Color32::from_rgb(30, 110, 235) };
  painter.line_segment([tail, tip], Stroke::new(2.0, ink));
  let n = u.rot90() * 5.0;
  painter.add(egui::Shape::convex_polygon(vec![tip + u * 6.0, tip + n, tip - n], ink, Stroke::NONE));
  if near && resp.drag_started_by(egui::PointerButton::Primary) {
    app.drag = Some(Drag::ConicRho);
  }
  if dragging && let Some(pos) = resp.interact_pointer_pos() {
    // rho: how far along the line from the chord's middle to the vertex the cursor is
    let (o, d) = view.ray(&app.cam, pos);
    if let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) {
      let cur = plane.to_local(hit);
      let (vx, vy) = (apex[0] - chord[0], apex[1] - chord[1]);
      let l2 = vx * vx + vy * vy;
      if l2 > 1e-12 {
        let t = ((cur[0] - chord[0]) * vx + (cur[1] - chord[1]) * vy) / l2;
        let lo = *fox_engine::sketch::RHO_RANGE.start();
        let hi = *fox_engine::sketch::RHO_RANGE.end();
        app.tstate.rho = Some((t * 100.0).round() / 100.0).map(|r| r.clamp(lo, hi));
        app.tstate.hud.typed[0].clear();
        app.tstate.hud.buf = None;
      }
    }
    return true;
  }
  if near && resp.clicked() {
    place_point(app, sk, apex, None);
    return true;
  }
  near
}

/// Fusion's pivot marker: a ringed dot at the point the camera orbits about.
fn pivot_marker(painter: &egui::Painter, at: Pos2) {
  painter.circle(at, 9.0, Color32::from_white_alpha(140), Stroke::new(1.5, Color32::from_gray(60)));
  painter.circle_filled(at, 2.5, crate::theme::ACCENT);
  for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
    painter.line_segment([at + d * 5.0, at + d * 12.0], Stroke::new(1.5, Color32::from_gray(60)));
  }
}

/// Fusion's cursor prompt for a sketch tool at its current step (`state`: what it has
/// collected so far), or None for a tool without one.
pub(crate) fn sketch_prompt(tool: Tool, state: &ToolState) -> Option<&'static str> {
  let n = state.pts.len();
  let picked = state.curves.len();
  Some(match tool {
    Tool::Select => return None,
    Tool::Constrain(kind) => crate::constrain::prompt(kind, state.con_picks, state.con_any),
    Tool::Line => {
      if n == 0 {
        "Place first point"
      } else {
        "Specify next point"
      }
    }
    Tool::Spline | Tool::ControlSpline => {
      if n == 0 {
        "Specify first point"
      } else {
        "Specify next point"
      }
    }
    Tool::Rect => {
      if n == 0 {
        "Place first corner"
      } else {
        "Specify size of rectangle"
      }
    }
    Tool::CenterRect => {
      if n == 0 {
        "Place center point"
      } else {
        "Specify size of rectangle"
      }
    }
    Tool::Rect3 => match n {
      0 => "Place first corner",
      1 => "Place second corner",
      _ => "Place third corner",
    },
    Tool::Circle => {
      if n == 0 {
        "Place center point"
      } else {
        "Specify diameter of circle"
      }
    }
    Tool::Circle2P => {
      if n == 0 {
        "Place first point"
      } else {
        "Specify diameter"
      }
    }
    Tool::Circle3P => match n {
      0 => "Place first point on circle",
      1 => "Place second point on circle",
      _ => "Place third point on circle",
    },
    Tool::Circle2T => match picked {
      0 => "Select first tangent of circle",
      1 => "Select second tangent of circle",
      _ => "Specify radius of circle",
    },
    Tool::Circle3T => match picked {
      0 => "Select first tangent of circle",
      1 => "Select second tangent of circle",
      _ => "Select third tangent on circle",
    },
    Tool::Arc3 => match n {
      0 => "Place start point",
      1 => "Place end point",
      _ => "Place point on arc",
    },
    Tool::CenterArc => match n {
      0 => "Place center point",
      1 => "Specify start point",
      _ => "Specify end point",
    },
    Tool::TangentArc => {
      if n == 0 {
        "Specify start point"
      } else {
        "Specify end point"
      }
    }
    Tool::Polygon | Tool::CircPolygon => {
      if n == 0 {
        "Place center point"
      } else {
        "Place point on polygon"
      }
    }
    Tool::EdgePolygon => match n {
      0 => "Place first point",
      1 => "Place second point",
      _ => "Specify the side of the polygon",
    },
    Tool::Slot => match n {
      0 => "Select start center point",
      1 => "Select end center point",
      _ => "Select point on slot",
    },
    Tool::OverallSlot => match n {
      0 => "Select start point",
      1 => "Select end point",
      _ => "Select point on slot",
    },
    Tool::CenterSlot => match n {
      0 => "Select center",
      1 => "Select second point",
      _ => "Select point on slot",
    },
    Tool::ArcSlot3 => match n {
      0 => "Select start center point",
      1 => "Select end center point",
      2 => "Select point on arc",
      _ => "Select point on slot",
    },
    Tool::ArcSlotCenter => match n {
      0 => "Select center point of arc",
      1 => "Select start point of arc",
      2 => "Select end point of arc",
      _ => "Select point on slot",
    },
    Tool::Point => "Draw point",
    Tool::Ellipse => match n {
      0 => "Place center point",
      1 => "Place first axis point",
      _ => "Place point on ellipse",
    },
    Tool::Conic => match n {
      0 => "Place first point on conic curve",
      1 => "Place second point on conic curve",
      2 => "Place vertex point of conic curve",
      _ => "Specify rho value",
    },
    Tool::MidpointLine => {
      if n == 0 {
        "Select midpoint of line"
      } else {
        "Select end of line"
      }
    }
    Tool::Dimension => match &state.dim_place {
      None if state.dim_picks.is_empty() => "Select sketch objects to dimension",
      // an angle only waits for its place; a length or diameter can still take more geometry
      Some(p) if p.kind.is_angle() => "Select location for dimension",
      _ => "Select additional sketch geometry or location for dimension",
    },
    Tool::Trim => "Select curve section to trim",
    Tool::Extend => "Select curve to extend",
    Tool::Break => "Select curve to Break",
    Tool::Blend => {
      if picked == 0 {
        "Select first curve or edge"
      } else {
        "Select second curve or edge"
      }
    }
    Tool::Fillet => {
      if picked == 0 {
        "Select first line or arc"
      } else {
        "Select second line or arc"
      }
    }
    Tool::Chamfer | Tool::ChamferAngle | Tool::Chamfer2 => {
      if picked == 0 {
        "Select a vertex or two lines to chamfer"
      } else {
        "Select a second line"
      }
    }
  })
}

/// Fusion's cursor prompt: the current step of the open command or sketch tool, in a small
/// tooltip just below-right of the pointer. It hides while the pointer is over a dialog or
/// panel, and during a drag.
fn cursor_prompt(app: &App, resp: &egui::Response) {
  let prompt = match &app.dialog {
    Some(d) => d.prompt(app.hover.is_some() || app.tstate.over_preview),
    None if app.s.edit_sketch.is_some() => sketch_prompt(app.tool, &app.tstate).map(String::from),
    None => None,
  };
  if !resp.contains_pointer() || app.drag.is_some() {
    return;
  }
  // a sketch tool that places points draws with a crosshair, as Fusion's do
  if app.dialog.is_none() && app.s.edit_sketch.is_some() && places_points(app.tool) {
    resp.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
  }
  // Preferences → General → Show command prompt
  let Some(prompt) = prompt.filter(|_| app.preferences.values.show_prompt) else { return };
  egui::Tooltip::always_open(
    resp.ctx.clone(),
    resp.layer_id,
    egui::Id::new("cursor-prompt"),
    egui::PopupAnchor::Pointer,
  )
  .gap(14.0)
  .show(|ui| {
    // on one line, however long ("Specify size of rectangle")
    ui.add(egui::Label::new(prompt).extend());
  });
}

/// The sketch tools that place points with the cursor (a crosshair), rather than pick curves
/// or select.
fn places_points(tool: Tool) -> bool {
  !matches!(tool, Tool::Select | Tool::Dimension | Tool::Trim | Tool::Extend | Tool::Constrain(_))
    && !tool.picks_curves()
}

// ---------------------------------------------------------------- sketch plane picking

/// Fusion's orange for the origin planes offered by Create Sketch.
const PLANE_RGB: [u8; 3] = [236, 150, 52];

/// The planes Create Sketch offers in the viewport, as (`sketch.create` plane, plane): the
/// origin planes and the construction planes.
fn sketch_planes(app: &App) -> Vec<(serde_json::Value, Plane)> {
  use fox_engine::model::OriginPlane;
  let frames = app.s.design.frames();
  let mut out = vec![
    (json!("xy"), Plane::XY),
    (json!("xz"), OriginPlane::Xz.plane(frames)),
    (json!("yz"), OriginPlane::Yz.plane(frames)),
  ];
  for (id, pl) in &app.s.model.planes {
    if !app.s.design.feature(*id).is_some_and(|f| f.suppressed) {
      out.push((json!({"feature": id.0}), *pl));
    }
  }
  out
}

/// Half the side of a construction plane's square: its side is twice an origin plane's, as Fusion
/// draws them (the same on screen whatever the zoom or the model's size).
pub(crate) fn plane_half_size(app: &App) -> f64 {
  origin_plane_size(app)
}

/// How far an origin plane's square reaches from the origin on screen, in points: Fusion keeps
/// it about 60 px whatever the zoom.
pub(crate) const ORIGIN_PLANE_PX: f64 = 60.0;
/// How far off each axis an origin plane's square starts, in points.
pub(crate) const ORIGIN_PLANE_GAP_PX: f64 = 4.0;
/// How far the origin axes reach, as a multiple of the origin planes' size: past the squares.
pub(crate) const ORIGIN_AXIS_REACH: f64 = 1.5;

/// How far an origin plane's square reaches from the origin, in world units: sized from the
/// camera so the square stays about 60 px on screen at the origin's depth.
pub(crate) fn origin_plane_size(app: &App) -> f64 {
  let h = app.canvas.height() as f64;
  let height = if h > 1.0 { h } else { 800.0 };
  app.cam.view_height() / height * ORIGIN_PLANE_PX
}

/// The corners of the square a `sketch.create` plane draws at: an origin plane's in the positive
/// quadrant of its axes, sized from the camera (as the Origin folder draws it); a construction
/// plane's centred on it at the size the scene draws it, so picking matches the picture.
fn square_of(app: &App, plane: &serde_json::Value, pl: &Plane) -> [DVec3; 4] {
  match crate::origin::OriginItem::from_sketch_plane(plane).and_then(|i| i.plane()) {
    Some(o) => crate::origin::plane_corners(o, origin_plane_size(app)),
    None => plane_square(pl, pl.origin, plane_half_size(app)),
  }
}

/// Whether a point of a `sketch.create` plane lies in its square ([`square_of`]).
fn in_square_of(app: &App, plane: &serde_json::Value, pl: &Plane, p: DVec3) -> bool {
  match crate::origin::OriginItem::from_sketch_plane(plane).and_then(|i| i.plane()) {
    Some(o) => crate::origin::in_square(o, p, origin_plane_size(app)),
    None => {
      let half = plane_half_size(app);
      pl.to_local(p).iter().all(|c| c.abs() <= half)
    }
  }
}

/// Which of `planes` (index) the pointer at `pos` picks: the square the ray through it hits
/// first, if nothing nearer (a body face) is in the way; with `infinite` (a sketch's or a
/// primitive's plane), away from every square and every body, the origin plane the ray meets
/// first taken as an infinite plane, as Fusion picks them, so some plane is always under the
/// pointer.
fn plane_under(
  app: &App,
  view: &View,
  pos: Pos2,
  planes: &[(serde_json::Value, Plane)],
  infinite: bool,
) -> Option<usize> {
  let (o, d) = view.ray(&app.cam, pos);
  let first = |squares: bool| {
    let mut best: Option<(f64, usize)> = None;
    for (i, (p, pl)) in planes.iter().enumerate() {
      if !squares && p.get("feature").is_some() {
        continue;
      }
      let Some(hit) = ray_plane(o, d, pl.origin, pl.normal()) else { continue };
      let t = (hit - o).dot(d);
      if t > 0.0 && (!squares || in_square_of(app, p, pl, hit)) && best.is_none_or(|(bt, _)| t < bt) {
        best = Some((t, i));
      }
    }
    best
  };
  let face = fox_engine::scene::ray_hits(&app.s, o, d).first().map(|h| h.0);
  match (first(true), face) {
    (Some((t, i)), f) if f.is_none_or(|f| t <= f) => Some(i),
    (_, Some(_)) => None,
    (_, None) => first(false).filter(|_| infinite).map(|(_, i)| i),
  }
}

/// The planes Create Sketch offers this frame.
struct PlanePick {
  planes: Vec<(serde_json::Value, Plane)>,
  /// The plane under the cursor (index).
  hot: Option<usize>,
}

/// While Create Sketch waits for a plane: the origin and construction planes, and which one is
/// under the cursor (that one wins over the face highlight). Otherwise the Origin folder's
/// visible point, axes and planes are drawn and picked by [`crate::origin`], and the one under
/// the cursor is noted here (`App::hover_origin`, winning over the face highlight too).
fn plane_pick(app: &mut App, resp: &egui::Response, view: &View) -> Option<PlanePick> {
  // Create Sketch, Box, Cylinder, Sphere, Torus and Coil before their base is drawn, Mirror,
  // Section Analysis waiting for its cut plane, and Draft's Pull Direction row
  let sketching = app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::PickPlane || d.picks_plane());
  let picking =
    sketching || app.dialog.as_ref().is_some_and(|d| d.wants_plane() || d.wants_cut_plane() || d.wants_pull_plane());
  app.hover_origin = None;
  app.grid_preview = None;
  app.hover_cplane = None;
  if !picking {
    // with Revolve open only the axes it takes light up: the planes stay shown without getting
    // in the way of the profiles on them; an open Offset Plane takes a plane clicked here
    let takes_axis = app.dialog.as_ref().map(|d| d.takes_origin_axis());
    let offset = app.dialog.as_ref().is_some_and(|d| d.takes_plane());
    if app.dialog.as_ref().is_none_or(|d| d.wants_origin() || d.takes_origin_axis() || offset)
      && app.drag.is_none()
      && let Some(pos) = resp.hover_pos()
    {
      let cam = app.cam;
      let project = |p| view.to_screen(&cam, p);
      let ray = |p| view.ray(&cam, p);
      app.hover_origin = crate::origin::under(app, pos, &project, &ray)
        .filter(|i| takes_axis != Some(true) || crate::origin::OriginItem::AXES.contains(i))
        .filter(|i| !offset || i.plane().is_some());
      if app.hover_origin.is_none() && app.sel_filter.construction && takes_axis != Some(true) {
        app.hover_cplane = crate::construction::under(app, pos, &ray);
      }
      if app.hover_origin.is_some() || app.hover_cplane.is_some() {
        app.hover = None;
      }
    }
    return None;
  }
  let mut planes = sketch_planes(app);
  // the Construction selection filter turned off: construction planes can't be picked
  if !app.sel_filter.construction {
    planes.retain(|(p, _)| p.get("feature").is_none());
  }
  let hot = if app.drag.is_none() {
    resp.hover_pos().and_then(|p| plane_under(app, view, p, &planes, sketching))
  } else {
    None
  };
  if let Some(i) = hot {
    app.hover = None;
    // a sketch's (or primitive's) plane: its sketch grid, labels and all, previewed across the
    // canvas
    if sketching {
      app.grid_preview = Some(planes[i].1);
    }
  }
  Some(PlanePick { planes, hot })
}

/// Fusion's blue-grey for the plane Create Sketch has under the pointer.
const PLANE_HOT_RGB: [u8; 3] = [120, 146, 178];

/// Draws Create Sketch's planes as see-through orange squares, as in Fusion, the hovered one
/// blue-grey. Returns the `sketch.create` plane under the cursor.
fn sketch_planes_overlay(
  app: &App,
  pick: Option<PlanePick>,
  painter: &egui::Painter,
  view: &View,
) -> Option<serde_json::Value> {
  let PlanePick { planes, hot } = pick?;
  let [r, g, b] = PLANE_RGB;
  // Create Sketch's small dashed axis triad at the origin (not doubled while the Origin folder
  // shows its own axes)
  if !crate::origin::OriginItem::AXES.iter().any(|a| crate::origin::visible(app, *a))
    && planes.iter().any(|(p, _)| p.get("feature").is_none())
  {
    origin_triad(app, painter, view, origin_plane_size(app) * ORIGIN_AXIS_REACH);
  }
  for (i, (p, pl)) in planes.iter().enumerate() {
    let Some(pts) = square_of(app, p, pl).iter().map(|c| view.to_screen(&app.cam, *c)).collect::<Option<Vec<Pos2>>>()
    else {
      continue;
    };
    let (fill, edge, width) = if hot == Some(i) {
      let [r, g, b] = PLANE_HOT_RGB;
      (Color32::from_rgba_unmultiplied(r, g, b, 110), Color32::from_rgb(r, g, b), 2.0)
    } else {
      (Color32::from_rgba_unmultiplied(r, g, b, 46), Color32::from_rgba_unmultiplied(r, g, b, 170), 1.0)
    };
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(width, edge)));
  }
  planes.into_iter().nth(hot?).map(|(p, _)| p)
}

/// The origin's axis triad: dashed stubs of `reach` along world X (red), Y (green) and Z (blue).
fn origin_triad(app: &App, painter: &egui::Painter, view: &View, reach: f64) {
  let frames = app.s.design.frames();
  let origin = fox_engine::model::OriginPlane::Xy.plane(frames).origin;
  for (dir, col) in [
    (DVec3::X, fox_engine::scene::ORIGIN_X_COLOR),
    (DVec3::Y, fox_engine::scene::ORIGIN_Y_COLOR),
    (DVec3::Z, fox_engine::scene::ORIGIN_Z_COLOR),
  ] {
    let (Some(a), Some(b)) = (view.to_screen(&app.cam, origin), view.to_screen(&app.cam, origin + dir * reach)) else {
      continue;
    };
    let [r, g, b2, _] = col;
    painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.5, Color32::from_rgb(r, g, b2)), 4.0, 3.0));
  }
}

/// The corners of the square drawn for a plane: `half` either side of `centre` along its axes.
pub(crate) fn plane_square(pl: &Plane, centre: DVec3, half: f64) -> [DVec3; 4] {
  [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|[u, v]| centre + pl.x * (u * half) + pl.y * (v * half))
}

/// Offset Plane's new plane as a see-through orange square at the offset, centred over its base
/// (a picked face's centre), as Fusion previews it.
fn offset_plane_overlay(app: &App, painter: &egui::Painter, view: &View) {
  let Some((pl, centre)) = app.dialog.as_ref().and_then(|d| d.offset_plane_preview(&app.s)) else { return };
  let Some(pts) = plane_square(&pl, centre, plane_half_size(app))
    .iter()
    .map(|p| view.to_screen(&app.cam, *p))
    .collect::<Option<Vec<Pos2>>>()
  else {
    return;
  };
  let [r, g, b] = PLANE_RGB;
  let fill = Color32::from_rgba_unmultiplied(r, g, b, 70);
  let edge = Color32::from_rgba_unmultiplied(r, g, b, 230);
  painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.5, edge)));
}

// ---------------------------------------------------------------- window selection, Select Other

/// Selects what a window from `a` to `b` covers. Left to right selects only what lies fully
/// inside; right to left anything the window touches. Priority and filters decide what a covered
/// pick selects (faces, edges, bodies or components; sketch geometry while sketching).
fn finish_box(app: &mut App, view: &View, a: Pos2, b: Pos2, add: bool) {
  let Some(frame) = &app.frame else { return };
  let window = b.x >= a.x;
  let ((x0, y0), (x1, y1)) = (view.to_px(a), view.to_px(b));
  let (w, h) = (frame.width as i64, frame.height as i64);
  let cl = |v: f64, m: i64| (v.round() as i64).clamp(0, m);
  let (x0, x1) = (cl(x0.min(x1), w), cl(x0.max(x1), w));
  let (y0, y1) = (cl(y0.min(y1), h), cl(y0.max(y1), h));
  let mut inside: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
  for y in y0..y1 {
    for x in x0..x1 {
      if let Some(&id) = frame.ids.get((y * w + x) as usize)
        && id != 0
      {
        *inside.entry(id).or_default() += 1;
      }
    }
  }
  crate::select::select_ids(app, &inside, window, add);
}

/// Opens the Select Other list for what lies under `pos`.
fn open_select_other(app: &mut App, view: &View, pos: Pos2) {
  let (o, d) = view.ray(&app.cam, pos);
  let hits = fox_engine::scene::ray_hits(&app.s, o, d);
  app.other = Some(crate::select::OtherMenu::new(&app.s, pos, app.hover, &hits));
}

// ---------------------------------------------------------------- manipulators, measure line

/// The screen direction a drag arrow's head points: along the drawn arrow, or along its axis
/// (`along`, the projection of a unit step along the arrow) while the arrow is too short to tell,
/// so a zero-length arrow still points the way a positive value goes; up when neither tells.
fn arrow_dir(drawn: Vec2, along: Option<Vec2>) -> Vec2 {
  let dir = drawn.normalized();
  if dir.is_finite() && drawn.length() > 1.0 {
    return dir;
  }
  match along.map(|v| v.normalized()) {
    Some(v) if v.is_finite() && v.length() > 0.5 => v,
    _ => Vec2::new(0.0, -1.0),
  }
}

/// The longest Fillet's radius arrow is drawn on screen (points): a large radius keeps the arrow
/// and its box near the edge, as in Fusion, rather than reaching off the canvas.
const RADIUS_ARROW_REACH: f32 = 90.0;

/// The open dialog's drag arrow: Fusion's solid blue arrow at the far end of the value (Fillet's
/// no further than [`RADIUS_ARROW_REACH`]), pointing along its axis, with the floating value box
/// beside it, clear of the arrow and inside the canvas (see [`crate::value_box`]) and, for
/// Extrude, the distance dimension along the axis (see [`crate::manip_arrow`]). Dragging it
/// changes the value by the pointer's travel along the arrow (snapped to a tenth of the grid), or
/// to a model vertex or edge the pointer passes, with Fusion's "Snap To" tooltip. Returns true
/// while the arrow has the pointer.
fn manipulator(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  view: &View,
  ctx: &egui::Context,
) -> bool {
  use crate::manip_arrow::{Arrow, dimension_number, paint_dimension, paint_track, snap_number, snap_point};
  let Some(m) = app.dialog.as_ref().and_then(|d| d.manipulator(&app.s)) else {
    // a dialog without an arrow may show the box at a feature handle instead (see `handles::show`)
    if !app.dialog.as_mut().is_some_and(|d| d.handle_field(0).is_some() && !d.handles(&app.s).is_empty()) {
      app.manip.value_box.hide();
    }
    return false;
  };
  let tip = m.origin + m.dir * m.value;
  let (Some(a), Some(b)) = (view.to_screen(&app.cam, m.origin), view.to_screen(&app.cam, tip)) else {
    app.manip.value_box.hide();
    return false;
  };
  let reach = app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::Fillet).then_some(RADIUS_ARROW_REACH);
  let b = capped(a, b, reach);
  // a zero-length arrow (Sketch Offset's, still on its curve) points along its axis
  let along = view.to_screen(&app.cam, m.origin + m.dir).map(|c| c - a);
  let arrow = Arrow { base: b, dir: arrow_dir(b - a, along) };
  let active = matches!(app.drag, Some(Drag::Manip));
  // Tab gave the value box to another value (Extrude's taper): the arrow waits in grey
  let has_box = app.manip.value_box.at_handle.is_none();
  if active {
    paint_track(painter, b, arrow.dir);
  }
  if app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::Extrude) {
    paint_dimension(painter, a, b, &dimension_number(app.s.design.units, m.value));
  }
  arrow.paint(painter, has_box || active);
  let hot = resp.hover_pos().is_some_and(|p| arrow.hit(p));
  // the value box beside the arrow edits the dialog's field; a dialog without one gets a label
  let head_at = crate::handles::box_beside(a, arrow.tip(), arrow.dir, view.rect);
  // Sketch Offset's box sits by the pointer while the offset is still on its curve, as Fusion
  // puts it, and moves to the arrow's head once the arrow is dragged or a distance typed
  let label_at = if app.dialog.as_ref().is_some_and(|d| d.box_by_pointer(&app.s)) {
    let at = resp.hover_pos().map(|p| p + Vec2::new(18.0, 0.0)).unwrap_or(head_at);
    app.manip.value_box.pin(at)
  } else {
    app.manip.value_box.unpin();
    head_at
  };
  let params = crate::value_box::params_of(&app.s);
  let mut boxed = false;
  let mut tabbed = false;
  if has_box
    && let Some(dlg) = app.dialog.as_mut()
    && let Some(text) = dlg.value_box_field()
  {
    app.manip.value_box.set_dragging(active);
    let out = crate::value_box::show(ctx, &mut app.manip.value_box, label_at, text, &params);
    if out.changed {
      dlg.value_box_edited();
    }
    tabbed = out.tab;
    boxed = true;
  }
  if tabbed {
    crate::handles::tab(app);
  }
  if !boxed && has_box {
    app.manip.value_box.hide();
    painter.text(
      label_at,
      egui::Align2::LEFT_CENTER,
      app.s.design.units.format(m.value),
      egui::FontId::proportional(12.0),
      Color32::from_gray(40),
    );
  }
  // a drag starts only once the pointer has moved: test where the press began
  let press = ctx.input(|i| i.pointer.press_origin());
  let pressed_on_head = press.is_some_and(|p| arrow.hit(p));
  if pressed_on_head && resp.drag_started_by(egui::PointerButton::Primary) {
    app.drag = Some(Drag::Manip);
    // the value moves by the pointer's travel from where it grabbed the arrow (a capped arrow's
    // head is short of its value)
    app.manip.arrow_grab = press.and_then(|p| {
      let (o, d) = view.ray(&app.cam, p);
      Some((m.value, axis_param(m.origin, m.dir, o, d)?))
    });
  }
  if matches!(app.drag, Some(Drag::Manip)) {
    if let Some(p) = resp.interact_pointer_pos() {
      let (o, d) = view.ray(&app.cam, p);
      // a model vertex or edge by the pointer, else the pointer's travel along the arrow's axis
      let snapped = app
        .dialog
        .as_ref()
        .filter(|dlg| dlg.arrow_snaps())
        .and_then(|dlg| snap_point(&app.s, &dlg.preview_bodies(&app.s), p, &|q| view.to_screen(&app.cam, q)))
        .map(|q| (q - m.origin).dot(m.dir));
      let step = app.increments.linear_step(grid_step(app, view) / 10.0);
      let value = match snapped {
        Some(v) => Some((v, 0.0)),
        None => axis_param(m.origin, m.dir, o, d)
          .map(|t| (app.manip.arrow_grab.map_or(t, |(start, grab)| start + t - grab), step)),
      };
      if let Some((v, step)) = value
        && let Some(dlg) = app.dialog.as_mut()
      {
        dlg.set_manipulator(v, step);
      }
      if let Some(v) = snapped {
        crate::manip_arrow::snap_tooltip(
          ctx,
          resp.layer_id,
          format!("Snap To : {}", snap_number(app.s.design.units, v)),
        );
      }
    }
    ctx.request_repaint();
    return true;
  }
  app.manip.arrow_grab = None;
  hot || pressed_on_head && ctx.input(|i| i.pointer.primary_down())
}

/// The screen point of world `p` in the canvas the last frame drew (tests drive handles by it).
#[cfg(test)]
pub(crate) fn screen_of(app: &App, p: DVec3) -> Option<Pos2> {
  let (rect, [w, h]) = (app.canvas, app.canvas_px);
  View { rect, ppp: w as f32 / rect.width(), w, h }.to_screen(&app.cam, p)
}

/// An arrow's head at `b` (from `a`), brought back along the arrow to at most `reach` points
/// from `a`.
fn capped(a: Pos2, b: Pos2, reach: Option<f32>) -> Pos2 {
  match reach {
    Some(r) if (b - a).length() > r => a + (b - a).normalized() * r,
    _ => b,
  }
}

/// Draws a manipulator arrow for a value from `a` to `b` (Fusion's solid blue arrow at `b`,
/// pointing on from `a`; it doesn't light up under the pointer, as Fusion's don't) and returns
/// where its value box or label goes, beside the head.
pub(crate) fn paint_arrow(painter: &egui::Painter, a: Pos2, b: Pos2) -> Pos2 {
  let dir2 = (b - a).normalized();
  let dir2 = if dir2.is_finite() && (b - a).length() > 1.0 { dir2 } else { Vec2::new(0.0, -1.0) };
  let arrow = crate::manip_arrow::Arrow { base: b, dir: dir2 };
  arrow.paint(painter, true);
  arrow.label_at()
}

/// Where along the axis from `origin` along unit `dir` the pointer ray (`o`, `d`) passes
/// closest: the parameter a dragged arrow head is at.
pub(crate) fn axis_param(origin: DVec3, dir: DVec3, o: DVec3, d: DVec3) -> Option<f64> {
  let w0 = origin - o;
  let (aa, bb, cc) = (dir.dot(dir), dir.dot(d), d.dot(d));
  let (dd, ee) = (dir.dot(w0), d.dot(w0));
  let den = aa * cc - bb * bb;
  (den.abs() > 1e-12).then(|| (bb * ee - cc * dd) / den)
}

/// The Measure dialog's result: a line between the closest points with the distance.
fn measure_overlay(app: &App, painter: &egui::Painter, view: &View) {
  for p in crate::analyses::centers(app).into_iter().filter_map(|c| view.to_screen(&app.cam, c)) {
    // centre of mass: Fusion's quartered circle
    let col = Color32::from_rgb(30, 30, 30);
    painter.circle_filled(p, 8.0, Color32::WHITE);
    painter.circle_stroke(p, 8.0, Stroke::new(1.5, col));
    for (a0, a1) in [(0.0f32, 0.5f32), (1.0, 1.5)] {
      let pts: Vec<Pos2> = std::iter::once(p)
        .chain((0..=8).map(|i| {
          let t = std::f32::consts::PI * (a0 + (a1 - a0) * i as f32 / 8.0);
          p + Vec2::new(t.cos(), -t.sin()) * 8.0
        }))
        .collect();
      painter.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    }
  }
  let Some((a, b, label)) = &app.measure else { return };
  if !app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::Inspect) {
    return;
  }
  let (Some(pa), Some(pb)) = (view.to_screen(&app.cam, *a), view.to_screen(&app.cam, *b)) else { return };
  // as Fusion draws it: a thin black arrow between the picks, its length in plain black beside
  // the middle, and small numbered tags at the two picks
  let col = Color32::from_rgb(20, 20, 20);
  let stroke = Stroke::new(1.0, col);
  painter.line_segment([pa, pb], stroke);
  if (pb - pa).length() > 1.0 {
    let d = (pb - pa).normalized();
    let n = Vec2::new(-d.y, d.x);
    for (tip, back) in [(pa, d), (pb, -d)] {
      let head = vec![tip, tip + back * 9.0 + n * 3.5, tip + back * 9.0 - n * 3.5];
      painter.add(egui::Shape::convex_polygon(head, col, Stroke::NONE));
    }
  }
  let mid = pa + (pb - pa) * 0.5;
  painter.text(mid + Vec2::new(6.0, -6.0), egui::Align2::LEFT_BOTTOM, label, egui::FontId::proportional(12.0), col);
  for (p, n) in [(pa, "1"), (pb, "2")] {
    let galley = painter.layout_no_wrap(n.to_string(), egui::FontId::proportional(10.0), col);
    let r = Rect::from_min_size(p + Vec2::new(5.0, -19.0), galley.size() + Vec2::new(6.0, 2.0));
    painter.rect(r, 2.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
    painter.galley(r.min + Vec2::new(3.0, 1.0), galley, col);
  }
}

/// Section Analysis's cut plane while its dialog is open: a large see-through peach square
/// through the middle of the model, as Fusion draws it.
fn section_plane_overlay(app: &App, painter: &egui::Painter, view: &View) {
  let Some(d) = app.dialog.as_ref().filter(|d| d.kind == crate::dialogs::Kind::Section) else { return };
  let Some(params) = d.section_params(&app.s) else { return };
  let Ok(o) = fox_engine::scene::view_options(&app.s, &json!({"section": params})) else { return };
  let Some(pl) = o.section else { return };
  let half = plane_half_size(app) * 1.4;
  let Some(pts) =
    plane_square(&pl, pl.origin, half).iter().map(|c| view.to_screen(&app.cam, *c)).collect::<Option<Vec<Pos2>>>()
  else {
    return;
  };
  painter.add(egui::Shape::convex_polygon(pts, SECTION_PLANE.0, Stroke::new(1.0, SECTION_PLANE.1)));
}

/// Section Analysis's plane: fill and edge.
const SECTION_PLANE: (Color32, Color32) =
  (Color32::from_rgba_premultiplied(64, 46, 34, 64), Color32::from_rgba_premultiplied(200, 130, 90, 200));

/// Object Visibility's Origin Plane Names and Construction Plane Names: each plane's name in
/// small grey text inside a corner of its square, as Fusion labels them.
fn plane_names_overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let origin = crate::origin::plane_names(app).into_iter().map(|(p, n)| (p, n.to_string()));
  for (at, name) in origin.chain(crate::construction::plane_names(app)) {
    if let Some(p) = project(at) {
      painter.text(p, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(11.0), PLANE_NAME_INK);
    }
  }
}

/// The plane names' grey.
const PLANE_NAME_INK: egui::Color32 = egui::Color32::from_gray(95);

#[cfg(test)]
mod tests {
  /// `ui:manip:arrow-length-offscreen`: a long arrow is drawn no further than its reach.
  #[test]
  fn a_capped_arrow_stops_at_its_reach() {
    let a = Pos2::new(10.0, 10.0);
    assert_eq!(super::capped(a, Pos2::new(310.0, 10.0), Some(90.0)), Pos2::new(100.0, 10.0));
    assert_eq!(super::capped(a, Pos2::new(50.0, 10.0), Some(90.0)), Pos2::new(50.0, 10.0));
    assert_eq!(super::capped(a, Pos2::new(310.0, 10.0), None), Pos2::new(310.0, 10.0));
  }

  /// The palette's Slice sections the model at the sketch plane, cutting away the camera's side.
  #[test]
  fn slice_sections_the_model_at_the_sketch_plane_towards_the_camera() {
    use fox_engine::kernel::Plane;
    use fox_engine::kernel::glam::DVec3;
    assert_eq!(super::slice_plane(Plane::XY, DVec3::new(1.0, 2.0, 30.0)).normal(), DVec3::Z);
    let flipped = super::slice_plane(Plane::XY, DVec3::new(1.0, 2.0, -30.0));
    assert_eq!(flipped.normal(), -DVec3::Z);
    assert_eq!(flipped.origin, DVec3::ZERO);
    let mut app = crate::App::with_session(fox_engine::Session::default());
    app.run("box", serde_json::json!({"width": 20, "depth": 10, "height": 5}));
    app.create_sketch(serde_json::json!("xy"));
    assert!(super::scene_options(&mut app).section.is_none(), "Slice starts off");
    app.palette.slice = true;
    let pl = super::scene_options(&mut app).section.expect("Slice sections at the sketch plane");
    assert!((app.cam.eye() - pl.origin).dot(pl.normal()) > 0.0, "the camera's side is cut away");
    app.finish_sketch();
    assert!(super::scene_options(&mut app).section.is_none(), "only while sketching");
  }

  /// Text reads along the sketch's x axis on the XY plane; on a plane whose x axis runs up or
  /// across the screen the text is turned to read left to right.
  #[test]
  fn text_reads_left_to_right_on_screen() {
    use fox_engine::kernel::Plane;
    use fox_engine::kernel::glam::DVec3;
    assert_eq!(super::reading_angle(&Plane::XY, DVec3::X), 0.0);
    // Fusion's YZ frame (x = -Z, y = +Y) seen with Z up and Y to the right
    let yz = Plane { origin: DVec3::ZERO, x: -DVec3::Z, y: DVec3::Y };
    assert_eq!(super::reading_angle(&yz, DVec3::Y), 90.0);
    assert_eq!(super::reading_angle(&yz, -DVec3::Y), 270.0);
    assert_eq!(super::reading_angle(&Plane::XY, -DVec3::X), 180.0);
  }

  use super::*;
  use fox_engine::Session;

  /// Outside a sketch, a visible sketch's curve near the cursor is picked over the face it lies
  /// on and over an edge, as a vertex is, and a sketch point beats a vertex at the same spot; in
  /// a sketch, points beat curves (curves within their own, tighter radius).
  #[test]
  fn sketch_curves_are_picked_over_faces_and_edges() {
    let (body, sk) = (fox_engine::model::BodyId(1), FeatureId(2));
    let face = Pick::Face { body, face: 1 };
    let edge = Pick::Edge { body, edge: 1 };
    let vertex = Pick::Vertex { body, edge: 1, end: false };
    let curve = Pick::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(1) };
    let point = Pick::SketchPoint { sketch: sk, point: fox_engine::sketch::PointId(1) };
    let near = |around: Vec<Pick>| move |want: fn(&Pick) -> bool| around.iter().copied().find(want);
    let none = |_: fn(&Pick) -> bool| None;
    let pick = |sketching, around: Vec<Pick>| preferred_pick(sketching, near(around.clone()), near(around), Some(face));
    assert_eq!(pick(false, vec![face, edge, curve]), Some(curve));
    assert_eq!(pick(false, vec![face, edge]), Some(edge));
    assert_eq!(pick(false, vec![face, edge, curve, vertex]), Some(vertex));
    assert_eq!(pick(false, vec![face, edge, vertex, point]), Some(point));
    assert_eq!(pick(false, vec![face]), Some(face));
    assert_eq!(pick(true, vec![curve, point]), Some(point));
    assert_eq!(pick(true, vec![curve]), Some(curve));
    // in a sketch the curve lookup has its own (tighter) radius
    assert_eq!(preferred_pick(true, near(vec![curve]), none, Some(face)), Some(face));
  }

  /// A dimension's tooltip names its parameter with the full value and unit, as Fusion's does.
  #[test]
  fn dimension_tooltip_names_the_parameter_with_its_full_value() {
    use fox_engine::model::LengthUnit;
    assert_eq!(dim_tooltip("d9", false, 0.4787837888 * 25.4, LengthUnit::In), "d9 = 0.4787837888 in");
    assert_eq!(dim_tooltip("d1", false, 12.7, LengthUnit::In), "d1 = 0.5 in");
    assert_eq!(dim_tooltip("d2", false, 40.0, LengthUnit::Mm), "d2 = 40.0 mm");
    assert_eq!(dim_tooltip("d3", true, std::f64::consts::FRAC_PI_4, LengthUnit::Mm), "d3 = 45.0 deg");
    // a driven dimension names its expression; a typed plain value still reads as its value
    let driven = fox_engine::sketch::Dim::expr("d1 * 2", 2.0);
    assert_eq!(dim_hover_tip("d21", false, &driven, LengthUnit::Mm), "d21 : d1 * 2");
    let plain = fox_engine::sketch::Dim::expr("( 1 / 2 ) * 1 in", 12.7);
    assert_eq!(dim_hover_tip("d4", false, &plain, LengthUnit::In), "d4 = 0.5 in");
  }

  /// The origin planes keep about 60 px on screen whatever the zoom: their size follows
  /// the camera's distance.
  #[test]
  fn origin_planes_are_sized_from_the_camera() {
    let mut app = App::with_session(Session::default());
    app.canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(1456.0, 800.0));
    let px_per_unit = 800.0 / app.cam.view_height();
    let half = origin_plane_size(&app);
    assert!((half * px_per_unit - ORIGIN_PLANE_PX).abs() < 1e-6, "{half}");
    app.cam.distance *= 3.0;
    let far = origin_plane_size(&app);
    assert!((far / half - 3.0).abs() < 1e-9, "{far} vs {half}");
    // construction planes are twice an origin plane's square, as in Fusion
    assert!((plane_half_size(&app) - far).abs() < 1e-9);
  }

  /// The labelled grid positions are the major lines (every fifth) within the grid's extent.
  #[test]
  fn grid_labels_sit_on_the_major_lines() {
    assert_eq!(grid_label_positions(10.0, 100.0), vec![-100.0, -50.0, 0.0, 50.0, 100.0]);
    // a tiny model still gets a 2-step grid: only the origin is a major line
    assert_eq!(grid_label_positions(10.0, 1.0), vec![0.0]);
  }

  /// As Fusion prints them: a bare number on every major line along both axes that is on
  /// screen, no "0" at the origin and no unit symbol, reading along the sketch's Y axis (straight
  /// up when the sketch is faced), the X labels above their axis and the Y labels left of it.
  #[test]
  fn grid_labels_are_bare_numbers_on_both_axes() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let view = View { rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)), ppp: 1.0, w: 800, h: 600 };
    // right after Create Sketch, before the camera's flight, and once it faces the sketch
    for faced in [false, true] {
      if faced {
        app.cam = app.view_now();
        app.cam_move = None;
      }
      let labels = grid_label_layout(&app, &view);
      let texts = |x_axis: bool| -> Vec<String> {
        labels.iter().filter(|l| l.x_axis == x_axis).map(|l| l.text.clone()).collect()
      };
      assert!(!texts(true).is_empty() && !texts(false).is_empty(), "both axes are labelled: {labels:?}");
      assert!(labels.iter().all(|l| l.text != "0" && !l.text.contains("mm")), "{labels:?}");
      if faced {
        // the ±100 lines are off screen at this zoom
        assert_eq!(texts(true), ["-50", "50"]);
        assert_eq!(texts(true), texts(false), "the same values along both axes");
        for l in &labels {
          assert!((l.up - Vec2::new(0.0, -1.0)).length() < 1e-3, "{l:?}");
          assert!((l.right - Vec2::new(1.0, 0.0)).length() < 1e-3, "{l:?}");
        }
        // X labels sit on the X axis (same height as the origin), Y labels on the Y axis
        let o = view.to_screen(&app.cam, DVec3::ZERO).unwrap();
        assert!(labels.iter().filter(|l| l.x_axis).all(|l| (l.at.y - o.y).abs() < 1e-3), "{labels:?}");
        assert!(labels.iter().filter(|l| !l.x_axis).all(|l| (l.at.x - o.x).abs() < 1e-3), "{labels:?}");
      }
    }
  }

  /// Fusion's prompts for Rectangle, and the curve-picking tools count picks, not points.
  #[test]
  fn the_sketch_prompt_follows_the_tools_step() {
    let mut st = ToolState::default();
    assert_eq!(sketch_prompt(Tool::Rect, &st), Some("Place first corner"));
    assert_eq!(sketch_prompt(Tool::Circle2T, &st), Some("Select first tangent of circle"));
    assert_eq!(sketch_prompt(Tool::Select, &st), None);
    st.pts.push([0.0, 0.0]);
    assert_eq!(sketch_prompt(Tool::Rect, &st), Some("Specify size of rectangle"));
    assert_eq!(sketch_prompt(Tool::Circle, &st), Some("Specify diameter of circle"));
    // the tangent circles name each tangent; the 2-tangent one then asks for its radius
    assert_eq!(sketch_prompt(Tool::Circle3T, &st), Some("Select first tangent of circle"));
    st.curves = vec![(1, [0.0, 0.0])];
    assert_eq!(sketch_prompt(Tool::Circle2T, &st), Some("Select second tangent of circle"));
    assert_eq!(sketch_prompt(Tool::Circle3T, &st), Some("Select second tangent of circle"));
    st.curves = vec![(1, [0.0, 0.0]), (2, [1.0, 0.0])];
    assert_eq!(sketch_prompt(Tool::Circle2T, &st), Some("Specify radius of circle"));
    assert_eq!(sketch_prompt(Tool::Circle3T, &st), Some("Select third tangent on circle"));
  }

  /// The per-step prompts Fusion shows for the create tools (observed 2026-10-21), and the
  /// Dimension tool's two steps.
  #[test]
  fn the_create_tools_prompt_as_fusion_does() {
    let steps = |tool: Tool| -> Vec<&'static str> {
      let mut st = ToolState::default();
      let mut out = Vec::new();
      for _ in 0..4 {
        out.push(sketch_prompt(tool, &st).unwrap());
        st.pts.push([0.0, 0.0]);
      }
      out.dedup();
      out
    };
    assert_eq!(steps(Tool::MidpointLine), ["Select midpoint of line", "Select end of line"]);
    let picks = |tool: Tool| {
      let mut st = ToolState::default();
      let first = sketch_prompt(tool, &st).unwrap();
      st.curves.push((1, [0.0, 0.0]));
      [first, sketch_prompt(tool, &st).unwrap()]
    };
    assert_eq!(picks(Tool::Fillet), ["Select first line or arc", "Select second line or arc"]);
    for t in [Tool::Chamfer, Tool::ChamferAngle, Tool::Chamfer2] {
      assert_eq!(picks(t), ["Select a vertex or two lines to chamfer", "Select a second line"]);
    }
    assert_eq!(picks(Tool::Blend), ["Select first curve or edge", "Select second curve or edge"]);
    assert_eq!(steps(Tool::Rect3), ["Place first corner", "Place second corner", "Place third corner"]);
    assert_eq!(steps(Tool::CenterRect), ["Place center point", "Specify size of rectangle"]);
    assert_eq!(
      steps(Tool::Circle3P),
      ["Place first point on circle", "Place second point on circle", "Place third point on circle"]
    );
    assert_eq!(steps(Tool::CenterArc), ["Place center point", "Specify start point", "Specify end point"]);
    assert_eq!(steps(Tool::TangentArc), ["Specify start point", "Specify end point"]);
    assert_eq!(steps(Tool::Polygon), ["Place center point", "Place point on polygon"]);
    assert_eq!(
      steps(Tool::EdgePolygon),
      ["Place first point", "Place second point", "Specify the side of the polygon"]
    );
    assert_eq!(steps(Tool::Ellipse), ["Place center point", "Place first axis point", "Place point on ellipse"]);
    assert_eq!(steps(Tool::Slot), ["Select start center point", "Select end center point", "Select point on slot"]);
    assert_eq!(steps(Tool::OverallSlot), ["Select start point", "Select end point", "Select point on slot"]);
    assert_eq!(steps(Tool::CenterSlot), ["Select center", "Select second point", "Select point on slot"]);
    assert_eq!(
      steps(Tool::ArcSlot3),
      ["Select start center point", "Select end center point", "Select point on arc", "Select point on slot"]
    );
    assert_eq!(
      steps(Tool::ArcSlotCenter),
      ["Select center point of arc", "Select start point of arc", "Select end point of arc", "Select point on slot"]
    );
    assert_eq!(steps(Tool::Spline), ["Specify first point", "Specify next point"]);
    assert_eq!(steps(Tool::ControlSpline), ["Specify first point", "Specify next point"]);
    assert_eq!(
      steps(Tool::Conic),
      [
        "Place first point on conic curve",
        "Place second point on conic curve",
        "Place vertex point of conic curve",
        "Specify rho value"
      ]
    );
    assert_eq!(steps(Tool::Point), ["Draw point"]);
    let mut st = ToolState::default();
    assert_eq!(sketch_prompt(Tool::Dimension, &st), Some("Select sketch objects to dimension"));
    st.dim_picks.push(Pick::SketchPoint { sketch: FeatureId(1), point: PointId(1) });
    assert_eq!(
      sketch_prompt(Tool::Dimension, &st),
      Some("Select additional sketch geometry or location for dimension")
    );
    // the point-placing tools draw with a crosshair; selecting and curve picking don't
    assert!(places_points(Tool::Slot) && places_points(Tool::Conic) && places_points(Tool::Point));
    assert!(!places_points(Tool::Select) && !places_points(Tool::Dimension) && !places_points(Tool::Trim));
  }

  /// Every create tool has Fusion's heads-up boxes at the steps it shows them.
  #[test]
  fn every_tool_has_its_heads_up_boxes() {
    use Field::*;
    let f = |t: Tool, n: usize| hud_fields(t, n).to_vec();
    assert_eq!(f(Tool::MidpointLine, 1), [Length, Angle]);
    assert_eq!(f(Tool::Rect3, 1), [Length]);
    assert_eq!(f(Tool::Rect3, 2), [Width]);
    // the 2-point rectangle focuses the box beside the cursor, its height, first
    assert_eq!(f(Tool::Rect, 1), [Height, Width]);
    assert_eq!(f(Tool::CenterRect, 1), [Width, Height]);
    // the 3-point arc's third step shows no box: the arc follows the pointer
    assert_eq!((f(Tool::Arc3, 1), f(Tool::Arc3, 2)), (vec![], vec![]));
    assert_eq!(f(Tool::Circle2P, 1), [Diameter]);
    assert_eq!((f(Tool::Circle3P, 1), f(Tool::Circle3P, 2)), (vec![], vec![Diameter]));
    assert_eq!((f(Tool::CenterArc, 1), f(Tool::CenterArc, 2)), (vec![Radius], vec![Angle]));
    assert_eq!(f(Tool::TangentArc, 1), [Radius]);
    assert_eq!(f(Tool::Polygon, 1), [Radius, Sides]);
    assert_eq!(f(Tool::EdgePolygon, 1), [Length, Sides]);
    // the 2-tangent circle's box appears once both tangents are picked
    assert_eq!((f(Tool::Circle2T, 1), f(Tool::Circle2T, 2)), (vec![], vec![Radius]));
    assert_eq!((f(Tool::Ellipse, 1), f(Tool::Ellipse, 2)), (vec![Length], vec![Length]));
    for slot in [Tool::Slot, Tool::OverallSlot, Tool::CenterSlot] {
      assert_eq!((f(slot, 1), f(slot, 2)), (vec![Length], vec![Width]), "{slot:?}");
    }
    assert_eq!(
      (f(Tool::ArcSlotCenter, 1), f(Tool::ArcSlotCenter, 2), f(Tool::ArcSlotCenter, 3)),
      (vec![Radius], vec![Angle], vec![Width])
    );
    assert_eq!(
      (f(Tool::ArcSlot3, 1), f(Tool::ArcSlot3, 2), f(Tool::ArcSlot3, 3)),
      (vec![Length], vec![Radius], vec![Width])
    );
    assert_eq!((f(Tool::Conic, 2), f(Tool::Conic, 3)), (vec![], vec![Rho]));
    assert!(f(Tool::Spline, 1).is_empty() && f(Tool::Point, 0).is_empty());
    assert_eq!(hud_display(fox_engine::model::LengthUnit::In, Rho, 0.75), "0.75");
    // the live values: a midpoint line's whole length, a slot's width across its axis, a centre
    // arc slot's sweep
    let (mut app, sk) = sketching(Tool::MidpointLine);
    place_point(&mut app, sk, [0.0, 0.0], None);
    let [len, ang] = hud_live(&app, [3.0, 4.0]);
    assert!((len - 10.0).abs() < 1e-9 && (ang - 53.1301).abs() < 1e-3, "{len} {ang}");
    let (mut app, sk) = sketching(Tool::Slot);
    place_point(&mut app, sk, [0.0, 0.0], None);
    assert_eq!(hud_live(&app, [10.0, 0.0])[0], 10.0);
    place_point(&mut app, sk, [10.0, 0.0], None);
    assert_eq!(hud_live(&app, [5.0, 2.0])[0], 4.0);
    let (mut app, sk) = sketching(Tool::ArcSlotCenter);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    assert!((hud_live(&app, [0.0, 10.0])[0] - 90.0).abs() < 1e-9);
  }

  /// Typed heads-up values hold the shape to them and lock it with dimensions: a midpoint line's
  /// length, a 2-point circle's diameter, a centre arc's radius, and a slot's length typed at
  /// the first step then its width at the second (held until the slot is drawn).
  #[test]
  fn typed_values_of_the_other_tools_hold_and_lock() {
    let (mut app, sk) = sketching(Tool::MidpointLine);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["40", ""]);
    assert_eq!(hud_apply(&app, [3.0, 0.0]), [20.0, 0.0], "half the typed length from the midpoint");
    place_point(&mut app, sk, [3.0, 0.0], None);
    assert_eq!(dims(&app, sk), vec![40.0]);
    let (lo, hi) = app.s.design.sketch(sk).unwrap().bounds().unwrap();
    assert!((lo[0] + 20.0).abs() < 1e-6 && (hi[0] - 20.0).abs() < 1e-6, "{lo:?} {hi:?}");

    let (mut app, sk) = sketching(Tool::Circle2P);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["12", ""]);
    place_point(&mut app, sk, [1.0, 0.0], None);
    assert_eq!(dims(&app, sk), vec![12.0]);
    let s = app.s.design.sketch(sk).unwrap();
    assert!(s.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Diameter { .. })));

    let (mut app, sk) = sketching(Tool::CenterArc);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["5", ""]);
    place_point(&mut app, sk, [3.0, 0.0], None);
    assert_eq!(app.tstate.pts, vec![[0.0, 0.0], [5.0, 0.0]], "the start is held at the typed radius");
    place_point(&mut app, sk, [0.0, 5.0], None);
    assert_eq!(dims(&app, sk), vec![5.0]);
    assert!(
      app.s.design.sketch(sk).unwrap().constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Radius { .. }))
    );

    // a typed rho shapes the conic; so does the arrow
    let (mut app, sk) = sketching(Tool::Conic);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    place_point(&mut app, sk, [5.0, 10.0], None);
    assert_eq!(app.tstate.pts.len(), 3, "the vertex click leaves the conic shaping");
    assert!(app.s.design.sketch(sk).unwrap().curves.is_empty());
    assert_eq!(conic_rho(&app), CONIC_RHO);
    typed(&mut app, ["0.3", ""]);
    assert_eq!(conic_rho(&app), 0.3);
    assert!(accept_tool(&mut app), "Enter commits the conic");
    let s = app.s.design.sketch(sk).unwrap();
    assert!(
      matches!(s.curves[0].kind, CurveKind::Conic { rho, .. } if (rho - 0.3).abs() < 1e-12),
      "{:?}",
      s.curves[0].kind
    );
    assert!(app.tstate.is_empty());
  }

  /// An edge polygon's third click picks the side; a 3-point arc slot's clicks run start, end,
  /// then a point on the arc, as in Fusion.
  #[test]
  fn edge_polygon_side_and_arc_slot_click_order() {
    let (mut app, sk) = sketching(Tool::EdgePolygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    assert!(app.s.design.sketch(sk).unwrap().curves.is_empty(), "two clicks don't draw it yet");
    place_point(&mut app, sk, [5.0, -3.0], None);
    let (lo, _) = app.s.design.sketch(sk).unwrap().bounds().unwrap();
    assert!(lo[1] < -1.0, "the polygon went below the edge, where the third click was: {lo:?}");

    let (mut app, sk) = sketching(Tool::ArcSlot3);
    place_point(&mut app, sk, [-10.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    place_point(&mut app, sk, [0.0, 10.0], None);
    place_point(&mut app, sk, [0.0, 12.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    let arcs: Vec<_> = s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Arc { .. })).collect();
    assert!(!arcs.is_empty());
    // the slot's long arcs are about the origin (the circle through the three clicks), as is its
    // construction arc centreline
    let about_origin = arcs.iter().filter(|c| {
      let centre = s.pos(s.curve_points(c.id)[0]).unwrap();
      centre[0].abs() < 1e-6 && centre[1].abs() < 1e-6
    });
    assert_eq!(about_origin.clone().filter(|c| !c.construction).count(), 2);
    assert_eq!(about_origin.filter(|c| c.construction).count(), 1);
  }

  /// The Dimension tool: a pick starts a preview that follows the cursor, the placing click opens
  /// the box with the full value and its unit, Escape keeps the dimension; a radius previews as
  /// "R0.228"; picking more geometry while placing changes the dimension.
  #[test]
  fn dimension_tool_places_before_it_asks_for_the_value() {
    let (mut app, sk) = sketching(Tool::Dimension);
    app.s.design.units = fox_engine::model::LengthUnit::In;
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [12.161108, 0]]})).unwrap();
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [0, 10]]})).unwrap();
    app
      .run("sketch.arc", json!({"sketch": sk.0, "center": [30, 0], "start": [35.7912, 0], "end": [30, 5.7912]}))
      .unwrap();
    let s = app.s.design.sketch(sk).unwrap().clone();
    let ids: Vec<_> = s.curves.iter().map(|c| c.id).collect();
    let pick = |c| Pick::SketchCurve { sketch: sk, curve: c };
    // the line: picked, previewed, placed
    app.hover = Some(pick(ids[0]));
    dimension_click(&mut app, Pos2::new(100.0, 100.0), false);
    assert!(app.dim_edit.is_none(), "no box yet: the dimension follows the cursor");
    let place = app.tstate.dim_place.as_ref().unwrap();
    assert!(matches!(place.kind, ConstraintKind::Distance { .. }));
    assert_eq!(
      sketch_prompt(Tool::Dimension, &app.tstate),
      Some("Select additional sketch geometry or location for dimension")
    );
    app.hover = None;
    app.tstate.hud.cursor = Some(([6.0, -8.0], None));
    dimension_click(&mut app, Pos2::new(120.0, 140.0), false);
    let (cid, text, _) = app.dim_edit.clone().unwrap();
    assert_eq!((cid, text.as_str()), (0, "0.4787837795 in"), "the full value with its unit");
    assert_eq!(app.tstate.dim_place.as_ref().unwrap().placed, Some([6.0, -8.0]));
    // Escape keeps it at that value
    settle_dim_edit(&mut app);
    assert!(app.dim_edit.is_none() && app.tstate.is_empty());
    assert!((dims(&app, sk)[0] - 12.161108).abs() < 1e-6, "{:?}", dims(&app, sk));
    // an arc previews its radius with an R; its placed shape leads out to the cursor
    app.hover = Some(pick(ids[2]));
    dimension_click(&mut app, Pos2::new(100.0, 100.0), false);
    let place = app.tstate.dim_place.as_ref().unwrap();
    assert!(matches!(place.kind, ConstraintKind::Radius { .. }));
    assert_eq!(format!("R{}", fixed_number(app.s.design.units, place.current.unwrap())), "R0.228");
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    let shape = dim_shape_at(&s, &place.kind, [50.0, 20.0], Vec2::new(40.0, 16.0), 1.0, &to_screen).unwrap();
    assert_eq!(shape.label, Pos2::new(50.0, -20.0), "the value sits at the cursor");
    // picking the other line while placing turns it into an angle between the two lines
    reset_tool_state(&mut app);
    app.hover = Some(pick(ids[0]));
    dimension_click(&mut app, Pos2::new(100.0, 100.0), false);
    app.hover = Some(pick(ids[1]));
    dimension_click(&mut app, Pos2::new(100.0, 100.0), false);
    let place = app.tstate.dim_place.as_ref().unwrap();
    assert!(matches!(place.kind, ConstraintKind::Angle { .. }), "{:?}", place.kind);
    assert!((place.current.unwrap() - 90.0).abs() < 1e-9);
    assert_eq!(dim_full_text(fox_engine::model::LengthUnit::Mm, true, 90.0), "90 deg");
    // an angle only waits for its place
    assert_eq!(sketch_prompt(Tool::Dimension, &app.tstate), Some("Select location for dimension"));
    // a linear dimension placed below the line sits on that side, at the cursor's offset
    let s = app.s.design.sketch(sk).unwrap();
    let d = s.constraints.iter().find(|c| matches!(c.kind, ConstraintKind::Distance { .. })).unwrap();
    let shape = dim_shape_at(s, &d.kind, [6.0, -8.0], Vec2::new(40.0, 16.0), 1.0, &to_screen).unwrap();
    assert!(
      (shape.label.y - 8.0).abs() < 1e-3,
      "the dimension line runs through the cursor's level: {:?}",
      shape.label
    );
  }

  /// A spline previews as the curve itself (not a polyline through the clicks), with its check
  /// mark under it; Escape discards it and Enter commits it (both kinds).
  #[test]
  fn spline_previews_the_curve_and_enter_commits_it() {
    for tool in [Tool::Spline, Tool::ControlSpline] {
      let (mut app, sk) = sketching(tool);
      app.tstate.pts = vec![[0.0, 0.0], [10.0, 10.0], [20.0, 0.0]];
      let bz = spline_beziers(&app, tool, &app.tstate.pts);
      assert!(!bz.is_empty(), "{tool:?}");
      let curve = flatten_beziers(&bz);
      assert!(curve.len() > 16, "{tool:?}: the curve itself, not a polyline through the clicks");
      if tool == Tool::Spline {
        // a fit spline passes through its clicks
        assert!(curve.iter().any(|p| (p[0] - 10.0).abs() < 1e-6 && (p[1] - 10.0).abs() < 1e-6), "{tool:?}");
      }
      assert!(accept_tool(&mut app), "Enter commits");
      assert_eq!(app.s.design.sketch(sk).unwrap().curves.len(), 1);
      assert!(app.tstate.is_empty());
      assert!(!accept_tool(&mut app), "nothing left to commit");
    }
  }

  #[test]
  fn zoom_window_fills_the_view_with_the_box() {
    let view = View { rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0)), ppp: 1.0, w: 400, h: 200 };
    let mut cam = render::Camera { distance: 100.0, ..render::Camera::default() };
    cam.set_view(render::StandardView::Top);
    // a box a quarter of the width, right of centre
    let bx = Rect::from_center_size(Pos2::new(300.0, 100.0), Vec2::new(100.0, 20.0));
    let to = zoom_window(&cam, &view, bx, None).unwrap();
    assert!((to.distance - 25.0).abs() < 1e-9, "{}", to.distance);
    // the box's centre is now the view's centre
    let (o, d) = view.ray(&cam, bx.center());
    let hit = ray_plane(o, d, DVec3::ZERO, DVec3::Z).unwrap();
    let (x, y, _) = to.project(hit, 400, 200).unwrap();
    assert!((x - 200.0).abs() < 1e-6 && (y - 100.0).abs() < 1e-6, "{x} {y}");
    assert!(zoom_window(&cam, &view, Rect::from_center_size(Pos2::new(50.0, 50.0), Vec2::splat(2.0)), None).is_none());
    // onto the model under the box: its point comes to the centre at the box's scale
    let near = DVec3::new(10.0, 0.0, 60.0);
    let to = zoom_window(&cam, &view, bx, Some(near)).unwrap();
    assert!((to.distance - 10.0).abs() < 1e-9, "{}", to.distance);
    let (x, y, _) = to.project(near, 400, 200).unwrap();
    assert!((x - 200.0).abs() < 1e-6 && (y - 100.0).abs() < 1e-6);
  }

  #[test]
  fn the_wheel_zooms_as_fusion_does_unless_reversed() {
    use crate::preferences::Prefs;
    // egui: rolling the wheel back (toward you) is a negative y; a factor below 1 comes closer
    let (back, away) = (-120.0f32, 120.0f32);
    let fusion = zoom_sign(Prefs::default().reverse_zoom);
    assert!(wheel_zoom_factor(back, fusion) < 1.0, "wheel toward you zooms in");
    assert!(wheel_zoom_factor(away, fusion) > 1.0, "wheel away zooms out");
    let reversed = zoom_sign(true);
    assert!(wheel_zoom_factor(back, reversed) > 1.0);
    assert!(wheel_zoom_factor(away, reversed) < 1.0);
    // a saved ReverseZoom=1 chose wheel-toward-you-zooms-in (before the default matched Fusion):
    // it still does; the new key is what the box says now
    let legacy = |v: &str| Prefs::parse(&format!("[Preferences]\nReverseZoom={v}\n")).reverse_zoom;
    assert!(wheel_zoom_factor(back, zoom_sign(legacy("1"))) < 1.0);
    assert!(!legacy("0"), "the old default takes the new one");
    assert!(Prefs::parse("[Preferences]\nZoomReversed=1\n").reverse_zoom);
    assert!(Prefs::parse("[Preferences]\nReverseZoom=1\nZoomReversed=1\n").reverse_zoom, "the new key wins");
    assert!(Prefs::parse("[Preferences]\nZoomReversed=1\nReverseZoom=1\n").reverse_zoom, "whatever the order");
    // saving writes only the new key
    let p = Prefs::parse("[Preferences]\nReverseZoom=1\n");
    let text = p.write_into("[Preferences]\nReverseZoom=1\n");
    assert!(text.contains("ZoomReversed=0") && !text.contains("ReverseZoom="), "{text}");
  }

  #[test]
  fn double_clicking_the_wheel_fits_the_view_gliding() {
    let ctx = egui::Context::default();
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let mut far = app.cam;
    far.distance *= 20.0;
    let input = |t: f64, events: Vec<egui::Event>| egui::RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
      time: Some(t),
      events,
      ..Default::default()
    };
    let mid = Pos2::new(600.0, 400.0);
    let button = |pressed| egui::Event::PointerButton {
      pos: mid,
      button: egui::PointerButton::Middle,
      pressed,
      modifiers: Default::default(),
    };
    let mut t = 0.0;
    let mut frame = |app: &mut App, events| {
      t += 0.05;
      ctx.run_ui(input(t, events), |ui| app.draw(ui)).drop_without_applying_deltas();
    };
    frame(&mut app, vec![egui::Event::PointerMoved(mid)]);
    app.cam_move = None;
    app.cam = far;
    // one middle click does nothing
    frame(&mut app, vec![button(true), button(false)]);
    assert!(app.cam_move.is_none() && app.cam == far);
    frame(&mut app, vec![button(true), button(false)]);
    let (_, to, _) = app.cam_move.expect("the double click glides to the fit");
    app.cam = far;
    app.fit_view_now();
    assert_eq!(to, app.cam, "where F6 goes");
    assert!(to.distance < far.distance / 2.0);
  }

  /// An app editing a new sketch on XY with `tool` active.
  /// PROJECT's Bodies filter in a sketch: the body under the pointer lights as a whole (no face
  /// highlight), the status names it ("Body1") and a click picks the body, not the face.
  #[test]
  fn bodies_priority_in_a_sketch_hovers_and_picks_the_whole_body() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10})).unwrap();
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let body = app.s.model.bodies[0].id;
    let face = app.s.model.bodies[0].faces()[0].id;
    app.hover = Some(Pick::Face { body, face });
    // with no command running the hover reads nothing (ui-status-readouts); PROJECT's picking does
    assert_eq!(crate::panels::selection_text(&app), None);
    app.start(crate::dialogs::Kind::Project);
    app.hover = Some(Pick::Face { body, face });
    app.sel_priority = crate::select::Priority::Face;
    // faces and edges (the default priority): the face lights and reads out
    assert!(crate::panels::selection_text(&app).unwrap().starts_with("1 Face"));
    let o = scene_options(&mut app);
    assert!(o.hover == app.hover && o.peek.is_empty());
    app.sel_priority = crate::select::Priority::Body;
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("Body1"));
    let o = scene_options(&mut app);
    assert!(o.hover.is_none() && o.peek == [body], "{:?} {:?}", o.hover, o.peek);
    select_click(&mut app, true);
    assert_eq!(app.s.selection, [Sel::Body { body }]);
  }

  fn sketching(tool: Tool) -> (App, FeatureId) {
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    app.set_tool(tool);
    (app, sk)
  }

  /// The sketch's dimension values (mm, or degrees for angles), in creation order.
  fn dims(app: &App, sk: FeatureId) -> Vec<f64> {
    let s = app.s.design.sketch(sk).unwrap();
    s.constraints
      .iter()
      .filter_map(|c| c.kind.dim().map(|d| if c.kind.is_angle() { d.value.to_degrees() } else { d.value }))
      .collect()
  }

  fn typed(app: &mut App, values: [&str; 2]) {
    app.tstate.hud.typed = values.map(String::from);
  }

  /// 2-Tangent Circle, as Fusion: after two picks the radius box reads the circle under the
  /// cursor and the status bar reads out the picks with their least distance; the placed circle
  /// has its two tangencies and no dimension, each tangent glyph beside the circle on its line;
  /// a typed radius sizes it.
  #[test]
  fn two_tangent_circle_has_a_radius_box_and_no_dimension() {
    let (mut app, sk) = sketching(Tool::Circle2T);
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [40, 0]]})).unwrap();
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [0, 40]]})).unwrap();
    let ids: Vec<u32> = app.s.design.sketch(sk).unwrap().curves.iter().map(|c| c.id.0).collect();
    app.tstate.curves = vec![(ids[0], [20.0, 0.0]), (ids[1], [0.0, 20.0])];
    assert_eq!(hud_fields(app.tool, hud_step(&app)), [Field::Radius]);
    // the box follows the cursor: its mean distance to the picks
    assert!((hud_live(&app, [6.0, 4.0])[0] - 5.0).abs() < 1e-9, "{:?}", hud_live(&app, [6.0, 4.0]));
    let (c, r) = tangent_circle_geometry(&app, sk, [6.0, 4.0]).unwrap();
    assert!((r - 5.0).abs() < 1e-9 && (c[0] - 5.0).abs() < 1e-6 && (c[1] - 5.0).abs() < 1e-6, "{c:?} {r}");
    app.s.design.units = fox_engine::model::LengthUnit::In;
    assert_eq!(hud_display(app.s.design.units, Field::Radius, 5.0), "0.197 in");
    assert_eq!(
      crate::panels::selection_text(&app).as_deref(),
      Some("2 Sketch Curves | Min Distance : 0.000 in"),
      "the picks read out like a selection"
    );
    app.s.design.units = fox_engine::model::LengthUnit::Mm;
    // a typed radius sizes the circle
    typed(&mut app, ["3", ""]);
    assert!((tangent_circle_geometry(&app, sk, [6.0, 4.0]).unwrap().1 - 3.0).abs() < 1e-9);
    place_tangent_circle(&mut app, sk, [6.0, 4.0]);
    assert!(app.tstate.is_empty() && app.tstate.hud.typed.iter().all(String::is_empty));
    let s = app.s.design.sketch(sk).unwrap();
    let circle = s.curves.iter().find(|c| matches!(c.kind, CurveKind::Circle { .. })).unwrap();
    assert!((s.radius(circle.id).unwrap() - 3.0).abs() < 1e-9);
    assert_eq!(s.constraints.iter().filter(|c| matches!(c.kind, ConstraintKind::Tangent { .. })).count(), 2);
    assert!(s.constraints.iter().all(|c| c.kind.dim().is_none()), "no dimension: {:?}", s.constraints);
    // each tangent glyph sits where the circle touches its line, not at the line's midpoint (the
    // sketch as the screen, y down, with the glyph's above-right offset taken back off)
    let screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    let anchors: Vec<([f32; 2], u32)> = s
      .constraints
      .iter()
      .filter(|c| matches!(c.kind, ConstraintKind::Tangent { .. }))
      .map(|c| crate::glyphs::places(s, &c.kind, None, &screen)[0])
      .map(|p| ([p.at.x - 10.0, -(p.at.y + 10.0)], p.key))
      .collect();
    let near = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-3;
    assert!(near(anchors[0].0, [3.0, 0.0]) && near(anchors[1].0, [0.0, 3.0]), "{anchors:?}");
    assert_ne!(anchors[0].1, anchors[1].1);
    let on_circle = crate::glyphs::places(s, &ConstraintKind::Horizontal { line: circle.id }, None, &screen)[0];
    assert_ne!(anchors[0].1, on_circle.key);
  }

  /// Two lines that don't meet read out their least distance; crossing ones none.
  #[test]
  fn min_distance_between_sketch_curves() {
    let (mut app, sk) = sketching(Tool::Select);
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [10, 0]]})).unwrap();
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[12, 3], [20, 3]]})).unwrap();
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[5, -5], [5, 5]]})).unwrap();
    app.run("sketch.circle", json!({"sketch": sk.0, "center": [0, 20], "radius": 4})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    let c: Vec<_> = s.curves.iter().map(|c| c.id).collect();
    assert!((curve_min_distance(s, c[0], c[1]).unwrap() - 13f64.sqrt()).abs() < 1e-9);
    assert_eq!(curve_min_distance(s, c[0], c[2]), Some(0.0));
    assert!((curve_min_distance(s, c[0], c[3]).unwrap() - 16.0).abs() < 1e-6);
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: c[0] }, Sel::SketchCurve { sketch: sk, curve: c[1] }];
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("2 Sketch Curves | Min Distance : 3.61 mm"));
  }

  /// A placed dimension keeps its value where the click put it: a diameter's text sits there
  /// with a leader to the circle (a diameter line towards it when inside), and the sketch stores
  /// the spot.
  #[test]
  fn placed_dimension_keeps_its_place() {
    let (mut app, sk) = sketching(Tool::Dimension);
    app.run("sketch.circle", json!({"sketch": sk.0, "center": [0, 0], "radius": 5}));
    let s = app.s.design.sketch(sk).unwrap().clone();
    let circle = s.curves[0].id;
    app.hover = Some(Pick::SketchCurve { sketch: sk, curve: circle });
    dimension_click(&mut app, Pos2::new(100.0, 100.0), false);
    app.hover = None;
    app.tstate.hud.cursor = Some(([20.0, 10.0], None));
    dimension_click(&mut app, Pos2::new(120.0, 140.0), false);
    assert_eq!(app.dim_edit.clone().unwrap().1, "10 mm");
    settle_dim_edit(&mut app);
    let s = app.s.design.sketch(sk).unwrap();
    let d = s.constraints.iter().find(|c| matches!(c.kind, ConstraintKind::Diameter { .. })).unwrap();
    assert_eq!(d.label, Some([20.0, 10.0]));
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    let shape = dim_shape(s, &d.kind, d.kind.dim().unwrap(), d.label, Vec2::new(40.0, 16.0), 1.0, &to_screen).unwrap();
    assert!((shape.label.x - 20.0).abs() < 1e-3 && (shape.label.y + 10.0).abs() < 1e-3, "{:?}", shape.label);
    assert_eq!(shape.arrows.len(), 2, "the leader runs through the circle");
    // inside the circle: a diameter line through the centre towards the value
    let inside = dim_shape_at(s, &d.kind, [2.0, 1.0], Vec2::new(40.0, 16.0), 1.0, &to_screen).unwrap();
    assert!((inside.label.x - 2.0).abs() < 1e-3 && (inside.label.y + 1.0).abs() < 1e-3);
    assert_eq!(inside.lines.len(), 1);
    let [a, b] = inside.lines[0];
    assert!((a.distance(b) - 10.0).abs() < 1e-3, "{a:?} {b:?}");
    // without a place the dimension draws as before, through the centre
    let free = dim_shape(s, &d.kind, d.kind.dim().unwrap(), None, Vec2::new(40.0, 16.0), 1.0, &to_screen).unwrap();
    assert_eq!(free.label, Pos2::new(0.0, 0.0));
  }

  #[test]
  fn alignment_snaps_free_axes_to_points_in_line() {
    let pts = [[20.0, 0.0], [0.0, 7.0], [20.3, 50.0]];
    // x within tolerance of the line end (and of a farther point): the nearer one anchors
    let (p, a) = align_to_points([20.4, -10.0], &pts, 0.5, [false, false]);
    assert_eq!(p, [20.3, -10.0]);
    assert_eq!(a, [Some([20.3, 50.0]), None]);
    let (p, a) = align_to_points([19.8, 7.2], &pts, 0.5, [false, false]);
    assert_eq!(p, [20.0, 7.0]);
    assert_eq!(a, [Some([20.0, 0.0]), Some([0.0, 7.0])]);
    // out of tolerance, or an axis the H/V snap holds: untouched
    assert_eq!(align_to_points([21.0, -10.0], &pts, 0.5, [false, false]), ([21.0, -10.0], [None, None]));
    assert_eq!(align_to_points([20.1, 3.0], &pts, 0.5, [true, false]), ([20.1, 3.0], [None, None]));
  }

  #[test]
  fn inference_points_take_ends_midpoints_and_earlier_clicks() {
    let (mut app, sk) = sketching(Tool::Line);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [20.0, 0.0], None);
    let pts = inference_points(&app, sk);
    assert!(pts.contains(&[0.0, 0.0]) && pts.contains(&[10.0, 0.0]));
    // the click the line runs from is the line's own H/V, not an inference
    assert!(!pts.contains(&[20.0, 0.0]), "{pts:?}");
  }

  #[test]
  fn line_end_below_another_end_snaps_under_it() {
    let (mut app, sk) = sketching(Tool::Line);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [20.0, 0.0], None);
    reset_tool_state(&mut app);
    app.cam.set_view(fox_engine::render::StandardView::Top);
    let view = View { rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)), ppp: 1.0, w: 800, h: 600 };
    place_point(&mut app, sk, [5.0, -30.0], None);
    let plane = sketch_plane(&app).unwrap().1;
    // a cursor 2 px right of the point directly below the first line's end
    let below = view.to_screen(&app.cam, plane.to_world([20.0, -10.0])).unwrap() + Vec2::new(2.0, 0.0);
    app.hover = None;
    let (p, snapped, aligned) = sketch_inference(&app, &view, below).unwrap();
    assert_eq!((p[0], snapped), (20.0, None), "{p:?}");
    assert_eq!(aligned[0], Some([20.0, 0.0]));
  }

  #[test]
  fn typed_rectangle_sizes_become_driving_dimensions() {
    let (mut app, sk) = sketching(Tool::Rect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    // the height box, then the width box
    typed(&mut app, ["25 mm", "40"]);
    // the cursor only gives the direction: up and to the left
    place_point(&mut app, sk, [-3.0, 7.0], None);
    assert_eq!(dims(&app, sk), vec![25.0, 40.0]);
    let (lo, hi) = app.s.design.sketch(sk).unwrap().bounds().unwrap();
    assert!((lo[0] + 40.0).abs() < 1e-6 && (hi[1] - 25.0).abs() < 1e-6, "{lo:?} {hi:?}");
    // the rectangle and its dimensions are one undo step
    app.s.undo();
    assert!(app.s.design.sketch(sk).unwrap().curves.is_empty());
  }

  #[test]
  fn typed_sizes_take_the_design_unit() {
    let (mut app, sk) = sketching(Tool::Rect);
    app.s.design.units = fox_engine::model::LengthUnit::In;
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["2", "10 mm"]);
    place_point(&mut app, sk, [3.0, 7.0], None);
    assert_eq!(dims(&app, sk), vec![50.8, 10.0]);
    let s = app.s.design.sketch(sk).unwrap();
    let exprs: Vec<String> = s.constraints.iter().filter_map(|c| c.kind.dim().map(|d| d.expr.clone())).collect();
    assert_eq!(exprs, ["2 in", "10 mm"], "a bare number is pinned to inches; an explicit unit stays");
    // the labels show inches
    let c = s.constraints.iter().find(|c| c.kind.dim().is_some()).unwrap();
    let (_, label) = dim_label(s, &c.kind, c.kind.dim().unwrap(), app.s.design.units).unwrap();
    assert_eq!(label, "2.000", "labels read at the document precision");
  }

  #[test]
  fn a_sized_rectangle_from_the_origin_draws_black() {
    use fox_engine::scene::{Pick, SKETCH_FIXED, SKETCH_FREE, SceneBuilder, ViewOptions};
    let (mut app, sk) = sketching(Tool::Rect);
    let colours = |app: &App| {
      let (scene, picks) = SceneBuilder::default().build(&app.s, &ViewOptions::default());
      let curve = |id| matches!(picks.get(id), Some(Pick::SketchCurve { .. }));
      scene.lines.iter().filter(|l| curve(l.id)).map(|l| l.color).collect::<Vec<_>>()
    };
    // sized but loose: it can still move
    place_point(&mut app, sk, [5.0, 5.0], None);
    typed(&mut app, ["40", "20"]);
    place_point(&mut app, sk, [9.0, 9.0], None);
    assert_eq!(colours(&app), [SKETCH_FREE; 4]);
    // started on the origin, the same rectangle is fully constrained
    place_point(&mut app, sk, [0.0, 0.0], Some(fox_engine::sketch::ORIGIN));
    typed(&mut app, ["40", "20"]);
    place_point(&mut app, sk, [9.0, 9.0], None);
    let c = colours(&app);
    assert_eq!(c[4..], [SKETCH_FIXED; 4], "{c:?}");
  }

  #[test]
  fn typed_centre_rectangle_holds_its_whole_size() {
    let (mut app, sk) = sketching(Tool::CenterRect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["30", ""]);
    place_point(&mut app, sk, [5.0, 4.0], None);
    assert_eq!(dims(&app, sk), vec![30.0]);
    let (lo, hi) = app.s.design.sketch(sk).unwrap().bounds().unwrap();
    assert!((hi[0] - lo[0] - 30.0).abs() < 1e-6 && (hi[1] - lo[1] - 8.0).abs() < 1e-6, "{lo:?} {hi:?}");
  }

  #[test]
  fn typed_line_length_and_angle() {
    let (mut app, sk) = sketching(Tool::Line);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["50", ""]);
    place_point(&mut app, sk, [3.0, 0.0], None);
    // the next segment: 20 long at 90° from the X axis, locked to the first one
    typed(&mut app, ["20", "90"]);
    place_point(&mut app, sk, [51.0, 1.0], None);
    assert_eq!(dims(&app, sk), vec![50.0, 20.0], "a vertical segment is locked by its V constraint, not an angle");
    let s = app.s.design.sketch(sk).unwrap();
    let (_, hi) = s.bounds().unwrap();
    assert!((hi[0] - 50.0).abs() < 1e-6 && (hi[1] - 20.0).abs() < 1e-6, "{hi:?}");
    assert!(s.curves.iter().all(|c| !c.construction), "no reference line yet");
    // a 135° segment gets its angle against a horizontal reference from its start, as in Fusion
    typed(&mut app, ["10", "135"]);
    place_point(&mut app, sk, [40.0, 30.0], None);
    assert_eq!(dims(&app, sk), vec![50.0, 20.0, 10.0, 135.0]);
    let s = app.s.design.sketch(sk).unwrap();
    let reference = s.curves.iter().find(|c| c.construction).expect("the reference line");
    let ends: Vec<[f64; 2]> = s.curve_points(reference.id).iter().map(|p| s.pos(*p).unwrap()).collect();
    assert!((ends[0][0] - 50.0).abs() < 1e-6 && (ends[0][1] - 20.0).abs() < 1e-6, "{ends:?}");
    assert!((ends[1][0] - 60.0).abs() < 1e-6 && (ends[1][1] - 20.0).abs() < 1e-6, "{ends:?}");
  }

  /// Fusion's "Tab 40 Tab 1 Enter": the line at 40 degrees with a "1.00" linear dimension along
  /// it and a "40.0" degree angular dimension against a dashed construction reference line that
  /// stays in the sketch; the Line tool continues from the new end.
  #[test]
  fn typed_line_angle_makes_a_reference_line_and_an_angle_dimension() {
    let (mut app, sk) = sketching(Tool::Line);
    place_point(&mut app, sk, [5.0, 5.0], None);
    typed(&mut app, ["1", "40"]);
    place_point(&mut app, sk, [9.0, 9.0], None);
    assert_eq!(dims(&app, sk), vec![1.0, 40.0]);
    let s = app.s.design.sketch(sk).unwrap();
    let lines: Vec<_> = s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Line { .. })).collect();
    assert_eq!(lines.len(), 2, "the segment and its reference");
    let (line, reference) = (lines[0], lines[1]);
    assert!(!line.construction && reference.construction, "the reference is construction geometry");
    let ends = |c: &fox_engine::sketch::Curve| -> Vec<[f64; 2]> {
      s.curve_points(c.id).iter().map(|p| s.pos(*p).unwrap()).collect()
    };
    let (le, re) = (ends(line), ends(reference));
    let (c, sn) = (40f64.to_radians().cos(), 40f64.to_radians().sin());
    assert!((le[1][0] - 5.0 - c).abs() < 1e-6 && (le[1][1] - 5.0 - sn).abs() < 1e-6, "{le:?}");
    // the reference runs horizontally from the line's start, as long as the line
    assert_eq!(re[0], [5.0, 5.0]);
    assert!((re[1][0] - 6.0).abs() < 1e-6 && (re[1][1] - 5.0).abs() < 1e-6, "{re:?}");
    assert!(
      s.constraints.iter().any(|k| matches!(k.kind, ConstraintKind::Horizontal { line } if line == reference.id))
    );
    let angle = s.constraints.iter().find(|k| k.kind.is_angle()).expect("the angle dimension");
    assert!(matches!(angle.kind, ConstraintKind::Angle { a, b, .. } if a == reference.id && b == line.id));
    // stored as Fusion spells an accepted value, with its unit
    assert_eq!(angle.kind.dim().unwrap().expr, "40 deg");
    let (_, label) = dim_label(s, &angle.kind, angle.kind.dim().unwrap(), app.s.design.units).unwrap();
    assert_eq!(label, "40.0°", "angles read at one decimal, as Fusion's");
    // the Line tool continues from the new end
    assert_eq!(app.tool, Tool::Line);
    assert_eq!(app.tstate.pts.len(), 2);
    assert!(app.tstate.hud.typed.iter().all(String::is_empty));
    // the shape and its three locking commands undo as one step
    app.run("undo", json!({})).unwrap();
    assert!(app.s.design.sketch(sk).unwrap().curves.is_empty());
  }

  /// The Line tool's heads-up graphics as Fusion draws them: the length dimension above the
  /// segment's middle between extension lines, the angle arc from a horizontal reference at the
  /// start with its box past the arc's end, and the "Specify next point" prompt.
  #[test]
  fn line_heads_up_layout_is_fusions() {
    // sketch units are screen points (y up, as on the sketch plane)
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, p[1] as f32));
    for (to, name) in [([100.0, 20.0], "rightward"), ([-100.0, -30.0], "leftward")] {
      let s = hud_length_shape([0.0, 0.0], to, 30.0, 1.0, &to_screen).unwrap();
      let mid = Pos2::new(to[0] as f32 / 2.0, to[1] as f32 / 2.0);
      assert!(s.label.y > mid.y + 20.0, "{name}: the length box sits above the middle, at {:?}", s.label);
      assert_eq!(s.lines.len(), 3, "{name}: two extension lines and the dimension line");
      assert_eq!(s.arrows.len(), 2);
    }
    let (s, reference) = hud_angle_shape([10.0, 10.0], [60.0, 60.0], 1.0, &to_screen).unwrap();
    assert_eq!(reference[0], Pos2::new(10.0, 10.0));
    assert!(reference[1].y == 10.0 && reference[1].x > 50.0, "a horizontal reference from the start: {reference:?}");
    assert!(s.lines.is_empty(), "the reference is drawn dashed, apart from the arc");
    assert_eq!(s.arc.len(), 25);
    assert_eq!(s.arc[0], Pos2::new(52.0, 10.0), "the arc starts on the reference");
    let end = s.arc[24];
    assert!((end.x - end.y).abs() < 1e-3, "and ends on the line: {end:?}");
    let d = s.label - Pos2::new(10.0, 10.0);
    assert!(d.length() > 60.0 && d.x > d.y && d.y > 0.0, "the box sits past the arc's end, inside the angle: {d:?}");
    let mut st = ToolState::default();
    st.pts.push([0.0, 0.0]);
    assert_eq!(sketch_prompt(Tool::Line, &st), Some("Specify next point"));
  }

  #[test]
  fn typed_circle_diameter() {
    let (mut app, sk) = sketching(Tool::Circle);
    place_point(&mut app, sk, [1.0, 1.0], None);
    typed(&mut app, ["12", ""]);
    place_point(&mut app, sk, [2.0, 1.0], None);
    assert_eq!(dims(&app, sk), vec![12.0]);
  }

  #[test]
  fn typed_polygon_radius_and_sides() {
    // Fusion: ".3 Tab 5 Enter" after the centre makes a pentagon with a radius dimension
    let (mut app, sk) = sketching(Tool::Polygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    assert_eq!(hud_fields(app.tool, 1), &[Field::Radius, Field::Sides]);
    assert_eq!(hud_live(&app, [3.0, 4.0]), [5.0, 6.0]);
    typed(&mut app, ["8", "5"]);
    assert_eq!(hud_sides_live(&app), 5);
    // the cursor only gives the direction
    let p = hud_apply(&app, [3.0, 4.0]);
    assert!((p[0] - 4.8).abs() < 1e-9 && (p[1] - 6.4).abs() < 1e-9, "{p:?}");
    place_point(&mut app, sk, [3.0, 4.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Line { .. })).count(), 5);
    assert_eq!(dims(&app, sk), vec![8.0], "centre to vertex");
    assert!(s.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Distance { .. })));
    // circumscribed: the radius reaches the middle of a side
    let (mut app, sk) = sketching(Tool::CircPolygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["8", "5"]);
    place_point(&mut app, sk, [1.0, 0.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Line { .. })).count(), 5);
    assert_eq!(dims(&app, sk), vec![8.0]);
    assert!(s.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::PointLineDistance { .. })));
    // an edge polygon's boxes are the edge's length and the sides
    let (mut app, sk) = sketching(Tool::EdgePolygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    assert_eq!(hud_fields(app.tool, 1), &[Field::Length, Field::Sides]);
    typed(&mut app, ["10", "4"]);
    place_point(&mut app, sk, [1.0, 0.0], None);
    // the third click picks the polygon's side; the typed values are held until then
    place_point(&mut app, sk, [5.0, 3.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Line { .. })).count(), 4);
    assert_eq!(dims(&app, sk), vec![10.0]);
    // too few sides falls back to Fusion's six
    let (mut app, sk) = sketching(Tool::Polygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["", "2"]);
    assert_eq!(hud_sides_live(&app), 6);
    place_point(&mut app, sk, [5.0, 0.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Line { .. })).count(), 6);
    assert!(dims(&app, sk).is_empty());
  }

  #[test]
  fn polygon_preview_follows_the_sides_box() {
    let hexagon = polygon_points([0.0, 0.0], 2.0, 0.0, 6);
    assert_eq!(hexagon.len(), 6);
    assert_eq!(hexagon[0], [2.0, 0.0]);
    assert!(hexagon.iter().all(|p| (p[0].hypot(p[1]) - 2.0).abs() < 1e-9));
    // an edge polygon's centre is on the edge's left
    let square = edge_polygon_points([0.0, 0.0], [10.0, 0.0], 4);
    assert_eq!(square.len(), 4);
    assert!(square.iter().all(|p| p[1] > -1e-9 && p[1] < 10.0 + 1e-9), "{square:?}");
    assert!(square.iter().any(|p| (p[0] - 10.0).abs() < 1e-9 && (p[1] - 10.0).abs() < 1e-9), "{square:?}");
  }

  #[test]
  fn heads_up_boxes_read_as_fusions() {
    use fox_engine::model::LengthUnit;
    // units on lengths at the document's precision, one decimal and "deg" on angles
    assert_eq!(hud_display(LengthUnit::Mm, Field::Length, 14.76), "14.76 mm");
    assert_eq!(hud_display(LengthUnit::In, Field::Length, 0.492 * 25.4), "0.492 in");
    assert_eq!(hud_display(LengthUnit::Mm, Field::Angle, 336.7876), "336.8 deg");
    assert_eq!(hud_display(LengthUnit::Mm, Field::Sides, 6.0), "6");
    // and each reads back as the value it shows
    let (mut app, sk) = sketching(Tool::Line);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["14.76 mm", "336.8 deg"]);
    assert!((hud_value(&app, 0).unwrap() - 14.76).abs() < 1e-9);
    assert!((hud_value(&app, 1).unwrap() - 336.8f64.to_radians()).abs() < 1e-9);
  }

  #[test]
  fn circle_diameter_runs_horizontally_through_the_centre() {
    let (mut app, sk) = sketching(Tool::Select);
    let made = app.run("sketch.circle", json!({"sketch": sk.0, "center": [10, 5], "radius": 6})).unwrap();
    let curve = made["curves"][0].as_u64().unwrap();
    app.run("sketch.dimension", json!({"sketch": sk.0, "type": "diameter", "curves": [curve], "value": "12"})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    let c = s.constraints.iter().find(|c| c.kind.dim().is_some()).unwrap();
    let (_, label) = dim_label(s, &c.kind, c.kind.dim().unwrap(), app.s.design.units).unwrap();
    assert_eq!(label, "⌀12.00");
    // sketch units are screen points
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, p[1] as f32));
    let shape = dim_shape(s, &c.kind, c.kind.dim().unwrap(), None, Vec2::new(30.0, 16.0), 1.0, &to_screen).unwrap();
    assert_eq!(shape.label, Pos2::new(10.0, 5.0), "the value sits on the line at the centre");
    assert_eq!(shape.lines, vec![[Pos2::new(4.0, 5.0), Pos2::new(16.0, 5.0)]]);
    assert_eq!(shape.arrows.len(), 2);
    assert!(shape.arrows.iter().all(|(tip, dir)| (tip.x - 10.0).signum() == dir.x.signum()), "arrows point out");
    // a radius keeps its leader; an arc's diameter does too
    app.run("sketch.dimension", json!({"sketch": sk.0, "type": "radius", "curves": [curve], "value": "6"})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    let r = s.constraints.iter().filter(|c| c.kind.dim().is_some()).nth(1).unwrap();
    let shape = dim_shape(s, &r.kind, r.kind.dim().unwrap(), None, Vec2::new(30.0, 16.0), 1.0, &to_screen).unwrap();
    assert_eq!(shape.arrows.len(), 1);
    // the ⌀ is painted, not a missing glyph: the text takes room for it
    let ctx = egui::Context::default();
    frame(&ctx, vec![], |ctx| {
      let painter = ctx.layer_painter(egui::LayerId::background());
      let font = egui::FontId::proportional(12.0);
      let dia = DimText::new(&painter, "⌀12", font.clone());
      let plain = DimText::new(&painter, "12", font.clone());
      assert!(dia.glyph && !plain.glyph);
      assert!(dia.size().x > plain.size().x + 5.0, "{} {}", dia.size().x, plain.size().x);
      assert_eq!(dia.galley.text(), "12", "the glyph never reaches the font");
      let r = DimText::new(&painter, "R6", font.clone());
      assert!(!r.glyph && r.galley.text() == "R6");
      // a driven diameter keeps its glyph behind the "fx: " mark
      let fx = DimText::new(&painter, "fx: ⌀12", font);
      assert!(fx.glyph && fx.lead.is_some() && fx.galley.text() == "12");
      assert!(fx.size().x > dia.size().x + 5.0);
    });
  }

  #[test]
  fn dimension_editor_shows_the_value_with_its_unit_and_parameter() {
    let (mut app, sk) = sketching(Tool::Select);
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [10, 0]]})).unwrap();
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 5], [8, 5]]})).unwrap();
    let lines: Vec<u32> = app.s.design.sketch(sk).unwrap().curves.iter().map(|c| c.id.0).collect();
    app
      .run("sketch.dimension", json!({"sketch": sk.0, "type": "distance", "curves": [lines[0]], "value": "10"}))
      .unwrap();
    app
      .run("sketch.dimension", json!({"sketch": sk.0, "type": "distance", "curves": [lines[1]], "value": "8"}))
      .unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    let dims: Vec<_> = s.constraints.iter().filter(|c| c.kind.dim().is_some()).cloned().collect();
    let unit = app.s.design.units;
    assert_eq!(dim_edit_text(&dims[0].kind, dims[0].kind.dim().unwrap(), unit), "10 mm");
    // the second dimension is the sketch's second model parameter
    assert_eq!(dim_param_name(&app, dims[0].id.0), Some("d1".into()));
    assert_eq!(dim_param_name(&app, dims[1].id.0), Some("d2".into()));
    assert_eq!(dim_param_name(&app, 999), None);
    // in inches the value reads in inches; an expression with a parameter stays as typed
    let inch = fox_engine::model::LengthUnit::In;
    assert_eq!(dim_edit_text(&dims[0].kind, &fox_engine::sketch::Dim::expr("2 in", 50.8), inch), "2 in");
    assert_eq!(dim_edit_text(&dims[0].kind, &fox_engine::sketch::Dim::expr("w + 2", 12.0), inch), "w + 2");
    let angle = ConstraintKind::Angle {
      a: fox_engine::sketch::CurveId(lines[0]),
      b: fox_engine::sketch::CurveId(lines[1]),
      value: fox_engine::sketch::Dim::expr("45", 45f64.to_radians()),
    };
    assert_eq!(dim_edit_text(&angle, angle.dim().unwrap(), unit), "45 deg");
    // a measured value opens in full, as Fusion's box shows it, and reads at one decimal
    let measured = ConstraintKind::Angle {
      a: fox_engine::sketch::CurveId(lines[0]),
      b: fox_engine::sketch::CurveId(lines[1]),
      value: fox_engine::sketch::Dim::expr("17.220379275 deg", 17.220379275f64.to_radians()),
    };
    assert_eq!(dim_edit_text(&measured, measured.dim().unwrap(), unit), "17.220379275 deg");
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(dim_label(s, &measured, measured.dim().unwrap(), unit).unwrap().1, "17.2°");
    let dia = ConstraintKind::Diameter {
      curve: fox_engine::sketch::CurveId(lines[0]),
      value: fox_engine::sketch::Dim::new(7.150823636),
    };
    assert_eq!(dim_edit_text(&dia, dia.dim().unwrap(), inch), "0.2815284896 in");
  }

  #[test]
  fn untyped_boxes_follow_the_cursor() {
    let (mut app, sk) = sketching(Tool::Rect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    assert_eq!(hud_apply(&app, [7.0, -3.0]), [7.0, -3.0]);
    assert_eq!(hud_live(&app, [7.0, -3.0]), [3.0, 7.0], "the height box first, then the width");
    place_point(&mut app, sk, [7.0, -3.0], None);
    assert!(dims(&app, sk).is_empty());
  }

  fn frame(ctx: &egui::Context, events: Vec<egui::Event>, f: impl FnMut(&egui::Context)) {
    let mut f = f;
    let raw = egui::RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
      events,
      ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| f(ui.ctx()));
    out.textures_delta.clear();
  }

  fn key(k: egui::Key) -> egui::Event {
    egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
  }

  #[test]
  fn dimension_editor_opens_focused_with_its_value_selected() {
    let (mut app, sk) = sketching(Tool::Select);
    app.run("sketch.line", json!({"points": [[0, 0], [58.45, 0]]})).unwrap();
    let curve = app.s.design.sketch(sk).unwrap().curves[0].id;
    let Some(CurveKind::Line { a, b }) = app.s.design.sketch(sk).unwrap().curve(curve).map(|c| c.kind.clone()) else {
      panic!()
    };
    app.tstate.pending_dim = Some(json!({"type": "distance", "points": [a.0, b.0]}));
    app.dim_edit = Some((0, "58.45".into(), Pos2::new(200.0, 200.0)));
    let ctx = egui::Context::default();
    frame(&ctx, vec![], |ctx| dim_editor(&mut app, ctx));
    assert!(ctx.egui_wants_keyboard_input(), "the box has the keyboard");
    // typing replaces the selected value; Enter commits
    frame(&ctx, vec![egui::Event::Text("50".into())], |ctx| dim_editor(&mut app, ctx));
    frame(&ctx, vec![key(egui::Key::Enter)], |ctx| dim_editor(&mut app, ctx));
    assert!(app.dim_edit.is_none());
    assert_eq!(dims(&app, sk), vec![50.0]);
  }

  #[test]
  fn typed_digits_go_to_the_heads_up_box() {
    let (mut app, sk) = sketching(Tool::Rect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    let ctx = egui::Context::default();
    frame(&ctx, vec![egui::Event::Text("4".into()), key(egui::Key::Num4)], |ctx| {
      route_typing(&mut app, ctx);
      assert!(!ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { .. }))), "digit keys consumed");
    });
    assert_eq!(app.tstate.hud.typed[0], "4");
    assert_eq!(app.tstate.hud.buf.as_deref(), Some("4"));
    // letters are shortcuts, not values, while no box is up
    let (mut app, _) = sketching(Tool::Rect);
    frame(&ctx, vec![egui::Event::Text("l".into())], |ctx| route_typing(&mut app, ctx));
    assert!(app.tstate.hud.buf.is_none());
  }

  /// With the first box focused (as it is after the first click) any character starts typing,
  /// letters included: "d1*2" goes straight in, and the key is no shortcut.
  #[test]
  fn a_letter_starts_typing_in_the_focused_heads_up_box() {
    let (mut app, sk) = sketching(Tool::Rect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    let ctx = egui::Context::default();
    frame(&ctx, vec![egui::Event::Text("d".into()), key(egui::Key::D)], |ctx| {
      route_typing(&mut app, ctx);
      assert!(!ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { .. }))), "the key is the box's");
    });
    assert_eq!(app.tstate.hud.typed[0], "d");
    assert_eq!(app.tstate.hud.buf.as_deref(), Some("d"));
    assert_eq!(app.tool, Tool::Rect, "not the Dimension shortcut");
  }

  /// Tab (and the lock) spell a typed value as Fusion writes it back: "1/2" becomes
  /// "( 1 / 2 ) * 1 in", a number takes the unit, an expression is spaced.
  #[test]
  fn tab_writes_the_accepted_spelling_back_into_the_box() {
    use fox_engine::model::LengthUnit::{In, Mm};
    assert_eq!(accepted_text(In, Field::Width, "1/2"), "( 1 / 2 ) * 1 in");
    assert_eq!(accepted_text(Mm, Field::Width, "1/2"), "( 1 / 2 ) * 1 mm");
    assert_eq!(accepted_text(In, Field::Width, " 2 "), "2 in");
    assert_eq!(accepted_text(In, Field::Width, "3 in/2"), "3 in / 2");
    assert_eq!(accepted_text(In, Field::Width, "d1*2"), "d1 * 2");
    assert_eq!(accepted_text(In, Field::Angle, "45"), "45 deg");
    assert_eq!(accepted_text(In, Field::Sides, "6"), "6");
    assert_eq!(accepted_text(In, Field::Width, "2 +"), "2 +", "what doesn't parse is kept");
    let (mut app, sk) = sketching(Tool::Rect);
    app.s.design.units = In;
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["1/2", "2 +"]);
    assert_eq!(hud_accepted(&app, 0).as_deref(), Some("( 1 / 2 ) * 1 in"));
    assert_eq!(hud_accepted(&app, 1), None, "a value Fusion refuses stays as typed");
  }

  /// A heads-up box takes an expression in the model parameters ("d1*2"): the side is driven
  /// by it, its dimension reads "fx: 1.00" and its edit box opens with "d1 * 2".
  #[test]
  fn a_heads_up_expression_can_use_a_model_parameter() {
    let (mut app, sk) = sketching(Tool::Rect);
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 20], [0.5, 20]]})).unwrap();
    let c = r["curves"][0].as_u64().unwrap();
    app.run("sketch.dimension", json!({"sketch": sk.0, "type": "distance", "curves": [c], "value": "0.5"})).unwrap();
    assert_eq!(app.s.model.params.get("d1"), Some(&0.5), "model parameters are in scope");
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["d1*2", ""]);
    assert_eq!(hud_value(&app, 0), Some(1.0));
    place_point(&mut app, sk, [5.0, 5.0], None);
    assert_eq!(dims(&app, sk), vec![0.5, 1.0]);
    let s = app.s.design.sketch(sk).unwrap();
    let c = s.constraints.iter().filter(|c| c.kind.dim().is_some()).nth(1).unwrap();
    let dim = c.kind.dim().unwrap();
    assert_eq!(dim.expr, "d1 * 2");
    let (_, label) = dim_label(s, &c.kind, dim, app.s.design.units).unwrap();
    assert_eq!(label, "fx: 1.00");
    assert_eq!(dim_edit_text(&c.kind, dim, app.s.design.units), "d1 * 2");
    assert_eq!(dim_param_name(&app, c.id.0).as_deref(), Some("d2"));
  }

  /// Enter on an emptied dimension box is refused: the box stays open until a value is typed
  /// (then remembered for the box menu) or Escape closes it unchanged.
  #[test]
  fn enter_on_an_empty_dimension_box_is_refused() {
    let (mut app, sk) = sketching(Tool::Select);
    app.run("sketch.line", json!({"points": [[0, 0], [40, 0]]})).unwrap();
    let curve = app.s.design.sketch(sk).unwrap().curves[0].id;
    let r = app.run("sketch.dimension", json!({"type": "distance", "curves": [curve.0], "value": "40"})).unwrap();
    let cid = r["constraint"].as_u64().unwrap() as u32;
    app.dim_edit = Some((cid, String::new(), Pos2::new(200.0, 200.0)));
    let ctx = egui::Context::default();
    frame(&ctx, vec![], |ctx| dim_editor(&mut app, ctx));
    frame(&ctx, vec![key(egui::Key::Enter)], |ctx| dim_editor(&mut app, ctx));
    assert!(app.dim_edit.is_some(), "the box stays open");
    assert_eq!(dims(&app, sk), vec![40.0]);
    assert!(app.dim_recent.is_empty());
    frame(&ctx, vec![egui::Event::Text("25".into())], |ctx| dim_editor(&mut app, ctx));
    frame(&ctx, vec![key(egui::Key::Enter)], |ctx| dim_editor(&mut app, ctx));
    assert!(app.dim_edit.is_none());
    assert_eq!(dims(&app, sk), vec![25.0]);
    assert_eq!(app.dim_recent, ["25"]);
    // Escape on an emptied box leaves the dimension as it was
    app.dim_edit = Some((cid, String::new(), Pos2::new(200.0, 200.0)));
    frame(&ctx, vec![], |ctx| dim_editor(&mut app, ctx));
    frame(&ctx, vec![key(egui::Key::Escape)], |ctx| dim_editor(&mut app, ctx));
    assert!(app.dim_edit.is_none());
    assert_eq!(dims(&app, sk), vec![25.0]);
    // and the recent values list is newest first, without repeats
    remember_dim_value(&mut app, "0.5 in");
    remember_dim_value(&mut app, "25");
    assert_eq!(app.dim_recent, ["25", "0.5 in"]);
  }

  /// Values typed in the heads-up boxes join the dimension box menu's recent values, in Fusion's
  /// spelling and newest first; a polygon's side count doesn't.
  #[test]
  fn heads_up_values_join_the_recent_values() {
    let (mut app, sk) = sketching(Tool::Rect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["20", "1/2"]);
    place_point(&mut app, sk, [3.0, 3.0], None);
    assert_eq!(app.dim_recent, ["( 1 / 2 ) * 1 mm", "20 mm"]);
    app.set_tool(Tool::Polygon);
    place_point(&mut app, sk, [50.0, 0.0], None);
    typed(&mut app, ["5", "6"]);
    place_point(&mut app, sk, [55.0, 0.0], None);
    assert_eq!(app.dim_recent, ["5 mm", "( 1 / 2 ) * 1 mm", "20 mm"]);
  }

  /// The 3-point arc's third step: the prompt asks for a point on the arc and there is no box.
  #[test]
  fn the_three_point_arc_third_step_has_no_box() {
    let (mut app, sk) = sketching(Tool::Arc3);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    assert_eq!(sketch_prompt(Tool::Arc3, &app.tstate), Some("Place point on arc"));
    assert!(hud_fields(Tool::Arc3, 2).is_empty());
    assert_eq!(hud_apply(&app, [5.0, 3.0]), [5.0, 3.0]);
    place_point(&mut app, sk, [5.0, 3.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert!(s.curves.iter().any(|c| matches!(c.kind, CurveKind::Arc { .. })));
    assert!(dims(&app, sk).is_empty());
  }

  #[test]
  fn linear_dimension_sits_off_its_geometry_away_from_the_shape() {
    // sketch units are screen points, y down
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, p[1] as f32));
    let s = linear_shape([0.0, 0.0], [100.0, 0.0], [1.0, 0.0], [50.0, 50.0], 30.0, 1.0, &to_screen).unwrap();
    assert_eq!(s.label, Pos2::new(50.0, -30.0));
    assert_eq!(s.arrows.len(), 2);
    // two extension lines and the dimension line
    assert_eq!(s.lines.len(), 3);
    assert!(s.lines[..2].iter().all(|l| l[0].x == l[1].x && l[0].y > l[1].y));
  }

  #[test]
  fn line_drag_from_the_last_point_draws_a_tangent_arc() {
    let (mut app, sk) = sketching(Tool::Line);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    // the preview: a quarter circle about (10, 10)
    let arc = tangent_arc_points(&app, [10.0, 0.0], [20.0, 10.0]).unwrap();
    assert!(arc.iter().all(|p| ((p[0] - 10.0).hypot(p[1] - 10.0) - 10.0).abs() < 1e-9));
    assert!((arc[32][0] - (10.0 + 10.0 * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-9, "{:?}", arc[32]);
    line_tangent_arc(&mut app, sk, [20.0, 10.0]);
    let s = app.s.design.sketch(sk).unwrap();
    assert!(s.curves.iter().any(|c| matches!(c.kind, CurveKind::Arc { .. })));
    assert!(s.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Tangent { .. })));
    assert_eq!(app.tstate.pts.last(), Some(&[20.0, 10.0]), "the chain carries on from the arc's end");
    // and the next click draws a line from there again
    place_point(&mut app, sk, [20.0, 30.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.iter().filter(|c| matches!(c.kind, CurveKind::Line { .. })).count(), 2);
  }

  #[test]
  fn trim_shows_the_piece_between_the_nearest_crossings() {
    let (mut app, sk) = sketching(Tool::Trim);
    let id =
      |v: Option<serde_json::Value>| fox_engine::sketch::CurveId(v.unwrap()["curves"][0].as_u64().unwrap() as u32);
    let long = id(app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [30, 0]]})));
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[10, -5], [10, 5]]}));
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[20, -5], [20, 5]]}));
    let s = app.s.design.sketch(sk).unwrap();
    let span = |at: [f64; 2]| -> Vec<[f64; 2]> {
      trim_piece(s, long, at)
        .iter()
        .map(|r| {
          let xs: Vec<f64> = r.iter().map(|p| p[0]).collect();
          [xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max)]
        })
        .collect()
    };
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    let mid = span([15.0, 0.0]);
    assert!(mid.len() == 1 && near(mid[0][0], 10.0) && near(mid[0][1], 20.0), "{mid:?}");
    let end = span([3.0, 0.0]);
    assert!(end.len() == 1 && near(end[0][0], 0.0) && near(end[0][1], 10.0), "{end:?}");
    // the sketch itself is untouched by the preview
    assert_eq!(app.s.design.sketch(sk).unwrap().curves.len(), 3);
    // a curve crossing nothing goes whole
    let lone = id(app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 20], [8, 20]]})));
    let all = trim_piece(app.s.design.sketch(sk).unwrap(), lone, [4.0, 20.0]);
    assert!(all.len() == 1 && all[0].first() == Some(&[0.0, 20.0]) && all[0].last() == Some(&[8.0, 20.0]), "{all:?}");
  }

  #[test]
  fn three_point_arc_slot_takes_start_end_then_a_point_on_the_arc() {
    // Fusion's clicks: start centre, end centre, a point on the arc, then the width
    let (mut app, sk) = sketching(Tool::ArcSlot3);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [20.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 5.0], None);
    assert!(app.s.design.sketch(sk).unwrap().curves.is_empty(), "nothing drawn until the width");
    place_point(&mut app, sk, [10.0, 7.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert!(app.tstate.pts.is_empty());
    // a small arc slot (not a three-quarter ring): the arc's R12.5 centre is below the chord
    let pts: Vec<[f64; 2]> = s.curves.iter().flat_map(|c| fox_engine::scene::curve_polyline(s, &c.kind)).collect();
    let lo = pts.iter().fold([f64::MAX; 2], |m, p| [m[0].min(p[0]), m[1].min(p[1])]);
    let hi = pts.iter().fold([f64::MIN; 2], |m, p| [m[0].max(p[0]), m[1].max(p[1])]);
    assert!(lo[0] > -3.0 && hi[0] < 23.0 && lo[1] > -3.0 && hi[1] < 10.0, "{lo:?} {hi:?}");
    assert_eq!(s.curves.iter().filter(|c| c.construction).count(), 1, "the arc centreline");
    let axis = s.curves.iter().find(|c| c.construction).unwrap();
    let CurveKind::Arc { center, .. } = axis.kind else { panic!("{axis:?}") };
    let c = s.pos(center).unwrap();
    assert!((c[0] - 10.0).abs() < 1e-6 && (c[1] + 7.5).abs() < 1e-6, "{c:?}");
    assert!(app.status.as_ref().is_none_or(|(_, err)| !*err), "{:?}", app.status);
  }

  #[test]
  fn edge_polygon_third_click_picks_the_side() {
    let (mut app, sk) = sketching(Tool::EdgePolygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    typed(&mut app, ["10", "4"]);
    place_point(&mut app, sk, [1.0, 0.0], None);
    // the second click only fixes the edge; the side comes next
    assert!(app.s.design.sketch(sk).unwrap().curves.is_empty());
    assert_eq!(app.tstate.pts.len(), 2);
    place_point(&mut app, sk, [5.0, -1.0], None);
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.len(), 4);
    assert!(s.points.iter().all(|p| p.y <= 1e-9), "below the edge: {:?}", s.points);
    assert_eq!(dims(&app, sk), vec![10.0], "the typed length still sizes the edge");
    assert!(app.tstate.pts.is_empty());
    // the other way, above
    let (mut app, sk) = sketching(Tool::EdgePolygon);
    place_point(&mut app, sk, [0.0, 0.0], None);
    place_point(&mut app, sk, [10.0, 0.0], None);
    place_point(&mut app, sk, [5.0, 3.0], None);
    assert!(app.s.design.sketch(sk).unwrap().points.iter().all(|p| p.y >= -1e-9));
  }

  #[test]
  fn slot_previews_are_the_finished_outline() {
    let extent = |pts: &[[f64; 2]]| {
      let lo = pts.iter().fold([f64::MAX; 2], |m, p| [m[0].min(p[0]), m[1].min(p[1])]);
      let hi = pts.iter().fold([f64::MIN; 2], |m, p| [m[0].max(p[0]), m[1].max(p[1])]);
      (lo, hi)
    };
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    // a straight slot: the caps reach half the width past the centres
    let o = slot_outline([0.0, 0.0], [20.0, 0.0], 10.0);
    let (lo, hi) = extent(&o);
    assert!(near(lo, [-5.0, -5.0]) && near(hi, [25.0, 5.0]), "{lo:?} {hi:?}");
    assert!(slot_outline([0.0, 0.0], [0.0, 0.0], 10.0).is_empty());
    // the arc through start, a point on it and end runs clockwise below the chord's middle point
    let (c, r, a0, sweep) = arc_through([0.0, 0.0], [10.0, 5.0], [20.0, 0.0]).unwrap();
    assert!(near(c, [10.0, -7.5]) && (r - 12.5).abs() < 1e-9, "{c:?} {r}");
    assert!(sweep < 0.0 && sweep.abs() < std::f64::consts::PI, "{sweep}");
    let arc = arc_points(c, r, a0, sweep);
    let mid = arc[arc.len() / 2];
    assert!(near(mid, [10.0, 5.0]), "{mid:?}");
    assert!(arc_through([0.0, 0.0], [10.0, 0.0], [20.0, 0.0]).is_none());
    // its slot outline stays near the arc
    let (lo, hi) = extent(&arc_slot_outline(c, r, a0, sweep, 4.0));
    assert!(lo[0] > -3.0 && hi[0] < 23.0 && lo[1] > -3.0 && hi[1] < 8.0, "{lo:?} {hi:?}");
  }

  #[test]
  fn glyphs_sit_at_junctions_and_polygon_centres() {
    let (mut app, sk) = sketching(Tool::Select);
    app.run("sketch.slot", json!({"sketch": sk.0, "start": [0, 0], "end": [20, 0], "width": 10})).unwrap();
    app.run("sketch.polygon", json!({"sketch": sk.0, "center": [50, 0], "vertex": [60, 0], "sides": 5})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    // the sketch as the screen, y down
    let screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    for c in &s.constraints {
      let places = crate::glyphs::places(s, &c.kind, None, &screen);
      let at: Vec<[f32; 2]> = places.iter().map(|p| [p.at.x, -p.at.y]).collect();
      match &c.kind {
        // a tangent glyph by the slot corner the line and the arc share: 12 px along the line
        // away from the arc and 9 px out of the slot
        ConstraintKind::Tangent { .. } => {
          let [at] = at[..] else { panic!("one tangent glyph: {at:?}") };
          assert!(
            ((at[0] - 12.0).abs() < 1e-4 || (at[0] - 8.0).abs() < 1e-4) && (at[1].abs() - 14.0).abs() < 1e-4,
            "{at:?}"
          );
        }
        // the polygon's glyph above-right of its centre
        ConstraintKind::Polygon { .. } => assert_eq!(at, [[60.0, 10.0]]),
        _ => {}
      }
    }
    assert!(s.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Polygon { .. })));
  }

  /// `ui:sketch:offset-default-follow`: a zero-length arrow (the offset still on its curve)
  /// points the way a positive value goes, not straight up.
  #[test]
  fn a_zero_length_arrow_points_along_its_axis() {
    assert_eq!(arrow_dir(Vec2::new(0.0, 30.0), Some(Vec2::new(0.0, 5.0))), Vec2::new(0.0, 1.0));
    assert_eq!(arrow_dir(Vec2::ZERO, Some(Vec2::new(4.0, 0.0))), Vec2::new(1.0, 0.0));
    assert_eq!(arrow_dir(Vec2::ZERO, Some(Vec2::new(-3.0, 0.0))), Vec2::new(-1.0, 0.0));
    // nothing to go by (the axis points straight at the camera): up
    assert_eq!(arrow_dir(Vec2::ZERO, Some(Vec2::ZERO)), Vec2::new(0.0, -1.0));
    assert_eq!(arrow_dir(Vec2::ZERO, None), Vec2::new(0.0, -1.0));
  }

  /// After a fillet the tangent glyphs sit beside each arc end: on the line, past the arc's end,
  /// outside the shape; the arc's centre is a sketch point (drawn as a small hollow circle).
  #[test]
  fn fillet_tangent_glyphs_sit_beside_the_arc_ends() {
    let (mut app, sk) = sketching(Tool::Select);
    let r = app.run("sketch.rect", json!({"sketch": sk.0, "corner1": [0, 0], "corner2": [40, 20]})).unwrap();
    let lines: Vec<u64> = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    app.run("sketch.fillet", json!({"sketch": sk.0, "curves": [lines[0], lines[1]], "radius": 5})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    // the sketch as the screen, y down
    let screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    let mut seen = 0;
    for c in &s.constraints {
      let ConstraintKind::Tangent { .. } = c.kind else { continue };
      let [place] = crate::glyphs::places(s, &c.kind, None, &screen)[..] else { panic!("one glyph") };
      let at = place.at;
      // 12 px along the line, 9 px out of the shape
      if (at.y - 9.0).abs() < 1e-3 {
        // the bottom line's end at (35, 0): along the line to the left, out of the shape downward
        assert!((at.x - 23.0).abs() < 1e-3, "{at:?}");
      } else {
        // the right line's end at (40, 5): up the line, out of the shape to the right
        assert!((at.x - 49.0).abs() < 1e-3 && (at.y + 17.0).abs() < 1e-3, "{at:?}");
      }
      seen += 1;
    }
    assert_eq!(seen, 2);
    // the arc's centre point is there to draw, not joined to anything: a hollow circle
    let centre = s.points.iter().find(|p| (p.x - 35.0).abs() < 1e-9 && (p.y - 5.0).abs() < 1e-9).expect("centre");
    assert!(!fox_engine::scene::joined_points(s).contains(&centre.id));
  }

  /// `ui:sketch:tangent-line-circle-motion`: a tangent glyph between a line and a circle it
  /// doesn't meet sits at the touch point, or at the line's end nearest it when only the line's
  /// extension touches.
  #[test]
  fn tangent_glyph_sits_where_the_line_touches_the_circle() {
    let (mut app, sk) = sketching(Tool::Select);
    // a horizontal line, and a circle tangent to its extension beyond its right end
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [40, 0]]})).unwrap();
    let line = r["curves"][0].as_u64().unwrap();
    let r = app.run("sketch.circle", json!({"sketch": sk.0, "center": [100, 10], "radius": 10})).unwrap();
    let circle = r["curves"][0].as_u64().unwrap();
    app.run("sketch.constrain", json!({"sketch": sk.0, "type": "tangent", "curves": [line, circle]})).unwrap();
    // the sketch as the screen, y down, with the glyph's above-right offset taken back off
    let screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    let anchor = |s: &Sketch, k: &ConstraintKind| {
      let p = crate::glyphs::places(s, k, None, &screen)[0];
      ([p.at.x - 10.0, -(p.at.y + 10.0)], p.key)
    };
    let s = app.s.design.sketch(sk).unwrap();
    let c = s.constraints.iter().find(|c| matches!(c.kind, ConstraintKind::Tangent { .. })).unwrap();
    let (at, key) = anchor(s, &c.kind);
    assert!((at[0] - 40.0).abs() < 1e-3 && at[1].abs() < 1e-3, "the right end, nearest the touch: {at:?}");
    let line_id = fox_engine::sketch::CurveId(line as u32);
    assert_ne!(key, anchor(s, &ConstraintKind::Horizontal { line: line_id }).1, "its own row");
    // a circle sitting on the segment itself: the glyph is at the touch point
    let r = app.run("sketch.circle", json!({"sketch": sk.0, "center": [20, 10], "radius": 10})).unwrap();
    let on = r["curves"][0].as_u64().unwrap();
    app.run("sketch.constrain", json!({"sketch": sk.0, "type": "tangent", "curves": [on, line]})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    let c = s.constraints.iter().rfind(|c| matches!(c.kind, ConstraintKind::Tangent { .. })).unwrap();
    let (at, _) = anchor(s, &c.kind);
    assert!((at[0] - 20.0).abs() < 1e-3 && at[1].abs() < 1e-3, "{at:?}");
  }

  /// `ui:sketch:trim-prompt`, `ui:sketch:break-preview`: Fusion's prompts for the curve
  /// modify tools.
  #[test]
  fn trim_extend_and_break_prompts_read_as_fusions() {
    let st = ToolState::default();
    assert_eq!(sketch_prompt(Tool::Trim, &st), Some("Select curve section to trim"));
    assert_eq!(sketch_prompt(Tool::Extend, &st), Some("Select curve to extend"));
    assert_eq!(sketch_prompt(Tool::Break, &st), Some("Select curve to Break"));
  }

  /// `ui:sketch:extend-preview`: the preview is the piece the click adds, from the end nearest
  /// the pointer to the curve it reaches, and nothing when there is nothing to extend to.
  #[test]
  fn extend_shows_the_piece_it_would_add() {
    let (mut app, sk) = sketching(Tool::Extend);
    let id =
      |v: Option<serde_json::Value>| fox_engine::sketch::CurveId(v.unwrap()["curves"][0].as_u64().unwrap() as u32);
    let line = id(app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [10, 0]]})));
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[20, -5], [20, 5]]}));
    let s = app.s.design.sketch(sk).unwrap();
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    let piece = extend_piece(s, line, [9.0, 0.0]);
    assert_eq!(piece.len(), 1, "{piece:?}");
    let xs: Vec<f64> = piece[0].iter().map(|p| p[0]).collect();
    let (lo, hi) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
    assert!(near(lo, 10.0) && near(hi, 20.0) && piece[0].iter().all(|p| p[1].abs() < 1e-6), "{piece:?}");
    // the other end reaches nothing
    assert!(extend_piece(s, line, [1.0, 0.0]).is_empty());
    // the sketch itself is untouched by the preview
    let s = app.s.design.sketch(sk).unwrap();
    assert_eq!(s.curves.len(), 2);
    assert_eq!(s.pos(s.curve_points(line)[1]), Some([10.0, 0.0]));
  }

  /// `ui:sketch:break-preview`: the preview marks the points the click splits the curve at: its
  /// nearest crossings on each side of the pointer, or the pointer's point on a lone curve.
  #[test]
  fn break_shows_its_split_points() {
    let (mut app, sk) = sketching(Tool::Break);
    let id =
      |v: Option<serde_json::Value>| fox_engine::sketch::CurveId(v.unwrap()["curves"][0].as_u64().unwrap() as u32);
    let long = id(app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [30, 0]]})));
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[10, -5], [10, 5]]}));
    app.run("sketch.line", json!({"sketch": sk.0, "points": [[20, -5], [20, 5]]}));
    let lone = id(app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 20], [8, 20]]})));
    let s = app.s.design.sketch(sk).unwrap();
    let xs = |at: [f64; 2], c| -> Vec<f64> {
      let mut v: Vec<f64> = break_points(s, c, at).iter().map(|p| (p[0] * 1e6).round() / 1e6).collect();
      v.sort_by(f64::total_cmp);
      v
    };
    assert_eq!(xs([15.0, 0.0], long), vec![10.0, 20.0]);
    assert_eq!(xs([3.0, 0.0], long), vec![10.0]);
    assert_eq!(xs([4.0, 20.0], lone), vec![4.0]);
    assert_eq!(app.s.design.sketch(sk).unwrap().curves.len(), 4, "the preview changes nothing");
  }

  /// `ui:sketch:modify-warning-toast`: a trim that drops a constraint raises Fusion's warning
  /// toast ("1 warning(s)" and the message), which the next command clears.
  #[test]
  fn trim_that_drops_constraints_raises_the_warning_toast() {
    let (mut app, sk) = sketching(Tool::Trim);
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [10, 0]]})).unwrap();
    let line = r["curves"][0].as_u64().unwrap();
    app.run("sketch.constrain", json!({"sketch": sk.0, "type": "horizontal", "curves": [line]}));
    assert!(app.warning_toast.is_none());
    app.run("sketch.trim", json!({"sketch": sk.0, "curve": line, "at": [5.0, 0.0]}));
    let toast = app.warning_toast.clone().expect("the trim dropped the Horizontal constraint");
    assert_eq!(toast.warnings, vec![fox_engine::CONSTRAINTS_REMOVED.to_string()]);
    assert_eq!(crate::panels::warning_title(&toast), "1 warning(s)");
    assert!(app.status.as_ref().is_none_or(|(_, err)| !err), "no error in the status bar: {:?}", app.status);
    // the next command clears it
    app.run("sketch.point", json!({"sketch": sk.0, "at": [3.0, 3.0]}));
    assert!(app.warning_toast.is_none());
  }

  /// `ui:sketch:dimension-placement-kept`: a placed dimension keeps its offset and side, a
  /// dragged value moves it (live in the solved sketch, stored on release), and horizontal and
  /// vertical distances draw at their place too.
  #[test]
  fn dragged_dimension_value_keeps_its_new_place() {
    let (mut app, sk) = sketching(Tool::Select);
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [20, 0]]})).unwrap();
    let line = r["curves"][0].as_u64().unwrap();
    let d = app.run(
      "sketch.dimension",
      json!({"sketch": sk.0, "type": "distance", "curves": [line], "value": "20", "at": [10, 8]}),
    );
    let cid = fox_engine::sketch::ConstraintId(d.unwrap()["constraint"].as_u64().unwrap() as u32);
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, -p[1] as f32));
    let shape_of = |app: &App| {
      let s = &app.s.model.sketches[&sk].sketch;
      let c = s.constraint(cid).unwrap();
      dim_shape(s, &c.kind, c.kind.dim().unwrap(), c.label, Vec2::new(30.0, 16.0), 1.0, &to_screen).unwrap()
    };
    assert!((shape_of(&app).label.y + 8.0).abs() < 1e-3, "above the line, 8 off: {:?}", shape_of(&app).label);
    // dragged below the line: the solved sketch follows at once, the design on release
    drag_dim_label(&mut app.s, sk, cid, [10.0, -12.0]);
    assert!((shape_of(&app).label.y - 12.0).abs() < 1e-3, "{:?}", shape_of(&app).label);
    assert_eq!(app.s.design.sketch(sk).unwrap().constraint(cid).unwrap().label, Some([10.0, 8.0]));
    app.run("sketch.dimension_label", json!({"sketch": sk.0, "constraint": cid.0, "at": [10.0, -12.0]})).unwrap();
    assert_eq!(app.s.design.sketch(sk).unwrap().constraint(cid).unwrap().label, Some([10.0, -12.0]));
    assert!((shape_of(&app).label.y - 12.0).abs() < 1e-3);
    // a horizontal distance between two points draws at its place
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 10], [15, 30]]})).unwrap();
    let h = app.run(
      "sketch.dimension",
      json!({"sketch": sk.0, "type": "horizontal", "curves": [r["curves"][0]], "value": "15", "at": [7, 40]}),
    );
    let hid = fox_engine::sketch::ConstraintId(h.unwrap()["constraint"].as_u64().unwrap() as u32);
    let s = &app.s.model.sketches[&sk].sketch;
    let c = s.constraint(hid).unwrap();
    let shape = dim_shape(s, &c.kind, c.kind.dim().unwrap(), c.label, Vec2::new(30.0, 16.0), 1.0, &to_screen).unwrap();
    assert!((shape.label.y + 40.0).abs() < 1e-3, "{:?}", shape.label);
    // nothing to move without a dimension
    assert!(app.run("sketch.dimension_label", json!({"sketch": sk.0, "constraint": 9999, "at": [0, 0]})).is_none());
  }

  /// `ui:sketch:dim-box-measure-fill`: Measure opened from a dimension box's menu writes what it
  /// measured into the box when it closes; Measure opened any other way leaves the box alone.
  #[test]
  fn measure_from_the_dimension_box_fills_it() {
    let (mut app, _) = sketching(Tool::Dimension);
    app.dim_edit = Some((0, "20 mm".into(), Pos2::new(200.0, 200.0)));
    let measured = || Some((DVec3::ZERO, DVec3::new(3.0, 4.0, 12.0), "13.00 mm".to_string()));
    // the box menu's Measure
    let mut d = crate::dialogs::Dialog::open(&mut app, crate::dialogs::Kind::Inspect);
    app.tstate.measure_fill = true;
    app.measure = measured();
    d.leave(&mut app);
    assert_eq!(app.dim_edit.as_ref().unwrap().1, "13 mm");
    assert!(!app.tstate.measure_fill);
    // Measure from the toolbar doesn't touch it
    app.dim_edit = Some((0, "20 mm".into(), Pos2::new(200.0, 200.0)));
    let mut d = crate::dialogs::Dialog::open(&mut app, crate::dialogs::Kind::Inspect);
    app.measure = measured();
    d.leave(&mut app);
    assert_eq!(app.dim_edit.as_ref().unwrap().1, "20 mm");
  }

  /// `ui:sketch:3d-sketch-input`: with 3D Sketch on, a Line click near a model vertex takes it
  /// where it is in 3D, and the segment to it is a 3D sketch line, drawn blue; with it off the
  /// same click stays on the sketch plane.
  #[test]
  fn three_d_sketch_line_takes_a_model_vertex_off_the_plane() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    app.set_tool(Tool::Line);
    let view = View { rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)), ppp: 1.0, w: 800, h: 600 };
    let (_, plane) = sketch_plane(&app).unwrap();
    // a top corner of the box, 10 above the sketch plane
    let corner = fox_engine::scene::body_vertices(&app.s.model.bodies[0])
      .into_iter()
      .map(|v| v.2)
      .find(|w| (w.z - 10.0).abs() < 1e-9)
      .unwrap();
    let at = view.to_screen(&app.cam, corner).unwrap();
    app.palette.sketch_3d = true;
    // the chain starts on the plane, far from the box
    assert!(!line_3d_click(&mut app, &view, Pos2::new(-500.0, -500.0), sk, &plane, [30.0, 30.0]));
    place_point(&mut app, sk, [30.0, 30.0], None);
    assert!(line_3d_click(&mut app, &view, at, sk, &plane, plane.to_local(corner)));
    let sketch = app.s.design.sketch(sk).unwrap();
    assert_eq!(sketch.curves3d.len(), 1, "a 3D sketch line");
    let c = &sketch.curves3d[0];
    assert!(c.drawn);
    let fox_engine::sketch::Curve3dKind::Line { a, b } = c.kind else { panic!("{:?}", c.kind) };
    assert_eq!(a, [30.0, 30.0, 0.0]);
    assert!((DVec3::from(b) - corner).length() < 1e-9, "{b:?}");
    // and the chain goes on from the vertex: back down to the plane is 3D too
    assert!(line_3d_click(&mut app, &view, Pos2::new(-500.0, -500.0), sk, &plane, [30.0, 0.0]));
    assert_eq!(app.s.design.sketch(sk).unwrap().curves3d.len(), 2);
    // drawn as the sketch's own curves: blue
    let opts = scene_options(&mut app);
    let (sc, _) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
    let blue = sc
      .lines
      .iter()
      .filter(|l| l.color == fox_engine::scene::SKETCH_FREE && l.points.iter().any(|p| (p[2] - 10.0).abs() < 1e-3))
      .count();
    assert!(blue >= 1, "the 3D line draws blue");
    // with 3D Sketch off, nothing leaves the plane
    app.palette.sketch_3d = false;
    viewport_reset(&mut app);
    place_point(&mut app, sk, [50.0, 50.0], None);
    place_point(&mut app, sk, plane.to_local(corner), None);
    assert_eq!(app.s.design.sketch(sk).unwrap().curves3d.len(), 2);
  }

  fn viewport_reset(app: &mut App) {
    reset_tool_state(app);
  }
}
