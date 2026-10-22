//! Trim's pieces to remove, picked in the viewport as in Fusion: with picking switched on, a
//! click on one of the trimmed surfaces adds the point under the cursor, and the piece of the
//! split surface there goes (the preview shows it gone; the list takes picks back out).

use egui::RichText;
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::forms::Input;
use crate::theme;

impl Dialog {
  /// The index of the form's pieces input, if it has one.
  fn pieces_input(&self) -> Option<usize> {
    let Kind::Form(f) = self.kind else { return None };
    f.inputs.iter().position(|i| matches!(i, Input::Pieces { .. }))
  }

  /// Do viewport clicks on `body` pick pieces to remove? Only while picking is on, and only on
  /// the surfaces being trimmed (not on a trimming body).
  pub fn picks_piece_of(&self, body: BodyId) -> bool {
    let Kind::Form(f) = self.kind else { return false };
    if self.pieces_input().is_none() || !self.form.picking_pieces || self.editing.is_some() {
      return false;
    }
    let id = json!(body.0);
    let mut trimmed = false;
    for (input, v) in f.inputs.iter().zip(&self.form.values) {
      match input {
        Input::Bodies { .. } if v.as_array().is_some_and(|a| a.contains(&id)) => trimmed = true,
        Input::Face { .. } if v.get("body") == Some(&id) => return false,
        _ => {}
      }
    }
    trimmed
  }

  /// Adds a point on a piece to remove; a showing preview is redone without that piece.
  pub fn add_piece(&mut self, at: [f64; 3]) {
    let Some(i) = self.pieces_input() else { return };
    let mut list = self.form.values[i].as_array().cloned().unwrap_or_default();
    list.push(json!(at.map(|c| (c * 1e6).round() / 1e6)));
    self.form.values[i] = json!(list);
    if self.applied {
      self.dirty = true;
    }
  }

