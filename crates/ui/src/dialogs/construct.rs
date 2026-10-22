//! Fusion's CONSTRUCTION GEOMETRY dialog: every CONSTRUCT plane, axis and point command opens it
//! with its Type (plane, axis or point, as icon toggles) and Method set; picking another Type or
//! Method switches the dialog to that method's inputs. Offset Plane's own rows follow: the Plane
//! chip, Extent, Distance and Resize.

use fox_engine::{Sel, Session};
use serde_json::Value;

use super::icon_row::{choice, greyed, icon_row};
use super::{Dialog, Kind};
use crate::forms;
use crate::icons::Icon;

/// The dialog's title for every construction command.
pub(crate) const TITLE: &str = "CONSTRUCTION GEOMETRY";

/// One construction method: its Type, its name in the Method list, and the command's dialog.
pub(crate) struct Method {
  pub ty: &'static str,
  pub name: &'static str,
  pub kind: Kind,
}

const fn m(ty: &'static str, name: &'static str, kind: Kind) -> Method {
  Method { ty, name, kind }
}

/// The methods, in the CONSTRUCT menu's order.
pub(crate) const METHODS: &[Method] = &[
  m("plane", "Offset", Kind::OffsetPlane),
  m("plane", "At Angle", Kind::Form(&forms::PLANE_ANGLE)),
  m("plane", "Tangent", Kind::Form(&forms::TANGENT_PLANE)),
  m("plane", "Midplane", Kind::Form(&forms::MIDPLANE)),
  m("plane", "Perpendicular Plane", Kind::Form(&forms::PERPENDICULAR_PLANE)),
  m("plane", "Through Two Edges", Kind::Form(&forms::PLANE_TWO_EDGES)),
  m("plane", "Through Three Points", Kind::Form(&forms::PLANE_THREE_POINTS)),
  m("plane", "Along Path", Kind::Form(&forms::PLANE_ALONG_PATH)),
  m("axis", "Through Cylinder/Cone/Torus", Kind::Form(&forms::AXIS_FACE)),
  m("axis", "Perpendicular to Face", Kind::Form(&forms::AXIS_NORMAL)),
  m("axis", "Through Two Planes", Kind::Form(&forms::AXIS_TWO_PLANES)),
  m("axis", "Through Two Points", Kind::Form(&forms::AXIS_TWO_POINTS)),
  m("axis", "Through Edge", Kind::Form(&forms::AXIS_EDGE)),
  m("point", "At Vertex", Kind::Form(&forms::POINT_VERTEX)),
  m("point", "Through Two Edges", Kind::Form(&forms::POINT_TWO_EDGES)),
  m("point", "Through Three Planes", Kind::Form(&forms::POINT_THREE_PLANES)),
  m("point", "At Center of Circle/Sphere/Torus", Kind::Form(&forms::POINT_CENTER)),
  m("point", "At Edge and Plane", Kind::Form(&forms::POINT_EDGE_PLANE)),
  m("point", "Along Path", Kind::Form(&forms::POINT_ALONG_PATH)),
];

/// The construction method a dialog kind is, if it is one.
pub(crate) fn method_of(kind: Kind) -> Option<&'static Method> {
  METHODS.iter().find(|m| m.kind == kind)
}

impl Dialog {
  /// The Type and Method rows of a construction command (fixed while editing one).
  pub(super) fn construct_rows(&mut self, ui: &mut egui::Ui) {
    let Some(cur) = method_of(self.kind) else { return };
    let creating = self.editing.is_none();
    ui.label("Type");
    let mut ty = cur.ty;
    let types = if creating {
      [
        choice("plane", Icon::Plane, "Plane"),
        choice("axis", Icon::Axis, "Axis"),
        choice("point", Icon::CPoint, "Point"),
      ]
    } else {
      [
        greyed("plane", Icon::Plane, "Plane"),
        greyed("axis", Icon::Axis, "Axis"),
        greyed("point", Icon::CPoint, "Point"),
      ]
    };
    if icon_row(ui, &mut ty, &types, "") {
      self.switch_to = METHODS.iter().find(|m| m.ty == ty).map(|m| m.kind);
    }
    ui.end_row();
    ui.label("Method");
    ui.add_enabled_ui(creating, |ui| {
      egui::ComboBox::from_id_salt("construct_method").selected_text(cur.name).width(170.0).show_ui(ui, |ui| {
        for m in METHODS.iter().filter(|m| m.ty == cur.ty) {
          if ui.selectable_label(m.kind == self.kind, m.name).clicked() && m.kind != self.kind {
            self.switch_to = Some(m.kind);
          }
        }
      });
    });
    ui.end_row();
  }

  /// Offset Plane's Plane chip: "1 selected" with an x that clears it, else "Select".
  pub(super) fn offset_plane_chip(&mut self, ui: &mut egui::Ui, s: &mut Session) {
    let has = !self.plane.is_null();
    let r = super::sel_chip(ui, self.accent(), if has { "1 selected" } else { "Select" }, has);
    let x = egui::Rect::from_center_size(r.rect.right_center() - egui::vec2(10.0, 0.0), egui::Vec2::splat(16.0));
    if has && ui.interact(x, ui.id().with("clear_plane"), egui::Sense::click()).on_hover_text("Clear").clicked() {
      self.clear_plane(s);
    }
  }

