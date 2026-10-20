//! Fusion's canvas manipulators for every feature dialog with a length or an angle: the dialog's
//! drag arrow ([`Dialog::manipulator`]) plus the handles beside it, all as [`Handle`]s the
//! viewport draws and drags (see `crate::manipulators::dialog_handles`). An arrow pushes a
//! length along a direction; a rotator is a ring segment about an axis with a ball on its end
//! (Revolve, Draft, Plane at Angle, Circular Pattern, Extrude's taper); a point slides on a
//! plane (Hole's centre). The handles are worked out from the dialog's fields every frame, so a
//! typed value moves them and a drag writes the field.

use fox_engine::Session;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, FeatureId};
use serde_json::{Value, json};

use super::manip::{deg, mm};
use super::{Dialog, Kind};
use crate::forms::{self, Form};

/// The dialog value a handle drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
  /// The dialog's drag arrow ([`Dialog::manipulator`]).
  Main,
  /// Extrude, two sides: side two's distance.
  Second,
  /// Extrude's taper angle.
  Taper,
  /// Revolve, Draft, Plane at Angle, Circular Pattern: the angle.
  Angle,
  /// Revolve, two sides: side two's angle.
  Angle2,
  /// Rib / Web depth.
  Depth,
  /// Hole: the single hole's centre.
  Centre,
}

/// Where a handle sits and how it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
  /// From `origin` along the unit `dir`; the value is the length along it (mm).
  Arrow { origin: DVec3, dir: DVec3 },
  /// A ring segment about the unit `axis` through `centre`, starting from the unit `from` (across
  /// the axis) and turning by the value (radians, right-handed about the axis). `radius` is in
  /// mm; zero draws it at a fixed size on screen.
  Rotator { centre: DVec3, axis: DVec3, from: DVec3, radius: f64 },
  /// A point dragged across the plane through it with this unit normal.
  Point { at: DVec3, normal: DVec3 },
}

/// A canvas manipulator of the open dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
  pub slot: Slot,
  pub shape: Shape,
  /// Arrow: length along it (mm); rotator: angle (radians); point: unused.
  pub value: f64,
  /// Arrow lengths per unit of the field's value: 1 for a length, more for Scale's factor.
  pub per: f64,
  /// The value as its heads-up box shows it.
  pub text: String,
}

impl Handle {
  fn arrow(slot: Slot, origin: DVec3, dir: DVec3, value: f64, unit: fox_engine::model::LengthUnit) -> Handle {
    Handle { slot, shape: Shape::Arrow { origin, dir }, value, per: 1.0, text: mm(unit, value) }
  }

  fn rotator(slot: Slot, centre: DVec3, axis: DVec3, from: DVec3, radius: f64, value: f64) -> Option<Handle> {
    let axis = axis.try_normalize()?;
    let from = (from - axis * from.dot(axis)).try_normalize()?;
    Some(Handle { slot, shape: Shape::Rotator { centre, axis, from, radius }, value, per: 1.0, text: deg(value) })
  }

  /// A rotator's point at angle `a` on a ring of radius `r` (its own radius when that is set).
  pub fn on_ring(&self, a: f64, r: f64) -> Option<DVec3> {
    let Shape::Rotator { centre, axis, from, radius } = self.shape else { return None };
    let r = if radius > 0.0 { radius } else { r };
    Some(centre + (from * a.cos() + axis.cross(from) * a.sin()) * r)
  }

  /// Is the value an angle (its box takes angles)?
  pub fn is_angle(&self) -> bool {
    matches!(self.shape, Shape::Rotator { .. })
  }
}

/// The closest point to `p` on the line through `o` along the unit `d`.
fn foot(o: DVec3, d: DVec3, p: DVec3) -> DVec3 {
  o + d * (p - o).dot(d)
}

