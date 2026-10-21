//! Dragging components in the viewport, as in Fusion: a jointed component moves within its
//! joint's freedoms so the grabbed point follows the cursor as closely as they allow (within the
//! joint's limits); an unjointed one slides in the view plane (a Move feature at the end of the
//! timeline). With contact sets on, the drag stops where their bodies first touch.

use fox_kernel::glam::DVec3;
use fox_model::assembly::{self, JointKind, Motion, MotionAxis};
use fox_model::{BodySel, ComponentId, Design, FeatureId, FeatureKind};
use serde_json::{Map, Value, json};

use crate::commands::{req_u32, req_vec3};
use crate::{CmdError, CmdResult, Session, mates};

/// `component.drag {component, from, to, view?}`: `from` is the grabbed point (on the component,
/// where it was when the drag started), `to` a point under the cursor and `view` the view
/// direction there (the cursor is a ray); without `view` the grabbed point goes to `to`.
pub(crate) fn drag(s: &mut Session, p: &Value) -> CmdResult {
  let c = ComponentId(req_u32(p, "component")?);
  let comp = s.design.component(c).ok_or(CmdError::new(format!("no component {}", c.0)))?;
  let name = comp.name.clone();
  let from = DVec3::from(req_vec3(p, "from")?);
  let to = DVec3::from(req_vec3(p, "to")?);
  let view = match p.get("view") {
    Some(Value::Null) | None => None,
    Some(_) => Some(DVec3::from(req_vec3(p, "view")?).try_normalize().ok_or(CmdError::new("'view' is zero"))?),
  };
  match moving_joint(&s.design, c) {
    Some(j) => drag_joint(s, j, from, to, view),
    None if grounded(&s.design, c) => Err(CmdError::new(format!("{name} is grounded"))),
    None => drag_free(s, c, from, to, view),
  }
}

/// The joint that places a component: the last one moving it, else its parent's (sub-assemblies
/// move with their parent).
pub(crate) fn moving_joint(d: &Design, c: ComponentId) -> Option<FeatureId> {
  let active = &d.features[..d.active_len()];
  let mut at = Some(c);
  let mut seen = Vec::new();
  while let Some(k) = at.filter(|k| !seen.contains(k)) {
    seen.push(k);
    let j = active.iter().rev().filter(|f| !f.suppressed).find_map(|f| match &f.kind {
      FeatureKind::Joint { moving, .. } if *moving == k => Some(f.id),
      _ => None,
    });
    if j.is_some() {
      return j;
    }
    at = d.component(k).and_then(|x| x.parent);
  }
  None
}

fn grounded(d: &Design, c: ComponentId) -> bool {
  let mut at = Some(c);
  let mut n = 0;
  while let Some(k) = at.and_then(|k| d.component(k)) {
    if k.grounded {
      return true;
    }
    at = k.parent;
    n += 1;
    if n > 64 {
      break;
    }
  }
  false
}

/// How far a point is from the cursor: from its ray, or from the point itself.
fn miss(x: DVec3, to: DVec3, view: Option<DVec3>) -> DVec3 {
  let w = x - to;
  match view {
    Some(v) => w - v * w.dot(v),
    None => w,
  }
}

/// The joint motions a drag may change, and what sets the others (motion links, cams).
fn free_axes(d: &Design, j: FeatureId, kind: JointKind) -> (Vec<MotionAxis>, Option<String>) {
  let mut set_by = None;
  let mut axes = Vec::new();
  for a in MotionAxis::ALL.into_iter().filter(|a| a.allowed(kind)) {
    let by = d.features[..d.active_len()].iter().filter(|f| !f.suppressed).find(|f| match &f.kind {
      FeatureKind::MotionLink { follower, follower_motion, .. } => *follower == j && *follower_motion == a,
      FeatureKind::CamFollower { follower, motion, .. } => *follower == j && motion.unwrap_or(MotionAxis::Slide) == a,
      _ => false,
    });
    match by {
      Some(f) => set_by = Some(f.name.clone()),
      None => axes.push(a),
    }
  }
  (axes, set_by)
}

