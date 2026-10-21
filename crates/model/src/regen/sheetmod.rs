//! Regeneration of the sheet metal MODIFY commands: Join by Bend, Sheet Metal Fillet and Sheet
//! Metal Chamfer.
//!
//! Fillets and chamfers only take edges that run through the sheet's thickness (flange
//! corners). Their new faces are side walls, square to the sheet, so the unfolder never walks
//! onto them and the rounded or bevelled corner stays in the flat pattern.

use fox_kernel::glam::DVec3;
use fox_kernel::{BoolOp, CurveKind, EdgeInfo, Plane, Shape, SurfaceKind};

use super::{Body, Ctx, FResult};
use crate::sheet::{self, SheetInfo};
use crate::{BodyId, EdgeRef, FeatureId, FeatureKind, JoinBendPosition, JoinRelief, ReliefShape};

/// A flange at the edge Join by Bend picked on it.
struct Flange {
  body: BodyId,
  info: SheetInfo,
  /// The edge's ends.
  a: DVec3,
  b: DVec3,
  /// Outward normal of the sheet face at the edge (the material is behind it).
  n: DVec3,
  /// Outward normal of the flange's end wall: the way the flange would grow.
  s: DVec3,
}

impl Flange {
  /// A point on the end of the flange, halfway through the sheet.
  fn mid(&self) -> DVec3 {
    self.a - self.n * self.info.thickness / 2.0
  }
}

