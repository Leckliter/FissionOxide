//! Dialog parts for the second batch of solid features: setback fillets, sweep guide rails and
//! twist, loft rails and centre line, extrude to an object, and the emboss / boundary fill /
//! replace face forms (their extra inputs and timeline editing).

use fox_engine::model::{BodyId, FeatureId, FeatureKind, OriginPlane, PlaneRef};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{Choice, choice as ic, icon_row};
use super::{Dialog, Kind, sel_chip};
use crate::App;
use crate::forms::{Form, Input};
use crate::icons::Icon;
use crate::theme;

/// Values the solid feature extensions add to a dialog.
#[derive(Debug, Clone, Default)]
pub(super) struct Extra {
  pub(super) setback: String,
  /// Fillet radius given as the chord length (the round's width).
  chord: bool,
  pub(super) rail: Option<FeatureId>,
  /// Sweep: a second guide rail (with `rail`), how the profile follows the rails ("" = the
  /// default: stretch between two rails, else scale) and a guide surface (`{body, face}`).
  pub(super) rail2: Option<FeatureId>,
  pub(super) scaling: &'static str,
  pub(super) guide_surface: Value,
  pub(super) twist: String,
  /// Sweep taper angle, and whether the profile keeps its orientation (parallel).
  pub(super) taper: String,
  pub(super) parallel: bool,
  /// Loft rails and centre line: each a sketch with its curves (empty = all of them).
  pub(super) rails: Vec<(FeatureId, Vec<u32>)>,
  pub(super) centerline: Option<(FeatureId, Vec<u32>)>,
  /// Extrude to object: the picked face or body (`{body, face}` or `{body}`).
  pub(super) to: Value,
  /// When editing an extrude to an object: its extent kind and stored reference.
  to_kind: &'static str,
  to_ref: Value,
  /// Fillet selection sets (empty: just the one in the dialog's fields) and the active one.
  pub(super) rows: Vec<FilletRow>,
  active: usize,
  /// Fillet Type (see [`FILLET_TYPES`]): "" (Fillet) or "full_round".
  pub(super) fillet_type: &'static str,
  /// Full Round Fillet's picks: side faces 1, centre faces, side faces 2; and the set the next
  /// picked face goes to.
  pub(super) round: [Vec<Value>; 3],
  round_active: usize,
  /// Fillet Tangency Weight (curvature continuity only, which the engine doesn't do yet).
  tangency_weight: String,
  /// Fillet: tangent chain off (on by default, as in Fusion).
  pub(super) no_chain: bool,
  /// Fillet corner type: setback (else rolling ball).
  pub(super) setback_corner: bool,
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
  /// Press Pull on a primitive's top face: the primitive it edits (see [`super::press_owner`]).
  pub(super) owner: super::press_owner::Owner,
  /// Revolve, Sweep and Loft rows (see [`super::rsl`]).
  pub(super) rsl: super::rsl::State,
  /// The operation's automatic Cut (see [`super::auto_cut`]).
  pub(super) cut: super::auto_cut::Cut,
}

/// Fusion's Fillet Type; Rule Fillet has no engine support and is greyed.
pub(super) const FILLET_TYPES: [(&str, &str); 3] =
  [("", "Fillet"), ("rule", "Rule Fillet"), ("full_round", "Full Round Fillet")];
/// Fusion's Radius Type.
const RADIUS_TYPES: [(&str, &str); 3] = [("constant", "Constant"), ("chord", "Chord length"), ("variable", "Variable")];
/// A fillet set's continuity; curvature (G2) has no engine support and is greyed.
const CONTINUITIES: [(&str, &str); 2] = [("", "Tangent (G1)"), ("g2", "Curvature (G2)")];
/// Full Round Fillet's selection rows.
const ROUND_SETS: [&str; 3] = ["Side Face 1", "Center Faces", "Side Face 2"];

/// Fusion's Chamfer Type.
pub(super) const CHAMFER_TYPES: [(&str, &str); 3] =
  [("equal", "Equal distance"), ("two", "Two distances"), ("angle", "Distance and angle")];
/// Fusion's Shell Direction.
pub(super) const SHELL_DIRECTIONS: [(&str, &str); 3] = [("inside", "Inside"), ("outside", "Outside"), ("both", "Both")];
/// Fusion's Shell Type: how the offset faces meet at the corners (the engine's `corners`).
const SHELL_TYPES: [Choice; 2] =
  [ic("sharp", Icon::ShellSharp, "Sharp corners"), ic("round", Icon::ShellRound, "Rounded corners")];
/// Fusion's Chamfer Corner Type; the engine chamfers the corners, so the rest are greyed.
const CORNER_TYPES: [(&str, &str); 3] = [("chamfer", "Chamfer"), ("miter", "Miter"), ("blend", "Blend")];
/// Fusion's Offset Face Offset Type (Press Pull on a face). Automatic and New Offset both add an
/// Offset Face feature; Modify Existing Feature needs the engine to edit the face's feature.
const OFFSET_TYPES: [(&str, &str, bool); 3] =
  [("automatic", "Automatic", true), ("modify", "Modify Existing Feature", false), ("new", "New Offset", true)];

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

/// One fillet selection set: edges, faces (all their edges), its radius and continuity (see
/// [`CONTINUITIES`]).
#[derive(Debug, Clone, Default)]
pub(super) struct FilletRow {
  pub(super) edges: Vec<Value>,
  pub(super) faces: Vec<Value>,
  pub(super) radius: String,
  pub(super) continuity: &'static str,
}

impl FilletRow {
  fn is_empty(&self) -> bool {
    self.edges.is_empty() && self.faces.is_empty()
  }

