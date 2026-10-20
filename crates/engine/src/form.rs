//! Form (T-spline) commands: Create Form with a box cage or a cage given outright, and pulling
//! a cage vertex. The cage lives in the form feature; editing it rebuilds the smooth body.

use fox_model::form::{FormCage, MAX_FACES};
use fox_model::{FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{add_feature, operation, plane_ref, req_vec3};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "form.box" => (|| {
      let (l, w, h) = (length(s, p, "length")?, length(s, p, "width")?, length(s, p, "height")?);
      let counts = [count(s, p, "length_faces")?, count(s, p, "width_faces")?, count(s, p, "height_faces")?];
      let mut cage = FormCage::quad_box(l, w, h, counts).map_err(CmdError::new)?;
      if let Some(at) = p.get("at").filter(|v| !v.is_null()) {
        let at = point2(at)?;
        for v in &mut cage.vertices {
          v[0] += at[0];
          v[1] += at[1];
        }
      }
      add_form(s, p, cage)
    })(),
    "form.cage" => (|| {
      let mut cage = FormCage::default();
      for v in p.get("vertices").and_then(Value::as_array).ok_or(CmdError::new("missing 'vertices'"))? {
        cage.vertices.push(req_vec3(&json!({ "v": v }), "v")?);
      }
      for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
        let ids: Vec<u32> = f
          .as_array()
          .map(|a| a.iter().filter_map(|i| i.as_u64().and_then(|i| u32::try_from(i).ok())).collect())
          .unwrap_or_default();
        let quad: [u32; 4] = ids.try_into().map_err(|_| CmdError::new("each face is four vertex indices"))?;
        cage.faces.push(quad);
        if cage.faces.len() > MAX_FACES {
          return Err(CmdError::new(format!("a cage can have at most {MAX_FACES} faces")));
        }
      }
      cage.check().map_err(CmdError::new)?;
      add_form(s, p, cage)
    })(),
    "form.move_vertex" => (|| {
      let id = form_feature(s, p)?;
      let Some(FeatureKind::Form { cage, .. }) = s.design.feature(id).map(|f| &f.kind) else {
        return Err(CmdError::new("that feature is not a form"));
      };
      let v = match (p.get("vertex").and_then(Value::as_u64), p.get("point")) {
        (Some(i), _) => usize::try_from(i).ok().filter(|i| *i < cage.vertices.len()),
        (None, Some(_)) => cage.nearest_vertex(req_vec3(p, "point")?),
        (None, None) => return Err(CmdError::new("pass the 'vertex' index or a 'point' near it")),
      }
      .ok_or(CmdError::new("no such vertex"))?;
      let old = cage.vertices[v];
      let new = match (p.get("to"), p.get("by")) {
        (Some(_), _) => req_vec3(p, "to")?,
        (None, Some(_)) => {
          let d = req_vec3(p, "by")?;
          [old[0] + d[0], old[1] + d[1], old[2] + d[2]]
        }
        (None, None) => return Err(CmdError::new("pass 'to' (a position) or 'by' (an offset)")),
      };
      s.edit(|d| {
        if let Some(FeatureKind::Form { cage, .. }) = d.feature_mut(id).map(|f| &mut f.kind) {
          cage.vertices[v] = new;
        }
        Ok(())
      })?;
      let st = s.model.status(id);
      Ok(json!({
        "feature": id.0,
        "vertex": v,
        "from": old,
        "to": new,
        "error": st.and_then(|x| x.error.clone()),
        "warning": st.and_then(|x| x.warning.clone()),
      }))
    })(),
    _ => return None,
  })
}

fn add_form(s: &mut Session, p: &Value, cage: FormCage) -> CmdResult {
  let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
  add_feature(s, FeatureKind::Form { plane, cage, operation: operation(p)? })
}

/// The form feature named by 'feature', or the last one.
fn form_feature(s: &Session, p: &Value) -> Result<FeatureId, CmdError> {
  match p.get("feature").and_then(Value::as_u64) {
    Some(f) => Ok(FeatureId(u32::try_from(f).map_err(|_| CmdError::new("bad feature"))?)),
    None => s
      .design
      .features
      .iter()
      .rev()
      .find(|f| matches!(f.kind, FeatureKind::Form { .. }))
      .map(|f| f.id)
      .ok_or(CmdError::new("there is no form feature")),
  }
}

/// A length: a number (mm) or an expression ("80 mm", "w / 2").
fn length(s: &Session, p: &Value, k: &str) -> Result<f64, CmdError> {
  let v = match p.get(k) {
    Some(Value::Number(n)) => n.as_f64().ok_or(CmdError::new(format!("bad '{k}'")))?,
    Some(Value::String(e)) => fox_expr::eval(e, fox_expr::Kind::Length, &s.model.params)
      .map_err(|err| CmdError::new(format!("'{k}': {err}")))?,
    _ => return Err(CmdError::new(format!("missing '{k}'"))),
  };
  if !(v.is_finite() && v > 0.0) {
    return Err(CmdError::new(format!("'{k}' must be positive")));
  }
  Ok(v)
}

/// A face count along a box side (default 2, as Fusion's Box).
fn count(s: &Session, p: &Value, k: &str) -> Result<u32, CmdError> {
  let n = match p.get(k) {
    None | Some(Value::Null) => return Ok(2),
    Some(Value::String(e)) => fox_expr::eval(e, fox_expr::Kind::Unitless, &s.model.params).ok(),
    Some(v) => v.as_f64(),
  };
  n.filter(|n| n.fract() == 0.0 && (1.0..=32.0).contains(n))
    .map(|n| n as u32)
    .ok_or(CmdError::new(format!("'{k}' must be a whole number from 1 to 32")))
}

fn point2(v: &Value) -> Result<[f64; 2], CmdError> {
  match v.as_array().map(Vec::as_slice) {
    Some([x, y]) => match (x.as_f64(), y.as_f64()) {
      (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok([x, y]),
      _ => Err(CmdError::new("'at' must be [x, y]")),
    },
    _ => Err(CmdError::new("'at' must be [x, y]")),
  }
}
