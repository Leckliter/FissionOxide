//! Configuration commands (the table of named variants) and base features: capture design
//! history on/off and the direct edits that go into a base feature. See
//! `docs/CONFIGURATIONS.md`.

use fox_model::assembly::MotionAxis;
use fox_model::config::{Column, Configuration, MAX_CONFIGURATIONS, Override};
use fox_model::direct::{BaseBody, DirectEdit};
use fox_model::{BodyId, Design, FaceRef, FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{expr_of, face_ref, opt_str, parse_color, req_str, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "config.list" => Ok(list(s)),
    "config.create" => create(s, p),
    "config.delete" => (|| {
      let n = existing(s, p, "name")?;
      s.edit(|d| {
        d.configurations.list.retain(|c| c.name != n);
        if d.configurations.active.as_deref() == Some(n.as_str()) {
          d.configurations.active = None;
        }
        Ok(())
      })?;
      Ok(list(s))
    })(),
    "config.rename" => (|| {
      let n = existing(s, p, "name")?;
      let new = new_name(s, p, "new_name")?;
      s.edit(|d| {
        if let Some(c) = d.configurations.get_mut(&n) {
          c.name = new.clone();
        }
        if d.configurations.active.as_deref() == Some(n.as_str()) {
          d.configurations.active = Some(new.clone());
        }
        Ok(())
      })?;
      Ok(list(s))
    })(),
    "config.activate" => (|| {
      let n = match p.get("name") {
        None | Some(Value::Null) => None,
        Some(_) => Some(existing(s, p, "name")?),
      };
      s.edit(|d| {
        d.configurations.active = n.clone();
        Ok(())
      })?;
      Ok(json!({"active": n, "errors": s.model.errors(), "bodies": s.model.bodies.len()}))
    })(),
    "config.set" => set(s, p),
    "config.table" => Ok(table(&s.design)),
    // export one configuration without switching the session's
    "export" if p.get("configuration").is_some() => export(s, p),
    "design.history" => history(s, p),
    "base.start" => (|| {
      let id = s.edit(|d| Ok(d.add_feature(FeatureKind::BaseFeature { bodies: Vec::new(), edits: Vec::new() })?))?;
      Ok(status(s, id))
    })(),
    "base.edit" => (|| {
      let id = FeatureId(req_u32(p, "feature")?);
      let i = s.design.index_of(id).ok_or(CmdError::new(format!("no feature {}", id.0)))?;
      if !matches!(s.design.features[i].kind, FeatureKind::BaseFeature { .. }) {
        return Err(CmdError::new("that feature is not a base feature"));
      }
      s.edit(|d| {
        d.set_marker((i + 1 < d.features.len()).then_some(i + 1));
        Ok(())
      })?;
      Ok(status(s, id))
    })(),
    "base.finish" => (|| {
      let open = s.design.open_base_feature();
      s.edit(|d| {
        d.set_marker(None);
        Ok(())
      })?;
      Ok(json!({"feature": open.map(|f| f.0), "errors": s.model.errors()}))
    })(),
    "direct.move_face" => (|| {
      let faces = faces(s, p)?;
      let translate = lengths3(s, p, "translate")?;
      direct(s, DirectEdit::MoveFaces { faces, translate })
    })(),
    "direct.press_pull" => (|| {
      let faces = faces(s, p)?;
      let distance = length(s, p, "distance")?;
      if distance.abs() < 1e-9 {
        return Err(CmdError::new("the distance must not be zero"));
      }
      direct(s, DirectEdit::PressPull { faces, distance })
    })(),
    "direct.delete_face" => (|| {
      let faces = faces(s, p)?;
      direct(s, DirectEdit::DeleteFaces { faces })
    })(),
    _ => return None,
  })
}

fn list(s: &Session) -> Value {
  let t = &s.design.configurations;
  json!({
    "active": t.active,
    "configurations": t.list,
    "history": !s.design.history_off,
  })
}

/// The table as the dialog shows it: one row per configuration, one column per override target,
/// cells as text (`null` = the design's own value).
pub fn table(d: &Design) -> Value {
  let t = &d.configurations;
  let cols = t.columns();
  json!({
    "active": t.active,
    "columns": cols.iter().map(|c| json!({"column": c, "label": c.label(d), "default": default_text(d, c)})).collect::<Vec<_>>(),
    "rows": t.list.iter().map(|r| json!({
      "name": r.name,
      "active": t.active.as_deref() == Some(r.name.as_str()),
      "cells": cols.iter().map(|c| r.cell(c).map(Override::value_text)).collect::<Vec<_>>(),
    })).collect::<Vec<_>>(),
  })
}

