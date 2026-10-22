//! The mesh tools' commands, with the settings of Fusion's dialogs: Reduce, Smooth, Convert Mesh
//! and Plane Cut.

use fox_model::construct::{ConvertMethod, FillType, MeshOp, MeshSettings, PlaneCutType, ReduceType};
use fox_model::{BodyId, FeatureKind};
use serde_json::Value;

use crate::commands::{add_feature, ids, opt_expr, opt_str, plane_ref};
use crate::{CmdError, CmdResult, Session};

/// `body` or `bodies`, each one there.
fn bodies(s: &Session, p: &Value) -> Result<Vec<BodyId>, CmdError> {
  let mut out: Vec<BodyId> = ids(p, "bodies").into_iter().map(BodyId).collect();
  if let Some(b) = p.get("body").and_then(Value::as_u64) {
    out.push(BodyId(b as u32));
  }
  if out.is_empty() {
    return Err(CmdError::new("missing 'body' (or 'bodies')"));
  }
  if let Some(b) = out.iter().find(|b| s.model.body(**b).is_none()) {
    return Err(CmdError::new(format!("no body {}", b.0)));
  }
  Ok(out)
}

fn choice<T: Copy>(p: &Value, k: &str, options: &[(&str, T)], default: T) -> Result<T, CmdError> {
  match opt_str(p, k) {
    None => Ok(default),
    Some(v) => options.iter().find(|(n, _)| *n == v).map(|(_, t)| *t).ok_or_else(|| {
      let names: Vec<&str> = options.iter().map(|(n, _)| *n).collect();
      CmdError::new(format!("unknown {k} '{v}' ({})", names.join(", ")))
    }),
  }
}

/// `mesh.reduce`, `mesh.smooth` and `mesh.convert`. Reduce with only `keep` and Smooth with
/// only `passes` work as they did before Fusion's dialogs (a fraction kept, Laplacian passes).
pub(crate) fn mesh_tool(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let bodies = bodies(s, p)?;
  let legacy = |keys: &[&str], old: &str| p.get(old).is_some() && !keys.iter().any(|k| p.get(*k).is_some());
  let (op, amount, settings) = match name {
    "mesh.reduce" if legacy(&["type", "proportion", "tolerance", "face_count", "remesh"], "keep") => {
      (MeshOp::Reduce, opt_expr(p, "keep").unwrap_or_else(|| "0.5".into()), None)
    }
    "mesh.smooth" if legacy(&["smoothness"], "passes") => {
      (MeshOp::Smooth, opt_expr(p, "passes").unwrap_or_else(|| "3".into()), None)
    }
    "mesh.reduce" => {
      let reduce = choice(
        p,
        "type",
        &[
          ("tolerance", ReduceType::Tolerance),
          ("proportion", ReduceType::Proportion),
          ("face_count", ReduceType::FaceCount),
        ],
        ReduceType::Proportion,
      )?;
      let uniform = choice(p, "remesh", &[("adaptive", false), ("uniform", true)], false)?;
      let amount = match reduce {
        ReduceType::Tolerance => opt_expr(p, "tolerance").unwrap_or_else(|| "0.1 mm".into()),
        ReduceType::Proportion => opt_expr(p, "proportion").unwrap_or_else(|| "25".into()),
        ReduceType::FaceCount => opt_expr(p, "face_count").ok_or(CmdError::new("missing 'face_count'"))?,
      };
      (MeshOp::Reduce, amount, Some(MeshSettings { reduce, uniform, ..Default::default() }))
    }
    "mesh.smooth" => {
      (MeshOp::Smooth, opt_expr(p, "smoothness").unwrap_or_else(|| "1".into()), Some(MeshSettings::default()))
    }
    _ => {
      let method = choice(
        p,
        "method",
        &[
          ("faceted", ConvertMethod::Faceted),
          ("prismatic", ConvertMethod::Prismatic),
          ("organic", ConvertMethod::Organic),
        ],
        ConvertMethod::Faceted,
      )?;
      let base_feature = choice(p, "operation", &[("parametric", false), ("base_feature", true)], false)?;
      (MeshOp::Convert, "1".into(), Some(MeshSettings { method, base_feature, ..Default::default() }))
    }
  };
  let convert = op == MeshOp::Convert;
  let r = add_feature(s, FeatureKind::Mesh { bodies: bodies.clone(), op, amount, settings })?;
  // Convert Mesh leaves the mesh body, hidden, beside the new solid, as Fusion does (undo
  // takes back both)
  if convert && r["error"].is_null() {
    for b in bodies {
      s.design.body_settings_mut(b).visible = false;
    }
  }
  Ok(r)
}

/// `mesh.plane_cut`: Fusion's Plane Cut on one mesh body.
pub(crate) fn plane_cut(s: &mut Session, p: &Value) -> CmdResult {
  let body = *bodies(s, p)?.first().ok_or(CmdError::new("missing 'body'"))?;
  let plane = plane_ref(s, p.get("plane").ok_or(CmdError::new("missing 'plane' (the cut plane)"))?)?;
  let cut = choice(
    p,
    "type",
    &[("trim", PlaneCutType::Trim), ("split_body", PlaneCutType::SplitBody), ("split_faces", PlaneCutType::SplitFaces)],
    PlaneCutType::Trim,
  )?;
  let fill = choice(
    p,
    "fill",
    &[("none", FillType::None), ("uniform", FillType::Uniform), ("minimal", FillType::Minimal)],
    FillType::Uniform,
  )?;
  let zero = || "0".to_string();
  add_feature(
    s,
    FeatureKind::MeshPlaneCut {
      body,
      plane,
      distance: opt_expr(p, "distance").unwrap_or_else(zero),
      y_angle: opt_expr(p, "y_angle").unwrap_or_else(zero),
      z_angle: opt_expr(p, "z_angle").unwrap_or_else(zero),
      flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
      cut,
      fill,
    },
  )
}
