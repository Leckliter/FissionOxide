//! The dialogs' viewport manipulators beyond Extrude's and Offset Plane's arrow: the radius arrow
//! of Fillet and Chamfer, Press Pull's distance arrow, the move/rotate triad of Move and the
//! offset arrow and angle ring of Joint (see [`crate::manipulators`]), the feature handles of
//! Shell (thickness arrows), Draft (angle dial) and the patterns (distance, count and angle; see
//! [`crate::handles`]), the primitives' arrows (see [`super::primitive`]); and picking joints and
//! saved joint origins in the viewport.

use fox_engine::kernel::Plane;
use fox_engine::kernel::glam::{DMat4, DQuat, DVec3};
use fox_engine::model::{BodyId, FeatureId, FeatureKind};
use fox_engine::scene::JointMark;
use fox_engine::sketch::{CurveId, Sketch};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind, Manipulator};
use crate::handles::Handle;
use crate::manipulators::{Triad, TriadEdit, snap};

/// Geometry a dialog's manipulator hangs on, worked out while its picks are valid (a preview
/// changes the very edges and faces they name) and kept while the preview shows.
#[derive(Default)]
pub(super) struct Cache {
  /// The pick the anchor was found for, and the anchor: a point and a unit direction.
  anchor: Option<(Value, DVec3, DVec3)>,
  /// Rectangular Pattern: the bodies and their centre before the preview.
  pivot: Option<(Vec<u32>, DVec3)>,
  /// Offset Plane: the base last taken from the selection (a planar face or construction
  /// plane), so a plane chosen from the dropdown afterwards isn't overridden by it.
  offset_base: Option<Value>,
  /// Offset Plane: the construction plane its preview added.
  preview_plane: Option<FeatureId>,
  /// Draft: the angle handle has been dragged, so its floating value box shows (Fusion opens it
  /// with the drag, not before).
  draft_box: bool,
  /// Draft: the pull direction it was found for, a point on its face or plane and the normal.
  pull: Option<(Value, DVec3, DVec3)>,
  /// Extrude of a face: the face it was found for, and the middle of the face's edge the taper
  /// handle sits on with the unit direction into the face.
  face_taper: Option<(Value, DVec3, DVec3)>,
}

/// Draft's angle handle turns in 5° steps, as Fusion's does.
const DRAFT_STEP: f64 = 5.0 * std::f64::consts::PI / 180.0;

/// A dragged length (mm) as a value box shows it, in the design's unit.
pub(super) fn mm(unit: fox_engine::model::LengthUnit, v: f64) -> String {
  unit.format((v * 1e4).round() / 1e4)
}

pub(super) fn deg(a: f64) -> String {
  format!("{} deg", fox_engine::sketch::Dim::new((a.to_degrees() * 1e4).round() / 1e4).expr)
}

impl Dialog {
  /// Is the dialog's command applied to the session as a preview?
  pub fn preview_applied(&self) -> bool {
    self.applied
  }

  /// Offset Plane: a planar face or construction plane picked in the viewport or browser becomes
  /// the base plane, as in Fusion (the dropdown keeps the origin planes).
  pub(super) fn sync_offset_base(&mut self, s: &Session) {
    let picked = s.selection.iter().find_map(|sel| match *sel {
      Sel::Face { body, face } if super::is_planar(s, body, face) => {
        Some((json!({"body": body.0, "face": face}), format!("Face of {}", s.body_name(body))))
      }
      Sel::Feature { feature } if s.model.planes.contains_key(&feature) && Some(feature) != self.previewed_plane() => {
        Some((json!({"feature": feature.0}), s.design.feature(feature).map(|f| f.name.clone()).unwrap_or_default()))
      }
      _ => None,
    });
    let key = picked.as_ref().map(|(v, _)| v.clone());
    if key == self.manip.offset_base {
      return;
    }
    self.manip.offset_base = key;
    if let Some((v, label)) = picked
      && v != self.plane
    {
      self.plane = v;
      self.plane_label = label;
      self.dirty = true;
    }
  }

  /// Records the construction plane an Offset Plane preview added.
  pub(super) fn note_preview(&mut self, result: &Value) {
    self.manip.preview_plane = (self.kind == Kind::OffsetPlane)
      .then(|| result.get("feature").and_then(Value::as_u64))
      .flatten()
      .map(|f| FeatureId(f as u32));
  }

  /// The construction plane the open Offset Plane is previewing (the viewport fills it instead
  /// of drawing the dashed outline of a finished plane).
  pub fn previewed_plane(&self) -> Option<FeatureId> {
    self.manip.preview_plane.filter(|_| self.applied && self.kind == Kind::OffsetPlane && self.editing.is_none())
  }

  /// Offset Plane's base plane and the point on it the arrow starts from: a picked face's centre,
  /// else the plane's origin.
  pub(super) fn offset_base(&self, s: &Session) -> Option<(Plane, DVec3)> {
    let base = super::plane_of(s, &self.plane)?;
    let get = |k: &str| self.plane.get(k).and_then(Value::as_u64).map(|x| x as u32);
    let centre = get("body")
      .zip(get("face"))
      .and_then(|(b, f)| s.model.body(BodyId(b))?.faces().iter().find(|x| x.id == f).map(|x| x.center))
      .map_or(base.origin, |c| base.to_world(base.to_local(c)));
    Some((base, centre))
  }

  /// The plane Offset Plane would make, and the centre of the square the viewport fills for it.
  pub fn offset_plane_preview(&self, s: &Session) -> Option<(Plane, DVec3)> {
    if self.kind != Kind::OffsetPlane || self.editing.is_some() {
      return None;
    }
    let (base, centre) = self.offset_base(s)?;
    let d = self.offset_sign(s) * self.len(s, &self.distance)?;
    Some((base.offset(d), centre + base.normal() * d))
  }

  /// Offsets from XZ go along +Y in designs made with XZ_OFFSET_PLUS_Y (see
  /// `OriginPlane::offset_sign`).
  pub(super) fn offset_sign(&self, s: &Session) -> f64 {
    if self.plane.as_str() == Some("xz") {
      fox_engine::model::OriginPlane::Xz.offset_sign(&s.design.behaviours)
    } else {
      1.0
    }
  }

  pub(super) fn len(&self, s: &Session, e: &str) -> Option<f64> {
    fox_engine::expr::eval(e, fox_engine::expr::Kind::Length, &s.model.params).ok()
  }

  pub(super) fn ang(&self, s: &Session, e: &str) -> Option<f64> {
    if e.trim().is_empty() {
      return Some(0.0);
    }
    fox_engine::expr::eval(e, fox_engine::expr::Kind::Angle, &s.model.params).ok()
  }

  /// The pick an arrow hangs on: the first edge (Fillet, Chamfer; a fillet by faces alone hangs
  /// on the first face) or face (Press Pull), or the offset curves (Sketch Offset).
  fn anchor_key(&self) -> Value {
    match self.kind {
      Kind::PressPull | Kind::OffsetFace | Kind::Shell | Kind::Draft | Kind::Extrude => json!(self.faces.first()),
      Kind::Scale => json!([self.bodies, self.point]),
      Kind::SkOffset => json!({"offset": self.curves}),
      Kind::Fillet => json!(self.edges.first().or(self.faces.first())),
      _ => json!(self.edges.first()),
    }
  }

