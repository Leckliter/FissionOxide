//! The sketch Fillet, Chamfer and Blend Curve tools, as Fusion runs them: pick-pick tools with no
//! dialog. The cursor prompt asks for the first curve, then the second; the first pick is drawn
//! selected (bold blue) and hovering the second shows a faint red preview of the result. On the
//! second pick the fillet or chamfer is applied at once with the last value, and a floating value
//! box (its value selected, with the ⋮ parameter menu) and a blue arrow out of the corner let it be
//! changed: typing then Enter commits and ends the tool, dragging the arrow changes it live. The
//! prompt meanwhile goes back to the first pick, so more corners can be done. Blend Curve joins
//! the nearest ends with a tangent (G1) or curvature (G2) continuous spline; the Sketch Palette's
//! Feature Options row chooses which, and the chamfer's kind.

use egui::{Color32, Pos2, Stroke};
use fox_engine::Sel;
use fox_engine::model::FeatureId;
use fox_engine::scene::Pick;
use fox_engine::sketch::{CurveId, CurveKind, PointId, Sketch};
use serde_json::{Value, json};

use crate::viewport::{self, Drag, Tool, View};
use crate::{App, value_box};

/// The tools' last values, which the next use starts from (Fusion keeps them for the session).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
  /// Fillet radius (a length expression).
  pub radius: String,
  /// Chamfer distance, the first of two, or the one with the angle.
  pub distance: String,
  /// Two Distance Chamfer's second distance.
  pub distance2: String,
  /// Distance and Angle Chamfer's angle.
  pub angle: String,
  /// Blend Curve: curvature continuous (G2) rather than tangent (G1).
  pub blend_g2: bool,
}

impl Default for Prefs {
  fn default() -> Self {
    Prefs {
      radius: "2 mm".into(),
      distance: "2 mm".into(),
      distance2: "2 mm".into(),
      angle: "45 deg".into(),
      blend_g2: false,
    }
  }
}

/// A fillet or chamfer just applied, while its value box and arrow show.
#[derive(Debug, Clone)]
pub struct Applied {
  /// The corner the fillet or chamfer took off (sketch coordinates): the arrow starts there.
  corner: [f64; 2],
  /// Unit direction (sketch coordinates) the arrow points: out of the corner, away from the shape.
  dir: [f64; 2],
  cmd: &'static str,
  /// The command's parameters, less the value the box edits.
  params: Value,
  /// The parameter the value box edits ("radius" or "distance").
  field: &'static str,
  /// The value box's text.
  pub text: String,
  /// The text last applied (a failed change goes back to it).
  applied_text: String,
  /// The applied value (mm), the arrow's length.
  value: f64,
  value_box: value_box::State,
  /// The step in the undo history that is this fillet or chamfer.
  history: &'static str,
}

/// Faint red: Fusion's preview of the arc, chamfer or blend the second pick would make.
const PREVIEW: Color32 = Color32::from_rgba_premultiplied(190, 50, 40, 120);

fn is_modify_tool(tool: Tool) -> bool {
  tool == Tool::Fillet || tool == Tool::Blend || tool.is_chamfer()
}

/// A click of one of the tools: `at` is where it landed on the sketch, `hovered` the curve under
/// the pointer, `hover` the pick there (a sketch point can start a chamfer at its vertex).
/// Returns true when the click applied the tool (made its result).
pub(crate) fn click(app: &mut App, sk: FeatureId, at: [f64; 2], hovered: Option<u32>, hover: Option<Pick>) -> bool {
  // the last fillet or chamfer stays as it is; its box closes
  if app.tstate.applied.is_some() {
    finish(app);
  }
  // a chamfer can start at a vertex: the corner two lines share
  if app.tool.is_chamfer()
    && app.tstate.curves.is_empty()
    && let Some(Pick::SketchPoint { sketch, point }) = hover
    && sketch == sk
    && let Some([a, b]) = corner_lines(app, sk, point)
  {
    apply(app, sk, a, b);
    return true;
  }
  let Some(c) = hovered else { return false };
  if app.tstate.curves.iter().any(|x| x.0 == c) || !accepts(app, sk, c) {
    return false;
  }
  app.tstate.curves.push((c, at));
  // the first pick is drawn selected, as Fusion draws it
  let sel = Sel::SketchCurve { sketch: sk, curve: CurveId(c) };
  if !app.s.selection.contains(&sel) {
    app.s.selection.push(sel);
  }
  if let [a, b] = app.tstate.picked_curves()[..] {
    apply(app, sk, a, b);
    return true;
  }
  false
}

