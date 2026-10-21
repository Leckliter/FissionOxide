//! What a timeline feature made, for the timeline's hover highlight (see
//! [`fox_model::regen::Made`]).

use fox_model::FeatureId;
use fox_model::regen::Made;

use crate::Session;

impl Session {
  /// What `feature` made: the bodies it created, or the faces it added to bodies that existed
  /// before it. Empty when nothing is known (the feature is rolled back, failed, or only changed
  /// faces that keep their names).
  pub fn feature_made(&self, feature: FeatureId) -> Made {
    let Some(i) = self.design.features.iter().position(|f| f.id == feature) else { return Made::default() };
    if i >= self.design.active_len() || self.design.features[i].suppressed {
      return Made::default();
    }
    self.regen.made(i).unwrap_or_default()
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::Session;

  #[test]
  fn a_feature_made_its_body_or_faces_unless_rolled_back() {
    let mut s = Session::default();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let body = s.model.bodies[0].id;
    let edge = s.model.bodies[0].edges()[0].id;
    s.execute("fillet", &json!({"edges": [{"body": body.0, "edge": edge}], "radius": 1})).unwrap();
    let (box_f, fillet) = (s.design.features[0].id, s.design.features[1].id);
    assert_eq!(s.feature_made(box_f).bodies, vec![body]);
    let made = s.feature_made(fillet);
    assert!(made.bodies.is_empty());
    assert_eq!(made.faces.len(), 1);
    assert_eq!(made.faces[0].1.len(), 1, "{made:?}");
    // rolled back, nothing is highlighted
    s.execute("timeline.marker", &json!({"at": 1})).unwrap();
    assert!(s.feature_made(fillet).is_empty());
    assert_eq!(s.feature_made(box_f).bodies, vec![body]);
    s.execute("timeline.marker", &json!({"at": null})).unwrap();
    s.execute("feature.suppress", &json!({"feature": fillet.0, "suppressed": true})).unwrap();
    assert!(s.feature_made(fillet).is_empty(), "suppressed: nothing");
  }
}
