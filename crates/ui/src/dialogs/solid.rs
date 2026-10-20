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
  /// Sweep taper angle, and whether the profile keeps its orientation (parallel).
  taper: String,
  parallel: bool,
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
  /// Fillet radius type Variable: the radius at the edges' ends (`radius` is at their starts).
  pub(super) variable: bool,
  pub(super) radius2: String,
  /// Revolve Type: Full (a whole turn) rather than Angle.
  pub(super) rev_full: bool,
  /// Revolve axis picked in the viewport: a straight body edge `{body, edge}` (axis "edge").
  pub(super) axis_edge: Value,
  /// The axis pick last seen in the viewport selection.
  axis_pick: Value,
  /// Chamfer Type (see [`CHAMFER_TYPES`]) with its second distance or angle.
  pub(super) chamfer_type: &'static str,
  pub(super) distance2: String,
  pub(super) chamfer_angle: String,
  /// Shell Direction (see [`SHELL_DIRECTIONS`]), the outside thickness when both, and tangent
  /// chain off (on by default, as in Fusion).
  pub(super) shell_dir: &'static str,
  pub(super) thickness_out: String,
  pub(super) shell_no_chain: bool,
  /// Hole placement and rectangular pattern options (see [`super::hole_pattern`]).
  pub(super) hp: super::hole_pattern::State,
}

/// Fusion's Chamfer Type.
pub(super) const CHAMFER_TYPES: [(&str, &str); 3] =
  [("equal", "Equal distance"), ("two", "Two distances"), ("angle", "Distance and angle")];
/// Fusion's Shell Direction.
pub(super) const SHELL_DIRECTIONS: [(&str, &str); 3] = [("inside", "Inside"), ("outside", "Outside"), ("both", "Both")];

