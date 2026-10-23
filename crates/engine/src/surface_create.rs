//! Commands for the SURFACE tab's Create commands as Fusion's dialogs run them: Extrude and
//! Revolve of picked profiles or curves, Patch with a continuity per boundary edge, Ruled grown
//! from (chained) edges, Offset with an Offset Type, and the surface Sweep and Loft.

use fox_kernel::glam::DVec3;
use fox_model::surface::{Continuity, OffsetCorners, PatchRail, RuledType, SurfaceSection};
use fox_model::{BodyId, EdgeRef, FeatureId, FeatureKind};
use fox_sketch::CurveId;
use serde_json::{Value, json};

use crate::commands::{
  add_feature, axis_ref, edge_ref_of, ids, opt_expr, opt_str, profile_points, req_expr, req_u32, sketch_for_feature,
};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  Some(match name {
    "surface.extrude" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sketch, p)?;
      if flag("symmetric") && p.get("distance2").is_some() {
        return Err(CmdError::new("an extrude is either symmetric or two-sided ('distance2'), not both"));
      }
      add_feature(
        s,
        FeatureKind::SurfaceExtrude {
          sketch,
          curves: curve_ids(p),
          distance: req_expr(p, "distance")?,
          symmetric: flag("symmetric"),
          flip: flag("flip"),
          profiles,
          offset: opt_expr(p, "offset").unwrap_or_default(),
          distance2: opt_expr(p, "distance2"),
        },
      )
    })(),
    "surface.revolve" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sketch, p)?;
      let axis = axis_ref(s, p.get("axis").ok_or(CmdError::new("missing 'axis'"))?)?;
      if flag("symmetric") && p.get("angle2").is_some() {
        return Err(CmdError::new("a revolve is either symmetric or two-sided ('angle2'), not both"));
      }
      add_feature(
        s,
        FeatureKind::SurfaceRevolve {
          sketch,
          curves: curve_ids(p),
          axis,
          angle: opt_expr(p, "angle").unwrap_or_else(|| "360 deg".into()),
          profiles,
          symmetric: flag("symmetric"),
          angle2: opt_expr(p, "angle2"),
        },
      )
    })(),
    "surface.patch" => (|| {
      // the boundary picks: body edges, and closed sketch curves `{sketch, curve}` (always
      // connected), each pick keeping its place in the continuity list
      let picks: Vec<Value> = p.get("edges").and_then(Value::as_array).cloned().unwrap_or_default();
      let (curve_picks, edge_picks): (Vec<(usize, &Value)>, Vec<(usize, &Value)>) =
        picks.iter().enumerate().partition(|(_, e)| e.get("sketch").is_some());
      let only_edges = json!({"edges": edge_picks.iter().map(|(_, e)| (*e).clone()).collect::<Vec<_>>()});
      // each picked edge with the boundary it chains on to (one group per pick)
      let groups: Vec<Vec<EdgeRef>> = if flag("chain") {
        chained(s, &only_edges, chain_boundary)?
      } else {
        list(&only_edges, "edges", |e| edge_ref_of(s, e))?.into_iter().map(|e| vec![e]).collect()
      };
      let edges: Vec<EdgeRef> = groups.iter().flatten().cloned().collect();
      let mut sketch = p.get("sketch").map(|_| req_u32(p, "sketch").map(FeatureId)).transpose()?;
      let mut curves: Vec<CurveId> = Vec::new();
      for (_, c) in &curve_picks {
        let sk = FeatureId(req_u32(c, "sketch")?);
        if sketch.is_some_and(|k| k != sk) {
          return Err(CmdError::new("a patch's boundary curves come from one sketch"));
        }
        sketch = Some(sk);
        let curve = CurveId(req_u32(c, "curve")?);
        if s.design.sketch(sk)?.curve(curve).is_none() {
          return Err(CmdError::new(format!("sketch {} has no curve {}", sk.0, curve.0)));
        }
        if !curves.contains(&curve) {
          curves.push(curve);
        }
      }
      if let Some(sk) = sketch {
        if flag("chain") && !curves.is_empty() {
          curves = chain_curves(s.design.sketch(sk)?, &curves);
        }
        s.design.sketch(sk)?;
        s.edit_sketch = None;
      }
      if edges.is_empty() && sketch.is_none() {
        return Err(CmdError::new("pass the boundary 'edges' or a 'sketch'"));
      }
      // one continuity for every edge, or one per picked edge (for the boundary it chains to)
      let continuity = match p.get("continuity") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => {
          let per: Vec<Continuity> = a.iter().map(continuity).collect::<Result<_, _>>()?;
          let per: Vec<Continuity> = edge_picks.iter().map(|(i, _)| per.get(*i).copied().unwrap_or_default()).collect();
          groups.iter().enumerate().flat_map(|(i, g)| vec![per.get(i).copied().unwrap_or_default(); g.len()]).collect()
        }
        Some(v) => vec![continuity(v)?; edges.len()],
      };
      let interior = list(p, "interior", |v| patch_rail(s, v))?;
      add_feature(s, FeatureKind::Patch { edges, sketch, curves, continuity, flip: flag("flip"), interior })
    })(),
    // the two-edge form stays for designs and scripts that use it
    "surface.ruled" if p.get("edges").is_some() => (|| {
      let edges = if flag("chain") {
        crate::fillet::selection_edges(s, p, true, &mut Vec::new())?
      } else {
        list(p, "edges", |e| edge_ref_of(s, e))?
      };
      if edges.is_empty() {
        return Err(CmdError::new("no edges given"));
      }
      let along = match opt_str(p, "kind").unwrap_or("normal") {
        "normal" => RuledType::Normal,
        "tangent" => RuledType::Tangent,
        o => return Err(CmdError::new(format!("unknown ruled kind '{o}' (normal, tangent)"))),
      };
      let angle = opt_expr(p, "angle").unwrap_or_else(|| "0 deg".into());
      add_feature(s, FeatureKind::RuledSurface { edges, distance: req_expr(p, "distance")?, angle, along })
    })(),
    "surface.offset" => (|| {
      let picked: Vec<Value> = p.get("faces").and_then(Value::as_array).cloned().unwrap_or_default();
      let picked = if flag("tangent_chain") { crate::solid::tangent_faces(s, picked) } else { picked };
      let faces = list(&json!({ "faces": picked }), "faces", |f| crate::commands::face_ref(s, f))?;
      let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
      let face_distances = crate::surfaces::face_distances(s, p)?;
      let corners = match opt_str(p, "corners") {
        None => OffsetCorners::Separate,
        Some("sharp") => OffsetCorners::Sharp,
        Some("round") => OffsetCorners::Round,
        Some(o) => return Err(CmdError::new(format!("unknown corners '{o}' (sharp or round)"))),
      };
      let distance = req_expr(p, "distance")?;
      add_feature(s, FeatureKind::SurfaceOffset { faces, bodies, distance, face_distances, corners })
    })(),
    "surface.sweep" => (|| {
      let path_sketch = FeatureId(req_u32(p, "path")?);
      s.design.sketch(path_sketch)?;
      let sketch = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sketch, p)?;
      let parallel = match opt_str(p, "orientation").unwrap_or("perpendicular") {
        "perpendicular" => false,
        "parallel" => true,
        o => return Err(CmdError::new(format!("unknown sweep orientation '{o}' (perpendicular, parallel)"))),
      };
      let path_curves = ids(p, "path_curves").into_iter().map(CurveId).collect();
      add_feature(
        s,
        FeatureKind::SurfaceSweep { sketch, profiles, curves: curve_ids(p), path_sketch, path_curves, parallel },
      )
    })(),
    "surface.loft" => (|| {
      let raw = p.get("sections").and_then(Value::as_array).ok_or(CmdError::new("missing 'sections'"))?;
      let mut sections = Vec::new();
      for v in raw {
        let sketch = match v.as_u64() {
          Some(id) => id,
          None => v
            .get("sketch")
            .and_then(Value::as_u64)
            .ok_or(CmdError::new("a section is a sketch id or {sketch, profile?: [x, y], curves?: [id]}"))?,
        };
        let sketch = FeatureId(u32::try_from(sketch).map_err(|_| CmdError::new("bad sketch id"))?);
        s.design.sketch(sketch)?;
        let profile = match v.get("profile") {
          None | Some(Value::Null) => None,
          Some(x) => Some(point(x).ok_or(CmdError::new("a section's 'profile' is [x, y]"))?),
        };
        sections.push(SurfaceSection { sketch, profile, curves: curve_ids(v) });
      }
      if sections.len() < 2 {
        return Err(CmdError::new("a loft needs at least two sections"));
      }
      s.edit_sketch = None;
      let mut rails = Vec::new();
      for v in p.get("rails").and_then(Value::as_array).into_iter().flatten() {
        rails.push(crate::solid::path_ref(s, v)?);
      }
      let centerline =
        p.get("centerline").filter(|v| !v.is_null()).map(|v| crate::solid::path_ref(s, v)).transpose()?;
      add_feature(s, FeatureKind::SurfaceLoft { sections, closed: flag("closed"), rails, centerline })
    })(),
    _ => return None,
  })
}

