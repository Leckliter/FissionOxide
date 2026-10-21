//! Dragging components in the viewport, as in Fusion: press on an ungrounded component's body
//! with the select tool and drag. A jointed component moves within its joint's freedoms, an
//! unjointed one in the view plane, and contact sets stop it where their bodies touch (see the
//! engine's `component.drag`). Each step replaces the last, so the drag is one undo.

use egui::Pos2;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::ComponentId;
use fox_engine::scene::Pick;
use serde_json::json;

use crate::App;
use crate::viewport::{Drag, Tool};

/// A component being dragged.
pub struct CompDrag {
  component: ComponentId,
  /// The grabbed point, where it was when the drag started.
  from: DVec3,
  /// Is a drag step applied (one undo takes it back)?
  applied: bool,
  /// Where the last step was worked out for.
  last: Option<Pos2>,
  contact: bool,
  error: Option<String>,
}

/// The component (and the grabbed point) under a press: the nearest body hit, if it is in a
/// component that isn't grounded.
pub(crate) fn grab(app: &App, o: DVec3, d: DVec3) -> Option<(ComponentId, DVec3)> {
  let (t, pick) = fox_engine::scene::ray_hits(&app.s, o, d).into_iter().next()?;
  let body = match pick {
    Pick::Face { body, .. } | Pick::Edge { body, .. } | Pick::Vertex { body, .. } => body,
    _ => return None,
  };
  let c = app.s.design.component_of(body)?;
  let mut at = Some(c);
  for _ in 0..64 {
    let Some(k) = at else { break };
    if k.grounded {
      return None;
    }
    at = k.parent.and_then(|p| app.s.design.component(p));
  }
  Some((c.id, o + d * t))
}

/// Starts, steps and keeps a component drag; true while one is on (the viewport takes no other
/// clicks then).
pub(crate) fn handle(
  app: &mut App,
  resp: &egui::Response,
  ctx: &egui::Context,
  pos: Pos2,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> bool {
  if resp.drag_started_by(egui::PointerButton::Primary)
    && app.drag.is_none()
    && app.tool == Tool::Select
    && app.dialog.is_none()
    && app.s.edit_sketch.is_none()
    && !ctx.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.alt)
  {
    let press = ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
    let (o, d) = ray(press);
    if let Some((component, from)) = grab(app, o, d) {
      app.comp_drag = Some(CompDrag { component, from, applied: false, last: None, contact: false, error: None });
      app.drag = Some(Drag::Component);
    }
  }
  if !matches!(app.drag, Some(Drag::Component)) {
    return false;
  }
  if resp.dragged_by(egui::PointerButton::Primary) {
    step(app, pos, ray(pos));
  }
  true
}

/// Moves the dragged component toward the cursor's ray `(o, d)`.
pub(crate) fn step(app: &mut App, pos: Pos2, (o, d): (DVec3, DVec3)) {
  let Some(drag) = app.comp_drag.as_mut() else { return };
  if drag.last.is_some_and(|l| (l - pos).length() < 0.5) {
    return;
  }
  drag.last = Some(pos);
  let (c, from, applied) = (drag.component, drag.from, drag.applied);
  if applied {
    let _ = app.s.execute("undo", &json!({}));
  }
  let r = app.s.execute(
    "component.drag",
    &json!({"component": c.0, "from": from.to_array(), "to": o.to_array(), "view": d.to_array()}),
  );
  let Some(drag) = app.comp_drag.as_mut() else { return };
  match r {
    Ok(v) => {
      drag.applied = true;
      drag.contact = v.get("contact").and_then(serde_json::Value::as_bool).unwrap_or(false);
      drag.error = v.get("error").and_then(serde_json::Value::as_str).map(str::to_string);
    }
    Err(e) => {
      drag.applied = false;
      drag.error = Some(e.0);
    }
  }
}

/// The drag ended: what it stopped at goes to the status bar.
pub(crate) fn finish(app: &mut App) {
  let Some(drag) = app.comp_drag.take() else { return };
  let name = app.s.design.component(drag.component).map(|c| c.name.clone()).unwrap_or_default();
  if let Some(e) = drag.error {
    app.status = Some((e, true));
  } else if drag.contact {
    app.status = Some((format!("{name} stopped where it touches (contact set)"), false));
  }
}
