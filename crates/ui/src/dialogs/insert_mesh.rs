//! Insert Mesh, as Fusion's: once the file is picked, a docked INSERT MESH dialog previews the
//! mesh (selected, carrying a move/rotate triad) with Unit Type, Flip Up Direction, a Position
//! group (Center, Move To Ground), a collapsed Numerical Inputs group (X, Y, Z Distance and X, Y,
//! Z Angle) and Reset. Nothing is added until OK.

use std::path::{Path, PathBuf};

use fox_engine::kernel::glam::DVec3;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::Dialog;
use super::manip::{deg, mm};
use crate::icons::{self, Icon};
use crate::manipulators::{Triad, TriadEdit};
use crate::{App, theme};

/// Unit Type's choices in Fusion's order: the `import` command's key and the label.
pub(crate) const UNITS: [(&str, &str); 5] =
  [("cm", "Centimeter"), ("mm", "Millimeter"), ("m", "Meter"), ("in", "Inch"), ("ft", "Foot")];

/// The rows of the Numerical Inputs group, distances then angles.
pub(crate) const NUMERIC: [&str; 6] = ["X Distance", "Y Distance", "Z Distance", "X Angle", "Y Angle", "Z Angle"];

/// An angle box's value with nothing turned, as Fusion shows it.
const NO_TURN: &str = "0.0 deg";

/// The INSERT MESH dialog's choices.
#[derive(Debug, Clone, Default)]
pub(crate) struct Options {
  pub path: Option<PathBuf>,
  /// A [`UNITS`] key.
  pub units: &'static str,
  pub flip: bool,
  /// X, Y and Z Distance (expressions in the design's unit).
  pub translate: [String; 3],
  /// X, Y and Z Angle, turns about the mesh's centre.
  pub rotate: [String; 3],
  /// The Numerical Inputs group is open (Fusion opens the dialog with it closed).
  pub numeric_open: bool,
  /// The mesh's box as read with the unit and flip (mm), before its turns and translation.
  bounds: Option<(DVec3, DVec3)>,
}

impl Dialog {
  /// Points the dialog at the mesh file `path`, in the design's unit when Unit Type lists it
  /// (Fusion ticks the design's unit first), placed where the file puts it.
  pub(crate) fn insert_mesh_begin(&mut self, s: &Session, path: &Path) {
    let unit = s.design.units.symbol();
    self.mesh.path = Some(path.to_path_buf());
    self.mesh.units = UNITS.iter().find(|u| u.0 == unit).map_or("mm", |u| u.0);
    self.mesh.flip = false;
    self.mesh.numeric_open = false;
    self.insert_mesh_reset();
  }

  /// Reset: the mesh back where the file puts it (the unit and flip stay).
  pub(crate) fn insert_mesh_reset(&mut self) {
    // "0.00 in", as Fusion fills the boxes
    let zero = self.unit.fixed(0.0, 2);
    self.mesh.translate = [zero.clone(), zero.clone(), zero];
    self.mesh.rotate = [NO_TURN.into(), NO_TURN.into(), NO_TURN.into()];
    self.read_mesh_bounds();
    self.dirty = true;
  }

  /// Unit Type: rescales the preview at once.
  pub(crate) fn insert_mesh_units(&mut self, units: &'static str) {
    self.mesh.units = units;
    self.read_mesh_bounds();
    self.dirty = true;
  }

  /// Flip Up Direction: the file's up (+Y) turned to the design's (+Z), or back.
  pub(crate) fn insert_mesh_flip(&mut self) {
    self.mesh.flip = !self.mesh.flip;
    self.read_mesh_bounds();
    self.dirty = true;
  }

  fn read_mesh_bounds(&mut self) {
    let Some(path) = &self.mesh.path else { return };
    let p = json!({"units": self.mesh.units, "flip_up": self.mesh.flip});
    self.mesh.bounds = fox_engine::files::mesh_file_bounds(path, &p).ok();
  }

  /// The `import` the dialog previews and OK keeps.
  pub(super) fn insert_mesh_command(&self) -> Option<(&'static str, Value)> {
    let o = &self.mesh;
    Some((
      "import",
      json!({
        "path": o.path.as_ref()?, "units": o.units, "flip_up": o.flip,
        "translate": o.translate, "rotate": o.rotate,
      }),
    ))
  }

  /// The bodies the preview shows (the mesh as it will be inserted), selected as Fusion shows it.
  pub(super) fn insert_mesh_select(&self, s: &mut Session) {
    s.selection = self.preview_bodies(s).into_iter().map(|body| Sel::Body { body }).collect();
  }

  /// The previewed mesh's box now (with its turns and translation).
  fn preview_box(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    let boxes = self.preview_bodies(s).into_iter().filter_map(|b| s.model.body(b)?.shape.mass().ok());
    boxes.fold(None, |acc, m| {
      Some(acc.map_or((m.bbox_min, m.bbox_max), |(lo, hi)| (m.bbox_min.min(lo), m.bbox_max.max(hi))))
    })
  }

  fn translation(&self, s: &Session) -> Option<DVec3> {
    let t = &self.mesh.translate;
    Some(DVec3::new(self.len(s, &t[0])?, self.len(s, &t[1])?, self.len(s, &t[2])?))
  }

