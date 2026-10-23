//! Inspect commands: measuring between entities, selecting everything, and centre of mass.

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind as EdgeKind, Shape, SurfaceKind};
use fox_model::BodyId;
use serde_json::{Value, json};

use crate::commands::{edge_at, expr_of, face_at, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Sel, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "measure" => measure(s, p),
    "select.all" => select_all(s, p),
    "select.invert" => select_invert(s),
    "select.by_name" => select_by_name(s, p),
    "select.by_size" => select_by_size(s, p),
    "select.by_boundary" => select_by_boundary(s, p),
    "select.seed_boundary" => select_seed_boundary(s, p),
    "inspect.center_of_mass" => center_of_mass(s, p),
    "body.classify" => classify(s, p),
    _ => return None,
  })
}

/// `body.classify`: in, out or on for each point.
fn classify(s: &Session, p: &Value) -> CmdResult {
  let id = BodyId(req_u32(p, "body")?);
  let body = s.model.body(id).ok_or_else(|| CmdError::new(format!("no body {}", id.0)))?;
  let points = p.get("points").and_then(Value::as_array).ok_or_else(|| CmdError::new("'points' is required"))?;
  let mut out = Vec::with_capacity(points.len());
  for (i, q) in points.iter().enumerate() {
    let xyz =
      q.as_array().filter(|a| a.len() == 3).and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?]));
    let Some([x, y, z]) = xyz else {
      return Err(CmdError::new(format!("point {i} is not [x, y, z]")));
    };
    let w = body.shape.classify(DVec3::new(x, y, z)).map_err(|e| CmdError::new(e.to_string()))?;
    out.push(match w {
      fox_kernel::Inside::In => "in",
      fox_kernel::Inside::Out => "out",
      fox_kernel::Inside::On => "on",
    });
  }
  Ok(json!({ "inside": out }))
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
/// selection item (`{"type": "face", ...}`, a sketch curve or sketch point too).
fn entity(s: &Session, v: &Value) -> Result<Entity, CmdError> {
  if let Some(sel) = v.get("type").and_then(Value::as_str) {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
      o.remove("type");
    }
    return match sel {
      "body" | "face" | "edge" => entity(s, &v),
      "vertex" => {
        let end = v.get("end").and_then(Value::as_bool).unwrap_or(false);
        let at = crate::scene::vertex_pos(s, BodyId(req_u32(&v, "body")?), req_u32(&v, "edge")?, end)
          .ok_or(CmdError::new("no such vertex"))?;
        entity(s, &json!({"point": at.to_array()}))
      }
      "sketch_curve" => sketch_curve_entity(s, &v),
      "sketch_point" => {
        let sketch = fox_model::FeatureId(req_u32(&v, "sketch")?);
        let (sk, plane) = crate::properties::solved(s, sketch)?;
        let q = sk.pos(fox_sketch::PointId(req_u32(&v, "point")?)).ok_or(CmdError::new("no such sketch point"))?;
        let plane = plane.ok_or(CmdError::new("the sketch has no plane"))?;
        entity(s, &json!({"point": plane.to_world(q).to_array()}))
      }
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
    // Fusion's Loop Length: the length of the face's boundary, every loop's edges once
    let loop_length: f64 = b.edges().iter().filter(|e| e.faces.contains(&face)).map(|e| e.length).sum();
    let mut props = json!({
      "kind": "face", "surface": format!("{:?}", f.kind).to_lowercase(), "area_mm2": f.area,
      "loop_length": loop_length, "center": f.center.to_array(),
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

/// A sketch curve, measured as an edge in space: its length (and a circle's or arc's radius),
/// and its direction when it is a line.
fn sketch_curve_entity(s: &Session, v: &Value) -> Result<Entity, CmdError> {
  let sketch = fox_model::FeatureId(req_u32(v, "sketch")?);
  let curve = fox_sketch::CurveId(req_u32(v, "curve")?);
  let (sk, plane) = crate::properties::solved(s, sketch)?;
  let plane = plane.ok_or(CmdError::new("the sketch has no plane"))?;
  let segs = sk.curve_segs(curve);
  if segs.is_empty() {
    return Err(CmdError::new(format!("sketch has no curve {}", curve.0)));
  }
  let info = crate::properties::sketch_curve(s, sketch, curve)?;
  let kind = info.get("curve").and_then(Value::as_str).unwrap_or("curve");
  let line = kind == "line";
  let mut props = json!({"kind": "edge", "curve": kind, "length": info["length"]});
  for k in ["start", "end", "center", "radius", "diameter"] {
    if let Some(x) = info.get(k) {
      props[k] = x.clone();
    }
  }
  let at = |k: &str| req_vec3(&info, k).ok().map(DVec3::from);
  let dir = if line { at("start").zip(at("end")).and_then(|(a, b)| (b - a).try_normalize()) } else { None };
  Ok(Entity {
    shape: Shape::path(&plane, &segs)?,
    dir,
    kind: if line { "line" } else { "edge" },
    props,
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

// ---------------------------------------------------------------- SELECT > Selection Tools

/// Does `name` match the wildcard `pattern` (`*` any run of characters, `?` any one), ignoring
/// case? A pattern without wildcards matches the whole name.
pub fn name_matches(pattern: &str, name: &str) -> bool {
  fn go(p: &[char], n: &[char]) -> bool {
    match p.split_first() {
      None => n.is_empty(),
      Some(('*', rest)) => (0..=n.len()).any(|i| go(rest, &n[i..])),
      Some(('?', rest)) => !n.is_empty() && go(rest, &n[1..]),
      Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
    }
  }
  let p: Vec<char> = pattern.trim().to_lowercase().chars().collect();
  let n: Vec<char> = name.to_lowercase().chars().collect();
  go(&p, &n)
}

/// `select.by_name`: the visible bodies, and the bodies of the components, whose name matches
/// the pattern (`*` and `?` wildcards, case ignored).
fn select_by_name(s: &mut Session, p: &Value) -> CmdResult {
  let pattern = p
    .get("name")
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|t| !t.is_empty())
    .ok_or_else(|| CmdError::new("'name' is required"))?;
  let kind = p.get("kind").and_then(Value::as_str).unwrap_or("all");
  if !matches!(kind, "all" | "bodies" | "components") {
    return Err(CmdError::new(format!("unknown kind '{kind}' (all, bodies, components)")));
  }
  let mut out: Vec<Sel> = Vec::new();
  if kind != "components" {
    for b in s.model.bodies.iter().filter(|b| s.body_visible(b.id)) {
      if name_matches(pattern, &s.body_name(b.id)) {
        out.push(Sel::Body { body: b.id });
      }
    }
  }
  if kind != "bodies" {
    for c in s.design.components.iter().filter(|c| name_matches(pattern, &c.name)) {
      for b in c.bodies.iter().filter(|b| s.model.body(**b).is_some() && s.body_visible(**b)) {
        let sel = Sel::Body { body: *b };
        if !out.contains(&sel) {
          out.push(sel);
        }
      }
    }
  }
  s.selection = out;
  Ok(json!({"selected": s.selection.len()}))
}

/// A size parameter: a number (mm) or an expression ("0.5 in", "width/2").
fn size_of(s: &Session, p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  let Some(v) = p.get(k).filter(|v| !v.is_null()) else { return Ok(None) };
  Ok(Some(fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, &s.model.params)?))
}

/// `select.by_size`: the visible bodies (by the diagonal of their bounding box) or faces (by the
/// square root of their area) no bigger than `max` and no smaller than `min`.
fn select_by_size(s: &mut Session, p: &Value) -> CmdResult {
  let kind = p.get("kind").and_then(Value::as_str).unwrap_or("bodies");
  let max = size_of(s, p, "max")?.ok_or_else(|| CmdError::new("'max' is required"))?;
  let min = size_of(s, p, "min")?.unwrap_or(0.0);
  let fits = |size: f64| size >= min && size <= max;
  let mut out = Vec::new();
  for b in s.model.bodies.iter().filter(|b| s.body_visible(b.id)) {
    match kind {
      "bodies" | "body" => {
        if let Ok(m) = b.shape.mass()
          && fits((m.bbox_max - m.bbox_min).length())
        {
          out.push(Sel::Body { body: b.id });
        }
      }
      "faces" | "face" => out.extend(
        b.faces().iter().filter(|f| fits(f.area.max(0.0).sqrt())).map(|f| Sel::Face { body: b.id, face: f.id }),
      ),
      o => return Err(CmdError::new(format!("unknown kind '{o}' (bodies, faces)"))),
    }
  }
  s.selection = out;
  Ok(json!({"selected": s.selection.len()}))
}

/// The `{body, edge}` entries of `k`, else the selected edges.
fn boundary_edges(s: &Session, p: &Value, k: &str) -> Vec<(BodyId, u32)> {
  let given: Vec<(BodyId, u32)> = p
    .get(k)
    .and_then(Value::as_array)
    .map(|a| {
      a.iter()
        .filter_map(|e| Some((BodyId(e.get("body")?.as_u64()? as u32), e.get("edge")?.as_u64()? as u32)))
        .collect()
    })
    .unwrap_or_default();
  if !given.is_empty() {
    return given;
  }
  s.selection
    .iter()
    .filter_map(|x| match x {
      Sel::Edge { body, edge } => Some((*body, *edge)),
      _ => None,
    })
    .collect()
}

/// The faces of `body` reachable from `seed` without crossing a boundary edge.
fn flood_faces(s: &Session, body: BodyId, seed: u32, boundary: &[u32]) -> Vec<u32> {
  let Some(b) = s.model.body(body) else { return Vec::new() };
  let mut out = vec![seed];
  let mut i = 0;
  while i < out.len() {
    let f = out[i];
    for e in b.edges().iter().filter(|e| !boundary.contains(&e.id) && e.faces.contains(&f)) {
      for g in e.faces {
        if g != 0 && !out.contains(&g) {
          out.push(g);
        }
      }
    }
    i += 1;
  }
  out
}

/// `select.by_boundary`: the faces enclosed by a closed chain of edges of one body (the smaller
/// of the two sides, by face count then by area); `edges` default to the selected edges.
fn select_by_boundary(s: &mut Session, p: &Value) -> CmdResult {
  let edges = boundary_edges(s, p, "edges");
  let Some(&(body, first)) = edges.first() else {
    return Err(CmdError::new("select the boundary edges first"));
  };
  if edges.iter().any(|(b, _)| *b != body) {
    return Err(CmdError::new("the boundary edges must belong to one body"));
  }
  let ids: Vec<u32> = edges.iter().map(|(_, e)| *e).collect();
  let b = s.model.body(body).ok_or_else(|| CmdError::new(format!("no body {}", body.0)))?;
  let edge = b.edges().iter().find(|e| e.id == first).ok_or_else(|| CmdError::new("no such edge"))?;
  let sides: Vec<Vec<u32>> = edge.faces.iter().filter(|f| **f != 0).map(|f| flood_faces(s, body, *f, &ids)).collect();
  let [a, c] = sides.as_slice() else {
    return Err(CmdError::new("the edges don't bound a region of the body"));
  };
  if a.len() == b.faces().len() || a.contains(&c[0]) {
    return Err(CmdError::new("the edges don't form a closed boundary"));
  }
  let area = |side: &[u32]| b.faces().iter().filter(|f| side.contains(&f.id)).map(|f| f.area).sum::<f64>();
  let inside = if (a.len(), area(a)) <= (c.len(), area(c)) { a } else { c };
  s.selection = inside.iter().map(|f| Sel::Face { body, face: *f }).collect();
  Ok(json!({"selected": s.selection.len()}))
}

/// `select.seed_boundary`: the faces connected to the seed face without crossing the boundary
/// edges (no boundary: every face of its body); the seed defaults to the selected face.
fn select_seed_boundary(s: &mut Session, p: &Value) -> CmdResult {
  let seed = match p.get("seed") {
    Some(v) => Some((BodyId(req_u32(v, "body")?), req_u32(v, "face")?)),
    None => s.selection.iter().find_map(|x| match x {
      Sel::Face { body, face } => Some((*body, *face)),
      _ => None,
    }),
  };
  let Some((body, face)) = seed else {
    return Err(CmdError::new("select the seed face first"));
  };
  let ids: Vec<u32> = boundary_edges(s, p, "edges").into_iter().filter(|(b, _)| *b == body).map(|(_, e)| e).collect();
  if s.model.body(body).is_none_or(|b| !b.faces().iter().any(|f| f.id == face)) {
    return Err(CmdError::new("no such face"));
  }
  s.selection = flood_faces(s, body, face, &ids).into_iter().map(|f| Sel::Face { body, face: f }).collect();
  Ok(json!({"selected": s.selection.len()}))
}
