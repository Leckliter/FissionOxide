//! Document units and model parameters.
//!
//! A design has a length unit (Fusion's Document Settings > Units). Values stay in millimetres
//! inside the model and in saved expressions: the unit only changes how lengths are shown and what
//! a bare number typed into a length field means (the UI pins such a number to the unit, so `2`
//! typed in an inch design is stored as `2 in`).
//!
//! Model parameters are Fusion's d1, d2, ...: every expression inside the timeline's features
//! (sketch dimensions, extrude distances, fillet radii, ...), numbered in timeline order.

use serde::{Deserialize, Serialize};

use crate::{Design, DesignError, FeatureId, FeatureKind, ParamKind};

/// A design's length unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
  #[default]
  Mm,
  Cm,
  M,
  In,
  Ft,
}

impl LengthUnit {
  pub const ALL: [LengthUnit; 5] = [LengthUnit::Mm, LengthUnit::Cm, LengthUnit::M, LengthUnit::In, LengthUnit::Ft];

  /// The unit as typed in expressions ("mm", "in").
  pub fn symbol(self) -> &'static str {
    match self {
      LengthUnit::Mm => "mm",
      LengthUnit::Cm => "cm",
      LengthUnit::M => "m",
      LengthUnit::In => "in",
      LengthUnit::Ft => "ft",
    }
  }

  /// The mass unit that goes with this length unit, as Fusion's Document Settings row names it
  /// ("Units: in, ouncemass"): ounces with inches, pounds with feet, grams and kilograms with the
  /// metric units.
  pub fn mass_symbol(self) -> &'static str {
    match self {
      LengthUnit::Mm | LengthUnit::Cm => "g",
      LengthUnit::M => "kg",
      LengthUnit::In => "ouncemass",
      LengthUnit::Ft => "lbmass",
    }
  }

  /// The unit's name, as Fusion's unit list shows it.
  pub fn label(self) -> &'static str {
    match self {
      LengthUnit::Mm => "Millimeter",
      LengthUnit::Cm => "Centimeter",
      LengthUnit::M => "Meter",
      LengthUnit::In => "Inch",
      LengthUnit::Ft => "Foot",
    }
  }

  /// Millimetres in one of this unit.
  pub fn mm(self) -> f64 {
    match self {
      LengthUnit::Mm => 1.0,
      LengthUnit::Cm => 10.0,
      LengthUnit::M => 1000.0,
      LengthUnit::In => 25.4,
      LengthUnit::Ft => 304.8,
    }
  }

  pub fn parse(s: &str) -> Option<LengthUnit> {
    let s = s.trim().to_ascii_lowercase();
    LengthUnit::ALL.into_iter().find(|u| u.symbol() == s || u.label().to_ascii_lowercase() == s)
  }

  pub fn is_mm(&self) -> bool {
    *self == LengthUnit::Mm
  }

  /// A length in millimetres, in this unit.
  pub fn from_mm(self, mm: f64) -> f64 {
    mm / self.mm()
  }

  /// Decimals kept when showing a number: about a ten-thousandth of a millimetre in any unit.
  fn decimals(self) -> usize {
    match self {
      LengthUnit::Mm => 4,
      LengthUnit::Cm => 5,
      LengthUnit::In => 6,
      LengthUnit::M | LengthUnit::Ft => 7,
    }
  }

  /// A length (mm) as a bare number in this unit, without trailing zeros ("0.393701").
  pub fn number(self, mm: f64) -> String {
    let s = format!("{:.*}", self.decimals(), self.from_mm(mm));
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
  }

  /// A length (mm) with this unit ("0.393701 in"): also a valid expression for that length.
  pub fn format(self, mm: f64) -> String {
    format!("{} {}", self.number(mm), self.symbol())
  }

  /// A length (mm) with a fixed number of decimals, for read-outs ("0.394 in").
  pub fn fixed(self, mm: f64, decimals: usize) -> String {
    format!("{:.*} {}", decimals, self.from_mm(mm), self.symbol())
  }

  /// An area (mm²) in this unit squared with a fixed number of decimals, as Fusion's status bar
  /// reads it out ("1.24 in^2").
  pub fn area(self, mm2: f64, decimals: usize) -> String {
    format!("{:.*} {}^2", decimals, mm2 / self.mm().powi(2), self.symbol())
  }

  /// A length typed into a length field: a value without any unit (and without parameters, which
  /// carry their own) means this unit, so it is pinned to it. `"2"` → `"2 in"`,
  /// `"2 * 3"` → `"(2 * 3) in"`. Anything else is kept as typed.
  pub fn typed(self, expr: &str) -> String {
    if self.is_mm() || !bare(expr) {
      return expr.to_string();
    }
    let t = expr.trim();
    if t.parse::<f64>().is_ok() { format!("{t} {}", self.symbol()) } else { format!("({t}) {}", self.symbol()) }
  }

  /// A length typed into a manipulator's value box, as Fusion's box fills it in: a bare number
  /// gets this unit written out even in millimetres (`"25"` → `"25 mm"`), so the dialog's row and
  /// the stored feature show it. Anything else is kept as typed.
  pub fn labelled(self, expr: &str) -> String {
    if !bare(expr) {
      return expr.to_string();
    }
    let t = expr.trim();
    if t.parse::<f64>().is_ok() { format!("{t} {}", self.symbol()) } else { format!("({t}) {}", self.symbol()) }
  }

  /// A stored length expression as a field shows it: stored bare numbers are millimetres, so in
  /// a design in another unit they are shown with "mm" to keep their meaning.
  pub fn shown(self, expr: &str) -> String {
    LengthUnit::Mm.typed_from(expr, self)
  }

  /// A stored length expression as an edit box shows it: always with a unit, as Fusion's boxes
  /// are. Stored bare numbers are millimetres, so `"2"` → `"2 mm"` in every design.
  pub fn pinned(expr: &str) -> String {
    LengthUnit::Mm.typed_from(expr, LengthUnit::In)
  }

  /// Pins a bare `expr` to `self` when the design unit is `design` (see [`LengthUnit::shown`]).
  fn typed_from(self, expr: &str, design: LengthUnit) -> String {
    if design == self || !bare(expr) {
      return expr.to_string();
    }
    let t = expr.trim();
    if t.parse::<f64>().is_ok() { format!("{t} {}", self.symbol()) } else { format!("({t}) {}", self.symbol()) }
  }

  /// A default written in millimetres (`"10 mm"`) in this unit (`"0.393701 in"`). Anything that
  /// isn't a plain millimetre value is kept.
  pub fn localize(self, default: &str) -> String {
    if self.is_mm() {
      return default.to_string();
    }
    match default.trim().strip_suffix("mm").map(|n| n.trim().parse::<f64>()) {
      Some(Ok(v)) => self.format(v),
      _ => default.to_string(),
    }
  }
}

