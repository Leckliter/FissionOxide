//! Dialog parts for the second batch of solid features: setback fillets, sweep guide rails and
//! twist, loft rails and centre line, extrude to an object, and the emboss / boundary fill /
//! replace face forms (their extra inputs and timeline editing).

use fox_engine::model::{BodyId, FeatureId, FeatureKind, OriginPlane, PlaneRef};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind, sel_chip};
use crate::App;
use crate::forms::{Form, Input};

/// Values the solid feature extensions add to a dialog.
#[derive(Debug, Clone, Default)]
pub(super) struct Extra {
  setback: String,
  /// Fillet radius given as the chord length (the round's width).
  chord: bool,
  rail: Option<FeatureId>,
  twist: String,
  rails: Vec<FeatureId>,
  centerline: Option<FeatureId>,
  /// Extrude to object: the picked face or body (`{body, face}` or `{body}`).
  to: Value,
  /// When editing an extrude to an object: its extent kind and stored reference.
  to_kind: &'static str,
  to_ref: Value,
  /// Fillet selection sets (empty: just the one in the dialog's fields) and the active one.
  pub(super) rows: Vec<FilletRow>,
  active: usize,
  /// Fillet: tangent chain off (on by default, as in Fusion).
  pub(super) no_chain: bool,
  /// Fillet corner type: setback (else rolling ball).
  setback_corner: bool,
  /// An edited fillet's stored sets after the first.
  edit_sets: Value,
  /// Extrude rows (see [`super::extrude`]).
  pub(super) ext: super::extrude::Rows,
}

/// One fillet selection set: edges, faces (all their edges) and its radius.
#[derive(Debug, Clone, Default)]
pub(super) struct FilletRow {
  pub(super) edges: Vec<Value>,
  pub(super) faces: Vec<Value>,
  pub(super) radius: String,
}

impl FilletRow {
  fn is_empty(&self) -> bool {
    self.edges.is_empty() && self.faces.is_empty()
  }

  /// "1 Face", "2 Edges", "1 Face, 2 Edges" or "Select".
  fn label(&self) -> String {
    let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    match (self.faces.len(), self.edges.len()) {
      (0, 0) => "Select".into(),
      (f, 0) => n(f, "Face", "Faces"),
      (0, e) => n(e, "Edge", "Edges"),
      (f, e) => format!("{}, {}", n(f, "Face", "Faces"), n(e, "Edge", "Edges")),
    }
  }
}

impl Dialog {
  // ---- fillet ----

