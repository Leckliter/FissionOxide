//! Section Analysis, as Fusion's: the dialog opens with only Cut Plane ("Select a planar object
//! to define the cut plane") and OK greyed. Once an origin or construction plane or a planar
//! face is picked it adds Distance, Angle 1, Angle 2, Flip, Section Color and Show Hatch, and
//! the canvas shows the cut with an arrow for the distance and two dials for the angles. OK
//! keeps the cut as a row ("Section1") in the browser's Analysis folder; Cancel leaves nothing.

use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::manip::{deg, mm};
use super::{Dialog, Kind, Manipulator, is_planar, plane_of, sel_chip, value_edit};
use crate::App;
use crate::handles::Handle;
use crate::icons::Icon;

/// The cursor prompt until a cut plane is picked.
pub(crate) const CUT_PLANE_HINT: &str = "Select a planar object to define the cut plane";

/// Section Color's choices.
const COLORS: [(&str, &str); 2] = [("component", "From Component"), ("single", "Single Color")];

/// A Section Analysis kept by OK: a row in the browser's Analysis folder.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptSection {
  pub name: String,
  /// The `section` view parameters (see [`fox_engine::scene::view_options`]).
  pub params: Value,
  pub visible: bool,
}

/// The newest kept section that shows (none while the Analysis folder is hidden).
pub(crate) fn shown_section(app: &App) -> Option<&Value> {
  if app.analysis_hidden {
    return None;
  }
  app.sections.iter().rev().find(|k| k.visible).map(|k| &k.params)
}

/// The name OK gives the next kept section: Section1, Section2, ...
pub(crate) fn next_section_name(app: &App) -> String {
  let n = app.sections.iter().filter_map(|k| k.name.strip_prefix("Section")?.parse::<u32>().ok()).max().unwrap_or(0);
  format!("Section{}", n + 1)
}

impl Dialog {
  /// Section Analysis's defaults; a planar face selected beforehand is the cut plane.
  pub(super) fn section_open(&mut self, s: &Session) {
    self.plane = Value::Null;
    self.plane_label.clear();
    self.distance = "0 mm".into();
    self.angle = "0 deg".into();
    self.angle2 = "0 deg".into();
    self.flag = true;
    self.choice = "component";
    self.section_sync(s);
  }

  /// Is Section Analysis waiting for its cut plane (the origin and construction planes show to
  /// click, as Create Sketch's do)?
  pub(crate) fn wants_cut_plane(&self) -> bool {
    self.kind == Kind::Section && self.plane.is_null()
  }

  /// Makes `plane` ("xy", `{feature}` or `{body, face}`) the cut plane.
  pub(crate) fn set_cut_plane(&mut self, s: &Session, plane: Value) {
    self.plane_label = match &plane {
      Value::String(n) => format!("{} Plane", n.to_uppercase()),
      v if v.get("feature").is_some() => v["feature"]
        .as_u64()
        .and_then(|f| s.design.feature(fox_engine::model::FeatureId(f as u32)))
        .map(|f| f.name.clone())
        .unwrap_or_default(),
      _ => "Face".into(),
    };
    // a face's normal points out of the body: cut away the material behind it instead
    self.flip = plane.get("face").is_some();
    self.plane = plane;
  }

  /// While Cut Plane is empty, a planar face or construction plane selected in the canvas or
  /// browser becomes the cut plane.
  pub(super) fn section_sync(&mut self, s: &Session) {
    if !self.plane.is_null() {
      return;
    }
    let picked = s.selection.iter().find_map(|x| match *x {
      Sel::Face { body, face } if is_planar(s, body, face) => Some(json!({"body": body.0, "face": face})),
      Sel::Feature { feature } if s.model.planes.contains_key(&feature) => Some(json!({"feature": feature.0})),
      _ => None,
    });
    if let Some(p) = picked {
      self.set_cut_plane(s, p);
    }
  }

  /// The cut plane before Distance and the angles, and the middle of the model on it (where the
  /// arrow and dials sit and the angles turn about).
  fn section_base(&self, s: &Session) -> Option<(Plane, DVec3)> {
    let pl = plane_of(s, &self.plane)?;
    let n = pl.normal();
    let mid = match &self.plane {
      // a face's own middle
      v if v.get("face").is_some() => pl.origin,
      _ => s.model.bounds().map(|(a, b)| (a + b) / 2.0).unwrap_or(pl.origin),
    };
    Some((pl, mid - n * n.dot(mid - pl.origin)))
  }

