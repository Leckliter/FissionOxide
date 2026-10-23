//! Simplify commands (Fusion's SOLID > MODIFY > Simplify): Remove Features finds fillets, holes,
//! chamfers, extruded and revolved bosses and pockets and other bumps on bodies and removes them;
//! Replace with Primitives swaps bodies for a box or cylinder around each.

use fox_kernel::glam::DVec3;
use std::collections::{HashMap, HashSet};

use fox_kernel::{CurveKind, FaceInfo, Inside, Meet, SurfaceKind};
use fox_model::bounding::BoundShape;
use fox_model::{BodyId, BodySel, FeatureKind};
use serde_json::{Value, json};

use crate::commands::ids;
use crate::{CmdError, CmdResult, Session};

/// The feature types Remove Features looks for, as its Features group lists them.
pub const FEATURE_TYPES: [&str; 6] = ["fillet", "hole", "chamfer", "extrude", "revolve", "other"];

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "remove_features" => remove_features(s, p),
    "replace_with_primitives" => replace_with_primitives(s, p),
    _ => return None,
  })
}

fn bodies(s: &Session, p: &Value) -> Result<Vec<BodyId>, CmdError> {
  let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if bodies.is_empty() {
    return Err(CmdError::new("select solid bodies"));
  }
  for b in &bodies {
    s.model.body(*b).ok_or(CmdError::new(format!("no body {}", b.0)))?;
  }
  Ok(bodies)
}

/// A recognised feature: its type, size (fillet radius, hole diameter) and faces.
struct Found {
  kind: &'static str,
  size: f64,
  body: BodyId,
  faces: Vec<u32>,
}

