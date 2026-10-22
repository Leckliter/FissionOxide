//! Blend curves kept to the curves they join: a blend's interior control points follow from the
//! two end conditions (see `quintic::blend_points`), so the solver places them rather than
//! solving for them and the blend adds no degrees of freedom, as in Fusion.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{ConstraintKind, CurveId, CurveKind, PointId, Sketch};

/// A blend curve and the curve ends it joins.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SketchBlend {
  /// The blend (a quintic spline from an end of `a` to an end of `b`).
  pub curve: CurveId,
  pub a: CurveId,
  /// Whether the blend starts at `a`'s last point (else its first).
  pub a_end: bool,
  pub b: CurveId,
  /// Whether the blend ends at `b`'s last point (else its first).
  pub b_end: bool,
  /// Curvature continuous (G2) rather than tangent (G1).
  pub g2: bool,
}

impl Sketch {
  /// The blends whose curves are still there and still meet the blend's ends.
  fn live_blends(&self) -> Vec<SketchBlend> {
    self.blends.iter().copied().filter(|b| self.blend_points_of(b).is_some()).collect()
  }

  /// The blend's control points, and the frames of the two ends it joins.
  fn blend_points_of(&self, b: &SketchBlend) -> Option<Vec<PointId>> {
    let Some(CurveKind::QuinticSpline { points }) = self.curve(b.curve).map(|c| &c.kind) else { return None };
    let end = |c: CurveId, last: bool| self.ends(c).map(|(p, q)| if last { q } else { p });
    (points.first().copied() == end(b.a, b.a_end) && points.last().copied() == end(b.b, b.b_end))
      .then(|| points.clone())
  }

  /// Blend curves' interior control points: placed from the curves they join, never solved for,
  /// picked or drawn.
  pub fn blend_interior(&self) -> HashSet<PointId> {
    self
      .live_blends()
      .iter()
      .filter_map(|b| self.blend_points_of(b))
      .flat_map(|pts| pts[1..pts.len() - 1].to_vec())
      .collect()
  }

  /// Whether `curve` is a blend kept to the curves it joins.
  pub fn is_blend(&self, curve: CurveId) -> bool {
    self.live_blends().iter().any(|b| b.curve == curve)
  }

  /// Whether constraint `kind` only states what a blend holds by construction (its tangency or
  /// curvature to the curves it joins), which the solver then leaves out.
  pub(crate) fn held_by_blend(&self, kind: &ConstraintKind, blends: &[SketchBlend]) -> bool {
    let (ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b }) = *kind else { return false };
    blends.iter().any(|bl| (a == bl.curve && (b == bl.a || b == bl.b)) || (b == bl.curve && (a == bl.a || a == bl.b)))
  }

  /// The live blends, for the solver.
  pub(crate) fn solver_blends(&self) -> Vec<SketchBlend> {
    self.live_blends()
  }

  /// Puts every blend's interior points where its end conditions put them.
  pub(crate) fn place_blends(&mut self) {
    for b in self.live_blends() {
      let Some(pts) = self.blend_points_of(&b) else { continue };
      let (Some(ka), Some(kb)) = (self.curve(b.a).map(|c| c.kind.clone()), self.curve(b.b).map(|c| c.kind.clone()))
      else {
        continue;
      };
      let pos = |p: PointId| self.pos(p);
      let (Some(fa), Some(fb)) = (
        crate::freeform::end_frame(&ka, b.a_end, &pos, &self.behaviours),
        crate::freeform::end_frame(&kb, b.b_end, &pos, &self.behaviours),
      ) else {
        continue;
      };
      let Some(ctrl) = crate::quintic::blend_points(fa, fb, b.g2) else { continue };
      for (p, c) in pts.iter().zip(ctrl.iter()).skip(1).take(pts.len() - 2) {
        if let Some(pt) = self.point_mut(*p) {
          pt.x = c[0];
          pt.y = c[1];
        }
      }
    }
  }
}
