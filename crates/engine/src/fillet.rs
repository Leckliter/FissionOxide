//! The "radii" option of the fillet command: radii at points along each edge.

use fox_model::RadiusPoint;
use serde_json::Value;

use crate::CmdError;
use crate::commands::expr_of;

/// `"radii": [[at, radius], ...]` or `[{"at": .., "radius": ..}, ...]`, `at` running from 0 at
/// each edge's start to 1 at its end.
pub(crate) fn radius_points(p: &Value) -> Result<Vec<RadiusPoint>, CmdError> {
  let Some(list) = p.get("radii") else { return Ok(Vec::new()) };
  let list = list.as_array().ok_or(CmdError::new("'radii' must be a list of [at, radius] points"))?;
  let mut out = Vec::new();
  for v in list {
    let (at, r) = match v {
      Value::Array(a) if a.len() == 2 => (&a[0], &a[1]),
      Value::Object(o) => match (o.get("at"), o.get("radius")) {
        (Some(at), Some(r)) => (at, r),
        _ => return Err(CmdError::new("a radius point needs 'at' and 'radius'")),
      },
      _ => return Err(CmdError::new("a radius point is [at, radius]")),
    };
    let at = at.as_f64().filter(|t| (0.0..=1.0).contains(t)).ok_or(CmdError::new("'at' must be from 0 to 1"))?;
    out.push(RadiusPoint::new(at, expr_of(r)?));
  }
  Ok(out)
}
