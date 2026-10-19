//! Sketch commands for editable text, Break, tangent circles and blend curves.

use fox_sketch::{CurveId, Dim, PointId, PtSpec, Sketch, TextId};
use serde_json::{Value, json};

use super::{ids, opt_expr, opt_str, params_scope, pt_of, req_expr, req_pt, req_str, sketch_id};
use crate::{CmdError, CmdResult, Session};

pub(super) fn run(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  match name {
    "sketch.text" => add_text(s, p),
    "sketch.text_edit" => edit_text(s, p),
    "sketch.text_explode" => explode_text(s, p),
    "sketch.break" => break_curve(s, p),
    "sketch.tangent_circle" => tangent_circle(s, p),
    "sketch.blend" => blend(s, p),
    _ => Err(CmdError::new(format!("unknown command '{name}'"))),
  }
}

/// Runs `f` on the sketch, solves, and reports the curves it added.
fn change(
  s: &mut Session,
  p: &Value,
  f: impl FnOnce(&mut Sketch) -> Result<Value, CmdError>,
) -> Result<(Value, Vec<CurveId>), CmdError> {
  let sk = sketch_id(s, p)?;
  let (out, made) = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let before: Vec<CurveId> = sketch.curves.iter().map(|c| c.id).collect();
    let out = f(sketch)?;
    if !sketch.solve().ok() {
      return Err(CmdError::new("the result conflicts with the sketch's constraints"));
    }
    Ok((out, sketch.curves.iter().map(|c| c.id).filter(|c| !before.contains(c)).collect::<Vec<_>>()))
  })?;
  Ok((out, made))
}

fn finish(s: &Session, p: &Value, mut out: Value, made: &[CurveId]) -> CmdResult {
  let sk = sketch_id(s, p)?;
  out["curves"] = json!(made.iter().map(|c| c.0).collect::<Vec<_>>());
  out["profiles"] = json!(s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0));
  Ok(out)
}

/// A length parameter (number or expression).
fn length(s: &Session, p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match opt_expr(p, k) {
    Some(e) => Ok(Some(fox_expr::eval(&e, fox_expr::Kind::Length, &params_scope(s))?)),
    None => Ok(None),
  }
}

/// An angle: a number in degrees, or an angle expression ("30 deg").
fn angle(s: &Session, p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Number(n)) => Ok(n.as_f64().map(f64::to_radians)),
    Some(_) => {
      let e = req_expr(p, k)?;
      Ok(Some(fox_expr::eval(&e, fox_expr::Kind::Angle, &params_scope(s))?))
    }
  }
}

fn add_text(s: &mut Session, p: &Value) -> CmdResult {
  let at = match p.get("at").and_then(Value::as_u64) {
    Some(id) => PtSpec::Point(PointId(id as u32)),
    None => req_pt(p, "at")?.into(),
  };
  let text = req_str(p, "text")?.to_string();
  let height = length(s, p, "height")?.unwrap_or(5.0);
  let angle = angle(s, p, "angle")?.unwrap_or(0.0);
  let font = opt_str(p, "font").unwrap_or("sans").to_string();
  let explode = p.get("explode").and_then(Value::as_bool).unwrap_or(false);
  let (out, made) = change(s, p, move |sk| {
    let id = sk.add_text(at, &text, &font, height, angle)?;
    if explode {
      sk.explode_text(id)?;
      return Ok(json!({}));
    }
    Ok(json!({"text": id.0}))
  })?;
  finish(s, p, out, &made)
}

/// The text a command means: `text_id`, the text owning `curve` (or the first of `curves`), or
/// the sketch's only text.
fn which_text(sketch: &Sketch, p: &Value) -> Result<TextId, CmdError> {
  if let Some(id) = p.get("text_id").and_then(Value::as_u64) {
    let id = TextId(id as u32);
    return sketch.text(id).map(|t| t.id).ok_or(CmdError::new("no such text"));
  }
  let curve = p.get("curve").and_then(Value::as_u64).map(|c| c as u32).or_else(|| ids(p, "curves").first().copied());
  if let Some(c) = curve {
    return sketch.text_of(CurveId(c)).ok_or(CmdError::new("that curve is not part of a text"));
  }
  match sketch.texts.as_slice() {
    [t] => Ok(t.id),
    [] => Err(CmdError::new("the sketch has no text")),
    _ => Err(CmdError::new("the sketch has several texts; pass 'text_id' or a curve of one")),
  }
}