/// A point and unit direction of an axis value: "x" / "y" / "z", `{"line": curve}` (of
/// `sketch`), `{"axis": feature}` or `{"body", "edge"}` (a straight edge).
pub(super) fn axis_line(s: &Session, v: &Value, sketch: Option<FeatureId>) -> Option<(DVec3, DVec3)> {
  let get = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
  if let Some(n) = v.as_str() {
    let d = match n {
      "x" => DVec3::X,
      "y" => DVec3::Y,
      "z" => DVec3::Z,
      _ => return None,
    };
    return Some((DVec3::ZERO, d));
  }
  if let Some(c) = get("line") {
    let sk = sketch?;
    let r = s.model.sketches.get(&sk)?;
    let curve = r.sketch.curves.iter().find(|x| x.id.0 == c)?;
    let fox_engine::sketch::CurveKind::Line { a, b } = &curve.kind else { return None };
    let (a, b) = (r.plane.to_world(r.sketch.pos(*a)?), r.plane.to_world(r.sketch.pos(*b)?));
    return Some((a, (b - a).try_normalize()?));
  }
  if let Some(f) = get("axis") {
    let (p, d) = *s.model.axes.get(&FeatureId(f))?;
    return Some((p, d.try_normalize()?));
  }
  let body = s.model.body(BodyId(get("body")?))?;
  let e = body.edges().iter().find(|e| Some(e.id) == get("edge"))?;
  Some(((e.start + e.end) / 2.0, (e.end - e.start).try_normalize()?))
}

impl Dialog {
  /// Is this the dialog of form `f`?
  pub(super) fn is_form(&self, f: &'static Form) -> bool {
    matches!(self.kind, Kind::Form(g) if std::ptr::eq(g, f))
  }

  /// A form's value by input key.
  pub(super) fn fval(&self, key: &str) -> Option<&Value> {
    let Kind::Form(f) = self.kind else { return None };
    let i = f.inputs.iter().position(|x| x.key() == key)?;
    self.form.values.get(i)
  }

  /// A form's text value by input key ("" when unset).
  pub(super) fn fstr(&self, key: &str) -> &str {
    self.fval(key).and_then(Value::as_str).unwrap_or("")
  }

  pub(super) fn fflag(&self, key: &str) -> bool {
    self.fval(key).and_then(Value::as_bool).unwrap_or(false)
  }

  pub(super) fn set_fval(&mut self, key: &str, v: Value) {
    let Kind::Form(f) = self.kind else { return };
    if let Some(i) = f.inputs.iter().position(|x| x.key() == key)
      && let Some(slot) = self.form.values.get_mut(i)
    {
      *slot = v;
    }
  }

  /// Every canvas manipulator the dialog shows: its drag arrow first, then the others.
  pub fn handles(&self, s: &Session) -> Vec<Handle> {
    let mut out = Vec::new();
    if let Some(m) = self.manipulator(s) {
      let mut h = Handle::arrow(Slot::Main, m.origin, m.dir, m.value, self.unit);
      if self.is_form(&forms::SCALE) {
        h.per = self.manip_scale_ref();
        h.text = fox_engine::sketch::Dim::new((m.value / h.per * 1e4).round() / 1e4).expr;
      }
      out.push(h);
    }
    out.extend(self.extra_handles(s));
    out
  }

  /// The handles beside the drag arrow.
  fn extra_handles(&self, s: &Session) -> Vec<Handle> {
    let mut out = Vec::new();
    match self.kind {
      Kind::Extrude => {
        let Some(m) = self.manipulator(s) else { return out };
        if self.extent == "two_sides"
          && let Some(d2) = self.len(s, &self.distance2)
        {
          // side two runs the other way from the profile
          let dir = if self.flip { m.dir } else { -m.dir };
          out.push(Handle::arrow(Slot::Second, m.origin, dir, d2, self.unit));
        }
        // the taper turns the side wall at the distance arrow's end
        let x = s.model.sketches.get(&self.sketch.unwrap_or(FeatureId(0))).map(|r| r.plane.x);
        let side = if m.value < 0.0 { -m.dir } else { m.dir };
        if let (Some(x), Some(t)) = (x, self.ang(s, &self.solid.ext.taper)) {
          out.extend(Handle::rotator(Slot::Taper, m.origin + m.dir * m.value, x, side, 0.0, t));
        }
      }
      Kind::Revolve => out.extend(self.revolve_handles(s)),
      Kind::Draft => out.extend(self.draft_handle(s)),
      Kind::CircPattern => out.extend(self.circ_pattern_handle(s)),
      Kind::Hole => out.extend(self.hole_centre_handle(s)),
      Kind::Form(f) if std::ptr::eq(f, &forms::PLANE_ANGLE) => out.extend(self.plane_angle_handle(s)),
      Kind::Form(f) if std::ptr::eq(f, &forms::RIB) || std::ptr::eq(f, &forms::WEB) => {
        if let Some((mid, _, grow)) = self.rib_frame(s)
          && let Some(d) = Some(self.fstr("depth")).filter(|d| !d.trim().is_empty()).and_then(|d| self.len(s, d))
        {
          out.push(Handle::arrow(Slot::Depth, mid, grow, d, self.unit));
        }
      }
      _ => {}
    }
    out
  }