/// The two lines that meet at `point`, for a chamfer started at a vertex.
fn corner_lines(app: &App, sk: FeatureId, point: PointId) -> Option<[u32; 2]> {
  let sketch = app.s.design.sketch(sk).ok()?;
  let lines: Vec<u32> = sketch
    .curves
    .iter()
    .filter(|c| matches!(c.kind, CurveKind::Line { a, b } if a == point || b == point))
    .map(|c| c.id.0)
    .collect();
  match lines[..] {
    [a, b] => Some([a, b]),
    _ => None,
  }
}

/// Does the tool take curve `c`? Fillet lines and arcs, a chamfer lines, Blend Curve anything
/// with ends.
fn accepts(app: &App, sk: FeatureId, c: u32) -> bool {
  let Some(kind) = app.s.design.sketch(sk).ok().and_then(|s| s.curve(CurveId(c))).map(|c| &c.kind) else {
    return false;
  };
  match app.tool {
    Tool::Fillet => matches!(kind, CurveKind::Line { .. } | CurveKind::Arc { .. }),
    Tool::Blend => !matches!(kind, CurveKind::Circle { .. } | CurveKind::Ellipse { .. }),
    _ => matches!(kind, CurveKind::Line { .. }),
  }
}

/// Takes the tool's picks out of the selection (they were drawn selected).
pub(crate) fn unselect_picks(app: &mut App) {
  let Some(sk) = app.s.edit_sketch else { return };
  let picked = app.tstate.picked_curves();
  if picked.is_empty() {
    return;
  }
  app
    .s
    .selection
    .retain(|s| !matches!(s, Sel::SketchCurve { sketch, curve } if *sketch == sk && picked.contains(&curve.0)));
}

/// The command a tool runs on curves `a` and `b`, with the parameters it starts from (the value
/// box's field and its text, and the history step's name): None for Blend Curve, which has none.
fn command(
  app: &App,
  sk: FeatureId,
  a: u32,
  b: u32,
) -> (&'static str, Value, Option<(&'static str, String, &'static str)>) {
  let p = &app.modify;
  let mut params = json!({"sketch": sk.0, "curves": [a, b]});
  match app.tool {
    Tool::Fillet => ("sketch.fillet", params, Some(("radius", p.radius.clone(), "Fillet"))),
    Tool::Blend => {
      params["continuity"] = json!(if p.blend_g2 { "g2" } else { "g1" });
      ("sketch.blend", params, None)
    }
    Tool::ChamferAngle => {
      params["angle"] = json!(p.angle);
      ("sketch.chamfer", params, Some(("distance", p.distance.clone(), "Chamfer")))
    }
    Tool::Chamfer2 => {
      params["distance2"] = json!(p.distance2);
      ("sketch.chamfer", params, Some(("distance", p.distance.clone(), "Chamfer")))
    }
    _ => ("sketch.chamfer", params, Some(("distance", p.distance.clone(), "Chamfer"))),
  }
}

