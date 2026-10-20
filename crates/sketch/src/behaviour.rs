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
//!    - the kernel: pass the answer in `fox_kernel::FaceOptions` to `Shape::face_with`, or, for
//!      a per-segment choice, on the profile's segments (as the one-edge flags below).
//! 3. Test both: a legacy sketch or design (`Behaviours::legacy()`) still builds the old geometry,
//!    a new one the new.
//!
//! A sketch added to an old design gets [`CURRENT`] (it is new geometry), not the design's set.
//!
//! # One edge per sketch curve
//!
//! Fusion makes one edge per sketch curve; a profile reaches the kernel as pieces (an ellipse as
//! two half arcs, a spline as Bézier pieces, a circle split where other curves touch it). Each
//! loop segment records the curve it was cut from and whether it may merge with the next segment
//! ([`Loop::src`](crate::Loop::src)). `find_profiles` sets that flag only for consecutive pieces of
//! the same curve whose kind's [`one_edge_behaviour`] is on ([`Sketch::merge_flags`]), so pieces
//! of different curves never merge, however smoothly they join. The kernel's `make_face` (and
//! `Shape::path_with` for sweep paths) turns each run of mergeable segments into one edge, and
//! builds everything else as before. To ship one of these, move its name from [`PENDING`] to
//! [`CURRENT`].

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{CurveId, CurveKind, Sketch};

// ---- behaviour names: add new ones at the end ----

/// One edge per ellipse: the pieces of a whole ellipse or elliptical arc (an ellipse reaches a
/// profile as two half arcs; curves touching or crossing it split it further) become one exact
/// edge where they run on into each other in a loop, so a whole ellipse is one closed edge and
/// extrudes to one side face, as in Fusion.
pub const ONE_EDGE_ELLIPSE: &str = "one_edge_ellipse";
/// One edge per control-point spline: its cubic Bézier pieces become one B-spline edge.
pub const ONE_EDGE_CONTROL_SPLINE: &str = "one_edge_control_spline";
/// One edge per fit-point spline: its cubic Bézier pieces become one B-spline edge.
pub const ONE_EDGE_FIT_SPLINE: &str = "one_edge_fit_spline";
/// One edge per arc or circle: adjacent arcs of one circle or arc, split where other curves touch
/// it (an end lying on it, a dangling line pruned from the profile), become one arc, or the whole
/// circle, where they run on into each other in a loop.
pub const ONE_EDGE_SPLIT_ARCS: &str = "one_edge_split_arcs";
/// Threads (feature level): a modeled thread is the ISO 68-1 flat-root groove cut square at its
/// ends (not a sharp V running out past them), its offset and flip are measured from the axis's
/// canonical low end over the face's exact extent, and the threaded crest is merged into one
/// face. Without it a thread rebuilds as before. A thread with an ISO designation always gets it:
/// files saved before this flag have no designation.
pub const ISO_THREADS: &str = "iso_threads";
/// Joint axes as in Fusion: an unflipped joint lines the moving origin's axes up with the fixed
/// origin's (the Z axes the same way), and flipping turns the moving frame half a turn about its X
/// so the Z axes face each other. Without it (designs saved before), the Z axes face each other
/// unless the joint is flipped.
pub const FUSION_JOINT_AXES: &str = "fusion_joint_axes";
/// A construction plane offset from the XZ origin plane moves along +Y for a positive offset, as
/// in Fusion (#62). Without it a positive offset moves along the XZ frame's normal, -Y.
pub const XZ_OFFSET_PLUS_Y: &str = "xz_offset_plus_y";
/// Combine Intersect with several tools keeps the part of the target inside any tool,
/// target ∩ (t1 ∪ t2 ∪ …), as in Fusion (#92). Without it the target is intersected with each
/// tool in turn, target ∩ t1 ∩ t2 ∩ ….
pub const INTERSECT_ANY_TOOL: &str = "intersect_any_tool";