  fn set_translation(&mut self, t: DVec3) {
    self.mesh.translate = [mm(self.unit, t.x), mm(self.unit, t.y), mm(self.unit, t.z)];
    self.dirty = true;
  }

  /// Center: the mesh's centre onto the origin.
  pub(crate) fn insert_mesh_center(&mut self, s: &Session) {
    let (Some((lo, hi)), Some(t)) = (self.preview_box(s), self.translation(s)) else { return };
    self.set_translation(t - (lo + hi) / 2.0);
  }

  /// Move To Ground: the mesh's lowest point onto the ground plane (XY).
  pub(crate) fn insert_mesh_ground(&mut self, s: &Session) {
    let (Some((lo, _)), Some(t)) = (self.preview_box(s), self.translation(s)) else { return };
    self.set_translation(t - DVec3::Z * lo.z);
  }

  /// The triad on the mesh's centre: arrows, planes and rings on the world axes, as Fusion's.
  pub(super) fn insert_mesh_triad(&self, s: &Session) -> Option<Triad> {
    let (lo, hi) = self.mesh.bounds?;
    let home = (lo + hi) / 2.0;
    let r = &self.mesh.rotate;
    let mut t = Triad::world(home + self.translation(s)?);
    t.home = home;
    t.angles = [self.ang(s, &r[0])?, self.ang(s, &r[1])?, self.ang(s, &r[2])?];
    Some(t)
  }

  /// A triad drag: the distances follow its centre, an angle its ring.
  pub(super) fn set_insert_mesh_triad(&mut self, edit: TriadEdit) -> bool {
    match edit {
      TriadEdit::Move(p) => {
        let Some((lo, hi)) = self.mesh.bounds else { return false };
        self.set_translation(p - (lo + hi) / 2.0);
      }
      TriadEdit::Rotate(i, a) => self.mesh.rotate[i] = deg(a),
      TriadEdit::Orient(_) => return false,
    }
    true
  }

  /// Unit Type, Flip Up Direction, the Position group and the collapsed Numerical Inputs group.
  pub(super) fn insert_mesh_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Unit Type");
    let cur = UNITS.iter().find(|u| u.0 == self.mesh.units).map_or("Millimeter", |u| u.1);
    let mut picked = None;
    egui::ComboBox::from_id_salt("insert-mesh-units").selected_text(cur).width(140.0).show_ui(ui, |ui| {
      for (key, label) in UNITS {
        if ui.selectable_label(self.mesh.units == key, label).clicked() {
          picked = Some(key);
        }
      }
    });
    if let Some(k) = picked.filter(|k| *k != self.mesh.units) {
      self.insert_mesh_units(k);
    }
    ui.end_row();
    ui.label("Flip Up Direction");
    if icons::icon_button(ui, Icon::Flip, 26.0, "Flip Up Direction", self.mesh.flip, None).clicked() {
      self.insert_mesh_flip();
    }
    ui.end_row();
    group_label(ui, "Position");
    ui.end_row();
    ui.label("Center");
    if icons::icon_button(ui, Icon::MeshCenter, 26.0, "Center the mesh on the origin", false, None).clicked() {
      self.insert_mesh_center(&app.s);
    }
    ui.end_row();
    ui.label("Move To Ground");
    if icons::icon_button(ui, Icon::MoveToGround, 26.0, "Move the mesh down onto the ground plane", false, None)
      .clicked()
    {
      self.insert_mesh_ground(&app.s);
    }
    ui.end_row();
    if group_toggle(ui, "Numerical Inputs", self.mesh.numeric_open) {
      self.mesh.numeric_open = !self.mesh.numeric_open;
    }
    ui.end_row();
    if self.mesh.numeric_open {
      for (i, label) in NUMERIC.iter().enumerate() {
        ui.label(*label);
        let v = if i < 3 { &mut self.mesh.translate[i] } else { &mut self.mesh.rotate[i - 3] };
        if super::value_edit(ui, v).changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
    }
  }
}

/// A group's title in the label column, as Fusion's (small, grey, bold).
fn group_label(ui: &mut egui::Ui, label: &str) {
  ui.label(egui::RichText::new(label).strong().color(theme::GROUP_LABEL));
  ui.label("");
}

/// A collapsible group's title with its triangle (right closed, down open); true when clicked.
fn group_toggle(ui: &mut egui::Ui, label: &str, open: bool) -> bool {
  let text = egui::RichText::new(format!("    {label}")).strong().color(theme::GROUP_LABEL);
  let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
  let c = r.rect.left_center() + egui::vec2(5.0, 0.0);
  let pts = if open {
    vec![c + egui::vec2(-4.0, -2.0), c + egui::vec2(4.0, -2.0), c + egui::vec2(0.0, 3.0)]
  } else {
    vec![c + egui::vec2(-2.0, -4.0), c + egui::vec2(3.0, 0.0), c + egui::vec2(-2.0, 4.0)]
  };
  ui.painter().add(egui::Shape::convex_polygon(pts, theme::GROUP_LABEL, egui::Stroke::NONE));
  ui.label("");
  r.clicked()
}
