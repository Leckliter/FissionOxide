//! Move/Copy, as Fusion's Move dialog: what moves (Move Object: bodies, a component, faces or
//! sketch objects), how (Move Type: Free Move with the triad's arrows, rings and planes,
//! Translate, Rotate about an axis, Point to Point, Point to Position) and Create Copy. Also
//! where the triad sits for each, and what its drags set.
//!
//! Free Move turns about the picked objects' centre (X, then Y, then Z angles) and then moves
//! by the X/Y/Z distances, so the distances are how far the centre goes and the triad's value
//! boxes read the same as the fields. Its triad also turns about any axis (the view ring and
//! trackball), which comes back as the X, Y and Z angles of the same turn. Rotate turns about a
//! world axis, a construction axis or a straight edge picked in the viewport, its ring on that
//! axis.

use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::{DQuat, DVec3};
use fox_engine::model::{BodyId, BodySel, ComponentId, FeatureId, FeatureKind};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::manip::{axis_line, centre, deg, mm};
use super::{Dialog, sel_chip, value_edit};
use crate::App;
use crate::manipulators::{Triad, TriadEdit, from_xyz, xyz_of};

/// Move Object choices.
const OBJECTS: [(&str, &str); 4] =
  [("bodies", "Bodies"), ("components", "Components"), ("faces", "Faces"), ("sketch", "Sketch Objects")];
/// Move Type choices.
const TYPES: [(&str, &str); 5] = [
  ("free", "Free Move"),
  ("translate", "Translate"),
  ("rotate", "Rotate"),
  ("point_to_point", "Point to Point"),
  ("point_to_position", "Point to Position"),
];
const AXES: [&str; 3] = ["x", "y", "z"];

/// What a Paste (Ctrl+V) placed with this dialog is. Fusion opens Initial Position on a pasted
/// component and Move/Copy on pasted bodies and sketch curves; Cancel removes the paste.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Paste {
  /// Another occurrence of a component (Paste): `component.copy`, rebuilt from the original.
  Occurrence,
  /// An independent copy of a component (Paste New): its bodies are copied, and OK puts the
  /// copies in a new component named after it (under the same parent, of the same material).
  /// `before` are the bodies there were before.
  New { source: u32, before: Vec<u32> },
  /// Copies of bodies, or of sketch curves in their sketch.
  Copy,
}
const WORLD: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

/// Move/Copy's own inputs (the distances and the Rotate angle and axis are the dialog's
/// `translate`, `angle` and `axis`).
#[derive(Debug, Clone)]
pub(super) struct MoveForm {
  pub(super) object: &'static str,
  pub(super) mtype: &'static str,
  pub(super) copy: bool,
  /// Free Move's X, Y and Z angles.
  pub(super) rotate: [String; 3],
  /// Point to Position: where the picked point goes.
  pub(super) position: [String; 3],
  /// Picked points in order (Point to Point: from, to), with where they were when picked.
  pub(super) points: Vec<(Sel, DVec3)>,
  /// Sketch objects: the sketch being edited and its plane.
  pub(super) sketch: Option<(FeatureId, Plane)>,
  /// The picks (as [`Dialog::move_key`]) and their centre before the move: the triad's home and
  /// Free Move's pivot.
  pub(super) pivot: Option<(Value, DVec3)>,
  /// Opened by Paste: what it placed (Create Copy stays on and what moves can't change).
  pub(super) paste: Option<Paste>,
}

impl Default for MoveForm {
  fn default() -> MoveForm {
    let zeros = |u: &str| [format!("0 {u}"), format!("0 {u}"), format!("0 {u}")];
    MoveForm {
      object: "bodies",
      mtype: "free",
      copy: false,
      rotate: zeros("deg"),
      position: zeros("mm"),
      points: Vec::new(),
      sketch: None,
      pivot: None,
      paste: None,
    }
  }
}

/// A Move dialog's axis choice as the command takes it (`edge`: the picked edge).
fn axis_json(a: &str, edge: &Value) -> Option<Value> {
  Some(match (a.strip_prefix("line:"), a.strip_prefix("axis:")) {
    (Some(n), _) => json!({"line": n.parse::<u32>().ok()?}),
    (_, Some(n)) => json!({"axis": n.parse::<u32>().ok()?}),
    _ if a == "edge" => Some(edge.clone()).filter(|e| !e.is_null())?,
    _ => json!(a),
  })
}

fn is_zero(e: &str) -> bool {
  let t = e.trim();
  t.is_empty() || t.trim_end_matches("deg").trim_end_matches("mm").trim().parse::<f64>().is_ok_and(|v| v == 0.0)
}

/// Where a picked vertex or sketch point is.
fn point_of(s: &Session, sel: &Sel) -> Option<DVec3> {
  match sel {
    Sel::Vertex { body, edge, end } => fox_engine::scene::vertex_pos(s, *body, *edge, *end),
    Sel::SketchPoint { sketch, point } => {
      let r = s.model.sketches.get(sketch)?;
      Some(r.plane.to_world(r.sketch.pos(*point)?))
    }
    _ => None,
  }
}

impl Dialog {
  /// Sets the dialog up to place a paste: `object` is Move Object ("components", "bodies" or
  /// "sketch"), taken from the session's selection (the pasted things).
  pub(crate) fn set_paste(&mut self, s: &Session, paste: Paste, object: &'static str) {
    self.mv.object = object;
    self.mv.mtype = "free";
    self.mv.copy = true;
    self.mv.pivot = None;
    self.mv.paste = Some(paste);
    self.sync_move(s);
    self.dirty = true;
  }

