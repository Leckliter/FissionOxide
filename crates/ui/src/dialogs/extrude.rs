//! The Extrude dialog in Fusion's layout: Type (solid / thin), Profiles, Start (profile plane /
//! offset / object), Direction (one side / two sides / symmetric), Extent Type (distance / to
//! object / all), Distance, Taper Angle, wall location and thickness, Operation. Direction and
//! Extent Type together make the engine's extent (`Dialog::extent`).

use fox_engine::model::{Extent, ThinSide};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, OPS, choice, sel_chip};
use crate::icons::Icon;

/// Extrude values beyond the extent, distances and operation.
#[derive(Debug, Clone, Default)]
pub(crate) struct Rows {
  /// Type: thin (else solid).
  pub(super) thin: bool,
  /// Wall location: "inside" (side 1), "outside" (side 2) or "center".
  thin_side: &'static str,
  thickness: String,
  taper: String,
  /// Side 2 taper of a two-sided extrude; empty = the same as `taper`.
  taper2: String,
  /// Start: "" (profile plane), "offset" or "object".
  pub(super) start: &'static str,
  start_offset: String,
  /// Start: object, the picked face (`{body, face}`); kept as stored when editing.
  pub(super) start_face: Value,
  /// Operation was chosen in the dialog: a body face's extrude no longer picks Join or Cut by
  /// the way its distance goes.
  op_chosen: bool,
}

const DIRECTIONS: [(&str, &str); 3] =
  [("one_side", "One side"), ("two_sides", "Two sides"), ("symmetric", "Symmetric")];
const EXTENT_TYPES: [(&str, &str); 3] = [("distance", "Distance"), ("to_object", "To object"), ("all", "All")];
const STARTS: [(&str, &str); 3] = [("", "Profile plane"), ("offset", "Offset"), ("object", "Object")];
const WALLS: [(&str, &str); 3] = [("inside", "Side 1"), ("outside", "Side 2"), ("center", "Center")];

