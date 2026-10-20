//! Named geometry-behaviour flags: old files keep rebuilding exactly as they did, new designs get
//! the new behaviour.
//!
//! A [`Sketch`](crate::Sketch) and a `Design` each carry a [`Behaviours`] set. A new sketch or
//! design starts with every name in [`CURRENT`]; a file read from disk keeps exactly the names it
//! stored, and a file saved before flags existed (no `behaviours` field) has none, so it rebuilds
//! with the legacy code. Names the running build does not know (from a newer build) are kept, so
//! they survive a load and save.
//!
//! Named flags rather than a version number, so parallel changes don't fight over one integer.
//!
//! # Gating a geometry change
//!
//! 1. Add a constant for the name at the end of the constants below and append it to
//!    [`CURRENT`] (one line each; never rename or remove a published name):
//!    `pub const MERGE_SMOOTH_EDGES: &str = "merge_smooth_edges";`
//! 2. Gate the new code on the set of the geometry it changes, keeping the old code path:
//!    - sketch level (profiles, solver, curve evaluation): `if self.behaviours.has(MERGE_SMOOTH_EDGES)`
//!      inside `Sketch` methods, or `sketch.behaviours.has(..)` in `find_profiles`;
//!    - feature / regen level: `self.behaviours.has(..)` in the model's `regen::Ctx` (the design's
//!      set);
//!    - the kernel: pass the answer in `fox_kernel::FaceOptions` to `Shape::face_with`.
//! 3. Test both: a legacy sketch or design (`Behaviours::legacy()`) still builds the old geometry,
//!    a new one the new.
//!
//! A sketch added to an old design gets [`CURRENT`] (it is new geometry), not the design's set.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

// ---- behaviour names: add new ones at the end ----

/// Every behaviour this build implements; new sketches and designs get all of them. Append only.
pub const CURRENT: &[&str] = &[
  // one name per line, e.g. MERGE_SMOOTH_EDGES,
];

/// A flag only this crate's own unit tests register, to exercise [`Behaviours::has`].
const TEST_ONLY: &str = "test_only_dummy";

/// The names new geometry gets: [`CURRENT`], plus a dummy one in this crate's tests.
fn registered() -> impl Iterator<Item = &'static str> {
  CURRENT.iter().copied().chain(cfg!(test).then_some(TEST_ONLY))
}

/// The geometry behaviours a sketch or design was made with (see the module docs).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Behaviours(BTreeSet<String>);

impl Behaviours {
  /// No flags: how files saved before a behaviour existed rebuild.
  pub fn legacy() -> Self {
    Self::default()
  }

  /// Every behaviour this build implements, for new geometry.
  pub fn current() -> Self {
    Behaviours(registered().map(str::to_owned).collect())
  }

  /// Whether the geometry was made with behaviour `name`. Pass the name's constant.
  pub fn has(&self, name: &str) -> bool {
    debug_assert!(registered().any(|n| n == name), "unknown behaviour {name:?}: add it to CURRENT");
    self.0.contains(name)
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// All names, known or not, in order.
  pub fn iter(&self) -> impl Iterator<Item = &str> {
    self.0.iter().map(String::as_str)
  }

  pub fn insert(&mut self, name: impl Into<String>) {
    self.0.insert(name.into());
  }

  pub fn remove(&mut self, name: &str) {
    self.0.remove(name);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Sketch;

  #[test]
  fn a_sketch_without_behaviours_is_legacy() {
    let s: Sketch = serde_json::from_str(r#"{"points": [], "curves": [], "constraints": [], "next_id": 1}"#).unwrap();
    assert!(s.behaviours.is_empty());
    assert!(!s.behaviours.has(TEST_ONLY));
  }

  #[test]
  fn a_new_sketch_has_every_current_behaviour() {
    let s = Sketch::new();
    assert_eq!(s.behaviours, Behaviours::current());
    for n in CURRENT {
      assert!(s.behaviours.has(n));
    }
    assert!(s.behaviours.has(TEST_ONLY));
  }

  #[test]
  fn unknown_names_survive_a_round_trip() {
    let mut s = Sketch::new();
    s.behaviours.insert("from_a_newer_build");
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("from_a_newer_build"));
    let back: Sketch = serde_json::from_str(&json).unwrap();
    assert_eq!(back.behaviours, s.behaviours);
    assert!(back.behaviours.iter().any(|n| n == "from_a_newer_build"));
  }

  #[test]
  fn a_legacy_sketch_saves_without_the_field() {
    let mut s = Sketch::new();
    s.behaviours = Behaviours::legacy();
    assert!(!serde_json::to_string(&s).unwrap().contains("behaviours"));
  }
}
