//! Plane Cut (MESH ▸ MODIFY), as Fusion's: Body, Cut Plane (a Select chip taking a planar face, a
//! construction plane or an origin plane), Flip, Type (Trim, Split Body, Split Faces), Fill Type
//! with an icon before each value (No Fill, Uniform, Minimal), and a Numerical Inputs group (X
//! Distance, Y Angle, Z Angle) once a plane is picked. OK waits for the plane. In the canvas the
//! plane shows with an arrow for the distance and two dials for the angles, and Trim tints the
//! part it removes red.

use egui::{Color32, Pos2, Shape, Stroke};
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::meshtools;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::manip::{deg, mm};
use super::{Dialog, Kind};
use crate::App;
use crate::handles::Handle;
use crate::icons::Icon;
use crate::manipulators::snap;

/// The cursor prompt until the body and the plane are picked (Fusion's wording).
pub(crate) const HINT: &str = "Select mesh body to cut";

pub(super) const TYPES: [(&str, &str); 3] =
  [("trim", "Trim"), ("split_body", "Split Body"), ("split_faces", "Split Faces")];
pub(super) const FILLS: [(&str, &str); 3] = [("none", "No Fill"), ("uniform", "Uniform"), ("minimal", "Minimal")];

fn fill_icon(key: &str) -> Option<Icon> {
  Some(match key {
    "none" => Icon::FillNone,
    "uniform" => Icon::FillUniform,
    "minimal" => Icon::FillMinimal,
    _ => return None,
  })
}

/// The red of the part Trim removes.
const REMOVED: Color32 = Color32::from_rgba_premultiplied(150, 20, 20, 110);
/// The cut plane's fill and outline.
const PLANE_FILL: Color32 = Color32::from_rgba_premultiplied(40, 80, 130, 50);
const PLANE_EDGE: Color32 = Color32::from_rgb(70, 130, 210);

#[derive(Debug, Clone)]
pub(super) struct State {
  /// The cut plane, as `mesh.plane_cut` takes it.
  pub picked: Option<Value>,
  /// The plane pick the selection last offered (a pick cleared with the chip's x stays cleared).
  seen: Option<Value>,
  pub flip: bool,
  pub cut: &'static str,
  pub fill: &'static str,
  pub distance: String,
  pub y_angle: String,
  pub z_angle: String,
  /// The Numerical Inputs group is open.
  pub numeric: bool,
  /// The body's triangles before any preview, for the red tint and the handles' place.
  mesh: Option<(u32, Vec<DVec3>, Vec<u32>)>,
  /// The selection the dialog opened with has been seen: a face picked to choose the body
  /// doesn't become the cut plane too.
  started: bool,
}

impl Default for State {
  fn default() -> State {
    State {
      picked: None,
      seen: None,
      flip: false,
      cut: "trim",
      fill: "uniform",
      distance: "0.00 mm".into(),
      y_angle: "0.0 deg".into(),
      z_angle: "0.0 deg".into(),
      numeric: true,
      mesh: None,
      started: false,
    }
  }
}

impl Dialog {
  /// The first pick is the body (a picked body, or the body of a picked face); then a planar
  /// face, a construction plane or an origin plane picked in the canvas or the browser is the
  /// cut plane.
  pub(super) fn plane_cut_sync(&mut self, app: &App) {
    if self.kind != Kind::PlaneCut {
      return;
    }
    let s = &app.s;
    let picked_body = s.selection.iter().find_map(|x| match x {
      Sel::Body { body } => Some(body.0),
      _ => None,
    });
    if let Some(b) = picked_body
      && self.bodies.first() != Some(&b)
    {
      self.bodies = vec![b];
      self.dirty = true;
    }
    if self.bodies.is_empty()
      && let Some(Sel::Face { body, .. }) = s.selection.iter().find(|x| matches!(x, Sel::Face { .. }))
    {
      self.bodies = vec![body.0];
      self.dirty = true;
      return;
    }
    if !self.applied {
      self.plane_cut_cache(s);
    }
    let offered = s
      .selection
      .iter()
      .find_map(|sel| match *sel {
        Sel::Face { body, face } if !self.bodies.is_empty() && super::is_planar(s, body, face) => {
          Some(json!({"body": body.0, "face": face}))
        }
        Sel::Feature { feature } if s.model.planes.contains_key(&feature) => Some(json!({"feature": feature.0})),
        _ => None,
      })
      .or_else(|| app.picked_plane.clone());
    if !std::mem::replace(&mut self.plane_cut.started, true) {
      self.plane_cut.seen = offered;
      return;
    }
    if offered == self.plane_cut.seen {
      return;
    }
    self.plane_cut.seen = offered.clone();
    if let Some(v) = offered
      && Some(&v) != self.plane_cut.picked.as_ref()
    {
      self.plane_cut.picked = Some(v);
      self.dirty = true;
    }
  }

