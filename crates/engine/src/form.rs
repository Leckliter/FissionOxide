//! Form (T-spline) commands, as Fusion's Form workspace: primitives (each a new form feature),
//! and edits of a form's control cage (Edit Form, insert/delete edge, insert point, subdivide,
//! extrude, bridge, fill hole, crease, symmetry, pull, Make Uniform, display). The cage lives in
//! the form feature; each edit is one undo step and rebuilds the smooth body.
//!
//! Picks name cage elements by index or by a point near them: `vertices: [3, [x, y, z]]`,
//! `edges: [[4, 9], [x, y, z]]` (the edge whose midpoint is nearest), `faces: [0, [x, y, z]]`
//! (the face whose centre is nearest). Points and vectors are in the form's plane coordinates.

use fox_kernel::Shape;
use fox_kernel::glam::{DMat4, DVec3};
use fox_model::form::{Display, FormCage, MAX_FACES, Selection, SymPlane, Symmetry, edit, prim};
use fox_model::{BodyId, FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{add_feature, operation, plane_ref, req_vec3};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "form.box" => (|| {
      let (l, w, h) = (length(s, p, "length")?, length(s, p, "width")?, length(s, p, "height")?);
      let counts = [count(s, p, "length_faces", 2)?, count(s, p, "width_faces", 2)?, count(s, p, "height_faces", 2)?];
      add_form(s, p, prim::quad_box(l, w, h, counts).map_err(CmdError::new)?)
    })(),
    "form.plane" => (|| {
      let (l, w) = (length(s, p, "length")?, length(s, p, "width")?);
      let counts = [count(s, p, "length_faces", 2)?, count(s, p, "width_faces", 2)?];
      add_form(s, p, prim::plane(l, w, counts).map_err(CmdError::new)?)
    })(),
    "form.cylinder" => (|| {
      let (d, h) = (length(s, p, "diameter")?, length(s, p, "height")?);
      let (around, along) = (count(s, p, "around_faces", 8)?, count(s, p, "height_faces", 2)?);
      let capped = p.get("capped").and_then(Value::as_bool).unwrap_or(false);
      add_form(s, p, prim::cylinder(d, h, around, along, capped).map_err(CmdError::new)?)
    })(),
    "form.sphere" => (|| {
      let d = length(s, p, "diameter")?;
      let (around, rings) = (count(s, p, "around_faces", 8)?, count(s, p, "height_faces", 4)?);
      add_form(s, p, prim::sphere(d, around, rings).map_err(CmdError::new)?)
    })(),
    "form.torus" => (|| {
      let (d, t) = (length(s, p, "diameter")?, length(s, p, "tube_diameter")?);
      let (around, tube) = (count(s, p, "around_faces", 8)?, count(s, p, "tube_faces", 4)?);
      add_form(s, p, prim::torus(d, t, around, tube).map_err(CmdError::new)?)
    })(),
    "form.quadball" => (|| {
      let d = length(s, p, "diameter")?;
      add_form(s, p, prim::quadball(d, count(s, p, "faces", 2)?).map_err(CmdError::new)?)
    })(),
    "form.face" => (|| {
      // or corner by corner, "p0", "p1", ... (the dialog's fields)
      let pts = match p.get("points") {
        Some(_) => points(p, "points")?,
        None => (0..).map_while(|i| p.get(format!("p{i}")).and_then(as_point)).collect(),
      };
      add_form(s, p, prim::face(&pts).map_err(CmdError::new)?)
    })(),
    "form.cage" => (|| {
      let mut cage = FormCage { vertices: points(p, "vertices")?, ..FormCage::default() };
      for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
        let ids: Vec<u32> = f
          .as_array()
          .map(|a| a.iter().filter_map(|i| i.as_u64().and_then(|i| u32::try_from(i).ok())).collect())
          .unwrap_or_default();
        if ids.len() < 3 || ids.len() != f.as_array().map_or(0, Vec::len) {
          return Err(CmdError::new("each face is three or more vertex indices"));
        }
        cage.faces.push(ids);
        if cage.faces.len() > MAX_FACES {
          return Err(CmdError::new(format!("a cage can have at most {MAX_FACES} faces")));
        }
      }
      if let Some(cr) = p.get("creases").and_then(Value::as_array) {
        for c in cr {
          cage.creases.push(edge_pair(c).ok_or(CmdError::new("each crease is two vertex indices"))?);
        }
      }
      cage.check().map_err(CmdError::new)?;
      cage.compact();
      add_form(s, p, cage)
    })(),
    "form.info" => (|| {
      let id = form_feature(s, p)?;
      let cage = cage_of(s, id)?;
      let patches = fox_form::subd::patches(&cage).map(|v| v.len()).ok();
      let mut out = json!({
        "feature": id.0,
        "vertices": cage.vertices.len(),
        "faces": cage.faces.len(),
        "edges": cage.edges().len(),
        "open_edges": cage.boundary_edges().len(),
        "creases": cage.creases.len(),
        "closed": cage.is_closed(),
        "patches": patches,
        "symmetry": cage.symmetry,
        "display": cage.display,
      });
      if p.get("cage").and_then(Value::as_bool).unwrap_or(false) {
        out["cage"] = serde_json::to_value(&cage).unwrap_or(Value::Null);
      }
      Ok(out)
    })(),
    "form.move_vertex" => (|| {
      let id = form_feature(s, p)?;
      let cage = cage_of(s, id)?;
      let v = match (p.get("vertex").and_then(Value::as_u64), p.get("point")) {
        (Some(i), _) => usize::try_from(i).ok().filter(|i| *i < cage.vertices.len()),
        (None, Some(_)) => cage.nearest_vertex(req_vec3(p, "point")?),
        (None, None) => return Err(CmdError::new("pass the 'vertex' index or a 'point' near it")),
      }
      .ok_or(CmdError::new("no such vertex"))?;
      let old = cage.vertices[v];
      let new = match (p.get("to"), p.get("by")) {
        (Some(_), _) => req_vec3(p, "to")?,
        (None, Some(_)) => {
          let d = req_vec3(p, "by")?;
          [old[0] + d[0], old[1] + d[1], old[2] + d[2]]
        }
        (None, None) => return Err(CmdError::new("pass 'to' (a position) or 'by' (an offset)")),
      };
      let by = DVec3::from(new) - DVec3::from(old);
      let sel = Selection { vertices: vec![v as u32], ..Selection::default() };
      let mut out = change(s, id, |c| edit::transform(c, &sel, DMat4::from_translation(by), 0.0).map(|_| json!({})))?;
      out["vertex"] = json!(v);
      out["from"] = json!(old);
      out["to"] = json!(new);
      Ok(out)
    })(),
    "form.edit" => (|| {
      let id = form_feature(s, p)?;
      let cage = cage_of(s, id)?;
      let sel = selection(&cage, p)?;
      let pivot = match p.get("pivot") {
        Some(v) if !v.is_null() => DVec3::from(req_vec3(p, "pivot")?),
        _ => edit::pivot(&cage, &sel),
      };
      let mv = opt_vec3(p, "move")?.unwrap_or(DVec3::ZERO);
      let (axis, angle) = match p.get("rotate") {
        Some(r) if !r.is_null() => {
          let a = r.get("angle").and_then(Value::as_f64).ok_or(CmdError::new("'rotate' needs an 'angle' (degrees)"))?;
          (opt_vec3(r, "axis")?.unwrap_or(DVec3::Z), a)
        }
        _ => (DVec3::Z, 0.0),
      };
      let scale = match p.get("scale") {
        None | Some(Value::Null) => DVec3::ONE,
        Some(Value::Number(n)) => DVec3::splat(n.as_f64().unwrap_or(1.0)),
        Some(_) => DVec3::from(req_vec3(p, "scale")?),
      };
      if scale.min_element() <= 0.0 {
        return Err(CmdError::new("scale factors must be positive"));
      }
      let soft = p.get("soft").and_then(Value::as_f64).unwrap_or(0.0);
      let m = edit::compose(mv, axis, angle, scale, pivot);
      change(s, id, |c| edit::transform(c, &sel, m, soft).map(|n| json!({ "moved": n })))
    })(),
    "form.insert_edge" => (|| {
      let id = form_feature(s, p)?;
      let sel = selection(&cage_of(s, id)?, p)?;
      let t = p.get("t").and_then(Value::as_f64).unwrap_or(0.5);
      change(s, id, |c| edit::insert_edge(c, &sel.edges, t).map(|v| json!({ "new_vertices": v })))
    })(),
    "form.insert_point" => (|| {
      let id = form_feature(s, p)?;
      let cage = cage_of(s, id)?;
      let mut pts = Vec::new();
      for q in p.get("points").and_then(Value::as_array).ok_or(CmdError::new("missing 'points'"))? {
        let e = q.get("edge").ok_or(CmdError::new("each point needs an 'edge'"))?;
        let e = pick_edge(&cage, e)?;
        pts.push((e, q.get("t").and_then(Value::as_f64).unwrap_or(0.5)));
      }
      change(s, id, |c| edit::insert_point(c, &pts).map(|v| json!({ "new_vertices": v })))
    })(),
    "form.delete" => (|| {
      let id = form_feature(s, p)?;
      let sel = selection(&cage_of(s, id)?, p)?;
      change(s, id, |c| edit::delete(c, &sel).map(|_| json!({})))
    })(),
    "form.subdivide" => (|| {
      let id = form_feature(s, p)?;
      let sel = selection(&cage_of(s, id)?, p)?;
      let exact = p.get("exact").and_then(Value::as_bool).unwrap_or(false);
      change(s, id, |c| edit::subdivide(c, &sel.faces, exact).map(|_| json!({})))
    })(),
    "form.extrude" => (|| {
      let id = form_feature(s, p)?;
      let sel = selection(&cage_of(s, id)?, p)?;
      let d = p.get("distance").and_then(Value::as_f64).unwrap_or(0.0);
      let offset = opt_vec3(p, "offset")?;
      if !sel.faces.is_empty() {
        change(s, id, |c| edit::extrude_faces(c, &sel.faces, d).map(|v| json!({ "new_vertices": v })))
      } else {
        change(s, id, |c| edit::extrude_edges(c, &sel.edges, d, offset).map(|v| json!({ "new_vertices": v })))
      }
    })(),
    "form.bridge" => (|| {
      let id = form_feature(s, p)?;
      let cage = cage_of(s, id)?;
      let side = |k: &str| -> Result<Vec<[u32; 2]>, CmdError> {
        let a = p.get(k).and_then(Value::as_array).ok_or(CmdError::new(format!("missing '{k}' (edges)")))?;
        a.iter().map(|e| pick_edge(&cage, e)).collect()
      };
      let (a, b) = (side("side_a")?, side("side_b")?);
      let n = count(s, p, "faces", 1)?;
      change(s, id, |c| edit::bridge(c, &a, &b, n).map(|_| json!({})))
    })(),
    "form.fill_hole" => (|| {
      let id = form_feature(s, p)?;
      let e = pick_edge(&cage_of(s, id)?, p.get("edge").ok_or(CmdError::new("missing 'edge'"))?)?;
      let mode = match p.get("mode").and_then(Value::as_str).unwrap_or("star") {
        "star" => edit::Fill::Star,
        "polygon" | "single" => edit::Fill::Polygon,
        m => return Err(CmdError::new(format!("unknown fill mode '{m}' (star, polygon)"))),
      };
      change(s, id, |c| edit::fill_hole(c, e, mode).map(|f| json!({ "new_faces": f })))
    })(),
    "form.crease" | "form.uncrease" => (|| {
      let id = form_feature(s, p)?;
      let sel = selection(&cage_of(s, id)?, p)?;
      let on = name == "form.crease";
      change(s, id, |c| edit::crease(c, &sel, on).map(|n| json!({ "edges": n })))
    })(),
    "form.symmetry" => (|| {
      let id = form_feature(s, p)?;
      let sym = if p.get("clear").and_then(Value::as_bool).unwrap_or(false) {
        Symmetry::None
      } else if let Some(c) = p.get("circular").filter(|v| !v.is_null()) {
        Symmetry::Circular {
          origin: opt_vec3(c, "origin")?.unwrap_or(DVec3::ZERO).to_array(),
          axis: opt_vec3(c, "axis")?.unwrap_or(DVec3::Z).to_array(),
          count: c.get("count").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).filter(|n| (2..=64).contains(n)).ok_or(CmdError::new("'count' must be 2 to 64"))?,
        }
      } else {
        let planes = match p.get("mirror") {
          Some(Value::Array(a)) => a.iter().map(sym_plane).collect::<Result<Vec<_>, _>>()?,
          Some(v) if !v.is_null() => vec![sym_plane(v)?],
          _ => return Err(CmdError::new("pass 'mirror' (planes), 'circular' or 'clear'")),
        };
        Symmetry::Mirror { planes }
      };
      change(s, id, |c| edit::set_symmetry(c, sym).map(|_| json!({})))
    })(),
    "form.mirror_duplicate" => (|| {
      let id = form_feature(s, p)?;
      let plane = sym_plane(p.get("plane").unwrap_or(&json!("yz")))?;
      change(s, id, |c| edit::mirror_duplicate(c, plane).map(|_| json!({})))
    })(),
    "form.circular_duplicate" => (|| {
      let id = form_feature(s, p)?;
      let o = opt_vec3(p, "origin")?.unwrap_or(DVec3::ZERO);
      let a = opt_vec3(p, "axis")?.unwrap_or(DVec3::Z);
      let n = p.get("count").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or(CmdError::new("missing 'count'"))?;
      change(s, id, |c| edit::circular_duplicate(c, o, a, n).map(|_| json!({})))
    })(),
    "form.pull" => (|| {
      let id = form_feature(s, p)?;
      let cage = cage_of(s, id)?;
      let sel = selection(&cage, p)?;
      let verts: Vec<u32> = sel.vertex_set(&cage).into_iter().collect();
      let plane = *s.model.forms.get(&id).ok_or(CmdError::new("the form failed to build"))?;
      let to_world = fox_model::regen::plane_matrix(&plane);
      let to_cage = to_world.inverse();
      let target = match p.get("body").and_then(Value::as_u64) {
        Some(b) => {
          let b = BodyId(u32::try_from(b).map_err(|_| CmdError::new("bad 'body'"))?);
          s.model.body(b).ok_or(CmdError::new("no such body"))?
        }
        None => s
          .model
          .bodies
          .iter()
          .find(|b| b.feature != id)
          .ok_or(CmdError::new("pass the 'body' to pull onto"))?,
      };
      let shape = target.shape.clone();
      let closest = |q: DVec3| -> Option<DVec3> {
        let w = to_world.transform_point3(q);
        let (_, _, on) = Shape::vertex(w).ok()?.distance_to(&shape).ok()?;
        Some(to_cage.transform_point3(on))
      };
      let limit = p.get("limit").and_then(Value::as_bool).unwrap_or(true);
      change(s, id, |c| edit::pull(c, &verts, limit, closest).map(|g| json!({ "gap": g })))
    })(),
    "form.make_uniform" => (|| {
      let id = form_feature(s, p)?;
      let n = p.get("iterations").and_then(Value::as_u64).unwrap_or(10).min(100) as u32;
      change(s, id, |c| edit::make_uniform(c, n).map(|v| json!({ "vertices": v })))
    })(),
    "form.weld" => (|| {
      let id = form_feature(s, p)?;
      let tol = p.get("tolerance").and_then(Value::as_f64).unwrap_or(0.01);
      change(s, id, |c| edit::weld(c, tol).map(|n| json!({ "merged": n })))
    })(),
    "form.flip" => (|| {
      let id = form_feature(s, p)?;
      change(s, id, |c| {
        edit::flip(c);
        Ok(json!({}))
      })
    })(),
    "form.display" => (|| {
      let id = form_feature(s, p)?;
      let mode = match p.get("mode").and_then(Value::as_str).unwrap_or("smooth") {
        "box" | "1" => Display::Box,
        "control_frame" | "control" | "2" => Display::ControlFrame,
        "smooth" | "3" => Display::Smooth,
        m => return Err(CmdError::new(format!("unknown display '{m}' (box, control_frame, smooth)"))),
      };
      change(s, id, |c| {
        c.display = mode;
        Ok(json!({ "display": mode }))
      })
    })(),
    _ => return None,
  })
}

