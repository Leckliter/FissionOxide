//! Sketch Mirror, Circular Pattern and Rectangular Pattern as Fusion sets them up: the mirror
//! line, the centre point and the pattern directions are selection rows picked in the canvas
//! (the typed centre and the Horizontal/Vertical directions stay as fallbacks). Circular Pattern
//! has Fusion's Type (Full, Angle, Symmetric) and Suppress; both patterns have drag handles
//! (Rectangular's distance arrows, Circular's angle ring) over their live preview.

use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::DVec3;
use fox_engine::sketch::{CurveId, CurveKind, PointId, Sketch};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::manip::{deg, mm};
use super::{Dialog, Kind, sel_chip, sel_curves};
use crate::manipulators::{Triad, TriadEdit};

/// The selection row that takes the next canvas pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Row {
  /// The curves to mirror or pattern.
  #[default]
  Objects,
  /// The mirror line, the centre point or the first direction.
  Ref,
  /// Rectangular Pattern's second direction.
  Ref2,
}

/// Circular Pattern's types, as Fusion names them.
pub(super) const CIRC_TYPES: [(&str, &str); 3] = [("full", "Full"), ("angle", "Angle"), ("symmetric", "Symmetric")];

/// The picked references of the sketch Mirror and pattern dialogs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Refs {
  pub row: Row,
  /// Circular Pattern: the picked centre point (its position is in the dialog's typed centre).
  pub center: Option<PointId>,
  /// Circular Pattern: a circle or arc picked for its centre (kept out of the objects).
  pub center_curve: Option<u32>,
  /// Rectangular Pattern: the lines picked for the two directions.
  pub dirs: [Option<u32>; 2],
  /// Circular Pattern: Suppress is on, and the suppressed slots (see `pattern_circ_suppressed`).
  pub suppress: bool,
  pub suppressed: Vec<usize>,
  /// What was selected the last time the selection was read: a pick is what's new.
  seen: Vec<u32>,
  seen_points: Vec<PointId>,
}

fn edit_sketch(s: &Session) -> Option<&Sketch> {
  s.design.sketch(s.edit_sketch?).ok()
}

fn sel_points(s: &Session) -> Vec<PointId> {
  s.selection.iter().filter_map(|x| if let Sel::SketchPoint { point, .. } = x { Some(*point) } else { None }).collect()
}

fn is_line(k: &Sketch, c: u32) -> bool {
  matches!(k.curve(CurveId(c)).map(|c| &c.kind), Some(CurveKind::Line { .. }))
}

/// A circle's or arc's centre point.
fn centre_of(k: &Sketch, c: u32) -> Option<PointId> {
  match k.curve(CurveId(c)).map(|c| &c.kind) {
    Some(CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. }) => Some(*center),
    _ => None,
  }
}

/// A line's unit direction, start to end.
fn line_dir(k: &Sketch, c: u32) -> Option<[f64; 2]> {
  let CurveKind::Line { a, b } = k.curve(CurveId(c))?.kind else { return None };
  let (a, b) = (k.pos(a)?, k.pos(b)?);
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l = dx.hypot(dy);
  (l > 1e-12).then(|| [dx / l, dy / l])
}

/// A number as a typed coordinate ("12.5", no trailing zeros).
fn coord(v: f64) -> String {
  let s = format!("{:.6}", (v * 1e6).round() / 1e6);
  let s = s.trim_end_matches('0').trim_end_matches('.');
  if s == "-0" { "0".into() } else { s.into() }
}

impl Dialog {
  /// The sketch Mirror / pattern dialog as it opens: the selection so far is the objects (Mirror
  /// takes the last line picked as its mirror line); with objects already picked, the reference
  /// row takes the next pick, as in Fusion.
  pub(super) fn open_sketch_refs(&mut self, s: &Session) {
    self.skp.seen = sel_curves(s);
    self.skp.seen_points = sel_points(s);
    if self.kind == Kind::SkCircPattern {
      self.choice = "full";
    }
    let missing = match self.kind {
      Kind::SkMirror => self.line.is_none(),
      Kind::SkCircPattern => true,
      _ => false,
    };
    if !self.curves.is_empty() && missing {
      self.skp.row = Row::Ref;
    }
  }

