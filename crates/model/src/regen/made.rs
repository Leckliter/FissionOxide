//! What each timeline feature made, for the timeline's hover highlight: the bodies a feature
//! created, or the faces it added to bodies that already existed (a fillet's rounds, a cut's
//! walls), told apart by the names regeneration gives faces (see [`crate::naming`]): a face none
//! of whose names the body had before the feature is the feature's.

use std::collections::HashSet;

use super::{Regenerator, State};
use crate::BodyId;

/// What a feature added to the model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Made {
  /// Bodies the feature created.
  pub bodies: Vec<BodyId>,
  /// Faces it added to bodies that existed before it (1-based face ids, per body).
  pub faces: Vec<(BodyId, Vec<u32>)>,
}

impl Made {
  pub fn is_empty(&self) -> bool {
    self.bodies.is_empty() && self.faces.is_empty()
  }
}

impl Regenerator {
  /// What the feature at timeline index `i` made, from the cached states before and after it.
  /// `None` when that step isn't cached (not regenerated yet). A rolled-back feature keeps its
  /// last cached state, so the caller checks the timeline marker.
  pub fn made(&self, i: usize) -> Option<Made> {
    let after = &self.cache.get(i)?.1;
    let before = i.checked_sub(1).and_then(|j| self.cache.get(j)).map(|c| &c.1);
    Some(made_between(before, after))
  }
}

fn made_between(before: Option<&State>, after: &State) -> Made {
  let mut made = Made::default();
  for body in &after.bodies {
    let Some(prev) = before.and_then(|s| s.bodies.iter().find(|b| b.id == body.id)) else {
      made.bodies.push(body.id);
      continue;
    };
    let known: HashSet<u64> = prev.names().faces.iter().flatten().copied().collect();
    // a face with no names (imported geometry, calls without history) can't be told apart
    let new: Vec<u32> = body
      .names()
      .faces
      .iter()
      .enumerate()
      .filter(|(_, names)| !names.is_empty() && names.iter().all(|n| !known.contains(n)))
      .map(|(k, _)| k as u32 + 1)
      .collect();
    if !new.is_empty() {
      made.faces.push((body.id, new));
    }
  }
  made
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::regen::Model;
  use crate::{Design, EdgeRef, FeatureId, FeatureKind, Operation, OriginPlane, PlaneRef, Primitive};

  const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };

  fn boxed() -> (Design, Regenerator) {
    let mut d = Design::new("made");
    d.add_feature(FeatureKind::Primitive {
      shape: Primitive::Box { width: "20".into(), depth: "20".into(), height: "10".into() },
      plane: XY,
      at: [0.0, 0.0],
      operation: Operation::NewBody,
    })
    .unwrap();
    (d, Regenerator::default())
  }

  /// Fillets one top edge of the box.
  fn filleted(d: &mut Design, m: &Model) -> FeatureId {
    let body = &m.bodies[0];
    let e = body.edges().iter().find(|e| (e.mid.z - 10.0).abs() < 1e-6).unwrap();
    d.add_feature(FeatureKind::Fillet {
      edges: vec![EdgeRef::from_edge(body.id, e, body.faces())],
      radius: "2".into(),
      radius2: None,
      setback: None,
      chord: false,
      radii: vec![],
      sets: vec![],
      distance2: None,
      flipped: false,
    })
    .unwrap()
  }

  #[test]
  fn a_box_makes_its_body_and_a_fillet_its_rounds() {
    let (mut d, mut r) = boxed();
    let m = r.regenerate(&mut d);
    let body = m.bodies[0].id;
    let made = r.made(0).expect("cached");
    assert_eq!(made.bodies, vec![body]);
    assert!(made.faces.is_empty());
    let f = filleted(&mut d, &m);
    let m = r.regenerate(&mut d);
    assert_eq!(m.status[1].error, None, "{:?}", m.status[1]);
    let i = d.features.iter().position(|x| x.id == f).unwrap();
    let made = r.made(i).expect("cached");
    assert!(made.bodies.is_empty(), "the body existed: {made:?}");
    assert_eq!(made.faces.len(), 1);
    assert_eq!(made.faces[0].0, body);
    assert_eq!(made.faces[0].1.len(), 1, "one round: {made:?}");
    assert_eq!(m.bodies[0].faces().len(), 7);
    assert!(r.made(5).is_none(), "nothing there");
  }
}