  /// Fusion's selection rows: each set of edges and faces with its own radius, the active one
  /// in `edges` / `faces` / `radius` (and the viewport selection), + for another set.
  pub(super) fn fillet_rows(&mut self, app: &mut App, ui: &mut egui::Ui, creating: bool) {
    let rows = self.fillet_sets();
    let accent = self.accent();
    let (mut switch, mut remove, mut add) = (None, None, false);
    for (i, row) in rows.iter().enumerate() {
      ui.label(if i == 0 { "Edges / Faces" } else { "" });
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let active = i == self.solid.active;
        let text = if creating { row.label() } else { format!("{} (unchanged)", row.edges.len()) };
        let chip = sel_chip(ui, accent, &text, active && (!row.edges.is_empty() || !row.faces.is_empty()));
        if !active && chip.interact(egui::Sense::click()).on_hover_text("Select this set").clicked() {
          switch = Some(i);
        }
        // the active set's radius takes typing aimed at the dialog
        let changed = if active {
          super::value_edit_w(ui, &mut self.radius, 64.0).changed()
        } else {
          ui.add(egui::TextEdit::singleline(&mut self.solid.rows[i].radius).desired_width(64.0)).changed()
        };
        if changed {
          self.dirty = !creating || self.applied;
          self.live_reset();
        }
        if creating && rows.len() > 1 && ui.small_button("✕").on_hover_text("Remove this set").clicked() {
          remove = Some(i);
        }
      });
      ui.end_row();
    }
    if creating {
      ui.label("");
      if ui.small_button("+").on_hover_text("Add a selection set with its own radius").clicked() {
        add = true;
      }
      ui.end_row();
    }
    ui.label("Tangent chain");
    let mut chain = !self.solid.no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.no_chain = !chain;
    }
    ui.end_row();
    let s = &mut app.s;
    if let Some(j) = switch {
      self.switch_fillet_set(s, j);
    } else if let Some(j) = remove {
      self.withdraw(s);
      self.store_fillet_set();
      self.solid.rows.remove(j);
      let k = if self.solid.active >= j && self.solid.active > 0 { self.solid.active - 1 } else { self.solid.active };
      self.solid.active = usize::MAX;
      self.switch_fillet_set(s, k.min(self.solid.rows.len() - 1));
    } else if add {
      self.store_fillet_set();
      let radius = self.radius.clone();
      self.solid.rows.push(FilletRow { radius, ..FilletRow::default() });
      self.switch_fillet_set(s, self.solid.rows.len() - 1);
    }
  }

  /// Radius type and Fusion's Corner Type: rolling ball, or setback (with its distance).
  pub(super) fn setback_row(&mut self, ui: &mut egui::Ui, creating: bool) {
    ui.label("Radius type");
    let mut changed = false;
    ui.horizontal(|ui| {
      changed |= ui.radio_value(&mut self.solid.chord, false, "Constant").changed();
      changed |= ui.radio_value(&mut self.solid.chord, true, "Chord length").changed();
    });
    ui.end_row();
    ui.label("Corner type");
    let shown = if self.solid.setback_corner { "Setback" } else { "Rolling ball" };
    egui::ComboBox::from_id_salt("fillet-corner").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      changed |= ui.selectable_value(&mut self.solid.setback_corner, false, "Rolling ball").changed();
      changed |= ui.selectable_value(&mut self.solid.setback_corner, true, "Setback").changed();
    });
    ui.end_row();
    if self.solid.setback_corner {
      ui.label("Setback");
      let r = ui.add(egui::TextEdit::singleline(&mut self.solid.setback).hint_text("distance").desired_width(140.0));
      changed |= r.changed();
      ui.end_row();
    }
    if changed && (!creating || self.applied) {
      self.dirty = true;
    }
  }

  /// Fillet parameters with the setback (left out when empty; `null` when editing clears it).
  pub(super) fn with_setback(&self, mut p: Value, editing: bool) -> Value {
    if self.solid.chord || editing {
      p["chord"] = json!(self.solid.chord);
    }
    let sb = if self.solid.setback_corner { self.solid.setback.trim() } else { "" };
    if !sb.is_empty() {
      p["setback"] = json!(sb);
    } else if editing {
      p["setback"] = Value::Null;
    }
    p
  }

  pub(super) fn load_setback(&mut self, setback: &Option<String>, chord: bool) {
    self.solid.setback = setback.clone().unwrap_or_default();
    self.solid.setback_corner = !self.solid.setback.trim().is_empty();
    self.solid.chord = chord;
  }

  /// An edited fillet's sets after the first: their edge counts, radii and stored edges.
  pub(super) fn load_fillet_sets(&mut self, sets: &[fox_engine::model::FilletSet]) {
    if sets.is_empty() {
      return;
    }
    self.solid.rows = vec![FilletRow::default()];
    for set in sets {
      self.solid.rows.push(FilletRow {
        edges: vec![Value::Null; set.edges.len()],
        faces: Vec::new(),
        radius: set.radius.clone(),
      });
    }
    self.solid.edit_sets = serde_json::to_value(sets).unwrap_or_default();
  }

  /// Every selection set, the active one as it is now.
  pub(super) fn fillet_sets(&self) -> Vec<FilletRow> {
    let now = FilletRow { edges: self.edges.clone(), faces: self.faces.clone(), radius: self.radius.clone() };
    if self.solid.rows.is_empty() {
      return vec![now];
    }
    let mut rows = self.solid.rows.clone();
    rows[self.solid.active] = now;
    rows
  }

  /// The fillet command: one set, or `sets` each with its radius; tangent chains on by default.
  pub(super) fn fillet_params(&self) -> Value {
    let rows: Vec<FilletRow> = self.fillet_sets().into_iter().filter(|r| !r.is_empty()).collect();
    let mut p = match rows.as_slice() {
      [] => json!({"edges": [], "radius": self.radius}),
      [one] => json!({"edges": one.edges, "faces": one.faces, "radius": one.radius}),
      many => {
        json!({"sets": many.iter().map(|r| json!({"edges": r.edges, "faces": r.faces, "radius": r.radius})).collect::<Vec<_>>()})
      }
    };
    p["tangent_chain"] = json!(!self.solid.no_chain);
    self.with_setback(p, false)
  }

  /// `feature.edit` fields of an edited fillet: the radius of each set.
  pub(super) fn fillet_edit_set(&self) -> Value {
    let rows = self.fillet_sets();
    let mut p = json!({"radius": rows[0].radius});
    if let Some(sets) = self.solid.edit_sets.as_array() {
      let mut sets = sets.clone();
      for (set, row) in sets.iter_mut().zip(rows.iter().skip(1)) {
        set["radius"] = json!(row.radius);
      }
      p["sets"] = json!(sets);
    }
    self.with_setback(p, true)
  }

  pub(super) fn fillet_ready(&self) -> bool {
    self.fillet_sets().iter().any(|r| !r.is_empty())
  }

  pub(super) fn store_fillet_set(&mut self) {
    let now = FilletRow { edges: self.edges.clone(), faces: self.faces.clone(), radius: self.radius.clone() };
    match self.solid.rows.get_mut(self.solid.active) {
      Some(r) => *r = now,
      None => {
        self.solid.rows = vec![now];
        self.solid.active = 0;
      }
    }
  }

  /// Makes set `j` the active one: its picks become the viewport selection.
  pub(super) fn switch_fillet_set(&mut self, s: &mut Session, j: usize) {
    self.withdraw(s);
    if self.solid.active < self.solid.rows.len() {
      self.store_fillet_set();
    }
    let row = self.solid.rows[j].clone();
    self.solid.active = j;
    self.edges = row.edges;
    self.faces = row.faces;
    self.radius = row.radius;
    if self.editing.is_none() {
      s.selection = self.picked_selection();
    }
    self.live_reset();
    self.dirty = self.editing.is_some();
  }

  // ---- sweep and loft ----

  pub(super) fn sweep_rows(&mut self, app: &App, ui: &mut egui::Ui, dirty: &mut bool) {
    let list = Self::sketches(app, self.sketch);
    sketch_combo(ui, "Guide rail", "sweep-rail", &list, &mut self.solid.rail, dirty);
    if self.solid.rail.is_none() {
      Self::field(ui, "Twist angle", &mut self.solid.twist, dirty);
    }
  }

  pub(super) fn loft_rows(&mut self, app: &App, ui: &mut egui::Ui, dirty: &mut bool) {
    let list: Vec<(FeatureId, String)> = Self::sketches(app, None)
      .into_iter()
      .filter(|(id, _)| !self.sections.iter().any(|(sk, _)| *sk == id.0))
      .collect();
    if self.solid.rails.is_empty() {
      sketch_combo(ui, "Centre line", "loft-center", &list, &mut self.solid.centerline, dirty);
    }
    if self.solid.centerline.is_none() {
      ui.label("Rails");
      ui.vertical(|ui| {
        if list.is_empty() {
          ui.label("(no other sketches)");
        }
        for (id, name) in &list {
          let mut on = self.solid.rails.contains(id);
          if ui.checkbox(&mut on, name).changed() {
            if on {
              self.solid.rails.push(*id);
            } else {
              self.solid.rails.retain(|r| r != id);
            }
            *dirty = true;
          }
        }
      });
      ui.end_row();
    }
  }

  /// Sweep / loft parameters with the guide rail, twist, rails and centre line.
  pub(super) fn with_guides(&self, mut p: Value) -> Value {
    match self.kind {
      Kind::Sweep => {
        if let Some(r) = self.solid.rail {
          p["rail"] = json!(r.0);
        } else if !self.solid.twist.trim().is_empty() {
          p["twist"] = json!(self.solid.twist.trim());
        }
      }
      Kind::Loft => {
        if let Some(c) = self.solid.centerline {
          p["centerline"] = json!(c.0);
        } else if !self.solid.rails.is_empty() {
          p["rails"] = json!(self.solid.rails.iter().map(|r| r.0).collect::<Vec<_>>());
        }
      }
      _ => {}
    }
    p
  }

  // ---- extrude to an object ----

  pub(super) fn object_row(&mut self, ui: &mut egui::Ui) {
    ui.label("Object");
    if self.editing.is_some() {
      ui.label("(unchanged)");
    } else {
      let has = !self.solid.to.is_null();
      let text = if !has {
        "Select a face or body"
      } else if self.solid.to.get("face").is_some() {
        "1 face"
      } else {
        "1 body"
      };
      sel_chip(ui, self.accent(), text, has);
    }
    ui.end_row();
  }

  /// The face or body picked for an extrude to an object (the last one picked) and, with Start:
  /// Object, the start face (the first face picked).
  pub(super) fn sync_object(&mut self, s: &Session) {
    let picks: Vec<Value> = s
      .selection
      .iter()
      .filter_map(|x| match x {
        Sel::Face { body, face } => Some(json!({"body": body.0, "face": face})),
        Sel::Body { body } => Some(json!({"body": body.0})),
        _ => None,
      })
      .collect();
    let start = match self.solid.ext.start {
      "object" => picks.iter().find(|v| v.get("face").is_some()).cloned().unwrap_or(Value::Null),
      _ => Value::Null,
    };
    if start != self.solid.ext.start_face && self.editing.is_none() {
      self.solid.ext.start_face = start.clone();
      self.dirty = true;
    }
    let to = picks.into_iter().rev().find(|v| start.is_null() || *v != start).unwrap_or(Value::Null);
    if to != self.solid.to {
      self.solid.to = to;
      if self.extent == "to_object" {
        self.dirty = true;
      }
    }
  }

  /// Extrude parameters for the "to object" extent: to a face, or to a body.
  pub(super) fn with_object(&self, mut p: Value) -> Option<Value> {
    if self.extent != "to_object" {
      return Some(p);
    }
    if self.solid.to.is_null() {
      return None;
    }
    p["extent"] = json!(if self.solid.to.get("face").is_some() { "to_face" } else { "to_body" });
    p["to"] = self.solid.to.clone();
    Some(p)
  }

  /// Remembers an edited extrude's object (the "to object" extent shows it unchanged).
  pub(super) fn load_object(&mut self, kind: &'static str, to: Value) {
    self.solid.to_kind = kind;
    self.solid.to_ref = to;
  }

  /// The extent for `feature.edit`: the stored object when it is still "to object".
  pub(super) fn edit_extent(&self, mut extent: Value) -> Value {
    if self.extent == "to_object" {
      extent["kind"] = json!(if self.solid.to_kind.is_empty() { "to_face" } else { self.solid.to_kind });
      extent["to"] = self.solid.to_ref.clone();
    }
    extent
  }

  // ---- forms ----

  /// The form inputs the solid features added: profiles, a face on another body, an optional
  /// plane and boundary fill cells.
  pub(super) fn solid_form_field(
    &mut self,
    f: &'static Form,
    i: usize,
    app: &App,
    ui: &mut egui::Ui,
    dirty: &mut bool,
  ) {
    let s = &app.s;
    let accent = self.accent();
    let v = self.form.values[i].clone();
    match &f.inputs[i] {
      Input::Profiles { label, .. } => {
        ui.label(*label);
        let n = v.as_array().map(Vec::len).unwrap_or(0);
        sel_chip(ui, accent, &if n == 0 { "All".to_string() } else { format!("{n} selected") }, n > 0);
        ui.end_row();
      }
      Input::FaceOther { label, .. } => {
        ui.label(*label);
        sel_chip(ui, accent, if v.is_null() { "Select" } else { "1 selected" }, !v.is_null());
        ui.end_row();
      }
      Input::PlaneOpt { label, .. } => {
        ui.label(*label);
        let mut options: Vec<(Value, String)> = vec![(Value::Null, "None".into())];
        for (k, l) in [("xy", "XY plane"), ("xz", "XZ plane"), ("yz", "YZ plane")] {
          options.push((json!(k), l.into()));
        }
        for id in s.model.planes.keys() {
          options.push((json!({"feature": id.0}), s.design.feature(*id).map(|f| f.name.clone()).unwrap_or_default()));
        }
        let shown = options.iter().find(|(k, _)| *k == v).map(|(_, l)| l.clone()).unwrap_or_default();
        egui::ComboBox::from_id_salt(("form-plane-opt", i)).selected_text(shown).width(140.0).show_ui(ui, |ui| {
          for (k, l) in &options {
            if ui.selectable_label(*k == v, l).clicked() {
              self.form.values[i] = k.clone();
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      Input::Cells { label, tools, .. } => {
        self.refresh_cells(f, i, tools, s);
        ui.label(*label);
        ui.vertical(|ui| {
          if self.form.cells.is_empty() {
            ui.label("(no closed cells)");
          }
          let picked: Vec<Value> = v.as_array().cloned().unwrap_or_default();
          for (k, (p, vol)) in self.form.cells.clone().into_iter().enumerate() {
            let pv = json!(p);
            let mut on = picked.contains(&pv);
            if ui.checkbox(&mut on, format!("Cell {} ({:.0} mm³)", k + 1, vol)).changed() {
              let mut list = picked.clone();
              if on {
                list.push(pv);
              } else {
                list.retain(|x| *x != pv);
              }
              self.form.values[i] = json!(list);
              *dirty = true;
            }
          }
          if picked.is_empty() && !self.form.cells.is_empty() {
            ui.label("None ticked: every cell");
          }
        });
        ui.end_row();
      }
      _ => {}
    }
  }

  /// Recomputes the boundary fill cells when the tools (or the plane) changed.
  fn refresh_cells(&mut self, f: &'static Form, i: usize, tools: &str, s: &Session) {
    let tool_v = f.inputs.iter().position(|x| x.key() == tools).map(|k| self.form.values[k].clone());
    let planes: Vec<Value> = f
      .inputs
      .iter()
      .zip(&self.form.values)
      .filter(|(x, v)| matches!(x, Input::PlaneOpt { .. }) && !v.is_null())
      .map(|(_, v)| v.clone())
      .collect();
    let key = json!([tool_v, planes]);
    if key == self.form.cells_for {
      return;
    }
    self.form.cells_for = key;
    let ids: Vec<BodyId> = tool_v
      .and_then(|v| v.as_array().cloned())
      .unwrap_or_default()
      .iter()
      .filter_map(|b| b.as_u64().map(|b| BodyId(b as u32)))
      .collect();
    let refs: Vec<PlaneRef> = planes.iter().filter_map(plane_ref).collect();
    let Ok(cells) = fox_engine::model::regen::boundary_cells(&s.model, &ids, &refs) else {
      // e.g. editing a fill whose tools it removed: keep the picks as they are
      self.form.cells.clear();
      return;
    };
    self.form.cells = cells
      .iter()
      .filter_map(|c| {
        let m = c.mass().ok()?;
        Some((inner_point(c, m.center_of_mass, m.bbox_min, m.bbox_max).to_array(), m.volume))
      })
      .collect();
    // ticks for cells that no longer exist go
    let have: Vec<Value> = self.form.cells.iter().map(|(p, _)| json!(p)).collect();
    if let Some(list) = self.form.values[i].as_array() {
      let kept: Vec<Value> = list.iter().filter(|p| have.contains(p)).cloned().collect();
      self.form.values[i] = json!(kept);
    }
  }
}

fn sketch_combo(
  ui: &mut egui::Ui,
  label: &str,
  id: &str,
  list: &[(FeatureId, String)],
  cur: &mut Option<FeatureId>,
  dirty: &mut bool,
) {
  ui.label(label);
  let shown = cur.and_then(|c| list.iter().find(|(id, _)| *id == c)).map(|(_, n)| n.clone());
  egui::ComboBox::from_id_salt(id).selected_text(shown.unwrap_or("None".into())).width(140.0).show_ui(ui, |ui| {
    if ui.selectable_label(cur.is_none(), "None").clicked() {
      *cur = None;
      *dirty = true;
    }
    for (fid, n) in list {
      if ui.selectable_label(*cur == Some(*fid), n).clicked() {
        *cur = Some(*fid);
        *dirty = true;
      }
    }
  });
  ui.end_row();
}

fn plane_ref(v: &Value) -> Option<PlaneRef> {
  if let Some(f) = v.get("feature").and_then(Value::as_u64) {
    return Some(PlaneRef::Construction { feature: FeatureId(f as u32) });
  }
  let plane = match v.as_str()? {
    "xy" => OriginPlane::Xy,
    "xz" => OriginPlane::Xz,
    "yz" => OriginPlane::Yz,
    _ => return None,
  };
  Some(PlaneRef::Origin { plane })
}

/// A point inside a solid: its centre of mass, or a probe of its box when that falls outside.
fn inner_point(
  s: &fox_engine::kernel::Shape,
  com: fox_engine::kernel::glam::DVec3,
  lo: fox_engine::kernel::glam::DVec3,
  hi: fox_engine::kernel::glam::DVec3,
) -> fox_engine::kernel::glam::DVec3 {
  let inside = |p| s.classify(p).is_ok_and(|w| w == fox_engine::kernel::Inside::In);
  if inside(com) {
    return com;
  }
  let n = 8;
  for i in 1..n {
    for j in 1..n {
      for k in 1..n {
        let p = lo + (hi - lo) * fox_engine::kernel::glam::DVec3::new(i as f64, j as f64, k as f64) / n as f64;
        if inside(p) {
          return p;
        }
      }
    }
  }
  com
}

/// The form of a feature the solid features added, for editing it from the timeline.
pub(super) fn edit_form(kind: &FeatureKind) -> Option<Dialog> {
  let f: &'static Form = match kind {
    FeatureKind::Emboss { .. } => &crate::forms::EMBOSS,
    FeatureKind::BoundaryFill { .. } => &crate::forms::BOUNDARY_FILL,
    FeatureKind::ReplaceFace { .. } => &crate::forms::REPLACE_FACE,
    _ => return None,
  };
  let mut d = Dialog::blank(Kind::Form(f));
  let v = serde_json::to_value(kind).ok()?;
  for (i, input) in f.inputs.iter().enumerate() {
    let editable = matches!(
      input,
      Input::Expr { .. } | Input::Text { .. } | Input::Check { .. } | Input::Choice { .. } | Input::Bodies { .. }
    ) || matches!(input, Input::Cells { .. });
    if let Some(x) = v.get(input.key()).filter(|_| editable) {
      d.form.values[i] = x.clone();
    }
  }
  Some(d)
}

/// `feature.edit` fields from a form: its values and cells (picks stay as they are).
pub(super) fn form_edit_set(f: &Form, values: &[Value]) -> Value {
  let mut set = serde_json::Map::new();
  for (input, v) in f.inputs.iter().zip(values) {
    if matches!(
      input,
      Input::Expr { .. } | Input::Text { .. } | Input::Check { .. } | Input::Choice { .. } | Input::Cells { .. }
    ) {
      set.insert(input.key().to_string(), v.clone());
    }
  }
  Value::Object(set)
}

#[cfg(test)]
mod tests {
  use fox_engine::kernel::SurfaceKind;
  use serde_json::{Value, json};

  use super::*;
  use crate::forms::{BOUNDARY_FILL, EMBOSS, REPLACE_FACE};

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn apply(s: &mut Session, d: &mut Dialog) {
    assert!(d.ready(), "{:?} not ready", d.kind);
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  fn form(s: &mut Session, f: &'static Form, sel: Vec<Sel>) -> Dialog {
    let mut d = Dialog::blank(Kind::Form(f));
    s.selection = sel;
    d.form.sync(f, s, None);
    d
  }

  fn set(d: &mut Dialog, f: &Form, k: &str, v: Value) {
    let i = f.inputs.iter().position(|x| x.key() == k).unwrap();
    d.form.values[i] = v;
  }

  #[test]
  fn emboss_form_and_its_timeline_edit() {
    let mut s = Session::default();
    run(&mut s, "cylinder", json!({"radius": 20, "height": 40}));
    run(&mut s, "plane.offset", json!({"plane": "xz", "offset": 30}));
    let front = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": front}}));
    run(&mut s, "sketch.rect", json!({"corner1": [-5, 15], "corner2": [5, 25]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let b = s.model.bodies[0].id;
    let side = s.model.bodies[0].faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
    let v0 = volume(&s);
    let sel = vec![Sel::Profile { sketch: sk, index: 0 }, Sel::Face { body: b, face: side }];
    let mut d = form(&mut s, &EMBOSS, sel);
    set(&mut d, &EMBOSS, "wrap", json!(true));
    set(&mut d, &EMBOSS, "depth", json!("2 mm"));
    apply(&mut s, &mut d);
    assert!(((volume(&s) - v0) - 210.0).abs() < 2.0, "{}", volume(&s) - v0);
    // edit from the timeline: deboss instead
    let id = s.design.features.last().unwrap().id;
    let mut e = edit_form(&s.design.feature(id).unwrap().kind).unwrap();
    e.editing = Some(id);
    assert_eq!(e.form.values[3], json!("2 mm"));
    set(&mut e, &EMBOSS, "deboss", json!(true));
    apply(&mut s, &mut e);
    assert!(((v0 - volume(&s)) - 190.0).abs() < 2.0, "{}", v0 - volume(&s));
  }

  #[test]
  fn boundary_fill_and_replace_face_forms() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    run(&mut s, "box", json!({"at": [10, 0], "width": 20, "depth": 20, "height": 20, "operation": "new_body"}));
    let sel: Vec<Sel> = s.model.bodies.iter().map(|b| Sel::Body { body: b.id }).collect();
    let mut d = form(&mut s, &BOUNDARY_FILL, sel);
    let ci = BOUNDARY_FILL.inputs.iter().position(|x| x.key() == "cells").unwrap();
    d.refresh_cells(&BOUNDARY_FILL, ci, "tools", &s);
    assert_eq!(d.form.cells.len(), 3);
    let mid = d.form.cells.iter().find(|(_, v)| (v - 4000.0).abs() < 1e-3).unwrap().0;
    set(&mut d, &BOUNDARY_FILL, "cells", json!([mid]));
    set(&mut d, &BOUNDARY_FILL, "remove_tools", json!(true));
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);
    assert!((volume(&s) - 4000.0).abs() < 1e-3);

    // replace the block's top with the top of a taller box beside it
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
    run(&mut s, "box", json!({"at": [40, 0], "width": 20, "depth": 20, "height": 15, "operation": "new_body"}));
    let top = |s: &Session, i: usize| {
      let b = &s.model.bodies[i];
      Sel::Face { body: b.id, face: b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id }
    };
    let sel = vec![top(&s, 0), top(&s, 1)];
    let mut d = form(&mut s, &REPLACE_FACE, sel);
    assert_eq!(d.form.values[0].as_array().unwrap().len(), 1, "the target is not a source face");
    assert!(!d.form.values[1].is_null());
    apply(&mut s, &mut d);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - 6000.0).abs() < 1e-3);
  }

  #[test]
  fn fillet_setback_sweep_twist_loft_rails_and_extrude_to_object() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let b = &s.model.bodies[0];
    let corner = fox_engine::kernel::glam::DVec3::new(10.0, 10.0, 20.0);
    let edges: Vec<Value> = b
      .edges()
      .iter()
      .filter(|e| (e.start - corner).length() < 1e-6 || (e.end - corner).length() < 1e-6)
      .map(|e| json!({"body": b.id.0, "edge": e.id}))
      .collect();
    let mut d = Dialog::blank(Kind::Fillet);
    d.edges = edges;
    d.radius = "3".into();
    d.solid.setback = "7".into();
    d.solid.setback_corner = true;
    apply(&mut s, &mut d);
    let f = s.design.features.last().unwrap();
    assert!(matches!(&f.kind, FeatureKind::Fillet { setback: Some(x), .. } if x == "7"));

    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [-4, -2], "corner2": [4, 2]}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = s.edit_sketch;
    d.path = Some(path);
    d.solid.twist = "90 deg".into();
    apply(&mut s, &mut d);
    assert!((volume(&s) - 960.0).abs() < 10.0, "{}", volume(&s));
    assert!(s.model.bodies[0].shape.mass().unwrap().bbox_max.y > 3.0);

    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
    let a = s.edit_sketch.unwrap();
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 40}));
    let top = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": top}}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
    let b = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.arc", json!({"start": [10, 0], "mid": [15, 20], "end": [10, 40]}));
    let rail = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Loft);
    let interior = |s: &Session, sk: FeatureId| s.model.sketches[&sk].profiles[0].interior_point();
    d.sections = vec![(a.0, interior(&s, a)), (b.0, interior(&s, b))];
    d.solid.rails = vec![rail];
    apply(&mut s, &mut d);
    let m = s.model.bodies[0].shape.mass().unwrap();
    assert!((m.bbox_max.x - 15.0).abs() < 0.3 && (m.bbox_min.x + 10.0).abs() < 0.3, "{m:?}");

    // extrude a bar up to the cylinder above it, picked in the viewport
    let mut s = Session::default();
    run(&mut s, "cylinder", json!({"plane": "yz", "radius": 20, "height": 40}));
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": -40}));
    let below = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": below}}));
    run(&mut s, "sketch.rect", json!({"corner1": [10, -5], "corner2": [30, 5]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    run(&mut s, "sketch.finish", json!({}));
    d.extent = "to_object";
    assert!(d.command().is_none(), "nothing picked yet");
    s.selection = vec![Sel::Body { body: s.model.bodies[0].id }];
    d.sync_object(&s);
    let v0 = volume(&s);
    apply(&mut s, &mut d);
    let under = 5.0 * 375.0f64.sqrt() + 400.0 * 0.25f64.asin();
    assert!((volume(&s) - v0 - 20.0 * (400.0 - under)).abs() < 1.0, "{}", volume(&s) - v0);
  }
}
