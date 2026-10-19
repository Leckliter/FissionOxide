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
  if app.show_viewcube {
    consumed |= viewcube(app, ui, &painter, &view);
  }
  let step = grid_step(app, &view) / 10.0;
  let (project, ray) = (|p| view.to_screen(&cam, p), |p| view.ray(&cam, p));
  consumed |= crate::manipulators::triad(app, &resp, &painter, &ctx, &project, &ray, step);
  consumed |= manipulator(app, &resp, &painter, &view, &ctx);
  consumed |= crate::select::joint_glyphs(app, &resp, &painter, &project);
  dim_editor(app, &ctx);
  tool_preview(app, &painter, &resp, &view);
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
  if resp.hovered() {
    let mut factor = 1.0;
    if scroll.y.abs() > 0.0 {
      factor *= (-scroll.y as f64 * 0.0015).exp();
    }
    if (zoom - 1.0).abs() > 1e-4 {
      factor /= zoom as f64;
    }
    if (factor - 1.0f64).abs() > 1e-6 {
      let toward = resp.hover_pos().and_then(|p| {
        let (o, d) = view.ray(&app.cam, p);
        let (_, _, back) = app.cam.basis();
        ray_plane(o, d, DVec3::from(app.cam.target), back)
      });
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
    d.unpreview(&mut app.s);
    app.status = Some(("Preview hidden — keep selecting, then Preview or OK".into(), false));
    return;
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
  let pts = &mut app.tstate.pts;
  pts.push(p);
  let n = pts.len();
  let first = pts[0];
  let sid = sk.0;
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
    Tool::Circle if n == 2 => {
      let r = (p[0] - first[0]).hypot(p[1] - first[1]);
      Some(("sketch.circle", json!({"sketch": sid, "center": first, "radius": r})))
    }
    Tool::Arc3 if n == 3 => Some(("sketch.arc", json!({"sketch": sid, "start": first, "end": pts[1], "mid": p}))),
    Tool::CenterArc if n == 3 => {
      Some(("sketch.arc", json!({"sketch": sid, "center": first, "start": pts[1], "end": p})))
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
      let a = [2.0 * first[0] - p[0], 2.0 * first[1] - p[1]];
      Some(("sketch.line", json!({"sketch": sid, "points": [a, p]})))
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
    if app.run(c, params).is_none() && multi {
      // a failed segment drops its point so the chain continues from the last good one
      app.tstate.pts.pop();
    }
  }
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
        | CurveKind::Bezier { .. },
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
  let mut keep = true;
  egui::Area::new(egui::Id::new("dim-editor"))
    .fixed_pos(pos + Vec2::new(8.0, -10.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::popup(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
          let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(110.0).hint_text("value or expression"));
          if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
          }
          let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
          if (r.lost_focus() && enter) || ui.small_button("✔").clicked() {
            keep = false;
            let Some(sk) = app.s.edit_sketch else { return };
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
          if ui.small_button("✖").clicked() {
            keep = false;
            app.tstate.pending_dim = None;
          }
        });
      });
    });
  if keep {
    app.dim_edit = Some((cid, text, pos));
  }
}

// ---------------------------------------------------------------- overlays

/// Dimension labels and constraint glyphs of the sketch being edited. Returns true when a click
/// was consumed.
fn sketch_overlay(app: &mut App, ui: &mut egui::Ui, painter: &egui::Painter, view: &View) -> bool {
  let Some((sk, plane)) = sketch_plane(app) else { return false };
  let Some(sketch) = app.s.model.sketches.get(&sk).map(|r| r.sketch.clone()) else { return false };
  let to_screen = |p: [f64; 2]| view.to_screen(&app.cam, plane.to_world(p));
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
      let Some(at) = to_screen(anchor) else { continue };
      // push linear dimensions off their geometry, away from the sketch's centre
      let at = match dim_span(&sketch, &c.kind).and_then(|(a, b)| Some((to_screen(a)?, to_screen(b)?))) {
        Some((a, b)) => {
          let d = (b - a).normalized();
          let mut n = Vec2::new(-d.y, d.x);
          if let Some(cen) = sketch_center(&sketch).and_then(to_screen)
            && n.dot(at - cen) < 0.0
          {
            n = -n;
          }
          at + n * 18.0
        }
        None => at,
      };
      let galley = painter.layout_no_wrap(label, font.clone(), Color32::from_rgb(20, 20, 30));
      let r = Rect::from_center_size(at, galley.size() + Vec2::new(8.0, 4.0));
      let resp = ui.interact(r, ui.id().with(("dim", c.id.0)), Sense::click());
      let bg = if selected {
        crate::theme::SELECTED
      } else if resp.hovered() {
        crate::theme::HOVER
      } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 170)
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

/// The two points a linear dimension measures between.
fn dim_span(s: &Sketch, k: &ConstraintKind) -> Option<([f64; 2], [f64; 2])> {
  match k {
    ConstraintKind::Distance { a, b, .. }
    | ConstraintKind::HorizontalDistance { a, b, .. }
    | ConstraintKind::VerticalDistance { a, b, .. } => Some((s.pos(*a)?, s.pos(*b)?)),
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
    ConstraintKind::Symmetric { .. } => Icon::Symmetric,
    ConstraintKind::Curvature { .. } => Icon::Curvature,
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
    (Tool::CircPolygon | Tool::EdgePolygon | Tool::TangentArc | Tool::MidpointLine, [a]) => line(*a, cur),
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
    (Tool::CenterArc, [c, a]) => circle(*c, (a[0] - c[0]).hypot(a[1] - c[1])),
    (Tool::Slot, [a, b]) => {
      let w = point_line_dist(cur, *a, *b);
      circle(*a, w);
      circle(*b, w);
      line(*a, *b);
    }
    _ => {}
  }
  // cursor marker and coordinate readout
  if let Some(p) = s(cur) {
    let col = if snapped.is_some() { Color32::from_rgb(230, 120, 20) } else { Color32::from_rgb(30, 110, 235) };
    painter.circle_stroke(p, 4.0, Stroke::new(1.5, col));
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

/// A clickable orientation cube in the top-right corner. Returns true when it took the click.
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
  let pointer = ui.input(|i| i.pointer.hover_pos());
  let clicked = ui.input(|i| i.pointer.primary_clicked());
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
  let menu = Rect::from_center_size(center + Vec2::new(size * 0.55, size * 1.15), Vec2::splat(16.0));
  let menu_hover = pointer.is_some_and(|p| menu.contains(p));
  crate::icons::draw(
    painter,
    menu,
    crate::icons::Icon::Component,
    if menu_hover { crate::theme::ACCENT } else { Color32::from_gray(70) },
  );
  // home button
  let home = Rect::from_center_size(center + Vec2::new(-62.0, -50.0), Vec2::splat(18.0));
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
  let cube_area = Rect::from_center_size(center, Vec2::splat(size * 2.2));
  pointer.is_some_and(|p| cube_area.contains(p) || home.contains(p) || menu.contains(p)) && clicked
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