fn drag_joint(s: &mut Session, j: FeatureId, from: DVec3, to: DVec3, view: Option<DVec3>) -> CmdResult {
  let Some(f) = s.design.feature(j) else { return Err(CmdError::new("no such joint")) };
  let jname = f.name.clone();
  let FeatureKind::Joint { kind, rotation, slide, slide2, pitch, limits, lead, as_built, .. } = &f.kind else {
    return Err(CmdError::new("not a joint"));
  };
  let (kind, as_built, limits) = (*kind, *as_built, limits.clone());
  let (axes, set_by) = free_axes(&s.design, j, kind);
  if axes.is_empty() {
    return Err(CmdError::new(match set_by {
      Some(by) => format!("{jname}'s motion is set by {by}"),
      None => format!("{jname} is rigid"),
    }));
  }
  let scope = &s.model.params;
  let len = |e: &str| fox_expr::eval(e, fox_expr::Kind::Length, scope).unwrap_or(0.0);
  let ang = |e: &str| fox_expr::eval(e, fox_expr::Kind::Angle, scope).unwrap_or(0.0);
  let lead = if kind == JointKind::Screw { Some(len(lead)) } else { None };
  let screw = |mut m: Motion| {
    if let Some(l) = lead {
      m.slide = assembly::screw_slide(m.rotation, l);
    }
    m
  };
  let now = screw(Motion { rotation: ang(rotation), slide: len(slide), slide2: len(slide2), pitch: ang(pitch) });
  // the joint's frame where it is now: its motions act in it
  let frame = crate::scene::joint_marks(s)
    .into_iter()
    .find(|m| m.joint && m.feature == j)
    .map(|m| m.frame)
    .ok_or(CmdError::new(format!("{jname} isn't placed")))?;
  let frame = if as_built {
    match &s.design.feature(j).map(|f| &f.kind) {
      Some(FeatureKind::Joint { fixed_origin, .. }) => crate::scene::geometry_frame(s, fixed_origin).unwrap_or(frame),
      _ => frame,
    }
  } else {
    frame
  };
  // back from the motion now to none, in the joint's frame
  let undo = now.matrix(kind).inverse() * frame.inverse();
  let motion = |q: &[f64]| {
    let mut m = now;
    for (a, v) in axes.iter().zip(q) {
      a.set(&mut m, *v);
    }
    screw(m)
  };
  let place = |q: &[f64]| (frame * motion(q).matrix(kind) * undo).transform_point3(from);
  let start: Vec<f64> = axes.iter().map(|a| a.get(&now)).collect();
  let mut q = solve(&start, &|q| miss(place(q), to, view));
  // within the joint's limits
  for (a, v) in axes.iter().zip(q.iter_mut()) {
    let lim = match a {
      MotionAxis::Rotation => limits.rotation.as_ref().map(|(lo, hi)| (ang(lo), ang(hi))),
      MotionAxis::Slide => limits.slide.as_ref().map(|(lo, hi)| (len(lo), len(hi))),
      _ => None,
    };
    if let Some((lo, hi)) = lim {
      *v = v.clamp(lo.min(hi), hi.max(lo));
    }
  }
  let mut set = Map::new();
  for (a, v) in axes.iter().zip(&q) {
    let e = if a.is_angle() { format!("{} deg", round(v.to_degrees())) } else { format!("{} mm", round(*v)) };
    set.insert(a.name().into(), json!(e));
  }
  let mut cmd = json!({"joint": j.0});
  for (k, v) in &set {
    cmd[k] = v.clone();
  }
  let mut out = s.execute("joint.drive", &cmd)?;
  out["joint"] = json!(j.0);
  if out.get("stopped_at").is_none() {
    out["set"] = json!(set);
  }
  Ok(out)
}

fn round(v: f64) -> f64 {
  (v * 1e6).round() / 1e6
}