  /// Brings the cached manipulator geometry up to date; the viewport calls it every frame.
  pub fn refresh_manip(&mut self, s: &Session) {
    // Edit Move keeps its triad, on the moved bodies
    if self.picks_fixed() && !(self.kind == Kind::Move && self.mv.adds) {
      return;
    }
    match self.kind {
      Kind::Fillet | Kind::Chamfer | Kind::PressPull | Kind::OffsetFace | Kind::Shell | Kind::Draft
        if !self.applied =>
      {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = anchor(s, &key).map(|(o, d)| (key, o, d));
        }
        if self.kind == Kind::Draft && self.manip.pull.as_ref().is_none_or(|p| p.0 != self.plane) {
          self.manip.pull = pull_anchor(s, &self.plane).map(|(o, n)| (self.plane.clone(), o, n));
        }
      }
      // the bodies' reach from the point, measured before the preview scales them
      Kind::Scale if !self.applied => {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = self.body_scale_anchor(s).map(|(o, d)| (key, o, d));
        }
      }
      // the offset curves stay as they are under the preview
      Kind::SkOffset => {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = offset_anchor(s, &self.curves).map(|(o, d)| (key, o, d));
        }
      }
      // the patterns' handles start at the objects' centre before the preview
      Kind::RectPattern | Kind::CircPattern
        if !self.applied && self.manip.pivot.as_ref().is_none_or(|p| p.0 != self.bodies) =>
      {
        self.manip.pivot = centre(s, &self.bodies).map(|c| (self.bodies.clone(), c));
      }
      // a face extrude's arrow and taper handle, on its face before the preview changes it
      Kind::Extrude if !self.applied && self.extrudes_faces() => {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = anchor(s, &key).map(|(o, d)| (key.clone(), o, d));
          self.manip.face_taper = face_taper_edge(s, &key).map(|(m, i)| (key, m, i));
        }
      }
      Kind::Move => self.refresh_move(s),
      // an edge flange's arrow, found on the sheet before the preview bends it
      Kind::Flange if self.choice == "edge" && !self.applied => {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = super::flange::flange_anchor(s, &key).map(|(o, d)| (key, o, d));
        }
      }
      // Extend's arrow leaves its first edge across the surface
      Kind::Form(f) if f.cmd == crate::forms::EXTEND_CMD && !self.applied => {
        let key = self.form_value("edges").and_then(|e| e.as_array()?.first().cloned()).unwrap_or(Value::Null);
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = extend_anchor(s, &key).map(|(o, d)| (key, o, d));
        }
      }
      // the arrow's reach is measured afresh each frame (the preview scales the very curves)
      Kind::SkScale => self.manip.anchor = self.scale_anchor(s).map(|(o, d)| (Value::Null, o, d)),
      _ => {}
    }
  }

  /// Scale: the arrow's foot (the point) and the world X axis as long as the bodies reach from
  /// the point (the arrow is that long at a factor of 1).
  fn body_scale_anchor(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    let at = DVec3::from(self.point?);
    let mut reach: f64 = 0.0;
    for b in &self.bodies {
      let Some(m) = s.model.body(BodyId(*b)).and_then(|b| b.mesh()) else { continue };
      for p in &m.positions {
        reach = reach.max((DVec3::from(p.map(f64::from)) - at).length());
      }
    }
    if reach < 1e-6 {
      reach = 10.0;
    }
    Some((at, DVec3::X * reach))
  }

  /// Draft: the pull direction once it is picked, as it was before the preview: a point on its
  /// face or plane and the way it points (flipped with Flip Pull Direction).
  pub fn draft_pull(&self) -> Option<(DVec3, DVec3)> {
    if self.kind != Kind::Draft || self.editing.is_some() {
      return None;
    }
    let (key, o, n) = self.manip.pull.as_ref()?;
    (*key == self.plane).then_some((*o, if self.flip { -*n } else { *n }))
  }

  /// Sketch Scale's factor as a number.
  fn scale_factor(&self, s: &Session) -> Option<f64> {
    fox_engine::expr::eval(&self.a, fox_engine::expr::Kind::Unitless, &s.model.params).ok()
  }

  /// Where Sketch Scale's arrow hangs: the picked point, and the sketch's x axis scaled to how
  /// far the entities reach from the point before the preview scaled them (the arrow is that
  /// long at a factor of 1).
  fn scale_anchor(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    if self.center.is_none() || self.curves.is_empty() {
      return None;
    }
    let sk = s.edit_sketch?;
    let sketch = s.design.sketch(sk).ok()?;
    let plane = s.model.sketches.get(&sk)?.plane;
    let at = self.at_pt()?;
    let mut reach: f64 = 0.0;
    for c in &self.curves {
      for p in sketch.curve_points(CurveId(*c)) {
        if let Some(q) = sketch.pos(p) {
          reach = reach.max((q[0] - at[0]).hypot(q[1] - at[1]));
        }
      }
    }
    if self.applied
      && let Some(f) = self.scale_factor(s).filter(|f| f.abs() > 1e-9)
    {
      reach /= f.abs();
    }
    if reach < 1e-6 {
      reach = 10.0;
    }
    Some((plane.to_world(at), plane.x * reach))
  }

  /// The arrows of Fillet and Chamfer (radius, out of the first edge) and Press Pull (distance,
  /// along the first face's normal). An edited fillet or chamfer keeps its arrow, on the edge the
  /// feature stored (see [`Dialog::load_edit_anchor`]); Press Pull's picks name the body before
  /// it, so it has none while editing.
  pub(super) fn more_manipulator(&self, s: &Session) -> Option<Manipulator> {
    // Scale (uniform): the arrow from the point, the factor times the bodies' reach long
    if self.kind == Kind::Scale {
      let (key, origin, reach) = self.manip.anchor.as_ref().filter(|_| self.choice != "non_uniform")?;
      if *key != self.anchor_key() {
        return None;
      }
      let r = reach.length();
      let f = fox_engine::expr::eval(&self.a, fox_engine::expr::Kind::Unitless, &s.model.params).ok()?;
      return Some(Manipulator { origin: *origin, dir: *reach / r, value: f * r });
    }
    // Sketch Scale: the arrow from the point, the factor times the entities' reach long
    if self.kind == Kind::SkScale {
      let (_, origin, reach) =
        self.manip.anchor.as_ref().filter(|_| self.center.is_some() && !self.curves.is_empty())?;
      let r = reach.length();
      return Some(Manipulator { origin: *origin, dir: *reach / r, value: self.scale_factor(s)? * r });
    }
    // Edge Flange: the Height arrow on the edge, the way the flange bends (Flip turns it over)
    if self.kind == Kind::Flange {
      let (key, origin, dir) = self.manip.anchor.as_ref().filter(|_| self.choice == "edge")?;
      if *key != self.anchor_key() {
        return None;
      }
      let sign = if self.flip { -1.0 } else { 1.0 };
      return Some(Manipulator {
        origin: *origin,
        dir: *dir * sign,
        value: self.len(s, &self.distance).unwrap_or(0.0),
      });
    }
    // Extend: the distance along the arrow out of the first edge
    if let Kind::Form(f) = self.kind
      && f.cmd == crate::forms::EXTEND_CMD
    {
      let edges = self.form_value("edges")?.as_array()?;
      let (_, origin, dir) = self.manip.anchor.as_ref().filter(|a| edges.first() == Some(&a.0))?;
      let value = self.len(s, self.form_value("distance")?.as_str()?).unwrap_or(0.0);
      return Some(Manipulator { origin: *origin, dir: *dir, value });
    }
    if !matches!(self.kind, Kind::Fillet | Kind::Chamfer | Kind::PressPull | Kind::OffsetFace | Kind::SkOffset)
      || (self.editing.is_some() && !matches!(self.kind, Kind::Fillet | Kind::Chamfer))
    {
      return None;
    }
    let (key, origin, dir) = self.manip.anchor.as_ref()?;
    if *key != self.anchor_key() {
      return None;
    }
    let field = if matches!(self.kind, Kind::PressPull | Kind::OffsetFace | Kind::SkOffset) {
      &self.distance
    } else {
      &self.radius
    };
    let value = match self.len(s, field) {
      Some(v) => v,
      // Sketch Offset's arrow stays on its curve (and its value box stays put) while the distance
      // is half typed (".") rather than vanishing and taking the keyboard with it
      None if self.kind == Kind::SkOffset => 0.0,
      None => return None,
    };
    // Press Pull editing a primitive: the arrow runs from its base, as long as its height
    let origin = *origin - *dir * if self.kind == Kind::PressPull { self.press_pull_base() } else { 0.0 };
    Some(Manipulator { origin, dir: *dir, value })
  }

  /// Does the floating value box sit by the pointer rather than at the arrow's head? Sketch
  /// Offset's does while the offset still sits on its curve (a zero or half-typed distance), as
  /// Fusion shows it when the command starts on a pre-selected curve.
  pub(crate) fn box_by_pointer(&self, s: &Session) -> bool {
    self.kind == Kind::SkOffset
      && self.editing.is_none()
      && !self.curves.is_empty()
      && self.len(s, &self.distance).is_none_or(|v| v.abs() < 1e-9)
  }

  /// Edit Feature: an edited fillet's or chamfer's arrow hangs on the midpoint of the first edge
  /// the feature stored, leaning out between the faces that met there (the edge itself is gone
  /// from the finished body).
  pub(super) fn load_edit_anchor(&mut self, kind: &FeatureKind) {
    let e = match kind {
      FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. } => edges.first(),
      _ => None,
    };
    let Some(e) = e else { return };
    let dir = (DVec3::from(e.n1) + DVec3::from(e.n2)).try_normalize().unwrap_or(DVec3::Z);
    // the edited dialog's edges are placeholders, so its anchor key is null
    self.manip.anchor = Some((self.anchor_key(), DVec3::from(e.point), dir));
  }

  /// Edit Feature: an edited edge flange's arrow hangs on the edge it was built on (see
  /// [`super::flange::flange_edit_anchor`]).
  pub(super) fn load_flange_edit_anchor(&mut self, s: &Session, kind: &FeatureKind) {
    if let Some((o, d)) = super::flange::flange_edit_anchor(s, kind) {
      self.manip.anchor = Some((self.anchor_key(), o, d));
    }
  }

  /// The feature handles this dialog shows (see [`crate::handles`]): Extrude's taper handle,
  /// Shell's thickness arrows, Draft's angle dial, the rectangular pattern's distance and count
  /// handles along each direction and the circular pattern's angle and count dials.
  pub fn handles(&self, s: &Session) -> Vec<Handle> {
    match self.kind {
      Kind::Extrude => self.taper_handle(s),
      Kind::Shell => self.shell_handles(s),
      Kind::Draft => self.draft_handles(s),
      Kind::RectPattern => self.pattern_centre().and_then(|c| self.pattern_handles(s, c)),
      Kind::CircPattern => self.pattern_centre().and_then(|c| self.circ_handles(s, c)),
      Kind::SkCircPattern => self.sk_circ_handles(s),
      Kind::SkRectPattern => self.sk_rect_handles(s),
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::SK_TEXT) => self.text_handles(s),
      Kind::Revolve => self.revolve_handles(s),
      Kind::PlaneCut => self.plane_cut_handles(s),
      // the SURFACE tab's Extrude, Revolve, Ruled and Offset (see [`super::surface`])
      Kind::SurfExtrude | Kind::SurfRevolve => self.surface_handles(s),
      Kind::Form(f) if matches!(f.cmd, "surface.ruled.form" | "surface.offset.form") => self.surface_handles(s),
      // Box, Cylinder, Sphere, Torus and Coil (see [`super::primitive`])
      k if super::primitive::base_of(k).is_some() => self.primitive_handles(s),
      Kind::Section if !self.plane.is_null() => self.section_handles(s),
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::PLANE_ANGLE) => self.angle_plane_handles(s),
      _ => None,
    }
    .unwrap_or_default()
  }

  /// Sketch Mirror / patterns: the picked objects' centre (sketch coordinates) and the sketch's
  /// plane, while the sketch is open.
  fn sketch_objects_centre(&self, s: &Session) -> Option<([f64; 2], Plane)> {
    let r = s.model.sketches.get(&s.edit_sketch?)?;
    let pts: Vec<[f64; 2]> =
      self.curves.iter().flat_map(|c| r.sketch.curve_points(CurveId(*c))).filter_map(|p| r.sketch.pos(p)).collect();
    if pts.is_empty() {
      return None;
    }
    let n = pts.len() as f64;
    Some(([pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n], r.plane))
  }

  /// Sketch Circular Pattern: a dot at the next instance for the quantity (its floating box has
  /// the spinner), round the centre through the objects' centre, and for the Angle and
  /// Symmetric distributions a dial for the total angle.
  fn sk_circ_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (c, plane) = self.sketch_objects_centre(s)?;
    let at = [self.at[0].trim().parse::<f64>().ok()?, self.at[1].trim().parse::<f64>().ok()?];
    let centre = plane.to_world(at);
    let spoke = plane.to_world(c) - centre;
    let zero = spoke.try_normalize()?;
    let r = Some(spoke.length());
    let total = if self.choice == "full" { std::f64::consts::TAU } else { self.ang(s, &self.angle)? };
    let n = count_of(&self.count);
    let step = circ_step(total, n);
    let axis = plane.normal();
    let mut out = vec![Handle::dial(centre, axis, zero, r, step, format!("{n} instances")).dot()];
    if self.choice != "full" {
      out.push(Handle::dial(centre, axis, zero, r, total, deg(total)));
    }
    Some(out)
  }

  /// Sketch Rectangular Pattern: along each picked direction from the objects' centre, an arrow
  /// for the distance and a dot for the quantity (both with floating boxes), once a direction
  /// is picked, as Fusion's boxes appear then.
  fn sk_rect_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (c, plane) = self.sketch_objects_centre(s)?;
    let origin = plane.to_world(c);
    let d1 = self.dir_lines.first()?.1;
    let d2 = self.dir_lines.get(1).map(|l| l.1).unwrap_or([-d1[1], d1[0]]);
    let mut out = Vec::new();
    for (k, d) in [d1, d2].into_iter().enumerate().take(if self.use_dir2 { 2 } else { 1 }) {
      let (field, count) = if k == 0 { (&self.spacing, &self.count) } else { (&self.spacing2, &self.count2) };
      let dist = self.len(s, field)?;
      let n = count_of(count);
      let steps = f64::from(n.max(2) - 1);
      let (step, total) = if self.pattern_extent { (dist / steps, dist) } else { (dist, dist * steps) };
      let dir = (plane.x * d[0] + plane.y * d[1]).try_normalize()?;
      let (at_dist, at_count) = if self.pattern_extent { (total, step) } else { (step, total) };
      out.push(Handle::arrow(origin, dir, at_dist, mm(self.unit, dist)));
      out.push(Handle::arrow(origin, dir, at_count, format!("{n} instances")).dot());
    }
    Some(out)
  }

  /// Text: the rotate handle at the frame's corner, a dial about the frame's first corner that
  /// turns the text (and reads its angle).
  fn text_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (a, b, angle) = self.text_frame()?;
    let plane = s.model.sketches.get(&s.edit_sketch?)?.plane;
    let sign = if b[0] >= a[0] { 1.0 } else { -1.0 };
    let r = (b[0] - a[0]).abs();
    let value = angle.to_radians();
    Some(vec![Handle::dial(plane.to_world(a), plane.normal(), plane.x * sign, Some(r), value, deg(value))])
  }

  /// Plane at Angle: Fusion's rotation handle on the previewed plane, a dial about the picked
  /// line from the 0° plane, its head on the plane at the angle.
  fn angle_plane_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    if !self.applied {
      return None;
    }
    let f = s.design.features.last()?;
    if !matches!(&f.kind, FeatureKind::PlaneBy { def: fox_engine::model::construct::PlaneDef::Angle { .. } }) {
      return None;
    }
    let pl = s.model.planes.get(&f.id)?;
    let Kind::Form(form) = self.kind else { return None };
    let i = form.inputs.iter().position(|x| x.key() == "angle")?;
    let a = self.ang(s, self.form.values[i].as_str()?)?;
    let d = pl.x.try_normalize()?;
    let zero = DQuat::from_axis_angle(d, -a) * pl.normal().cross(d);
    Some(vec![Handle::dial(pl.origin, d, zero, None, a, deg(a))])
  }

  /// Takes handle `i`'s dragged value (not yet snapped; lengths snap to `step` world units,
  /// angles to `step` radians) into the dialog's fields.
  pub fn set_handle(&mut self, s: &Session, i: usize, v: f64, step: f64) {
    let v_snapped = snap(v, step);
    match self.kind {
      Kind::Section => self.set_section_handle(i, v_snapped),
      // a taper stays short of a right angle; none at all is an empty box
      Kind::Extrude => {
        let a = v_snapped.clamp(-89f64.to_radians(), 89f64.to_radians());
        self.solid.ext.set_taper(if a.abs() < 1e-9 { String::new() } else { deg(a) });
      }
      Kind::Shell => {
        let t = mm(self.unit, v_snapped.max(step.max(1e-3)));
        if i == 1 { self.solid.thickness_out = t } else { self.radius = t }
      }
      // a draft stays short of a right angle; the box it opens reads to a tenth ("35.0 deg")
      Kind::Draft => {
        let a = v_snapped.clamp(-89f64.to_radians(), 89f64.to_radians());
        self.angle = format!("{:.1} deg", (a.to_degrees() * 10.0).round() / 10.0);
        self.manip.draft_box = true;
      }
      Kind::RectPattern => {
        if !self.set_pattern_handle(s, i, v, step) {
          return;
        }
      }
      Kind::CircPattern => {
        if !self.set_circ_handle(s, i, v, step) {
          return;
        }
      }
      Kind::SkCircPattern => {
        let total = if self.choice == "full" { Some(std::f64::consts::TAU) } else { self.ang(s, &self.angle) };
        let Some(total) = total else { return };
        if i == 0 {
          // the quantity that puts the next instance nearest the dot
          if v * total <= 0.0 {
            return;
          }
          let n = (total / v).round() + if is_full_turn(total) { 0.0 } else { 1.0 };
          self.count = (n.clamp(2.0, MAX_COUNT) as u32).to_string();
        } else {
          let a = snap(v, step);
          if a.abs() < 1e-9 {
            return;
          }
          self.angle = deg(a);
        }
      }
      Kind::SkRectPattern => {
        if !self.set_pattern_handle(s, i, v, step) {
          return;
        }
      }
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::SK_TEXT) => {
        self.set_text_angle(snap(v, step).to_degrees());
      }
      // a revolve turns by something, up to a whole turn either way
      Kind::Revolve => {
        let a = v_snapped.clamp(-std::f64::consts::TAU, std::f64::consts::TAU);
        if a.abs() < 1e-9 {
          return;
        }
        self.angle = deg(a);
      }
      k if super::primitive::base_of(k).is_some() => {
        if !self.set_primitive_handle(s, i, v, step) {
          return;
        }
      }
      Kind::PlaneCut => self.set_plane_cut_handle(i, v, step),
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::PLANE_ANGLE) => {
        let Some(i) = f.inputs.iter().position(|x| x.key() == "angle") else { return };
        self.form.values[i] = json!(deg(v_snapped));
      }
      _ => {
        if !self.set_surface_handle(v, step) {
          return;
        }
      }
    }
    self.dirty = true;
  }

  /// The pattern's objects' centre before the preview, while they are the ones picked.
  fn pattern_centre(&self) -> Option<DVec3> {
    let (bodies, c) = self.manip.pivot.as_ref()?;
    (*bodies == self.bodies).then_some(*c)
  }

  /// The face anchor (a point and the outward normal) while it is for the first picked face.
  pub(super) fn face_anchor(&self) -> Option<(DVec3, DVec3)> {
    let (key, o, n) = self.manip.anchor.as_ref()?;
    (*key == self.anchor_key() && !self.faces.is_empty()).then_some((*o, *n))
  }

  /// Extrude: Fusion's taper handle, a disc on a short arc beside the profile's edge farthest from
  /// the distance arrow's foot (never on the arrow), turning about that edge from the extrude direction
  /// inward for a positive taper (which narrows the extrusion), for the taper angle.
  fn taper_handle(&self, s: &Session) -> Option<Vec<Handle>> {
    let m = self.manipulator(s)?;
    let a = self.ang(s, self.solid.ext.taper())?;
    let up = if m.value < 0.0 { -m.dir } else { m.dir };
    if self.extrudes_faces() {
      // beside the picked face's edge farthest from the arrow
      let (key, mid, inward) = self.manip.face_taper.as_ref()?;
      return (*key == self.anchor_key())
        .then(|| vec![Handle::dial(*mid, up.cross(*inward), up, None, a, deg(a)).taper()]);
    }
    let r = s.model.sketches.get(&self.sketch?)?;
    // (an edited extrude without picks keeps its stored profile, as its arrow does)
    let at = match self.profiles.first() {
      Some(p) => *p,
      None => r.profiles.first()?.interior_point(),
    };
    let profile = r.profiles.iter().find(|p| p.contains(at))?;
    let (mid, inward) = taper_edge(&profile.outer.polygon(), at, &|p| profile.contains(p))?;
    let inward = (r.plane.x * inward[0] + r.plane.y * inward[1]).try_normalize()?;
    let center = r.plane.to_world(mid);
    Some(vec![Handle::dial(center, up.cross(inward), up, None, a, deg(a)).taper()])
  }

  /// Shell: an arrow from the first opened face into the wall for an inside thickness, out of it
  /// for an outside one; both for Both (the inside one first).
  fn shell_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (o, n) = self.face_anchor()?;
    let t = self.len(s, &self.radius)?;
    let arrow = |dir: DVec3, v: f64| Handle::arrow(o, dir, v, mm(self.unit, v));
    Some(match self.solid.shell_dir {
      "outside" => vec![arrow(n, t)],
      "both" => {
        let out =
          if self.solid.thickness_out.trim().is_empty() { t } else { self.len(s, &self.solid.thickness_out)? };
        vec![arrow(-n, t), arrow(n, out)]
      }
      _ => vec![arrow(-n, t)],
    })
  }

  /// Draft, as Fusion shows it: the angle as a disc on an arc about the first face's hinge
  /// (where it meets the neutral plane), turning from the pull direction the way a positive angle
  /// tilts the face in 5° steps; and the pull direction as a hollow arrow from the hinge.
  fn draft_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    let (c, n) = self.face_anchor()?;
    // (the pull plane as picked: a pull face may change under the preview)
    let (origin, pull) = self.draft_pull()?;
    let (zero, axis, hinge) = draft_dial(c, n, origin, pull)?;
    let a = self.ang(s, &self.angle)?;
    Some(vec![Handle::dial(hinge, axis, zero, None, a, deg(a)).disc(DRAFT_STEP), Handle::pull(hinge, pull)])
  }

  /// Circular Pattern: dials about its axis through the objects' centre, one for the total angle
  /// and a dot at the next instance for the count.
  fn circ_handles(&self, s: &Session, c: DVec3) -> Option<Vec<Handle>> {
    let (o, d) = axis_line(s, &self.axis)?;
    let foot = o + d * (c - o).dot(d);
    let (zero, r) = match (c - foot).try_normalize() {
      Some(z) => (z, Some((c - foot).length())),
      None => (d.any_orthonormal_vector(), None),
    };
    let total = self.ang(s, &self.angle)?;
    let n = count_of(&self.count);
    let step = circ_step(total, n);
    let instances = format!("{n} instances");
    Some(vec![Handle::dial(foot, d, zero, r, total, deg(total)), Handle::dial(foot, d, zero, r, step, instances).dot()])
  }

  /// A dragged circular pattern handle: the total angle, or the count that puts the next instance
  /// nearest the dot. False when the value can't apply.
  fn set_circ_handle(&mut self, s: &Session, i: usize, v: f64, step: f64) -> bool {
    if i == 0 {
      let a = snap(v, step);
      if a.abs() < 1e-9 {
        return false;
      }
      self.angle = deg(a);
      return true;
    }
    let Some(total) = self.ang(s, &self.angle) else { return false };
    if v * total <= 0.0 {
      return false;
    }
    let full = is_full_turn(total);
    let n = (total / v).round() + if full { 0.0 } else { 1.0 };
    self.count = (n.clamp(2.0, MAX_COUNT) as u32).to_string();
    true
  }

  /// Takes a dragged arrow value (already rounded to `step`) into the field; false for dialogs
  /// without such an arrow.
  pub(super) fn set_more_manipulator(&mut self, v: f64, step: f64) -> bool {
    match self.kind {
      // a radius or distance can't reach zero: the smallest step is as far as it goes
      Kind::Fillet | Kind::Chamfer => self.radius = mm(self.unit, v.max(step.max(1e-3))),
      Kind::PressPull | Kind::OffsetFace => self.distance = mm(self.unit, v),
      // a flange's height stops at the edge
      Kind::Flange if self.choice == "edge" => self.distance = mm(self.unit, v.max(0.0)),
      // Extend doesn't go backwards
      Kind::Form(f) if f.cmd == crate::forms::EXTEND_CMD => {
        let Some(i) = f.inputs.iter().position(|x| x.key() == "distance") else { return false };
        self.form.values[i] = json!(mm(self.unit, v.max(0.0)));
      }
      // an offset is never zero: it keeps to the side it is on
      Kind::SkOffset => {
        self.distance = mm(self.unit, nonzero(v, step));
        self.follow = false;
      }
      // the factor to two decimals, never zero or negative (as Fusion's box shows it)
      Kind::Scale => {
        let Some(r) = self.manip.anchor.as_ref().map(|a| a.2.length()).filter(|r| *r > 1e-9) else { return false };
        self.a = format!("{:.2}", (v / r).max(0.01));
      }
      // the factor to two decimals, never zero or negative
      Kind::SkScale => {
        let Some(r) = self.manip.anchor.as_ref().map(|a| a.2.length()).filter(|r| *r > 1e-9) else { return false };
        let f = (v / r * 100.0).round() / 100.0;
        self.a = fox_engine::expr::trim_number(f.max(0.01));
      }
      _ => return false,
    }
    true
  }

  /// The field feature handle `i`'s floating value box (see [`crate::value_box`]) edits: Shell's
  /// inside (or only) thickness, and its outside one for Both; Extrude's taper angle once Tab
  /// moves the box there.
  pub fn handle_field(&mut self, i: usize) -> Option<&mut String> {
    match (self.kind, i) {
      (Kind::Extrude, 0) => Some(self.solid.ext.taper_mut()),
      (Kind::Shell, 0) => Some(&mut self.radius),
      (Kind::Shell, 1) => Some(&mut self.solid.thickness_out),
      // Draft's angle, once its handle has been dragged
      (Kind::Draft, 0) if self.manip.draft_box => Some(&mut self.angle),
      // the sketch patterns' quantity (with its spinner) and distance boxes
      (Kind::SkCircPattern, 0) => Some(&mut self.count),
      (Kind::SkRectPattern, 0) => Some(&mut self.spacing),
      (Kind::SkRectPattern, 1) => Some(&mut self.count),
      (Kind::SkRectPattern, 2) => Some(&mut self.spacing2),
      (Kind::SkRectPattern, 3) => Some(&mut self.count2),
      (Kind::Revolve, 0) => Some(&mut self.angle),
      (k, _) if super::primitive::base_of(k).is_some() => self.primitive_field(i),
      (_, 0) => self.surface_handle_field(),
      _ => None,
    }
  }

  /// Does every handle with a field get its own floating box (the sketch Rectangular Pattern's
  /// distance and quantity boxes), rather than the first alone?
  pub fn boxes_every_handle(&self) -> bool {
    self.kind == Kind::SkRectPattern
  }

  /// Does the drag arrow snap to model vertices and edges the pointer passes (Extrude's distance
  /// and Offset Plane's offset, as in Fusion)?
  pub(crate) fn arrow_snaps(&self) -> bool {
    matches!(self.kind, Kind::Extrude | Kind::OffsetPlane)
  }

  /// Does Sketch Offset follow the cursor (curves picked, not placed by a click yet)?
  pub(crate) fn follows_cursor(&self) -> bool {
    self.kind == Kind::SkOffset && self.follow && !self.curves.is_empty() && self.editing.is_none()
  }

  /// A click placed the offset: it stays where it is.
  pub(crate) fn stop_following(&mut self) {
    self.follow = false;
  }

  /// Sets Sketch Offset's distance and side from the cursor at `at` (sketch coordinates, open
  /// sketch), rounded to `step`.
  pub(crate) fn follow_cursor(&mut self, s: &Session, at: [f64; 2], step: f64) {
    if !self.follows_cursor() {
      return;
    }
    let Some(sketch) = s.edit_sketch.and_then(|sk| s.design.sketch(sk).ok()) else { return };
    let curves: Vec<CurveId> = self.curves.iter().map(|c| CurveId(*c)).collect();
    let Some(v) = offset_at(sketch, &curves, at) else { return };
    let v = if step > 0.0 { (v / step).round() * step } else { v };
    let text = mm(self.unit, nonzero(v, step));
    if text != self.distance {
      self.distance = text;
      self.dirty = true;
    }
  }

  /// The move/rotate triad this dialog shows: Move's on the moved bodies' centre, Joint's (an
  /// offset arrow and an angle ring about the joint's Z) on the fixed side's joint origin.
  pub fn triad(&self, s: &Session) -> Option<Triad> {
    if self.editing.is_some() && !(self.kind == Kind::Move && self.mv.adds) {
      return None;
    }
    match self.kind {
      Kind::Move => self.move_triad(s),
      Kind::InsertMesh => self.insert_mesh_triad(s),
      Kind::Joint => {
        let f = self.joint_fixed_frame(s)?;
        let (x, y, z, o) = (f.x_axis.truncate(), f.y_axis.truncate(), f.z_axis.truncate(), f.w_axis.truncate());
        let (offset, angle) = (self.len(s, &self.distance)?, self.ang(s, &self.angle)?);
        let q = DQuat::from_axis_angle(z, angle);
        Some(Triad {
          origin: o + z * offset,
          axes: [q * x, q * y, z],
          arrows: [false, false, true],
          rings: [false, false, true],
          planes: [false; 3],
          home: o,
          angles: [0.0, 0.0, angle],
          free: false,
          orient: DQuat::IDENTITY,
        })
      }
      _ => None,
    }
  }

  /// Takes a triad drag into the fields: Move's X/Y/Z distances, axis and angle (turning about
  /// the triad, which stays put); Joint's offset and angle.
  pub fn set_triad(&mut self, s: &Session, edit: TriadEdit) {
    match self.kind {
      Kind::Move => {
        if !self.set_move_triad(edit) {
          return;
        }
      }
      Kind::InsertMesh => {
        if !self.set_insert_mesh_triad(edit) {
          return;
        }
      }
      Kind::Joint => match edit {
        TriadEdit::Move(p) => {
          let Some(f) = self.joint_fixed_frame(s) else { return };
          self.distance = mm(self.unit, (p - f.w_axis.truncate()).dot(f.z_axis.truncate()));
        }
        TriadEdit::Rotate(_, a) => self.angle = deg(a),
        // Joint's triad isn't free
        TriadEdit::Orient(_) => return,
      },
      _ => return,
    }
    self.dirty = true;
  }

  /// The joint origin frame of the Joint dialog's fixed side, on the model as it is.
  fn joint_fixed_frame(&self, s: &Session) -> Option<DMat4> {
    if !self.saved[1].is_empty() {
      let f = s.design.features.iter().find(|f| f.name == self.saved[1])?;
      return s.model.joint_origins.get(&f.id).map(|(_, m)| *m);
    }
    // picks fill the moving side first, unless a saved origin stands in for it
    let (b, o) = self.origins.get(if self.saved[0].is_empty() { 1 } else { 0 })?;
    pick_frame(s, BodyId(*b), o)
  }

  /// A joint or saved joint origin clicked in the viewport: a saved origin goes to the Joint
  /// dialog's next side (the moving one until that is set, then the fixed one); a joint to Drive
  /// Joints or Motion Study. False when this dialog has no use for it.
  pub fn pick_joint_mark(&mut self, s: &mut Session, mark: &JointMark, name: &str) -> bool {
    match self.kind {
      Kind::Joint if !mark.joint && self.editing.is_none() => {
        self.withdraw(s);
        let moving_set = !self.saved[0].is_empty() || !self.origins.is_empty();
        self.saved[usize::from(moving_set)] = name.to_string();
        self.dirty = true;
        true
      }
      Kind::DriveJoint | Kind::MotionStudy if mark.joint => {
        self.withdraw(s);
        self.target = Some(mark.feature.0);
        self.load_joint(s);
        self.report = None;
        true
      }
      _ => false,
    }
  }
}

