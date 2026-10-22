//! Stitch's own rows as Fusion draws them: the Tolerance slider, the collapsed Stitch Result
//! group and the open Unstitched Edges group, a table of the open edges left wider apart than the
//! tolerance (Edge / Max Gap) with a magnifier that zooms to the picked one.

use egui::RichText;
use fox_engine::Session;
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::App;
use crate::forms::{Form, Input, State};
use crate::icons::Icon;
use crate::theme;

/// How far apart open edges may be and still be listed (mm): the slider's top.
const MAX_GAP: f64 = 1.0;

/// Stitch: finds the picked bodies' open edges again when the picks change (on the model
/// without the preview: the sync runs only then).
pub(super) fn find_open_edges(state: &mut State, f: &Form, s: &Session) {
  if f.cmd != "surface.stitch" {
    return;
  }
  let Some(v) = f.inputs.iter().position(|x| x.key() == "bodies").map(|i| state.values[i].clone()) else { return };
  if v == state.open_edges_for {
    return;
  }
  let bodies: Vec<BodyId> =
    v.as_array().into_iter().flatten().filter_map(Value::as_u64).map(|b| BodyId(b as u32)).collect();
  state.open_edges = fox_engine::model::surface::open_edges(&s.model, &bodies, MAX_GAP);
  state.open_edges_for = v;
  state.open_edge_row = None;
}

/// The tolerance as a length (mm), if it reads as one.
fn tolerance_mm(text: &str) -> Option<f64> {
  fox_engine::expr::eval(text, fox_engine::expr::Kind::Length, &fox_engine::expr::Empty).ok()
}

/// The open edges the stitch leaves: those further apart than the tolerance.
fn unstitched(state: &State, tolerance: f64) -> Vec<(usize, &fox_engine::model::surface::OpenEdge)> {
  state.open_edges.iter().enumerate().filter(|(_, o)| o.gap > tolerance).collect()
}

impl Dialog {
  /// Stitch's Tolerance row and the slider under it (0 to 1 mm).
  pub(super) fn stitch_tolerance(&mut self, i: usize, input: &Input, ui: &mut egui::Ui, dirty: &mut bool) {
    let Input::Expr { label, .. } = input else { return };
    let mut t = self.form.values[i].as_str().unwrap_or("").to_string();
    Self::field(ui, label, &mut t, dirty);
    ui.label("");
    let mut mm = tolerance_mm(&t).unwrap_or(0.0).clamp(0.0, MAX_GAP);
    if ui.add(egui::Slider::new(&mut mm, 0.0..=MAX_GAP).show_value(false)).changed() {
      t = format!("{} mm", fox_engine::expr::trim_number((mm * 1e3).round() / 1e3));
      *dirty = true;
    }
    ui.end_row();
    self.form.values[i] = json!(t);
  }