  /// Keeps the body's triangles as they are without the preview.
  fn plane_cut_cache(&mut self, s: &Session) {
    let Some(&b) = self.bodies.first() else {
      self.plane_cut.mesh = None;
      return;
    };
    if self.plane_cut.mesh.as_ref().is_some_and(|m| m.0 == b) {
      return;
    }
    self.plane_cut.mesh = s
      .model
      .body(fox_engine::model::BodyId(b))
      .and_then(|x| x.mesh())
      .map(fox_engine::model::meshops::weld)
      .map(|(p, t)| (b, p, t));
  }

  pub(super) fn plane_cut_ready(&self) -> bool {
    !self.bodies.is_empty() && self.plane_cut.picked.is_some()
  }

  pub(super) fn plane_cut_command(&self) -> Option<(&'static str, Value)> {
    let st = &self.plane_cut;
    Some((
      "mesh.plane_cut",
      json!({
        "body": self.bodies.first()?, "plane": st.picked.clone()?, "flip": st.flip, "type": st.cut,
        "fill": st.fill, "distance": st.distance, "y_angle": st.y_angle, "z_angle": st.z_angle,
      }),
    ))
  }

  /// The rows: Body, Cut Plane, Flip, Type, Fill Type and the Numerical Inputs group.
  pub(super) fn plane_cut_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let accent = self.accent();
    let mut dirty = false;
    ui.label("Body");
    let n = self.bodies.len();
    let chip = super::sel_chip(ui, accent, if n == 0 { "Select" } else { "1 selected" }, n > 0);
    if n > 0 && clear_clicked(ui, &chip, "plane-cut-body") {
      self.bodies.clear();
      app.s.selection.clear();
      dirty = true;
    }
    ui.end_row();
    ui.label("Cut Plane");
    let has = self.plane_cut.picked.is_some();
    // lit while it waits for its pick, once the body is in
    let chip = super::sel_chip_lit(ui, accent, if has { "1 selected" } else { "Select" }, has || n > 0, has);
    if has && clear_clicked(ui, &chip, "plane-cut-plane") {
      self.plane_cut.picked = None;
      app.picked_plane = None;
      dirty = true;
    }
    ui.end_row();
    ui.label("Flip");
    if crate::icons::icon_button(ui, Icon::Flip, 24.0, "Flip", self.plane_cut.flip, None).clicked() {
      self.plane_cut.flip = !self.plane_cut.flip;
      dirty = true;
    }
    ui.end_row();
    Self::choose(ui, "Type", "plane-cut-type", &mut self.plane_cut.cut, &TYPES, &mut dirty);
    ui.label("Fill Type");
    if super::choice::combo(ui, "plane-cut-fill", &FILLS, &mut self.plane_cut.fill, fill_icon) {
      dirty = true;
    }
    ui.end_row();
    let arrow = if self.plane_cut.numeric { "⏷" } else { "⏵" };
    if ui.add(egui::Label::new(format!("{arrow} Numerical Inputs")).sense(egui::Sense::click())).clicked() {
      self.plane_cut.numeric = !self.plane_cut.numeric;
    }
    ui.end_row();
    if self.plane_cut.numeric && has {
      Self::field(ui, "X Distance", &mut self.plane_cut.distance, &mut dirty);
      Self::field(ui, "Y Angle", &mut self.plane_cut.y_angle, &mut dirty);
      Self::field(ui, "Z Angle", &mut self.plane_cut.z_angle, &mut dirty);
    }
    self.dirty |= dirty;
  }

  /// The cut plane in the world (origin and unit normal towards the part Trim removes), and the
  /// body's centre on the picked plane before it moves.
  fn plane_cut_frame(&self, s: &Session) -> Option<(DVec3, DVec3, DVec3, fox_engine::kernel::Plane)> {
    let pl = super::plane_of(s, self.plane_cut.picked.as_ref()?)?;
    let st = &self.plane_cut;
    let (d, ya, za) = (self.len(s, &st.distance)?, self.ang(s, &st.y_angle)?, self.ang(s, &st.z_angle)?);
    let (o, n) = meshtools::cut_plane(pl.origin, pl.x, pl.y, d, ya, za, st.flip);
    let (_, pos, _) = self.plane_cut.mesh.as_ref()?;
    let c = pos.iter().sum::<DVec3>() / pos.len().max(1) as f64;
    let base_n = pl.normal();
    let foot = c - base_n * (c - pl.origin).dot(base_n);
    Some((o, n, foot, pl))
  }

  /// An arrow along the plane's normal for X Distance and dials about the plane's X and Y
  /// directions for Y Angle and Z Angle.
  pub(super) fn plane_cut_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (_, _, foot, pl) = self.plane_cut_frame(s)?;
    let st = &self.plane_cut;
    let (d, ya, za) = (self.len(s, &st.distance)?, self.ang(s, &st.y_angle)?, self.ang(s, &st.z_angle)?);
    let n = pl.normal();
    let at = foot + n * d;
    Some(vec![
      Handle::arrow(foot, n, d, mm(self.unit, d)),
      Handle::dial(at, pl.x, n, None, ya, deg(ya)),
      Handle::dial(at, pl.y, n, None, za, deg(za)),
    ])
  }

  pub(super) fn set_plane_cut_handle(&mut self, i: usize, v: f64, step: f64) {
    let v = snap(v, step);
    match i {
      0 => self.plane_cut.distance = mm(self.unit, v),
      1 => self.plane_cut.y_angle = format!("{:.1} deg", v.to_degrees()),
      _ => self.plane_cut.z_angle = format!("{:.1} deg", v.to_degrees()),
    }
  }

  /// The triangles Trim would remove, as the body is without the preview.
  pub(crate) fn plane_cut_removed(&self, s: &Session) -> Option<(Vec<DVec3>, Vec<u32>)> {
    if self.kind != Kind::PlaneCut || self.plane_cut.cut != "trim" {
      return None;
    }
    let (o, n, _, _) = self.plane_cut_frame(s)?;
    let (_, pos, tris) = self.plane_cut.mesh.as_ref()?;
    let (p, _, front) = meshtools::plane_cut(pos, tris, o, n);
    Some((p, front))
  }
}

