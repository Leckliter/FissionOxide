//! Edit Feature on a Revolve, Shell, Offset Plane, Mirror or pattern: the creation dialog filled
//! in, its selection chips live. The stored picks are found again on the rolled-back model (and
//! selected, as the creation dialog shows them), and a new pick goes back to the feature through
//! `feature.edit`'s `picks`, as Fillet's, Chamfer's and Extrude's do.

use fox_engine::model::{AxisRef, BodySel, FeatureId, FeatureKind, OriginPlane, PlaneRef};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind};

/// The regions of `sketch` the stored profile points fall in, with a point inside each (none
/// stored: every region). None when one of them is gone.
fn regions(s: &Session, sketch: FeatureId, profiles: &[[f64; 2]]) -> Option<Vec<(usize, [f64; 2])>> {
  let r = s.model.sketches.get(&sketch)?;
  if profiles.is_empty() {
    return Some(r.profiles.iter().enumerate().map(|(i, x)| (i, x.interior_point())).collect());
  }
  profiles
    .iter()
    .map(|p| r.profiles.iter().position(|x| x.contains(*p)).map(|i| (i, r.profiles[i].interior_point())))
    .collect()
}

/// A stored plane as the dialogs pick one: an origin plane's name, `{feature}` or `{body, face}`.
fn plane_pick(s: &Session, plane: &PlaneRef) -> Option<Value> {
  Some(match plane {
    PlaneRef::Origin { plane } => json!(match plane {
      OriginPlane::Xy => "xy",
      OriginPlane::Xz => "xz",
      OriginPlane::Yz => "yz",
    }),
    PlaneRef::Construction { feature } => json!({"feature": feature.0}),
    PlaneRef::Face { face, .. } => json!({"body": face.body.0, "face": s.model.body(face.body)?.find_face(face)?}),
    PlaneRef::Free { .. } => return None,
  })
}

/// What a plane pick selects: a face or a construction plane (an origin plane isn't selected).
fn plane_sel(v: &Value) -> Option<Sel> {
  let get = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
  match (get("body"), get("face"), get("feature")) {
    (Some(b), Some(f), _) => Some(Sel::Face { body: fox_engine::model::BodyId(b), face: f }),
    (_, _, Some(f)) => Some(Sel::Feature { feature: FeatureId(f) }),
    _ => None,
  }
}

/// The bodies a stored body selection names on the rolled-back model; None for a component's.
fn body_ids(s: &Session, sel: &BodySel) -> Option<Vec<u32>> {
  let ids: Vec<u32> = match sel {
    BodySel::Bodies { ids } => ids.iter().map(|b| b.0).collect(),
    BodySel::All => s.model.bodies.iter().map(|b| b.id.0).collect(),
    BodySel::Component { .. } => return None,
  };
  ids.iter().all(|b| s.model.body(fox_engine::model::BodyId(*b)).is_some()).then_some(ids)
}

fn body_sels(ids: &[u32]) -> impl Iterator<Item = Sel> + '_ {
  ids.iter().map(|b| Sel::Body { body: fox_engine::model::BodyId(*b) })
}

