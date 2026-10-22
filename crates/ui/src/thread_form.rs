//! Thread's Thread Type, Size, Designation and Class: Fusion's drop-downs, fed by the thread
//! tables (`fox_model::thread`), and the `thread` command their values make.
//!
//! ISO Metric profile threads go to the engine by designation and class (the face is sized to
//! the class). ANSI Unified Screw Threads go as their pitch: the engine cuts the basic 60°
//! profile on the cylinder as drawn (it has no Unified class allowances yet).

use fox_engine::Session;
use fox_engine::kernel::SurfaceKind;
use fox_engine::model::thread::{ISO_SIZES, UNIFIED_SIZES, basic_minor};
use fox_engine::model::{BodyId, LengthUnit};
use serde_json::{Map, Value, json};

use crate::forms::{Form, State, short};

/// Thread Type: the thread families, as Fusion names them.
pub const TYPES: &[(&str, &str)] = &[("ansi", "ANSI Unified Screw Threads"), ("iso", "ISO Metric profile")];

const MM_PER_IN: f64 = 25.4;

/// One size of a thread type.
#[derive(Debug, Clone, PartialEq)]
pub struct Size {
  /// The value the form stores: ISO's nominal mm ("10"), ANSI's inches ("0.25", "1.0").
  pub key: String,
  /// How the Size drop-down shows it ("10.0 mm", "1.0 in").
  pub label: String,
  /// Nominal (major) diameter, mm.
  pub diameter: f64,
  /// Designations ("M10x1.5", "1-8 UNC") with their pitches (mm), coarse first.
  pub designations: Vec<(String, f64)>,
}

/// The sizes of a thread type, smallest first.
pub fn sizes(kind: &str) -> Vec<Size> {
  if kind == "ansi" {
    UNIFIED_SIZES
      .iter()
      .map(|(name, d, tpis)| Size {
        key: inch(*d),
        label: format!("{} in", inch(*d)),
        diameter: d * MM_PER_IN,
        designations: tpis
          .iter()
          .map(|(n, series)| (format!("{name}-{} {series}", short(*n)), MM_PER_IN / n))
          .collect(),
      })
      .collect()
  } else {
    ISO_SIZES
      .iter()
      .map(|(d, pitches)| Size {
        key: short(*d),
        label: format!("{d:.1} mm"),
        diameter: *d,
        designations: pitches.iter().map(|p| (format!("M{}x{}", short(*d), short(*p)), *p)).collect(),
      })
      .collect()
  }
}

/// An inch size as Fusion lists it: "1.0", "0.25", "0.3125".
fn inch(d: f64) -> String {
  let t = format!("{d:.4}");
  let t = t.trim_end_matches('0');
  if t.ends_with('.') { format!("{t}0") } else { t.to_string() }
}

/// The tolerance classes of a thread type, outside (rods) or inside (holes).
pub fn classes(kind: &str, internal: bool) -> &'static [&'static str] {
  match (kind, internal) {
    ("ansi", false) => &["1A", "2A", "3A"],
    ("ansi", true) => &["1B", "2B", "3B"],
    (_, false) => &["4g6g", "4h6h", "6e", "6f", "6g", "6h"],
    (_, true) => &["4H", "5H", "6G", "6H", "7G", "7H"],
  }
}

/// The class a new thread gets.
pub fn default_class(kind: &str, internal: bool) -> &'static str {
  match (kind, internal) {
    ("ansi", false) => "3A",
    ("ansi", true) => "3B",
    (_, false) => "6g",
    (_, true) => "6H",
  }
}

/// Is a class an inside (hole) one? ISO's inside classes are upper case (6H), the Unified
/// ones end in B (3B; outside 3A).
pub fn internal_class(class: &str) -> bool {
  class.chars().last().is_some_and(|c| c == 'B' || c.is_ascii_uppercase() && c != 'A')
}

/// The size and coarse designation that fit a cylinder drawn at `diameter` (mm): a rod by its
/// major diameter, a hole by the minor diameter it is tapped at.
pub fn fit(kind: &str, diameter: f64, internal: bool) -> Option<(String, String)> {
  let drawn = |s: &Size| {
    let p = s.designations.first().map(|d| d.1).unwrap_or(0.0);
    if internal { basic_minor(s.diameter, p) } else { s.diameter }
  };
  let all = sizes(kind);
  let best = all.iter().min_by(|a, b| (drawn(a) - diameter).abs().total_cmp(&(drawn(b) - diameter).abs()))?;
  Some((best.key.clone(), best.designations.first()?.0.clone()))
}

/// A picked cylindrical face: its diameter (mm), whether it is a hole and its length (mm).
pub fn face_cylinder(s: &Session, v: &Value) -> Option<(f64, bool, f64)> {
  let body = s.model.body(BodyId(v.get("body")?.as_u64()? as u32))?;
  let id = v.get("face")?.as_u64()? as u32;
  let f = body.faces().iter().find(|f| f.id == id)?;
  if f.kind != SurfaceKind::Cylinder || f.radius <= 0.0 {
    return None;
  }
  // a hole has material just outside its wall (a full face's centre is on its axis: any way out)
  let axis = f.normal.normalize_or_zero();
  let on_axis = f.origin + axis * (f.center - f.origin).dot(axis);
  let out = (f.center - on_axis).try_normalize().unwrap_or_else(|| axis.any_orthonormal_vector());
  let probe = on_axis + out * (f.radius + (f.radius * 1e-3).max(1e-4));
  let internal = body.shape.classify(probe).is_ok_and(|w| w == fox_engine::kernel::Inside::In);
  let length = f.area / (std::f64::consts::TAU * f.radius);
  Some((2.0 * f.radius, internal, length))
}

