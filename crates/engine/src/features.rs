//! Commands for construction geometry by method, threads, coils, pipes, ribs and webs, scale,
//! split/remove faces, thicken, surfaces, mesh tools and appearances.

use fox_kernel::glam::DVec3;
use fox_model::construct::{AxisDef, CoilSection, LineRef, MeshOp, PipeSection, PlaneDef, PointDef, PointRef};
use fox_model::{BodyId, EdgeRef, FaceRef, FeatureId, FeatureKind, PathRef, PlaneRef};
use fox_sketch::{CurveId, CurveKind};
use serde_json::{Value, json};

use crate::commands::{
  add_feature, axis_ref, body_sel, edge_ref_of, expr_of, face_ref, ids, operation, opt_expr, opt_str, plane_ref,
  req_expr, req_u32, req_vec3, sketch_for_feature, sketch_line_at,
};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "plane" => plane(s, p),
    "axis" => axis(s, p),
    "point" => point(s, p),
    "thread" => (|| {
      let faces = faces(s, p)?;
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      add_feature(
        s,
        FeatureKind::Thread {
          faces,
          pitch: opt_expr(p, "pitch").unwrap_or_default(),
          length: opt_expr(p, "length").unwrap_or_default(),
          offset: opt_expr(p, "offset").unwrap_or_else(|| "0".into()),
          modeled: flag("modeled"),
          left: flag("left") || p.get("right_handed").and_then(Value::as_bool) == Some(false),
          flip: flag("flip") || opt_str(p, "location") == Some("high_end"),
          designation: opt_str(p, "designation").unwrap_or_default().to_string(),
          class: opt_str(p, "class").unwrap_or_default().to_string(),
        },
      )
    })(),
    "coil" => (|| {
      let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
      let at = match p.get("at").and_then(Value::as_array) {
        Some(a) if a.len() == 2 => [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)],
        _ => [0.0, 0.0],
      };
      let section = match opt_str(p, "section").unwrap_or("circular") {
        "circular" | "circle" => CoilSection::Circular,
        "square" => CoilSection::Square,
        "triangle_out" | "triangle" => CoilSection::TriangleOut,
        "triangle_in" => CoilSection::TriangleIn,
        o => return Err(CmdError::new(format!("unknown section '{o}' (circular, square, triangle_out, triangle_in)"))),
      };
      let height = match (opt_expr(p, "height"), opt_expr(p, "revolutions")) {
        (Some(h), _) => h,
        (None, Some(n)) => format!("({n}) * ({})", req_expr(p, "pitch")?),
        _ => return Err(CmdError::new("missing 'height' or 'revolutions'")),
      };
      add_feature(
        s,
        FeatureKind::Coil {
          plane,
          at,
          diameter: req_expr(p, "diameter")?,
          pitch: req_expr(p, "pitch")?,
          height,
          section,
          section_size: req_expr(p, "section_size")?,
          left: p.get("left").and_then(Value::as_bool).unwrap_or(false),
          operation: operation(p)?,
        },
      )
    })(),
    "sketch.intersection_curve" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      s.design.sketch(sketch)?;
      let curves = PathRef { sketch, curves: ids(p, "curves").into_iter().map(CurveId).collect() };
      let faces = match p.get("faces") {
        Some(_) => faces(s, p)?,
        None => Vec::new(),
      };
      let mut with = Vec::new();
      for w in p.get("with").and_then(Value::as_array).into_iter().flatten() {
        with.push(crate::solid::path_ref(s, w)?);
      }
      if faces.is_empty() && with.is_empty() {
        return Err(CmdError::new("give 'faces' or 'with' (sketch curves) to intersect with"));
      }
      add_feature(s, FeatureKind::IntersectionCurve { curves, faces, with })
    })(),
    "pipe" => (|| {
      const KEYS: [&str; 10] =
        ["cmd", "path", "path_curves", "diameter", "size", "wall", "section", "distance", "distance_one", "operation"];
      if let Some(k) = p.as_object().and_then(|o| o.keys().find(|k| !KEYS.contains(&k.as_str()))) {
        return Err(CmdError::new(format!("pipe doesn't take '{k}' ({})", KEYS[1..].join(", "))));
      }
      let path_sketch = FeatureId(req_u32(p, "path")?);
      // a sketch, or the 3D curves of a Project To Surface or an intersection curve
      let curves3d = s.design.feature(path_sketch).is_some_and(|f| {
        matches!(f.kind, FeatureKind::ProjectToSurface { .. } | FeatureKind::IntersectionCurve { .. })
      });
      if !curves3d {
        s.design.sketch(path_sketch)?;
      }
      let section = match opt_str(p, "section").unwrap_or("circular") {
        "circular" | "circle" => PipeSection::Circular,
        "square" => PipeSection::Square,
        "triangular" | "triangle" => PipeSection::Triangular,
        o => return Err(CmdError::new(format!("unknown section '{o}' (circular, square, triangular)"))),
      };
      let diameter = match opt_expr(p, "size") {
        Some(_) if p.get("diameter").is_some() => {
          return Err(CmdError::new("pipe takes 'diameter' or 'size', not both"));
        }
        Some(size) => size,
        None => req_expr(p, "diameter").map_err(|_| CmdError::new("missing 'diameter' (or 'size')"))?,
      };
      add_feature(
        s,
        FeatureKind::Pipe {
          path_sketch,
          path_curves: ids(p, "path_curves").into_iter().map(CurveId).collect(),
          diameter,
          wall: opt_expr(p, "wall").unwrap_or_default(),
          operation: operation(p)?,
          section,
          distance: opt_expr(p, "distance").or_else(|| opt_expr(p, "distance_one")).unwrap_or_default(),
        },
      )
    })(),
    "rib" | "web" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      add_feature(
        s,
        FeatureKind::Rib {
          sketch,
          curves: ids(p, "curves").into_iter().map(CurveId).collect(),
          thickness: req_expr(p, "thickness")?,
          depth: opt_expr(p, "depth").unwrap_or_default(),
          flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
          web: name == "web",
          one_side: p.get("one_side").and_then(Value::as_bool).unwrap_or(false),
        },
      )
    })(),
    "scale" => (|| {
      let factors = match p.get("factors").and_then(Value::as_array) {
        Some(a) if a.len() == 3 => Some([expr_of(&a[0])?, expr_of(&a[1])?, expr_of(&a[2])?]),
        Some(_) => return Err(CmdError::new("factors needs [x, y, z]")),
        None => None,
      };
      let factor = opt_expr(p, "factor").unwrap_or_else(|| "1".into());
      let center = if p.get("center").is_some() { req_vec3(p, "center")? } else { [0.0; 3] };
      add_feature(s, FeatureKind::Scale { bodies: body_sel(p), center, factor, factors })
    })(),
    "split_face" => (|| {
      let faces = faces(s, p)?;
      let tool = plane_ref(s, p.get("tool").or(p.get("plane")).ok_or(CmdError::new("missing 'tool' (a plane)"))?)?;
      add_feature(s, FeatureKind::SplitFace { faces, tool })
    })(),
    "remove_faces" => (|| {
      let faces = faces(s, p)?;
      add_feature(s, FeatureKind::RemoveFaces { faces })
    })(),
    "thicken" => (|| {
      let faces = match p.get("faces") {
        Some(_) => faces(s, p)?,
        None => Vec::new(),
      };
      let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      let round = match p.get("corners").and_then(Value::as_str) {
        None | Some("sharp") => false,
        Some("round") => true,
        Some(o) => return Err(CmdError::new(format!("unknown corners '{o}' (sharp or round)"))),
      };
      add_feature(
        s,
        FeatureKind::Thicken {
          faces,
          bodies,
          thickness: req_expr(p, "thickness")?,
          symmetric: flag("symmetric"),
          flip: flag("flip"),
          operation: operation(p)?,
          round,
        },
      )
    })(),
    "surface.extrude" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      add_feature(
        s,
        FeatureKind::SurfaceExtrude {
          sketch,
          curves: ids(p, "curves").into_iter().map(CurveId).collect(),
          distance: req_expr(p, "distance")?,
          symmetric: flag("symmetric"),
          flip: flag("flip"),
        },
      )
    })(),
    "surface.revolve" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      let axis = axis_ref(s, p.get("axis").ok_or(CmdError::new("missing 'axis'"))?)?;
      add_feature(
        s,
        FeatureKind::SurfaceRevolve {
          sketch,
          curves: ids(p, "curves").into_iter().map(CurveId).collect(),
          axis,
          angle: opt_expr(p, "angle").unwrap_or_else(|| "360 deg".into()),
        },
      )
    })(),
    "surface.patch" => (|| {
      let mut edges = Vec::new();
      for e in p.get("edges").and_then(Value::as_array).into_iter().flatten() {
        edges.push(edge_ref_of(s, e)?);
      }
      let sketch = p.get("sketch").map(|_| req_u32(p, "sketch").map(FeatureId)).transpose()?;
      if let Some(sk) = sketch {
        s.design.sketch(sk)?;
        s.edit_sketch = None;
      }
      if edges.is_empty() && sketch.is_none() {
        return Err(CmdError::new("pass the boundary 'edges' or a 'sketch'"));
      }
      add_feature(s, FeatureKind::Patch { edges, sketch })
    })(),
    "surface.stitch" => (|| {
      let bodies = body_list(s, p)?;
      add_feature(
        s,
        FeatureKind::Stitch { bodies, tolerance: opt_expr(p, "tolerance").unwrap_or_else(|| "0.01 mm".into()) },
      )
    })(),
    "surface.unstitch" => (|| {
      let bodies = body_list(s, p)?;
      add_feature(s, FeatureKind::Unstitch { bodies })
    })(),
    "surface.ruled" => (|| {
      let (a, b) = (edge(s, p, "a")?, edge(s, p, "b")?);
      add_feature(s, FeatureKind::Ruled { a, b })
    })(),
    "surface.offset" => (|| {
      let faces = if p.get("faces").is_some() { faces(s, p)? } else { Vec::new() };
      let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
      let face_distances = crate::surfaces::face_distances(s, p)?;
      add_feature(s, FeatureKind::SurfaceOffset { faces, bodies, distance: req_expr(p, "distance")?, face_distances })
    })(),
    "surface.trim" => (|| {
      let bodies = body_list(s, p)?;
      let tool = plane_of(s, p, "tool")?;
      let flip = p.get("flip").and_then(Value::as_bool).unwrap_or(false);
      let (keep, remove) = (crate::surfaces::points(p, "keep")?, crate::surfaces::points(p, "remove")?);
      add_feature(s, FeatureKind::Trim { bodies, tool, flip, keep, remove })
    })(),
    "surface.reverse" => (|| {
      let bodies = body_list(s, p)?;
      add_feature(s, FeatureKind::ReverseNormal { bodies })
    })(),
    "mesh.reduce" | "mesh.smooth" | "mesh.convert" => (|| {
      let bodies = body_list(s, p)?;
      let (op, default) = match name {
        "mesh.reduce" => (MeshOp::Reduce, "0.5"),
        "mesh.smooth" => (MeshOp::Smooth, "3"),
        _ => (MeshOp::Convert, "1"),
      };
      let key = if op == MeshOp::Smooth { "passes" } else { "keep" };
      let amount = opt_expr(p, key).or_else(|| opt_expr(p, "amount")).unwrap_or_else(|| default.into());
      add_feature(s, FeatureKind::Mesh { bodies, op, amount })
    })(),
    "body.appearance" => appearance(s, p),
    "appearances" => Ok(json!(
      APPEARANCES
        .iter()
        .map(|(n, c)| json!({"name": n, "color": c, "folder": appearance_folder(n)}))
        .collect::<Vec<_>>()
    )),
    "sketch.project_to_surface" => (|| {
      let sketch = sketch_for_feature(s, p)?;
      let faces = faces(s, p)?;
      let along =
        match opt_str(p, "type").unwrap_or(if p.get("direction").is_some() { "along_vector" } else { "closest_point" })
        {
          "closest_point" | "closest" => None,
          "along_vector" | "along" | "vector" => {
            let d = p.get("direction").ok_or(CmdError::new("along_vector needs a 'direction' (x, y, z or an axis)"))?;
            Some(axis_ref(s, d)?)
          }
          o => return Err(CmdError::new(format!("unknown type '{o}' (closest_point, along_vector)"))),
        };
      let curves = ids(p, "curves").into_iter().map(CurveId).collect();
      add_feature(s, FeatureKind::ProjectToSurface { sketch, curves, faces, along })
    })(),
    _ => return None,
  })
}