impl Dialog {
  /// Finds a Revolve's, Shell's, Offset Plane's, Mirror's or pattern's stored picks on the
  /// rolled-back model and selects them; false when one can't be found (or `kind` is none of
  /// these), and the rows keep the stored picks.
  pub(super) fn load_more_edit_picks(&mut self, s: &mut Session, kind: &FeatureKind) -> bool {
    let find_edge = |s: &Session, er: &fox_engine::model::EdgeRef| -> Option<Value> {
      Some(json!({"body": er.body.0, "edge": s.model.body(er.body)?.find_edge(er)?}))
    };
    let selection: Vec<Sel> = match kind {
      FeatureKind::Revolve { sketch, profiles, axis, .. } => {
        let Some(picked) = regions(s, *sketch, profiles) else { return false };
        if let AxisRef::Edge { edge } = axis {
          let Some(e) = find_edge(s, edge) else { return false };
          self.solid.axis_edge = e;
        }
        self.sketch = Some(*sketch);
        self.profiles = picked.iter().map(|(_, p)| *p).collect();
        picked.iter().map(|(index, _)| Sel::Profile { sketch: *sketch, index: *index }).collect()
      }
      FeatureKind::Shell { faces, bodies, .. } => {
        let mut found = Vec::new();
        for fr in faces {
          let Some(f) = s.model.body(fr.body).and_then(|b| b.find_face(fr)) else { return false };
          found.push(json!({"body": fr.body.0, "face": f}));
        }
        if bodies.iter().any(|b| s.model.body(*b).is_none()) {
          return false;
        }
        self.faces = found;
        self.bodies = bodies.iter().map(|b| b.0).collect();
        let mut sel = self.picked_selection();
        sel.extend(body_sels(&self.bodies));
        sel
      }
      FeatureKind::ConstructionPlane { base, .. } => {
        let Some(plane) = plane_pick(s, base) else { return false };
        self.plane_label = "1 selected".into();
        self.manip.offset_base = plane_sel(&plane).map(|_| plane.clone());
        self.plane = plane;
        plane_sel(&self.plane).into_iter().collect()
      }
      FeatureKind::Mirror { bodies, plane, .. } => {
        let Some(plane) = plane_pick(s, plane) else { return false };
        if self.choice != "features" {
          let Some(ids) = body_ids(s, bodies) else { return false };
          self.bodies = ids;
        }
        self.last_pick = plane.clone();
        self.plane = plane;
        body_sels(&self.bodies).chain(plane_sel(&self.plane)).collect()
      }
      FeatureKind::RectangularPattern { bodies, .. } => {
        let Some(ids) = body_ids(s, bodies) else { return false };
        self.bodies = ids;
        body_sels(&self.bodies).collect()
      }
      FeatureKind::CircularPattern { bodies, axis, angle, symmetric, .. } => {
        let Some(ids) = body_ids(s, bodies) else { return false };
        if let AxisRef::Edge { edge } = axis {
          let Some(e) = find_edge(s, edge) else { return false };
          self.solid.axis_edge = e;
        }
        self.bodies = ids;
        self.choice = match (*symmetric, angle.trim() == "360 deg") {
          (true, _) => "symmetric",
          (false, true) => "full",
          (false, false) => "angle",
        };
        body_sels(&self.bodies).collect()
      }
      _ => return false,
    };
    s.selection = selection;
    true
  }

  /// `feature.edit`'s `picks` for an edited Revolve, Shell, Offset Plane, Mirror or pattern
  /// whose picks are live.
  pub(super) fn more_edit_picks(&self) -> Option<Value> {
    Some(match self.kind {
      Kind::Revolve => {
        json!({"sketch": self.sketch?.0, "profiles": self.profiles, "axis": self.revolve_axis()?})
      }
      Kind::Shell => {
        json!({"faces": self.faces, "bodies": self.bodies, "tangent_chain": !self.solid.shell_no_chain})
      }
      Kind::OffsetPlane => json!({"plane": self.plane}),
      Kind::Mirror if self.choice == "features" => json!({"plane": self.plane}),
      Kind::Mirror => json!({"plane": self.plane, "bodies": self.bodies}),
      Kind::RectPattern if self.pattern_kind() == "components" => return None,
      Kind::RectPattern => json!({"bodies": self.bodies}),
      Kind::CircPattern => json!({"bodies": self.bodies, "axis": self.circ_axis()?}),
      _ => return None,
    })
  }

  /// An edited Circular Pattern with live rows: its Distribution names the angle and symmetry.
  pub(super) fn circ_edit_distribution(&self, set: &mut Value) {
    if self.kind != Kind::CircPattern || !self.repicks {
      return;
    }
    set["symmetric"] = json!(self.choice == "symmetric");
    if self.choice == "full" {
      set["angle"] = json!("360 deg");
    }
  }
}