/// The sketch curves joined end to end to `seeds` (and the seeds), as Patch's Enable Chaining
/// takes a whole closed loop from one of its curves.
fn chain_curves(sk: &fox_sketch::Sketch, seeds: &[CurveId]) -> Vec<CurveId> {
  let ends = |c: CurveId| {
    let segs = sk.curve_segs(c);
    Some([segs.first()?.start(), segs.last()?.end()])
  };
  let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6;
  let mut chain = seeds.to_vec();
  loop {
    let reached: Vec<[f64; 2]> = chain.iter().filter_map(|c| ends(*c)).flatten().collect();
    let next = sk.curves.iter().find(|c| {
      !c.reference()
        && !chain.contains(&c.id)
        && ends(c.id).is_some_and(|e| e.iter().any(|p| reached.iter().any(|q| near(*p, *q))))
    });
    match next {
      Some(c) => chain.push(c.id),
      None => return chain,
    }
  }
}

fn curve_ids(p: &Value) -> Vec<CurveId> {
  ids(p, "curves").into_iter().map(CurveId).collect()
}

fn point(v: &Value) -> Option<[f64; 2]> {
  let a = v.as_array()?;
  match a.as_slice() {
    [x, y] => Some([x.as_f64()?, y.as_f64()?]),
    _ => None,
  }
}