/// The area-weighted centre of the bodies' surfaces: it turns and moves with them, and finely
/// tessellated spots (holes, fillets) don't pull it their way.
pub(super) fn centre(s: &Session, bodies: &[u32]) -> Option<DVec3> {
  let (mut sum, mut area) = (DVec3::ZERO, 0.0);
  for b in bodies {
    let Some(m) = s.model.body(BodyId(*b)).and_then(|b| b.mesh()) else { continue };
    let v = |i: u32| DVec3::from(m.positions[i as usize].map(f64::from));
    for t in m.indices.chunks_exact(3) {
      let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
      let w = (b - a).cross(c - a).length();
      sum += (a + b + c) / 3.0 * w;
      area += w;
    }
  }
  (area > 0.0).then(|| sum / area)
}

/// The most instances a count handle asks for.
pub(super) const MAX_COUNT: f64 = 1000.0;

/// A count field's whole number of instances (at least one).
pub(super) fn count_of(e: &str) -> u32 {
  e.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 1.0).map_or(1, |n| n as u32)
}

/// Is a circular pattern's total angle a whole turn (the instances share it out evenly)?
fn is_full_turn(total: f64) -> bool {
  (total.abs() - std::f64::consts::TAU).abs() < 1e-9
}

/// The angle between a circular pattern's instances, as the command spaces them.
fn circ_step(total: f64, n: u32) -> f64 {
  if is_full_turn(total) || n < 2 { total / f64::from(n) } else { total / f64::from(n - 1) }
}

