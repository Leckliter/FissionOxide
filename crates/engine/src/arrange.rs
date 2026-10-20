//! The arrange command: copies of components packed flat on an envelope for manufacturing (see
//! [`fox_model::arrange`]).

use fox_model::arrange::{ArrangeItem, ArrangeRotation, ArrangeSolver};
use fox_model::{BodyId, ComponentId, Design, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{opt_expr, opt_str, plane_ref, req_expr};
use crate::{CmdError, CmdResult, Session};

/// Bodies per feature in regenerated body ids (as `fox_model::regen`).
const MAX_BODIES_PER_FEATURE: u32 = 1000;

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "arrange" => arrange(s, p),
    _ => return None,
  })
}

fn rotation(v: &Value) -> Result<ArrangeRotation, CmdError> {
  let r = match v {
    Value::Number(n) => n.to_string(),
    Value::String(t) => t.to_ascii_lowercase(),
    _ => return Err(CmdError::new("'rotation' must be none, 180, 90 or all")),
  };
  Ok(match r.as_str() {
    "none" | "0" => ArrangeRotation::None,
    "180" => ArrangeRotation::Half,
    "90" | "90/270" | "quarter" => ArrangeRotation::Quarter,
    "all" => ArrangeRotation::All,
    o => return Err(CmdError::new(format!("unknown rotation '{o}' (none, 180, 90, all)"))),
  })
}

fn arrange(s: &mut Session, p: &Value) -> CmdResult {
  let all = p.get("rotation").map(rotation).transpose()?.unwrap_or_default();
  let list = p.get("components").and_then(Value::as_array).ok_or(CmdError::new("missing 'components'"))?;
  let mut items = Vec::new();
  for v in list {
    let (c, rot, quantity) = match v {
      Value::Object(_) => (
        v.get("component"),
        v.get("rotation").map(rotation).transpose()?.unwrap_or(all),
        v.get("quantity").map(|q| q.as_u64().filter(|n| (1..=1000).contains(n))).unwrap_or(Some(1)),
      ),
      _ => (Some(v), all, Some(1)),
    };
    let id =
      c.and_then(Value::as_u64).ok_or(CmdError::new("each component is an id or {component, rotation?, quantity?}"))?;
    let component = ComponentId(u32::try_from(id).map_err(|_| CmdError::new("bad component"))?);
    s.design.component(component).ok_or(CmdError::new(format!("no component {}", component.0)))?;
    let quantity = quantity.ok_or(CmdError::new("'quantity' must be 1 to 1000"))? as u32;
    items.push(ArrangeItem { component, rotation: rot, quantity });
  }
  if items.is_empty() {
    return Err(CmdError::new("select components to arrange"));
  }
  let solver = match opt_str(p, "solver").map(str::to_ascii_lowercase).as_deref() {
    None | Some("rectangular" | "2d_rectangular") => ArrangeSolver::Rectangular,
    Some("true_shape" | "true-shape" | "2d_true_shape") => ArrangeSolver::TrueShape,
    Some(o) => return Err(CmdError::new(format!("unknown solver '{o}' (rectangular, true_shape)"))),
  };
  let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
  let kind = FeatureKind::Arrange {
    items: items.clone(),
    plane,
    length: req_expr(p, "length")?,
    width: req_expr(p, "width")?,
    solver,
    spacing: opt_expr(p, "spacing").unwrap_or_else(|| "0".into()),
    frame: opt_expr(p, "frame").unwrap_or_else(|| "0".into()),
  };
  // how many bodies each copy has: the feature makes them in this order (see `apply_arrange`)
  let counts: Vec<usize> = items
    .iter()
    .map(|it| {
      subtree(&s.design, it.component)
        .iter()
        .filter_map(|c| s.design.component(*c))
        .flat_map(|c| c.bodies.iter())
        .filter(|b| s.model.body(**b).is_some())
        .count()
    })
    .collect();
  let (fid, top, envelope, placed) = s.edit(|d| {
    let fid = d.add_feature(kind)?;
    let fname = d.feature(fid).map(|f| f.name.clone()).unwrap_or_else(|| "Arrange".into());
    let n = fname.trim_start_matches("Arrange");
    let top = d.add_component(&fname, Vec::new())?;
    let envelope = d.add_component(&format!("Envelope{n}"), Vec::new())?;
    if let Some(c) = d.component_mut(envelope) {
      c.parent = Some(top);
    }
    let mut k = 0;
    let mut placed = Vec::new();
    for (it, count) in items.iter().zip(&counts) {
      let (base, material) =
        d.component(it.component).map(|c| (c.name.clone(), c.material.clone())).unwrap_or_default();
      for q in 0..it.quantity {
        let bodies: Vec<BodyId> =
          (0..*count).map(|j| BodyId(fid.0 * MAX_BODIES_PER_FEATURE + (k + j) as u32)).collect();
        k += count;
        let cid = d.add_component(&format!("{base}:{}", q + 1), bodies)?;
        if let Some(c) = d.component_mut(cid) {
          c.parent = Some(envelope);
          c.material = material.clone();
        }
        placed.push(cid.0);
      }
    }
    Ok((fid, top, envelope, placed))
  })?;
  let st = s.model.status(fid);
  Ok(json!({
    "feature": fid.0,
    "component": top.0,
    "envelope": envelope.0,
    "components": placed,
    "error": st.and_then(|x| x.error.clone()),
  }))
}

/// A component and everything below it.
fn subtree(d: &Design, c: ComponentId) -> Vec<ComponentId> {
  let mut out = vec![c];
  let mut i = 0;
  while i < out.len() {
    let p = out[i];
    out.extend(
      d.components.iter().filter(|k| k.parent == Some(p) && !out.contains(&k.id)).map(|k| k.id).collect::<Vec<_>>(),
    );
    i += 1;
  }
  out
}
