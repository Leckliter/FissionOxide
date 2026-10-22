//! `sketch.scale_to`: Scale to Size and Fit in Box. Sketch curves scaled uniformly about an anchor
//! so their bounding box gets a given width or height, or fits in a box (optionally turned to the
//! angle that lets it grow most). The box is the curves' true extent (arc and ellipse bulges,
//! spline and text outlines), not their end points. The change itself is a `sketch.transform`.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use fox_sketch::{ConstraintId, ConstraintKind, CurveId, PointId, Seg, Sketch, TextId, Xform2};
use serde_json::{Value, json};

use super::{expr_of, ids, opt_str, params_scope, sketch_id};
use crate::{CmdError, CmdResult, Session};

pub(super) fn scale_to(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let scope = params_scope(s);
  let len = |v: &Value| -> Result<f64, CmdError> { Ok(fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, &scope)?) };
  let opt_len = |k: &str| -> Result<Option<f64>, CmdError> { p.get(k).filter(|v| !v.is_null()).map(len).transpose() };
  let mode = match opt_str(p, "mode") {
    Some(m) => m,
    None if p.get("box").is_some() => "box",
    None if p.get("width").is_some() => "width",
    None if p.get("height").is_some() => "height",
    None => return Err(CmdError::new("missing 'mode' (width, height or box)")),
  };
  let margin = opt_len("margin")?.unwrap_or(0.0);
  if !margin.is_finite() || margin < 0.0 {
    return Err(CmdError::new("the margin can't be negative"));
  }
  let target = |v: f64, what: &str| -> Result<f64, CmdError> {
    let t = v - 2.0 * margin;
    if t.is_finite() && t > 1e-9 {
      Ok(t)
    } else {
      Err(CmdError::new(format!("the {what} less the margins must be positive")))
    }
  };
  let goal = match mode {
    "width" => Goal::Width(target(opt_len("width")?.ok_or(CmdError::new("missing 'width'"))?, "width")?),
    "height" => Goal::Height(target(opt_len("height")?.ok_or(CmdError::new("missing 'height'"))?, "height")?),
    "box" => {
      let b = p.get("box").and_then(Value::as_array).filter(|a| a.len() == 2);
      let b = b.ok_or(CmdError::new("'box' must be [width, height]"))?;
      Goal::Box(target(len(&b[0])?, "box width")?, target(len(&b[1])?, "box height")?)
    }
    m => return Err(CmdError::new(format!("'mode' is width, height or box, not \"{m}\""))),
  };
  let rotate = p.get("rotate").and_then(Value::as_bool).unwrap_or(false);

  let sketch = s.model.sketches.get(&sk).map(|r| &r.sketch).map_or_else(|| s.design.sketch(sk), Ok)?;
  let mut curves: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  if curves.is_empty() {
    // projected and included geometry is fixed: it doesn't move, so it doesn't count (a circle
    // on the origin, whose only point is the fixed origin, still grows, and so do a text's glyphs)
    let projected = |c: CurveId| {
      if sketch.text_of(c).is_some() {
        return false;
      }
      let pts = sketch.curve_points(c);
      pts.iter().all(|q| sketch.point(*q).is_none_or(|q| q.fixed)) && pts.iter().any(|q| *q != fox_sketch::ORIGIN)
    };
    curves = sketch.curves.iter().map(|c| c.id).filter(|c| !projected(*c)).collect();
  }
  for c in &curves {
    sketch.curve(*c).ok_or_else(|| CmdError::new(format!("no curve {} in the sketch", c.0)))?;
  }
  let segs = measured_segs(sketch, &curves);
  if segs.is_empty() {
    return Err(CmdError::new("nothing to scale: the sketch has no curves"));
  }
  let (w0, h0) = size(&segs, 0.0);

  // the factor at a rotation, and the best rotation for a box
  let factor = |angle: f64| -> Option<f64> {
    let (w, h) = size(&segs, angle);
    goal.factor(w, h)
  };
  let mut angle = 0.0;
  let mut best = factor(0.0).ok_or_else(|| {
    CmdError::new(match goal {
      Goal::Width(_) => "the curves have no width to scale",
      Goal::Height(_) => "the curves have no height to scale",
      Goal::Box(..) => "the curves have no size to scale",
    })
  })?;
  if rotate && matches!(goal, Goal::Box(..)) {
    // whole degrees from 0 to 90, then hundredths around the best; a turn only when it helps
    let better = |a: f64, angle: &mut f64, best: &mut f64| {
      if let Some(k) = factor(a)
        && k > *best * (1.0 + 1e-9)
      {
        (*angle, *best) = (a, k);
      }
    };
    for d in 1..=90 {
      better(f64::from(d).to_radians(), &mut angle, &mut best);
    }
    let coarse = angle;
    for i in -100..=100 {
      let a = coarse + f64::from(i) * 0.01f64.to_radians();
      if (0.0..=FRAC_PI_2 + 1e-12).contains(&a) {
        better(a, &mut angle, &mut best);
      }
    }
  }

  let anchor = match opt_str(p, "anchor").unwrap_or("centroid") {
    "centroid" => centroid(&segs),
    "origin" => [0.0, 0.0],
    "corner" => {
      let (lo, _) = bounds(&segs, 0.0);
      lo
    }
    a => return Err(CmdError::new(format!("'anchor' is centroid, origin or corner, not \"{a}\""))),
  };
  // the corner anchor keeps the box's lower left corner where it was, rotated or not
  let mut translate = [0.0, 0.0];
  if opt_str(p, "anchor") == Some("corner") && angle != 0.0 {
    let (lo, _) = bounds(&segs, angle);
    let ra = rotated(anchor, angle);
    translate = [anchor[0] - (anchor[0] + best * (lo[0] - ra[0])), anchor[1] - (anchor[1] + best * (lo[1] - ra[1]))];
  }

  let t = Xform2 { scale: best, angle, center: anchor, translate };
  // a text's glyphs are rebuilt (with new ids) when it scales: follow the texts, not the glyphs
  let texts: Vec<TextId> = {
    let mut v: Vec<TextId> = curves.iter().filter_map(|c| sketch.text_of(*c)).collect();
    v.dedup();
    v
  };
  let plain: Vec<CurveId> = curves.iter().copied().filter(|c| sketch.text_of(*c).is_none()).collect();
  let now = |sk: &Sketch| -> Vec<CurveId> {
    let glyphs = sk.texts.iter().filter(|x| texts.contains(&x.id)).flat_map(|x| x.curves.iter().copied());
    plain.iter().copied().chain(glyphs).collect()
  };
  let (report, unpinned, total, (w1, h1)) = s.edit(|d| {
    let sketch = d.sketch_mut(sk)?;
    // a moved point held to a fixed one (a corner on the origin) would pull the shape out of true
    let moved: Vec<PointId> = curves.iter().flat_map(|c| sketch.curve_points(*c)).collect();
    let free = |sk: &Sketch, q: PointId| moved.contains(&q) && sk.point(q).is_some_and(|q| !q.fixed);
    let pinned = |sk: &Sketch, q: PointId| {
      sk.point(q).filter(|q| q.fixed).is_some_and(|q| {
        let to = t.apply([q.x, q.y]);
        (to[0] - q.x).hypot(to[1] - q.y) > 1e-9
      })
    };
    let unpinned: Vec<ConstraintId> = sketch
      .constraints
      .iter()
      .filter(|c| match c.kind {
        ConstraintKind::Coincident { a, b } => {
          (free(sketch, a) && pinned(sketch, b)) || (free(sketch, b) && pinned(sketch, a))
        }
        _ => false,
      })
      .map(|c| c.id)
      .collect();
    for c in &unpinned {
      sketch.remove_constraint(*c)?;
    }
    let mut report = sketch.transform(&curves, &t)?;
    let mut total = best;
    // regenerated text may come out a hair off a plain scale: measure again and close the gap
    for _ in 0..3 {
      let cur = now(sketch);
      let (w, h) = size(&measured_segs(sketch, &cur), 0.0);
      let Some(k) = goal.factor(w, h).filter(|k| (k - 1.0).abs() > 1e-9) else { break };
      let more = sketch.transform(&cur, &Xform2 { scale: k, center: anchor, ..Xform2::default() })?;
      report.removed.extend(more.removed);
      report.unlinked.extend(more.unlinked);
      total *= k;
    }
    if !sketch.solve().ok() {
      return Err(CmdError::new("the scaled geometry conflicts with its constraints"));
    }
    Ok((report, unpinned, total, size(&measured_segs(sketch, &now(sketch)), 0.0)))
  })?;
  let mut removed: Vec<u32> = unpinned.iter().chain(&report.removed).map(|c| c.0).collect();
  removed.dedup();
  let out = json!({
    "removed_constraints": removed,
    "unlinked_dimensions": report.unlinked.iter().map(|c| c.0).collect::<Vec<_>>(),
    "scale": total,
    "angle": angle.to_degrees(),
    "anchor": anchor,
    "before": [w0, h0],
    "size": [w1, h1],
  });
  Ok(out)
}