/// Both curves picked: applies the tool at once, with the last value, and (fillet, chamfer)
/// shows the value box and arrow for it. The picks are done with either way.
fn apply(app: &mut App, sk: FeatureId, a: u32, b: u32) {
  unselect_picks(app);
  app.tstate.curves.clear();
  let (cmd, mut params, boxed) = command(app, sk, a, b);
  let Some((field, text, history)) = boxed else {
    app.run(cmd, params);
    return;
  };
  let corner = app.s.design.sketch(sk).ok().and_then(|s| corner_of(s, CurveId(a), CurveId(b)));
  params[field] = json!(text);
  let Some(result) = app.run(cmd, params.clone()) else { return };
  if result.get("error").is_some() {
    return;
  }
  if let Some(o) = params.as_object_mut() {
    o.remove(field);
  }
  let made = result["curves"].as_array().and_then(|v| v.first()).and_then(Value::as_u64).map(|c| CurveId(c as u32));
  let (corner, dir) = arrow_of(app, sk, corner, made);
  let value = length(app, &text).unwrap_or(0.0);
  app.tstate.applied = Some(Applied {
    corner,
    dir,
    cmd,
    params,
    field,
    text: text.clone(),
    applied_text: text,
    value,
    value_box: value_box::State::default(),
    history,
  });
}

/// The corner two curves make: the end they share, else where two lines (extended) cross.
fn corner_of(s: &Sketch, a: CurveId, b: CurveId) -> Option<[f64; 2]> {
  let (pa, pb) = (s.curve_points(a), s.curve_points(b));
  let ends = |pts: &[PointId], kind: Option<&CurveKind>| -> Vec<PointId> {
    match kind {
      // an arc's first point is its centre
      Some(CurveKind::Arc { .. }) => pts.iter().skip(1).copied().collect(),
      _ => pts.to_vec(),
    }
  };
  let (ea, eb) = (ends(&pa, s.curve(a).map(|c| &c.kind)), ends(&pb, s.curve(b).map(|c| &c.kind)));
  if let Some(p) = ea.iter().find(|p| eb.contains(p)) {
    return s.pos(*p);
  }
  let line = |pts: &[PointId]| -> Option<([f64; 2], [f64; 2])> { Some((s.pos(*pts.first()?)?, s.pos(*pts.get(1)?)?)) };
  let ((a0, a1), (b0, b1)) = (line(&ea)?, line(&eb)?);
  let (u, v) = ([a1[0] - a0[0], a1[1] - a0[1]], [b1[0] - b0[0], b1[1] - b0[1]]);
  let den = u[0] * v[1] - u[1] * v[0];
  if den.abs() < 1e-12 {
    return None;
  }
  let w = [b0[0] - a0[0], b0[1] - a0[1]];
  let t = (w[0] * v[1] - w[1] * v[0]) / den;
  Some([a0[0] + u[0] * t, a0[1] + u[1] * t])
}

/// Where the applied fillet's or chamfer's arrow starts and points: from the corner it took off
/// (else the middle of what it made), away from what it made.
fn arrow_of(app: &App, sk: FeatureId, corner: Option<[f64; 2]>, made: Option<CurveId>) -> ([f64; 2], [f64; 2]) {
  let mid = app.s.design.sketch(sk).ok().and_then(|s| {
    let pts = fox_engine::scene::curve_polyline(s, &s.curve(made?)?.kind);
    pts.get(pts.len() / 2).copied()
  });
  let corner = corner.or(mid).unwrap_or([0.0, 0.0]);
  let dir = mid.and_then(|m| {
    let v = [corner[0] - m[0], corner[1] - m[1]];
    let l = v[0].hypot(v[1]);
    (l > 1e-9).then(|| [v[0] / l, v[1] / l])
  });
  (corner, dir.unwrap_or([1.0, 0.0]))
}

/// A length expression in mm, with the design's parameters.
fn length(app: &App, text: &str) -> Option<f64> {
  fox_engine::expr::eval(text, fox_engine::expr::Kind::Length, &app.s.model.params).ok()
}

