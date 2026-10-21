//! Preparing the artwork and building the layers: Size Sign, Place Hardware, New Layer.

use serde_json::{Value, json};

use crate::geo::{IN, Info, inch, pick_feature};
use crate::{fox, num};

/// The sketch picked in a dialog, if one was.
fn picked_sketch(v: &Value) -> Option<u64> {
  v["sketch"].as_array().and_then(|a| a.first()).and_then(pick_feature)
}

/// Size Sign: scale the artwork to the sign's width or height, or the largest size that fits a
/// box (the 20 x 20 box with 0.25" all round, 36 x 10, or your own).
pub fn size_sign(v: &Value) -> Result<Value, String> {
  let mut p = json!({
    "anchor": if v["anchor"] == "Origin" { "origin" } else { "centroid" },
  });
  if let Some(s) = picked_sketch(v) {
    p["sketch"] = json!(s);
  }
  let what = match v["mode"].as_str().unwrap_or("Fit in box") {
    "Width" => {
      p["mode"] = json!("width");
      p["width"] = json!(num(v, "width", 36.0 * IN));
      format!("{} wide", inch(num(v, "width", 36.0 * IN)))
    }
    "Height" => {
      p["mode"] = json!("height");
      p["height"] = json!(num(v, "height", 18.0 * IN));
      format!("{} high", inch(num(v, "height", 18.0 * IN)))
    }
    _ => {
      let (w, h, margin) = match v["box"].as_str().unwrap_or("") {
        "36 x 10" => (36.0 * IN, 10.0 * IN, 0.0),
        "Custom" => (num(v, "box_width", 20.0 * IN), num(v, "box_height", 20.0 * IN), num(v, "margin", 0.25 * IN)),
        _ => (20.0 * IN, 20.0 * IN, 0.25 * IN),
      };
      p["mode"] = json!("box");
      p["box"] = json!([w, h]);
      p["margin"] = json!(margin);
      p["rotate"] = json!(v["rotate"].as_bool().unwrap_or(false));
      format!("in a {} x {} box, {} margin", inch(w), inch(h), inch(margin))
    }
  };
  let r = fox::execute("sketch.scale_to", p)?;
  Ok(json!({"message": format!("Sized the artwork {what}"), "result": r}))
}

/// Place Hardware: a part of the LumenArt library at a point of the sketch.
pub fn place_part(v: &Value) -> Result<Value, String> {
  let part = v["part"].as_str().unwrap_or("Keyholes 16 in");
  let mut p = json!({
    "part": format!("LumenArt/{part}"),
    "at": [num(v, "x", 0.0), num(v, "y", 0.0)],
    "angle": num(v, "angle", 0.0),
  });
  if let Some(s) = picked_sketch(v) {
    p["sketch"] = json!(s);
  }
  let r = fox::execute("sketch.library.insert", p)?;
  Ok(json!({"message": format!("Placed {part}"), "result": r}))
}

/// New Layer: the picked profiles become the next numbered component (`01`, `02 - Letters`, …),
/// one sheet thick, its bottom `height` above the sketch.
pub fn new_layer(v: &Value) -> Result<Value, String> {
  let picks = v["profiles"].as_array().cloned().unwrap_or_default();
  let mut sketch = None;
  let mut indices = Vec::new();
  for p in &picks {
    let s = p["sketch"].as_u64().ok_or("pick profiles of a sketch")?;
    if sketch.is_some_and(|k| k != s) {
      return Err("the profiles of one layer must all come from one sketch".into());
    }
    sketch = Some(s);
    indices.push(p["index"].clone());
  }
  let sketch = sketch.ok_or("pick the profiles of this layer")?;
  let thickness = num(v, "thickness", 0.063 * IN);
  let height = num(v, "height", 0.0);
  let prefix = fox::execute("component.next_number", json!({}))?["prefix"].as_str().unwrap_or("01").to_string();
  let label = v["name"].as_str().map(str::trim).unwrap_or("");
  let name = if label.is_empty() { prefix.clone() } else { format!("{prefix} - {label}") };
  let made = fox::execute(
    "extrude",
    json!({
      "sketch": sketch, "profiles": indices, "distance": thickness, "start": height,
      "operation": "new_component", "component_name": name,
    }),
  )?;
  if let Some(e) = made["error"].as_str() {
    return Err(e.to_string());
  }
  let body = made["result_bodies"][0].as_u64().ok_or("the layer made no body")?;
  let info = Info::read()?;
  if let Some((comp, _)) = info.component_of(body) {
    let target = json!({"component": comp});
    for (k, val) in [
      ("Sequence", prefix.clone()),
      ("BaseSketch", sketch.to_string()),
      ("ThicknessIn", format!("{:.4}", thickness / IN)),
      ("HeightIn", format!("{:.4}", height / IN)),
    ] {
      fox::execute("attribute.set", json!({"target": target, "group": "LumenArt", "name": k, "value": val}))?;
    }
  }
  fox::execute("body.rename", json!({"body": body, "name": name}))?;
  // keep the artwork showing for the next layer's profiles
  let _ = fox::execute("sketch.visible", json!({"sketch": sketch, "visible": true}));
  Ok(json!({
    "message": format!("Layer {name}: {} thick at {}", inch(thickness), inch(height)),
    "component": name, "body": body,
  }))
}
