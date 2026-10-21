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

use crate::icons::{self, Icon};
use crate::preferences::NavAction;
use crate::{App, RenderKey};

/// Screen-space snap radius for sketch points (points, logical pixels).
const SNAP_PX: f32 = 9.0;
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
}

impl Tool {
  /// Tools that click curves (where on the curve matters) rather than place points.
  pub fn picks_curves(self) -> bool {
    matches!(self, Tool::Trim | Tool::Extend | Tool::Break | Tool::Circle2T | Tool::Circle3T | Tool::Blend)
  }
}

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
  /// left what the box touches. A press held without moving opens Select Other instead.
  Box {
    start: Pos2,
    now: Pos2,
    since: f64,
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
}

/// Clicks collected by the active sketch tool (sketch coordinates).
#[derive(Default)]
pub struct ToolState {
  pts: Vec<[f64; 2]>,
  /// Dimension tool: first picked entity.
  dim_first: Option<Pick>,
  /// Design before a sketch-point drag started.
  drag_backup: Option<fox_engine::model::Design>,
  /// Dimension command waiting for its value.
  pending_dim: Option<serde_json::Value>,
  /// Curves picked so far by a curve-picking tool, with where each was clicked.
  curves: Vec<(u32, [f64; 2])>,
  /// Heads-up value boxes of the shape being drawn.
  hud: HeadsUp,
  /// Center Point Arc: the cursor's last angle around the centre and the signed sweep it has
  /// wound from the start, so the arc runs the way the cursor went (negative = clockwise).
  arc_wind: Option<(f64, f64)>,
  /// Line: a press on the end of the last segment is being dragged; it draws a tangent arc.
  line_arc: bool,
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
}

/// The heads-up boxes of `tool` once it has `n` points.
fn hud_fields(tool: Tool, n: usize) -> &'static [Field] {
  match (tool, n) {
    (Tool::Line, 1..) => &[Field::Length, Field::Angle],
    (Tool::Rect | Tool::CenterRect, 1) => &[Field::Width, Field::Height],
    (Tool::Circle, 1) => &[Field::Diameter],
    // Fusion's polygon boxes: the radius (or the edge's length) and the number of sides
    (Tool::Polygon | Tool::CircPolygon, 1) => &[Field::Radius, Field::Sides],
    (Tool::EdgePolygon, 1) => &[Field::Length, Field::Sides],
    _ => &[],
  }
}

impl ToolState {
  pub fn is_empty(&self) -> bool {
    self.pts.is_empty() && self.dim_first.is_none() && self.curves.is_empty()
  }
}

pub fn reset_tool_state(app: &mut App) {
  app.tstate.pts.clear();
  app.tstate.dim_first = None;
  app.tstate.curves.clear();
  app.tstate.hud = HeadsUp::default();
  app.tstate.arc_wind = None;
  app.tstate.line_arc = false;
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
struct View {
  rect: Rect,
  ppp: f32,
  w: u32,
  h: u32,
}

impl View {
  fn to_px(&self, p: Pos2) -> (f64, f64) {
    (((p.x - self.rect.min.x) * self.ppp) as f64, ((p.y - self.rect.min.y) * self.ppp) as f64)
  }

  fn to_screen(&self, cam: &render::Camera, p: DVec3) -> Option<Pos2> {
    let (x, y, z) = cam.project(p, self.w, self.h)?;
    if !(-1.0..=1.0).contains(&z) {
      return None;
    }
    Some(Pos2::new(self.rect.min.x + x as f32 / self.ppp, self.rect.min.y + y as f32 / self.ppp))
  }

  fn ray(&self, cam: &render::Camera, p: Pos2) -> (DVec3, DVec3) {
    let (x, y) = self.to_px(p);
    cam.ray(x, y, self.w, self.h)
  }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  let rect = ui.available_rect_before_wrap();
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
  if app.s.edit_sketch.is_some() {
    if app.show_grid {
      grid_labels(app, &painter, &view);
    }
    consumed |= sketch_overlay(app, ui, &painter, &view);
  }
  let hot_plane = sketch_planes_overlay(app, planes, &resp, &painter, &view);
  offset_plane_overlay(app, &painter, &view);
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
      app.cancel_dialog();
      app.create_sketch(plane);
    }
  }
  let step = app.increments.linear_step(grid_step(app, &view) / 10.0);
  let (project, ray) = (|p| view.to_screen(&cam, p), |p| view.ray(&cam, p));
  consumed |= crate::manipulators::triad(app, &resp, &painter, &ctx, &project, &ray, step);
  consumed |= crate::form_ui::overlay(app, &resp, &painter, &ctx, &project, &ray);
  consumed |= crate::handles::show(app, &resp, &painter, &ctx, &project, &ray, step);
  consumed |= manipulator(app, &resp, &painter, &view, &ctx);
  if app.objects.joints {
    consumed |= crate::select::joint_glyphs(app, &resp, &painter, &project);
  }
  dim_editor(app, &ctx);
  track_arc_wind(app, &resp, &view);
  offset_follow(app, &resp, &view);
  tool_preview(app, &painter, &resp, &view);
  trim_preview(app, &painter, &resp, &view);
  heads_up(app, &painter, &resp, &view, &ctx);
  measure_overlay(app, &painter, &view);
  hud(app, &painter, &resp, &view);
  if let Some(Drag::Box { start, now, .. }) = app.drag {
    let r = Rect::from_two_pos(start, now);
    // left to right: window (blue, solid); right to left: crossing (green)
    let col = if now.x >= start.x { Color32::from_rgb(40, 120, 230) } else { Color32::from_rgb(40, 170, 90) };
    painter.rect(r, 0.0, col.gamma_multiply(0.12), Stroke::new(1.0, col), egui::StrokeKind::Inside);
  }
  if let Some(Drag::ZoomWindow { start, now }) = app.drag {
    let col = Color32::from_rgb(40, 120, 230);
    painter.rect_stroke(Rect::from_two_pos(start, now), 0.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
  }
  crate::select::region_overlay(app, &painter);
  crate::select::show_other(app, &ctx);

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
      Drag::Box { start, now, since } => {
        let held = ctx.input(|i| i.time) - since;
        if (now - start).length() < 4.0 && held > 0.45 {
          open_select_other(app, view, start);
        } else {
          let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command) || app.dialog.is_some();
          finish_box(app, view, start, now, add);
        }
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

/// A swatch from the Appearance panel over the canvas: the face under it lights up, and letting
/// go applies the appearance to that face or its body.
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
      let target = if drag.faces { Sel::Face { body, face } } else { Sel::Body { body } };
      crate::dialogs::apply_appearance(app, &drag.name, drag.color, drag.faces, &[target]);
    }
  }
}

