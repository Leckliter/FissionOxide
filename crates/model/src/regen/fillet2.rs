//! Fillets OpenCASCADE's rolling-ball fillet can't make: asymmetric fillets (an elliptical round
//! with a different offset on each face) and full round fillets (a centre face replaced by one
//! round tangent to it and the faces on both sides). Both are built from their cross-section,
//! extruded along the edge and cut from (or added to) the body, so they take straight edges
//! between flat faces.

use std::f64::consts::TAU;

use fox_kernel::glam::{DMat3, DVec2, DVec3};
use fox_kernel::{BoolOp, CurveKind, EdgeInfo, FaceInfo, Inside, KernelError, Plane, Shape, SurfaceKind};
use fox_sketch::{Loop, Profile, Seg};

use super::{Ctx, FResult, ke};
use crate::{FeatureId, FullRoundSet};

/// The order [`crate::EdgeRef`] gives an edge's faces: by outward normal, x then y then z.
fn sorts_first(a: DVec3, b: DVec3) -> bool {
  (a.x, a.y, a.z) <= (b.x, b.y, b.z)
}

fn input(msg: &str) -> KernelError {
  KernelError::Input(msg.to_string())
}

/// A prism of `segs` (one closed loop on `plane`) running `along` from the plane.
fn prism(plane: &Plane, segs: Vec<Seg>, along: DVec3) -> fox_kernel::Result<Shape> {
  Shape::face(plane, &Profile { outer: Loop { segs }, holes: vec![] })?.extrude(along)
}

/// Rounds straight edges between flat faces with a section tangent to both faces, `a` from the
/// edge on its first face (see [`sorts_first`]) and `b` on its second. The section is the
/// affine image of a circular round (a quarter ellipse where the faces are square), so equal
/// offsets give the ordinary circular fillet. Convex edges lose material, concave ones gain it.
pub(super) fn asymmetric(s: &Shape, ids: &[u32], a: f64, b: f64) -> fox_kernel::Result<Shape> {
  if !(a > 1e-9 && b > 1e-9 && a.is_finite() && b.is_finite()) {
    return Err(input("the fillet offsets must be positive"));
  }
  let (faces, edges) = (s.faces()?, s.edges()?);
  let flat = |id: u32| faces.iter().find(|f| f.id == id && f.kind == SurfaceKind::Plane);
  let unsupported = || input("an asymmetric fillet rounds straight edges between flat faces");
  let (mut cut, mut add) = (Vec::new(), Vec::new());
  for &id in ids {
    let e = edges.iter().find(|e| e.id == id).ok_or_else(|| input("no such edge"))?;
    if e.kind != CurveKind::Line {
      return Err(unsupported());
    }
    let (f1, f2) = (flat(e.faces[0]).ok_or_else(unsupported)?, flat(e.faces[1]).ok_or_else(unsupported)?);
    let (n1, n2) = if sorts_first(f1.normal, f2.normal) { (f1.normal, f2.normal) } else { (f2.normal, f1.normal) };
    let along = e.end - e.start;
    let t = along.try_normalize().ok_or_else(unsupported)?;
    if n1.cross(n2).length() < 1e-6 {
      return Err(input("the faces at the edge are tangent: there is no corner to round"));
    }
    // just outside face 1 and inside face 2's plane: in the solid only at a concave edge
    let probe = e.mid + (n1 - n2).normalize() * 1e-3 * a.min(b);
    let concave = s.classify(probe)? == Inside::In;
    let side = if concave { 1.0 } else { -1.0 };
    // across each face, away from the edge: towards the other face's outside at a concave edge
    let into = |n: DVec3, other: DVec3| {
      let u = t.cross(n).normalize();
      if u.dot(other) * side < 0.0 { -u } else { u }
    };
    let (u1, u2) = (into(n1, n2), into(n2, n1));
    let plane = Plane::from_normal(e.start, t, u1)?;
    let (c, p1, p2) = ([0.0, 0.0], plane.to_local(e.start + u1 * a), plane.to_local(e.start + u2 * b));
    let w = (u1.angle_between(u2) / 2.0).sin();
    let segs = vec![Seg::Line { a: c, b: p1 }, Seg::Conic { p: [p1, c, p2], w }, Seg::Line { a: p2, b: c }];
    let tool = prism(&plane, segs, along)?;
    if concave { add.push(tool) } else { cut.push(tool) }
  }
  let mut out = s.clone();
  for tool in &cut {
    out = out.boolean(tool, BoolOp::Subtract)?;
  }
  for tool in &add {
    out = out.boolean(tool, BoolOp::Union)?;
  }
  Ok(out)
}