fn add_form(s: &mut Session, p: &Value, mut cage: FormCage) -> CmdResult {
  if let Some(at) = p.get("at").filter(|v| !v.is_null()) {
    let xyz: Vec<f64> = at.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let at = match xyz.as_slice() {
      [x, y] => DVec3::new(*x, *y, 0.0),
      [x, y, z] => DVec3::new(*x, *y, *z),
      _ => return Err(CmdError::new("'at' must be [x, y] or [x, y, z]")),
    };
    for v in &mut cage.vertices {
      *v = (DVec3::from(*v) + at).to_array();
    }
  }
  let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
  add_feature(s, FeatureKind::Form { plane, cage, operation: operation(p)? })
}

/// Applies an edit to a copy of the form's cage, then stores it as one undo step.
fn change(s: &mut Session, id: FeatureId, f: impl FnOnce(&mut FormCage) -> Result<Value, String>) -> CmdResult {
  let mut cage = cage_of(s, id)?;
  let mut out = f(&mut cage).map_err(CmdError::new)?;
  s.edit(|d| {
    if let Some(FeatureKind::Form { cage: c, .. }) = d.feature_mut(id).map(|f| &mut f.kind) {
      *c = cage;
    }
    Ok(())
  })?;
  let st = s.model.status(id);
  out["feature"] = json!(id.0);
  out["error"] = json!(st.and_then(|x| x.error.clone()));
  out["warning"] = json!(st.and_then(|x| x.warning.clone()));
  Ok(out)
}

