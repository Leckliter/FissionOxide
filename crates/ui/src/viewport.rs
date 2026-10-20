//! The 3D viewport: rendering, camera navigation, picking, sketch tools, dimension labels and the
//! ViewCube.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use fox_engine::Sel;
use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::FeatureId;
use fox_engine::render::{self, StandardView, ray_plane};
use fox_engine::scene::{Pick, ViewOptions};
use fox_engine::sketch::{ConstraintKind, CurveKind, PointId, Sketch};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::{App, RenderKey};

/// Screen-space snap radius for sketch points (points, logical pixels).
const SNAP_PX: f32 = 9.0;
/// Lines within this angle of horizontal/vertical snap to it (degrees).
const HV_SNAP_DEG: f64 = 2.5;
/// Max rendered pixels (keeps software rendering interactive on HiDPI screens).
const MAX_PIXELS: f32 = 3_000_000.0;

/// Navigation bar mode: while set, a left-drag orbits, pans or zooms instead of selecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
  Orbit,
  Pan,
  Zoom,
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
}

/// The heads-up boxes of `tool` once it has `n` points.
fn hud_fields(tool: Tool, n: usize) -> &'static [Field] {
  match (tool, n) {
    (Tool::Line, 1..) => &[Field::Length, Field::Angle],
    (Tool::Rect | Tool::CenterRect, 1) => &[Field::Width, Field::Height],
    (Tool::Circle, 1) => &[Field::Diameter],
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

  navigate(app, &resp, &view, &ctx);
  update_hover(app, &resp, &view);
  let planes = plane_pick(app, &resp, &view);
  render_if_needed(app, &view, &ctx);

  let painter = ui.painter_at(rect);
  let gpu_drawn = crate::gpu::paint(app, &painter, rect, [view.w, view.h], &ctx);
  if !gpu_drawn && let Some(tex) = &app.tex {
    painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
  }

  let cam = app.cam;
  crate::trace::overlay(app, &painter, &|p| view.to_screen(&cam, p));
  crate::preview::overlay(app, &painter, &|p| view.to_screen(&cam, p));

  // overlays that take clicks come first
  let mut consumed = false;
  if app.s.edit_sketch.is_some() {
    consumed |= sketch_overlay(app, ui, &painter, &view);
  }
  let hot_plane = sketch_planes_overlay(app, planes, &resp, &painter, &view);
  if app.show_viewcube {
    consumed |= viewcube(app, ui, &painter, &view);
  }
  if !consumed && let Some(plane) = hot_plane {
    consumed = true;
    if resp.clicked() {
      app.cancel_dialog();
      app.create_sketch(plane);
    }
  }
  let step = grid_step(app, &view) / 10.0;
  let (project, ray) = (|p| view.to_screen(&cam, p), |p| view.ray(&cam, p));
  consumed |= crate::manipulators::triad(app, &resp, &painter, &ctx, &project, &ray, step);
  consumed |= manipulator(app, &resp, &painter, &view, &ctx);
  consumed |= crate::select::joint_glyphs(app, &resp, &painter, &project);
  dim_editor(app, &ctx);
  track_arc_wind(app, &resp, &view);
  tool_preview(app, &painter, &resp, &view);
  heads_up(app, &painter, &resp, &view, &ctx);
  measure_overlay(app, &painter, &view);
  hud(app, &painter, &resp, &view);
  if let Some(Drag::Box { start, now, .. }) = app.drag {
    let r = Rect::from_two_pos(start, now);
    // left to right: window (blue, solid); right to left: crossing (green)
    let col = if now.x >= start.x { Color32::from_rgb(40, 120, 230) } else { Color32::from_rgb(40, 170, 90) };
    painter.rect(r, 0.0, col.gamma_multiply(0.12), Stroke::new(1.0, col), egui::StrokeKind::Inside);
  }
  crate::select::region_overlay(app, &painter);
  crate::select::show_other(app, &ctx);

  // right-click without dragging opens the marking menu; while it's open it owns the clicks
  if resp.secondary_clicked()
    && let Some(p) = resp.interact_pointer_pos()
  {
    crate::marking::open(app, p);
  } else if !consumed && app.nav_mode.is_none() && app.marking.is_none() {
    handle_clicks(app, &resp, &view, &ctx);
  }
}