  /// Takes a dragged handle's value (mm along an arrow, radians round a rotator) into its field,
  /// rounded to `step` (lengths) or `turn` (angles), and re-previews.
  pub fn set_handle(&mut self, _s: &Session, slot: Slot, v: f64, step: f64, turn: f64) {
    let a = crate::manipulators::snap(v, turn);
    let len = crate::manipulators::snap(v, step);
    let min = step.max(1e-3);
    match slot {
      Slot::Main => {
        self.set_manipulator(v, step);
        return;
      }
      Slot::Second => self.distance2 = mm(self.unit, len.max(min)),
      Slot::Taper => self.solid.ext.taper = deg(a.clamp(-1.5, 1.5)),
      Slot::Angle | Slot::Angle2 => match self.kind {
        Kind::Revolve => self.set_revolve_angle(slot, a),
        // a draft stays under a right angle either way
        Kind::Draft => self.angle = deg(a.clamp(-1.5, 1.5)),
        Kind::CircPattern => {
          let half = if self.symmetric[0] { 2.0 } else { 1.0 };
          let full = std::f64::consts::TAU;
          self.angle = deg((a * half).clamp(turn.max(1e-3), full));
        }
        Kind::Form(f) if std::ptr::eq(f, &forms::PLANE_ANGLE) => self.set_fval("angle", json!(deg(a))),
        _ => return,
      },
      Slot::Depth => {
        let t = mm(self.unit, len.max(min));
        self.set_fval("depth", json!(t));
      }
      Slot::Centre => return,
    }
    self.dirty = true;
  }

  /// A point handle dragged to `p` (Hole's centre), distances rounded to `step`.
  pub fn move_handle(&mut self, s: &Session, slot: Slot, p: DVec3, step: f64) {
    if slot == Slot::Centre && self.kind == Kind::Hole && self.move_hole_centre(s, p, step) {
      self.dirty = true;
    }
  }

  // ---- revolve ----

  /// The profile point the revolve turns and the axis it turns about: its centre on the axis, the
  /// unit axis and the radius out to the point.
  fn revolve_frame(&self, s: &Session) -> Option<(DVec3, DVec3, DVec3)> {
    let r = s.model.sketches.get(&self.sketch?)?;
    let at = match self.profiles.first() {
      Some(p) => *p,
      None => r.profiles.first()?.interior_point(),
    };
    let p = r.plane.to_world(at);
    let (o, d) = axis_line(s, &self.revolve_axis()?, self.sketch)?;
    Some((foot(o, d, p), d, p))
  }

  fn revolve_handles(&self, s: &Session) -> Vec<Handle> {
    let Some((c, axis, p)) = self.revolve_frame(s) else { return Vec::new() };
    let radius = (p - c).length();
    let full = std::f64::consts::TAU;
    let a = if self.solid.rev_full { Some(full) } else { self.ang(s, &self.angle) };
    let Some(a) = a else { return Vec::new() };
    let mut out = Vec::new();
    match self.extent {
      // symmetric: the angle is split either side of the profile
      "symmetric" => out.extend(Handle::rotator(Slot::Angle, c, axis, p - c, radius, a / 2.0)),
      "two_sides" => {
        out.extend(Handle::rotator(Slot::Angle, c, axis, p - c, radius, a));
        if let Some(a2) = self.ang(s, &self.angle2) {
          out.extend(Handle::rotator(Slot::Angle2, c, -axis, p - c, radius, a2));
        }
      }
      _ => out.extend(Handle::rotator(Slot::Angle, c, axis, p - c, radius, a)),
    }
    out
  }

  fn set_revolve_angle(&mut self, slot: Slot, a: f64) {
    let full = std::f64::consts::TAU;
    if slot == Slot::Angle2 {
      self.angle2 = deg(a.clamp(-full, full));
      return;
    }
    let a = if self.extent == "symmetric" { a * 2.0 } else { a };
    // a revolve turns by something, at most a whole turn
    let a = a.clamp(-full, full);
    let a = if a.abs() < 1e-6 { 1f64.to_radians() } else { a };
    self.solid.rev_full = false;
    self.angle = deg(a);
  }

  // ---- draft ----

  /// Draft's angle as a rotator on the first face: about the hinge across the pull direction,
  /// from the pull direction (a positive draft leans the face in, away from its outside).
  fn draft_handle(&self, s: &Session) -> Option<Handle> {
    let (c, n) = self.cached_anchor()?;
    let pull = super::plane_of(s, &self.plane)?.normal();
    let pull = if self.flip { -pull } else { pull };
    let hinge = n.cross(pull);
    Handle::rotator(Slot::Angle, c, hinge, pull, 0.0, self.ang(s, &self.angle)?)
  }

