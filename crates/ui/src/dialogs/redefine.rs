//! Redefine Sketch Plane, from a sketch row's menu, as Fusion's: one Plane row whose Select chip
//! takes a planar face or a work plane. While it is open the timeline is rolled back to the
//! sketch (what comes after leaves the browser and the canvas) and the sketch shows on the picked
//! plane; OK puts the timeline back and moves the sketch in one undo step, Cancel puts back the
//! sketch and the timeline as they were.

use fox_engine::model::FeatureId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::App;

/// The cursor prompt until a plane is picked (Fusion's wording).
pub(crate) const HINT: &str = "Select a planar face or a work plane";

#[derive(Debug, Clone, Default)]
pub(super) struct State {
  /// The plane picked, as `sketch.plane` takes it.
  pub picked: Option<Value>,
  /// The pick the selection last offered (a pick cleared with the chip's x stays cleared).
  seen: Option<Value>,
  /// The timeline was rolled back to the sketch (one undo step to take back).
  rolled: bool,
}

impl Dialog {
  /// Points the dialog at `sketch` and rolls the timeline back to it.
  pub(crate) fn redefine_begin(&mut self, s: &mut Session, sketch: FeatureId) {
    self.sketch = Some(sketch);
    s.selection.clear();
    let Some(i) = s.design.index_of(sketch) else { return };
    if s.design.active_len() != i + 1 {
      self.redefine.rolled = s.execute("timeline.marker", &json!({"at": i + 1})).is_ok();
    }
  }

  /// Takes back the timeline roll-back (Cancel, and before OK commits). The preview must be
  /// withdrawn first.
  pub(super) fn redefine_restore(&mut self, s: &mut Session) {
    if std::mem::take(&mut self.redefine.rolled) {
      s.undo();
      s.drop_redo();
    }
  }

  /// A planar face or a construction plane picked in the canvas or the browser (or an origin
  /// plane picked in the browser) becomes the new plane.
  pub(super) fn redefine_sync(&mut self, app: &App) {
    if self.kind != Kind::RedefinePlane {
      return;
    }
    let s = &app.s;
    let offered = s
      .selection
      .iter()
      .find_map(|sel| match *sel {
        Sel::Face { body, face } if super::is_planar(s, body, face) => Some(json!({"body": body.0, "face": face})),
        Sel::Feature { feature } if s.model.planes.contains_key(&feature) => Some(json!({"feature": feature.0})),
        _ => None,
      })
      .or_else(|| app.picked_plane.clone());
    if offered == self.redefine.seen {
      return;
    }
    self.redefine.seen = offered.clone();
    if let Some(v) = offered
      && Some(&v) != self.redefine.picked.as_ref()
    {
      self.redefine.picked = Some(v);
      self.dirty = true;
    }
  }

  /// The preview: the sketch on the picked plane.
  pub(super) fn redefine_command(&self) -> Option<(&'static str, Value)> {
    Some(("sketch.plane", json!({"sketch": self.sketch?.0, "plane": self.redefine.picked.clone()?})))
  }

  /// OK: the preview and the roll-back are taken back and the sketch moves on the whole timeline,
  /// one undo step. Returns whether the dialog closes.
  pub(super) fn redefine_ok(&mut self, app: &mut App) -> bool {
    let Some((cmd, params)) = self.redefine_command() else {
      self.error = Some(self.missing_input().into());
      return false;
    };
    self.withdraw(&mut app.s);
    self.redefine_restore(&mut app.s);
    app.run(cmd, params);
    app.s.selection.clear();
    true
  }

  /// The Plane row: the Select chip, lit while it waits for a pick, "1 selected" with an x that
  /// clears the pick after one.
  pub(super) fn redefine_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Plane");
    let has = self.redefine.picked.is_some();
    let chip = super::sel_chip_lit(ui, self.accent(), if has { "1 selected" } else { "Select" }, true, has);
    if has {
      let x = egui::Rect::from_min_max(egui::pos2(chip.rect.right() - 20.0, chip.rect.top()), chip.rect.max);
      if ui.interact(x, ui.id().with("redefine-clear"), egui::Sense::click()).clicked() {
        self.redefine.picked = None;
        app.s.selection.clear();
        app.picked_plane = None;
        self.dirty = true;
      }
    }
    ui.end_row();
  }
}