fn cage_of(s: &Session, id: FeatureId) -> Result<FormCage, CmdError> {
  match s.design.feature(id).map(|f| &f.kind) {
    Some(FeatureKind::Form { cage, .. }) => Ok(cage.clone()),
    _ => Err(CmdError::new("that feature is not a form")),
  }
}

/// The form feature named by 'feature', or the last one.
fn form_feature(s: &Session, p: &Value) -> Result<FeatureId, CmdError> {
  match p.get("feature").and_then(Value::as_u64) {
    Some(f) => Ok(FeatureId(u32::try_from(f).map_err(|_| CmdError::new("bad feature"))?)),
    None => s
      .design
      .features
      .iter()
      .rev()
      .find(|f| matches!(f.kind, FeatureKind::Form { .. }))
      .map(|f| f.id)
      .ok_or(CmdError::new("there is no form feature")),
  }
}

fn as_point(v: &Value) -> Option<[f64; 3]> {
  match v.as_array().map(Vec::as_slice) {
    Some([x, y, z]) => {
      let p = [x.as_f64()?, y.as_f64()?, z.as_f64()?];
      p.iter().all(|c| c.is_finite()).then_some(p)
    }
    _ => None,
  }
}

fn edge_pair(v: &Value) -> Option<[u32; 2]> {
  match v.as_array().map(Vec::as_slice) {
    Some([a, b]) => Some([u32::try_from(a.as_u64()?).ok()?, u32::try_from(b.as_u64()?).ok()?]),
    _ => None,
  }
}

