//! Fusion's REMOVE takes components as well as bodies. A component is picked as all of its
//! bodies (a click on its browser row, or Select Component Priority in the canvas), and goes to
//! `body.remove` as a component, so its row leaves the browser with its bodies. A component with
//! no bodies (nothing to pick it by in the canvas) is picked as itself from its browser row.

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

/// Does component `cid` exist without a body in it or below it?
fn empty_component(s: &Session, cid: u32) -> bool {
  s.design.component(ComponentId(cid)).is_some()
    && fox_engine::model::assembly::bodies_below(&s.design.components, &[ComponentId(cid)]).is_empty()
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
  pub(super) fn is_remove(&self) -> bool {
    matches!(self.kind, Kind::Form(f) if std::ptr::eq(f, &crate::forms::REMOVE))
  }

  /// A component's browser row clicked while REMOVE is open: a component with no bodies is
  /// picked as itself (or unpicked, when it was). Returns whether REMOVE took the click.
  pub(crate) fn take_remove_component(&mut self, s: &Session, cid: u32) -> bool {
    if !self.is_remove() || !empty_component(s, cid) {
      return false;
    }
    match self.comps.iter().position(|c| *c == Some(cid)) {
      Some(i) => {
        self.comps.remove(i);
      }
      None => self.comps.push(Some(cid)),
    }
    true
  }

  /// Is component `cid` one of REMOVE's picks?
  pub(crate) fn removes_component(&self, cid: u32) -> bool {
    self.is_remove() && self.comps.contains(&Some(cid))
  }

  /// REMOVE's empty components picked from the browser (counted on its Objects chip, and enough
  /// for OK).
  pub(super) fn removed_empty_components(&self, s: &Session) -> Vec<u32> {
    if !self.is_remove() {
      return Vec::new();
    }
    self.comps.iter().flatten().copied().filter(|c| empty_component(s, *c)).collect()
  }

  /// REMOVE's picked components, noted while the session is at hand (the command is built
  /// without it).
  pub(super) fn sync_remove(&mut self, s: &Session) {
    if !self.is_remove() {
      return;
    }
    let empty = self.removed_empty_components(s);
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
    // the empty components picked from the browser stay picked (no body holds them)
    self.comps.extend(empty.into_iter().map(Some));
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

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use super::super::{Dialog, Kind};
  use crate::App;

  /// `ui:dialogs:remove-components`: in an assembly with one new, empty component, a click on
  /// its browser row picks it for REMOVE's Objects (the chip names it, OK is ready, the command
  /// sends it as a component); a second click unpicks it. OK removes its browser row.
  #[test]
  fn remove_takes_an_empty_component_from_its_browser_row() {
    let mut app = App::with_session(Session::default());
    let r = app.run("component.create", json!({"name": "External1"})).unwrap();
    let c = r["component"].as_u64().unwrap() as u32;
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::REMOVE));
    assert!(!d.ready(), "Objects waits");
    assert!(d.take_remove_component(&app.s, c));
    d.sync_selection(&app.s);
    assert!(d.removes_component(c) && d.ready());
    assert_eq!(d.removed_empty_components(&app.s), [c]);
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["components"]), ("body.remove", &json!([c])));
    assert!(d.take_remove_component(&app.s, c), "a second click unpicks it");
    d.sync_selection(&app.s);
    assert!(!d.removes_component(c) && !d.ready());
    // another dialog leaves the row's click alone
    let mut other = Dialog::open(&mut app, Kind::Combine);
    assert!(!other.take_remove_component(&app.s, c));
    // OK: the component's row leaves the browser
    d.take_remove_component(&app.s, c);
    let (cmd, p) = d.command().unwrap();
    app.run(cmd, p).unwrap();
    let f = &app.s.design.features.last().unwrap().kind;
    let fox_engine::model::FeatureKind::RemoveBodies { components, .. } = f else { panic!("{f:?}") };
    assert_eq!(components, &[fox_engine::model::ComponentId(c)]);
  }
}