/// Fusion's common appearances (library names) and the colour FissionOxide renders them in.
pub const APPEARANCES: [(&str, [u8; 3]); 16] = [
  ("Steel - Satin", [205, 200, 190]),
  ("Aluminum - Anodized Black", [52, 54, 58]),
  ("Aluminum - Anodized Red", [176, 46, 44]),
  ("Aluminum - Anodized Blue", [46, 92, 170]),
  ("Aluminum - Brushed", [205, 207, 210]),
  ("Brass - Polished", [204, 170, 84]),
  ("Copper", [190, 112, 72]),
  ("Gold - Polished", [222, 182, 70]),
  ("Plastic - Matte (White)", [236, 236, 232]),
  ("Plastic - Matte (Black)", [40, 40, 42]),
  ("ABS (Red)", [208, 50, 46]),
  ("ABS (Yellow)", [236, 196, 48]),
  ("ABS (Green)", [70, 160, 80]),
  ("Rubber - Black", [30, 30, 30]),
  ("Wood - Oak", [180, 132, 82]),
  ("Glass - Clear", [190, 220, 230]),
];

/// The folders of Fusion's appearance library (Fusion Appearances), in its order.
pub const APPEARANCE_FOLDERS: [&str; 18] = [
  "Ceramic",
  "Concrete",
  "Emissive",
  "Fabric",
  "Flooring",
  "Glass",
  "Layered",
  "Leather",
  "Liquid",
  "Metal",
  "Miscellaneous",
  "Other",
  "Paint",
  "Plastic",
  "Roofing",
  "Stone",
  "Wall Covering",
  "Wood",
];

