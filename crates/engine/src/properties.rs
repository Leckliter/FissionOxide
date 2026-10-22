//! The Properties inspector: what each selected entity is and its geometry (`inspect.properties`),
//! and the overall width, height and thickness of bodies (`inspect.overall_size`), with JSON, CSV
//! and text exports.
//!
//! Lengths are in mm, areas in mm² (`*_mm2`), volumes in mm³ (`*_mm3`), masses in grams (`*_g`)
//! and angles in degrees (`*_deg`), so a reader can convert by the key's suffix.

use std::path::Path;

use fox_kernel::glam::{DMat4, DVec3};
use fox_kernel::{Shape, SurfaceKind};
use fox_model::units::LengthUnit;
use fox_model::{BodyId, ComponentId, FeatureId, FeatureKind};
use fox_sketch::{CurveKind, Sketch};
use serde_json::{Map, Value, json};

use crate::commands::{ids, opt_str};
use crate::{CmdError, CmdResult, Sel, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "inspect.properties" => properties(s, p),
    "inspect.overall_size" => overall_size(s, p),
    _ => return None,
  })
}

/// What `inspect.properties` can describe: a selection item, a component or a whole sketch.
enum Item {
  Sel(Sel),
  Component(ComponentId),
  Sketch(FeatureId),
}

/// A selection item (`{"type": "face", "body", "face"}`, …), `{"type": "component", "component"}`,
/// `{"type": "sketch", "sketch"}`, or the same without `type` (inferred from the keys).
fn item_of(v: &Value) -> Result<Item, CmdError> {
  let num = |k: &str| v.get(k).and_then(Value::as_u64).map(|n| n as u32);
  let kind = match v.get("type").and_then(Value::as_str) {
    Some(t) => t.to_string(),
    None => {
      let has = |k: &str| v.get(k).is_some();
      let t = if has("component") {
        "component"
      } else if has("face") {
        "face"
      } else if has("edge") && has("end") {
        "vertex"
      } else if has("edge") {
        "edge"
      } else if has("body") {
        "body"
      } else if has("sketch") && has("curve") {
        "sketch_curve"
      } else if has("sketch") && has("point") {
        "sketch_point"
      } else if has("sketch") && has("index") {
        "profile"
      } else if has("sketch") {
        "sketch"
      } else if has("feature") {
        "feature"
      } else {
        return Err(CmdError::new(format!("can't tell what {v} is")));
      };
      t.to_string()
    }
  };
  match kind.as_str() {
    "component" => Ok(Item::Component(ComponentId(num("component").ok_or(CmdError::new("missing 'component'"))?))),
    "sketch" => Ok(Item::Sketch(FeatureId(num("sketch").ok_or(CmdError::new("missing 'sketch'"))?))),
    t => {
      let mut o = v.as_object().cloned().unwrap_or_default();
      o.insert("type".into(), json!(t));
      serde_json::from_value::<Sel>(Value::Object(o))
        .map(Item::Sel)
        .map_err(|e| CmdError::new(format!("bad item {v}: {e}")))
    }
  }
}

/// `inspect.properties`: one record per item (default the selection), optionally written to a
/// `.json` or `.txt` file.
fn properties(s: &Session, p: &Value) -> CmdResult {
  let items: Vec<Item> = match p.get("items").and_then(Value::as_array) {
    Some(a) => a.iter().map(item_of).collect::<Result<_, _>>()?,
    None => s.selection.iter().map(|x| Item::Sel(*x)).collect(),
  };
  let records: Vec<Value> = items.iter().map(|i| describe(s, i)).collect::<Result<_, _>>()?;
  let mut out = json!({"count": records.len(), "items": records});
  if let Some(path) = opt_str(p, "export") {
    let path = Path::new(path);
    let text = match ext(path).as_str() {
      "json" => serde_json::to_string_pretty(&json!({"design": s.design.name, "items": out["items"]}))?,
      "txt" => properties_text(&out["items"]),
      e => return Err(CmdError::new(format!("can't export properties as '.{e}' (use .json or .txt)"))),
    };
    write(path, &text)?;
    out["path"] = json!(path);
  }
  Ok(out)
}