fn update_hover(app: &mut App, resp: &egui::Response, view: &View) {
  if app.drag.is_some() {
    return;
  }
  let Some(pos) = resp.hover_pos() else {
    app.hover = None;
    return;
  };
  let Some(frame) = &app.frame else { return };
  let (x, y) = view.to_px(pos);
  let sketching = app.s.edit_sketch.is_some();
  let r = ((if sketching { SNAP_PX } else { 5.0 }) * view.ppp).round() as i32;
  let picks = &app.picks;
  // preferred kinds win within the search radius
  let pick_kind = |want: fn(&Pick) -> bool| -> Option<Pick> {
    let id = frame.pick(x, y, r, |id| picks.get(id).is_some_and(|p| want(&p)));
    picks.get(id).filter(want)
  };
  let exact = picks.get(frame.pick(x, y, 0, |_| true));
  app.hover = if sketching {
    pick_kind(|p| matches!(p, Pick::SketchPoint { .. }))
      .or_else(|| pick_kind(|p| matches!(p, Pick::SketchCurve { .. })))
      .or(exact)
  } else {
    // vertices (drawn only with their filter on) before edges before faces
    pick_kind(|p| matches!(p, Pick::Vertex { .. })).or_else(|| pick_kind(|p| matches!(p, Pick::Edge { .. }))).or(exact)
  };
  // while a sketch tool draws, only points matter for snapping
  if sketching && app.tool.picks_curves() {
    // trim / extend / break / tangent circles / blend pick curves
    app.hover = pick_kind(|p| matches!(p, Pick::SketchCurve { .. }));
  } else if sketching && app.tool != Tool::Select && app.tool != Tool::Dimension {
    app.hover = app.hover.filter(|p| matches!(p, Pick::SketchPoint { .. }));
  } else if !sketching {
    let (filter, priority) = (app.sel_filter, app.sel_priority);
    let locked = &app.ctx_menu.unselectable;
    let ok = |p: &Pick| filter.allows(p, priority) && !crate::select::pick_body(p).is_some_and(|b| locked.contains(&b));
    if !app.hover.as_ref().is_some_and(ok) {
      // the front-most pick is filtered out: the nearest allowed one around the cursor wins
      app.hover = picks.get(frame.pick(x, y, r, |id| picks.get(id).is_some_and(|p| ok(&p)))).filter(ok);
    }
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
  ViewOptions {
    hover: app.hover,
    hide_edges: !app.visual_style.edges(),
    hide_grid: !app.show_grid,
    // the origin axes draw only while the browser's Origin folder is shown (and Object Visibility
    // has origins on), as short stubs matching the origin planes' size
    hide_origin: !(vis.origins && app.origin_shown),
    origin_axis_len: Some(origin_plane_half_size(app) * ORIGIN_AXIS_REACH),
    hide_sketches: !vis.sketches || analysis.hide_sketches,
    hide_construction: !vis.construction,
    grid_step: app.grid.step(),
    show_profiles_of: profiles_of,
    hide_profiles_of: app.hidden_profiles.clone(),
    show_vertices: app.sel_filter.vertices && app.s.edit_sketch.is_none() && vis.bodies,
    peek: app.ctx_menu.peek.clone(),
    peek_faces: app.ctx_menu.peek_faces.clone(),
    peek_sketches: app.ctx_menu.peek_sketches.clone(),
    only_bodies: if vis.bodies {
      app.ctx_menu.isolate.clone().or(analysis.only_bodies.clone())
    } else {
      Some(Vec::new())
    },
    opacity: app.ctx_menu.opacity.clone(),
    // Offset Plane's preview is filled over the image (see `offset_plane_overlay`)
    hide_planes: app.dialog.as_ref().and_then(|d| d.previewed_plane()).into_iter().collect(),
    ..analysis
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
    show_grid: app.show_grid,
    grid_step: app.grid.step(),
    visible: app.s.design.bodies.iter().map(|b| (b.body.0, b.visible)).collect(),
    profiles_of,
    hidden_profiles: app.hidden_profiles.clone(),
    view: view_params(app),
    vertices: app.sel_filter.vertices && app.s.edit_sketch.is_none(),
    peek: app.ctx_menu.peek.iter().map(|b| b.0).collect(),
    isolate: app.ctx_menu.isolate.as_ref().map(|v| v.iter().map(|b| b.0).collect()),
    opacity: app.ctx_menu.opacity.iter().map(|(b, a)| (b.0, *a)).collect(),
    peek_faces: app.ctx_menu.peek_faces.iter().map(|(b, f)| (b.0, *f)).collect(),
    peek_sketches: app.ctx_menu.peek_sketches.iter().map(|s| s.0).collect(),
  };
  if crate::gpu::active(app) && !crate::gpu::scene_changed(app, &key) {
    return;
  }
  if app.render_key.as_ref() == Some(&key) && app.tex.is_some() {
    return;
  }
  let opts = scene_options(app);
  let (mut scene, picks) = app.scene.build(&app.s, &opts);
  scene.style = app.visual_style;
  app.effects.apply(&mut scene);
  if crate::gpu::active(app) {
    // the Render workspace shades the bodies with their materials
    let look = crate::render_ui::viewport_look(app, &mut scene, &picks);
    crate::gpu::set_look(app, look);
    crate::gpu::set_scene(app, scene, picks, key);
    return;
  }
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

/// The section and analysis settings as render parameters.
fn view_params(app: &App) -> serde_json::Value {
  let mut v = app.analysis.clone().unwrap_or(json!({}));
  if app.manufacture {
    v["toolpaths"] = json!(app.s.toolpaths_of.map(|t| t.1));
  }
  if app.manufacture || app.cut_window.is_some() {
    v["cut"] = json!(app.s.cut_preview.as_ref().map(|c| [c.job as u64, c.revision]));
  }
  if let Some(sim) = crate::sim_ui::view_json(app) {
    v["sim"] = sim;
  }
  if let Some(sec) = &app.section {
    v["section"] = sec.clone();
  }
  if crate::render_ui::shading(app) {
    v["render_look"] = crate::render_ui::look_key(app);
  }
  v
}

// ---------------------------------------------------------------- clicks & tools

fn sketch_plane(app: &App) -> Option<(FeatureId, Plane)> {
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

fn grid_step(app: &App, view: &View) -> f64 {
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
  if resp.drag_started_by(egui::PointerButton::Primary) && app.drag.is_none() && app.tool == Tool::Select {
    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
    let since = ctx.input(|i| i.pointer.press_start_time()).unwrap_or(0.0);
    app.drag = Some(Drag::Box { start, now: pos, since });
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
  app.last_click = Some(pos);
  if ctx.input(|i| i.modifiers.alt) && app.tool == Tool::Select {
    open_select_other(app, view, pos);
    return;
  }
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
  if !sketching || app.tool == Tool::Select {
    // with a command dialog open, picks add to (or toggle out of) its selection, as in Fusion
    select_click(app, add || app.dialog.is_some());
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
    curve_tool_click(app, view, pos, ctx.input(|i| i.modifiers.shift));
    return;
  }
  let Some((p, snapped)) = sketch_cursor(app, view, pos) else { return };
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

/// Adds the active tool's next point; the last one draws the shape. Values typed in the heads-up
/// boxes override the cursor and become driving dimensions.
fn place_point(app: &mut App, sk: FeatureId, p: [f64; 2], snapped: Option<PointId>) {
  let locks = hud_locks(app);
  let (p, snapped) = if locks.is_empty() { (p, snapped) } else { (hud_apply(app, p), None) };
  app.tstate.hud = HeadsUp::default();
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
    Tool::EdgePolygon if n == 2 => {
      Some(("sketch.polygon", json!({"sketch": sid, "edge": [first, p], "sides": hud_sides(&locks)})))
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
      // the width is twice the last click's distance from the arc
      let (a, b, c) = (first, pts[1], pts[2]);
      let (center, r) = if app.tool == Tool::ArcSlotCenter {
        (Some(a), (b[0] - a[0]).hypot(b[1] - a[1]))
      } else {
        let o = fox_engine::sketch::circumcenter(a, b, c);
        (o, o.map(|o| (a[0] - o[0]).hypot(a[1] - o[1])).unwrap_or(0.0))
      };
      let w = center.map(|o| 2.0 * ((p[0] - o[0]).hypot(p[1] - o[1]) - r).abs()).unwrap_or(1.0).max(0.1);
      let q = if app.tool == Tool::ArcSlotCenter {
        json!({"sketch": sid, "arc_center": a, "start": b, "end": c, "width": w})
      } else {
        json!({"sketch": sid, "start": a, "mid": b, "end": c, "width": w})
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
    Tool::Conic if n == 3 => {
      Some(("sketch.conic", json!({"sketch": sid, "start": first, "end": pts[1], "apex": p, "rho": 0.5})))
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
  let fields = hud_fields(app.tool, app.tstate.pts.len());
  fields
    .iter()
    .enumerate()
    .filter(|(i, _)| hud_value(app, *i).is_some())
    .map(|(i, f)| (*f, hud_text(app, *f, &app.tstate.hud.typed[i])))
    .collect()
}

/// Heads-up box text as an expression: a bare length means the design's unit.
fn hud_text(app: &App, field: Field, text: &str) -> String {
  let t = text.trim();
  if matches!(field, Field::Angle | Field::Sides) { t.to_string() } else { app.s.design.units.typed(t) }
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
  let field = *hud_fields(app.tool, app.tstate.pts.len()).get(i)?;
  let text = app.tstate.hud.typed.get(i)?.trim();
  if text.is_empty() {
    return None;
  }
  let kind = match field {
    Field::Angle => fox_engine::expr::Kind::Angle,
    Field::Sides => fox_engine::expr::Kind::Unitless,
    _ => fox_engine::expr::Kind::Length,
  };
  let v = fox_engine::expr::eval(&hud_text(app, field, text), kind, &app.s.model.params).ok()?;
  match field {
    Field::Angle => Some(v),
    // the polygon command takes 3 to 256 sides
    Field::Sides => (3.0..=256.0).contains(&v).then_some(v.round()),
    _ => (v > 1e-9).then_some(v),
  }
}

/// The cursor moved to where the typed heads-up values put it.
fn hud_apply(app: &App, cur: [f64; 2]) -> [f64; 2] {
  let Some(&last) = app.tstate.pts.last() else { return cur };
  if hud_fields(app.tool, app.tstate.pts.len()).is_empty() {
    return cur;
  }
  let v = |i| hud_value(app, i);
  let (dx, dy) = (cur[0] - last[0], cur[1] - last[1]);
  let sign = |d: f64| if d < 0.0 { -1.0 } else { 1.0 };
  match app.tool {
    Tool::Line => {
      let len = v(0).unwrap_or(dx.hypot(dy));
      let ang = v(1).unwrap_or(dy.atan2(dx));
      [last[0] + len * ang.cos(), last[1] + len * ang.sin()]
    }
    Tool::Rect | Tool::CenterRect => {
      // a centre rectangle's boxes hold its whole width and height
      let k = if app.tool == Tool::CenterRect { 0.5 } else { 1.0 };
      [v(0).map_or(cur[0], |w| last[0] + sign(dx) * w * k), v(1).map_or(cur[1], |h| last[1] + sign(dy) * h * k)]
    }
    Tool::Circle => match v(0) {
      Some(d) => {
        let l = dx.hypot(dy);
        let (ux, uy) = if l > 1e-12 { (dx / l, dy / l) } else { (1.0, 0.0) };
        [last[0] + d / 2.0 * ux, last[1] + d / 2.0 * uy]
      }
      None => cur,
    },
    // a typed radius (or edge length) holds the point that far from the centre (or edge start)
    Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon => match v(0) {
      Some(r) => {
        let l = dx.hypot(dy);
        let (ux, uy) = if l > 1e-12 { (dx / l, dy / l) } else { (1.0, 0.0) };
        [last[0] + r * ux, last[1] + r * uy]
      }
      None => cur,
    },
    _ => cur,
  }
}

/// The values the heads-up boxes show for a cursor at `cur`: length and angle (degrees), width
/// and height, or diameter.
fn hud_live(app: &App, cur: [f64; 2]) -> [f64; 2] {
  let Some(&last) = app.tstate.pts.last() else { return [0.0; 2] };
  let (dx, dy) = (cur[0] - last[0], cur[1] - last[1]);
  match app.tool {
    Tool::Line => [dx.hypot(dy), dy.atan2(dx).to_degrees().rem_euclid(360.0)],
    Tool::Rect => [dx.abs(), dy.abs()],
    Tool::CenterRect => [2.0 * dx.abs(), 2.0 * dy.abs()],
    Tool::Circle => [2.0 * dx.hypot(dy), 0.0],
    Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon => [dx.hypot(dy), 6.0],
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

/// The number of sides the polygon being drawn will have: what its heads-up box holds, else 6.
fn hud_sides_live(app: &App) -> usize {
  let i = hud_fields(app.tool, app.tstate.pts.len()).iter().position(|f| *f == Field::Sides);
  i.and_then(|i| hud_value(app, i)).map_or(6, |v| v as usize)
}

/// A heads-up box's live value as Fusion shows it: a length with its unit at the document's
/// display precision ("0.492 in"), an angle to one decimal with "deg" ("336.8 deg"), a count as it
/// is ("6"). Each is also a valid expression to type back.
fn hud_display(unit: fox_engine::model::LengthUnit, field: Field, v: f64) -> String {
  match field {
    Field::Angle => format!("{v:.1} deg"),
    Field::Sides => format!("{}", v.round() as i64),
    _ => unit.fixed(v, display_decimals(unit)),
  }
}

/// Fusion's default display precision for a unit (Document Settings → Units).
fn display_decimals(unit: fox_engine::model::LengthUnit) -> usize {
  use fox_engine::model::LengthUnit;
  match unit {
    LengthUnit::Mm | LengthUnit::Cm => 2,
    LengthUnit::In => 3,
    LengthUnit::M | LengthUnit::Ft => 4,
  }
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
    // rectangles are drawn bottom, right, top, left: the bottom sets the width, the right the height
    let curve = match field {
      Field::Width | Field::Length => curves.first(),
      Field::Height => curves.get(1),
      Field::Angle | Field::Diameter | Field::Radius | Field::Sides => None,
    };
    if let Some(c) = curve {
      app.run("sketch.dimension", json!({"sketch": sk.0, "type": "distance", "curves": [c], "value": text}));
    }
    if *field == Field::Angle
      && let (Some(&line), Some(from)) = (curves.first(), from)
    {
      lock_angle(app, sk, line, from);
    }
  }
}

/// Locks a polygon's typed radius as Fusion dimensions it: centre to the first vertex (inscribed)
/// or centre to the first side (circumscribed); an edge polygon's typed length dimensions its first
/// side. `curves` is `sketch.polygon`'s result: the construction circle, then the sides.
fn lock_polygon(app: &mut App, sk: FeatureId, curves: &[u32], locks: &[(Field, String)]) {
  let (Some(&circle), Some(&side)) = (curves.first(), curves.get(1)) else { return };
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let point = |c: u32| sketch.curve_points(fox_engine::sketch::CurveId(c)).first().copied();
  let (Some(center), Some(vertex)) = (point(circle), point(side)) else { return };
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

/// Locks a chained line's angle to the segment before it. A first segment has nothing to measure
/// from; a horizontal or vertical one is locked by its H/V constraint already.
fn lock_angle(app: &mut App, sk: FeatureId, line: u32, from: [f64; 2]) {
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let near = |id: PointId| sketch.pos(id).is_some_and(|q| (q[0] - from[0]).hypot(q[1] - from[1]) < 1e-6);
  let prev = sketch.curves.iter().rev().find_map(|c| match c.kind {
    CurveKind::Line { a, b } if c.id.0 != line && (near(a) || near(b)) => Some(c.id),
    _ => None,
  });
  let Some(prev) = prev else { return };
  let ends = sketch.curve_points(fox_engine::sketch::CurveId(line));
  if let (Some(p), Some(q)) = (ends.first().and_then(|p| sketch.pos(*p)), ends.get(1).and_then(|p| sketch.pos(*p)))
    && ((p[0] - q[0]).abs() < 1e-9 || (p[1] - q[1]).abs() < 1e-9)
  {
    return;
  }
  let Some(ang) = line_angle(sketch, prev, fox_engine::sketch::CurveId(line)) else { return };
  let deg = ang.to_degrees();
  if deg.abs() < 1e-6 || (deg - 180.0).abs() < 1e-6 {
    return;
  }
  let value = format!("{} deg", fox_engine::sketch::Dim::new(deg).expr);
  app.run("sketch.dimension", json!({"sketch": sk.0, "type": "angle", "curves": [prev.0, line], "value": value}));
}

/// Commits the fit-point spline being drawn (Enter, or clicking its last point again).
/// Returns whether there was a spline to commit.
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

/// The tangent arc from `start` (the end of a line or arc) to `end`, as the engine's
/// `sketch.arc {tangent}` draws it: points along it, or None when nothing ends at `start` or the
/// end lies straight ahead.
fn tangent_arc_points(app: &App, start: [f64; 2], end: [f64; 2]) -> Option<Vec<[f64; 2]>> {
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
  if l < 1e-12 {
    return None;
  }
  let n = [-dir[1] / l, dir[0] / l];
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
  Some(
    (0..=64)
      .map(|i| {
        let a = a0 + sweep * i as f64 / 64.0;
        [c[0] + r * a.cos(), c[1] + r * a.sin()]
      })
      .collect(),
  )
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
  let size = line.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f64>();
  let tol = (size * 1e-4).max(1e-9);
  const SPLIT: usize = 8;
  let mut samples: Vec<[f64; 2]> = Vec::new();
  for w in line.windows(2) {
    for i in 0..SPLIT {
      let k = i as f64 / SPLIT as f64;
      samples.push([w[0][0] + (w[1][0] - w[0][0]) * k, w[0][1] + (w[1][1] - w[0][1]) * k]);
    }
  }
  samples.extend(line.last().copied());
  let removed = |p: [f64; 2]| !kept.iter().any(|k| after.distance_to_curve(*k, p).is_some_and(|d| d <= tol));
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
fn curve_tool_click(app: &mut App, view: &View, pos: Pos2, shift: bool) {
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
  let picked = app.tstate.curves.len();
  // the 2-tangent circle's third click is anywhere: it places the circle
  if app.tool == Tool::Circle2T && picked == 2 {
    let curves: Vec<u32> = app.tstate.curves.iter().map(|c| c.0).collect();
    let r = tangent_radius(app, sk, &curves, at);
    app.tstate.curves.clear();
    app.run("sketch.tangent_circle", json!({"sketch": sk.0, "curves": curves, "radius": r, "near": at}));
    return;
  }
  let Some(c) = hovered else {
    app.status = Some(("Click a curve".into(), true));
    return;
  };
  if app.tstate.curves.iter().any(|x| x.0 == c) {
    return;
  }
  app.tstate.curves.push((c, at));
  let picks = app.tstate.curves.clone();
  let curves: Vec<u32> = picks.iter().map(|c| c.0).collect();
  match (app.tool, picks.as_slice()) {
    (Tool::Circle3T, [a, b, c]) => {
      app.tstate.curves.clear();
      let near = [(a.1[0] + b.1[0] + c.1[0]) / 3.0, (a.1[1] + b.1[1] + c.1[1]) / 3.0];
      app.run("sketch.tangent_circle", json!({"sketch": sk.0, "curves": curves, "near": near}));
    }
    (Tool::Blend, [a, b]) => {
      app.tstate.curves.clear();
      let continuity = if shift { "g2" } else { "g1" };
      app.run("sketch.blend", json!({"sketch": sk.0, "curves": curves, "near": [a.1, b.1], "continuity": continuity}));
    }
    _ => {}
  }
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
  app.picked_plane = None;
  match app.hover {
    Some(p) if app.s.edit_sketch.is_some() => crate::panels::toggle_sel(&mut app.s, p.to_sel(), add),
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

/// Dimension tool: line → length, circle → diameter, arc → radius; point then point/line →
/// distance; shift-click a line then another line → angle.
fn dimension_click(app: &mut App, pos: Pos2, shift: bool) {
  let Some(sk) = app.s.edit_sketch else { return };
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let sketch = sketch.clone();
  let hover = app.hover;
  let first = app.tstate.dim_first;
  let (cmd, current) = match (first, hover) {
    (None, Some(Pick::SketchCurve { curve, .. })) => match sketch.curve(curve).map(|c| c.kind.clone()) {
      Some(CurveKind::Line { a, b }) => {
        if shift {
          app.tstate.dim_first = hover;
          app.hint("Pick a second line for an angle");
          return;
        }
        (json!({"type": "distance", "points": [a.0, b.0]}), dist(&sketch, a, b))
      }
      Some(CurveKind::Circle { radius, .. }) => (json!({"type": "diameter", "curves": [curve.0]}), Some(2.0 * radius)),
      Some(CurveKind::Arc { .. }) => (json!({"type": "radius", "curves": [curve.0]}), sketch.radius(curve)),
      Some(
        CurveKind::Spline { .. }
        | CurveKind::ControlSpline { .. }
        | CurveKind::Ellipse { .. }
        | CurveKind::Bezier { .. }
        | CurveKind::EllipticalArc { .. }
        | CurveKind::Conic { .. }
        | CurveKind::QuinticSpline { .. },
      )
      | None => return,
    },
    (None, Some(Pick::SketchPoint { .. })) => {
      app.tstate.dim_first = hover;
      app.hint("Pick a second point or a line");
      return;
    }
    (Some(Pick::SketchPoint { point: a, .. }), Some(Pick::SketchPoint { point: b, .. })) if a != b => {
      app.tstate.dim_first = None;
      (json!({"type": "distance", "points": [a.0, b.0]}), dist(&sketch, a, b))
    }
    (Some(Pick::SketchPoint { point, .. }), Some(Pick::SketchCurve { curve, .. })) => {
      app.tstate.dim_first = None;
      let d = match (sketch.pos(point), sketch.curve(curve)) {
        (Some(p), Some(_)) => sketch.distance_to_curve(curve, p),
        _ => None,
      };
      (json!({"type": "point_line", "points": [point.0], "curves": [curve.0]}), d)
    }
    (Some(Pick::SketchCurve { curve: a, .. }), Some(Pick::SketchCurve { curve: b, .. })) if a != b => {
      app.tstate.dim_first = None;
      let ang = line_angle(&sketch, a, b);
      (json!({"type": "angle", "curves": [a.0, b.0]}), ang.map(f64::to_degrees))
    }
    _ => {
      app.tstate.dim_first = None;
      return;
    }
  };
  // the measured value, in the design's unit (degrees for an angle)
  let angle = cmd["type"] == "angle";
  let unit = app.s.design.units;
  let text =
    current.map(|v| if angle { fox_engine::sketch::Dim::new(v).expr } else { unit.number(v) }).unwrap_or_default();
  // stash the pending dimension in the editor (constraint id 0 = new)
  app.dim_edit = Some((0, text, pos));
  app.tstate.pending_dim = Some(cmd);
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
  let mut open_params = false;
  let mut delete = false;
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
          let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0).hint_text("value or expression"));
          let r = match &param {
            Some(name) => r.on_hover_text(format!("{name} : {text}")),
            None => r,
          };
          if fresh {
            focus_text(ctx, &r, &text, true);
          } else if !r.has_focus() && !r.lost_focus() && !egui::Popup::is_any_open(ctx) {
            r.request_focus();
          }
          // Enter commits and Esc cancels, as in Fusion (no OK button); ⋮ holds the dimension's menu
          let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
          cancel = esc;
          commit = enter && !esc && (r.has_focus() || r.lost_focus());
          if cid != 0 {
            let m = ui.add(egui::Button::new("").min_size(Vec2::new(14.0, 20.0)).frame(false));
            let dot = Color32::from_gray(60);
            for dy in [-4.0, 0.0, 4.0] {
              ui.painter().circle_filled(m.rect.center() + Vec2::new(0.0, dy), 1.3, dot);
            }
            egui::Popup::menu(&m).show(|ui| {
              if ui.button("Change Parameters").clicked() {
                open_params = true;
              }
              if ui.button("Delete").clicked() {
                delete = true;
              }
            });
          }
        });
      });
    });
  if open_params {
    app.show_params = true;
  }
  if delete && let Some(sk) = app.s.edit_sketch {
    app.run("sketch.delete", json!({"sketch": sk.0, "constraints": [cid]}));
    return;
  }
  if commit {
    // the Enter is this box's, not the open dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    if let Some(sk) = app.s.edit_sketch {
      // a bare length means the design's unit
      let unit = app.s.design.units;
      if cid == 0 {
        if let Some(mut cmd) = app.tstate.pending_dim.take() {
          let value = if cmd["type"] == "angle" { text.clone() } else { unit.typed(&text) };
          cmd["sketch"] = json!(sk.0);
          cmd["value"] = json!(value);
          app.run("sketch.dimension", cmd);
        }
      } else {
        let angle = app
          .s
          .design
          .sketch(sk)
          .ok()
          .and_then(|s| s.constraint(fox_engine::sketch::ConstraintId(cid)))
          .is_some_and(|c| c.kind.is_angle());
        let value = if angle { text.clone() } else { unit.typed(&text) };
        app.run("sketch.set_dimension", json!({"sketch": sk.0, "constraint": cid, "value": value}));
      }
    }
  }
  if cancel {
    app.tstate.pending_dim = None;
  }
  if !commit && !cancel {
    app.dim_edit = Some((cid, text, pos));
  }
}

/// The model parameter that drives the open sketch's dimension `cid` ("d7"), as the Parameters
/// dialog names it.
fn dim_param_name(app: &App, cid: u32) -> Option<String> {
  let sk = app.s.edit_sketch?;
  let sketch = app.s.design.sketch(sk).ok()?;
  let index = sketch.constraints.iter().filter(|c| c.kind.dim().is_some()).position(|c| c.id.0 == cid)?;
  app.s.design.model_parameters().into_iter().find(|p| p.feature == sk && p.index == index).map(|p| p.name)
}

/// What the dimension editor opens with: a plain value with its unit ("0.4 in", "45 deg") as Fusion
/// shows it; an expression with parameters as it was typed.
fn dim_edit_text(k: &ConstraintKind, dim: &fox_engine::sketch::Dim, unit: fox_engine::model::LengthUnit) -> String {
  if fox_engine::expr::eval_raw(&dim.expr, &fox_engine::expr::Empty).is_err() {
    return dim.expr.clone();
  }
  if k.is_angle() {
    format!("{} deg", fox_engine::sketch::Dim::new(dim.value.to_degrees()).expr)
  } else {
    unit.format(dim.value)
  }
}

/// Gives a text box the keyboard, with all its text selected (typing replaces it) or the caret
/// at the end.
pub(crate) fn focus_text(ctx: &egui::Context, r: &egui::Response, text: &str, select_all: bool) {
  use egui::text::{CCursor, CCursorRange};
  r.request_focus();
  let n = text.chars().count();
  let mut state = egui::text_edit::TextEditState::load(ctx, r.id).unwrap_or_default();
  let range =
    if select_all { CCursorRange::two(CCursor::new(0), CCursor::new(n)) } else { CCursorRange::one(CCursor::new(n)) };
  state.cursor.set_char_range(Some(range));
  state.store(ctx, r.id);
}

// ---------------------------------------------------------------- overlays

/// The dimensions of sketches the browser's Show Dimensions turned on, drawn (not editable)
/// while they're not being edited.
fn shown_dims_overlay(app: &App, painter: &egui::Painter, view: &View) {
  let wpp = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let font = egui::FontId::proportional(12.0);
  for sk in &app.shown_dims {
    if app.s.edit_sketch == Some(*sk) || !fox_engine::scene::sketch_visible(&app.s, *sk) {
      continue;
    }
    let Some(r) = app.s.model.sketches.get(sk) else { continue };
    let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, r.plane.to_world(p));
    for c in &r.sketch.constraints {
      let Some(dim) = c.kind.dim() else { continue };
      let Some((anchor, label)) = dim_label(&r.sketch, &c.kind, dim, app.s.design.units) else { continue };
      let text = DimText::new(painter, &label, font.clone());
      let size = text.size() + Vec2::new(8.0, 4.0);
      let shape = dim_shape(&r.sketch, &c.kind, dim, size, wpp, &to_screen);
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
      painter.rect_filled(rect, 1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 225));
      text.paint(painter, rect.min + Vec2::new(4.0, 2.0), Color32::BLACK);
    }
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
      let shape = dim_shape(&sketch, &c.kind, dim, size, wpp, &to_screen);
      let at = match &shape {
        Some(s) => s.label,
        None => match to_screen(anchor) {
          Some(p) => p,
          None => continue,
        },
      };
      let r = Rect::from_center_size(at, size);
      let resp = ui.interact(r, ui.id().with(("dim", c.id.0)), Sense::click());
      if let Some(s) = &shape {
        let ink = if selected || resp.hovered() { crate::theme::ACCENT } else { DIM_INK };
        paint_dim(painter, s, ink);
      }
      let bg = if selected {
        crate::theme::SELECTED
      } else if resp.hovered() {
        crate::theme::HOVER
      } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 225)
      };
      painter.rect_filled(r, 1.0, bg);
      if selected || resp.hovered() {
        painter.rect_stroke(r, 1.0, Stroke::new(1.0, crate::theme::ACCENT), egui::StrokeKind::Inside);
      }
      text.paint(painter, r.min + Vec2::new(4.0, 2.0), Color32::BLACK);
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
    // geometric constraint glyph near its first curve (or point)
    let Some((anchor, key)) = glyph_anchor(&sketch, &c.kind) else { continue };
    let k = glyph_count.entry(key).or_default();
    let Some(at) = to_screen(anchor) else { continue };
    let at = at + Vec2::new(10.0 + 14.0 * *k as f32, -12.0);
    *k += 1;
    let r = Rect::from_center_size(at, Vec2::splat(15.0));
    let resp = ui.interact(r, ui.id().with(("glyph", c.id.0)), Sense::click());
    let (bg, edge) = if selected {
      (crate::theme::SELECTED, crate::theme::ACCENT)
    } else if resp.hovered() {
      (crate::theme::HOVER, crate::theme::ACCENT)
    } else {
      (Color32::from_rgba_unmultiplied(255, 255, 255, 230), Color32::from_gray(170))
    };
    painter.rect(r, 1.0, bg, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    icons::draw_color(painter, r.shrink(2.0), glyph(&c.kind));
    if resp.clicked() {
      let add = ui.input(|i| i.modifiers.shift);
      crate::panels::toggle_sel(&mut app.s, Sel::SketchConstraint { sketch: sk, constraint: c.id }, add);
      consumed = true;
    }
    resp.on_hover_text(format!("{} (click to select, Del to remove)", c.kind.name()));
  }
  consumed
}

/// A dimension's value as drawn. The font has no ⌀ (U+2300), so a diameter's glyph is painted
/// the way Fusion draws it, a circle with a slash, in front of the number.
struct DimText {
  glyph: bool,
  galley: std::sync::Arc<egui::Galley>,
  em: f32,
}

impl DimText {
  fn new(painter: &egui::Painter, label: &str, font: egui::FontId) -> DimText {
    let (glyph, rest) = match label.strip_prefix('⌀') {
      Some(rest) => (true, rest),
      None => (false, label),
    };
    DimText { glyph, em: font.size, galley: painter.layout_no_wrap(rest.to_string(), font, Color32::BLACK) }
  }

  fn glyph_width(&self) -> f32 {
    if self.glyph { self.em * 0.8 } else { 0.0 }
  }

  fn size(&self) -> Vec2 {
    self.galley.size() + Vec2::new(self.glyph_width(), 0.0)
  }

  fn paint(&self, painter: &egui::Painter, at: Pos2, col: Color32) {
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

/// The sketch grid's coordinate labels along both axes in the design's unit, as Fusion prints
/// them (-2.5 ... 2.5 in), and a small marker at the origin.
fn grid_labels(app: &App, painter: &egui::Painter, view: &View) {
  let Some((_, plane)) = sketch_plane(app) else { return };
  let step = app.grid.step().unwrap_or_else(|| fox_engine::scene::grid_step(&app.s));
  let size = app.s.model.bounds().map(|(a, b)| (b - a).length()).unwrap_or(100.0).max(50.0);
  let positions = grid_label_positions(step, size);
  let unit = app.s.design.units;
  let ink = Color32::from_gray(120);
  let font = egui::FontId::proportional(11.0);
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let last = positions.last().copied().unwrap_or(0.0);
  for &t in &positions {
    let mut text = unit.number(t);
    // the unit's symbol once per axis, on the outermost label
    if t == last {
      text = format!("{text} {}", unit.symbol());
    }
    // along the sketch's X axis (below the axis) and its Y axis (left of the axis); zero once
    if let Some(p) = to_screen([t, 0.0])
      && view.rect.contains(p)
    {
      painter.text(p + Vec2::new(0.0, 4.0), egui::Align2::CENTER_TOP, &text, font.clone(), ink);
    }
    if t != 0.0
      && let Some(p) = to_screen([0.0, t])
      && view.rect.contains(p)
    {
      painter.text(p + Vec2::new(-5.0, 0.0), egui::Align2::RIGHT_CENTER, &text, font.clone(), ink);
    }
  }
  if let Some(o) = to_screen([0.0, 0.0])
    && view.rect.contains(o)
  {
    painter.circle_stroke(o, 3.5, Stroke::new(1.0, Color32::from_gray(90)));
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
const DIM_INK: Color32 = Color32::from_rgb(25, 25, 30);
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

/// The leader graphics of dimension `k` (see [`DimShape`]); `size` is its value's box.
fn dim_shape(
  s: &Sketch,
  k: &ConstraintKind,
  dim: &fox_engine::sketch::Dim,
  size: Vec2,
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
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
  // a plain value ("10", "1 in") shows as the number; an expression with parameters as "w = 10"
  let numeric = fox_engine::expr::eval_raw(&dim.expr, &fox_engine::expr::Empty).is_ok();
  let val = if k.is_angle() {
    format!("{}°", fox_engine::sketch::Dim::new(dim.value.to_degrees()).expr)
  } else {
    // in the design's unit, without the unit (as Fusion labels sketch dimensions)
    fox_engine::expr::trim_number(unit.from_mm(dim.value))
  };
  let shown = |prefix: &str| if numeric { format!("{prefix}{val}") } else { format!("{prefix}{} = {val}", dim.expr) };
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

fn glyph(k: &ConstraintKind) -> Icon {
  match k {
    ConstraintKind::Coincident { .. } | ConstraintKind::PointOnCurve { .. } => Icon::Coincident,
    ConstraintKind::Fix { .. } => Icon::Fix,
    ConstraintKind::Horizontal { .. } | ConstraintKind::HorizontalPoints { .. } => Icon::Horizontal,
    ConstraintKind::Vertical { .. } | ConstraintKind::VerticalPoints { .. } => Icon::Vertical,
    ConstraintKind::Parallel { .. } => Icon::Parallel,
    ConstraintKind::Perpendicular { .. } => Icon::Perpendicular,
    ConstraintKind::Collinear { .. } => Icon::Collinear,
    ConstraintKind::Equal { .. } => Icon::Equal,
    ConstraintKind::Tangent { .. } => Icon::Tangent,
    ConstraintKind::Concentric { .. } => Icon::Concentric,
    ConstraintKind::Midpoint { .. } => Icon::Midpoint,
    ConstraintKind::Symmetric { .. } | ConstraintKind::SymmetricCurves { .. } => Icon::Symmetric,
    ConstraintKind::Curvature { .. } => Icon::Curvature,
    ConstraintKind::Polygon { .. } => Icon::Equal,
    _ => Icon::Fix,
  }
}

/// Where a geometric constraint's glyph goes, and a key grouping glyphs on the same curve/point.
fn glyph_anchor(s: &Sketch, k: &ConstraintKind) -> Option<([f64; 2], u32)> {
  if let Some(c) = k.curves().first() {
    let curve = s.curve(*c)?;
    let pts = fox_engine::scene::curve_polyline(s, &curve.kind);
    let m = *pts.get(pts.len() / 2)?;
    let m = if pts.len() == 2 { [(pts[0][0] + pts[1][0]) / 2.0, (pts[0][1] + pts[1][1]) / 2.0] } else { m };
    return Some((m, c.0));
  }
  let p = *k.points().first()?;
  Some((s.pos(p)?, p.0 | 0x8000_0000))
}

/// Rubber-band preview of the shape being drawn.
fn tool_preview(app: &App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  let Some((_, plane)) = sketch_plane(app) else { return };
  if matches!(app.tool, Tool::Select | Tool::Dimension) || app.tool.picks_curves() && app.tool != Tool::Circle2T {
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
    (Tool::Spline | Tool::ControlSpline | Tool::Conic | Tool::ArcSlot3 | Tool::ArcSlotCenter, [_, ..]) => {
      let mut all = pts.clone();
      all.push(cur);
      poly(&all, false);
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
    (Tool::OverallSlot | Tool::CenterSlot | Tool::Ellipse, [a]) => line(*a, cur),
    (Tool::OverallSlot | Tool::CenterSlot | Tool::Ellipse, [a, b]) => {
      line(*a, *b);
      let w = point_line_dist(cur, *a, *b);
      if app.tool == Tool::Ellipse {
        // through the cursor; off the ends of the axis, as far from it as the cursor
        let w = Ellipse2::minor_through(*a, *b, cur).unwrap_or(w);
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
      } else {
        circle(*a, w);
        circle(*b, w);
      }
    }
    (Tool::Rect, [a]) => poly(&[*a, [cur[0], a[1]], cur, [a[0], cur[1]]], true),
    (Tool::CenterRect, [c]) => {
      let o = [2.0 * c[0] - cur[0], 2.0 * c[1] - cur[1]];
      poly(&[o, [cur[0], o[1]], cur, [o[0], cur[1]]], true);
    }
    (Tool::Circle, [c]) => circle(*c, (cur[0] - c[0]).hypot(cur[1] - c[1])),
    (Tool::Arc3 | Tool::CenterArc | Tool::Slot, [a]) => line(*a, cur),
    (Tool::Arc3, [a, b]) => {
      if let Some(c) = fox_engine::sketch::circumcenter(*a, cur, *b) {
        circle(c, (a[0] - c[0]).hypot(a[1] - c[1]));
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
      let w = point_line_dist(cur, *a, *b);
      circle(*a, w);
      circle(*b, w);
      line(*a, *b);
    }
    _ => {}
  }
  paint_inference_lines(app, painter, view, pos);
  // cursor marker, and a coordinate readout where no heads-up dimensions show
  if let Some(p) = s(cur) {
    let col = if snapped.is_some() { Color32::from_rgb(230, 120, 20) } else { Color32::from_rgb(30, 110, 235) };
    painter.circle_stroke(p, 4.0, Stroke::new(1.5, col));
    if !hud_fields(app.tool, pts.len()).is_empty() {
      return;
    }
    let u = app.s.design.units;
    let txt = format!("{}, {}", u.number(cur[0]), u.number(cur[1]));
    painter.text(
      p + Vec2::new(10.0, 10.0),
      egui::Align2::LEFT_TOP,
      txt,
      egui::FontId::proportional(11.0),
      Color32::from_gray(70),
    );
  }
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
  if !typed.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '-') {
    return;
  }
  if app.s.edit_sketch.is_some() && !hud_fields(app.tool, app.tstate.pts.len()).is_empty() {
    let hud = &mut app.tstate.hud;
    hud.typed[hud.active] = typed.clone();
    hud.buf = Some(typed);
    hud.focus = Some(false);
  } else if app.dialog.is_some() {
    crate::dialogs::type_into(ctx, typed);
  } else {
    return;
  }
  use egui::Key;
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
    && !hud_fields(app.tool, app.tstate.pts.len()).is_empty()
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
  let fields = hud_fields(app.tool, app.tstate.pts.len());
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
  let Some(&last) = app.tstate.pts.last() else { return };
  let cur = hud_apply(app, raw);
  let live = hud_live(app, cur);
  let wpp = app.cam.view_height() / view.h.max(1) as f64 * view.ppp as f64;
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
  let ink = Color32::from_rgb(30, 110, 235);
  let off = 30.0 * wpp;
  // each box's dimension graphics, ending where its box goes
  let mut at: Vec<Option<Pos2>> = Vec::new();
  for f in fields {
    let shape = match (app.tool, f) {
      (Tool::Line | Tool::EdgePolygon, Field::Length) => {
        let (dx, dy) = (cur[0] - last[0], cur[1] - last[1]);
        let l = dx.hypot(dy).max(1e-12);
        // the dimension sits on the line's left
        let away = [(last[0] + cur[0]) / 2.0 + dy / l, (last[1] + cur[1]) / 2.0 - dx / l];
        linear_shape(last, cur, [dx / l, dy / l], away, off, wpp, &to_screen)
      }
      (Tool::Line, Field::Angle) => hud_angle_shape(last, cur, wpp, &to_screen),
      (Tool::Rect | Tool::CenterRect, Field::Width | Field::Height) => {
        let o = if app.tool == Tool::CenterRect { [2.0 * last[0] - cur[0], 2.0 * last[1] - cur[1]] } else { last };
        let mid = [(o[0] + cur[0]) / 2.0, (o[1] + cur[1]) / 2.0];
        let corner = [cur[0], o[1]];
        if *f == Field::Width {
          linear_shape(o, corner, [1.0, 0.0], mid, off, wpp, &to_screen)
        } else {
          linear_shape(corner, cur, [0.0, 1.0], mid, off, wpp, &to_screen)
        }
      }
      (Tool::Circle, Field::Diameter) => {
        let r = (cur[0] - last[0]).hypot(cur[1] - last[1]);
        radial_shape(last, r, cur, true, Vec2::new(60.0, 18.0), &to_screen)
      }
      (Tool::Polygon | Tool::CircPolygon, Field::Radius) => {
        let r = (cur[0] - last[0]).hypot(cur[1] - last[1]);
        radial_shape(last, r, cur, false, Vec2::new(60.0, 18.0), &to_screen)
      }
      _ => None,
    };
    if let Some(s) = &shape {
      paint_dim(painter, s, ink);
    }
    at.push(shape.map(|s| s.label));
  }
  // a polygon's sides box sits under its radius box, as in Fusion
  if let Some(i) = fields.iter().position(|f| *f == Field::Sides) {
    let under = at.first().copied().flatten().map(|p| p + Vec2::new(0.0, 26.0));
    at[i] = under;
  }
  // lengths in the design's unit (a bare number typed back means that unit, see [`hud_text`])
  let unit = app.s.design.units;
  let fmt = |i: usize| hud_display(unit, fields[i], live[i]);
  let n = fields.len();
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
          // a typed value is locked: a padlock before the box, which stays editable until Enter
          if typed {
            let (lr, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            paint_lock(ui.painter(), lr.center(), ink);
          }
          // a fixed id: the padlock appearing before the box must not change it (and drop focus)
          let r = ui.add(
            egui::TextEdit::singleline(&mut buf)
              .id(egui::Id::new(("hud-edit", i)))
              .desired_width(62.0)
              .font(egui::FontId::proportional(12.0))
              .margin(egui::Margin::symmetric(3, 1))
              .lock_focus(true),
          );
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
      hud.buf = keep.then_some(buf).filter(|_| !commit);
      continue;
    }
    // the other boxes show the live value, or what was typed (locked, with its unit) after a
    // padlock
    let text = if typed {
      let t = hud.typed[i].trim();
      if matches!(fields[i], Field::Angle | Field::Sides) { t.to_string() } else { unit.typed(t) }
    } else {
      fmt(i)
    };
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text, font, if typed { Color32::BLACK } else { Color32::from_gray(60) });
    let lock_w = if typed { 14.0 } else { 0.0 };
    let r = Rect::from_center_size(p, galley.size() + Vec2::new(10.0 + lock_w, 5.0));
    let active = i == hud.active;
    painter.rect(
      r,
      2.0,
      Color32::from_rgba_unmultiplied(255, 255, 255, 235),
      Stroke::new(if active || typed { 1.5 } else { 1.0 }, if active || typed { ink } else { Color32::from_gray(160) }),
      egui::StrokeKind::Inside,
    );
    if typed {
      paint_lock(painter, r.left_center() + Vec2::new(9.0, 0.0), ink);
    }
    painter.galley(r.min + Vec2::new(5.0 + lock_w, 2.5), galley, Color32::BLACK);
  }
  if commit {
    // this Enter places the point; it isn't the open dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let ends = matches!(app.tool, Tool::Rect | Tool::CenterRect) && app.tstate.pts.len() == 1;
    place_point(app, sk, raw, snapped);
    // Enter on typed values finishes the rectangle and its command; a click-drawn one keeps the
    // tool, as in Fusion
    if ends && app.tstate.pts.is_empty() {
      app.set_tool(Tool::Select);
    }
  }
}

/// A small padlock: the glyph Fusion puts before a heads-up value once it's typed.
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

/// A line's heads-up angle: an arc from the horizontal to the line at its start.
fn hud_angle_shape(
  from: [f64; 2],
  to: [f64; 2],
  wpp: f64,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Option<DimShape> {
  let ang = (to[1] - from[1]).atan2(to[0] - from[0]);
  let r = 42.0 * wpp;
  let arc: Vec<Pos2> = (0..=24)
    .filter_map(|k| {
      let t = ang * k as f64 / 24.0;
      to_screen([from[0] + r * t.cos(), from[1] + r * t.sin()])
    })
    .collect();
  let o = to_screen(from)?;
  let base = to_screen([from[0] + r * 1.3, from[1]])?;
  let mid =
    to_screen([from[0] + (r + 22.0 * wpp) * (ang / 2.0).cos(), from[1] + (r + 22.0 * wpp) * (ang / 2.0).sin()])?;
  let mut arrows = Vec::new();
  if arc.len() > 2 && ang.abs() > 0.15 {
    let k = arc.len();
    arrows.push((arc[0], arc[0] - arc[1]));
    arrows.push((arc[k - 1], arc[k - 1] - arc[k - 2]));
  }
  Some(DimShape { lines: vec![[o, base]], arc, arrows, label: mid })
}

/// The 2-tangent circle the next click would place.
fn tangent_circle_preview(app: &App, painter: &egui::Painter, view: &View, pos: Pos2, plane: Plane) {
  let Some(sk) = app.s.edit_sketch else { return };
  let curves: Vec<u32> = app.tstate.curves.iter().map(|c| c.0).collect();
  if curves.len() != 2 {
    return;
  }
  let (o, d) = view.ray(&app.cam, pos);
  let Some(hit) = ray_plane(o, d, plane.origin, plane.normal()) else { return };
  let at = plane.to_local(hit);
  let r = tangent_radius(app, sk, &curves, at);
  let Ok(mut sketch) = app.s.design.sketch(sk).cloned() else { return };
  let ids: Vec<_> = curves.iter().map(|c| fox_engine::sketch::CurveId(*c)).collect();
  let Ok(c) = sketch.add_tangent_circle(&ids, Some(fox_engine::sketch::Dim::new(r)), at) else { return };
  let Some(center) = sketch.curve_points(c).first().and_then(|p| sketch.pos(*p)) else { return };
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
  if let Some(p) = view.to_screen(&app.cam, plane.to_world(at)) {
    let txt = format!("R {}", app.s.design.units.number(r));
    painter.text(
      p + Vec2::new(10.0, 10.0),
      egui::Align2::LEFT_TOP,
      txt,
      egui::FontId::proportional(11.0),
      Color32::from_gray(70),
    );
  }
}

/// Fusion's pivot marker: a ringed dot at the point the camera orbits about.
fn pivot_marker(painter: &egui::Painter, at: Pos2) {
  painter.circle(at, 9.0, Color32::from_white_alpha(140), Stroke::new(1.5, Color32::from_gray(60)));
  painter.circle_filled(at, 2.5, crate::theme::ACCENT);
  for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
    painter.line_segment([at + d * 5.0, at + d * 12.0], Stroke::new(1.5, Color32::from_gray(60)));
  }
}

/// Hints in the corner of the viewport.
fn hud(app: &App, painter: &egui::Painter, _resp: &egui::Response, view: &View) {
  let hint = match (app.s.edit_sketch.is_some(), app.tool) {
    (true, Tool::Line) => {
      "Line: click points · drag from the last point for a tangent arc · click the first point to close · Esc to end"
    }
    (true, Tool::Rect) => "Rectangle: click two corners",
    (true, Tool::CenterRect) => "Centre rectangle: click the centre, then a corner",
    (true, Tool::Circle) => "Circle: click the centre, then a point on the circle",
    (true, Tool::Arc3) => "3-point arc: start, end, then a point on the arc",
    (true, Tool::CenterArc) => "Centre arc: centre, start, end",
    // Fusion's prompt once the centre (or the edge's start) is down
    (true, Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon) if !app.tstate.pts.is_empty() => {
      "Place point on polygon"
    }
    (true, Tool::Polygon) => "Polygon: centre, then a vertex",
    (true, Tool::Slot) => "Slot: first centre, second centre, then the width",
    (true, Tool::Point) => "Point: click to place",
    (true, Tool::Spline) => "Spline: click fit points · click the last point again or Enter to finish",
    (true, Tool::Trim) => "Select curve section to trim · drag across curves to trim each one",
    (true, Tool::Extend) => "Extend: click near the end of a line to extend it to the next curve",
    (true, Tool::Rect3) => "3-point rectangle: two corners along one side, then the height",
    (true, Tool::Circle2P) => "2-point circle: the two ends of a diameter",
    (true, Tool::Circle3P) => "3-point circle: three points on the circle",
    (true, Tool::TangentArc) => "Tangent arc: click the end of a line or arc, then the arc's end",
    (true, Tool::CircPolygon) => "Circumscribed polygon: centre, then the middle of a side",
    (true, Tool::EdgePolygon) => "Edge polygon: the two ends of one side",
    (true, Tool::OverallSlot) => "Overall slot: the two ends, then the width",
    (true, Tool::CenterSlot) => "Centre point slot: centre, one end, then the width",
    (true, Tool::ArcSlot3) => "3-point arc slot: start, a point on the arc, end, then the width",
    (true, Tool::ArcSlotCenter) => "Centre point arc slot: arc centre, start, end, then the width",
    (true, Tool::ControlSpline) => {
      "Control point spline: click control points · click the last again or Enter to finish"
    }
    (true, Tool::Ellipse) => "Ellipse: centre, end of the major axis, then a point on the ellipse",
    (true, Tool::Conic) => "Conic: start, end, then the apex it bends towards",
    (true, Tool::MidpointLine) => "Midpoint line: the midpoint, then one end",
    (true, Tool::Dimension) => "Dimension: click a line, circle or arc — or a point then a point/line",
    (true, Tool::Select) => "Select / drag points · Del deletes · constraints apply to the selection",
    (true, Tool::Break) => "Break: click a curve to split it at its crossings (or where you click)",
    (true, Tool::Circle2T) => match app.tstate.curves.len() {
      0 | 1 => "2-tangent circle: click two lines, circles or arcs",
      _ => "2-tangent circle: click to place it (the size follows the cursor)",
    },
    (true, Tool::Circle3T) => "3-tangent circle: click three lines, circles or arcs near where it should touch",
    (true, Tool::Blend) => "Blend curve: click near the ends of two curves · Shift+click for G2 (curvature continuous)",
    (false, _) => return,
  };
  if app.status.is_some() || !app.preferences.values.show_prompt {
    return;
  }
  painter.text(
    view.rect.left_bottom() + Vec2::new(12.0, -48.0),
    egui::Align2::LEFT_BOTTOM,
    hint,
    egui::FontId::proportional(12.0),
    Color32::from_gray(90),
  );
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

/// Half the side of a plane's square: the size the scene draws construction planes at.
pub(crate) fn plane_half_size(app: &App) -> f64 {
  app.s.model.bounds().map(|(a, b)| (b - a).length()).unwrap_or(100.0).max(50.0) * 0.35
}

/// Half the side of an origin plane's square on screen, in points: Fusion keeps them about
/// 120 px wide (twice the ViewCube) whatever the zoom.
pub(crate) const ORIGIN_PLANE_HALF_PX: f64 = 60.0;
/// How far the origin axis stubs reach, as a multiple of the origin planes' half size.
pub(crate) const ORIGIN_AXIS_REACH: f64 = 1.3;

/// Half the side of an origin plane's square, in world units: sized from the camera so the
/// square stays about 120 px on screen at the origin's depth.
pub(crate) fn origin_plane_half_size(app: &App) -> f64 {
  let h = app.canvas.height() as f64;
  let height = if h > 1.0 { h } else { 800.0 };
  app.cam.view_height() / height * ORIGIN_PLANE_HALF_PX
}

/// The half size a `sketch.create` plane draws at: origin planes follow the camera, construction
/// planes the model (the size the scene draws them at, so picking matches the picture).
fn half_of(app: &App, plane: &serde_json::Value) -> f64 {
  if plane.get("feature").is_none() { origin_plane_half_size(app) } else { plane_half_size(app) }
}

/// Which of `planes` (index) the ray through `pos` hits first, if nothing nearer (a body face)
/// is in the way.
fn plane_under(app: &App, view: &View, pos: Pos2, planes: &[(serde_json::Value, Plane)]) -> Option<usize> {
  let (o, d) = view.ray(&app.cam, pos);
  let mut best: Option<(f64, usize)> = None;
  for (i, (p, pl)) in planes.iter().enumerate() {
    let half = half_of(app, p);
    let Some(hit) = ray_plane(o, d, pl.origin, pl.normal()) else { continue };
    let t = (hit - o).dot(d);
    let [u, v] = pl.to_local(hit);
    if t > 0.0 && u.abs() <= half && v.abs() <= half && best.is_none_or(|(bt, _)| t < bt) {
      best = Some((t, i));
    }
  }
  let (t, i) = best?;
  let face = fox_engine::scene::ray_hits(&app.s, o, d).first().map(|h| h.0);
  face.is_none_or(|f| t <= f).then_some(i)
}

/// The planes shown for picking a sketch plane this frame.
struct PlanePick {
  planes: Vec<(serde_json::Value, Plane)>,
  /// Create Sketch is waiting for a plane (else only the plane picked in the browser shows).
  picking: bool,
  /// The plane under the cursor (index).
  hot: Option<usize>,
}

/// While Create Sketch waits for a plane: the origin and construction planes, and which one is
/// under the cursor (that one wins over the face highlight). Otherwise the plane picked in the
/// browser, if any.
fn plane_pick(app: &mut App, resp: &egui::Response, view: &View) -> Option<PlanePick> {
  let picking = app.dialog.as_ref().is_some_and(|d| d.kind == crate::dialogs::Kind::PickPlane);
  let picked = app.picked_plane.as_ref().filter(|_| app.s.edit_sketch.is_none());
  let origin_shown = app.origin_shown && app.s.edit_sketch.is_none();
  if !picking && picked.is_none() && app.shown_planes.is_empty() && !origin_shown {
    return None;
  }
  let mut planes = sketch_planes(app);
  // the Construction selection filter turned off: construction planes can't be picked
  if picking && !app.sel_filter.construction {
    planes.retain(|(p, _)| p.get("feature").is_none());
  }
  if !picking {
    // the plane picked in the browser, the origin planes Show/Hide turned on, and all three
    // while the Origin folder is shown
    planes.retain(|(p, _)| {
      Some(p) == picked || app.shown_planes.contains(p) || (origin_shown && p.get("feature").is_none())
    });
  }
  let hot = if picking && app.drag.is_none() {
    resp.hover_pos().and_then(|p| plane_under(app, view, p, &planes))
  } else {
    None
  };
  if hot.is_some() {
    app.hover = None;
  }
  Some(PlanePick { planes, picking, hot })
}

/// Draws the planes as see-through orange squares, as in Fusion, the hovered (or browser-picked)
/// one brighter. Returns the `sketch.create` plane under the cursor.
fn sketch_planes_overlay(
  app: &App,
  pick: Option<PlanePick>,
  resp: &egui::Response,
  painter: &egui::Painter,
  view: &View,
) -> Option<serde_json::Value> {
  let PlanePick { planes, picking, hot } = pick?;
  let [r, g, b] = PLANE_RGB;
  // Create Sketch's small dashed axis triad at the origin (the scene draws solid stubs while the
  // Origin folder is shown, so it isn't doubled then)
  if picking && !app.origin_shown && planes.iter().any(|(p, _)| p.get("feature").is_none()) {
    origin_triad(app, painter, view, origin_plane_half_size(app) * ORIGIN_AXIS_REACH);
  }
  for (i, (p, pl)) in planes.iter().enumerate() {
    let half = half_of(app, p);
    let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
    let Some(pts) = corners.iter().map(|c| view.to_screen(&app.cam, pl.to_world(*c))).collect::<Option<Vec<Pos2>>>()
    else {
      continue;
    };
    let lit = hot == Some(i) || (!picking && app.picked_plane.as_ref() == Some(p));
    let fill = Color32::from_rgba_unmultiplied(r, g, b, if lit { 110 } else { 46 });
    let edge = Color32::from_rgba_unmultiplied(r, g, b, if lit { 255 } else { 170 });
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(if lit { 2.0 } else { 1.0 }, edge)));
  }
  if picking {
    resp.clone().on_hover_text_at_pointer(crate::dialogs::PICK_PLANE_HINT);
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

/// Select Other from the menu: at the last click, or the middle of the viewport.
pub(crate) fn select_other_at_last_click(app: &mut App) {
  let pos = app.last_click.unwrap_or(app.canvas.center());
  let ppp = app.frame.as_ref().map(|f| f.width as f32 / app.canvas.width().max(1.0)).unwrap_or(1.0);
  let view = View {
    rect: app.canvas,
    ppp,
    w: app.frame.as_ref().map(|f| f.width).unwrap_or(1),
    h: app.frame.as_ref().map(|f| f.height).unwrap_or(1),
  };
  open_select_other(app, &view, pos);
}

// ---------------------------------------------------------------- manipulators, measure line

/// The open dialog's drag arrow: drawn from its base along its direction to the current value,
/// with the floating value box at its head (see [`crate::value_box`]); dragging the head changes
/// the value (snapped to a tenth of the grid). Returns true while the arrow has the pointer.
fn manipulator(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  view: &View,
  ctx: &egui::Context,
) -> bool {
  let Some(m) = app.dialog.as_ref().and_then(|d| d.manipulator(&app.s)) else {
    app.manip.value_box.hide();
    return false;
  };
  let tip = m.origin + m.dir * m.value;
  let (Some(a), Some(b)) = (view.to_screen(&app.cam, m.origin), view.to_screen(&app.cam, tip)) else {
    app.manip.value_box.hide();
    return false;
  };
  let hot = resp.hover_pos().is_some_and(|p| (p - b).length() < 12.0);
  let active = matches!(app.drag, Some(Drag::Manip));
  let col = if hot || active { Color32::from_rgb(255, 140, 20) } else { Color32::from_rgb(30, 110, 235) };
  painter.line_segment([a, b], Stroke::new(2.0, col));
  let dir2 = (b - a).normalized();
  let dir2 = if dir2.is_finite() && (b - a).length() > 1.0 { dir2 } else { Vec2::new(0.0, -1.0) };
  let side = Vec2::new(-dir2.y, dir2.x);
  let head = [b + dir2 * 12.0, b + side * 6.0, b - side * 6.0];
  painter.add(egui::Shape::convex_polygon(head.to_vec(), col, Stroke::NONE));
  // the value box beside the head edits the dialog's field; a dialog without one gets a label
  let label_at = b + side * 14.0 + dir2 * 4.0;
  let params = crate::value_box::params_of(&app.s);
  let mut boxed = false;
  if let Some(dlg) = app.dialog.as_mut()
    && let Some(text) = dlg.value_box_field()
  {
    if crate::value_box::show(ctx, &mut app.manip.value_box, label_at, text, &params).changed {
      dlg.value_box_edited();
    }
    boxed = true;
  }
  if !boxed {
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
  let pressed_on_head = ctx.input(|i| i.pointer.press_origin()).is_some_and(|p| (p - b).length() < 14.0);
  if pressed_on_head && resp.drag_started_by(egui::PointerButton::Primary) {
    app.drag = Some(Drag::Manip);
  }
  if matches!(app.drag, Some(Drag::Manip)) {
    if let Some(p) = resp.interact_pointer_pos() {
      let (o, d) = view.ray(&app.cam, p);
      // closest point on the arrow's axis to the pointer ray
      let w0 = m.origin - o;
      let (aa, bb, cc) = (m.dir.dot(m.dir), m.dir.dot(d), d.dot(d));
      let (dd, ee) = (m.dir.dot(w0), d.dot(w0));
      let den = aa * cc - bb * bb;
      if den.abs() > 1e-12 {
        let t = (bb * ee - cc * dd) / den;
        let step = app.increments.linear_step(grid_step(app, view) / 10.0);
        if let Some(dlg) = app.dialog.as_mut() {
          dlg.set_manipulator(t, step);
        }
      }
    }
    ctx.request_repaint();
    return true;
  }
  hot || pressed_on_head && ctx.input(|i| i.pointer.primary_down())
}

/// The Measure dialog's result: a line between the closest points with the distance.
fn measure_overlay(app: &App, painter: &egui::Painter, view: &View) {
  if let Some(c) = app.dialog.as_ref().and_then(|d| d.marker())
    && let Some(p) = view.to_screen(&app.cam, c)
  {
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
  let col = Color32::from_rgb(230, 90, 30);
  painter.line_segment([pa, pb], Stroke::new(2.0, col));
  for p in [pa, pb] {
    painter.circle_filled(p, 3.5, col);
  }
  let mid = pa + (pb - pa) * 0.5;
  let galley = painter.layout_no_wrap(label.clone(), egui::FontId::proportional(12.0), Color32::WHITE);
  let r = Rect::from_center_size(mid + Vec2::new(0.0, -14.0), galley.size() + Vec2::new(10.0, 4.0));
  painter.rect_filled(r, 3.0, col);
  painter.galley(r.min + Vec2::new(5.0, 2.0), galley, Color32::WHITE);
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  /// The origin planes keep about 120 px on screen whatever the zoom: their half size follows
  /// the camera's distance.
  #[test]
  fn origin_planes_are_sized_from_the_camera() {
    let mut app = App::with_session(Session::default());
    app.canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(1456.0, 800.0));
    let px_per_unit = 800.0 / app.cam.view_height();
    let half = origin_plane_half_size(&app);
    assert!((half * px_per_unit - ORIGIN_PLANE_HALF_PX).abs() < 1e-6, "{half}");
    app.cam.distance *= 3.0;
    let far = origin_plane_half_size(&app);
    assert!((far / half - 3.0).abs() < 1e-9, "{far} vs {half}");
    // construction planes keep the model-sized square the scene draws them at
    assert!((plane_half_size(&app) - 100.0 * 0.35).abs() < 1e-9);
  }

  /// The labelled grid positions are the major lines (every fifth) within the grid's extent.
  #[test]
  fn grid_labels_sit_on_the_major_lines() {
    assert_eq!(grid_label_positions(10.0, 100.0), vec![-100.0, -50.0, 0.0, 50.0, 100.0]);
    // a tiny model still gets a 2-step grid: only the origin is a major line
    assert_eq!(grid_label_positions(10.0, 1.0), vec![0.0]);
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
    typed(&mut app, ["40", "25 mm"]);
    // the cursor only gives the direction: up and to the left
    place_point(&mut app, sk, [-3.0, 7.0], None);
    assert_eq!(dims(&app, sk), vec![40.0, 25.0]);
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
    assert_eq!(label, "2");
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
    assert_eq!(dims(&app, sk), vec![50.0, 20.0]);
    let s = app.s.design.sketch(sk).unwrap();
    let (_, hi) = s.bounds().unwrap();
    assert!((hi[0] - 50.0).abs() < 1e-6 && (hi[1] - 20.0).abs() < 1e-6, "{hi:?}");
    // a 45° segment gets an angle to the previous one
    typed(&mut app, ["10", "135"]);
    place_point(&mut app, sk, [40.0, 30.0], None);
    let d = dims(&app, sk);
    assert_eq!(d.len(), 4, "{d:?}");
    assert!((d[3] - 45.0).abs() < 1e-6 || (d[3] - 135.0).abs() < 1e-6, "{d:?}");
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
    assert_eq!(label, "⌀12");
    // sketch units are screen points
    let to_screen = |p: [f64; 2]| Some(Pos2::new(p[0] as f32, p[1] as f32));
    let shape = dim_shape(s, &c.kind, c.kind.dim().unwrap(), Vec2::new(30.0, 16.0), 1.0, &to_screen).unwrap();
    assert_eq!(shape.label, Pos2::new(10.0, 5.0), "the value sits on the line at the centre");
    assert_eq!(shape.lines, vec![[Pos2::new(4.0, 5.0), Pos2::new(16.0, 5.0)]]);
    assert_eq!(shape.arrows.len(), 2);
    assert!(shape.arrows.iter().all(|(tip, dir)| (tip.x - 10.0).signum() == dir.x.signum()), "arrows point out");
    // a radius keeps its leader; an arc's diameter does too
    app.run("sketch.dimension", json!({"sketch": sk.0, "type": "radius", "curves": [curve], "value": "6"})).unwrap();
    let s = app.s.design.sketch(sk).unwrap();
    let r = s.constraints.iter().filter(|c| c.kind.dim().is_some()).nth(1).unwrap();
    let shape = dim_shape(s, &r.kind, r.kind.dim().unwrap(), Vec2::new(30.0, 16.0), 1.0, &to_screen).unwrap();
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
      let r = DimText::new(&painter, "R6", font);
      assert!(!r.glyph && r.galley.text() == "R6");
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
  }

  #[test]
  fn untyped_boxes_follow_the_cursor() {
    let (mut app, sk) = sketching(Tool::Rect);
    place_point(&mut app, sk, [0.0, 0.0], None);
    assert_eq!(hud_apply(&app, [7.0, -3.0]), [7.0, -3.0]);
    assert_eq!(hud_live(&app, [7.0, -3.0]), [7.0, 3.0]);
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
    // letters are shortcuts, not values
    let (mut app, _) = sketching(Tool::Rect);
    frame(&ctx, vec![egui::Event::Text("l".into())], |ctx| route_typing(&mut app, ctx));
    assert!(app.tstate.hud.buf.is_none());
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
}