// ---------------------------------------------------------------- camera

fn navigate(app: &mut App, resp: &egui::Response, view: &View, ctx: &egui::Context) {
  let (shift, scroll, zoom) = ctx.input(|i| (i.modifiers.shift, i.smooth_scroll_delta(), i.zoom_delta()));
  // the canvas still zooms with the pointer over its own overlays (the ViewCube), not over
  // panels and dialogs drawn above it
  if ctx.rect_contains_pointer(resp.layer_id, resp.rect) {
    let mut factor = 1.0;
    if scroll.y.abs() > 0.0 {
      factor *= (-scroll.y as f64 * 0.0015).exp();
    }
    if (zoom - 1.0).abs() > 1e-4 {
      factor /= zoom as f64;
    }
    if (factor - 1.0f64).abs() > 1e-6 {
      let toward = ctx.pointer_latest_pos().and_then(|p| {
        let (o, d) = view.ray(&app.cam, p);
        let (_, _, back) = app.cam.basis();
        ray_plane(o, d, DVec3::from(app.cam.target), back)
      });
      app.stop_cam_move();
      app.cam.zoom(factor, toward);
    }
  }
  if resp.drag_started_by(egui::PointerButton::Secondary) {
    app.drag = Some(if shift { Drag::Pan } else { Drag::Orbit });
  }
  if resp.drag_started_by(egui::PointerButton::Middle) {
    app.drag = Some(if shift { Drag::Orbit } else { Drag::Pan });
  }
  if let Some(mode) = app.nav_mode
    && resp.drag_started_by(egui::PointerButton::Primary)
  {
    app.drag = Some(match mode {
      NavMode::Orbit => Drag::Orbit,
      NavMode::Pan => Drag::Pan,
      NavMode::Zoom => Drag::Zoom,
    });
  }
  let d = resp.drag_delta();
  if d != Vec2::ZERO && matches!(app.drag, Some(Drag::Orbit | Drag::Pan | Drag::Zoom)) {
    app.stop_cam_move();
  }
  match app.drag {
    Some(Drag::Orbit) => app.cam.orbit(-d.x as f64 * 0.008, d.y as f64 * 0.008),
    Some(Drag::Pan) => app.cam.pan(d.x as f64 * view.ppp as f64, d.y as f64 * view.ppp as f64, view.h as f64),
    Some(Drag::Zoom) => app.cam.zoom((d.y as f64 * 0.006).exp(), None),
    _ => {}
  }
  if resp.drag_stopped()
    && let Some(drag) = app.drag.take()
  {
    match drag {
      Drag::SketchPoint { sketch, point } => finish_point_drag(app, sketch, point),
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
      _ => {}
    }
  }
}