fn ext(path: &Path) -> String {
  path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn write(path: &Path, text: &str) -> Result<(), CmdError> {
  if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
    std::fs::create_dir_all(dir).map_err(|e| CmdError::new(format!("{}: {e}", dir.display())))?;
  }
  std::fs::write(path, text).map_err(|e| CmdError::new(format!("{}: {e}", path.display())))
}

fn v3(v: DVec3) -> Value {
  json!(v.to_array())
}

fn bbox_json(lo: DVec3, hi: DVec3) -> Value {
  json!({"min": v3(lo), "max": v3(hi), "size": v3(hi - lo)})
}

fn shape_box(shape: &Shape) -> Value {
  shape.mass().map_or(Value::Null, |m| bbox_json(m.bbox_min, m.bbox_max))
}

/// The component a body belongs to (the innermost one when sub-assemblies list it too).
fn component_of(s: &Session, body: BodyId) -> Option<&fox_model::Component> {
  s.design.components.iter().rev().find(|c| c.bodies.contains(&body))
}

fn component_json(s: &Session, body: BodyId) -> Value {
  component_of(s, body).map_or(Value::Null, |c| json!({"id": c.id.0, "name": c.name}))
}

/// Bodies of a component and of every component below it.
fn component_bodies(s: &Session, id: ComponentId) -> Vec<BodyId> {
  let mut out = Vec::new();
  let mut todo = vec![id];
  while let Some(c) = todo.pop() {
    if let Some(comp) = s.design.component(c) {
      for b in &comp.bodies {
        if !out.contains(b) {
          out.push(*b);
        }
      }
    }
    todo.extend(s.design.components.iter().filter(|x| x.parent == Some(c)).map(|x| x.id));
  }
  out
}

/// Grams per mm³ of a body's material: its own, else the one its component names.
fn density(s: &Session, body: BodyId) -> Option<(String, f64)> {
  s.design
    .body_settings(body)
    .and_then(|b| b.material.clone())
    .or_else(|| {
      s.design
        .components
        .iter()
        .find(|c| c.bodies.contains(&body) && !c.material.is_empty())
        .map(|c| c.material.clone())
    })
    .map(|m| {
      let d = fox_model::assembly::density(&m) / 1000.0;
      (m, d)
    })
}

