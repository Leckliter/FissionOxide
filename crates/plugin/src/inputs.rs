//! A command's dialog values: checked against the manifest's inputs, with defaults filled in,
//! before they reach the plugin as JSON.
//!
//! Values reach the plugin as: selection → a list of picks (`{"type": "face", "body": 1,
//! "face": 3}` as the `selection` command returns them); value → a number in mm, degrees or
//! plain (expressions such as `"0.5 in"` or `"width / 2"` are evaluated); integer → an integer;
//! slider → a number; checkbox → a bool; choice and text → a string. Buttons, labels and groups
//! carry no value.

use fox_engine::{Sel, Session};
use serde_json::{Map, Value, json};

use crate::manifest::{Command, Input, InputKind, Unit};

/// Every value input, checked, with defaults for the ones not given. A missing selection is taken
/// from the current selection (the picks that pass its filter).
pub fn resolve(cmd: &Command, given: &Value, s: &Session) -> Result<Value, String> {
  let given = match given {
    Value::Null => Map::new(),
    Value::Object(m) => m.clone(),
    _ => return Err("inputs must be a JSON object".into()),
  };
  if let Some(k) = given.keys().find(|k| !cmd.inputs.iter().any(|i| &i.id == *k && i.kind.has_value())) {
    return Err(format!("'{}' has no input '{k}'", cmd.label()));
  }
  let mut out = Map::new();
  for i in cmd.inputs.iter().filter(|i| i.kind.has_value()) {
    let v = value(i, given.get(&i.id), s).map_err(|e| format!("{}: {e}", i.label()))?;
    out.insert(i.id.clone(), v);
  }
  Ok(Value::Object(out))
}

/// Like [`resolve`], keeping what can be read and skipping the rest (for `input_changed` while
/// the dialog is being filled in).
pub fn resolve_partial(cmd: &Command, given: &Value, s: &Session) -> Value {
  let mut out = Map::new();
  for i in cmd.inputs.iter().filter(|i| i.kind.has_value()) {
    if let Ok(v) = value(i, given.get(&i.id), s) {
      out.insert(i.id.clone(), v);
    }
  }
  Value::Object(out)
}

/// The value an input starts with in a dialog (selections start empty).
pub fn initial(i: &Input) -> Value {
  match (i.kind, &i.default) {
    (InputKind::Selection, _) => json!([]),
    (_, Some(d)) => d.clone(),
    (InputKind::Value, None) => json!(0),
    (InputKind::Integer | InputKind::Slider, None) => json!(i.min.unwrap_or(0.0)),
    (InputKind::Checkbox, None) => json!(false),
    (InputKind::Choice, None) => json!(i.options.first()),
    (InputKind::Text | InputKind::Label, None) => json!(""),
    _ => Value::Null,
  }
}