/// The library folder (one of [`APPEARANCE_FOLDERS`]) an appearance of [`APPEARANCES`] sits in.
pub fn appearance_folder(name: &str) -> &'static str {
  let first = name.split([' ', '-']).next().unwrap_or("");
  match first {
    "Steel" | "Aluminum" | "Brass" | "Copper" | "Gold" => "Metal",
    "Plastic" | "ABS" | "Rubber" => "Plastic",
    "Wood" => "Wood",
    "Glass" => "Glass",
    _ => "Other",
  }
}

/// Sets (or with no appearance/colour, clears) the appearance of bodies or a component's bodies.
fn appearance(s: &mut Session, p: &Value) -> CmdResult {
  let mut bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if let Some(b) = p.get("body").and_then(Value::as_u64) {
    bodies.push(BodyId(b as u32));
  }
  if let Some(c) = p.get("component").and_then(Value::as_u64) {
    let comp =
      s.design.components.iter().find(|x| x.id.0 as u64 == c).ok_or(CmdError::new(format!("no component {c}")))?;
    bodies.extend(comp.bodies.iter().copied());
  }
  if bodies.is_empty() {
    return Err(CmdError::new("pass 'body', 'bodies' or 'component'"));
  }
  let color = match (opt_str(p, "appearance"), p.get("color")) {
    (Some(name), _) => Some(
      APPEARANCES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, c)| *c)
        .ok_or_else(|| CmdError::new(format!("unknown appearance '{name}' (see 'appearances')")))?,
    ),
    (None, None | Some(Value::Null)) => None,
    (None, Some(c)) => Some(crate::commands::parse_color(c)?),
  };
  s.edit(|d| {
    for b in &bodies {
      d.body_settings_mut(*b).color = color;
    }
    Ok(())
  })?;
  Ok(json!({"bodies": bodies.iter().map(|b| b.0).collect::<Vec<_>>(), "color": color}))
}

