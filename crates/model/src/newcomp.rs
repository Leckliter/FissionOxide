//! New Component: the operation of Extrude, Revolve, Loft, Sweep, the primitives and the other
//! body-making features that puts the result in a component of its own, as Fusion's "New
//! Component" option does. The feature regenerates its bodies like New Body; after each
//! regeneration [`sync`] puts the bodies it created in its component (see [`Feature::component`]).

use crate::regen::{Body, MAX_BODIES_PER_FEATURE};
use crate::{BodyId, Design, FeatureKind, Operation};

/// The operation of a feature that makes bodies with one (new body, join, cut, …).
pub fn operation(kind: &FeatureKind) -> Option<Operation> {
  Some(match kind {
    FeatureKind::Extrude { operation, .. }
    | FeatureKind::Revolve { operation, .. }
    | FeatureKind::Primitive { operation, .. }
    | FeatureKind::Loft { operation, .. }
    | FeatureKind::Sweep { operation, .. }
    | FeatureKind::Flange { operation, .. }
    | FeatureKind::Coil { operation, .. }
    | FeatureKind::Pipe { operation, .. }
    | FeatureKind::Thicken { operation, .. }
    | FeatureKind::BoundaryFill { operation, .. }
    | FeatureKind::Form { operation, .. } => *operation,
    _ => return None,
  })
}

/// Whether `body` was created by feature `feature` (body ids are numbered per feature).
pub fn created_by(body: BodyId, feature: crate::FeatureId) -> bool {
  body.0 / MAX_BODIES_PER_FEATURE == feature.0
}

/// Gives every New Component feature without a component a new top-level one, and puts the
/// bodies each created (and that no component holds yet) in its component. Returns whether a
/// body changed component, so the caller regenerates again for joints to move it.
pub fn sync(design: &mut Design, bodies: &[Body]) -> bool {
  let mut moved = false;
  for i in 0..design.features.len() {
    let f = &design.features[i];
    if operation(&f.kind) != Some(Operation::NewComponent) {
      continue;
    }
    let fid = f.id;
    let comp = match f.component {
      Some(c) => c,
      None => {
        let Ok(c) = design.add_component("", Vec::new()) else { return moved };
        design.features[i].component = Some(c);
        c
      }
    };
    // a component the user deleted stays deleted; its bodies stay loose
    if design.component(comp).is_none() {
      continue;
    }
    let new: Vec<BodyId> =
      bodies.iter().map(|b| b.id).filter(|b| created_by(*b, fid) && design.component_of(*b).is_none()).collect();
    if let (false, Some(c)) = (new.is_empty(), design.component_mut(comp)) {
      c.bodies.extend(new);
      moved = true;
    }
  }
  moved
}

/// The leading number of a component name (`"02 - Letters"` → 2), if it starts with digits.
pub fn leading_number(name: &str) -> Option<u32> {
  let t = name.trim_start();
  let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
  digits.parse().ok()
}

/// The smallest number from 1 up that no top-level component name starts with (filling gaps),
/// zero-padded to `digits`: `"01"`, `"02"`, … The number layered parts are named with.
pub fn next_number(design: &Design, digits: usize) -> (u32, String) {
  let used: Vec<u32> =
    design.components.iter().filter(|c| c.parent.is_none()).filter_map(|c| leading_number(&c.name)).collect();
  let n = (1..).find(|n| !used.contains(n)).unwrap_or(1);
  (n, format!("{n:0digits$}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn numbers_and_gaps() {
    assert_eq!(leading_number("02 - Letters"), Some(2));
    assert_eq!(leading_number(" 7"), Some(7));
    assert_eq!(leading_number("Component1"), None);
    let mut d = Design::new("t");
    assert_eq!(next_number(&d, 2), (1, "01".into()));
    for n in ["01", "02 - Back", "04"] {
      d.add_component(n, Vec::new()).unwrap();
    }
    assert_eq!(next_number(&d, 2), (3, "03".into()));
  }
}