  /// What Paste placed with this dialog, if it did.
  #[cfg(test)]
  pub(crate) fn paste(&self) -> Option<&Paste> {
    self.mv.paste.as_ref()
  }

  /// The header: Fusion's Initial Position for a pasted component.
  pub(super) fn move_title(&self) -> Option<&'static str> {
    match self.mv.paste {
      Some(Paste::Occurrence | Paste::New { .. }) => Some("INITIAL POSITION"),
      _ => None,
    }
  }

  /// After OK: Paste New's copied bodies become a component of their own.
  pub(crate) fn finish_paste(&self, app: &mut App) {
    let Some(Paste::New { source, before }) = &self.mv.paste else { return };
    let Some(src) = app.s.design.component(ComponentId(*source)).cloned() else { return };
    let bodies: Vec<u32> = app.s.model.bodies.iter().map(|b| b.id.0).filter(|b| !before.contains(b)).collect();
    if bodies.is_empty() {
      return;
    }
    let taken = |n: &str| app.s.design.components.iter().any(|c| c.name == n);
    let name = (1..).map(|k| format!("{} ({k})", src.name)).find(|n| !taken(n)).unwrap_or_default();
    let made = app.run("component.create", json!({"name": name, "bodies": bodies, "parent": src.parent.map(|p| p.0)}));
    if let Some(c) = made.and_then(|r| r["component"].as_u64())
      && !src.material.is_empty()
    {
      app.run("component.material", json!({"component": c, "material": src.material}));
    }
  }

  /// Edit Feature on a Move: its type follows what it stores.
  pub(super) fn load_move(&mut self, kind: &FeatureKind) {
    let FeatureKind::Move { bodies, translate, angle, rotate, copy, axis, .. } = kind else { return };
    self.translate = translate.clone();
    self.angle = angle.clone();
    if let Some(a) = axis {
      self.axis = super::axis_key(a);
    }
    self.mv.copy = *copy;
    self.mv.object = if matches!(bodies, BodySel::Component { .. }) { "components" } else { "bodies" };
    self.mv.mtype = match rotate {
      Some(r) => {
        self.mv.rotate = r.clone();
        "free"
      }
      None if axis.is_some() && !is_zero(angle) => "rotate",
      None => "translate",
    };
  }

  /// The picked objects, to tell when the triad's centre must be found again.
  fn move_key(&self) -> Value {
    match self.mv.object {
      "faces" => json!(["faces", self.faces]),
      "sketch" => json!(["sketch", self.curves]),
      o => json!([o, self.bodies]),
    }
  }

  fn move_picked(&self) -> bool {
    match self.mv.object {
      "faces" => !self.faces.is_empty(),
      "sketch" => !self.curves.is_empty() && self.mv.sketch.is_some(),
      "components" => self.comps.first().is_some_and(Option::is_some),
      _ => !self.bodies.is_empty(),
    }
  }

  /// Points the type needs: Point to Point two, Point to Position one.
  fn points_needed(&self) -> usize {
    match self.mv.mtype {
      "point_to_point" => 2,
      "point_to_position" => 1,
      _ => 0,
    }
  }

  /// Does Move turn anything?
  fn move_turns(&self) -> bool {
    match self.mv.mtype {
      "free" => self.mv.rotate.iter().any(|a| !is_zero(a)),
      "rotate" => !is_zero(&self.angle),
      _ => false,
    }
  }

  pub(super) fn move_ready(&self) -> bool {
    self.move_picked() && self.mv.points.len() >= self.points_needed() && self.move_missing().is_none()
  }

  /// What Move still needs, or what it can't do.
  pub(super) fn move_missing(&self) -> Option<&'static str> {
    if !self.move_picked() {
      return Some(match self.mv.object {
        "faces" => "select faces",
        "sketch" => "select sketch curves (in an open sketch)",
        "components" => "select a body of a component",
        _ => "select bodies",
      });
    }
    if self.mv.points.len() < self.points_needed() {
      return Some(if self.points_needed() == 2 && self.mv.points.is_empty() {
        "select the point to move from, then the point to move to"
      } else if self.points_needed() == 2 {
        "select the point to move to"
      } else {
        "select the point to move"
      });
    }
    if self.mv.object == "faces" && self.move_turns() {
      return Some("faces can only be moved, not turned");
    }
    if self.mv.object == "components" && self.mv.copy && self.move_turns() {
      return Some("a copied component can only be moved, not turned");
    }
    None
  }

  /// Takes the selection: bodies (and their components), faces, sketch curves and points.
  pub(super) fn sync_move(&mut self, s: &Session) {
    match self.mv.object {
      "faces" => {
        let f = super::sel_faces(s);
        if !f.is_empty() && f != self.faces {
          self.faces = f;
          self.dirty = true;
        }
      }
      "sketch" => {
        self.mv.sketch = s.edit_sketch.and_then(|sk| Some((sk, s.model.sketches.get(&sk)?.plane)));
        let c = super::sel_curves(s);
        if !c.is_empty() && c != self.curves {
          self.curves = c;
          self.dirty = true;
        }
      }
      _ => {
        // Rotate: a straight edge or construction axis picked is the axis, not something to move
        let rotate = self.mv.mtype == "rotate";
        if rotate {
          self.sync_revolve_axis(s);
        }
        let b: Vec<u32> = if rotate {
          let mut b: Vec<u32> = Vec::new();
          for x in &s.selection {
            if let Sel::Body { body } | Sel::Face { body, .. } = x
              && !b.contains(&body.0)
            {
              b.push(body.0);
            }
          }
          b
        } else {
          super::sel_bodies(s)
        };
        if !b.is_empty() && b != self.bodies {
          self.comps = b.iter().map(|x| s.design.component_of(BodyId(*x)).map(|c| c.id.0)).collect();
          self.bodies = b;
          self.dirty = true;
        }
      }
    }
    let need = self.points_needed();
    if need == 0 {
      return;
    }
    for sel in &s.selection {
      if self.mv.points.iter().any(|(x, _)| x == sel) {
        continue;
      }
      let Some(p) = point_of(s, sel) else { continue };
      self.mv.points.push((*sel, p));
      if self.mv.points.len() > need {
        self.mv.points.remove(0);
      }
      if need == 1 {
        // Point to Position starts where the point is
        let p = self.local(p);
        self.mv.position = [mm(self.unit, p.x), mm(self.unit, p.y), mm(self.unit, p.z)];
      }
      self.dirty = true;
    }
  }

  /// A world point in the coordinates Move's fields use: the sketch's for sketch objects (z
  /// along its normal), else the world's.
  fn local(&self, p: DVec3) -> DVec3 {
    match (self.mv.object, self.mv.sketch) {
      ("sketch", Some((_, pl))) => {
        let [x, y] = pl.to_local(p);
        DVec3::new(x, y, (p - pl.origin).dot(pl.normal()))
      }
      _ => p,
    }
  }

  /// The picked objects' centre as they are now.
  fn move_centre(&self, s: &Session) -> Option<DVec3> {
    match self.mv.object {
      "faces" => {
        let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
        let cs: Vec<DVec3> = self
          .faces
          .iter()
          .filter_map(|f| {
            let b = s.model.body(BodyId(get(f, "body")?))?;
            let id = get(f, "face")?;
            b.faces().iter().find(|x| x.id == id).map(|x| x.center)
          })
          .collect();
        (!cs.is_empty()).then(|| cs.iter().sum::<DVec3>() / cs.len() as f64)
      }
      "sketch" => {
        let (sk, pl) = self.mv.sketch?;
        let r = s.model.sketches.get(&sk)?;
        let mut pts = Vec::new();
        for c in &self.curves {
          for p in r.sketch.curve_points(fox_engine::sketch::CurveId(*c)) {
            if !pts.contains(&p) {
              pts.push(p);
            }
          }
        }
        let xy: Vec<[f64; 2]> = pts.iter().filter_map(|p| r.sketch.pos(*p)).collect();
        if xy.is_empty() {
          return None;
        }
        let n = xy.len() as f64;
        Some(pl.to_world([xy.iter().map(|p| p[0]).sum::<f64>() / n, xy.iter().map(|p| p[1]).sum::<f64>() / n]))
      }
      "components" => {
        let c = s.design.component(ComponentId((*self.comps.first()?)?))?;
        centre(s, &c.bodies.iter().map(|b| b.0).collect::<Vec<_>>())
      }
      _ => centre(s, &self.bodies),
    }
  }

  /// Where the move takes a point, for Free Move and Translate (the centre doesn't turn) and
  /// Rotate about X, Y or Z; None for a rotation about some other axis or missing points.
  fn move_point(&self, s: &Session, c: DVec3) -> Option<DVec3> {
    if self.mv.mtype == "rotate" {
      let a = self.ang(s, &self.angle)?;
      if self.mv.object == "sketch" {
        return Some(c);
      }
      let i = AXES.iter().position(|x| *x == self.axis)?;
      return Some(DQuat::from_axis_angle(WORLD[i], a) * c);
    }
    Some(c + self.move_vector(s)?)
  }

  /// The move's translation in world space.
  fn move_vector(&self, s: &Session) -> Option<DVec3> {
    let p = |i: usize| self.mv.points.get(i).map(|x| x.1);
    let v = match self.mv.mtype {
      "rotate" => return Some(DVec3::ZERO),
      "point_to_point" => return Some(p(1)? - p(0)?),
      "point_to_position" => {
        let to = [0, 1, 2].map(|i| self.len(s, &self.mv.position[i]));
        let to = DVec3::new(to[0]?, to[1]?, if self.mv.object == "sketch" { 0.0 } else { to[2]? });
        to - self.local(p(0)?)
      }
      _ => {
        let t = [0, 1, 2].map(|i| self.len(s, &self.translate[i]));
        DVec3::new(t[0]?, t[1]?, if self.mv.object == "sketch" { 0.0 } else { t[2]? })
      }
    };
    Some(match (self.mv.object, self.mv.sketch) {
      ("sketch", Some((_, pl))) => pl.x * v.x + pl.y * v.y,
      ("sketch", None) => return None,
      _ => v,
    })
  }

  /// Finds the picks' centre before the move (the viewport calls it every frame).
  pub(super) fn refresh_move(&mut self, s: &Session) {
    let key = self.move_key();
    if self.mv.pivot.as_ref().is_some_and(|p| p.0 == key) {
      return;
    }
    let Some(now) = self.move_centre(s) else {
      self.mv.pivot = None;
      return;
    };
    // a showing preview has moved the picks already (unless it copied them): take it back out
    let before = if self.applied && !self.mv.copy {
      match self.mv.mtype {
        "rotate" if self.mv.object != "sketch" => match (self.move_axis(s), self.ang(s, &self.angle)) {
          (Some((o, d)), Some(a)) => o + DQuat::from_axis_angle(d, -a) * (now - o),
          _ => now,
        },
        _ => self.move_vector(s).map_or(now, |v| now - v),
      }
    } else {
      now
    };
    self.mv.pivot = Some((key, before));
  }

  /// The triad Move shows: Free Move's arrows, rings and planes on the picks' centre (a sketch's
  /// in its plane), Translate's without rings, Rotate's single ring on its axis.
  pub(super) fn move_triad(&self, s: &Session) -> Option<Triad> {
    let (key, c) = self.mv.pivot.as_ref()?;
    if *key != self.move_key() || !self.move_picked() {
      return None;
    }
    let c = *c;
    let sketch = self.mv.object == "sketch";
    let a = |e: &String| self.ang(s, e);
    match self.mv.mtype {
      m @ ("free" | "translate") => {
        let free = m == "free";
        let mut t = Triad::world(self.move_point(s, c)?);
        t.home = c;
        match self.mv.object {
          "sketch" => {
            let (_, pl) = self.mv.sketch?;
            t.axes = [pl.x, pl.y, pl.normal()];
            t.arrows = [true, true, false];
            t.planes = [false, false, true];
            t.rings = [false, false, free];
            if free {
              t.angles[2] = a(&self.mv.rotate[2])?;
            }
          }
          "faces" => t.rings = [false; 3],
          _ => {
            t.rings = [free; 3];
            if free {
              t.angles = [a(&self.mv.rotate[0])?, a(&self.mv.rotate[1])?, a(&self.mv.rotate[2])?];
              // and the view ring and trackball, which turn about any axis
              t.free = true;
              t.orient = from_xyz(t.angles);
            }
          }
        }
        Some(t)
      }
      "rotate" if self.mv.object != "faces" => {
        let angle = a(&self.angle)?;
        let (origin, axes, i) = if sketch {
          let (_, pl) = self.mv.sketch?;
          (c, [pl.x, pl.y, pl.normal()], 2)
        } else if let Some(i) = AXES.iter().position(|x| *x == self.axis) {
          (WORLD[i] * c.dot(WORLD[i]), WORLD, i)
        } else {
          // any other axis: the ring on it, level with the centre
          let (o, d) = self.move_axis(s)?;
          let x = d.any_orthonormal_vector();
          (o + d * (c - o).dot(d), [x, d.cross(x), d], 2)
        };
        let mut rings = [false; 3];
        rings[i] = true;
        let mut angles = [0.0; 3];
        angles[i] = angle;
        Some(Triad { rings, arrows: [false; 3], planes: [false; 3], axes, angles, ..Triad::world(origin) })
      }
      _ => None,
    }
  }

  /// Rotate's axis as a world line (a point and unit direction): a world or construction axis, or
  /// the picked straight edge.
  pub(super) fn move_axis(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    if self.axis == "edge" {
      let (a, b) = super::hole_pattern::line_of(s, &self.solid.axis_edge)?;
      return Some((a, (b - a).try_normalize()?));
    }
    axis_line(s, &self.axis)
  }

  /// Takes a triad drag into Move's fields; false when the edit doesn't apply.
  pub(super) fn set_move_triad(&mut self, edit: TriadEdit) -> bool {
    let sketch = self.mv.object == "sketch";
    match (self.mv.mtype, edit) {
      ("free" | "translate", TriadEdit::Move(p)) => {
        let Some((_, c)) = self.mv.pivot.clone() else { return false };
        let d = self.local(p) - self.local(c);
        self.translate =
          [mm(self.unit, d.x), mm(self.unit, d.y), if sketch { self.translate[2].clone() } else { mm(self.unit, d.z) }];
      }
      ("free", TriadEdit::Rotate(i, a)) => self.mv.rotate[if sketch { 2 } else { i }] = deg(a),
      // a turn about any axis, as the X, Y and Z angles of the same turn
      ("free", TriadEdit::Orient(q)) if !sketch => self.mv.rotate = xyz_of(q).map(deg),
      ("rotate", TriadEdit::Rotate(i, a)) => {
        if !sketch && AXES.contains(&self.axis.as_str()) {
          self.axis = AXES[i].into();
        }
        self.angle = deg(a);
      }
      _ => return false,
    }
    true
  }

  /// The translation as the command takes it: expressions, in the sketch's coordinates for
  /// sketch objects.
  fn move_translate(&self) -> Option<[String; 3]> {
    let p = |i: usize| self.mv.points.get(i).map(|x| self.local(x.1));
    Some(match self.mv.mtype {
      "rotate" => ["0 mm".into(), "0 mm".into(), "0 mm".into()],
      "point_to_point" => {
        let d = p(1)? - p(0)?;
        [mm(self.unit, d.x), mm(self.unit, d.y), mm(self.unit, d.z)]
      }
      "point_to_position" => {
        let from = p(0)?.to_array();
        [0, 1, 2].map(|i| format!("({}) - ({})", self.mv.position[i], mm(self.unit, from[i])))
      }
      _ => self.translate.clone(),
    })
  }

  /// The command Move/Copy applies.
  pub(super) fn move_command(&self) -> Option<(&'static str, Value)> {
    let t = self.move_translate()?;
    let (free, rotate) = (self.mv.mtype == "free", self.mv.mtype == "rotate");
    match self.mv.object {
      "faces" => Some(("direct.move_face", json!({"faces": self.faces, "translate": t}))),
      "sketch" => {
        let (sk, pl) = self.mv.sketch?;
        let angle = if free {
          self.mv.rotate[2].clone()
        } else if rotate {
          self.angle.clone()
        } else {
          "0 deg".into()
        };
        let mut p = json!({"sketch": sk.0, "curves": self.curves, "translate": [t[0], t[1]], "angle": angle});
        if let Some((_, c)) = &self.mv.pivot {
          p["center"] = json!(pl.to_local(*c));
        }
        if self.mv.copy {
          p["from"] = json!({"sketch": sk.0});
          return Some(("sketch.insert", p));
        }
        Some(("sketch.transform", p))
      }
      o => {
        let mut p = if o == "components" {
          let c = (*self.comps.first()?)?;
          if self.mv.copy && !self.move_turns() {
            return Some(("component.copy", json!({"component": c, "translate": t})));
          }
          json!({"component": c, "translate": t})
        } else {
          json!({"bodies": self.bodies, "translate": t})
        };
        if free && self.move_turns() {
          p["rotate"] = json!(self.mv.rotate);
          p["pivot"] = json!(self.mv.pivot.as_ref()?.1.to_array());
        }
        if rotate && self.move_turns() {
          p["axis"] = axis_json(&self.axis, &self.solid.axis_edge)?;
          p["angle"] = json!(self.angle);
        }
        if self.mv.copy {
          p["copy"] = json!(true);
        }
        Some(("move", p))
      }
    }
  }

  /// Fields for `feature.edit` on a Move.
  pub(super) fn move_edit_set(&self) -> Value {
    let mut set = json!({"translate": self.translate, "angle": self.angle, "copy": self.mv.copy});
    if self.mv.mtype == "free" {
      set["rotate"] = json!(self.mv.rotate);
    }
    set
  }

  fn move_combo(&mut self, ui: &mut egui::Ui, id: &str, items: &[(&'static str, &'static str)], types: bool) {
    let cur = if types { self.mv.mtype } else { self.mv.object };
    let label = items.iter().find(|(k, _)| *k == cur).map_or(cur, |(_, l)| *l);
    let mut pick = None;
    ui.add_enabled_ui(self.editing.is_none() && (types || self.mv.paste.is_none()), |ui| {
      egui::ComboBox::from_id_salt(id).selected_text(label).width(140.0).show_ui(ui, |ui| {
        for (k, l) in items {
          if ui.selectable_label(cur == *k, *l).clicked() && cur != *k {
            pick = Some(*k);
          }
        }
      });
    });
    let Some(k) = pick else { return };
    if types {
      self.mv.mtype = k;
    } else {
      self.mv.object = k;
      self.mv.pivot = None;
    }
    self.mv.points.clear();
    self.dirty = true;
  }

  fn move_value(&mut self, ui: &mut egui::Ui, label: &str, which: (u8, usize)) {
    ui.label(label);
    let v = match which.0 {
      0 => &mut self.translate[which.1],
      1 => &mut self.mv.rotate[which.1],
      2 => &mut self.mv.position[which.1],
      _ => &mut self.angle,
    };
    if value_edit(ui, v).changed() {
      self.dirty = true;
    }
    ui.end_row();
  }

  /// The Move/Copy dialog's rows, in Fusion's order.
  pub(super) fn move_fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Move Object");
    self.move_combo(ui, "move-object", &OBJECTS, false);
    ui.end_row();
    let (name, text, has) = match self.mv.object {
      "faces" => ("Faces", format!("{} selected", self.faces.len()), !self.faces.is_empty()),
      "sketch" => ("Objects", format!("{} selected", self.curves.len()), !self.curves.is_empty()),
      "components" => {
        let c = self.comps.first().copied().flatten().and_then(|c| app.s.design.component(ComponentId(c)));
        ("Component", c.map_or("Select".into(), |c| c.name.clone()), c.is_some())
      }
      _ => ("Bodies", format!("{} selected", self.bodies.len()), !self.bodies.is_empty()),
    };
    ui.label(name);
    sel_chip(ui, self.accent(), if has { &text } else { "Select" }, has);
    ui.end_row();
    ui.label("Move Type");
    self.move_combo(ui, "move-type", &TYPES, true);
    ui.end_row();
    let sketch = self.mv.object == "sketch";
    let n = if sketch { 2 } else { 3 };
    const DIST: [&str; 3] = ["X Distance", "Y Distance", "Z Distance"];
    const ANGLE: [&str; 3] = ["X Angle", "Y Angle", "Z Angle"];
    const POS: [&str; 3] = ["X", "Y", "Z"];
    match self.mv.mtype {
      "free" | "translate" => {
        for (i, l) in DIST.iter().enumerate().take(n) {
          self.move_value(ui, l, (0, i));
        }
        if self.mv.mtype == "free" {
          if sketch {
            self.move_value(ui, "Angle", (1, 2));
          } else if self.mv.object != "faces" {
            for (i, l) in ANGLE.iter().enumerate() {
              self.move_value(ui, l, (1, i));
            }
          }
        }
      }
      "rotate" => {
        if !sketch {
          ui.label("Axis");
          let axes: Vec<u32> = app.s.model.axes.keys().map(|f| f.0).collect();
          self.axis_combo(ui, "axis", false, &axes);
          ui.end_row();
        }
        self.move_value(ui, "Angle", (3, 0));
      }
      m => {
        let pick = |i: usize| self.mv.points.get(i).map(|(_, p)| self.local(*p));
        let show = |p: Option<DVec3>| match p {
          Some(p) => format!("{}, {}, {}", mm(self.unit, p.x), mm(self.unit, p.y), mm(self.unit, p.z)),
          None => "Select".into(),
        };
        let labels: &[&str] = if m == "point_to_point" { &["Origin Point", "Target Point"] } else { &["Point"] };
        for (i, l) in labels.iter().enumerate() {
          ui.label(*l);
          sel_chip(ui, self.accent(), &show(pick(i)), pick(i).is_some())
            .on_hover_text("Select a vertex or sketch point");
          ui.end_row();
        }
        if m == "point_to_position" {
          for (i, l) in POS.iter().enumerate().take(n) {
            self.move_value(ui, l, (2, i));
          }
        }
      }
    }
    if self.mv.object != "faces" && self.mv.paste.is_none() {
      ui.label("");
      if ui.checkbox(&mut self.mv.copy, "Create Copy").changed() {
        // copies leave the picks where they are: the centre is found afresh
        self.mv.pivot = None;
        self.dirty = true;
      }
      ui.end_row();
    }
    if let Some(m) = self.move_missing().filter(|_| self.move_picked()) {
      ui.label("");
      ui.label(egui::RichText::new(m).color(crate::theme::WARNING).small());
      ui.end_row();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dialogs::Kind;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn apply(s: &mut Session, d: &mut Dialog) {
    d.refresh_manip(s);
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
  }

  fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
  }

  fn bbox(s: &Session, i: usize) -> (DVec3, DVec3) {
    let m = s.model.bodies[i].shape.mass().unwrap();
    (m.bbox_min, m.bbox_max)
  }

  fn move_dialog(s: &mut Session, sel: Vec<Sel>) -> Dialog {
    s.selection = sel;
    let mut d = Dialog::blank(Kind::Move);
    d.angle = "0 deg".into();
    d.sync_move(s);
    d
  }

  #[test]
  fn free_move_triad_moves_and_turns_about_the_centre() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
    let b = s.model.bodies[0].id;
    let c = centre(&s, &[b.0]).unwrap();
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    assert_eq!((d.mv.object, d.mv.mtype, d.bodies.clone()), ("bodies", "free", vec![b.0]));
    apply(&mut s, &mut d);
    let t = d.triad(&s).unwrap();
    assert!(close(t.origin, c) && close(t.home, c) && t.arrows == [true; 3] && t.rings == [true; 3]);
    // an arrow drag sets how far the centre goes
    d.set_triad(&s, TriadEdit::Move(c + DVec3::new(10.0, 0.0, -4.0)));
    assert_eq!(d.translate, ["10 mm", "0 mm", "-4 mm"]);
    apply(&mut s, &mut d);
    let p = c + DVec3::new(10.0, 0.0, -4.0);
    assert!(close(centre(&s, &[b.0]).unwrap(), p));
    // a quarter turn on the Z ring turns the box about its own centre: it stays put
    d.set_triad(&s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_2));
    assert_eq!(d.mv.rotate[2], "90 deg");
    apply(&mut s, &mut d);
    assert!(close(centre(&s, &[b.0]).unwrap(), p));
    let (lo, hi) = bbox(&s, 0);
    assert!(((hi - lo) - DVec3::new(20.0, 10.0, 30.0)).length() < 1e-6, "{lo:?} {hi:?}");
    let t = d.triad(&s).unwrap();
    assert!(close(t.origin, p) && (t.angles[2] - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    // the X ring then adds its own angle (the Z one stays)
    d.set_triad(&s, TriadEdit::Rotate(0, std::f64::consts::FRAC_PI_2));
    assert_eq!(d.mv.rotate, ["90 deg", "0 deg", "90 deg"]);
    apply(&mut s, &mut d);
    let (lo, hi) = bbox(&s, 0);
    assert!(((hi - lo) - DVec3::new(30.0, 10.0, 20.0)).length() < 1e-6, "{lo:?} {hi:?}");
    assert!(close(centre(&s, &[b.0]).unwrap(), p));
    // a dialog opened on the previewed bodies finds where they started
    let mut again = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    again.translate = d.translate.clone();
    again.mv.rotate = d.mv.rotate.clone();
    again.applied = true;
    again.refresh_manip(&s);
    assert!(close(again.mv.pivot.as_ref().unwrap().1, c));
    // OK keeps one Move feature that edits back into Free Move
    let f = s.design.features.last().unwrap();
    let FeatureKind::Move { rotate: Some(r), pivot: Some(pv), copy: false, .. } = &f.kind else {
      panic!("{:?}", f.kind)
    };
    assert_eq!(r, &["90 deg".to_string(), "0 deg".into(), "90 deg".into()]);
    assert!(close(DVec3::from(*pv), c));
    let mut e = Dialog::blank(Kind::Move);
    e.load_move(&f.kind);
    assert_eq!((e.mv.mtype, e.mv.object), ("free", "bodies"));
  }

  #[test]
  fn translate_rotate_and_create_copy() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    // the angle starts at zero, not a full turn
    assert!(is_zero(&Dialog::blank(Kind::Move).mv.rotate[0]));
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    d.mv.mtype = "translate";
    d.mv.copy = true;
    d.translate = ["25 mm".into(), "0 mm".into(), "0 mm".into()];
    apply(&mut s, &mut d);
    // the original stays, a moved copy is added
    assert_eq!(s.model.bodies.len(), 2);
    assert!(close(bbox(&s, 0).0, DVec3::new(-5.0, -5.0, 0.0)));
    assert!(close(bbox(&s, 1).0, DVec3::new(20.0, -5.0, 0.0)));
    // the triad sits on the copy, with arrows and planes but no rings
    let t = d.triad(&s).unwrap();
    assert!(t.rings == [false; 3] && t.arrows == [true; 3] && close(t.origin - t.home, DVec3::X * 25.0));
    d.cancel(&mut s);
    assert_eq!(s.model.bodies.len(), 1);

    // Rotate: a ring on the axis, level with the centre; turning about world Z through the origin
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    d.mv.mtype = "rotate";
    d.axis = "z".into();
    d.refresh_manip(&s);
    let t = d.triad(&s).unwrap();
    assert!(t.rings == [false, false, true] && t.arrows == [false; 3] && close(t.origin, DVec3::new(0.0, 0.0, 5.0)));
    d.set_triad(&s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_4));
    assert_eq!(d.angle, "45 deg");
    apply(&mut s, &mut d);
    let (lo, hi) = bbox(&s, 0);
    assert!(((hi - lo).x - 200f64.sqrt()).abs() < 1e-6);
    d.cancel(&mut s);
  }

  #[test]
  fn point_to_point_and_point_to_position() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    let verts = fox_engine::scene::body_vertices(&s.model.bodies[0]);
    let lowest = |f: &dyn Fn(DVec3) -> f64| *verts.iter().min_by(|a, b| f(a.2).total_cmp(&f(b.2))).unwrap();
    // the corner at (-5, -5, 0) and the one at (5, 5, 10)
    let from = lowest(&|p| p.x + p.y + p.z);
    let to = lowest(&|p| -(p.x + p.y + p.z));
    let vsel = |v: (u32, bool, DVec3)| Sel::Vertex { body: b, edge: v.0, end: v.1 };
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    d.mv.mtype = "point_to_point";
    assert!(!d.move_ready());
    s.selection = vec![vsel(from)];
    d.sync_move(&s);
    assert_eq!(d.move_missing(), Some("select the point to move to"));
    // picking the next point (without Shift) replaces the selection: both are kept, in order
    s.selection = vec![vsel(to)];
    d.sync_move(&s);
    assert!(d.move_ready() && d.bodies == vec![b.0]);
    apply(&mut s, &mut d);
    assert!(close(bbox(&s, 0).0, DVec3::new(5.0, 5.0, 10.0)));
    d.cancel(&mut s);

    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    d.mv.mtype = "point_to_position";
    s.selection = vec![vsel(from)];
    d.sync_move(&s);
    // the position starts at the point; then put the corner at (100, 0, 0)
    assert_eq!(d.mv.position, ["-5 mm", "-5 mm", "0 mm"]);
    d.mv.position = ["100 mm".into(), "0 mm".into(), "0 mm".into()];
    apply(&mut s, &mut d);
    assert!(close(bbox(&s, 0).0, DVec3::new(100.0, 0.0, 0.0)));
    // no triad for the point types
    assert!(d.triad(&s).is_none());
  }

  #[test]
  fn components_faces_and_sketch_objects() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    run(&mut s, "component.create", json!({"name": "Block", "bodies": [b.0]}));
    // Components: the body's component moves
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    d.mv.object = "components";
    d.mv.mtype = "translate";
    d.translate = ["0 mm".into(), "0 mm".into(), "7 mm".into()];
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "move");
    assert!(p.get("component").is_some() && p.get("bodies").is_none(), "{p}");
    apply(&mut s, &mut d);
    assert!((bbox(&s, 0).0.z - 7.0).abs() < 1e-6);
    // ... and a copy of a component is another component
    d.mv.copy = true;
    d.dirty = true;
    assert_eq!(d.command().unwrap().0, "component.copy");
    apply(&mut s, &mut d);
    assert_eq!(s.design.components.len(), 2);
    // turning a copied component isn't offered
    d.mv.mtype = "free";
    d.mv.rotate[0] = "10 deg".into();
    assert!(!d.move_ready());
    d.cancel(&mut s);

    // Faces: the top face goes up 3 (a direct edit)
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let mut d = move_dialog(&mut s, vec![Sel::Face { body: b, face: top }]);
    d.mv.object = "faces";
    d.sync_move(&s);
    d.mv.mtype = "translate";
    d.translate = ["0 mm".into(), "0 mm".into(), "3 mm".into()];
    d.refresh_manip(&s);
    let t = d.triad(&s).unwrap();
    assert!(t.rings == [false; 3] && close(t.home, DVec3::new(0.0, 0.0, 10.0)));
    apply(&mut s, &mut d);
    assert!((bbox(&s, 0).1.z - 13.0).abs() < 1e-6, "{:?}", bbox(&s, 0));
    d.cancel(&mut s);

    // Sketch objects: a line moved in its sketch, then copied
    let sk = run(&mut s, "sketch.create", json!({"plane": "xz"}))["sketch"].as_u64().unwrap() as u32;
    let r = run(&mut s, "sketch.line", json!({"points": [[20, 0], [30, 0]]}));
    let line = r["curves"][0].as_u64().or(r["curve"].as_u64()).unwrap_or_else(|| panic!("{r}")) as u32;
    let sel = vec![Sel::SketchCurve { sketch: FeatureId(sk), curve: fox_engine::sketch::CurveId(line) }];
    let mut d = move_dialog(&mut s, sel);
    d.mv.object = "sketch";
    d.sync_move(&s);
    d.refresh_manip(&s);
    let t = d.triad(&s).unwrap();
    // in the sketch's plane: XZ in Fusion's frame (sketch y = -Z, normal +Y), the line's middle at
    // (25, 0, 0)
    assert!(close(t.origin, DVec3::new(25.0, 0.0, 0.0)) && t.arrows == [true, true, false] && t.rings[2]);
    // dragging up world Z moves the line 4 down the sketch's Y (-Z)
    d.set_triad(&s, TriadEdit::Move(DVec3::new(25.0, 0.0, 4.0)));
    assert_eq!(&d.translate[..2], ["0 mm", "-4 mm"]);
    apply(&mut s, &mut d);
    let pts = |s: &Session| {
      let sketch = s.design.sketch(FeatureId(sk)).unwrap();
      sketch.curve_points(fox_engine::sketch::CurveId(line)).iter().map(|p| sketch.pos(*p).unwrap()).collect::<Vec<_>>()
    };
    assert_eq!(pts(&s), vec![[20.0, -4.0], [30.0, -4.0]]);
    d.mv.copy = true;
    d.dirty = true;
    apply(&mut s, &mut d);
    assert_eq!(pts(&s), vec![[20.0, 0.0], [30.0, 0.0]], "the original stays");
    assert_eq!(s.design.sketch(FeatureId(sk)).unwrap().curves.len(), 2);
  }

  #[test]
  fn free_move_turns_about_any_axis_and_rotate_about_a_picked_edge() {
    use crate::manipulators::Handle as TriadHandle;
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
    let b = s.model.bodies[0].id;
    let c = centre(&s, &[b.0]).unwrap();
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    apply(&mut s, &mut d);
    let t = d.triad(&s).unwrap();
    assert!(t.free && t.orient.angle_between(DQuat::IDENTITY) < 1e-9);
    // a third of a turn about (1, 1, 1) — X to Y, Y to Z, Z to X — as the X, Y and Z angles
    let q = DQuat::from_axis_angle(DVec3::ONE.normalize(), std::f64::consts::TAU / 3.0);
    d.set_triad(&s, TriadEdit::Orient(q));
    let a: Vec<f64> = d.mv.rotate.iter().map(|e| d.ang(&s, e).unwrap()).collect();
    assert!(from_xyz([a[0], a[1], a[2]]).angle_between(q) < 1e-6, "{:?}", d.mv.rotate);
    apply(&mut s, &mut d);
    let (lo, hi) = bbox(&s, 0);
    assert!(((hi - lo) - DVec3::new(30.0, 10.0, 20.0)).length() < 1e-6, "{lo:?} {hi:?}");
    assert!(close(centre(&s, &[b.0]).unwrap(), c));
    // the triad carries the turn: a trackball drag continues from it
    let t = d.triad(&s).unwrap();
    assert!(t.orient.angle_between(q) < 1e-6);
    assert!(crate::manipulators::value_text(&t, TriadHandle::View).ends_with("deg"));
    d.cancel(&mut s);

    // Rotate about a straight edge picked in the viewport: the edge doesn't join what moves
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    let e = s.model.bodies[0]
      .edges()
      .iter()
      .find(|e| {
        (e.start.x - 5.0).abs() < 1e-6
          && (e.start.y - 5.0).abs() < 1e-6
          && (e.end.x - 5.0).abs() < 1e-6
          && (e.end.y - 5.0).abs() < 1e-6
      })
      .unwrap()
      .id;
    let mut d = move_dialog(&mut s, vec![Sel::Body { body: b }]);
    d.mv.mtype = "rotate";
    s.selection.push(Sel::Edge { body: b, edge: e });
    d.sync_move(&s);
    assert_eq!((d.axis.as_str(), d.bodies.clone()), ("edge", vec![b.0]));
    d.refresh_manip(&s);
    let t = d.triad(&s).unwrap();
    // its ring on the edge, level with the centre
    assert!(t.rings == [false, false, true] && t.arrows == [false; 3] && !t.free);
    assert!(close(t.origin, DVec3::new(5.0, 5.0, 5.0)) && t.axes[2].z.abs() > 0.999, "{t:?}");
    d.set_triad(&s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_2));
    assert_eq!((d.axis.as_str(), d.angle.as_str()), ("edge", "90 deg"));
    apply(&mut s, &mut d);
    let (lo, _) = bbox(&s, 0);
    // a quarter turn about the corner edge swings the box round it
    let want = if t.axes[2].z > 0.0 { DVec3::new(5.0, -5.0, 0.0) } else { DVec3::new(-5.0, 5.0, 0.0) };
    assert!(close(lo, want), "{lo:?}");
    // previewed, the triad stays on the edge
    d.mv.pivot = None;
    d.refresh_manip(&s);
    assert!(close(d.triad(&s).unwrap().origin, DVec3::new(5.0, 5.0, 5.0)));
    // and Rotate about a construction axis
    let ax = run(&mut s, "axis", json!({"method": "two_points", "a": {"at": [0, 0, 0]}, "b": {"at": [1, 1, 0]}}));
    d.axis = format!("axis:{}", ax["feature"]);
    let t = d.triad(&s).unwrap();
    assert!(close(t.axes[2], DVec3::new(1.0, 1.0, 0.0).normalize()));
  }
}