  // ---- circular pattern ----

  fn circ_pattern_handle(&self, s: &Session) -> Option<Handle> {
    let (o, d) = axis_line(s, &self.revolve_axis()?, None)?;
    let p = self.circ_centre()?;
    let c = foot(o, d, p);
    let a = self.ang(s, &self.angle)?;
    let a = if self.symmetric[0] { a / 2.0 } else { a };
    Handle::rotator(Slot::Angle, c, d, p - c, (p - c).length(), a)
  }

  // ---- plane at angle ----

  /// The rotator of Plane at Angle: about the line, from the reference plane's direction across
  /// it, as the engine turns the plane (`PlaneDef::Angle`).
  fn plane_angle_handle(&self, s: &Session) -> Option<Handle> {
    let (o, d) = axis_line(s, self.fval("axis")?, None)?;
    let n0 = super::plane_of(s, self.fval("plane")?)?.normal();
    let m = (n0 - d * n0.dot(d)).try_normalize().unwrap_or_else(|| d.any_orthonormal_vector());
    let a = self.ang(s, self.fstr("angle"))?;
    Handle::rotator(Slot::Angle, o, d, m.cross(d), 0.0, a)
  }

  // ---- rib / web ----

  /// The first line a rib or web grows from: its middle, the unit thickness direction (the
  /// thickness is centred on the line) and the unit direction it grows in (as the engine's
  /// `FeatureKind::Rib`).
  pub(super) fn rib_frame(&self, s: &Session) -> Option<(DVec3, DVec3, DVec3)> {
    let sk = FeatureId(self.fval("sketch")?.as_u64()? as u32);
    let r = s.model.sketches.get(&sk)?;
    let picked: Vec<u32> = self
      .fval("curves")
      .and_then(Value::as_array)
      .map(|a| a.iter().filter_map(|c| c.as_u64()).map(|c| c as u32).collect())
      .unwrap_or_default();
    let (a, b) = r.sketch.curves.iter().find_map(|c| {
      if !(picked.is_empty() && !c.construction || picked.contains(&c.id.0)) {
        return None;
      }
      let fox_engine::sketch::CurveKind::Line { a, b } = &c.kind else { return None };
      Some((r.plane.to_world(r.sketch.pos(*a)?), r.plane.to_world(r.sketch.pos(*b)?)))
    })?;
    let n = r.plane.normal();
    let u = (b - a).try_normalize()?;
    let across = n.cross(u);
    let web = self.is_form(&forms::WEB);
    let (thick, grow) = if web { (across, n) } else { (n, across) };
    let grow = if self.fflag("flip") { -grow } else { grow };
    Some(((a + b) / 2.0, thick, grow))
  }

  /// `v` turned by `a` about the unit `axis`.
  #[cfg(test)]
  pub(super) fn turned(axis: DVec3, a: f64, v: DVec3) -> DVec3 {
    fox_engine::kernel::glam::DQuat::from_axis_angle(axis, a) * v
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::forms::{EMBOSS, OFFSET_FACE, PLANE_ANGLE, RIB, SCALE, THICKEN, WEB};

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
  }

  fn handle(d: &Dialog, s: &Session, slot: Slot) -> Handle {
    d.handles(s).into_iter().find(|h| h.slot == slot).unwrap_or_else(|| panic!("{:?}: no {slot:?} handle", d.kind))
  }

  fn arrow(h: &Handle) -> (DVec3, DVec3) {
    match h.shape {
      Shape::Arrow { origin, dir } => (origin, dir),
      _ => panic!("{h:?} is not an arrow"),
    }
  }

  fn rotator(h: &Handle) -> (DVec3, DVec3, DVec3) {
    match h.shape {
      Shape::Rotator { centre, axis, from, .. } => (centre, axis, from),
      _ => panic!("{h:?} is not a rotator"),
    }
  }

  /// A 10 × 20 × 30 box from the origin and the id of its top face.
  fn boxed(s: &mut Session) -> (u32, u32) {
    run(s, "box", json!({"width": 10, "depth": 20, "height": 30, "at": [5, 10]}));
    let b = &s.model.bodies[0];
    (b.id.0, b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id)
  }

  fn form(f: &'static Form) -> Dialog {
    let mut d = Dialog::blank(Kind::Form(f));
    d.form = forms::State::new(f);
    d
  }

