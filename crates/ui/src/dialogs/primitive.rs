//! Box, Cylinder, Sphere, Torus and Coil as Fusion runs them. No dialog at first: the cursor asks
//! for a plane or planar face (the planes show as for Create Sketch), then the base is drawn on
//! it (Box a rectangle by two corners, Cylinder, Torus and Coil a circle by its centre and a
//! point on it, Sphere a point), and only then does the dialog open, sized as drawn. Also their
//! rows (Fusion's names: Length / Width / Height, diameters rather than radii, Torus's Position)
//! and their manipulators (Box's height arrow and four side arrows, Cylinder's height and
//! diameter arrows, Sphere's arrow, Torus's two, Coil's height, diameter and section arrows).

use egui::{Color32, Pos2, Stroke};
use fox_engine::Sel;
use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use fox_engine::{Session, model::Primitive};
use serde_json::{Value, json};

use super::manip::mm;
use super::{Dialog, Kind, OPS, is_planar, plane_of, sel_chip, value_edit};
use crate::App;
use crate::handles::Handle;
use crate::manipulators::snap;

/// What is drawn on the plane before the dialog opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Base {
  Rect,
  Circle,
  Point,
}

/// How far the placement has got.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum Step {
  /// Waiting for a plane or planar face.
  Plane,
  /// Waiting for the first click on the plane (a corner, a centre or the point).
  First,
  /// The first click is at this point (plane coordinates); waiting for the second.
  Second([f64; 2]),
  /// Drawn (or nothing to draw): the dialog shows.
  #[default]
  Done,
}

/// Torus's Position: where the tube sits on the drawn circle.
pub(super) const TORUS_POSITIONS: [(&str, &str); 3] =
  [("center", "On Center"), ("inside", "Inside"), ("outside", "Outside")];

/// The base `kind` draws, for the commands that start with a plane pick.
pub(crate) fn base_of(kind: Kind) -> Option<Base> {
  match kind {
    Kind::Box => Some(Base::Rect),
    Kind::Cylinder | Kind::Torus => Some(Base::Circle),
    Kind::Sphere => Some(Base::Point),
    Kind::Form(f) if std::ptr::eq(f, &crate::forms::COIL) => Some(Base::Circle),
    _ => None,
  }
}

/// A radius expression for a diameter typed in a row.
pub(super) fn radius_of(d: &str) -> String {
  format!("({}) / 2", d.trim())
}

/// The diameter a stored radius expression came from: the inside of [`radius_of`]'s, twice a
/// plain value, or twice the expression.
pub(super) fn diameter_of(r: &str) -> String {
  let r = r.trim();
  if let Some(inner) = r.strip_suffix(") / 2").and_then(|x| x.strip_prefix('('))
    && balanced(inner)
  {
    return inner.to_string();
  }
  let (num, unit) = r.split_once(' ').unwrap_or((r, ""));
  match num.parse::<f64>() {
    Ok(v) if unit.chars().all(char::is_alphabetic) => {
      let n = fox_engine::sketch::Dim::new(v * 2.0).expr;
      if unit.is_empty() { n } else { format!("{n} {unit}") }
    }
    _ => format!("2 * ({r})"),
  }
}

/// Do `e`'s brackets pair up without one closing before it opens?
fn balanced(e: &str) -> bool {
  let mut depth = 0i32;
  for ch in e.chars() {
    depth += match ch {
      '(' => 1,
      ')' => -1,
      _ => 0,
    };
    if depth < 0 {
      return false;
    }
  }
  depth == 0
}

/// Torus's major radius for an Inner Diameter `a`, a Torus Diameter `b` and a Position.
pub(super) fn torus_major(a: &str, b: &str, position: &str) -> String {
  let (a, b) = (a.trim(), b.trim());
  match position {
    "inside" => format!("({a}) / 2 - ({b}) / 2"),
    "outside" => format!("({a}) / 2 + ({b}) / 2"),
    _ => radius_of(a),
  }
}

/// The Inner Diameter and Position a stored torus came from (see [`torus_major`]).
pub(super) fn torus_inner(major: &str, minor: &str) -> (String, &'static str) {
  let b = diameter_of(minor);
  for (pos, sign) in [("inside", '-'), ("outside", '+')] {
    if let Some(rest) = major.trim().strip_suffix(&format!(" {sign} ({b}) / 2"))
      && let Some(a) = rest.strip_suffix(") / 2").and_then(|x| x.strip_prefix('('))
      && balanced(a)
    {
      return (a.to_string(), pos);
    }
  }
  (diameter_of(major), "center")
}

/// A plane coordinate as the dialog's position fields hold it.
fn coord(v: f64) -> String {
  fox_engine::sketch::Dim::new((v * 1e6).round() / 1e6).expr
}

/// The point of `plane` a pointer ray `o + d·t` meets (plane coordinates).
fn on_plane(plane: &Plane, o: DVec3, d: DVec3) -> Option<[f64; 2]> {
  let n = plane.normal();
  let den = d.dot(n);
  if den.abs() < 1e-9 {
    return None;
  }
  let t = (plane.origin - o).dot(n) / den;
  Some(plane.to_local(o + d * t))
}

impl Dialog {
  /// A new primitive starts with the plane pick, as in Fusion: or with the drawing, on a plane or
  /// planar face picked before the command.
  pub(super) fn start_placing(&mut self, app: &mut App) {
    if base_of(self.kind).is_none() {
      return;
    }
    self.place = Step::Plane;
    if let Some(p) = app.preselected_plane() {
      self.place_on(&app.s, p);
    }
    app.picked_plane = None;
    app.s.selection.clear();
  }

  /// Box's Length and Width rows.
  #[cfg(test)]
  pub(crate) fn box_sizes(&self) -> [&str; 2] {
    [&self.a, &self.b]
  }

  /// Is the plane pick or the drawing still going (no dialog yet)?
  pub fn placing(&self) -> bool {
    self.place != Step::Done
  }

  /// Is the dialog waiting for a plane (the viewport offers them as for Create Sketch)?
  pub fn picks_plane(&self) -> bool {
    self.place == Step::Plane
  }

  /// The base goes on `plane` (a `sketch.create` plane value): the drawing starts.
  pub fn place_on(&mut self, s: &Session, plane: Value) {
    self.plane_label = plane_name(s, &plane);
    self.plane = plane;
    self.place = Step::First;
  }

