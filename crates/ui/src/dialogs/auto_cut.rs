//! Fusion's automatic Cut: when a Revolve, Sweep, Loft or Pipe preview starts inside an existing
//! body, Operation turns to Cut by itself, the removed volume shows translucent red, and the
//! bodies it cuts are listed in an "Objects To Cut" group (collapsed) under Operation.

use fox_engine::Session;
use fox_engine::kernel::{BoolOp, TriMesh};
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::forms::{Form, Input};
use crate::theme;

#[derive(Debug, Clone, Default)]
pub(super) struct Cut {
  /// The overlap test ran (once, on the first preview).
  checked: bool,
  /// The operation was chosen in the dialog: it no longer follows the preview.
  chosen: bool,
  /// The bodies the cut may change, each with its tick (Fusion's Objects To Cut).
  pub(super) objects: Vec<(u32, bool)>,
  /// The Objects To Cut group is expanded.
  open: bool,
  /// The cutting tool's triangles, drawn translucent red over the preview.
  ghost: Vec<TriMesh>,
  /// Bumped whenever `ghost` changes, so the canvas renders again.
  ghost_rev: u64,
}

/// The index of a form's Operation row.
fn op_index(f: &Form) -> Option<usize> {
  f.inputs.iter().position(|i| matches!(i, Input::Choice { key: "operation", .. }))
}

/// Does the body `b` share volume with the body `n`?
fn overlaps(s: &Session, b: BodyId, n: BodyId) -> bool {
  let (Some(b), Some(n)) = (s.model.body(b), s.model.body(n)) else { return false };
  b.shape.boolean(&n.shape, BoolOp::Intersect).ok().and_then(|r| r.mass().ok()).is_some_and(|m| m.volume > 1e-9)
}

impl Dialog {
  /// Dialogs whose operation follows the preview: Revolve, Sweep, Loft and Pipe, while creating.
  pub(super) fn auto_cuts(&self) -> bool {
    self.editing.is_none()
      && match self.kind {
        Kind::Revolve | Kind::Sweep | Kind::Loft => true,
        Kind::Form(f) => std::ptr::eq(f, &crate::forms::PIPE),
        _ => false,
      }
  }

  /// The operation: the dialog's, or a form's Operation row.
  pub(super) fn operation(&self) -> &str {
    match self.kind {
      Kind::Form(f) => op_index(f).and_then(|i| self.form.values.get(i)).and_then(Value::as_str).unwrap_or("new_body"),
      _ => self.op,
    }
  }

  fn set_operation(&mut self, op: &'static str) {
    match self.kind {
      Kind::Form(f) => {
        if let Some(v) = op_index(f).and_then(|i| self.form.values.get_mut(i)) {
          *v = json!(op);
        }
      }
      _ => self.op = op,
    }
  }

  /// The operation was chosen in the dialog: it stays as chosen, and the bodies to cut are found
  /// again on the next preview.
  pub(super) fn op_chosen(&mut self) {
    self.solid.cut.chosen = true;
    self.solid.cut.objects.clear();
  }

  /// The command's parameters with the ticked Objects To Cut as its `targets`, when it cuts.
  pub(super) fn with_cut_targets(&self, mut p: Value) -> Value {
    if self.auto_cuts() && self.operation() == "cut" && !self.solid.cut.objects.is_empty() {
      p["targets"] = json!(self.solid.cut.objects.iter().filter(|(_, on)| *on).map(|(b, _)| *b).collect::<Vec<_>>());
    }
    p
  }

  /// Before a cut previews: the same command as a new body, its triangles kept as the red ghost
  /// and the bodies it overlaps as the Objects To Cut (when there are none yet). The session is
  /// left as it was.
  pub(super) fn cut_ghost(&mut self, s: &mut Session, cmd: &str, params: &Value) {
    let mut ghost = Vec::new();
    if self.auto_cuts() && self.operation() == "cut" {
      let mut p = params.clone();
      p["operation"] = json!("new_body");
      if let Some(o) = p.as_object_mut() {
        o.remove("targets");
      }
      let before: Vec<BodyId> = s.model.bodies.iter().map(|b| b.id).collect();
      if s.execute(cmd, &p).is_ok() {
        let made: Vec<BodyId> = s.model.bodies.iter().map(|b| b.id).filter(|b| !before.contains(b)).collect();
        for b in &made {
          if let Some(m) = s.model.body(*b).and_then(|x| x.shape.tessellate((x.diagonal() * 0.002).max(1e-3)).ok()) {
            ghost.push(m);
          }
        }
        if self.solid.cut.objects.is_empty() {
          self.solid.cut.objects =
            before.iter().filter(|b| made.iter().any(|n| overlaps(s, **b, *n))).map(|b| (b.0, true)).collect();
        }
        s.undo();
        s.drop_redo();
      }
    }
    if !(ghost.is_empty() && self.solid.cut.ghost.is_empty()) {
      self.solid.cut.ghost = ghost;
      self.solid.cut.ghost_rev += 1;
    }
  }