  #[test]
  fn extrude_has_a_second_arrow_for_two_sides_and_a_taper_rotator() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = Some(sk);
    d.distance = "10 mm".into();
    d.distance2 = "4 mm".into();
    d.extent = "one_side";
    assert!(d.handles(&s).iter().all(|h| h.slot != Slot::Second), "one side: one arrow");
    d.extent = "two_sides";
    let main = handle(&d, &s, Slot::Main);
    let (o, dir) = arrow(&main);
    assert!(close(dir, DVec3::Z) && (main.value - 10.0).abs() < 1e-9);
    let second = handle(&d, &s, Slot::Second);
    let (o2, dir2) = arrow(&second);
    assert!(close(o2, o) && close(dir2, -DVec3::Z) && (second.value - 4.0).abs() < 1e-9, "{second:?}");
    d.set_handle(&s, Slot::Second, 6.26, 0.5, 0.0);
    assert_eq!(d.distance2, "6.5 mm");
    assert!(d.dirty);
    // the taper turns about the sketch's X at the arrow's end, from the extrude direction
    let t = handle(&d, &s, Slot::Taper);
    let (c, axis, from) = rotator(&t);
    assert!(close(c, o + DVec3::Z * 10.0) && close(axis, DVec3::X) && close(from, DVec3::Z), "{t:?}");
    assert_eq!(t.value, 0.0);
    d.set_handle(&s, Slot::Taper, 0.1, 0.0, 1f64.to_radians());
    assert_eq!(d.solid.ext.taper, "6 deg");
    assert!((handle(&d, &s, Slot::Taper).value - 6f64.to_radians()).abs() < 1e-9, "typed taper moves it");
    // the preview takes the dragged values
    d.preview(&mut s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  }

