//! MESH tab commands: Tessellate, Repair (and its Quick Analysis), Generate and Combine Face
//! Groups, Create Mesh Section Sketch, and the Direct Mesh Editing environment's start and finish.

use fox_model::direct::BaseBody;
use fox_model::meshprep::{self, RepairType};
use fox_model::{BodyId, FeatureId, FeatureKind};
use fox_sketch::Sketch;
use serde_json::{Value, json};

use crate::commands::{add_feature, face_ref, ids, opt_expr, opt_str, plane_ref, req_u32};
use crate::{CmdError, CmdResult, Session};

/// Is `body` a mesh body (see `fox_model::regen::BodyKind`)? The mesh commands and dialogs ask
/// here.
pub fn is_mesh_body(s: &Session, body: BodyId) -> bool {
  s.model.body(body).is_some_and(|b| b.is_mesh())
}

/// What Finish Direct Editing appends to the edited body's name.
pub const EDITED: &str = "_Edited";

fn body(s: &Session, p: &Value, k: &str) -> Result<BodyId, CmdError> {
  let b = BodyId(req_u32(p, k)?);
  s.model.body(b).ok_or_else(|| CmdError::new(format!("no body {}", b.0)))?;
  Ok(b)
}

fn bodies(s: &Session, p: &Value) -> Result<Vec<BodyId>, CmdError> {
  let list: Vec<BodyId> = ids(p, "bodies").into_iter().chain(ids(p, "body")).map(BodyId).collect();
  if list.is_empty() {
    return Err(CmdError::new("missing 'bodies'"));
  }
  for b in &list {
    s.model.body(*b).ok_or_else(|| CmdError::new(format!("no body {}", b.0)))?;
  }
  Ok(list)
}

/// A body's mesh analysis, as Repair's Quick Analysis shows it.
pub fn analysis(s: &Session, body: BodyId) -> Option<meshprep::Analysis> {
  let m = s.model.body(body)?.mesh()?;
  let (pos, tris) = fox_model::meshops::weld(m);
  Some(meshprep::analyze(&pos, &tris))
}

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "mesh.tessellate" => (|| {
      let body = body(s, p, "body")?;
      let refinement = opt_str(p, "refinement").unwrap_or("medium").to_lowercase();
      if !matches!(refinement.as_str(), "high" | "medium" | "low" | "custom") {
        return Err(CmdError::new(format!("unknown refinement '{refinement}' (high, medium, low, custom)")));
      }
      let get = |k: &str| opt_expr(p, k).unwrap_or_default();
      add_feature(
        s,
        FeatureKind::Tessellate {
          body,
          refinement,
          surface_deviation: get("surface_deviation"),
          normal_deviation: get("normal_deviation"),
          max_edge: get("max_edge"),
          aspect_ratio: get("aspect_ratio"),
          quads: p.get("quads").and_then(Value::as_bool).unwrap_or(false),
        },
      )
    })(),
    "mesh.repair" => (|| {
      let bodies = bodies(s, p)?;
      let ty = opt_str(p, "type").unwrap_or("stitch_and_remove");
      let repair = RepairType::parse(ty).ok_or_else(|| {
        CmdError::new(format!(
          "unknown repair type '{ty}' (close_holes, stitch_and_remove, wrap, one_touch_fix, rebuild)"
        ))
      })?;
      add_feature(s, FeatureKind::MeshRepair { bodies, repair })
    })(),
    "mesh.analyze" => (|| {
      let body = body(s, p, "body")?;
      let a = analysis(s, body).ok_or(CmdError::new("the body has no mesh"))?;
      Ok(json!({
        "clean": a.clean(),
        "triangles": a.triangles,
        "open_edges": a.open_edges,
        "holes": a.holes,
        "non_manifold_edges": a.non_manifold_edges,
        "degenerate": a.degenerate,
        "flipped": a.flipped,
        "shells": a.shells,
      }))
    })(),
    "mesh.face_groups" => (|| {
      let bodies = bodies(s, p)?;
      let accurate = match opt_str(p, "type").unwrap_or("fast") {
        "fast" => false,
        "accurate" => true,
        o => return Err(CmdError::new(format!("unknown type '{o}' (fast, accurate)"))),
      };
      let angle = opt_expr(p, "angle").unwrap_or_else(|| "25 deg".into());
      let min_size = opt_expr(p, "min_size").unwrap_or_else(|| "0.2".into());
      let r = add_feature(s, FeatureKind::GenerateFaceGroups { bodies: bodies.clone(), accurate, angle, min_size })?;
      Ok(with_groups(s, r, &bodies))
    })(),
    "mesh.combine_face_groups" => (|| {
      let list = p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))?;
      let faces = list.iter().map(|f| face_ref(s, f)).collect::<Result<Vec<_>, _>>()?;
      let bodies: Vec<BodyId> = faces.iter().map(|f| f.body).collect();
      let r = add_feature(s, FeatureKind::CombineFaceGroups { faces })?;
      Ok(with_groups(s, r, &bodies))
    })(),
    "mesh.section_sketch" => section_sketch(s, p),
    "mesh.edit.start" => (|| {
      let fid = s.edit(|d| Ok(d.add_feature(FeatureKind::BaseFeature { bodies: Vec::new(), edits: Vec::new() })?))?;
      Ok(json!({"feature": fid.0}))
    })(),
    "mesh.edit.finish" => finish_edit(s, p),
    _ => return None,
  })
}