  /// While the plane is picked: a planar face or construction plane selected in the canvas or
  /// the browser is the plane.
  pub(super) fn place_sync(&mut self, app: &mut App) {
    if self.place != Step::Plane {
      return;
    }
    let picked = app.s.selection.iter().find_map(|x| match *x {
      Sel::Face { body, face } => Some(is_planar(&app.s, body, face).then(|| json!({"body": body.0, "face": face}))),
      Sel::Feature { feature } if app.s.model.planes.contains_key(&feature) => {
        Some(Some(json!({"feature": feature.0})))
      }
      _ => None,
    });
    match picked {
      Some(Some(p)) => {
        self.place_on(&app.s, p);
        app.s.selection.clear();
      }
      Some(None) => {
        app.status = Some(("Pick a planar face".into(), true));
        app.s.selection.clear();
      }
      None => {}
    }
  }

  /// Fusion's cursor prompt for the placement step.
  pub(super) fn place_prompt(&self) -> Option<&'static str> {
    let base = base_of(self.kind)?;
    Some(match (self.place, base) {
      (Step::Done, _) => return None,
      (Step::Plane, _) => super::PICK_PLANE_HINT,
      (Step::First, Base::Rect) => "Place first corner",
      (Step::First, Base::Circle) => "Place center point",
      (Step::First, Base::Point) => "Draw point",
      (Step::Second(_), Base::Rect) => "Specify size of rectangle",
      (Step::Second(_), _) => "Specify diameter of circle",
    })
  }

  /// A click on the plane at `p` (plane coordinates): the first places the corner, centre or
  /// point; the second sizes the rectangle or circle and opens the dialog.
  pub fn place_click(&mut self, p: [f64; 2]) {
    let Some(base) = base_of(self.kind) else { return };
    match (self.place, base) {
      (Step::First, Base::Point) => self.placed(p, p),
      (Step::First, _) => self.place = Step::Second(p),
      (Step::Second(a), Base::Rect) if (p[0] - a[0]).abs() > 1e-9 && (p[1] - a[1]).abs() > 1e-9 => self.placed(a, p),
      (Step::Second(a), Base::Circle) if (p[0] - a[0]).hypot(p[1] - a[1]) > 1e-9 => self.placed(a, p),
      _ => {}
    }
  }

  /// The base is drawn from `a` to `b`: the dialog's values take its size and place.
  fn placed(&mut self, a: [f64; 2], b: [f64; 2]) {
    let unit = self.unit;
    let r = (b[0] - a[0]).hypot(b[1] - a[1]);
    let centre = if self.kind == Kind::Box { [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0] } else { a };
    self.at = [coord(centre[0]), coord(centre[1])];
    match self.kind {
      Kind::Box => {
        self.a = mm(unit, (b[0] - a[0]).abs());
        self.b = mm(unit, (b[1] - a[1]).abs());
      }
      Kind::Cylinder | Kind::Torus => self.a = mm(unit, 2.0 * r),
      Kind::Form(f) => {
        for (i, input) in f.inputs.iter().enumerate() {
          match input.key() {
            "plane" => {
              self.form.values[i] = self.plane.clone();
              self.form.chosen[i] = Some(self.plane.clone());
            }
            "at" => self.form.values[i] = json!(a),
            "diameter" => self.form.values[i] = json!(mm(unit, 2.0 * r)),
            _ => {}
          }
        }
      }
      _ => {}
    }
    self.place = Step::Done;
    self.dirty = true;
  }

  /// The rows of Box, Cylinder, Sphere and Torus, as Fusion's: the placement chip (the picked
  /// plane; the drawn point or circle), the sizes, Torus's Position and the Operation.
  pub(super) fn primitive_rows(&mut self, ui: &mut egui::Ui, creating: bool) {
    let accent = self.accent();
    let (chip, labels): (&str, &[&str]) = match self.kind {
      Kind::Box => ("Placement", &["Length", "Width", "Height"]),
      Kind::Cylinder => ("Placement", &["Diameter", "Height"]),
      Kind::Sphere => ("Point", &["Diameter"]),
      _ => ("Profile", &["Inner Diameter", "Torus Diameter"]),
    };
    ui.label(chip);
    // the edited feature's chip is blank, as Fusion's
    let text = match (creating, chip) {
      (false, _) => "",
      (true, "Placement") => self.plane_label.as_str(),
      _ => "1 selected",
    };
    let text = text.to_string();
    sel_chip(ui, accent, &text, true);
    ui.end_row();
    for (i, l) in labels.iter().enumerate() {
      ui.label(*l);
      let f = match i {
        0 => &mut self.a,
        1 => &mut self.b,
        _ => &mut self.c,
      };
      if value_edit(ui, f).changed() {
        self.dirty = true;
      }
      ui.end_row();
    }
    if self.kind == Kind::Torus {
      ui.label("Position");
      let mut cur = self.position;
      if super::choice::combo(ui, "torus-position", &TORUS_POSITIONS, &mut cur, |_| None) {
        self.position = cur;
        self.dirty = true;
      }
      ui.end_row();
    }
    ui.label("Operation");
    self.combo(ui, "op", &OPS, false);
    ui.end_row();
    self.comp_name_row(ui);
  }

  /// The primitive's command parameters (the shape's own; the plane, place and operation are
  /// added by the caller): diameters become the engine's radii.
  pub(super) fn primitive_shape(&self) -> Value {
    match self.kind {
      Kind::Box => json!({"width": self.a, "depth": self.b, "height": self.c}),
      Kind::Cylinder => json!({"radius": radius_of(&self.a), "height": self.b}),
      Kind::Sphere => json!({"radius": radius_of(&self.a)}),
      _ => json!({"major": torus_major(&self.a, &self.b, self.position), "minor": radius_of(&self.b)}),
    }
  }

  /// Fills the rows from a stored primitive (Edit Feature).
  pub(super) fn load_primitive(&mut self, shape: &Primitive) {
    match shape {
      Primitive::Box { width, depth, height } => {
        (self.a, self.b, self.c) = (width.clone(), depth.clone(), height.clone())
      }
      Primitive::Cylinder { radius, height } => (self.a, self.b) = (diameter_of(radius), height.clone()),
      Primitive::Sphere { radius } => self.a = diameter_of(radius),
      Primitive::Torus { major, minor } => {
        let (a, position) = torus_inner(major, minor);
        (self.a, self.b, self.position) = (a, diameter_of(minor), position);
      }
    }
  }

  /// The plane, centre (world), in-plane axes and normal the primitive sits on.
  fn primitive_frame(&self, s: &Session) -> Option<(DVec3, DVec3, DVec3, DVec3)> {
    let pl = plane_of(s, &self.plane)?;
    let at = self.at_pt()?;
    Some((pl.to_world(at), pl.x, pl.y, pl.normal()))
  }

  /// The handles of a primitive being created, as Fusion's: Box's height arrow up from the base
  /// centre and its four side arrows (length and width) at half height; Cylinder's height and
  /// diameter arrows; Sphere's arrow; Torus's tube (Torus Diameter) and ring (Inner Diameter)
  /// arrows. The first edits the floating value box's field (see [`Dialog::primitive_field`]).
  pub(super) fn primitive_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    if self.placing() {
      return None;
    }
    let (c, x, y, n) = self.primitive_frame(s)?;
    let unit = self.unit;
    let len = |e: &str| self.len(s, e);
    Some(match self.kind {
      Kind::Box => {
        let (l, w, h) = (len(&self.a)?, len(&self.b)?, len(&self.c)?);
        let mid = c + n * h / 2.0;
        vec![
          Handle::arrow(c, n, h, mm(unit, h)),
          Handle::arrow(mid, x, l / 2.0, String::new()),
          Handle::arrow(mid, -x, l / 2.0, String::new()),
          Handle::arrow(mid, y, w / 2.0, String::new()),
          Handle::arrow(mid, -y, w / 2.0, String::new()),
        ]
      }
      Kind::Cylinder => {
        let (d, h) = (len(&self.a)?, len(&self.b)?);
        vec![Handle::arrow(c, n, h, mm(unit, h)), Handle::arrow(c, x, d / 2.0, mm(unit, d))]
      }
      Kind::Sphere => {
        let d = len(&self.a)?;
        vec![Handle::arrow(c, x, d / 2.0, mm(unit, d))]
      }
      Kind::Torus => {
        let (a, b) = (len(&self.a)?, len(&self.b)?);
        let ring = match self.position {
          "inside" => (a - b) / 2.0,
          "outside" => (a + b) / 2.0,
          _ => a / 2.0,
        };
        vec![Handle::arrow(c + x * ring, n, b / 2.0, mm(unit, b)), Handle::arrow(c, -x, a / 2.0, mm(unit, a))]
      }
      Kind::Form(f) => return self.coil_handles(s, f),
      _ => return None,
    })
  }

  /// Coil's handles: the height (while the Type asks for it) up the axis, the diameter out from
  /// the axis, the section size up from the section at the coil's start, and Fusion's Angle
  /// handle, a disc on an arc turning about the coil's foot from the axis direction (outward for
  /// a positive angle, which widens the coil as it rises).
  fn coil_handles(&self, s: &Session, f: &crate::forms::Form) -> Option<Vec<Handle>> {
    let pl = plane_of(s, &self.form_value("plane")?.clone())?;
    let at = self.form_value("at")?.as_array()?;
    let c = pl.to_world([at.first()?.as_f64()?, at.get(1)?.as_f64()?]);
    let (x, n) = (pl.x, pl.normal());
    let unit = self.unit;
    let mut out = Vec::new();
    for k in self.coil_handle_keys(f) {
      if k == "angle" {
        let a = self.ang(s, self.form_value("angle")?.as_str()?).unwrap_or(0.0);
        let d = self.len(s, self.form_value("diameter")?.as_str()?)?;
        out.push(Handle::dial(c + x * d / 2.0, n.cross(x), n, None, a, super::manip::deg(a)).taper());
        continue;
      }
      let v = self.len(s, self.form_value(k)?.as_str()?)?;
      out.push(match k {
        "height" => Handle::arrow(c, n, v, mm(unit, v)),
        "diameter" => Handle::arrow(c, x, v / 2.0, mm(unit, v)),
        _ => {
          let d = self.len(s, self.form_value("diameter")?.as_str()?)?;
          Handle::arrow(c + x * d / 2.0, n, v / 2.0, mm(unit, v))
        }
      });
    }
    Some(out)
  }

  /// The Coil fields its arrows edit, in order.
  fn coil_handle_keys(&self, f: &crate::forms::Form) -> Vec<&'static str> {
    let shown = |k: &str| f.inputs.iter().position(|x| x.key() == k).is_some_and(|i| self.form.shown(f, i));
    ["height", "diameter", "section_size", "angle"].into_iter().filter(|k| shown(k)).collect()
  }

  /// The field the `i`th primitive handle's floating box edits: the first handle's alone.
  pub(super) fn primitive_field(&mut self, i: usize) -> Option<&mut String> {
    if i != 0 || base_of(self.kind).is_none() {
      return None;
    }
    match self.kind {
      Kind::Box => Some(&mut self.c),
      Kind::Cylinder | Kind::Torus => Some(&mut self.b),
      Kind::Sphere => Some(&mut self.a),
      Kind::Form(f) => {
        let key = *self.coil_handle_keys(f).first()?;
        let j = f.inputs.iter().position(|x| x.key() == key)?;
        match &mut self.form.values[j] {
          Value::String(t) => Some(t),
          _ => None,
        }
      }
      _ => None,
    }
  }

  /// Takes primitive handle `i`'s dragged value `v` (snapped to `step`) into the rows. False
  /// when nothing changed.
  pub(super) fn set_primitive_handle(&mut self, s: &Session, i: usize, v: f64, step: f64) -> bool {
    let unit = self.unit;
    // Coil's Angle: short of a right angle either way
    if let Kind::Form(f) = self.kind
      && self.coil_handle_keys(f).get(i) == Some(&"angle")
    {
      let Some(j) = f.inputs.iter().position(|x| x.key() == "angle") else { return false };
      let a = snap(v, step).clamp(-89f64.to_radians(), 89f64.to_radians());
      self.form.values[j] = json!(super::manip::deg(a));
      return true;
    }
    let v = snap(v, step).max(step.max(1e-3) / 2.0);
    let size = |half: f64| mm(unit, 2.0 * half);
    match (self.kind, i) {
      (Kind::Box, 0) => self.c = mm(unit, v),
      // a side arrow moves its side: the opposite one stays where it is
      (Kind::Box, 1..=4) => {
        let k = if i <= 2 { 0 } else { 1 };
        let field = if k == 0 { &self.a } else { &self.b };
        let Some(old) = self.len(s, field) else { return false };
        let new = v + old / 2.0;
        let sign = if i % 2 == 1 { 1.0 } else { -1.0 };
        let Some(mut at) = self.at_pt() else { return false };
        at[k] += sign * (new - old) / 2.0;
        // the side arrows run along the plane's axes, the box's length along x
        if k == 0 {
          self.a = mm(unit, new);
        } else {
          self.b = mm(unit, new);
        }
        self.at = [coord(at[0]), coord(at[1])];
      }
      (Kind::Cylinder, 0) => self.b = mm(unit, v),
      (Kind::Cylinder, 1) | (Kind::Sphere, 0) | (Kind::Torus, 1) => self.a = size(v),
      (Kind::Torus, 0) => self.b = size(v),
      (Kind::Form(f), _) => {
        let Some(key) = self.coil_handle_keys(f).get(i).copied() else { return false };
        let Some(j) = f.inputs.iter().position(|x| x.key() == key) else { return false };
        self.form.values[j] = json!(if key == "height" { mm(unit, v) } else { size(v) });
      }
      _ => return false,
    }
    true
  }
}