/// Does `expr` evaluate without any unit or parameter (so a field's unit applies to it)?
fn bare(expr: &str) -> bool {
  matches!(fox_expr::eval_raw(expr, &fox_expr::Empty), Ok(v) if !v.has_unit)
}

/// Does `expr` name a length unit (so the field holding it takes lengths)?
pub fn names_length_unit(expr: &str) -> bool {
  fox_expr::identifiers(expr).iter().any(|i| matches!(i.as_str(), "mm" | "cm" | "m" | "um" | "in" | "ft"))
}

/// One of a feature's expressions, as Change Parameters lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelParam {
  /// d1, d2, ... in timeline order.
  pub name: String,
  pub feature: FeatureId,
  pub feature_name: String,
  /// Which of the feature's expressions (see [`crate::for_each_expr`]).
  pub index: usize,
  /// What it sets ("distance", "angle", "radius", "count").
  pub role: String,
  pub expr: String,
  pub kind: ParamKind,
  pub favorite: bool,
}

/// A model parameter's place: its feature and which of the feature's expressions it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelParamRef {
  pub feature: FeatureId,
  pub index: usize,
}

impl Design {
  /// Fusion's model parameters: every expression of every feature, d1, d2, ... in timeline order.
  pub fn model_parameters(&self) -> Vec<ModelParam> {
    let mut out = Vec::new();
    for f in &self.features {
      for (index, (expr, role, kind)) in feature_exprs(&f.kind).into_iter().enumerate() {
        out.push(ModelParam {
          name: format!("d{}", out.len() + 1),
          feature: f.id,
          feature_name: f.name.clone(),
          index,
          role,
          expr,
          kind,
          favorite: self.model_favorites.contains(&ModelParamRef { feature: f.id, index }),
        });
      }
    }
    out
  }