/// Every behaviour this build implements; new sketches and designs get all of them. Append only.
pub const CURRENT: &[&str] = &[
  // one name per line, e.g. MERGE_SMOOTH_EDGES,
  ISO_THREADS,
  FUSION_JOINT_AXES,
  XZ_OFFSET_PLUS_Y,
  INTERSECT_ANY_TOOL,
];

/// Behaviours declared and implemented but not yet given to new geometry; a change ships by
/// moving its name from here to [`CURRENT`]. Tests turn them on with
/// [`Behaviours::enable_pending_for_tests`].
pub const PENDING: &[&str] = &[ONE_EDGE_ELLIPSE, ONE_EDGE_CONTROL_SPLINE, ONE_EDGE_FIT_SPLINE, ONE_EDGE_SPLIT_ARCS];

/// A flag only this crate's own unit tests register, to exercise [`Behaviours::has`].
const TEST_ONLY: &str = "test_only_dummy";

/// The names new geometry gets: [`CURRENT`], plus a dummy one in this crate's tests.
fn registered() -> impl Iterator<Item = &'static str> {
  CURRENT.iter().copied().chain(cfg!(test).then_some(TEST_ONLY))
}

/// The names this build can act on: [`registered`] and [`PENDING`].
fn known() -> impl Iterator<Item = &'static str> {
  registered().chain(PENDING.iter().copied())
}

/// The behaviour under which consecutive profile pieces of one curve of this kind become one edge,
/// or `None` when the kind's pieces always stay separate edges.
pub fn one_edge_behaviour(kind: &CurveKind) -> Option<&'static str> {
  match kind {
    CurveKind::Ellipse { .. } | CurveKind::EllipticalArc { .. } => Some(ONE_EDGE_ELLIPSE),
    CurveKind::ControlSpline { .. } => Some(ONE_EDGE_CONTROL_SPLINE),
    CurveKind::Spline { .. } => Some(ONE_EDGE_FIT_SPLINE),
    CurveKind::Arc { .. } | CurveKind::Circle { .. } => Some(ONE_EDGE_SPLIT_ARCS),
    // a quintic spline (blend curve) reaches a profile as one segment, never split: already one edge
    CurveKind::Line { .. } | CurveKind::Bezier { .. } | CurveKind::Conic { .. } | CurveKind::QuinticSpline { .. } => {
      None
    }
  }
}

impl Sketch {
  /// Whether a segment cut from curve `a`, followed in a chain by one cut from curve `b`, may
  /// become one edge with it: both come from the same curve and that curve kind's
  /// [`one_edge_behaviour`] is on for this sketch.
  pub fn may_merge(&self, a: CurveId, b: CurveId) -> bool {
    a == b && self.curve(a).and_then(|c| one_edge_behaviour(&c.kind)).is_some_and(|name| self.behaviours.has(name))
  }

  /// Merge flags for a chain of segments cut from the curves `src`, in order: entry `k` says
  /// segment `k` may become one edge with segment `k + 1` ([`Sketch::may_merge`]). In a `cyclic`
  /// chain (a loop) the last segment runs on into the first; in an open one its flag is false. A
  /// lone segment never merges with itself.
  pub fn merge_flags(&self, src: &[CurveId], cyclic: bool) -> Vec<bool> {
    let n = src.len();
    (0..n)
      .map(|k| {
        let next = if k + 1 < n {
          Some(k + 1)
        } else if cyclic && n > 1 {
          Some(0)
        } else {
          None
        };
        next.is_some_and(|j| self.may_merge(src[k], src[j]))
      })
      .collect()
  }
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
    debug_assert!(known().any(|n| n == name), "unknown behaviour {name:?}: add it to CURRENT");
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

  /// Turns on a [`PENDING`] behaviour, which new geometry doesn't get yet: for testing a change
  /// before it ships.
  #[doc(hidden)]
  pub fn enable_pending_for_tests(&mut self, name: &str) {
    assert!(PENDING.contains(&name), "{name:?} is not a pending behaviour");
    self.insert(name);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