/// How the dialogs name a plane value: the origin planes, a construction plane's name, a face.
fn plane_name(s: &Session, plane: &Value) -> String {
  if let Some(n) = plane.as_str() {
    return format!("{} plane", n.to_uppercase());
  }
  if let Some(f) = plane.get("feature").and_then(Value::as_u64) {
    return s.design.feature(fox_engine::model::FeatureId(f as u32)).map(|f| f.name.clone()).unwrap_or_default();
  }
  match plane.get("body").and_then(Value::as_u64) {
    Some(b) => format!("Face of {}", s.body_name(fox_engine::model::BodyId(b as u32))),
    None => "Plane".into(),
  }
}

/// The heads-up boxes while a rectangle or circle base is sized, as Fusion's: what was typed in
/// each (empty: the box follows the pointer) and the box with the keyboard. Typing goes into the
/// active box, Tab moves to the next and Enter places the base at the typed size (see
/// [`take_keys`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Hud {
  pub typed: [String; 2],
  pub active: usize,
  /// Enter was pressed: the base goes down at the typed size this frame.
  pub commit: bool,
  /// Where the pointer last met the plane: Enter places the base towards it even while the
  /// pointer rests on a box rather than the canvas.
  pub pointer: Option<[f64; 2]>,
}

fn hud_id() -> egui::Id {
  egui::Id::new("primitive-hud")
}

