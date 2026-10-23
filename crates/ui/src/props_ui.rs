//! INSPECT > Properties: a panel docked on the right, as Fusion's Properties, that follows the
//! selection. It lists what each selected entity is with its geometry (`inspect.properties`), and
//! the overall width, height and thickness (`inspect.overall_size`) when bodies or nothing are
//! selected. Copy puts the text on the clipboard; Export… writes JSON, TXT (or CSV for a size).

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, RichText};
use fox_engine::Sel;
use fox_engine::model::units::LengthUnit;
use serde_json::{Value, json};

use crate::{App, theme};

static OPEN: AtomicBool = AtomicBool::new(false);

pub fn toggle() {
  OPEN.fetch_xor(true, Ordering::Relaxed);
}

pub fn is_open() -> bool {
  OPEN.load(Ordering::Relaxed)
}

/// What the panel shows, recomputed when the selection or the model changes.
#[derive(Default)]
struct Shown {
  key: Option<(Vec<Sel>, u64)>,
  items: Vec<Value>,
  /// The items as passed to `inspect.properties` (the selection, plus a whole selected component).
  query: Vec<Value>,
  size: Option<Value>,
  /// The bodies the size was taken of (`None` = every visible one).
  size_of: Option<Vec<u32>>,
  error: Option<String>,
}

thread_local! {
  static SHOWN: RefCell<Shown> = RefCell::new(Shown::default());
}

fn refresh(app: &mut App, sh: &mut Shown) {
  let key = (app.s.selection.clone(), app.s.revision);
  if sh.key.as_ref() == Some(&key) || app.rebuild.is_some() {
    return;
  }
  let mut query: Vec<Value> = app.s.selection.iter().filter_map(|x| serde_json::to_value(x).ok()).collect();
  let bodies: Vec<u32> =
    app.s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(body.0) } else { None }).collect();
  let all_bodies = bodies.len() == app.s.selection.len();
  // every body of a component selected (as clicking it in the browser does): the component too
  if all_bodies && !bodies.is_empty() {
    for c in &app.s.design.components {
      if !c.bodies.is_empty() && c.bodies.len() == bodies.len() && c.bodies.iter().all(|b| bodies.contains(&b.0)) {
        query.insert(0, json!({"type": "component", "component": c.id.0}));
      }
    }
  }
  *sh = Shown { key: Some(key), query, ..Shown::default() };
  match app.s.execute("inspect.properties", &json!({"items": sh.query})) {
    Ok(v) => sh.items = v["items"].as_array().cloned().unwrap_or_default(),
    Err(e) => sh.error = Some(e.0),
  }
  if all_bodies {
    sh.size_of = (!bodies.is_empty()).then_some(bodies);
    let p = match &sh.size_of {
      Some(b) => json!({"bodies": b}),
      None => json!({}),
    };
    sh.size = app.s.execute("inspect.overall_size", &p).ok();
  }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  if !is_open() {
    return;
  }
  SHOWN.with_borrow_mut(|sh| {
    refresh(app, sh);
    egui::Panel::right("properties_panel")
      .default_size(300.0)
      .resizable(true)
      .frame(
        egui::Frame::NONE
          .fill(theme::SIDE_PANEL)
          .inner_margin(egui::Margin::same(8))
          .stroke(egui::Stroke::new(1.0, theme::BORDER)),
      )
      .show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.label(RichText::new("PROPERTIES").small().strong().color(theme::GROUP_LABEL));
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").on_hover_text("Close the Properties panel").clicked() {
              OPEN.store(false, Ordering::Relaxed);
            }
            if ui.small_button("Export…").on_hover_text("Write these properties to a JSON or text file").clicked() {
              export(app, sh);
            }
            if ui.small_button("Copy").on_hover_text("Copy these properties as text").clicked() {
              ui.ctx().copy_text(text(sh, app.s.design.units));
              app.status = Some(("Properties copied to the clipboard".into(), false));
            }
          });
        });
        ui.separator();
        let u = app.s.design.units;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
          if let Some(e) = &sh.error {
            ui.colored_label(theme::ERROR, e);
          }
          if sh.items.is_empty() && sh.error.is_none() {
            ui.label(RichText::new("Nothing selected").italics().color(Color32::from_gray(110)));
          }
          for (i, item) in sh.items.iter().enumerate() {
            egui::CollapsingHeader::new(RichText::new(title(item)).strong())
              .id_salt(("prop-item", i))
              .default_open(true)
              .show(ui, |ui| rows(ui, ("prop-grid", i), item, u));
          }
          if let Some(size) = &sh.size {
            ui.add_space(6.0);
            let of = if sh.size_of.is_some() { "selected bodies" } else { "visible bodies" };
            egui::CollapsingHeader::new(RichText::new(format!("Overall size ({of})")).strong())
              .id_salt("prop-size")
              .default_open(true)
              .show(ui, |ui| {
                egui::Grid::new("prop-size-grid").num_columns(2).striped(true).show(ui, |ui| {
                  for (label, k) in [("Width", "width"), ("Height", "height"), ("Thickness", "thickness")] {
                    ui.label(label);
                    ui.label(u.format(size[k].as_f64().unwrap_or(0.0)));
                    ui.end_row();
                  }
                  ui.label("Measured");
                  ui.label(if size["mode"] == "planar" { "across the largest flat face" } else { "box extents" });
                  ui.end_row();
                });
              });
          }
        });
      });
  });
}

