//! Document units and Change Parameters' model parameters (d1, d2, ...), favourites and
//! comments. User parameters themselves are `param.add` / `param.set` (see `commands.rs`).

use fox_model::{FeatureId, FeatureKind, LengthUnit, ModelParamRef, ParamKind};
use serde_json::{Value, json};

use crate::commands::{opt_str, req_expr, req_str, req_u32};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "design.units" => units(s, p),
    "param.model" => Ok(model_list(s)),
    "param.model_set" => model_set(s, p),
    "param.favorite" => favorite(s, p),
    "param.comment" => (|| {
      let n = req_str(p, "name")?.to_string();
      let c = opt_str(p, "comment").unwrap_or("").to_string();
      s.edit(|d| {
        let prm = d.parameters.iter_mut().find(|x| x.name == n).ok_or(CmdError::new(format!("no parameter '{n}'")))?;
        prm.comment = c;
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    _ => return None,
  })
}

/// Reads or sets the design's length unit. Geometry doesn't change: only how lengths show and
/// what a bare number typed means.
fn units(s: &mut Session, p: &Value) -> CmdResult {
  if let Some(u) = opt_str(p, "unit") {
    let unit = LengthUnit::parse(u).ok_or(CmdError::new(format!("unknown unit '{u}' (mm, cm, m, in, ft)")))?;
    if unit != s.design.units {
      s.edit(|d| {
        d.units = unit;
        Ok(())
      })?;
    }
  }
  Ok(json!({"unit": s.design.units.symbol(), "name": s.design.units.label()}))
}

fn kind_name(k: ParamKind) -> &'static str {
  match k {
    ParamKind::Length => "length",
    ParamKind::Angle => "angle",
    ParamKind::Unitless => "unitless",
  }
}

/// Every model parameter with its value (mm, radians or a plain number).
pub(crate) fn model_list(s: &Session) -> Value {
  let rows: Vec<Value> = s
    .design
    .model_parameters()
    .into_iter()
    .map(|m| {
      let value = fox_expr::eval(&m.expr, m.kind, &s.model.params).ok();
      json!({
        "name": m.name, "feature": m.feature.0, "feature_name": m.feature_name, "index": m.index,
        "role": m.role, "expr": m.expr, "kind": kind_name(m.kind), "value": value, "favorite": m.favorite,
      })
    })
    .collect();
  json!(rows)
}

fn model_set(s: &mut Session, p: &Value) -> CmdResult {
  let feature = FeatureId(req_u32(p, "feature")?);
  let index = req_u32(p, "index")? as usize;
  let expr = req_expr(p, "expr")?;
  let m = s
    .design
    .model_parameters()
    .into_iter()
    .find(|m| m.feature == feature && m.index == index)
    .ok_or(CmdError::new("no such model parameter"))?;
  // a sketch dimension goes through the sketch, so it solves
  if let Some(FeatureKind::Sketch { sketch, .. }) = s.design.feature(feature).map(|f| &f.kind) {
    let c = sketch.constraints.iter().filter(|c| c.kind.dim().is_some()).nth(index).map(|c| c.id.0);
    let c = c.ok_or(CmdError::new("no such dimension"))?;
    return s.execute("sketch.set_dimension", &json!({"sketch": feature.0, "constraint": c, "value": expr}));
  }
  fox_expr::eval(&expr, m.kind, &s.model.params)?;
  s.edit(|d| Ok(d.set_model_parameter(feature, index, &expr)?))?;
  let err = s.model.status(feature).and_then(|st| st.error.clone());
  Ok(json!({"name": m.name, "error": err}))
}

/// Stars a user parameter (`name`) or a model parameter (`feature`, `index`); `on` defaults to
/// toggling.
fn favorite(s: &mut Session, p: &Value) -> CmdResult {
  let on = p.get("on").and_then(Value::as_bool);
  if let Some(n) = opt_str(p, "name") {
    let n = n.to_string();
    let cur = s.design.parameter(&n).ok_or(CmdError::new(format!("no parameter '{n}'")))?.favorite;
    let v = on.unwrap_or(!cur);
    s.edit(|d| {
      if let Some(x) = d.parameters.iter_mut().find(|x| x.name == n) {
        x.favorite = v;
      }
      Ok(())
    })?;
    return Ok(json!({"favorite": v}));
  }
  let at = ModelParamRef { feature: FeatureId(req_u32(p, "feature")?), index: req_u32(p, "index")? as usize };
  let v = on.unwrap_or(!s.design.model_favorites.contains(&at));
  s.edit(|d| {
    d.set_model_favorite(at, v);
    Ok(())
  })?;
  Ok(json!({"favorite": v}))
}

#[cfg(test)]
mod tests {
  use serde_json::{Value, json};