/// Replaces flat face `center` with the round tangent to it and to flat faces `one` and `two`,
/// which meet it along parallel straight edges: the material beyond the chord between the
/// round's tangent points on the sides is cut away and the circular segment added back.
fn full_round(
  s: &Shape,
  edges: &[EdgeInfo],
  center: &FaceInfo,
  one: &FaceInfo,
  two: &FaceInfo,
) -> fox_kernel::Result<Shape> {
  if [center, one, two].iter().any(|f| f.kind != SurfaceKind::Plane) {
    return Err(input("a full round fillet rounds between flat faces"));
  }
  let (n0, n1, n2) = (center.normal, one.normal, two.normal);
  let t = n0.cross(n1).try_normalize().ok_or_else(|| input("side one is parallel to the centre face"))?;
  let t2 = n0.cross(n2).try_normalize().ok_or_else(|| input("side two is parallel to the centre face"))?;
  if t.cross(t2).length() > 1e-6 {
    return Err(input("the side faces must meet the centre face along parallel edges"));
  }
  // how far the round runs: the centre face's edges with the sides
  let bordering = |side: u32| {
    edges.iter().filter(move |e| e.kind == CurveKind::Line && e.faces.contains(&center.id) && e.faces.contains(&side))
  };
  let (mut lo, mut hi, mut base) = (f64::INFINITY, f64::NEG_INFINITY, None);
  for side in [one.id, two.id] {
    let mut found = false;
    for e in bordering(side) {
      found = true;
      for p in [e.start, e.end] {
        lo = lo.min(p.dot(t));
        hi = hi.max(p.dot(t));
        base.get_or_insert(p);
      }
    }
    if !found {
      return Err(input("each side face must border the centre face"));
    }
  }
  let base = base.ok_or_else(|| input("each side face must border the centre face"))?;
  if hi - lo < 1e-9 {
    return Err(input("the centre face has no length to round"));
  }
  let plane = Plane::from_normal(base - t * (base.dot(t) - lo), t, n0.cross(t))?;
  // each face as a line n·p = d in the section
  let line = |f: &FaceInfo| {
    let n = DVec2::new(f.normal.dot(plane.x), f.normal.dot(plane.y));
    (n, (f.origin - plane.origin).dot(f.normal))
  };
  let (l0, l1, l2) = (line(center), line(one), line(two));
  // the circle r inside all three
  let m = DMat3::from_cols(DVec3::new(l0.0.x, l1.0.x, l2.0.x), DVec3::new(l0.0.y, l1.0.y, l2.0.y), DVec3::ONE);
  if m.determinant().abs() < 1e-12 {
    return Err(input("no round touches the three faces"));
  }
  let sol = m.inverse() * DVec3::new(l0.1, l1.1, l2.1);
  let (x, r) = (DVec2::new(sol.x, sol.y), sol.z);
  if !(r > 1e-9 && r.is_finite()) {
    return Err(input("no round touches the three faces inside the body"));
  }
  let corner = |a: (DVec2, f64), b: (DVec2, f64)| {
    let det = a.0.perp_dot(b.0);
    DVec2::new(a.1 * b.0.y - b.1 * a.0.y, a.0.x * b.1 - b.0.x * a.1) / det
  };
  let (e1, e2) = (corner(l0, l1), corner(l0, l2));
  let touch = |l: (DVec2, f64)| x + l.0 * r;
  let (t0, t1, t2) = (touch(l0), touch(l1), touch(l2));
  if !(e1.is_finite() && e2.is_finite()) {
    return Err(input("no round touches the three faces"));
  }
  let pt = |v: DVec2| [v.x, v.y];
  let along = t * (hi - lo);
  let cap = vec![
    Seg::Line { a: pt(t1), b: pt(e1) },
    Seg::Line { a: pt(e1), b: pt(e2) },
    Seg::Line { a: pt(e2), b: pt(t2) },
    Seg::Line { a: pt(t2), b: pt(t1) },
  ];
  let angle = |p: DVec2| (p - x).y.atan2((p - x).x);
  let (a1, a0, a2) = (angle(t1), angle(t0), angle(t2));
  let ccw = (a2 - a1).rem_euclid(TAU);
  let sweep = if (a0 - a1).rem_euclid(TAU) < ccw { ccw } else { ccw - TAU };
  let round = vec![Seg::Line { a: pt(t2), b: pt(t1) }, Seg::Arc { center: pt(x), radius: r, start_angle: a1, sweep }];
  s.boolean(&prism(&plane, cap, along)?, BoolOp::Subtract)?
    .boolean(&prism(&plane, round, along)?, BoolOp::Union)?
    .unified()
}

impl Ctx<'_> {
  /// The full round fillet feature: each set on its body in turn.
  pub(super) fn full_round(&mut self, id: FeatureId, sets: &mut [FullRoundSet]) -> FResult {
    if sets.is_empty() {
      return Err("select a centre face and a face on each side of it".into());
    }
    for set in sets.iter_mut() {
      let (bi, center) = self.face_info(&mut set.center)?;
      let [one] = set.side_one.as_mut_slice() else { return Err("select one face on side one".into()) };
      let (b1, one) = self.face_info(one)?;
      let [two] = set.side_two.as_mut_slice() else { return Err("select one face on side two".into()) };
      let (b2, two) = self.face_info(two)?;
      if b1 != bi || b2 != bi {
        return Err("the centre and side faces must be on one body".into());
      }
      let body = &self.state.bodies[bi];
      let shape = full_round(&body.shape, body.edges(), &center, &one, &two).map_err(ke)?;
      self.state.bodies[bi] = self.state.bodies[bi].with_shape(shape, id);
    }
    Ok(None)
  }
}
