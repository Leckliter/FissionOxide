//! Flange Tools: an example FissionOxide plugin (see README.md and docs/PLUGINS.md).

#[allow(dead_code)]
mod fox;

use std::f64::consts::PI;

use serde_json::{Value, json};

/// Called when the user presses OK: `inputs` holds the dialog's values (lengths in mm).
#[unsafe(no_mangle)]
pub extern "C" fn fox_run(cmd: *const u8, cmd_len: usize, inputs: *const u8, inputs_len: usize) -> i64 {
  let cmd = fox::arg(cmd, cmd_len);
  let inputs: Value = serde_json::from_str(&fox::arg(inputs, inputs_len)).unwrap_or_default();
  fox::reply(match cmd.as_str() {
    "flange" => flange(&inputs),
    "report" => report(&inputs),
    _ => Err(format!("unknown command '{cmd}'")),
  })
}

/// Called while the dialog is open when an input changes: returns updates for other inputs.
#[unsafe(no_mangle)]
pub extern "C" fn fox_input_changed(
  cmd: *const u8,
  cmd_len: usize,
  input: *const u8,
  input_len: usize,
  inputs: *const u8,
  inputs_len: usize,
) -> i64 {
  let (cmd, input) = (fox::arg(cmd, cmd_len), fox::arg(input, input_len));
  let v: Value = serde_json::from_str(&fox::arg(inputs, inputs_len)).unwrap_or_default();
  if cmd != "flange" {
    return fox::reply(Ok(json!({})));
  }
  let holes = v["holes"].as_i64().unwrap_or(0);
  let text = match holes {
    0 => "No bolt holes".to_string(),
    n => format!("A hole every {}°", trim(360.0 / n as f64)),
  };
  let mut updates = json!({"spacing": {"text": text}});
  // until the bolt circle is set by hand, show where it will go
  if input != "pitch" && !v["custom_pitch"].as_bool().unwrap_or(false) {
    let (outer, bore) = (v["outer"].as_f64().unwrap_or(0.0), v["bore"].as_f64().unwrap_or(0.0));
    updates["pitch"] = json!({"value": format!("{} mm", trim((outer + bore) / 2.0))});
  }
  fox::reply(Ok(updates))
}

fn trim(x: f64) -> String {
  let s = format!("{x:.3}");
  s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn num(v: &Value, k: &str) -> Result<f64, String> {
  v[k].as_f64().ok_or_else(|| format!("missing input '{k}'"))
}

fn flange(v: &Value) -> Result<Value, String> {
  let (outer, bore, thickness, hole) = (num(v, "outer")?, num(v, "bore")?, num(v, "thickness")?, num(v, "hole")?);
  let holes = v["holes"].as_i64().unwrap_or(0).max(0) as usize;
  let pitch = if v["custom_pitch"].as_bool().unwrap_or(false) { num(v, "pitch")? } else { (outer + bore) / 2.0 };
  if bore >= outer {
    return Err("the bore must be smaller than the outer diameter".into());
  }
  if holes > 0 && (pitch - hole <= bore || pitch + hole >= outer) {
    return Err("the bolt holes must sit between the bore and the rim".into());
  }
  if holes > 1 && pitch * (PI / holes as f64).sin() <= hole {
    return Err("the bolt holes overlap: fewer holes or a smaller hole diameter".into());
  }
  let plane = v["plane"].as_str().unwrap_or("XY").to_lowercase();
  let sketch = fox::execute("sketch.create", json!({"plane": plane}))?["sketch"].clone();
  fox::execute("sketch.circle", json!({"center": [0, 0], "diameter": outer}))?;
  if bore > 0.0 {
    fox::execute("sketch.circle", json!({"center": [0, 0], "diameter": bore}))?;
  }
  for k in 0..holes {
    let a = 2.0 * PI * k as f64 / holes as f64;
    fox::execute("sketch.circle", json!({"center": [pitch / 2.0 * a.cos(), pitch / 2.0 * a.sin()], "diameter": hole}))?;
    if !fox::progress((k + 1) as f32 / (holes + 1) as f32, "Bolt holes") {
      return Err("cancelled".into());
    }
  }
  fox::execute("sketch.finish", json!({}))?;
  // the plate: the region between the rim, the bore and the holes (a point between two holes)
  let a = if holes > 0 { PI / holes as f64 } else { 0.0 };
  let r = if bore > 0.0 || holes > 0 { pitch / 2.0 } else { 0.0 };
  let made = fox::execute(
    "extrude",
    json!({"sketch": sketch, "profiles": [[r * a.cos(), r * a.sin()]], "distance": thickness, "operation": "new_body"}),
  )?;
  let count = fox::storage_get("flanges").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) + 1;
  fox::storage_set("flanges", &count.to_string());
  let body = made["result_bodies"][0].clone();
  if !body.is_null() {
    fox::execute("body.rename", json!({"body": body, "name": format!("Flange {count}")}))?;
  }
  fox::log(2, &format!("flange {count}: Ø{outer} with {holes} holes on Ø{pitch}"));
  Ok(json!({"sketch": sketch, "body": body, "flanges_made": count}))
}

fn csv_field(s: &str) -> String {
  if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn report(v: &Value) -> Result<Value, String> {
  let file = v["file"].as_str().filter(|f| !f.trim().is_empty()).unwrap_or("bodies.csv");
  let hidden = v["hidden"].as_bool().unwrap_or(true);
  let info = fox::execute("info", json!({}))?;
  let header = fox::resource("resources/report-header.txt").map(|b| String::from_utf8_lossy(&b).into_owned());
  let mut csv = header.unwrap_or_default();
  csv.push_str("id,name,volume_mm3,area_mm2,size_x,size_y,size_z\n");
  let mut rows = 0;
  for b in info["bodies"].as_array().into_iter().flatten() {
    if !hidden && b["visible"] == json!(false) {
      continue;
    }
    let size = |k: usize| match (b["bbox"][1][k].as_f64(), b["bbox"][0][k].as_f64()) {
      (Some(hi), Some(lo)) => trim(hi - lo),
      _ => String::new(),
    };
    let n = |k: &str| b[k].as_f64().map(trim).unwrap_or_default();
    csv.push_str(&format!(
      "{},{},{},{},{},{},{}\n",
      b["id"],
      csv_field(b["name"].as_str().unwrap_or("")),
      n("volume_mm3"),
      n("area_mm2"),
      size(0),
      size(1),
      size(2)
    ));
    rows += 1;
  }
  fox::write_file(file, csv.as_bytes())?;
  fox::log(2, &format!("wrote {rows} bodies to {file}"));
  Ok(json!({"file": file, "bodies": rows}))
}