fn describe(s: &Session, item: &Item) -> CmdResult {
  match item {
    Item::Component(c) => component(s, *c),
    Item::Sketch(sk) => sketch(s, *sk),
    Item::Sel(sel) => match *sel {
      Sel::Body { body } => body_props(s, body),
      Sel::Face { body, face } => face_props(s, body, face),
      Sel::Edge { body, edge } => edge_props(s, body, edge),
      Sel::Vertex { body, edge, end } => {
        let at = crate::scene::vertex_pos(s, body, edge, end).ok_or(CmdError::new("no such vertex"))?;
        Ok(json!({
          "type": "vertex", "id": {"body": body.0, "edge": edge, "end": end}, "body": s.body_name(body),
          "component": component_json(s, body), "point": v3(at),
        }))
      }
      Sel::SketchCurve { sketch, curve } => sketch_curve(s, sketch, curve),
      Sel::SketchPoint { sketch, point } => {
        let (sk, plane) = solved(s, sketch)?;
        let at = sk.pos(point).ok_or(CmdError::new(format!("sketch has no point {}", point.0)))?;
        let fixed = sk.point(point).is_some_and(|p| p.fixed);
        Ok(json!({
          "type": "sketch_point", "id": {"sketch": sketch.0, "point": point.0}, "sketch": feature_name(s, sketch),
          "point_2d": at, "point": plane.map(|pl| v3(pl.to_world(at))), "fixed": fixed,
        }))
      }
      Sel::Profile { sketch, index } => {
        let r = s.model.sketches.get(&sketch).ok_or(CmdError::new(format!("no sketch {}", sketch.0)))?;
        let pr = r.profiles.get(index).ok_or(CmdError::new(format!("sketch has no profile {index}")))?;
        let poly = pr.outer.polygon();
        let (lo, hi) = bounds2(poly.iter().copied());
        Ok(json!({
          "type": "profile", "id": {"sketch": sketch.0, "index": index}, "sketch": feature_name(s, sketch),
          "area_mm2": pr.area(), "holes": pr.holes.len(), "box_2d": {"min": lo, "max": hi},
        }))
      }
      Sel::SketchConstraint { sketch, constraint } => {
        let sk = s.design.sketch(sketch)?;
        let c = sk
          .constraints
          .iter()
          .find(|c| c.id == constraint)
          .ok_or(CmdError::new(format!("sketch has no constraint {}", constraint.0)))?;
        Ok(json!({
          "type": "sketch_constraint", "id": {"sketch": sketch.0, "constraint": constraint.0},
          "sketch": feature_name(s, sketch), "constraint": c,
        }))
      }
      Sel::Feature { feature } => {
        let f = s.design.feature(feature).ok_or(CmdError::new(format!("no feature {}", feature.0)))?;
        if matches!(f.kind, FeatureKind::Sketch { .. }) {
          return sketch(s, feature);
        }
        let st = s.model.status(feature);
        Ok(json!({
          "type": "feature", "id": feature.0, "name": f.name, "kind": f.kind.base_name(),
          "suppressed": f.suppressed, "error": st.and_then(|x| x.error.clone()),
        }))
      }
    },
  }
}

fn body_props(s: &Session, body: BodyId) -> CmdResult {
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let m = b.shape.mass()?;
  let kind = if b.is_mesh() {
    "mesh"
  } else if b.sheet.is_some() {
    "sheet_metal"
  } else if b.shape.solid_count() > 0 {
    "solid"
  } else {
    "surface"
  };
  let mut out = json!({
    "type": "body", "id": body.0, "name": s.body_name(body), "component": component_json(s, body),
    "kind": kind, "volume_mm3": m.volume, "area_mm2": m.area, "center_of_mass": v3(m.center_of_mass),
    "box": bbox_json(m.bbox_min, m.bbox_max), "faces": b.faces().len(), "edges": b.edges().len(),
    "visible": s.body_visible(body),
  });
  if let Some(sheet) = &b.sheet {
    out["thickness"] = json!(sheet.thickness);
  }
  if let Some((material, density)) = density(s, body) {
    out["material"] = json!(material);
    out["mass_g"] = json!(m.volume * density);
  }
  Ok(out)
}

fn face_props(s: &Session, body: BodyId, face: u32) -> CmdResult {
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let f = b.faces().iter().find(|f| f.id == face).ok_or(CmdError::new(format!("body has no face {face}")))?;
  let shape = b.shape.face_shape(face)?;
  let (at, normal) = if f.kind == SurfaceKind::Plane {
    (f.center, Some(f.normal))
  } else {
    // the point of the face nearest its centroid, and the surface normal there (from the mesh)
    let at = Shape::vertex(f.center).and_then(|v| shape.distance_to(&v)).map(|(_, p, _)| p).unwrap_or(f.center);
    (at, b.mesh().and_then(|m| mesh_normal(m, face, at)))
  };
  let mut out = json!({
    "type": "face", "id": {"body": body.0, "face": face}, "body": s.body_name(body),
    "component": component_json(s, body), "surface": format!("{:?}", f.kind).to_lowercase(), "area_mm2": f.area,
    "center": v3(at), "normal": normal.map(v3), "box": shape_box(&shape),
  });
  if f.radius > 0.0 {
    out["radius"] = json!(f.radius);
    out["diameter"] = json!(2.0 * f.radius);
  }
  if matches!(f.kind, SurfaceKind::Cylinder | SurfaceKind::Cone) {
    out["axis"] = json!({"origin": v3(f.origin), "direction": v3(f.normal)});
  }
  Ok(out)
}