/// Ends the value phase: a value typed in the box but not applied yet is applied now, and the
/// box and arrow close. The fillet or chamfer stays.
pub(crate) fn finish(app: &mut App) {
  let Some(applied) = app.tstate.applied.as_ref() else { return };
  if applied.text != applied.applied_text {
    let text = applied.text.clone();
    reapply(app, &text);
  }
  if let Some(a) = app.tstate.applied.take() {
    remember(app, a.field, &a.applied_text);
  }
}

/// The value last applied becomes the tool's next default.
fn remember(app: &mut App, field: &str, text: &str) {
  match (app.tool, field) {
    (Tool::Fillet, _) => app.modify.radius = text.to_string(),
    (_, "distance") => app.modify.distance = text.to_string(),
    _ => {}
  }
}

/// Applies the fillet or chamfer again with `text` as its value, in place of the one applied (one
/// undo step either way). A value that doesn't work (a radius too big for the lines) leaves the
/// last one applied and its error in the status bar.
fn reapply(app: &mut App, text: &str) -> bool {
  let Some(a) = app.tstate.applied.clone() else { return false };
  if app.s.undo_history().first().map(String::as_str) != Some(a.history) {
    return false;
  }
  app.s.undo();
  let run = |app: &mut App, text: &str| -> bool {
    let mut params = a.params.clone();
    params[a.field] = json!(text);
    app.run(a.cmd, params).is_some_and(|r| r.get("error").is_none())
  };
  if run(app, text) {
    let value = length(app, text);
    if let Some(applied) = app.tstate.applied.as_mut() {
      applied.applied_text = text.to_string();
      applied.text = text.to_string();
      applied.value = value.unwrap_or(applied.value);
    }
    return true;
  }
  let error = app.status.take();
  run(app, &a.applied_text);
  if error.is_some() {
    app.status = error;
  }
  if let Some(applied) = app.tstate.applied.as_mut() {
    applied.text = a.applied_text.clone();
  }
  false
}

/// The Feature Options row switched the chamfer's kind: the line already picked carries over.
pub(crate) fn switch_chamfer(app: &mut App, tool: Tool) {
  finish(app);
  app.tool = tool;
}

/// The blue arrow out of the corner and the floating value box of the fillet or chamfer just
/// applied. Dragging the arrow's head changes the value live; Enter in the box commits what was
/// typed and ends the tool; Escape ends it keeping the value applied. Returns true while the
/// arrow has the pointer.
pub(crate) fn manipulator(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  view: &View,
  ctx: &egui::Context,
) -> bool {
  if app.tstate.applied.is_none() || !is_modify_tool(app.tool) {
    app.tstate.applied = None;
    return false;
  }
  let Some((_, plane)) = viewport::sketch_plane(app) else { return false };
  let Some(a) = app.tstate.applied.as_ref() else { return false };
  let origin = plane.to_world(a.corner);
  let dir = (plane.to_world([a.corner[0] + a.dir[0], a.corner[1] + a.dir[1]]) - origin).normalize_or_zero();
  let tip = origin + dir * a.value.max(0.0);
  let (Some(pa), Some(pb)) = (view.to_screen(&app.cam, origin), view.to_screen(&app.cam, tip)) else { return false };
  let hot = resp.hover_pos().is_some_and(|p| (p - pb).length() < 12.0);
  let active = matches!(app.drag, Some(Drag::ToolManip));
  let label_at = viewport::paint_arrow(painter, pa, pb, hot || active);
  let params = value_box::params_of(&app.s);
  let mut done = false;
  let mut typed: Option<String> = None;
  if let Some(a) = app.tstate.applied.as_mut() {
    let shown = value_box::show(ctx, &mut a.value_box, label_at, &mut a.text, &params);
    if shown.enter {
      typed = Some(a.text.clone());
      done = true;
      ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    } else if shown.escape {
      done = true;
    }
  }
  if done {
    if let Some(t) = typed {
      reapply(app, &t);
    }
    finish(app);
    app.set_tool(Tool::Select);
    return false;
  }
  let pressed_on_head = ctx.input(|i| i.pointer.press_origin()).is_some_and(|p| (p - pb).length() < 14.0);
  if pressed_on_head && resp.drag_started_by(egui::PointerButton::Primary) {
    app.drag = Some(Drag::ToolManip);
  }
  if matches!(app.drag, Some(Drag::ToolManip)) {
    if let Some(p) = resp.interact_pointer_pos() {
      let (o, d) = view.ray(&app.cam, p);
      if let Some(t) = viewport::axis_param(origin, dir, o, d) {
        let step = app.increments.linear_step(viewport::grid_step(app, view) / 10.0);
        // a radius or distance can't reach zero: the smallest step is as far as it goes
        let v = if step > 0.0 { (t / step).round() * step } else { t }.max(step.max(1e-3));
        let text = app.s.design.units.format((v * 1e4).round() / 1e4);
        if app.tstate.applied.as_ref().is_some_and(|a| a.applied_text != text) {
          reapply(app, &text);
        }
      }
    }
    ctx.request_repaint();
    return true;
  }
  hot || pressed_on_head && ctx.input(|i| i.pointer.primary_down())
}

