//! Copy and Paste, as in Fusion: Ctrl+C copies the selected bodies, component or sketch curves;
//! Ctrl+V pastes a copy and opens the Move dialog with its triad to place it (Initial Position
//! for a component, which gets another occurrence of itself); Ctrl+Shift+V (Paste New) makes an
//! independent copy of a component. Cancel removes the paste. In a sketch, Ctrl+V pastes copies
//! of the copied curves and opens Move/Copy on them.

use fox_engine::Sel;
use fox_engine::model::{BodyId, ComponentId, FeatureId};
use fox_engine::sketch::CurveId;

use crate::dialogs::{Dialog, Kind, Paste};
use crate::{App, Tool};

/// What Ctrl+C copied.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Clip {
  Bodies(Vec<BodyId>),
  /// Every body of a component was selected (as clicking it in the browser, or with Select
  /// Component Priority, selects it).
  Component(ComponentId),
  Curves {
    sketch: FeatureId,
    curves: Vec<CurveId>,
  },
}

/// What the selection would copy: sketch curves of the open sketch, else the selected bodies
/// (a whole component's as the component).
pub(crate) fn clip_of(app: &App) -> Option<Clip> {
  let sel = &app.s.selection;
  if let Some(sketch) = app.s.edit_sketch {
    let curves: Vec<CurveId> = sel
      .iter()
      .filter_map(|x| match x {
        Sel::SketchCurve { sketch: s, curve } if *s == sketch => Some(*curve),
        _ => None,
      })
      .collect();
    if !curves.is_empty() {
      return Some(Clip::Curves { sketch, curves });
    }
  }
  let mut bodies: Vec<BodyId> = Vec::new();
  for x in sel {
    if let Sel::Body { body } = x
      && !bodies.contains(body)
    {
      bodies.push(*body);
    }
  }
  if bodies.is_empty() {
    return None;
  }
  let whole = app
    .s
    .design
    .components
    .iter()
    .find(|c| !c.bodies.is_empty() && c.bodies.len() == bodies.len() && c.bodies.iter().all(|b| bodies.contains(b)));
  Some(match whole {
    Some(c) => Clip::Component(c.id),
    None => Clip::Bodies(bodies),
  })
}

/// Ctrl+C.
pub(crate) fn copy(app: &mut App, ctx: &egui::Context) {
  let Some(clip) = clip_of(app) else {
    app.hint("Copy: select bodies, a component or sketch curves first");
    return;
  };
  let what = match &clip {
    Clip::Bodies(b) if b.len() == 1 => app.s.body_name(b[0]),
    Clip::Bodies(b) => format!("{} bodies", b.len()),
    Clip::Component(c) => app.s.design.component(*c).map(|c| c.name.clone()).unwrap_or_default(),
    Clip::Curves { curves, .. } => format!("{} sketch curves", curves.len()),
  };
  // the system clipboard gets a note too: the window only hears Ctrl+V while it holds something
  ctx.copy_text(format!("FissionOxide: {what}"));
  app.clipboard = Some(clip);
  app.status = Some((format!("Copied {what}"), false));
}