/// A dialog's axis choice ("x", "y", "z", "axis:7") as a world line: a point and unit direction.
pub(super) fn axis_line(s: &Session, key: &str) -> Option<(DVec3, DVec3)> {
  Some(match key {
    "x" => (DVec3::ZERO, DVec3::X),
    "y" => (DVec3::ZERO, DVec3::Y),
    "z" => (DVec3::ZERO, DVec3::Z),
    k => {
      let f = FeatureId(k.strip_prefix("axis:")?.parse().ok()?);
      let (o, d) = *s.model.axes.get(&f)?;
      (o, d.try_normalize()?)
    }
  })
}

/// A point on Draft's pull direction and its normal: a picked face's centre and normal, else the
/// plane's origin and normal.
fn pull_anchor(s: &Session, plane: &Value) -> Option<(DVec3, DVec3)> {
  if plane.is_null() {
    return None;
  }
  if plane.get("face").is_some() {
    return anchor(s, plane);
  }
  let pl = super::plane_of(s, plane)?;
  Some((pl.origin, pl.normal()))
}

/// Draft's dial for a face through `c` with outward normal `n`, the neutral plane through
/// `origin` and the pull direction: where it turns from (the pull direction), about which axis
/// (so a positive angle leans the face in, as the draft does) and its hinge on the neutral plane.
fn draft_dial(c: DVec3, n: DVec3, origin: DVec3, pull: DVec3) -> Option<(DVec3, DVec3, DVec3)> {
  let n = (n - pull * n.dot(pull)).try_normalize()?;
  let axis = n.cross(pull).try_normalize()?;
  Some((pull, axis, c - pull * (c - origin).dot(pull)))
}

