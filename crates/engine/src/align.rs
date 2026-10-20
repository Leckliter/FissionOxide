//! Align (assembly) constraints: a moving component's face, axis or point made flush, concentric
//! or coincident with one on a fixed component, moving it only as far as that takes.

use fox_model::assembly::{AlignRelation, JointGeometry};
use fox_model::{ComponentId, FeatureKind};
use serde_json::Value;

use crate::commands::{opt_expr, opt_str};
use crate::mates::{feature_json, rename};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "align" => align(s, p),
    _ => return None,
  })
}

/// The component `k` names, else the one the geometry lies on (or its saved origin's).
fn component_for(s: &Session, p: &Value, k: &str, g: &JointGeometry) -> Result<Option<ComponentId>, CmdError> {
  match p.get(k) {
    Some(Value::Null) => Ok(None),
    Some(v) => {
      let id = ComponentId(v.as_u64().ok_or(CmdError::new(format!("'{k}' must be a component id")))? as u32);
      s.design.component(id).ok_or(CmdError::new(format!("no component {}", id.0)))?;
      Ok(Some(id))
    }
    None => {
      if let Some(o) = g.origin() {
        return Ok(crate::mates::origin_component(s, o));
      }
      Ok(g.body().and_then(|b| s.design.component_of(b)).map(|c| c.id))
    }
  }
}

fn align(s: &mut Session, p: &Value) -> CmdResult {
  let geometry = |k: &str| crate::assembly::geometry(s, p.get(k).ok_or(CmdError::new(format!("missing '{k}'")))?);
  let moving_geometry = geometry("moving_geometry")?;
  let fixed_geometry = geometry("fixed_geometry")?;
  let moving = component_for(s, p, "moving", &moving_geometry)?
    .ok_or(CmdError::new("missing 'moving' (the moving geometry isn't on a component)"))?;
  let fixed = component_for(s, p, "fixed", &fixed_geometry)?;
  if fixed == Some(moving) {
    return Err(CmdError::new("both geometries are on the same component"));
  }
  let relation = match opt_str(p, "relation") {
    None => AlignRelation::Auto,
    Some(r) => AlignRelation::parse(r)
      .ok_or(CmdError::new(format!("unknown relation '{r}' (auto, flush, coincident, concentric)")))?,
  };
  let kind = FeatureKind::AlignConstraint {
    moving,
    moving_geometry,
    fixed,
    fixed_geometry,
    relation,
    offset: opt_expr(p, "offset").unwrap_or_else(|| "0".into()),
    flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
  };
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  rename(s, id, p)?;
  Ok(feature_json(s, id))
}