/// An edge by its two vertices, or the one whose midpoint is nearest a point.
fn pick_edge(cage: &FormCage, v: &Value) -> Result<[u32; 2], CmdError> {
  if let Some(e) = edge_pair(v) {
    let k = [e[0].min(e[1]), e[0].max(e[1])];
    return if cage.edges().contains(&k) { Ok(e) } else { Err(CmdError::new(format!("{}-{} isn't an edge", e[0], e[1]))) };
  }
  as_point(v).and_then(|p| cage.nearest_edge(p)).ok_or(CmdError::new("an edge is [a, b] or a point [x, y, z] near it"))
}

/// The picked vertices, edges and faces (see the module docs).
fn selection(cage: &FormCage, p: &Value) -> Result<Selection, CmdError> {
  let mut sel = Selection::default();
  let list = |k: &str| p.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
  for v in list("vertices") {
    let i = match (v.as_u64(), as_point(&v)) {
      (Some(i), _) => usize::try_from(i).ok().filter(|i| *i < cage.vertices.len()),
      (None, Some(q)) => cage.nearest_vertex(q),
      _ => None,
    };
    sel.vertices.push(i.ok_or(CmdError::new("a vertex is an index or a point [x, y, z] near it"))? as u32);
  }
  for e in list("edges") {
    sel.edges.push(pick_edge(cage, &e)?);
  }
  for f in list("faces") {
    let i = match (f.as_u64(), as_point(&f)) {
      (Some(i), _) => usize::try_from(i).ok().filter(|i| *i < cage.faces.len()),
      (None, Some(q)) => cage.nearest_face(q),
      _ => None,
    };
    sel.faces.push(i.ok_or(CmdError::new("a face is an index or a point [x, y, z] near its centre"))? as u32);
  }
  sel.normalize();
  Ok(sel)
}

