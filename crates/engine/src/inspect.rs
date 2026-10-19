//! Inspect commands: measuring between entities, selecting everything, and centre of mass.

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind as EdgeKind, Shape, SurfaceKind};
use fox_model::BodyId;
use serde_json::{Value, json};

use crate::commands::{edge_at, face_at, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Sel, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "measure" => measure(s, p),
    "select.all" => select_all(s, p),
    "select.invert" => select_invert(s),
    "inspect.center_of_mass" => center_of_mass(s, p),
    _ => return None,
  })
}

/// What a measured entity is: its shape plus a direction (planar face normal or line) and a
/// display name.
struct Entity {
  shape: Shape,
  dir: Option<DVec3>,
  /// Planar face or straight edge: angles between two such are meaningful.
  kind: &'static str,
  props: Value,
}

/// `{body}`, `{body, face}`, `{body, edge}`, `{point: [x,y,z]}` (a point in space), or a
/// selection item (`{"type": "face", ...}`).
fn entity(s: &Session, v: &Value) -> Result<Entity, CmdError> {
  if let Some(sel) = v.get("type").and_then(Value::as_str) {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
      o.remove("type");
    }
    return match sel {
      "body" | "face" | "edge" => entity(s, &v),
      o => Err(CmdError::new(format!("can't measure a {o}"))),
    };
  }
  if v.get("point").is_some() && v.get("body").is_none() {
    let at = DVec3::from(req_vec3(v, "point")?);
    let snap = v.get("snap").and_then(Value::as_str);
    // a point on a face or edge measures to that face or edge
    if snap == Some("face")
      && let Ok((body, face)) = face_at(s, at, None)
    {
      return entity(s, &json!({"body": body.0, "face": face}));
    }
    if snap == Some("edge")
      && let Ok((body, edge)) = edge_at(s, at)
    {
      return entity(s, &json!({"body": body.0, "edge": edge}));
    }
    return Ok(Entity {
      shape: Shape::vertex(at)?,
      dir: None,
      kind: "point",
      props: json!({"kind": "point", "at": at.to_array()}),
    });
  }
  let body = BodyId(req_u32(v, "body")?);
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  if v.get("face").is_some() {
    let face = req_u32(v, "face")?;
    let f = b.faces().iter().find(|f| f.id == face).ok_or(CmdError::new(format!("body has no face {face}")))?;
    let planar = f.kind == SurfaceKind::Plane;
    let mut props = json!({
      "kind": "face", "surface": format!("{:?}", f.kind).to_lowercase(), "area_mm2": f.area,
      "center": f.center.to_array(),
    });
    if planar {
      props["normal"] = json!(f.normal.to_array());
    }
    if f.radius > 0.0 {
      props["radius"] = json!(f.radius);
      props["diameter"] = json!(2.0 * f.radius);
    }
    return Ok(Entity {
      shape: b.shape.face_shape(face)?,
      dir: planar.then_some(f.normal),
      kind: if planar { "plane" } else { "face" },
      props,
    });
  }
  if v.get("edge").is_some() {
    let edge = req_u32(v, "edge")?;
    let e = b.edges().iter().find(|e| e.id == edge).ok_or(CmdError::new(format!("body has no edge {edge}")))?;
    let line = e.kind == EdgeKind::Line;
    let mut props = json!({
      "kind": "edge", "curve": format!("{:?}", e.kind).to_lowercase(), "length": e.length,
      "start": e.start.to_array(), "end": e.end.to_array(),
    });
    if e.kind == EdgeKind::Circle {
      props["radius"] = json!(e.radius);
      props["diameter"] = json!(2.0 * e.radius);
      props["center"] = json!(e.center.to_array());
    }
    return Ok(Entity {
      shape: b.shape.edge_shape(edge)?,
      dir: if line { (e.end - e.start).try_normalize() } else { None },
      kind: if line { "line" } else { "edge" },
      props,
    });
  }
  let m = b.shape.mass()?;
  Ok(Entity {
    shape: b.shape.clone(),
    dir: None,
    kind: "body",
    props: json!({
      "kind": "body", "name": s.body_name(body), "volume_mm3": m.volume, "area_mm2": m.area,
      "center_of_mass": m.center_of_mass.to_array(),
      "size": (m.bbox_max - m.bbox_min).to_array(),
    }),
  })
}