/// The engine extent of a direction and an extent type.
fn extent_of(direction: &'static str, kind: &str) -> &'static str {
  match (kind, direction) {
    ("to_object", _) => "to_object",
    ("all", "one_side") => "through_all",
    ("all", _) => "through_all_both",
    (_, d) => d,
  }
}

impl Rows {
  /// The taper angle's text (empty: no taper).
  pub(super) fn taper(&self) -> &str {
    &self.taper
  }

  /// Sets the taper angle's text (the taper handle; empty: no taper).
  pub(super) fn set_taper(&mut self, text: String) {
    self.taper = text;
  }

  /// The taper angle's text, for the floating value box Tab moves to the taper handle.
  pub(super) fn taper_mut(&mut self) -> &mut String {
    &mut self.taper
  }
}

impl Dialog {
  fn direction(&self) -> &'static str {
    match self.extent {
      "symmetric" | "through_all_both" => "symmetric",
      "two_sides" => "two_sides",
      _ => "one_side",
    }
  }

  fn extent_type(&self) -> &'static str {
    match self.extent {
      "through_all" | "through_all_both" => "all",
      "to_object" => "to_object",
      _ => "distance",
    }
  }

  /// Rows of the Extrude dialog.
  pub(super) fn extrude_rows(&mut self, ui: &mut egui::Ui) {
    let creating = !self.picks_fixed();
    let mut dirty = false;
    let ext = &mut self.solid.ext;
    // Type: Fusion's pair of icon toggles
    ui.label("Type");
    let mut kind = usize::from(ext.thin);
    if choice::toggles(ui, &[(Icon::ExtrudeSolid, "Solid"), (Icon::ExtrudeThin, "Thin")], &mut kind) {
      ext.thin = kind == 1;
      dirty = true;
    }
    ui.end_row();

    ui.label("Profiles");
    // sketch profiles, else the planar faces picked in their place
    let n = if self.profiles.is_empty() { self.faces.len() } else { self.profiles.len() };
    if !creating {
      ui.label("(unchanged)");
    } else if n == 0 {
      sel_chip(ui, self.accent(), "Select", false);
    } else {
      sel_chip(ui, self.accent(), &format!("{n} selected"), true);
    }
    ui.end_row();
    // the value rows wait for a profile, as in Fusion
    let has_profile = !creating || n > 0;

    let ext = &mut self.solid.ext;
    dirty |= pick(ui, "Start", "ext-start", &STARTS, &mut ext.start);
    if ext.start == "offset" {
      ui.label("Offset");
      let mut r = ui.add(egui::TextEdit::singleline(&mut ext.start_offset).desired_width(140.0));
      crate::units::pin(ui, &mut r, &mut ext.start_offset);
      dirty |= r.changed();
      ui.end_row();
    } else if ext.start == "object" {
      ui.label("Start object");
      if !creating && !ext.start_face.is_null() {
        ui.label("(unchanged)");
      } else {
        let has = !ext.start_face.is_null();
        sel_chip(ui, self.accent(), if has { "1 face" } else { "Select a flat face" }, has);
      }
      ui.end_row();
    }

    let (mut direction, mut kind) = (self.direction(), self.extent_type());
    let one_side_only = kind == "to_object";
    ui.label("Direction");
    ui.add_enabled_ui(!one_side_only, |ui| {
      dirty |= combo(ui, "ext-direction", &DIRECTIONS, &mut direction);
    })
    .response
    .on_disabled_hover_text("To object extends one side");
    ui.end_row();
    dirty |= pick(ui, "Extent type", "ext-type", &EXTENT_TYPES, &mut kind);
    self.extent = extent_of(if kind == "to_object" { "one_side" } else { direction }, kind);
    if self.extent == "to_object" {
      self.object_row(ui);
    }
    // no Flip row: a negative distance, typed or dragged, extrudes the other way (`split_sign`)
    match self.extent {
      _ if !has_profile => {}
      "through_all" | "through_all_both" => {}
      "two_sides" => {
        ui.label("Distance 1");
        self.text(ui, true);
        ui.end_row();
        ui.label("Distance 2");
        let mut r = ui.add(egui::TextEdit::singleline(&mut self.distance2).desired_width(140.0));
        crate::units::pin(ui, &mut r, &mut self.distance2);
        dirty |= r.changed();
        ui.end_row();
      }
      e => {
        ui.label(if e == "to_object" { "Offset" } else { "Distance" });
        self.text(ui, true);
        ui.end_row();
      }
    }

    let two_sides = self.extent == "two_sides";
    let ext = &mut self.solid.ext;
    if has_profile {
      // an empty taper reads as Fusion's "0.0 deg", in the box's own colour rather than a grey hint
      let ink = ui.visuals().text_color();
      let shown = |t: &str| egui::RichText::new(t).color(ink);
      ui.label(if two_sides { "Taper angle 1" } else { "Taper angle" });
      let zero = crate::units::shown_angle("0 deg");
      dirty |=
        ui.add(egui::TextEdit::singleline(&mut ext.taper).hint_text(shown(&zero)).desired_width(140.0)).changed();
      ui.end_row();
      if two_sides {
        ui.label("Taper angle 2");
        let hint = if ext.taper.trim().is_empty() { zero } else { ext.taper.trim().to_string() };
        dirty |=
          ui.add(egui::TextEdit::singleline(&mut ext.taper2).hint_text(shown(&hint)).desired_width(140.0)).changed();
        ui.end_row();
      }
    }
    if ext.thin {
      if ext.thin_side.is_empty() {
        ext.thin_side = "inside";
      }
      dirty |= pick(ui, "Wall location", "ext-wall", &WALLS, &mut ext.thin_side);
      ui.label("Wall thickness");
      let mut r = ui.add(egui::TextEdit::singleline(&mut ext.thickness).hint_text("1 mm").desired_width(140.0));
      crate::units::pin(ui, &mut r, &mut ext.thickness);
      dirty |= r.changed();
      ui.end_row();
    }

    ui.label("Operation");
    let op = self.op;
    self.combo(ui, "op", &OPS, false);
    self.solid.ext.op_chosen |= self.op != op;
    ui.end_row();
    self.comp_name_row(ui);
    if dirty {
      self.dirty = true;
    }
  }

  /// A body face's extrude, as Fusion's: Operation stays as it was while the distance is 0.00,
  /// then joins the body when the distance goes out of it and cuts when it goes in, until an
  /// operation is chosen in the dialog.
  pub(super) fn face_extrude_op(&mut self, s: &Session) {
    if self.kind != super::Kind::Extrude || self.editing.is_some() || !self.extrudes_faces() || self.solid.ext.op_chosen
    {
      return;
    }
    let (d, flip) = self.extrude_distance();
    let Ok(v) = fox_engine::expr::eval(&d, fox_engine::expr::Kind::Length, &s.model.params) else { return };
    if v.abs() < 1e-9 || !matches!(self.op, "new_body" | "join" | "cut") {
      return;
    }
    self.op = if (v > 0.0) != flip { "join" } else { "cut" };
  }

  /// Taper, thin wall and start for the extrude command; None while Start: Object has no face.
  pub(super) fn with_extrude_rows(&self, mut p: Value) -> Option<Value> {
    let ext = &self.solid.ext;
    if !ext.taper.trim().is_empty() {
      p["taper"] = json!(ext.taper.trim());
    }
    if self.extent == "two_sides" && !ext.taper2.trim().is_empty() {
      p["taper2"] = json!(ext.taper2.trim());
    }
    if ext.thin {
      p["thin"] = json!(if ext.thickness.trim().is_empty() { "1 mm" } else { ext.thickness.trim() });
      p["thin_side"] = json!(if ext.thin_side.is_empty() { "inside" } else { ext.thin_side });
    }
    match ext.start {
      "offset" if !ext.start_offset.trim().is_empty() => p["start"] = json!(ext.start_offset.trim()),
      "object" => {
        if ext.start_face.is_null() {
          return None;
        }
        p["start_face"] = ext.start_face.clone();
      }
      _ => {}
    }
    Some(p)
  }

  /// The edited extrude's extent fields beyond kind, distances and flip.
  pub(super) fn edit_extrude_rows(&self, mut extent: Value) -> Value {
    let ext = &self.solid.ext;
    extent["taper"] = json!(ext.taper.trim());
    extent["taper2"] = json!(if self.extent == "two_sides" { ext.taper2.trim() } else { "" });
    extent["thin"] = json!(if ext.thin { ext.thickness.trim() } else { "" });
    extent["thin_side"] = json!(if ext.thin_side.is_empty() { "inside" } else { ext.thin_side });
    extent["start"] = json!(if ext.start == "offset" { ext.start_offset.trim() } else { "" });
    if ext.start == "object" && !ext.start_face.is_null() {
      extent["start_face"] = ext.start_face.clone();
    }
    extent
  }

  /// Loads an edited extrude's extent.
  pub(super) fn load_extrude_rows(&mut self, e: &Extent) {
    let ext = &mut self.solid.ext;
    ext.taper = e.taper.clone();
    ext.taper2 = e.taper2.clone();
    ext.thin = !e.thin.trim().is_empty();
    ext.thickness = e.thin.clone();
    ext.thin_side = match e.thin_side {
      ThinSide::Inside => "inside",
      ThinSide::Outside => "outside",
      ThinSide::Center => "center",
    };
    ext.start_offset = e.start.clone();
    ext.start = if e.start_face.is_some() {
      "object"
    } else if e.start.trim().is_empty() {
      ""
    } else {
      "offset"
    };
    ext.start_face = e.start_face.as_ref().and_then(|f| serde_json::to_value(f).ok()).unwrap_or(Value::Null);
  }

  /// Takes the selected planar faces of bodies as the profiles while no sketch profile is picked,
  /// as Fusion's Extrude does. Once To object or a start object is chosen, later face picks are
  /// for those rows: the profile faces stay (less any deselected).
  pub(super) fn sync_extrude_faces(&mut self, s: &Session) {
    let planar: Vec<Value> = s
      .selection
      .iter()
      .filter_map(|x| match x {
        Sel::Face { body, face } if super::is_planar(s, *body, *face) => Some(json!({"body": body.0, "face": face})),
        _ => None,
      })
      .collect();
    let objects = self.extent == "to_object" || self.solid.ext.start == "object";
    let faces = if !self.profiles.is_empty() {
      Vec::new()
    } else if objects && !self.faces.is_empty() {
      self.faces.iter().filter(|f| planar.contains(f)).cloned().collect()
    } else if objects {
      planar.into_iter().take(1).collect()
    } else {
      planar
    };
    if faces != self.faces {
      // Fusion starts a body face's extrude at 0.00 (no preview, OK greyed until a distance is
      // typed or the arrow dragged), keeping the operation as it was
      if self.faces.is_empty() && self.editing.is_none() {
        self.distance = self.unit.fixed(0.0, 2);
      }
      self.faces = faces;
      self.dirty = true;
    }
  }

  /// "N Profiles | Area : …" for the status bar while an extrude has profiles picked, in the
  /// design's unit as Fusion shows it ("1 Profile | Area : 1.24 in^2"); Thread's face radius; Chamfer's "Chain".
  pub fn profile_status(&self, s: &Session) -> Option<String> {
    if let Some(t) = self.thread_status(s) {
      return Some(t);
    }
    if let Some(t) = self.chamfer_status() {
      return Some(t.into());
    }
    if !matches!(self.kind, super::Kind::Extrude | super::Kind::Revolve) || self.profiles.is_empty() {
      return None;
    }
    let r = s.model.sketches.get(&self.sketch?)?;
    let mut area = 0.0;
    for p in &self.profiles {
      let prof = r.profiles.iter().find(|x| x.contains(*p))?;
      area += prof.area().abs();
    }
    let n = self.profiles.len();
    let area = s.design.units.area(area, 2);
    Some(format!("{n} Profile{} | Area : {area}", if n == 1 { "" } else { "s" }))
  }
}