fn faces(s: &Session, p: &Value) -> Result<Vec<FaceRef>, CmdError> {
  let mut out = Vec::new();
  for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
    out.push(face_ref(s, f)?);
  }
  if out.is_empty() {
    return Err(CmdError::new("no faces given"));
  }
  Ok(out)
}

fn body_list(s: &Session, p: &Value) -> Result<Vec<BodyId>, CmdError> {
  let bodies: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if bodies.is_empty() {
    return Err(CmdError::new("missing 'bodies'"));
  }
  for b in &bodies {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  Ok(bodies)
}

fn edge(s: &Session, p: &Value, k: &str) -> Result<EdgeRef, CmdError> {
  edge_ref_of(s, p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}' (an edge)")))?)
}

/// A straight line: an axis as `axis_ref` takes it ("x"/"y"/"z" or {axis: id}), a sketch line
/// {sketch, curve}, or a body edge ({point} or {body, edge}).
fn line(s: &Session, p: &Value, k: &str) -> Result<LineRef, CmdError> {
  let v = p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}' (an edge, axis or sketch line)")))?;
  if v.is_string() || v.get("axis").is_some() {
    return Ok(LineRef::Axis(axis_ref(s, v)?));
  }
  if v.get("sketch").is_some() {
    let sketch = FeatureId(req_u32(v, "sketch")?);
    let curve = CurveId(req_u32(v, "curve")?);
    match s.design.sketch(sketch)?.curve(curve).map(|c| &c.kind) {
      Some(CurveKind::Line { .. }) => return Ok(LineRef::Sketch { sketch, curve }),
      _ => return Err(CmdError::new(format!("curve {} of that sketch is not a line", curve.0))),
    }
  }
  Ok(LineRef::Edge(edge_ref_of(s, v)?))
}