/// `v`, or the smallest step on its side when it rounds to nothing.
fn nonzero(v: f64, step: f64) -> f64 {
  let min = step.max(1e-3);
  if v.abs() >= min {
    v
  } else if v < 0.0 {
    -min
  } else {
    min
  }
}

/// The signed Sketch Offset distance that puts the offset of `curves` through `at`: its distance
/// from the curves, positive on the side a positive offset goes (outward for a closed chain, to
/// the left for an open one).
pub(crate) fn offset_at(sketch: &Sketch, curves: &[CurveId], at: [f64; 2]) -> Option<f64> {
  let d0 = curves.iter().filter_map(|c| sketch.distance_to_curve(*c, at)).min_by(f64::total_cmp)?;
  if d0 < 1e-9 {
    return None;
  }
  // which side: the offset by d0 on that side runs through the cursor
  let miss = |d: f64| -> Option<f64> {
    let mut trial = sketch.clone();
    let made = trial.offset(curves, d).ok()?;
    made.iter().filter_map(|c| trial.distance_to_curve(*c, at)).min_by(f64::total_cmp)
  };
  match (miss(d0), miss(-d0)) {
    (Some(p), Some(n)) => Some(if p <= n { d0 } else { -d0 }),
    (Some(_), None) => Some(d0),
    (None, Some(_)) => Some(-d0),
    (None, None) => None,
  }
}

/// The offset arrow's base and direction: the middle of the first offset curve, pointing the way
/// a positive offset moves it.
fn offset_anchor(s: &Session, curves: &[u32]) -> Option<(DVec3, DVec3)> {
  let sk = s.edit_sketch?;
  let sketch = s.design.sketch(sk).ok()?;
  let plane = s.model.sketches.get(&sk)?.plane;
  let ids: Vec<CurveId> = curves.iter().map(|c| CurveId(*c)).collect();
  let first = sketch.curve(*ids.first()?)?;
  let line = fox_engine::scene::curve_polyline(sketch, &first.kind);
  let m = if line.len() == 2 {
    [(line[0][0] + line[1][0]) / 2.0, (line[0][1] + line[1][1]) / 2.0]
  } else {
    *line.get(line.len() / 2)?
  };
  let (lo, hi) = sketch.bounds()?;
  let d = ((hi[0] - lo[0]).hypot(hi[1] - lo[1]) * 1e-3).max(1e-3);
  let mut trial = sketch.clone();
  let made = trial.offset(&ids, d).ok()?;
  // the offset copy's nearest point: the arrow points there
  let mut best: Option<(f64, [f64; 2])> = None;
  for c in made {
    let Some(k) = trial.curve(c) else { continue };
    let pts = fox_engine::scene::curve_polyline(&trial, &k.kind);
    for w in pts.windows(2) {
      let q = closest_on_segment(w[0], w[1], m);
      let dist = (q[0] - m[0]).hypot(q[1] - m[1]);
      if best.is_none_or(|b| dist < b.0) {
        best = Some((dist, q));
      }
    }
  }
  let q = best?.1;
  let origin = plane.to_world(m);
  let dir = (plane.to_world(q) - origin).try_normalize()?;
  Some((origin, dir))
}

fn closest_on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> [f64; 2] {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l2 = dx * dx + dy * dy;
  let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
  [a[0] + dx * t, a[1] + dy * t]
}

