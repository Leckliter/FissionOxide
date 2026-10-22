//! Rows of the MESH > MODIFY forms drawn as Fusion draws them: a value box with a slider
//! after it (Remesh's Density and Shape Preservation, Shell's Thickness), Scale Mesh's Point
//! chip (filled in by itself) and its Scale Type drop-down with an icon.

use serde_json::{Value, json};

use super::{Dialog, choice, sel_chip, value_edit_w};
use crate::forms::{Form, Input};
use crate::icons::Icon;

/// The number a slider shows for `text` and the unit after it ("2 mm" → (2, " mm")); an
/// expression the slider can't read shows as 0.
pub(crate) fn split_value(text: &str) -> (f64, String) {
  let t = text.trim();
  let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(t.len());
  match t[..end].parse::<f64>() {
    Ok(v) if t[end..].trim().chars().all(|c| c.is_ascii_alphabetic()) => {
      let unit = t[end..].trim();
      (v, if unit.is_empty() { String::new() } else { format!(" {unit}") })
    }
    _ => (0.0, String::new()),
  }
}

/// `v` written back with `unit`, to the slider's two decimals.
pub(crate) fn join_value(v: f64, unit: &str) -> String {
  format!("{v:.2}{unit}")
}

impl Dialog {
  /// Draws input `i` of a mesh form when it has a row of its own; true when drawn here.
  pub(super) fn mesh_modify_field(&mut self, f: &'static Form, i: usize, ui: &mut egui::Ui, dirty: &mut bool) -> bool {
    let accent = self.accent();
    match (&f.inputs[i], f.cmd) {
      (Input::Slider { label, max, .. }, _) => {
        let mut t = self.form.values[i].as_str().unwrap_or("").to_string();
        ui.label(*label);
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 6.0;
          if value_edit_w(ui, &mut t, 70.0).changed() {
            *dirty = true;
          }
          let (mut v, unit) = split_value(&t);
          let hi = (*max as f64).max(v);
          let r = ui.add(egui::Slider::new(&mut v, 0.0..=hi).show_value(false));
          if r.changed() {
            t = join_value(v, &unit);
            *dirty = true;
          }
        });
        ui.end_row();
        self.form.values[i] = json!(t);
        true
      }
      // the point the bodies scale about: their centre, picked for you
      (Input::Text { key: "point", label, .. }, "mesh.scale") => {
        let picked = f
          .inputs
          .iter()
          .position(|x| x.key() == "bodies")
          .and_then(|j| self.form.values[j].as_array())
          .is_some_and(|a| !a.is_empty());
        ui.label(*label);
        sel_chip(ui, accent, if picked { "1 selected" } else { "Select" }, picked);
        ui.end_row();
        true
      }
      (Input::Choice { key: "scale_type", label, options }, "mesh.scale") => {
        ui.label(*label);
        let cur = self.form.values[i].as_str().unwrap_or("");
        let mut sel: &'static str = options.iter().find(|(k, _)| *k == cur).map_or("uniform", |(k, _)| *k);
        if choice::combo(ui, &format!("form-choice-{i}"), options, &mut sel, |_| Some(Icon::Scale)) {
          *dirty = true;
        }
        self.form.values[i] = Value::from(sel);
        ui.end_row();
        true
      }
      _ => false,
    }
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::{Sel, Session};

  use super::super::Kind;
  use super::*;
  use crate::App;
  use crate::forms::{MESH_COMBINE, MESH_REMESH, MESH_SEPARATE, MESH_SHELL, SCALE_MESH};

  const LPRISM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/reference/464-stl-millimeter/lprism.stl");

  fn shown(d: &Dialog, f: &'static Form, key: &str) -> bool {
    d.form.shown(f, f.inputs.iter().position(|x| x.key() == key).unwrap())
  }

  fn set(d: &mut Dialog, f: &'static Form, key: &str, v: Value) {
    let i = f.inputs.iter().position(|x| x.key() == key).unwrap();
    d.form.values[i] = v;
  }

  fn tris(app: &App) -> usize {
    app.s.model.bodies.iter().map(|b| b.mesh().unwrap().indices.len() / 3).sum()
  }

  /// MESH > MODIFY's Remesh, Shell, Combine, Separate and Scale Mesh open as Fusion's do on an
  /// inserted mesh that is selected: the body chip pre-filled, Preview unticked, Fusion's
  /// prompts; OK runs them.
  #[test]
  fn mesh_modify_dialogs() {
    let mut app = App::with_session(Session::default());
    app.run("import", json!({"path": LPRISM})).unwrap();
    let b = app.s.model.bodies[0].id;
    let volume = |app: &App| app.s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum::<f64>();

    // Remesh: Density with its slider; Edge Length instead once the Method asks for it
    app.s.selection = vec![Sel::Body { body: b }];
    let mut d = Dialog::open(&mut app, Kind::Form(&MESH_REMESH));
    assert_eq!(Kind::Form(&MESH_REMESH).title(), "REMESH");
    assert!(!d.live.on && d.ready());
    assert!(shown(&d, &MESH_REMESH, "density") && !shown(&d, &MESH_REMESH, "edge_length"));
    let (_, p) = d.command().unwrap();
    assert_eq!(
      (p["type"].as_str(), p["density"].as_str(), p["shape"].as_str()),
      (Some("adaptive"), Some("0.25"), Some("0.50"))
    );
    set(&mut d, &MESH_REMESH, "method", json!("edge_length"));
    assert!(!shown(&d, &MESH_REMESH, "density") && shown(&d, &MESH_REMESH, "edge_length"));
    set(&mut d, &MESH_REMESH, "type", json!("uniform"));
    let before = tris(&app);
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(tris(&app) > before, "{before} → {}", tris(&app));
    d.withdraw(&mut app.s);
    assert_eq!(tris(&app), before);

    // Shell: Thickness, then Fusion's prompt for direction and thickness
    app.s.selection = vec![Sel::Body { body: b }];
    let mut d = Dialog::open(&mut app, Kind::Form(&MESH_SHELL));
    assert!(!d.live.on && d.ready());
    assert_eq!(d.prompt(false).as_deref(), Some("Adjust direction and thickness"));
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(volume(&app) < 1900.0, "{}", volume(&app));
    d.withdraw(&mut app.s);

    // Scale Mesh: the Point fills in by itself; Fusion keeps prompting for the entities
    app.s.selection = vec![Sel::Body { body: b }];
    let mut d = Dialog::open(&mut app, Kind::Form(&SCALE_MESH));
    assert!(!d.live.on && d.ready());
    assert_eq!(d.prompt(false).as_deref(), Some("Select bodies or sketches to scale"));
    assert!(shown(&d, &SCALE_MESH, "factor") && !shown(&d, &SCALE_MESH, "x"));
    set(&mut d, &SCALE_MESH, "factor", json!("2"));
    let (_, p) = d.command().unwrap();
    assert!(p.get("point").is_none() && p.get("center").is_none(), "{p}");
    d.preview(&mut app.s);
    assert!((volume(&app) - 16000.0).abs() < 1e-3, "{}", volume(&app));
    d.withdraw(&mut app.s);

    // Separate: one shell has nothing to separate
    app.s.selection = vec![Sel::Body { body: b }];
    let mut d = Dialog::open(&mut app, Kind::Form(&MESH_SEPARATE));
    assert!(!d.live.on && d.ready());
    assert_eq!(d.prompt(false).as_deref(), Some("Select mesh body to separate"));
    d.preview(&mut app.s);
    assert!(d.error.as_deref().is_some_and(|e| e.contains("single shell")), "{:?}", d.error);
    d.withdraw(&mut app.s);

    // Combine: the first body is the target; OK waits for a tool body
    app.run("import", json!({"path": LPRISM})).unwrap();
    let t = app.s.model.bodies[1].id;
    app.run("move", json!({"bodies": [t.0], "translate": [0, 0, 2.5]})).unwrap();
    app.s.selection = vec![Sel::Body { body: b }];
    let mut d = Dialog::open(&mut app, Kind::Form(&MESH_COMBINE));
    assert!(!d.live.on && !d.ready());
    assert_eq!(d.prompt(false).as_deref(), Some("Select target body"));
    app.s.selection = vec![Sel::Body { body: b }, Sel::Body { body: t }];
    d.form.sync(&MESH_COMBINE, &app.s, None);
    assert!(d.ready());
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, p["target"].as_u64(), p["tools"].clone()), ("mesh.combine", Some(b.0 as u64), json!([t.0])));
    assert_eq!(p["operation"], "join");
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies.len(), 1);
    assert!((volume(&app) - 3000.0).abs() < 1.0, "{}", volume(&app));
  }

  /// The MESH tab's Remesh, Shell, Combine, Separate and Scale Mesh open their dialogs.
  #[test]
  fn mesh_modify_menu_entries_open_dialogs() {
    use crate::panels::{Cmd, Tab};
    let cmds = crate::menus::commands(&[Tab::Mesh]);
    for (label, form) in [
      ("Remesh", &MESH_REMESH),
      ("Shell", &MESH_SHELL),
      ("Combine", &MESH_COMBINE),
      ("Separate", &MESH_SEPARATE),
      ("Scale Mesh", &SCALE_MESH),
    ] {
      assert!(cmds.iter().any(|e| e.label == label && e.cmd == Cmd::Dialog(Kind::Form(form))), "{label}");
    }
  }

  #[test]
  fn slider_values_keep_their_unit() {
    assert_eq!(split_value("0.25"), (0.25, String::new()));
    assert_eq!(split_value("2 mm"), (2.0, " mm".into()));
    assert_eq!(split_value("0.079 in"), (0.079, " in".into()));
    assert_eq!(split_value("d1 * 2"), (0.0, String::new()));
    assert_eq!(join_value(0.5, ""), "0.50");
    assert_eq!(join_value(3.0, " mm"), "3.00 mm");
  }
}