/// The boxes a base has: the rectangle's length and width, the circle's diameter.
fn hud_boxes(base: Base) -> usize {
  match base {
    Base::Rect => 2,
    Base::Circle => 1,
    Base::Point => 0,
  }
}

/// While a primitive's base is sized, its heads-up boxes take the keyboard before anything else
/// sees it (the shortcuts, the dialogs' typing): each key in the order typed, so `8 Tab 8 Enter`
/// works however the keys are split over frames. Characters go into the active box, Backspace
/// takes one back, Tab (Shift+Tab) moves to the next (previous) box and Enter places the base.
/// True when the boxes took the keys.
pub(crate) fn take_keys(app: &App, ctx: &egui::Context) -> bool {
  let Some(d) = app.dialog.as_ref() else { return false };
  let Some(base) = base_of(d.kind) else { return false };
  let n = hud_boxes(base);
  if !matches!(d.place, Step::Second(_)) || n == 0 || ctx.input(|i| i.modifiers.command || i.modifiers.alt) {
    return false;
  }
  use egui::{Event, Key};
  let mut hud: Hud = ctx.data(|m| m.get_temp(hud_id())).unwrap_or_default();
  hud.active = hud.active.min(n - 1);
  ctx.input_mut(|i| {
    let mut done = false;
    i.events.retain(|e| {
      if done {
        return true;
      }
      match e {
        Event::Text(t) => {
          hud.typed[hud.active].extend(t.chars().filter(|c| !c.is_control()));
          false
        }
        Event::Key { key: Key::Escape, .. } => true,
        Event::Key { key, pressed, modifiers, .. } => {
          if *pressed {
            match key {
              Key::Backspace => {
                hud.typed[hud.active].pop();
              }
              Key::Tab if modifiers.shift => hud.active = (hud.active + n - 1) % n,
              Key::Tab => hud.active = (hud.active + 1) % n,
              Key::Enter => {
                hud.commit = true;
                done = true;
              }
              _ => {}
            }
          }
          false
        }
        _ => true,
      }
    });
  });
  ctx.data_mut(|m| m.insert_temp(hud_id(), hud));
  true
}

/// A typed size (mm): a bare number or unitless expression is in the design's unit.
pub(super) fn typed_mm(s: &Session, unit: fox_engine::model::LengthUnit, text: &str) -> Option<f64> {
  use fox_engine::expr::{Empty, Kind as K, eval, eval_raw};
  let t = text.trim();
  if t.is_empty() {
    return None;
  }
  let bare = matches!(eval_raw(t, &Empty), Ok(v) if !v.has_unit);
  let e = if bare { format!("({t}) * 1 {}", unit.symbol()) } else { t.to_string() };
  eval(&e, K::Length, &s.model.params).ok().filter(|v| *v > 1e-9)
}

/// Where the typed sizes put the base's second point, from its first `a` towards the pointer `c`.
pub(super) fn hud_point(base: Base, a: [f64; 2], c: [f64; 2], sizes: [Option<f64>; 2]) -> [f64; 2] {
  let sign = |d: f64| if d < 0.0 { -1.0 } else { 1.0 };
  match base {
    Base::Rect => {
      [sizes[0].map_or(c[0], |w| a[0] + sign(c[0] - a[0]) * w), sizes[1].map_or(c[1], |h| a[1] + sign(c[1] - a[1]) * h)]
    }
    Base::Circle => sizes[0].map_or(c, |d| {
      let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
      let l = dx.hypot(dy);
      let (ux, uy) = if l > 1e-9 { (dx / l, dy / l) } else { (1.0, 0.0) };
      [a[0] + ux * d / 2.0, a[1] + uy * d / 2.0]
    }),
    Base::Point => c,
  }
}

/// While a primitive is being made on a picked plane: the name of Fusion's temporary sketch its
/// base is drawn on, which the browser and timeline list until OK.
pub(crate) fn temp_sketch_name(app: &App) -> Option<String> {
  let d = app.dialog.as_ref()?;
  if base_of(d.kind).is_none() || d.editing.is_some() || d.place == Step::Plane {
    return None;
  }
  let features = &app.s.design.features;
  let n = features.iter().filter(|f| matches!(f.kind, fox_engine::model::FeatureKind::Sketch { .. })).count() + 1;
  (n..).map(|k| format!("Sketch{k}")).find(|name| !features.iter().any(|f| f.name == *name))
}