/// An Extrude dropdown: every value with its icon before the text, as in Fusion.
fn combo(ui: &mut egui::Ui, id: &str, items: &[(&'static str, &'static str)], cur: &mut &'static str) -> bool {
  choice::combo(ui, id, items, cur, choice::extrude_icon)
}

/// Extrude's distance as the engine takes it: the magnitude and whether the sign flips the
/// direction. A value typed with a leading minus ("-10 mm", "-d1", "-(a + b)") or one that
/// works out negative extrudes the other way, as in Fusion, where there is no Flip row.
pub(super) fn split_sign(expr: &str) -> (String, bool) {
  let t = expr.trim();
  if let Some(rest) = t.strip_prefix('-') {
    let rest = rest.trim_start();
    if wrapped(rest) {
      return (rest[1..rest.len() - 1].trim().to_string(), true);
    }
    if !rest.contains(['+', '-']) {
      return (rest.to_string(), true);
    }
  }
  // a sum that comes out negative; one with parameters is the engine's to evaluate
  match fox_engine::expr::eval(t, fox_engine::expr::Kind::Length, &fox_engine::expr::Empty) {
    Ok(v) if v < 0.0 => (format!("-({t})"), true),
    _ => (t.to_string(), false),
  }
}

/// Is `expr` one parenthesised term, "(…)"?
fn wrapped(expr: &str) -> bool {
  if !expr.starts_with('(') || !expr.ends_with(')') {
    return false;
  }
  let mut depth = 0usize;
  for (i, c) in expr.char_indices() {
    match c {
      '(' => depth += 1,
      ')' => {
        depth = depth.saturating_sub(1);
        if depth == 0 && i + 1 < expr.len() {
          return false;
        }
      }
      _ => {}
    }
  }
  true
}