  #[test]
  fn revolve_rotator_turns_the_way_the_revolve_goes() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [10, 0], "corner2": [14, 5]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Revolve);
    d.sketch = Some(sk);
    d.axis = "z".into();
    d.angle = "90 deg".into();
    d.solid.rev_full = false;
    let h = handle(&d, &s, Slot::Angle);
    let (c, axis, from) = rotator(&h);
    assert!(close(axis, DVec3::Z), "{h:?}");
    assert!(c.x.abs() < 1e-9 && c.y.abs() < 1e-9, "the centre is on the axis: {c}");
    assert!((h.value - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    // the ball sits where the revolved profile ends
    let end = h.on_ring(h.value, 0.0).unwrap();
    d.preview(&mut s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let (lo, hi) = s.model.bounds().unwrap();
    assert!(
      end.x >= lo.x - 1e-6 && end.x <= hi.x + 1e-6 && end.y >= lo.y - 1e-6 && end.y <= hi.y + 1e-6,
      "{end} outside {lo} {hi} (from {from})"
    );
    d.set_handle(&s, Slot::Angle, 2.0, 0.0, 5f64.to_radians());
    assert_eq!(d.angle, "115 deg");
    // symmetric splits it either side; two sides adds side two's rotator the other way round
    d.extent = "symmetric";
    assert!((handle(&d, &s, Slot::Angle).value - 115f64.to_radians() / 2.0).abs() < 1e-9);
    d.set_handle(&s, Slot::Angle, 0.5, 0.0, 0.0);
    assert_eq!(d.angle, deg(1.0));
    d.extent = "two_sides";
    d.angle2 = "30 deg".into();
    let (_, axis2, _) = rotator(&handle(&d, &s, Slot::Angle2));
    assert!(close(axis2, -DVec3::Z));
    d.set_handle(&s, Slot::Angle2, 0.25, 0.0, 0.0);
    assert_eq!(d.angle2, deg(0.25));
  }

  #[test]
  fn shell_thicken_offset_face_emboss_and_hole_arrows() {
    let mut s = Session::default();
    let (b, top) = boxed(&mut s);
    let top_face = json!({"body": b, "face": top});
    let top_c = DVec3::new(5.0, 10.0, 30.0);

    // shell: into the body from the opened face
    let mut d = Dialog::blank(Kind::Shell);
    d.faces = vec![top_face.clone()];
    d.radius = "2 mm".into();
    d.refresh_manip(&s);
    let m = d.manipulator(&s).expect("a shell arrow");
    assert!(close(m.origin, top_c) && close(m.dir, -DVec3::Z) && (m.value - 2.0).abs() < 1e-9, "{m:?}");
    d.set_manipulator(3.2, 0.5);
    assert_eq!(d.radius, "3 mm");
    d.solid.shell_dir = "outside";
    assert!(close(d.manipulator(&s).unwrap().dir, DVec3::Z));

    // offset face: along the face's normal
    let mut d = form(&OFFSET_FACE);
    d.set_fval("faces", json!([top_face]));
    d.refresh_manip(&s);
    let m = d.manipulator(&s).expect("an offset face arrow");
    assert!(close(m.origin, top_c) && close(m.dir, DVec3::Z) && (m.value - 1.0).abs() < 1e-9);
    d.set_manipulator(-2.0, 0.5);
    assert_eq!(d.fstr("distance"), "-2 mm");

    // emboss: out of the face, into it when debossed
    let mut d = form(&EMBOSS);
    d.set_fval("face", top_face.clone());
    d.refresh_manip(&s);
    let m = d.manipulator(&s).expect("an emboss arrow");
    assert!(close(m.origin, top_c) && close(m.dir, DVec3::Z) && (m.value - 1.0).abs() < 1e-9);
    d.set_fval("deboss", json!(true));
    assert!(close(d.manipulator(&s).unwrap().dir, -DVec3::Z));
    d.set_manipulator(2.5, 0.5);
    assert_eq!(d.fstr("depth"), "2.5 mm");

    // hole: depth into the face from the hole's centre, which drags across the face
    let mut d = Dialog::blank(Kind::Hole);
    d.solid.hp.single = true;
    d.faces = vec![top_face.clone()];
    d.b = "10 mm".into();
    d.refresh_manip(&s);
    let m = d.manipulator(&s).expect("a hole depth arrow");
    assert!(close(m.origin, top_c) && close(m.dir, -DVec3::Z) && (m.value - 10.0).abs() < 1e-9, "{m:?}");
    d.set_manipulator(12.3, 0.5);
    assert_eq!(d.b, "12.5 mm");
    let c = handle(&d, &s, Slot::Centre);
    assert_eq!(c.shape, Shape::Point { at: top_c, normal: DVec3::Z });
    d.move_handle(&s, Slot::Centre, DVec3::new(2.0, 3.0, 30.0), 0.5);
    assert!(d.dirty);
    let m = d.manipulator(&s).unwrap();
    assert!(close(m.origin, DVec3::new(2.0, 3.0, 30.0)), "the arrow follows the centre: {m:?}");
    assert_eq!(d.hole_params().unwrap()["at"], json!([2.0, 3.0, 30.0]));
    // through all: no depth to drag
    d.flag = true;
    assert!(d.manipulator(&s).is_none());

    // thicken: off a surface body's face, the other way when flipped
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "surface.patch", json!({"sketch": sk.0}));
    let surf = s.model.bodies.last().unwrap();
    let (sid, n) = (surf.id.0, surf.faces()[0].normal);
    let mut d = form(&THICKEN);
    d.set_fval("bodies", json!([sid]));
    d.refresh_manip(&s);
    let m = d.manipulator(&s).expect("a thicken arrow");
    assert!(close(m.origin, DVec3::new(5.0, 5.0, 0.0)) && close(m.dir, n) && (m.value - 1.0).abs() < 1e-9, "{m:?}");
    d.set_fval("flip", json!(true));
    assert!(close(d.manipulator(&s).unwrap().dir, -n));
    d.set_manipulator(3.0, 0.5);
    assert_eq!(d.fstr("thickness"), "3 mm");
  }

  #[test]
  fn rib_and_web_arrows_and_scale_factor() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let n = s.model.sketches[&sk].plane.normal();
    for (f, web) in [(&RIB, false), (&WEB, true)] {
      let mut d = form(f);
      d.set_fval("sketch", json!(sk.0));
      d.set_fval("thickness", json!("4 mm"));
      let m = d.manipulator(&s).expect("a thickness arrow");
      // the thickness is centred on the line: the arrow reaches half of it
      assert!(close(m.origin, DVec3::new(10.0, 0.0, 0.0)) && (m.value - 2.0).abs() < 1e-9, "{m:?}");
      let across = n.cross(DVec3::X);
      assert!(close(m.dir, if web { across } else { n }), "{f:?}: {m:?}");
      d.set_manipulator(3.0, 0.5);
      assert_eq!(d.fstr("thickness"), "6 mm");
      assert!(d.handles(&s).iter().all(|h| h.slot != Slot::Depth), "to next: no depth arrow");
      d.set_fval("depth", json!("8 mm"));
      let h = handle(&d, &s, Slot::Depth);
      let (o, dir) = arrow(&h);
      assert!(close(o, m.origin) && close(dir, if web { n } else { across }) && (h.value - 8.0).abs() < 1e-9);
      d.set_handle(&s, Slot::Depth, 5.2, 0.5, 0.0);
      assert_eq!(d.fstr("depth"), "5 mm");
    }

    // scale: an arrow along X from the point whose length is the factor times the bodies' size
    let mut s = Session::default();
    boxed(&mut s);
    let mut d = form(&SCALE);
    d.refresh_manip(&s);
    let r = d.manip_scale_ref();
    let expect = (DVec3::new(10.0, 20.0, 30.0)).length() / 2.0;
    assert!((r - expect).abs() < 1e-6, "{r}");
    let h = handle(&d, &s, Slot::Main);
    let (o, dir) = arrow(&h);
    assert!(close(o, DVec3::ZERO) && close(dir, DVec3::X) && (h.value - 2.0 * r).abs() < 1e-9, "{h:?}");
    assert_eq!((h.per, h.text.as_str()), (r, "2"));
    d.set_manipulator(r * 1.5, 0.0);
    assert_eq!(d.fstr("factor"), "1.5");
    // the size holds under a showing preview
    d.preview(&mut s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    d.refresh_manip(&s);
    assert!((d.manip_scale_ref() - r).abs() < 1e-6);
  }

  #[test]
  fn rotators_of_draft_circular_pattern_and_plane_at_angle() {
    let mut s = Session::default();
    let (b, _) = boxed(&mut s);
    // draft the +X side face about the XY pull direction
    let side = s.model.bodies[0].faces().iter().find(|f| f.normal.x > 0.9).unwrap().id;
    let mut d = Dialog::blank(Kind::Draft);
    d.faces = vec![json!({"body": b, "face": side})];
    d.plane = json!("xy");
    d.angle = "3 deg".into();
    d.refresh_manip(&s);
    let h = handle(&d, &s, Slot::Angle);
    let (c, axis, from) = rotator(&h);
    assert!(close(c, DVec3::new(10.0, 10.0, 15.0)) && close(from, DVec3::Z), "{h:?}");
    assert!(close(axis, DVec3::X.cross(DVec3::Z)), "the hinge runs along the face across the pull");
    assert!((h.value - 3f64.to_radians()).abs() < 1e-9);
    d.set_handle(&s, Slot::Angle, 0.2, 0.0, 1f64.to_radians());
    assert_eq!(d.angle, "11 deg");
    // the ball leans the way the drafted face does (in, its top short of the side): the face
    // runs along it
    let lean = handle(&d, &s, Slot::Angle).on_ring(11f64.to_radians(), 1.0).unwrap() - c;
    assert!(lean.x < 0.0, "{lean}");
    d.preview(&mut s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let drafted = s.model.bodies[0].faces().iter().find(|f| f.normal.x > 0.9).unwrap().normal;
    assert!(drafted.dot(lean).abs() < 1e-6, "face normal {drafted}, ball {lean}");
    d.cancel(&mut s);

    // circular pattern about Z: from the axis out to the bodies' centre
    let mut d = Dialog::blank(Kind::CircPattern);
    d.bodies = vec![b];
    d.axis = "z".into();
    d.angle = "180 deg".into();
    d.refresh_manip(&s);
    let h = handle(&d, &s, Slot::Angle);
    let (c, axis, from) = rotator(&h);
    assert!(close(c, DVec3::new(0.0, 0.0, 15.0)) && close(axis, DVec3::Z), "{h:?}");
    assert!(close(from, DVec3::new(5.0, 10.0, 0.0).normalize()));
    assert!((h.value - std::f64::consts::PI).abs() < 1e-9);
    d.set_handle(&s, Slot::Angle, 1.0, 0.0, 0.0);
    assert_eq!(d.angle, deg(1.0));
    d.symmetric[0] = true;
    assert!((handle(&d, &s, Slot::Angle).value - 0.5).abs() < 1e-5, "the degrees it shows are rounded");
    d.set_handle(&s, Slot::Angle, 1.0, 0.0, 0.0);
    assert_eq!(d.angle, deg(2.0));
    // the ball sits where the last instance goes
    d.symmetric[0] = false;
    d.angle = "90 deg".into();
    d.count = "2".into();
    let end = handle(&d, &s, Slot::Angle).on_ring(std::f64::consts::FRAC_PI_2, 0.0).unwrap();
    d.preview(&mut s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let (lo, hi) = s.model.bounds().unwrap();
    assert!(end.x > lo.x - 1e-6 && end.x < 0.0 && hi.x <= 10.0 + 1e-6, "{end} in {lo} {hi}");
    assert!(lo.x < -15.0 && end.y > 0.0, "the instance went to -X: {end} {lo}");
    d.cancel(&mut s);

    // plane at angle about the box's edge along Y, from the XY plane
    let e = s.model.bodies[0]
      .edges()
      .iter()
      .find(|e| (e.end - e.start).y.abs() > 1.0 && e.start.x.abs() < 1e-6 && e.start.z.abs() < 1e-6)
      .unwrap()
      .id;
    let mut d = form(&PLANE_ANGLE);
    d.set_fval("axis", json!({"body": b, "edge": e}));
    d.set_fval("plane", json!("xy"));
    let h = handle(&d, &s, Slot::Angle);
    let (c, axis, from) = rotator(&h);
    assert!(close(c, DVec3::new(0.0, 10.0, 0.0)) && axis.cross(DVec3::Y).length() < 1e-9, "{h:?}");
    assert!(from.z.abs() < 1e-9 && from.dot(axis).abs() < 1e-9, "from lies in XY across the line");
    assert!((h.value - 45f64.to_radians()).abs() < 1e-9);
    // the ball's direction is in the plane the command makes
    let made = run(&mut s, "plane", d.form.params(&PLANE_ANGLE));
    let pl = s.model.planes[&FeatureId(made["feature"].as_u64().unwrap() as u32)];
    let tip = Dialog::turned(axis, h.value, from);
    assert!(tip.dot(pl.normal()).abs() < 1e-9, "{tip} vs {}", pl.normal());
    d.set_handle(&s, Slot::Angle, 0.5, 0.0, 0.0);
    assert_eq!(d.fstr("angle"), deg(0.5));
  }

  #[test]
  fn edited_features_show_their_manipulators() {
    // a fillet: the radius arrow on the rounded edge, from the edge's stored reference
    let mut s = Session::default();
    let (b, _) = boxed(&mut s);
    let e = s.model.bodies[0]
      .edges()
      .iter()
      .find(|e| e.start.z > 29.9 && e.end.z > 29.9 && (e.end - e.start).x.abs() > 1.0 && e.mid.y < 1e-6)
      .unwrap();
    let (mid, eid) = (e.mid, e.id);
    run(&mut s, "fillet", json!({"edges": [{"body": b, "edge": eid}], "radius": "2 mm"}));
    let app = crate::App::with_session(s);
    let id = app.s.design.features.last().unwrap().id;
    let mut d = Dialog::edit(&app, id).unwrap();
    d.refresh_manip(&app.s);
    let m = d.manipulator(&app.s).expect("an edited fillet shows its arrow");
    assert!(close(m.origin, mid) && close(m.dir, DVec3::new(0.0, -1.0, 1.0).normalize()), "{m:?}");
    assert!((m.value - 2.0).abs() < 1e-9);
    d.set_manipulator(3.0, 0.5);
    assert_eq!(d.radius, "3 mm");

    // a revolve's rotator and a circular pattern's, from the stored sketch and axis
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [10, 0], "corner2": [14, 5]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "revolve", json!({"axis": "z", "angle": "90 deg"}));
    let revolve = s.design.features.last().unwrap().id;
    let body = s.model.bodies[0].id.0;
    run(
      &mut s,
      "pattern.circular",
      json!({"bodies": [body], "axis": "z", "count": 3, "angle": "120 deg", "join": false}),
    );
    let app = crate::App::with_session(s);
    let d = Dialog::edit(&app, revolve).unwrap();
    assert!((handle(&d, &app.s, Slot::Angle).value - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    let id = app.s.design.features.last().unwrap().id;
    let mut d = Dialog::edit(&app, id).unwrap();
    d.refresh_manip(&app.s);
    let h = handle(&d, &app.s, Slot::Angle);
    assert!((h.value - 120f64.to_radians()).abs() < 1e-9 && close(rotator(&h).1, DVec3::Z), "{h:?}");

    // an offset plane edits along its own base plane's normal
    let mut s = Session::default();
    run(&mut s, "plane.offset", json!({"plane": "yz", "offset": "5 mm"}));
    let app = crate::App::with_session(s);
    let id = app.s.design.features.last().unwrap().id;
    let d = Dialog::edit(&app, id).unwrap();
    let m = d.manipulator(&app.s).unwrap();
    assert!(m.dir.cross(DVec3::X).length() < 1e-9 && (m.value - 5.0).abs() < 1e-9, "{m:?}");
  }
}