fn plane_of(s: &Session, p: &Value, k: &str) -> Result<PlaneRef, CmdError> {
  plane_ref(s, p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}' (a plane)")))?)
}

/// {"at": [x,y,z]} | {"vertex": edge, "end"?: bool} | {"center": edge} | {"point": feature}.
pub(crate) fn point_ref(s: &Session, v: &Value) -> Result<PointRef, CmdError> {
  if v.get("at").is_some() {
    return Ok(PointRef::At { at: req_vec3(v, "at")? });
  }
  if let Some(e) = v.get("vertex") {
    return Ok(PointRef::Vertex {
      edge: edge_ref_of(s, e)?,
      end: v.get("end").and_then(Value::as_bool).unwrap_or(false),
    });
  }
  if let Some(e) = v.get("center") {
    return Ok(PointRef::Center { edge: edge_ref_of(s, e)? });
  }
  if v.get("point").is_some() {
    let feature = FeatureId(req_u32(v, "point")?);
    if !s.model.points.contains_key(&feature) {
      return Err(CmdError::new("that feature is not a construction point"));
    }
    return Ok(PointRef::Construction { feature });
  }
  if v.as_array().is_some() {
    return Ok(PointRef::At { at: req_vec3(&json!({"at": v}), "at")? });
  }
  Err(CmdError::new("a point is {at: [x,y,z]}, {vertex: edge, end?}, {center: edge} or {point: id}"))
}

fn pref(s: &Session, p: &Value, k: &str) -> Result<PointRef, CmdError> {
  point_ref(s, p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}' (a point)")))?)
}

fn t_of(p: &Value) -> String {
  opt_expr(p, "t").unwrap_or_else(|| "0.5".into())
}