// ---------------------------------------------------------------- picking

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
    let ok = |p: &Pick| filter.allows(p, priority);
    if !app.hover.as_ref().is_some_and(ok) {
      // the front-most pick is filtered out: the nearest allowed one around the cursor wins
      app.hover = picks.get(frame.pick(x, y, r, |id| picks.get(id).is_some_and(|p| ok(&p)))).filter(ok);
    }
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
    show_edges: app.show_edges,
    show_grid: app.show_grid,
    visible: app.s.design.bodies.iter().map(|b| (b.body.0, b.visible)).collect(),
    profiles_of: profiles_of.clone(),
    view: view_params(app),
    vertices: app.sel_filter.vertices && app.s.edit_sketch.is_none(),
    peek: app.ctx_menu.peek.iter().map(|b| b.0).collect(),
    isolate: app.ctx_menu.isolate.as_ref().map(|v| v.iter().map(|b| b.0).collect()),
  };
  if crate::gpu::active(app) && !crate::gpu::scene_changed(app, &key) {
    return;
  }
  if app.render_key.as_ref() == Some(&key) && app.tex.is_some() {
    return;
  }
  let analysis = match fox_engine::scene::view_options(&app.s, &key.view) {
    Ok(o) => o,
    Err(e) => {
      app.status = Some((e, true));
      ViewOptions::default()
    }
  };
  let opts = ViewOptions {
    hover: app.hover,
    hide_edges: !app.show_edges,
    hide_grid: !app.show_grid,
    show_profiles_of: profiles_of,
    show_vertices: key.vertices,
    peek: app.ctx_menu.peek.clone(),
    only_bodies: app.ctx_menu.isolate.clone().or(analysis.only_bodies.clone()),
    ..analysis
  };
  let (scene, picks) = app.scene.build(&app.s, &opts);
  if crate::gpu::active(app) {
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
  if let Some(sec) = &app.section {
    v["section"] = sec.clone();
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
  let (sk, plane) = sketch_plane(app)?;
  // snap to the hovered point (exact coordinates)
  if let Some(Pick::SketchPoint { sketch, point }) = app.hover
    && sketch == sk
    && let Some(p) = app.s.design.sketch(sk).ok().and_then(|s| s.pos(point))
  {
    return Some((p, Some(point)));
  }
  let (o, d) = view.ray(&app.cam, pos);
  let hit = ray_plane(o, d, plane.origin, plane.normal())?;
  let mut p = plane.to_local(hit);
  // H/V inference against the previous click
  if let Some(prev) = app.tstate.pts.last() {
    let (dx, dy) = (p[0] - prev[0], p[1] - prev[1]);
    let ang = dy.atan2(dx).to_degrees().abs();
    if ang < HV_SNAP_DEG || (180.0 - ang) < HV_SNAP_DEG {
      p[1] = prev[1];
    } else if (ang - 90.0).abs() < HV_SNAP_DEG {
      p[0] = prev[0];
    }
  }
  // tidy coordinates: round to a precision that matches the zoom (a quarter grid cell with Snap)
  let step = grid_step(app, view) / if app.snap { 4.0 } else { 100.0 };
  p = [round_to(p[0], step), round_to(p[1], step)];
  Some((p, None))
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
  // window selection with the select tool
  if resp.drag_started_by(egui::PointerButton::Primary) && app.drag.is_none() && app.tool == Tool::Select {
    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
    let since = ctx.input(|i| i.pointer.press_start_time()).unwrap_or(0.0);
    app.drag = Some(Drag::Box { start, now: pos, since });
  }
  if let Some(Drag::Box { now, .. }) = &mut app.drag {
    *now = pos;
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
  if !sketching || app.tool == Tool::Select {
    // with a command dialog open, picks add to (or toggle out of) its selection, as in Fusion
    select_click(app, add || app.dialog.is_some());
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
      Some(("sketch.polygon", json!({"sketch": sid, "center": first, "vertex": p, "sides": 6})))
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
    Tool::CircPolygon if n == 2 => {
      Some(("sketch.polygon", json!({"sketch": sid, "center": first, "vertex": p, "sides": 6, "circumscribed": true})))
    }
    Tool::EdgePolygon if n == 2 => Some(("sketch.polygon", json!({"sketch": sid, "edge": [first, p], "sides": 6}))),
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
    Tool::Ellipse if n == 3 => {
      let minor = point_line_dist(p, first, pts[1]);
      Some(("sketch.ellipse", json!({"sketch": sid, "center": first, "major": pts[1], "minor": minor.max(0.01)})))
    }
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
    .map(|(i, f)| (*f, app.tstate.hud.typed[i].trim().to_string()))
    .collect()
}

/// The value typed in heads-up box `i` (mm, or radians for an angle), if any.
fn hud_value(app: &App, i: usize) -> Option<f64> {
  let field = *hud_fields(app.tool, app.tstate.pts.len()).get(i)?;
  let text = app.tstate.hud.typed.get(i)?.trim();
  if text.is_empty() {
    return None;
  }
  let kind = if field == Field::Angle { fox_engine::expr::Kind::Angle } else { fox_engine::expr::Kind::Length };
  let v = fox_engine::expr::eval(text, kind, &app.s.model.params).ok()?;
  (field == Field::Angle || v > 1e-9).then_some(v)
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
    _ => [0.0; 2],
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
  for (field, text) in locks {
    // rectangles are drawn bottom, right, top, left: the bottom sets the width, the right the height
    let curve = match field {
      Field::Width | Field::Length => curves.first(),
      Field::Height => curves.get(1),
      Field::Angle | Field::Diameter => None,
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
      let continuity = if shift { "g1" } else { "g2" };
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
          app.status = Some(("Pick a second line for an angle".into(), false));
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
        | CurveKind::Conic { .. },
      )
      | None => return,
    },
    (None, Some(Pick::SketchPoint { .. })) => {
      app.tstate.dim_first = hover;
      app.status = Some(("Pick a second point or a line".into(), false));
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
  let text = current.map(|v| fox_engine::sketch::Dim::new(v).expr).unwrap_or_default();
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
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0).hint_text("value or expression"));
        if fresh {
          focus_text(ctx, &r, &text, true);
        } else if !r.has_focus() && !r.lost_focus() {
          r.request_focus();
        }
        // Enter commits and Esc cancels, as in Fusion (no buttons)
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        cancel = esc;
        commit = enter && !esc && (r.has_focus() || r.lost_focus());
      });
    });
  if commit {
    // the Enter is this box's, not the open dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    if let Some(sk) = app.s.edit_sketch {
      if cid == 0 {
        if let Some(mut cmd) = app.tstate.pending_dim.take() {
          cmd["sketch"] = json!(sk.0);
          cmd["value"] = json!(text);
          app.run("sketch.dimension", cmd);
        }
      } else {
        app.run("sketch.set_dimension", json!({"sketch": sk.0, "constraint": cid, "value": text}));
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
      let Some((anchor, label)) = dim_label(&sketch, &c.kind, dim) else { continue };
      let galley = painter.layout_no_wrap(label, font.clone(), DIM_INK);
      let size = galley.size() + Vec2::new(8.0, 4.0);
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
      painter.galley(r.min + Vec2::new(4.0, 2.0), galley, Color32::BLACK);
      if resp.double_clicked() || (resp.clicked() && app.tool == Tool::Dimension) {
        app.dim_edit = Some((c.id.0, dim.expr.clone(), at));
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
      // circles are dimensioned at 45°, arcs at their middle
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

fn dim_label(s: &Sketch, k: &ConstraintKind, dim: &fox_engine::sketch::Dim) -> Option<([f64; 2], String)> {
  let numeric = dim.expr.trim().parse::<f64>().is_ok();
  let val = if k.is_angle() {
    format!("{}°", fox_engine::sketch::Dim::new(dim.value.to_degrees()).expr)
  } else {
    fox_engine::sketch::Dim::new(dim.value).expr
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
    (Tool::Line, [.., last]) => line(*last, cur),
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
    (Tool::CircPolygon | Tool::EdgePolygon | Tool::TangentArc, [a]) => line(*a, cur),
    (Tool::OverallSlot | Tool::CenterSlot | Tool::Ellipse, [a]) => line(*a, cur),
    (Tool::OverallSlot | Tool::CenterSlot | Tool::Ellipse, [a, b]) => {
      line(*a, *b);
      let w = point_line_dist(cur, *a, *b);
      if app.tool == Tool::Ellipse {
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
    (Tool::Circle | Tool::Polygon, [c]) => circle(*c, (cur[0] - c[0]).hypot(cur[1] - c[1])),
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
  // cursor marker, and a coordinate readout where no heads-up dimensions show
  if let Some(p) = s(cur) {
    let col = if snapped.is_some() { Color32::from_rgb(230, 120, 20) } else { Color32::from_rgb(30, 110, 235) };
    painter.circle_stroke(p, 4.0, Stroke::new(1.5, col));
    if !hud_fields(app.tool, pts.len()).is_empty() {
      return;
    }
    let txt = format!("{}, {}", fox_engine::sketch::Dim::new(cur[0]).expr, fox_engine::sketch::Dim::new(cur[1]).expr);
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

/// The heads-up dimensions of the shape being drawn: each value's dimension follows the cursor
/// with its box. Typing sets the active box, Tab moves to the next, Enter places the point.
fn heads_up(app: &mut App, painter: &egui::Painter, resp: &egui::Response, view: &View, ctx: &egui::Context) {
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
      (Tool::Line, Field::Length) => {
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
      _ => None,
    };
    if let Some(s) = &shape {
      paint_dim(painter, s, ink);
    }
    at.push(shape.map(|s| s.label));
  }
  let fmt = |i: usize| {
    let v = fox_engine::sketch::Dim::new(live[i]).expr;
    if fields[i] == Field::Angle { format!("{v}°") } else { v }
  };
  let n = fields.len();
  let hud = &mut app.tstate.hud;
  hud.active = hud.active.min(n - 1);
  let (tab, shift_tab) = ctx.input(|i| {
    (i.key_pressed(egui::Key::Tab) && !i.modifiers.shift, i.key_pressed(egui::Key::Tab) && i.modifiers.shift)
  });
  let mut commit = false;
  // Tab with no box focused starts typing in the next box, with its value selected
  if hud.buf.is_none() && (tab || shift_tab) && !ctx.egui_wants_keyboard_input() && n > 1 {
    hud.active = (hud.active + 1) % n;
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
          let r = ui.add(
            egui::TextEdit::singleline(&mut buf)
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
          if r.has_focus() && (tab || shift_tab) && n > 1 {
            // Tab keeps what was typed and moves on
            let next = (i + 1) % n;
            hud.active = next;
            buf = if hud.typed[next].is_empty() { fmt(next) } else { hud.typed[next].clone() };
            hud.focus = Some(true);
          } else if enter && (r.has_focus() || r.lost_focus()) {
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
    // the other boxes show the live value, or what was typed (locked) in bold
    let text = if typed { hud.typed[i].clone() } else { fmt(i) };
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text, font, if typed { Color32::BLACK } else { Color32::from_gray(60) });
    let r = Rect::from_center_size(p, galley.size() + Vec2::new(10.0, 5.0));
    let active = i == hud.active;
    painter.rect(
      r,
      2.0,
      Color32::from_rgba_unmultiplied(255, 255, 255, 235),
      Stroke::new(if active || typed { 1.5 } else { 1.0 }, if active || typed { ink } else { Color32::from_gray(160) }),
      egui::StrokeKind::Inside,
    );
    painter.galley(r.center() - galley.size() / 2.0, galley, Color32::BLACK);
  }
  if commit {
    // this Enter places the point; it isn't the open dialog's OK
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    place_point(app, sk, raw, snapped);
  }
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
    let txt = format!("R {}", fox_engine::sketch::Dim::new(r).expr);
    painter.text(
      p + Vec2::new(10.0, 10.0),
      egui::Align2::LEFT_TOP,
      txt,
      egui::FontId::proportional(11.0),
      Color32::from_gray(70),
    );
  }
}

/// Hints in the corner of the viewport.
fn hud(app: &App, painter: &egui::Painter, _resp: &egui::Response, view: &View) {
  let hint = match (app.s.edit_sketch.is_some(), app.tool) {
    (true, Tool::Line) => "Line: click points · click the first point to close · Esc to end",
    (true, Tool::Rect) => "Rectangle: click two corners",
    (true, Tool::CenterRect) => "Centre rectangle: click the centre, then a corner",
    (true, Tool::Circle) => "Circle: click the centre, then a point on the circle",
    (true, Tool::Arc3) => "3-point arc: start, end, then a point on the arc",
    (true, Tool::CenterArc) => "Centre arc: centre, start, end",
    (true, Tool::Polygon) => "Polygon: centre, then a vertex",
    (true, Tool::Slot) => "Slot: first centre, second centre, then the width",
    (true, Tool::Point) => "Point: click to place",
    (true, Tool::Spline) => "Spline: click fit points · click the last point again or Enter to finish",
    (true, Tool::Trim) => "Trim: click the piece of a curve to remove",
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
    (true, Tool::Ellipse) => "Ellipse: centre, end of the major axis, then a point on the minor axis",
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
    (true, Tool::Blend) => "Blend curve: click near the ends of two curves · Shift+click for G1 (tangent only)",
    (false, _) => return,
  };
  if app.status.is_some() {
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

/// A clickable orientation cube in the top-right corner. Returns true when it took the click or
/// is being dragged.
fn viewcube(app: &mut App, ui: &mut egui::Ui, painter: &egui::Painter, view: &View) -> bool {
  let size = 50.0f32;
  let center = view.rect.right_top() + Vec2::new(-92.0, 80.0);
  let (right, up, back) = app.cam.basis();
  let proj = |v: DVec3| center + Vec2::new(v.dot(right) as f32, -v.dot(up) as f32) * size * 0.62;
  let faces: [(DVec3, &str, StandardView); 6] = [
    (DVec3::Z, "TOP", StandardView::Top),
    (-DVec3::Z, "BOTTOM", StandardView::Bottom),
    (-DVec3::Y, "FRONT", StandardView::Front),
    (DVec3::Y, "BACK", StandardView::Back),
    (DVec3::X, "RIGHT", StandardView::Right),
    (-DVec3::X, "LEFT", StandardView::Left),
  ];
  let mut visible: Vec<(f64, Vec<Pos2>, &str, StandardView)> = Vec::new();
  for (n, label, v) in faces {
    let facing = n.dot(back);
    if facing <= 0.02 {
      continue;
    }
    // face corners
    let (a, b) = if n.z.abs() > 0.5 {
      (DVec3::X, DVec3::Y)
    } else if n.x.abs() > 0.5 {
      (DVec3::Y, DVec3::Z)
    } else {
      (DVec3::X, DVec3::Z)
    };
    let corners = [n + a + b, n - a + b, n - a - b, n + a - b].map(proj);
    visible.push((facing, corners.to_vec(), label, v));
  }
  visible.sort_by(|x, y| x.0.total_cmp(&y.0));
  // the cube with its Home and menu buttons is a widget like any other, so a dialog, menu or panel
  // drawn over it takes the pointer instead
  let menu = Rect::from_center_size(center + Vec2::new(size * 0.55, size * 1.15), Vec2::splat(16.0));
  let home = Rect::from_center_size(center + Vec2::new(-62.0, -50.0), Vec2::splat(18.0));
  let cube_area = Rect::from_center_size(center, Vec2::splat(size * 2.2));
  let resp = ui.interact(cube_area.union(home).union(menu), ui.id().with("viewcube"), Sense::click_and_drag());
  // dragging the cube orbits, as on Fusion's
  if resp.dragged_by(egui::PointerButton::Primary) {
    let d = resp.drag_delta();
    app.cam.orbit(-d.x as f64 * 0.008, d.y as f64 * 0.008);
  }
  let pointer = if resp.dragged() { None } else { resp.hover_pos() };
  let clicked = resp.clicked();
  let mut hit: Option<StandardView> = None;
  for (facing, pts, _, v) in &visible {
    if *facing > 0.0 && pointer.is_some_and(|p| point_in_quad(p, pts)) {
      hit = Some(*v);
    }
  }
  // lettered axis stubs leave the cube from its silhouette, as on Fusion's ViewCube; the ones
  // pointing away from the viewer go behind the cube
  let sgn = |bit: usize| if bit != 0 { 1.0 } else { -1.0 };
  let axes = [
    (DVec3::X, "X", Color32::from_rgb(222, 40, 36)),
    (DVec3::Y, "Y", Color32::from_rgb(40, 170, 50)),
    (DVec3::Z, "Z", Color32::from_rgb(30, 70, 220)),
  ];
  let stub = |painter: &egui::Painter, axis: DVec3, label: &str, col: Color32| {
    // the silhouette corner furthest along the axis's on-screen direction
    let d = proj(axis) - proj(DVec3::ZERO);
    let base = (0..8)
      .map(|i| DVec3::new(sgn(i & 1), sgn(i & 2), sgn(i & 4)))
      .max_by(|a, b| {
        ((proj(*a) - center).dot(d) + a.dot(axis) as f32).total_cmp(&((proj(*b) - center).dot(d) + b.dot(axis) as f32))
      })
      .unwrap_or(DVec3::ZERO);
    painter.line_segment([proj(base), proj(base + axis * 0.55)], Stroke::new(2.5, col));
    painter.text(proj(base + axis * 0.85), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), col);
  };
  for (axis, label, col) in axes {
    if axis.dot(back) < 0.0 {
      stub(painter, axis, label, col);
    }
  }
  // soft shadow under the cube
  painter.add(egui::Shape::ellipse_filled(
    center + Vec2::new(4.0, size * 0.95),
    Vec2::new(size * 0.8, size * 0.16),
    Color32::from_black_alpha(28),
  ));
  for (facing, pts, label, v) in &visible {
    // grey, lit from above-left like Fusion's cube
    let n = match v {
      StandardView::Top => DVec3::Z,
      StandardView::Bottom => -DVec3::Z,
      StandardView::Front => -DVec3::Y,
      StandardView::Back => DVec3::Y,
      StandardView::Right => DVec3::X,
      _ => -DVec3::X,
    };
    let light = (n.dot(up) * 0.6 - n.dot(right) * 0.25 + 0.55).clamp(0.0, 1.0) as f32;
    let shade = (196.0 + 48.0 * light) as u8;
    let fill = if hit == Some(*v) { Color32::from_rgb(170, 208, 240) } else { Color32::from_gray(shade) };
    painter.add(egui::Shape::convex_polygon(pts.clone(), fill, Stroke::new(1.0, Color32::from_gray(140))));
    if *facing > 0.25 {
      // the label lies in the face: rotate it along the face's horizontal
      let h = match v {
        StandardView::Top | StandardView::Bottom | StandardView::Front => DVec3::X,
        StandardView::Back => -DVec3::X,
        StandardView::Right => DVec3::Y,
        _ => -DVec3::Y,
      };
      let dir = proj(n + h) - proj(n);
      let angle = dir.y.atan2(dir.x);
      let c = pts.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2()) / 4.0;
      let color = Color32::from_gray(60).gamma_multiply((*facing as f32 * 1.5).min(1.0));
      let galley = painter.layout_no_wrap(label.to_string(), egui::FontId::proportional(10.5), color);
      let half = galley.size() / 2.0;
      let rot = egui::emath::Rot2::from_angle(angle);
      let pos = c.to_pos2() - rot * half;
      painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(angle));
    }
  }
  for (axis, label, col) in axes {
    if axis.dot(back) >= 0.0 {
      stub(painter, axis, label, col);
    }
  }
  // the ViewCube menu button under the cube
  let menu_hover = pointer.is_some_and(|p| menu.contains(p));
  crate::icons::draw(
    painter,
    menu,
    crate::icons::Icon::Component,
    if menu_hover { crate::theme::ACCENT } else { Color32::from_gray(70) },
  );
  // home button
  let home_hover = pointer.is_some_and(|p| home.contains(p));
  let hc = if home_hover { crate::theme::ACCENT } else { Color32::from_gray(90) };
  let hs = Stroke::new(1.5, hc);
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
    hs,
  ));
  if clicked {
    if menu_hover {
      app.cam.ortho = !app.cam.ortho;
      app.status = Some((format!("Camera: {}", if app.cam.ortho { "Orthographic" } else { "Perspective" }), false));
      return true;
    }
    if home_hover {
      app.cam.set_view(StandardView::Home);
      app.fit_view();
      return true;
    }
    if let Some(v) = hit {
      app.cam.set_view(v);
      return true;
    }
  }
  clicked || resp.dragged()
}

fn point_in_quad(p: Pos2, q: &[Pos2]) -> bool {
  let mut inside = false;
  let n = q.len();
  for i in 0..n {
    let (a, b) = (q[i], q[(i + 1) % n]);
    if (a.y > p.y) != (b.y > p.y) {
      let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
      if p.x < x {
        inside = !inside;
      }
    }
  }
  inside
}

// ---------------------------------------------------------------- sketch plane picking

/// Fusion's orange for the origin planes offered by Create Sketch.
const PLANE_RGB: [u8; 3] = [236, 150, 52];

/// The planes Create Sketch offers in the viewport, as (`sketch.create` plane, plane): the
/// origin planes and the construction planes.
fn sketch_planes(app: &App) -> Vec<(serde_json::Value, Plane)> {
  let mut out = vec![(json!("xy"), Plane::XY), (json!("xz"), Plane::XZ), (json!("yz"), Plane::YZ)];
  for (id, pl) in &app.s.model.planes {
    if !app.s.design.feature(*id).is_some_and(|f| f.suppressed) {
      out.push((json!({"feature": id.0}), *pl));
    }
  }
  out
}

/// Half the side of a plane's square: the size the scene draws construction planes at.
fn plane_half_size(app: &App) -> f64 {
  app.s.model.bounds().map(|(a, b)| (b - a).length()).unwrap_or(100.0).max(50.0) * 0.35
}

/// Which of `planes` (index) the ray through `pos` hits first, if nothing nearer (a body face)
/// is in the way.
fn plane_under(app: &App, view: &View, pos: Pos2, planes: &[(serde_json::Value, Plane)], half: f64) -> Option<usize> {
  let (o, d) = view.ray(&app.cam, pos);
  let mut best: Option<(f64, usize)> = None;
  for (i, (_, pl)) in planes.iter().enumerate() {
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
  half: f64,
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
  if !picking && (app.picked_plane.is_none() || app.s.edit_sketch.is_some()) {
    return None;
  }
  let mut planes = sketch_planes(app);
  if !picking {
    planes.retain(|(p, _)| Some(p) == app.picked_plane.as_ref());
  }
  let half = plane_half_size(app);
  let hot = if picking && app.drag.is_none() {
    resp.hover_pos().and_then(|p| plane_under(app, view, p, &planes, half))
  } else {
    None
  };
  if hot.is_some() {
    app.hover = None;
  }
  Some(PlanePick { planes, half, picking, hot })
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
  let PlanePick { planes, half, picking, hot } = pick?;
  let [r, g, b] = PLANE_RGB;
  let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
  for (i, (_, pl)) in planes.iter().enumerate() {
    let Some(pts) = corners.iter().map(|c| view.to_screen(&app.cam, pl.to_world(*c))).collect::<Option<Vec<Pos2>>>()
    else {
      continue;
    };
    let lit = hot == Some(i) || !picking;
    let fill = Color32::from_rgba_unmultiplied(r, g, b, if lit { 110 } else { 46 });
    let edge = Color32::from_rgba_unmultiplied(r, g, b, if lit { 255 } else { 170 });
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(if lit { 2.0 } else { 1.0 }, edge)));
  }
  if picking {
    resp.clone().on_hover_text_at_pointer(crate::dialogs::PICK_PLANE_HINT);
  }
  planes.into_iter().nth(hot?).map(|(p, _)| p)
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

/// The open dialog's drag arrow: drawn from its base along its direction to the current value;
/// dragging the head changes the value (snapped to a tenth of the grid). Returns true while the
/// arrow has the pointer.
fn manipulator(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  view: &View,
  ctx: &egui::Context,
) -> bool {
  let Some(m) = app.dialog.as_ref().and_then(|d| d.manipulator(&app.s)) else { return false };
  let tip = m.origin + m.dir * m.value;
  let (Some(a), Some(b)) = (view.to_screen(&app.cam, m.origin), view.to_screen(&app.cam, tip)) else { return false };
  let hot = resp.hover_pos().is_some_and(|p| (p - b).length() < 12.0);
  let active = matches!(app.drag, Some(Drag::Manip));
  let col = if hot || active { Color32::from_rgb(255, 140, 20) } else { Color32::from_rgb(30, 110, 235) };
  painter.line_segment([a, b], Stroke::new(2.0, col));
  let dir2 = (b - a).normalized();
  let dir2 = if dir2.is_finite() && (b - a).length() > 1.0 { dir2 } else { Vec2::new(0.0, -1.0) };
  let side = Vec2::new(-dir2.y, dir2.x);
  let head = [b + dir2 * 12.0, b + side * 6.0, b - side * 6.0];
  painter.add(egui::Shape::convex_polygon(head.to_vec(), col, Stroke::NONE));
  painter.text(
    b + side * 14.0 + dir2 * 4.0,
    egui::Align2::LEFT_CENTER,
    format!("{} mm", fox_engine::sketch::Dim::new(m.value).expr),
    egui::FontId::proportional(12.0),
    Color32::from_gray(40),
  );
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
        let step = grid_step(app, view) / 10.0;
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
}