  /// Takes canvas picks into the rows: a new pick goes to the active reference row (a line for
  /// a mirror line or direction, a point, circle or arc for a centre), everything else selected
  /// is the objects.
  pub(super) fn sync_sketch_refs(&mut self, s: &Session) {
    let sel = sel_curves(s);
    let pts = sel_points(s);
    let new_curves: Vec<u32> = sel.iter().filter(|c| !self.skp.seen.contains(c)).copied().collect();
    let new_points: Vec<PointId> = pts.iter().filter(|p| !self.skp.seen_points.contains(p)).copied().collect();
    self.skp.seen = sel.clone();
    self.skp.seen_points = pts;
    let Some(k) = edit_sketch(s) else { return };
    let new_line = new_curves.iter().rev().copied().find(|c| is_line(k, *c));
    match (self.kind, self.skp.row) {
      (Kind::SkMirror, Row::Ref) => {
        if let Some(l) = new_line {
          self.line = Some(l);
          self.skp.row = Row::Objects;
          self.dirty = true;
        }
      }
      // a picked point is the centre (the objects are curves); in the centre row a circle or
      // arc gives its centre too
      (Kind::SkCircPattern, row) => {
        let from_curve =
          (row == Row::Ref).then(|| new_curves.iter().rev().find_map(|c| centre_of(k, *c).map(|p| (p, *c)))).flatten();
        if let Some(p) = new_points.last() {
          self.set_centre(k, *p, None);
        } else if let Some((p, c)) = from_curve {
          self.set_centre(k, p, Some(c));
        }
      }
      (Kind::SkRectPattern, Row::Ref | Row::Ref2) => {
        if let Some(l) = new_line {
          let i = usize::from(self.skp.row == Row::Ref2);
          self.skp.dirs[i] = Some(l);
          if i == 1 {
            self.use_dir2 = true;
          }
          self.skp.row = Row::Objects;
          self.dirty = true;
        }
      }
      _ => {}
    }
    let refs = [self.line, self.skp.center_curve, self.skp.dirs[0], self.skp.dirs[1]];
    let mut c = sel;
    c.retain(|x| !refs.contains(&Some(*x)));
    if !c.is_empty() && c != self.curves {
      self.curves = c;
      self.dirty = true;
    }
  }

  fn set_centre(&mut self, k: &Sketch, p: PointId, curve: Option<u32>) {
    let Some(at) = k.pos(p) else { return };
    self.skp.center = Some(p);
    self.skp.center_curve = curve;
    self.at = [coord(at[0]), coord(at[1])];
    self.skp.row = Row::Objects;
    self.dirty = true;
  }

  /// The circular pattern's `sketch.pattern_circ` parameters.
  pub(super) fn circ_params(&self) -> Option<Value> {
    let full = self.choice == "full" || self.choice.is_empty();
    let mut p = json!({
      "curves": self.curves, "center": self.at_pt()?, "count": self.count.trim().parse::<u64>().ok()?,
      "angle": if full { "360 deg" } else { self.angle.as_str() }, "symmetric": self.choice == "symmetric",
    });
    if self.skp.suppress && !self.skp.suppressed.is_empty() {
      p["suppress"] = json!(self.skp.suppressed);
    }
    Some(p)
  }

  /// A rectangular pattern direction: the picked line, else the Horizontal/Vertical choice.
  pub(super) fn rect_dir(&self, i: usize) -> Value {
    if let Some(l) = self.skp.dirs[i] {
      return json!(l);
    }
    let a = if i == 0 { &self.dir } else { &self.dir2 };
    if a == "y" { json!([0.0, 1.0]) } else { json!([1.0, 0.0]) }
  }