fn points(p: &Value, k: &str) -> Result<Vec<[f64; 3]>, CmdError> {
  p.get(k)
    .and_then(Value::as_array)
    .ok_or(CmdError::new(format!("missing '{k}'")))?
    .iter()
    .map(|v| as_point(v).ok_or(CmdError::new(format!("each of '{k}' must be [x, y, z]"))))
    .collect()
}

fn opt_vec3(p: &Value, k: &str) -> Result<Option<DVec3>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => as_point(v).map(|a| Some(DVec3::from(a))).ok_or(CmdError::new(format!("'{k}' must be [x, y, z]"))),
  }
}

/// A mirror plane: "yz" / "xz" / "xy" (through the origin) or {origin?, normal}.
fn sym_plane(v: &Value) -> Result<SymPlane, CmdError> {
  if let Some(n) = v.as_str() {
    let normal = match n.to_ascii_lowercase().as_str() {
      "yz" | "x" => [1.0, 0.0, 0.0],
      "xz" | "y" => [0.0, 1.0, 0.0],
      "xy" | "z" => [0.0, 0.0, 1.0],
      o => return Err(CmdError::new(format!("unknown mirror plane '{o}' (yz, xz, xy)"))),
    };
    return Ok(SymPlane { origin: [0.0; 3], normal });
  }
  let normal = opt_vec3(v, "normal")?.ok_or(CmdError::new("a mirror plane needs a 'normal'"))?;
  if normal.length() < 1e-12 {
    return Err(CmdError::new("the mirror plane's normal is zero"));
  }
  Ok(SymPlane { origin: opt_vec3(v, "origin")?.unwrap_or(DVec3::ZERO).to_array(), normal: normal.normalize().to_array() })
}