/// `expr` the other way round, as a value box shows a flipped extrude's distance.
pub(super) fn negate(expr: &str) -> String {
  let t = expr.trim();
  if let Some(rest) = t.strip_prefix('-') {
    return rest.trim_start().to_string();
  }
  if t.contains(['+', '-']) { format!("-({t})") } else { format!("-{t}") }
}

fn pick(
  ui: &mut egui::Ui,
  label: &str,
  id: &str,
  items: &[(&'static str, &'static str)],
  cur: &mut &'static str,
) -> bool {
  ui.label(label);
  let changed = combo(ui, id, items, cur);
  ui.end_row();
  changed
}

#[cfg(test)]
mod tests {
  use fox_engine::model::FeatureKind;
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use super::extent_of;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn apply(s: &mut Session, d: &mut Dialog) {
    assert!(d.ready(), "not ready");
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?} {:?}", d.error, d.command());
  }

  fn zs(s: &Session, i: usize) -> (f64, f64) {
    let m = s.model.bodies[i].shape.mass().unwrap();
    (m.bbox_min.z, m.bbox_max.z)
  }

  /// A 10 × 10 square on the ground and an extrude dialog on it.
  fn square(s: &mut Session) -> Dialog {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    run(s, "sketch.finish", json!({}));
    d
  }

  #[test]
  fn direction_and_extent_type_make_the_extent() {
    for (dir, kind, e) in [
      ("one_side", "distance", "one_side"),
      ("symmetric", "distance", "symmetric"),
      ("two_sides", "distance", "two_sides"),
      ("one_side", "all", "through_all"),
      ("symmetric", "all", "through_all_both"),
      ("two_sides", "to_object", "to_object"),
    ] {
      assert_eq!(extent_of(dir, kind), e);
      let mut d = Dialog::blank(Kind::Extrude);
      d.extent = e;
      assert_eq!(d.extent_type(), kind);
      if kind != "to_object" && !(kind == "all" && dir == "two_sides") {
        assert_eq!(d.direction(), dir);
      }
    }
  }

  #[test]
  fn thin_tapered_offset_start_and_area_readout() {
    let mut s = Session::default();
    let mut d = square(&mut s);
    assert_eq!(d.profile_status(&s).as_deref(), Some("1 Profile | Area : 100.00 mm^2"));
    // in the design's unit: 100 mm² is 0.155 in²
    s.design.units = fox_engine::model::LengthUnit::In;
    assert_eq!(d.profile_status(&s).as_deref(), Some("1 Profile | Area : 0.16 in^2"));
    s.design.units = fox_engine::model::LengthUnit::Mm;
    d.solid.ext.thin = true;
    d.solid.ext.thickness = "1 mm".into();
    d.solid.ext.start = "offset";
    d.solid.ext.start_offset = "5 mm".into();
    apply(&mut s, &mut d);
    assert!(((s.model.bodies[0].shape.mass().unwrap().volume) - 360.0).abs() < 1e-6);
    let (lo, hi) = zs(&s, 0);
    assert!((lo - 5.0).abs() < 1e-6 && (hi - 15.0).abs() < 1e-6, "{lo} {hi}");
    // the preview's body stays out of the browser until OK
    assert_eq!(d.preview_bodies(&s), vec![s.model.bodies[0].id]);

    // taper narrows it; editing keeps the taper and thin wall
    d.withdraw(&mut s);
    d.solid.ext.thin = false;
    d.solid.ext.taper = "10 deg".into();
    apply(&mut s, &mut d);
    let v = s.model.bodies[0].shape.mass().unwrap().volume;
    assert!(v < 1000.0 - 1.0, "{v}");
    let id = s.design.features.last().unwrap().id;
    let FeatureKind::Extrude { extent, .. } = s.design.feature(id).unwrap().kind.clone() else { panic!() };
    let mut e = Dialog::blank(Kind::Extrude);
    e.editing = Some(id);
    e.distance = extent.distance.clone();
    e.load_extrude_rows(&extent);
    assert_eq!(
      (e.solid.ext.taper.as_str(), e.solid.ext.start, e.solid.ext.start_offset.as_str()),
      ("10 deg", "offset", "5 mm")
    );
    e.distance = "20 mm".into();
    apply(&mut s, &mut e);
    assert!(e.preview_bodies(&s).is_empty());
    let FeatureKind::Extrude { extent, .. } = &s.design.feature(id).unwrap().kind else { panic!() };
    assert_eq!((extent.taper.as_str(), extent.start.as_str(), extent.distance.as_str()), ("10 deg", "5 mm", "20 mm"));
  }

  #[test]
  fn start_at_an_object_and_all_both_ways() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10], "operation": "new_body"}));
    let mut d = square(&mut s);
    d.op = "new_body";
    d.solid.ext.start = "object";
    assert!(d.command().is_none(), "no start face yet");
    let b = &s.model.bodies[0];
    let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    s.selection = vec![Sel::Face { body: b.id, face: top }];
    d.sync_object(&s);
    apply(&mut s, &mut d);
    let (lo, hi) = zs(&s, 1);
    assert!((lo - 20.0).abs() < 1e-6 && (hi - 30.0).abs() < 1e-6, "{lo} {hi}");

    // a cut through everything both ways from a sketch inside the block
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
    let mid = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": mid}}));
    run(&mut s, "sketch.rect", json!({"corner1": [2, 2], "corner2": [8, 8]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    d.op = "cut";
    d.extent = extent_of("symmetric", "all");
    apply(&mut s, &mut d);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - (8000.0 - 36.0 * 20.0)).abs() < 1e-6);
  }

  #[test]
  fn a_negative_distance_flips_the_extrude() {
    use super::{negate, split_sign};
    assert_eq!(split_sign("10 mm"), ("10 mm".into(), false));
    assert_eq!(split_sign(" -10 mm "), ("10 mm".into(), true));
    assert_eq!(split_sign("-d1 * 2"), ("d1 * 2".into(), true));
    assert_eq!(split_sign("-(d1 + 2 mm)"), ("d1 + 2 mm".into(), true));
    assert_eq!(split_sign("-(a) * (b)"), ("(a) * (b)".into(), true));
    assert_eq!(split_sign("-(a) + (b)"), ("-(a) + (b)".into(), false));
    // a sum that works out negative; one with parameters is left to the engine
    assert_eq!(split_sign("2 mm - 12 mm"), ("-(2 mm - 12 mm)".into(), true));
    assert_eq!(split_sign("-2 mm + 12 mm"), ("-2 mm + 12 mm".into(), false));
    assert_eq!(split_sign("d1 - 12 mm"), ("d1 - 12 mm".into(), false));
    assert_eq!(negate("10 mm"), "-10 mm");
    assert_eq!(negate("-10 mm"), "10 mm");
    assert_eq!(negate("d1 + 2 mm"), "-(d1 + 2 mm)");
    assert_eq!(split_sign(&negate("d1 + 2 mm")), ("d1 + 2 mm".into(), true));
  }
}