  /// Stitch's groups under its rows: Stitch Result (collapsed) and Unstitched Edges (open).
  pub(super) fn stitch_groups(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let Kind::Form(f) = self.kind else { return };
    let tol = self.form_value("tolerance").and_then(Value::as_str).and_then(tolerance_mm).unwrap_or(0.0);
    let picked = f
      .inputs
      .iter()
      .position(|x| x.key() == "bodies")
      .and_then(|i| self.form.values[i].as_array().map(Vec::len))
      .unwrap_or(0);
    let left = unstitched(&self.form, tol).len();
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Stitch Result").default_open(false).show(ui, |ui| {
      egui::Grid::new("stitch-result").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
        ui.label("Surfaces");
        ui.label(picked.to_string());
        ui.end_row();
        ui.label("Unstitched edges");
        ui.label(left.to_string());
        ui.end_row();
      });
    });
    egui::CollapsingHeader::new("Unstitched Edges").default_open(true).show(ui, |ui| {
      let rows: Vec<(usize, fox_engine::model::surface::OpenEdge)> =
        unstitched(&self.form, tol).into_iter().map(|(k, o)| (k, *o)).collect();
      egui::Frame::new().stroke(egui::Stroke::new(1.0, theme::BORDER)).inner_margin(4.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("stitch-gaps").num_columns(2).min_col_width(110.0).spacing([10.0, 3.0]).striped(true).show(
          ui,
          |ui| {
            ui.label(RichText::new("Edge").strong());
            ui.label(RichText::new("Max Gap").strong());
            ui.end_row();
            for (n, (k, o)) in rows.iter().enumerate() {
              let on = self.form.open_edge_row == Some(*k);
              if ui.selectable_label(on, format!("Edge{}", n + 1)).clicked() {
                self.form.open_edge_row = Some(*k);
              }
              ui.label(super::manip::mm(self.unit, o.gap));
              ui.end_row();
            }
          },
        );
        if rows.is_empty() {
          ui.label(RichText::new("No open edges").color(theme::MUTED));
        }
      });
      ui.horizontal(|ui| {
        let row = self.form.open_edge_row.and_then(|k| self.form.open_edges.get(k)).copied();
        let zoom = crate::icons::icon_button(ui, Icon::Zoom, 22.0, "Zoom to the picked edge", false, None);
        if zoom.clicked()
          && let Some(o) = row.or_else(|| rows.first().map(|r| r.1))
        {
          app.zoom_to(o.lo, o.hi);
        }
        ui.add_enabled_ui(false, |ui| crate::icons::icon_button(ui, Icon::Select, 22.0, "", false, None))
          .inner
          .on_disabled_hover_text("Selecting the unstitched edges isn't in FissionOxide yet");
      });
    });
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use crate::forms::*;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  /// The rows a form draws as it stands.
  fn rows(f: &Form, d: &Dialog) -> Vec<&'static str> {
    (0..f.inputs.len()).filter(|&i| d.form.shown(f, i) && !State::hidden(f, i)).map(|i| f.inputs[i].label()).collect()
  }

  fn options(f: &Form, key: &str) -> Vec<&'static str> {
    match f.inputs.iter().find(|x| x.key() == key) {
      Some(Input::Choice { options, .. }) => options.iter().map(|o| o.1).collect(),
      _ => panic!("{} has no {key} choice", f.name),
    }
  }

  /// A 10 × 10 surface on XY from `x` along.
  fn sheet(s: &mut Session, x: f64) -> fox_engine::model::BodyId {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [x, 0], "corner2": [x + 10.0, 10]}));
    let sk = s.edit_sketch.unwrap().0;
    run(s, "surface.patch", json!({"sketch": sk}));
    s.model.bodies.last().unwrap().id
  }

  #[test]
  fn surface_modify_dialogs_open_with_fusions_rows() {
    let open = |f: &'static Form| (f, Dialog::blank(Kind::Form(f)));
    let expect: [(&'static Form, &[&str]); 6] = [
      (&TRIM, &["Trim Tool"]),
      (&EXTEND, &["Edges", "Tangent Chain", "Type", "Edge Alignment"]),
      (&UNTRIM, &["Faces", "Type"]),
      (&STITCH, &["Stitch Surfaces", "Tolerance"]),
      (&UNSTITCH, &["Faces / Bodies", "Chain Selection"]),
      (&REVERSE_NORMAL, &["Faces"]),
    ];
    for (f, want) in expect {
      let (f, d) = open(f);
      assert_eq!(rows(f, &d), want, "{}", f.name);
      // no Preview check box, and OK greyed until the picks are made
      assert!(always_previews(f), "{}", f.name);
      assert!(!d.ok_enabled(), "{}", f.name);
    }
    assert_eq!(options(&EXTEND, "mode"), ["Natural", "Perpendicular", "Tangent"]);
    assert_eq!(options(&EXTEND, "alignment"), ["Free Edges", "Align Edges"]);
    assert_eq!(options(&UNTRIM, "loops"), ["All Edges", "Internal Edges", "External Edges", "Manual"]);
    // Extend has Sweep's and Loft's Feature and Analysis tabs
    assert!(Dialog::blank(Kind::Form(&EXTEND)).has_tabs() && !Dialog::blank(Kind::Form(&TRIM)).has_tabs());
    let d = Dialog::blank(Kind::Form(&STITCH));
    assert_eq!(d.form_value("tolerance"), Some(&json!("0.10 mm")));
    let d = Dialog::blank(Kind::Form(&UNSTITCH));
    assert_eq!(d.form_value("chain"), Some(&json!(true)));
  }

  #[test]
  fn extend_asks_for_its_distance_after_the_pick() {
    let mut s = Session::default();
    let b = sheet(&mut s, 0.0);
    let e = s.model.body(b).unwrap().edges().iter().find(|e| (e.mid.x - 10.0).abs() < 1e-6).unwrap().id;
    let mut d = Dialog::blank(Kind::Form(&EXTEND));
    s.selection = vec![Sel::Edge { body: b, edge: e }];
    d.sync_selection(&s);
    assert_eq!(rows(&EXTEND, &d), ["Edges", "Tangent Chain", "Distance", "Type", "Edge Alignment"]);
    assert_eq!(d.form_value("distance"), Some(&json!("0.00 mm")));
    assert_eq!(d.prompt(false).as_deref(), Some("Select extension type or specify distance"));
    assert!(!d.ok_enabled());
    // the arrow sits on the edge, pointing out of the sheet
    d.refresh_manip(&s);
    let m = d.manipulator(&s).expect("Extend's arrow");
    assert!((m.origin.x - 10.0).abs() < 1e-6 && (m.dir.x - 1.0).abs() < 1e-6, "{:?} {:?}", m.origin, m.dir);
    d.set_manipulator(3.0, 0.1);
    assert!(d.ready());
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["mode"]), ("surface.extend", &json!("natural")));
    assert!(p.get("close_corners").is_none());
    // Manual untrim isn't there yet: OK stays greyed with it
    let mut u = Dialog::blank(Kind::Form(&UNTRIM));
    s.selection = vec![Sel::Face { body: b, face: 1 }];
    u.sync_selection(&s);
    assert!(u.ready() && rows(&UNTRIM, &u).contains(&"Extension"));
    let i = UNTRIM.inputs.iter().position(|x| x.key() == "loops").unwrap();
    u.form.values[i] = json!("manual");
    assert!(!u.ready() && !rows(&UNTRIM, &u).contains(&"Extension"));
  }

  #[test]
  fn unstitch_and_reverse_normal_pick_faces() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    s.selection = vec![Sel::Face { body: b, face: 1 }];
    let mut d = Dialog::blank(Kind::Form(&UNSTITCH));
    d.sync_selection(&s);
    // Chain Selection: the whole body
    assert_eq!(d.command().unwrap(), ("surface.unstitch", json!({"bodies": [b.0]})));
    // without it, just the face (the chip counts faces)
    let i = UNSTITCH.inputs.iter().position(|x| x.key() == "chain").unwrap();
    d.form.values[i] = json!(false);
    assert!(d.form.filter_faces(&UNSTITCH));
    assert_eq!(d.command().unwrap(), ("surface.unstitch", json!({"faces": [{"body": b.0, "face": 1}]})));
    let mut r = Dialog::blank(Kind::Form(&REVERSE_NORMAL));
    r.sync_selection(&s);
    assert_eq!(r.command().unwrap(), ("surface.reverse", json!({"bodies": [b.0]})));
  }

  #[test]
  fn stitch_lists_the_edges_its_tolerance_leaves_open() {
    // two sheets half a millimetre apart: their facing edges, until the tolerance spans the gap
    let mut s = Session::default();
    let a = sheet(&mut s, 0.0);
    let b = sheet(&mut s, 10.5);
    s.selection = vec![Sel::Body { body: a }, Sel::Body { body: b }];
    let mut d = Dialog::blank(Kind::Form(&STITCH));
    d.sync_selection(&s);
    assert_eq!(d.form.open_edges.len(), 2);
    assert_eq!(super::unstitched(&d.form, 0.1).len(), 2);
    assert!(super::unstitched(&d.form, 0.6).is_empty());
    assert_eq!(d.command().unwrap().1["tolerance"], json!("0.10 mm"));
  }
}