  /// The Objects row and a reference row of these dialogs.
  pub(super) fn sketch_ref_rows(&mut self, app: &crate::App, ui: &mut egui::Ui, dirty: &mut bool) {
    let accent = self.accent();
    let row = self.skp.row;
    let n = self.curves.len();
    let objects = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
    if ref_row(ui, accent, "Objects", &objects, n > 0, row == Row::Objects) {
      self.skp.row = Row::Objects;
    }
    match self.kind {
      Kind::SkMirror => {
        let text = if self.line.is_some() { "1 Line" } else { "Select" };
        if ref_row(ui, accent, "Mirror line", text, self.line.is_some(), row == Row::Ref) {
          self.skp.row = Row::Ref;
        }
        // the sketch's lines by name, for a line hard to click
        ui.label("");
        let lines = self.sketch_lines_of(app.s.edit_sketch, app);
        let cur = self.line.map(|l| format!("Sketch line {l}")).unwrap_or("Pick from list".into());
        egui::ComboBox::from_id_salt("mline").selected_text(cur).width(140.0).show_ui(ui, |ui| {
          for (id, label) in lines {
            if ui.selectable_label(self.line == Some(id), label).clicked() {
              self.line = Some(id);
              self.curves.retain(|c| *c != id);
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      Kind::SkCircPattern => {
        let has = self.skp.center.is_some();
        let text = match (self.skp.center, self.skp.center_curve) {
          (Some(_), Some(_)) => "1 Center",
          (Some(_), None) => "1 Point",
          _ => "Select",
        };
        if ref_row(ui, accent, "Center point", text, has, row == Row::Ref) {
          self.skp.row = Row::Ref;
        }
        // the typed centre: the fallback (the sketch origin to start)
        ui.label(if has { "" } else { "Center X, Y" });
        ui.horizontal(|ui| {
          for v in self.at.iter_mut() {
            if ui.add(egui::TextEdit::singleline(v).desired_width(64.0)).changed() {
              *dirty = true;
              self.skp.center = None;
              self.skp.center_curve = None;
            }
          }
        });
        ui.end_row();
      }
      Kind::SkRectPattern => {
        let has = self.skp.dirs[0].is_some();
        if ref_row(ui, accent, "Direction/s", if has { "1 Line" } else { "Select" }, has, row == Row::Ref) {
          self.skp.row = Row::Ref;
        }
        if !has {
          const DIRS: [(&str, &str); 2] = [("x", "Horizontal"), ("y", "Vertical")];
          let mut d1: &'static str = if self.dir == "y" { "y" } else { "x" };
          Self::choose(ui, "", "sdir1", &mut d1, &DIRS, dirty);
          self.dir = d1.into();
        }
      }
      _ => {}
    }
  }

  /// Circular Pattern's Type, Quantity, Angle and Suppress.
  pub(super) fn circ_pattern_fields(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    let mut ty: &'static str = if self.choice.is_empty() { "full" } else { self.choice };
    let before = ty;
    Self::choose(ui, "Type", "sctype", &mut ty, &CIRC_TYPES, dirty);
    if ty != before && before == "full" && self.angle.trim() == "360 deg" {
      // a partial pattern starts at Fusion's 180°
      self.angle = "180 deg".into();
    }
    self.choice = ty;
    Self::field(ui, "Quantity", &mut self.count, dirty);
    if ty != "full" {
      Self::field(ui, "Total angle", &mut self.angle, dirty);
    }
    Self::check(ui, "Suppress", &mut self.skp.suppress, dirty);
    if self.skp.suppress {
      ui.label("");
      let n = self.skp.suppressed.len();
      ui.label(
        egui::RichText::new(if n == 0 {
          "click an instance's box to suppress it".to_string()
        } else {
          format!("{n} suppressed")
        })
        .color(crate::theme::MUTED),
      );
      ui.end_row();
    }
  }

  /// Rectangular Pattern's second direction pick (with its fallback).
  pub(super) fn rect_dir2_row(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    let accent = self.accent();
    let has = self.skp.dirs[1].is_some();
    if ref_row(ui, accent, "Direction 2", if has { "1 Line" } else { "Select" }, has, self.skp.row == Row::Ref2) {
      self.skp.row = Row::Ref2;
    }
    if !has {
      const DIRS: [(&str, &str); 2] = [("x", "Horizontal"), ("y", "Vertical")];
      let mut d2: &'static str = if self.dir2 == "x" { "x" } else { "y" };
      Self::choose(ui, "", "sdir2", &mut d2, &DIRS, dirty);
      self.dir2 = d2.into();
    }
  }

  // ---- drag handles

  /// The sketch plane and the pattern objects' centre (sketch coordinates).
  fn objects_centre(&self, s: &Session) -> Option<(Plane, [f64; 2])> {
    let sk = s.edit_sketch?;
    let plane = s.model.sketches.get(&sk)?.plane;
    let k = s.design.sketch(sk).ok()?;
    let pts: Vec<[f64; 2]> =
      self.curves.iter().flat_map(|c| k.curve_points(CurveId(*c))).filter_map(|p| k.pos(p)).collect();
    if pts.is_empty() {
      return None;
    }
    let n = pts.len() as f64;
    Some((plane, [pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n]))
  }

  /// The two pattern directions in the sketch (unit): the first, and the second (its Vertical
  /// fallback when not in use, so the handles stay apart).
  fn rect_dirs(&self, s: &Session) -> Option<([f64; 2], [f64; 2])> {
    let k = edit_sketch(s)?;
    let pick = |i: usize, fallback: &str| match self.skp.dirs[i] {
      Some(l) => line_dir(k, l),
      None => Some(if fallback == "y" { [0.0, 1.0] } else { [1.0, 0.0] }),
    };
    Some((pick(0, &self.dir)?, pick(1, if self.use_dir2 { &self.dir2 } else { "y" })?))
  }

  /// The handles over a sketch pattern's preview: Rectangular's distance arrows along its
  /// directions from the objects, Circular's angle ring about its centre.
  pub(super) fn sketch_pattern_triad(&self, s: &Session) -> Option<Triad> {
    match self.kind {
      Kind::SkRectPattern => {
        let (plane, c) = self.objects_centre(s)?;
        let (d1, d2) = self.rect_dirs(s)?;
        let w = |d: [f64; 2]| (plane.x * d[0] + plane.y * d[1]).normalize();
        let (w1, w2) = (w(d1), w(d2));
        let home = plane.to_world(c);
        let s1 = self.len(s, &self.spacing)?;
        let s2 = if self.use_dir2 { self.len(s, &self.spacing2)? } else { 0.0 };
        Some(Triad {
          origin: home + w1 * s1 + w2 * s2,
          axes: [w1, w2, plane.normal()],
          arrows: [true, self.use_dir2, false],
          rings: [false; 3],
          planes: [false; 3],
          home,
          angles: [0.0; 3],
        })
      }
      Kind::SkCircPattern => {
        let sk = s.edit_sketch?;
        let plane = s.model.sketches.get(&sk)?.plane;
        let centre = plane.to_world(self.at_pt()?);
        let full = self.choice == "full" || self.choice.is_empty();
        let angle = if full { std::f64::consts::TAU } else { self.ang(s, &self.angle)? };
        Some(Triad {
          origin: centre,
          axes: [plane.x, plane.y, plane.normal()],
          arrows: [false; 3],
          rings: [false, false, true],
          planes: [false; 3],
          home: centre,
          angles: [0.0, 0.0, angle],
        })
      }
      _ => None,
    }
  }

  /// Takes a handle drag into the fields: distances along the directions, or the total angle
  /// (a Full pattern dragged becomes an Angle one).
  pub(super) fn set_sketch_pattern_triad(&mut self, s: &Session, edit: TriadEdit) -> bool {
    match (self.kind, edit) {
      (Kind::SkRectPattern, TriadEdit::Move(p)) => {
        let Some((plane, c)) = self.objects_centre(s) else { return false };
        let Some((d1, d2)) = self.rect_dirs(s) else { return false };
        let w = |d: [f64; 2]| (plane.x * d[0] + plane.y * d[1]).normalize();
        let (w1, w2) = (w(d1), w(d2));
        let delta: DVec3 = p - plane.to_world(c);
        let (a, b) = if self.use_dir2 {
          // delta = a·w1 + b·w2 (the directions needn't be square)
          let (g11, g12, g22) = (w1.dot(w1), w1.dot(w2), w2.dot(w2));
          let (r1, r2) = (delta.dot(w1), delta.dot(w2));
          let det = g11 * g22 - g12 * g12;
          if det.abs() < 1e-12 {
            return false;
          }
          ((r1 * g22 - r2 * g12) / det, (r2 * g11 - r1 * g12) / det)
        } else {
          (delta.dot(w1), 0.0)
        };
        self.spacing = mm(self.unit, a.max(1e-3));
        if self.use_dir2 {
          self.spacing2 = mm(self.unit, b.max(1e-3));
        }
        true
      }
      (Kind::SkCircPattern, TriadEdit::Rotate(_, a)) => {
        if a.abs() < 1e-6 {
          return false;
        }
        if self.choice == "full" || self.choice.is_empty() {
          self.choice = "angle";
        }
        self.angle = deg(a.clamp(-std::f64::consts::TAU, std::f64::consts::TAU));
        true
      }
      _ => false,
    }
  }

  // ---- suppress

  /// Circular Pattern with Suppress on: each instance's slot, where it lands (sketch
  /// coordinates) and whether it is suppressed; with the sketch plane.
  pub fn suppress_marks(&self, s: &Session) -> Option<(Plane, Vec<([f64; 2], usize, bool)>)> {
    if self.kind != Kind::SkCircPattern || !self.skp.suppress || self.curves.is_empty() {
      return None;
    }
    let (plane, c0) = self.objects_centre(s)?;
    let centre = self.at_pt()?;
    let n = self.count.trim().parse::<usize>().ok().filter(|n| (1..=360).contains(n))?;
    let full = self.choice == "full" || self.choice.is_empty();
    let angle = if full { std::f64::consts::TAU } else { self.ang(s, &self.angle)? };
    let full = full || (angle.abs() - std::f64::consts::TAU).abs() < 1e-9;
    let step = if full || n < 2 { angle / n as f64 } else { angle / (n - 1) as f64 };
    let back = if self.choice == "symmetric" && !full { (n - 1) / 2 } else { 0 };
    let marks = (0..n)
      .filter(|k| *k != back)
      .map(|k| {
        let (sn, cs) = (step * (k as f64 - back as f64)).sin_cos();
        let v = [c0[0] - centre[0], c0[1] - centre[1]];
        let at = [centre[0] + cs * v[0] - sn * v[1], centre[1] + sn * v[0] + cs * v[1]];
        (at, k, self.skp.suppressed.contains(&k))
      })
      .collect();
    Some((plane, marks))
  }

  /// A click on an instance's box: suppresses it, or brings it back.
  pub fn toggle_suppressed(&mut self, slot: usize) {
    if let Some(i) = self.skp.suppressed.iter().position(|k| *k == slot) {
      self.skp.suppressed.remove(i);
    } else {
      self.skp.suppressed.push(slot);
      self.skp.suppressed.sort_unstable();
    }
    self.dirty = true;
  }
}

/// A selection row: its label and chip; the active row (the one the next canvas pick goes to) is
/// outlined in the accent. True when the chip was clicked to make it active.
fn ref_row(ui: &mut egui::Ui, accent: egui::Color32, label: &str, text: &str, has: bool, active: bool) -> bool {
  ui.label(label);
  let r = sel_chip(ui, accent, text, has);
  if active {
    let chip = egui::Rect::from_min_size(r.rect.min, egui::vec2(r.rect.width() - 20.0, r.rect.height()));
    ui.painter().rect_stroke(chip.expand(1.0), 4.0, egui::Stroke::new(2.0, accent), egui::StrokeKind::Outside);
  }
  let clicked = r.interact(egui::Sense::click()).on_hover_text("Select in the canvas for this row").clicked();
  ui.end_row();
  clicked
}

#[cfg(test)]
mod tests {
  use super::*;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    let v = s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    assert!(v.get("error").is_none_or(Value::is_null), "{cmd}: {v}");
    v
  }

  fn id(v: &Value) -> u32 {
    v["curves"][0].as_u64().unwrap() as u32
  }

  fn pick_curve(s: &mut Session, c: u32) {
    let sketch = s.edit_sketch.unwrap();
    s.selection.push(Sel::SketchCurve { sketch, curve: CurveId(c) });
  }

  fn count_curves(s: &Session) -> usize {
    edit_sketch(s).unwrap().curves.len()
  }

  /// Applies the dialog's command as its preview would.
  fn preview(s: &mut Session, d: &mut Dialog) {
    d.preview(s);
    assert!(d.applied, "{:?}", d.error);
  }

  #[test]
  fn mirror_line_is_picked_in_the_canvas() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let circle = id(&run(&mut s, "sketch.circle", json!({"center": [10, 5], "radius": 2})));
    let axis = id(&run(&mut s, "sketch.line", json!({"points": [[0, -20], [0, 20]]})));
    // the circle is picked first, then the dialog opens: the Mirror line row takes the next pick
    pick_curve(&mut s, circle);
    let mut d = Dialog::blank(Kind::SkMirror);
    d.curves = sel_curves(&s);
    d.open_sketch_refs(&s);
    assert_eq!(d.skp.row, Row::Ref);
    assert!(!d.ready());
    pick_curve(&mut s, axis);
    d.sync_sketch_refs(&s);
    assert_eq!(d.line, Some(axis));
    assert_eq!(d.curves, vec![circle], "the mirror line isn't an object");
    assert_eq!(d.skp.row, Row::Objects, "back to the objects once the line is in");
    let before = count_curves(&s);
    preview(&mut s, &mut d);
    assert_eq!(count_curves(&s), before + 1);
    // the copy sits across the picked line
    let k = edit_sketch(&s).unwrap();
    let copy = k.curves.last().unwrap();
    let c = k.pos(centre_of(k, copy.id.0).unwrap()).unwrap();
    assert!((c[0] + 10.0).abs() < 1e-6 && (c[1] - 5.0).abs() < 1e-6, "{c:?}");
  }

  #[test]
  fn circular_pattern_centre_is_a_picked_point_with_type_and_suppress() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let hub = id(&run(&mut s, "sketch.circle", json!({"center": [30, 20], "radius": 5})));
    let peg = id(&run(&mut s, "sketch.circle", json!({"center": [40, 20], "radius": 1})));
    pick_curve(&mut s, peg);
    let mut d = Dialog::blank(Kind::SkCircPattern);
    d.curves = sel_curves(&s);
    d.count = "4".into();
    d.open_sketch_refs(&s);
    assert_eq!((d.choice, d.skp.row), ("full", Row::Ref));
    // picking the hub circle in the centre row gives its centre
    pick_curve(&mut s, hub);
    d.sync_sketch_refs(&s);
    assert_eq!(d.at, ["30".to_string(), "20".to_string()]);
    assert_eq!(d.skp.center_curve, Some(hub));
    assert_eq!(d.curves, vec![peg], "the centre circle isn't an object");
    let p = d.circ_params().unwrap();
    assert_eq!(
      (p["center"].clone(), p["angle"].clone(), p["symmetric"].clone()),
      (json!([30.0, 20.0]), json!("360 deg"), json!(false))
    );
    // a picked sketch point is the centre too
    let k = edit_sketch(&s).unwrap();
    let hub_centre = centre_of(k, hub).unwrap();
    s.selection.retain(|x| !matches!(x, Sel::SketchCurve { curve, .. } if curve.0 == hub));
    s.selection.push(Sel::SketchPoint { sketch: s.edit_sketch.unwrap(), point: fox_engine::sketch::ORIGIN });
    d.sync_sketch_refs(&s);
    assert_eq!(d.skp.center, Some(fox_engine::sketch::ORIGIN));
    assert_eq!(d.at, ["0".to_string(), "0".to_string()]);
    s.selection.retain(|x| !matches!(x, Sel::SketchPoint { .. }));
    s.selection.push(Sel::SketchPoint { sketch: s.edit_sketch.unwrap(), point: hub_centre });
    d.sync_sketch_refs(&s);
    assert_eq!(d.at, ["30".to_string(), "20".to_string()]);

    // Type: Angle and Symmetric spread over the typed angle
    d.choice = "symmetric";
    d.angle = "90 deg".into();
    let p = d.circ_params().unwrap();
    assert_eq!((p["angle"].clone(), p["symmetric"].clone()), (json!("90 deg"), json!(true)));
    d.choice = "full";

    // Suppress: the instances' boxes sit where the copies go; a suppressed one isn't made
    d.skp.suppress = true;
    let (_, marks) = d.suppress_marks(&s).unwrap();
    let slots: Vec<usize> = marks.iter().map(|m| m.1).collect();
    assert_eq!(slots, vec![1, 2, 3]);
    let at2 = marks[1].0;
    assert!((at2[0] - 20.0).abs() < 1e-6 && (at2[1] - 20.0).abs() < 1e-6, "{at2:?}");
    d.toggle_suppressed(2);
    let before = count_curves(&s);
    preview(&mut s, &mut d);
    assert_eq!(count_curves(&s), before + 2, "slot 2 suppressed");

    // the angle ring sits on the centre; dragging it makes an Angle pattern
    let t = d.sketch_pattern_triad(&s).unwrap();
    assert!((t.origin - DVec3::new(30.0, 20.0, 0.0)).length() < 1e-6);
    assert_eq!(t.rings, [false, false, true]);
    assert!(d.set_sketch_pattern_triad(&s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_2)));
    assert_eq!((d.choice, d.angle.as_str()), ("angle", "90 deg"));
  }

  #[test]
  fn rectangular_pattern_directions_are_picked_lines_with_drag_arrows() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let peg = id(&run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 1})));
    let slope = id(&run(&mut s, "sketch.line", json!({"points": [[10, 0], [13, 4]]})));
    let up = id(&run(&mut s, "sketch.line", json!({"points": [[20, 0], [20, 5]]})));
    pick_curve(&mut s, peg);
    let mut d = Dialog::blank(Kind::SkRectPattern);
    d.curves = sel_curves(&s);
    d.count = "3".into();
    d.spacing = "10 mm".into();
    d.open_sketch_refs(&s);
    assert_eq!(d.skp.row, Row::Objects);
    // the typed fallback: Horizontal
    assert_eq!(d.rect_dir(0), json!([1.0, 0.0]));
    d.skp.row = Row::Ref;
    pick_curve(&mut s, slope);
    d.sync_sketch_refs(&s);
    assert_eq!(d.skp.dirs[0], Some(slope));
    assert_eq!(d.rect_dir(0), json!(slope));
    assert_eq!(d.curves, vec![peg]);
    d.skp.row = Row::Ref2;
    pick_curve(&mut s, up);
    d.sync_sketch_refs(&s);
    assert_eq!((d.skp.dirs[1], d.use_dir2), (Some(up), true));
    assert_eq!(d.curves, vec![peg]);
    let before = count_curves(&s);
    preview(&mut s, &mut d);
    assert_eq!(count_curves(&s), before + 3 * 2 - 1);

    // the arrows run along the picked lines from the objects; dragging sets the distances
    let t = d.sketch_pattern_triad(&s).unwrap();
    assert_eq!(t.arrows, [true, true, false]);
    assert!((t.axes[0] - DVec3::new(0.6, 0.8, 0.0)).length() < 1e-9);
    let to = t.home + t.axes[0] * 15.0 + t.axes[1] * 7.0;
    assert!(d.set_sketch_pattern_triad(&s, TriadEdit::Move(to)));
    assert_eq!((d.spacing.as_str(), d.spacing2.as_str()), ("15 mm", "7 mm"));
  }
}