fn value(i: &Input, v: Option<&Value>, s: &Session) -> Result<Value, String> {
  let v = v.cloned().filter(|v| !v.is_null());
  match i.kind {
    InputKind::Selection => {
      let picks: Vec<Sel> = match v {
        Some(Value::Array(a)) => {
          a.into_iter().map(|p| serde_json::from_value(p).map_err(|e| format!("bad pick: {e}"))).collect::<Result<_, _>>()?
        }
        Some(p @ Value::Object(_)) => vec![serde_json::from_value(p).map_err(|e| format!("bad pick: {e}"))?],
        Some(_) => return Err("expected picks like {\"type\": \"body\", \"body\": 1}".into()),
        None => s.selection.iter().copied().filter(|p| accepts(i, p, s)).collect(),
      };
      if let Some(p) = picks.iter().find(|p| !accepts(i, p, s)) {
        return Err(format!("can't pick a {} here (wants {})", filter_of(p, s), i.filter.join(", ")));
      }
      let (min, max) = (i.min_count(), i.max_count());
      if picks.len() < min {
        return Err(format!("select at least {min}"));
      }
      if max.is_some_and(|m| picks.len() > m) {
        return Err(format!("select at most {}", max.unwrap_or(0)));
      }
      serde_json::to_value(picks).map_err(|e| e.to_string())
    }
    InputKind::Value => {
      let unit = i.unit.unwrap_or_default();
      let v = v.or_else(|| i.default.clone()).unwrap_or(json!(0));
      let n = match &v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(e) => {
          let kind = match unit {
            Unit::Length => fox_engine::expr::Kind::Length,
            Unit::Angle => fox_engine::expr::Kind::Angle,
            Unit::Unitless => fox_engine::expr::Kind::Unitless,
          };
          let x = fox_engine::expr::eval(e, kind, &s.model.params).map_err(|err| format!("'{e}': {err}"))?;
          if unit == Unit::Angle { x.to_degrees() } else { x }
        }
        _ => return Err("expected a number or an expression".into()),
      };
      in_range(i, n)?;
      Ok(json!(n))
    }
    InputKind::Integer => {
      let v = v.or_else(|| i.default.clone()).unwrap_or(json!(i.min.unwrap_or(0.0)));
      let n = v.as_f64().ok_or("expected a whole number")?;
      if n.fract() != 0.0 {
        return Err("expected a whole number".into());
      }
      in_range(i, n)?;
      Ok(json!(n as i64))
    }
    InputKind::Slider => {
      let n = v.or_else(|| i.default.clone()).unwrap_or(json!(i.min.unwrap_or(0.0))).as_f64().ok_or("expected a number")?;
      in_range(i, n)?;
      Ok(json!(n))
    }
    InputKind::Checkbox => Ok(json!(v.or_else(|| i.default.clone()).unwrap_or(json!(false)).as_bool().ok_or("expected true or false")?)),
    InputKind::Choice => {
      let c = v.or_else(|| i.default.clone()).unwrap_or_else(|| json!(i.options[0]));
      match c.as_str() {
        Some(c) if i.options.iter().any(|o| o == c) => Ok(json!(c)),
        _ => Err(format!("expected one of {}", i.options.join(", "))),
      }
    }
    InputKind::Text => Ok(json!(v.or_else(|| i.default.clone()).unwrap_or(json!("")).as_str().ok_or("expected text")?)),
    InputKind::Button | InputKind::Label | InputKind::Group => Ok(Value::Null),
  }
}

fn in_range(i: &Input, n: f64) -> Result<(), String> {
  if !n.is_finite() {
    return Err("not a finite number".into());
  }
  if let Some(lo) = i.min
    && n < lo
  {
    return Err(format!("must be at least {lo}"));
  }
  if let Some(hi) = i.max
    && n > hi
  {
    return Err(format!("must be at most {hi}"));
  }
  Ok(())
}

/// What a pick is, in selection filter terms.
pub fn filter_of(p: &Sel, s: &Session) -> &'static str {
  match p {
    Sel::Body { .. } => "body",
    Sel::Face { .. } => "face",
    Sel::Edge { .. } => "edge",
    Sel::Vertex { .. } => "vertex",
    Sel::Profile { .. } => "profile",
    Sel::SketchPoint { .. } => "point",
    Sel::SketchCurve { .. } => "curve",
    Sel::SketchConstraint { .. } => "constraint",
    Sel::Feature { feature } if s.design.sketch(*feature).is_ok() => "sketch",
    Sel::Feature { .. } => "feature",
  }
}

pub fn accepts(i: &Input, p: &Sel, s: &Session) -> bool {
  let f = filter_of(p, s);
  if i.filter.is_empty() {
    return f != "constraint";
  }
  i.filter.iter().any(|w| w == f || (w == "feature" && f == "sketch"))
}

/// Resolved values as a built-in command's parameters: a selection limited to one pick passes
/// the pick itself (a body, sketch or feature as its id; a face, edge ... as `{body, face}` and
/// so on), others a list of them.
pub fn as_params(cmd: &Command, values: &Value) -> Value {
  let mut out = values.as_object().cloned().unwrap_or_default();
  for i in cmd.inputs.iter().filter(|i| i.kind == InputKind::Selection) {
    let Some(Value::Array(picks)) = out.get(&i.id).cloned() else { continue };
    let picks: Vec<Value> = picks.into_iter().map(pick_param).collect();
    let v = match (i.max_count(), picks.as_slice()) {
      (Some(1), [one]) => one.clone(),
      (Some(1), []) => Value::Null,
      _ => Value::Array(picks),
    };
    out.insert(i.id.clone(), v);
  }
  out.retain(|_, v| !v.is_null());
  Value::Object(out)
}

fn pick_param(p: Value) -> Value {
  let Value::Object(mut m) = p else { return p };
  let ty = m.remove("type");
  match (ty.as_ref().and_then(Value::as_str), m.len()) {
    (Some("body"), 1) => m.remove("body").unwrap_or(Value::Null),
    (Some("feature"), 1) => m.remove("feature").unwrap_or(Value::Null),
    _ => Value::Object(m),
  }
}