fn list<T>(p: &Value, k: &str, mut f: impl FnMut(&Value) -> Result<T, CmdError>) -> Result<Vec<T>, CmdError> {
  p.get(k).and_then(Value::as_array).into_iter().flatten().map(&mut f).collect()
}

/// One of Patch's Interior Rails/Points: a body edge `{body, edge}`, a sketch curve `{sketch,
/// curve}`, a sketch point `{sketch, point}` or a point `{at: [x, y, z]}` / `[x, y, z]`.
fn patch_rail(s: &Session, v: &Value) -> Result<PatchRail, CmdError> {
  let at = v.get("at").unwrap_or(v);
  if let Some(a) = at.as_array() {
    let xyz: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
    let [x, y, z] = xyz[..] else { return Err(CmdError::new("a point is [x, y, z]")) };
    return Ok(PatchRail::Point { at: [x, y, z] });
  }
  if v.get("sketch").is_some() {
    let sketch = FeatureId(req_u32(v, "sketch")?);
    if v.get("point").is_some() {
      let point = fox_sketch::PointId(req_u32(v, "point")?);
      let r = s.model.sketches.get(&sketch).ok_or(CmdError::new("that sketch has no solved geometry"))?;
      let q = r.sketch.pos(point).ok_or(CmdError::new("that sketch has no such point"))?;
      return Ok(PatchRail::Point { at: r.plane.to_world(q).to_array() });
    }
    let curve = CurveId(req_u32(v, "curve")?);
    if s.design.sketch(sketch)?.curve(curve).is_none() {
      return Err(CmdError::new(format!("the sketch has no curve {}", curve.0)));
    }
    return Ok(PatchRail::Curve { sketch, curve });
  }
  Ok(PatchRail::Edge { edge: edge_ref_of(s, v)? })
}