/// A physical distance along the path (empty = use `t`).
fn distance_of(p: &Value) -> String {
  opt_expr(p, "distance").unwrap_or_default()
}

/// What a point or plane along a path runs along.
enum AlongPath {
  Edge(EdgeRef),
  Curve(PathRef),
}

/// The path of `along_path`: `edge` (or `path`) is a body edge, or {sketch, curve} / {sketch,
/// curves: [id]} for sketch curves.
fn along_path(s: &Session, p: &Value) -> Result<AlongPath, CmdError> {
  let v = p.get("edge").or(p.get("path")).ok_or(CmdError::new("missing 'edge' (the path)"))?;
  if v.get("sketch").is_none() {
    return Ok(AlongPath::Edge(edge_ref_of(s, v)?));
  }
  let sketch = FeatureId(req_u32(v, "sketch")?);
  let sk = s.design.sketch(sketch)?;
  let curves: Vec<CurveId> = match v.get("curve") {
    Some(_) => vec![CurveId(req_u32(v, "curve")?)],
    None => ids(v, "curves").into_iter().map(CurveId).collect(),
  };
  if let Some(c) = curves.iter().find(|c| sk.curve(**c).is_none() && sk.curve3d(**c).is_none()) {
    return Err(CmdError::new(format!("the sketch has no curve {}", c.0)));
  }
  Ok(AlongPath::Curve(PathRef { sketch, curves }))
}

fn plane(s: &mut Session, p: &Value) -> CmdResult {
  let def = match opt_str(p, "method").unwrap_or("") {
    "angle" => PlaneDef::Angle {
      axis: axis_ref(s, p.get("axis").ok_or(CmdError::new("missing 'axis'"))?)?,
      base: plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?,
      angle: req_expr(p, "angle")?,
    },
    "midplane" => PlaneDef::Midplane { a: plane_of(s, p, "a")?, b: plane_of(s, p, "b")? },
    "three_points" => PlaneDef::ThreePoints { a: pref(s, p, "a")?, b: pref(s, p, "b")?, c: pref(s, p, "c")? },
    "two_edges" => PlaneDef::TwoEdges { a: line(s, p, "a")?, b: line(s, p, "b")? },
    "tangent" => PlaneDef::Tangent {
      face: face_ref(s, p.get("face").ok_or(CmdError::new("missing 'face'"))?)?,
      near: req_vec3(p, "near")?,
    },
    "along_path" => match along_path(s, p)? {
      AlongPath::Edge(edge) => PlaneDef::AlongPath { edge, t: t_of(p), distance: distance_of(p) },
      AlongPath::Curve(path) => PlaneDef::AlongCurve { path, t: t_of(p), distance: distance_of(p) },
    },
    "perpendicular" => PlaneDef::Perpendicular {
      base: plane_of(s, p, "plane")?,
      through: p.get("through").map(|v| point_ref(s, v)).transpose()?,
      distance: opt_expr(p, "distance").unwrap_or_else(|| "0".into()),
      v: match opt_str(p, "direction").unwrap_or("u") {
        "u" | "U" => false,
        "v" | "V" => true,
        o => return Err(CmdError::new(format!("unknown direction '{o}' (u, v)"))),
      },
    },
    o => {
      return Err(CmdError::new(format!(
        "unknown plane method '{o}' (angle, midplane, three_points, two_edges, tangent, along_path, perpendicular; plane.offset for offsets)"
      )));
    }
  };
  add_feature(s, FeatureKind::PlaneBy { def })
}