/// The normal of the face's mesh triangle nearest `at` (vertex normals averaged).
fn mesh_normal(m: &fox_kernel::TriMesh, face: u32, at: DVec3) -> Option<DVec3> {
  let p = |i: u32| DVec3::from(m.positions[i as usize].map(f64::from));
  let n = |i: u32| DVec3::from(m.normals.get(i as usize).copied().unwrap_or_default().map(f64::from));
  m.indices
    .chunks_exact(3)
    .zip(&m.tri_face)
    .filter(|(_, f)| **f == face)
    .map(|(t, _)| {
      let c = (p(t[0]) + p(t[1]) + p(t[2])) / 3.0;
      (c.distance_squared(at), t)
    })
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .and_then(|(_, t)| {
      let avg = n(t[0]) + n(t[1]) + n(t[2]);
      avg.try_normalize().or_else(|| (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).try_normalize())
    })
}

fn edge_props(s: &Session, body: BodyId, edge: u32) -> CmdResult {
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let e = b.edges().iter().find(|e| e.id == edge).ok_or(CmdError::new(format!("body has no edge {edge}")))?;
  let mut out = json!({
    "type": "edge", "id": {"body": body.0, "edge": edge}, "body": s.body_name(body),
    "component": component_json(s, body), "curve": format!("{:?}", e.kind).to_lowercase(), "length": e.length,
    "start": v3(e.start), "end": v3(e.end), "mid": v3(e.mid),
    "box": b.shape.edge_shape(edge).map_or(Value::Null, |sh| shape_box(&sh)),
  });
  if e.radius > 0.0 {
    out["radius"] = json!(e.radius);
    out["diameter"] = json!(2.0 * e.radius);
    out["center"] = v3(e.center);
    out["axis"] = v3(e.axis);
  }
  Ok(out)
}

fn component(s: &Session, id: ComponentId) -> CmdResult {
  let c = s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
  let bodies = component_bodies(s, id);
  let mut volume = 0.0;
  let mut area = 0.0;
  let mut mass = 0.0;
  let mut bounds: Option<(DVec3, DVec3)> = None;
  for b in bodies.iter().filter_map(|b| s.model.body(*b)) {
    let Ok(m) = b.shape.mass() else { continue };
    volume += m.volume;
    area += m.area;
    mass += m.volume * crate::assembly::density_of(s, b.id);
    bounds = Some(bounds.map_or((m.bbox_min, m.bbox_max), |(lo, hi)| (lo.min(m.bbox_min), hi.max(m.bbox_max))));
  }
  let parent = c.parent.and_then(|p| s.design.component(p)).map(|p| json!({"id": p.id.0, "name": p.name}));
  let mut out = json!({
    "type": "component", "id": id.0, "name": c.name, "parent": parent,
    "bodies": bodies.iter().map(|b| b.0).collect::<Vec<_>>(),
    "sheet_metal": c.sheet_metal, "volume_mm3": volume, "area_mm2": area,
    "box": bounds.map_or(Value::Null, |(lo, hi)| bbox_json(lo, hi)),
  });
  if !c.material.is_empty() {
    out["material"] = json!(c.material);
    out["mass_g"] = json!(mass);
  }
  if !c.part_number.is_empty() {
    out["part_number"] = json!(c.part_number);
  }
  Ok(out)
}

fn feature_name(s: &Session, id: FeatureId) -> String {
  s.design.feature(id).map(|f| f.name.clone()).unwrap_or_else(|| format!("Sketch {}", id.0))
}

