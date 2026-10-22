//! Simplify commands (Fusion's SOLID > MODIFY > Simplify): Remove Features finds fillets and
//! holes on bodies and removes them; Replace with Primitives swaps bodies for a box or cylinder
//! around each.

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind, Inside, SurfaceKind};
use fox_model::bounding::BoundShape;
use fox_model::{BodyId, BodySel, FeatureKind};
use serde_json::{Value, json};

use crate::commands::ids;
use crate::{CmdError, CmdResult, Session};

/// The feature types Remove Features looks for, as its Features group lists them. FissionOxide
/// recognises the first two.
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
  let faces: Vec<Value> = list.iter().flat_map(|f| f["faces"].as_array().cloned().unwrap_or_default()).collect();
  let mut r = crate::commands::run(s, "remove_faces", &json!({ "faces": faces }))?;
  r["removed"] = json!(list);
  Ok(r)
}

/// Fillets (round faces tangent to a neighbour: cylinders, and tori and spheres at corners) and
/// holes (concave cylinders meeting their neighbours at an angle; the halves of one hole are one).
fn recognise(s: &Session, b: BodyId) -> Result<Vec<Found>, CmdError> {
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
fn normal_at(f: &fox_kernel::FaceInfo, p: DVec3) -> Option<DVec3> {
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

/// Each body becomes a box or cylinder enclosing it (a Bounding Solid), and the body goes: one
/// undo step.
fn replace_with_primitives(s: &mut Session, p: &Value) -> CmdResult {
  let bodies = bodies(s, p)?;
  let shape = match p.get("shape").and_then(Value::as_str).unwrap_or("box") {
    "box" => BoundShape::Box,
    "cylinder" => BoundShape::Cylinder,
    o => return Err(CmdError::new(format!("unknown shape '{o}' (box, cylinder)"))),
  };
  let active = s.active_component;
  let made = s.edit(|d| {
    let mut made = Vec::new();
    for b in &bodies {
      let kind = FeatureKind::BoundingSolid {
        bodies: BodySel::Bodies { ids: vec![*b] },
        shape,
        plane: None,
        offset: "0".into(),
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