/// Least squares (damped Gauss–Newton) for the motions that bring `r` nearest zero.
fn solve(start: &[f64], r: &dyn Fn(&[f64]) -> DVec3) -> Vec<f64> {
  let n = start.len();
  let mut q = start.to_vec();
  let mut lambda = 1e-6;
  let cost = |q: &[f64]| r(q).length_squared();
  let mut c = cost(&q);
  for _ in 0..100 {
    let r0 = r(&q);
    // the Jacobian, by differences
    let mut jac = vec![DVec3::ZERO; n];
    for (i, col) in jac.iter_mut().enumerate() {
      let mut qh = q.clone();
      let h = 1e-6 * (1.0 + q[i].abs());
      qh[i] += h;
      *col = (r(&qh) - r0) / h;
    }
    let mut stepped = false;
    for _ in 0..12 {
      // (JᵀJ + λ diag) δ = −Jᵀr
      let mut a = vec![vec![0.0; n]; n];
      let mut b = vec![0.0; n];
      for i in 0..n {
        for k in 0..n {
          a[i][k] = jac[i].dot(jac[k]);
        }
        a[i][i] += lambda * (1.0 + a[i][i]);
        b[i] = -jac[i].dot(r0);
      }
      let Some(d) = gauss(a, b) else { break };
      // the best of a few steps along it: a full step can overshoot back and forth
      let (cn, next) = [1.0, 0.75, 0.5, 0.25, 0.1]
        .into_iter()
        .map(|k| {
          let next: Vec<f64> = q.iter().zip(&d).map(|(x, d)| x + d * k).collect();
          (cost(&next), next)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((f64::MAX, Vec::new()));
      if cn < c {
        let small = q.iter().zip(&next).all(|(a, b)| (a - b).abs() < 1e-12);
        q = next;
        c = cn;
        lambda = (lambda * 0.3).max(1e-12);
        stepped = !small;
        break;
      }
      lambda *= 10.0;
    }
    if !stepped {
      break;
    }
  }
  q
}

/// Solves a small linear system (partial pivoting); `None` when it is singular.
fn gauss(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
  let n = b.len();
  for i in 0..n {
    let p = (i..n).max_by(|x, y| a[*x][i].abs().total_cmp(&a[*y][i].abs()))?;
    if a[p][i].abs() < 1e-300 {
      return None;
    }
    a.swap(i, p);
    b.swap(i, p);
    for r in i + 1..n {
      let f = a[r][i] / a[i][i];
      for k in i..n {
        a[r][k] -= f * a[i][k];
      }
      b[r] -= f * b[i];
    }
  }
  let mut x = vec![0.0; n];
  for i in (0..n).rev() {
    let s: f64 = (i + 1..n).map(|k| a[i][k] * x[k]).sum();
    x[i] = (b[i] - s) / a[i][i];
  }
  Some(x)
}

/// An unjointed component dragged in the view plane through the grabbed point: the last
/// feature if it is already a Move of just this component, else a new one.
fn drag_free(s: &mut Session, c: ComponentId, from: DVec3, to: DVec3, view: Option<DVec3>) -> CmdResult {
  let t = match view {
    Some(v) => {
      // where the cursor's ray crosses the plane through the grabbed point, square to the view
      let along = (from - to).dot(v);
      to + v * along - from
    }
    None => to - from,
  };
  let last = s.design.features.len().checked_sub(1).filter(|i| *i + 1 == s.design.active_len());
  let reuse = last.and_then(|i| {
    let f = &s.design.features[i];
    match &f.kind {
      FeatureKind::Move {
        bodies: BodySel::Component { id },
        translate,
        axis: None,
        rotate: None,
        copy: false,
        angle,
        ..
      } if *id == c && !f.suppressed && fox_expr::eval(angle, fox_expr::Kind::Angle, &s.model.params) == Ok(0.0) => {
        let l = |e: &str| fox_expr::eval(e, fox_expr::Kind::Length, &s.model.params).ok();
        Some((f.id, DVec3::new(l(&translate[0])?, l(&translate[1])?, l(&translate[2])?)))
      }
      _ => None,
    }
  });
  let (base, mut trial_design, fid) = match reuse {
    Some((fid, base)) => (base, s.design.clone(), fid),
    None => {
      let mut d = s.design.clone();
      let fid = d
        .add_feature(FeatureKind::Move {
          bodies: BodySel::Component { id: c },
          translate: ["0 mm".into(), "0 mm".into(), "0 mm".into()],
          axis: None,
          angle: "0".into(),
          rotate: None,
          pivot: None,
          copy: false,
        })
        .map_err(|e| CmdError::new(e.to_string()))?;
      (DVec3::ZERO, d, fid)
    }
  };
  let keys = ["x", "y", "z"];
  let start: Vec<(String, f64)> = keys.iter().zip(base.to_array()).map(|(k, v)| (k.to_string(), v)).collect();
  let goal: Vec<(String, f64)> = keys.iter().zip((base + t).to_array()).map(|(k, v)| (k.to_string(), v)).collect();
  let mut contact = None;
  let mut stop = mates::between(&start, &goal, 1.0);
  if mates::has_contacts(&s.design) {
    let before = mates::contacts(&s.design, &s.model.bodies);
    let mut trial = mates::Trial::over(&trial_design, fid, before);
    if let Some((k, clash)) = mates::first_contact(&mut trial, &start, &goal) {
      stop = mates::between(&start, &goal, k);
      contact = clash.first().map(|(a, b)| [a.0, b.0]);
    }
  }
  let translate: Vec<String> = keys.iter().map(|k| stop[*k].as_str().unwrap_or("0").to_string()).collect();
  if let Some(FeatureKind::Move { translate: tr, .. }) = trial_design.feature_mut(fid).map(|f| &mut f.kind) {
    tr.clone_from_slice(&translate);
  }
  s.edit(|d| {
    *d = trial_design;
    Ok(())
  })?;
  let mut out = crate::mates::feature_json(s, fid);
  out["translate"] = json!(translate);
  if let Some(bodies) = contact {
    out["contact"] = json!(true);
    out["bodies"] = json!(bodies);
  }
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn least_squares_finds_the_nearest_motion() {
    // a point on a unit circle about Z, pulled toward (0, 2, 0): it turns to 90°
    let r = |q: &[f64]| DVec3::new(q[0].cos(), q[0].sin(), 0.0) - DVec3::new(0.0, 2.0, 0.0);
    let q = solve(&[0.1], &r);
    assert!((q[0] - std::f64::consts::FRAC_PI_2).abs() < 1e-6, "{q:?}");
    // a slide and a turn together reach a point exactly
    let r = |q: &[f64]| DVec3::new(q[1].cos() * 2.0, q[1].sin() * 2.0, q[0]) - DVec3::new(0.0, -2.0, 5.0);
    let q = solve(&[0.0, 0.0], &r);
    assert!((q[0] - 5.0).abs() < 1e-6 && (q[1] + std::f64::consts::FRAC_PI_2).abs() < 1e-6, "{q:?}");
  }

  #[test]
  fn misses_are_measured_from_the_cursor_ray() {
    let m = miss(DVec3::new(1.0, 2.0, 3.0), DVec3::ZERO, Some(DVec3::Z));
    assert!((m - DVec3::new(1.0, 2.0, 0.0)).length() < 1e-12);
  }
}