fn remove_features(s: &mut Session, p: &Value) -> CmdResult {
  let bodies = bodies(s, p)?;
  let types: Vec<String> = match p.get("types").and_then(Value::as_array) {
    Some(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
    None => FEATURE_TYPES.iter().map(|t| t.to_string()).collect(),
  };
  for t in &types {
    if !FEATURE_TYPES.contains(&t.as_str()) {
      return Err(CmdError::new(format!("unknown feature type '{t}' ({})", FEATURE_TYPES.join(", "))));
    }
  }
  let mut found = Vec::new();
  for b in &bodies {
    found.extend(recognise(s, *b)?.into_iter().filter(|f| types.iter().any(|t| t == f.kind)));
  }
  let list: Vec<Value> = found
    .iter()
    .map(|f| {
      let faces: Vec<Value> = f.faces.iter().map(|x| json!({"body": f.body.0, "face": x})).collect();
      json!({"type": f.kind, "size": f.size, "body": f.body.0, "faces": faces})
    })
    .collect();
  if p.get("dry_run").and_then(Value::as_bool).unwrap_or(false) {
    return Ok(json!({ "features": list }));
  }
  if found.is_empty() {
    return Err(CmdError::new("no features of those types on the bodies"));
  }
  let mut faces: Vec<Value> = Vec::new();
  for f in list.iter().flat_map(|f| f["faces"].as_array().cloned().unwrap_or_default()) {
    if !faces.contains(&f) {
      faces.push(f);
    }
  }
  let mut r = crate::commands::run(s, "remove_faces", &json!({ "faces": faces }))?;
  r["removed"] = json!(list);
  Ok(r)
}

/// Fillets and holes, then chamfers, then what stands on or is sunk into a face (where it meets
/// the face in an inner loop): extruded and revolved bosses and pockets, and other shapes.
fn recognise(s: &Session, b: BodyId) -> Result<Vec<Found>, CmdError> {
  let mut out = rounds(s, b)?;
  let body = s.model.body(b).ok_or(CmdError::new(format!("no body {}", b.0)))?;
  let faces = body.faces();
  let edges = body.edges();
  let uses = body.shape.edge_uses().map_err(|e| CmdError::new(e.to_string()))?;
  let face = |id: u32| faces.iter().find(|f| f.id == id);
  // how the faces meet at each edge, and which wire of its face each edge is in
  let mut meet: HashMap<u32, Meet> = HashMap::new();
  let mut wire: HashMap<(u32, u32), u32> = HashMap::new();
  for u in &uses {
    meet.insert(u.edge, u.meet);
    wire.insert((u.face, u.edge), u.wire);
  }
  // each face's neighbours across its edges (a seam's own face left out)
  let mut across: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
  for e in edges.iter().filter(|e| e.kind != CurveKind::Degenerate && e.faces[0] != e.faces[1]) {
    if e.faces.contains(&0) {
      continue;
    }
    across.entry(e.faces[0]).or_default().push((e.id, e.faces[1]));
    across.entry(e.faces[1]).or_default().push((e.id, e.faces[0]));
  }
  let adjacent = |a: u32, c: u32| across.get(&a).is_some_and(|v| v.iter().any(|(_, g)| *g == c));
  let mut claimed: HashSet<u32> = out.iter().flat_map(|f| f.faces.iter().copied()).collect();
  let holes: HashSet<u32> = out.iter().filter(|f| f.kind == "hole").flat_map(|f| f.faces.iter().copied()).collect();
  // the outward normal of a flat or round face at a point on its edge
  let outward = |f: &FaceInfo, p: DVec3| -> Option<DVec3> {
    // (a plane's stored normal can face into the body: each is turned by looking just off it)
    let n = normal_at(f, p)?;
    let step = 1e-3_f64.max(f.radius * 1e-3);
    (body.shape.classify(p + n * step).ok()? == Inside::Out).then_some(n).or(Some(-n))
  };
  // chamfers: a flat or conical face, convex all round, cutting off the edge where two faces
  // that no longer touch used to meet, at a slant to both, and smaller than them
  for f in faces.iter().filter(|f| matches!(f.kind, SurfaceKind::Plane | SurfaceKind::Cone)) {
    if claimed.contains(&f.id) {
      continue;
    }
    let nb = across.get(&f.id).cloned().unwrap_or_default();
    if nb.is_empty() || nb.iter().any(|(e, _)| meet.get(e) != Some(&Meet::Convex)) {
      continue;
    }
    let slant = |e: u32, g: &FaceInfo| {
      let Some(mid) = edges.iter().find(|x| x.id == e).map(|x| x.mid) else { return false };
      match (outward(f, mid), outward(g, mid)) {
        (Some(n), Some(m)) => n.dot(m) > 0.05 && n.dot(m) < 0.999,
        // (a cone's normal isn't worked out: its rails are circles on its neighbours)
        (None, _) if f.kind == SurfaceKind::Cone => true,
        _ => false,
      }
    };
    let rails = nb.iter().enumerate().find_map(|(i, (e1, a))| {
      nb[i + 1..].iter().find_map(|(e2, c)| {
        let (fa, fc) = (face(*a)?, face(*c)?);
        let ok = a != c && !adjacent(*a, *c) && slant(*e1, fa) && slant(*e2, fc) && f.area < 0.5 * fa.area.min(fc.area);
        ok.then_some(())
      })
    });
    if rails.is_some() {
      claimed.insert(f.id);
      out.push(Found { kind: "chamfer", size: 0.0, body: b, faces: vec![f.id] });
    }
  }
  // bosses and pockets: the faces reached across an inner loop of a face without coming back
  // to it or entering another face through one of its inner loops
  let mut regions: Vec<(u32, Vec<u32>)> = Vec::new();
  let mut loops: Vec<(u32, u32)> = uses.iter().filter(|u| u.wire > 0).map(|u| (u.face, u.wire)).collect();
  loops.sort_unstable();
  loops.dedup();
  for (base, w) in loops {
    let seeds: Vec<u32> = uses
      .iter()
      .filter(|u| u.face == base && u.wire == w)
      .filter_map(|u| edges.iter().find(|e| e.id == u.edge))
      .filter_map(|e| if e.faces[0] == base { Some(e.faces[1]) } else { Some(e.faces[0]) })
      .filter(|g| *g != base && *g != 0)
      .collect();
    if seeds.is_empty() || seeds.iter().any(|g| holes.contains(g)) {
      continue;
    }
    let mut region: Vec<u32> = Vec::new();
    let mut todo = seeds.clone();
    while let Some(g) = todo.pop() {
      if region.contains(&g) {
        continue;
      }
      region.push(g);
      for (e, h) in across.get(&g).into_iter().flatten() {
        if *h != base && !region.contains(h) && wire.get(&(*h, *e)).is_none_or(|w| *w == 0) {
          todo.push(*h);
        }
      }
    }
    region.sort_unstable();
    if region.len() * 2 > faces.len() || regions.iter().any(|(_, r)| *r == region) {
      continue;
    }
    regions.push((base, region));
  }
  for (base, region) in regions {
    let own: Vec<&FaceInfo> = region.iter().filter(|g| !claimed.contains(g)).filter_map(|g| face(*g)).collect();
    if own.is_empty() {
      continue;
    }
    let kind = match face(base) {
      Some(bf) if extruded(bf, &own) => "extrude",
      Some(bf) if revolved(bf, &own) => "revolve",
      _ => "other",
    };
    out.push(Found { kind, size: 0.0, body: b, faces: region });
  }
  Ok(out)
}

/// Do the faces stand straight up from a flat base: walls square to it (flat, or round about
/// an axis along its normal) and caps parallel to it?
fn extruded(base: &FaceInfo, faces: &[&FaceInfo]) -> bool {
  if base.kind != SurfaceKind::Plane {
    return false;
  }
  let n = base.normal;
  let mut walls = 0;
  for g in faces {
    let d = g.normal.dot(n).abs();
    match g.kind {
      SurfaceKind::Plane if d < 1e-6 => walls += 1,
      SurfaceKind::Plane if d > 1.0 - 1e-6 => {}
      SurfaceKind::Cylinder if d > 1.0 - 1e-6 => walls += 1,
      _ => return false,
    }
  }
  walls > 0
}

/// Are the faces all turned about one axis (round faces on it, flat ones square to it)? (Walls
/// straight up from the base are an extrude's, asked first.)
fn revolved(base: &FaceInfo, faces: &[&FaceInfo]) -> bool {
  let Some(lead) =
    faces.iter().find(|g| matches!(g.kind, SurfaceKind::Cylinder | SurfaceKind::Cone | SurfaceKind::Torus))
  else {
    // a dome: spheres about a point, turned about the base's normal there
    return base.kind == SurfaceKind::Plane
      && faces.iter().all(|g| g.kind == SurfaceKind::Sphere || g.kind == SurfaceKind::Plane)
      && faces.iter().any(|g| g.kind == SurfaceKind::Sphere);
  };
  let (o, a) = (lead.origin, lead.normal.normalize_or_zero());
  let on_axis = |p: DVec3| (p - o).cross(a).length() < 1e-6;
  faces.iter().all(|g| match g.kind {
    SurfaceKind::Plane => g.normal.dot(a).abs() > 1.0 - 1e-6,
    SurfaceKind::Cylinder | SurfaceKind::Cone | SurfaceKind::Torus => {
      g.normal.cross(a).length() < 1e-6 && on_axis(g.origin)
    }
    SurfaceKind::Sphere => on_axis(g.origin),
    SurfaceKind::Other => false,
  })
}

/// Fillets (round faces tangent to a neighbour: cylinders, and tori and spheres at corners) and
/// holes (concave cylinders meeting their neighbours at an angle; the halves of one hole are one).
fn rounds(s: &Session, b: BodyId) -> Result<Vec<Found>, CmdError> {
  let body = s.model.body(b).ok_or(CmdError::new(format!("no body {}", b.0)))?;
  let faces = body.faces();
  let edges = body.edges();
  let face = |id: u32| faces.iter().find(|f| f.id == id);
  let mut out: Vec<Found> = Vec::new();
  for f in faces {
    if !matches!(f.kind, SurfaceKind::Cylinder | SurfaceKind::Torus | SurfaceKind::Sphere) {
      continue;
    }
    let own: Vec<_> = edges.iter().filter(|e| e.faces.contains(&f.id) && e.kind != CurveKind::Degenerate).collect();
    let tangent = own.iter().any(|e| {
      let other = if e.faces[0] == f.id { e.faces[1] } else { e.faces[0] };
      match (face(other), normal_at(f, e.mid)) {
        (Some(g), Some(n)) if other != f.id => normal_at(g, e.mid).is_some_and(|m| n.dot(m).abs() > 1.0 - 1e-6),
        _ => false,
      }
    });
    if tangent || f.kind != SurfaceKind::Cylinder {
      // a torus or sphere is a corner blend only beside other rounds or flat faces it touches
      if tangent || f.kind == SurfaceKind::Torus {
        // (a torus's stored radius is its major one: the blend's size isn't known)
        let size = if f.kind == SurfaceKind::Torus { 0.0 } else { f.radius };
        out.push(Found { kind: "fillet", size, body: b, faces: vec![f.id] });
      }
      continue;
    }
    // concave: just inside the surface, towards the axis, is outside the material
    let Some(e) = own.first() else { continue };
    let Some(n) = normal_at(f, e.mid) else { continue };
    let inward = if (e.mid - f.origin).dot(n) > 0.0 { -n } else { n };
    let q = e.mid + inward * (f.radius * 0.05).min(0.05);
    if body.shape.classify(q).map_err(|e| CmdError::new(e.to_string()))? != Inside::Out {
      continue;
    }
    let same_hole = |h: &Found| {
      h.kind == "hole"
        && face(h.faces[0]).is_some_and(|g| {
          (g.radius - f.radius).abs() < 1e-6
            && g.normal.cross(f.normal).length() < 1e-6
            && (g.origin - f.origin).cross(f.normal).length() < 1e-6
        })
    };
    match out.iter_mut().find(|h| same_hole(h)) {
      Some(h) => h.faces.push(f.id),
      None => out.push(Found { kind: "hole", size: 2.0 * f.radius, body: b, faces: vec![f.id] }),
    }
  }
  Ok(out)
}

/// A face's normal direction (either sense) at a point on it.
fn normal_at(f: &FaceInfo, p: DVec3) -> Option<DVec3> {
  let n = match f.kind {
    SurfaceKind::Plane => f.normal,
    SurfaceKind::Cylinder => {
      let r = p - f.origin;
      r - f.normal * r.dot(f.normal)
    }
    SurfaceKind::Sphere => p - f.origin,
    _ => return None,
  };
  n.try_normalize()
}

/// Each body becomes a box or cylinder enclosing it (a Bounding Solid, standing `offset` off it),
/// and the body goes: one undo step.
fn replace_with_primitives(s: &mut Session, p: &Value) -> CmdResult {
  let bodies = bodies(s, p)?;
  let shape = match p.get("shape").and_then(Value::as_str).unwrap_or("box") {
    "box" => BoundShape::Box,
    "cylinder" => BoundShape::Cylinder,
    o => return Err(CmdError::new(format!("unknown shape '{o}' (box, cylinder)"))),
  };
  let offset = crate::commands::opt_expr(p, "offset").unwrap_or_else(|| "0".into());
  let active = s.active_component;
  let made = s.edit(|d| {
    let mut made = Vec::new();
    for b in &bodies {
      let kind = FeatureKind::BoundingSolid {
        bodies: BodySel::Bodies { ids: vec![*b] },
        shape,
        plane: None,
        offset: offset.clone(),
        sides: Vec::new(),
      };
      let id = d.add_feature(kind)?;
      crate::newcomp::attach(d, id, None, active)?;
      made.push(id);
    }
    let id = d.add_feature(FeatureKind::RemoveBodies { bodies: bodies.clone(), components: Vec::new() })?;
    crate::newcomp::attach(d, id, None, active)?;
    made.push(id);
    Ok(made)
  })?;
  for id in &made {
    if let Some(e) = s.model.status(*id).and_then(|x| x.error.clone()) {
      return Err(CmdError::new(e));
    }
  }
  Ok(json!({ "features": made.iter().map(|f| f.0).collect::<Vec<_>>() }))
}