#[derive(Clone, Copy)]
enum Goal {
  Width(f64),
  Height(f64),
  Box(f64, f64),
}

impl Goal {
  /// The scale that takes a w x h box to the goal, if it has the size to scale.
  fn factor(self, w: f64, h: f64) -> Option<f64> {
    let k = match self {
      Goal::Width(t) => (w > 1e-9).then(|| t / w)?,
      Goal::Height(t) => (h > 1e-9).then(|| t / h)?,
      Goal::Box(bw, bh) => match (w > 1e-9, h > 1e-9) {
        (true, true) => (bw / w).min(bh / h),
        (true, false) => bw / w,
        (false, true) => bh / h,
        (false, false) => return None,
      },
    };
    k.is_finite().then_some(k)
  }
}

/// The segments a size is measured on: the normal curves of `curves`, or all of them when they
/// are all construction geometry.
fn measured_segs(sk: &Sketch, curves: &[CurveId]) -> Vec<Seg> {
  let normal: Vec<CurveId> = curves.iter().copied().filter(|c| sk.curve(*c).is_some_and(|c| !c.reference())).collect();
  let use_ = if normal.is_empty() { curves } else { &normal };
  use_.iter().flat_map(|c| sk.curve_segs(*c)).collect()
}

fn rotated(p: [f64; 2], a: f64) -> [f64; 2] {
  let (s, c) = a.sin_cos();
  [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// Width and height of the segments' bounding box once turned by `angle` (radians).
fn size(segs: &[Seg], angle: f64) -> (f64, f64) {
  let (lo, hi) = bounds(segs, angle);
  (hi[0] - lo[0], hi[1] - lo[1])
}

/// Exact axis-aligned bounds of the segments turned by `angle` about the origin.
pub(crate) fn bounds(segs: &[Seg], angle: f64) -> ([f64; 2], [f64; 2]) {
  let (s, c) = angle.sin_cos();
  // rotated x is p·(cos, −sin), rotated y is p·(sin, cos)
  let ux = [c, -s];
  let uy = [s, c];
  let mut lo = [f64::INFINITY; 2];
  let mut hi = [f64::NEG_INFINITY; 2];
  for seg in segs {
    for (i, u) in [ux, uy].into_iter().enumerate() {
      let (a, b) = extent(seg, u);
      lo[i] = lo[i].min(a);
      hi[i] = hi[i].max(b);
    }
  }
  (lo, hi)
}

fn dot(p: [f64; 2], u: [f64; 2]) -> f64 {
  p[0] * u[0] + p[1] * u[1]
}

/// Whether angle `a` lies on the sweep from `start` by `sweep` (either way round).
fn on_sweep(a: f64, start: f64, sweep: f64) -> bool {
  let lo = if sweep >= 0.0 { start } else { start + sweep };
  (a - lo).rem_euclid(TAU) <= sweep.abs() + 1e-12
}

/// The range of `p·u` over a segment.
fn extent(seg: &Seg, u: [f64; 2]) -> (f64, f64) {
  let mut lo = f64::INFINITY;
  let mut hi = f64::NEG_INFINITY;
  let mut add = |v: f64| {
    lo = lo.min(v);
    hi = hi.max(v);
  };
  match *seg {
    Seg::Line { a, b } => {
      add(dot(a, u));
      add(dot(b, u));
    }
    Seg::Circle { center, radius, .. } => {
      add(dot(center, u) - radius);
      add(dot(center, u) + radius);
    }
    // c·u + r cos(t − φ): its extremes at φ and φ + π, where the arc reaches them
    Seg::Arc { center, radius, start_angle, sweep } => {
      add(dot(seg.start(), u));
      add(dot(seg.end(), u));
      let phi = u[1].atan2(u[0]);
      for (t, sign) in [(phi, 1.0), (phi + PI, -1.0)] {
        if on_sweep(t, start_angle, sweep) {
          add(dot(center, u) + sign * radius);
        }
      }
    }
    // c·u + A cos t + B sin t = c·u + R cos(t − ψ)
    Seg::EllipseArc { center, major, minor, start, sweep } => {
      add(dot(seg.start(), u));
      add(dot(seg.end(), u));
      let m = major[0].hypot(major[1]).max(1e-300);
      let v = [-major[1] / m * minor, major[0] / m * minor];
      let (a, b) = (dot(major, u), dot(v, u));
      let (r, psi) = (a.hypot(b), b.atan2(a));
      for (t, sign) in [(psi, 1.0), (psi + PI, -1.0)] {
        if on_sweep(t, start, sweep) {
          add(dot(center, u) + sign * r);
        }
      }
    }
    Seg::Bezier { p } => bezier_extent(&p, u, &mut add),
    Seg::Conic { p, w } => {
      for b in fox_sketch::conic_beziers(&p, w) {
        bezier_extent(&b, u, &mut add);
      }
    }
    Seg::Quintic { p } => {
      for b in fox_sketch::quintic_beziers(&p) {
        bezier_extent(&b, u, &mut add);
      }
    }
  }
  (lo, hi)
}

/// A cubic's extremes along `u`: its ends and where the derivative (a quadratic) is zero.
fn bezier_extent(p: &[[f64; 2]; 4], u: [f64; 2], add: &mut impl FnMut(f64)) {
  let d: Vec<f64> = p.iter().map(|q| dot(*q, u)).collect();
  add(d[0]);
  add(d[3]);
  let (a, b, c) = (d[1] - d[0], d[2] - d[1], d[3] - d[2]);
  // a(1−t)² + 2b(1−t)t + ct² = (a − 2b + c)t² + 2(b − a)t + a
  let (qa, qb, qc) = (a - 2.0 * b + c, 2.0 * (b - a), a);
  let mut roots = Vec::new();
  if qa.abs() < 1e-14 {
    if qb.abs() > 1e-14 {
      roots.push(-qc / qb);
    }
  } else {
    let disc = qb * qb - 4.0 * qa * qc;
    if disc >= 0.0 {
      let r = disc.sqrt();
      roots.push((-qb + r) / (2.0 * qa));
      roots.push((-qb - r) / (2.0 * qa));
    }
  }
  for t in roots.into_iter().filter(|t| (0.0..=1.0).contains(t)) {
    add(dot(fox_sketch::bezier_at(p, t), u));
  }
}

/// The mean of points spread along the segments (Auto Scale's "centroid of samples").
fn centroid(segs: &[Seg]) -> [f64; 2] {
  let mut pts = Vec::new();
  for s in segs {
    s.flatten(&mut pts);
    pts.push(s.end());
  }
  let n = pts.len().max(1) as f64;
  let sum = pts.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
  [sum[0] / n, sum[1] / n]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn arcs_and_cubics_bulge_past_their_ends() {
    // a half circle from (1, 0) to (−1, 0) through (0, 1)
    let arc = Seg::Arc { center: [0.0, 0.0], radius: 1.0, start_angle: 0.0, sweep: PI };
    let (lo, hi) = bounds(&[arc], 0.0);
    assert!((lo[0] + 1.0).abs() < 1e-12 && (hi[0] - 1.0).abs() < 1e-12);
    assert!(lo[1].abs() < 1e-12 && (hi[1] - 1.0).abs() < 1e-12);
    // turned 90°, it spans x −1..0
    let (lo, hi) = bounds(&[arc], FRAC_PI_2);
    assert!((lo[0] + 1.0).abs() < 1e-9 && hi[0].abs() < 1e-9, "{lo:?} {hi:?}");
    // a cubic with both handles up at 1 peaks at 0.75
    let b = Seg::Bezier { p: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]] };
    let (_, hi) = bounds(&[b], 0.0);
    assert!((hi[1] - 0.75).abs() < 1e-12);
    let e = Seg::EllipseArc { center: [0.0, 0.0], major: [2.0, 0.0], minor: 1.0, start: 0.0, sweep: TAU };
    let (w, h) = size(&[e], FRAC_PI_2);
    assert!((w - 2.0).abs() < 1e-9 && (h - 4.0).abs() < 1e-9);
  }
}