/// A length: a number (mm) or an expression ("80 mm", "w / 2").
fn length(s: &Session, p: &Value, k: &str) -> Result<f64, CmdError> {
  let v = match p.get(k) {
    Some(Value::Number(n)) => n.as_f64().ok_or(CmdError::new(format!("bad '{k}'")))?,
    Some(Value::String(e)) => fox_expr::eval(e, fox_expr::Kind::Length, &s.model.params)
      .map_err(|err| CmdError::new(format!("'{k}': {err}")))?,
    _ => return Err(CmdError::new(format!("missing '{k}'"))),
  };
  if !(v.is_finite() && v > 0.0) {
    return Err(CmdError::new(format!("'{k}' must be positive")));
  }
  Ok(v)
}

/// A face count (whole number 1 to 64), `default` if not given.
fn count(s: &Session, p: &Value, k: &str, default: u32) -> Result<u32, CmdError> {
  let n = match p.get(k) {
    None | Some(Value::Null) => return Ok(default),
    Some(Value::String(e)) => fox_expr::eval(e, fox_expr::Kind::Unitless, &s.model.params).ok(),
    Some(v) => v.as_f64(),
  };
  n.filter(|n| n.fract() == 0.0 && (1.0..=64.0).contains(n))
    .map(|n| n as u32)
    .ok_or(CmdError::new(format!("'{k}' must be a whole number from 1 to 64")))
}