/// Ctrl+V, or Ctrl+Shift+V (`new`, Paste New).
pub(crate) fn paste(app: &mut App, new: bool) {
  let Some(clip) = app.clipboard.clone() else {
    app.hint("Paste: copy (Ctrl+C) bodies, a component or sketch curves first");
    return;
  };
  let bodies = |ids: &[BodyId]| -> Vec<Sel> {
    ids.iter().filter(|b| app.s.model.body(**b).is_some()).map(|b| Sel::Body { body: *b }).collect()
  };
  let (selection, paste, object) = match &clip {
    Clip::Curves { sketch, curves } => {
      if app.s.edit_sketch != Some(*sketch) {
        app.hint("Paste: sketch curves paste into the sketch they were copied from");
        return;
      }
      let sk = app.s.design.sketch(*sketch).ok();
      let sel: Vec<Sel> = curves
        .iter()
        .filter(|c| sk.is_some_and(|sk| sk.curve(**c).is_some()))
        .map(|c| Sel::SketchCurve { sketch: *sketch, curve: *c })
        .collect();
      (sel, Paste::Copy, "sketch")
    }
    _ if app.s.edit_sketch.is_some() => {
      app.hint("Paste: finish the sketch to paste bodies or components");
      return;
    }
    Clip::Bodies(b) => (bodies(b), Paste::Copy, "bodies"),
    Clip::Component(c) => {
      let Some(comp) = app.s.design.component(*c) else {
        app.hint("Paste: the copied component is gone");
        return;
      };
      let sel = bodies(&comp.bodies.clone());
      if new {
        let before = app.s.model.bodies.iter().map(|b| b.id.0).collect();
        (sel, Paste::New { source: c.0, before }, "bodies")
      } else {
        (sel, Paste::Occurrence, "components")
      }
    }
  };
  if selection.is_empty() {
    app.hint("Paste: what was copied is gone");
    return;
  }
  app.end_hint();
  app.cancel_dialog();
  app.tool = Tool::Select;
  app.s.selection = selection;
  let mut d = Dialog::open(app, Kind::Move);
  d.set_paste(&app.s, paste, object);
  d.refresh_manip(&app.s);
  d.preview(&mut app.s);
  app.dialog = Some(d);
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use super::*;

  fn with_box() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    app
  }

  fn body(app: &App) -> BodyId {
    app.s.model.bodies[0].id
  }

  #[test]
  fn pasting_a_body_adds_a_copy_that_cancel_removes() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let b = body(&app);
    app.s.selection = vec![Sel::Body { body: b }];
    copy(&mut app, &ctx);
    assert_eq!(app.clipboard, Some(Clip::Bodies(vec![b])));
    paste(&mut app, false);
    let d = app.dialog.as_ref().expect("Move/Copy opens");
    assert_eq!((d.kind, d.paste()), (Kind::Move, Some(&Paste::Copy)));
    assert!(d.triad(&app.s).is_some(), "with the triad");
    assert_eq!(app.s.model.bodies.len(), 2, "the copy is there");
    app.cancel_dialog();
    assert_eq!(app.s.model.bodies.len(), 1, "Cancel removes it");
  }

  #[test]
  fn pasting_a_component_adds_an_occurrence_paste_new_an_independent_copy() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let b = body(&app);
    app.run("component.create", json!({"name": "Arm", "bodies": [b.0]})).unwrap();
    let arm = app.s.design.components[0].id;
    // the browser selects a component as all its bodies
    app.s.selection = vec![Sel::Body { body: b }];
    copy(&mut app, &ctx);
    assert_eq!(app.clipboard, Some(Clip::Component(arm)));

    // Paste: Initial Position on another occurrence of Arm
    paste(&mut app, false);
    assert_eq!(app.dialog.as_ref().and_then(|d| d.paste()), Some(&Paste::Occurrence));
    assert_eq!(app.s.design.components.len(), 2);
    assert_eq!(app.s.design.components[1].name, "Arm:2");
    assert!(app.dialog.as_ref().unwrap().triad(&app.s).is_some());
    app.cancel_dialog();
    assert_eq!(app.s.design.components.len(), 1, "Cancel removes the paste");

    // Paste New: an independent copy, its own component once placed
    paste(&mut app, true);
    assert!(matches!(app.dialog.as_ref().and_then(|d| d.paste()), Some(Paste::New { .. })));
    assert_eq!(app.s.model.bodies.len(), 2, "the bodies are copied");
    app.cancel_dialog();
    assert_eq!((app.s.model.bodies.len(), app.s.design.components.len()), (1, 1), "Cancel removes it");
    paste(&mut app, true);
    // OK keeps the previewed copy and makes it a component
    let d = app.dialog.take().unwrap();
    d.finish_paste(&mut app);
    let names: Vec<&str> = app.s.design.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Arm", "Arm (1)"]);
    let copy = &app.s.design.components[1];
    assert!(copy.bodies.len() == 1 && copy.bodies[0] != b, "its own body");
    assert!(
      !app.s.design.features.iter().any(|f| matches!(f.kind, fox_engine::model::FeatureKind::CopyComponent { .. })),
      "not rebuilt from Arm"
    );
  }

  #[test]
  fn pasting_sketch_curves_copies_them_in_their_sketch() {
    let ctx = egui::Context::default();
    let mut app = App::with_session(Session::default());
    let sk = app.run("sketch.create", json!({"plane": "xy"})).unwrap()["sketch"].as_u64().unwrap() as u32;
    let r = app.run("sketch.line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let line = r["curves"][0].as_u64().or(r["curve"].as_u64()).unwrap() as u32;
    let sketch = FeatureId(sk);
    app.s.selection = vec![Sel::SketchCurve { sketch, curve: CurveId(line) }];
    copy(&mut app, &ctx);
    assert!(matches!(app.clipboard, Some(Clip::Curves { .. })));
    let count = |app: &App| app.s.design.sketch(sketch).unwrap().curves.len();
    paste(&mut app, false);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), Some(Kind::Move));
    assert_eq!(count(&app), 2, "a copy of the line");
    app.cancel_dialog();
    assert_eq!(count(&app), 1);
  }
}
