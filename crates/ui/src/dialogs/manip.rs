//! The dialogs' viewport manipulators beyond Extrude's and Offset Plane's arrow: the radius arrow
//! of Fillet and Chamfer, Press Pull's distance arrow, the move/rotate triad of Move and the
//! offset arrow and angle ring of Joint (see [`crate::manipulators`]); and picking joints and
//! saved joint origins in the viewport.

use fox_engine::Session;
use fox_engine::kernel::glam::{DMat4, DQuat, DVec3};
use fox_engine::model::BodyId;
use fox_engine::scene::JointMark;
use fox_engine::sketch::{CurveId, Sketch};
use serde_json::{Value, json};

use super::{Dialog, Kind, Manipulator};
use crate::manipulators::{Triad, TriadEdit};

/// Geometry a dialog's manipulator hangs on, worked out while its picks are valid (a preview
/// changes the very edges and faces they name) and kept while the preview shows.
#[derive(Default)]
pub(super) struct Cache {
  /// The pick the anchor was found for, and the anchor: a point and a unit direction.
  anchor: Option<(Value, DVec3, DVec3)>,
  /// Move: the bodies and their centre before the move.
  pivot: Option<(Vec<u32>, DVec3)>,
}

const AXES: [&str; 3] = ["x", "y", "z"];

fn mm(v: f64) -> String {
  format!("{} mm", fox_engine::sketch::Dim::new((v * 1e4).round() / 1e4).expr)
}

fn deg(a: f64) -> String {
  format!("{} deg", fox_engine::sketch::Dim::new((a.to_degrees() * 1e4).round() / 1e4).expr)
}

/// Rotation about world axis `i` (0 = X) by `a` radians.
fn about(i: usize, a: f64) -> DQuat {
  DQuat::from_axis_angle([DVec3::X, DVec3::Y, DVec3::Z][i], a)
}

impl Dialog {
  /// Is the dialog's command applied to the session as a preview?
  pub fn preview_applied(&self) -> bool {
    self.applied
  }

  fn len(&self, s: &Session, e: &str) -> Option<f64> {
    fox_engine::expr::eval(e, fox_engine::expr::Kind::Length, &s.model.params).ok()
  }

  fn ang(&self, s: &Session, e: &str) -> Option<f64> {
    if e.trim().is_empty() {
      return Some(0.0);
    }
    fox_engine::expr::eval(e, fox_engine::expr::Kind::Angle, &s.model.params).ok()
  }

  /// The pick an arrow hangs on: the first edge (Fillet, Chamfer) or face (Press Pull), or the
  /// offset curves (Sketch Offset).
  fn anchor_key(&self) -> Value {
    match self.kind {
      Kind::PressPull => json!(self.faces.first()),
      Kind::SkOffset => json!({"offset": self.curves}),
      _ => json!(self.edges.first()),
    }
  }

