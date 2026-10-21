//! Sketch > Modify > Scale to Size: the form's choices and how its values become a
//! `sketch.scale_to` command (box presets, margin mode).

use serde_json::{Value, json};

pub const MODES: &[(&str, &str)] = &[("box", "Fit in Box"), ("width", "Width"), ("height", "Height")];

pub const PRESETS: &[(&str, &str)] =
  &[("19.5x19.5", "19.5 x 19.5 (20 x 20 box)"), ("36x10", "36 x 10"), ("custom", "Custom")];

pub const ANCHORS: &[(&str, &str)] =
  &[("centroid", "Centroid"), ("origin", "Sketch origin"), ("corner", "Lower left corner")];

/// The `sketch.scale_to` parameters for the form's values. In margin mode the custom box is the
/// outer size and the padding is kept free on every side (the original's 20 x 20 less 0.25").
pub fn params(p: &Value) -> (&'static str, Value) {
  let text = |k: &str, d: &str| {
    p.get(k).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).unwrap_or(d).to_string()
  };
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  let mode = text("mode", "box");
  let margin_mode = flag("margin_mode");
  let mut q = json!({
    "mode": mode,
    "anchor": text("anchor", "centroid"),
    "rotate": flag("rotate"),
  });
  match mode.as_str() {
    "width" => q["width"] = json!(text("width", "36 in")),
    "height" => q["height"] = json!(text("height", "18 in")),
    _ => {
      let custom = json!([text("box_width", "20 in"), text("box_height", "20 in")]);
      q["box"] = match text("preset", "19.5x19.5").as_str() {
        _ if margin_mode => custom,
        "36x10" => json!(["36 in", "10 in"]),
        "custom" => custom,
        _ => json!(["19.5 in", "19.5 in"]),
      };
    }
  }
  if margin_mode {
    q["margin"] = json!(text("padding", "0.25 in"));
  }
  for k in ["sketch", "curves"] {
    if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
      q[k] = v.clone();
    }
  }
  ("sketch.scale_to", q)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn presets_and_margin_mode() {
    let (cmd, q) = params(&json!({"mode": "box", "preset": "19.5x19.5", "anchor": "centroid", "rotate": true}));
    assert_eq!(cmd, "sketch.scale_to");
    assert_eq!(q["box"], json!(["19.5 in", "19.5 in"]));
    assert_eq!(q["rotate"], json!(true));
    assert!(q.get("margin").is_none());
    let (_, q) = params(&json!({"mode": "box", "preset": "36x10"}));
    assert_eq!(q["box"], json!(["36 in", "10 in"]));
    // margin mode: the outer 20 x 20 less 0.25 on each side
    let (_, q) = params(
      &json!({"mode": "box", "preset": "19.5x19.5", "box_width": "20 in", "box_height": "20 in", "margin_mode": true, "padding": "0.25 in"}),
    );
    assert_eq!(q["box"], json!(["20 in", "20 in"]));
    assert_eq!(q["margin"], json!("0.25 in"));
    let (_, q) = params(&json!({"mode": "width", "width": "36 in", "curves": [3, 4], "sketch": 2}));
    assert_eq!(q["width"], json!("36 in"));
    assert_eq!(q["curves"], json!([3, 4]));
    assert!(q.get("box").is_none());
  }
}