/// What an axis through an edge runs along: a body edge ({body, edge} or {point}), a sketch line
/// ({sketch, line}, or {point} on it when no body edge is there) or another axis ({axis}).
fn axis_along(s: &Session, v: &Value) -> Result<AxisDef, CmdError> {
  if let Some(f) = v.get("axis") {
    let feature = FeatureId(req_u32(&json!({"axis": f}), "axis")?);
    if !s.model.axes.contains_key(&feature) {
      return Err(CmdError::new("that feature is not a construction axis"));
    }
    return Ok(AxisDef::Axis { feature });
  }
  if v.get("sketch").is_some() {
    let sketch = FeatureId(req_u32(v, "sketch")?);
    let sk = s.model.sketches.get(&sketch).ok_or(CmdError::new(format!("no sketch {}", sketch.0)))?;
    let curve = CurveId(req_u32(v, if v.get("line").is_some() { "line" } else { "curve" })?);
    if !matches!(sk.sketch.curve(curve).map(|c| &c.kind), Some(fox_sketch::CurveKind::Line { .. })) {
      return Err(CmdError::new(format!("sketch {} has no line {}", sketch.0, curve.0)));
    }
    return Ok(AxisDef::SketchLine { sketch, curve });
  }
  match edge_ref_of(s, v) {
    Ok(edge) => Ok(AxisDef::Edge { edge }),
    Err(e) if v.get("point").is_some() => match sketch_line_at(s, DVec3::from(req_vec3(v, "point")?)) {
      Ok((sketch, curve)) => Ok(AxisDef::SketchLine { sketch, curve }),
      Err(_) => Err(e),
    },
    Err(e) => Err(e),
  }
}

fn axis(s: &mut Session, p: &Value) -> CmdResult {
  let face = |s: &Session| face_ref(s, p.get("face").ok_or(CmdError::new("missing 'face'"))?);
  let def = match opt_str(p, "method").unwrap_or("") {
    "face" | "cylinder" => AxisDef::Face { face: face(s)? },
    "normal" => AxisDef::Normal { face: face(s)?, near: req_vec3(p, "near")? },
    "two_planes" => AxisDef::TwoPlanes { a: plane_of(s, p, "a")?, b: plane_of(s, p, "b")? },
    "two_points" => AxisDef::TwoPoints { a: pref(s, p, "a")?, b: pref(s, p, "b")? },
    "edge" => axis_along(s, p.get("edge").ok_or(CmdError::new("missing 'edge' (an edge, sketch line or axis)"))?)?,
    o => {
      return Err(CmdError::new(format!("unknown axis method '{o}' (face, normal, two_planes, two_points, edge)")));
    }
  };
  add_feature(s, FeatureKind::ConstructionAxis { def })
}

fn point(s: &mut Session, p: &Value) -> CmdResult {
  let def = match opt_str(p, "method").unwrap_or("") {
    "vertex" | "at" | "center_of_edge" => PointDef::Ref { point: pref(s, p, "point")? },
    "two_edges" => PointDef::TwoEdges { a: line(s, p, "a")?, b: line(s, p, "b")? },
    "three_planes" => {
      PointDef::ThreePlanes { a: plane_of(s, p, "a")?, b: plane_of(s, p, "b")?, c: plane_of(s, p, "c")? }
    }
    "center" => PointDef::Center { face: face_ref(s, p.get("face").ok_or(CmdError::new("missing 'face'"))?)? },
    "edge_plane" => {
      let e = p.get("edge").ok_or(CmdError::new("missing 'edge' (an edge, sketch line or axis)"))?;
      let plane = plane_of(s, p, "plane")?;
      if e.is_string() || e.get("sketch").is_some() || e.get("axis").is_some() {
        PointDef::LinePlane { line: axis_ref(s, e)?, plane }
      } else {
        PointDef::EdgePlane { edge: edge_ref_of(s, e)?, plane }
      }
    }
    "along_path" => match along_path(s, p)? {
      AlongPath::Edge(edge) => PointDef::AlongPath { edge, t: t_of(p), distance: distance_of(p) },
      AlongPath::Curve(path) => PointDef::AlongCurve { path, t: t_of(p), distance: distance_of(p) },
    },
    o => {
      return Err(CmdError::new(format!(
        "unknown point method '{o}' (vertex, two_edges, three_planes, center, edge_plane, along_path)"
      )));
    }
  };
  add_feature(s, FeatureKind::ConstructionPoint { def })
}