/// A combo box over `(key, label)` choices; true when the choice changed.
fn choice(ui: &mut egui::Ui, id: &str, cur: &mut &'static str, options: &[(&'static str, &str)]) -> bool {
  let shown = options.iter().find(|(k, _)| k == cur).map(|(_, l)| *l).unwrap_or_default();
  let mut changed = false;
  egui::ComboBox::from_id_salt(id).selected_text(shown).width(140.0).show_ui(ui, |ui| {
    for (k, l) in options {
      changed |= ui.selectable_value(cur, *k, *l).changed();
    }
  });
  changed
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
          let mut r = ui.add(egui::TextEdit::singleline(&mut self.solid.rows[i].radius).desired_width(64.0));
          crate::units::pin(ui, &mut r, &mut self.solid.rows[i].radius);
          r.changed()
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
    let mut kind = match (self.solid.chord, self.solid.variable) {
      (true, _) => 1,
      (_, true) => 2,
      _ => 0,
    };
    ui.horizontal(|ui| {
      changed |= ui.radio_value(&mut kind, 0, "Constant").changed();
      changed |= ui.radio_value(&mut kind, 1, "Chord length").changed();
      changed |= ui.radio_value(&mut kind, 2, "Variable").changed();
    });
    (self.solid.chord, self.solid.variable) = (kind == 1, kind == 2);
    ui.end_row();
    if self.solid.variable {
      // Fusion's variable radius: the first row's radius at the start, this one at the end
      ui.label("End radius");
      if self.solid.radius2.is_empty() {
        self.solid.radius2 = self.radius.clone();
      }
      changed |= super::value_edit_w(ui, &mut self.solid.radius2, 140.0).changed();
      ui.end_row();
    }
    ui.label("Corner type");
    let shown = if self.solid.setback_corner { "Setback" } else { "Rolling ball" };
    egui::ComboBox::from_id_salt("fillet-corner").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      changed |= ui.selectable_value(&mut self.solid.setback_corner, false, "Rolling ball").changed();
      changed |= ui.selectable_value(&mut self.solid.setback_corner, true, "Setback").changed();
    });
    ui.end_row();
    if self.solid.setback_corner {
      ui.label("Setback");
      let mut r =
        ui.add(egui::TextEdit::singleline(&mut self.solid.setback).hint_text("distance").desired_width(140.0));
      crate::units::pin(ui, &mut r, &mut self.solid.setback);
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
    let r2 = if self.solid.variable { self.solid.radius2.trim() } else { "" };
    if !r2.is_empty() {
      p["radius2"] = json!(r2);
    } else if editing {
      p["radius2"] = Value::Null;
    }
    let sb = if self.solid.setback_corner { self.solid.setback.trim() } else { "" };
    if !sb.is_empty() {
      p["setback"] = json!(sb);
    } else if editing {
      p["setback"] = Value::Null;
    }
    p
  }

  pub(super) fn load_setback(&mut self, setback: &Option<String>, chord: bool, radius2: &Option<String>) {
    self.solid.setback = setback.clone().unwrap_or_default();
    self.solid.setback_corner = !self.solid.setback.trim().is_empty();
    self.solid.chord = chord;
    self.solid.radius2 = radius2.clone().unwrap_or_default();
    self.solid.variable = !self.solid.radius2.trim().is_empty();
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

  // ---- revolve ----

  /// Fusion's Type (Angle or Full) and, for Angle, the angle.
  pub(super) fn revolve_type_rows(&mut self, ui: &mut egui::Ui) {
    ui.label("Type");
    let mut changed = false;
    ui.horizontal(|ui| {
      changed |= ui.radio_value(&mut self.solid.rev_full, false, "Angle").changed();
      changed |= ui.radio_value(&mut self.solid.rev_full, true, "Full").changed();
    });
    ui.end_row();
    if !self.solid.rev_full {
      ui.label("Angle");
      changed |= super::value_edit(ui, &mut self.angle).changed();
      ui.end_row();
    }
    if changed {
      self.dirty = true;
    }
  }

  /// The revolve angle: a whole turn for Type Full.
  pub(super) fn revolve_angle(&self) -> String {
    if self.solid.rev_full { "360 deg".into() } else { self.angle.clone() }
  }

  /// The `axis` of the revolve command: X/Y/Z, a sketch line, a construction axis or an edge.
  pub(super) fn revolve_axis(&self) -> Option<Value> {
    Some(match (self.axis.strip_prefix("line:"), self.axis.strip_prefix("axis:")) {
      (Some(n), _) => json!({"line": n.parse::<u32>().ok()?}),
      (_, Some(n)) => json!({"axis": n.parse::<u32>().ok()?}),
      _ if self.axis == "edge" => Some(self.solid.axis_edge.clone()).filter(|v| !v.is_null())?,
      _ => json!(self.axis),
    })
  }

  /// An axis picked in the viewport: a straight body edge, a line of the revolve's sketch or a
  /// construction axis (the last one picked).
  pub(super) fn sync_revolve_axis(&mut self, s: &Session) {
    let picked = s.selection.iter().rev().find_map(|x| match x {
      Sel::Edge { body, edge } => {
        let straight = s.model.body(*body).and_then(|b| b.edges().iter().find(|e| e.id == *edge).map(|e| e.kind));
        (straight == Some(fox_engine::kernel::CurveKind::Line))
          .then(|| ("edge".to_string(), json!({"body": body.0, "edge": edge})))
      }
      Sel::SketchCurve { sketch, curve } if Some(*sketch) == self.sketch => {
        let line = s.design.sketch(*sketch).is_ok_and(|sk| {
          sk.curves.iter().any(|c| c.id == *curve && matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. }))
        });
        line.then(|| (format!("line:{}", curve.0), Value::Null))
      }
      Sel::Feature { feature } if s.model.axes.contains_key(feature) => {
        Some((format!("axis:{}", feature.0), Value::Null))
      }
      _ => None,
    });
    // only a new pick changes the axis (the dropdown may have chosen another since)
    let key = json!(picked);
    if key == self.solid.axis_pick {
      return;
    }
    self.solid.axis_pick = key;
    if let Some((axis, edge)) = picked
      && (axis != self.axis || edge != self.solid.axis_edge)
    {
      self.axis = axis;
      self.solid.axis_edge = edge;
      self.dirty = true;
    }
  }

  // ---- chamfer ----

  /// Fusion's Chamfer Type with its distances or angle.
  pub(super) fn chamfer_rows(&mut self, ui: &mut egui::Ui, creating: bool) {
    if self.solid.chamfer_type.is_empty() {
      self.solid.chamfer_type = "equal";
    }
    ui.label("Chamfer type");
    let mut changed = choice(ui, "chamfer-type", &mut self.solid.chamfer_type, &CHAMFER_TYPES);
    ui.end_row();
    let two = self.solid.chamfer_type == "two";
    ui.label(if two { "Distance 1" } else { "Distance" });
    changed |= super::value_edit(ui, &mut self.radius).changed();
    ui.end_row();
    let second = match self.solid.chamfer_type {
      "two" => Some(("Distance 2", &mut self.solid.distance2)),
      "angle" => Some(("Angle", &mut self.solid.chamfer_angle)),
      _ => None,
    };
    if let Some((label, v)) = second {
      ui.label(label);
      if v.is_empty() {
        *v = if two { self.radius.clone() } else { "45 deg".into() };
      }
      changed |= super::value_edit(ui, v).changed();
      ui.end_row();
    }
    if changed && (!creating || self.applied) {
      self.dirty = true;
    }
  }

  /// Chamfer parameters: the distance and, by type, `distance2` or `angle` (`null` when editing
  /// clears them).
  pub(super) fn chamfer_params(&self, mut p: Value, editing: bool) -> Value {
    p["distance"] = json!(self.radius);
    let (d2, a) = match self.solid.chamfer_type {
      "two" => (json!(self.solid.distance2), Value::Null),
      "angle" => (Value::Null, json!(self.solid.chamfer_angle)),
      _ => (Value::Null, Value::Null),
    };
    for (k, v) in [("distance2", d2), ("angle", a)] {
      if !v.is_null() || editing {
        p[k] = v;
      }
    }
    p
  }

  pub(super) fn load_chamfer(&mut self, distance2: &Option<String>, angle: &Option<String>) {
    self.solid.chamfer_type = match (distance2, angle) {
      (_, Some(a)) => {
        self.solid.chamfer_angle = a.clone();
        "angle"
      }
      (Some(d), None) => {
        self.solid.distance2 = d.clone();
        "two"
      }
      _ => "equal",
    };
  }

  // ---- shell ----

  /// Faces or bodies, Tangent Chain, Direction and the inside / outside thicknesses.
  pub(super) fn shell_rows(&mut self, ui: &mut egui::Ui, creating: bool) {
    if self.solid.shell_dir.is_empty() {
      self.solid.shell_dir = "inside";
    }
    ui.label("Faces / Body");
    let (nf, nb) = (self.faces.len(), self.bodies.len());
    let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    let text = match (nf, nb) {
      (0, 0) => "Select".to_string(),
      (f, 0) => n(f, "face", "faces"),
      (0, b) => n(b, "body", "bodies"),
      (f, b) => format!("{}, {}", n(f, "face", "faces"), n(b, "body", "bodies")),
    };
    if creating {
      sel_chip(ui, self.accent(), &text, nf + nb > 0);
    } else {
      ui.label(format!("{text} (unchanged)"));
    }
    ui.end_row();
    ui.label("Tangent chain");
    let mut chain = !self.solid.shell_no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.shell_no_chain = !chain;
    }
    ui.end_row();
    ui.label("Direction");
    let mut changed = choice(ui, "shell-dir", &mut self.solid.shell_dir, &SHELL_DIRECTIONS);
    ui.end_row();
    let dir = self.solid.shell_dir;
    ui.label(if dir == "outside" { "Outside thickness" } else { "Inside thickness" });
    changed |= super::value_edit(ui, &mut self.radius).changed();
    ui.end_row();
    if dir == "both" {
      ui.label("Outside thickness");
      if self.solid.thickness_out.is_empty() {
        self.solid.thickness_out = self.radius.clone();
      }
      changed |= super::value_edit(ui, &mut self.solid.thickness_out).changed();
      ui.end_row();
    }
    ui.label("");
    changed |= ui.checkbox(&mut self.flag, "Round corners").changed();
    ui.end_row();
    if changed && (!creating || self.applied) {
      self.dirty = true;
    }
  }

  /// The shell command: faces to open, whole bodies, direction and thicknesses.
  pub(super) fn shell_params(&self) -> Value {
    let mut p = json!({
      "faces": self.faces, "bodies": self.bodies, "thickness": self.radius,
      "corners": if self.flag { "round" } else { "sharp" }, "tangent_chain": !self.solid.shell_no_chain,
    });
    self.shell_direction(&mut p, "outside_thickness");
    p
  }

  /// `feature.edit` fields of an edited shell.
  pub(super) fn shell_edit_set(&self) -> Value {
    let mut p = json!({"thickness": self.radius, "round": self.flag, "thickness_out": ""});
    self.shell_direction(&mut p, "thickness_out");
    p
  }

  fn shell_direction(&self, p: &mut Value, out_key: &str) {
    let dir = if self.solid.shell_dir.is_empty() { "inside" } else { self.solid.shell_dir };
    p["direction"] = json!(dir);
    if dir == "both" && !self.solid.thickness_out.trim().is_empty() {
      p[out_key] = json!(self.solid.thickness_out.trim());
    }
  }

  pub(super) fn load_shell(
    &mut self,
    direction: fox_engine::model::ShellDirection,
    thickness_out: &str,
    bodies: usize,
  ) {
    use fox_engine::model::ShellDirection as D;
    self.solid.shell_dir = match direction {
      D::Inside => "inside",
      D::Outside => "outside",
      D::Both => "both",
    };
    self.solid.thickness_out = thickness_out.to_string();
    self.bodies = vec![0; bodies];
  }

  // ---- sweep and loft ----

  pub(super) fn sweep_rows(&mut self, app: &App, ui: &mut egui::Ui, dirty: &mut bool) {
    let list = Self::sketches(app, self.sketch);
    sketch_combo(ui, "Guide rail", "sweep-rail", &list, &mut self.solid.rail, dirty);
    if self.solid.rail.is_none() {
      if !self.solid.parallel {
        Self::field(ui, "Twist angle", &mut self.solid.twist, dirty);
      }
      Self::field(ui, "Taper angle", &mut self.solid.taper, dirty);
      ui.label("Orientation");
      if ui.checkbox(&mut self.solid.parallel, "Parallel").changed() {
        *dirty = true;
      }
      ui.end_row();
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
        } else {
          if self.solid.parallel {
            p["orientation"] = json!("parallel");
          } else if !self.solid.twist.trim().is_empty() {
            p["twist"] = json!(self.solid.twist.trim());
          }
          if !self.solid.taper.trim().is_empty() {
            p["taper"] = json!(self.solid.taper.trim());
          }
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
  use fox_engine::kernel::glam::DVec3;
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

  /// The last feature's dialog, opened from the timeline.
  fn edit_last(s: Session) -> (crate::App, Dialog) {
    let app = crate::App::with_session(s);
    let id = app.s.design.features.last().unwrap().id;
    let d = Dialog::edit(&app, id).unwrap();
    (app, d)
  }

  #[test]
  fn revolve_type_full_and_axis_picked_in_the_canvas() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [12, 0], "corner2": [14, 5]}));
    run(&mut s, "sketch.line", json!({"points": [[30, 0], [30, 10]]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Revolve);
    d.sketch = Some(sk);
    d.op = "new_body";
    // Type Full is a whole turn whatever the angle field says
    d.angle = "90 deg".into();
    d.solid.rev_full = true;
    assert_eq!(d.command().unwrap().1["angle"], json!("360 deg"));
    d.solid.rev_full = false;
    assert_eq!(d.command().unwrap().1["angle"], json!("90 deg"));
    d.solid.rev_full = true;

    // a straight body edge picked in the canvas becomes the axis
    let b = &s.model.bodies[0];
    let edge = b
      .edges()
      .iter()
      .find(|e| e.kind == fox_engine::kernel::CurveKind::Line && (e.mid - DVec3::new(10.0, 10.0, 10.0)).length() < 1e-6)
      .unwrap()
      .id;
    s.selection = vec![Sel::Edge { body: b.id, edge }];
    d.sync_selection(&s);
    assert_eq!(d.axis, "edge");
    assert_eq!(d.command().unwrap().1["axis"], json!({"body": b.id.0, "edge": edge}));
    let v0 = volume(&s);
    apply(&mut s, &mut d);
    // the rectangle (x 12..14 at y = 0) sweeps a 5 mm high ring about the vertical edge at
    // (10, 10): from √(2² + 10²) out to √(4² + 10²)
    let want = std::f64::consts::PI * (116.0 - 104.0) * 5.0;
    assert!((volume(&s) - v0 - want).abs() < 0.5, "{} vs {want}", volume(&s) - v0);
    // the dropdown may choose another axis while the edge stays selected
    d.axis = "z".into();
    d.sync_selection(&s);
    assert_eq!(d.axis, "z");

    // a line of the revolve's sketch (the one beside the rectangle), picked in the canvas
    let line = s
      .design
      .sketch(sk)
      .unwrap()
      .curves
      .iter()
      .rev()
      .find(|c| matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. }))
      .map(|c| c.id)
      .unwrap();
    s.selection = vec![Sel::SketchCurve { sketch: sk, curve: line }];
    d.sync_selection(&s);
    assert_eq!(d.axis, format!("line:{}", line.0));
    assert_eq!(d.command().unwrap().1["axis"], json!({"line": line.0}));
    // a line of another sketch is not this revolve's axis
    s.selection = vec![Sel::SketchCurve { sketch: FeatureId(sk.0 + 100), curve: line }];
    d.sync_selection(&s);
    assert_eq!(d.axis, format!("line:{}", line.0));

    // a full revolve reopens as Type Full; its edit keeps the whole turn
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Revolve);
    d.sketch = Some(sk);
    d.solid.rev_full = true;
    apply(&mut s, &mut d);
    let ring = std::f64::consts::PI * 75.0 * 10.0;
    assert!((volume(&s) - ring).abs() < 0.5, "{}", volume(&s));
    let (mut app, mut e) = edit_last(s);
    assert!(e.solid.rev_full);
    e.editing = Some(app.s.design.features.last().unwrap().id);
    e.solid.rev_full = false;
    e.angle = "180 deg".into();
    apply(&mut app.s, &mut e);
    assert!((volume(&app.s) - ring / 2.0).abs() < 0.5, "{}", volume(&app.s));
  }

  /// A 20 mm cube and the edge along its top at +x (a 20 mm edge between two flat faces).
  fn cube_edge() -> (Session, Value) {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let b = &s.model.bodies[0];
    let e = b.edges().iter().find(|e| (e.mid - DVec3::new(10.0, 0.0, 20.0)).length() < 1e-6).unwrap().id;
    let edge = json!({"body": b.id.0, "edge": e});
    (s, edge)
  }

  #[test]
  fn chamfer_types_and_their_timeline_edit() {
    for (ty, second, cut) in [("equal", "", 10.0), ("two", "2 mm", 20.0), ("angle", "45 deg", 10.0)] {
      let (mut s, edge) = cube_edge();
      let mut d = Dialog::blank(Kind::Chamfer);
      d.edges = vec![edge];
      d.radius = "1 mm".into();
      d.solid.chamfer_type = ty;
      match ty {
        "two" => d.solid.distance2 = second.into(),
        "angle" => d.solid.chamfer_angle = second.into(),
        _ => {}
      }
      let (_, p) = d.command().unwrap();
      assert_eq!(p.get("distance2").cloned(), (ty == "two").then(|| json!(second)), "{ty}: {p}");
      assert_eq!(p.get("angle").cloned(), (ty == "angle").then(|| json!(second)), "{ty}: {p}");
      apply(&mut s, &mut d);
      assert!((8000.0 - volume(&s) - cut).abs() < 1e-6, "{ty}: {}", 8000.0 - volume(&s));
      // reopened from the timeline with its type; switching to equal distance clears the rest
      let (mut app, mut e) = edit_last(s);
      assert_eq!(e.solid.chamfer_type, ty);
      e.editing = Some(app.s.design.features.last().unwrap().id);
      e.solid.chamfer_type = "equal";
      e.radius = "2 mm".into();
      apply(&mut app.s, &mut e);
      assert!((8000.0 - volume(&app.s) - 40.0).abs() < 1e-6, "{ty}: {}", 8000.0 - volume(&app.s));
      let f = app.s.design.features.last().unwrap();
      assert!(matches!(&f.kind, FeatureKind::Chamfer { distance2: None, angle: None, .. }), "{:?}", f.kind);
    }
  }

  #[test]
  fn variable_radius_fillet() {
    let (mut s, edge) = cube_edge();
    let mut d = Dialog::blank(Kind::Fillet);
    d.edges = vec![edge];
    d.radius = "2 mm".into();
    d.solid.variable = true;
    d.solid.radius2 = "5 mm".into();
    assert_eq!(d.command().unwrap().1["radius2"], json!("5 mm"));
    apply(&mut s, &mut d);
    let cut = 8000.0 - volume(&s);
    let round = |r: f64| (1.0 - std::f64::consts::FRAC_PI_4) * r * r * 20.0;
    assert!(cut > round(2.0) + 1.0 && cut < round(5.0) - 1.0, "{cut}");
    let (mut app, mut e) = edit_last(s);
    assert!(e.solid.variable && e.solid.radius2 == "5 mm");
    e.editing = Some(app.s.design.features.last().unwrap().id);
    e.solid.variable = false;
    apply(&mut app.s, &mut e);
    assert!((8000.0 - volume(&app.s) - round(2.0)).abs() < 1e-3, "{}", 8000.0 - volume(&app.s));
    assert!(matches!(&app.s.design.features.last().unwrap().kind, FeatureKind::Fillet { radius2: None, .. }));
    // constant dialogs send no end radius
    let mut d = Dialog::blank(Kind::Fillet);
    d.edges = vec![json!({"body": 1, "edge": 1})];
    assert!(d.command().unwrap().1.get("radius2").is_none());
  }

  #[test]
  fn shell_direction_body_pick_and_timeline_edit() {
    // a face picked: walls outside the cube
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let b = &s.model.bodies[0];
    let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let sel = vec![Sel::Face { body: b.id, face: top }];
    let mut d = Dialog::blank(Kind::Shell);
    s.selection = sel;
    d.sync_selection(&s);
    d.solid.shell_dir = "outside";
    let (_, p) = d.command().unwrap();
    assert_eq!((p["direction"].clone(), p["tangent_chain"].clone()), (json!("outside"), json!(true)), "{p}");
    assert!(p.get("outside_thickness").is_none());
    apply(&mut s, &mut d);
    assert!((volume(&s) - (24.0 * 24.0 * 22.0 - 8000.0)).abs() < 1e-3, "{}", volume(&s));

    // the whole body picked: hollow and closed, 2 mm inside and 1 mm outside
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let mut d = Dialog::blank(Kind::Shell);
    s.selection = vec![Sel::Body { body: s.model.bodies[0].id }];
    d.sync_selection(&s);
    assert!(d.ready() && d.faces.is_empty());
    d.solid.shell_dir = "both";
    d.solid.thickness_out = "1 mm".into();
    assert_eq!(d.command().unwrap().1["outside_thickness"], json!("1 mm"));
    apply(&mut s, &mut d);
    assert!((volume(&s) - (22f64.powi(3) - 16f64.powi(3))).abs() < 1e-3, "{}", volume(&s));
    // reopened: both directions, the body; a thicker outside wall
    let (mut app, mut e) = edit_last(s);
    assert_eq!((e.solid.shell_dir, e.solid.thickness_out.as_str(), e.bodies.len()), ("both", "1 mm", 1));
    e.editing = Some(app.s.design.features.last().unwrap().id);
    e.solid.thickness_out = "2 mm".into();
    apply(&mut app.s, &mut e);
    assert!((volume(&app.s) - (24f64.powi(3) - 16f64.powi(3))).abs() < 1e-3, "{}", volume(&app.s));
    // and back to inside only
    e.solid.shell_dir = "inside";
    e.dirty = true;
    apply(&mut app.s, &mut e);
    assert!((volume(&app.s) - (8000.0 - 16f64.powi(3))).abs() < 1e-3, "{}", volume(&app.s));
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