/// Hovering the second curve: a faint red preview of the arc, chamfer or blend the click would
/// make, as Fusion shows it.
pub(crate) fn preview(app: &App, painter: &egui::Painter, resp: &egui::Response, view: &View) {
  if !is_modify_tool(app.tool) || app.drag.is_some() {
    return;
  }
  let Some((sk, plane)) = viewport::sketch_plane(app) else { return };
  let [first] = app.tstate.picked_curves()[..] else { return };
  let Some(Pick::SketchCurve { sketch, curve }) = app.hover else { return };
  if sketch != sk || curve.0 == first || !accepts(app, sk, curve.0) {
    return;
  }
  let Some(pts) = preview_curve(app, sk, first, curve.0) else { return };
  let screen: Vec<Pos2> = pts.iter().filter_map(|p| view.to_screen(&app.cam, plane.to_world(*p))).collect();
  if screen.len() >= 2 && resp.contains_pointer() {
    painter.add(egui::Shape::line(screen, Stroke::new(2.0, PREVIEW)));
  }
}

/// The curve the tool would make on `a` and `b` with its current values, on a copy of the sketch.
fn preview_curve(app: &App, sk: FeatureId, a: u32, b: u32) -> Option<Vec<[f64; 2]>> {
  let mut s = app.s.design.sketch(sk).ok()?.clone();
  let (a, b) = (CurveId(a), CurveId(b));
  let p = &app.modify;
  let made = match app.tool {
    Tool::Fillet => s.fillet(a, b, length(app, &p.radius)?),
    Tool::Blend => s.blend(a, b, None, p.blend_g2),
    Tool::ChamferAngle => {
      let angle = fox_engine::expr::eval(&p.angle, fox_engine::expr::Kind::Angle, &app.s.model.params).ok()?;
      s.chamfer_angle(a, b, length(app, &p.distance)?, angle)
    }
    Tool::Chamfer2 => s.chamfer(a, b, length(app, &p.distance)?, length(app, &p.distance2)?),
    _ => {
      let d = length(app, &p.distance)?;
      s.chamfer(a, b, d, d)
    }
  }
  .ok()?;
  Some(fox_engine::scene::curve_polyline(&s, &s.curve(made)?.kind))
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;
  use fox_engine::sketch::ConstraintKind;

  fn sketching(tool: Tool) -> (App, FeatureId) {
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    app.set_tool(tool);
    (app, sk)
  }

  fn curves(r: &Value) -> Vec<u32> {
    r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect()
  }

  fn sketch(app: &App, sk: FeatureId) -> &Sketch {
    app.s.design.sketch(sk).unwrap()
  }

  fn dims(app: &App, sk: FeatureId) -> Vec<(&'static str, f64)> {
    sketch(app, sk).constraints.iter().filter_map(|c| c.kind.dim().map(|d| (c.kind.name(), d.value))).collect()
  }

  /// A click on curve `c` at `at`, as the viewport hands it over (the curve is under the pointer).
  fn pick(app: &mut App, sk: FeatureId, c: u32, at: [f64; 2]) {
    let hover = Some(Pick::SketchCurve { sketch: sk, curve: CurveId(c) });
    click(app, sk, at, Some(c), hover);
  }

  #[test]
  fn fillet_applies_on_the_second_pick_with_the_last_radius_and_keeps_prompting() {
    let (mut app, sk) = sketching(Tool::Fillet);
    let r = app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]})).unwrap();
    let lines = curves(&r);
    app.modify.radius = "3 mm".into();
    pick(&mut app, sk, lines[0], [20.0, 0.0]);
    // the first pick is selected (drawn bold blue) and the prompt asks for the second
    assert_eq!(app.s.selection, vec![Sel::SketchCurve { sketch: sk, curve: CurveId(lines[0]) }]);
    assert_eq!(viewport::sketch_prompt(Tool::Fillet, &app.tstate), Some("Select second line or arc"));
    assert!(app.tstate.applied.is_none(), "nothing applied yet");
    pick(&mut app, sk, lines[1], [40.0, 10.0]);
    // applied at once: an arc with its radius dimension, two tangents, the box and arrow showing
    assert_eq!(dims(&app, sk), vec![("Radius", 3.0)]);
    assert_eq!(
      sketch(&app, sk).constraints.iter().filter(|c| matches!(c.kind, ConstraintKind::Tangent { .. })).count(),
      2
    );
    let a = app.tstate.applied.as_ref().expect("the value box shows");
    assert_eq!(a.text, "3 mm");
    assert_eq!(a.corner, [40.0, 0.0], "the arrow starts at the corner the fillet took off");
    assert!(a.dir[0] > 0.0 && a.dir[1] < 0.0, "and points out of it: {:?}", a.dir);
    assert!(app.s.selection.is_empty(), "the picks are no longer selected");
    assert_eq!(viewport::sketch_prompt(Tool::Fillet, &app.tstate), Some("Select first line or arc"));
    assert_eq!(app.s.undo_history().first().map(String::as_str), Some("Fillet"));
    // typing a new radius and committing replaces the fillet's value in the same undo step
    let steps = app.s.undo_history().len();
    app.tstate.applied.as_mut().unwrap().text = "5".into();
    finish(&mut app);
    assert_eq!(dims(&app, sk), vec![("Radius", 5.0)]);
    assert_eq!(app.s.undo_history().len(), steps);
    assert!(app.tstate.applied.is_none());
    assert_eq!(app.modify.radius, "5", "the next fillet starts from it");
    // a value that doesn't fit leaves the last one applied, with the error in the status bar
    let r = app.run("sketch.rect", json!({"corner1": [100, 0], "corner2": [120, 10]})).unwrap();
    let small = curves(&r);
    pick(&mut app, sk, small[0], [110.0, 0.0]);
    pick(&mut app, sk, small[1], [120.0, 5.0]);
    assert!(app.tstate.applied.is_some());
    assert!(!reapply(&mut app, "40 mm"), "too big");
    assert!(app.status.as_ref().is_some_and(|(_, err)| *err));
    assert_eq!(app.tstate.applied.as_ref().unwrap().applied_text, "5");
    assert_eq!(dims(&app, sk).len(), 2);
  }

  #[test]
  fn a_new_first_pick_keeps_the_applied_fillet_and_escape_ends_the_tool() {
    let (mut app, sk) = sketching(Tool::Fillet);
    let lines = curves(&app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]})).unwrap());
    pick(&mut app, sk, lines[0], [20.0, 0.0]);
    pick(&mut app, sk, lines[1], [40.0, 10.0]);
    assert!(app.tstate.applied.is_some());
    // the next corner: the box closes, the first fillet stays, the pick is the new first pick
    pick(&mut app, sk, lines[2], [20.0, 20.0]);
    assert!(app.tstate.applied.is_none());
    assert_eq!(dims(&app, sk).len(), 1);
    assert_eq!(app.tstate.picked_curves(), vec![lines[2]]);
    pick(&mut app, sk, lines[3], [0.0, 10.0]);
    assert_eq!(dims(&app, sk).len(), 2);
    // Esc: the fillet stays, the box closes, the tool ends
    finish(&mut app);
    assert!(app.tstate.applied.is_none() && app.tstate.is_empty());
    assert_eq!(dims(&app, sk).len(), 2);
    // a circle isn't a fillet pick
    let circle = curves(&app.run("sketch.circle", json!({"center": [80, 80], "radius": 5})).unwrap())[0];
    pick(&mut app, sk, circle, [85.0, 80.0]);
    assert!(app.tstate.curves.is_empty());
  }

  #[test]
  fn chamfer_applies_with_its_two_distance_dimensions_and_warns_of_the_removed_coincidence() {
    let (mut app, sk) = sketching(Tool::Chamfer);
    let lines = curves(&app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]})).unwrap());
    app.modify.distance = "2.5 mm".into();
    pick(&mut app, sk, lines[0], [20.0, 0.0]);
    assert_eq!(viewport::sketch_prompt(Tool::Chamfer, &app.tstate), Some("Select a second line"));
    pick(&mut app, sk, lines[1], [40.0, 10.0]);
    assert_eq!(dims(&app, sk), vec![("Chamfer distance", 2.5), ("Chamfer distance", 2.5)]);
    assert_eq!(
      app.status.as_ref().map(|s| s.0.as_str()),
      Some("Constraints and/or dimensions were removed during operation.")
    );
    assert_eq!(app.tstate.applied.as_ref().unwrap().text, "2.5 mm");
    // the arrow drag's live value: the chamfer is re-applied in the same undo step
    let steps = app.s.undo_history().len();
    assert!(reapply(&mut app, "4 mm"));
    assert_eq!(dims(&app, sk), vec![("Chamfer distance", 4.0), ("Chamfer distance", 4.0)]);
    assert_eq!(app.s.undo_history().len(), steps);
    finish(&mut app);
    assert_eq!(app.modify.distance, "4 mm");
    // a vertex click chamfers the corner its two lines share
    pick(&mut app, sk, lines[2], [20.0, 20.0]);
    app.set_tool(Tool::Chamfer);
    assert!(app.tstate.curves.is_empty() && app.s.selection.is_empty(), "a tool change drops the pick");
    let corner = sketch(&app, sk).points.iter().find(|p| p.x.abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9).unwrap().id;
    click(&mut app, sk, [0.0, 20.0], None, Some(Pick::SketchPoint { sketch: sk, point: corner }));
    assert_eq!(dims(&app, sk).len(), 4);
    assert!(app.tstate.applied.is_some());
  }

  #[test]
  fn chamfer_kinds_take_their_angle_or_second_distance_and_switch_mid_pick() {
    let (mut app, sk) = sketching(Tool::Chamfer2);
    let lines = curves(&app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]})).unwrap());
    app.modify.distance = "2 mm".into();
    app.modify.distance2 = "3 mm".into();
    pick(&mut app, sk, lines[0], [20.0, 0.0]);
    // the Feature Options row switches the kind without losing the pick
    switch_chamfer(&mut app, Tool::ChamferAngle);
    assert_eq!(app.tstate.picked_curves(), vec![lines[0]]);
    switch_chamfer(&mut app, Tool::Chamfer2);
    pick(&mut app, sk, lines[1], [40.0, 10.0]);
    assert_eq!(dims(&app, sk), vec![("Chamfer distance", 2.0), ("Chamfer distance", 3.0)]);
    finish(&mut app);
    app.set_tool(Tool::ChamferAngle);
    app.modify.angle = "30 deg".into();
    pick(&mut app, sk, lines[2], [20.0, 20.0]);
    pick(&mut app, sk, lines[3], [0.0, 10.0]);
    let d = dims(&app, sk);
    assert_eq!(d.len(), 4);
    assert!((d[2].1 - 2.0).abs() < 1e-9);
    // 2 along the first line at 30°: the second leg is 2·sin30/sin60 (a dimension's value is
    // rounded as its expression is)
    assert!((d[3].1 - 2.0 * 30f64.to_radians().sin() / 60f64.to_radians().sin()).abs() < 1e-4, "{d:?}");
  }

  /// The prober's case: a vertical line picked near its bottom and a diagonal picked near its top,
  /// whose top ends are the nearest pair. Fusion joins the nearest ends, whatever was clicked.
  #[test]
  fn blend_joins_the_nearest_ends_with_the_palettes_continuity() {
    let (mut app, sk) = sketching(Tool::Blend);
    let v = curves(&app.run("sketch.line", json!({"points": [[0, 0], [0, 30]]})).unwrap())[0];
    let d = curves(&app.run("sketch.line", json!({"points": [[40, 0], [20, 25]]})).unwrap())[0];
    pick(&mut app, sk, v, [0.0, 2.0]);
    assert_eq!(viewport::sketch_prompt(Tool::Blend, &app.tstate), Some("Select second curve or edge"));
    pick(&mut app, sk, d, [22.0, 22.0]);
    assert!(app.tstate.applied.is_none(), "a blend has no value box");
    let s = sketch(&app, sk);
    let blend = s.curves.iter().find(|c| matches!(c.kind, CurveKind::QuinticSpline { .. })).expect("a blend curve");
    let pts = s.curve_points(blend.id);
    let (start, end) = (s.pos(pts[0]).unwrap(), s.pos(*pts.last().unwrap()).unwrap());
    assert_eq!(start, [0.0, 30.0], "the vertical line's top end");
    assert_eq!(end, [20.0, 25.0], "the diagonal's top end");
    assert_eq!(s.constraints.iter().filter(|c| matches!(c.kind, ConstraintKind::Tangent { .. })).count(), 2, "G1");
    assert_eq!(viewport::sketch_prompt(Tool::Blend, &app.tstate), Some("Select first curve or edge"));
    // G2 from the Feature Options row
    app.modify.blend_g2 = true;
    let e = curves(&app.run("sketch.line", json!({"points": [[100, 0], [100, 30]]})).unwrap())[0];
    let f = curves(&app.run("sketch.line", json!({"points": [[140, 0], [120, 25]]})).unwrap())[0];
    pick(&mut app, sk, e, [100.0, 2.0]);
    pick(&mut app, sk, f, [122.0, 22.0]);
    let s = sketch(&app, sk);
    assert_eq!(s.constraints.iter().filter(|c| matches!(c.kind, ConstraintKind::Curvature { .. })).count(), 2, "G2");
  }

  #[test]
  fn the_second_pick_is_previewed() {
    let (mut app, sk) = sketching(Tool::Fillet);
    let lines = curves(&app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]})).unwrap());
    pick(&mut app, sk, lines[0], [20.0, 0.0]);
    let pts = preview_curve(&app, sk, lines[0], lines[1]).expect("an arc");
    assert!(pts.len() > 2);
    // the arc lies inside the corner, 2 mm from each line
    assert!(pts.iter().all(|p| p[0] <= 40.0 + 1e-9 && p[1] >= -1e-9), "{pts:?}");
    assert_eq!(dims(&app, sk).len(), 0, "the preview changes nothing");
    app.set_tool(Tool::Blend);
    let v = curves(&app.run("sketch.line", json!({"points": [[60, 0], [60, 30]]})).unwrap())[0];
    let d = curves(&app.run("sketch.line", json!({"points": [[100, 0], [80, 25]]})).unwrap())[0];
    let pts = preview_curve(&app, sk, v, d).expect("a blend");
    assert_eq!(pts.first().copied(), Some([60.0, 30.0]));
  }
}