/// The base as drawn, in plane coordinates: the rectangle's corners or the circle's points.
fn base_outline(base: Base, a: [f64; 2], c: [f64; 2]) -> Vec<[f64; 2]> {
  match base {
    Base::Rect => vec![a, [c[0], a[1]], c, [a[0], c[1]]],
    Base::Circle => {
      let r = (c[0] - a[0]).hypot(c[1] - a[1]);
      (0..64)
        .map(|k| {
          let t = std::f64::consts::TAU * k as f64 / 64.0;
          [a[0] + r * t.cos(), a[1] + r * t.sin()]
        })
        .collect()
    }
    Base::Point => Vec::new(),
  }
}

/// The drawn base's outline once the dialog is open: Fusion draws the base's edges blue on the
/// grey preview body.
fn placed_outline(d: &Dialog, s: &Session) -> Option<(Plane, Vec<[f64; 2]>)> {
  let base = base_of(d.kind)?;
  let plane = plane_of(s, &d.plane)?;
  let unit_len = |e: &str| d.len(s, e);
  let at = d.at_pt()?;
  let pts = match (base, d.kind) {
    (Base::Rect, _) => {
      let (l, w) = (unit_len(&d.a)?, unit_len(&d.b)?);
      base_outline(base, [at[0] - l / 2.0, at[1] - w / 2.0], [at[0] + l / 2.0, at[1] + w / 2.0])
    }
    (Base::Circle, Kind::Form(_)) => {
      let c = d.form_value("at")?.as_array()?;
      let c = [c.first()?.as_f64()?, c.get(1)?.as_f64()?];
      let r = unit_len(d.form_value("diameter")?.as_str()?)? / 2.0;
      base_outline(base, c, [c[0] + r, c[1]])
    }
    (Base::Circle, _) => base_outline(base, at, [at[0] + unit_len(&d.a)? / 2.0, at[1]]),
    _ => return None,
  };
  Some((plane, pts))
}

/// The plane pick's and the base drawing's canvas overlay: the base following the pointer with
/// its heads-up size boxes, a dot at the pointer, and the click that places the next point. Once
/// the dialog is open, the base's outline in blue. True when it took the canvas's pointer.
pub(crate) fn overlay(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
  step: f64,
) -> bool {
  let Some(d) = app.dialog.as_ref() else { return false };
  let Some(base) = base_of(d.kind) else { return false };
  let ink = Color32::from_rgb(30, 110, 235);
  let stroke = Stroke::new(1.5, ink);
  let from = match d.place {
    Step::First => None,
    Step::Second(a) => Some(a),
    Step::Done => {
      // the dialog is open: the drawn base stays outlined in blue on the preview
      if d.editing.is_none()
        && let Some((plane, pts)) = placed_outline(d, &app.s)
      {
        let v: Vec<Pos2> = pts.iter().filter_map(|p| project(plane.to_world(*p))).collect();
        if v.len() == pts.len() && v.len() > 1 {
          painter.add(egui::Shape::closed_line(v, stroke));
        }
      }
      return false;
    }
    Step::Plane => return false,
  };
  let Some(plane) = plane_of(&app.s, &d.plane) else { return false };
  let unit = d.unit;
  let ctx = resp.ctx.clone();
  let mut hud: Hud = ctx.data(|m| m.get_temp(hud_id())).unwrap_or_default();
  if from.is_none() {
    hud = Hud::default();
  }
  let n = hud_boxes(base);
  let raw = resp.hover_pos().or(resp.interact_pointer_pos()).and_then(|p| {
    let (o, dir) = ray(p);
    on_plane(&plane, o, dir).map(|q| q.map(|v| snap(v, step)))
  });
  if raw.is_some() {
    hud.pointer = raw;
  }
  let raw = raw.or(hud.pointer.filter(|_| from.is_some()));
  let sizes = [typed_mm(&app.s, unit, &hud.typed[0]), typed_mm(&app.s, unit, &hud.typed[1])];
  let cur = match (raw, from) {
    (Some(c), Some(a)) => Some(hud_point(base, a, c, sizes)),
    (c, _) => c,
  };
  let s = |p: [f64; 2]| project(plane.to_world(p));
  // the boxes' places and the live sizes they show
  let mut boxes: Vec<(Pos2, String)> = Vec::new();
  if let Some(c) = cur {
    if let Some(p) = raw.and_then(s) {
      painter.circle_filled(p, 3.0, ink);
    }
    if let Some(a) = from {
      let pts = base_outline(base, a, c);
      let v: Vec<Pos2> = pts.iter().filter_map(|p| s(*p)).collect();
      if v.len() == pts.len() {
        painter.add(egui::Shape::closed_line(v, stroke));
      }
      match base {
        Base::Rect => {
          if let Some(p) = s([(a[0] + c[0]) / 2.0, a[1]]) {
            boxes.push((p + egui::vec2(0.0, 14.0), mm(unit, (c[0] - a[0]).abs())));
          }
          if let Some(p) = s([c[0], (a[1] + c[1]) / 2.0]) {
            boxes.push((p + egui::vec2(36.0, 0.0), mm(unit, (c[1] - a[1]).abs())));
          }
        }
        _ => {
          let r = (c[0] - a[0]).hypot(c[1] - a[1]);
          if let (Some(p), Some(q)) = (s(a), s(c)) {
            painter.add(egui::Shape::dashed_line(&[p, q], Stroke::new(1.0, ink), 4.0, 3.0));
            boxes.push((q + egui::vec2(30.0, -14.0), mm(unit, 2.0 * r)));
          }
        }
      }
    }
  }
  // Enter (taken with the typing in `take_keys`) places the base at the typed size
  let commit = std::mem::take(&mut hud.commit) && from.is_some();
  if from.is_some() && n > 0 && boxes.len() == n {
    hud.active = hud.active.min(n - 1);
    for (i, (p, live)) in boxes.iter().enumerate() {
      // the box with the keyboard shows what was typed (else the live size, greyed) and a caret
      let active = i == hud.active;
      let typed = !hud.typed[i].is_empty();
      let text = if typed { hud.typed[i].clone() } else { live.clone() };
      let color = if active && !typed { Color32::from_gray(150) } else { Color32::from_gray(30) };
      let g = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), color);
      let width = if active { g.size().x.max(56.0) } else { g.size().x };
      let r = egui::Rect::from_center_size(*p, egui::vec2(width + 10.0, g.size().y + 4.0));
      let border = Stroke::new(if active { 1.5 } else { 1.0 }, ink);
      painter.rect(r, 2.0, Color32::WHITE, border, egui::StrokeKind::Inside);
      let at = r.min + egui::vec2(5.0, 2.0);
      if active {
        let x = at.x + if typed { g.size().x + 1.0 } else { 0.0 };
        painter.line_segment([egui::pos2(x, r.top() + 3.0), egui::pos2(x, r.bottom() - 3.0)], Stroke::new(1.0, ink));
      }
      painter.galley(at, g, color);
    }
  }
  if resp.contains_pointer() {
    resp.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
  }
  let placed = (resp.clicked() || commit) && cur.is_some();
  if placed
    && let Some(c) = cur
    && let Some(d) = app.dialog.as_mut()
  {
    d.place_click(c);
    hud = Hud::default();
  }
  ctx.data_mut(|m| m.insert_temp(hud_id(), hud));
  resp.contains_pointer()
}

