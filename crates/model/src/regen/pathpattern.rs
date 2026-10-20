//! Regeneration of Pattern on Path: copies of bodies, or the tools of features re-applied, at
//! points along a sketch path.

use fox_kernel::Shape;
use fox_kernel::glam::{DMat4, DVec3};

use super::{Ctx, FResult, ke};
use crate::pathpattern::{PathLayout, PathOrientation, path_at, path_length};
use crate::{BodyId, FeatureId, FeatureKind, Operation};

/// A tool solid a feature applied, with its operation and the bodies it was limited to.
#[derive(Clone)]
pub(super) struct Tool {
  pub(super) shape: Shape,
  pub(super) op: Operation,
  pub(super) targets: Vec<BodyId>,
}

impl Ctx<'_> {
  /// Remembers the tool `feature` applies, so a feature pattern can apply it again elsewhere.
  pub(super) fn record_tool(&mut self, feature: FeatureId, tool: &Shape, op: Operation) {
    let targets = self.targets.to_vec();
    self.state.tools.entry(feature).or_default().push(Tool { shape: tool.clone(), op, targets });
  }

  pub(super) fn apply_path_pattern(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    let FeatureKind::PathPattern {
      bodies,
      features,
      path_sketch,
      path_curves,
      count,
      distance,
      extent,
      start,
      flip,
      symmetric,
      orientation,
      join,
    } = kind
    else {
      return Err("not a path pattern".into());
    };
    let ps = self.state.sketches.get(path_sketch).ok_or("the path sketch failed")?;
    let segs = crate::sheet::open_path(&ps.sketch, path_curves)?;
    let plane = ps.plane;
    let start = if start.trim().is_empty() {
      0.0
    } else {
      fox_expr::eval(start, fox_expr::Kind::Unitless, self.params).map_err(|err| format!("'{start}': {err}"))?
    };
    if !(0.0..=1.0).contains(&start) {
      return Err("the start point is a fraction of the path, from 0 to 1".into());
    }
    let layout = PathLayout {
      count: self.count(count)?,
      distance: self.len(distance)?,
      extent: *extent,
      start,
      flip: *flip,
      symmetric: *symmetric,
    };
    let length = path_length(&segs);
    if length < 1e-9 {
      return Err("the path has no length".into());
    }
    let frame = |s: f64| -> Result<(DVec3, DVec3), String> {
      let (p, t) = path_at(&segs, s).ok_or("the path has no direction")?;
      Ok((plane.to_world(p), plane.x * t[0] + plane.y * t[1]))
    };
    let n = plane.normal();
    let (p0, t0) = frame(layout.origin(length))?;
    let mut moves = Vec::new();
    for s in layout.copies(length) {
      let (p, t) = frame(s)?;
      let turn = match orientation {
        PathOrientation::Identical => 0.0,
        PathOrientation::AlongPath => t0.cross(t).dot(n).atan2(t0.dot(t)),
      };
      moves.push(DMat4::from_translation(p) * DMat4::from_axis_angle(n, turn) * DMat4::from_translation(-p0));
    }
    if features.is_empty() {
      for bi in self.select(bodies)? {
        let body = self.state.bodies[bi].clone();
        let copies: Result<Vec<Shape>, String> = moves.iter().map(|m| body.shape.transformed(*m).map_err(ke)).collect();
        self.add_copies(id, bi, copies?, *join)?;
      }
      return Ok(None);
    }
    let mut tools = Vec::new();
    for f in features.iter() {
      let t = self.state.tools.get(f).ok_or_else(|| format!("feature {} made no solid to pattern", f.0))?;
      tools.extend(t.iter().cloned());
    }
    // a copy that misses every body is skipped, as Fusion leaves such an instance out
    let (mut placed, mut missed) = (0, None);
    for m in &moves {
      for t in &tools {
        let shape = t.shape.transformed(*m).map_err(ke)?;
        let mut ctx = Ctx {
          params: self.params,
          rules: self.rules,
          components: self.components,
          targets: &t.targets,
          state: &mut *self.state,
          mates: self.mates,
        };
        match ctx.operate(id, shape, t.op) {
          Ok(()) => placed += 1,
          Err(e) => missed = Some(e),
        }
      }
    }
    match missed {
      Some(e) if placed == 0 && !moves.is_empty() => Err(e),
      Some(e) => Ok(Some(format!("some instances could not be placed: {e}"))),
      None => Ok(None),
    }
  }
}