/// The solved sketch and its plane (`None` before it has been computed).
fn solved(s: &Session, id: FeatureId) -> Result<(&Sketch, Option<fox_kernel::Plane>), CmdError> {
  match s.model.sketches.get(&id) {
    Some(r) => Ok((&r.sketch, Some(r.plane))),
    None => Ok((s.design.sketch(id)?, None)),
  }
}

fn plane_json(pl: Option<fox_kernel::Plane>) -> Value {
  pl.map_or(Value::Null, |pl| json!({"origin": v3(pl.origin), "x": v3(pl.x), "y": v3(pl.y), "normal": v3(pl.normal())}))
}

fn bounds2(pts: impl Iterator<Item = [f64; 2]>) -> ([f64; 2], [f64; 2]) {
  pts.fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), q| {
    ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])
  })
}

fn sketch(s: &Session, id: FeatureId) -> CmdResult {
  let (sk, plane) = solved(s, id)?;
  let pts: Vec<[f64; 2]> = sk.curves.iter().flat_map(|c| crate::scene::curve_polyline(sk, &c.kind)).collect();
  let bbox = (!pts.is_empty()).then(|| {
    let (lo, hi) = bounds2(pts.into_iter());
    json!({"min": lo, "max": hi, "size": [hi[0] - lo[0], hi[1] - lo[1]]})
  });
  let profiles = s.model.sketches.get(&id).map(|r| r.profiles.len());
  Ok(json!({
    "type": "sketch", "id": id.0, "name": feature_name(s, id), "plane": plane_json(plane),
    "curves": sk.curves.len(), "points": sk.points.len(), "constraints": sk.constraints.len(),
    "profiles": profiles, "box_2d": bbox,
  }))
}

fn sketch_curve(s: &Session, sketch: FeatureId, curve: fox_sketch::CurveId) -> CmdResult {
  let (sk, plane) = solved(s, sketch)?;
  let c = sk.curve(curve).ok_or(CmdError::new(format!("sketch has no curve {}", curve.0)))?;
  let kind = serde_json::to_value(&c.kind)?.get("type").and_then(Value::as_str).unwrap_or("curve").to_string();
  let world = |q: [f64; 2]| plane.map(|pl| v3(pl.to_world(q)));
  let poly = crate::scene::curve_polyline(sk, &c.kind);
  let mut out = Map::new();
  out.insert("type".into(), json!("sketch_curve"));
  out.insert("id".into(), json!({"sketch": sketch.0, "curve": curve.0}));
  out.insert("sketch".into(), json!(feature_name(s, sketch)));
  out.insert("curve".into(), json!(kind));
  out.insert("construction".into(), json!(c.construction));
  out.insert("centerline".into(), json!(c.centerline));
  let mut put_point = |key: &str, q: Option<[f64; 2]>| {
    if let Some(q) = q {
      out.insert(format!("{key}_2d"), json!(q));
      out.insert(key.into(), world(q).unwrap_or(Value::Null));
    }
  };
  let length = match &c.kind {
    CurveKind::Line { a, b } => {
      let (pa, pb) = (sk.pos(*a), sk.pos(*b));
      put_point("start", pa);
      put_point("end", pb);
      pa.zip(pb).map(|(pa, pb)| (pb[0] - pa[0]).hypot(pb[1] - pa[1]))
    }
    CurveKind::Circle { center, radius } => {
      put_point("center", sk.pos(*center));
      out.insert("radius".into(), json!(radius));
      out.insert("diameter".into(), json!(2.0 * radius));
      Some(std::f64::consts::TAU * radius)
    }
    CurveKind::Arc { center, start, end } => {
      let (c0, a, b) = (sk.pos(*center), sk.pos(*start), sk.pos(*end));
      put_point("start", a);
      put_point("end", b);
      put_point("center", c0);
      match (c0, a, b) {
        (Some(c0), Some(a), Some(b)) => {
          let r = (a[0] - c0[0]).hypot(a[1] - c0[1]);
          let mut sweep = (b[1] - c0[1]).atan2(b[0] - c0[0]) - (a[1] - c0[1]).atan2(a[0] - c0[0]);
          if sweep <= 0.0 {
            sweep += std::f64::consts::TAU;
          }
          out.insert("radius".into(), json!(r));
          out.insert("sweep_deg".into(), json!(sweep.to_degrees()));
          Some(r * sweep)
        }
        _ => None,
      }
    }
    _ => {
      put_point("start", poly.first().copied());
      put_point("end", poly.last().copied());
      None
    }
  };
  let length = length.unwrap_or_else(|| poly.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum());
  out.insert("length".into(), json!(length));
  out.insert("sketch_plane".into(), plane_json(plane));
  Ok(Value::Object(out))
}