  /// Brings the cached manipulator geometry up to date; the viewport calls it every frame.
  pub fn refresh_manip(&mut self, s: &Session) {
    if self.editing.is_some() {
      return;
    }
    match self.kind {
      Kind::Fillet | Kind::Chamfer | Kind::PressPull if !self.applied => {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = anchor(s, &key).map(|(o, d)| (key, o, d));
        }
      }
      // the offset curves stay as they are under the preview
      Kind::SkOffset => {
        let key = self.anchor_key();
        if self.manip.anchor.as_ref().is_none_or(|a| a.0 != key) {
          self.manip.anchor = offset_anchor(s, &self.curves).map(|(o, d)| (key, o, d));
        }
      }
      Kind::Move if self.manip.pivot.as_ref().is_none_or(|p| p.0 != self.bodies) => {
        let Some(now) = centre(s, &self.bodies) else {
          self.manip.pivot = None;
          return;
        };
        // a showing preview has moved the bodies already: take the move back out
        let before = match self.move_values(s) {
          Some((t, r)) if self.applied => r.inverse() * now - t,
          _ => now,
        };
        self.manip.pivot = Some((self.bodies.clone(), before));
      }
      _ => {}
    }
  }

  /// The arrows of Fillet and Chamfer (radius, out of the first edge) and Press Pull (distance,
  /// along the first face's normal).
  pub(super) fn more_manipulator(&self, s: &Session) -> Option<Manipulator> {
    if !matches!(self.kind, Kind::Fillet | Kind::Chamfer | Kind::PressPull | Kind::SkOffset) || self.editing.is_some() {
      return None;
    }
    let (key, origin, dir) = self.manip.anchor.as_ref()?;
    if *key != self.anchor_key() {
      return None;
    }
    let field = if matches!(self.kind, Kind::PressPull | Kind::SkOffset) { &self.distance } else { &self.radius };
    Some(Manipulator { origin: *origin, dir: *dir, value: self.len(s, field)? })
  }

  /// Takes a dragged arrow value (already rounded to `step`) into the field; false for dialogs
  /// without such an arrow.
  pub(super) fn set_more_manipulator(&mut self, v: f64, step: f64) -> bool {
    match self.kind {
      // a radius or distance can't reach zero: the smallest step is as far as it goes
      Kind::Fillet | Kind::Chamfer => self.radius = mm(v.max(step.max(1e-3))),
      Kind::PressPull => self.distance = mm(v),
      // an offset is never zero: it keeps to the side it is on
      Kind::SkOffset => {
        self.distance = mm(nonzero(v, step));
        self.follow = false;
      }
      _ => return false,
    }
    true
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
    let text = mm(nonzero(v, step));
    if text != self.distance {
      self.distance = text;
      self.dirty = true;
    }
  }

  /// Move's translation and its rotation (about a world axis through the origin, applied after
  /// the translation); None when it rotates about some other axis.
  fn move_values(&self, s: &Session) -> Option<(DVec3, DQuat)> {
    let t =
      DVec3::new(self.len(s, &self.translate[0])?, self.len(s, &self.translate[1])?, self.len(s, &self.translate[2])?);
    let a = self.ang(s, &self.angle)?;
    if a == 0.0 {
      return Some((t, DQuat::IDENTITY));
    }
    let i = AXES.iter().position(|x| *x == self.axis)?;
    Some((t, about(i, a)))
  }

  /// The move/rotate triad this dialog shows: Move's on the moved bodies' centre, Joint's (an
  /// offset arrow and an angle ring about the joint's Z) on the fixed side's joint origin.
  pub fn triad(&self, s: &Session) -> Option<Triad> {
    if self.editing.is_some() {
      return None;
    }
    match self.kind {
      Kind::Move => {
        let (bodies, c) = self.manip.pivot.as_ref()?;
        if bodies.is_empty() || *bodies != self.bodies {
          return None;
        }
        let (t, r) = self.move_values(s)?;
        let mut tr = Triad::world(r * (*c + t));
        let a = self.ang(s, &self.angle)?;
        if a != 0.0 {
          tr.angle = a;
          tr.ring = AXES.iter().position(|x| *x == self.axis);
        }
        Some(tr)
      }
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
          angle,
          ring: Some(2),
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
        let Some((_, c)) = self.manip.pivot.clone() else { return };
        let Some((t, r)) = self.move_values(s) else { return };
        // the regeneration moves by t, then turns about the world axis: centre' = r (c + t)
        let (to, r) = match edit {
          TriadEdit::Move(p) => (p, r),
          TriadEdit::Rotate(i, a) => {
            self.axis = AXES[i].into();
            self.angle = deg(a);
            (r * (c + t), about(i, a))
          }
        };
        let t = r.inverse() * to - c;
        self.translate = [mm(t.x), mm(t.y), mm(t.z)];
      }
      Kind::Joint => match edit {
        TriadEdit::Move(p) => {
          let Some(f) = self.joint_fixed_frame(s) else { return };
          self.distance = mm((p - f.w_axis.truncate()).dot(f.z_axis.truncate()));
        }
        TriadEdit::Rotate(_, a) => self.angle = deg(a),
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
fn centre(s: &Session, bodies: &[u32]) -> Option<DVec3> {
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
fn anchor(s: &Session, key: &Value) -> Option<(DVec3, DVec3)> {
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

  #[test]
  fn move_triad_moves_and_turns_about_the_bodies_centre() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
    let b = s.model.bodies[0].id.0;
    let c = centre(&s, &[b]).unwrap();
    let mut d = Dialog::blank(Kind::Move);
    d.bodies = vec![b];
    d.translate = ["0 mm".into(), "0 mm".into(), "0 mm".into()];
    d.angle = "0 deg".into();
    d.axis = "z".into();
    apply(&mut s, &mut d);
    d.refresh_manip(&s);
    let t = d.triad(&s).unwrap();
    assert!(close(t.origin, c) && t.arrows == [true; 3] && t.ring.is_none());
    d.set_triad(&s, TriadEdit::Move(c + DVec3::new(10.0, 0.0, -4.0)));
    assert_eq!(d.translate, ["10 mm", "0 mm", "-4 mm"]);
    apply(&mut s, &mut d);
    let p = c + DVec3::new(10.0, 0.0, -4.0);
    assert!(close(centre(&s, &[b]).unwrap(), p));
    // a quarter turn about Z keeps the bodies where the triad is
    d.set_triad(&s, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_2));
    assert_eq!((d.axis.as_str(), d.angle.as_str()), ("z", "90 deg"));
    apply(&mut s, &mut d);
    d.refresh_manip(&s);
    assert!(close(centre(&s, &[b]).unwrap(), p), "{:?}", centre(&s, &[b]));
    let t = d.triad(&s).unwrap();
    assert!(close(t.origin, p) && t.ring == Some(2) && (t.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    let bb = s.model.bodies[0].shape.mass().unwrap();
    assert!(((bb.bbox_max - bb.bbox_min) - DVec3::new(20.0, 10.0, 30.0)).length() < 1e-6, "{bb:?}");
    // then moving again goes along world X, still turned
    d.set_triad(&s, TriadEdit::Move(p + DVec3::X * 5.0));
    apply(&mut s, &mut d);
    assert!(close(centre(&s, &[b]).unwrap(), p + DVec3::X * 5.0));
    // a dialog opened on bodies already moved by its preview finds where they started
    let mut again = Dialog::blank(Kind::Move);
    again.bodies = vec![b];
    again.translate = d.translate.clone();
    again.angle = d.angle.clone();
    again.axis = d.axis.clone();
    again.applied = true;
    again.refresh_manip(&s);
    assert!(close(again.manip.pivot.as_ref().unwrap().1, c));
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
}