  /// "1 Face", "2 Edges", "1 Face, 2 Edges" or "Select".
  pub(super) fn label(&self) -> String {
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

  /// Fusion's FILLET rows: Type, the selection-set table (one row per set: its picks, radius,
  /// setback and continuity, with ✕) and +, Radius Type, the active set's Edges/Faces/Features
  /// chip, Tangent Chain and Tangency Weight. The active set is the one in `edges` / `faces` /
  /// `radius` (and the viewport selection). Full Round Fillet shows its three face rows instead.
  pub(super) fn fillet_rows(&mut self, app: &mut App, ui: &mut egui::Ui, creating: bool) {
    ui.label("Type");
    let shown = FILLET_TYPES.iter().find(|(k, _)| *k == self.solid.fillet_type).map_or("Fillet", |(_, l)| *l);
    let mut switch_type = None;
    egui::ComboBox::from_id_salt("fillet-type").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      for (k, l) in FILLET_TYPES {
        if k == "rule" {
          ui.add_enabled(false, egui::Button::selectable(false, l))
            .on_disabled_hover_text("Rule fillets are not supported yet");
        } else if ui.selectable_label(self.solid.fillet_type == k, l).clicked() && self.solid.fillet_type != k {
          switch_type = Some(k);
        }
      }
    });
    ui.end_row();
    if let Some(k) = switch_type.filter(|_| self.editing.is_none()) {
      self.set_fillet_type(&mut app.s, k);
    }
    if self.solid.fillet_type == "full_round" {
      self.full_round_rows(ui, creating);
      return;
    }
    let rows = self.fillet_sets();
    let accent = self.accent();
    let (mut switch, mut remove, mut add) = (None, None, false);
    let mut changed = false;
    for (i, row) in rows.iter().enumerate() {
      // the set's picks; clicking another set's makes it the active one
      let active = i == self.solid.active;
      let text = if creating { row.label() } else { format!("{} (unchanged)", row.edges.len()) };
      let cell = ui.add(egui::Label::new(egui::RichText::new(text).strong()).sense(egui::Sense::click()));
      if !active && creating && cell.on_hover_text("Select this set").clicked() {
        switch = Some(i);
      }
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        // the active set's radius takes typing aimed at the dialog
        changed |= if active {
          super::value_edit_w(ui, &mut self.radius, 52.0).changed()
        } else {
          let mut r = ui.add(egui::TextEdit::singleline(&mut self.solid.rows[i].radius).desired_width(52.0));
          crate::units::pin(ui, &mut r, &mut self.solid.rows[i].radius);
          r.changed()
        };
        // the corner setback, for Corner Type Setback
        let mut sb = ui.add_enabled(
          self.solid.setback_corner,
          egui::TextEdit::singleline(&mut self.solid.setback).hint_text("setback").desired_width(44.0),
        );
        if self.solid.setback_corner {
          crate::units::pin(ui, &mut sb, &mut self.solid.setback);
        }
        changed |= sb.changed();
        // continuity: tangent; curvature has no engine support yet
        let cont = CONTINUITIES.iter().find(|(k, _)| *k == row.continuity).map_or("Tangent (G1)", |(_, l)| *l);
        egui::ComboBox::from_id_salt(("fillet-continuity", i)).selected_text(cont).width(96.0).show_ui(ui, |ui| {
          for (k, l) in CONTINUITIES {
            if k == "g2" {
              ui.add_enabled(false, egui::Button::selectable(false, l))
                .on_disabled_hover_text("Curvature continuity is not supported yet");
            } else {
              let _ = ui.selectable_label(row.continuity == k, l);
            }
          }
        });
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
    changed |= self.radius_type_row(ui);
    // the active set's picks
    ui.label("Edges/Faces/Features");
    let row = &rows[self.solid.active.min(rows.len() - 1)];
    if creating {
      sel_chip(ui, accent, &row.label(), !row.is_empty());
    } else {
      ui.label(format!("{} (unchanged)", row.edges.len()));
    }
    ui.end_row();
    self.tangent_chain_row(ui, creating);
    ui.label("Tangency weight");
    if self.solid.tangency_weight.is_empty() {
      self.solid.tangency_weight = "1.00".into();
    }
    ui.add_enabled(false, egui::TextEdit::singleline(&mut self.solid.tangency_weight).desired_width(140.0))
      .on_disabled_hover_text("Tangency weight applies to curvature continuity, which is not supported yet");
    ui.end_row();
    if changed {
      self.dirty = !creating || self.applied;
      self.live_reset();
    }
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

  /// Fusion's Tangent Chain checkbox (on by default).
  fn tangent_chain_row(&mut self, ui: &mut egui::Ui, creating: bool) {
    ui.label("Tangent chain");
    let mut chain = !self.solid.no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.no_chain = !chain;
    }
    ui.end_row();
  }

  /// Radius Type: Constant, Chord length or Variable (with its end radius); true when changed.
  fn radius_type_row(&mut self, ui: &mut egui::Ui) -> bool {
    ui.label("Radius type");
    let mut kind = match (self.solid.chord, self.solid.variable) {
      (true, _) => "chord",
      (_, true) => "variable",
      _ => "constant",
    };
    let mut changed = choice(ui, "fillet-radius-type", &mut kind, &RADIUS_TYPES);
    (self.solid.chord, self.solid.variable) = (kind == "chord", kind == "variable");
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
    changed
  }

  /// Full Round Fillet: Side Face 1, Center Faces and Side Face 2 (a click on a row's chip makes
  /// it the one the next picked faces go to), and Tangent Chain.
  fn full_round_rows(&mut self, ui: &mut egui::Ui, creating: bool) {
    let accent = self.accent();
    for (i, label) in ROUND_SETS.iter().enumerate() {
      ui.label(*label);
      let n = self.solid.round[i].len();
      let active = i == self.solid.round_active;
      let text = match n {
        0 => "Select".to_string(),
        1 => "1 Face".to_string(),
        n => format!("{n} Faces"),
      };
      let chip = sel_chip(ui, accent, &text, active || n > 0);
      if creating && !active && chip.interact(egui::Sense::click()).on_hover_text("Pick faces for this set").clicked() {
        self.solid.round_active = i;
      }
      ui.end_row();
    }
    self.tangent_chain_row(ui, creating);
  }

  /// Switches the Fillet Type, dropping the preview: the picked faces so far become Full Round's
  /// first set, or the plain fillet's faces.
  fn set_fillet_type(&mut self, s: &mut Session, kind: &'static str) {
    self.withdraw(s);
    self.solid.fillet_type = kind;
    self.solid.round = Default::default();
    self.solid.round_active = 0;
    self.round_sync();
    self.live_reset();
  }