/// "Face 3 · Body1", "Body · Plate", "Sketch Curve 2 · Sketch1".
fn title(item: &Value) -> String {
  let kind = item["type"].as_str().unwrap_or("item");
  let mut name = label(kind);
  if let Some(n) = item["name"].as_str() {
    name = format!("{name} · {n}");
  } else {
    let id = match &item["id"] {
      Value::Object(o) => o.iter().filter(|(k, _)| *k != "body" && *k != "sketch").map(|(_, v)| v.to_string()).next(),
      v => Some(v.to_string()),
    };
    if let Some(id) = id {
      name = format!("{name} {id}");
    }
    if let Some(parent) = item["body"].as_str().or(item["sketch"].as_str()) {
      name = format!("{name} · {parent}");
    }
  }
  name
}

/// `area_mm2` → "Area", `start_2d` → "Start (2D)", `sheet_metal` → "Sheet metal".
fn label(key: &str) -> String {
  let (stem, tail) = match key {
    k if k.ends_with("_2d") => (&k[..k.len() - 3], " (2D)"),
    k => (k.trim_end_matches("_mm2").trim_end_matches("_mm3").trim_end_matches("_deg").trim_end_matches("_g"), ""),
  };
  let mut s = stem.replace('_', " ");
  if let Some(f) = s.get(0..1) {
    s = f.to_uppercase() + &s[1..];
  }
  s + tail
}

/// Keys whose numbers are directions, not lengths.
const UNITLESS: &[&str] = &["normal", "x", "y", "direction", "axis"];
/// Keys whose numbers are counts.
const COUNTS: &[&str] = &["faces", "edges", "curves", "points", "constraints", "profiles", "holes", "bodies"];

