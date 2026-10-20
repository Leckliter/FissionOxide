//! The fillet command's options: radii at points along each edge, selection sets of edges and
//! faces with their own radii, and tangent chains.

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind, EdgeInfo};
use fox_model::{BodyId, EdgeRef, FilletSet, RadiusPoint};
use serde_json::Value;

use crate::commands::{edge_ref_of, expr_of, req_expr, req_u32};
use crate::{CmdError, Session};

/// `"radii": [[at, radius], ...]` or `[{"at": .., "radius": ..}, ...]`, `at` running from 0 at
/// each edge's start to 1 at its end.
pub(crate) fn radius_points(p: &Value) -> Result<Vec<RadiusPoint>, CmdError> {
  let Some(list) = p.get("radii") else { return Ok(Vec::new()) };
  let list = list.as_array().ok_or(CmdError::new("'radii' must be a list of [at, radius] points"))?;
  let mut out = Vec::new();
  for v in list {
    let (at, r) = match v {
      Value::Array(a) if a.len() == 2 => (&a[0], &a[1]),
      Value::Object(o) => match (o.get("at"), o.get("radius")) {
        (Some(at), Some(r)) => (at, r),
        _ => return Err(CmdError::new("a radius point needs 'at' and 'radius'")),
      },
      _ => return Err(CmdError::new("a radius point is [at, radius]")),
    };
    let at = at.as_f64().filter(|t| (0.0..=1.0).contains(t)).ok_or(CmdError::new("'at' must be from 0 to 1"))?;
    out.push(RadiusPoint::new(at, expr_of(r)?));
  }
  Ok(out)
}

/// The edges of a fillet selection: `edges`, every edge of each of `faces`, and (with `chain`)
/// the edges that continue them tangentially. Edges in `seen` (earlier sets) are left out.
pub(crate) fn selection_edges(
  s: &Session,
  p: &Value,
  chain: bool,
  seen: &mut Vec<(BodyId, u32)>,
) -> Result<Vec<EdgeRef>, CmdError> {
  let mut picked: Vec<(BodyId, u32)> = Vec::new();
  fn add(picked: &mut Vec<(BodyId, u32)>, b: BodyId, e: u32) {
    if !picked.contains(&(b, e)) {
      picked.push((b, e));
    }
  }
  for e in p.get("edges").and_then(Value::as_array).into_iter().flatten() {
    let r = edge_ref_of(s, e)?;
    let b = s.model.body(r.body).ok_or(CmdError::new("no such body"))?;
    for id in r.resolve_all(b.edges(), b.faces(), b.diagonal()) {
      add(&mut picked, r.body, id);
    }
  }
  for f in p.get("faces").and_then(Value::as_array).into_iter().flatten() {
    let body = BodyId(req_u32(f, "body")?);
    let face = req_u32(f, "face")?;
    let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
    if !b.faces().iter().any(|x| x.id == face) {
      return Err(CmdError::new(format!("body has no face {face}")));
    }
    for e in b.edges().iter().filter(|e| e.faces.contains(&face) && e.kind != CurveKind::Degenerate) {
      add(&mut picked, body, e.id);
    }
  }
  if chain {
    let mut i = 0;
    while i < picked.len() {
      let (body, id) = picked[i];
      if let Some(b) = s.model.body(body) {
        for next in tangent_neighbours(b.edges(), id) {
          add(&mut picked, body, next);
        }
      }
      i += 1;
    }
  }
  let mut out = Vec::new();
  for (body, id) in picked {
    if seen.contains(&(body, id)) {
      continue;
    }
    seen.push((body, id));
    let b = s.model.body(body).ok_or(CmdError::new("no such body"))?;
    let info = b.edges().iter().find(|x| x.id == id).ok_or(CmdError::new(format!("body has no edge {id}")))?;
    out.push(EdgeRef::from_edge(body, info, b.faces()));
  }
  Ok(out)
}

/// The fillet command's selection sets: `sets: [{edges?, faces?, radius}]`, or the one set
/// `{edges?, faces?, radius}` given at the top level.
pub(crate) fn selection_sets(s: &Session, p: &Value) -> Result<(Vec<EdgeRef>, String, Vec<FilletSet>), CmdError> {
  let chain = p.get("tangent_chain").and_then(Value::as_bool).unwrap_or(false);
  let Some(list) = p.get("sets") else {
    let edges = selection_edges(s, p, chain, &mut Vec::new())?;
    if edges.is_empty() {
      return Err(CmdError::new("no edges given"));
    }
    return Ok((edges, req_expr(p, "radius")?, Vec::new()));
  };
  let list = list.as_array().filter(|l| !l.is_empty()).ok_or(CmdError::new("'sets' must be a non-empty list"))?;
  let mut sets: Vec<FilletSet> = Vec::new();
  let mut seen = Vec::new();
  for v in list {
    // an edge already in an earlier set stays there
    let edges = selection_edges(s, v, chain, &mut seen)?;
    if edges.is_empty() {
      return Err(CmdError::new("a fillet set has no edges"));
    }
    sets.push(FilletSet { edges, radius: req_expr(v, "radius")? });
  }
  let first = sets.remove(0);
  Ok((first.edges, first.radius, sets))
}

/// Edges meeting `id` end to end without a kink (Fusion's tangent chain).
pub fn tangent_neighbours(edges: &[EdgeInfo], id: u32) -> Vec<u32> {
  let Some(e) = edges.iter().find(|x| x.id == id) else { return Vec::new() };
  let tol = 1e-6 * (1.0 + e.length);
  let mut out = Vec::new();
  for p in [e.start, e.end] {
    let Some(t) = tangent_at(e, p) else { continue };
    for o in edges.iter().filter(|o| o.id != id && o.kind != CurveKind::Degenerate) {
      let q = if (o.start - p).length() < tol {
        o.start
      } else if (o.end - p).length() < tol {
        o.end
      } else {
        continue;
      };
      if let Some(u) = tangent_at(o, q)
        && t.cross(u).length() < 1e-3
      {
        out.push(o.id);
      }
    }
  }
  out
}

/// The unit direction of a line or circular edge at a point on it.
fn tangent_at(e: &EdgeInfo, p: DVec3) -> Option<DVec3> {
  match e.kind {
    CurveKind::Line => (e.end - e.start).try_normalize(),
    CurveKind::Circle => e.axis.cross(p - e.center).try_normalize(),
    _ => None,
  }
}
