//! Auto Export: everything a product listing and the cut order need, in one folder named after
//! the product (`LAP-0002/`): renders, part pictures and data, the cut file's numbers and DXF,
//! the sign's size and a master JSON pointing at all of it.

use serde_json::{Map, Value, json};

use crate::geo::Info;
use crate::{fox, num};

/// The product label: `LAP-0002` (letters, a dash, 3 to 6 digits, maybe `.1`) from the design's
/// name, else a 4-digit number in it, else the name itself made safe for a folder.
pub fn label_of(name: &str) -> String {
  let chars: Vec<char> = name.chars().collect();
  let mut i = 0;
  while i < chars.len() {
    let start = i;
    while i < chars.len() && chars[i].is_ascii_alphabetic() {
      i += 1;
    }
    let letters = i - start;
    if letters >= 2 && (start == 0 || !chars[start - 1].is_ascii_alphanumeric()) && chars.get(i) == Some(&'-') {
      let mut j = i + 1;
      while j < chars.len() && chars[j].is_ascii_digit() {
        j += 1;
      }
      let digits = j - i - 1;
      if (3..=6).contains(&digits) && !chars.get(j).is_some_and(|c| c.is_ascii_alphanumeric()) {
        let mut end = j;
        if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
          end = j + 1;
          while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
          }
        }
        return chars[start..end].iter().collect::<String>().to_uppercase();
      }
    }
    i = start + letters.max(1);
  }
  // a number of exactly four digits
  if let Some(word) =
    name.split(|c: char| !c.is_ascii_alphanumeric()).find(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()))
  {
    return word.to_string();
  }
  let safe: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) { ' ' } else { c }).collect();
  let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
  if safe.is_empty() { "Sign".into() } else { safe }
}

/// One step of the export: what it wrote, or why it didn't.
fn step(done: &mut Map<String, Value>, problems: &mut Vec<String>, key: &str, r: Result<Value, String>) {
  match r {
    Ok(v) => {
      done.insert(key.into(), v);
    }
    Err(e) => problems.push(format!("{key}: {e}")),
  }
}

pub fn auto_export(v: &Value) -> Result<Value, String> {
  let info = Info::read()?;
  let given = v["label"].as_str().map(str::trim).unwrap_or("");
  let label = if given.is_empty() { label_of(info.raw["name"].as_str().unwrap_or("Sign")) } else { label_of(given) };
  let size = num(v, "size", 1080.0).round() as i64;
  let spread = num(v, "explode", 2.0);
  let base = label.clone();
  let mut done = Map::new();
  let mut problems = Vec::new();
  let total = 6.0;
  let go = |k: f32, what: &str| if fox::progress(k / total, what) { Ok(()) } else { Err("cancelled".to_string()) };

  go(0.0, "Product render")?;
  let product = format!("{base}/Photos/{label} - Render Product.png");
  let r = fox::execute(
    "view.render",
    json!({"path": product, "width": size, "height": size, "view": "top", "ortho": true, "explode": false}),
  );
  step(&mut done, &mut problems, "product_render_png", r.map(|_| json!(product)));

  go(1.0, "Part renders")?;
  let r = fox::execute(
    "view.render_components",
    json!({"folder": format!("{base}/Parts"), "size": size, "label": label, "manifest": format!("{label} - Part Data")}),
  );
  step(&mut done, &mut problems, "parts", r);

  go(2.0, "Exploded render")?;
  let exploded = format!("{base}/Photos/{label} - Render Exploded.png");
  let r = fox::execute(
    "view.render",
    json!({
      "path": exploded, "width": size, "height": size, "yaw": 45, "pitch": 30, "ortho": true,
      "explode": {"direction": "z", "factor": spread},
    }),
  );
  step(&mut done, &mut problems, "exploded_render_png", r.map(|_| json!(exploded)));

  go(3.0, "Cut file")?;
  let mut cut_sketch = info.feature_named("Cut File");
  let sheet_metal = info.raw["bodies"].as_array().is_some_and(|b| b.iter().any(|b| b["sheet_metal"].is_object()));
  if cut_sketch.is_none() && sheet_metal {
    match fox::execute("sheet.collect_flat", json!({})) {
      Ok(_) => cut_sketch = Info::read()?.feature_named("Cut File"),
      Err(e) => problems.push(format!("cut file: {e}")),
    }
  }
  match cut_sketch {
    Some(sk) => {
      let data = format!("{base}/Sketch/{label} - Sketch Data");
      let est = fox::execute("cut.estimate", json!({"sketch": sk, "export": format!("{data}.json")}));
      let _ = fox::execute("cut.estimate", json!({"sketch": sk, "export": format!("{data}.csv")}));
      step(
        &mut done,
        &mut problems,
        "sketch",
        est.map(|e| json!({"summary": e, "path_json": format!("{data}.json"), "path_csv": format!("{data}.csv")})),
      );
      let check = format!("{base}/Sketch/{label} - SendCutSend Check.json");
      let r = fox::execute("cut.service.check", json!({"sketch": sk, "export": check}));
      step(&mut done, &mut problems, "cut_check", r.map(|c| json!({"result": c, "path_json": check})));
      let dxf = format!("{base}/{label}.dxf");
      let r = fox::execute("export", json!({"path": dxf, "sketch": sk}));
      step(&mut done, &mut problems, "dxf", r.map(|_| json!(dxf)));
    }
    None => problems.push("cut file: no sketch named 'Cut File' (Collect Flat Patterns makes one)".into()),
  }

  go(4.0, "Dimensions")?;
  let dims = format!("{base}/File/{label} - Dimensions");
  let r = fox::execute("inspect.overall_size", json!({"export": format!("{dims}.json"), "units": "in"}));
  for ext in ["csv", "txt"] {
    let _ = fox::execute("inspect.overall_size", json!({"export": format!("{dims}.{ext}"), "units": "in"}));
  }
  step(&mut done, &mut problems, "dimensions", r.map(|d| json!({"data": d, "path_json": format!("{dims}.json")})));

  go(5.0, "Master file")?;
  let master = json!({
    "lumenart_master_export": {
      "version": "2.0.0",
      "label": label,
      "design": info.raw["name"],
      "outputs": Value::Object(done.clone()),
      "problems": problems,
    }
  });
  let path = format!("{base}/{label} - Master Export.json");
  fox::write_file(&path, serde_json::to_string_pretty(&master).unwrap_or_default().as_bytes())?;
  let mut msg = format!("Exported {label} ({} outputs)", done.len());
  if !problems.is_empty() {
    msg = format!("{msg}; {} problem(s), first: {}", problems.len(), problems[0]);
  }
  Ok(json!({"message": msg, "label": label, "master": path, "problems": problems}))
}