/// The records as readable text: one block per item, a `key: value` line per property.
pub fn properties_text(items: &Value) -> String {
  let mut out = String::new();
  for (i, item) in items.as_array().into_iter().flatten().enumerate() {
    if i > 0 {
      out.push('\n');
    }
    out.push_str(&format!("{} {}\n", item["type"].as_str().unwrap_or("item"), scalar(&item["id"])));
    if let Some(o) = item.as_object() {
      for (k, v) in o.iter().filter(|(k, _)| *k != "type" && *k != "id") {
        lines(&mut out, k, v, 1);
      }
    }
  }
  out
}

fn lines(out: &mut String, key: &str, v: &Value, depth: usize) {
  let pad = "  ".repeat(depth);
  match v {
    Value::Object(o) if !o.is_empty() => {
      out.push_str(&format!("{pad}{key}:\n"));
      for (k, x) in o {
        lines(out, k, x, depth + 1);
      }
    }
    _ => out.push_str(&format!("{pad}{key}: {}\n", scalar(v))),
  }
}

/// A value on one line: numbers to 6 decimals, points as `(x, y, z)`, small objects as `k=v`.
fn scalar(v: &Value) -> String {
  match v {
    Value::Null => "-".into(),
    Value::Number(n) if n.is_f64() => {
      let s = format!("{:.6}", n.as_f64().unwrap_or(0.0));
      let s = s.trim_end_matches('0').trim_end_matches('.');
      if s == "-0" { "0".into() } else { s.into() }
    }
    Value::String(s) => s.clone(),
    Value::Array(a) => format!("({})", a.iter().map(scalar).collect::<Vec<_>>().join(", ")),
    Value::Object(o) => o.iter().map(|(k, x)| format!("{k}={}", scalar(x))).collect::<Vec<_>>().join(" "),
    v => v.to_string(),
  }
}

// ---------------------------------------------------------------- overall size

