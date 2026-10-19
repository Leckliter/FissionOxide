//! The 3D viewport: rendering, camera navigation, picking, sketch tools, dimension labels and the
//! ViewCube.

use clf_engine::Sel;
use clf_engine::kernel::Plane;
use clf_engine::kernel::glam::DVec3;
use clf_engine::model::FeatureId;
use clf_engine::render::{self, StandardView, ray_plane};
use clf_engine::scene::{Pick, ViewOptions};
use clf_engine::sketch::{ConstraintKind, CurveKind, PointId, Sketch};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
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
}

/// Clicks collected by the active sketch tool (sketch coordinates).
#[derive(Default)]
pub struct ToolState {
  pts: Vec<[f64; 2]>,
  /// Dimension tool: first picked entity.
  dim_first: Option<Pick>,
  /// Design before a sketch-point drag started.
  drag_backup: Option<clf_engine::model::Design>,
  /// Dimension command waiting for its value.
  pending_dim: Option<serde_json::Value>,
}

impl ToolState {
  pub fn is_empty(&self) -> bool {
    self.pts.is_empty() && self.dim_first.is_none()
  }
}

pub fn reset_tool_state(app: &mut App) {
  app.tstate.pts.clear();
  app.tstate.dim_first = None;
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
  if let Some(tex) = &app.tex {
    painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
  }

  // overlays that take clicks come first
  let mut consumed = false;
  if app.s.edit_sketch.is_some() {
    consumed |= sketch_overlay(app, ui, &painter, &view);
  }
  if app.show_viewcube {
    consumed |= viewcube(app, ui, &painter, &view);
  }
  dim_editor(app, &ctx);
  tool_preview(app, &painter, &resp, &view);
  hud(app, &painter, &resp, &view);

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
    && let Drag::SketchPoint { sketch, point } = drag
  {
    finish_point_drag(app, sketch, point);
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
    pick_kind(|p| matches!(p, Pick::Edge { .. })).or(exact)
  };
  // while a sketch tool draws, only points matter for snapping
  if sketching && app.tool != Tool::Select && app.tool != Tool::Dimension {
    app.hover = app.hover.filter(|p| matches!(p, Pick::SketchPoint { .. }));
  }
}

fn render_if_needed(app: &mut App, view: &View, ctx: &egui::Context) {
  let profiles_of: Vec<FeatureId> = app.dialog.as_ref().and_then(|d| d.profile_sketch()).into_iter().collect();
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
  };
  if app.render_key.as_ref() == Some(&key) && app.tex.is_some() {
    return;
  }
  let opts = ViewOptions {
    hover: app.hover,
    hide_edges: !app.show_edges,
    hide_grid: !app.show_grid,
    show_profiles_of: profiles_of,
  };
  let (scene, picks) = app.scene.build(&app.s, &opts);
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
  clf_engine::scene::nice_step(world_per_px * 40.0)
}

fn handle_clicks(app: &mut App, resp: &egui::Response, view: &View, ctx: &egui::Context) {
  let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) else { return };
  let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);

  // start dragging a sketch point with the select tool
  if resp.drag_started_by(egui::PointerButton::Primary)
    && app.tool == Tool::Select
    && let Some(Pick::SketchPoint { sketch, point }) = app.hover
    && point != clf_engine::sketch::ORIGIN
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

  if !resp.clicked() {
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
    select_click(app, add);
    return;
  }
  if app.tool == Tool::Dimension {
    dimension_click(app, pos, add);
    return;
  }
  let Some((sk, _)) = sketch_plane(app) else { return };
  let Some((p, snapped)) = sketch_cursor(app, view, pos) else { return };
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

fn point_line_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l = dx.hypot(dy);
  if l < 1e-12 { (p[0] - a[0]).hypot(p[1] - a[1]) } else { ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / l }
}