/// A command's result with the face group count of each body.
fn with_groups(s: &Session, mut r: Value, bodies: &[BodyId]) -> Value {
  let counts: Vec<Value> = bodies
    .iter()
    .map(|b| {
      let n = s.model.face_groups.get(b).map_or(0, |g| g.iter().max().map_or(0, |m| m + 1));
      json!({"body": b.0, "groups": n})
    })
    .collect();
  r["face_groups"] = json!(counts);
  r
}

/// Create Mesh Section Sketch: a sketch on the plane (moved `offset` along its normal) holding
/// the lines where it cuts the body, as one undo step.
fn section_sketch(s: &mut Session, p: &Value) -> CmdResult {
  let body = body(s, p, "body")?;
  let plane = p.get("plane").ok_or(CmdError::new("missing 'plane'"))?.clone();
  let offset =
    opt_expr(p, "offset").filter(|o| fox_expr::eval(o, fox_expr::Kind::Length, &s.model.params).ok() != Some(0.0));
  let own = !s.in_transaction();
  if own {
    s.begin_transaction()?;
  }
  let r = (|| {
    let plane = match offset {
      Some(o) => {
        let made = crate::commands::run(s, "plane.offset", &json!({"plane": plane, "offset": o}))?;
        let id =
          made.get("feature").or(made.get("id")).and_then(Value::as_u64).ok_or(CmdError::new("no offset plane"))?;
        plane_ref(s, &json!({"feature": id}))?
      }
      None => plane_ref(s, &plane)?,
    };
    let sk =
      s.edit(|d| Ok(d.add_feature(FeatureKind::Sketch { plane, sketch: Sketch::new(), links: Vec::new() })?))?;
    let out = crate::commands::run(s, "sketch.intersect", &json!({"sketch": sk.0, "bodies": [body.0]}))?;
    Ok(json!({"sketch": sk.0, "curves": out["curves"]}))
  })();
  if own {
    match r {
      Ok(_) => {
        s.commit_transaction()?;
      }
      Err(_) => {
        s.rollback_transaction()?;
      }
    }
  }
  r
}

/// Finish Direct Editing: the edited mesh body is kept as a new body, "<name>_Edited", in the
/// base feature the environment started, and the timeline marker goes back to the end.
fn finish_edit(s: &mut Session, p: &Value) -> CmdResult {
  let feature = p.get("feature").and_then(Value::as_u64).map(|f| FeatureId(f as u32));
  let fid = feature
    .or_else(|| {
      s.design.features.iter().rev().find(|f| matches!(f.kind, FeatureKind::BaseFeature { .. })).map(|f| f.id)
    })
    .ok_or(CmdError::new("no base feature is open"))?;
  let edited = match p.get("body").and_then(Value::as_u64) {
    Some(b) => {
      let b = s.model.body(BodyId(b as u32)).ok_or_else(|| CmdError::new(format!("no body {b}")))?;
      Some((format!("{}{EDITED}", b.name), b.shape.to_brep()?))
    }
    None => None,
  };
  let made = s.edit(|d| {
    let mut made = None;
    if let Some((name, brep)) = edited {
      let Some(FeatureKind::BaseFeature { bodies, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) else {
        return Err(CmdError::new("that feature is not a base feature"));
      };
      let id = BodyId(fid.0 * 1000 + bodies.len() as u32);
      bodies.push(BaseBody { id, name, brep, mesh: true });
      made = Some(id);
    }
    d.set_marker(None);
    Ok(made)
  })?;
  Ok(json!({"feature": fid.0, "body": made.map(|b| b.0), "errors": s.model.errors()}))
}
