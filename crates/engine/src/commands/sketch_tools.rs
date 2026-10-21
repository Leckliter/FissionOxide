//! Sketch commands for editable text, Break, tangent circles and blend curves.

use fox_sketch::{
  ConstraintId, ConstraintKind, CurveId, Dim, HAlign, PointId, PtSpec, Sketch, TextAlign, TextId, TextPath, TextStyle,
  VAlign,
};
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

/// `style` with the text options `p` gives (bold, italic, underline, halign, valign, spacing).
fn text_style(mut style: TextStyle, p: &Value) -> Result<TextStyle, CmdError> {
  let flag = |k: &str| p.get(k).and_then(Value::as_bool);
  if let Some(b) = flag("bold") {
    style.bold = b;
  }
  if let Some(b) = flag("italic") {
    style.italic = b;
  }
  if let Some(b) = flag("underline") {
    style.underline = b;
  }
  if let Some(h) = opt_str(p, "halign") {
    style.halign = match h {
      "left" => HAlign::Left,
      "center" | "centre" => HAlign::Center,
      "right" => HAlign::Right,
      _ => return Err(CmdError::new(format!("halign '{h}': left, center or right"))),
    };
  }
  if let Some(v) = opt_str(p, "valign") {
    style.valign = match v {
      "baseline" => VAlign::Baseline,
      "top" => VAlign::Top,
      "middle" => VAlign::Middle,
      "bottom" => VAlign::Bottom,
      _ => return Err(CmdError::new(format!("valign '{v}': baseline, top, middle or bottom"))),
    };
  }
  match p.get("spacing") {
    None | Some(Value::Null) => {}
    Some(v) => {
      let pct = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(t) => t.trim().trim_end_matches('%').trim().parse::<f64>().ok(),
        _ => None,
      };
      style.spacing = pct.ok_or(CmdError::new("spacing: a percentage of the text height"))?;
    }
  }
  Ok(style)
}

fn add_text(s: &mut Session, p: &Value) -> CmdResult {
  let path = text_path(p, None)?;
  let at = match (p.get("at").and_then(Value::as_u64), &path) {
    (_, Some(_)) => None,
    (Some(id), None) => Some(PtSpec::Point(PointId(id as u32))),
    (None, None) => Some(req_pt(p, "at")?.into()),
  };
  let text = req_str(p, "text")?.to_string();
  let height = length(s, p, "height")?.unwrap_or(5.0);
  let angle = angle(s, p, "angle")?.unwrap_or(0.0);
  let font = opt_str(p, "font").unwrap_or("sans").to_string();
  let explode = p.get("explode").and_then(Value::as_bool).unwrap_or(false);
  let style = text_style(TextStyle::default(), p)?;
  let (out, made) = change(s, p, move |sk| {
    let id = match (path, at) {
      (Some(path), _) => {
        let id = sk.add_text_on_path(path, &text, &font, height)?;
        if !style.is_plain() {
          sk.set_text_style(id, style)?;
        }
        id
      }
      (None, Some(at)) => sk.add_text_styled(at, &text, &font, height, angle, style)?,
      (None, None) => return Err(CmdError::new("pass 'at' or 'path'")),
    };
    if explode {
      sk.explode_text(id)?;
      return Ok(json!({}));
    }
    Ok(json!({"text": id.0}))
  })?;
  finish(s, p, out, &made)
}

/// Text along a path from `path` (a curve id), `above` (default true) and `align` ("left",
/// "center" or "right"); `old` fills what's missing when editing. None without a `path`.
fn text_path(p: &Value, old: Option<TextPath>) -> Result<Option<TextPath>, CmdError> {
  let curve = match p.get("path") {
    None => old.map(|o| o.curve),
    Some(Value::Null) => return Ok(None),
    Some(v) => Some(CurveId(v.as_u64().ok_or(CmdError::new("'path' must be a curve id"))? as u32)),
  };
  let Some(curve) = curve else { return Ok(None) };
  let above = p.get("above").and_then(Value::as_bool).or(old.map(|o| o.above)).unwrap_or(true);
  let align = match opt_str(p, "align") {
    None => old.map(|o| o.align).unwrap_or_default(),
    Some("left") => TextAlign::Left,
    Some("center" | "centre") => TextAlign::Center,
    Some("right") => TextAlign::Right,
    Some(a) => return Err(CmdError::new(format!("unknown align '{a}' (left, center or right)"))),
  };
  Ok(Some(TextPath { curve, above, align }))
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
    let before = sk.text(id).map(|t| t.style.clone()).unwrap_or_default();
    let style = text_style(before.clone(), &q)?;
    if style != before {
      sk.set_text_style(id, style)?;
    }
    if ["path", "above", "align"].iter().any(|k| q.get(*k).is_some()) {
      let old = sk.text(id).and_then(|t| t.path);
      sk.set_text_path(id, text_path(&q, old)?)?;
    }
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
    let before: Vec<ConstraintId> = sk.constraints.iter().map(|c| c.id).collect();
    let parts = sk.break_curve(c, at, at_point, snap)?;
    let mut out = json!({"pieces": parts.iter().map(|c| c.0).collect::<Vec<_>>()});
    // Fusion warns when a break drops constraints or dimensions
    super::report_removed(&mut out, before.iter().filter(|id| sk.constraint(**id).is_none()).count());
    Ok(out)
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
  // Fusion's 2-Tangent Circle sizes the circle without dimensioning it; the radius drives only
  // when asked (the default, for scripts)
  let dimension = p.get("dimension").and_then(Value::as_bool).unwrap_or(true);
  let (out, made) = change(s, p, move |sk| {
    // by default the circle nearest the middle of the picked curves (inside a corner or triangle)
    let mids: Vec<[f64; 2]> = curves.iter().filter_map(|c| sk.curve_segs(*c).first().map(|g| g.mid())).collect();
    let centroid = [
      mids.iter().map(|m| m[0]).sum::<f64>() / mids.len().max(1) as f64,
      mids.iter().map(|m| m[1]).sum::<f64>() / mids.len().max(1) as f64,
    ];
    let c = sk.add_tangent_circle(&curves, radius, near.unwrap_or(centroid))?;
    if !dimension {
      let radius_dims: Vec<_> = sk
        .constraints
        .iter()
        .filter(|k| matches!(k.kind, ConstraintKind::Radius { curve, .. } if curve == c))
        .map(|k| k.id)
        .collect();
      for id in radius_dims {
        sk.remove_constraint(id)?;
      }
    }
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
  // tangent continuous unless asked, as Fusion
  let g2 = match opt_str(p, "continuity").unwrap_or("g1") {
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