/// `inspect.overall_size`: overall width, height and thickness of bodies (default every visible
/// one). With a planar face among them, measured in the frame of the largest one: thickness along
/// its normal, width along world X laid onto it (world Y when X is along the normal), height across
/// both. Otherwise the box extents, largest first.
fn overall_size(s: &Session, p: &Value) -> CmdResult {
  let mut bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  for c in ids(p, "components") {
    if s.design.component(ComponentId(c)).is_none() {
      return Err(CmdError::new(format!("no component {c}")));
    }
    for b in component_bodies(s, ComponentId(c)) {
      if !bodies.contains(&b) {
        bodies.push(b);
      }
    }
  }
  let explicit = p.get("bodies").is_some() || p.get("components").is_some();
  if !explicit {
    bodies = s.model.bodies.iter().filter(|b| s.body_visible(b.id)).map(|b| b.id).collect();
  }
  let list: Vec<&fox_model::regen::Body> = bodies
    .iter()
    .map(|b| s.model.body(*b).ok_or(CmdError::new(format!("no body {}", b.0))))
    .collect::<Result<_, _>>()?;
  if list.is_empty() {
    return Err(CmdError::new("there are no bodies to measure"));
  }
  let all = Shape::compound(list.iter().map(|b| &b.shape))?;
  let m = all.mass()?;
  let world_box = bbox_json(m.bbox_min, m.bbox_max);
  let largest = list
    .iter()
    .flat_map(|b| b.faces().iter().filter(|f| f.kind == SurfaceKind::Plane).map(move |f| (b.id, f)))
    .max_by(|a, b| a.1.area.total_cmp(&b.1.area));
  let mut out = match largest {
    Some((body, f)) => {
      let n = f.normal.normalize();
      let onto = |v: DVec3| (v - n * v.dot(n)).try_normalize();
      let x = onto(DVec3::X).filter(|_| n.cross(DVec3::X).length() > 1e-6).or_else(|| onto(DVec3::Y)).unwrap_or(f.xdir);
      let y = n.cross(x);
      let frame = DMat4::from_cols(x.extend(0.0), y.extend(0.0), n.extend(0.0), f.origin.extend(1.0));
      let local = all.transformed(frame.inverse())?.mass()?;
      let size = local.bbox_max - local.bbox_min;
      json!({
        "mode": "planar", "width": size.x, "height": size.y, "thickness": size.z,
        "frame": {"face": {"body": body.0, "face": f.id}, "origin": v3(f.origin), "x": v3(x), "y": v3(y), "normal": v3(n)},
      })
    }
    None => {
      let size = m.bbox_max - m.bbox_min;
      let mut e = [size.x, size.y, size.z];
      e.sort_by(|a, b| b.total_cmp(a));
      json!({"mode": "box", "width": e[0], "height": e[1], "thickness": e[2]})
    }
  };
  out["box"] = world_box;
  out["bodies"] = json!(list.iter().map(|b| b.id.0).collect::<Vec<_>>());
  if let Some(path) = opt_str(p, "export") {
    let units = match opt_str(p, "units") {
      Some(u) => LengthUnit::parse(u).ok_or(CmdError::new(format!("unknown units '{u}' (mm, cm, m, in, ft)")))?,
      None => s.design.units,
    };
    let path = Path::new(path);
    write(path, &size_export(&out, units, &s.design.name, &ext(path))?)?;
    out["path"] = json!(path);
  }
  Ok(out)
}

/// An overall size result as a `.json`, `.csv` or `.txt` file in `units`.
fn size_export(r: &Value, units: LengthUnit, design: &str, ext: &str) -> Result<String, CmdError> {
  let u = |k: &str| units.from_mm(r[k].as_f64().unwrap_or(0.0));
  let vec_u = |v: &Value| -> Vec<f64> {
    v.as_array().into_iter().flatten().map(|x| units.from_mm(x.as_f64().unwrap_or(0.0))).collect()
  };
  let (w, h, t) = (u("width"), u("height"), u("thickness"));
  let mode = r["mode"].as_str().unwrap_or("box");
  let sym = units.symbol();
  Ok(match ext {
    "json" => serde_json::to_string_pretty(&json!({
      "design": design, "units": sym, "width": w, "height": h, "thickness": t, "mode": mode,
      "box": {"min": vec_u(&r["box"]["min"]), "max": vec_u(&r["box"]["max"]), "size": vec_u(&r["box"]["size"])},
      "bodies": r["bodies"],
    }))?,
    "csv" => format!(
      "design,width,height,thickness,units,mode\n\"{}\",{w},{h},{t},{sym},{mode}\n",
      design.replace('"', "\"\"")
    ),
    "txt" => format!(
      "{design}\nWidth: {} {sym}\nHeight: {} {sym}\nThickness: {} {sym}\nMode: {mode}\n",
      units.number(r["width"].as_f64().unwrap_or(0.0)),
      units.number(r["height"].as_f64().unwrap_or(0.0)),
      units.number(r["thickness"].as_f64().unwrap_or(0.0)),
    ),
    e => return Err(CmdError::new(format!("can't export the overall size as '.{e}' (use .json, .csv or .txt)"))),
  })
}
