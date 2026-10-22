//! Dialogs whose only model picks are bodies (Combine's target and tools, Split Body's bodies to
//! split, Scale's entities): while one is open a click picks the whole body, as in Fusion, so the
//! body lights blue on hover and pick and the status bar names it ("Body1"), not the face under
//! the pointer.

use crate::App;
use crate::select::Priority;

use super::{Dialog, Kind};

impl Dialog {
  /// Does every viewport pick this dialog takes go to a body chip?
  pub(super) fn picks_bodies(&self) -> bool {
    match self.kind {
      Kind::Combine | Kind::SplitBody => true,
      Kind::Form(f) => std::ptr::eq(f, &crate::forms::SCALE),
      _ => false,
    }
  }

  /// Picks whole bodies while a body chip is active (the selection priority is put back when the
  /// dialog closes, see [`Dialog::leave`]).
  pub(super) fn body_pick_priority(&mut self, app: &mut App) {
    if !self.picks_bodies() || self.editing.is_some() {
      return;
    }
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
    }
    app.sel_priority = Priority::Body;
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::scene::Pick;
  use fox_engine::{Sel, Session};
  use serde_json::json;

  use super::super::{Dialog, Kind};
  use crate::App;
  use crate::select::Priority;

  /// Two 20 mm boxes, Body1 at the origin and Body2 at X 40, as the prober's design.
  fn two_boxes() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
    app.run("box", json!({"width": 20, "depth": 20, "height": 20, "at": [40, 0]})).unwrap();
    app.s.selection.clear();
    app
  }

  fn a_face(app: &App, i: usize) -> Pick {
    let b = &app.s.model.bodies[i];
    Pick::Face { body: b.id, face: b.faces()[0].id }
  }

  /// Combine, Split Body and Scale pick the body under the pointer, not its face: the hover
  /// lights the whole body, the pick is the body, the status bar names the pick, and the dialog takes
  /// it. The selection priority is put back when the dialog closes.
  #[test]
  fn body_chips_pick_whole_bodies() {
    for kind in [Kind::Combine, Kind::SplitBody, Kind::Form(&crate::forms::SCALE)] {
      let mut app = two_boxes();
      let (b1, b2) = (app.s.model.bodies[0].id, app.s.model.bodies[1].id);
      let mut d = Dialog::open(&mut app, kind);
      assert!(d.picks_bodies());
      d.body_pick_priority(&mut app);
      assert_eq!(app.sel_priority, Priority::Body, "{kind:?}");
      app.hover = Some(a_face(&app, 0));
      assert_eq!(crate::select::hover_bodies(&app), vec![b1], "{kind:?}: the hover lights the body");
      assert_eq!(crate::panels::selection_text(&app), None, "a hover reads nothing (ui-status-readouts)");
      let p = a_face(&app, 0);
      crate::select::pick(&mut app, p, true);
      assert_eq!(app.s.selection, vec![Sel::Body { body: b1 }], "{kind:?}");
      assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("Body1"));
      d.sync_selection(&app.s);
      let taken =
        if d.kind == Kind::Combine || d.kind == Kind::SplitBody { json!(d.bodies) } else { d.form.values[0].clone() };
      assert_eq!(taken, json!([b1.0]), "{kind:?}: the dialog takes the body");
      let p = a_face(&app, 1);
      crate::select::pick(&mut app, p, true);
      d.sync_selection(&app.s);
      if kind == Kind::Combine {
        assert_eq!(d.bodies, vec![b1.0, b2.0], "target, then a tool");
      }
      d.cancel(&mut app.s);
      d.leave(&mut app);
      assert_eq!(app.sel_priority, Priority::Face, "{kind:?}: put back on close");
    }
    // a dialog that picks faces leaves the priority alone
    let mut app = two_boxes();
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::OFFSET_FACE));
    d.body_pick_priority(&mut app);
    assert_eq!(app.sel_priority, Priority::Face);
  }

  /// While Combine's preview shows, the browser keeps every body row (the consumed tool body's
  /// too, under its name); rows change only after OK, and without a preview the browser lists
  /// the model's bodies.
  #[test]
  fn a_combine_preview_leaves_the_browser_rows() {
    let mut app = two_boxes();
    let (b1, b2) = (app.s.model.bodies[0].id, app.s.model.bodies[1].id);
    let mut d = Dialog::open(&mut app, Kind::Combine);
    assert!(d.browser_bodies(&app.s).is_none(), "no preview: the model's bodies");
    app.s.selection = vec![Sel::Body { body: b1 }, Sel::Body { body: b2 }];
    d.sync_selection(&app.s);
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(app.s.model.body(b2).is_none(), "the preview consumes Body2");
    let rows = d.browser_bodies(&app.s).unwrap();
    assert_eq!(rows, vec![(b1, "Body1".to_string()), (b2, "Body2".to_string())]);
    d.cancel(&mut app.s);
    assert!(d.browser_bodies(&app.s).is_none());
    assert!(app.s.model.body(b2).is_some());
  }
}