/// Where an arrow sits and points for a picked edge (its midpoint, out between its faces) or face
/// (planar: its centre along the normal; curved: a point of it along the surface normal).
pub(super) fn anchor(s: &Session, key: &Value) -> Option<(DVec3, DVec3)> {
  let get = |k: &str| key.get(k).and_then(Value::as_u64).map(|v| v as u32);
  let body = s.model.body(BodyId(get("body")?))?;
  if let Some(edge) = get("edge") {
    let e = body.edges().iter().find(|e| e.id == edge)?;
    let n: DVec3 = body
      .faces()
      .iter()
      .filter(|f| e.faces.contains(&f.id) && f.kind == fox_engine::kernel::SurfaceKind::Plane)
      .map(|f| f.normal)
      .sum();
    let dir = n.try_normalize().or_else(|| {
      // curved faces: away from the body's middle
      let c = centre(s, &[body.id.0])?;
      (e.mid - c).try_normalize()
    });
    return Some((e.mid, dir.unwrap_or(DVec3::Z)));
  }
  let face = get("face")?;
  let f = body.faces().iter().find(|f| f.id == face)?;
  if f.kind == fox_engine::kernel::SurfaceKind::Plane {
    return Some((f.center, f.normal.try_normalize()?));
  }
  let m = body.mesh()?;
  let t = m.tri_face.iter().position(|x| *x == face)?;
  let idx = &m.indices[3 * t..3 * t + 3];
  let v = |k: usize, of: &Vec<[f32; 3]>| DVec3::from(of[idx[k] as usize].map(f64::from));
  let at = (v(0, &m.positions) + v(1, &m.positions) + v(2, &m.positions)) / 3.0;
  let n = (v(0, &m.normals) + v(1, &m.normals) + v(2, &m.normals)).try_normalize()?;
  Some((at, n))
}

/// Extend's arrow: the middle of the edge `key` names, pointing away from its face across the
/// surface (the way the extension grows).
fn extend_anchor(s: &Session, key: &Value) -> Option<(DVec3, DVec3)> {
  let get = |k: &str| key.get(k).and_then(Value::as_u64).map(|v| v as u32);
  let body = s.model.body(BodyId(get("body")?))?;
  let edge = get("edge")?;
  let e = body.edges().iter().find(|e| e.id == edge)?;
  let f = body.faces().iter().find(|f| f.id == e.faces[0])?;
  let along = (e.end - e.start).try_normalize().unwrap_or(DVec3::ZERO);
  let mut d = e.mid - f.center;
  d -= along * d.dot(along);
  if f.kind == fox_engine::kernel::SurfaceKind::Plane {
    d -= f.normal * d.dot(f.normal);
  }
  Some((e.mid, d.try_normalize()?))
}

/// The joint origin frame of a picked face, edge or point (as the joint command reads them).
fn pick_frame(s: &Session, body: BodyId, o: &Value) -> Option<DMat4> {
  use fox_engine::model::assembly;
  let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
  let b = s.model.body(body)?;
  if let Some(f) = o.get("face") {
    let id = get(f, "face")?;
    return assembly::face_frame(b.faces().iter().find(|x| x.id == id)?).ok();
  }
  if let Some(e) = o.get("edge") {
    let id = get(e, "edge")?;
    return assembly::edge_frame(b.edges().iter().find(|x| x.id == id)?, b.faces()).ok();
  }
  let at: Vec<f64> = o.get("at")?.as_array()?.iter().filter_map(Value::as_f64).collect();
  let at = DVec3::from_slice(at.get(..3)?);
  assembly::frame(at, DVec3::Z, DVec3::X).ok()
}

/// Where Extrude's taper handle sits on a profile's outline `poly` (sketch coordinates, closed):
/// the middle of the side farthest from `at` (the distance arrow's foot, so it stays clear of it), with the unit direction
/// across it into the profile (`inside` tells which way that is).
/// A planar face's edge farthest from its centre (`key` is `{body, face}`): its middle, and the
/// unit direction from it into the face, in the face's plane.
fn face_taper_edge(s: &Session, key: &Value) -> Option<(DVec3, DVec3)> {
  let get = |k: &str| key.get(k).and_then(Value::as_u64).map(|v| v as u32);
  let (body, face) = (s.model.body(BodyId(get("body")?))?, get("face")?);
  let f = body.faces().iter().find(|f| f.id == face)?;
  let e = body
    .edges()
    .iter()
    .filter(|e| e.faces.contains(&face))
    .max_by(|x, y| (x.mid - f.center).length().total_cmp(&(y.mid - f.center).length()))?;
  let v = f.center - e.mid;
  let along = (e.end - e.start).try_normalize().unwrap_or(DVec3::ZERO);
  let inward = v - f.normal * v.dot(f.normal) - along * v.dot(along);
  Some((e.mid, inward.try_normalize()?))
}

fn taper_edge(poly: &[[f64; 2]], at: [f64; 2], inside: &dyn Fn([f64; 2]) -> bool) -> Option<([f64; 2], [f64; 2])> {
  let n = poly.len();
  let (a, b) = (0..n)
    .map(|k| (poly[k], poly[(k + 1) % n]))
    .filter(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]) > 1e-9)
    .max_by(|x, y| {
      let d = |(a, b): &([f64; 2], [f64; 2])| ((a[0] + b[0]) / 2.0 - at[0]).hypot((a[1] + b[1]) / 2.0 - at[1]);
      d(x).total_cmp(&d(y))
    })?;
  let len = (b[0] - a[0]).hypot(b[1] - a[1]);
  let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
  let across = [-(b[1] - a[1]) / len, (b[0] - a[0]) / len];
  let eps = (len * 1e-3).max(1e-6);
  let inward =
    if inside([mid[0] + across[0] * eps, mid[1] + across[1] * eps]) { across } else { [-across[0], -across[1]] };
  Some((mid, inward))
}

#[cfg(test)]
mod tests {
  use fox_engine::Sel;

  use super::*;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn apply(s: &mut Session, d: &mut Dialog) {
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
  }

  fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
  }

  #[test]
  fn fillet_chamfer_and_press_pull_arrows_hold_while_previewing() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
    let b = &s.model.bodies[0];
    let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
    let (top_id, top_c) = (top.id, top.center);
    // the top edge along X on the -Y side: the arrow leans out between top and front
    let e = b
      .edges()
      .iter()
      .find(|e| {
        (e.start.z - 30.0).abs() < 1e-6
          && (e.end.z - 30.0).abs() < 1e-6
          && (e.end - e.start).x.abs() > 1.0
          && e.mid.y < 0.0
      })
      .unwrap();
    let (edge, mid) = (json!({"body": b.id.0, "edge": e.id}), e.mid);
    for kind in [Kind::Fillet, Kind::Chamfer] {
      let mut d = Dialog::blank(kind);
      d.edges = vec![edge.clone()];
      d.radius = "2 mm".into();
      d.refresh_manip(&s);
      let m = d.manipulator(&s).unwrap();
      assert!(close(m.origin, mid) && close(m.dir, DVec3::new(0.0, -1.0, 1.0).normalize()), "{m:?}");
      assert!((m.value - 2.0).abs() < 1e-9);
      d.set_manipulator(3.26, 0.5);
      assert_eq!(d.radius, "3.5 mm");
      apply(&mut s, &mut d);
      // the edge is gone from the previewed body; the arrow stays where it was
      d.refresh_manip(&s);
      let m = d.manipulator(&s).unwrap();
      assert!(close(m.origin, mid) && (m.value - 3.5).abs() < 1e-9);
      // dragged through zero, a radius stops at the smallest step
      d.set_manipulator(-4.0, 0.5);
      assert_eq!(d.radius, "0.5 mm");
      d.cancel(&mut s);
    }
    let mut d = Dialog::blank(Kind::PressPull);
    d.faces = vec![json!({"body": s.model.bodies[0].id.0, "face": top_id})];
    d.distance = "2 mm".into();
    d.refresh_manip(&s);
    let m = d.manipulator(&s).unwrap();
    assert!(close(m.origin, top_c) && close(m.dir, DVec3::Z));
    d.set_manipulator(-5.2, 0.5);
    assert_eq!(d.distance, "-5 mm");
    apply(&mut s, &mut d);
    let bb = s.model.bodies[0].shape.mass().unwrap();
    assert!((bb.bbox_max.z - 25.0).abs() < 1e-6, "{bb:?}");
    // no arrow when editing an existing feature (its picks name the body before it)
    d.editing = Some(fox_engine::model::FeatureId(1));
    assert!(d.manipulator(&s).is_none());
  }

  /// Extrude's taper handle: a dial at the arrow's head that writes the taper angle; the floating
  /// value box edits the distance (Extrude) or radius (Fillet) the arrow shows.
  #[test]
  fn a_face_extrudes_taper_handle_sits_on_the_faces_edge() {
    use crate::handles::Shape;
    // `ui:dialogs:extrude-body-face-pick`: the arrow and the taper disc on a box's top face, as
    // found before the preview, the disc on the middle of an outer edge turning inward
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 34, "depth": 20, "height": 10}));
    let b = &s.model.bodies[0];
    let top = *b.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = None;
    d.faces = vec![json!({"body": b.id.0, "face": top.id})];
    d.distance = "5 mm".into();
    d.refresh_manip(&s);
    let m = d.manipulator(&s).unwrap();
    assert!(close(m.origin, top.center) && close(m.dir, DVec3::Z), "{m:?}");
    let h = d.handles(&s);
    let Shape::Dial { center, axis, zero, .. } = h[0].shape else { panic!("{h:?}") };
    assert_eq!(h[0].head, crate::handles::Head::Taper);
    assert!((center.z - top.center.z).abs() < 1e-9 && close(zero, DVec3::Z), "{center:?}");
    let inward = axis.cross(zero);
    assert!(inward.dot(top.center - center) > 0.0 && inward.z.abs() < 1e-9, "{inward:?}");
  }

  #[test]
  fn extrude_taper_handle_and_the_value_box_fields() {
    use crate::handles::Shape;
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = Some(sk);
    d.profiles = vec![s.model.sketches[&sk].profiles[0].interior_point()];
    d.distance = "5 mm".into();
    let m = d.manipulator(&s).unwrap();
    let h = d.handles(&s);
    assert_eq!(h.len(), 1);
    // `ui:manip:extrude-taper-handle`: a disc on the middle of the profile's side farthest from
    // the arrow's foot (on the sketch plane, clear of the arrow), turning about that side from the
    // extrude direction inward (a positive taper narrows)
    let Shape::Dial { center, axis, zero, radius } = h[0].shape else { panic!("{:?}", h[0]) };
    assert_eq!(h[0].head, crate::handles::Head::Taper);
    assert!(center.z.abs() < 1e-9 && close(zero, DVec3::Z), "{:?}", h[0]);
    let (foot, edge_mid) = (m.origin, center);
    assert!((edge_mid - foot).length() > 4.9, "{edge_mid:?} beside the arrow at {foot:?}");
    let on_side = [0.0, 10.0].iter().any(|v| (center.x - v).abs() < 1e-9 || (center.y - v).abs() < 1e-9);
    assert!(on_side, "{center:?}");
    let inward = axis.cross(zero);
    assert!(((foot - center).normalize() - inward).length() < 0.3, "{inward:?}");
    assert!(radius.is_none() && h[0].value.abs() < 1e-9);
    // dragged to 10°: the taper angle box says so; back to nothing: the box is empty again
    d.set_handle(&s, 0, 10f64.to_radians() + 0.001, 1f64.to_radians());
    assert_eq!(d.solid.ext.taper(), "10 deg");
    assert!(d.dirty);
    assert!((d.handles(&s)[0].value - 10f64.to_radians()).abs() < 1e-9);
    apply(&mut s, &mut d);
    assert_eq!(d.command().unwrap().1["taper"], json!("10 deg"));
    d.set_handle(&s, 0, 0.0, 1f64.to_radians());
    assert_eq!(d.solid.ext.taper(), "");
    // and it stays short of a right angle
    d.set_handle(&s, 0, 2.0, 1f64.to_radians());
    assert_eq!(d.solid.ext.taper(), "89 deg");
    // the value box edits the field the arrow shows
    assert_eq!(d.value_box_field().map(|t| t.clone()), Some("5 mm".to_string()));
    *d.value_box_field().unwrap() = "7 mm".into();
    d.value_box_edited();
    assert!(d.dirty && d.distance == "7 mm");
    let mut f = Dialog::blank(Kind::Fillet);
    f.radius = "3 mm".into();
    assert_eq!(f.value_box_field().map(|t| t.clone()), Some("3 mm".to_string()));
    assert!(Dialog::blank(Kind::Combine).value_box_field().is_none());
    assert!(Dialog::blank(Kind::Shell).handles(&s).is_empty(), "no taper handle on other dialogs");
  }

  /// Edit Feature on a fillet or chamfer shows its arrow and value box again, on the edge the
  /// feature stored, and dragging it edits the radius.
  #[test]
  fn an_edited_fillet_keeps_its_arrow() {
    for kind in [Kind::Fillet, Kind::Chamfer] {
      let mut s = Session::default();
      run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
      let b = &s.model.bodies[0];
      let e = b.edges().iter().find(|e| (e.mid - DVec3::new(0.0, -10.0, 30.0)).length() < 1e-6).unwrap();
      let (edge, mid) = (json!({"body": b.id.0, "edge": e.id}), e.mid);
      let mut d = Dialog::blank(kind);
      d.edges = vec![edge];
      d.radius = "2 mm".into();
      apply(&mut s, &mut d);
      let app = crate::App::with_session(s);
      let id = app.s.design.features.last().unwrap().id;
      let mut e = Dialog::edit(&app, id).unwrap();
      let s = app.s;
      let m = e.manipulator(&s).expect("the edited feature's arrow");
      assert!(close(m.origin, mid) && close(m.dir, DVec3::new(0.0, -1.0, 1.0).normalize()), "{m:?}");
      assert!((m.value - 2.0).abs() < 1e-9);
      assert_eq!(e.value_box_field().map(|t| t.clone()), Some("2 mm".to_string()));
      e.set_manipulator(3.0, 0.5);
      assert_eq!(e.radius, "3 mm");
      let field = if kind == Kind::Fillet { "radius" } else { "distance" };
      assert!(e.edit_set().unwrap()[field] == json!("3 mm"), "{:?}", e.edit_set());
      e.refresh_manip(&s);
      assert!((e.manipulator(&s).unwrap().value - 3.0).abs() < 1e-9, "the arrow follows the edit");
    }
  }

  #[test]
  fn joint_triad_sets_offset_and_angle_and_glyphs_pick_origins_and_joints() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
    let base = s.model.bodies[0].id.0;
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 5, "at": [0, 200]}));
    let arm = s.model.bodies[1].id.0;
    for (b, n) in [(base, "Base"), (arm, "Arm")] {
      run(&mut s, "component.create", json!({"name": n, "bodies": [b]}));
    }
    let face = |s: &Session, i: usize, up: bool| {
      let b = &s.model.bodies[i];
      let f = b.faces().iter().find(|f| if up { f.normal.z > 0.9 } else { f.normal.z < -0.9 }).unwrap();
      (b.id.0, json!({"face": {"body": b.id.0, "face": f.id}}))
    };
    let comp = |s: &Session, b: u32| s.design.component_of(BodyId(b)).map(|c| c.id.0);
    let mut d = Dialog::blank(Kind::Joint);
    d.choice = "rigid";
    d.angle = "0 deg".into();
    d.distance = "0 mm".into();
    // the arm's bottom onto the base's top: flipped, the Z axes face each other and it sits on top
    d.flip = true;
    d.origins = vec![face(&s, 1, false), face(&s, 0, true)];
    d.comps = vec![comp(&s, arm), comp(&s, base)];
    let t = d.triad(&s).unwrap();
    assert!(close(t.origin, DVec3::new(50.0, 30.0, 10.0)), "{t:?}");
    assert!(t.arrows == [false, false, true] && t.rings == [false, false, true] && t.planes == [false; 3]);
    d.set_triad(&s, TriadEdit::Move(DVec3::new(50.0, 30.0, 13.0)));
    d.set_triad(&s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_4));
    assert_eq!((d.distance.as_str(), d.angle.as_str()), ("3 mm", "45 deg"));
    apply(&mut s, &mut d);
    let bb = s.model.bodies[1].shape.mass().unwrap();
    assert!((bb.bbox_min.z - 13.0).abs() < 1e-6, "{bb:?}");
    // the fixed side didn't move: the triad follows the offset
    let t = d.triad(&s).unwrap();
    assert!(close(t.origin, DVec3::new(50.0, 30.0, 13.0)));
    let joint = s.design.features.last().unwrap().id;

    // glyphs: saved origins fill the Joint dialog's sides in turn; a joint goes to Drive Joints
    let base_c = comp(&s, base);
    run(&mut s, "joint_origin", json!({"component": base_c, "at": [10, 10, 10], "name": "Corner"}));
    let marks = fox_engine::scene::joint_marks(&s);
    let origin = *marks.iter().find(|m| !m.joint).unwrap();
    let jm = *marks.iter().find(|m| m.joint && m.feature == joint).unwrap();
    let mut j = Dialog::blank(Kind::Joint);
    assert!(j.pick_joint_mark(&mut s, &origin, "Corner"));
    assert_eq!(j.saved, ["Corner".to_string(), String::new()]);
    assert!(j.pick_joint_mark(&mut s, &origin, "Corner"));
    assert_eq!(j.saved[1], "Corner");
    assert!(!j.pick_joint_mark(&mut s, &jm, "Joint1"));
    let mut drive = Dialog::blank(Kind::DriveJoint);
    assert!(drive.pick_joint_mark(&mut s, &jm, "Joint1"));
    assert_eq!(drive.target, Some(joint.0));
    assert!(!Dialog::blank(Kind::Fillet).pick_joint_mark(&mut s, &jm, "Joint1"));

    // a picked vertex is a joint origin at that point
    let v = fox_engine::scene::body_vertices(&s.model.bodies[0])[0];
    s.selection = vec![Sel::Vertex { body: BodyId(base), edge: v.0, end: v.1 }];
    let o = super::super::sel_origins(&s);
    assert_eq!(o, vec![(base, json!({"at": v.2.to_array()}))]);
  }

  #[test]
  fn sketch_offset_follows_the_cursor_to_either_side() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let l = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
    let line = l["curves"][0].as_u64().unwrap() as u32;
    let sk = s.edit_sketch.unwrap();
    let sketch = s.design.sketch(sk).unwrap().clone();
    // an open curve offsets to its left for a positive distance
    let ids = [CurveId(line)];
    assert!((offset_at(&sketch, &ids, [5.0, 3.0]).unwrap() - 3.0).abs() < 1e-9);
    assert!((offset_at(&sketch, &ids, [5.0, -2.0]).unwrap() + 2.0).abs() < 1e-9);
    assert_eq!(offset_at(&sketch, &ids, [5.0, 0.0]), None, "on the curve: no offset");
    // the dialog follows the cursor until a click places it, with the arrow on the curve
    let mut d = Dialog::blank(Kind::SkOffset);
    d.follow = true;
    d.curves = vec![line];
    assert!(d.follows_cursor());
    d.follow_cursor(&s, [4.0, -2.04], 0.1);
    assert_eq!(d.distance, "-2 mm");
    assert!(d.dirty);
    d.refresh_manip(&s);
    let m = d.more_manipulator(&s).expect("an offset arrow");
    assert!(close(m.origin, DVec3::new(5.0, 0.0, 0.0)) && close(m.dir, DVec3::Y), "{:?} {:?}", m.origin, m.dir);
    assert!((m.value + 2.0).abs() < 1e-9);
    d.stop_following();
    d.follow_cursor(&s, [4.0, 7.0], 0.1);
    assert_eq!(d.distance, "-2 mm", "placed: the cursor no longer moves it");
    // dragging the arrow sets the distance and stops the following too
    d.follow = true;
    assert!(d.set_more_manipulator(3.0, 0.1));
    assert_eq!(d.distance, "3 mm");
    assert!(!d.follows_cursor());
    // a closed chain offsets outward for a positive distance
    run(&mut s, "sketch.rect", json!({"corner1": [20, 0], "corner2": [30, 10]}));
    let sketch = s.design.sketch(sk).unwrap().clone();
    let rect: Vec<CurveId> = sketch.curves.iter().skip(1).map(|c| c.id).collect();
    assert!((offset_at(&sketch, &rect, [35.0, 5.0]).unwrap() - 5.0).abs() < 1e-9);
    assert!((offset_at(&sketch, &rect, [25.0, 8.0]).unwrap() + 2.0).abs() < 1e-9);
  }

  #[test]
  fn offset_plane_arrow_follows_the_design_behaviour() {
    let arrow = |legacy: bool| {
      let mut s = Session::default();
      if legacy {
        s.design.behaviours = fox_engine::model::Behaviours::legacy();
      }
      let mut d = Dialog::blank(Kind::OffsetPlane);
      d.plane = json!("xz");
      d.distance = "10 mm".into();
      d.manipulator(&s).unwrap()
    };
    // a new design offsets from XZ along +Y; one from before XZ_OFFSET_PLUS_Y along -Y
    assert!(close(arrow(false).dir, DVec3::Y), "{:?}", arrow(false));
    assert!(close(arrow(true).dir, -DVec3::Y), "{:?}", arrow(true));
  }

  #[test]
  fn shell_draft_and_circular_pattern_handles_write_their_fields() {
    use crate::handles::Shape;
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
    let b = s.model.bodies[0].id.0;
    let face_of = |s: &Session, dir: DVec3| {
      let f = s.model.bodies[0].faces().iter().find(|f| f.normal.dot(dir) > 0.9).unwrap();
      (json!({"body": b, "face": f.id}), f.center)
    };
    // Shell: an arrow from the opened face into the wall, out of it for an outside thickness
    let (top, top_c) = face_of(&s, DVec3::Z);
    let mut d = Dialog::blank(Kind::Shell);
    d.faces = vec![top.clone()];
    d.refresh_manip(&s);
    let h = d.handles(&s);
    assert_eq!(h.len(), 1);
    assert!(matches!(h[0].shape, Shape::Arrow { origin, dir } if close(origin, top_c) && close(dir, -DVec3::Z)));
    assert!((h[0].value - 2.0).abs() < 1e-9 && h[0].label == "2 mm");
    d.set_handle(&s, 0, 3.26, 0.5);
    assert_eq!(d.radius, "3.5 mm");
    assert!(d.dirty);
    // never through zero
    d.set_handle(&s, 0, -1.0, 0.5);
    assert_eq!(d.radius, "0.5 mm");
    d.solid.shell_dir = "both";
    let h = d.handles(&s);
    assert_eq!(h.len(), 2);
    assert!(matches!(h[1].shape, Shape::Arrow { dir, .. } if close(dir, DVec3::Z)) && (h[1].value - 0.5).abs() < 1e-9);
    d.set_handle(&s, 1, 1.1, 0.5);
    assert_eq!((d.radius.as_str(), d.solid.thickness_out.as_str()), ("0.5 mm", "1 mm"));
    d.solid.shell_dir = "inside";
    d.radius = "1 mm".into();
    apply(&mut s, &mut d);
    // the face is gone from the preview; the arrow stays where it was
    d.refresh_manip(&s);
    assert!(matches!(d.handles(&s)[0].shape, Shape::Arrow { origin, .. } if close(origin, top_c)));
    d.cancel(&mut s);

    // Draft (`ui:manip:draft-angle-handle`): a disc on a dial at the face's hinge in the neutral
    // plane, from the pull direction, leaning the face in for a positive angle in 5° steps, and
    // the pull direction as a hollow arrow from the hinge; no box until the disc is dragged
    let (front, _) = face_of(&s, -DVec3::Y);
    let before = s.model.bodies[0].shape.mass().unwrap().volume;
    let mut d = Dialog::blank(Kind::Draft);
    d.angle = "3 deg".into();
    d.plane = json!("xy");
    d.faces = vec![front];
    d.refresh_manip(&s);
    let h = d.handles(&s);
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].head, h[0].snap), (crate::handles::Head::Disc, Some(5f64.to_radians())));
    let Shape::Dial { center, axis, zero, radius: None } = h[0].shape else { panic!("{h:?}") };
    assert!(close(center, DVec3::new(0.0, -10.0, 0.0)) && close(zero, DVec3::Z), "{center} {zero}");
    assert!((h[0].value - 3f64.to_radians()).abs() < 1e-9 && h[0].label == "3 deg");
    assert_eq!(h[1].head, crate::handles::Head::Pull);
    assert!(matches!(h[1].shape, Shape::Arrow { origin, dir } if close(origin, center) && close(dir, DVec3::Z)));
    // the head turns from +Z towards +Y: into the body
    assert!(close(axis.cross(zero), DVec3::Y), "{axis}");
    assert!(d.handle_field(0).is_none(), "no box before the drag");
    d.set_handle(&s, 0, 33.3f64.to_radians(), 5f64.to_radians());
    assert_eq!(d.angle, "35.0 deg");
    assert_eq!(d.handle_field(0).map(|f| f.clone()).as_deref(), Some("35.0 deg"), "the drag opens the box");
    d.set_handle(&s, 0, 120f64.to_radians(), 5f64.to_radians());
    assert_eq!(d.angle, "89.0 deg", "short of a right angle");
    d.angle = "10 deg".into();
    apply(&mut s, &mut d);
    let m = s.model.bodies[0].shape.mass().unwrap();
    assert!(m.volume < before && (m.bbox_min.y + 10.0).abs() < 1e-6, "{m:?}");
    // flipped, the dial turns from -Z
    d.flip = true;
    let Shape::Dial { zero, .. } = d.handles(&s)[0].shape else { panic!() };
    assert!(close(zero, -DVec3::Z));
    d.cancel(&mut s);

    // Circular Pattern: dials about the axis through the objects' centre, the total angle and a
    // dot at the next instance for the count
    run(&mut s, "box", json!({"width": 4, "depth": 4, "height": 30, "at": [30, 0]}));
    let small = s.model.bodies[1].id.0;
    let mut d = Dialog::blank(Kind::CircPattern);
    d.bodies = vec![small];
    d.axis = "z".into();
    d.count = "4".into();
    d.angle = "360 deg".into();
    d.refresh_manip(&s);
    let h = d.handles(&s);
    assert_eq!(h.len(), 2);
    let Shape::Dial { center, axis, zero, radius } = h[0].shape else { panic!() };
    assert!(close(center, DVec3::new(0.0, 0.0, 15.0)) && close(axis, DVec3::Z) && close(zero, DVec3::X));
    assert!((radius.unwrap() - 30.0).abs() < 1e-6);
    assert!((h[0].value - std::f64::consts::TAU).abs() < 1e-9);
    assert!((h[1].value - std::f64::consts::FRAC_PI_2).abs() < 1e-9 && h[1].label == "4 instances");
    // a whole turn shared out: the dot at 70° asks for 5
    d.set_handle(&s, 1, 70f64.to_radians(), 1f64.to_radians());
    assert_eq!(d.count, "5");
    // the wrong way round asks for nothing
    d.dirty = false;
    d.set_handle(&s, 1, -0.3, 1f64.to_radians());
    assert!(!d.dirty && d.count == "5");
    // a part turn: the instances span it, the first on the original
    d.set_handle(&s, 0, 180.4f64.to_radians(), 1f64.to_radians());
    assert_eq!(d.angle, "180 deg");
    d.set_handle(&s, 1, 61f64.to_radians(), 1f64.to_radians());
    assert_eq!(d.count, "4");
    let h = d.handles(&s);
    assert!((h[1].value - 60f64.to_radians()).abs() < 1e-9);
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 5);
    // the handles stay on the centre found before the preview
    d.refresh_manip(&s);
    assert!(matches!(d.handles(&s)[0].shape, Shape::Dial { radius: Some(r), .. } if (r - 30.0).abs() < 1e-6));
    d.cancel(&mut s);
    // about a construction axis: the dial is on it
    let ax = run(&mut s, "axis", json!({"method": "two_points", "a": {"at": [10, 0, 0]}, "b": {"at": [10, 0, 1]}}));
    d.axis = format!("axis:{}", ax["feature"]);
    let Shape::Dial { center, radius, .. } = d.handles(&s)[0].shape else { panic!() };
    assert!(close(center, DVec3::new(10.0, 0.0, 15.0)) && (radius.unwrap() - 20.0).abs() < 1e-6);
    // Edit Feature shows the handles as creation does
    d.editing = Some(FeatureId(1));
    assert!(!d.handles(&s).is_empty());
  }
}
