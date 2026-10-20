//! Include 3D Geometry (Fusion's `Sketch.include`): body edges and other sketches' curves brought
//! into a sketch at their real positions. Those in the sketch plane become ordinary (fixed)
//! sketch curves that still form profiles; the rest stay 3D curves, for sweep paths and rails.
//! Both follow their source as the model changes unless `associative` is false.

use fox_kernel::Plane;
use fox_kernel::glam::DVec3;
use fox_model::include::{edge_curve3d, on_plane, sketch_curve3d};
use fox_model::{BodyId, EdgeRef, FeatureId, FeatureKind, SketchLink};
use fox_sketch::{ConstraintKind, Curve3dKind, CurveId, CurveSource, Dim, PointId, PtSpec, Sketch, SketchError};
use serde_json::{Value, json};

use super::{edge_at, req_u32, req_vec3, sketch_id};
use crate::{CmdError, CmdResult, Session};

struct Item {
  kind: Curve3dKind,
  edge: Option<EdgeRef>,
  source: Option<CurveSource>,
}

pub(super) fn include(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let plane = s.model.sketches.get(&sk).map(|r| r.plane).ok_or(CmdError::new("the sketch failed to build"))?;
  let associative = p.get("associative").and_then(Value::as_bool).unwrap_or(true);
  let mut todo = Vec::new();
  for e in p.get("edges").and_then(Value::as_array).into_iter().flatten() {
    let (body, edge) = if e.get("point").is_some() {
      edge_at(s, DVec3::from(req_vec3(e, "point")?))?
    } else {
      (BodyId(req_u32(e, "body")?), req_u32(e, "edge")?)
    };
    let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
    let info = b.edges().iter().find(|x| x.id == edge).ok_or(CmdError::new(format!("body has no edge {edge}")))?;
    let kind = edge_curve3d(info).map_err(CmdError::new)?;
    let edge = associative.then(|| EdgeRef::from_edge(body, info, b.faces()));
    todo.push(Item { kind, edge, source: None });
  }
  let here = s.design.index_of(sk);
  for c in p.get("curves").and_then(Value::as_array).into_iter().flatten() {
    let from = FeatureId(req_u32(c, "sketch")?);
    let curve = CurveId(req_u32(c, "curve")?);
    if s.design.index_of(from).is_none_or(|i| here.is_none_or(|h| i >= h)) {
      return Err(CmdError::new(format!("sketch {} is not an earlier sketch", from.0)));
    }
    let r = s.model.sketches.get(&from).ok_or(CmdError::new(format!("sketch {} failed to build", from.0)))?;
    let kind = sketch_curve3d(r, curve).map_err(|e| CmdError::new(format!("sketch {}: {e}", from.0)))?;
    todo.push(Item { kind, edge: None, source: associative.then_some(CurveSource { sketch: from.0, curve }) });
  }
  if todo.is_empty() {
    return Err(CmdError::new("give the edges or sketch curves to include"));
  }
  let made = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let before: Vec<PointId> = sketch.points.iter().map(|p| p.id).collect();
    let mut made = Vec::new();
    let mut links = Vec::new();
    for it in todo {
      let ids = if on_plane(&it.kind, &plane) {
        add_planar(sketch, &plane, &it.kind, &before)?
      } else {
        vec![sketch.add_curve3d(it.kind, it.source)]
      };
      if let (Some(edge), [id]) = (it.edge, ids.as_slice()) {
        links.push(SketchLink { curve: *id, edge });
      }
      made.extend(ids);
    }
    for pt in sketch.points.iter_mut().filter(|p| !before.contains(&p.id)) {
      pt.fixed = true;
    }
    if let Some(FeatureKind::Sketch { links: l, .. }) = d.feature_mut(sk).map(|f| &mut f.kind) {
      l.extend(links);
    }
    Ok(made)
  })?;
  let r = s.model.sketches.get(&sk);
  let in_3d = made.iter().filter(|c| r.is_some_and(|r| r.sketch.curve3d(**c).is_some())).count();
  Ok(json!({
    "curves": made.iter().map(|c| c.0).collect::<Vec<_>>(),
    "curves_3d": in_3d,
    "profiles": r.map(|r| r.profiles.len()).unwrap_or(0),
  }))
}

/// An included curve that lies in the sketch plane, as ordinary sketch curves joined to the
/// points this include already made.
fn add_planar(
  sketch: &mut Sketch,
  plane: &Plane,
  kind: &Curve3dKind,
  before: &[PointId],
) -> Result<Vec<CurveId>, SketchError> {
  let l = |p: [f64; 3]| plane.to_local(DVec3::from(p));
  let at = |sketch: &Sketch, q: [f64; 2]| match sketch.nearest_point(q, 1e-6, &[]) {
    Some(id) if !before.contains(&id) => PtSpec::Point(id),
    _ => PtSpec::At(q[0], q[1]),
  };
  let mut out = Vec::new();
  match kind {
    Curve3dKind::Circle { center, radius, .. } => {
      let c = at(sketch, l(*center));
      let id = sketch.add_circle(c, *radius)?;
      sketch.add_constraint(ConstraintKind::Radius { curve: id, value: Dim::new(*radius) })?;
      out.push(id);
    }
    Curve3dKind::Arc { start, mid, end } => {
      let (a, b) = (at(sketch, l(*start)), at(sketch, l(*end)));
      out.push(sketch.add_arc_3pt(a, l(*mid), b)?);
    }
    Curve3dKind::Line { a, b } => {
      let (a, b) = (l(*a), l(*b));
      if (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-6 {
        let (sa, sb) = (at(sketch, a), at(sketch, b));
        out.push(sketch.add_line(sa, sb)?.0);
      }
    }
    Curve3dKind::Polyline { points } => {
      for w in points.windows(2) {
        let (a, b) = (l(w[0]), l(w[1]));
        if (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-6 {
          let (sa, sb) = (at(sketch, a), at(sketch, b));
          out.push(sketch.add_line(sa, sb)?.0);
        }
      }
    }
  }
  Ok(out)
}
