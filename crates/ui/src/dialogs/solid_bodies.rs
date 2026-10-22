//! Solid commands don't see mesh bodies: Fusion's Combine, Shell, the patterns, Mirror, Split
//! Body and the other SOLID commands that take whole bodies skip a mesh body until it is
//! converted (Convert Mesh), so while one of them is open a mesh body neither lights up under
//! the pointer nor goes into its rows, and one picked before the command opened is left out.

use fox_engine::Session;
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::forms::Input;

/// The form commands (SOLID and SURFACE) whose body rows skip mesh bodies.
const SOLID_FORMS: &[&str] = &[
  "pattern.path",
  "remove_features.form",
  "replace_with_primitives",
  "boundary_fill",
  "thicken",
  "surface.stitch",
  "surface.trim.body",
  "surface.trim.sketch",
  "surface.offset.variable",
];

impl Kind {
  /// Does this command's body selection skip mesh bodies?
  pub(crate) fn skips_mesh_bodies(self) -> bool {
    match self {
      Kind::Combine | Kind::Shell | Kind::RectPattern | Kind::CircPattern | Kind::Mirror | Kind::SplitBody => true,
      Kind::Form(f) => SOLID_FORMS.contains(&f.cmd),
      _ => false,
    }
  }
}

impl Dialog {
  /// Drops the mesh bodies from the rows of a command that skips them.
  pub(super) fn drop_mesh_bodies(&mut self, s: &Session) {
    if !self.kind.skips_mesh_bodies() {
      return;
    }
    let mesh = |b: u32| crate::mesh_ui::is_mesh(s, BodyId(b));
    self.bodies.retain(|b| !mesh(*b));
    if self.target.is_some_and(mesh) {
      self.target = None;
    }
    if let Kind::Form(f) = self.kind {
      for (i, input) in f.inputs.iter().enumerate() {
        if let Input::Bodies { .. } = input
          && let Some(a) = self.form.values.get(i).and_then(Value::as_array)
          && a.iter().any(|v| v.as_u64().is_some_and(|b| mesh(b as u32)))
        {
          let kept: Vec<Value> = a.iter().filter(|v| !v.as_u64().is_some_and(|b| mesh(b as u32))).cloned().collect();
          self.form.values[i] = json!(kept);
        }
      }
    }
  }
}

impl Dialog {
  /// Does this dialog's body selection skip mesh bodies?
  pub(crate) fn skips_mesh_bodies(&self) -> bool {
    self.kind.skips_mesh_bodies()
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Sel;
  use fox_engine::scene::Pick;

  use super::super::{Dialog, Kind};
  use crate::mesh_body_tests::mesh_and_box;

  /// Combine, Shell, the patterns, Mirror, Split Body and Path Pattern leave a mesh body out:
  /// one picked before the command opens isn't taken, it doesn't light up under the pointer and a
  /// click on it picks nothing; the solid is taken as before. Move still takes a mesh body.
  #[test]
  fn solid_commands_skip_mesh_bodies() {
    let kinds = [
      Kind::Combine,
      Kind::Shell,
      Kind::RectPattern,
      Kind::CircPattern,
      Kind::Mirror,
      Kind::SplitBody,
      Kind::Form(&crate::forms::PATH_PATTERN),
    ];
    for kind in kinds {
      let mut app = mesh_and_box();
      let mesh = app.s.model.bodies.iter().find(|b| b.is_mesh()).unwrap().id;
      let solid = app.s.model.bodies.iter().find(|b| !b.is_mesh()).unwrap().id;
      app.s.selection = vec![Sel::Body { body: mesh }];
      let mut d = Dialog::open(&mut app, kind);
      d.sync_selection(&app.s);
      assert!(!d.bodies.contains(&mesh.0) && d.target != Some(mesh.0), "{kind:?}: the preselected mesh is left out");
      if let Kind::Form(f) = kind {
        let i = f.inputs.iter().position(|x| x.key() == "bodies").unwrap();
        assert_eq!(d.form.values[i], serde_json::json!([]), "{kind:?}");
      }
      app.s.selection.clear();
      app.dialog = Some(d);
      let on_mesh = Pick::Face { body: mesh, face: 0 };
      assert!(crate::select::mesh_skipped(&app, &on_mesh), "{kind:?}");
      crate::select::pick(&mut app, on_mesh, false);
      assert!(app.s.selection.is_empty(), "{kind:?}: a click on the mesh picks nothing");
      let on_solid = Pick::Face { body: solid, face: 0 };
      assert!(!crate::select::mesh_skipped(&app, &on_solid));
      app.s.selection = vec![Sel::Body { body: solid }];
      let mut d = app.dialog.take().unwrap();
      d.sync_selection(&app.s);
      let took = d.bodies.contains(&solid.0) || d.target == Some(solid.0) || matches!(kind, Kind::Form(_));
      assert!(took, "{kind:?}: the solid is taken");
      d.cancel(&mut app.s);
    }
    let mut app = mesh_and_box();
    let mesh = app.s.model.bodies.iter().find(|b| b.is_mesh()).unwrap().id;
    app.s.selection = vec![Sel::Body { body: mesh }];
    let d = Dialog::open(&mut app, Kind::Move);
    assert!(!d.skips_mesh_bodies(), "Move takes mesh bodies");
  }
}