  /// Sets one of a feature's expressions (see [`Design::model_parameters`]).
  pub fn set_model_parameter(&mut self, feature: FeatureId, index: usize, expr: &str) -> Result<(), DesignError> {
    let f = self.feature_mut(feature).ok_or(DesignError::NoFeature(feature))?;
    let mut i = 0;
    let mut found = false;
    crate::for_each_expr(&mut f.kind, &mut |e| {
      if i == index {
        *e = expr.to_string();
        found = true;
      }
      i += 1;
    });
    if !found {
      return Err(DesignError::NoParameter(format!("{}:{index}", f.name)));
    }
    // a sketch dimension's value follows on the next solve; keep it close meanwhile
    if let FeatureKind::Sketch { sketch, .. } = &mut f.kind
      && let Some(c) = sketch.constraints.iter_mut().filter(|c| c.kind.dim().is_some()).nth(index)
    {
      let k = if c.kind.is_angle() { ParamKind::Angle } else { ParamKind::Length };
      if let (Ok(v), Some(d)) = (fox_expr::eval(expr, k, &fox_expr::Empty), c.kind.dim_mut()) {
        d.value = v;
      }
    }
    Ok(())
  }

  /// Stars or un-stars a model parameter (Fusion's Favorites).
  pub fn set_model_favorite(&mut self, at: ModelParamRef, on: bool) {
    self.model_favorites.retain(|r| *r != at);
    if on {
      self.model_favorites.push(at);
    }
  }
}

/// A feature's expressions with what each sets and its kind of quantity.
///
/// Sketch dimensions take their kind from the constraint. Other features' expressions are found
/// by the field holding them: [`crate::for_each_expr`] marks each one, and the marks are looked
/// up in the feature's JSON.
fn feature_exprs(kind: &FeatureKind) -> Vec<(String, String, ParamKind)> {
  if let FeatureKind::Sketch { sketch, .. } = kind {
    return sketch
      .constraints
      .iter()
      .filter_map(|c| {
        let d = c.kind.dim()?;
        let k = if c.kind.is_angle() { ParamKind::Angle } else { ParamKind::Length };
        let role = serde_json::to_value(&c.kind)
          .ok()
          .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
          .unwrap_or_else(|| "dimension".into());
        Some((d.expr.clone(), role, k))
      })
      .collect();
  }
  let mut exprs = Vec::new();
  let mut marked = kind.clone();
  crate::for_each_expr(&mut marked, &mut |e| {
    exprs.push(e.clone());
    *e = format!("\u{1}{}", exprs.len() - 1);
  });
  let mut roles = vec![String::new(); exprs.len()];
  if let Ok(v) = serde_json::to_value(&marked) {
    find_marks(&v, "", &mut roles);
  }
  exprs
    .into_iter()
    .zip(roles)
    .map(|(e, role)| {
      let k = kind_of(&role, &e);
      let role = if role.is_empty() { "value".to_string() } else { role };
      (e, role, k)
    })
    .collect()
}

fn find_marks(v: &serde_json::Value, key: &str, roles: &mut [String]) {
  match v {
    serde_json::Value::String(s) => {
      if let Some(i) = s.strip_prefix('\u{1}').and_then(|n| n.parse::<usize>().ok())
        && let Some(r) = roles.get_mut(i)
      {
        *r = key.to_string();
      }
    }
    serde_json::Value::Array(a) => a.iter().for_each(|x| find_marks(x, key, roles)),
    serde_json::Value::Object(o) => {
      for (k, x) in o {
        // limits: {"rotation": [lo, hi]} keeps "rotation"; plain values keep their own key
        find_marks(x, k, roles);
      }
    }
    _ => {}
  }
}