  /// Hole: the shapes of the bodies a new hole may cut (the picked face's body, or every body
  /// for holes at sketch points), taken before it previews.
  pub(super) fn hole_before(&self, s: &Session) -> Vec<(BodyId, fox_engine::kernel::Shape)> {
    if self.kind != Kind::Hole || self.editing.is_some() {
      return Vec::new();
    }
    let on: Vec<u32> =
      self.faces.iter().filter_map(|f| f.get("body").and_then(Value::as_u64)).map(|b| b as u32).collect();
    s.model.bodies.iter().filter(|b| on.is_empty() || on.contains(&b.id.0)).map(|b| (b.id, b.shape.clone())).collect()
  }

  /// After a hole previews: what it took out of each body (`before` from [`Self::hole_before`])
  /// becomes the red ghost, as Fusion previews a hole in red.
  pub(super) fn hole_ghost(&mut self, s: &Session, before: &[(BodyId, fox_engine::kernel::Shape)]) {
    if before.is_empty() {
      return;
    }
    let mut ghost = Vec::new();
    for (id, was) in before {
      let Some(now) = s.model.body(*id) else { continue };
      let Ok(gone) = was.boolean(&now.shape, BoolOp::Subtract) else { continue };
      if gone.mass().is_ok_and(|m| m.volume > 1e-9)
        && let Ok(m) = gone.tessellate((now.diagonal() * 0.002).max(1e-3))
      {
        ghost.push(m);
      }
    }
    if !(ghost.is_empty() && self.solid.cut.ghost.is_empty()) {
      self.solid.cut.ghost = ghost;
      self.solid.cut.ghost_rev += 1;
    }
  }

  /// After the first preview of a new body: when it overlaps a body that was there before,
  /// Operation turns to Cut with those bodies to cut, and the preview is withdrawn. True when it
  /// switched (the caller previews again).
  pub(super) fn auto_cut_check(&mut self, s: &mut Session) -> bool {
    if !self.auto_cuts() || !self.applied || self.solid.cut.checked || self.solid.cut.chosen {
      return false;
    }
    self.solid.cut.checked = true;
    if self.operation() != "new_body" {
      return false;
    }
    let made = self.preview_bodies(s);
    let hit: Vec<u32> = s
      .model
      .bodies
      .iter()
      .map(|b| b.id)
      .filter(|b| !made.contains(b) && made.iter().any(|n| overlaps(s, *b, *n)))
      .map(|b| b.0)
      .collect();
    if hit.is_empty() {
      return false;
    }
    self.withdraw(s);
    self.set_operation("cut");
    self.solid.cut.objects = hit.into_iter().map(|b| (b, true)).collect();
    true
  }

  /// The Objects To Cut group under Operation while the dialog cuts: collapsed to start with,
  /// each body to cut with its tick.
  pub(super) fn objects_to_cut_rows(&mut self, ui: &mut egui::Ui) {
    if !self.auto_cuts() || self.operation() != "cut" || self.solid.cut.objects.is_empty() {
      return;
    }
    let open = self.solid.cut.open;
    let head =
      egui::RichText::new(format!("{} Objects To Cut", if open { "⏷" } else { "⏵" })).color(theme::GROUP_LABEL);
    if ui.add(egui::Label::new(head).sense(egui::Sense::click())).clicked() {
      self.solid.cut.open = !open;
    }
    let n = self.solid.cut.objects.iter().filter(|(_, on)| *on).count();
    ui.label(egui::RichText::new(format!("{n} selected")).color(theme::MUTED));
    ui.end_row();
    if !open {
      return;
    }
    let mut changed = false;
    for (i, (b, on)) in self.solid.cut.objects.iter_mut().enumerate() {
      ui.label(egui::RichText::new(format!("  Body{}", i + 1)).color(theme::MUTED)).on_hover_text(format!("Body {b}"));
      changed |= ui.checkbox(on, "Cut").changed();
      ui.end_row();
    }
    if changed {
      self.dirty = true;
    }
  }