/// Was the clear x at the right of a selection chip clicked?
fn clear_clicked(ui: &mut egui::Ui, chip: &egui::Response, id: &str) -> bool {
  let x = egui::Rect::from_min_max(egui::pos2(chip.rect.right() - 20.0, chip.rect.top()), chip.rect.max);
  ui.interact(x, ui.id().with(id), egui::Sense::click()).clicked()
}

/// Draws the open Plane Cut's plane round the body and tints red the part Trim removes.
pub(crate) fn overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  let Some(d) = app.dialog.as_ref().filter(|d| d.kind == Kind::PlaneCut) else { return };
  let s = &app.s;
  if let Some((p, front)) = d.plane_cut_removed(s) {
    for t in front.chunks_exact(3) {
      let pts: Option<Vec<Pos2>> = t.iter().map(|&i| project(p[i as usize])).collect();
      if let Some(pts) = pts {
        painter.add(Shape::convex_polygon(pts, REMOVED, Stroke::NONE));
      }
    }
  }
  let Some((o, n, foot, _)) = d.plane_cut_frame(s) else { return };
  let Some((_, pos, _)) = d.plane_cut.mesh.as_ref() else { return };
  let size = pos.iter().map(|p| (*p - foot).length()).fold(0.0, f64::max).max(1.0) * 1.2;
  let c = foot + n * n.dot(o - foot);
  let u = n.any_orthonormal_vector();
  let v = n.cross(u);
  let corners: Option<Vec<Pos2>> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
    .iter()
    .map(|(a, b)| project(c + (u * *a + v * *b) * size))
    .collect();
  if let Some(q) = corners {
    painter.add(Shape::convex_polygon(q.clone(), PLANE_FILL, Stroke::NONE));
    painter.add(Shape::closed_line(q, Stroke::new(1.2, PLANE_EDGE)));
  }
}