fn edit_text(s: &mut Session, p: &Value) -> CmdResult {
  let text = opt_str(p, "text").map(str::to_string);
  let font = opt_str(p, "font").map(str::to_string);
  let height = length(s, p, "height")?;
  let angle = angle(s, p, "angle")?;
  let at = p.get("at").and_then(pt_of);
  let q = p.clone();
  let (out, made) = change(s, p, move |sk| {
    let id = which_text(sk, &q)?;
    sk.edit_text(id, text.as_deref(), font.as_deref(), height, angle)?;
    if let (Some(at), Some(anchor)) = (at, sk.text(id).map(|t| t.anchor))
      && !sk.drag(anchor, at).ok()
    {
      return Err(CmdError::new("the text can't move there"));
    }
    Ok(json!({"text": id.0}))
  })?;
  finish(s, p, out, &made)
}

fn explode_text(s: &mut Session, p: &Value) -> CmdResult {
  let q = p.clone();
  let (out, _) = change(s, p, move |sk| {
    let id = which_text(sk, &q)?;
    let curves = sk.explode_text(id)?;
    Ok(json!({"exploded": curves.iter().map(|c| c.0).collect::<Vec<_>>()}))
  })?;
  finish(s, p, out, &[])
}

fn break_curve(s: &mut Session, p: &Value) -> CmdResult {
  let at = req_pt(p, "at")?;
  let curve = p.get("curve").and_then(Value::as_u64).map(|c| CurveId(c as u32));
  let at_point = p.get("at_point").and_then(Value::as_bool).unwrap_or(false);
  let snap = length(s, p, "snap")?.unwrap_or(0.0);
  let (out, _) = change(s, p, move |sk| {
    let c = match curve {
      Some(c) => c,
      None => sk.nearest_curve(at, 1.0).ok_or(CmdError::new("no curve there"))?,
    };
    let parts = sk.break_curve(c, at, at_point, snap)?;
    Ok(json!({"pieces": parts.iter().map(|c| c.0).collect::<Vec<_>>()}))
  })?;
  finish(s, p, out, &[])
}

fn tangent_circle(s: &mut Session, p: &Value) -> CmdResult {
  let curves: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  let radius = match opt_expr(p, "radius") {
    Some(e) => {
      let v = fox_expr::eval(&e, fox_expr::Kind::Length, &params_scope(s))?;
      Some(Dim::expr(e, v))
    }
    None => None,
  };
  let near = p.get("near").and_then(pt_of);
  let (out, made) = change(s, p, move |sk| {
    // by default the circle nearest the middle of the picked curves (inside a corner or triangle)
    let mids: Vec<[f64; 2]> = curves.iter().filter_map(|c| sk.curve_segs(*c).first().map(|g| g.mid())).collect();
    let centroid = [
      mids.iter().map(|m| m[0]).sum::<f64>() / mids.len().max(1) as f64,
      mids.iter().map(|m| m[1]).sum::<f64>() / mids.len().max(1) as f64,
    ];
    let c = sk.add_tangent_circle(&curves, radius, near.unwrap_or(centroid))?;
    Ok(json!({"circle": c.0}))
  })?;
  finish(s, p, out, &made)
}

fn blend(s: &mut Session, p: &Value) -> CmdResult {
  let curves: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  let [a, b] = curves.as_slice() else { return Err(CmdError::new("a blend curve needs two curves")) };
  let (a, b) = (*a, *b);
  let near = match p.get("near").and_then(Value::as_array).map(|v| v.iter().filter_map(pt_of).collect::<Vec<_>>()) {
    Some(v) if v.len() == 2 => Some([v[0], v[1]]),
    Some(_) => return Err(CmdError::new("'near' is one [x, y] per curve")),
    None => None,
  };
  let g2 = match opt_str(p, "continuity").unwrap_or("g2") {
    "g2" | "G2" | "curvature" => true,
    "g1" | "G1" | "tangent" => false,
    other => return Err(CmdError::new(format!("unknown continuity '{other}' (g1 or g2)"))),
  };
  let (out, made) = change(s, p, move |sk| {
    let c = sk.blend(a, b, near, g2)?;
    Ok(json!({"blend": c.0}))
  })?;
  finish(s, p, out, &made)
}