impl Ctx<'_> {
  pub(super) fn apply_sheet_modify(&mut self, id: FeatureId, kind: &mut FeatureKind) -> FResult {
    match kind {
      FeatureKind::SheetFillet { edges, radius } => {
        let r = self.len(radius)?;
        if !(r > 1e-6) {
          return Err("the fillet radius must be positive".into());
        }
        self.thickness_edges(edges, "Sheet Metal Fillet")?;
        self.edge_op(id, edges, |s, ids| s.fillet(ids, r))
      }
      FeatureKind::SheetChamfer { edges, distance, distance2, flipped } => {
        let d = self.len(distance)?;
        let d2 = distance2.as_deref().filter(|e| !e.trim().is_empty()).map(|e| self.len(e)).transpose()?;
        if !(d > 1e-6) || d2.is_some_and(|d2| !(d2 > 1e-6)) {
          return Err("the chamfer distances must be positive".into());
        }
        self.thickness_edges(edges, "Sheet Metal Chamfer")?;
        let flip = *flipped;
        match d2 {
          Some(d2) => self.edge_op(id, edges, |s, ids| s.chamfer_two(ids, d, d2, false, flip)),
          None => self.edge_op(id, edges, |s, ids| s.chamfer(ids, d)),
        }
      }
      FeatureKind::JoinByBend { edge1, edge2, bend_radius, position, relief } => {
        let r = if bend_radius.trim().is_empty() { None } else { Some(self.len(bend_radius)?) };
        self.join_by_bend(id, edge1, edge2, r, *position, *relief)
      }
      _ => Err("this feature type isn't supported".into()),
    }
  }

  /// Checks that every edge is on a folded sheet metal body and runs through its thickness.
  fn thickness_edges(&self, edges: &[EdgeRef], what: &str) -> Result<(), String> {
    if edges.is_empty() {
      return Err("select edges".into());
    }
    for er in edges {
      let body = &self.state.bodies[self.body_index(er.body)?];
      let info = body.sheet.as_ref().ok_or(format!("{what} works on sheet metal bodies"))?;
      let Some(eid) = body.find_edge(er) else { continue };
      let e = body.edges().iter().find(|e| e.id == eid).ok_or("an edge could not be found")?;
      if !thickness_edge(body, e, info.thickness) {
        return Err(format!("{what}: pick edges that run through the sheet's thickness (flange corners)"));
      }
    }
    Ok(())
  }

  /// The flange an edge picked for Join by Bend is on (refreshing the reference).
  fn join_flange(&self, er: &mut EdgeRef) -> Result<(usize, Flange), String> {
    let bi = self.body_index(er.body)?;
    let body = &self.state.bodies[bi];
    let info = body.sheet.clone().ok_or("Join by Bend joins sheet metal bodies")?;
    if info.flat {
      return Err("refold the unfolded body before joining it".into());
    }
    let eid = body.find_edge(er).ok_or("an edge could not be found")?;
    let e = *body.edges().iter().find(|e| e.id == eid).ok_or("an edge could not be found")?;
    *er = body.edge_ref(&e, er.names);
    if e.kind != CurveKind::Line {
      return Err("pick a straight edge at the end of each flange".into());
    }
    let (panel, wall) = sheet::edge_faces(body.faces(), e.faces, e.length, info.thickness)
      .ok_or("pick an edge where a face of the sheet meets the flange's square end")?;
    Ok((bi, Flange { body: er.body, info, a: e.start, b: e.end, n: panel.normal, s: wall.normal }))
  }

  /// Joins two sheet metal bodies with a bend between the flanges at `e1` and `e2`.
  fn join_by_bend(
    &mut self,
    id: FeatureId,
    e1: &mut EdgeRef,
    e2: &mut EdgeRef,
    radius: Option<f64>,
    position: JoinBendPosition,
    relief: JoinRelief,
  ) -> FResult {
    if e1.body == e2.body {
      return Err("pick an edge on each of two separate sheet metal bodies".into());
    }
    let (b1, f1) = self.join_flange(e1)?;
    let (b2, f2) = self.join_flange(e2)?;
    let t = f1.info.thickness;
    if (f2.info.thickness - t).abs() > 1e-6 * t.max(1.0) {
      return Err("the bodies must be the same thickness".into());
    }
    let d = (f1.b - f1.a).normalize_or_zero();
    if (f2.b - f2.a).normalize_or_zero().dot(d).abs() < 1.0 - 1e-6 {
      return Err("the edges must be parallel".into());
    }
    // work in the section across the edges: x out of the first flange, y = d × x
    let o = f1.a;
    let (ex, ey) = (f1.s, d.cross(f1.s));
    let p2 = |p: DVec3| [(p - o).dot(ex), (p - o).dot(ey)];
    let v2 = |v: DVec3| [v.dot(ex), v.dot(ey)];
    let to3 = |p: [f64; 2], z: f64| o + ex * p[0] + ey * p[1] + d * z;
    let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
    let (m1, m2) = (p2(f1.mid()), p2(f2.mid()));
    let (n1, n2, s1, s2) = (v2(f1.n), v2(f2.n), v2(f1.s), v2(f2.s));
    let cross = n1[0] * n2[1] - n1[1] * n2[0];
    if cross.abs() < 1e-6 {
      return Err("the flanges are parallel: Join by Bend joins flanges at an angle".into());
    }
    // the bend's centre lies on the side of each flange the other one is on
    let side = |n: [f64; 2], other_s: [f64; 2]| if dot(n, other_s) > 0.0 { -1.0 } else { 1.0 };
    let (g1, g2) = (side(n1, s2), side(n2, s1));
    // the bend's centre c and its radius to the middle of the sheet rm:
    // n1·c = n1·m1 + g1 rm, n2·c = n2·m2 + g2 rm
    let solve = |rm: f64| {
      let (k1, k2) = (dot(n1, m1) + g1 * rm, dot(n2, m2) + g2 * rm);
      [(k1 * n2[1] - k2 * n1[1]) / cross, (n1[0] * k2 - n2[0] * k1) / cross]
    };
    let rule_r = radius.unwrap_or(f1.info.bend_radius);
    let (c, rm) = match position {
      JoinBendPosition::Tangent => (solve(rule_r + t / 2.0), rule_r + t / 2.0),
      // the bend starts at one flange's edge: its centre is square off that edge, as far from
      // the other flange's middle as from this one's
      JoinBendPosition::First | JoinBendPosition::Second => {
        let first = position == JoinBendPosition::First;
        let (ma, na, ga, mb, nb, gb) = if first { (m1, n1, g1, m2, n2, g2) } else { (m2, n2, g2, m1, n1, g1) };
        let rm = dot(nb, [ma[0] - mb[0], ma[1] - mb[1]]) / (gb - ga * dot(na, nb));
        if !(rm.is_finite() && rm >= t / 2.0 - 1e-9) {
          return Err(format!(
            "the bend can't start at the {} edge: the other flange is too close",
            if first { "first" } else { "second" }
          ));
        }
        ([ma[0] + ga * rm * na[0], ma[1] + ga * rm * na[1]], rm)
      }
    };
    let r_in = (rm - t / 2.0).max(0.0);
    let tangent = |n: [f64; 2], g: f64| [c[0] - g * rm * n[0], c[1] - g * rm * n[1]];
    let (t1, t2) = (tangent(n1, g1), tangent(n2, g2));
    // the span both edges cover
    let span = |f: &Flange| {
      let (a, b) = ((f.a - o).dot(d), (f.b - o).dot(d));
      (a.min(b), a.max(b))
    };
    let ((lo1, hi1), (lo2, hi2)) = (span(&f1), span(&f2));
    let (lo, hi) = (lo1.max(lo2), hi1.min(hi2));
    if hi - lo < 1e-6 {
      return Err("the edges don't overlap along their length".into());
    }
    let ke = |e: fox_kernel::KernelError| e.to_string();
    // a box over the span in a flange's plane, from `from` to `to` along s, through the sheet
    let slab = |f: &Flange, at: [f64; 2], from: f64, to: f64, pad: f64| -> Result<Shape, String> {
      let plane = Plane { origin: to3(at, lo), x: f.s, y: f.n };
      let rect = sheet::rectangle([from, -t / 2.0 - pad], [to, t / 2.0 + pad]);
      Shape::face(&plane, &rect).map_err(ke)?.extrude(d * (hi - lo)).map_err(ke)
    };
    let mut adds = Vec::new();
    let mut shapes = Vec::new();
    for (bi, f, m, s, tp, (flo, fhi)) in [(b1, &f1, m1, s1, t1, (lo1, hi1)), (b2, &f2, m2, s2, t2, (lo2, hi2))] {
      let mut shape = self.state.bodies[bi].shape.clone();
      let grow = dot(s, tp) - dot(s, m);
      if grow > 1e-7 {
        adds.push(slab(f, m, 0.0, grow, 0.0)?);
      } else if grow < -1e-7 {
        // the flange runs past the bend: trim it back over the span
        let cut = slab(f, tp, 0.0, -grow + t, t)?;
        shape = shape.boolean(&cut, BoolOp::Subtract).map_err(ke)?;
      }
      // bend relief where the flange runs on past an end of the bend
      let (shape_kind, w, depth) = match relief {
        JoinRelief::None => (ReliefShape::None, 0.0, 0.0),
        JoinRelief::Rule => (f.info.relief, f.info.relief_width, f.info.relief_depth),
        JoinRelief::Rectangular => (ReliefShape::Rectangular, f.info.relief_width, f.info.relief_depth),
        JoinRelief::Round => (ReliefShape::Round, f.info.relief_width, f.info.relief_depth),
      };
      if shape_kind != ReliefShape::None {
        let outer = (-grow).max(0.0) + t;
        for (needed, along) in [(flo < lo - 1e-6, (lo - w, lo)), (fhi > hi + 1e-6, (hi, hi + w))] {
          if !needed {
            continue;
          }
          let plane = Plane { origin: to3(tp, 0.0) + f.n * t, x: d, y: f.s };
          let prof = sheet::relief_profile(along, -depth, outer, shape_kind == ReliefShape::Round);
          let notch = Shape::face(&plane, &prof).map_err(ke)?.extrude(f.n * (-2.0 * t)).map_err(ke)?;
          shape = shape.boolean(&notch, BoolOp::Subtract).map_err(ke)?;
        }
      }
      shapes.push(shape);
    }
    // the bend: the sheet's section at the first tangent line turned about the bend's axis
    let (u1, u2) = ([(t1[0] - c[0]) / rm, (t1[1] - c[1]) / rm], [(t2[0] - c[0]) / rm, (t2[1] - c[1]) / rm]);
    let angle = dot(u1, u2).clamp(-1.0, 1.0).acos();
    if angle > 1e-6 {
      let axis = if u1[0] * u2[1] - u1[1] * u2[0] >= 0.0 { d } else { -d };
      let radial = ex * u1[0] + ey * u1[1];
      let origin = to3(c, lo);
      let plane = Plane { origin, x: radial, y: d };
      let section = sheet::rectangle([r_in, 0.0], [r_in + t, hi - lo]);
      adds.push(Shape::face(&plane, &section).map_err(ke)?.revolve(origin, axis, angle).map_err(ke)?);
    }
    let mut acc = shapes[0].clone();
    for a in adds.iter().chain(std::iter::once(&shapes[1])) {
      acc = acc.boolean(a, BoolOp::Union).map_err(ke)?;
    }
    let acc = acc.unified().unwrap_or(acc);
    if acc.solids().map_err(ke)?.len() != 1 {
      return Err("the bend doesn't connect the two bodies".into());
    }
    self.state.bodies[b1] = self.state.bodies[b1].with_shape(acc, id);
    let gone = f2.body;
    self.state.bodies.retain(|b| b.id != gone);
    let warning = (position != JoinBendPosition::Tangent && (r_in - rule_r).abs() > 1e-6)
      .then(|| format!("the bend's radius is {r_in:.3} mm so that it starts at the picked edge"));
    Ok(warning)
  }
}

/// Is `e` a straight edge through the sheet: as long as it is thick, square to a face of the
/// sheet that it starts or ends on?
fn thickness_edge(body: &Body, e: &EdgeInfo, t: f64) -> bool {
  if e.kind != CurveKind::Line || (e.length - t).abs() > 1e-3 * t.max(1.0) {
    return false;
  }
  let dir = (e.end - e.start).normalize_or_zero();
  let tol = 1e-6 * body.diagonal().max(1.0);
  body.faces().iter().any(|f| {
    f.kind == SurfaceKind::Plane
      && f.normal.dot(dir).abs() > 1.0 - 1e-6
      && [e.start, e.end].iter().any(|p| (*p - f.origin).dot(f.normal).abs() < tol)
  })
}