/// A feature expression's kind, by its field name (and its units when it names some).
fn kind_of(role: &str, expr: &str) -> ParamKind {
  let ids = fox_expr::identifiers(expr);
  if ids.iter().any(|i| i == "deg" || i == "rad") {
    return ParamKind::Angle;
  }
  if names_length_unit(expr) {
    return ParamKind::Length;
  }
  let r = role.to_ascii_lowercase();
  if r.contains("angle") || r.contains("taper") || r.contains("twist") || r == "rotation" {
    ParamKind::Angle
  } else if r.starts_with("count") || r.starts_with("factor") || r == "bends" || r == "amount" {
    ParamKind::Unitless
  } else {
    ParamKind::Length
  }
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::panic)]
  use super::*;
  use crate::{Extent, Operation, OriginPlane, PlaneRef};
  use fox_sketch::{Dim, Sketch};

  #[test]
  fn formats_lengths_in_the_unit() {
    assert_eq!(LengthUnit::Mm.format(25.4), "25.4 mm");
    assert_eq!(LengthUnit::In.format(25.4), "1 in");
    assert_eq!(LengthUnit::Cm.format(12.0), "1.2 cm");
    assert_eq!(LengthUnit::M.format(1500.0), "1.5 m");
    assert_eq!(LengthUnit::Ft.format(304.8 * 2.0), "2 ft");
    assert_eq!(LengthUnit::In.number(10.0), "0.393701");
    assert_eq!(LengthUnit::In.fixed(25.4, 3), "1.000 in");
    // what it shows reads back as the same length
    let back = fox_expr::eval(&LengthUnit::In.format(10.0), ParamKind::Length, &fox_expr::Empty).unwrap();
    assert!((back - 10.0).abs() < 1e-4);
  }

  #[test]
  fn bare_numbers_take_the_unit() {
    let u = LengthUnit::In;
    assert_eq!(u.typed("2"), "2 in");
    assert_eq!(u.typed(" 2 * 3 "), "(2 * 3) in");
    assert_eq!(u.typed("5 mm"), "5 mm", "an explicit unit stays");
    assert_eq!(u.typed("w / 2"), "w / 2", "parameters carry their own unit");
    assert_eq!(u.typed("2 +"), "2 +", "broken input is left for the field to report");
    assert_eq!(LengthUnit::Mm.typed("2"), "2", "bare numbers already mean mm");
    assert_eq!(LengthUnit::Mm.labelled("25"), "25 mm", "the box writes millimetres out");
    assert_eq!(LengthUnit::Mm.labelled("2 * 3"), "(2 * 3) mm");
    assert_eq!(u.labelled("2"), "2 in");
    assert_eq!(u.labelled("w / 2"), "w / 2");
    assert_eq!(u.labelled("-"), "-", "half-typed input is kept");
    let mm = |s: &str| fox_expr::eval(s, ParamKind::Length, &fox_expr::Empty).unwrap();
    assert_eq!(mm(&u.typed("2")), 50.8);
    assert_eq!(mm(&LengthUnit::Cm.typed("1.5")), 15.0);
    assert_eq!(mm(&LengthUnit::M.typed("2/4")), 500.0);
    // a stored bare number is millimetres, so it is shown with mm in an inch design
    assert_eq!(u.shown("10"), "10 mm");
    assert_eq!(u.shown("1 in"), "1 in");
    assert_eq!(LengthUnit::Mm.shown("10"), "10");
    // an edit box always shows the unit, in a mm design too
    assert_eq!(LengthUnit::pinned("2"), "2 mm");
    assert_eq!(LengthUnit::pinned("2 * 3"), "(2 * 3) mm");
    assert_eq!(LengthUnit::pinned("0.1 in"), "0.1 in");
    assert_eq!(LengthUnit::pinned("w / 2"), "w / 2");
    assert_eq!(LengthUnit::pinned(""), "");
    assert_eq!(u.localize("10 mm"), "0.393701 in");
    assert_eq!(u.localize("30 deg"), "30 deg");
    assert_eq!(LengthUnit::parse("IN"), Some(LengthUnit::In));
    assert_eq!(LengthUnit::parse("Foot"), Some(LengthUnit::Ft));
    assert_eq!(LengthUnit::parse("yd"), None);
    assert!(names_length_unit("1 in + 2") && !names_length_unit("45 deg") && !names_length_unit("6"));
  }

  #[test]
  fn units_round_trip_and_old_files_load_as_mm() {
    let mut d = Design::new("Plate");
    assert_eq!(d.units, LengthUnit::Mm);
    assert!(!d.to_json().contains("\"units\""), "mm designs save as before");
    d.units = LengthUnit::In;
    let json = d.to_json();
    assert!(json.contains("\"units\": \"in\""), "{json}");
    assert_eq!(Design::from_json(&json).unwrap().units, LengthUnit::In);
    let old = "{\"version\": 2, \"name\": \"Old\"}";
    assert_eq!(Design::from_json(old).unwrap().units, LengthUnit::Mm);
  }

  #[test]
  fn model_parameters_list_every_feature_expression() {
    let mut d = Design::new("t");
    let mut sk = Sketch::new();
    let a = sk.add_point(0.0, 0.0);
    let b = sk.add_point(10.0, 0.0);
    sk.add_constraint(fox_sketch::ConstraintKind::Distance { a, b, value: Dim::expr("10", 10.0) }).unwrap();
    let s = d
      .add_feature(FeatureKind::Sketch {
        plane: PlaneRef::Origin { plane: OriginPlane::Xy },
        sketch: sk,
        links: vec![],
      })
      .unwrap();
    let e = d
      .add_feature(FeatureKind::Extrude {
        sketch: s,
        profiles: vec![],
        extent: Extent::distance("5 mm"),
        operation: Operation::NewBody,
      })
      .unwrap();
    let r = d
      .add_feature(FeatureKind::CircularPattern {
        bodies: crate::BodySel::default(),
        axis: crate::AxisRef::Z,
        count: "6".into(),
        angle: "360".into(),
        symmetric: false,
        join: true,
      })
      .unwrap();
    let list = d.model_parameters();
    let row = |n: &str| list.iter().find(|p| p.name == n).unwrap();
    assert_eq!((row("d1").feature, row("d1").expr.as_str(), row("d1").kind), (s, "10", ParamKind::Length));
    assert_eq!(row("d1").role, "distance");
    assert_eq!((row("d2").feature, row("d2").role.as_str(), row("d2").expr.as_str()), (e, "distance", "5 mm"));
    let pat: Vec<_> = list.iter().filter(|p| p.feature == r).collect();
    assert_eq!(pat.len(), 2);
    assert_eq!((pat[0].role.as_str(), pat[0].kind), ("count", ParamKind::Unitless));
    assert_eq!((pat[1].role.as_str(), pat[1].kind), ("angle", ParamKind::Angle));
    assert_eq!(pat[0].feature_name, d.feature(r).unwrap().name);
    // names run on through the timeline
    let names: Vec<_> = list.iter().map(|p| p.name.clone()).collect();
    assert_eq!(names[..2], ["d1".to_string(), "d2".to_string()]);
    // editing one changes that expression only
    d.set_model_parameter(e, 0, "8 mm").unwrap();
    let FeatureKind::Extrude { extent, .. } = &d.feature(e).unwrap().kind else { panic!() };
    assert_eq!(extent.distance, "8 mm");
    d.set_model_parameter(s, 0, "12").unwrap();
    assert_eq!(d.model_parameters()[0].expr, "12");
    assert!(d.set_model_parameter(e, 99, "1").is_err());
    // favourites survive a save
    d.set_model_favorite(ModelParamRef { feature: e, index: 0 }, true);
    let back = Design::from_json(&d.to_json()).unwrap();
    assert!(back.model_parameters().iter().any(|p| p.feature == e && p.index == 0 && p.favorite));
    d.set_model_favorite(ModelParamRef { feature: e, index: 0 }, false);
    assert!(d.model_parameters().iter().all(|p| !p.favorite));
  }
}