  /// Keeps Full Round Fillet's sets in step with the picked faces: faces newly picked go to the
  /// active set, faces unpicked leave theirs.
  pub(super) fn round_sync(&mut self) {
    if self.solid.fillet_type != "full_round" {
      return;
    }
    let faces = self.faces.clone();
    for set in self.solid.round.iter_mut() {
      set.retain(|f| faces.contains(f));
    }
    let known: Vec<Value> = self.solid.round.iter().flatten().cloned().collect();
    let active = self.solid.round_active.min(2);
    self.solid.round[active].extend(faces.into_iter().filter(|f| !known.contains(f)));
  }

  /// Full Round Fillet's command: one full round per centre face, between the two side sets.
  fn full_round_params(&self) -> Value {
    let [side_one, centers, side_two] = &self.solid.round;
    let sets: Vec<Value> =
      centers.iter().map(|c| json!({"center": c, "side_one": side_one, "side_two": side_two})).collect();
    json!({"full_round": sets})
  }

  /// Radius type and Fusion's Corner Type: rolling ball, or setback (its distance is in the
  /// selection table). Full Round Fillet has neither.
  pub(super) fn setback_row(&mut self, ui: &mut egui::Ui, creating: bool) {
    if self.solid.fillet_type == "full_round" {
      return;
    }
    let mut changed = false;
    ui.label("Corner type");
    let shown = if self.solid.setback_corner { "Setback" } else { "Rolling ball" };
    egui::ComboBox::from_id_salt("fillet-corner").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      changed |= ui.selectable_value(&mut self.solid.setback_corner, false, "Rolling ball").changed();
      changed |= ui.selectable_value(&mut self.solid.setback_corner, true, "Setback").changed();
    });
    ui.end_row();
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
        continuity: "",
      });
    }
    self.solid.edit_sets = serde_json::to_value(sets).unwrap_or_default();
  }

  /// Every selection set, the active one as it is now.
  pub(super) fn fillet_sets(&self) -> Vec<FilletRow> {
    let continuity = self.solid.rows.get(self.solid.active).map_or("", |r| r.continuity);
    let now =
      FilletRow { edges: self.edges.clone(), faces: self.faces.clone(), radius: self.radius.clone(), continuity };
    if self.solid.rows.is_empty() {
      return vec![now];
    }
    let mut rows = self.solid.rows.clone();
    rows[self.solid.active] = now;
    rows
  }

  /// The fillet command: one set, or `sets` each with its radius; tangent chains on by default.
  /// A Full Round Fillet's command instead (see [`Dialog::full_round_params`]).
  pub(super) fn fillet_params(&self) -> Value {
    if self.solid.fillet_type == "full_round" {
      return self.full_round_params();
    }
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
    if self.solid.fillet_type == "full_round" {
      return self.solid.round.iter().all(|set| !set.is_empty());
    }
    self.fillet_sets().iter().any(|r| !r.is_empty())
  }

  pub(super) fn store_fillet_set(&mut self) {
    let continuity = self.solid.rows.get(self.solid.active).map_or("", |r| r.continuity);
    let now =
      FilletRow { edges: self.edges.clone(), faces: self.faces.clone(), radius: self.radius.clone(), continuity };
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
      // none picked yet
      _ if self.axis.is_empty() => return None,
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
    // only a new pick changes the axis (an origin axis may have been clicked since)
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

  /// The picked edges with their tangent chains (Fusion's Tangent Chain, on by default).
  pub(super) fn chamfer_picks(&self, s: &Session) -> Vec<Value> {
    let picked = super::sel_edges(s);
    if self.solid.no_chain { picked } else { super::with_tangent_chain(s, &picked) }
  }

  /// Fusion's Chamfer dialog: the selection-set table (the edges with their distances, + and ✕),
  /// Type, the Edges chip, Tangent Chain and Corner Type.
  pub(super) fn chamfer_rows(&mut self, app: &mut App, ui: &mut egui::Ui, creating: bool) {
    if self.solid.chamfer_type.is_empty() {
      self.solid.chamfer_type = "equal";
    }
    let kind = self.solid.chamfer_type;
    let n = self.edges.len();
    let edges = if n == 0 { "Select".to_string() } else { format!("{n} Edge{}", if n == 1 { "" } else { "s" }) };
    // the table: one set, its edges and the two distances (or the distance and angle)
    let mut changed = false;
    let mut clear = false;
    ui.label("");
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 4.0;
      ui.label(egui::RichText::new(&edges).color(if n > 0 { theme::TEXT } else { theme::MUTED }));
      ui.label(egui::RichText::new("|").color(theme::MUTED));
      changed |= super::value_edit_w(ui, &mut self.radius, 56.0).changed();
      ui.label(egui::RichText::new("|").color(theme::MUTED));
      match kind {
        "two" => {
          if self.solid.distance2.is_empty() {
            self.solid.distance2 = self.radius.clone();
          }
          let mut r = ui.add(egui::TextEdit::singleline(&mut self.solid.distance2).desired_width(56.0));
          crate::units::pin(ui, &mut r, &mut self.solid.distance2);
          changed |= r.changed();
        }
        "angle" => {
          if self.solid.chamfer_angle.is_empty() {
            self.solid.chamfer_angle = "45 deg".into();
          }
          changed |= ui.add(egui::TextEdit::singleline(&mut self.solid.chamfer_angle).desired_width(56.0)).changed();
        }
        // equal distance: the second column reads the same as the first
        _ => {
          let mut same = self.radius.clone();
          ui.add_enabled(false, egui::TextEdit::singleline(&mut same).desired_width(56.0));
        }
      }
      ui.add_enabled(false, egui::Button::new("+").small())
        .on_disabled_hover_text("Selection sets with their own distances need chamfer sets in the engine");
      if creating && ui.add_enabled(n > 0, egui::Button::new("✕").small()).on_hover_text("Clear this set").clicked() {
        clear = true;
      }
    });
    ui.end_row();
    ui.label("Type");
    let type_changed = choice(ui, "chamfer-type", &mut self.solid.chamfer_type, &CHAMFER_TYPES);
    changed |= type_changed;
    ui.end_row();
    ui.label("Edges");
    if creating {
      sel_chip(ui, self.accent(), &edges, n > 0);
    } else {
      ui.label(format!("{n} (unchanged)"));
    }
    ui.end_row();
    ui.label("Tangent Chain");
    let mut chain = !self.solid.no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.no_chain = !chain;
      if !self.applied {
        self.edges = self.chamfer_picks(&app.s);
      }
      changed = true;
    }
    ui.end_row();
    ui.label("Corner Type");
    ui.add_enabled_ui(false, |ui| {
      let mut corner: &'static str = "chamfer";
      choice(ui, "chamfer-corner", &mut corner, &CORNER_TYPES);
    })
    .response
    .on_disabled_hover_text("Corner Type needs engine support; the corners are chamfered");
    ui.end_row();
    if clear {
      self.withdraw(&mut app.s);
      self.edges.clear();
      app.s.selection.clear();
      self.live_reset();
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
    ui.label("Tangent Chain");
    let mut chain = !self.solid.shell_no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.shell_no_chain = !chain;
    }
    ui.end_row();
    ui.label("Shell Type");
    let mut shell_type: &'static str = if self.flag { "round" } else { "sharp" };
    let mut changed = icon_row(ui, &mut shell_type, &SHELL_TYPES, "");
    self.flag = shell_type == "round";
    ui.end_row();
    let dir = self.solid.shell_dir;
    ui.label(if dir == "outside" { "Outside Thickness" } else { "Inside Thickness" });
    changed |= super::value_edit(ui, &mut self.radius).changed();
    ui.end_row();
    if dir == "both" {
      ui.label("Outside Thickness");
      if self.solid.thickness_out.is_empty() {
        self.solid.thickness_out = self.radius.clone();
      }
      changed |= super::value_edit(ui, &mut self.solid.thickness_out).changed();
      ui.end_row();
    }
    ui.label("Direction");
    changed |= choice(ui, "shell-dir", &mut self.solid.shell_dir, &SHELL_DIRECTIONS);
    ui.end_row();
    if changed && (!creating || self.applied) {
      self.dirty = true;
    }
  }

  // ---- press pull / offset face ----

  /// Fusion's Offset Type row (Press Pull on a face).
  pub(super) fn offset_type_row(&mut self, ui: &mut egui::Ui) {
    ui.label("Offset Type");
    let shown = OFFSET_TYPES.iter().find(|(k, _, _)| *k == self.choice).map_or("Automatic", |(_, l, _)| *l);
    egui::ComboBox::from_id_salt("offset-type")
      .selected_text(shown)
      .width(140.0)
      .show_ui(ui, |ui| {
        for (k, l, ok) in OFFSET_TYPES {
          ui.add_enabled_ui(ok, |ui| {
            if ui.selectable_label(self.choice == k, l).clicked() {
              self.choice = k;
            }
          })
          .response
          .on_disabled_hover_text("Editing the face's own feature needs engine support");
        }
      })
      .response
      .on_hover_text("Automatic and New Offset add an Offset Face feature");
    ui.end_row();
  }

  // ---- sketch offset ----

  /// The picked curves, with the curves chained to them when Chain Selection is on.
  pub(super) fn offset_picks(&self, s: &Session) -> Vec<u32> {
    let picked = super::sel_curves(s);
    if self.solid.no_chain {
      return picked;
    }
    match s.edit_sketch.and_then(|sk| s.design.sketch(sk).ok()) {
      Some(sketch) => super::with_curve_chain(sketch, &picked),
      None => picked,
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

  /// Sweep / loft parameters with the guide rail, twist, rails and centre line.
  pub(super) fn with_guides(&self, mut p: Value) -> Value {
    match self.kind {
      Kind::Sweep => {
        if !self.solid.rsl.path_curves.is_empty() {
          p["path_curves"] = json!(self.solid.rsl.path_curves);
        }
        if let Some(r) = self.solid.rail {
          p["rail"] = json!(r.0);
          if let Some(r2) = self.solid.rail2.filter(|r2| *r2 != r) {
            p["rail2"] = json!(r2.0);
          }
          if !self.solid.scaling.is_empty() {
            p["profile_scaling"] = json!(self.solid.scaling);
          }
        } else if !self.solid.guide_surface.is_null() {
          p["guide_surface"] = self.solid.guide_surface.clone();
        } else {
          if self.solid.parallel {
            p["orientation"] = json!("parallel");
          } else if super::rsl::angle_set(&self.solid.twist) {
            p["twist"] = json!(self.solid.twist.trim());
          }
          if super::rsl::angle_set(&self.solid.taper) {
            p["taper"] = json!(self.solid.taper.trim());
          }
        }
      }
      Kind::Loft => {
        let guide = |(sk, c): &(FeatureId, Vec<u32>)| {
          if c.is_empty() { json!({"sketch": sk.0}) } else { json!({"sketch": sk.0, "curves": c}) }
        };
        if let Some(c) = &self.solid.centerline {
          p["centerline"] = guide(c);
        } else if !self.solid.rails.is_empty() {
          p["rails"] = json!(self.solid.rails.iter().map(guide).collect::<Vec<_>>());
        }
      }
      _ => {}
    }
    p
  }

  /// The face last picked in the viewport, as a sweep's guide surface.
  pub(super) fn sync_guide_surface(&mut self, s: &Session) {
    let picked = s.selection.iter().rev().find_map(|x| match x {
      Sel::Face { body, face } => Some(json!({"body": body.0, "face": face})),
      _ => None,
    });
    if let Some(f) = picked
      && f != self.solid.guide_surface
      && self.solid.rail.is_none()
    {
      self.solid.guide_surface = f;
      self.dirty = true;
    }
  }

  // ---- extrude to an object ----

  pub(super) fn object_row(&mut self, ui: &mut egui::Ui) {
    ui.label("Object");
    if self.editing.is_some() {
      ui.label("(unchanged)");
    } else {
      let has = !self.solid.to.is_null();
      let text = if !has {
        "Select a face, body or vertex"
      } else if self.solid.to.get("face").is_some() {
        "1 face"
      } else if self.solid.to.get("vertex").is_some() {
        "1 vertex"
      } else if is_point(&self.solid.to) {
        "1 point"
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
        // (not a face extruded as a profile)
        Sel::Face { body, face } => Some(json!({"body": body.0, "face": face})).filter(|f| !self.faces.contains(f)),
        Sel::Body { body } => Some(json!({"body": body.0})),
        // to a vertex: the plane through it parallel to the sketch
        Sel::Vertex { body, edge, end } => Some(json!({"vertex": {"body": body.0, "edge": edge}, "end": end})),
        Sel::SketchPoint { sketch, point } => {
          let r = s.model.sketches.get(sketch)?;
          Some(json!({"at": r.plane.to_world(r.sketch.pos(*point)?).to_array()}))
        }
        Sel::Feature { feature } if s.model.points.contains_key(feature) => Some(json!({"point": feature.0})),
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
    p["extent"] = json!(if self.solid.to.get("face").is_some() {
      "to_face"
    } else if is_point(&self.solid.to) {
      "to_vertex"
    } else {
      "to_body"
    });
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
      extent[if self.solid.to_kind == "to_vertex" { "to_point" } else { "to" }] = self.solid.to_ref.clone();
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
        // Emboss waits for its picks (Select); elsewhere none picked means every profile
        let none = if f.cmd == "emboss" { "Select" } else { "All" };
        sel_chip(ui, accent, &if n == 0 { none.to_string() } else { format!("{n} selected") }, n > 0);
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
        // Fusion's Select Cells: a chip, and the cells the tools bound to tick below it
        self.refresh_cells(f, i, tools, s);
        ui.label(*label);
        let picked: Vec<Value> = v.as_array().cloned().unwrap_or_default();
        let n = picked.len();
        sel_chip(ui, accent, &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
        ui.end_row();
        if self.form.cells.is_empty() {
          return;
        }
        ui.label("");
        ui.vertical(|ui| {
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
        });
        ui.end_row();
      }
      _ => {}
    }
  }

  /// Recomputes the boundary fill cells when the tools (bodies or planes) changed.
  fn refresh_cells(&mut self, f: &'static Form, i: usize, tools: &str, s: &Session) {
    let tool_v = f.inputs.iter().position(|x| x.key() == tools).map(|k| self.form.values[k].clone());
    let planes: Vec<Value> = f
      .inputs
      .iter()
      .zip(&self.form.values)
      .filter(|(x, _)| matches!(x, Input::Planes { .. }))
      .flat_map(|(_, v)| v.as_array().cloned().unwrap_or_default())
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
  d.form.edit = true;
  let v = serde_json::to_value(kind).ok()?;
  for (i, input) in f.inputs.iter().enumerate() {
    let editable = matches!(
      input,
      Input::Expr { .. } | Input::Text { .. } | Input::Check { .. } | Input::Choice { .. } | Input::Bodies { .. }
    ) || matches!(input, Input::Cells { .. });
    if let Some(x) = v.get(input.key()).filter(|_| editable) {
      d.form.values[i] = x.clone();
    }
    // Emboss's Effect toggles are its deboss flag
    if input.key() == "effect" {
      let deboss = v.get("deboss").and_then(Value::as_bool).unwrap_or(false);
      d.form.values[i] = json!(if deboss { "deboss" } else { "emboss" });
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
      Input::Expr { .. }
        | Input::Text { .. }
        | Input::Check { .. }
        | Input::Choice { .. }
        | Input::Cells { .. }
        | Input::Icons { .. }
    ) {
      set.insert(input.key().to_string(), v.clone());
    }
  }
  // the rows' values as the feature's fields (Emboss's Effect is its deboss flag)
  crate::forms::engine_keys(f.cmd, &mut set);
  Value::Object(set)
}

/// Whether a "to object" pick is a point (a vertex, sketch point or construction point).
fn is_point(to: &Value) -> bool {
  ["vertex", "at", "point"].iter().any(|k| to.get(*k).is_some())
}

#[cfg(test)]
mod tests {
  use fox_engine::kernel::SurfaceKind;
  use fox_engine::kernel::glam::DVec3;
  use serde_json::{Value, json};

  use super::*;
  use crate::forms::{BOUNDARY_FILL, EMBOSS, REPLACE_FACE, RIB, THICKEN, THREAD, WEB};

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
    d.profiles = vec![s.model.sketches[&sk].profiles[0].interior_point()];
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
    d.profiles = vec![s.model.sketches[&sk].profiles[0].interior_point()];
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

  /// Fillet Type: Full Round Fillet picks three face sets (the active one takes the next picks)
  /// and runs the full round command; the plain type's rows carry their continuity.
  #[test]
  fn fillet_type_full_round_and_set_continuity() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
    let b = s.model.bodies[0].id;
    let face = |s: &Session, n: DVec3| {
      let f = s.model.bodies[0].faces().iter().find(|f| f.normal.dot(n) > 0.9).unwrap().id;
      (json!({"body": b.0, "face": f}), Sel::Face { body: b, face: f })
    };
    let (top, top_sel) = face(&s, DVec3::Z);
    let (bottom, bottom_sel) = face(&s, DVec3::NEG_Z);
    let (side, side_sel) = face(&s, DVec3::X);
    let mut d = Dialog::blank(Kind::Fillet);
    assert_eq!(d.solid.fillet_type, "");
    assert_eq!(d.fillet_sets()[0].continuity, "", "tangent (G1) by default");
    // picked before the switch, the top face becomes Side Face 1
    s.selection = vec![top_sel];
    d.sync_selection(&s);
    d.set_fillet_type(&mut s, "full_round");
    assert_eq!(d.solid.round, [vec![top.clone()], vec![], vec![]]);
    assert!(!d.fillet_ready(), "three sets are needed");
    // the next picks go to the active set: the centre, then side 2
    d.solid.round_active = 1;
    s.selection = vec![top_sel, side_sel];
    d.sync_selection(&s);
    d.solid.round_active = 2;
    s.selection = vec![top_sel, side_sel, bottom_sel];
    d.sync_selection(&s);
    assert_eq!(d.solid.round, [vec![top.clone()], vec![side.clone()], vec![bottom.clone()]]);
    assert!(d.fillet_ready());
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "fillet");
    assert_eq!(p, json!({"full_round": [{"center": side, "side_one": [top], "side_two": [bottom]}]}));
    apply(&mut s, &mut d);
    assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::FullRoundFillet { .. }));
    // the rounded side is a full cylinder between top and bottom: less volume than the box
    assert!(volume(&s) < 4000.0 - 1.0, "{}", volume(&s));
    d.cancel(&mut s);
    // unpicking a face leaves its set; switching back to Fillet keeps the faces as picks
    s.selection = vec![top_sel, bottom_sel];
    d.sync_selection(&s);
    assert_eq!(d.solid.round[1], Vec::<Value>::new());
    d.set_fillet_type(&mut s, "");
    assert_eq!(d.faces.len(), 2);
    assert!(d.fillet_ready() && d.command().unwrap().1.get("full_round").is_none());
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
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
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
    // Fusion's rows: Sketch Profiles and Faces chips, Tangent Chain, Effect; OK waits for the
    // picks and Depth and the placement rows come after them
    let d = form(&mut s, &EMBOSS, vec![]);
    assert_eq!(d.form.missing(&EMBOSS), Some("Sketch Profiles"));
    assert!(!d.ready());
    let shown = |d: &Dialog, k: &str| d.form.shown(&EMBOSS, EMBOSS.inputs.iter().position(|x| x.key() == k).unwrap());
    assert!(!shown(&d, "depth") && !shown(&d, "rotation") && shown(&d, "effect"));
    assert!(crate::forms::always_previews(&EMBOSS));
    let sel = vec![Sel::Profile { sketch: sk, index: 0 }, Sel::Face { body: b, face: side }];
    let mut d = form(&mut s, &EMBOSS, sel);
    assert!(shown(&d, "depth") && shown(&d, "horizontal"));
    // a curved face: the profiles wrap around it, with no row for that
    let wrap = EMBOSS.inputs.iter().position(|x| x.key() == "wrap").unwrap();
    assert_eq!(d.form.values[wrap], json!(true));
    assert!(crate::forms::State::hidden(&EMBOSS, wrap));
    set(&mut d, &EMBOSS, "depth", json!("2 mm"));
    let (_, p) = d.command().unwrap();
    assert_eq!((p["deboss"].clone(), p.get("effect"), p.get("tangent_chain")), (json!(false), None, None));
    apply(&mut s, &mut d);
    assert!(((volume(&s) - v0) - 210.0).abs() < 2.0, "{}", volume(&s) - v0);
    // edit from the timeline: deboss instead
    let id = s.design.features.last().unwrap().id;
    let mut e = edit_form(&s.design.feature(id).unwrap().kind).unwrap();
    e.editing = Some(id);
    let depth = EMBOSS.inputs.iter().position(|x| x.key() == "depth").unwrap();
    assert_eq!(e.form.values[depth], json!("2 mm"));
    assert!(e.form.shown(&EMBOSS, depth), "an edit shows the rows after the picks");
    set(&mut e, &EMBOSS, "effect", json!("deboss"));
    apply(&mut s, &mut e);
    assert!(((v0 - volume(&s)) - 190.0).abs() < 2.0, "{}", v0 - volume(&s));
  }

  /// A U channel (40 × 20, walls 20 tall) with a line across its top on a plane through its
  /// middle: the line a rib grows down from. Returns the line's pick.
  fn channel_and_rib_line(s: &mut Session) -> Sel {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
    run(s, "extrude", json!({"distance": 4}));
    run(s, "sketch.create", json!({"plane": {"point": [20, 10, 4], "normal": [0, 0, 1]}}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [4, 20]}));
    run(s, "sketch.rect", json!({"corner1": [36, 0], "corner2": [40, 20]}));
    run(s, "extrude", json!({"distance": 20, "operation": "join"}));
    run(s, "plane.offset", json!({"plane": "xz", "offset": 10}));
    let pl = s.design.features.last().unwrap().id.0;
    run(s, "sketch.create", json!({"plane": {"feature": pl}}));
    run(s, "sketch.line", json!({"points": [[4, -20], [36, -20]]}));
    let sk = s.edit_sketch.unwrap();
    run(s, "sketch.finish", json!({}));
    let curve = s.design.sketch(sk).unwrap().curves.iter().find(|c| !c.reference()).unwrap().id;
    Sel::SketchCurve { sketch: sk, curve }
  }

  #[test]
  fn rib_and_web_rows_wait_for_a_profile() {
    let mut s = Session::default();
    let line = channel_and_rib_line(&mut s);
    let shown = |d: &Dialog, f: &Form, k: &str| d.form.shown(f, f.inputs.iter().position(|x| x.key() == k).unwrap());
    // Profile chip, Thickness Direction, Extent Type (To Next), Flip Direction; OK greyed
    let d = form(&mut s, &RIB, vec![]);
    assert_eq!(d.form.missing(&RIB), Some("Profile"));
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select open sketch profile to create rib"));
    assert!(!shown(&d, &RIB, "thickness") && !shown(&d, &RIB, "depth") && shown(&d, &RIB, "extent"));
    let sketch = RIB.inputs.iter().position(|x| x.key() == "sketch").unwrap();
    assert!(crate::forms::State::hidden(&RIB, sketch), "no Sketch drop-down");
    // the line picked: Thickness shows; Depth only with the Depth extent
    let mut d = form(&mut s, &RIB, vec![line]);
    assert!(d.ok_enabled() && shown(&d, &RIB, "thickness") && !shown(&d, &RIB, "depth"));
    let (_, p) = d.command().unwrap();
    assert!(p.get("depth").is_none() && p.get("extent").is_none() && p.get("one_side").is_none(), "{p}");
    let v0 = volume(&s);
    apply(&mut s, &mut d);
    assert!(((volume(&s) - v0) - 32.0 * 16.0 * 2.0).abs() < 1.0, "{}", volume(&s) - v0);
    d.withdraw(&mut s);
    // one side, to a depth of 5
    let mut d = form(&mut s, &RIB, vec![line]);
    set(&mut d, &RIB, "thickness_dir", json!("one_side"));
    set(&mut d, &RIB, "extent", json!("depth"));
    set(&mut d, &RIB, "depth", json!("5 mm"));
    assert!(shown(&d, &RIB, "depth"));
    let (_, p) = d.command().unwrap();
    assert_eq!((p["one_side"].clone(), p["depth"].clone()), (json!(true), json!("5 mm")));
    apply(&mut s, &mut d);
    assert!(((volume(&s) - v0) - 32.0 * 5.0 * 2.0).abs() < 1.0, "{}", volume(&s) - v0);
    d.withdraw(&mut s);

    // Web: Rib's rows and Extend Curves (ticked, greyed: the engine doesn't extend them)
    let d = form(&mut s, &WEB, vec![]);
    assert_eq!(d.prompt(false).as_deref(), Some("Select open sketch profile to create web"));
    let extend = WEB.inputs.iter().position(|x| x.key() == "extend").unwrap();
    assert_eq!(d.form.values[extend], json!(true));
    assert!(crate::forms::greyed(&WEB, "extend").is_some());
    let d = form(&mut s, &WEB, vec![line]);
    let (cmd, p) = d.command().unwrap();
    assert!(cmd == "web" && p.get("extend").is_none(), "{p}");
  }

  #[test]
  fn thread_rows_from_the_thread_tables() {
    use fox_engine::model::LengthUnit;
    // a 1 in rod in an inch design: ANSI Unified, 1.0 in, 1-8 UNC, 3A, as Fusion picks
    let mut s = Session::default();
    run(&mut s, "cylinder", json!({"radius": 12.7, "height": 50}));
    s.design.units = LengthUnit::In;
    let b = s.model.bodies[0].id;
    let side = s.model.bodies[0].faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
    let get = |d: &Dialog, k: &str| d.form.values[THREAD.inputs.iter().position(|x| x.key() == k).unwrap()].clone();
    let mut d = form(&mut s, &THREAD, vec![Sel::Face { body: b, face: side }]);
    let picked = ["thread_type", "size", "designation", "class", "direction"].map(|k| get(&d, k));
    assert_eq!(picked, [json!("ansi"), json!("1.0"), json!("1-8 UNC"), json!("3A"), json!("right")]);
    assert_eq!((get(&d, "modeled"), get(&d, "full_length")), (json!(false), json!(true)));
    assert_eq!(d.thread_status(&s).as_deref(), Some("1 Face | Radius: 0.500 in"));
    assert_eq!(d.prompt(false).as_deref(), Some("Specify size, or hold Ctrl/CMD to modify selections"));
    // Full Length hides the span's rows
    let length = THREAD.inputs.iter().position(|x| x.key() == "length").unwrap();
    assert!(!d.form.shown(&THREAD, length));
    set(&mut d, &THREAD, "full_length", json!(false));
    assert!(d.form.shown(&THREAD, length));
    assert_eq!(get(&d, "length"), json!("50 mm"));
    set(&mut d, &THREAD, "full_length", json!(true));
    let (_, p) = d.command().unwrap();
    assert_eq!(p["pitch"], json!("3.175000 mm"));
    assert!(p.get("designation").is_none() && p.get("class").is_none() && p.get("length").is_none(), "{p}");
    apply(&mut s, &mut d);
    d.withdraw(&mut s);

    // a Ø8.5 bore in a mm design: ISO Metric, M10x1.5, an inside class
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 30, "depth": 30, "height": 10}));
    run(&mut s, "sketch.create", json!({"plane": {"point": [0, 0, 10], "normal": [0, 0, 1]}}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 4.25}));
    run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
    let b = s.model.bodies[0].id;
    let bore = s.model.bodies[0].faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
    let mut d = form(&mut s, &THREAD, vec![Sel::Face { body: b, face: bore }]);
    let picked = ["thread_type", "size", "designation", "class"].map(|k| get(&d, k));
    assert_eq!(picked, [json!("iso"), json!("10"), json!("M10x1.5"), json!("6H")]);
    set(&mut d, &THREAD, "direction", json!("left"));
    let (_, p) = d.command().unwrap();
    assert_eq!(
      (p["designation"].clone(), p["class"].clone(), p["left"].clone()),
      (json!("M10x1.5"), json!("6H"), json!(true))
    );
    apply(&mut s, &mut d);
  }

  #[test]
  fn thicken_rows() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
    run(&mut s, "surface.extrude", json!({"distance": 5}));
    let b = s.model.bodies[0].id;
    let thickness = THICKEN.inputs.iter().position(|x| x.key() == "thickness").unwrap();
    let d = form(&mut s, &THICKEN, vec![]);
    assert_eq!(d.prompt(false).as_deref(), Some("Select Surfaces to Thicken"));
    assert!(!d.form.shown(&THICKEN, thickness), "the thickness row comes after the pick");
    let mut d = form(&mut s, &THICKEN, vec![Sel::Body { body: b }]);
    assert!(d.form.shown(&THICKEN, thickness));
    // Direction Symmetric; a negative thickness thickens the other way
    set(&mut d, &THICKEN, "direction", json!("symmetric"));
    set(&mut d, &THICKEN, "thickness", json!("-2 mm"));
    let (_, p) = d.command().unwrap();
    assert_eq!(
      (p["symmetric"].clone(), p["flip"].clone(), p["thickness"].clone()),
      (json!(true), json!(true), json!("2 mm"))
    );
    assert!(p.get("direction").is_none() && p.get("chain").is_none(), "{p}");
    apply(&mut s, &mut d);
    // (Symmetric: the thickness on each side)
    assert!((volume(&s) - 20.0 * 5.0 * 4.0).abs() < 1e-3, "{}", volume(&s));
  }

  #[test]
  fn boundary_fill_tools_are_bodies_and_planes() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
    let plane = s.design.features.last().unwrap().id;
    let d = form(&mut s, &BOUNDARY_FILL, vec![]);
    assert_eq!(d.prompt(false).as_deref(), Some("Select bodies & planes"));
    let b = s.model.bodies[0].id;
    let mut d = form(&mut s, &BOUNDARY_FILL, vec![Sel::Body { body: b }, Sel::Feature { feature: plane }]);
    let pi = BOUNDARY_FILL.inputs.iter().position(|x| x.key() == "planes").unwrap();
    assert_eq!(d.form.values[pi], json!([{"feature": plane.0}]));
    assert!(crate::forms::State::hidden(&BOUNDARY_FILL, pi), "no Plane drop-down");
    let ci = BOUNDARY_FILL.inputs.iter().position(|x| x.key() == "cells").unwrap();
    d.refresh_cells(&BOUNDARY_FILL, ci, "tools", &s);
    assert_eq!(d.form.cells.len(), 2, "{:?}", d.form.cells);
    let upper = d.form.cells.iter().max_by(|a, b| a.0[2].total_cmp(&b.0[2])).unwrap().0;
    set(&mut d, &BOUNDARY_FILL, "cells", json!([upper]));
    set(&mut d, &BOUNDARY_FILL, "remove_tools", json!(true));
    apply(&mut s, &mut d);
    assert!((volume(&s) - 4000.0).abs() < 1e-3, "{}", volume(&s));
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
    // Fusion's rows: Source Faces stays active after a pick and OK waits for a target (no
    // error line); the target is picked once its chip is clicked
    let source = top(&s, 0);
    let mut d = form(&mut s, &REPLACE_FACE, vec![source]);
    let labels: Vec<&str> = REPLACE_FACE
      .inputs
      .iter()
      .map(|x| match x {
        Input::Pick { label, .. } | Input::Check { label, .. } => *label,
        _ => "",
      })
      .collect();
    assert_eq!(labels, ["Source Faces", "Tangent Chain", "Target Faces"]);
    assert_eq!((d.form.row, d.form.values[1].clone()), (0, json!(false)), "Tangent Chain starts off");
    assert_eq!(d.prompt(false).as_deref(), Some("Select faces to replace"));
    assert!(!d.ok_enabled() && d.error.is_none());
    d.form.row = 2;
    let target = top(&s, 1);
    s.selection.push(target);
    d.form.sync(&REPLACE_FACE, &s, None);
    assert_eq!(d.form.values[0].as_array().unwrap().len(), 1, "the target is not a source face");
    assert!(!d.form.values[2].is_null());
    assert!(d.ok_enabled());
    apply(&mut s, &mut d);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - 6000.0).abs() < 1e-3);
  }

  #[test]
  fn fillet_setback_sweep_twist_loft_rails_and_extrude_to_object() {
    let mut s = Session::default();
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
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
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [-4, -2], "corner2": [4, 2]}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    d.path = Some(path);
    d.solid.twist = "90 deg".into();
    apply(&mut s, &mut d);
    assert!((volume(&s) - 960.0).abs() < 10.0, "{}", volume(&s));
    assert!(s.model.bodies[0].shape.mass().unwrap().bbox_max.y > 3.0);

    let mut s = Session::default();
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
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
    d.solid.rails = vec![(rail, Vec::new())];
    apply(&mut s, &mut d);
    let m = s.model.bodies[0].shape.mass().unwrap();
    assert!((m.bbox_max.x - 15.0).abs() < 0.3 && (m.bbox_min.x + 10.0).abs() < 0.3, "{m:?}");

    // extrude a bar up to the cylinder above it, picked in the viewport
    let mut s = Session::default();
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
    run(&mut s, "cylinder", json!({"plane": "yz", "radius": 20, "height": 40}));
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": -40}));
    let below = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": below}}));
    run(&mut s, "sketch.rect", json!({"corner1": [10, -5], "corner2": [30, 5]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
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

  #[test]
  fn sweep_two_rails_guide_surface_and_extrude_to_a_vertex() {
    // a 10 × 4 rectangle stretched between a straight rail and one leaning out
    let mut s = Session::default();
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
    let line = |s: &mut Session, a: [f64; 2], b: [f64; 2]| {
      run(s, "sketch.create", json!({"plane": "xz"}));
      run(s, "sketch.line", json!({"points": [a, b]}));
      s.edit_sketch.unwrap()
    };
    let path = line(&mut s, [5.0, 0.0], [5.0, 50.0]);
    let rail = line(&mut s, [0.0, 0.0], [0.0, 50.0]);
    let rail2 = line(&mut s, [10.0, 0.0], [20.0, 50.0]);
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, -2], "corner2": [10, 2]}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    d.path = Some(path);
    d.solid.rail = Some(rail);
    d.solid.rail2 = Some(rail2);
    let (_, p) = d.command().unwrap();
    assert_eq!((p["rail"].clone(), p["rail2"].clone()), (json!(rail.0), json!(rail2.0)), "{p}");
    apply(&mut s, &mut d);
    assert!((volume(&s) - 3000.0).abs() < 30.0, "{}", volume(&s));

    // a strip standing out from a cylinder, swept round it with the side face as guide surface
    let mut s = Session::default();
    s.design.behaviours = fox_engine::model::Behaviours::legacy(); // a design saved before Fusion's sketch frames
    run(&mut s, "cylinder", json!({"radius": 20, "height": 20}));
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
    let mid = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": mid}}));
    let h = 20.0 / 2f64.sqrt();
    run(&mut s, "sketch.arc", json!({"start": [20, 0], "mid": [h, h], "end": [0, 20]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [20, 9], "corner2": [22, 11]}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    run(&mut s, "sketch.finish", json!({}));
    d.path = Some(path);
    d.op = "new_body";
    let b = &s.model.bodies[0];
    let side = b.faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
    s.selection = vec![Sel::Face { body: b.id, face: side }];
    d.sync_guide_surface(&s);
    assert_eq!(d.command().unwrap().1["guide_surface"], json!({"body": b.id.0, "face": side}));
    apply(&mut s, &mut d);
    let swept = s.model.bodies.last().unwrap().shape.mass().unwrap().volume;
    let expected = std::f64::consts::PI / 2.0 * 21.0 * 4.0;
    assert!((swept - expected).abs() < 0.01 * expected, "{swept} vs {expected}");

    // a square extruded up to the plane through the top corner of a box beside it
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [30, 0], "corner2": [40, 10]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    run(&mut s, "sketch.finish", json!({}));
    d.op = "new_body";
    d.extent = "to_object";
    d.distance = "0 mm".into(); // no offset past the vertex
    let b = &s.model.bodies[0];
    let e = b.edges().iter().find(|e| e.end.z > 19.0).unwrap().id;
    s.selection = vec![Sel::Vertex { body: b.id, edge: e, end: true }];
    d.sync_object(&s);
    assert_eq!(d.command().unwrap().1["extent"], json!("to_vertex"));
    apply(&mut s, &mut d);
    let tall = |s: &Session| s.model.bodies.last().unwrap().shape.mass().unwrap();
    assert!((tall(&s).volume - 2000.0).abs() < 1e-3, "{:?}", tall(&s));
    // reopened from the timeline: still to the vertex, now 5 mm past it
    let (mut app, mut e) = edit_last(s);
    assert_eq!((e.extent, e.solid.to_kind), ("to_object", "to_vertex"));
    e.editing = Some(app.s.design.features.last().unwrap().id);
    e.distance = "5 mm".into();
    e.dirty = true;
    apply(&mut app.s, &mut e);
    assert!((tall(&app.s).bbox_max.z - 25.0).abs() < 1e-6, "{:?}", tall(&app.s));
  }
}