  /// A form dialog's Objects To Cut (Pipe), after its rows: an Operation changed in them counts as
  /// chosen.
  pub(super) fn form_cut_rows(&mut self, ui: &mut egui::Ui, op_before: &str) {
    if !self.auto_cuts() {
      return;
    }
    if self.operation() != op_before {
      self.op_chosen();
    }
    self.objects_to_cut_rows(ui);
  }

  /// The red ghost's revision while it shows (0 when none does): part of the canvas's render key.
  pub fn cut_ghost_key(&self) -> u64 {
    if self.applied && !self.solid.cut.ghost.is_empty() { self.solid.cut.ghost_rev } else { 0 }
  }

  /// Adds the cutting tool, translucent red, to the canvas's scene while the cut previews.
  pub fn add_cut_ghost(&self, scene: &mut fox_engine::render::Scene) {
    if self.cut_ghost_key() == 0 {
      return;
    }
    let [r, g, b, _] = theme::CUT_PREVIEW.to_array();
    let color = [r, g, b, CUT_ALPHA];
    for m in &self.solid.cut.ghost {
      scene.meshes.push(fox_engine::render::Mesh {
        positions: m.positions.clone(),
        normals: m.normals.clone(),
        indices: m.indices.clone(),
        color,
        highlight_color: color,
        ..Default::default()
      });
    }
  }
}

/// How opaque the cut preview's red is.
const CUT_ALPHA: u8 = 96;

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  /// A 40 mm cube, and a sketch on XZ with a 4 × 4 square inside it, beside the Z axis (the XZ
  /// sketch's Y runs down Z).
  fn cube_and_square() -> (Session, fox_engine::model::FeatureId) {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 40}));
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [5, -10], "corner2": [9, -14]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    (s, sk)
  }

  #[test]
  fn a_revolve_inside_a_body_turns_to_cut() {
    let (mut s, sk) = cube_and_square();
    let v0 = volume(&s);
    let mut d = Dialog::blank(Kind::Revolve);
    d.sketch = Some(sk);
    d.profiles = vec![s.model.sketches[&sk].profiles[0].interior_point()];
    d.axis = "z".into();
    d.angle = "90 deg".into();
    d.preview(&mut s);
    // the ring quarter starts inside the cube: Operation turned to Cut, the cube is the object
    assert_eq!(d.op, "cut");
    let cube = s.model.bodies[0].id.0;
    assert_eq!(d.solid.cut.objects, vec![(cube, true)]);
    assert_eq!(d.command().unwrap().1["targets"], json!([cube]));
    assert_eq!(s.model.bodies.len(), 1, "cut, not a new body");
    assert!(volume(&s) < v0 - 1.0, "{} vs {v0}", volume(&s));
    // the removed volume shows as the red ghost
    assert!(d.cut_ghost_key() > 0);
    let mut scene = fox_engine::render::Scene::default();
    d.add_cut_ghost(&mut scene);
    assert!(!scene.meshes.is_empty() && scene.meshes.iter().all(|m| m.transparent()));
    // chosen in the dialog, New Body stays
    d.op = "new_body";
    d.op_chosen();
    d.preview(&mut s);
    assert_eq!((d.op, s.model.bodies.len()), ("new_body", 2));
    assert_eq!(d.cut_ghost_key(), 0);
  }

  #[test]
  fn a_revolve_outside_stays_a_new_body() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [100, 100]}));
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [9, 4]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Revolve);
    d.sketch = Some(sk);
    d.profiles = vec![s.model.sketches[&sk].profiles[0].interior_point()];
    d.axis = "z".into();
    d.preview(&mut s);
    assert_eq!((d.op, s.model.bodies.len()), ("new_body", 2));
    assert!(d.solid.cut.objects.is_empty());
  }

  #[test]
  fn a_pipe_inside_a_body_turns_to_cut() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 40}));
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, -10], [0, -30]]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let f = &crate::forms::PIPE;
    let mut d = Dialog::blank(Kind::Form(f));
    let key = |k: &str| f.inputs.iter().position(|i| i.key() == k).unwrap();
    d.form.values[key("path")] = json!(path.0);
    d.form.values[key("path_curves")] = json!([s.design.sketch(path).unwrap().curves[0].id.0]);
    d.form.values[key("diameter")] = json!("4 mm");
    assert_eq!(d.operation(), "new_body");
    let v0 = volume(&s);
    d.preview(&mut s);
    assert_eq!(d.operation(), "cut");
    assert_eq!(s.model.bodies.len(), 1);
    assert!(volume(&s) < v0 - 1.0);
  }
}