  use crate::Session;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    match s.execute(cmd, &p) {
      Ok(v) => {
        assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
        v
      }
      Err(e) => panic!("{cmd} {p}: {e}"),
    }
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().map(|m| m.volume).unwrap_or(0.0)).sum()
  }

  #[test]
  fn units_change_display_not_geometry_and_undo() {
    let mut s = Session::default();
    assert_eq!(run(&mut s, "design.units", json!({}))["unit"], "mm");
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    run(&mut s, "extrude", json!({"distance": "5 mm"}));
    let v = volume(&s);
    assert_eq!(run(&mut s, "design.units", json!({"unit": "in"}))["unit"], "in");
    assert_eq!(s.design.units, fox_model::LengthUnit::In);
    assert!((volume(&s) - v).abs() < 1e-6, "geometry stays put");
    assert!(s.execute("design.units", &json!({"unit": "furlong"})).is_err());
    s.undo();
    assert_eq!(s.design.units, fox_model::LengthUnit::Mm);
  }

  #[test]
  fn model_parameters_list_and_edit() {
    let mut s = Session::default();
    run(&mut s, "param.add", json!({"name": "w", "expr": "10 mm"}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let e = run(&mut s, "extrude", json!({"distance": "5 mm"}))["feature"].as_u64().unwrap();
    let list = run(&mut s, "param.model", json!({}));
    let rows = list.as_array().unwrap();
    let ext: Vec<&Value> = rows.iter().filter(|r| r["feature"] == e).collect();
    assert_eq!(ext[0]["role"], "distance");
    assert_eq!(ext[0]["value"], 5.0);
    assert_eq!(ext[0]["kind"], "length");
    assert!(rows.iter().enumerate().all(|(i, r)| r["name"] == format!("d{}", i + 1)));
    let v = volume(&s);
    run(&mut s, "param.model_set", json!({"feature": e, "index": 0, "expr": "w"}));
    assert!((volume(&s) - v * 2.0).abs() < 1e-3, "{} vs {v}", volume(&s));
    assert!(s.execute("param.model_set", &json!({"feature": e, "index": 0, "expr": "nope"})).is_err());
    // favourites and comments
    run(&mut s, "param.favorite", json!({"feature": e, "index": 0}));
    let rows = run(&mut s, "param.model", json!({}));
    assert!(rows.as_array().unwrap().iter().any(|r| r["feature"] == e && r["favorite"] == true));
    assert_eq!(run(&mut s, "param.favorite", json!({"name": "w"}))["favorite"], true);
    run(&mut s, "param.comment", json!({"name": "w", "comment": "width"}));
    assert_eq!(s.design.parameter("w").unwrap().comment, "width");
    assert!(s.design.parameter("w").unwrap().favorite);
  }

  #[test]
  fn sketch_dimensions_are_model_parameters() {
    let mut s = Session::default();
    let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
    let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let c = r["curves"][0].as_u64().unwrap();
    run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [c], "value": "10"}));
    let rows = run(&mut s, "param.model", json!({}));
    let d1 = &rows[0];
    assert_eq!((d1["name"].as_str(), d1["feature"].as_u64(), d1["expr"].as_str()), (Some("d1"), Some(sk), Some("10")));
    run(&mut s, "param.model_set", json!({"feature": sk, "index": 0, "expr": "1 in"}));
    let rows = run(&mut s, "param.model", json!({}));
    assert_eq!(rows[0]["expr"], "1 in");
    assert!((rows[0]["value"].as_f64().unwrap() - 25.4).abs() < 1e-9);
  }

  /// A dimension can read the model parameters, as Fusion's heads-up boxes let it ("d1 * 2"):
  /// it evaluates on creation and follows d1.
  #[test]
  fn a_dimension_expression_can_use_a_model_parameter() {
    let mut s = Session::default();
    let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
    let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let (w, h) = (r["curves"][0].as_u64().unwrap(), r["curves"][1].as_u64().unwrap());
    run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [w], "value": "0.5"}));
    run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [h], "value": "d1 * 2"}));
    let rows = run(&mut s, "param.model", json!({}));
    assert_eq!(rows[1]["expr"], "d1 * 2");
    assert!((rows[1]["value"].as_f64().unwrap() - 1.0).abs() < 1e-9, "{rows}");
    assert!((s.model.params["d2"] - 1.0).abs() < 1e-9, "model parameters are in the session's scope");
    // d1 changes: d2 follows on the regeneration
    run(&mut s, "param.model_set", json!({"feature": sk, "index": 0, "expr": "3"}));
    let rows = run(&mut s, "param.model", json!({}));
    assert!((rows[1]["value"].as_f64().unwrap() - 6.0).abs() < 1e-9, "{rows}");
    let sketch = s.design.sketch(fox_model::FeatureId(sk as u32)).unwrap();
    let dims: Vec<f64> = sketch.constraints.iter().filter_map(|c| c.kind.dim().map(|d| d.value)).collect();
    assert_eq!(dims, [3.0, 6.0]);
    // a user parameter named like a model parameter wins
    run(&mut s, "param.add", json!({"name": "d1", "expr": "7", "kind": "length"}));
    assert!((s.model.params["d1"] - 7.0).abs() < 1e-9);
  }
}
