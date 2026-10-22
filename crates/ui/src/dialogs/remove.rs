//! Fusion's REMOVE takes components as well as bodies. A component is picked as all of its
//! bodies (a click on its browser row, or Select Component Priority in the canvas), and goes to
//! `body.remove` as a component, so its row leaves the browser with its bodies.

use fox_engine::Session;
use fox_engine::model::{BodyId, ComponentId};
use serde_json::{Value, json};

use super::{Dialog, Kind};

/// The top-most components all of whose bodies (theirs and those below them, at least one) are
/// in `bodies`.
pub(crate) fn whole_components(s: &Session, bodies: &[u32]) -> Vec<u32> {
  let all = &s.design.components;
  let whole = |c: ComponentId| {
    let b = fox_engine::model::assembly::bodies_below(all, &[c]);
    !b.is_empty() && b.iter().all(|x| bodies.contains(&x.0))
  };
  all
    .iter()
    .filter(|c| whole(c.id) && !c.parent.is_some_and(|p| s.design.component(p).is_some() && whole(p)))
    .map(|c| c.id.0)
    .collect()
}

/// `body.remove`'s parameters for picked bodies: whole components as components, the rest as
/// bodies.
pub(crate) fn remove_params(s: &Session, bodies: &[u32]) -> Value {
  let comps = whole_components(s, bodies);
  let ids: Vec<ComponentId> = comps.iter().map(|c| ComponentId(*c)).collect();
  let taken = fox_engine::model::assembly::bodies_below(&s.design.components, &ids);
  let rest: Vec<u32> = bodies.iter().copied().filter(|b| !taken.contains(&BodyId(*b))).collect();
  if comps.is_empty() { json!({"bodies": rest}) } else { json!({"bodies": rest, "components": comps}) }
}

impl Dialog {
  /// REMOVE's picked components, noted while the session is at hand (the command is built
  /// without it).
  pub(super) fn sync_remove(&mut self, s: &Session) {
    if !matches!(self.kind, Kind::Form(f) if std::ptr::eq(f, &crate::forms::REMOVE)) {
      return;
    }
    let bodies: Vec<u32> = self
      .form
      .values
      .first()
      .and_then(Value::as_array)
      .map_or(Vec::new(), |a| a.iter().filter_map(Value::as_u64).map(|b| b as u32).collect());
    let p = remove_params(s, &bodies);
    self.comps = p
      .get("components")
      .and_then(Value::as_array)
      .map_or(Vec::new(), |a| a.iter().map(|c| c.as_u64().map(|c| c as u32)).collect());
  }

  /// REMOVE's command with its picked components (their bodies, listed too, go with them).
  pub(super) fn with_removed_components(&self, cmd: (&'static str, Value)) -> (&'static str, Value) {
    let (name, mut p) = cmd;
    if name == "body.remove" && !self.comps.is_empty() {
      p["components"] = json!(self.comps.iter().flatten().collect::<Vec<_>>());
    }
    (name, p)
  }
}