/// The design's own value for a column, as text.
pub fn default_text(d: &Design, c: &Column) -> String {
  match c {
    Column::Parameter { name } => d.parameter(name).map(|p| p.expr.clone()).unwrap_or("(missing)".into()),
    Column::Suppress { feature } => match d.feature(*feature) {
      Some(f) if f.suppressed => "suppressed".into(),
      Some(_) => "active".into(),
      None => "(missing)".into(),
    },
    Column::Appearance { body } => match d.body_settings(*body).and_then(|b| b.color) {
      Some(c) => format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
      None => "none".into(),
    },
    Column::Joint { joint, motion } => {
      let mut k = d.feature(*joint).map(|f| f.kind.clone());
      k.as_mut().and_then(|k| fox_model::config::joint_field(k, *motion).cloned()).unwrap_or("(missing)".into())
    }
  }
}

fn create(s: &mut Session, p: &Value) -> CmdResult {
  let name = new_name(s, p, "name")?;
  if s.design.configurations.list.len() >= MAX_CONFIGURATIONS {
    return Err(CmdError::new("too many configurations"));
  }
  let overrides = match opt_str(p, "copy") {
    Some(from) => {
      s.design.configurations.get(from).ok_or(CmdError::new(format!("no configuration '{from}'")))?.overrides.clone()
    }
    None => Vec::new(),
  };
  let activate = p.get("activate").and_then(Value::as_bool).unwrap_or(false);
  s.edit(|d| {
    d.configurations.list.push(Configuration { name: name.clone(), overrides });
    if activate {
      d.configurations.active = Some(name.clone());
    }
    Ok(())
  })?;
  Ok(list(s))
}

fn existing(s: &Session, p: &Value, k: &str) -> Result<String, CmdError> {
  let n = req_str(p, k)?;
  s.design.configurations.get(n).ok_or(CmdError::new(format!("no configuration '{n}'")))?;
  Ok(n.to_string())
}

fn new_name(s: &Session, p: &Value, k: &str) -> Result<String, CmdError> {
  let n = req_str(p, k)?.trim();
  if n.is_empty() || n.len() > 200 {
    return Err(CmdError::new("a configuration needs a name (up to 200 characters)"));
  }
  if s.design.configurations.get(n).is_some() {
    return Err(CmdError::new(format!("a configuration named '{n}' already exists")));
  }
  Ok(n.to_string())
}

/// Sets or (with a `null` value) clears one cell of a configuration.
fn set(s: &mut Session, p: &Value) -> CmdResult {
  let row = existing(s, p, "configuration")?;
  let clear = |k: &str| matches!(p.get(k), Some(Value::Null));
  let (col, cell) = if let Some(name) = opt_str(p, "parameter") {
    s.design.parameter(name).ok_or(CmdError::new(format!("no parameter '{name}'")))?;
    let col = Column::Parameter { name: name.to_string() };
    if clear("expr") || p.get("expr").is_none() {
      (col, None)
    } else {
      let expr = expr_of(p.get("expr").unwrap_or(&Value::Null))?;
      (col, Some(Override::Parameter { name: name.to_string(), expr }))
    }
  } else if p.get("feature").is_some() {
    let feature = FeatureId(req_u32(p, "feature")?);
    s.design.feature(feature).ok_or(CmdError::new(format!("no feature {}", feature.0)))?;
    let col = Column::Suppress { feature };
    match p.get("suppressed") {
      None | Some(Value::Null) => (col, None),
      Some(v) => {
        let suppressed = v.as_bool().ok_or(CmdError::new("'suppressed' is true or false"))?;
        (col, Some(Override::Suppress { feature, suppressed }))
      }
    }
  } else if p.get("body").is_some() {
    let body = BodyId(req_u32(p, "body")?);
    if s.model.body(body).is_none() && s.design.body_settings(body).is_none() {
      return Err(CmdError::new(format!("no body {}", body.0)));
    }
    let col = Column::Appearance { body };
    let color = match (opt_str(p, "appearance"), p.get("color")) {
      (Some(name), _) => Some(
        crate::features::APPEARANCES
          .iter()
          .find(|(n, _)| n.eq_ignore_ascii_case(name))
          .map(|(_, c)| *c)
          .ok_or_else(|| CmdError::new(format!("unknown appearance '{name}' (see 'appearances')")))?,
      ),
      (None, Some(Value::Null)) => None,
      (None, Some(c)) => Some(parse_color(c)?),
      // no appearance or colour: back to the default
      (None, None) => return set_cell(s, &row, col, None),
    };
    (col, Some(Override::Appearance { body, color }))
  } else if p.get("joint").is_some() {
    let joint = FeatureId(req_u32(p, "joint")?);
    let kind = match s.design.feature(joint).map(|f| &f.kind) {
      Some(FeatureKind::Joint { kind, .. }) => *kind,
      _ => return Err(CmdError::new("that feature is not a joint")),
    };
    let motion = match opt_str(p, "motion") {
      Some(m) => {
        MotionAxis::parse(m).ok_or(CmdError::new(format!("unknown motion '{m}' (rotation, slide, slide2, pitch)")))?
      }
      None => MotionAxis::first(kind).ok_or(CmdError::new("a rigid joint has no motion"))?,
    };
    if !motion.allowed(kind) {
      return Err(CmdError::new(format!("this joint has no {} motion", motion.name())));
    }
    let col = Column::Joint { joint, motion };
    if clear("value") || p.get("value").is_none() {
      (col, None)
    } else {
      let value = expr_of(p.get("value").unwrap_or(&Value::Null))?;
      (col, Some(Override::Joint { joint, motion, value }))
    }
  } else {
    return Err(CmdError::new("pass 'parameter', 'feature', 'body' or 'joint'"));
  };
  set_cell(s, &row, col, cell)
}