  /// The pieces row: the switch for picking in the viewport, and the pieces picked so far (each
  /// can be put back).
  pub(super) fn pieces_field(&mut self, i: usize, label: &str, ui: &mut egui::Ui, dirty: &mut bool) {
    // Fusion's Trim picks pieces as soon as its tool is picked: no switch
    let switch = !matches!(self.kind, Kind::Form(f) if f.cmd == crate::forms::TRIM_CMD);
    ui.label(label);
    ui.vertical(|ui| {
      if switch
        && ui
          .checkbox(&mut self.form.picking_pieces, "Pick in the viewport")
          .on_hover_text("Click the pieces of the surfaces to remove, instead of keeping a side")
          .changed()
      {
        *dirty = true;
      }
      if !self.form.picking_pieces {
        return;
      }
      let mut list: Vec<Value> = self.form.values[i].as_array().cloned().unwrap_or_default();
      if list.is_empty() {
        ui.label(RichText::new("Click the pieces to remove").color(theme::MUTED));
      }
      let mut back = None;
      for k in 0..list.len() {
        ui.horizontal(|ui| {
          ui.label(format!("Piece {}", k + 1));
          if ui.small_button("✕").on_hover_text("Keep this piece").clicked() {
            back = Some(k);
          }
        });
      }
      if let Some(k) = back {
        list.remove(k);
        self.form.values[i] = json!(list);
        *dirty = true;
      }
    });
    ui.end_row();
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::model::{BodyId, FeatureId};
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use crate::forms::{EXTEND, TRIM, TRIM_BODY};

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn area(s: &Session, b: BodyId) -> f64 {
    s.model.body(b).unwrap().shape.mass().unwrap().area
  }

  /// One frame of the dialog: picks from the selection, then a live preview when due.
  fn frame(s: &mut Session, d: &mut Dialog) {
    d.sync_selection(s);
    d.live_check();
    if d.dirty {
      d.preview(s);
    }
  }

  fn sheet(s: &mut Session) -> BodyId {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap().0;
    run(s, "surface.patch", json!({"sketch": sk}));
    s.model.bodies[0].id
  }

  #[test]
  fn trim_removes_the_pieces_clicked() {
    // a 10 x 10 sheet cut by a sphere: a disc and the rest
    let mut s = Session::default();
    let b = sheet(&mut s);
    run(&mut s, "sphere", json!({"at": [5, 5], "radius": 2}));
    let tool = s.model.bodies[1].id;
    let mut d = Dialog::blank(Kind::Form(&TRIM_BODY));
    s.selection = vec![Sel::Body { body: b }, Sel::Face { body: tool, face: 1 }];
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let hole = 100.0 - 4.0 * std::f64::consts::PI;
    assert!((area(&s, b) - hole).abs() < 1e-6, "by side: the disc goes");
    // picking pieces: nothing previews until one is clicked; the tool isn't a piece
    d.form.picking_pieces = true;
    d.dirty = true;
    frame(&mut s, &mut d);
    assert!(!d.applied);
    assert!(d.picks_piece_of(b) && !d.picks_piece_of(tool));
    // the picks survived the withdrawn preview
    frame(&mut s, &mut d);
    assert!(d.picks_piece_of(b));
    // clicking the outer part removes it: only the disc is left
    d.add_piece([1.0, 1.0, 0.0]);
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!((area(&s, b) - 4.0 * std::f64::consts::PI).abs() < 1e-6, "{}", area(&s, b));
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.trim");
    assert_eq!(p["remove"], json!([[1.0, 1.0, 0.0]]));
    // picking off again: back to the side
    d.form.picking_pieces = false;
    d.dirty = true;
    frame(&mut s, &mut d);
    assert!((area(&s, b) - hole).abs() < 1e-6);
    assert!(d.command().unwrap().1.get("remove").is_none());
  }

  fn plane(s: &mut Session, x: f64) -> Sel {
    let f = run(s, "plane.offset", json!({"plane": "yz", "offset": format!("{x} mm")}))["feature"].as_u64().unwrap();
    Sel::Feature { feature: FeatureId(f as u32) }
  }

  #[test]
  fn trim_picks_its_tool_then_the_pieces() {
    // the sheet split at x = 4 by a construction plane; the clicked strip left of it goes
    let mut s = Session::default();
    let b = sheet(&mut s);
    let tool = plane(&mut s, 4.0);
    let mut d = Dialog::blank(Kind::Form(&TRIM));
    // one row, the tool, and OK greyed until it is picked
    frame(&mut s, &mut d);
    assert_eq!(d.form.missing(&TRIM), Some("Trim Tool"));
    assert!(!d.ok_enabled() && !d.applied);
    assert!(!d.form.shown(&TRIM, 2), "no pieces row before the tool");
    s.selection = vec![tool];
    frame(&mut s, &mut d);
    // the tool's cut previews: the sheet in two pieces, nothing gone yet; OK waits for a piece
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert_eq!(s.model.body(b).unwrap().faces().len(), 2);
    assert!((area(&s, b) - 100.0).abs() < 1e-6, "{}", area(&s, b));
    assert!(d.form.shown(&TRIM, 2) && !d.ok_enabled());
    assert!(d.picks_piece_of(b));
    d.add_piece([2.0, 5.0, 0.0]);
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!((area(&s, b) - 60.0).abs() < 1e-6, "{}", area(&s, b));
    assert!(d.ok_enabled());
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["cut_only"], &p["bodies"]), ("surface.trim", &json!(true), &json!([b.0])));
  }

  #[test]
  fn trim_tool_that_cuts_nothing_is_an_error() {
    // Plane1 past the sheet: Fusion's error, and OK stays greyed
    let mut s = Session::default();
    sheet(&mut s);
    s.selection = vec![plane(&mut s, 40.0)];
    let mut d = Dialog::blank(Kind::Form(&TRIM));
    frame(&mut s, &mut d);
    assert!(
      d.error.as_deref().is_some_and(|e| e.contains("Cutting Tool does not intersect any surface!")),
      "{:?}",
      d.error
    );
    assert!(!d.ok_enabled());
  }

  #[test]
  fn trim_with_a_body_or_sketch_curves_as_the_tool() {
    // a sphere picked by a face cuts a disc out of the sheet; the clicked outer part goes
    let mut s = Session::default();
    let b = sheet(&mut s);
    run(&mut s, "sphere", json!({"at": [5, 5], "radius": 2}));
    let ball = s.model.bodies[1].id;
    s.selection = vec![Sel::Face { body: ball, face: 1 }];
    let mut d = Dialog::blank(Kind::Form(&TRIM));
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    // the solid tool isn't one of the surfaces it trims
    assert!(d.picks_piece_of(b) && !d.picks_piece_of(ball));
    d.add_piece([1.0, 1.0, 0.0]);
    frame(&mut s, &mut d);
    assert!((area(&s, b) - 4.0 * std::f64::consts::PI).abs() < 1e-6, "{}", area(&s, b));
    d.cancel(&mut s);
    // a sketch line at x = 5: the half clicked goes
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.line", json!({"points": [[5, -1], [5, 11]]}));
    let sk = s.edit_sketch.unwrap();
    let curve = s.design.sketch(sk).unwrap().curves[0].id;
    run(&mut s, "sketch.finish", json!({}));
    s.selection = vec![Sel::SketchCurve { sketch: sk, curve }];
    let mut d = Dialog::blank(Kind::Form(&TRIM));
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    d.add_piece([2.0, 5.0, 0.0]);
    frame(&mut s, &mut d);
    assert!((area(&s, b) - 50.0).abs() < 1e-6, "{}", area(&s, b));
  }

  #[test]
  fn extend_keeps_its_edges_when_the_preview_is_withdrawn() {
    // two edges picked one after the other with the preview showing between: both extend
    let mut s = Session::default();
    let b = sheet(&mut s);
    let edge = |s: &Session, x: f64, y: f64| {
      let e = s.model.body(b).unwrap().edges().iter().find(|e| (e.mid.x - x).abs() + (e.mid.y - y).abs() < 1e-6);
      Sel::Edge { body: b, edge: e.unwrap().id }
    };
    let mut d = Dialog::blank(Kind::Form(&EXTEND));
    s.selection = vec![edge(&s, 10.0, 5.0)];
    frame(&mut s, &mut d);
    // the Distance row starts at 0: nothing previews until it is set
    assert!(!d.applied);
    let i = EXTEND.inputs.iter().position(|x| x.key() == "distance").unwrap();
    d.form.values[i] = json!("5 mm");
    frame(&mut s, &mut d);
    assert!(d.applied);
    d.unpreview(&mut s);
    assert_eq!(s.selection.len(), 1, "the pick is back");
    s.selection.push(edge(&s, 5.0, 0.0));
    let i = EXTEND.inputs.iter().position(|x| x.key() == "alignment").unwrap();
    d.form.values[i] = json!("align");
    frame(&mut s, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!((area(&s, b) - 225.0).abs() < 1e-6, "15 x 15 with the corner: {}", area(&s, b));
  }
}
