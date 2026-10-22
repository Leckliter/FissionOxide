//! Two edge manipulators: Fillet's setback handle, Fusion's black-and-white disc at the picked
//! edge's end that slides along the edge (a drag sets Corner Type to Setback and its distance),
//! and Hem's arrow with its floating length box ("Thickness * 4") on the picked edge, the Edge
//! Flange's arrow out of the sheet (see [`super::flange::flange_anchor`]).

use fox_engine::Session;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::BodyId;
use serde_json::Value;

use super::manip::mm;
use super::{Dialog, Kind, Manipulator};
use crate::handles::{Handle, Head, Shape};

/// The end of the edge `key` names and the unit direction from there along the edge towards
/// its middle (the way a setback grows).
pub(super) fn setback_anchor(s: &Session, key: &Value) -> Option<(DVec3, DVec3)> {
  let get = |k: &str| key.get(k).and_then(Value::as_u64).map(|v| v as u32);
  let body = s.model.body(BodyId(get("body")?))?;
  let e = body.edges().iter().find(|e| Some(e.id) == get("edge"))?;
  let dir = (e.mid - e.start).try_normalize().or_else(|| (e.end - e.start).try_normalize())?;
  Some((e.start, dir))
}

/// A sheet metal length expression in the scope of the sheet the edge `key` is on: the design's
/// parameters and `Thickness`.
fn sheet_len(s: &Session, key: &Value, e: &str) -> Option<f64> {
  struct Sheet<'a> {
    params: &'a std::collections::HashMap<String, f64>,
    thickness: Option<f64>,
  }
  impl fox_engine::expr::Scope for Sheet<'_> {
    fn get(&self, name: &str) -> Option<f64> {
      match self.thickness {
        Some(t) if name == "Thickness" => Some(t),
        _ => self.params.get(name).copied(),
      }
    }
  }
  let body = key.get("body").and_then(Value::as_u64).and_then(|b| s.model.body(BodyId(b as u32)));
  let scope = Sheet { params: &s.model.params, thickness: body.and_then(|b| b.sheet.as_ref()).map(|i| i.thickness) };
  fox_engine::expr::eval(e, fox_engine::expr::Kind::Length, &scope).ok()
}

impl Dialog {
  /// Fillet's setback handle: on the first picked edge's end, as it was before the preview, at
  /// the corner setback (none: on the end itself).
  pub(super) fn fillet_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    if self.kind != Kind::Fillet || self.editing.is_some() || self.edges.is_empty() {
      return None;
    }
    let (key, origin, dir) = self.manip.setback.as_ref()?;
    if Some(key) != self.edges.first() {
      return None;
    }
    let value = if self.solid.setback_corner { self.len(s, &self.solid.setback).unwrap_or(0.0).max(0.0) } else { 0.0 };
    Some(vec![Handle {
      shape: Shape::Arrow { origin: *origin, dir: *dir },
      value,
      head: Head::Disc,
      label: String::new(),
      snap: None,
    }])
  }

  /// A dragged setback: Corner Type turns to Setback, at the dragged distance (never negative).
  pub(super) fn set_setback_handle(&mut self, v: f64) {
    self.solid.setback_corner = true;
    self.solid.setback = mm(self.unit, v.max(0.0));
  }

  /// Hem's arrow: out of the first picked edge the way an edge flange's goes, as long as the hem.
  pub(super) fn hem_manipulator(&self, s: &Session) -> Option<Manipulator> {
    let (key, origin, dir) = self.manip.anchor.as_ref()?;
    if Some(key) != self.edges.first() {
      return None;
    }
    let sign = if self.flip { -1.0 } else { 1.0 };
    Some(Manipulator { origin: *origin, dir: *dir * sign, value: sheet_len(s, key, &self.distance)? })
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use fox_engine::kernel::glam::DVec3;
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use crate::handles::{Head, Shape};

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e:?}"))
  }

  /// Fillet shows a disc at the picked edge's end besides the radius arrow; dragging it along
  /// the edge sets Corner Type Setback and the distance, and the handle follows the field.
  #[test]
  fn fillet_has_a_setback_handle_at_the_edges_end() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let b = &s.model.bodies[0];
    let e = b.edges()[0];
    let key = json!({"body": b.id.0, "edge": e.id});
    let mut d = Dialog::blank(Kind::Fillet);
    d.edges = vec![key];
    d.radius = "2 mm".into();
    d.refresh_manip(&s);
    let h = d.handles(&s);
    assert_eq!(h.len(), 1, "the setback handle");
    assert_eq!(h[0].head, Head::Disc, "black and white");
    let Shape::Arrow { origin, dir } = h[0].shape else { panic!("an arrow along the edge") };
    assert!(origin.distance(e.start) < 1e-9, "at the edge's end");
    assert!(dir.dot((e.end - e.start).normalize()) > 0.99, "along the edge");
    assert_eq!(h[0].value, 0.0);
    assert!(d.manipulator(&s).is_some(), "the radius arrow stays");
    d.set_handle(&s, 0, 3.0, 0.1);
    assert!(d.solid.setback_corner, "a drag sets Corner Type to Setback");
    assert_eq!(d.solid.setback, "3 mm");
    assert!((d.handles(&s)[0].value - 3.0).abs() < 1e-9);
    assert!(d.handle_field(0).is_none(), "the radius keeps the value box");
  }

  /// Hem shows the edge flange's arrow on its edge, its value box holding "Thickness * 4" and
  /// the arrow as long as four thicknesses; a drag writes the length.
  #[test]
  fn hem_has_an_arrow_and_a_length_box_on_its_edge() {
    let mut s = Session::default();
    run(&mut s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
    run(&mut s, "sheet.flange", json!({}));
    let b = &s.model.bodies[0];
    let t = b.sheet.as_ref().unwrap().thickness;
    let e = b.edges().iter().find(|e| e.mid.distance(DVec3::new(50.0, 50.0, t)) < 1e-6).copied().unwrap();
    let mut app = crate::App::with_session(s);
    let mut d = Dialog::open(&mut app, Kind::Hem);
    d.edges = vec![json!({"body": app.s.model.bodies[0].id.0, "edge": e.id})];
    d.refresh_manip(&app.s);
    let m = d.manipulator(&app.s).expect("the hem's arrow");
    assert!(m.origin.distance(e.mid) < 1e-6, "on the edge");
    assert!((m.value - 4.0 * t).abs() < 1e-6, "four thicknesses long");
    assert!(m.dir.dot(DVec3::Z).abs() > 0.99, "out of the sheet");
    assert_eq!(d.value_box_field().cloned().as_deref(), Some("Thickness * 4"));
    d.set_manipulator(6.0, 0.1);
    assert_eq!(d.distance, "6 mm");
  }
}
