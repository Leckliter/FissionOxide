//! MESH > MODIFY commands that rebuild mesh bodies: Remesh, Shell, Combine, Separate and Scale
//! Mesh (Reduce, Smooth and Convert Mesh are in `features`).

use fox_model::construct::MeshCombineOp;
use fox_model::{BodyId, BodySel, FeatureKind};
use serde_json::Value;

use crate::commands::{add_feature, ids, opt_expr, opt_str, req_expr, req_u32};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "mesh.remesh" => (|| {
      let bodies = bodies(s, p, "bodies")?;
      let adaptive = match opt_str(p, "type").unwrap_or("adaptive") {
        "adaptive" => true,
        "uniform" => false,
        o => return Err(CmdError::new(format!("unknown type '{o}' (adaptive, uniform)"))),
      };
      let edge_length = match opt_str(p, "method").unwrap_or("density") {
        "density" => None,
        "edge_length" => Some(req_expr(p, "edge_length")?),
        o => return Err(CmdError::new(format!("unknown method '{o}' (density, edge_length)"))),
      };
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      add_feature(
        s,
        FeatureKind::MeshRemesh {
          bodies,
          adaptive,
          edge_length,
          density: opt_expr(p, "density").unwrap_or_else(|| "0.25".into()),
          shape: opt_expr(p, "shape").unwrap_or_else(|| "0.5".into()),
          sharp: flag("sharp"),
          boundaries: flag("boundaries"),
        },
      )
    })(),
    "mesh.shell" => (|| {
      let bodies = bodies(s, p, "bodies")?;
      add_feature(s, FeatureKind::MeshShell { bodies, thickness: req_expr(p, "thickness")? })
    })(),
    "mesh.combine" => (|| {
      let target = BodyId(req_u32(p, "target")?);
      bodies(s, &serde_json::json!({"t": [target.0]}), "t")?;
      let tools: Vec<BodyId> = bodies(s, p, "tools")?.into_iter().filter(|t| *t != target).collect();
      if tools.is_empty() {
        return Err(CmdError::new("pick tool bodies other than the target"));
      }
      let operation = match opt_str(p, "operation").unwrap_or("join") {
        "join" => MeshCombineOp::Join,
        "cut" => MeshCombineOp::Cut,
        "intersect" => MeshCombineOp::Intersect,
        "merge" => MeshCombineOp::Merge,
        o => return Err(CmdError::new(format!("unknown operation '{o}' (join, cut, intersect, merge)"))),
      };
      let keep_tools = p.get("keep_tools").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::MeshCombine { target, tools, operation, keep_tools })
    })(),
    "mesh.separate" => (|| {
      let body = match p.get("body") {
        Some(b) => BodyId(b.as_u64().ok_or(CmdError::new("'body' must be a body id"))? as u32),
        None => *bodies(s, p, "bodies")?.first().ok_or(CmdError::new("missing 'body'"))?,
      };
      bodies(s, &serde_json::json!({"b": [body.0]}), "b")?;
      match opt_str(p, "type").unwrap_or("shells") {
        "shells" => {}
        o => return Err(CmdError::new(format!("unknown type '{o}' (shells)"))),
      }
      let keep_body = p.get("keep_body").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::MeshSeparate { body, keep_body })
    })(),
    "mesh.scale" => (|| {
      let bodies = bodies(s, p, "bodies")?;
      // the point is the bodies' centre unless one is given, as Fusion fills it in
      let center = match p.get("center").and_then(Value::as_array) {
        Some(c) => {
          let v: Vec<f64> = c.iter().filter_map(Value::as_f64).collect();
          <[f64; 3]>::try_from(v.as_slice()).map_err(|_| CmdError::new("'center' needs three numbers"))?
        }
        None => center_of(s, &bodies),
      };
      let factors = match opt_str(p, "scale_type").unwrap_or("uniform") {
        "uniform" => None,
        "non_uniform" => Some([req_expr(p, "x")?, req_expr(p, "y")?, req_expr(p, "z")?]),
        o => return Err(CmdError::new(format!("unknown scale_type '{o}' (uniform, non_uniform)"))),
      };
      let factor = opt_expr(p, "factor").unwrap_or_else(|| "1".into());
      add_feature(s, FeatureKind::Scale { bodies: BodySel::Bodies { ids: bodies }, center, factor, factors })
    })(),
    _ => return None,
  })
}

/// The bodies under `k`, each checked to exist.
fn bodies(s: &Session, p: &Value, k: &str) -> Result<Vec<BodyId>, CmdError> {
  let bodies: Vec<BodyId> = ids(p, k).into_iter().map(BodyId).collect();
  if bodies.is_empty() {
    return Err(CmdError::new(format!("missing '{k}'")));
  }
  for b in &bodies {
    if s.model.body(*b).is_none() {
      return Err(CmdError::new(format!("no body {}", b.0)));
    }
  }
  Ok(bodies)
}

/// The middle of the bodies' bounding box.
fn center_of(s: &Session, bodies: &[BodyId]) -> [f64; 3] {
  let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
  for b in bodies {
    if let Some(m) = s.model.body(*b).and_then(|b| b.shape.mass().ok()) {
      for k in 0..3 {
        lo[k] = lo[k].min(m.bbox_min[k]);
        hi[k] = hi[k].max(m.bbox_max[k]);
      }
    }
  }
  if !lo[0].is_finite() {
    return [0.0; 3];
  }
  [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0)
}