// ---- Pipe's arrows ----

/// Pipe: its path (the picked curves of the path sketch, chained as the engine chains them) in
/// sketch coordinates, with the sketch's plane.
fn pipe_path(d: &Dialog, s: &Session) -> Option<(Vec<fox_engine::sketch::Seg>, Plane)> {
  let sketch = fox_engine::model::FeatureId(u32::try_from(d.form_value("path")?.as_u64()?).ok()?);
  let curves: Vec<fox_engine::sketch::CurveId> = d
    .form_value("path_curves")
    .and_then(Value::as_array)
    .map(|a| a.iter().filter_map(Value::as_u64).map(|c| fox_engine::sketch::CurveId(c as u32)).collect())
    .unwrap_or_default();
  let r = s.model.sketches.get(&sketch)?;
  let segs = fox_engine::model::sheet::open_path(&r.sketch, &curves).ok()?;
  (!segs.is_empty()).then_some((segs, r.plane))
}

/// Is the dialog Pipe's?
pub(super) fn is_pipe(kind: Kind) -> bool {
  matches!(kind, Kind::Form(f) if std::ptr::eq(f, &crate::forms::PIPE))
}

impl Dialog {
  /// Fusion's Pipe arrows: one at the pipe's end along the path for the Distance (the fraction
  /// of the path it runs), one out from the path's start for the Section Size. (Fusion's arrow
  /// at the path's start drags the second Distance, which the engine doesn't take: a pipe runs
  /// from its path's start.)
  pub(super) fn pipe_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (segs, pl) = pipe_path(self, s)?;
    let total = fox_engine::model::pathpattern::path_length(&segs);
    let f = self.form_value("distance")?.as_str()?.trim().parse::<f64>().ok()?.clamp(0.0, 1.0);
    let world = |t: [f64; 2]| (pl.x * t[0] + pl.y * t[1]).try_normalize();
    let (end, t_end) = fox_engine::model::pathpattern::path_at(&segs, total * f)?;
    let (start, t_start) = fox_engine::model::pathpattern::path_at(&segs, 0.0)?;
    let size = self.len(s, self.form_value("diameter")?.as_str()?)?;
    let side = pl.normal().cross(world(t_start)?).try_normalize()?;
    Some(vec![
      Handle::arrow(pl.to_world(end), world(t_end)?, total * f, format!("{f:.2}")),
      Handle::arrow(pl.to_world(start), side, size / 2.0, mm(self.unit, size)),
    ])
  }

  /// Takes Pipe arrow `i`'s dragged value `v` (snapped to `step`) into its row: the Distance as
  /// the fraction of the path, or the Section Size. False when nothing changed.
  pub(super) fn set_pipe_handle(&mut self, s: &Session, i: usize, v: f64, step: f64) -> bool {
    let Kind::Form(f) = self.kind else { return false };
    let (key, value) = match i {
      0 => {
        let Some((segs, _)) = pipe_path(self, s) else { return false };
        let total = fox_engine::model::pathpattern::path_length(&segs);
        if total <= 1e-9 {
          return false;
        }
        ("distance", format!("{:.2}", (v / total).clamp(0.01, 1.0)))
      }
      1 => ("diameter", mm(self.unit, 2.0 * snap(v, step).max(step.max(1e-3) / 2.0))),
      _ => return false,
    };
    let Some(j) = f.inputs.iter().position(|x| x.key() == key) else { return false };
    self.form.values[j] = json!(value);
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::forms::{COIL, PIPE};
  use fox_engine::model::FeatureKind;

  fn app() -> App {
    App::with_session(Session::default())
  }

  /// Starts `kind` and draws its base on the XY plane with clicks at `clicks`.
  fn drawn(app: &mut App, kind: Kind, clicks: &[[f64; 2]]) {
    app.start(kind);
    let d = app.dialog.as_mut().unwrap();
    d.place_on(&app.s, json!("xy"));
    for c in clicks {
      d.place_click(*c);
    }
    assert!(!d.placing(), "{kind:?} is drawn");
  }

  fn volume(app: &App) -> f64 {
    app.s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  /// `ui:dialogs:primitive-plane-pick-flow`: Box opens no dialog; the cursor asks for a plane,
  /// then the rectangle's corners, and only then do the rows show, sized as drawn.
  #[test]
  fn box_starts_with_a_plane_pick_and_a_drawn_rectangle() {
    let mut app = app();
    app.start(Kind::Box);
    let d = app.dialog.as_mut().unwrap();
    assert!(d.placing() && d.picks_plane());
    assert_eq!(d.prompt(false).as_deref(), Some(super::super::PICK_PLANE_HINT));
    assert!(!d.ready(), "nothing to preview yet");
    d.place_on(&app.s, json!("xy"));
    assert!(!d.picks_plane());
    assert_eq!(d.prompt(false).as_deref(), Some("Place first corner"));
    d.place_click([0.0, 0.0]);
    assert_eq!(d.prompt(false).as_deref(), Some("Specify size of rectangle"));
    // a rectangle without width isn't one
    d.place_click([0.0, 5.0]);
    assert!(d.placing());
    d.place_click([20.0, 10.0]);
    assert!(!d.placing());
    assert_eq!((d.a.as_str(), d.b.as_str(), d.c.as_str()), ("20 mm", "10 mm", "2.54 mm"));
    assert_eq!(d.plane_label, "XY plane");
    d.preview(&mut app.s);
    assert!(d.preview_applied() && d.error.is_none(), "{:?}", d.error);
    assert!((volume(&app) - 20.0 * 10.0 * 2.54).abs() < 1e-6);
    let (lo, hi) = app.s.model.bounds().unwrap();
    assert!(lo.distance(DVec3::ZERO) < 1e-3 && hi.distance(DVec3::new(20.0, 10.0, 2.54)) < 1e-3, "{lo} {hi}");
    // OK keeps the preview; the timeline's edit reopens the rows as EDIT FEATURE
    app.dialog = None;
    let id = app.s.design.features.last().unwrap().id;
    let e = Dialog::edit(&app, id).unwrap();
    assert_eq!(e.heading(), "EDIT FEATURE");
    assert!(!e.placing());
    assert_eq!((e.a.as_str(), e.b.as_str(), e.c.as_str()), ("20 mm", "10 mm", "2.54 mm"));
  }

  /// A planar face picked during the plane pick is the plane; a curved one is refused.
  #[test]
  fn a_planar_face_picked_first_is_the_plane() {
    let mut app = app();
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = &app.s.model.bodies[0];
    let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let body = b.id;
    app.s.selection = vec![Sel::Face { body, face: top }];
    app.start(Kind::Cylinder);
    let d = app.dialog.as_ref().unwrap();
    assert!(d.placing() && !d.picks_plane(), "the picked face is the plane");
    assert_eq!(d.prompt(false).as_deref(), Some("Place center point"));
    assert_eq!(d.plane, json!({"body": body.0, "face": top}));
    assert!(app.s.selection.is_empty());
  }

  /// `ui:dialogs:primitive-rows`: Cylinder, Sphere and Torus take diameters; Torus's Position
  /// puts the tube on, inside or outside the drawn circle.
  #[test]
  fn cylinder_sphere_and_torus_take_diameters() {
    use std::f64::consts::PI;
    let mut app = app();
    drawn(&mut app, Kind::Cylinder, &[[0.0, 0.0], [10.0, 0.0]]);
    let d = app.dialog.as_mut().unwrap();
    assert_eq!(d.prompt(false), None);
    assert_eq!((d.a.as_str(), d.b.as_str()), ("20 mm", "2.54 mm"));
    d.preview(&mut app.s);
    assert!((volume(&app) - PI * 100.0 * 2.54).abs() < 1e-3, "{}", volume(&app));
    app.cancel_dialog();

    drawn(&mut app, Kind::Sphere, &[[1.0, 2.0]]);
    let d = app.dialog.as_mut().unwrap();
    assert_eq!((d.a.as_str(), d.at[0].as_str(), d.at[1].as_str()), ("10.16 mm", "1", "2"));
    d.preview(&mut app.s);
    assert!((volume(&app) - 4.0 / 3.0 * PI * 5.08f64.powi(3)).abs() < 1e-2, "{}", volume(&app));
    app.cancel_dialog();

    drawn(&mut app, Kind::Torus, &[[0.0, 0.0], [0.0, 10.0]]);
    let d = app.dialog.as_mut().unwrap();
    assert_eq!((d.a.as_str(), d.b.as_str(), d.position), ("20 mm", "5.08 mm", "center"));
    d.position = "outside";
    d.preview(&mut app.s);
    let (r, big) = (2.54, 10.0 + 2.54);
    assert!((volume(&app) - 2.0 * PI * PI * big * r * r).abs() < 1e-2, "{}", volume(&app));
    app.dialog = None;
    let id = app.s.design.features.last().unwrap().id;
    let e = Dialog::edit(&app, id).unwrap();
    assert_eq!((e.a.as_str(), e.b.as_str(), e.position), ("20 mm", "5.08 mm", "outside"));
  }

  /// `ui:dialogs:primitive-manipulators`: Box's height arrow (with the floating box) and four
  /// side arrows; a side arrow moves its side and leaves the opposite one.
  #[test]
  fn box_arrows_size_it_and_move_one_side() {
    let mut app = app();
    drawn(&mut app, Kind::Box, &[[0.0, 0.0], [20.0, 10.0]]);
    let d = app.dialog.as_mut().unwrap();
    let h = d.handles(&app.s);
    assert_eq!(h.len(), 5);
    assert_eq!(d.handle_field(0).cloned(), Some("2.54 mm".to_string()), "the height has the floating box");
    assert!(d.handle_field(1).is_none());
    d.set_handle(&app.s, 1, 15.0, 0.5);
    assert_eq!((d.a.as_str(), d.at[0].as_str()), ("25 mm", "12.5"));
    d.set_handle(&app.s, 4, 7.0, 0.5);
    assert_eq!((d.b.as_str(), d.at[1].as_str()), ("12 mm", "4"));
    d.set_handle(&app.s, 0, 4.0, 0.5);
    assert_eq!(d.c, "4 mm");
    d.preview(&mut app.s);
    let (lo, hi) = app.s.model.bounds().unwrap();
    assert!(
      lo.distance(DVec3::new(0.0, -2.0, 0.0)) < 1e-3 && hi.distance(DVec3::new(25.0, 10.0, 4.0)) < 1e-3,
      "{lo} {hi}"
    );
    // Cylinder: height and diameter arrows; Sphere one; Torus two
    for (kind, clicks, n) in [
      (Kind::Cylinder, &[[0.0, 0.0], [5.0, 0.0]][..], 2),
      (Kind::Sphere, &[[0.0, 0.0]][..], 1),
      (Kind::Torus, &[[0.0, 0.0], [5.0, 0.0]][..], 2),
    ] {
      drawn(&mut app, kind, clicks);
      let d = app.dialog.as_mut().unwrap();
      assert_eq!(d.handles(&app.s).len(), n, "{kind:?}");
      assert!(d.handle_field(0).is_some());
    }
    let d = app.dialog.as_mut().unwrap();
    d.set_handle(&app.s, 1, 8.0, 0.5);
    assert_eq!(d.a, "16 mm", "the ring arrow sets the Inner Diameter");
  }

  /// `ui:dialogs:coil-rows`: Coil draws its circle first, then asks Fusion's rows; the Type's two
  /// values make the pitch, Rotation the hand and the Section Position the centre diameter.
  #[test]
  fn coil_draws_its_circle_and_takes_fusions_rows() {
    let mut app = app();
    drawn(&mut app, Kind::Form(&COIL), &[[0.0, 0.0], [10.0, 0.0]]);
    let d = app.dialog.as_mut().unwrap();
    assert_eq!(d.form_value("diameter"), Some(&json!("20 mm")));
    assert_eq!(d.form_value("revolutions"), Some(&json!("3")));
    assert_eq!(d.form_value("type"), Some(&json!("revolution_height")));
    d.preview(&mut app.s);
    assert!(d.preview_applied() && d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies.len(), 1);
    let h = d.handles(&app.s);
    assert_eq!(h.len(), 4, "height, diameter and section arrows, and the Angle handle");
    assert!(h[3].angular() && h[3].head == crate::handles::Head::Taper && h[3].value.abs() < 1e-9);
    // the Angle handle dragged out 10°: the coil widens as it rises
    let before = app.s.model.bodies[0].shape.mass().unwrap();
    d.set_handle(&app.s, 3, 10f64.to_radians() + 0.001, 1f64.to_radians());
    assert_eq!(d.form_value("angle"), Some(&json!("10 deg")));
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    let after = app.s.model.bodies[0].shape.mass().unwrap();
    let width = |m: &fox_engine::kernel::MassProps| m.bbox_max.x - m.bbox_min.x;
    assert!(width(&after) > width(&before) + 5.0, "{before:?} → {after:?}");
    assert!((after.bbox_max.z - before.bbox_max.z).abs() < 0.5, "same height");
    let (cmd, p) = crate::forms::engine_command(
      &COIL,
      json!({"plane": "xy", "at": [0.0, 0.0], "type": "revolution_height", "rotation": "ccw", "diameter": "20 mm",
        "revolutions": "3", "height": "15 mm", "section": "circular", "section_position": "inside",
        "section_size": "2 mm", "angle": "5 deg"}),
    );
    assert_eq!(cmd, "coil");
    assert_eq!(p["pitch"], json!("(15 mm) / (3)"));
    assert_eq!(p["height"], json!("15 mm"));
    assert_eq!(p["diameter"], json!("(20 mm) - (2 mm)"));
    assert_eq!(p["left"], json!(true));
    assert_eq!(p["angle"], json!("5 deg"), "the taper angle");
    let (_, p) = crate::forms::engine_command(
      &COIL,
      json!({"type": "revolution_pitch", "revolutions": "4", "pitch": "2 mm", "diameter": "10 mm", "section_size": "1 mm"}),
    );
    assert_eq!((p["revolutions"].clone(), p["pitch"].clone(), p.get("height")), (json!("4"), json!("2 mm"), None));
    // the rows each Type shows
    let mut d = Dialog::blank(Kind::Form(&COIL));
    let shown = |d: &Dialog| -> Vec<&str> {
      COIL.inputs.iter().enumerate().filter(|(i, _)| d.form.shown(&COIL, *i)).map(|(_, x)| x.key()).collect()
    };
    assert!(shown(&d).contains(&"height") && !shown(&d).contains(&"pitch"));
    let ty = COIL.inputs.iter().position(|x| x.key() == "type").unwrap();
    d.form.values[ty] = json!("height_pitch");
    assert!(shown(&d).contains(&"pitch") && !shown(&d).contains(&"revolutions"));
    assert!(shown(&d).contains(&"angle"));
    d.form.values[ty] = json!("spiral");
    assert!(!d.ready(), "a spiral coil isn't built");
    assert!(!shown(&d).contains(&"angle"), "a spiral has no taper");
  }

  /// `ui:dialogs:pipe-rows`: Pipe waits for a path (OK greyed, Fusion's prompt), chains the
  /// curves joined to the one picked, and adds the wall only when Hollow is ticked.
  #[test]
  fn pipe_waits_for_a_path_and_chains_it() {
    let mut app = app();
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    app.run("sketch.line", json!({"points": [[0, 0], [30, 0], [30, 20]]})).unwrap();
    app.run("sketch.line", json!({"points": [[50, 50], [60, 50]]})).unwrap();
    app.finish_sketch();
    app.start(Kind::Form(&PIPE));
    let d = app.dialog.as_mut().unwrap();
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select a path to sweep the profile along"));
    let sk = app.s.design.features.iter().find(|f| matches!(f.kind, FeatureKind::Sketch { .. })).unwrap().id;
    let first = app.s.design.sketch(sk).unwrap().curves[0].id;
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: first }];
    let d = app.dialog.as_mut().unwrap();
    d.sync_selection(&app.s);
    assert!(d.ok_enabled());
    let curves = d.form_value("path_curves").and_then(Value::as_array).unwrap().len();
    assert_eq!(curves, 2, "the corner's other line is chained, the loose one isn't");
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, p.get("wall")), ("pipe", None));
    let hollow = PIPE.inputs.iter().position(|x| x.key() == "hollow").unwrap();
    d.form.values[hollow] = json!(true);
    let (_, p) = d.command().unwrap();
    assert_eq!(p["wall"], json!("0.5 mm"));
    d.preview(&mut app.s);
    assert!(d.preview_applied() && d.error.is_none(), "{:?}", d.error);
  }

  #[test]
  fn diameters_round_trip_through_the_engines_radii() {
    assert_eq!(radius_of("20 mm"), "(20 mm) / 2");
    assert_eq!(diameter_of(&radius_of("20 mm")), "20 mm");
    assert_eq!(diameter_of(&radius_of("d1 + 2 mm")), "d1 + 2 mm");
    // radii stored by scripts: a plain value doubles, anything else is twice itself
    assert_eq!(diameter_of("5"), "10");
    assert_eq!(diameter_of("2.5 mm"), "5 mm");
    assert_eq!(diameter_of("r1"), "2 * (r1)");
    for pos in ["center", "inside", "outside"] {
      let major = torus_major("30 mm", "4 mm", pos);
      assert_eq!(torus_inner(&major, &radius_of("4 mm")), ("30 mm".to_string(), pos), "{major}");
    }
    assert_eq!(torus_inner("20", "5"), ("40".to_string(), "center"));
    // a sum isn't taken for a halved diameter
    assert_eq!(diameter_of("(a) / 2 + (b) / 2"), "2 * ((a) / 2 + (b) / 2)");
  }

  #[test]
  fn a_ray_meets_the_plane_where_it_crosses_it() {
    let p = on_plane(&Plane::XY, DVec3::new(3.0, -2.0, 10.0), DVec3::NEG_Z).unwrap();
    assert_eq!(p, [3.0, -2.0]);
    assert!(on_plane(&Plane::XY, DVec3::new(0.0, 0.0, 1.0), DVec3::X).is_none());
  }
}