fn set_cell(s: &mut Session, row: &str, col: Column, cell: Option<Override>) -> CmdResult {
  s.edit(|d| {
    let c = d.configurations.get_mut(row).ok_or(CmdError::new(format!("no configuration '{row}'")))?;
    match cell {
      Some(o) => c.set(o),
      None => {
        c.clear(&col);
      }
    }
    Ok(())
  })?;
  let mut out = table(&s.design);
  out["errors"] = json!(s.model.errors());
  Ok(out)
}

/// Exports a configuration (`null` = none) from a copy of the session.
fn export(s: &Session, p: &Value) -> CmdResult {
  let name = match p.get("configuration") {
    Some(Value::Null) | None => None,
    Some(v) => Some(v.as_str().ok_or(CmdError::new("'configuration' is a name"))?),
  };
  let d = s.design.with_configuration(name).map_err(CmdError)?;
  let mut other = Session::new(d);
  other.store_root = s.store_root.clone();
  let mut out = crate::files::export(&other, p)?;
  out["configuration"] = json!(name);
  Ok(out)
}

/// Turns "capture design history" on or off. Off collapses the timeline into a base feature
/// holding the bodies as they are now.
fn history(s: &mut Session, p: &Value) -> CmdResult {
  let capture = p.get("capture").and_then(Value::as_bool).ok_or(CmdError::new("missing 'capture' (true or false)"))?;
  if capture == !s.design.history_off {
    return Ok(json!({"history": capture, "changed": false}));
  }
  if capture {
    s.edit(|d| {
      d.history_off = false;
      Ok(())
    })?;
    return Ok(json!({"history": true, "changed": true}));
  }
  let mut bodies = Vec::new();
  for b in &s.model.bodies {
    bodies.push(BaseBody { id: b.id, name: b.name.clone(), brep: b.shape.to_brep()? });
  }
  let n = bodies.len();
  let id = s.edit(|d| {
    d.history_off = true;
    Ok(d.collapse_history(bodies))
  })?;
  Ok(json!({"history": false, "changed": true, "feature": id.0, "bodies": n, "errors": s.model.errors()}))
}

fn faces(s: &Session, p: &Value) -> Result<Vec<FaceRef>, CmdError> {
  let list = p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))?;
  if list.is_empty() {
    return Err(CmdError::new("select faces"));
  }
  list.iter().map(|f| face_ref(s, f)).collect()
}

/// A length now: a number (mm) or an expression evaluated with the current parameters.
fn length(s: &Session, p: &Value, k: &str) -> Result<f64, CmdError> {
  let e = expr_of(p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}'")))?)?;
  Ok(fox_expr::eval(&e, fox_expr::Kind::Length, &s.model.params)?)
}

/// `[x, y, z]` given as numbers (mm) or length expressions.
fn lengths3(s: &Session, p: &Value, k: &str) -> Result<[f64; 3], CmdError> {
  let a = p.get(k).and_then(Value::as_array).filter(|a| a.len() == 3);
  let a = a.ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y, z]")))?;
  if !a.iter().any(Value::is_string) {
    return req_vec3(p, k);
  }
  let len = |v: &Value| -> Result<f64, CmdError> {
    let v = fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, &s.model.params)?;
    if v.is_finite() { Ok(v) } else { Err(CmdError::new(format!("'{k}' must be finite"))) }
  };
  Ok([len(&a[0])?, len(&a[1])?, len(&a[2])?])
}

/// Adds a direct edit to the open base feature (or a new one) and reports its status.
fn direct(s: &mut Session, edit: DirectEdit) -> CmdResult {
  let id = s.edit(|d| Ok(d.add_direct_edit(edit)?))?;
  Ok(status(s, id))
}

fn status(s: &Session, id: FeatureId) -> Value {
  let st = s.model.status(id);
  let edits = match s.design.feature(id).map(|f| &f.kind) {
    Some(FeatureKind::BaseFeature { edits, .. }) => edits.len(),
    _ => 0,
  };
  json!({
    "feature": id.0,
    "name": s.design.feature(id).map(|f| f.name.clone()),
    "edits": edits,
    "error": st.and_then(|x| x.error.clone()),
    "warning": st.and_then(|x| x.warning.clone()),
    "bodies": s.model.bodies.len(),
  })
}
