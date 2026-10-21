//! New Component on body-making features (Extrude, Revolve, Loft, Sweep, the primitives, …):
//! the component the feature creates, and `component.next_number` for tools that number their
//! components (`01`, `02 - Letters`, …). See [`fox_model::newcomp`].

use fox_model::{ComponentId, Design, FeatureId, Operation};
use serde_json::{Value, json};

use crate::commands::opt_str;
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "component.next_number" => (|| {
      let digits = match p.get("prefix_digits") {
        None | Some(Value::Null) => 2,
        Some(v) => v.as_u64().filter(|d| (1..=9).contains(d)).ok_or(CmdError::new("prefix_digits is 1 to 9"))?,
      };
      let (n, prefix) = fox_model::newcomp::next_number(&s.design, digits as usize);
      Ok(json!({"number": n, "prefix": prefix}))
    })(),
    _ => return None,
  })
}

/// Gives a just-added New Component feature its component: named `name` (or the next default
/// name), under `parent` (the active component) or at the top level. Other features are left as
/// they are.
pub(crate) fn attach(
  d: &mut Design,
  feature: FeatureId,
  name: Option<&str>,
  parent: Option<ComponentId>,
) -> Result<(), CmdError> {
  let Some(f) = d.feature(feature) else { return Ok(()) };
  if fox_model::newcomp::operation(&f.kind) != Some(Operation::NewComponent) {
    return Ok(());
  }
  let parent = parent.filter(|c| d.component(*c).is_some());
  let id = d.add_component(name.unwrap_or("").trim(), Vec::new())?;
  if let Some(c) = d.component_mut(id) {
    c.parent = parent;
  }
  if let Some(f) = d.feature_mut(feature) {
    f.component = Some(id);
  }
  Ok(())
}

/// The `component_name` parameter of a feature command.
pub(crate) fn name_param(p: &Value) -> Option<&str> {
  opt_str(p, "component_name").filter(|n| !n.trim().is_empty())
}