  /// The `section` view parameters for the dialog's rows (none until a cut plane is picked).
  pub(crate) fn section_params(&self, s: &Session) -> Option<Value> {
    if self.plane.is_null() {
      return None;
    }
    let (_, centre) = self.section_base(s)?;
    let offset = self.len(s, &self.distance)?;
    let a1 = self.ang(s, &self.angle)?.to_degrees();
    let a2 = self.ang(s, &self.angle2)?.to_degrees();
    Some(json!({
      "plane": self.plane, "offset": offset, "flip": self.flip, "angle1": a1, "angle2": a2,
      "hatch": self.flag, "color": self.choice, "center": centre.to_array(),
    }))
  }

  /// The rows: Cut Plane, then (once picked) Distance, Angle 1, Angle 2, Flip, Section Color and
  /// Show Hatch. The cut follows them live.
  pub(super) fn section_fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Cut Plane");
    let has = !self.plane.is_null();
    let text = if has { self.plane_label.clone() } else { "Select".to_string() };
    let chip = sel_chip(ui, self.accent(), &text, has);
    // a click on the filled chip empties it for another pick
    if has && chip.interact(egui::Sense::click()).on_hover_text("Pick another cut plane").clicked() {
      self.plane = Value::Null;
      app.s.selection.clear();
      app.section = None;
      ui.end_row();
      return;
    }
    ui.end_row();
    if has {
      let mut dirty = false;
      for (label, f) in [("Distance", &mut self.distance), ("Angle 1", &mut self.angle), ("Angle 2", &mut self.angle2)]
      {
        ui.label(label);
        dirty |= value_edit(ui, f).changed();
        ui.end_row();
      }
      ui.label("Flip");
      if crate::icons::icon_button(ui, Icon::Flip, 24.0, "Flip", false, None).clicked() {
        self.flip = !self.flip;
      }
      ui.end_row();
      Self::choose(ui, "Section Color", "section-color", &mut self.choice, &COLORS, &mut dirty);
      ui.label("Show Hatch");
      ui.checkbox(&mut self.flag, "");
      ui.end_row();
      self.dirty |= dirty;
    }
    app.section = self.section_params(&app.s);
  }

  /// The distance arrow, out of the middle of the model along the cut plane's normal.
  pub(super) fn section_manipulator(&self, s: &Session) -> Option<Manipulator> {
    if self.kind != Kind::Section || self.plane.is_null() {
      return None;
    }
    let (pl, centre) = self.section_base(s)?;
    Some(Manipulator { origin: centre, dir: pl.normal(), value: self.len(s, &self.distance)? })
  }

  /// The two rotation dials at the arrow's foot: Angle 1 about the plane's x axis, Angle 2
  /// about its y axis.
  pub(super) fn section_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (pl, centre) = self.section_base(s)?;
    let at = centre + pl.normal() * self.len(s, &self.distance)?;
    let (a1, a2) = (self.ang(s, &self.angle)?, self.ang(s, &self.angle2)?);
    Some(vec![
      Handle::dial(at, pl.x, pl.normal(), None, a1, deg(a1)),
      Handle::dial(at, pl.y, pl.normal(), None, a2, deg(a2)),
    ])
  }

  /// Takes a dragged dial's angle (radians) into Angle 1 or Angle 2.
  pub(super) fn set_section_handle(&mut self, i: usize, v: f64) {
    let a = deg(v);
    if i == 0 { self.angle = a } else { self.angle2 = a }
  }

  /// The arrow's dragged distance (mm) into Distance.
  pub(super) fn set_section_distance(&mut self, v: f64) {
    self.distance = mm(self.unit, v);
  }

  /// OK: keeps the cut as the Analysis folder's next "SectionN" row.
  pub(super) fn keep_section(&self, app: &mut App) {
    if let Some(params) = self.section_params(&app.s) {
      let name = next_section_name(app);
      app.sections.push(KeptSection { name, params, visible: true });
      app.analysis_hidden = false;
    }
    app.section = None;
  }
}
