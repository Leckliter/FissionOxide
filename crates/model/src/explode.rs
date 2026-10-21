//! Exploded views: components pulled apart for display. An exploded view never enters the
//! timeline and never moves a component for joints, drawings' geometry or exact export; it only
//! offsets what the viewport, renders and (on request) mesh exports show.

use std::collections::HashMap;

use fox_kernel::glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::{BodyId, ComponentId, Design};

/// The order components are spread in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplodeOrder {
  /// By each component's centre along the direction (the lowest stays put).
  #[default]
  Position,
  /// As the components are listed in the browser.
  Browser,
}

/// A named exploded view, saved with the design.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExplodedView {
  pub name: String,
  /// The components that move apart (each carries the components below it); empty = every
  /// top-level component.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub components: Vec<ComponentId>,
  /// The direction they move in; `None` = picked from the layout (see [`auto_direction`]).
  #[serde(default)]
  pub direction: Option<[f64; 3]>,
  /// Spacing: the i-th of N components moves i × range × factor / N, the range being the
  /// assembly's extent along the direction.
  pub factor: f64,
  #[serde(default)]
  pub order: ExplodeOrder,
}

impl Default for ExplodedView {
  fn default() -> Self {
    ExplodedView {
      name: "Exploded View".into(),
      components: Vec::new(),
      direction: Some([0.0, 0.0, 1.0]),
      factor: 1.0,
      order: ExplodeOrder::Position,
    }
  }
}

/// A component and every component below it.
pub fn subtree(d: &Design, root: ComponentId) -> Vec<ComponentId> {
  let mut out = vec![root];
  let mut k = 0;
  while k < out.len() && out.len() < 4096 {
    let p = out[k];
    out.extend(d.components.iter().filter(|c| c.parent == Some(p) && !out.contains(&c.id)).map(|c| c.id).collect::<Vec<_>>());
    k += 1;
  }
  out
}

/// The bodies of a component and of every component below it.
pub fn subtree_bodies(d: &Design, root: ComponentId) -> Vec<BodyId> {
  subtree(d, root).into_iter().filter_map(|c| d.component(c)).flat_map(|c| c.bodies.iter().copied()).collect()
}

/// Of the world axes, the one the parts' centres spread along most for the assembly's size that
/// way: the stacking direction of a layered part (Z for a sign lying flat).
pub fn auto_direction(centres: &[DVec3], lo: DVec3, hi: DVec3) -> DVec3 {
  let mut best = (DVec3::Z, -1.0);
  for axis in [DVec3::Z, DVec3::X, DVec3::Y] {
    let ext = (hi - lo).dot(axis);
    let (a, b) = centres.iter().fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c.dot(axis)), b.max(c.dot(axis))));
    let score = if ext > 1e-9 && b >= a { (b - a) / ext } else { 0.0 };
    if score > best.1 + 1e-9 {
      best = (axis, score);
    }
  }
  best.0
}

impl ExplodedView {
  /// The moving units: the listed components (else the top-level ones) with their bodies. A
  /// listed component below another listed one moves with it only.
  pub fn units(&self, d: &Design) -> Vec<(ComponentId, Vec<BodyId>)> {
    let roots: Vec<ComponentId> = if self.components.is_empty() {
      d.components.iter().filter(|c| c.parent.is_none()).map(|c| c.id).collect()
    } else {
      self.components.iter().copied().filter(|c| d.component(*c).is_some()).collect()
    };
    roots
      .iter()
      .filter(|c| !roots.iter().any(|o| o != *c && subtree(d, *o).contains(c)))
      .map(|c| (*c, subtree_bodies(d, *c)))
      .filter(|(_, b)| !b.is_empty())
      .collect()
  }

  /// Each moving body's offset, given the bodies' bounding boxes.
  pub fn offsets(&self, d: &Design, bounds: impl Fn(BodyId) -> Option<(DVec3, DVec3)>) -> HashMap<BodyId, DVec3> {
    let mut units: Vec<(Vec<BodyId>, DVec3, DVec3)> = Vec::new();
    for (_, bodies) in self.units(d) {
      let mut acc: Option<(DVec3, DVec3)> = None;
      for b in &bodies {
        if let Some((lo, hi)) = bounds(*b) {
          let (a, c) = acc.get_or_insert((lo, hi));
          *a = a.min(lo);
          *c = c.max(hi);
        }
      }
      if let Some((lo, hi)) = acc {
        units.push((bodies, lo, hi));
      }
    }
    let mut out = HashMap::new();
    if units.len() < 2 {
      return out;
    }
    let lo = units.iter().fold(DVec3::splat(f64::MAX), |a, u| a.min(u.1));
    let hi = units.iter().fold(DVec3::splat(f64::MIN), |a, u| a.max(u.2));
    let centre = |u: &(Vec<BodyId>, DVec3, DVec3)| (u.1 + u.2) / 2.0;
    let dir = match self.direction.map(DVec3::from).and_then(|v| v.try_normalize()) {
      Some(v) => v,
      None => auto_direction(&units.iter().map(centre).collect::<Vec<_>>(), lo, hi),
    };
    // the assembly's extent along the direction
    let corners = (0..8).map(|i| {
      DVec3::new(
        if i & 1 == 0 { lo.x } else { hi.x },
        if i & 2 == 0 { lo.y } else { hi.y },
        if i & 4 == 0 { lo.z } else { hi.z },
      )
    });
    let (a, b) = corners.fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c.dot(dir)), b.max(c.dot(dir))));
    let range = (b - a).max(0.0);
    if self.order == ExplodeOrder::Position {
      // stable: equal centres keep the browser's order
      units.sort_by(|x, y| centre(x).dot(dir).total_cmp(&centre(y).dot(dir)));
    }
    let n = units.len() as f64;
    let factor = if self.factor.is_finite() { self.factor } else { 0.0 };
    for (i, (bodies, _, _)) in units.iter().enumerate() {
      let off = dir * (i as f64 * range * factor / n);
      for body in bodies {
        out.insert(*body, off);
      }
    }
    out
  }
}