  /// Offset Plane's rows below its Plane: Extent ("Distance"), Distance and Resize.
  pub(super) fn offset_plane_rows(&mut self, ui: &mut egui::Ui) {
    ui.label("Extent");
    egui::ComboBox::from_id_salt("offset_extent").selected_text("Distance").width(140.0).show_ui(ui, |ui| {
      let _ = ui.selectable_label(true, "Distance");
    });
    ui.end_row();
    ui.label("Distance");
    self.text(ui, true);
    ui.end_row();
    ui.label("Resize");
    // the plane is drawn at a fixed size on screen; a custom size isn't in FissionOxide yet
    let mut size: &'static str = "auto";
    icon_row(
      ui,
      &mut size,
      &[choice("auto", Icon::Fit, "Automatic size"), greyed("custom", Icon::Scale, "Custom size")],
      "not in FissionOxide yet",
    );
    ui.end_row();
  }

  /// An Offset Plane being made: a plane picked in the browser or the canvas becomes its plane.
  pub(crate) fn takes_plane(&self) -> bool {
    self.kind == Kind::OffsetPlane && self.editing.is_none()
  }

  /// Clears Offset Plane's plane (the chip's x): the picked face or plane is deselected and the
  /// dialog asks for a plane again.
  pub(super) fn clear_plane(&mut self, s: &mut Session) {
    self.plane = Value::Null;
    self.plane_label.clear();
    s.selection.retain(|x| !matches!(x, Sel::Face { .. } | Sel::Feature { .. }));
    self.dirty = true;
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;
  use crate::App;

  /// Draws the open dialog for one frame, as the app does.
  fn frame(app: &mut App) {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
      ..Default::default()
    };
    let mut d = app.dialog.take().expect("a dialog");
    let mut keep = false;
    let mut out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
    out.textures_delta.clear();
    app.after_dialog_frame(d, keep);
  }

  /// Offset Plane is the construction dialog's plane Offset method: its Plane chip's x clears the
  /// plane (OK greys, the prompt asks for one), a plane picked in the browser fills it again, and
  /// another Method switches to that command with this one's preview withdrawn.
  #[test]
  fn offset_plane_is_the_construction_dialog() {
    let mut app = App::with_session(fox_engine::Session::default());
    app.canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
    app.start(Kind::OffsetPlane);
    frame(&mut app);
    assert_eq!(app.s.model.planes.len(), 1, "previewed from XY");
    let d = app.dialog.as_mut().unwrap();
    assert!(d.takes_plane() && d.ok_enabled());
    d.clear_plane(&mut app.s);
    frame(&mut app);
    let d = app.dialog.as_ref().unwrap();
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select a plane"));
    assert!(app.s.model.planes.is_empty(), "the preview is withdrawn");
    crate::panels::pick_plane(&mut app, json!("xz"));
    let d = app.dialog.as_ref().unwrap();
    assert_eq!((d.plane.clone(), d.plane_label.as_str()), (json!("xz"), "XZ plane"));
    assert_eq!(app.s.model.planes.len(), 1);
    assert!(app.picked_plane.is_none(), "the browser pick went to the dialog");

    // Method ▸ Midplane: that command's dialog, this preview gone
    app.dialog.as_mut().unwrap().switch_to = method_of(Kind::Form(&forms::MIDPLANE)).map(|m| m.kind);
    frame(&mut app);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!(d.kind, Kind::Form(&forms::MIDPLANE));
    assert_eq!(d.kind.title(), TITLE);
    // (the Midplane form previews from its default planes; the offset plane is gone)
    let offset =
      |f: &fox_engine::model::Feature| matches!(f.kind, fox_engine::model::FeatureKind::ConstructionPlane { .. });
    assert!(!app.s.design.features.iter().any(offset), "the offset plane was withdrawn");
  }

  #[test]
  fn every_construct_command_is_a_method() {
    // the CONSTRUCT menu's plane, axis and point commands each open this dialog
    let commands = crate::menus::commands(&[crate::panels::Tab::Solid]);
    for name in
      ["Offset Plane", "Plane at Angle", "Midplane", "Perpendicular Plane", "Axis Through Edge", "Point At Vertex"]
    {
      let Some(crate::panels::Cmd::Dialog(kind)) = commands.iter().find(|e| e.label == name).map(|e| e.cmd) else {
        panic!("{name} isn't a dialog command");
      };
      assert!(method_of(kind).is_some(), "{name}");
      assert_eq!(kind.title(), TITLE, "{name}");
    }
    assert_eq!(method_of(Kind::OffsetPlane).map(|m| (m.ty, m.name)), Some(("plane", "Offset")));
    assert!(method_of(Kind::Extrude).is_none());
    assert_eq!(Kind::Extrude.title(), "EXTRUDE");
    let types: Vec<&str> = METHODS.iter().map(|m| m.ty).collect();
    assert_eq!(types.iter().filter(|t| **t == "plane").count(), 8);
    assert_eq!(types.iter().filter(|t| **t == "axis").count(), 5);
    assert_eq!(types.iter().filter(|t| **t == "point").count(), 6);
  }
}