/// Measures one entity (its properties) or two (minimum distance, closest points, angle).
fn measure(s: &Session, p: &Value) -> CmdResult {
  let mut list: Vec<Value> = Vec::new();
  if let Some(a) = p.get("a") {
    list.push(a.clone());
    if let Some(b) = p.get("b") {
      list.push(b.clone());
    }
  } else if let Some(items) = p.get("items").and_then(Value::as_array) {
    list.extend(items.iter().cloned());
  } else {
    list.extend(s.selection.iter().filter_map(|x| serde_json::to_value(x).ok()));
  }
  match list.as_slice() {
    [] => Err(CmdError::new("measure: pass 'a' (and 'b'), or select one or two entities")),
    [a] => Ok(entity(s, a)?.props),
    [a, b, ..] => {
      let (ea, eb) = (entity(s, a)?, entity(s, b)?);
      let (d, pa, pb) = ea.shape.distance_to(&eb.shape)?;
      let mut out = json!({
        "distance": d,
        "from": pa.to_array(),
        "to": pb.to_array(),
        "delta": (pb - pa).to_array(),
        "a": ea.props,
        "b": eb.props,
      });
      if let (Some(da), Some(db)) = (ea.dir, eb.dir) {
        let cos = da.dot(db).clamp(-1.0, 1.0);
        let angle = match (ea.kind, eb.kind) {
          // between two lines, or a line and a plane, the acute angle
          ("line", "line") => cos.abs().acos().to_degrees(),
          ("line", "plane") | ("plane", "line") => 90.0 - cos.abs().acos().to_degrees(),
          _ => cos.acos().to_degrees(),
        };
        out["angle_deg"] = json!(angle);
      }
      Ok(out)
    }
  }
}

/// Selects every body (`kind`: bodies, the default), every face or edge of the visible bodies,
/// or (while a sketch is edited) every curve of the sketch.
fn select_all(s: &mut Session, p: &Value) -> CmdResult {
  let kind = p.get("kind").and_then(Value::as_str);
  let mut out = Vec::new();
  if let (Some(sk), None | Some("sketch")) = (s.edit_sketch, kind) {
    for c in &s.design.sketch(sk)?.curves {
      out.push(Sel::SketchCurve { sketch: sk, curve: c.id });
    }
  } else {
    for b in s.model.bodies.iter().filter(|b| s.body_visible(b.id)) {
      match kind.unwrap_or("bodies") {
        "bodies" | "body" => out.push(Sel::Body { body: b.id }),
        "faces" | "face" => out.extend(b.faces().iter().map(|f| Sel::Face { body: b.id, face: f.id })),
        "edges" | "edge" => out.extend(b.edges().iter().map(|e| Sel::Edge { body: b.id, edge: e.id })),
        o => return Err(CmdError::new(format!("unknown kind '{o}' (bodies, faces, edges, sketch)"))),
      }
    }
  }
  s.selection = out;
  Ok(json!({"selected": s.selection.len()}))
}

/// Selects what isn't selected, among things of the kinds currently selected (bodies by default).
fn select_invert(s: &mut Session) -> CmdResult {
  let has = |f: fn(&Sel) -> bool| s.selection.iter().any(f);
  let kind = if has(|x| matches!(x, Sel::SketchCurve { .. })) {
    "sketch"
  } else if has(|x| matches!(x, Sel::Face { .. })) {
    "faces"
  } else if has(|x| matches!(x, Sel::Edge { .. })) {
    "edges"
  } else if s.edit_sketch.is_some() && s.selection.is_empty() {
    "sketch"
  } else {
    "bodies"
  };
  let before = std::mem::take(&mut s.selection);
  select_all(s, &json!({"kind": kind}))?;
  s.selection.retain(|x| !before.contains(x));
  Ok(json!({"selected": s.selection.len()}))
}

/// Centre of mass of the given bodies (default all visible), with total volume and mass.
fn center_of_mass(s: &Session, p: &Value) -> CmdResult {
  let want = crate::commands::ids(p, "bodies");
  let mut volume = 0.0;
  let mut mass = 0.0;
  let mut moment = DVec3::ZERO;
  let mut used = Vec::new();
  for b in &s.model.bodies {
    if !(want.is_empty() && s.body_visible(b.id) || want.contains(&b.id.0)) {
      continue;
    }
    let m = b.shape.mass()?;
    let density = crate::assembly::density_of(s, b.id);
    let w = m.volume * density;
    volume += m.volume;
    mass += w;
    moment += m.center_of_mass * w;
    used.push(b.id.0);
  }
  if used.is_empty() || mass <= 0.0 {
    return Err(CmdError::new("there are no bodies to weigh"));
  }
  Ok(json!({
    "center": (moment / mass).to_array(),
    "volume_mm3": volume,
    "mass_g": mass,
    "bodies": used,
  }))
}