fn select_click(app: &mut App, add: bool) {
  match app.hover {
    Some(p) => crate::panels::toggle_sel(&mut app.s, p.to_sel(), add),
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
    res.profiles = clf_engine::sketch::find_profiles(&solved);
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
      None => return,
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
  let text = current.map(|v| clf_engine::sketch::Dim::new(v).expr).unwrap_or_default();
  // stash the pending dimension in the editor (constraint id 0 = new)
  app.dim_edit = Some((0, text, pos));
  app.tstate.pending_dim = Some(cmd);
}

fn dist(s: &Sketch, a: PointId, b: PointId) -> Option<f64> {
  let (p, q) = (s.pos(a)?, s.pos(b)?);
  Some((p[0] - q[0]).hypot(p[1] - q[1]))
}

fn line_angle(s: &Sketch, a: clf_engine::sketch::CurveId, b: clf_engine::sketch::CurveId) -> Option<f64> {
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

fn dim_label(s: &Sketch, k: &ConstraintKind, dim: &clf_engine::sketch::Dim) -> Option<([f64; 2], String)> {
  let numeric = dim.expr.trim().parse::<f64>().is_ok();
  let val = if k.is_angle() {
    format!("{}°", clf_engine::sketch::Dim::new(dim.value.to_degrees()).expr)
  } else {
    clf_engine::sketch::Dim::new(dim.value).expr
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
    _ => Icon::Fix,
  }
}

/// Where a geometric constraint's glyph goes, and a key grouping glyphs on the same curve/point.
fn glyph_anchor(s: &Sketch, k: &ConstraintKind) -> Option<([f64; 2], u32)> {
  if let Some(c) = k.curves().first() {
    let curve = s.curve(*c)?;
    let pts = clf_engine::scene::curve_polyline(s, &curve.kind);
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
  if app.tool == Tool::Select || app.tool == Tool::Dimension {
    return;
  }
  let Some(pos) = resp.hover_pos() else { return };
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
    (Tool::Rect, [a]) => poly(&[*a, [cur[0], a[1]], cur, [a[0], cur[1]]], true),
    (Tool::CenterRect, [c]) => {
      let o = [2.0 * c[0] - cur[0], 2.0 * c[1] - cur[1]];
      poly(&[o, [cur[0], o[1]], cur, [o[0], cur[1]]], true);
    }
    (Tool::Circle | Tool::Polygon, [c]) => circle(*c, (cur[0] - c[0]).hypot(cur[1] - c[1])),
    (Tool::Arc3 | Tool::CenterArc | Tool::Slot, [a]) => line(*a, cur),
    (Tool::Arc3, [a, b]) => {
      if let Some(c) = clf_engine::sketch::circumcenter(*a, cur, *b) {
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
    let txt = format!("{}, {}", clf_engine::sketch::Dim::new(cur[0]).expr, clf_engine::sketch::Dim::new(cur[1]).expr);
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
    (true, Tool::Dimension) => "Dimension: click a line, circle or arc — or a point then a point/line",
    (true, Tool::Select) => "Select / drag points · Del deletes · constraints apply to the selection",
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
  let size = 44.0f32;
  let center = view.rect.right_top() + Vec2::new(-70.0, 70.0);
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
  for (facing, pts, label, v) in &visible {
    let shade = (226.0 + 26.0 * *facing as f32).min(252.0) as u8;
    let fill = if hit == Some(*v) { Color32::from_rgb(160, 205, 240) } else { Color32::from_gray(shade) };
    painter.add(egui::Shape::convex_polygon(pts.clone(), fill, Stroke::new(1.0, Color32::from_gray(150))));
    if *facing > 0.3 {
      let c = pts.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2()) / 4.0;
      painter.text(
        c.to_pos2(),
        egui::Align2::CENTER_CENTER,
        *label,
        egui::FontId::proportional(9.5),
        Color32::from_gray(70).gamma_multiply((*facing as f32 * 1.6).min(1.0)),
      );
    }
  }
  // axis triad along the cube edges from the origin corner, as on Fusion's ViewCube
  let corner = DVec3::splat(-1.0);
  let faces_toward = |n: DVec3| n.dot(back) > 0.02;
  for (axis, col, sides) in [
    (DVec3::X, Color32::from_rgb(220, 50, 47), [-DVec3::Y, -DVec3::Z]),
    (DVec3::Y, Color32::from_rgb(60, 170, 60), [-DVec3::X, -DVec3::Z]),
    (DVec3::Z, Color32::from_rgb(40, 110, 220), [-DVec3::X, -DVec3::Y]),
  ] {
    if sides.iter().any(|n| faces_toward(*n)) {
      painter.line_segment([proj(corner), proj(corner + axis * 2.35)], Stroke::new(2.0, col));
    }
  }
  // home button
  let home = Rect::from_center_size(center + Vec2::new(-48.0, -48.0), Vec2::splat(18.0));
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
  pointer.is_some_and(|p| cube_area.contains(p) || home.contains(p)) && clicked
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