fn continuity(v: &Value) -> Result<Continuity, CmdError> {
  match v.as_str() {
    Some("connected") | Some("g0") => Ok(Continuity::Connected),
    Some("tangent") | Some("g1") => Ok(Continuity::Tangent),
    Some("curvature") | Some("g2") => Ok(Continuity::Curvature),
    _ => Err(CmdError::new("continuity is connected, tangent or curvature")),
  }
}

/// Each picked edge with the edges `grow` chains on to it (one group per pick; an edge an
/// earlier group reached stays there).
fn chained(
  s: &Session,
  p: &Value,
  grow: fn(&[fox_kernel::EdgeInfo], u32) -> Vec<u32>,
) -> Result<Vec<Vec<EdgeRef>>, CmdError> {
  let mut seen: Vec<(BodyId, u32)> = Vec::new();
  let mut groups = Vec::new();
  for e in p.get("edges").and_then(Value::as_array).into_iter().flatten() {
    let r = edge_ref_of(s, e)?;
    let b = s.model.body(r.body).ok_or(CmdError::new("no such body"))?;
    let mut group: Vec<u32> = Vec::new();
    for id in r.resolve_all(b.edges(), b.faces(), b.diagonal()) {
      for g in grow(b.edges(), id) {
        if !seen.contains(&(r.body, g)) && !group.contains(&g) {
          group.push(g);
        }
      }
    }
    let mut refs = Vec::new();
    for id in group {
      seen.push((r.body, id));
      let info = b.edges().iter().find(|x| x.id == id).ok_or(CmdError::new(format!("body has no edge {id}")))?;
      refs.push(b.edge_ref(info, [0; 2]));
    }
    if !refs.is_empty() {
      groups.push(refs);
    }
  }
  Ok(groups)
}

/// Patch's Enable Chaining: from an open edge of a surface, the open edges joined end to end to it
/// until the boundary closes (a closed edge is a boundary by itself); from an edge between two
/// faces, the edges that continue it smoothly.
pub fn chain_boundary(edges: &[fox_kernel::EdgeInfo], id: u32) -> Vec<u32> {
  let Some(e) = edges.iter().find(|x| x.id == id) else { return Vec::new() };
  let near = |a: DVec3, b: DVec3| (a - b).length() < 1e-6;
  let open = |x: &fox_kernel::EdgeInfo| x.faces[1] == 0 && x.faces[0] != 0;
  if !open(e) {
    let mut chain = vec![id];
    let mut i = 0;
    while i < chain.len() {
      for n in crate::fillet::tangent_neighbours(edges, chain[i]) {
        if !chain.contains(&n) {
          chain.push(n);
        }
      }
      i += 1;
    }
    return chain;
  }
  let mut chain = vec![id];
  let (start, mut end) = (e.start, e.end);
  while !near(start, end) {
    let next = edges
      .iter()
      .find(|x| open(x) && !chain.contains(&x.id) && !near(x.start, x.end) && (near(x.start, end) || near(x.end, end)));
    let Some(x) = next else { break };
    chain.push(x.id);
    end = if near(x.start, end) { x.end } else { x.start };
  }
  chain
}