/// After [`State::sync`]: a new face pick picks the size that fits it (in the design's thread
/// type, ANSI in an inch design as in Fusion) and its length.
pub fn sync(form: &Form, state: &mut State, before: &[Value], s: &Session) {
  let at = |k: &str| form.inputs.iter().position(|x| x.key() == k);
  let (Some(ti), Some(fi)) = (at("thread_type"), at("faces")) else { return };
  if state.values[ti].as_str().is_none_or(str::is_empty) {
    state.values[ti] = json!(if s.design.units == LengthUnit::In { "ansi" } else { "iso" });
  }
  let picked_anew = before.get(fi) != Some(&state.values[fi]);
  let size_unset = at("size").is_some_and(|i| state.values[i].as_str().is_none_or(str::is_empty));
  if !picked_anew && !size_unset {
    return;
  }
  let face = state.values[fi].as_array().and_then(|a| a.first()).and_then(|f| face_cylinder(s, f));
  let kind = state.values[ti].as_str().unwrap_or("iso").to_string();
  let (diameter, internal, length) = face.unwrap_or((if kind == "ansi" { MM_PER_IN } else { 10.0 }, false, 0.0));
  refit(form, state, &kind, diameter, internal);
  if let Some(li) = at("length")
    && length > 0.0
  {
    state.values[li] = json!(format!("{} mm", short(length)));
  }
}

/// Sets Size, Designation and Class to fit a cylinder of `diameter`.
pub fn refit(form: &Form, state: &mut State, kind: &str, diameter: f64, internal: bool) {
  let Some((size, designation)) = fit(kind, diameter, internal) else { return };
  for (input, v) in form.inputs.iter().zip(state.values.iter_mut()) {
    match input.key() {
      "size" => *v = json!(size),
      "designation" => *v = json!(designation),
      "class" => *v = json!(default_class(kind, internal)),
      _ => {}
    }
  }
}

/// The `thread` command of the form's parameters.
pub fn engine_params(p: &Value) -> Value {
  let mut m: Map<String, Value> = p.as_object().cloned().unwrap_or_default();
  let kind = m.remove("thread_type");
  let size = m.remove("size");
  m.remove("remember");
  m.remove("full_length");
  if m.remove("direction") == Some(json!("left")) {
    m.insert("left".into(), json!(true));
  }
  if kind == Some(json!("ansi")) {
    // the pitch of the picked designation; the engine has no Unified classes
    let designation = m.remove("designation");
    m.remove("class");
    let pitch = sizes("ansi")
      .into_iter()
      .filter(|s| size.as_ref().is_none_or(|k| *k == json!(s.key)))
      .flat_map(|s| s.designations)
      .find(|d| designation.as_ref() == Some(&json!(d.0)))
      .map(|d| d.1);
    if let Some(p) = pitch {
      m.insert("pitch".into(), json!(format!("{p:.6} mm")));
    }
  }
  Value::Object(m)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sizes_designations_and_fits() {
    let ansi = sizes("ansi");
    let one = ansi.iter().find(|s| s.key == "1.0").unwrap();
    assert_eq!(one.label, "1.0 in");
    assert_eq!(one.designations[0].0, "1-8 UNC");
    assert!((one.designations[0].1 - 3.175).abs() < 1e-9);
    assert_eq!(ansi.iter().find(|s| s.key == "0.25").unwrap().label, "0.25 in");
    assert_eq!(ansi.iter().find(|s| s.key == "0.19").unwrap().designations[0].0, "10-24 UNC");
    let iso = sizes("iso");
    let m10 = iso.iter().find(|s| s.key == "10").unwrap();
    assert_eq!(m10.label, "10.0 mm");
    assert_eq!(
      m10.designations.iter().map(|d| d.0.as_str()).collect::<Vec<_>>(),
      ["M10x1.5", "M10x1.25", "M10x1", "M10x0.75"]
    );
    // a 1 in rod takes 1-8 UNC; a Ø8.5 bore is tapped M10x1.5
    assert_eq!(fit("ansi", 25.4, false), Some(("1.0".into(), "1-8 UNC".into())));
    assert_eq!(fit("iso", 8.5, true), Some(("10".into(), "M10x1.5".into())));
    assert_eq!(fit("iso", 9.9, false), Some(("10".into(), "M10x1.5".into())));
    assert!(internal_class("6H") && internal_class("3B") && !internal_class("6g") && !internal_class("3A"));
  }

  #[test]
  fn ansi_threads_go_by_pitch() {
    let p = json!({"faces": [], "thread_type": "ansi", "size": "1.0", "designation": "1-8 UNC", "class": "3A",
      "direction": "left", "full_length": true, "remember": true, "modeled": false});
    let q = engine_params(&p);
    assert_eq!(q, json!({"faces": [], "pitch": "3.175000 mm", "left": true, "modeled": false}));
    let p = json!({"faces": [], "thread_type": "iso", "size": "10", "designation": "M10x1.5", "class": "6g",
      "direction": "right"});
    assert_eq!(engine_params(&p), json!({"faces": [], "designation": "M10x1.5", "class": "6g"}));
  }
}
