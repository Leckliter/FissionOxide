//! Press Pull on a primitive's top face edits the primitive, as in Fusion: OFFSET FACE opens on
//! the box's or cylinder's Height and OK sets that height (Edit Box or Edit Cylinder shows it),
//! adding no timeline item.

use fox_engine::Session;
use fox_engine::model::{BodyId, FeatureId, FeatureKind, Primitive, naming};
use serde_json::{Value, json};

use super::Dialog;

/// The primitive a Press Pull edits instead of offsetting the face.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Owner {
  pub(super) feature: Option<FeatureId>,
  /// "box" or "cylinder", and its other sizes: width and depth, or the radius.
  shape: &'static str,
  sizes: [String; 2],
  /// Its height (mm) before the edit: the arrow starts that far below the picked face.
  height: f64,
}

/// The Box or Cylinder whose top face is the one face picked, with its height expression. A
/// primitive's faces are named by their place in the shape (`naming::by_index`): a box's sixth
/// face and a cylinder's second are their tops.
fn owner_of(s: &Session, faces: &[Value]) -> Option<(Owner, String)> {
  let [f] = faces else { return None };
  let body = BodyId(u32::try_from(f.get("body")?.as_u64()?).ok()?);
  let face = u32::try_from(f.get("face")?.as_u64()?).ok()?;
  let names = s.model.body(body)?.names().of(face).to_vec();
  let d = &s.design;
  d.features.iter().take(d.active_len()).filter(|f| !f.suppressed).find_map(|feat| {
    let FeatureKind::Primitive { shape, .. } = &feat.kind else { return None };
    let (kind, count, top, sizes, height) = match shape {
      Primitive::Box { width, depth, height } => ("box", 6, 6, [width.clone(), depth.clone()], height),
      Primitive::Cylinder { radius, height } => ("cylinder", 3, 2, [radius.clone(), String::new()], height),
      _ => return None,
    };
    let name = *naming::by_index(count, feat.id).faces.get(top - 1)?.first()?;
    if !names.contains(&name) {
      return None;
    }
    let h = fox_engine::expr::eval(height, fox_engine::expr::Kind::Length, &s.model.params).ok()?;
    Some((Owner { feature: Some(feat.id), shape: kind, sizes, height: h }, height.clone()))
  })
}

impl Dialog {
  /// Press Pull: finds the primitive the picked face tops; while there is one, Distance is its
  /// height, as Fusion fills it.
  pub(super) fn sync_press_owner(&mut self, s: &Session) {
    let found = owner_of(s, &self.faces);
    if found.as_ref().map(|(o, _)| o.feature) == Some(self.solid.owner.feature) {
      return;
    }
    match found {
      Some((o, height)) => {
        self.distance = self.unit.shown(&height);
        self.solid.owner = o;
      }
      None => {
        if self.solid.owner.feature.is_some() {
          self.distance = "2 mm".into();
        }
        self.solid.owner = Owner::default();
      }
    }
  }

  /// Press Pull's command: the primitive's height set to Distance, else the face offset.
  pub(super) fn press_pull_command(&self) -> (&'static str, Value) {
    let o = &self.solid.owner;
    // New Offset adds an Offset Face feature whatever the face
    let Some(id) = o.feature.filter(|_| self.choice != "new") else {
      return ("press_pull", json!({"faces": self.faces, "distance": self.distance}));
    };
    let shape = match o.shape {
      "box" => json!({"type": "box", "width": o.sizes[0], "depth": o.sizes[1], "height": self.distance}),
      _ => json!({"type": "cylinder", "radius": o.sizes[0], "height": self.distance}),
    };
    ("feature.edit", json!({"feature": id.0, "set": {"shape": shape}}))
  }

  /// How far below the picked face Press Pull's arrow starts: the edited primitive's base (0 for
  /// a face offset).
  pub(super) fn press_pull_base(&self) -> f64 {
    if self.solid.owner.feature.is_some() && self.choice != "new" { self.solid.owner.height } else { 0.0 }
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use fox_engine::model::{FeatureKind, Primitive};
  use serde_json::json;

  use super::super::{Dialog, Kind};
  use crate::App;
  use fox_engine::Sel;

  fn top_face(app: &App) -> Sel {
    let b = &app.s.model.bodies[0];
    let plane = fox_engine::kernel::SurfaceKind::Plane;
    let f = b.faces().iter().find(|f| f.kind == plane && f.normal.z > 0.9).unwrap().id;
    Sel::Face { body: b.id, face: f }
  }

  #[test]
  fn press_pull_on_a_cylinders_top_edits_its_height() {
    let mut app = App::with_session(Session::default());
    app.s.execute("cylinder", &json!({"radius": 10, "height": 20})).unwrap();
    app.s.selection = vec![top_face(&app)];
    let mut d = Dialog::open(&mut app, Kind::PressPull);
    // OFFSET FACE opens on the cylinder's height
    assert_eq!((d.kind, d.distance.as_str()), (Kind::PressPull, "20"));
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, p["set"]["shape"]["radius"].clone()), ("feature.edit", json!("10")));
    // the arrow starts at the base and reaches the top
    d.refresh_manip(&app.s);
    let m = d.manipulator(&app.s).expect("the distance arrow");
    assert!((m.origin.z, m.value) == (0.0, 20.0), "{m:?}");
    d.distance = "5 mm".into();
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    // no new timeline item: the cylinder is 5 mm tall
    assert_eq!(app.s.design.features.len(), 1);
    let FeatureKind::Primitive { shape: Primitive::Cylinder { height, .. }, .. } = &app.s.design.features[0].kind
    else {
      panic!("{:?}", app.s.design.features[0].kind)
    };
    assert_eq!(height, "5 mm");
    let top = app.s.model.bodies[0].shape.mass().unwrap().bbox_max.z;
    assert!((top - 5.0).abs() < 1e-6, "{top}");
    // New Offset still adds an Offset Face feature
    d.unpreview(&mut app.s);
    d.choice = "new";
    assert_eq!(d.command().unwrap().0, "press_pull");
  }

  #[test]
  fn press_pull_on_a_boxs_top_edits_its_height_and_elsewhere_offsets() {
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 20, "depth": 30, "height": 10})).unwrap();
    app.s.selection = vec![top_face(&app)];
    let d = Dialog::open(&mut app, Kind::PressPull);
    assert_eq!(d.distance, "10");
    let p = d.command().unwrap().1;
    assert_eq!(p["set"]["shape"], json!({"type": "box", "width": "20", "depth": "30", "height": "10"}));
    // the bottom face is an ordinary offset
    let b = &app.s.model.bodies[0];
    let bottom = b.faces().iter().find(|f| f.normal.z < -0.9).unwrap().id;
    app.s.selection = vec![Sel::Face { body: b.id, face: bottom }];
    let d = Dialog::open(&mut app, Kind::PressPull);
    assert_eq!((d.command().unwrap().0, d.distance.as_str()), ("press_pull", "2 mm"));
  }
}