/// One property value in the design's units.
fn value(key: &str, v: &Value, u: LengthUnit) -> String {
  let num = |x: f64| -> String {
    if key.ends_with("_mm2") {
      u.area(x, 4)
    } else if key.ends_with("_mm3") {
      format!("{:.4} {}^3", x / u.mm().powi(3), u.symbol())
    } else if key.ends_with("_g") {
      format!("{x:.3} g")
    } else if key.ends_with("_deg") {
      format!("{x:.3}°")
    } else if UNITLESS.contains(&key) {
      format!("{x:.4}")
    } else {
      u.format(x)
    }
  };
  match v {
    Value::Null => "—".into(),
    Value::Bool(b) => (if *b { "Yes" } else { "No" }).into(),
    // identifiers ("plane", "sheet_metal") read as words; names stay as they are
    Value::String(s) if s.chars().all(|c| c.is_ascii_lowercase() || c == '_') => label(s),
    Value::String(s) => s.clone(),
    Value::Number(n) if COUNTS.contains(&key) || (n.is_u64() && key == "id") => n.to_string(),
    Value::Number(n) => num(n.as_f64().unwrap_or(0.0)),
    Value::Array(a) if a.iter().all(Value::is_number) => {
      let inner: Vec<String> = a
        .iter()
        .map(|x| {
          let x = x.as_f64().unwrap_or(0.0);
          if UNITLESS.contains(&key) { format!("{x:.4}") } else { u.number(x) }
        })
        .collect();
      let unit = if UNITLESS.contains(&key) || COUNTS.contains(&key) { "" } else { u.symbol() };
      format!("({}) {unit}", inner.join(", ")).trim_end().to_string()
    }
    Value::Array(a) => format!("{} items", a.len()),
    Value::Object(o) => o.iter().map(|(k, x)| format!("{k} {}", value(k, x, u))).collect::<Vec<_>>().join(", "),
  }
}

/// The item's properties as label/value rows; nested objects (box, plane, frame) as indented rows.
fn rows(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, item: &Value, u: LengthUnit) {
  egui::Grid::new(id).num_columns(2).striped(true).show(ui, |ui| {
    let Some(o) = item.as_object() else { return };
    for (k, v) in o.iter().filter(|(k, _)| *k != "type") {
      match (k.as_str(), v) {
        ("component" | "parent", Value::Object(c)) => {
          ui.label(label(k));
          ui.label(c.get("name").and_then(Value::as_str).unwrap_or("-"));
          ui.end_row();
        }
        ("id", Value::Object(c)) => {
          ui.label("Id");
          ui.label(c.iter().map(|(k, x)| format!("{k} {x}")).collect::<Vec<_>>().join(", "));
          ui.end_row();
        }
        (_, Value::Object(c)) if c.values().any(|x| x.is_array() || x.is_object()) => {
          ui.label(RichText::new(label(k)).strong());
          ui.end_row();
          for (k2, x) in c {
            ui.label(format!("    {}", label(k2)));
            ui.label(value(k2, x, u));
            ui.end_row();
          }
        }
        (_, Value::Null) => {}
        _ => {
          ui.label(label(k));
          ui.label(value(k, v, u));
          ui.end_row();
        }
      }
    }
  });
}

/// Everything the panel shows, as plain text.
fn text(sh: &Shown, u: LengthUnit) -> String {
  let mut out = fox_engine::properties::properties_text(&json!(sh.items));
  if let Some(size) = &sh.size {
    if !out.is_empty() {
      out.push('\n');
    }
    out.push_str("overall size\n");
    for k in ["width", "height", "thickness"] {
      out.push_str(&format!("  {k}: {}\n", u.format(size[k].as_f64().unwrap_or(0.0))));
    }
    out.push_str(&format!("  mode: {}\n", size["mode"].as_str().unwrap_or("")));
  }
  out
}

/// Export…: the properties as JSON or text, or (with nothing selected) the overall size as JSON,
/// CSV or text.
fn export(app: &mut App, sh: &Shown) {
  let name = app.s.design.name.clone();
  let size_only = sh.items.is_empty() && sh.size.is_some();
  let d = rfd::FileDialog::new().add_filter("JSON", &["json"]).add_filter("Text", &["txt"]);
  let d = if size_only {
    d.set_title("Export overall size").add_filter("CSV", &["csv"]).set_file_name(format!("{name} - Dimensions.json"))
  } else {
    d.set_title("Export properties").set_file_name(format!("{name} - Properties.json"))
  };
  let Some(path) = d.save_file() else { return };
  let path = path.to_string_lossy().to_string();
  let done = if size_only {
    let mut p = json!({"export": path});
    if let Some(b) = &sh.size_of {
      p["bodies"] = json!(b);
    }
    app.run("inspect.overall_size", p)
  } else {
    app.run("inspect.properties", json!({"items": sh.query, "export": path}))
  };
  if done.is_some() {
    app.status = Some((format!("Written to {path}"), false));
  }
}
